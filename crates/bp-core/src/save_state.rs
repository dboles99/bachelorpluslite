//! Save state, and the type-level separation between a disk save and a
//! recovery checkpoint.

use time::OffsetDateTime;

/// The moment a write reached disk, flushed and verified.
///
/// This is a distinct type from [`CheckpointTime`] on purpose. specs.md
/// section 3 requires that "a recovery checkpoint must never be shown as a
/// successful disk save", and the status bar shows both. Two newtypes over
/// the same instant mean the compiler rejects passing one where the other is
/// expected, so the mistake cannot be made silently in UI wiring.
///
/// Constructing one is an assertion that the write actually completed --
/// [`crate::Document::record_disk_save`] is the intended caller, after
/// `bp-files` reports a verified atomic write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct DiskSaveTime(OffsetDateTime);

impl DiskSaveTime {
    /// Assert that a write to disk completed at `at`.
    pub const fn new(at: OffsetDateTime) -> Self {
        Self(at)
    }

    pub const fn get(self) -> OffsetDateTime {
        self.0
    }
}

/// The moment a recovery checkpoint was written to the edit journal.
///
/// Deliberately not convertible to [`DiskSaveTime`]. A checkpoint means the
/// user's work is recoverable after a crash; it does not mean their file on
/// disk is up to date, and conflating the two would tell the user their data
/// is safe when it is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CheckpointTime(OffsetDateTime);

impl CheckpointTime {
    pub const fn new(at: OffsetDateTime) -> Self {
        Self(at)
    }

    pub const fn get(self) -> OffsetDateTime {
        self.0
    }
}

/// What has actually reached disk for a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SaveState {
    /// No successful write has ever completed for this document.
    #[default]
    NeverSaved,
    /// The buffer matches the bytes on disk, as of this save.
    Clean { at: DiskSaveTime },
    /// The buffer has edits that are not on disk. `last_disk_save` is `None`
    /// when the document has never been written at all -- that is the
    /// "edited but never saved" case, distinct from [`Self::NeverSaved`],
    /// which is an untouched new buffer.
    Dirty {
        last_disk_save: Option<DiskSaveTime>,
    },
}

impl SaveState {
    /// True when there are edits not yet on disk.
    pub const fn is_dirty(self) -> bool {
        matches!(self, Self::Dirty { .. })
    }

    /// The last verified disk save, if any.
    ///
    /// Never returns a recovery checkpoint -- see [`CheckpointTime`].
    pub const fn last_disk_save(self) -> Option<DiskSaveTime> {
        match self {
            Self::NeverSaved => None,
            Self::Clean { at } => Some(at),
            Self::Dirty { last_disk_save } => last_disk_save,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    const T1: OffsetDateTime = datetime!(2026-08-16 20:05 UTC);

    #[test]
    fn never_saved_has_no_disk_save() {
        assert_eq!(SaveState::NeverSaved.last_disk_save(), None);
        assert!(!SaveState::NeverSaved.is_dirty());
    }

    #[test]
    fn dirty_remembers_the_previous_disk_save() {
        let s = SaveState::Dirty {
            last_disk_save: Some(DiskSaveTime::new(T1)),
        };
        assert!(s.is_dirty());
        assert_eq!(s.last_disk_save().map(DiskSaveTime::get), Some(T1));
    }

    #[test]
    fn edited_but_never_saved_is_dirty_with_no_disk_save() {
        let s = SaveState::Dirty {
            last_disk_save: None,
        };
        assert!(s.is_dirty());
        assert_eq!(s.last_disk_save(), None);
    }
}
