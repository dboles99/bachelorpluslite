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
//! * **The journal is plaintext or it does not exist.** ADR-0022 gave it a
//!   second form, sealed with the document's own passphrase and filed under a
//!   digest of its path; ADR-0064 removed that with `bp-crypto`. A profile
//!   that used to ask for an encrypted journal now asks for none at all --
//!   *no journal* rather than *a journal somebody was told was encrypted*,
//!   which is ADR-0020's rule about a control that quietly weakens itself.
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
//!
//! **Every run writes under its own session tag** (ADR-0084). A document id
//! is a counter that starts at 1 in every run, so a file named only by it
//! was shared by every run that ever had a document 1 -- and the clean,
//! empty *Untitled* each launch opens is document 1. Its checkpoint pass
//! discarded the previous run's `1.json` within seconds, before anybody had
//! answered whether to recover it. A run now discards only what it wrote,
//! and a previous run's work stays until somebody decides about it.

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

use bp_security::Recovery;
use serde::{Deserialize, Serialize};

/// Crate identity used by workspace smoke tests and diagnostics.
pub const CRATE_NAME: &str = "bp-history";

/// A document's encoding, as recorded in a [`Checkpoint`].
///
/// This mirrors `bp_core::Encoding` rather than reusing it, so that this
/// crate's on-disk journal format does not depend on `bp-core`'s type -- the
/// caller (`bp-ui`, which already depends on both) converts at the boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CheckpointEncoding {
    #[default]
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
}

/// A document's line-ending convention, as recorded in a [`Checkpoint`].
///
/// Mirrors `bp_core::LineEnding` for the same reason as [`CheckpointEncoding`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckpointLineEnding {
    Lf,
    CrLf,
}

/// One document's unsaved state.
///
/// `encoding` and `line_ending` were added after this format shipped, so both
/// are `#[serde(default)]`: a journal file written by an older build simply
/// lacks the keys, and must still deserialize rather than losing the
/// recovery entirely. See each field's doc comment for what an absent value
/// falls back to.
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
    /// The document's encoding at the moment of the checkpoint.
    ///
    /// Absent on a pre-upgrade journal, in which case there is genuinely no
    /// way to recover it: the checkpoint holds already-decoded text, and
    /// nothing about *that* says whether the file on disk was UTF-8,
    /// UTF-8-with-BOM, or UTF-16. A missing value falls back to
    /// [`CheckpointEncoding::default`] -- a guess, not a recovered fact --
    /// rather than pretending to know.
    #[serde(default)]
    pub encoding: CheckpointEncoding,
    /// The document's line-ending convention at the moment of the
    /// checkpoint.
    ///
    /// Absent on a pre-upgrade journal. Unlike `encoding`, the text itself
    /// carries evidence here, so the caller resolves a missing value at
    /// recovery time with `LineEnding::detect` rather than a blind platform
    /// default -- the same guess the rest of this codebase makes when
    /// certainty isn't available. `None` is also what a *freshly written*
    /// checkpoint carries for text with no line break at all, so this stays
    /// optional rather than eagerly resolving during deserialization.
    #[serde(default)]
    pub line_ending: Option<CheckpointLineEnding>,
}

/// What a checkpoint attempt did.
///
/// More than a `bool`, because "not written" has several meanings and only
/// some are worth telling the user about: a profile that forbids a journal is
/// working as asked, while one that cannot journal because the document has
/// no passphrase is something they can fix.
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
            // The only refusal left, and it is a profile doing exactly what
            // it was set to do. Two others were actionable and are gone with
            // the encrypted journal (ADR-0064); this `match` is kept rather
            // than collapsed to `None` because the *question* -- is this
            // refusal worth interrupting somebody about -- outlives the
            // answers, and a bare `None` would lose it.
            Self::ProfileForbidsIt => None,
        }
    }
}

/// Where checkpoints are kept.
#[derive(Debug, Clone)]
pub struct Journal {
    dir: PathBuf,
    session: String,
}

/// Which checkpoint on disk an entry is, so a decision about it can name it
/// rather than "everything".
///
/// Opaque on purpose: what matters is that forgetting an entry removes that
/// file and no other, not how the file is named.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    stem: String,
    id: u64,
    session: Option<String>,
}

impl Entry {
    /// The document id the checkpoint was written under, in the run that
    /// wrote it. Meaningless in any other run.
    #[must_use]
    pub const fn id(&self) -> u64 {
        self.id
    }

    /// Read a file stem: `<session>-<id>` from this build, or a bare `<id>`
    /// from a build before sessions existed, which must still be offered.
    fn parse(stem: &str) -> Option<Self> {
        let (session, id) = match stem.rsplit_once('-') {
            Some((session, id)) if !session.is_empty() => (Some(session.to_owned()), id),
            Some(_) => return None,
            None => (None, stem),
        };
        Some(Self {
            stem: stem.to_owned(),
            id: id.parse().ok()?,
            session,
        })
    }
}

/// A tag no earlier run can have had: the time, the process, and a counter
/// for journals made within one process.
///
/// Time and process id alone are not enough. A process id is reused by a
/// later run (trap 5 in `DECISIONS.md`), and on Windows the clock ticks in
/// 100 ns steps, so two journals made back to back in one process can read
/// the same instant.
fn new_session() -> String {
    static MADE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let count = MADE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("{nanos:x}.{:x}.{count:x}", std::process::id())
}

/// Whether any file in `dir` was written under `session`.
fn session_in_use(dir: &Path, session: &str) -> bool {
    let prefix = format!("{session}-");
    std::fs::read_dir(dir).is_ok_and(|entries| {
        entries
            .filter_map(Result::ok)
            .any(|entry| entry.file_name().to_string_lossy().starts_with(&prefix))
    })
}

/// How old a temporary file must be before it is debris rather than a
/// checkpoint being written. A write and its rename take milliseconds; a
/// minute is long enough that nothing live is that slow.
const STALE_TEMPORARY: std::time::Duration = std::time::Duration::from_secs(60);

impl Journal {
    /// Use `dir` for checkpoints, as a new session.
    ///
    /// A tag already on disk is refused and another drawn. Time, process and
    /// counter make a repeat all but impossible, and "all but" is not good
    /// enough when a repeat would have this run overwrite an earlier one's
    /// unsaved work and then treat it as its own.
    pub fn new(dir: PathBuf) -> Self {
        let mut session = new_session();
        while session_in_use(&dir, &session) {
            session = new_session();
        }
        Self { dir, session }
    }

    /// The directory checkpoints are written to, for showing the user.
    pub fn location(&self) -> &Path {
        &self.dir
    }

    fn file_for(&self, id: u64) -> PathBuf {
        self.dir.join(format!("{}-{id}.json", self.session))
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
    /// [`Recovery::Encrypted`] seals the checkpoint with `passphrase`, which
    /// is the document's own. Without one it is refused rather than
    /// downgraded: writing plaintext for a user who has been told it is
    /// encrypted is worse than keeping no journal at all (ADR-0020).
    ///
    /// **Two forms until ADR-0064**, which removed the sealed one with
    /// `bp-crypto`. Tightening a profile still removes the journal a looser
    /// one wrote, which is the half that survives and the half that matters:
    /// a document whose profile now forbids a journal must not leave one
    /// behind.
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

    /// Forget this session's checkpoint for `id`.
    ///
    /// Call this only after a successful save. Earlier, and there is a window
    /// where neither the file nor the journal holds the work.
    ///
    /// **Only this session's.** Another run's document with the same id is a
    /// different document.
    pub fn discard(&self, id: u64) -> std::io::Result<()> {
        match std::fs::remove_file(self.file_for(id)) {
            Ok(()) => Ok(()),
            // Already gone is the desired state, not a failure.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Every checkpoint currently on disk, this session's included, newest
    /// first.
    ///
    /// Unreadable or corrupt entries are skipped rather than failing the
    /// whole scan: one bad file must not cost the user the other recoveries.
    pub fn pending(&self) -> Vec<(Entry, Checkpoint)> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };

        let mut found: Vec<(Entry, Checkpoint)> = entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let path = entry.path();
                if path.extension()? != "json" {
                    return None;
                }
                let key = Entry::parse(path.file_stem()?.to_str()?)?;
                let text = std::fs::read_to_string(&path).ok()?;
                let checkpoint: Checkpoint = serde_json::from_str(&text).ok()?;
                Some((key, checkpoint))
            })
            .collect();

        // Newest first: `Reverse` rather than a hand-rolled comparator, which
        // clippy rightly points out is the same thing with more rope.
        found.sort_by_key(|(_, c)| std::cmp::Reverse(c.written_at));
        found
    }

    /// The checkpoints some other run left: what there is to offer to
    /// recover. This session's own are live, not left.
    pub fn left_behind(&self) -> Vec<(Entry, Checkpoint)> {
        let mut found = self.pending();
        found.retain(|(entry, _)| entry.session.as_deref() != Some(self.session.as_str()));
        found
    }

    /// Remove exactly these entries: the ones the user was asked about and
    /// declined, or recovered and has now checkpointed again under this
    /// session.
    ///
    /// Named entries rather than "all", because by the time the answer
    /// arrives this session may have checkpoints of its own, and they are not
    /// what was declined.
    pub fn forget(&self, entries: &[Entry]) -> std::io::Result<()> {
        for entry in entries {
            match std::fs::remove_file(self.dir.join(format!("{}.json", entry.stem))) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    /// Delete stray temporary files left by a crash mid-checkpoint.
    ///
    /// **Only stale ones.** The directory is shared by every run, and another
    /// instance may be between writing its temporary file and renaming it;
    /// deleting that would make its rename fail and cost it the checkpoint.
    /// Nothing reads a `.tmp`, so one left a little longer does no harm.
    pub fn clean_temporaries(&self) {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return;
        };
        let now = std::time::SystemTime::now();
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if !path.extension().is_some_and(|e| e == "tmp") {
                continue;
            }
            let stale = entry.metadata().and_then(|m| m.modified()).is_ok_and(|at| {
                now.duration_since(at)
                    .is_ok_and(|age| age >= STALE_TEMPORARY)
            });
            if stale {
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
            encoding: CheckpointEncoding::Utf8,
            line_ending: Some(CheckpointLineEnding::CrLf),
        }
    }

    #[test]
    fn a_checkpoint_round_trips() {
        let (_dir, journal) = journal();
        let entry = checkpoint("unsaved work");
        journal.checkpoint(1, &entry, Recovery::Plaintext).unwrap();

        let pending = journal.pending();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].0.id(), 1);
        assert_eq!(pending[0].1, entry);
    }

    #[test]
    fn a_checkpoints_encoding_and_line_ending_round_trip() {
        // Non-default values on purpose: a bug that always falls back to
        // `CheckpointEncoding::default()` would still pass a test that only
        // ever wrote the default.
        let (_dir, journal) = journal();
        let mut entry = checkpoint("unsaved work");
        entry.encoding = CheckpointEncoding::Utf16Le;
        entry.line_ending = Some(CheckpointLineEnding::Lf);

        journal.checkpoint(1, &entry, Recovery::Plaintext).unwrap();

        let pending = journal.pending();
        assert_eq!(pending[0].1.encoding, CheckpointEncoding::Utf16Le);
        assert_eq!(pending[0].1.line_ending, Some(CheckpointLineEnding::Lf));
    }

    #[test]
    fn a_pre_upgrade_journal_entry_without_encoding_or_line_ending_still_deserializes() {
        // Journals already on disk were written before these fields existed.
        // An old entry must still recover -- with the documented fallbacks --
        // rather than failing to deserialize and losing the recovery outright.
        let old_shape = r#"{
            "path": "/notes/a.txt",
            "name": "a.txt",
            "text": "work from before the upgrade",
            "written_at": 1000
        }"#;

        let entry: Checkpoint = serde_json::from_str(old_shape).unwrap();
        assert_eq!(entry.encoding, CheckpointEncoding::Utf8);
        assert_eq!(entry.line_ending, None);
        assert_eq!(entry.text, "work from before the upgrade");
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
        std::fs::write(journal.location().join("-3.json"), "{}").unwrap();

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

        // Simulate the debris of an interrupted write -- an old one, because
        // a fresh temporary file may be another instance's write in progress.
        let debris = journal.location().join("1.json.tmp");
        std::fs::write(&debris, "{ half writ").unwrap();
        age(&debris);

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
    fn forgetting_everything_offered_clears_what_the_last_run_left() {
        let (_dir, crashed) = journal();
        crashed
            .checkpoint(1, &checkpoint("a"), Recovery::Plaintext)
            .unwrap();
        crashed
            .checkpoint(2, &checkpoint("b"), Recovery::Plaintext)
            .unwrap();

        let relaunched = next_run(&crashed);
        let offered: Vec<Entry> = relaunched
            .left_behind()
            .into_iter()
            .map(|(e, _)| e)
            .collect();
        relaunched.forget(&offered).unwrap();
        assert!(relaunched.pending().is_empty());
    }

    #[test]
    fn an_unnamed_document_can_still_be_recovered() {
        let (_dir, journal) = journal();
        let entry = Checkpoint {
            path: None,
            name: "Untitled".to_owned(),
            text: "notes with no file yet".to_owned(),
            written_at: now_unix(),
            encoding: CheckpointEncoding::Utf8,
            line_ending: None,
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
            .checkpoint(1, &checkpoint("now Confidential"), Recovery::Disabled)
            .unwrap();

        assert!(
            journal.pending().is_empty(),
            "tightening the profile must remove the earlier plaintext, not \
             merely stop adding to it"
        );
    }

    #[test]
    fn a_profile_doing_what_it_was_set_to_do_is_not_news() {
        // The distinction the refusals exist for. There were three and there
        // is one: the two that carried an actionable notice were about the
        // encrypted journal and left under ADR-0064. The rule survives them
        // -- a refusal is worth interrupting somebody about only when they
        // can act on it -- and this holds the one case left.
        assert_eq!(Refusal::ProfileForbidsIt.notice(), None);
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
        assert_eq!(pending[0].0.id(), 1);
        assert_eq!(pending[0].1.text, "keep me");
    }

    /// The next launch: a second journal over the same directory, which is
    /// what a relaunch is.
    fn next_run(journal: &Journal) -> Journal {
        Journal::new(journal.location().to_path_buf())
    }

    #[test]
    fn a_clean_document_in_the_next_run_does_not_discard_the_last_runs_checkpoint() {
        // The defect ADR-0084 found. Every launch opens a clean Untitled as
        // document 1, and the checkpoint pass discards the journal of every
        // clean document -- which, with files named by id alone, was the
        // previous run's unsaved document 1, gone before the recovery
        // question was answered.
        let (_dir, crashed) = journal();
        crashed
            .checkpoint(
                1,
                &checkpoint("work from the crashed run"),
                Recovery::Plaintext,
            )
            .unwrap();

        let relaunched = next_run(&crashed);
        relaunched.discard(1).unwrap();

        let left = relaunched.left_behind();
        assert_eq!(left.len(), 1, "the crashed run's checkpoint must survive");
        assert_eq!(left[0].1.text, "work from the crashed run");
    }

    #[test]
    fn what_this_run_wrote_is_live_rather_than_left_behind() {
        let (_dir, journal) = journal();
        journal
            .checkpoint(1, &checkpoint("typing now"), Recovery::Plaintext)
            .unwrap();

        assert!(
            journal.left_behind().is_empty(),
            "a run must not offer to recover its own live work"
        );
        assert_eq!(journal.pending().len(), 1, "but it is still on disk");
        assert_eq!(next_run(&journal).left_behind().len(), 1);
    }

    #[test]
    fn a_checkpoint_written_before_sessions_existed_is_still_offered_and_can_be_forgotten() {
        // Unsaved work already on disk from an earlier build is named
        // `7.json`. Losing it to a filename change would be the upgrade
        // deleting somebody's work.
        let (_dir, journal) = journal();
        std::fs::create_dir_all(journal.location()).unwrap();
        let old = serde_json::to_vec(&checkpoint("from an earlier build")).unwrap();
        std::fs::write(journal.location().join("7.json"), old).unwrap();

        let left = journal.left_behind();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].0.id(), 7);

        journal.forget(&[left[0].0.clone()]).unwrap();
        assert!(journal.pending().is_empty());
    }

    #[test]
    fn forgetting_removes_the_named_entries_and_nothing_else() {
        // Declining recovery answers for what was offered. By the time the
        // answer arrives this run may have checkpoints of its own, and a
        // "forget everything" would take those too.
        let (_dir, crashed) = journal();
        crashed
            .checkpoint(1, &checkpoint("a"), Recovery::Plaintext)
            .unwrap();
        crashed
            .checkpoint(2, &checkpoint("b"), Recovery::Plaintext)
            .unwrap();

        let relaunched = next_run(&crashed);
        let offered: Vec<Entry> = relaunched
            .left_behind()
            .into_iter()
            .filter(|(_, c)| c.text == "a")
            .map(|(e, _)| e)
            .collect();
        relaunched
            .checkpoint(1, &checkpoint("typed since"), Recovery::Plaintext)
            .unwrap();

        relaunched.forget(&offered).unwrap();

        let mut texts: Vec<String> = relaunched
            .pending()
            .into_iter()
            .map(|(_, c)| c.text)
            .collect();
        texts.sort();
        assert_eq!(texts, ["b", "typed since"]);
    }

    #[test]
    fn two_journals_made_in_one_process_are_two_sessions() {
        // Trap 5: unique must mean unique, including back to back in one
        // process, where the clock may not have moved.
        let (_dir, first) = journal();
        let second = next_run(&first);
        first
            .checkpoint(1, &checkpoint("first"), Recovery::Plaintext)
            .unwrap();
        second
            .checkpoint(1, &checkpoint("second"), Recovery::Plaintext)
            .unwrap();

        assert_eq!(
            first.pending().len(),
            2,
            "document 1 of each run must be two files, not one overwritten"
        );
    }

    /// Backdate a file past `STALE_TEMPORARY`.
    fn age(path: &Path) {
        let then =
            std::time::SystemTime::now() - STALE_TEMPORARY - std::time::Duration::from_secs(5);
        std::fs::File::options()
            .write(true)
            .open(path)
            .and_then(|f| f.set_modified(then))
            .expect("backdate the file");
    }

    #[test]
    fn a_fresh_temporary_file_is_another_runs_write_in_progress_and_is_left_alone() {
        // Found in review of PR #6. Every run shares the directory, and a
        // launch that deleted every `.tmp` could take one another instance
        // was about to rename, failing that checkpoint.
        let (_dir, journal) = journal();
        std::fs::create_dir_all(journal.location()).unwrap();
        let live = journal.location().join("other-run-1.json.tmp");
        std::fs::write(&live, "{ being written").unwrap();

        next_run(&journal).clean_temporaries();

        assert!(live.exists(), "a write in progress was deleted");
    }

    #[test]
    fn a_session_tag_already_on_disk_is_never_reused() {
        // Found in review of PR #6: a reused process id and a clock that
        // repeats could draw a tag an earlier run used, and the new run would
        // overwrite that run's work and call it its own.
        let (_dir, journal) = journal();
        journal
            .checkpoint(1, &checkpoint("earlier"), Recovery::Plaintext)
            .unwrap();

        assert!(session_in_use(journal.location(), &journal.session));
        assert!(!session_in_use(journal.location(), "a-tag-nobody-used"));
        assert!(
            !session_in_use(&journal.location().join("missing"), &journal.session),
            "a directory that does not exist holds no sessions"
        );
    }
}
