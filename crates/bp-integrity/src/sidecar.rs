//! The `.sig` sidecar: a detached signature as a file on a disk.
//!
//! `bp-crypto` produces 64 bytes and stops there, on purpose. Everything
//! below is the part it left: what the file beside the document is called,
//! what is in it, and what a reader is told when the answer is no.
//!
//! ## The name: `report.docx` is signed by `report.docx.sig`
//!
//! The suffix is **appended to the whole file name**, not substituted for the
//! extension. Substituting would give `report.docx` and `report.pdf` the same
//! `report.sig`, so exporting a document would silently overwrite the
//! signature over the original -- a data-loss bug with a security blast
//! radius. Appending also sorts the sidecar directly under its document in
//! every file manager, which is the cheapest possible defence against the one
//! real hazard of a detached signature: the two files getting separated.
//!
//! It is the same convention as `linux-6.1.tar.xz.sig`, `.asc` and `.sha256`,
//! so it needs no explaining to anyone who has ever verified a download.
//!
//! ## The contents: four lines of text, not 64 raw bytes
//!
//! ```text
//! BachelorPad+ signature v1
//! algorithm: ed25519
//! key: 3b6a27bcceb6a42d62a3a8d02a6f0d73653215771de243a63ac048a18b59da29
//! signature: e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e06...
//! ```
//!
//! `bp-crypto`'s module note describes a `.sig` file as the bare 64 bytes,
//! and those exact 64 bytes are here -- on the last line, in hex, decoding to
//! the identical value. What is added is the line above it, and that line is
//! the reason this format exists rather than the bare one.
//!
//! **A bare signature cannot say who signed.** Ed25519 verification needs a
//! public key supplied from outside, so with a raw `.sig` the only two
//! answers available are "verifies under the key you gave me" and "does not".
//! A user handed the second cannot tell a document that was tampered with
//! from a document that is perfectly intact and was signed by a colleague --
//! and those two call for opposite reactions. Recording the signer's key
//! makes [`Verification::DoesNotMatch`] and
//! [`Verification::SignedByAnotherKey`] two separate verdicts, and that is
//! the entire justification for the framing. Note what it does *not* do: the
//! recorded key is a claim, not a credential. It says which key to check
//! against, and `bp-crypto` is explicit that no key proves whose it is.
//!
//! The rest follows from that one decision:
//!
//! * **text, not binary**, because a sidecar separated from its document
//!   travels by email and chat, where 64 raw bytes do not survive and hex
//!   does; and because a user can open it and read the signer's key without
//!   any tool of ours;
//! * **a version on the first line**, because a format that can grow needs
//!   somewhere to say that it has, and the raw form has nowhere. A reader of
//!   `v1` refuses a `v2` file loudly instead of decoding it wrongly;
//! * **the algorithm named**, so that "this is an Ed25519 signature" is a
//!   fact in the file rather than an assumption in the reader.
//!
//! ## The format is fixed
//!
//! Four lines, in that order, lowercase hex, `\n` between them and one at the
//! end. A reader additionally accepts `\r\n` and trailing blank lines,
//! because a text file that has been through Windows, a mail client and a
//! paste buffer comes back with those, and refusing it would be pedantry at
//! the user's expense. Nothing else is accepted: no reordering, no extra
//! fields, no uppercase digits. A security artefact with several spellings is
//! a security artefact whose comparisons are unreliable.
//!
//! Signing is deterministic in `bp-crypto` and this encoding is canonical on
//! the way out, so re-signing an unchanged document reproduces the sidecar
//! byte for byte rather than showing the user a spurious change to review.

use std::io;
use std::path::{Path, PathBuf};

use bp_crypto::{SignError, Signature, SigningKey, VerifyingKey, sign_document, verify_document};

use crate::atomic::{Restrict, write_atomically};
use crate::error::IntegrityError;
use crate::hex;

/// What is appended to a document's file name to name its signature.
///
/// Public because a file dialog has to filter on it and a "signed documents"
/// view has to recognise it; two callers spelling it slightly differently is
/// how a sidecar becomes invisible to the product that wrote it.
pub const SIDECAR_SUFFIX: &str = ".sig";

/// The first line of every sidecar, version included.
///
/// Exposed so a caller can identify one of our sidecars without parsing it,
/// and so the version is a value in one place rather than a string retyped
/// wherever somebody writes a reader.
pub const SIDECAR_MAGIC: &str = "BachelorPad+ signature v1";

/// The only algorithm this version names, spelled the way the standard does.
const ALGORITHM_NAME: &str = "ed25519";

const FIELD_ALGORITHM: &str = "algorithm";
const FIELD_KEY: &str = "key";
const FIELD_SIGNATURE: &str = "signature";

/// Where the signature for `document` lives.
///
/// A function rather than a rule left to each caller, because two callers
/// that disagree about the name produce a document that one half of the
/// product believes is unsigned.
///
/// Fails only on a path that names no file at all -- `..`, `/`, a bare drive
/// letter -- because there is then no file name to append to.
pub fn sidecar_path(document: &Path) -> Result<PathBuf, IntegrityError> {
    let name = document
        .file_name()
        .ok_or_else(|| IntegrityError::NotAFilePath(document.to_owned()))?;
    let mut sidecar = name.to_os_string();
    sidecar.push(SIDECAR_SUFFIX);
    Ok(document.with_file_name(sidecar))
}

/// Everything a `.sig` file holds: who signed, and the signature.
///
/// A named type rather than a `(VerifyingKey, Signature)` tuple so that the
/// encoding lives in exactly one place and cannot be half-implemented by a
/// caller who only needs to read sidecars.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sidecar {
    key: VerifyingKey,
    signature: Signature,
}

impl Sidecar {
    /// Pair a signature with the key that made it.
    ///
    /// Nothing checks the pairing here: a `Sidecar` is a record of a claim,
    /// and testing the claim is [`verify_file`]'s job, against a document
    /// this type has never seen.
    #[must_use]
    pub const fn new(key: VerifyingKey, signature: Signature) -> Self {
        Self { key, signature }
    }

    /// The key the sidecar names as the signer.
    ///
    /// Worth showing a user even when verification failed, clearly marked as
    /// unconfirmed: it is the only clue they have about where a bad signature
    /// came from.
    #[must_use]
    pub const fn verifying_key(&self) -> VerifyingKey {
        self.key
    }

    /// The detached signature itself -- `bp-crypto`'s 64 bytes, unchanged.
    #[must_use]
    pub const fn signature(&self) -> Signature {
        self.signature
    }

    /// The exact bytes of the `.sig` file, as text.
    ///
    /// Canonical: one `Sidecar` has one spelling, so an unchanged document
    /// re-signed produces an identical file.
    #[must_use]
    pub fn to_text(&self) -> String {
        format!(
            "{SIDECAR_MAGIC}\n\
             {FIELD_ALGORITHM}: {ALGORITHM_NAME}\n\
             {FIELD_KEY}: {}\n\
             {FIELD_SIGNATURE}: {}\n",
            self.key.to_hex(),
            hex::encode(&self.signature.to_bytes()),
        )
    }

    /// Read a sidecar back.
    ///
    /// Every rejection names what was wrong, because the realistic causes --
    /// a truncated copy, a file from a newer build, something that was never
    /// a sidecar -- have different fixes, and a single "malformed" would hide
    /// which one applies.
    pub fn parse(text: &str) -> Result<Self, MalformedSidecar> {
        let mut lines = text.lines();

        let magic = lines.next().ok_or(MalformedSidecar::Truncated {
            missing: "the format marker",
        })?;
        if magic.trim_end() != SIDECAR_MAGIC {
            return Err(MalformedSidecar::WrongMagic {
                found: excerpt(magic),
            });
        }

        let algorithm = field(lines.next(), FIELD_ALGORITHM)?;
        if algorithm != ALGORITHM_NAME {
            return Err(MalformedSidecar::UnknownAlgorithm {
                found: excerpt(algorithm),
            });
        }
        let key_digits = field(lines.next(), FIELD_KEY)?;
        let signature_digits = field(lines.next(), FIELD_SIGNATURE)?;

        // Blank lines at the end are what an editor or a mail client adds;
        // anything else means the file is not only ours.
        for extra in lines {
            if !extra.trim().is_empty() {
                return Err(MalformedSidecar::TrailingContent {
                    found: excerpt(extra),
                });
            }
        }

        let key_bytes =
            hex::decode(key_digits).ok_or(MalformedSidecar::NotHex { field: FIELD_KEY })?;
        let key = VerifyingKey::from_bytes(&key_bytes).map_err(MalformedSidecar::BadKey)?;

        let signature_bytes = hex::decode(signature_digits).ok_or(MalformedSidecar::NotHex {
            field: FIELD_SIGNATURE,
        })?;
        let signature =
            Signature::from_bytes(&signature_bytes).map_err(MalformedSidecar::BadSignature)?;

        Ok(Self { key, signature })
    }
}

/// Why a `.sig` file could not be read as one.
///
/// Deliberately *not* folded into "the signature does not verify". A
/// truncated file is a copying accident and a bad signature is an accusation;
/// telling a user the second when the first happened sends them looking for
/// an attacker who does not exist. It is the same distinction `bp-crypto`
/// draws when it refuses a short signature by length rather than by verdict.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum MalformedSidecar {
    /// A `.sig` file is text; this one is not, so it was never one of ours.
    #[error("this is not a text file, so it is not a signature this product wrote")]
    NotText,

    #[error("this does not start with \"{SIDECAR_MAGIC}\"; it starts with {found:?}")]
    WrongMagic { found: String },

    #[error("this signature file stops early -- {missing} is missing")]
    Truncated { missing: &'static str },

    #[error("expected a \"{expected}\" line here, found {found:?}")]
    WrongField {
        expected: &'static str,
        found: String,
    },

    /// A file from a future version, or from another tool. Refused rather
    /// than guessed at, because a signature decoded under the wrong scheme
    /// fails to verify and then looks exactly like a tampered document.
    #[error("this signature was made with {found:?}, which this version cannot check")]
    UnknownAlgorithm { found: String },

    #[error("the \"{field}\" line is not lowercase hexadecimal")]
    NotHex { field: &'static str },

    #[error("the key in this signature file is unusable: {0}")]
    BadKey(#[source] SignError),

    #[error("the signature in this file is unusable: {0}")]
    BadSignature(#[source] SignError),

    #[error("there is unexpected content after the signature: {found:?}")]
    TrailingContent { found: String },
}

/// Whose signature the caller will accept.
///
/// An enum rather than an `Option<&VerifyingKey>` so that the permissive
/// choice has to be typed out. The two are not two settings of one knob: one
/// of them answers a question about a person and the other does not, and a
/// bare `None` at a call site does not make that visible to the next reader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expectation {
    /// Only this key will do. The verdict then means what a user assumes a
    /// verdict means: these bytes were approved by the holder of that key.
    Key(VerifyingKey),

    /// Any key, reported back rather than checked.
    ///
    /// **This is a weaker claim than it looks.** It establishes only that the
    /// document has not changed since the key *named in the sidecar* signed
    /// it, and anyone who alters a document can also re-sign it with a key of
    /// their own and get [`Verification::Verified`] out of this mode. It is
    /// honest for "show me who signed this", and for a first look at a file
    /// whose signer is not yet known; it is not a security check on its own,
    /// and the key it returns has to be put in front of the user for them to
    /// recognise.
    AnySigner,
}

/// The answer, in the five shapes a user has to be able to tell apart.
///
/// Returned as a value rather than raised as an error precisely because every
/// variant is an answer somebody asked for. Four of the five are "no", and
/// they are four rather than one because their fixes are four different
/// things: find the file, get an undamaged copy, get an untampered document,
/// get the right key.
#[derive(Debug, PartialEq, Eq)]
pub enum Verification {
    /// The signature is good and, if a key was demanded, it was that key.
    Verified {
        /// Echoed back so the caller can show it. Under
        /// [`Expectation::AnySigner`] this is the only thing said about who
        /// signed, and it is not a proof of identity.
        signer: VerifyingKey,
    },

    /// There is no sidecar. **The unsigned document fails**, and this is the
    /// variant that makes the whole design fail closed: a signature that can
    /// be deleted to produce a pass is not a signature. The path travels with
    /// it so the user can be told exactly which file to go and find.
    SignatureMissing { expected_at: PathBuf },

    /// A sidecar exists and is not readable as one. Distinct from a failed
    /// check, because nothing here says anything about the document.
    SignatureMalformed {
        path: PathBuf,
        reason: MalformedSidecar,
    },

    /// The sidecar holds together as a file but not as evidence: the
    /// signature does not verify under the key the sidecar itself names.
    /// Either the document changed after signing or the sidecar did, and the
    /// bytes cannot say which.
    DoesNotMatch {
        /// The key the sidecar claims. Unconfirmed -- the check that would
        /// have confirmed it is the one that just failed -- so it must be
        /// shown as a claim and never as an attribution.
        named_signer: VerifyingKey,
    },

    /// The document is intact and was signed by somebody else.
    ///
    /// The verdict a bare 64-byte `.sig` cannot produce, and the reason this
    /// crate's sidecar carries a key. Nothing is damaged and nothing was
    /// tampered with: the user is looking at a legitimately signed document
    /// from a signer they were not expecting, which is a filing question
    /// rather than an alarm.
    SignedByAnotherKey {
        signer: VerifyingKey,
        expected: VerifyingKey,
    },
}

impl Verification {
    /// The one-line question: may this document be trusted?
    ///
    /// Exists so that no caller has to write the `match`, because a caller
    /// who writes it and forgets a variant fails *open* -- and the variant
    /// most easily forgotten is the missing sidecar.
    #[must_use]
    pub const fn is_verified(&self) -> bool {
        matches!(self, Self::Verified { .. })
    }

    /// What to put in front of the user.
    ///
    /// Written here rather than in the shell so that every surface -- status
    /// bar, dialog, audit entry -- says the same thing about the same
    /// verdict, and so that the wording of a security message is reviewed
    /// together with the logic that produces it.
    #[must_use]
    pub fn explain(&self) -> String {
        match self {
            Self::Verified { signer } => {
                format!("Signature verified. Signed by key {}.", signer.to_hex())
            }
            Self::SignatureMissing { expected_at } => format!(
                "Not signed: no signature file at {}. An unsigned document cannot be verified.",
                expected_at.display()
            ),
            Self::SignatureMalformed { path, reason } => format!(
                "The signature file {} cannot be read: {reason}",
                path.display()
            ),
            Self::DoesNotMatch { named_signer } => format!(
                "Signature does NOT match. This document has been altered since it was signed, \
                 or the signature file has. It claims to be signed by key {}, which could not be \
                 confirmed.",
                named_signer.to_hex()
            ),
            Self::SignedByAnotherKey { signer, expected } => format!(
                "Signed, but by a different key. This document is intact and was signed by key \
                 {}; you asked for key {}.",
                signer.to_hex(),
                expected.to_hex()
            ),
        }
    }
}

/// Sign the file at `document` and write the sidecar beside it.
///
/// Signs **what is on the disk**, not what an editor happens to hold in
/// memory, because what is on the disk is what a recipient will receive and
/// check. A caller with unsaved changes must save first; signing the buffer
/// would produce a signature that fails against the very file it was written
/// for.
///
/// The sidecar is written atomically, so a crash part-way leaves the previous
/// signature intact rather than a truncated file that reads as tampering.
///
/// Returns the path written, because the caller almost always has to name it
/// -- in a status message, in an audit entry, or to the person who must be
/// sent both files.
pub fn sign_file(document: &Path, key: &SigningKey) -> Result<PathBuf, IntegrityError> {
    let path = sidecar_path(document)?;
    let bytes = std::fs::read(document).map_err(IntegrityError::read(document))?;
    let sidecar = Sidecar::new(key.verifying_key(), sign_document(key, &bytes));

    write_atomically(&path, sidecar.to_text().as_bytes(), Restrict::No)
        .map_err(IntegrityError::write(&path))?;
    Ok(path)
}

/// Check the file at `document` against its sidecar.
///
/// The order of the checks is a decision rather than an accident. The
/// signature is tested against the key the *sidecar* names before that key is
/// compared with the caller's: a sidecar that does not hold together
/// internally is unauthenticated text, so the key written in it is not
/// evidence of anything, and reporting "signed by someone else" off the back
/// of it would be repeating an attacker's claim. A broken sidecar is
/// [`Verification::DoesNotMatch`] whatever key it names.
///
/// A missing *document* is an error rather than a verdict -- there is nothing
/// to have an opinion about -- whereas a missing *signature* is a verdict,
/// and a failing one.
pub fn verify_file(document: &Path, expect: &Expectation) -> Result<Verification, IntegrityError> {
    let path = sidecar_path(document)?;

    // The document first: "no signature" is a misleading thing to say about a
    // file that is not there either.
    let bytes = std::fs::read(document).map_err(IntegrityError::read(document))?;

    let raw = match std::fs::read(&path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Ok(Verification::SignatureMissing { expected_at: path });
        }
        Err(e) => return Err(IntegrityError::read(&path)(e)),
    };

    let Ok(text) = String::from_utf8(raw) else {
        return Ok(Verification::SignatureMalformed {
            path,
            reason: MalformedSidecar::NotText,
        });
    };

    let sidecar = match Sidecar::parse(&text) {
        Ok(sidecar) => sidecar,
        Err(reason) => return Ok(Verification::SignatureMalformed { path, reason }),
    };

    if verify_document(&sidecar.key, &bytes, &sidecar.signature).is_err() {
        return Ok(Verification::DoesNotMatch {
            named_signer: sidecar.key,
        });
    }

    match expect {
        Expectation::AnySigner => Ok(Verification::Verified {
            signer: sidecar.key,
        }),
        Expectation::Key(expected) if *expected == sidecar.key => Ok(Verification::Verified {
            signer: sidecar.key,
        }),
        Expectation::Key(expected) => Ok(Verification::SignedByAnotherKey {
            signer: sidecar.key,
            expected: *expected,
        }),
    }
}

/// Read one field line, `name: value`.
fn field<'a>(line: Option<&'a str>, expected: &'static str) -> Result<&'a str, MalformedSidecar> {
    let line = line.ok_or(MalformedSidecar::Truncated { missing: expected })?;
    let (name, value) = line
        .split_once(':')
        .ok_or_else(|| MalformedSidecar::WrongField {
            expected,
            found: excerpt(line),
        })?;
    if name.trim() != expected {
        return Err(MalformedSidecar::WrongField {
            expected,
            found: excerpt(name),
        });
    }
    Ok(value.trim())
}

/// Enough of an offending line to recognise it, and no more.
///
/// A malformed sidecar can be any file at all, including a large one, and an
/// error message quoting the whole of it is an error message nobody can read
/// and a log line that can carry someone's document into a bug report.
fn excerpt(line: &str) -> String {
    const LIMIT: usize = 40;
    let trimmed = line.trim();
    if trimmed.chars().count() <= LIMIT {
        return trimmed.to_owned();
    }
    let mut out: String = trimmed.chars().take(LIMIT).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests;
