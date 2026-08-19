//! Create, flush, rename -- the same discipline `bp-files` applies to
//! documents, applied here to the small artefacts this crate writes.
//!
//! ## Why this is not a call into `bp-files`
//!
//! `bp-files::atomic_write` is the right tool for a document and the wrong
//! one for a key, in two specific ways:
//!
//! * it *preserves* the permissions of the file it replaces, which is
//!   correct for a note the user chmod'ed themselves and exactly wrong for a
//!   private key -- a key file that arrived in a tarball as world-readable
//!   would keep being world-readable every time it was rewritten;
//! * it reads the file back into a plain buffer to verify the save, which
//!   for a 32-byte secret means a second unwiped copy of it in memory for the
//!   sake of confirming 32 bytes.
//!
//! So the mechanism is copied and the policy is not. Everything that makes
//! the write atomic is identical: the temporary file is created in the
//! *same* directory as the target, so the final rename is a rename and not a
//! cross-filesystem copy.

use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use tempfile::NamedTempFile;

/// How much of the world should be able to read the file being written.
///
/// A parameter rather than an inference from the file's name, so that the
/// decision is visible at every call site: three of this crate's four
/// artefacts are meant to be public, and one is a private key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Restrict {
    /// A signature, a manifest, a public key. These exist to be copied
    /// around, so on Unix they are widened to `0644`.
    ///
    /// Widened, not left alone, because the temporary file underneath is
    /// created by `mkstemp` and is therefore `0600`: doing nothing would
    /// quietly write a *public* key that only its owner can read, and the
    /// user would discover it when a colleague could not verify their
    /// document. The process umask is deliberately not consulted -- reading
    /// it needs `libc` and this crate takes no third-party dependency for it
    /// -- which is acceptable precisely because everything written this way
    /// is public by construction.
    No,
    /// A private key. Narrowed as far as the platform allows *before* any
    /// secret byte is written, which is the only ordering with no window in
    /// which the file exists, contains the key, and is readable.
    Yes,
}

/// Write `bytes` to `path`, or leave whatever was there untouched.
///
/// The caller gets the raw [`io::Error`] rather than a crate error because
/// only the caller knows which of its several files this path was.
pub(crate) fn write_atomically(path: &Path, bytes: &[u8], restrict: Restrict) -> io::Result<()> {
    let parent = parent_dir(path);
    let mut temp = NamedTempFile::new_in(&parent)?;

    // Before `write_all`, deliberately. `tempfile` already creates with
    // owner-only permissions on Unix, but relying on that would make our
    // guarantee a property of somebody else's crate that could change in a
    // patch release -- and the public case has to be widened from it anyway.
    set_mode(temp.as_file(), restrict)?;

    temp.write_all(bytes)?;
    temp.flush()?;
    // Past the OS cache, not merely past Rust's buffer: a signature that is
    // only in a write-back cache is a signature that a power cut turns into
    // an empty file next to an intact document.
    temp.as_file().sync_all()?;

    temp.persist(path).map_err(|e| e.error)?;
    sync_directory(&parent);
    Ok(())
}

/// The directory to create the temporary file in.
///
/// A bare `key.bpkey` has a parent of `""`, which is not a directory any
/// syscall accepts, so it means the current one.
fn parent_dir(path: &Path) -> PathBuf {
    match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_owned(),
        _ => PathBuf::from("."),
    }
}

/// `0600` for a secret, `0644` for everything else.
///
/// Set through `std::os::unix`, so `#![forbid(unsafe_code)]` still holds.
#[cfg(unix)]
fn set_mode(file: &File, restrict: Restrict) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    let mode = match restrict {
        Restrict::Yes => 0o600,
        Restrict::No => 0o644,
    };
    file.set_permissions(std::fs::Permissions::from_mode(mode))
}

/// Nothing, on Windows, and this crate says so out loud rather than
/// pretending.
///
/// `std::fs::Permissions` on Windows exposes exactly one bit -- the readonly
/// attribute -- which controls *writing*, not reading, and would additionally
/// break the rename that replaces an existing key file. Narrowing the ACL
/// instead needs `SetNamedSecurityInfo`, which means a Win32 binding and
/// `unsafe`, and this crate forbids `unsafe`. See
/// [`crate::KeyFileProtection`] for what the caller is told instead.
#[cfg(not(unix))]
#[allow(clippy::unnecessary_wraps)]
fn set_mode(_file: &File, _restrict: Restrict) -> io::Result<()> {
    Ok(())
}

/// Flush the directory entry, so the rename survives a crash too.
///
/// Ignored on failure: some filesystems legitimately refuse to sync a
/// directory, and a signature written without this is still a signature.
#[cfg(unix)]
fn sync_directory(dir: &Path) {
    if let Ok(handle) = File::open(dir) {
        let _ = handle.sync_all();
    }
}

/// Windows has no directory handle to sync; `MoveFileEx` orders the metadata
/// write itself.
#[cfg(not(unix))]
fn sync_directory(_dir: &Path) {}
