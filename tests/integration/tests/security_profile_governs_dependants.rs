//! Seam: `bp-security`'s `Policy` against the two things it still governs --
//! `bp-history`'s recovery journal and `bp-storage`'s `record_document`.
//!
//! **It was three until ADR-0061**, which removed `bp-clipboard` and the
//! `Clipboard` axis with it. An axis whose subject has left governs nothing,
//! and a test asserting that it does would pass while meaning nothing.
//!
//! `bp-security` decides and performs nothing; the other two perform and
//! decide nothing. Every one of them is tested alone, which means the join --
//! *does the decision actually reach the thing that acts on it* -- is tested
//! nowhere. ADR-0020's own consequence section is explicit that the failure
//! mode is a control that quietly weakens itself, and a control that is never
//! wired up is the same failure with fewer steps.
//!
//! **These tests assert absence.** A test that a permissive profile works is
//! nearly worthless here: the leak is at the strict end. So for every profile
//! and under Privacy Mode, a sentinel string is pushed through each dependant
//! and then hunted for -- in every byte of every file the dependant wrote,
//! and in the in-memory structure it kept. The permissive cases are asserted
//! too, but only so that a dependant cannot pass by doing nothing at all.

mod common;

use common::no_files;

use std::path::{Path, PathBuf};

use bp_history::{Checkpoint, Journal, Refusal, Written};
use bp_security::{Metadata, Policy, Privacy, Profile, Recovery, Security};
use bp_storage::Store;
use proptest::prelude::*;
use tempfile::TempDir;

/// A string that appears nowhere else, so finding it in a file is proof of a
/// leak rather than a coincidence. Not a credential, and not shaped like one.
const SENTINEL: &str = "sentinel-b41d7e0a-not-a-secret";

/// A distinct sentinel for the extracted title, so a `PathOnly` store that
/// leaked the title but not the body is still caught.
const TITLE_SENTINEL: &str = "title-9c3f8b12-not-a-secret";

/// Every byte of every file under `dir`, concatenated.
///
/// Recursive, and it reads *everything*: a leak into a stray temporary file
/// or a write-ahead log is still a leak, and both are exactly where one would
/// hide from a test that only looked at the file it expected.
fn all_bytes_under(dir: &Path) -> Vec<u8> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            out.extend(all_bytes_under(&path));
        } else if let Ok(bytes) = std::fs::read(&path) {
            out.extend(bytes);
        }
    }
    out
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    let needle = needle.as_bytes();
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// The policy actually in force for a named profile under a privacy mode.
fn policy_of(profile: Profile, privacy: Privacy) -> Policy {
    Security::Named(profile).policy_under(privacy)
}

fn every_case() -> impl Iterator<Item = (Profile, Privacy)> {
    Profile::all()
        .iter()
        .copied()
        .flat_map(|p| [(p, Privacy::Off), (p, Privacy::On)])
}

// --- the journal ---------------------------------------------------------

struct JournalFixture {
    _dir: TempDir,
    journal: Journal,
    document: PathBuf,
}

fn journal_fixture() -> JournalFixture {
    let dir = tempfile::tempdir().expect("temp dir");
    let journal = Journal::new(dir.path().join("journal"));
    let document = dir.path().join("documents").join("note.txt");
    JournalFixture {
        _dir: dir,
        journal,
        document,
    }
}

fn checkpoint_for(path: Option<&Path>) -> Checkpoint {
    Checkpoint {
        path: path.map(Path::to_path_buf),
        name: "note.txt".to_owned(),
        text: format!("unsaved work\n{SENTINEL}\n"),
        written_at: 1_700_000_000,
        encoding: bp_history::CheckpointEncoding::Utf8,
        line_ending: Some(bp_history::CheckpointLineEnding::Lf),
    }
}

#[test]
fn the_journal_writes_plaintext_only_where_the_policy_says_plaintext() {
    for (profile, privacy) in every_case() {
        let policy = policy_of(profile, privacy);
        let fixture = journal_fixture();
        let checkpoint = checkpoint_for(Some(&fixture.document));

        let written = fixture
            .journal
            .checkpoint(1, &checkpoint, policy.recovery)
            .expect("checkpoint");

        let on_disk = all_bytes_under(fixture.journal.location());
        let case = format!("{} under privacy {:?}", profile.name(), privacy);

        match policy.recovery {
            Recovery::Plaintext => {
                assert_eq!(written, Written::Yes, "{case}: no journal was written");
                assert!(
                    contains(&on_disk, SENTINEL),
                    "{case}: a plaintext journal that does not hold the work is not a journal"
                );
                assert_eq!(fixture.journal.pending().len(), 1, "{case}");
            }
            Recovery::Disabled => {
                assert_eq!(
                    written,
                    Written::Refused(Refusal::ProfileForbidsIt),
                    "{case}"
                );
                assert!(
                    !contains(&on_disk, SENTINEL),
                    "{case}: a disabled journal wrote the document to disk"
                );
                assert!(fixture.journal.pending().is_empty(), "{case}");
            }
        }
    }
}

#[test]
fn under_privacy_mode_no_profile_journals_anything() {
    // The one assertion Privacy Mode exists to earn. Every profile, including
    // the one whose own policy is plaintext.

    for profile in Profile::all().iter().copied() {
        let policy = policy_of(profile, Privacy::On);
        let fixture = journal_fixture();
        let checkpoint = checkpoint_for(Some(&fixture.document));

        let written = fixture
            .journal
            .checkpoint(1, &checkpoint, policy.recovery)
            .expect("checkpoint");

        assert_eq!(
            written,
            Written::Refused(Refusal::ProfileForbidsIt),
            "{} journalled under Privacy Mode",
            profile.name()
        );
        assert!(
            !contains(&all_bytes_under(fixture.journal.location()), SENTINEL),
            "{} left unsaved work on disk under Privacy Mode",
            profile.name()
        );
    }
}

#[test]
fn tightening_a_profile_removes_what_the_looser_one_wrote() {
    // The failure this prevents is the worst kind: the journal reports itself
    // as protected while this morning's plaintext sits beside it.

    for (profile, privacy) in every_case() {
        let policy = policy_of(profile, privacy);
        if policy.recovery == Recovery::Plaintext {
            continue;
        }
        let fixture = journal_fixture();
        let checkpoint = checkpoint_for(Some(&fixture.document));

        // Under Standard first, which writes it in clear.
        fixture
            .journal
            .checkpoint(1, &checkpoint, Recovery::Plaintext)
            .expect("checkpoint");
        assert!(contains(
            &all_bytes_under(fixture.journal.location()),
            SENTINEL
        ));

        // Then the stricter profile.
        fixture
            .journal
            .checkpoint(1, &checkpoint, policy.recovery)
            .expect("checkpoint");

        assert!(
            !contains(&all_bytes_under(fixture.journal.location()), SENTINEL),
            "{} under privacy {:?} left the earlier plaintext journal behind",
            profile.name(),
            privacy
        );
    }
}

// --- the metadata store --------------------------------------------------

#[test]
fn the_store_records_exactly_what_the_policy_permits_and_nothing_more() {
    for (profile, privacy) in every_case() {
        let policy = policy_of(profile, privacy);
        let case = format!("{} under privacy {:?}", profile.name(), privacy);

        let dir = tempfile::tempdir().expect("temp dir");
        let db = dir.path().join("metadata.sqlite3");
        let document = dir.path().join(format!("{SENTINEL}.txt"));

        {
            let store = Store::open(&db).expect("open store");
            let id = store
                .record_document(
                    &document,
                    Some(TITLE_SENTINEL),
                    1_700_000_000,
                    policy.metadata,
                )
                .expect("record");

            match policy.metadata {
                Metadata::Summary => {
                    assert!(id.is_some(), "{case}: nothing was recorded");
                    let record = store.document(&document).expect("read").expect("a row");
                    assert_eq!(record.title.as_deref(), Some(TITLE_SENTINEL), "{case}");
                }
                Metadata::PathOnly => {
                    assert!(id.is_some(), "{case}: the path was not recorded");
                    let record = store.document(&document).expect("read").expect("a row");
                    assert_eq!(
                        record.title, None,
                        "{case}: the extracted title was recorded anyway"
                    );
                }
                Metadata::Disabled => {
                    assert!(id.is_none(), "{case}: a disabled store recorded something");
                    assert!(
                        store.document(&document).expect("read").is_none(),
                        "{case}: a row exists for a document that must not be recorded"
                    );
                    assert!(
                        store.recent_documents(10).expect("read").is_empty(),
                        "{case}"
                    );
                }
            }
        }
        // Dropped, so SQLite has checkpointed its write-ahead log: the bytes
        // below are everything the store put on this disk.

        let on_disk = all_bytes_under(dir.path());
        assert_eq!(
            contains(&on_disk, TITLE_SENTINEL),
            policy.metadata == Metadata::Summary,
            "{case}: the title's presence on disk disagrees with the policy"
        );
        if policy.metadata == Metadata::Disabled {
            assert!(
                !contains(&on_disk, SENTINEL),
                "{case}: a disabled store left the document's path on disk"
            );
        }
    }
}

// --- the model itself, across every policy a document can carry ----------

fn any_policy() -> impl Strategy<Value = Policy> {
    (
        prop_oneof![Just(Recovery::Plaintext), Just(Recovery::Disabled)],
        prop_oneof![
            Just(Metadata::Summary),
            Just(Metadata::PathOnly),
            Just(Metadata::Disabled)
        ],
    )
        .prop_map(|(recovery, metadata)| Policy { recovery, metadata })
}

proptest! {
    #![proptest_config(no_files(64))]

    /// Privacy Mode reaches the dependants, not merely the `Policy` type.
    ///
    /// For *any* policy a document could carry -- named or `Custom` -- turning
    /// Privacy Mode on must leave nothing behind in any of the three. Asserted
    /// against the dependants rather than against `Privacy::clamp`, because
    /// `bp-security` already proves the clamp and what has never been checked
    /// is whether the clamped value is the one that gets used.
    #[test]
    fn privacy_mode_silences_all_three_dependants(policy in any_policy()) {
        let in_force = Security::Custom(policy).policy_under(Privacy::On);

        let fixture = journal_fixture();
        let checkpoint = checkpoint_for(Some(&fixture.document));
        let written = fixture
            .journal
            .checkpoint(1, &checkpoint, in_force.recovery)
            .expect("checkpoint");
        prop_assert_eq!(written, Written::Refused(Refusal::ProfileForbidsIt));
        prop_assert!(!contains(&all_bytes_under(fixture.journal.location()), SENTINEL));

        let store = Store::in_memory().expect("store");
        let document = PathBuf::from(format!("/tmp/{SENTINEL}.txt"));
        prop_assert!(
            store
                .record_document(&document, Some(TITLE_SENTINEL), 1, in_force.metadata)
                .expect("record")
                .is_none()
        );
    }

    /// Leaving Privacy Mode restores what the document itself asked for,
    /// rather than the default. Privacy Mode is an override, not a rewrite.
    #[test]
    fn leaving_privacy_mode_restores_the_documents_own_policy(policy in any_policy()) {
        let security = Security::Custom(policy);
        prop_assert_eq!(security.policy_under(Privacy::Off), policy);
        prop_assert!(
            security
                .policy_under(Privacy::On)
                .is_at_least_as_strict_as(&security.policy_under(Privacy::Off))
        );
    }
}
