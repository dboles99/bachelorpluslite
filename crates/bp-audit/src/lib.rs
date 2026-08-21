//! Security audit history: what happened to a document, never what was in it.
//!
//! specs.md section 15 lists "security audit history" beside encryption,
//! signatures and redaction, and ADR-0011 names *audit events* among the
//! artefacts a document's security policy propagates to. This crate is both
//! halves of that: the vocabulary of security-relevant events, and the
//! append-only log they are written to.
//!
//! Three decisions shape everything below.
//!
//! **An audit log is a new place secrets live.** That is the whole hazard.
//! `bp-secrets` already refuses to carry the matched text of a finding for
//! this reason; the same argument applies here with more force, because a
//! finding is transient and a log is on disk for ever. So no type reachable
//! from a [`Record`] can hold text or bytes at all. [`Event`]'s payloads are
//! counts, flags and closed enums; a document is named by a [`DocumentId`],
//! which is a `u64`. There is no field a passphrase, a line of the document,
//! a secret's value or a redacted span could be put in — not by accident and
//! not on purpose. The prose promise is the same one every leaky logger
//! started with, so it is the compiler that makes it true: see
//! [`Record`] on why `Copy` is a constraint here rather than a convenience.
//! A property test over the serialised form catches the rest.
//!
//! **The log is subject to the document's security profile, like every other
//! derived artefact.** A Maximum-profile document whose audit trail is a
//! plaintext file saying "unlock failed for document 7 at 14:02" has leaked
//! the pattern the profile existed to hide — when the document is touched,
//! how often, and that somebody is guessing at it. [`Destination`] resolves
//! the policy to one of three answers, and, exactly as ADR-0020 requires, a
//! policy that cannot be honoured is refused rather than downgraded: a
//! profile that wants a sealed log and has no key gets no log and a notice,
//! never a plaintext one.
//!
//! **Append-only, as far as a process can make it so.** See
//! [`AuditLog`] for what is enforced and what honestly is not.
//!
//! Like the rest of this workspace, the clock is a parameter. Nothing here
//! reads the time, which is what makes the whole crate exhaustively testable
//! and an audit line reproducible in a test rather than "roughly now".

#![forbid(unsafe_code)]

mod passphrase;

pub use passphrase::PassphraseSealer;

use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use bp_security::{Metadata, Policy, Privacy, Recovery, Security};
use serde::{Deserialize, Serialize};
use time::{OffsetDateTime, UtcOffset};

/// Crate identity used by workspace smoke tests and diagnostics.
pub const CRATE_NAME: &str = "bp-audit";

// ---------------------------------------------------------------------------
// Identity
// ---------------------------------------------------------------------------

/// Which document an event was about, as a number and nothing else.
///
/// **This is the crate's sharpest edge, so it is worth being blunt about.**
/// The obvious design is a `PathBuf`, and the obvious design is the leak: an
/// audit file that names `Divorce settlement.bpadx` has disclosed what the
/// user keeps, which is most of what a strict profile was protecting. A
/// `u64` cannot hold a filename, so the file physically cannot say one.
///
/// The session holds the mapping from id to document and resolves it for the
/// UI, exactly as `bp-history` keys checkpoints by a `u64` id. A caller that
/// wants ids to mean the same thing in a later session should derive one from
/// a digest of the path (`bp_crypto::stable_name` computes that digest today)
/// — a stable number, still not a name.
///
/// Deliberately *not* `From<&Path>`: an ergonomic conversion would put a path
/// one keystroke away from the log, and one keystroke is the whole distance
/// this type exists to keep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DocumentId(u64);

impl DocumentId {
    /// Label a document by a number the caller already uses for it.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// The number, for looking the document up in whatever holds the mapping.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// A security profile named without carrying its contents.
///
/// A profile change is worth recording — it is the moment a document's
/// protection was raised or, more interestingly, lowered. What is *not* worth
/// recording is the whole [`bp_security::Policy`], because writing seven axes
/// into every line makes the log a copy of the security model rather than a
/// history of decisions, and it would drift the moment an eighth axis
/// appeared. Five closed variants, matching [`Security::name`], say the thing
/// a reader of the history wants to know.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileLabel {
    Standard,
    Private,
    Confidential,
    Maximum,
    /// A policy spelled out rather than named. Which axes it set is not
    /// recorded, for the reason above; the security inspector shows the
    /// current one.
    Custom,
}

impl ProfileLabel {
    /// Label whatever the document's setting currently is.
    ///
    /// Goes through [`Security::name`] rather than matching on the variants
    /// itself, so a profile added to `bp-security` shows up here as a
    /// compile error in one place instead of being silently logged as
    /// `Custom`.
    #[must_use]
    pub fn of(security: &Security) -> Self {
        match security.name() {
            "Standard" => Self::Standard,
            "Private" => Self::Private,
            "Confidential" => Self::Confidential,
            "Maximum" => Self::Maximum,
            _ => Self::Custom,
        }
    }

    /// What to show the user, spelled the same way the Security menu does.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Standard => "Standard",
            Self::Private => "Private",
            Self::Confidential => "Confidential",
            Self::Maximum => "Maximum",
            Self::Custom => "Custom",
        }
    }
}

// ---------------------------------------------------------------------------
// The vocabulary
// ---------------------------------------------------------------------------

/// A security-relevant thing that happened.
///
/// The list is specs.md section 15 read as verbs: the document was encrypted,
/// decrypted or unlocked, a passphrase was rejected, the profile changed,
/// Privacy Mode was entered or left, a secret scan ran, a signature was made
/// or checked, a redaction was applied.
///
/// **Every payload is a number, a flag or a closed enum, and that is the
/// crate's central guarantee rather than a coincidence.** `SecretScanFinished`
/// carries how many secrets were found and not one character of any of them —
/// a log line reading "3 credentials found" is the whole point of the scan,
/// while a line reading "found `AKIA...`" is the leak the scan was run to
/// prevent. The same reasoning removes the passphrase from `UnlockFailed`,
/// the text from `RedactionApplied` and the filename from all of them.
///
/// Not `#[non_exhaustive]`, deliberately: the shell should be made to decide
/// how to show a new security event, and a `_` arm that quietly renders one
/// as nothing is a worse outcome than a compile error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    /// A plaintext document was written out as an encrypted `.bpadx`.
    DocumentEncrypted,
    /// An encrypted document was deliberately written back out in clear —
    /// the one event here that describes protection being *removed*, and the
    /// reason "decrypted" is a separate verb from "unlocked".
    DocumentDecrypted,
    /// A correct passphrase opened an encrypted document for editing.
    DocumentUnlocked,
    /// A passphrase was rejected.
    ///
    /// `attempt` is what turns a nuisance into a signal: one failure is a
    /// typo, and forty in a row is somebody working through a list. Counting
    /// them is the only way the history can show the difference, and the
    /// count is safe to keep in a way the attempted passphrase never is.
    UnlockFailed { attempt: u32 },
    /// The document's security profile was changed.
    ///
    /// Both ends are recorded because the direction is the interesting part:
    /// a move down from Confidential to Standard re-enables everything the
    /// profile was switched on to stop.
    SecurityProfileChanged {
        from: ProfileLabel,
        to: ProfileLabel,
    },
    /// Privacy Mode was switched on for the session.
    PrivacyModeEntered,
    /// Privacy Mode was switched off, which restores every document to
    /// whatever its own profile permits — worth a line for the same reason a
    /// profile downgrade is.
    PrivacyModeLeft,
    /// A secret scan completed.
    ///
    /// The count only. Where the findings were is in the document, which the
    /// user has in front of them; putting the offsets here would make the log
    /// a map to the credentials in a file it sits next to.
    SecretScanFinished { findings: u32 },
    /// A signature was produced over the document.
    SignatureMade,
    /// A signature was checked.
    ///
    /// `valid` is a flag rather than two variants so that a caller cannot
    /// record a check without recording its outcome — a verification event
    /// that omitted the answer would be worse than no event, because it reads
    /// as reassurance.
    SignatureVerified { valid: bool },
    /// A redaction was applied.
    ///
    /// How much went, not what: `bytes_removed` lets the history show that a
    /// redaction was substantial without becoming a record of the removed
    /// text — which, being the text somebody chose to destroy, is the most
    /// sensitive content the product ever handles.
    RedactionApplied { spans: u32, bytes_removed: u64 },
}

impl Event {
    /// A short fixed name, for grouping or filtering in the UI.
    ///
    /// Separate from [`Event::describe`] because a filter wants a stable
    /// noun and a history line wants a sentence, and conflating them means
    /// the filter list changes whenever the wording is improved.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::DocumentEncrypted => "encrypted",
            Self::DocumentDecrypted => "decrypted",
            Self::DocumentUnlocked => "unlocked",
            Self::UnlockFailed { .. } => "unlock failed",
            Self::SecurityProfileChanged { .. } => "profile changed",
            Self::PrivacyModeEntered => "privacy mode on",
            Self::PrivacyModeLeft => "privacy mode off",
            Self::SecretScanFinished { .. } => "secret scan",
            Self::SignatureMade => "signed",
            Self::SignatureVerified { .. } => "signature checked",
            Self::RedactionApplied { .. } => "redacted",
        }
    }

    /// One line for a human reading the history.
    ///
    /// Composed here rather than in `bp-ui` so the wording is testable
    /// without a window, and so there is exactly one place to check that no
    /// sentence ever grew a field it should not have.
    #[must_use]
    pub fn describe(self) -> String {
        match self {
            Self::DocumentEncrypted => "document encrypted".to_owned(),
            Self::DocumentDecrypted => "document decrypted to plaintext".to_owned(),
            Self::DocumentUnlocked => "document unlocked".to_owned(),
            Self::UnlockFailed { attempt } => {
                format!("passphrase rejected (attempt {attempt})")
            }
            Self::SecurityProfileChanged { from, to } => {
                format!(
                    "security profile changed from {} to {}",
                    from.name(),
                    to.name()
                )
            }
            Self::PrivacyModeEntered => "Privacy Mode on".to_owned(),
            Self::PrivacyModeLeft => "Privacy Mode off".to_owned(),
            Self::SecretScanFinished { findings: 0 } => "secret scan found nothing".to_owned(),
            Self::SecretScanFinished { findings: 1 } => "secret scan found 1 candidate".to_owned(),
            Self::SecretScanFinished { findings } => {
                format!("secret scan found {findings} candidates")
            }
            Self::SignatureMade => "document signed".to_owned(),
            Self::SignatureVerified { valid: true } => "signature verified".to_owned(),
            Self::SignatureVerified { valid: false } => "signature did NOT verify".to_owned(),
            Self::RedactionApplied {
                spans,
                bytes_removed,
            } => format!("{spans} span(s) redacted, {bytes_removed} bytes removed"),
        }
    }
}

// ---------------------------------------------------------------------------
// Records
// ---------------------------------------------------------------------------

/// One line of history: when, to which document, and what.
///
/// Fields are private and there is no public constructor. A `Record` is
/// minted only by [`AuditLog::append`] or read back from a log, because the
/// sequence number is the log's property rather than the caller's — a shell
/// that could hand in its own would be a shell that could forge the order of
/// history, which is the one thing an append-only log is for.
///
/// **`Copy` is a constraint here, not a convenience, and it is what makes
/// this crate's central promise the compiler's problem rather than a
/// reviewer's.** A `Copy` type cannot own a `String`, a `Vec<u8>`, a
/// `PathBuf` or a `Box<str>` — which is to say, it cannot hold any text or
/// bytes acquired at run time. Adding a passphrase, a filename, a matched
/// secret or a redacted span to a `Record` or an [`Event`] is therefore not
/// a review comment somebody might miss on a busy afternoon. It is a build
/// failure. `Record`, `Event`, [`DocumentId`], [`ProfileLabel`] and
/// [`SealFailed`] are all held to it by an assertion below, kept explicit
/// because a `#[derive]` is easy to delete while chasing a compile error and
/// the deletion would look like a fix.
///
/// What `Copy` still permits is `&'static str`, and that is the intended
/// residue: text fixed when the code was written, which is why `SealFailed`
/// takes one and why every user-facing string in this crate is one. Nothing a
/// document contains becomes `&'static str` by accident.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Record {
    seq: u64,
    at: OffsetDateTime,
    document: Option<DocumentId>,
    event: Event,
}

impl Record {
    fn new(seq: u64, at: OffsetDateTime, document: Option<DocumentId>, event: Event) -> Self {
        Self {
            seq,
            at: whole_seconds(at),
            document,
            event,
        }
    }

    /// Position in the log, counting from 1.
    ///
    /// The record's identity and its order. Timestamps do not order a log —
    /// a clock can be set backwards, and a log that trusted one could be
    /// reordered by changing the system time — so this is what
    /// [`AuditLog::open`] checks.
    #[must_use]
    pub const fn seq(self) -> u64 {
        self.seq
    }

    /// When it happened, at whole-second resolution.
    ///
    /// Seconds and not nanoseconds: sub-second precision on an audit line
    /// implies the timestamps establish an order, and they do not — [`seq`]
    /// does. Recording the offset alongside the instant means a line written
    /// at 14:02 local still reads as 14:02 after the user flies somewhere
    /// else, which is how they will recognise the event they remember.
    ///
    /// [`seq`]: Record::seq
    #[must_use]
    pub const fn at(self) -> OffsetDateTime {
        self.at
    }

    /// Which document, if the event was about one.
    ///
    /// `None` for the session-wide events — Privacy Mode covers every open
    /// tab, so attributing it to one document would be a small lie in a file
    /// whose only value is being true.
    #[must_use]
    pub const fn document(self) -> Option<DocumentId> {
        self.document
    }

    /// What happened.
    #[must_use]
    pub const fn event(self) -> Event {
        self.event
    }
}

// The crate's central promise, made the compiler's problem. See `Record`:
// a `Copy` type cannot own a `String`, a `Vec<u8>` or a `PathBuf`, so no
// passphrase, filename, secret value or redacted span can be added to any of
// these without failing the build on this line.
const fn holds_no_owned_data<T: Copy>() {}

const _: () = {
    holds_no_owned_data::<Record>();
    holds_no_owned_data::<Event>();
    holds_no_owned_data::<DocumentId>();
    holds_no_owned_data::<ProfileLabel>();
    holds_no_owned_data::<SealFailed>();
    holds_no_owned_data::<NotWritten>();
    holds_no_owned_data::<Appended>();
};

/// Drop sub-second precision, keeping the offset.
///
/// Total by construction: 0 is always a valid nanosecond, so the fallback is
/// unreachable — but it is a fallback rather than an `unwrap`, because a
/// panic inside an audit log is a panic at the worst possible moment.
fn whole_seconds(at: OffsetDateTime) -> OffsetDateTime {
    at.replace_nanosecond(0).unwrap_or(at)
}

/// What actually goes on a line.
///
/// `time` is built here without its `serde` feature, so the timestamp is
/// written as two integers rather than a format this crate does not control.
/// That is the better outcome anyway: the on-disk shape of an audit log
/// should not change because a dependency changed its default encoding.
#[derive(Serialize, Deserialize)]
struct Wire {
    seq: u64,
    at_unix: i64,
    at_offset: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    document: Option<DocumentId>,
    event: Event,
}

impl Serialize for Record {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Wire {
            seq: self.seq,
            at_unix: self.at.unix_timestamp(),
            at_offset: self.at.offset().whole_seconds(),
            document: self.document,
            event: self.event,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Record {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error as _;

        let wire = Wire::deserialize(deserializer)?;
        let offset = UtcOffset::from_whole_seconds(wire.at_offset)
            .map_err(|_| D::Error::custom("audit timestamp has an impossible UTC offset"))?;
        let at = OffsetDateTime::from_unix_timestamp(wire.at_unix)
            .map_err(|_| D::Error::custom("audit timestamp is outside the representable range"))?
            .to_offset(offset);
        Ok(Self {
            seq: wire.seq,
            at,
            document: wire.document,
            event: wire.event,
        })
    }
}

// ---------------------------------------------------------------------------
// Where the log is allowed to go
// ---------------------------------------------------------------------------

/// Where a document's audit history may be kept, given its policy.
///
/// Ordered permissive-first, the same convention every axis in `bp-security`
/// follows, so "at least as strict as" stays a comparison rather than a
/// table — and so the property that tightening a profile never loosens the
/// log is checkable rather than asserted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Destination {
    /// A readable file of JSON lines. Today's behaviour, and Standard's.
    PlaintextFile,
    /// A file of sealed lines, readable only with the document's key.
    Sealed,
    /// Nowhere. Nothing about this document may outlive the session, so
    /// `bp-audit` writes nothing and keeps nothing; a shell that wants to
    /// show the user what happened this session holds it in memory itself.
    SessionOnly,
}

impl Destination {
    /// Resolve a policy to a destination.
    ///
    /// Two of `bp-security`'s axes answer the two questions an audit log
    /// actually asks, and no third axis is invented for it:
    ///
    /// * **May anything at all be recorded about this document?** That is
    ///   `metadata`, whose whole subject is what the machine writes down
    ///   about a document rather than in it. ADR-0019 refused to store an
    ///   extracted title on exactly this ground, and an audit trail is the
    ///   same kind of object: a durable local record of a document's
    ///   existence and use. `Metadata::Disabled` therefore means no file.
    /// * **May a derived artefact sit on the disk in clear?** That is
    ///   `recovery`, the axis that already governs the other file this
    ///   product writes beside a document. A profile unwilling to leave an
    ///   unencrypted journal is not willing to leave an unencrypted audit
    ///   trail either.
    ///
    /// Reading it as a table: Standard writes a plaintext file, Private
    /// writes a sealed one, and Confidential and Maximum write nothing.
    /// `Recovery::Disabled` with metadata still permitted lands on `Sealed`
    /// rather than `PlaintextFile`, because a profile that has switched off
    /// plaintext recovery has stated its view of plaintext artefacts.
    #[must_use]
    pub const fn for_policy(policy: Policy) -> Self {
        match policy.metadata {
            Metadata::Disabled => Self::SessionOnly,
            Metadata::Summary | Metadata::PathOnly => match policy.recovery {
                Recovery::Plaintext => Self::PlaintextFile,
                Recovery::Encrypted | Recovery::Disabled => Self::Sealed,
            },
        }
    }

    /// The destination actually in force, given the session's Privacy Mode.
    ///
    /// The one function callers should go through, for the reason
    /// [`Security::policy_under`] gives: a caller that resolves a document's
    /// own policy directly is a caller Privacy Mode does not reach, and an
    /// audit log still writing filenames-by-id to disk while the user is
    /// screen-sharing is precisely what Privacy Mode was switched on to stop.
    #[must_use]
    pub fn in_force(security: &Security, privacy: Privacy) -> Self {
        Self::for_policy(security.policy_under(privacy))
    }

    /// Whether this destination puts anything on the disk.
    ///
    /// For the security inspector, which should be able to tell the user
    /// "nothing is being written" without knowing which variant means that.
    #[must_use]
    pub const fn writes_to_disk(self) -> bool {
        !matches!(self, Self::SessionOnly)
    }
}

// ---------------------------------------------------------------------------
// The crypto seam
// ---------------------------------------------------------------------------

/// Something that can seal an audit line with the document's own key.
///
/// A trait rather than a direct dependency on `bp-crypto`, for the reason
/// ADR-0020 gives about `bp-security` itself: this crate decides *that* a
/// line must be sealed, and something else performs it. The practical payoff
/// is that the refusal path — sealing required, no key available — is
/// exercised by tests here without a passphrase, an Argon2id derivation or a
/// second crate in the loop.
///
/// The implementation the shell supplies wraps the document's `.bpadx`
/// envelope, so an audit log is readable by exactly the people who can read
/// the document it describes. That is the correct access rule and it comes
/// out for free.
///
/// [`PassphraseSealer`] is that implementation. It seals each line into its
/// own `.bpadx` envelope rather than re-sealing the file, which is what keeps
/// the log append-only and a write O(1); see its module for why an audit log
/// is framed differently from ADR-0022's recovery journal even though both are
/// sealed with the same key.
pub trait Sealer {
    /// Seal one line. The output may be any bytes; the log hex-encodes them.
    ///
    /// # Errors
    /// If the key is unusable or the underlying primitive fails.
    fn seal(&self, plaintext: &[u8]) -> Result<Vec<u8>, SealFailed>;

    /// Recover one sealed line.
    ///
    /// # Errors
    /// If the key is wrong, or the line has been altered — which an
    /// authenticated envelope detects, and which is the tamper-evidence a
    /// sealed log has and a plaintext one does not.
    fn unseal(&self, sealed: &[u8]) -> Result<Vec<u8>, SealFailed>;
}

/// A sealer could not do its job.
///
/// The reason is a `&'static str` rather than a `String` on purpose. Error
/// messages are the classic route by which the thing that failed ends up in
/// a log file, and a type that can only hold text fixed at compile time
/// cannot carry a key, a passphrase or a fragment of ciphertext no matter how
/// carelessly an implementation is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{reason}")]
pub struct SealFailed {
    reason: &'static str,
}

impl SealFailed {
    /// Explain a failure in words chosen when the code was written.
    #[must_use]
    pub const fn new(reason: &'static str) -> Self {
        Self { reason }
    }

    /// The explanation, for showing the user.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        self.reason
    }
}

// ---------------------------------------------------------------------------
// Outcomes and errors
// ---------------------------------------------------------------------------

/// Why an event was recorded but not written down.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NotWritten {
    /// The document's policy keeps no durable record of it. Working as asked.
    ProfileForbidsIt,
    /// The policy requires a sealed log and no key was supplied.
    ///
    /// Refused rather than downgraded to plaintext, which ADR-0020 states in
    /// as many words: a security control that quietly weakens itself is worse
    /// than an absent one, because the user has been told it is on.
    NoSealer,
}

impl NotWritten {
    /// What to tell the user, if anything.
    ///
    /// A profile doing what it was set to do is not news, and a notice for
    /// every such event would train the user to dismiss the notices that
    /// matter. A profile that *cannot* be honoured is news, and the message
    /// says what fixes it — the same test `bp-history` applies to its own
    /// refusals.
    #[must_use]
    pub const fn notice(self) -> Option<&'static str> {
        match self {
            Self::ProfileForbidsIt => None,
            Self::NoSealer => Some(
                "this profile keeps the security history sealed -- encrypt this \
                 document (Security menu) so its security events can be kept",
            ),
        }
    }
}

/// What [`AuditLog::append`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Appended {
    /// Written to the log, at this sequence number.
    Persisted(u64),
    /// Deliberately not written. The caller may still show it for the
    /// session; nothing about it reaches the disk.
    NotWritten(NotWritten),
}

/// Something went wrong reading or writing the log.
///
/// Note what these messages do *not* contain: the content of the offending
/// line. An audit log is read when something has already gone wrong, and an
/// error that quotes the line is one more place its contents get copied — to
/// a terminal, a bug report, a screenshot. The line number is enough to find
/// it, and the user already has the file.
#[derive(Debug, thiserror::Error)]
pub enum AuditError {
    /// The log file could not be read or written.
    #[error("the security history at {path} could not be used: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// A line is not a record this version understands.
    #[error(
        "line {line} of the security history is not a record this version \
         understands; the file may have been edited or truncated"
    )]
    Malformed { line: usize },
    /// A line is sealed and no key was supplied to open it.
    #[error(
        "line {line} of the security history is sealed; unlock the document \
         it belongs to in order to read its history"
    )]
    Sealed { line: usize },
    /// The sequence is not the unbroken run 1, 2, 3, ... that an append-only
    /// log produces, so at least one line has been removed or reordered.
    #[error(
        "the security history has been altered: line {line} should be entry \
         {expected} and says it is entry {found}"
    )]
    OutOfOrder {
        line: usize,
        expected: u64,
        found: u64,
    },
    /// The sealer refused.
    #[error("the security history could not be sealed or opened: {0}")]
    Seal(#[from] SealFailed),
    /// The sequence has reached `u64::MAX`.
    ///
    /// Unreachable in practice and handled anyway, because the alternative to
    /// an error here is wrapping to zero — an audit log that silently starts
    /// renumbering is one that silently reports itself as tampered with for
    /// ever after.
    #[error("the security history is full and cannot record more entries")]
    SequenceExhausted,
}

// ---------------------------------------------------------------------------
// The log
// ---------------------------------------------------------------------------

/// An append-only history of security events, on disk as one line each.
///
/// **What append-only means here, precisely, because a vague claim is worse
/// than a narrow one.**
///
/// Enforced by this crate:
///
/// * There is no public operation that edits, removes or truncates an entry.
///   The type offers exactly one mutation, [`append`], and reading.
/// * Every write opens the file in append mode, so the operating system
///   positions it at the end. The crate never seeks, never truncates, and
///   writes each record as a single line-plus-newline in one call, so a
///   second process appending concurrently interleaves whole entries rather
///   than shredding one.
/// * Each entry is numbered, and [`open`] refuses a file whose numbers are
///   not the unbroken run 1, 2, 3, .... Deleting or reordering any line but
///   the last is therefore detected and reported, loudly, as tampering.
/// * Each entry is flushed to the device before `append` returns. An audit
///   line still in the page cache when the machine loses power is exactly
///   the line about the thing that went wrong.
///
/// **Not enforced, and no pretence otherwise.** A process with write access
/// to the file can truncate the tail, or rewrite the whole file with
/// consistent numbering, and nothing inside this crate can tell. Detecting
/// that needs a per-entry authentication chain keyed by something the editor
/// does not hold — a signing key (specs.md section 15) or an append-only
/// store the user's own account cannot rewrite. A sealed log gets part of the
/// way there for free, since each line is authenticated by the document's
/// envelope and cannot be *altered* undetectably, only removed. Deliberately
/// no home-rolled hash chain: specs.md section 15 says no custom
/// cryptography, and a chain that looks like tamper-evidence but is not is
/// worse than the honest gap.
///
/// [`append`]: AuditLog::append
/// [`open`]: AuditLog::open
#[derive(Debug)]
pub struct AuditLog {
    path: PathBuf,
    records: Vec<Record>,
}

impl AuditLog {
    /// Open the history at `path`, reading and checking whatever is there.
    ///
    /// The only constructor, which is what makes the sequence trustworthy: a
    /// log you can only get by reading the existing one is a log you cannot
    /// accidentally restart from 1 over the top of a real history.
    ///
    /// A missing file is an empty history and not an error — the first
    /// security event a user ever triggers has to work.
    ///
    /// `sealer` is needed only if the file contains sealed lines; a plaintext
    /// history opens with `None`. A file holding both forms is normal and is
    /// read as such, because a document whose profile was tightened has
    /// exactly that: plaintext lines from before, sealed lines after. It is
    /// also why the sealed lines are not silently skipped when no key is
    /// given — a history that quietly showed you only the readable half would
    /// be a history that hid the interesting part.
    ///
    /// # Errors
    /// [`AuditError::Io`] if the file cannot be read, [`AuditError::Malformed`]
    /// or [`AuditError::OutOfOrder`] if it has been damaged or edited, and
    /// [`AuditError::Sealed`] if it holds sealed lines and no key was given.
    pub fn open(path: PathBuf, sealer: Option<&dyn Sealer>) -> Result<Self, AuditError> {
        let records = Self::read(&path, sealer)?;
        Ok(Self { path, records })
    }

    /// Where the history is kept, so the UI can say.
    #[must_use]
    pub fn location(&self) -> &Path {
        &self.path
    }

    /// Everything in the history, oldest first.
    ///
    /// Includes what this session appended, so a caller does not have to
    /// re-open the file to show the line it just wrote.
    #[must_use]
    pub fn records(&self) -> &[Record] {
        &self.records
    }

    /// The number the next persisted entry will carry.
    ///
    /// Public so a security inspector can show how long the history is
    /// without the caller counting, and so a test can pin the boundary.
    #[must_use]
    pub fn next_sequence(&self) -> u64 {
        self.records.last().map_or(1, |r| r.seq.saturating_add(1))
    }

    /// Record that something happened, if the policy permits writing it down.
    ///
    /// `at` is passed in and never read from a clock: that is what lets every
    /// ordering and formatting question here be settled by a test rather than
    /// by waiting.
    ///
    /// Pass the policy *in force* — `security.policy_under(privacy)`. A
    /// caller that passes `security.policy()` is a caller Privacy Mode does
    /// not reach.
    ///
    /// Returns [`Appended::NotWritten`] rather than an error when the profile
    /// says no, because that is not a failure: the log is doing what it was
    /// configured to do, and the caller decides whether the reason is worth
    /// a notice via [`NotWritten::notice`]. The event is still the caller's
    /// to display for the session; nothing about it touches the disk.
    ///
    /// # Errors
    /// [`AuditError::Io`] if the write fails, [`AuditError::Seal`] if sealing
    /// fails, and [`AuditError::SequenceExhausted`] at the end of `u64`.
    pub fn append(
        &mut self,
        at: OffsetDateTime,
        document: Option<DocumentId>,
        event: Event,
        policy: Policy,
        sealer: Option<&dyn Sealer>,
    ) -> Result<Appended, AuditError> {
        let sealer = match Destination::for_policy(policy) {
            Destination::SessionOnly => {
                return Ok(Appended::NotWritten(NotWritten::ProfileForbidsIt));
            }
            Destination::PlaintextFile => None,
            Destination::Sealed => match sealer {
                Some(sealer) => Some(sealer),
                None => return Ok(Appended::NotWritten(NotWritten::NoSealer)),
            },
        };

        let seq = self.next_sequence();
        // Reaching `u64::MAX` must not wrap into a number already used: the
        // history would then read as reordered for the rest of its life.
        if let Some(last) = self.records.last()
            && seq == last.seq
        {
            return Err(AuditError::SequenceExhausted);
        }

        let record = Record::new(seq, at, document, event);
        // Unreachable: a `Record` is numbers, flags and unit-like enums, none
        // of which serde can fail on. Reported as I/O rather than given an
        // error of its own, because a variant nothing can produce is a variant
        // nobody can test -- and this is not the crate to carry dead paths.
        let json = serde_json::to_string(&record).map_err(|e| AuditError::Io {
            path: self.path.clone(),
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, e),
        })?;
        let line = match sealer {
            None => json,
            Some(sealer) => hex_encode(&sealer.seal(json.as_bytes())?),
        };

        self.write_line(&line)?;
        self.records.push(record);
        Ok(Appended::Persisted(seq))
    }

    /// Append one line, durably.
    ///
    /// One `write_all` for the line and its newline together, so a competing
    /// appender cannot land inside it, and `sync_all` before returning, so the
    /// entry survives the crash it might be describing.
    fn write_line(&self, line: &str) -> Result<(), AuditError> {
        let io = |source| AuditError::Io {
            path: self.path.clone(),
            source,
        };

        if let Some(parent) = self.path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(io)?;

        let mut bytes = String::with_capacity(line.len() + 1);
        bytes.push_str(line);
        bytes.push('\n');
        file.write_all(bytes.as_bytes()).map_err(io)?;
        file.sync_all().map_err(io)
    }

    /// Read and validate an existing history, all of it, before returning any
    /// of it.
    ///
    /// A thin wrapper over [`read_lazy`], kept so there is exactly one
    /// implementation of the read-and-decrypt logic. [`open`] needs the whole
    /// history anyway -- it cannot trust [`next_sequence`] until every line
    /// has been checked -- so it collects the iterator to completion here
    /// rather than driving it by hand.
    ///
    /// [`read_lazy`]: AuditLog::read_lazy
    /// [`open`]: AuditLog::open
    /// [`next_sequence`]: AuditLog::next_sequence
    fn read(path: &Path, sealer: Option<&dyn Sealer>) -> Result<Vec<Record>, AuditError> {
        Self::read_lazy(path, sealer).collect()
    }

    /// Read a history lazily, one record at a time.
    ///
    /// Neither the file nor a sealed line's decryption happens until the
    /// caller asks for it by calling `.next()` -- see ADR-0036. Where
    /// [`read`] (and, through it, [`open`]) must see every line to validate
    /// the sequence before it can trust the result, a caller that only wants
    /// the newest few records, or wants to show progress while decrypting a
    /// long history, can drive this directly and stop whenever it likes,
    /// paying for exactly the Argon2id derivations it asked for and none it
    /// did not.
    ///
    /// A missing file yields no records and no error, matching [`open`]: the
    /// first call to `.next()` returns `None` immediately.
    ///
    /// [`read`]: AuditLog::read
    /// [`open`]: AuditLog::open
    #[must_use]
    pub fn read_lazy<'a>(path: &Path, sealer: Option<&'a dyn Sealer>) -> Records<'a> {
        Records {
            path: path.to_path_buf(),
            sealer,
            state: RecordsState::Unopened,
            expected: 1,
            stopped: false,
        }
    }

    /// Turn one line back into a record, sealed or not.
    ///
    /// The two forms are told apart by the leading `{` of JSON, so a log
    /// whose profile changed mid-life reads back whole rather than needing
    /// the caller to know where the change happened.
    fn parse_line(
        raw: &str,
        line: usize,
        sealer: Option<&dyn Sealer>,
    ) -> Result<Record, AuditError> {
        if raw.starts_with('{') {
            return serde_json::from_str(raw).map_err(|_| AuditError::Malformed { line });
        }
        let sealed = hex_decode(raw).ok_or(AuditError::Malformed { line })?;
        let sealer = sealer.ok_or(AuditError::Sealed { line })?;
        let plain = sealer.unseal(&sealed)?;
        serde_json::from_slice(&plain).map_err(|_| AuditError::Malformed { line })
    }
}

// ---------------------------------------------------------------------------
// Reading lazily
// ---------------------------------------------------------------------------

/// A history read one record at a time, decrypting a sealed line only when
/// the caller asks for it.
///
/// Built by [`AuditLog::read_lazy`]; see there, and ADR-0036, for why this
/// exists instead of the `Vec<Record>` [`AuditLog::read`] returns, which a
/// caller has no way to stop partway through.
///
/// # After an error, the iterator is finished
///
/// Once `next()` yields `Some(Err(_))`, every later call returns `None`
/// rather than skipping the bad line and continuing, or repeating the same
/// error forever. The sequence-order check depends on unbroken state --
/// "the next record is one more than the last one this iterator saw" -- so
/// there is no line past a malformed or out-of-order one that this iterator
/// could resume validating. Stopping for good is the same choice
/// [`AuditLog::open`] already makes about the file as a whole; this just
/// makes it per-iterator instead of per-file.
pub struct Records<'a> {
    path: PathBuf,
    sealer: Option<&'a dyn Sealer>,
    state: RecordsState,
    expected: u64,
    stopped: bool,
}

/// Whether the file behind a [`Records`] has been opened yet.
///
/// Kept out of `Records` itself so the file read genuinely does not happen
/// until the first call to `.next()` -- constructing a `Records` touches
/// neither the filesystem nor the sealer.
enum RecordsState {
    /// Nothing has touched the filesystem yet.
    Unopened,
    /// The file has been split into lines; each is parsed, and (if sealed)
    /// decrypted, only when its turn comes.
    Reading {
        lines: std::vec::IntoIter<String>,
        line: usize,
    },
}

impl Iterator for Records<'_> {
    type Item = Result<Record, AuditError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.stopped {
            return None;
        }

        if matches!(self.state, RecordsState::Unopened) {
            match std::fs::read_to_string(&self.path) {
                Ok(text) => {
                    let lines: Vec<String> = text.lines().map(str::to_owned).collect();
                    self.state = RecordsState::Reading {
                        lines: lines.into_iter(),
                        line: 0,
                    };
                }
                // A history that has never been written is an empty one, not
                // a problem to report to somebody who has done nothing
                // wrong.
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    self.stopped = true;
                    return None;
                }
                Err(source) => {
                    self.stopped = true;
                    return Some(Err(AuditError::Io {
                        path: self.path.clone(),
                        source,
                    }));
                }
            }
        }

        let RecordsState::Reading { lines, line } = &mut self.state else {
            unreachable!("the branch above always leaves Unopened for Reading");
        };

        loop {
            let raw = lines.next()?;
            *line += 1;
            let raw = raw.trim();
            // A crash between the newline and the next entry can leave a
            // blank line. It carries no claim, so it is not tampering.
            if raw.is_empty() {
                continue;
            }

            let record = match AuditLog::parse_line(raw, *line, self.sealer) {
                Ok(record) => record,
                Err(err) => {
                    self.stopped = true;
                    return Some(Err(err));
                }
            };
            // Exactly 1, 2, 3, ... -- see `Record::seq`'s note on what this
            // does and does not catch.
            if record.seq != self.expected {
                self.stopped = true;
                return Some(Err(AuditError::OutOfOrder {
                    line: *line,
                    expected: self.expected,
                    found: record.seq,
                }));
            }
            self.expected = self.expected.saturating_add(1);
            return Some(Ok(record));
        }
    }
}

/// Hex, so sealed bytes can share a line-oriented file with JSON ones.
///
/// Hex and not base64 because it needs no dependency and no alphabet
/// argument, and an audit log is small enough that doubling it costs nothing
/// anyone will notice. This is an encoding, not a cipher, and calling it one
/// would be the sort of thing specs.md section 15 forbids.
fn hex_encode(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[usize::from(byte >> 4)] as char);
        out.push(DIGITS[usize::from(byte & 0x0f)] as char);
    }
    out
}

/// The inverse, rejecting anything that is not exactly hex.
fn hex_decode(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.chunks_exact(2) {
        let hi = (pair[0] as char).to_digit(16)?;
        let lo = (pair[1] as char).to_digit(16)?;
        out.push(u8::try_from(hi * 16 + lo).ok()?);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    use bp_security::{Clipboard, Embeddings, Network, Profile, TemporaryFiles, Zeroise};
    use proptest::prelude::*;

    // -- helpers -----------------------------------------------------------

    /// A sealer that is not cryptography and does not pretend to be: it
    /// reverses the bytes. What the tests need from it is only that its
    /// output differs from its input and round-trips, so that "the plaintext
    /// never reached the file" is a claim about the log rather than about a
    /// cipher.
    struct Reversing;

    impl Sealer for Reversing {
        fn seal(&self, plaintext: &[u8]) -> Result<Vec<u8>, SealFailed> {
            Ok(plaintext.iter().rev().copied().collect())
        }
        fn unseal(&self, sealed: &[u8]) -> Result<Vec<u8>, SealFailed> {
            Ok(sealed.iter().rev().copied().collect())
        }
    }

    /// A sealer that always refuses, for the path where the key is present
    /// but unusable.
    struct Broken;

    impl Sealer for Broken {
        fn seal(&self, _: &[u8]) -> Result<Vec<u8>, SealFailed> {
            Err(SealFailed::new("the document key was rejected"))
        }
        fn unseal(&self, _: &[u8]) -> Result<Vec<u8>, SealFailed> {
            Err(SealFailed::new("the document key was rejected"))
        }
    }

    /// [`Reversing`], but counting how many times a line was actually
    /// unsealed -- the only way to see from outside whether `Records`
    /// touched a line it was never asked for.
    struct CountingUnseal<'a> {
        calls: &'a std::cell::Cell<usize>,
    }

    impl Sealer for CountingUnseal<'_> {
        fn seal(&self, plaintext: &[u8]) -> Result<Vec<u8>, SealFailed> {
            Reversing.seal(plaintext)
        }
        fn unseal(&self, sealed: &[u8]) -> Result<Vec<u8>, SealFailed> {
            self.calls.set(self.calls.get() + 1);
            Reversing.unseal(sealed)
        }
    }

    fn at(seconds: i64) -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(seconds).expect("timestamp in range")
    }

    fn log_in(dir: &tempfile::TempDir) -> AuditLog {
        AuditLog::open(dir.path().join("security.log"), None).expect("a fresh log opens")
    }

    // -- strategies --------------------------------------------------------

    fn any_profile_label() -> impl Strategy<Value = ProfileLabel> {
        prop_oneof![
            Just(ProfileLabel::Standard),
            Just(ProfileLabel::Private),
            Just(ProfileLabel::Confidential),
            Just(ProfileLabel::Maximum),
            Just(ProfileLabel::Custom),
        ]
    }

    /// Every variant, with payloads spanning their full range.
    ///
    /// Kept exhaustive by hand and checked by `every_event_variant_is_generated`
    /// below, because a strategy that quietly stopped covering a variant would
    /// take the no-secrets property down with it without failing anything.
    fn any_event() -> impl Strategy<Value = Event> {
        prop_oneof![
            Just(Event::DocumentEncrypted),
            Just(Event::DocumentDecrypted),
            Just(Event::DocumentUnlocked),
            any::<u32>().prop_map(|attempt| Event::UnlockFailed { attempt }),
            (any_profile_label(), any_profile_label())
                .prop_map(|(from, to)| Event::SecurityProfileChanged { from, to }),
            Just(Event::PrivacyModeEntered),
            Just(Event::PrivacyModeLeft),
            any::<u32>().prop_map(|findings| Event::SecretScanFinished { findings }),
            Just(Event::SignatureMade),
            any::<bool>().prop_map(|valid| Event::SignatureVerified { valid }),
            (any::<u32>(), any::<u64>()).prop_map(|(spans, bytes_removed)| {
                Event::RedactionApplied {
                    spans,
                    bytes_removed,
                }
            }),
        ]
    }

    fn any_record() -> impl Strategy<Value = Record> {
        (
            any::<u64>(),
            // Year 1 to year 9999, which is the range `time` represents
            // without its `large-dates` feature.
            -62_135_596_800i64..=253_402_300_799i64,
            -93_599i32..=93_599i32,
            // Sub-second precision, so the round trip is only true because
            // `Record::new` drops it rather than by accident.
            0u32..1_000_000_000u32,
            proptest::option::of(any::<u64>()),
            any_event(),
        )
            .prop_map(|(seq, unix, offset, nanos, document, event)| {
                let at = OffsetDateTime::from_unix_timestamp(unix)
                    .expect("timestamp in range")
                    .replace_nanosecond(nanos)
                    .expect("nanosecond in range")
                    .to_offset(UtcOffset::from_whole_seconds(offset).expect("offset in range"));
                Record::new(seq, at, document.map(DocumentId::new), event)
            })
    }

    fn any_axis_policy() -> impl Strategy<Value = Policy> {
        (
            prop_oneof![
                Just(Recovery::Plaintext),
                Just(Recovery::Encrypted),
                Just(Recovery::Disabled)
            ],
            prop_oneof![
                Just(Clipboard::Persistent),
                Just(Clipboard::InMemory),
                Just(Clipboard::Disabled)
            ],
            prop_oneof![
                Just(Metadata::Summary),
                Just(Metadata::PathOnly),
                Just(Metadata::Disabled)
            ],
            prop_oneof![
                Just(Embeddings::Cloud),
                Just(Embeddings::Local),
                Just(Embeddings::None)
            ],
            prop_oneof![Just(Network::Allowed), Just(Network::Denied)],
            prop_oneof![Just(TemporaryFiles::Allowed), Just(TemporaryFiles::Denied)],
            prop_oneof![Just(Zeroise::Off), Just(Zeroise::On)],
        )
            .prop_map(
                |(recovery, clipboard, metadata, embeddings, network, temporary_files, zeroise)| {
                    Policy {
                        recovery,
                        clipboard,
                        metadata,
                        embeddings,
                        network,
                        temporary_files,
                        zeroise,
                    }
                },
            )
    }

    // -- the no-secrets guarantee -----------------------------------------

    /// Every word an audit record is permitted to contain.
    ///
    /// Field names, event tags and profile labels. Nothing else: a record is
    /// otherwise numbers and flags. **Adding a `String` to any type reachable
    /// from `Record` makes the test below fail**, and the only way to make it
    /// pass again is to write the new free text into this list — which is a
    /// deliberate act with this comment sitting above it, not something that
    /// slips through a review as one more convenient field.
    const VOCABULARY: &[&str] = &[
        // Wire field names.
        "seq",
        "at_unix",
        "at_offset",
        "document",
        "event",
        // Event tag and payload field names.
        "kind",
        "attempt",
        "from",
        "to",
        "findings",
        "valid",
        "spans",
        "bytes_removed",
        // Event tags.
        "document_encrypted",
        "document_decrypted",
        "document_unlocked",
        "unlock_failed",
        "security_profile_changed",
        "privacy_mode_entered",
        "privacy_mode_left",
        "secret_scan_finished",
        "signature_made",
        "signature_verified",
        "redaction_applied",
        // Profile labels.
        "standard",
        "private",
        "confidential",
        "maximum",
        "custom",
    ];

    /// Collect every string anywhere in a JSON value, key or value.
    fn strings_in(value: &serde_json::Value, into: &mut Vec<String>) {
        match value {
            serde_json::Value::String(s) => into.push(s.clone()),
            serde_json::Value::Array(items) => {
                for item in items {
                    strings_in(item, into);
                }
            }
            serde_json::Value::Object(map) => {
                for (key, item) in map {
                    into.push(key.clone());
                    strings_in(item, into);
                }
            }
            _ => {}
        }
    }

    proptest! {
        #[test]
        fn a_record_can_only_ever_write_words_from_a_closed_vocabulary(record in any_record()) {
            // The crate's central claim, checked rather than promised: an
            // audit line is numbers plus a fixed dictionary. There is no
            // shape of `Record` that puts a passphrase, a filename, a secret
            // value or a line of the document on disk, because there is
            // nowhere in the type to put one.
            let json = serde_json::to_value(record).expect("a record serialises");
            let mut found = Vec::new();
            strings_in(&json, &mut found);
            for word in found {
                prop_assert!(
                    VOCABULARY.contains(&word.as_str()),
                    "an audit record wrote the free text {word:?}; \
                     an audit log must record that something happened, never \
                     the sensitive thing it happened to"
                );
            }
        }
    }

    #[test]
    fn every_event_variant_is_generated_by_the_strategy() {
        // Guards the property above. A variant missing from `any_event`
        // would never be serialised, so a `String` added to it would sail
        // through the vocabulary check.
        use proptest::strategy::ValueTree as _;

        let mut runner = proptest::test_runner::TestRunner::deterministic();
        let strategy = any_event();
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..2_000 {
            let event = strategy
                .new_tree(&mut runner)
                .expect("a value is generated")
                .current();
            seen.insert(event.label());
        }
        let expected = [
            "encrypted",
            "decrypted",
            "unlocked",
            "unlock failed",
            "profile changed",
            "privacy mode on",
            "privacy mode off",
            "secret scan",
            "signed",
            "signature checked",
            "redacted",
        ];
        for label in expected {
            assert!(seen.contains(label), "`any_event` never generated {label}");
        }
        assert_eq!(
            seen.len(),
            expected.len(),
            "an event variant was added without being generated or listed here"
        );
    }

    #[test]
    fn the_file_on_disk_never_names_the_document_or_its_passphrase() {
        // The same guarantee, checked where it actually matters: the bytes.
        // The document below has a name that would be ruinous to disclose and
        // a passphrase to match, and neither is anywhere near the API -- the
        // log takes a number.
        let dir = tempfile::tempdir().expect("a temporary directory");
        let mut log = log_in(&dir);
        let policy = Profile::Standard.policy();
        let secrets = [
            "Divorce settlement.bpadx",
            "hunter2",
            "AKIAIOSFODNN7EXAMPLE",
            "C:/Users/danie/Documents",
        ];

        log.append(
            at(1_700_000_000),
            Some(DocumentId::new(0x7f3a_1c04)),
            Event::DocumentUnlocked,
            policy,
            None,
        )
        .expect("Standard writes a plaintext line");
        log.append(
            at(1_700_000_060),
            Some(DocumentId::new(0x7f3a_1c04)),
            Event::UnlockFailed { attempt: 3 },
            policy,
            None,
        )
        .expect("the second line is written");
        log.append(
            at(1_700_000_120),
            Some(DocumentId::new(0x7f3a_1c04)),
            Event::SecretScanFinished { findings: 2 },
            policy,
            None,
        )
        .expect("the third line is written");

        let written = std::fs::read_to_string(log.location()).expect("the log is readable");
        for secret in secrets {
            assert!(
                !written.contains(secret),
                "the audit file contains {secret:?}"
            );
        }
    }

    // -- the security profile ---------------------------------------------

    #[test]
    fn each_named_profile_sends_the_history_where_adr_0020_says() {
        // The table, readable against the profile rows in `bp-security`.
        assert_eq!(
            Destination::for_policy(Profile::Standard.policy()),
            Destination::PlaintextFile,
            "Standard is today's behaviour and must stay a no-op"
        );
        assert_eq!(
            Destination::for_policy(Profile::Private.policy()),
            Destination::Sealed
        );
        assert_eq!(
            Destination::for_policy(Profile::Confidential.policy()),
            Destination::SessionOnly,
            "Confidential records nothing about the document beyond the session"
        );
        assert_eq!(
            Destination::for_policy(Profile::Maximum.policy()),
            Destination::SessionOnly,
            "a Maximum document must not leave a file saying when it was opened"
        );
    }

    #[test]
    fn the_destination_never_loosens_as_the_profile_tightens() {
        // The `bp-security` monotonicity property, extended to the axis this
        // crate adds. The failure it exists to catch is the dull one: an
        // eighth axis appears, the interesting rows get filled in, and one
        // profile is left writing more to disk than the stricter one below.
        for pair in Profile::all().windows(2) {
            let (looser, stricter) = (pair[0], pair[1]);
            assert!(
                Destination::for_policy(stricter.policy())
                    >= Destination::for_policy(looser.policy()),
                "{} keeps a looser audit history than {}",
                stricter.name(),
                looser.name()
            );
        }
    }

    proptest! {
        #[test]
        fn a_stricter_policy_never_gets_a_looser_history(
            a in any_axis_policy(),
            b in any_axis_policy(),
        ) {
            // Over every policy the type permits, including the Custom ones
            // that sit outside the named chain and are where an unclamped
            // axis would hide.
            if a.is_at_least_as_strict_as(&b) {
                prop_assert!(Destination::for_policy(a) >= Destination::for_policy(b));
            }
        }

        #[test]
        fn privacy_mode_always_takes_the_history_off_the_disk(policy in any_axis_policy()) {
            // Privacy Mode's whole guarantee, applied here: whatever a
            // document's own profile permits, nothing about it is written
            // down while the switch is on.
            let clamped = Privacy::On.clamp(policy);
            prop_assert_eq!(Destination::for_policy(clamped), Destination::SessionOnly);
            prop_assert!(!Destination::for_policy(clamped).writes_to_disk());
        }

        #[test]
        fn the_destination_in_force_is_the_document_and_the_session_together(
            policy in any_axis_policy(),
        ) {
            let security = Security::Custom(policy);
            prop_assert_eq!(
                Destination::in_force(&security, Privacy::Off),
                Destination::for_policy(policy)
            );
            prop_assert_eq!(
                Destination::in_force(&security, Privacy::On),
                Destination::SessionOnly
            );
        }
    }

    #[test]
    fn a_profile_that_forbids_a_history_leaves_no_file_at_all() {
        // Not "an empty file", not "a file of refusals". Nothing. A file that
        // exists is itself a disclosure -- it says this document was opened.
        let dir = tempfile::tempdir().expect("a temporary directory");
        let mut log = log_in(&dir);

        let outcome = log
            .append(
                at(1_700_000_000),
                Some(DocumentId::new(9)),
                Event::DocumentUnlocked,
                Profile::Maximum.policy(),
                Some(&Reversing),
            )
            .expect("a refusal is not an error");

        assert_eq!(outcome, Appended::NotWritten(NotWritten::ProfileForbidsIt));
        assert!(!log.location().exists(), "a refused history created a file");
        assert!(log.records().is_empty());
        assert_eq!(
            NotWritten::ProfileForbidsIt.notice(),
            None,
            "a profile doing what it was set to do is not news"
        );
    }

    #[test]
    fn a_profile_needing_a_sealed_history_refuses_rather_than_writing_plaintext() {
        // ADR-0020 in one test: a control that quietly weakens itself is
        // worse than an absent one. Private wants a sealed log; without a key
        // it gets none, and the user is told what fixes it.
        let dir = tempfile::tempdir().expect("a temporary directory");
        let mut log = log_in(&dir);

        let outcome = log
            .append(
                at(1_700_000_000),
                Some(DocumentId::new(9)),
                Event::DocumentUnlocked,
                Profile::Private.policy(),
                None,
            )
            .expect("a refusal is not an error");

        assert_eq!(outcome, Appended::NotWritten(NotWritten::NoSealer));
        assert!(!log.location().exists());
        let notice = NotWritten::NoSealer.notice().expect("an actionable notice");
        assert!(
            notice.contains("encrypt this document"),
            "the notice must say what fixes it, and said: {notice}"
        );
    }

    #[test]
    fn a_sealed_history_is_unreadable_without_the_key_and_exact_with_it() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("security.log");
        let mut log = AuditLog::open(path.clone(), Some(&Reversing)).expect("a fresh log opens");

        log.append(
            at(1_700_000_000),
            Some(DocumentId::new(0x2b)),
            Event::SignatureVerified { valid: false },
            Profile::Private.policy(),
            Some(&Reversing),
        )
        .expect("Private with a key writes a sealed line");

        let raw = std::fs::read_to_string(&path).expect("the log is readable");
        assert!(
            !raw.contains("signature_verified"),
            "a sealed line left its contents in clear: {raw}"
        );

        let err = AuditLog::open(path.clone(), None).expect_err("no key, no history");
        assert!(
            matches!(err, AuditError::Sealed { line: 1 }),
            "expected a sealed-line error, got {err}"
        );
        assert!(
            err.to_string().contains("unlock the document"),
            "the message must say what to do, and said: {err}"
        );

        let reopened = AuditLog::open(path, Some(&Reversing)).expect("the key opens it");
        assert_eq!(reopened.records(), log.records());
    }

    #[test]
    fn a_history_whose_profile_tightened_reads_back_whole() {
        // Plaintext lines from before the change, sealed lines after. The
        // realistic case, and the one a reader that assumed a single form
        // would silently truncate.
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("security.log");
        let mut log = AuditLog::open(path.clone(), Some(&Reversing)).expect("a fresh log opens");

        log.append(
            at(1),
            None,
            Event::DocumentEncrypted,
            Profile::Standard.policy(),
            Some(&Reversing),
        )
        .expect("plaintext line");
        log.append(
            at(2),
            None,
            Event::SecurityProfileChanged {
                from: ProfileLabel::Standard,
                to: ProfileLabel::Private,
            },
            Profile::Standard.policy(),
            Some(&Reversing),
        )
        .expect("the change itself is recorded under the old profile");
        log.append(
            at(3),
            None,
            Event::DocumentUnlocked,
            Profile::Private.policy(),
            Some(&Reversing),
        )
        .expect("sealed line");

        let reopened = AuditLog::open(path, Some(&Reversing)).expect("both forms read back");
        assert_eq!(reopened.records().len(), 3);
        assert_eq!(reopened.records(), log.records());
    }

    #[test]
    fn a_sealer_that_refuses_is_an_error_and_not_a_plaintext_line() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let mut log = log_in(&dir);

        let err = log
            .append(
                at(1),
                None,
                Event::DocumentUnlocked,
                Profile::Private.policy(),
                Some(&Broken),
            )
            .expect_err("a broken key must not degrade to plaintext");

        assert!(matches!(err, AuditError::Seal(_)), "got {err}");
        assert!(err.to_string().contains("the document key was rejected"));
        assert!(!log.location().exists(), "a failed seal still wrote a file");
    }

    // -- the log ----------------------------------------------------------

    #[test]
    fn a_history_that_has_never_been_written_is_empty_and_not_an_error() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let log = log_in(&dir);
        assert!(log.records().is_empty());
        assert_eq!(log.next_sequence(), 1);
    }

    #[test]
    fn a_file_of_nothing_but_blank_lines_is_an_empty_history() {
        // A crash between the newline and the next entry leaves one of these.
        // A blank line makes no claim, so it is not tampering.
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("security.log");
        std::fs::write(&path, "\n\n   \n").expect("the file is written");

        let log = AuditLog::open(path, None).expect("blank lines are not damage");
        assert!(log.records().is_empty());
    }

    proptest! {
        #[test]
        fn a_record_round_trips_through_a_log_line(record in any_record()) {
            let json = serde_json::to_string(&record).expect("a record serialises");
            prop_assert_eq!(
                serde_json::from_str::<Record>(&json).expect("and deserialises"),
                record
            );
        }

        #[test]
        fn appending_numbers_entries_1_2_3_and_reads_them_back_in_order(
            events in proptest::collection::vec(any_event(), 0..12),
        ) {
            let dir = tempfile::tempdir().expect("a temporary directory");
            let path = dir.path().join("security.log");
            let mut log = AuditLog::open(path.clone(), None).expect("a fresh log opens");

            for (index, event) in events.iter().enumerate() {
                let seq = u64::try_from(index).expect("a small index") + 1;
                let outcome = log
                    .append(
                        at(1_700_000_000 + i64::try_from(index).expect("a small index")),
                        Some(DocumentId::new(1)),
                        *event,
                        Profile::Standard.policy(),
                        None,
                    )
                    .expect("Standard writes");
                prop_assert_eq!(outcome, Appended::Persisted(seq));
            }

            let reopened = AuditLog::open(path, None).expect("the history reopens");
            prop_assert_eq!(reopened.records(), log.records());
            for (index, record) in reopened.records().iter().enumerate() {
                prop_assert_eq!(record.seq(), u64::try_from(index).expect("small") + 1);
                prop_assert_eq!(record.event(), events[index]);
            }
        }

        #[test]
        fn reopening_a_history_continues_its_numbering(
            first in 1usize..6,
            second in 1usize..6,
        ) {
            // The reason `open` is the only constructor: a second session
            // must not restart at 1 over the top of a real history.
            let dir = tempfile::tempdir().expect("a temporary directory");
            let path = dir.path().join("security.log");

            for run in [first, second] {
                let mut log = AuditLog::open(path.clone(), None).expect("the history opens");
                for _ in 0..run {
                    log.append(
                        at(1_700_000_000),
                        None,
                        Event::PrivacyModeEntered,
                        Profile::Standard.policy(),
                        None,
                    )
                    .expect("Standard writes");
                }
            }

            let reopened = AuditLog::open(path, None).expect("the history reopens");
            prop_assert_eq!(reopened.records().len(), first + second);
            prop_assert_eq!(
                reopened.next_sequence(),
                u64::try_from(first + second).expect("small") + 1
            );
        }

        #[test]
        fn deleting_any_line_but_the_last_is_detected(
            total in 2usize..8,
            victim in 0usize..7,
        ) {
            // What append-only buys inside the process, and its exact limit:
            // remove any entry other than the tail and the numbering says so.
            prop_assume!(victim < total - 1);

            let dir = tempfile::tempdir().expect("a temporary directory");
            let path = dir.path().join("security.log");
            let mut log = AuditLog::open(path.clone(), None).expect("a fresh log opens");
            for _ in 0..total {
                log.append(
                    at(1_700_000_000),
                    None,
                    Event::SignatureMade,
                    Profile::Standard.policy(),
                    None,
                )
                .expect("Standard writes");
            }

            let text = std::fs::read_to_string(&path).expect("readable");
            let kept: Vec<&str> = text
                .lines()
                .enumerate()
                .filter_map(|(i, l)| (i != victim).then_some(l))
                .collect();
            std::fs::write(&path, kept.join("\n") + "\n").expect("the edit is written");

            let err = AuditLog::open(path, None).expect_err("an edited history is reported");
            prop_assert!(
                matches!(err, AuditError::OutOfOrder { .. }),
                "expected tampering to be reported, got {}",
                err
            );
            prop_assert!(err.to_string().contains("has been altered"));
        }
    }

    #[test]
    fn removing_the_last_line_is_not_detected_and_the_docs_say_so() {
        // The honest gap, pinned so nobody later reads the append-only
        // wording as a stronger claim than it is. Closing it needs a signing
        // key the editor does not hold; a home-rolled hash chain would be
        // custom cryptography (specs.md section 15) that only looked like a
        // fix.
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("security.log");
        let mut log = AuditLog::open(path.clone(), None).expect("a fresh log opens");
        for _ in 0..3 {
            log.append(
                at(1_700_000_000),
                None,
                Event::SignatureMade,
                Profile::Standard.policy(),
                None,
            )
            .expect("Standard writes");
        }

        let text = std::fs::read_to_string(&path).expect("readable");
        let kept: Vec<&str> = text.lines().take(2).collect();
        std::fs::write(&path, kept.join("\n") + "\n").expect("the truncation is written");

        let reopened = AuditLog::open(path, None).expect("truncation is undetectable");
        assert_eq!(reopened.records().len(), 2);
    }

    #[test]
    fn a_history_numbered_from_something_other_than_one_is_rejected() {
        // Deleting the head, or splicing in a fragment of another log.
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("security.log");
        std::fs::write(
            &path,
            "{\"seq\":4,\"at_unix\":0,\"at_offset\":0,\"event\":{\"kind\":\"signature_made\"}}\n",
        )
        .expect("the file is written");

        let err = AuditLog::open(path, None).expect_err("a history must start at 1");
        assert!(
            matches!(
                err,
                AuditError::OutOfOrder {
                    line: 1,
                    expected: 1,
                    found: 4
                }
            ),
            "got {err}"
        );
    }

    #[test]
    fn a_line_that_is_not_a_record_names_its_number_and_not_its_contents() {
        // What the user is told when it fails -- and what they are not told.
        // An audit log is read after something has gone wrong, and an error
        // that quotes the line copies it into a terminal, a bug report, a
        // screenshot.
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("security.log");
        std::fs::write(
            &path,
            "{\"seq\":1,\"at_unix\":0,\"at_offset\":0,\"event\":{\"kind\":\"signature_made\"}}\n\
             {\"seq\":2,\"this is not\":\"a record\",\"passphrase\":\"hunter2\"}\n",
        )
        .expect("the file is written");

        let err = AuditLog::open(path, None).expect_err("a damaged line is reported");
        let message = err.to_string();
        assert!(
            matches!(err, AuditError::Malformed { line: 2 }),
            "got {err}"
        );
        assert!(message.contains("line 2"), "got {message}");
        assert!(
            !message.contains("hunter2"),
            "the error quoted the line back: {message}"
        );
        assert!(
            message.contains("edited or truncated"),
            "the message must suggest what happened: {message}"
        );
    }

    // -- reading lazily (ADR-0036) ------------------------------------------

    #[test]
    fn the_lazy_reader_yields_the_same_records_in_the_same_order_as_read() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("security.log");
        let mut log = log_in(&dir);
        for (index, event) in [
            Event::DocumentEncrypted,
            Event::DocumentUnlocked,
            Event::SignatureMade,
        ]
        .into_iter()
        .enumerate()
        {
            log.append(
                at(1_700_000_000 + i64::try_from(index).expect("small")),
                None,
                event,
                Profile::Standard.policy(),
                None,
            )
            .expect("Standard writes");
        }

        let eager = AuditLog::open(path.clone(), None)
            .expect("the eager reader opens")
            .records()
            .to_vec();
        let lazy: Vec<Record> = AuditLog::read_lazy(&path, None)
            .collect::<Result<_, _>>()
            .expect("the lazy reader reads the same file");

        assert_eq!(lazy, eager);
        assert_eq!(lazy, log.records());
    }

    #[test]
    fn the_lazy_reader_decrypts_sealed_lines_just_like_read() {
        // The round trip already proven for `read` (formerly `open`'s only
        // path), proven again for `read_lazy`, so the eager path is not the
        // only one this crate's sealed/plaintext promise holds for.
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("security.log");
        let mut log = AuditLog::open(path.clone(), Some(&Reversing)).expect("a fresh log opens");
        log.append(
            at(1),
            Some(DocumentId::new(9)),
            Event::DocumentDecrypted,
            Profile::Private.policy(),
            Some(&Reversing),
        )
        .expect("Private with a key writes a sealed line");
        log.append(
            at(2),
            Some(DocumentId::new(9)),
            Event::SignatureVerified { valid: true },
            Profile::Private.policy(),
            Some(&Reversing),
        )
        .expect("a second sealed line");

        let lazy: Vec<Record> = AuditLog::read_lazy(&path, Some(&Reversing))
            .collect::<Result<_, _>>()
            .expect("sealed lines decrypt through the lazy path too");
        assert_eq!(lazy, log.records());

        let err = AuditLog::read_lazy(&path, None)
            .next()
            .expect("a sealed file is not an empty history")
            .expect_err("no key, no record");
        assert!(matches!(err, AuditError::Sealed { line: 1 }), "got {err}");
    }

    #[test]
    fn the_lazy_reader_stops_decrypting_once_the_caller_stops_asking() {
        // The entire point: a caller that only calls `.next()` a few times
        // must not have paid for the Argon2id derivation of every other
        // line. Counting actual `unseal` calls is the only way to observe
        // that from outside `Records`.
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("security.log");
        let calls = std::cell::Cell::new(0);
        {
            let sealer = CountingUnseal { calls: &calls };
            let mut log = AuditLog::open(path.clone(), Some(&sealer)).expect("a fresh log opens");
            for _ in 0..5 {
                log.append(
                    at(1),
                    None,
                    Event::DocumentUnlocked,
                    Profile::Private.policy(),
                    Some(&sealer),
                )
                .expect("Private with a key writes");
            }
        }
        calls.set(0); // writing seals, not unseals, but reset to be exact.

        let sealer = CountingUnseal { calls: &calls };
        let mut records = AuditLog::read_lazy(&path, Some(&sealer));
        assert_eq!(
            records
                .next()
                .expect("first record")
                .expect("decrypts")
                .seq(),
            1
        );
        assert_eq!(
            records
                .next()
                .expect("second record")
                .expect("decrypts")
                .seq(),
            2
        );
        drop(records);

        assert_eq!(
            calls.get(),
            2,
            "the lazy reader decrypted more than the two lines actually asked for"
        );
    }

    #[test]
    fn the_lazy_reader_reports_out_of_order_lines_at_the_same_point_as_read() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("security.log");
        std::fs::write(
            &path,
            "{\"seq\":1,\"at_unix\":0,\"at_offset\":0,\"event\":{\"kind\":\"signature_made\"}}\n\
             {\"seq\":3,\"at_unix\":0,\"at_offset\":0,\"event\":{\"kind\":\"signature_made\"}}\n",
        )
        .expect("the file is written");

        let mut records = AuditLog::read_lazy(&path, None);
        assert_eq!(records.next().expect("first record").expect("ok").seq(), 1);
        let err = records
            .next()
            .expect("a second item, an error")
            .expect_err("the sequence jumped from 1 to 3");
        assert!(
            matches!(
                err,
                AuditError::OutOfOrder {
                    line: 2,
                    expected: 2,
                    found: 3
                }
            ),
            "got {err}"
        );

        assert!(
            records.next().is_none(),
            "the iterator must not resume past a sequence error"
        );
    }

    #[test]
    fn the_lazy_reader_reports_a_malformed_line_at_the_same_point_as_read() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("security.log");
        std::fs::write(
            &path,
            "{\"seq\":1,\"at_unix\":0,\"at_offset\":0,\"event\":{\"kind\":\"signature_made\"}}\n\
             {\"seq\":2,\"this is not\":\"a record\",\"passphrase\":\"hunter2\"}\n",
        )
        .expect("the file is written");

        let mut records = AuditLog::read_lazy(&path, None);
        assert_eq!(records.next().expect("first record").expect("ok").seq(), 1);
        let err = records
            .next()
            .expect("a second item, an error")
            .expect_err("line 2 is not a record");
        assert!(
            matches!(err, AuditError::Malformed { line: 2 }),
            "got {err}"
        );

        assert!(
            records.next().is_none(),
            "the iterator must not resume past a malformed line"
        );
    }

    #[test]
    fn a_history_that_has_never_been_written_is_empty_and_not_an_error_when_read_lazily() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("security.log");

        let mut records = AuditLog::read_lazy(&path, None);
        assert!(
            records.next().is_none(),
            "a missing file must not be reported as an error"
        );
    }

    #[test]
    fn a_file_of_nothing_but_blank_lines_is_an_empty_history_when_read_lazily() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("security.log");
        std::fs::write(&path, "\n\n   \n").expect("the file is written");

        let mut records = AuditLog::read_lazy(&path, None);
        assert!(records.next().is_none(), "blank lines are not records");
    }

    #[test]
    fn a_timestamp_the_domain_cannot_represent_is_rejected_at_the_boundary() {
        // The case a caller can express in the types and the domain does not
        // permit. 93_599 seconds is the largest offset `time` accepts
        // (25:59:59); one second past it must not become a silent 0.
        let ok = format!(
            "{{\"seq\":1,\"at_unix\":0,\"at_offset\":{},\"event\":{{\"kind\":\"signature_made\"}}}}",
            93_599
        );
        let record: Record = serde_json::from_str(&ok).expect("the boundary is accepted");
        assert_eq!(record.at().offset().whole_seconds(), 93_599);

        for bad in [
            "{\"seq\":1,\"at_unix\":0,\"at_offset\":93600,\"event\":{\"kind\":\"signature_made\"}}",
            "{\"seq\":1,\"at_unix\":9223372036854775807,\"at_offset\":0,\
             \"event\":{\"kind\":\"signature_made\"}}",
        ] {
            assert!(
                serde_json::from_str::<Record>(bad).is_err(),
                "an impossible timestamp was accepted: {bad}"
            );
        }
    }

    #[test]
    fn the_end_of_the_sequence_is_an_error_and_not_a_wrap_to_zero() {
        // A boundary nobody will reach, handled because the alternative is a
        // log that renumbers itself and then reports as tampered with for
        // ever.
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("security.log");
        let mut log = AuditLog::open(path, None).expect("a fresh log opens");
        log.records.push(Record::new(
            u64::MAX,
            at(1_700_000_000),
            None,
            Event::SignatureMade,
        ));

        let err = log
            .append(
                at(1_700_000_001),
                None,
                Event::SignatureMade,
                Profile::Standard.policy(),
                None,
            )
            .expect_err("the sequence is exhausted");
        assert!(matches!(err, AuditError::SequenceExhausted), "got {err}");
    }

    #[test]
    fn timestamps_are_kept_to_the_second_with_their_offset() {
        // Sub-second precision would imply the timestamps order the log, and
        // they do not -- the sequence does. The offset is kept so a line
        // written at 14:02 local still reads as 14:02 next month in another
        // country.
        let tokyo = UtcOffset::from_hms(9, 0, 0).expect("a real offset");
        let moment = at(1_700_000_000).to_offset(tokyo);
        let record = Record::new(1, moment, None, Event::SignatureMade);

        assert_eq!(record.at().offset(), tokyo);
        assert_eq!(record.at().nanosecond(), 0);
        let json = serde_json::to_string(&record).expect("serialises");
        assert_eq!(
            serde_json::from_str::<Record>(&json).expect("deserialises"),
            record
        );
    }

    // -- vocabulary and wording -------------------------------------------

    #[test]
    fn a_profile_label_matches_what_the_security_menu_calls_it() {
        // `ProfileLabel` and `bp-security` must not drift, or the history
        // would name a profile the user has never seen in the UI.
        for profile in Profile::all() {
            let security = Security::Named(*profile);
            assert_eq!(ProfileLabel::of(&security).name(), security.name());
        }
        let custom = Security::Custom(Profile::Maximum.policy());
        assert_eq!(ProfileLabel::of(&custom), ProfileLabel::Custom);
        assert_eq!(ProfileLabel::of(&custom).name(), custom.name());
    }

    proptest! {
        #[test]
        fn a_described_event_says_something_and_never_says_nothing(event in any_event()) {
            // A history line the user cannot read is a history they will not
            // check.
            let described = event.describe();
            prop_assert!(!described.trim().is_empty());
            prop_assert!(!event.label().is_empty());
        }
    }

    #[test]
    fn the_counts_a_scan_reports_read_naturally_at_their_boundaries() {
        // Zero, one and many. "found 1 candidates" is the sort of thing that
        // makes a security feature look unfinished, which is exactly when a
        // user stops trusting it.
        assert_eq!(
            Event::SecretScanFinished { findings: 0 }.describe(),
            "secret scan found nothing"
        );
        assert_eq!(
            Event::SecretScanFinished { findings: 1 }.describe(),
            "secret scan found 1 candidate"
        );
        assert_eq!(
            Event::SecretScanFinished { findings: 2 }.describe(),
            "secret scan found 2 candidates"
        );
        assert!(
            Event::SignatureVerified { valid: false }
                .describe()
                .contains("did NOT"),
            "a failed verification must not read like a successful one"
        );
    }

    proptest! {
        #[test]
        fn hex_round_trips_every_byte_string(bytes in proptest::collection::vec(any::<u8>(), 0..64)) {
            prop_assert_eq!(hex_decode(&hex_encode(&bytes)), Some(bytes));
        }

        #[test]
        fn hex_rejects_anything_that_is_not_hex(text in "[g-zG-Z][0-9a-f]{0,8}") {
            prop_assert_eq!(hex_decode(&text), None);
        }
    }

    #[test]
    fn an_odd_length_hex_line_is_not_half_a_record() {
        assert_eq!(hex_decode("abc"), None);
        assert_eq!(hex_decode(""), Some(Vec::new()));
    }
}
