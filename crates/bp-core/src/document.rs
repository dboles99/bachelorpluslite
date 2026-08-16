//! The document model: one open buffer and everything the status bar and the
//! naming engine need to know about it.

use std::path::{Path, PathBuf};

use time::OffsetDateTime;

use crate::{CheckpointTime, DiskSaveTime, Encoding, LineEnding, SaveState};

/// Opaque handle to a document within a [`crate::Workspace`].
///
/// Opaque so that tabs can be reordered and documents closed without any
/// caller depending on a positional index that would then be wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DocumentId(u64);

impl DocumentId {
    pub(crate) const fn new(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Label shown when a document has no path yet.
pub const UNTITLED: &str = "Untitled";

/// Label shown in the status bar's path row when nothing is assigned.
pub const NO_LOCATION: &str = "Location not assigned";

/// One open document.
///
/// This type holds no text. The buffer lives in `bp-buffer`, so the document
/// model stays cheap to clone, inspect and test, and so the status bar never
/// has to borrow the editing buffer to render.
#[derive(Debug, Clone)]
pub struct Document {
    id: DocumentId,
    path: Option<PathBuf>,
    title: String,
    created_at: OffsetDateTime,
    save_state: SaveState,
    last_checkpoint: Option<CheckpointTime>,
    encoding: Encoding,
    line_ending: LineEnding,
}

impl Document {
    /// A new empty document that has never touched disk.
    pub fn new(id: DocumentId, created_at: OffsetDateTime) -> Self {
        Self {
            id,
            path: None,
            title: UNTITLED.to_owned(),
            created_at,
            save_state: SaveState::NeverSaved,
            last_checkpoint: None,
            encoding: Encoding::default(),
            line_ending: LineEnding::default(),
        }
    }

    /// A document opened from an existing file.
    ///
    /// `created_at` is the document's creation date for naming purposes, not
    /// the time it was opened -- callers should pass the filesystem creation
    /// time where it is available.
    pub fn opened(id: DocumentId, path: PathBuf, created_at: OffsetDateTime) -> Self {
        let title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(UNTITLED)
            .to_owned();
        Self {
            id,
            path: Some(path),
            title,
            created_at,
            // Opening is not saving. Until a write completes, this document
            // has no disk save of its own to report.
            save_state: SaveState::NeverSaved,
            last_checkpoint: None,
            encoding: Encoding::default(),
            line_ending: LineEnding::default(),
        }
    }

    pub const fn id(&self) -> DocumentId {
        self.id
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = title.into();
    }

    /// The document's creation timestamp.
    ///
    /// BP-ADR-0004: this is the date the default filename uses, and it does
    /// not move when the document is saved. There is deliberately no setter.
    pub const fn created_at(&self) -> OffsetDateTime {
        self.created_at
    }

    pub const fn save_state(&self) -> SaveState {
        self.save_state
    }

    pub const fn is_dirty(&self) -> bool {
        self.save_state.is_dirty()
    }

    /// Last verified write to disk. Never a recovery checkpoint.
    pub const fn last_disk_save(&self) -> Option<DiskSaveTime> {
        self.save_state.last_disk_save()
    }

    /// Last recovery checkpoint. Never a disk save.
    pub const fn last_checkpoint(&self) -> Option<CheckpointTime> {
        self.last_checkpoint
    }

    pub const fn encoding(&self) -> Encoding {
        self.encoding
    }

    pub fn set_encoding(&mut self, encoding: Encoding) {
        self.encoding = encoding;
    }

    pub const fn line_ending(&self) -> LineEnding {
        self.line_ending
    }

    pub fn set_line_ending(&mut self, line_ending: LineEnding) {
        self.line_ending = line_ending;
    }

    /// Record that a verified write reached disk at `at`.
    ///
    /// Call this only after `bp-files` reports a successful atomic write. It
    /// clears the dirty flag, which tells the user their data is safe.
    pub fn record_disk_save(&mut self, at: OffsetDateTime) {
        self.save_state = SaveState::Clean {
            at: DiskSaveTime::new(at),
        };
    }

    /// Record a recovery checkpoint.
    ///
    /// This deliberately does not touch [`Self::save_state`]. A checkpoint
    /// makes the work recoverable after a crash; the file on disk is still
    /// stale, and the document is still dirty.
    pub fn record_checkpoint(&mut self, at: OffsetDateTime) {
        self.last_checkpoint = Some(CheckpointTime::new(at));
    }

    /// Record that the buffer was edited.
    ///
    /// Idempotent, and preserves the previous disk-save time so the status
    /// bar can keep showing "last disk save 8:01 PM" while unsaved.
    pub fn mark_modified(&mut self) {
        if !self.save_state.is_dirty() {
            self.save_state = SaveState::Dirty {
                last_disk_save: self.save_state.last_disk_save(),
            };
        }
    }

    /// Assign or change the document's path, as Save As does.
    pub fn set_path(&mut self, path: PathBuf) {
        self.path = Some(path);
    }

    /// Tab label: the file name, or `Untitled`.
    pub fn display_name(&self) -> &str {
        self.path
            .as_deref()
            .and_then(Path::file_name)
            .and_then(|s| s.to_str())
            .unwrap_or(UNTITLED)
    }

    /// Status-bar path row: the full path, or a clear statement that there
    /// isn't one. Never an empty string -- a blank row reads as a bug.
    pub fn location_label(&self) -> String {
        self.path
            .as_deref()
            .map_or_else(|| NO_LOCATION.to_owned(), |p| p.display().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    const CREATED: OffsetDateTime = datetime!(2026-08-16 09:00 UTC);
    const SAVED: OffsetDateTime = datetime!(2026-08-16 20:05 UTC);
    const LATER: OffsetDateTime = datetime!(2026-08-17 11:00 UTC);

    fn doc() -> Document {
        Document::new(DocumentId::new(1), CREATED)
    }

    #[test]
    fn new_document_is_untitled_and_unsaved() {
        let d = doc();
        assert_eq!(d.display_name(), UNTITLED);
        assert_eq!(d.location_label(), NO_LOCATION);
        assert_eq!(d.save_state(), SaveState::NeverSaved);
        assert!(!d.is_dirty());
    }

    #[test]
    fn a_checkpoint_is_not_a_disk_save() {
        // The rule from specs.md section 3, asserted directly.
        let mut d = doc();
        d.mark_modified();
        d.record_checkpoint(SAVED);

        assert!(d.last_checkpoint().is_some());
        assert_eq!(d.last_disk_save(), None);
        assert!(d.is_dirty(), "a checkpoint must not clear the dirty flag");
    }

    #[test]
    fn saving_clears_dirty_and_records_the_time() {
        let mut d = doc();
        d.mark_modified();
        d.record_disk_save(SAVED);

        assert!(!d.is_dirty());
        assert_eq!(d.last_disk_save().map(DiskSaveTime::get), Some(SAVED));
    }

    #[test]
    fn editing_after_a_save_keeps_the_previous_save_time() {
        let mut d = doc();
        d.record_disk_save(SAVED);
        d.mark_modified();

        assert!(d.is_dirty());
        assert_eq!(
            d.last_disk_save().map(DiskSaveTime::get),
            Some(SAVED),
            "status bar still needs to show the last real save while dirty"
        );
    }

    #[test]
    fn mark_modified_is_idempotent() {
        let mut d = doc();
        d.record_disk_save(SAVED);
        d.mark_modified();
        d.mark_modified();
        assert_eq!(d.last_disk_save().map(DiskSaveTime::get), Some(SAVED));
    }

    #[test]
    fn creation_date_does_not_move_when_saving() {
        // BP-ADR-0004. The default filename date would otherwise drift on
        // every save.
        let mut d = doc();
        d.record_disk_save(LATER);
        assert_eq!(d.created_at(), CREATED);
    }

    #[test]
    fn opening_a_file_is_not_a_save() {
        let d = Document::opened(
            DocumentId::new(2),
            PathBuf::from("/notes/Report_16AUG2026.txt"),
            CREATED,
        );
        assert_eq!(d.display_name(), "Report_16AUG2026.txt");
        assert_eq!(d.title(), "Report_16AUG2026");
        assert_eq!(d.last_disk_save(), None);
        assert!(!d.is_dirty());
    }

    #[test]
    fn save_as_updates_the_displayed_location() {
        let mut d = doc();
        d.set_path(PathBuf::from("/notes/Thing_16AUG2026.txt"));
        assert_eq!(d.display_name(), "Thing_16AUG2026.txt");
        assert!(d.location_label().contains("Thing_16AUG2026.txt"));
    }
}
