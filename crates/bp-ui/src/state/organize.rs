//! Organize (ADR-0037): wiring `bp-storage`'s local metadata store into the
//! save path, and the two capabilities it makes possible.
//!
//! `bp-storage` decides what the store *is* -- the schema, the policy gate on
//! `record_document`, and now `related_to` and `possible_duplicates`. This
//! module owns what only the shell can: which file on this machine the store
//! is, tagging a document from something already extracted (`bp-semantic`'s
//! keywords -- there is no manual-tagging UI, and building one is out of
//! scope for this phase), and turning a query result into the sentence a
//! user reads.
//!
//! ## The store is a field, not a function
//!
//! The same shape `signing_key` and `audit_path` already needed, for the same
//! reason, and by now a familiar one: a function that resolves the real
//! platform Data directory means `cargo test` opens -- or creates -- a real
//! `.sqlite` file in the developer's own profile the moment any test reaches
//! [`AppState::record_for_organize`], which runs on every successful save.
//! `bp_storage::Store::in_memory()` exists precisely for this, and unlike
//! `signing_key`'s path there is nothing to keep unique across a process:
//! an in-memory SQLite connection has no file to collide over.
//!
//! `AppState::store` is `None` when the platform does not say where the Data
//! directory is, mirroring `signing_key`. Every capability here degrades to
//! quietly doing nothing rather than panicking on a missing store -- Organize
//! is not a feature this product refuses to run without.

use std::path::Path;

use bp_core::{Document, DocumentId};
use bp_storage::{DocumentRecord, Store};

use super::AppState;

/// How many keywords tag a document.
///
/// The same cap Note ▸ Keywords itself uses (`bp_semantic::keywords(&text,
/// 12)`): Related Notes then surfaces the same words a user can already see
/// on that menu, rather than a second, unrelated notion of "the keywords" of
/// a document existing only for this store.
const TAG_LIMIT: usize = 12;

/// How many related documents Organize ▸ Related Notes shows.
const RELATED_LIMIT: usize = 20;

/// Where the metadata store lives: in the Data directory, in a file of its
/// own -- see `bp_platform::DirKind`.
///
/// `None` when the environment does not say where the user's profile is.
/// `bp-platform` refuses to guess one, and this says so rather than putting a
/// database beside whatever file the user happened to open. A failure to
/// create the directory or open the file (a locked or corrupt store, say) is
/// folded into the same `None`: Organize is a convenience layered on
/// `bp-storage`, not a requirement the rest of the shell depends on, so a
/// store that cannot be opened is a store this session does not have, not a
/// reason to refuse to start.
///
/// **`create_dir_all` first, matching the exact pattern `begin_signing`
/// already uses for its own key file** (`state/security.rs`) -- found by
/// driving the built binary, not by a test: `Store::open` is a bare
/// `Connection::open`, which does not create a missing parent directory, and
/// `DirKind::Data` names a directory nothing had ever created before this.
/// The signing key's directory exists on a machine that has already signed
/// something; a fresh machine's `data` directory does not exist until
/// something asks for it to.
#[cfg(not(test))]
pub(super) fn default_store() -> Option<Store> {
    let dir = bp_platform::dirs::host_directory(bp_platform::DirKind::Data)?;
    std::fs::create_dir_all(&dir).ok()?;
    Store::open(&dir.join("organize.sqlite")).ok()
}

/// The same, redirected under test.
///
/// The fourth time this crate has needed the shape `signing_key`'s doc
/// comment describes. Without it, `AppState::new()` opens a real SQLite file
/// in the developer's own `%LOCALAPPDATA%` the moment any test's save path
/// reaches [`AppState::record_for_organize`] -- which is most of them, since
/// it runs on every successful save. `Store::in_memory()` is `bp-storage`'s
/// own answer for exactly this, and it needs no per-process uniqueness the
/// way the signing key's temp-file path does: an in-memory connection is
/// already private to the `Store` that opened it, so two states in one
/// process cannot share one by construction.
#[cfg(test)]
pub(super) fn default_store() -> Option<Store> {
    Store::in_memory().ok()
}

impl AppState {
    /// Record the just-saved document, tag it from its keywords, and check
    /// for a possible duplicate -- all of it governed by the document's
    /// `Metadata` policy (ADR-0020), which `record_document` already applies.
    ///
    /// Called after the write in [`Self::save_document`], never before: a
    /// store that is slow, missing or refuses to open must never be the
    /// reason a save fails. Quietly does nothing when `store` is `None`.
    pub(crate) fn record_for_organize(&mut self, id: DocumentId, path: &Path) {
        let Some(store) = &self.store else { return };
        let Some(doc) = self.workspace.get(id) else {
            return;
        };
        let policy = doc.security().policy_under(self.privacy).metadata;
        let text = self.text_of(id);
        let title = bp_semantic::suggest_title(&text);

        let Ok(stored_id) = store.record_document(path, title.as_deref(), unix_now(), policy)
        else {
            return;
        };

        // Tags need something to tag, and `record_document` returns `None`
        // under a `Disabled` policy -- there is no row to attach a tag to,
        // which is the point: nothing about this document was recorded at
        // all.
        if let Some(stored_id) = stored_id {
            for tag in bp_semantic::keywords(&text, TAG_LIMIT) {
                let _ = store.tag_document(stored_id, &tag);
            }
        }

        // A document recorded under `PathOnly` or `Disabled` has no title in
        // the store to check duplicates by -- reading `title` again here
        // rather than re-deriving it is what keeps this arm and
        // `record_document`'s own policy gate unable to disagree about what
        // this document's title is allowed to be.
        if let Some(title) = &title
            && let Some(notice) = duplicate_notice(&self.possible_duplicates(title, path))
        {
            self.error = Some(notice);
        }
    }

    /// Possible duplicates for `title`, excluding `exclude_path`.
    ///
    /// The one place this question is asked: both the automatic check in
    /// [`Self::record_for_organize`] and the on-demand Organize ▸ Duplicate
    /// Detection row read through here, so a save-time notice and a click on
    /// the menu row can never disagree about what counts as a duplicate.
    fn possible_duplicates(&self, title: &str, exclude_path: &Path) -> Vec<DocumentRecord> {
        let Some(store) = &self.store else {
            return Vec::new();
        };
        store
            .possible_duplicates(title, exclude_path)
            .unwrap_or_default()
    }

    /// Organize ▸ Duplicate Detection, run for the active document on
    /// request.
    ///
    /// The same check [`Self::record_for_organize`] runs automatically at
    /// save time -- this is the row for the moment before a save, when a
    /// user wants the answer without committing to one, or simply wants to
    /// ask again for a document they have not just changed.
    pub(crate) fn duplicate_detection_report(&self) -> String {
        let text = self.active_text();
        let Some(title) = bp_semantic::suggest_title(&text) else {
            return "There is nothing in this document to check for duplicates.".to_owned();
        };
        let exclude = self
            .workspace
            .active_id()
            .and_then(|id| self.workspace.get(id))
            .and_then(Document::path)
            .map(Path::to_path_buf)
            .unwrap_or_default();

        let duplicates = self.possible_duplicates(&title, &exclude);
        if duplicates.is_empty() {
            "Nothing looked like a duplicate.".to_owned()
        } else {
            let mut body = format!(
                "{} possible duplicate{} found under the title “{title}”:\n\n",
                duplicates.len(),
                if duplicates.len() == 1 { "" } else { "s" }
            );
            for record in &duplicates {
                body.push_str("- ");
                body.push_str(filename_of(&record.path));
                body.push('\n');
            }
            body
        }
    }

    /// The active document's related notes: other documents in the store
    /// that share a tag with it, for Organize ▸ Related Notes.
    ///
    /// Empty rather than an error whenever there is nothing to relate --
    /// no store, no active document, no path (an unsaved document has never
    /// been recorded), or a path the store has never seen.
    pub(crate) fn related_notes_for_active(&self) -> Vec<DocumentRecord> {
        let Some(store) = &self.store else {
            return Vec::new();
        };
        let Some(id) = self.workspace.active_id() else {
            return Vec::new();
        };
        let Some(path) = self.workspace.get(id).and_then(Document::path) else {
            return Vec::new();
        };
        let Some(record) = store.document(path).ok().flatten() else {
            return Vec::new();
        };
        store
            .related_to(record.id, RELATED_LIMIT)
            .unwrap_or_default()
    }
}

/// The compact, non-blocking notice the automatic half of Duplicate
/// Detection shows in the status bar.
///
/// The first one or two filenames, never content -- a status-bar line is not
/// the place for a full report, and content is not a thing this store ever
/// held to begin with (ADR-0019).
fn duplicate_notice(duplicates: &[DocumentRecord]) -> Option<String> {
    if duplicates.is_empty() {
        return None;
    }
    let names: Vec<&str> = duplicates
        .iter()
        .take(2)
        .map(|d| filename_of(&d.path))
        .collect();
    Some(format!(
        "possible duplicate{} of {} — Organize ▸ Duplicate Detection for more",
        if duplicates.len() == 1 { "" } else { "s" },
        names.join(", ")
    ))
}

/// A document's filename, never its content -- the one thing a duplicate
/// notice is allowed to show.
fn filename_of(path: &str) -> &str {
    Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
}

/// Seconds since the epoch, for `record_document`'s `seen_at` parameter.
///
/// `bp_history::now_unix` already exists for the recovery journal's
/// timestamps and returns `u64`; `bp-storage` takes `i64` (SQLite has no
/// unsigned integer type), so this is the one place that conversion happens
/// rather than each call site repeating it.
fn unix_now() -> i64 {
    i64::try_from(bp_history::now_unix()).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use bp_security::Metadata;

    #[test]
    fn a_test_store_is_in_memory_and_never_touches_the_users_own_profile() {
        // The protection itself, pinned, and the fourth time this crate has
        // needed one: `record_for_organize` runs on every successful save, so
        // a test that merely saves a document reaches it.
        let state = AppState::new();
        let store = state.store.as_ref().expect("a test store is always set");

        // An in-memory connection has no file at all, so recording a
        // document succeeds without ever touching a path on disk -- which a
        // real `Store::open` could not promise.
        let id = store
            .record_document(Path::new("/notes/a.md"), Some("A"), 1, Metadata::Summary)
            .unwrap();
        assert!(id.is_some());

        // Two states in one process must not share a database. If they did,
        // this document would already be visible from a second, freshly-made
        // state.
        let other = AppState::new();
        assert!(
            other
                .store
                .as_ref()
                .unwrap()
                .document(Path::new("/notes/a.md"))
                .unwrap()
                .is_none(),
            "a fresh state's store must start empty"
        );
    }

    /// A state with one saved document, and the path it was saved to.
    fn a_saved_document(text: &str) -> (tempfile::TempDir, AppState, PathBuf) {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("report.txt");
        std::fs::write(&path, text).expect("write");

        let mut state = AppState::new();
        state.open(path.clone());
        (dir, state, path)
    }

    #[test]
    fn saving_records_the_document_and_tags_it_from_its_keywords() {
        let (_dir, mut state, path) =
            a_saved_document("# Quarterly Rust Report\n\nrust rust rust notes notes budget");
        let id = state.workspace.active_id().unwrap();

        state.save_document(id, Some(path.clone()));

        let store = state.store.as_ref().unwrap();
        let record = store.document(&path).unwrap().expect("recorded");
        assert_eq!(record.title.as_deref(), Some("Quarterly Rust Report"));

        let tags = store.tags_of(record.id).unwrap();
        assert!(
            tags.iter().any(|t| t.eq_ignore_ascii_case("rust")),
            "got {tags:?}"
        );
    }

    #[test]
    fn saving_under_a_disabled_policy_records_nothing() {
        let (_dir, mut state, path) = a_saved_document("secret plans");
        let id = state.workspace.active_id().unwrap();
        state.set_security(bp_security::Security::Named(
            bp_security::Profile::Confidential,
        ));

        state.save_document(id, Some(path.clone()));

        assert!(
            state
                .store
                .as_ref()
                .unwrap()
                .document(&path)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn saving_a_second_document_under_the_same_title_notices_the_first() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut state = AppState::new();

        let first_path = dir.path().join("first.txt");
        std::fs::write(&first_path, "# Weekly Notes\n\nfirst").unwrap();
        state.open(first_path.clone());
        let first_id = state.workspace.active_id().unwrap();
        state.save_document(first_id, Some(first_path.clone()));

        let second_path = dir.path().join("second.txt");
        std::fs::write(&second_path, "# Weekly Notes\n\nsecond").unwrap();
        state.open(second_path.clone());
        let second_id = state.workspace.active_id().unwrap();
        state.save_document(second_id, Some(second_path));

        let notice = state.error.expect("a duplicate notice");
        assert!(notice.contains("first.txt"), "got {notice:?}");
    }

    #[test]
    fn duplicate_detection_on_demand_agrees_with_the_automatic_check() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut state = AppState::new();

        let first_path = dir.path().join("first.txt");
        std::fs::write(&first_path, "# Shared Title\n\none").unwrap();
        state.open(first_path.clone());
        let first_id = state.workspace.active_id().unwrap();
        state.save_document(first_id, Some(first_path));

        let second_path = dir.path().join("second.txt");
        std::fs::write(&second_path, "# Shared Title\n\ntwo").unwrap();
        state.open(second_path.clone());
        let second_id = state.workspace.active_id().unwrap();
        state.save_document(second_id, Some(second_path));

        let report = state.duplicate_detection_report();
        assert!(report.contains("first.txt"), "got {report}");
        assert!(report.starts_with('1'), "got {report}");
    }

    #[test]
    fn duplicate_detection_says_so_when_nothing_matches() {
        let (_dir, state, _path) = a_saved_document("# Unique Title\n\nnothing else like it");
        assert_eq!(
            state.duplicate_detection_report(),
            "Nothing looked like a duplicate."
        );
    }

    #[test]
    fn duplicate_detection_on_an_empty_document_says_there_is_nothing_to_check() {
        let state = AppState::new();
        assert_eq!(
            state.duplicate_detection_report(),
            "There is nothing in this document to check for duplicates."
        );
    }

    #[test]
    fn related_notes_are_empty_before_anything_is_saved() {
        let state = AppState::new();
        assert!(state.related_notes_for_active().is_empty());
    }

    #[test]
    fn related_notes_share_a_tag_with_the_active_document() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut state = AppState::new();

        let a_path = dir.path().join("a.txt");
        std::fs::write(&a_path, "rust rust rust notes").unwrap();
        state.open(a_path.clone());
        let a_id = state.workspace.active_id().unwrap();
        state.save_document(a_id, Some(a_path.clone()));

        let b_path = dir.path().join("b.txt");
        std::fs::write(&b_path, "rust rust rust cooking").unwrap();
        state.open(b_path.clone());
        let b_id = state.workspace.active_id().unwrap();
        state.save_document(b_id, Some(b_path.clone()));

        // Back to `a`: it should now see `b` as related, sharing "rust".
        state.workspace.set_active(a_id);
        let related = state.related_notes_for_active();
        assert!(
            related.iter().any(|r| r.path.ends_with("b.txt")),
            "got {related:?}"
        );
    }
}
