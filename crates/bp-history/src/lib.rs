//! Crash-safe recovery journal.
//!
//! specs.md section 6: an edit journal and session recovery, so that work
//! survives a crash or a power cut between saves.
//!
//! **This writes document content to disk.** That is the entire point, and it
//! is also the thing to be careful about: a recovery journal is a copy of
//! your unsaved work sitting outside the file you were editing.
//!
//! The document's security profile governs it (ADR-0011, ADR-0020), and
//! `checkpoint` takes that policy rather than assuming one. Two things follow
//! that are easy to miss:
//!
//! * **A refusal deletes what is already there.** Tightening a profile has to
//!   remove what the looser one wrote, or the journal would report itself as
//!   protected while this morning's plaintext sat beside it.
//! * **A profile wanting encryption is refused, not downgraded.** `bp-crypto`
//!   arrives in phase 15; until then the honest answer is "not journalled",
//!   and `Refusal::notice` is how the user hears it.
//!
//! Under the default profile the journal is plaintext in the user's own
//! config directory, and `Journal::location` exists so the UI can say where.
//!
//! Two properties matter more than speed here:
//!
//! * A checkpoint is written atomically, so a crash *during* a checkpoint
//!   cannot corrupt the previous one.
//! * A journal is discarded only after a successful save. Deleting it any
//!   earlier would open a window in which neither the file nor the journal
//!   holds the work.

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

use bp_security::Recovery;
use serde::{Deserialize, Serialize};

/// Crate identity used by workspace smoke tests and diagnostics.
pub const CRATE_NAME: &str = "bp-history";

/// One document's unsaved state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkpoint {
    /// The file this belonged to, if it had one. `None` for an unsaved
    /// document that never had a name.
    pub path: Option<PathBuf>,
    /// A label for the recovery prompt, so the user is not asked about
    /// "document 3".
    pub name: String,
    pub text: String,
    /// Seconds since the Unix epoch. A plain integer rather than a timestamp
    /// type so the journal format does not depend on a crate's serialisation.
    pub written_at: u64,
}

/// What a checkpoint attempt did.
///
/// Three outcomes rather than a `bool`, because "not written" has two very
/// different meanings to the user: a profile that forbids a journal is
/// working as asked, and one that wants encryption we cannot yet provide is
/// a capability gap worth saying out loud.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Written {
    Yes,
    Refused(Refusal),
}

/// Why a checkpoint was not written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The document's profile disables the recovery journal outright.
    ProfileForbidsIt,
    /// The profile requires an encrypted journal, and `bp-crypto` does not
    /// exist yet (phase 15).
    EncryptionUnavailable,
}

impl Refusal {
    /// What to tell the user, if anything.
    ///
    /// A profile doing what it was set to do is not news. A profile that
    /// cannot be honoured is, and staying silent about it would leave
    /// somebody believing their work is being journalled when it is not.
    #[must_use]
    pub const fn notice(self) -> Option<&'static str> {
        match self {
            Self::ProfileForbidsIt => None,
            Self::EncryptionUnavailable => Some(
                "this profile needs an encrypted recovery journal (phase 15); \
                 unsaved work is not being journalled",
            ),
        }
    }
}

/// Where checkpoints are kept.
#[derive(Debug, Clone)]
pub struct Journal {
    dir: PathBuf,
}

impl Journal {
    /// Use `dir` for checkpoints.
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// The directory checkpoints are written to, for showing the user.
    pub fn location(&self) -> &Path {
        &self.dir
    }

    fn file_for(&self, id: u64) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }

    /// Write a checkpoint for `id` if the document's policy permits it.
    ///
    /// Atomic: written to a temporary file and renamed, so a crash midway
    /// leaves the *previous* checkpoint intact rather than a truncated one.
    /// A journal that can be half-written is worse than no journal, because
    /// it looks recoverable.
    ///
    /// **A refusal deletes any checkpoint already on disk for `id`.** That is
    /// the whole point of asking: tightening a document's profile has to
    /// remove what the looser one wrote, or moving to Confidential would
    /// leave this morning's plaintext sitting in the recovery directory and
    /// report itself as protected.
    ///
    /// [`Recovery::Encrypted`] is refused rather than downgraded. `bp-crypto`
    /// arrives in phase 15; until then a profile asking for encryption cannot
    /// be honoured, and ADR-0020 requires that to fail visibly. Writing
    /// plaintext for a user who has been told it is encrypted is worse than
    /// keeping no journal at all.
    pub fn checkpoint(
        &self,
        id: u64,
        checkpoint: &Checkpoint,
        recovery: Recovery,
    ) -> std::io::Result<Written> {
        match recovery {
            Recovery::Plaintext => {}
            Recovery::Disabled => {
                self.discard(id)?;
                return Ok(Written::Refused(Refusal::ProfileForbidsIt));
            }
            Recovery::Encrypted => {
                self.discard(id)?;
                return Ok(Written::Refused(Refusal::EncryptionUnavailable));
            }
        }
        std::fs::create_dir_all(&self.dir)?;
        let json = serde_json::to_vec_pretty(checkpoint)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let target = self.file_for(id);
        let temp = target.with_extension("json.tmp");
        std::fs::write(&temp, &json)?;
        std::fs::rename(&temp, &target)?;
        Ok(Written::Yes)
    }

    /// Forget the checkpoint for `id`.
    ///
    /// Call this only after a successful save. Earlier, and there is a window
    /// where neither the file nor the journal holds the work.
    pub fn discard(&self, id: u64) -> std::io::Result<()> {
        match std::fs::remove_file(self.file_for(id)) {
            Ok(()) => Ok(()),
            // Already gone is the desired state, not a failure.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Every checkpoint currently on disk, newest first.
    ///
    /// Unreadable or corrupt entries are skipped rather than failing the
    /// whole scan: one bad file must not cost the user the other recoveries.
    pub fn pending(&self) -> Vec<(u64, Checkpoint)> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };

        let mut found: Vec<(u64, Checkpoint)> = entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let path = entry.path();
                if path.extension()? != "json" {
                    return None;
                }
                let id: u64 = path.file_stem()?.to_str()?.parse().ok()?;
                let text = std::fs::read_to_string(&path).ok()?;
                let checkpoint: Checkpoint = serde_json::from_str(&text).ok()?;
                Some((id, checkpoint))
            })
            .collect();

        // Newest first: `Reverse` rather than a hand-rolled comparator, which
        // clippy rightly points out is the same thing with more rope.
        found.sort_by_key(|(_, c)| std::cmp::Reverse(c.written_at));
        found
    }

    /// Remove every checkpoint. Used when the user declines recovery.
    pub fn discard_all(&self) -> std::io::Result<()> {
        for (id, _) in self.pending() {
            self.discard(id)?;
        }
        Ok(())
    }

    /// Delete stray temporary files left by a crash mid-checkpoint.
    pub fn clean_temporaries(&self) {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "tmp") {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}

/// Seconds since the Unix epoch, saturating at 0 if the clock is before it.
pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn journal() -> (TempDir, Journal) {
        let dir = TempDir::new().unwrap();
        let journal = Journal::new(dir.path().join("recovery"));
        (dir, journal)
    }

    fn checkpoint(text: &str) -> Checkpoint {
        Checkpoint {
            path: Some(PathBuf::from("/notes/a.txt")),
            name: "a.txt".to_owned(),
            text: text.to_owned(),
            written_at: now_unix(),
        }
    }

    #[test]
    fn a_checkpoint_round_trips() {
        let (_dir, journal) = journal();
        let entry = checkpoint("unsaved work");
        journal.checkpoint(1, &entry, Recovery::Plaintext).unwrap();

        let pending = journal.pending();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].0, 1);
        assert_eq!(pending[0].1, entry);
    }

    #[test]
    fn checkpointing_twice_replaces_rather_than_accumulates() {
        let (_dir, journal) = journal();
        journal
            .checkpoint(1, &checkpoint("first"), Recovery::Plaintext)
            .unwrap();
        journal
            .checkpoint(1, &checkpoint("second"), Recovery::Plaintext)
            .unwrap();

        let pending = journal.pending();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].1.text, "second");
    }

    #[test]
    fn discarding_is_idempotent() {
        let (_dir, journal) = journal();
        journal
            .checkpoint(1, &checkpoint("work"), Recovery::Plaintext)
            .unwrap();

        journal.discard(1).unwrap();
        assert!(journal.pending().is_empty());
        // Already gone is the desired state, not an error.
        journal.discard(1).unwrap();
    }

    #[test]
    fn pending_is_newest_first() {
        let (_dir, journal) = journal();
        let mut old = checkpoint("old");
        old.written_at = 1_000;
        let mut new = checkpoint("new");
        new.written_at = 2_000;

        journal.checkpoint(1, &old, Recovery::Plaintext).unwrap();
        journal.checkpoint(2, &new, Recovery::Plaintext).unwrap();

        let pending = journal.pending();
        assert_eq!(pending[0].1.text, "new");
        assert_eq!(pending[1].1.text, "old");
    }

    #[test]
    fn a_corrupt_entry_does_not_cost_the_others() {
        // One bad file must not lose the user their other recoveries.
        let (_dir, journal) = journal();
        journal
            .checkpoint(1, &checkpoint("good"), Recovery::Plaintext)
            .unwrap();
        std::fs::write(journal.location().join("2.json"), "{ not json").unwrap();
        std::fs::write(journal.location().join("notanumber.json"), "{}").unwrap();

        let pending = journal.pending();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].1.text, "good");
    }

    #[test]
    fn a_missing_directory_is_simply_nothing_to_recover() {
        let dir = TempDir::new().unwrap();
        let journal = Journal::new(dir.path().join("never-created"));
        assert!(journal.pending().is_empty());
        // And must not panic or create anything.
        assert!(!journal.location().exists());
    }

    #[test]
    fn checkpointing_creates_the_directory() {
        let (_dir, journal) = journal();
        assert!(!journal.location().exists());
        journal
            .checkpoint(1, &checkpoint("work"), Recovery::Plaintext)
            .unwrap();
        assert!(journal.location().exists());
    }

    #[test]
    fn a_crash_mid_checkpoint_leaves_the_previous_one_readable() {
        // The reason checkpoints are written to a temp file and renamed.
        let (_dir, journal) = journal();
        journal
            .checkpoint(1, &checkpoint("survivable"), Recovery::Plaintext)
            .unwrap();

        // Simulate the debris of an interrupted write.
        std::fs::write(journal.location().join("1.json.tmp"), "{ half writ").unwrap();

        let pending = journal.pending();
        assert_eq!(
            pending.len(),
            1,
            "the .tmp file must not be read as a checkpoint"
        );
        assert_eq!(pending[0].1.text, "survivable");

        journal.clean_temporaries();
        assert!(!journal.location().join("1.json.tmp").exists());
    }

    #[test]
    fn discard_all_clears_everything() {
        let (_dir, journal) = journal();
        journal
            .checkpoint(1, &checkpoint("a"), Recovery::Plaintext)
            .unwrap();
        journal
            .checkpoint(2, &checkpoint("b"), Recovery::Plaintext)
            .unwrap();

        journal.discard_all().unwrap();
        assert!(journal.pending().is_empty());
    }

    #[test]
    fn an_unnamed_document_can_still_be_recovered() {
        let (_dir, journal) = journal();
        let entry = Checkpoint {
            path: None,
            name: "Untitled".to_owned(),
            text: "notes with no file yet".to_owned(),
            written_at: now_unix(),
        };
        journal.checkpoint(7, &entry, Recovery::Plaintext).unwrap();

        let pending = journal.pending();
        assert_eq!(pending[0].1.path, None);
        assert_eq!(pending[0].1.text, "notes with no file yet");
    }

    // --- security policy --------------------------------------------------

    #[test]
    fn a_profile_that_disables_recovery_writes_nothing() {
        let (_dir, journal) = journal();

        let written = journal
            .checkpoint(1, &checkpoint("secret"), Recovery::Disabled)
            .unwrap();

        assert_eq!(written, Written::Refused(Refusal::ProfileForbidsIt));
        assert!(journal.pending().is_empty());
    }

    #[test]
    fn tightening_a_profile_deletes_what_the_looser_one_already_wrote() {
        // The whole point of asking the policy. Without this, moving a
        // document to Confidential leaves this morning's plaintext sitting in
        // the recovery directory while the profile reports it as protected --
        // which is worse than never having had a profile, because the user
        // has been told they are covered.
        let (_dir, journal) = journal();
        journal
            .checkpoint(
                1,
                &checkpoint("written while Standard"),
                Recovery::Plaintext,
            )
            .unwrap();
        assert_eq!(journal.pending().len(), 1, "the plaintext is on disk");

        journal
            .checkpoint(1, &checkpoint("now Confidential"), Recovery::Encrypted)
            .unwrap();

        assert!(
            journal.pending().is_empty(),
            "tightening the profile must remove the earlier plaintext, not \
             merely stop adding to it"
        );
    }

    #[test]
    fn a_profile_wanting_encryption_refuses_rather_than_writing_plaintext() {
        // `bp-crypto` arrives in phase 15. Until then the honest answer is
        // "not journalled", not a plaintext file under a profile that says
        // encrypted. ADR-0020 requires the failure to be visible.
        let (_dir, journal) = journal();

        let written = journal
            .checkpoint(1, &checkpoint("sensitive"), Recovery::Encrypted)
            .unwrap();

        assert_eq!(written, Written::Refused(Refusal::EncryptionUnavailable));
        assert!(journal.pending().is_empty());
        assert!(
            matches!(written, Written::Refused(r) if r.notice().is_some()),
            "a capability gap has to reach the user; a profile merely doing \
             what it was set to do does not"
        );
    }

    #[test]
    fn a_disabled_journal_is_silent_but_a_missing_capability_is_not() {
        // The distinction the two refusals exist for.
        assert_eq!(Refusal::ProfileForbidsIt.notice(), None);
        assert!(Refusal::EncryptionUnavailable.notice().is_some());
    }

    #[test]
    fn refusing_a_document_leaves_other_documents_journals_alone() {
        // One document going Confidential must not lose another's unsaved
        // work. The refusal path calls `discard`, and discarding the wrong id
        // would be a data-loss bug wearing a security fix.
        let (_dir, journal) = journal();
        journal
            .checkpoint(1, &checkpoint("keep me"), Recovery::Plaintext)
            .unwrap();

        journal
            .checkpoint(2, &checkpoint("forget me"), Recovery::Disabled)
            .unwrap();

        let pending = journal.pending();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].0, 1);
        assert_eq!(pending[0].1.text, "keep me");
    }
}
