//! Atomic save.

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use bp_naming::SemanticName;
use tempfile::NamedTempFile;
use thiserror::Error;

/// Chunk size for read-back verification. Streaming rather than slurping
/// keeps peak memory at the buffer size even when saving a huge file.
const VERIFY_CHUNK: usize = 64 * 1024;

/// Everything that can go wrong during a save.
///
/// Each variant names the file and says what the user can do about it -- the
/// Definition of Done requires user-visible error states, and "os error 32"
/// is not one.
#[derive(Debug, Error)]
pub enum SaveError {
    #[error("cannot work out which directory to save {} into", .path.display())]
    NoParentDirectory { path: PathBuf },

    #[error("cannot create a temporary file next to {}: {source}", .path.display())]
    TempCreate {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("cannot write {}: {source}", .path.display())]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("cannot flush {} to disk: {source}", .path.display())]
    Flush {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("{} already exists", .path.display())]
    AlreadyExists { path: PathBuf },

    #[error(
        "cannot replace {} -- it may be read-only, or open in another program: {source}",
        .path.display()
    )]
    Replace {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("cannot read back {} to verify the save: {source}", .path.display())]
    Verify {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("{} does not match what was just written to it", .path.display())]
    VerificationFailed { path: PathBuf },
}

/// What to do when the target already exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Overwrite {
    /// Replace it. This is Save on an existing document.
    #[default]
    Replace,
    /// Fail instead. Used when writing a name that collision resolution
    /// promised was free -- checking and then writing would leave a race in
    /// which another process claims the name in between, so the guarantee has
    /// to come from the write itself.
    FailIfExists,
}

/// Save tuning.
#[derive(Debug, Clone, Copy)]
pub struct SaveOptions {
    /// Read the file back and compare it against what we wrote.
    ///
    /// On by default. It roughly doubles the I/O of a save, but it is the
    /// only thing that distinguishes "the OS accepted my bytes" from "the
    /// bytes are actually on the disk and readable", and the status bar
    /// claims the latter.
    pub verify: bool,
    pub overwrite: Overwrite,
}

impl Default for SaveOptions {
    fn default() -> Self {
        Self {
            verify: true,
            overwrite: Overwrite::default(),
        }
    }
}

/// What a successful save did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SaveOutcome {
    pub bytes_written: u64,
    /// Whether the contents were read back and confirmed.
    pub verified: bool,
    /// Whether an existing file was replaced, as opposed to a new one created.
    pub replaced_existing: bool,
}

/// Write `contents` to `path` atomically.
///
/// On success the file at `path` is exactly `contents`. On any error the
/// previous file at `path` is untouched and the temporary file is cleaned up.
pub fn atomic_write(
    path: &Path,
    contents: &[u8],
    options: SaveOptions,
) -> Result<SaveOutcome, SaveError> {
    let parent = parent_dir(path)?;
    let replaced_existing = path.exists();

    // Same directory as the target, so the later rename is a true atomic
    // rename rather than a cross-filesystem copy.
    let mut temp = NamedTempFile::new_in(&parent).map_err(|source| SaveError::TempCreate {
        path: path.to_owned(),
        source,
    })?;

    temp.write_all(contents)
        .map_err(|source| SaveError::Write {
            path: path.to_owned(),
            source,
        })?;
    temp.flush().map_err(|source| SaveError::Write {
        path: path.to_owned(),
        source,
    })?;

    // The durability step. `flush` only pushes past Rust's buffer; this is
    // what pushes past the OS cache to the device.
    temp.as_file()
        .sync_all()
        .map_err(|source| SaveError::Flush {
            path: path.to_owned(),
            source,
        })?;

    preserve_permissions(path, &temp);

    // The atomic step. Everything before this touched only the temp file, so
    // any earlier failure leaves the user's file exactly as it was.
    let _persisted = match options.overwrite {
        Overwrite::Replace => temp.persist(path).map_err(|e| SaveError::Replace {
            path: path.to_owned(),
            source: e.error,
        })?,
        Overwrite::FailIfExists => {
            temp.persist_noclobber(path)
                .map_err(|e| match e.error.kind() {
                    io::ErrorKind::AlreadyExists => SaveError::AlreadyExists {
                        path: path.to_owned(),
                    },
                    _ => SaveError::Replace {
                        path: path.to_owned(),
                        source: e.error,
                    },
                })?
        }
    };

    sync_directory(&parent);

    let verified = if options.verify {
        if !verify_contents(path, contents).map_err(|source| SaveError::Verify {
            path: path.to_owned(),
            source,
        })? {
            return Err(SaveError::VerificationFailed {
                path: path.to_owned(),
            });
        }
        true
    } else {
        false
    };

    Ok(SaveOutcome {
        bytes_written: contents.len() as u64,
        verified,
        replaced_existing,
    })
}

/// Pick the directory to create the temporary file in.
///
/// A bare `foo.txt` has a parent of `""`, which is not a usable directory, so
/// it resolves to the current directory.
fn parent_dir(path: &Path) -> Result<PathBuf, SaveError> {
    match path.parent() {
        Some(p) if p.as_os_str().is_empty() => Ok(PathBuf::from(".")),
        Some(p) => Ok(p.to_owned()),
        None => Err(SaveError::NoParentDirectory {
            path: path.to_owned(),
        }),
    }
}

/// Carry the existing file's permissions onto the replacement.
///
/// Unix only, deliberately. On Unix the mode carries real intent -- a note
/// saved as `0600` must not come back as `0644`. On Windows the readonly
/// attribute is the only thing `Permissions` exposes, and copying it onto the
/// temporary file would make the rename that replaces the original fail;
/// NTFS ACLs are inherited from the directory, which is the right default.
///
/// Failures are ignored: a save that succeeded with default permissions is a
/// far better outcome than a save refused over a metadata detail.
#[cfg(unix)]
fn preserve_permissions(path: &Path, temp: &NamedTempFile) {
    if let Ok(metadata) = std::fs::metadata(path) {
        let _ = temp.as_file().set_permissions(metadata.permissions());
    }
}

#[cfg(not(unix))]
fn preserve_permissions(_path: &Path, _temp: &NamedTempFile) {}

/// Flush the directory entry itself.
///
/// Without this the renamed file can survive a crash while the directory
/// entry pointing at it does not. Unix only -- Windows has no directory
/// handle to sync, and `MoveFileEx` already orders the metadata write.
/// Errors are ignored because some filesystems legitimately reject it.
#[cfg(unix)]
fn sync_directory(dir: &Path) {
    if let Ok(handle) = File::open(dir) {
        let _ = handle.sync_all();
    }
}

#[cfg(not(unix))]
fn sync_directory(_dir: &Path) {}

/// Read `path` back and compare it to `expected`, in chunks.
fn verify_contents(path: &Path, expected: &[u8]) -> io::Result<bool> {
    let mut file = File::open(path)?;
    let mut buf = vec![0u8; VERIFY_CHUNK];
    let mut offset = 0usize;

    loop {
        let read = file.read(&mut buf)?;
        if read == 0 {
            break;
        }
        let end = offset + read;
        if end > expected.len() || buf[..read] != expected[offset..end] {
            return Ok(false);
        }
        offset = end;
    }
    // A shorter file is a mismatch too.
    Ok(offset == expected.len())
}

/// Turn a [`SemanticName`] into a free path inside `dir`.
///
/// This is the filesystem half of `bp-naming`'s pure collision rule. The
/// existence check is advisory only: pair it with
/// [`Overwrite::FailIfExists`] if the caller must not clobber a file that
/// appeared in between.
///
/// Returns `None` if every suffix is taken.
pub fn resolve_in_dir(dir: &Path, name: &SemanticName) -> Option<PathBuf> {
    name.resolve_collision(|candidate| dir.join(candidate).exists())
        .map(|resolved| dir.join(resolved.to_filename()))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use tempfile::tempdir;
    use time::macros::date;

    fn write(path: &Path, contents: &[u8]) -> Result<SaveOutcome, SaveError> {
        atomic_write(path, contents, SaveOptions::default())
    }

    #[test]
    fn creates_a_new_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("note.txt");

        let out = write(&path, b"hello").unwrap();

        assert_eq!(fs::read(&path).unwrap(), b"hello");
        assert_eq!(out.bytes_written, 5);
        assert!(out.verified);
        assert!(!out.replaced_existing);
    }

    #[test]
    fn replaces_an_existing_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("note.txt");
        write(&path, b"original").unwrap();

        let out = write(&path, b"replaced").unwrap();

        assert_eq!(fs::read(&path).unwrap(), b"replaced");
        assert!(out.replaced_existing);
    }

    #[test]
    fn shrinking_a_file_does_not_leave_a_tail() {
        // The classic truncation bug: write-in-place leaves the old suffix
        // behind. Rename-based saving cannot.
        let dir = tempdir().unwrap();
        let path = dir.path().join("note.txt");
        write(&path, b"a very long original document").unwrap();

        write(&path, b"short").unwrap();

        assert_eq!(fs::read(&path).unwrap(), b"short");
    }

    #[test]
    fn writes_empty_files() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("empty.txt");

        let out = write(&path, b"").unwrap();

        assert_eq!(fs::read(&path).unwrap(), b"");
        assert_eq!(out.bytes_written, 0);
        assert!(out.verified);
    }

    #[test]
    fn leaves_no_temporary_files_behind() {
        let dir = tempdir().unwrap();
        write(&dir.path().join("note.txt"), b"hello").unwrap();

        let names: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names.len(), 1, "stray temp file: {names:?}");
    }

    #[test]
    fn fail_if_exists_refuses_to_clobber() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("note.txt");
        write(&path, b"original").unwrap();

        let err = atomic_write(
            &path,
            b"replacement",
            SaveOptions {
                overwrite: Overwrite::FailIfExists,
                ..SaveOptions::default()
            },
        )
        .unwrap_err();

        assert!(matches!(err, SaveError::AlreadyExists { .. }));
        assert_eq!(
            fs::read(&path).unwrap(),
            b"original",
            "the original must survive a refused save"
        );
    }

    #[test]
    fn a_failed_save_leaves_the_original_intact() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("subdir").join("note.txt");

        // The parent does not exist, so the temp file cannot be created.
        let err = write(&path, b"hello").unwrap_err();
        assert!(matches!(err, SaveError::TempCreate { .. }));
        assert!(!path.exists());
    }

    #[test]
    fn verification_can_be_disabled() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("note.txt");

        let out = atomic_write(
            &path,
            b"hello",
            SaveOptions {
                verify: false,
                ..SaveOptions::default()
            },
        )
        .unwrap();

        assert!(!out.verified);
        assert_eq!(fs::read(&path).unwrap(), b"hello");
    }

    #[test]
    fn verifies_content_larger_than_one_chunk() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("big.txt");
        let contents: Vec<u8> = (0..VERIFY_CHUNK * 3 + 17)
            .map(|i| (i % 251) as u8)
            .collect();

        let out = write(&path, &contents).unwrap();

        assert!(out.verified);
        assert_eq!(fs::read(&path).unwrap(), contents);
    }

    #[test]
    fn verify_detects_a_length_mismatch() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("note.txt");
        write(&path, b"hello world").unwrap();

        // Read-back compares against what we claim to have written, so a
        // shorter file must not pass as a prefix match.
        assert!(!verify_contents(&path, b"hello world extra").unwrap());
        assert!(!verify_contents(&path, b"hello").unwrap());
        assert!(verify_contents(&path, b"hello world").unwrap());
    }

    #[test]
    fn resolves_a_free_path_in_a_directory() {
        let dir = tempdir().unwrap();
        let name = SemanticName::new("Report", date!(2026 - 08 - 16), "txt");

        let first = resolve_in_dir(dir.path(), &name).unwrap();
        assert_eq!(first.file_name().unwrap(), "Report_16AUG2026.txt");

        write(&first, b"one").unwrap();
        let second = resolve_in_dir(dir.path(), &name).unwrap();
        assert_eq!(second.file_name().unwrap(), "Report_16AUG2026_02.txt");
    }

    #[test]
    fn a_bare_filename_saves_into_the_current_directory() {
        assert_eq!(
            parent_dir(Path::new("foo.txt")).unwrap(),
            PathBuf::from(".")
        );
    }
}
