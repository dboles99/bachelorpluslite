//! Where the security history lives, and how the shell reads it back.
//!
//! `bp-audit` owns everything about *what* a security history is -- the
//! records, their order, the sealing, and what each profile permits writing
//! down (ADR-0024). This module owns only the two questions the crate
//! deliberately refuses to answer, because both are the shell's: which file
//! on this machine it is, and what a person reading it sees.
//!
//! It does **not** decide whether an event may be written. That is
//! `Destination::for_policy`, and duplicating the judgement here is how the
//! menu and the log come to disagree about what a profile means.

use std::path::{Path, PathBuf};

use bp_audit::{AuditLog, Record};

/// The file the security history is appended to.
///
/// Beside the recovery journal rather than beside `config.toml`, and for the
/// same reason: it is machine-specific state rather than settings a user
/// might copy between machines. `bp-platform` calls this `DirKind::State`,
/// and both this and `recovery_dir` still hang off the config directory --
/// which on Windows roams. That is one change owning `bp-config` and
/// `bp-ui`, recorded in `project/WORK_QUEUE.md`, and it moves both together
/// or neither.
#[cfg(not(test))]
pub(crate) fn audit_path() -> PathBuf {
    crate::state::recovery_dir().with_file_name("security-history.log")
}

/// The same, redirected and made unique under test.
///
/// The one `cfg(test)` in this crate, and both halves of it were learned the
/// hard way. Recording happens inside `set_security`, `set_privacy`,
/// `scan_for_secrets` and two more, so **every** test that touches a security
/// operation appends to whatever this returns:
///
/// * pointed at the real config directory, `cargo test` wrote a live security
///   history into the user's `%APPDATA%` -- which it did, once, before this
///   existed;
/// * pointed at one shared temp file, parallel tests each read the log, each
///   computed the same next sequence number, and the log then refused to open
///   as out of order -- so every one of them failed, on a file none of them
///   was really testing.
///
/// A counter rather than a random name, so a failing run names a file that
/// can be looked at, and the process id so two runs at once cannot collide.
#[cfg(test)]
pub(crate) fn audit_path() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "bpad-test-security-history-{}-{n}.log",
        std::process::id()
    ))
}

/// The report shown by Security ▸ Security History.
///
/// Oldest first, which is the order the file is in and the order an
/// investigation reads in: the question asked of a history is almost always
/// "what led to this", not "what happened last".
///
/// The location is on the report because a history the user cannot find is a
/// history they cannot keep, hand to somebody, or delete.
pub(crate) fn history_report(log: &AuditLog) -> String {
    let records = log.records();
    if records.is_empty() {
        return empty_report(log.location());
    }

    let mut out = String::with_capacity(records.len() * 64);
    out.push_str(&format!(
        "{} event{} recorded.\n\n",
        records.len(),
        if records.len() == 1 { "" } else { "s" }
    ));
    for record in records {
        out.push_str(&line_for(record));
        out.push('\n');
    }
    out.push_str(&format!("\nKept in {}", log.location().display()));
    out
}

/// One line per record: when, then what.
///
/// The timestamp leads because the lines are read as a sequence, and a column
/// that starts every line is the one an eye can scan down. The wording of the
/// event itself is `bp-audit`'s -- `Event::describe` composes it there so it
/// is testable without a window and so there is one place to check that no
/// sentence ever grew a field it should not carry.
fn line_for(record: &Record) -> String {
    format!(
        "{:>4}.  {}  {}",
        record.seq(),
        record.at().date(),
        record.event().describe()
    )
}

/// What an empty history says.
///
/// Deliberately not "no events": nothing having happened and nothing having
/// been *recorded* are different statements, and under a profile that keeps
/// the history for the session only the second is the true one. Naming the
/// file makes which one this is checkable.
pub(crate) fn empty_report(path: &Path) -> String {
    format!(
        "No security events have been recorded.\n\n\
         A history is written when a document is encrypted or unlocked, when a \
         security profile changes, when Privacy Mode is switched on or off, and \
         when a scan, redaction or signature check finishes -- and only where \
         the document's profile permits keeping it.\n\n\
         It would be kept in {}",
        path.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use bp_audit::{Event, ProfileLabel};
    use time::macros::datetime;

    /// A log holding `events`, built the only way a caller outside `bp-audit`
    /// can build one -- by appending. `Record` has no public constructor on
    /// purpose: a record you can only get from a log is a record whose
    /// sequence number means something.
    fn a_log_of(dir: &tempfile::TempDir, events: &[Event]) -> AuditLog {
        let path = dir.path().join("security-history.log");
        let mut log = AuditLog::open(path, None).expect("a missing file is an empty log");
        for &event in events {
            log.append(
                datetime!(2026-08-20 09:30:00 UTC),
                None,
                event,
                bp_security::Security::default().policy(),
                None,
            )
            .expect("a default profile keeps a plaintext history");
        }
        log
    }

    #[test]
    fn the_history_never_lands_in_the_config_directory_during_a_test() {
        // The protection itself, pinned. Recording happens inside five
        // ordinary state operations, so a test that merely changes a security
        // profile appends -- and pointed at the real path, `cargo test` wrote
        // a live security history into the user's `%APPDATA%`. It did, once.
        let path = audit_path();
        assert!(
            path.starts_with(std::env::temp_dir()),
            "the test history must live in the temp directory, not {}",
            path.display()
        );
    }

    #[test]
    fn a_line_leads_with_the_sequence_and_the_date() {
        let dir = tempfile::tempdir().expect("temp dir");
        let log = a_log_of(&dir, &[Event::DocumentEncrypted]);
        let line = line_for(&log.records()[0]);
        assert!(line.starts_with("   1.  2026-08-20"), "got {line:?}");
        assert!(line.ends_with("document encrypted"), "got {line:?}");
    }

    #[test]
    fn the_wording_of_an_event_comes_from_the_crate_and_not_from_here() {
        // If this ever fails because the sentence changed, the fix is in
        // `bp-audit`. A second spelling of the same event in the shell is how
        // a history and the notice that produced it start disagreeing.
        let events = [
            Event::DocumentUnlocked,
            Event::UnlockFailed { attempt: 3 },
            Event::SecretScanFinished { findings: 2 },
            Event::SignatureVerified { valid: false },
        ];
        let dir = tempfile::tempdir().expect("temp dir");
        let log = a_log_of(&dir, &events);
        for (record, event) in log.records().iter().zip(events) {
            assert!(
                line_for(record).ends_with(&event.describe()),
                "the shell is rewording {event:?}"
            );
        }
    }

    #[test]
    fn an_empty_history_says_nothing_was_recorded_rather_than_nothing_happened() {
        let report = empty_report(Path::new("/tmp/security-history.log"));
        assert!(
            report.contains("recorded"),
            "an empty history must not claim nothing happened: {report}"
        );
        assert!(
            report.contains("security-history.log"),
            "a history the user cannot find is one they cannot delete"
        );
    }

    #[test]
    fn a_report_counts_its_events_and_says_where_they_are() {
        let dir = tempfile::tempdir().expect("temp dir");
        let log = a_log_of(
            &dir,
            &[
                Event::PrivacyModeEntered,
                Event::SecurityProfileChanged {
                    from: ProfileLabel::Standard,
                    to: ProfileLabel::Private,
                },
            ],
        );

        let report = history_report(&log);
        assert!(report.starts_with("2 events recorded."), "got {report}");
        // Asked of the crate rather than spelled here, for the reason the
        // test above gives: a second spelling in the shell is how the report
        // and the notice that produced it start disagreeing.
        assert!(
            report.contains(&Event::PrivacyModeEntered.describe()),
            "got {report}"
        );
        assert!(report.contains("security-history.log"), "got {report}");
    }

    #[test]
    fn one_event_is_not_reported_as_one_events() {
        let dir = tempfile::tempdir().expect("temp dir");
        let log = a_log_of(&dir, &[Event::PrivacyModeLeft]);
        assert!(history_report(&log).starts_with("1 event recorded."));
    }

    #[test]
    fn a_history_with_nothing_in_it_reports_as_empty_rather_than_as_a_list_of_none() {
        let dir = tempfile::tempdir().expect("temp dir");
        let log = a_log_of(&dir, &[]);
        assert_eq!(history_report(&log), empty_report(log.location()));
    }
}
