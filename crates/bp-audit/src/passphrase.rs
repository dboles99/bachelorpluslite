//! Sealing an audit line with the document's own passphrase.
//!
//! [`Sealer`] is the seam ADR-0024 left open: `bp-audit` decides *that* a line
//! must be sealed, and something else performs it. This module is that
//! something else, and it is the only part of the crate that depends on
//! `bp-crypto` or touches key-shaped bytes. Everything reachable from a
//! [`Record`] stays where it was — numbers, flags and closed enums, `Copy`,
//! incapable of owning a secret. Nothing here is reachable from one.
//!
//! [`Record`]: crate::Record
//!
//! ## The framing: one `.bpadx` envelope per line
//!
//! ADR-0022 sealed the recovery journal with the document's own passphrase,
//! and that reasoning carries over unchanged in one respect and not at all in
//! another.
//!
//! **What carries over: the key.** An audit trail about a document is a
//! derived artefact of that document, so the people who may read it are
//! exactly the people who may read the document — no more, and no fewer. The
//! document's passphrase expresses that set precisely, and expresses it
//! without inventing a second secret for the user to lose. A separate audit
//! passphrase would be a key that can be forgotten independently, which turns
//! a security history into a thing that is routinely unreadable; a key derived
//! from something the machine holds would make the log readable by anyone with
//! the disk, which is the property the strict profiles exist to remove. So:
//! the document's passphrase, exactly as ADR-0022 chose.
//!
//! **What does not carry over: the shape.** A recovery journal is *rewritten
//! wholesale* — one checkpoint, replaced each time. An audit log is *appended
//! to*, many times, across a session. Sealing it the way ADR-0022 seals a
//! journal would mean re-encrypting every line already written on every new
//! event: O(n²) bytes written over a session, and — worse — one Argon2id
//! derivation per append over a file that keeps growing. Argon2id is
//! deliberately expensive (19 MiB, ~50–100 ms at ADR-0021's baseline). It also
//! destroys the append-only property `AuditLog` is built on: a writer that
//! rewrites the file is a writer that seeks and truncates, and the one thing
//! this crate offers is that it never does.
//!
//! So each record is sealed on its own, into its own complete `.bpadx`
//! envelope, hex-encoded onto one line. The file stays append-only; a write
//! stays O(1); and every ADR-0021 guarantee applies to every line
//! individually, because every line *is* an ADR-0021 document.
//!
//! ## Nonces, and why there is no nonce question here
//!
//! Nonce reuse under a fixed key is catastrophic for both AEADs ADR-0021
//! offers — XChaCha20-Poly1305 leaks the XOR of the two plaintexts and
//! AES-256-GCM additionally leaks its authentication subkey, which forgery
//! follows from. Any framing that puts many records under one key has to argue
//! carefully that its counter can never repeat.
//!
//! **This framing does not put many records under one key.** Every
//! [`bp_crypto::seal`] call draws a fresh 16-byte random salt, so every line's
//! AEAD key is a *different* Argon2id output even though every line's
//! passphrase is the same. A `(key, nonce)` pair therefore cannot repeat
//! across lines, whatever the nonces do. Within a line, ADR-0021's own scheme
//! applies unchanged: a fresh random nonce prefix per envelope, then the chunk
//! index as a big-endian counter. An audit record is one chunk, so the counter
//! never leaves zero.
//!
//! That is the whole cost/benefit of this framing stated plainly: it buys
//! nonce uniqueness for free by paying a key derivation per line, where a
//! session-key framing would buy a cheap derivation by taking on a nonce
//! counter to get right. Argon2id's price is real and is discussed on
//! [`PassphraseSealer`]; getting a nonce counter wrong is unrecoverable, and
//! the counter would have to be durable across process restarts, which is a
//! piece of state an append-only log deliberately does not keep.
//!
//! ## What sealing adds, and what it does not
//!
//! It adds per-line authentication. A sealed line cannot be *edited*
//! undetectably — not its event, not its timestamp, and not its sequence
//! number, which sits inside the ciphertext. The existing sequence check is
//! untouched and still catches deletion and reordering of any line but the
//! last, and it now runs on records that are individually authentic rather
//! than merely well-formed.
//!
//! It does not close ADR-0024's named gap. Truncating the tail, or discarding
//! the log entirely, remains undetectable, because the envelope authenticates
//! each line against its key and not against its neighbours or its file. A
//! line lifted from another log sealed with the same passphrase would also
//! verify, if it happened to land at the right sequence number. Closing either
//! needs something outside the editor's reach, exactly as ADR-0024 says.

use std::fmt;

use bp_crypto::{CryptoError, SealOptions};
use zeroize::Zeroizing;

use crate::{SealFailed, Sealer};

/// Seals audit lines with the document's own passphrase, through the `.bpadx`
/// envelope (ADR-0021).
///
/// The implementation ADR-0024 anticipated: "the implementation the shell
/// supplies wraps the document's `.bpadx` envelope, so an audit log is
/// readable by exactly the people who can read the document it describes".
///
/// **This type holds a passphrase, which is why it lives here and not beside
/// [`Record`].** It is deliberately not `Copy`, not `Clone` and not
/// `Serialize`: a copy of it is a copy of the document's key, and a
/// serialisable one could be written to the very file it exists to protect.
/// The passphrase is kept in a [`Zeroizing`] wrapper, so it is wiped when the
/// sealer is dropped rather than left in freed memory for the rest of the
/// process's life, and [`fmt::Debug`] prints the envelope settings and never
/// the secret.
///
/// [`Record`]: crate::Record
///
/// # Cost
///
/// One Argon2id derivation per line sealed, and one per line read back —
/// ~50–100 ms each at ADR-0021's baseline. That is affordable because
/// [`Event`] is a closed vocabulary of discrete user actions (a document was
/// unlocked; a scan finished; a redaction was applied) rather than anything a
/// loop produces: a busy session writes tens of lines, not thousands.
///
/// The read side is the one to watch, because opening the history derives once
/// per sealed line. A history of a few hundred lines is seconds to open. If
/// that ever becomes the complaint, the fix is a derive-once API on
/// `bp-crypto` — not a cheaper KDF here, which would weaken every line against
/// an offline attacker holding the file.
///
/// [`Event`]: crate::Event
pub struct PassphraseSealer {
    /// Wiped on drop. This is the document's key material and nothing else in
    /// this crate is.
    passphrase: Zeroizing<String>,
    options: SealOptions,
}

impl PassphraseSealer {
    /// Seal this document's audit lines with `passphrase`.
    ///
    /// Uses `bp-crypto`'s shipping defaults: XChaCha20-Poly1305 and Argon2id
    /// at ADR-0021's baseline. There is no cheaper variant of this
    /// constructor, because an audit log sealed at a lower cost than the
    /// document it describes would be the weakest copy of the document's
    /// secret on the disk.
    ///
    /// # Errors
    /// [`SealFailed`] if the passphrase is empty. Refused here rather than at
    /// the first event, so the shell finds out while it still has the user's
    /// attention on a passphrase prompt — and because an audit log that
    /// "opened with no passphrase" is not sealed in any sense the user would
    /// recognise (ADR-0021 refuses the same thing for the same reason).
    pub fn new(passphrase: &str) -> Result<Self, SealFailed> {
        Self::with_options(passphrase, SealOptions::default())
    }

    /// The same, with the envelope's settings chosen explicitly.
    ///
    /// For a caller that must pin a suite — AES-256-GCM answers compliance
    /// questions ADR-0021 expects to be asked — and for tests, which cannot
    /// afford a 19 MiB derivation per generated case.
    ///
    /// **Lowering `options.kdf` weakens every line this sealer writes.** It
    /// adds no capability that `bp_crypto::seal` did not already offer, so it
    /// is exposed rather than hidden; it is not the shipping path, and
    /// [`new`] is.
    ///
    /// [`new`]: PassphraseSealer::new
    ///
    /// # Errors
    /// [`SealFailed`] if the passphrase is empty.
    pub fn with_options(passphrase: &str, options: SealOptions) -> Result<Self, SealFailed> {
        if passphrase.is_empty() {
            return Err(SealFailed::new(EMPTY));
        }
        Ok(Self {
            passphrase: Zeroizing::new(passphrase.to_owned()),
            options,
        })
    }

    /// How lines are sealed, so a security inspector can say which suite and
    /// what cost without the caller having remembered what it passed in.
    #[must_use]
    pub const fn options(&self) -> SealOptions {
        self.options
    }
}

impl Sealer for PassphraseSealer {
    fn seal(&self, plaintext: &[u8]) -> Result<Vec<u8>, SealFailed> {
        bp_crypto::seal(plaintext, self.passphrase.as_str(), self.options).map_err(refuse)
    }

    fn unseal(&self, sealed: &[u8]) -> Result<Vec<u8>, SealFailed> {
        // `bp_crypto::open` hands back a `Zeroizing<Vec<u8>>` because it is
        // built for document content. Copying out of it into a plain `Vec` is
        // correct *here* and would not be anywhere else: what comes out is one
        // serialised `Record`, and a `Record` is numbers, flags and words from
        // a closed vocabulary. There is no secret in it to leave behind — that
        // is the crate's central guarantee, and this is the one place that
        // guarantee is being cashed in rather than merely asserted.
        bp_crypto::open(sealed, self.passphrase.as_str())
            .map(|plain| plain.to_vec())
            .map_err(refuse)
    }
}

impl fmt::Debug for PassphraseSealer {
    /// Derived `Debug` would print the passphrase, and a `Debug` line is one
    /// of the two classic routes by which a secret reaches a log file. The
    /// other is an error message, which [`SealFailed`] closes by holding only
    /// `&'static str`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PassphraseSealer")
            .field("suite", &self.options.suite.name())
            .field("kdf", &self.options.kdf)
            .finish_non_exhaustive()
    }
}

const EMPTY: &str = "an audit log cannot be sealed with an empty passphrase";

/// Restate a [`CryptoError`] in words fixed when this code was written.
///
/// Deliberately a `match` and not `error.to_string()`. `SealFailed` holds a
/// `&'static str` precisely so that no run-time text can reach an audit error
/// path, and two of `CryptoError`'s variants carry a `String` from the OS or
/// from `argon2`. Formatting the error would launder that text straight into
/// the message the user is shown and pastes into a bug report. The exhaustive
/// match also means a new `CryptoError` variant is a compile error here rather
/// than a silently generic message.
///
/// The wording keeps ADR-0021's most important admission: a wrong passphrase
/// and an altered line are the same event, and no honest implementation can
/// tell them apart.
fn refuse(error: CryptoError) -> SealFailed {
    SealFailed::new(match error {
        CryptoError::CannotOpen => {
            "wrong passphrase, or this line of the security history has been altered or damaged"
        }
        CryptoError::NotBpadx => "this line of the security history is not a sealed record",
        CryptoError::UnsupportedVersion { .. } => {
            "this security history was written by a newer BachelorPad+"
        }
        CryptoError::UnsupportedSuite(_) | CryptoError::UnsupportedKdf(_) => {
            "this security history was sealed with encryption this build does not know"
        }
        CryptoError::Truncated | CryptoError::Corrupt => {
            "this line of the security history is incomplete or damaged"
        }
        CryptoError::UnreasonableCost { .. } | CryptoError::UnreasonableChunkSize(_) => {
            "this line of the security history asks for an unreasonable amount of \
             work to open, and was refused"
        }
        CryptoError::EmptyPassphrase => EMPTY,
        CryptoError::NoRandomness(_) => {
            "the system random number generator could not be read, so nothing was sealed"
        }
        CryptoError::Kdf(_) => "the document key could not be derived from its passphrase",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{AuditError, AuditLog, DocumentId, Event, ProfileLabel, Record};
    use bp_crypto::KdfParams;
    use bp_security::Profile;
    use proptest::prelude::*;
    use time::OffsetDateTime;

    // -- helpers -----------------------------------------------------------

    /// `bp-crypto`'s minimum Argon2id parameters, used by every test below
    /// that is not *about* the cost.
    ///
    /// The shipping default is 19 MiB, t=2, and ~50-100 ms per line (ADR-0021),
    /// which is the right cost for a real audit log and is what
    /// `PassphraseSealer::new` uses. Multiplied across a property test that
    /// seals and reopens a whole log per case, it is minutes of gate time for
    /// no extra coverage, because none of these properties has anything to do
    /// with how expensive the derivation is. The test named for the shipping
    /// cost asserts, separately and once, that the shipping path is not this
    /// one.
    ///
    /// Matches `bp-crypto`'s own `cheap()`, which exists for the same reason.
    fn cheap() -> SealOptions {
        SealOptions {
            kdf: KdfParams {
                memory_kib: 8,
                iterations: 1,
                lanes: 1,
            },
            ..SealOptions::default()
        }
    }

    /// Not a real credential, and never read from anywhere: the whole point of
    /// this crate is that a fixture cannot leak, because the log has nowhere
    /// to put one.
    const PASS: &str = "correct horse battery staple";
    const OTHER: &str = "a different document entirely";

    fn sealer(passphrase: &str) -> PassphraseSealer {
        PassphraseSealer::with_options(passphrase, cheap()).expect("a non-empty passphrase")
    }

    fn at(seconds: i64) -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(seconds).expect("timestamp in range")
    }

    fn any_event() -> impl Strategy<Value = Event> {
        prop_oneof![
            Just(Event::DocumentEncrypted),
            Just(Event::DocumentDecrypted),
            Just(Event::DocumentUnlocked),
            any::<u32>().prop_map(|attempt| Event::UnlockFailed { attempt }),
            Just(Event::SecurityProfileChanged {
                from: ProfileLabel::Standard,
                to: ProfileLabel::Maximum,
            }),
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

    /// Write `events` to a sealed history under the Private profile, and hand
    /// back the path, the records as the writer saw them, and the raw file.
    fn sealed_log(
        dir: &tempfile::TempDir,
        keyed: &PassphraseSealer,
        events: &[Event],
    ) -> (std::path::PathBuf, Vec<Record>, String) {
        let path = dir.path().join("security.log");
        let mut log = AuditLog::open(path.clone(), Some(keyed)).expect("a fresh log opens");
        for (index, event) in events.iter().enumerate() {
            log.append(
                at(1_700_000_000 + i64::try_from(index).expect("a small index")),
                Some(DocumentId::new(0x7f3a_1c04)),
                *event,
                Profile::Private.policy(),
                Some(keyed),
            )
            .expect("Private with a key writes a sealed line");
        }
        let raw = if events.is_empty() {
            String::new()
        } else {
            std::fs::read_to_string(&path).expect("the log is readable")
        };
        (path, log.records().to_vec(), raw)
    }

    // -- construction ------------------------------------------------------

    #[test]
    fn an_empty_passphrase_is_refused_when_the_sealer_is_built() {
        // Refused here rather than at the first event, so the shell learns of
        // it while the user is still looking at a passphrase prompt.
        let err = PassphraseSealer::new("").expect_err("an empty passphrase seals nothing");
        assert_eq!(err.reason(), EMPTY);
        assert!(err.to_string().contains("empty passphrase"));
    }

    #[test]
    fn the_shipping_cost_is_the_envelopes_own_default() {
        // The one test that pays the real derivation, and the reason every
        // other test may use `cheap()` without that being a way for the
        // shipping cost to drift downwards unnoticed.
        let keyed = PassphraseSealer::new(PASS).expect("a non-empty passphrase");
        assert_eq!(keyed.options(), SealOptions::default());
        assert_eq!(keyed.options().kdf, KdfParams::default());
        assert_eq!(keyed.options().kdf.memory_kib, 19 * 1024);

        // And it actually works at that cost, once.
        let sealed = keyed.seal(b"{\"seq\":1}").expect("a line seals");
        assert_eq!(keyed.unseal(&sealed).expect("and opens"), b"{\"seq\":1}");
    }

    #[test]
    fn debug_names_the_envelope_and_never_the_passphrase() {
        // A `Debug` line is one of the two classic routes a secret takes into
        // a log file. The other is an error message, which `SealFailed`
        // closes by construction.
        let rendered = format!("{:?}", sealer(PASS));
        assert!(
            !rendered.contains("correct horse"),
            "the sealer printed its passphrase: {rendered}"
        );
        assert!(rendered.contains("XChaCha20-Poly1305"), "got {rendered}");
    }

    // -- the headline properties -------------------------------------------

    proptest! {
        // Each case seals and reopens a whole log, and every append fsyncs.
        // Fewer cases, deliberately: the interesting space here is the shape
        // of a log, not the number of logs.
        #![proptest_config(ProptestConfig::with_cases(24))]

        #[test]
        fn a_sealed_log_round_trips_exactly(
            events in proptest::collection::vec(any_event(), 1..6),
        ) {
            // Headline property one. Not "reads back", but *exactly*: same
            // events, same sequence, same timestamps, same document.
            let dir = tempfile::tempdir().expect("a temporary directory");
            let keyed = sealer(PASS);
            let (path, written, _) = sealed_log(&dir, &keyed, &events);

            let reopened = AuditLog::open(path, Some(&keyed)).expect("the key opens it");
            prop_assert_eq!(reopened.records(), written.as_slice());
            for (index, record) in reopened.records().iter().enumerate() {
                prop_assert_eq!(record.seq(), u64::try_from(index).expect("small") + 1);
                prop_assert_eq!(record.event(), events[index]);
            }
        }

        #[test]
        fn a_sealed_log_is_unreadable_without_the_passphrase(
            events in proptest::collection::vec(any_event(), 1..4),
        ) {
            // Headline property two, in both of the two ways a reader can
            // lack the key: no sealer at all, and the wrong one.
            let dir = tempfile::tempdir().expect("a temporary directory");
            let keyed = sealer(PASS);
            let (path, _, _) = sealed_log(&dir, &keyed, &events);

            let err = AuditLog::open(path.clone(), None).expect_err("no key, no history");
            prop_assert!(matches!(err, AuditError::Sealed { line: 1 }), "got {}", err);

            // A log sealed with one document's passphrase does not open under
            // another's: the access rule ADR-0024 wanted, arrived at for free.
            let wrong = sealer(OTHER);
            let err = AuditLog::open(path, Some(&wrong)).expect_err("the wrong key opens nothing");
            prop_assert!(matches!(err, AuditError::Seal(_)), "got {}", err);
            prop_assert!(err.to_string().contains("wrong passphrase"));
        }

        #[test]
        fn the_file_on_disk_never_holds_the_plaintext_of_any_event(
            events in proptest::collection::vec(any_event(), 1..6),
        ) {
            // Headline property three, and the one that would fail loudest if
            // a future change wrote anything beside the ciphertext.
            //
            // Checked against the *decoded* bytes as well as the text on the
            // line, which is the version of this test that actually holds: hex
            // is an encoding, so a log that had quietly stopped encrypting and
            // was only hex-encoding its records would pass a substring search
            // of the file and fail here. Every line is asserted to be a real
            // `.bpadx` envelope for the same reason.
            let dir = tempfile::tempdir().expect("a temporary directory");
            let keyed = sealer(PASS);
            let (_, written, raw) = sealed_log(&dir, &keyed, &events);

            let mut decoded: Vec<u8> = Vec::new();
            for line in raw.lines() {
                let bytes = crate::hex_decode(line).expect("a sealed line is hex");
                prop_assert!(
                    bp_crypto::is_bpadx(&bytes),
                    "a sealed audit line is not a `.bpadx` envelope"
                );
                decoded.extend_from_slice(&bytes);
            }

            for word in [
                "seq", "at_unix", "at_offset", "document", "event", "kind",
                "document_encrypted", "document_decrypted", "document_unlocked",
                "unlock_failed", "security_profile_changed", "privacy_mode_entered",
                "privacy_mode_left", "secret_scan_finished", "signature_made",
                "signature_verified", "redaction_applied",
            ] {
                prop_assert!(
                    !raw.contains(word),
                    "a sealed audit file contains the plaintext {:?}",
                    word
                );
                prop_assert!(
                    !contains(&decoded, word.as_bytes()),
                    "a sealed audit line decodes to something containing {:?}",
                    word
                );
            }
            for record in &written {
                let json = serde_json::to_string(record).expect("a record serialises");
                prop_assert!(!raw.contains(&json), "a sealed audit file contains a record in clear");
                prop_assert!(
                    !contains(&decoded, json.as_bytes()),
                    "a sealed audit line decodes to a record in clear"
                );
            }
            prop_assert!(!raw.contains(PASS), "a sealed audit file contains the passphrase");
            prop_assert!(
                !contains(&decoded, PASS.as_bytes()),
                "a sealed audit line decodes to the passphrase"
            );
        }

        #[test]
        fn sealing_one_plaintext_twice_never_repeats_the_key_or_the_nonce(
            count in 2usize..6,
        ) {
            // Why there is no nonce question in this framing, checked on the
            // sharpest case there is: the *same bytes*, sealed repeatedly
            // under the *same* passphrase.
            //
            // Two assertions, and the second is the one that matters. Distinct
            // outputs would follow from a fresh nonce alone. Distinct
            // *envelope headers* is the stronger claim: the random salt and
            // the random nonce prefix both live there, so two identical
            // headers would mean one Argon2id key reused with one nonce --
            // which for XChaCha20-Poly1305 leaks the XOR of the two records
            // and for AES-256-GCM additionally leaks the authentication
            // subkey, from which forgery follows. That is the failure this
            // framing exists to make unreachable, and it is unreachable
            // because a fresh salt per envelope means the key is fresh per
            // line, not merely the nonce.
            let keyed = sealer(PASS);
            let plaintext = b"{\"seq\":1,\"at_unix\":0,\"at_offset\":0,\
                              \"event\":{\"kind\":\"privacy_mode_entered\"}}";

            let sealed: Vec<Vec<u8>> = (0..count)
                .map(|_| keyed.seal(plaintext).expect("a line seals"))
                .collect();

            let whole: std::collections::BTreeSet<&[u8]> =
                sealed.iter().map(Vec::as_slice).collect();
            prop_assert_eq!(whole.len(), count, "one plaintext sealed to identical bytes twice");

            // The magic, version, suite tag and KDF cost are legitimately the
            // same on every line; the salt begins after them and everything
            // from there on must vary. Sliced rather than parsed because
            // `Header::parse` is `bp-crypto`'s own business.
            const AFTER_THE_FIXED_HEADER_FIELDS: usize = 22;
            let headers: std::collections::BTreeSet<&[u8]> = sealed
                .iter()
                .map(|s| &s[AFTER_THE_FIXED_HEADER_FIELDS..AFTER_THE_FIXED_HEADER_FIELDS + 16])
                .collect();
            prop_assert_eq!(
                headers.len(),
                count,
                "two envelopes drew the same salt, so two lines share one key"
            );
        }
    }

    /// Substring search over bytes, for asserting that a decoded line does not
    /// contain something it must not.
    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        !needle.is_empty() && haystack.windows(needle.len()).any(|w| w == needle)
    }

    // -- what sealing preserves, and what it adds --------------------------

    #[test]
    fn deleting_an_interior_sealed_line_is_still_detected() {
        // The property that already existed, checked under the new framing.
        // Sealing per line must not cost the sequence check: the numbers live
        // inside the ciphertext, so the run 1, 2, 3, ... is verified after
        // each line is opened, exactly as before.
        let dir = tempfile::tempdir().expect("a temporary directory");
        let keyed = sealer(PASS);
        let (path, _, raw) = sealed_log(
            &dir,
            &keyed,
            &[
                Event::DocumentUnlocked,
                Event::SignatureMade,
                Event::PrivacyModeLeft,
            ],
        );

        let kept: Vec<&str> = raw.lines().skip(1).collect();
        std::fs::write(&path, kept.join("\n") + "\n").expect("the edit is written");

        let err = AuditLog::open(path, Some(&keyed)).expect_err("an edited history is reported");
        assert!(
            matches!(
                err,
                AuditError::OutOfOrder {
                    line: 1,
                    expected: 1,
                    found: 2
                }
            ),
            "got {err}"
        );
    }

    #[test]
    fn altering_a_sealed_line_is_detected_which_a_plaintext_one_is_not() {
        // What sealing *adds* over ADR-0024's plaintext log. There, a line can
        // be rewritten with a consistent sequence number and nothing can tell.
        // Here every line is authenticated by the document's envelope, so a
        // single flipped nibble is caught -- which is exactly the "part of the
        // way for free" ADR-0024 claimed and could not yet demonstrate.
        let dir = tempfile::tempdir().expect("a temporary directory");
        let keyed = sealer(PASS);
        let (path, _, raw) = sealed_log(&dir, &keyed, &[Event::SignatureVerified { valid: false }]);

        let line = raw.lines().next().expect("one line");
        for victim in [0usize, line.len() / 2, line.len() - 1] {
            let mut bytes: Vec<u8> = line.as_bytes().to_vec();
            // Stay inside the hex alphabet, so this is a changed *record* and
            // not merely an unparseable line.
            bytes[victim] = if bytes[victim] == b'a' { b'b' } else { b'a' };
            let edited = String::from_utf8(bytes).expect("hex is ASCII");
            std::fs::write(&path, edited + "\n").expect("the edit is written");

            let err =
                AuditLog::open(path.clone(), Some(&keyed)).expect_err("an altered line is caught");
            assert!(
                matches!(err, AuditError::Seal(_)),
                "an altered sealed line was accepted at byte {victim}: {err}"
            );
        }
    }

    #[test]
    fn a_history_whose_profile_tightened_reads_back_whole_under_a_real_key() {
        // ADR-0024's mixed file, with the real sealer rather than a stand-in:
        // plaintext lines from before the profile change, sealed lines after,
        // one continuous sequence.
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("security.log");
        let keyed = sealer(PASS);
        let mut log = AuditLog::open(path.clone(), Some(&keyed)).expect("a fresh log opens");

        log.append(
            at(1),
            None,
            Event::DocumentEncrypted,
            Profile::Standard.policy(),
            Some(&keyed),
        )
        .expect("plaintext line");
        log.append(
            at(2),
            None,
            Event::DocumentUnlocked,
            Profile::Private.policy(),
            Some(&keyed),
        )
        .expect("sealed line");

        let raw = std::fs::read_to_string(&path).expect("readable");
        let mut lines = raw.lines();
        assert!(
            lines.next().expect("first line").starts_with('{'),
            "the pre-change line should still be plaintext JSON"
        );
        assert!(
            !lines.next().expect("second line").starts_with('{'),
            "the post-change line should be sealed"
        );

        let reopened = AuditLog::open(path, Some(&keyed)).expect("both forms read back");
        assert_eq!(reopened.records(), log.records());
        assert_eq!(reopened.records().len(), 2);
    }

    // -- what a refusal is allowed to say ----------------------------------

    #[test]
    fn a_refusal_says_only_words_chosen_when_this_code_was_written() {
        // `SealFailed` holds a `&'static str`, so this cannot fail by
        // accident -- but `refuse` could still have been written as
        // `error.to_string()` against a `String`-carrying variant, and the
        // two variants that carry one are the two whose text comes from the
        // OS and from `argon2`. This pins the choice.
        let keyed = sealer(PASS);

        // Not an envelope at all.
        let err = keyed.unseal(b"not a bpadx document").expect_err("refused");
        assert_eq!(err.reason(), refuse(CryptoError::NotBpadx).reason());

        // The dynamic variants, restated rather than formatted.
        let leaky = "/home/danie/Divorce settlement.bpadx: hunter2";
        for error in [
            CryptoError::NoRandomness(leaky.to_owned()),
            CryptoError::Kdf(leaky.to_owned()),
        ] {
            let message = refuse(error).reason();
            assert!(
                !message.contains("hunter2") && !message.contains("Divorce"),
                "a crypto error's run-time text reached an audit message: {message}"
            );
        }
    }

    #[test]
    fn a_wrong_passphrase_and_a_damaged_line_are_the_same_message() {
        // ADR-0021's admission, carried through rather than papered over. An
        // audit message that claimed to tell them apart would be claiming to
        // trust something outside the authenticated data.
        let keyed = sealer(PASS);
        let sealed = keyed.seal(b"{\"seq\":1}").expect("a line seals");

        let wrong_key = sealer(OTHER).unseal(&sealed).expect_err("wrong key");

        let mut damaged = sealed.clone();
        let last = damaged.len() - 1;
        damaged[last] ^= 0x01;
        let altered = keyed.unseal(&damaged).expect_err("altered ciphertext");

        assert_eq!(wrong_key, altered);
        assert!(wrong_key.to_string().contains("wrong passphrase"));
        assert!(wrong_key.to_string().contains("altered or damaged"));
    }

    proptest! {
        #[test]
        fn a_sealed_line_round_trips_any_bytes(
            plaintext in proptest::collection::vec(any::<u8>(), 0..512),
        ) {
            // The `Sealer` contract on its own, away from the log: whatever
            // goes in comes back, byte for byte, including empty.
            let keyed = sealer(PASS);
            let sealed = keyed.seal(&plaintext).expect("seals");
            prop_assert_ne!(sealed.as_slice(), plaintext.as_slice());
            prop_assert_eq!(keyed.unseal(&sealed).expect("opens"), plaintext);
        }
    }
}
