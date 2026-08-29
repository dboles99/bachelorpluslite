//! Seam: `bp-history` writing a recovery journal and a later session reading
//! it back.
//!
//! **It covered the sealed form too until ADR-0064**, which removed it with
//! `bp-crypto`. What remains is the path every document now takes, and it is
//! the path that always mattered most: the plaintext journal is the only one
//! there has ever been for an ordinary document.
//!
//! The journal exists for exactly one moment -- the power cut -- and that
//! moment is the one nothing has ever rehearsed. `bp-history` is tested for
//! what it writes; nothing has taken a written journal, thrown the `Journal`
//! away as a crash would, and asked whether the document comes back. The
//! A checkpoint that cannot be read back is indistinguishable from no
//! checkpoint at all: the work is gone either way.
//!
//! So the property throughout is the only one that matters: **what comes back
//! is the document that was lost, byte for byte, and it is still a document
//! the rope will accept.**
//!
//! Document text is wrapped in `Doc` so a failing case reports a size rather
//! than someone's notes.

mod common;

use common::{Doc, no_files};

use std::path::{Path, PathBuf};

use bp_buffer::Buffer;
use bp_history::{Checkpoint, Journal, Written};
use bp_security::Recovery;
use proptest::prelude::*;
use tempfile::TempDir;

/// Text assembled from tokens that have broken a round trip before: both line
/// breaks, characters of two, three and four bytes, a quote and a backslash
/// (the journal is JSON), and a tab.
fn document() -> impl Strategy<Value = Doc> {
    proptest::collection::vec(
        proptest::sample::select(
            &[
                "a", "note ", "\n", "\r\n", "\r", "é", "中", "😀", "\t", "\"", "\\", "\u{0}",
            ][..],
        ),
        0..60,
    )
    .prop_map(|parts| Doc(parts.concat()))
}

struct Fixture {
    _dir: TempDir,
    dir: PathBuf,
    document: PathBuf,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().expect("temp dir");
    let journal_dir = dir.path().join("recovery");
    let document = dir.path().join("documents").join("note.txt");
    Fixture {
        _dir: dir,
        dir: journal_dir,
        document,
    }
}

impl Fixture {
    /// A journal over the same directory, as a fresh process would build.
    ///
    /// Recovery is not "ask the object that wrote it"; it is a new run of the
    /// program finding files it did not create. Reusing the `Journal` that
    /// wrote the checkpoint would let an in-memory cache pass this test.
    fn after_a_restart(&self) -> Journal {
        Journal::new(self.dir.clone())
    }
}

fn checkpoint(path: Option<&Path>, text: &str, written_at: u64) -> Checkpoint {
    Checkpoint {
        path: path.map(Path::to_path_buf),
        name: "note.txt".to_owned(),
        text: text.to_owned(),
        written_at,
        encoding: bp_history::CheckpointEncoding::Utf8,
        line_ending: Some(bp_history::CheckpointLineEnding::Lf),
    }
}

proptest! {
    #![proptest_config(no_files(128))]

    /// **A plaintext journal reproduces the document exactly**, across a
    /// restart, and the recovered text is still something the rope accepts
    /// unchanged -- which is what the shell does with it next.
    #[test]
    fn a_plaintext_journal_reproduces_the_document(doc in document()) {
        let fixture = fixture();
        let original = checkpoint(Some(&fixture.document), doc.as_str(), 1_700_000_000);

        let written = fixture
            .after_a_restart()
            .checkpoint(7, &original, Recovery::Plaintext)
            .expect("checkpoint");
        prop_assert_eq!(written, Written::Yes);

        let pending = fixture.after_a_restart().pending();
        prop_assert_eq!(pending.len(), 1);
        let (id, recovered) = &pending[0];
        prop_assert_eq!(*id, 7u64);
        prop_assert!(recovered == &original, "the recovered checkpoint differs from the one written");
        prop_assert!(
            Buffer::from_text(&recovered.text).to_string() == doc.0,
            "the recovered text changed on its way into the rope"
        );
    }

    /// A checkpoint written over an earlier one replaces it rather than
    /// accumulating, and the survivor is the newer text.
    #[test]
    fn a_later_checkpoint_replaces_the_earlier_one(first in document(), second in document()) {
        let fixture = fixture();
        let journal = fixture.after_a_restart();

        journal
            .checkpoint(1, &checkpoint(Some(&fixture.document), first.as_str(), 1), Recovery::Plaintext)
            .expect("first");
        journal
            .checkpoint(1, &checkpoint(Some(&fixture.document), second.as_str(), 2), Recovery::Plaintext)
            .expect("second");

        let pending = fixture.after_a_restart().pending();
        prop_assert_eq!(pending.len(), 1, "the journal accumulated instead of replacing");
        prop_assert!(pending[0].1.text == second.0);
    }
}

// --- named cases ----------------------------------------------------------

#[test]
fn the_empty_document_recovers_as_an_empty_document() {
    // An empty unsaved buffer is still work in progress -- the user deleted
    // everything and has not saved. Recovering it as "no journal" would put
    // the old contents back.
    let fixture = fixture();
    let original = checkpoint(Some(&fixture.document), "", 1);

    fixture
        .after_a_restart()
        .checkpoint(1, &original, Recovery::Plaintext)
        .expect("checkpoint");

    let pending = fixture.after_a_restart().pending();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].1.text, "");
}

#[test]
fn a_document_that_was_never_saved_still_journals_in_plaintext() {
    // `path: None` is the untitled buffer, which is precisely the document a
    // crash costs the most: there is no file to fall back to.
    let fixture = fixture();
    let original = checkpoint(None, "unsaved and unnamed\n", 1);

    let written = fixture
        .after_a_restart()
        .checkpoint(3, &original, Recovery::Plaintext)
        .expect("checkpoint");

    assert_eq!(written, Written::Yes);
    let pending = fixture.after_a_restart().pending();
    assert_eq!(pending.len(), 1);
    assert!(pending[0].1 == original);
}

#[test]
fn several_documents_recover_independently_and_newest_first() {
    let fixture = fixture();
    let journal = fixture.after_a_restart();

    for (id, at) in [(1u64, 100u64), (2, 300), (3, 200)] {
        journal
            .checkpoint(
                id,
                &checkpoint(Some(&fixture.document), &format!("document {id}\n"), at),
                Recovery::Plaintext,
            )
            .expect("checkpoint");
    }

    let pending = fixture.after_a_restart().pending();
    assert_eq!(pending.len(), 3);
    assert_eq!(
        pending.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        vec![2, 3, 1],
        "checkpoints must be offered newest first"
    );
}

#[test]
fn a_crash_midway_through_a_checkpoint_leaves_the_previous_one_recoverable() {
    // What a half-written checkpoint looks like on disk: the temporary file
    // exists and the rename never happened. The previous checkpoint must
    // still be the one offered, and the debris must be cleanable.
    let fixture = fixture();
    let journal = fixture.after_a_restart();
    let good = checkpoint(Some(&fixture.document), "the work that survived\n", 1);

    journal
        .checkpoint(1, &good, Recovery::Plaintext)
        .expect("checkpoint");
    std::fs::write(fixture.dir.join("1.json.tmp"), b"{ truncated").expect("debris");

    let journal = fixture.after_a_restart();
    let pending = journal.pending();
    assert_eq!(pending.len(), 1, "the debris was offered as a recovery");
    assert!(pending[0].1 == good);

    journal.clean_temporaries();
    assert!(
        !fixture.dir.join("1.json.tmp").exists(),
        "the debris was not cleaned up"
    );
    assert_eq!(fixture.after_a_restart().pending().len(), 1);
}

#[test]
fn an_unreadable_checkpoint_does_not_cost_the_user_the_others() {
    let fixture = fixture();
    let journal = fixture.after_a_restart();
    let good = checkpoint(Some(&fixture.document), "the work that survived\n", 1);

    journal
        .checkpoint(1, &good, Recovery::Plaintext)
        .expect("checkpoint");
    std::fs::write(fixture.dir.join("2.json"), b"not json at all").expect("corrupt");

    let pending = fixture.after_a_restart().pending();
    assert_eq!(pending.len(), 1);
    assert!(pending[0].1 == good);
}

#[test]
fn declining_recovery_removes_every_plaintext_checkpoint() {
    let fixture = fixture();
    let journal = fixture.after_a_restart();
    for id in 1..=3u64 {
        journal
            .checkpoint(
                id,
                &checkpoint(Some(&fixture.document), "work\n", id),
                Recovery::Plaintext,
            )
            .expect("checkpoint");
    }

    journal.discard_all().expect("discard all");

    assert!(fixture.after_a_restart().pending().is_empty());
}
