//! Noticing that a file changed underneath us.
//!
//! specs.md section 6 requires an external-change watcher and a
//! compare/reload/keep-mine flow. This is the detection half: a cheap stamp
//! of what we last saw on disk, and a comparison against what is there now.
//!
//! Deliberately polling rather than an OS watch API. Polling two numbers per
//! open document costs nothing at these scales, works identically on Windows
//! and Linux, needs no dependency and no background thread delivering events
//! into the UI loop, and -- most usefully -- the comparison is a pure
//! function that can be tested without touching a filesystem. A real watcher
//! belongs here later; it would replace `changed`, not the decision logic
//! around it.

use std::path::Path;
use std::time::SystemTime;

/// What a file looked like when we last read or wrote it.
///
/// Modification time *and* length. Time alone misses a change made within
/// the filesystem's timestamp granularity, and length alone misses an edit
/// that preserves the size -- which is exactly what "fix a typo" does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileStamp {
    pub modified: Option<SystemTime>,
    pub len: u64,
}

impl FileStamp {
    /// Read the current stamp of `path`.
    ///
    /// `None` when the file cannot be stat'd -- it may have been deleted or
    /// become unreadable, which is a different situation from "changed" and
    /// is left to the caller to interpret.
    pub fn of(path: &Path) -> Option<Self> {
        let metadata = std::fs::metadata(path).ok()?;
        Some(Self {
            // Not every filesystem reports mtime; length still works there.
            modified: metadata.modified().ok(),
            len: metadata.len(),
        })
    }
}

/// What a check of an open document found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskState {
    /// Matches what we last saw.
    Unchanged,
    /// Someone else wrote to it.
    Modified,
    /// It is no longer there, or no longer readable.
    Missing,
}

/// Compare a file against the stamp taken when it was last read or written.
pub fn check(path: &Path, since: FileStamp) -> DiskState {
    match FileStamp::of(path) {
        None => DiskState::Missing,
        Some(now) if now == since => DiskState::Unchanged,
        Some(_) => DiskState::Modified,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SaveOptions, atomic_write};
    use tempfile::tempdir;

    fn write(path: &Path, text: &str) {
        atomic_write(path, text.as_bytes(), SaveOptions::default()).unwrap();
    }

    #[test]
    fn an_untouched_file_is_unchanged() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a.txt");
        write(&path, "hello");

        let stamp = FileStamp::of(&path).unwrap();
        assert_eq!(check(&path, stamp), DiskState::Unchanged);
    }

    #[test]
    fn a_rewritten_file_is_modified() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a.txt");
        write(&path, "hello");
        let stamp = FileStamp::of(&path).unwrap();

        write(&path, "hello world");

        assert_eq!(check(&path, stamp), DiskState::Modified);
    }

    #[test]
    fn a_same_length_edit_is_still_noticed() {
        // The case length-only detection misses, and the reason the stamp
        // carries both fields: correcting a typo does not change the size.
        let dir = tempdir().unwrap();
        let path = dir.path().join("a.txt");
        write(&path, "teh cat");
        let stamp = FileStamp::of(&path).unwrap();

        // Force a distinct mtime; some filesystems have coarse granularity.
        std::thread::sleep(std::time::Duration::from_millis(20));
        write(&path, "the cat");

        let now = FileStamp::of(&path).unwrap();
        assert_eq!(now.len, stamp.len, "same length, by construction");
        assert_eq!(
            check(&path, stamp),
            DiskState::Modified,
            "mtime must catch what length cannot"
        );
    }

    #[test]
    fn a_deleted_file_is_missing_not_modified() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a.txt");
        write(&path, "hello");
        let stamp = FileStamp::of(&path).unwrap();

        std::fs::remove_file(&path).unwrap();

        assert_eq!(
            check(&path, stamp),
            DiskState::Missing,
            "deleted is a different problem from changed"
        );
    }

    #[test]
    fn a_file_that_never_existed_is_missing() {
        let dir = tempdir().unwrap();
        let stamp = FileStamp {
            modified: None,
            len: 0,
        };
        assert_eq!(
            check(&dir.path().join("nope.txt"), stamp),
            DiskState::Missing
        );
    }

    #[test]
    fn saving_through_us_updates_the_stamp() {
        // Our own atomic save must not look like an external change, or the
        // editor would warn about itself after every write.
        let dir = tempdir().unwrap();
        let path = dir.path().join("a.txt");
        write(&path, "first");

        std::thread::sleep(std::time::Duration::from_millis(20));
        write(&path, "second");
        let after_save = FileStamp::of(&path).unwrap();

        assert_eq!(check(&path, after_save), DiskState::Unchanged);
    }
}
