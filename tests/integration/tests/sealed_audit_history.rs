//! Seam: `bp-audit` sealed with `bp-crypto`, governed by `bp-security`.
//!
//! Three crates that each refuse to do the others' job. `bp-security` decides
//! and performs nothing. `bp-audit` decides *that* a line must be sealed and
//! performs no cryptography -- it holds a `Sealer` trait and nothing else.
//! `bp-crypto` seals bytes and has never heard of a policy. The sealed path
//! only became real when `PassphraseSealer` arrived, and until now nothing has
//! ever run a policy, a sealer and a file together.
//!
//! **These tests assert absence at the strict end**, the same way
//! `security_profile_governs_dependants` does, because that is where the leak
//! is. A profile that writes a readable log is easy to test and nearly
//! worthless; what matters is that the profiles which promised a sealed log
//! wrote a file with no event, no verb and no number of anybody's in it, and
//! that the profiles which promised nothing wrote nothing at all.
//!
//! The properties:
//!
//! * every profile's destination is the one `bp-security` implies, under
//!   Privacy Mode as well as off;
//! * a sealed file contains **no plaintext of any event** and does not name
//!   the document -- checked by hunting for every label and sentence the
//!   vocabulary can produce, not just the ones this test appended;
//! * a sealed file is opaque: every line is hex and nothing else;
//! * a sealed history reopens with the key, and the sequence run is intact;
//! * a sealed history refuses to open **without** the key rather than
//!   quietly showing the readable half;
//! * deleting an interior line is detected and reported as tampering;
//! * a profile that forbids a durable record leaves no file behind at all.
//!
//! ## Cost
//!
//! `PassphraseSealer` uses `bp-crypto`'s shipping defaults, which are the
//! OWASP baseline -- 19 MiB and two passes of Argon2id, per line, on the way
//! in *and* on the way out. That is right for a document and ruinous for a
//! test suite that seals a few dozen lines and reopens them. Every sealer
//! here is built with [`common::cheap`], which changes **only** the KDF
//! parameters: the suite, the framing, the nonce construction and the AAD are
//! all the shipping ones, so what is under test is the format that ships.

mod common;

use common::{Pass, cheap};

use std::path::Path;

use bp_audit::{
    Appended, AuditError, AuditLog, Destination, DocumentId, Event, NotWritten, PassphraseSealer,
    ProfileLabel, Sealer,
};
use bp_security::{
    Embeddings, Metadata, Network, Privacy, Profile, Recovery, Security, TemporaryFiles, Zeroise,
};
use tempfile::{TempDir, tempdir};
use time::OffsetDateTime;

/// A filename that appears nowhere else, so finding it in the log is proof
/// that a path reached a file that must never hold one. Not a credential.
const DOCUMENT_SENTINEL: &str = "divorce-settlement-b41d7e0a.bpadx";

/// The passphrase every sealer in this file uses. Never printed; see `Pass`.
fn passphrase() -> Pass {
    Pass("correct horse battery staple".to_owned())
}

fn sealer() -> PassphraseSealer {
    // `cheap()` and not `PassphraseSealer::new`: see the module docs. The
    // envelope is the shipping one; only the derivation cost is turned down.
    PassphraseSealer::with_options(passphrase().as_str(), cheap()).expect("sealer")
}

/// A representative slice of the whole vocabulary, including every payload
/// shape: a count, an attempt number, a flag, and a pair of profile labels.
fn the_events() -> Vec<Event> {
    vec![
        Event::DocumentUnlocked,
        Event::UnlockFailed { attempt: 41 },
        Event::SecretScanFinished { findings: 3 },
        Event::SecurityProfileChanged {
            from: ProfileLabel::Standard,
            to: ProfileLabel::Confidential,
        },
        Event::SignatureVerified { valid: false },
        Event::RedactionApplied {
            spans: 2,
            bytes_removed: 91,
        },
    ]
}

fn at(index: u64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(1_700_000_000 + index as i64).expect("a real time")
}

/// Every phrase the crate can put in front of a user, plus every stable
/// label, plus the payload numbers.
///
/// Hunting for all of them rather than only the ones appended is deliberate:
/// a sealer that leaked one event's rendering would otherwise be caught only
/// if that event happened to be in the fixture.
fn every_plaintext_phrase() -> Vec<String> {
    let mut phrases = Vec::new();
    for event in the_events() {
        phrases.push(event.describe());
        phrases.push(event.label().to_owned());
    }
    for extra in [
        "unlock",
        "redact",
        "signature",
        "privacy",
        "profile",
        "seq",
        "kind",
        "document",
    ] {
        phrases.push(extra.to_owned());
    }
    phrases.push(DOCUMENT_SENTINEL.to_owned());
    phrases
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    let needle = needle.as_bytes();
    !needle.is_empty() && haystack.windows(needle.len()).any(|w| w == needle)
}

struct Fixture {
    _dir: TempDir,
    log: std::path::PathBuf,
}

fn fixture() -> Fixture {
    let dir = tempdir().expect("temp dir");
    // Deliberately named after the document, so that a crate which ever
    // learned to write its own location into a line would be caught here as
    // well as by the sentinel hunt.
    let log = dir.path().join(format!("{DOCUMENT_SENTINEL}.audit"));
    Fixture { _dir: dir, log }
}

/// Append the whole vocabulary under `security`/`privacy`, and say what
/// happened to each.
fn append_everything(
    path: &Path,
    security: &Security,
    privacy: Privacy,
    sealer: Option<&dyn Sealer>,
) -> Vec<Appended> {
    let policy = security.policy_under(privacy);
    let mut log = AuditLog::open(path.to_path_buf(), sealer).expect("open");
    the_events()
        .into_iter()
        .enumerate()
        .map(|(index, event)| {
            log.append(
                at(index as u64),
                Some(DocumentId::new(7)),
                event,
                policy,
                sealer,
            )
            .expect("append")
        })
        .collect()
}

/// A policy spelled out rather than named.
///
/// Needed because only two of `bp-security`'s axes reach `Destination`, and
/// among the four named profiles exactly one -- Private, with Privacy Mode off
/// -- lands on `Sealed`. Testing the sealed path through that one profile
/// alone would leave both of `Destination::for_policy`'s routes to it
/// half-covered. The pair below covers both: `Recovery::Encrypted` with
/// metadata still permitted, and `Recovery::Disabled` with metadata still
/// permitted -- the second being the case `Destination::for_policy` calls out
/// by name, because a profile that has switched off plaintext recovery has
/// stated its view of plaintext artefacts.
fn custom(metadata: Metadata, recovery: Recovery) -> Security {
    Security::Custom(bp_security::Policy {
        recovery,
        metadata,
        embeddings: Embeddings::Local,
        network: Network::Denied,
        temporary_files: TemporaryFiles::Allowed,
        zeroise: Zeroise::On,
    })
}

/// Every security setting this file exercises, under both privacy modes.
fn every_case() -> Vec<(Security, Privacy)> {
    let mut settings: Vec<Security> = Profile::all()
        .iter()
        .copied()
        .map(Security::Named)
        .collect();
    settings.push(custom(Metadata::Summary, Recovery::Encrypted));
    settings.push(custom(Metadata::PathOnly, Recovery::Disabled));
    settings.push(custom(Metadata::Disabled, Recovery::Plaintext));
    settings
        .into_iter()
        .flat_map(|s| [(s, Privacy::Off), (s, Privacy::On)])
        .collect()
}

// --- where each profile sends its history --------------------------------

#[test]
fn the_destination_is_the_one_the_profile_implies_under_both_privacy_modes() {
    // Not a restatement of `Destination::for_policy`: this goes through
    // `in_force`, which is the call the shell must make, and it pins the
    // reading of the table that the strict assertions below depend on. If
    // this drifts, every "no plaintext" assertion below could pass by testing
    // a profile that never wrote anything.
    for (security, privacy) in every_case() {
        let policy = security.policy_under(privacy);
        let destination = Destination::in_force(&security, privacy);

        // Read off `bp-security`'s two governing axes rather than off a table
        // of profile names, so this stays a statement about the *policy* --
        // which is what `Destination` claims to resolve.
        let expected = match (policy.metadata, policy.recovery) {
            (Metadata::Disabled, _) => Destination::SessionOnly,
            (_, Recovery::Plaintext) => Destination::PlaintextFile,
            (_, Recovery::Encrypted | Recovery::Disabled) => Destination::Sealed,
        };
        assert_eq!(
            destination,
            expected,
            "{} under privacy {:?}",
            security.name(),
            privacy
        );

        // Privacy Mode can only tighten, so it can never turn a history that
        // was going nowhere into one that goes to a file. It happens to
        // silence *every* setting, because it clamps metadata to Disabled --
        // which is why the sealed cases below are reached with Privacy Mode
        // off, and why `custom` exists at all.
        if privacy.is_on() {
            assert_eq!(
                destination,
                Destination::SessionOnly,
                "Privacy Mode left {} still writing to disk",
                security.name()
            );
        }
    }
}

#[test]
fn a_profile_that_keeps_no_durable_record_leaves_no_file_at_all() {
    for (security, privacy) in every_case() {
        if Destination::in_force(&security, privacy).writes_to_disk() {
            continue;
        }
        let fixture = fixture();
        let sealer = sealer();

        let outcomes = append_everything(&fixture.log, &security, privacy, Some(&sealer));
        assert!(
            outcomes
                .iter()
                .all(|o| *o == Appended::NotWritten(NotWritten::ProfileForbidsIt)),
            "{} wrote something it promised not to: {outcomes:?}",
            security.name()
        );
        assert!(
            !fixture.log.exists(),
            "{} created a history file despite keeping no durable record",
            security.name()
        );
    }
}

#[test]
fn a_sealed_profile_refuses_rather_than_downgrading_when_there_is_no_key() {
    // The rule ADR-0020 states in as many words: a security control that
    // quietly weakens itself is worse than an absent one. A plaintext line
    // here would be the whole leak.
    for (security, privacy) in every_case() {
        if Destination::in_force(&security, privacy) != Destination::Sealed {
            continue;
        }
        let fixture = fixture();

        let outcomes = append_everything(&fixture.log, &security, privacy, None);
        assert!(
            outcomes
                .iter()
                .all(|o| *o == Appended::NotWritten(NotWritten::NoSealer)),
            "{} fell back to something without a key: {outcomes:?}",
            security.name()
        );
        assert!(
            !fixture.log.exists(),
            "{} wrote a file with no key to seal it with",
            security.name()
        );
        assert!(
            NotWritten::NoSealer.notice().is_some(),
            "the user is told nothing about a history that could not be kept"
        );
    }
}

// --- the strict end: what a sealed file may contain ----------------------

#[test]
fn a_sealed_history_holds_no_plaintext_and_does_not_name_the_document() {
    for (security, privacy) in every_case() {
        if Destination::in_force(&security, privacy) != Destination::Sealed {
            continue;
        }
        let fixture = fixture();
        let sealer = sealer();

        let outcomes = append_everything(&fixture.log, &security, privacy, Some(&sealer));
        assert!(
            outcomes
                .iter()
                .enumerate()
                .all(|(i, o)| *o == Appended::Persisted(i as u64 + 1)),
            "{} did not persist the run 1..n: {outcomes:?}",
            security.name()
        );

        let bytes = std::fs::read(&fixture.log).expect("read the history");
        assert!(!bytes.is_empty(), "the sealed history is empty");

        for phrase in every_plaintext_phrase() {
            assert!(
                !contains(&bytes, &phrase),
                "{} left {phrase:?} in clear in its sealed history",
                security.name()
            );
        }
        assert!(
            !contains(&bytes, passphrase().as_str()),
            "{} wrote the passphrase into the file it protects",
            security.name()
        );

        // Opaque, not merely free of the words this test thought of. Every
        // line is lowercase hex of an authenticated envelope and nothing
        // else, so there is no room in the file for anything readable.
        let text = String::from_utf8(bytes).expect("hex is text");
        for (number, line) in text.lines().enumerate() {
            assert!(
                !line.is_empty()
                    && line.len().is_multiple_of(2)
                    && line
                        .chars()
                        .all(|c| c.is_ascii_digit() || matches!(c, 'a'..='f')),
                "line {} of {}'s sealed history is not opaque hex",
                number + 1,
                security.name(),
            );
        }
    }
}

#[test]
fn a_plaintext_profile_really_does_write_the_readable_thing() {
    // The control for the test above. Without it, a `bp-audit` that had
    // silently stopped writing anything at all would pass every absence
    // assertion in this file.
    let security = Security::Named(Profile::Standard);
    let fixture = fixture();

    let outcomes = append_everything(&fixture.log, &security, Privacy::Off, None);
    assert!(outcomes.iter().all(|o| matches!(o, Appended::Persisted(_))));

    let text = std::fs::read_to_string(&fixture.log).expect("read");
    assert!(
        text.contains("unlock_failed") || text.contains("unlock"),
        "Standard's history is not readable, so the sealed comparison is vacuous"
    );
    // Even in clear, the document is a number and never a name.
    assert!(
        !text.contains(DOCUMENT_SENTINEL),
        "the plaintext history names the document"
    );
}

// --- reopening, and detecting an edit -------------------------------------

#[test]
fn a_sealed_history_reopens_with_the_key_and_the_run_is_intact() {
    let security = Security::Named(Profile::Private);
    let fixture = fixture();
    let sealer = sealer();

    append_everything(&fixture.log, &security, Privacy::Off, Some(&sealer));

    let reopened = AuditLog::open(fixture.log.clone(), Some(&sealer)).expect("reopen");
    let records = reopened.records();
    assert_eq!(records.len(), the_events().len());
    for (index, (record, event)) in records.iter().zip(the_events()).enumerate() {
        assert_eq!(record.seq(), index as u64 + 1, "the run is not 1..n");
        assert_eq!(record.event(), event, "entry {} came back different", index);
        assert_eq!(record.at(), at(index as u64));
        assert_eq!(record.document(), Some(DocumentId::new(7)));
    }
    assert_eq!(reopened.next_sequence(), the_events().len() as u64 + 1);
}

#[test]
fn a_sealed_history_will_not_open_without_the_key() {
    // Not "shows the readable half": a history that quietly hid the
    // interesting part would be worse than one that refused.
    let security = Security::Named(Profile::Private);
    let fixture = fixture();
    let sealer = sealer();
    append_everything(&fixture.log, &security, Privacy::Off, Some(&sealer));

    match AuditLog::open(fixture.log.clone(), None) {
        Err(AuditError::Sealed { line }) => assert_eq!(line, 1),
        other => panic!("a sealed history opened without its key: {other:?}"),
    }
}

#[test]
fn a_sealed_history_will_not_open_under_the_wrong_key() {
    let security = Security::Named(Profile::Private);
    let fixture = fixture();
    let sealer = sealer();
    append_everything(&fixture.log, &security, Privacy::Off, Some(&sealer));

    let wrong = PassphraseSealer::with_options("correct horse battery stapl", cheap())
        .expect("wrong sealer");
    assert!(
        AuditLog::open(fixture.log.clone(), Some(&wrong)).is_err(),
        "the wrong passphrase opened the history"
    );
}

#[test]
fn deleting_an_interior_line_of_a_sealed_history_is_detected() {
    let security = Security::Named(Profile::Private);
    let fixture = fixture();
    let sealer = sealer();
    append_everything(&fixture.log, &security, Privacy::Off, Some(&sealer));

    let original = std::fs::read_to_string(&fixture.log).expect("read");
    let lines: Vec<&str> = original.lines().collect();
    assert!(
        lines.len() >= 4,
        "the fixture is too short to have an interior"
    );

    // Every interior line in turn: removing any one of them breaks the run,
    // and a check that only noticed the first would still pass a one-line
    // test.
    for removed in 1..lines.len() - 1 {
        let mut kept = lines.clone();
        kept.remove(removed);
        let mut damaged = kept.join("\n");
        damaged.push('\n');
        std::fs::write(&fixture.log, &damaged).expect("write");

        match AuditLog::open(fixture.log.clone(), Some(&sealer)) {
            Err(AuditError::OutOfOrder {
                line,
                expected,
                found,
            }) => {
                assert_eq!(line, removed + 1);
                assert_eq!(expected, removed as u64 + 1);
                assert_eq!(found, removed as u64 + 2);
            }
            other => panic!("removing line {} was not detected: {other:?}", removed + 1),
        }
    }
}

#[test]
fn altering_a_sealed_line_is_detected_rather_than_decoded() {
    // What sealing buys over the plain sequence check: a line cannot be
    // *edited* undetectably, because the envelope authenticates it. The
    // sequence number sits inside the ciphertext, so an attacker cannot even
    // renumber without the key.
    let security = Security::Named(Profile::Private);
    let fixture = fixture();
    let sealer = sealer();
    append_everything(&fixture.log, &security, Privacy::Off, Some(&sealer));

    let original = std::fs::read_to_string(&fixture.log).expect("read");
    let lines: Vec<String> = original.lines().map(str::to_owned).collect();

    for target in 0..lines.len() {
        let mut damaged = lines.clone();
        // Flip one hex digit. `f` -> `e` and anything else -> `f`, so the
        // line stays hex and stays the same length: the change has to be
        // caught by the envelope rather than by the shape of the file.
        let mut chars: Vec<char> = damaged[target].chars().collect();
        let middle = chars.len() / 2;
        chars[middle] = if chars[middle] == 'f' { 'e' } else { 'f' };
        damaged[target] = chars.into_iter().collect();

        let mut text = damaged.join("\n");
        text.push('\n');
        std::fs::write(&fixture.log, &text).expect("write");

        assert!(
            AuditLog::open(fixture.log.clone(), Some(&sealer)).is_err(),
            "an altered byte in line {} of a sealed history went undetected",
            target + 1,
        );
    }
}

#[test]
fn a_history_that_was_never_written_is_empty_rather_than_an_error() {
    // The first security event a user ever triggers has to work.
    let fixture = fixture();
    let sealer = sealer();
    let log = AuditLog::open(fixture.log.clone(), Some(&sealer)).expect("open a missing history");
    assert!(log.records().is_empty());
    assert_eq!(log.next_sequence(), 1);
}
