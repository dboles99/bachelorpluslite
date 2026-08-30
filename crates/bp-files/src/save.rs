//! Atomic save.

use std::borrow::Cow;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use bp_naming::SemanticName;
use bp_platform::{
    Platform,
    paths::{
        PathProblem, file_name, file_name_problems, needs_extended_length_prefix,
        to_extended_length,
    },
};
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
    #[error(
        "cannot save to {} -- {reserved} is a Windows device rather than a file, so writing \
         there would send the document to the device and leave nothing on disk; choose \
         another name",
        .path.display()
    )]
    ReservedDeviceName {
        path: PathBuf,
        /// The canonical spelling from `bp-platform`, so the message names
        /// the device without echoing the user's casing back at them.
        reserved: &'static str,
    },

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

    /// The file on disk is marked read-only, and we know it rather than
    /// guessing.
    ///
    /// **Separate from [`SaveError::Replace`] because the way out differs.**
    /// Replace lists three candidate causes and hedges with "may be", which is
    /// honest when the cause is unknown and needlessly vague when it is not:
    /// at the moment a replace fails we can stat the file and read the
    /// attribute. Where the answer is yes, this says so and names Save As.
    ///
    /// Only raised when the attribute is *set*. On Unix
    /// `Permissions::readonly()` reports "no write bit for anyone", which is a
    /// different question from "may this process write it" -- a file owned by
    /// somebody else with mode 644 answers `false` and still refuses us. That
    /// case keeps `Replace`'s hedge, because there the hedge is true.
    #[error(
        "cannot save {} -- the file is marked read-only. Use Save As to write a \
         copy somewhere else, or clear the read-only attribute and try again",
        .path.display()
    )]
    ReadOnlyFile { path: PathBuf },

    #[error(
        "cannot replace {} -- it may be read-only, open in another program, or in a \
         directory that no longer exists: {source}",
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

/// Whether `path` carries the read-only attribute.
///
/// `false` for a path that cannot be stat'ed at all: this refines an error
/// message and must never invent a cause. A file that vanished between the
/// failed replace and this call is a `Replace`, which is what it was.
fn is_marked_read_only(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|meta| meta.permissions().readonly())
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
    refuse_device_name(Platform::HOST, path)?;

    // Everything below writes through `target`; `path` stays the caller's own
    // and is what every error names.
    let target = writable_path(Platform::HOST, path);
    let target = target.as_ref();

    let parent = parent_dir(target)?;
    let replaced_existing = target.exists();

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

    preserve_permissions(target, &temp);

    // The atomic step. Everything before this touched only the temp file, so
    // any earlier failure leaves the user's file exactly as it was.
    let _persisted = match options.overwrite {
        Overwrite::Replace => temp.persist(target).map_err(|e| {
            // Asked only once the write has already failed, so the ordinary
            // path pays nothing for it -- and asked of the *target* now rather
            // than remembered from the open, because the attribute can be set
            // while a document is open and a remembered answer would be wrong
            // in exactly that case.
            if is_marked_read_only(target) {
                SaveError::ReadOnlyFile {
                    path: path.to_owned(),
                }
            } else {
                SaveError::Replace {
                    path: path.to_owned(),
                    source: e.error,
                }
            }
        })?,
        Overwrite::FailIfExists => {
            temp.persist_noclobber(target)
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
        if !verify_contents(target, contents).map_err(|source| SaveError::Verify {
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

/// The path to actually hand the filesystem.
///
/// On Windows a path past `MAX_PATH` fails every call made against it with
/// `os error 3` unless it carries the `\\?\` prefix -- and this module then
/// reported that as [`SaveError::Replace`], which blames a read-only file or
/// another program and sends the user to look at the wrong thing entirely.
/// The name was never the problem: `SemanticName::to_filename` promises
/// names of up to 255 bytes and they are legal by every rule `bp-platform`
/// states. It is the *path* that is too long, and long titles are what a
/// semantic filename scheme is for.
///
/// Applied only where it is needed. The prefix means "pass this to the object
/// manager verbatim", which costs every normalisation Win32 usually does --
/// `..` stops meaning a parent, a forward slash stops being a separator --
/// so a path that fits without it is left exactly as the caller wrote it.
///
/// The original path is what any error carries. A user who typed one path and
/// is shown another, with four punctuation marks bolted to the front, has been
/// told about a machine rather than about their file.
fn writable_path(platform: Platform, path: &Path) -> Cow<'_, Path> {
    if platform != Platform::Windows {
        return Cow::Borrowed(path);
    }
    let text = path.to_string_lossy().into_owned();
    if !needs_extended_length_prefix(&text) {
        return Cow::Borrowed(path);
    }

    // The conversion refuses a relative path, because the working directory is
    // not applied to an extended-length one -- so a relative path has to be
    // made absolute first, and *that* is where the platform parameter earns
    // its keep. `std::path::absolute` resolves against the host's working
    // directory and by the host's rules, so calling it unconditionally would
    // make this function answer about the host however it was called: on the
    // Linux leg `C:\docs\...` is a *relative* path, and the Windows rule would
    // be tested by prepending a Linux working directory to it.
    //
    // **This is the third time that trap has been sprung in this repository**
    // -- `PathBuf::join` standing in for `paths::join` (4390593), a device
    // name split with `Path::file_name` (d3c2040), and now this. The rule is
    // in `project/NEXT_SESSION.md`: a function taking a `Platform` must not
    // let `std::path` answer for it.
    //
    // So a path already absolute *for that platform* needs no working
    // directory and is converted directly; a relative one is resolved only
    // when the platform asked about is the one running.
    let absolute = if bp_platform::paths::is_absolute(platform, &text) {
        text.clone()
    } else if platform == Platform::HOST {
        // On Windows this also resolves `.` and `..` lexically, which the
        // object manager would not -- so a path carrying one is rescued here
        // rather than refused.
        std::path::absolute(path)
            .map_or_else(|_| text.clone(), |p| p.to_string_lossy().into_owned())
    } else {
        return Cow::Borrowed(path);
    };

    // Falls back to the path as given if it cannot be rewritten at all. That
    // leaves the operating system to say what is wrong with it, which is
    // better than this function inventing a diagnosis: the shapes that reach
    // here and cannot be converted are malformed rather than long, and a
    // "path too long" error would be a confident answer to the wrong question.
    to_extended_length(&absolute)
        .map_or(Cow::Borrowed(path), |long| Cow::Owned(PathBuf::from(long)))
}

/// Refuse a path whose file name is one of Win32's device names./// Refuse a path whose file name is one of Win32's device names.
///
/// This is the only failure in this module that is checked rather than
/// attempted, and the reason is that attempting it does not fail. Opening
/// `con.txt`, `aux.log` or `NUL.dat` on Windows *succeeds* -- it opens the
/// device -- so the write returns `Ok`, the read-back verification reads the
/// console back, and the status bar says the document is safe while nothing
/// was ever written to disk. Every other error here is the filesystem saying
/// no; this one is the filesystem saying yes to the wrong thing.
///
/// `bp-naming` cannot cover it. It sanitises the names *this program*
/// generates, and the name reaching here may equally have been typed into a
/// Save dialog, which no crate had ever checked.
///
/// The platform is a parameter rather than a `cfg`, so both CI legs execute
/// both rule sets: the Windows rule is not one that only a Windows run can
/// test. `bp-platform` owns both the device list and the fact that it is a
/// Windows rule -- `file_name_problems` reports `ReservedName` on no other
/// platform, which is why `Platform::Linux` needs no arm here.
///
/// Only `ReservedName` is refused, and not the rest of what
/// `file_name_problems` can report. The others -- a forbidden character, a
/// trailing dot, an over-long component -- all end in the filesystem refusing
/// the write, which is a loud failure this function would only be duplicating.
/// This one ends in silence.
fn refuse_device_name(platform: Platform, path: &Path) -> Result<(), SaveError> {
    // `bp_platform::paths::file_name` and deliberately not
    // `Path::file_name`, which splits by the rules of the platform this
    // binary was *compiled* for: on the Linux leg that reads
    // `C:\Users\me\con.txt` as one long file name, finds no device, and
    // leaves the Windows rule untested by half of CI while appearing to
    // pass. It is the same function with the platform made explicit, so the
    // split and the judgement answer about the same platform.
    //
    // Lossy rather than `to_str`, so a name Windows accepts but Rust cannot
    // represent as UTF-8 is still checked. Replacement characters can only be
    // added, never removed, so a lossy name that reads as a device is one
    // that was a device.
    let path_text = path.to_string_lossy();
    let Some(name) = file_name(platform, &path_text) else {
        return Ok(());
    };

    match file_name_problems(platform, name)
        .into_iter()
        .find(|p| matches!(p, PathProblem::ReservedName { .. }))
    {
        Some(PathProblem::ReservedName { reserved, .. }) => Err(SaveError::ReservedDeviceName {
            path: path.to_owned(),
            reserved,
        }),
        _ => Ok(()),
    }
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

    // --- device names ----------------------------------------------------

    /// The device a refusal names, or `None` if the path is allowed.
    fn refusal(platform: Platform, path: &str) -> Option<&'static str> {
        match refuse_device_name(platform, Path::new(path)) {
            Ok(()) => None,
            Err(SaveError::ReservedDeviceName { reserved, .. }) => Some(reserved),
            Err(other) => panic!("wrong refusal for {path:?}: {other}"),
        }
    }

    #[test]
    fn windows_device_names_are_refused_whatever_follows_them() {
        // The stem is what Win32 looks at, so an extension saves none of
        // these -- which is exactly why attempting the write is no test.
        for path in [
            "con",
            "CON.txt",
            "con.tar.gz",
            "aux.log",
            "NUL.dat",
            "lpt1.bak",
            "prn.2026",
            "CONIN$.notes",
            "conout$",
        ] {
            assert!(
                refusal(Platform::Windows, path).is_some(),
                "{path:?} was allowed on Windows"
            );
        }
    }

    #[test]
    fn a_device_name_is_refused_wherever_in_the_tree_it_sits() {
        // A device is a device in every directory, so the check is on the
        // last component and not on how the user got to it.
        //
        // **The backslash path is the one that matters**, and it failed on the
        // Linux leg first. `std::path` splits by the rules of the platform it
        // was compiled for, so a Linux build reads the whole Windows path as
        // one file name and finds no device -- the Windows rule silently
        // untested by half of CI. The split has to take the platform the same
        // way the judgement does.
        for path in [
            r"C:\Users\someone\Documents\con.txt",
            "C:/Users/someone/Documents/con.txt",
            "/home/someone/notes/aux.log",
            "./nul",
        ] {
            assert!(
                refusal(Platform::Windows, path).is_some(),
                "{path:?} was allowed on Windows"
            );
        }
    }

    #[test]
    fn a_backslash_path_is_one_component_on_linux_and_several_on_windows() {
        // The asymmetry `bp_platform::paths::components` documents, asserted
        // here because this function is where getting it wrong is expensive.
        // On Linux a backslash is an ordinary filename character, so this
        // whole string is one name -- and one name that is not a device.
        let windows_path = r"C:\Users\someone\con.txt";
        assert_eq!(refusal(Platform::Windows, windows_path), Some("CON"));
        assert_eq!(refusal(Platform::Linux, windows_path), None);
    }

    #[test]
    fn the_refusal_names_the_device_in_its_canonical_spelling() {
        // The message says CON, not the user's `con.txt`, so it reads as a
        // statement about the system rather than about their typing.
        assert_eq!(refusal(Platform::Windows, "con.txt"), Some("CON"));
        assert_eq!(refusal(Platform::Windows, "LpT1.bak"), Some("LPT1"));
    }

    #[test]
    fn ordinary_names_are_allowed_on_both_platforms() {
        // An over-eager fix is as bad as no fix: every one of these is a file.
        for path in [
            "CONTENTS.md",
            "console.log",
            "printer.cfg",
            "Report_16AUG2026.txt",
            "AUXILIARY",
        ] {
            for &platform in Platform::ALL {
                assert_eq!(
                    refusal(platform, path),
                    None,
                    "{path:?} was refused on {}",
                    platform.token()
                );
            }
        }
    }

    #[test]
    fn linux_has_no_device_names_and_may_save_all_of_them() {
        // `con.txt` is an ordinary file on Linux, and refusing it there would
        // be this fix doing harm. The platform is a parameter for that reason
        // as much as for running the Windows rule on the Linux leg.
        for path in ["con", "CON.txt", "aux.log", "NUL.dat", "lpt1.bak"] {
            assert_eq!(refusal(Platform::Linux, path), None, "{path:?}");
        }
    }

    #[test]
    fn a_path_with_no_file_name_is_not_refused_here() {
        // Nothing to judge. It fails later, on its own terms.
        assert_eq!(refusal(Platform::Windows, ".."), None);
        assert_eq!(refusal(Platform::Windows, "/"), None);
    }

    #[test]
    fn no_error_message_carries_a_run_of_spaces() {
        // A wrapped message in this module is one string joined by `\` line
        // continuations, and a continuation lost in an edit leaves the next
        // line's indentation *inside* the sentence -- ten spaces in the middle
        // of something the user reads. Two of these shipped that way, because
        // every test that looked at a message used `contains` on a fragment
        // and a fragment does not span the join.
        //
        // Built by hand rather than reflected over, because `thiserror`'s
        // formatting is what produces the final string and only a real value
        // goes through it.
        let path = PathBuf::from("notes.txt");
        let io_error = || io::Error::new(io::ErrorKind::PermissionDenied, "denied");
        let messages = [
            SaveError::ReservedDeviceName {
                path: path.clone(),
                reserved: "CON",
            }
            .to_string(),
            SaveError::NoParentDirectory { path: path.clone() }.to_string(),
            SaveError::TempCreate {
                path: path.clone(),
                source: io_error(),
            }
            .to_string(),
            SaveError::Write {
                path: path.clone(),
                source: io_error(),
            }
            .to_string(),
            SaveError::Flush {
                path: path.clone(),
                source: io_error(),
            }
            .to_string(),
            SaveError::AlreadyExists { path: path.clone() }.to_string(),
            SaveError::Replace {
                path: path.clone(),
                source: io_error(),
            }
            .to_string(),
            SaveError::Verify {
                path: path.clone(),
                source: io_error(),
            }
            .to_string(),
            SaveError::VerificationFailed { path }.to_string(),
        ];

        for message in messages {
            assert!(
                !message.contains("  "),
                "a run of spaces in a message the user reads: {message:?}"
            );
        }
    }

    // --- long paths --------------------------------------------------------

    #[test]
    fn a_short_path_is_handed_to_the_filesystem_exactly_as_written() {
        // The prefix costs every normalisation Win32 usually does -- `..`
        // stops meaning a parent, a forward slash stops being a separator --
        // so a path that fits without it must come through untouched.
        let short = Path::new(r"C:\docs\notes.txt");
        for platform in [Platform::Windows, Platform::Linux] {
            assert_eq!(
                writable_path(platform, short),
                Cow::Borrowed(short),
                "a short path was rewritten on {}",
                platform.token()
            );
        }
    }

    #[test]
    fn a_windows_path_past_the_limit_gains_the_prefix() {
        let long = format!(r"C:\docs\{}\notes.txt", "d".repeat(300));
        let rewritten = writable_path(Platform::Windows, Path::new(&long));
        let text = rewritten.to_string_lossy();
        assert!(
            text.starts_with(r"\\?\"),
            "a path past MAX_PATH must gain the escape hatch: {text}"
        );
        assert!(
            text.ends_with("notes.txt"),
            "the rewrite must not lose the name: {text}"
        );
    }

    #[test]
    fn a_windows_path_is_judged_by_windows_rules_on_either_leg() {
        // The trap this function was written into before it was written out
        // of: `std::path::absolute` resolves by the *host's* rules, so on the
        // Linux leg `C:\docs\...` is a relative path and the Windows rule
        // would have been tested against a Linux working directory. A path
        // already absolute for the platform asked about needs no working
        // directory at all, which is what makes this answer the same on both
        // legs -- and the Linux leg is where it first came out wrong.
        let long = format!(r"C:\docs\{}\notes.txt", "d".repeat(300));
        let text = writable_path(Platform::Windows, Path::new(&long))
            .to_string_lossy()
            .into_owned();
        assert!(
            text.starts_with(r"\\?\C:\docs\"),
            "Windows and Linux must agree here, and this is not the agreed answer: {text}"
        );
    }

    #[test]
    fn linux_never_gains_the_prefix_however_long_the_path() {
        // There is no such limit there, and an extended-length path on Linux
        // is a directory literally called `?`. The platform is a parameter
        // here for the same reason it is everywhere else in this module.
        let long = format!("/home/me/{}/notes.txt", "d".repeat(300));
        assert_eq!(
            writable_path(Platform::Linux, Path::new(&long)),
            Cow::Borrowed(Path::new(&long))
        );
    }

    #[test]
    fn a_long_absolute_path_containing_a_parent_component_is_left_alone() {
        // The documented limit of this rewrite, pinned so it is a decision
        // rather than a surprise. An extended-length path may not contain
        // `..`, because the object manager does not resolve one -- and this
        // function will not resolve it either, because doing so lexically is
        // a rule about a *platform*, and borrowing the host's resolver for it
        // is exactly the mistake the parameter exists to prevent.
        //
        // So the path goes over unchanged and the system says what is wrong
        // with it. A save into a directory both past MAX_PATH *and* reached
        // through `..` therefore still fails -- a corner of a corner, and a
        // lexical normaliser in `bp-platform` is what would close it.
        let long = format!(r"C:\docs\sub\..\{}\notes.txt", "d".repeat(300));
        let path = Path::new(&long);
        assert_eq!(
            writable_path(Platform::Windows, path),
            Cow::Borrowed(path),
            "a path that cannot be expressed must be passed through, not guessed at"
        );
    }

    #[test]
    fn atomic_write_refuses_or_allows_a_device_name_as_the_host_requires() {
        // The seam between the rule and its caller, asserted without a `cfg`:
        // each leg checks its own answer and both run the same test.
        let dir = tempdir().unwrap();
        let path = dir.path().join("con.txt");
        let result = atomic_write(&path, b"body", SaveOptions::default());

        match Platform::HOST {
            Platform::Windows => {
                let Err(SaveError::ReservedDeviceName { reserved, .. }) = result else {
                    panic!("Windows must refuse a device name, got {result:?}");
                };
                assert_eq!(reserved, "CON");
                assert!(!path.exists(), "nothing may be created for a refused name");
            }
            Platform::Linux => {
                result.expect("con.txt is an ordinary file on Linux");
                assert_eq!(std::fs::read(&path).unwrap(), b"body");
            }
        }
    }

    /// Mark `path` read-only, the way each platform means it.
    ///
    /// `Permissions::set_readonly(true)` is the portable call and does the
    /// right thing on both: the read-only attribute on Windows, and clearing
    /// every write bit on Unix.
    fn make_read_only(path: &Path) {
        let mut perms = std::fs::metadata(path).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(path, perms).unwrap();
    }

    #[test]
    fn a_read_only_file_is_refused_by_name_rather_than_by_guess() {
        // **The whole of what read-only means in this product** (ADR-0066).
        // `bp_buffer::Access` claimed to be the mechanism and was never once
        // set by anything; the real answer has always been the save refusing,
        // and until now it refused with a message listing three candidate
        // causes and hedging with "may be". At the moment a replace fails we
        // can stat the file, so where the attribute is set we say so.
        let dir = tempdir().unwrap();
        let path = dir.path().join("locked.txt");
        std::fs::write(&path, b"original").unwrap();
        make_read_only(&path);

        let result = atomic_write(&path, b"replacement", SaveOptions::default());

        // Cleared before any assertion can panic, or the temporary directory
        // cannot be removed on Windows.
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        std::fs::set_permissions(&path, perms).unwrap();

        match result {
            Err(SaveError::ReadOnlyFile { path: named }) => {
                assert_eq!(named, path, "the refusal must name the file");
                let said = SaveError::ReadOnlyFile { path: named }.to_string();
                assert!(
                    said.contains("read-only") && said.contains("Save As"),
                    "a refusal has to say what is wrong and what to do: {said}"
                );
            }
            // Unix permits replacing a read-only file when its *directory* is
            // writable -- the rename does not open the file at all -- so the
            // save legitimately succeeds there. Asserted rather than skipped,
            // because "it succeeded" and "the test did not run" look the same
            // in a green log.
            Ok(_) => {
                assert_eq!(Platform::HOST, Platform::Linux, "{result:?}");
                assert_eq!(std::fs::read(&path).unwrap(), b"replacement");
            }
            other => panic!("expected a named refusal or a successful replace, got {other:?}"),
        }

        assert!(path.exists(), "the original must still be there either way");
    }
}
