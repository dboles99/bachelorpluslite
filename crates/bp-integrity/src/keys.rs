//! Key files: the moment this crate can leak.
//!
//! A signing key is a 32-byte seed. Everything else here -- signatures,
//! manifests, verdicts -- is public by construction and safe to lose. This
//! module is the one place where a mistake means somebody else can sign in
//! the user's name, and it is written on that assumption.
//!
//! ## The encodings, which are `bp-crypto`'s and are stable
//!
//! * a **signing key file** is exactly the 32 raw seed bytes -- no header, no
//!   newline, no hex;
//! * a **verifying key file** is the key in lowercase hex with a trailing
//!   newline.
//!
//! The asymmetry is deliberate. Hex would double the number of copies of the
//! secret in memory and leave a `String` allocation that nobody wipes, which
//! is exactly why `bp-crypto` refuses to give [`SigningKey`] a `to_hex` at
//! all; the raw form has one buffer and it is a `Zeroizing` one. A verifying
//! key has the opposite requirement -- it exists to be pasted into an email
//! -- so it gets the text form, and reading it tolerates the whitespace that
//! survives that journey.
//!
//! A raw seed file has no magic bytes, so nothing but its length says it is a
//! key. That is the price of matching RFC 8032 rather than inventing a
//! wrapper, and it is the same trade `bp-crypto` documents: a key this
//! product writes is readable by every other Ed25519 tool, and a key from one
//! of them is readable here.
//!
//! ## What "protected" means, per platform
//!
//! ADR-0001 makes Windows and Linux both first-class, and they are not
//! equally protectable from safe Rust. [`KeyFileProtection`] is the value
//! that says which one the caller got, so that a user interface can tell the
//! truth instead of showing a padlock everywhere. See its variants for the
//! full list of what is and is not enforced.
//!
//! One limit applies to both: **nothing here protects the key in memory once
//! it is loaded.** It is held in `bp-crypto`'s zeroizing types, so it is
//! wiped on drop, but pinning it out of the swap file needs `mlock`/
//! `VirtualLock` and therefore `unsafe`, which this crate forbids. A key read
//! from disk can reach the page file. Nor is a key file's storage shredded
//! when it is replaced: on a journalling, copy-on-write or flash-backed
//! filesystem the previous seed's blocks may outlive the rewrite, so a key
//! believed to be rotated may still be recoverable from the media.

use std::path::Path;

use bp_crypto::{SigningKey, VerifyingKey};
use zeroize::Zeroizing;

use crate::atomic::{Restrict, write_atomically};
use crate::error::IntegrityError;

/// How well the operating system is actually protecting a key file.
///
/// Returned rather than assumed, and returned even on success, because the
/// honest answer differs by platform and a product that says "your key is
/// safe" on a platform where it did not narrow anything is lying. The user
/// can act on this: move the key to a per-user directory, fix the mode, or
/// accept the risk knowingly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyFileProtection {
    /// **Unix.** The file is mode `0600` -- owner read/write, nothing for
    /// group or other -- and was set that way before the first secret byte
    /// was written, so there is no instant at which it held the key and was
    /// readable by anyone else.
    ///
    /// Still not enforced, even here: the containing *directory*'s
    /// permissions (a world-writable directory lets anyone replace the key
    /// file, though not read it); POSIX ACLs, which can widen access past
    /// what the mode bits show; `root`, which reads everything; and
    /// filesystems that do not carry Unix modes at all -- an exFAT stick, an
    /// NTFS or SMB mount with a fixed `fmask`, a Windows drive mounted under
    /// WSL -- where the mode is quietly ignored and this variant will still
    /// be reported because the call to set it succeeded.
    OwnerOnly,

    /// **Unix.** The file exists and its mode lets somebody other than the
    /// owner read it. Only ever reported by [`key_file_protection`] about a
    /// file this crate did not write -- one restored from a tar archive,
    /// copied off a stick, or checked out of a repository, all of which lose
    /// or flatten the mode. The mode is included so the message can be
    /// specific instead of vague.
    ReadableByOthers { mode: u32 },

    /// **Windows.** The file's access control list is whatever it inherited
    /// from the directory it was created in, and this crate did not narrow
    /// it.
    ///
    /// It cannot: `std::fs::Permissions` on Windows exposes exactly one bit,
    /// the read-only attribute, which controls *writing* rather than reading
    /// and would additionally break the atomic rename that replaces an
    /// existing key file. Narrowing the DACL properly means
    /// `SetNamedSecurityInfo` or a `SECURITY_ATTRIBUTES` on create, both of
    /// which are Win32 calls, and this crate is `#![forbid(unsafe_code)]`.
    ///
    /// In practice inheritance is often adequate and sometimes not, and the
    /// difference is the directory, which is why the caller is told rather
    /// than reassured. Under `%LOCALAPPDATA%` or the user's profile the
    /// inherited ACL usually grants that user, `SYSTEM` and the local
    /// `Administrators` group -- so other standard users cannot read the key,
    /// but any local administrator can. Under a drive root, a shared folder,
    /// a network share or a synchronised folder it can be far wider, and a
    /// synchronised folder additionally copies the key somewhere else
    /// entirely.
    ///
    /// There is no umask on Windows and no equivalent fallback, so the only
    /// honest mitigation this crate can offer is to say so here.
    InheritedFromDirectory,
}

impl KeyFileProtection {
    /// Whether the platform *confirmed* that only the owner can read the
    /// file.
    ///
    /// False for [`Self::InheritedFromDirectory`], which is not the same as
    /// "insecure" -- it means unknown, and a security indicator that treats
    /// unknown as safe is the one that gets somebody's key copied.
    #[must_use]
    pub const fn is_confirmed_private(self) -> bool {
        matches!(self, Self::OwnerOnly)
    }

    /// A sentence for the user, matching what the variant actually promises.
    #[must_use]
    pub fn describe(self) -> String {
        match self {
            Self::OwnerOnly => "Only your account can read this key file (mode 0600).".to_owned(),
            Self::ReadableByOthers { mode } => format!(
                "This key file is readable by other users (mode {mode:04o}). \
                 Restrict it with: chmod 600"
            ),
            Self::InheritedFromDirectory => {
                "This key file uses the permissions of the folder it is in. Windows permissions \
                 cannot be narrowed by this application, so keep the key in a folder only you \
                 can read -- your profile or AppData, not a shared or synchronised folder."
                    .to_owned()
            }
        }
    }
}

/// Write a signing key to `path`, as tightly as the platform allows.
///
/// Returns what protection was actually achieved, rather than `()`, because
/// on one of the two platforms this crate supports the answer is "none of my
/// doing" and a caller that never sees that will never tell the user.
///
/// The write is atomic and, unlike `bp-files`, does **not** inherit the
/// permissions of any key file it replaces: a key that arrived
/// world-readable must not stay world-readable simply because it was rotated
/// in place.
pub fn write_signing_key(
    path: &Path,
    key: &SigningKey,
) -> Result<KeyFileProtection, IntegrityError> {
    // `Zeroizing` all the way to the syscall: every early return between here
    // and the end -- including the I/O failures -- has to leave nothing
    // behind.
    let seed = key.to_bytes();
    write_atomically(path, seed.as_ref(), Restrict::Yes).map_err(IntegrityError::write(path))?;
    Ok(freshly_written_protection())
}

/// Read a signing key back.
///
/// The bytes land in a `Zeroizing` buffer that is wiped when this returns,
/// however it returns; the key itself then lives in `bp-crypto`'s type, which
/// wipes itself on drop.
///
/// A file of the wrong length is [`IntegrityError::NotAKeyFile`] rather than
/// a silent pad or truncate. That matters more here than anywhere else in
/// this crate: a key read one byte short signs perfectly well and matches
/// nothing anyone has ever verified against, and the failure only surfaces at
/// the recipient.
///
/// Says nothing about the file's permissions. Ask [`key_file_protection`] for
/// that, separately, because a key that is loose on disk is still the right
/// key and refusing to load it would leave the user unable to fix anything.
pub fn read_signing_key(path: &Path) -> Result<SigningKey, IntegrityError> {
    let bytes = Zeroizing::new(std::fs::read(path).map_err(IntegrityError::read(path))?);
    SigningKey::from_bytes(&bytes).map_err(|source| IntegrityError::NotAKeyFile {
        path: path.to_owned(),
        source,
    })
}

/// Write a verifying key to `path` as hex text.
///
/// Deliberately *not* narrowed -- on Unix it is widened to `0644`. A public
/// key that only its owner can read is a public key that cannot do its job,
/// and the temporary file the atomic write goes through is created `0600`, so
/// leaving it alone would produce exactly that. The atomic write is still
/// used, so the file is never observed half-written by whatever is about to
/// email it.
pub fn write_verifying_key(path: &Path, key: &VerifyingKey) -> Result<(), IntegrityError> {
    let text = format!("{}\n", key.to_hex());
    write_atomically(path, text.as_bytes(), Restrict::No).map_err(IntegrityError::write(path))
}

/// Read a verifying key from a hex file.
///
/// Tolerant of surrounding whitespace and line wrapping, because the
/// realistic input is a key that was pasted out of an email and saved, and
/// refusing that teaches users to retype keys by hand -- which is how a key
/// acquires a wrong digit.
pub fn read_verifying_key(path: &Path) -> Result<VerifyingKey, IntegrityError> {
    let text = std::fs::read_to_string(path).map_err(IntegrityError::read(path))?;
    VerifyingKey::from_hex(&text).map_err(|source| IntegrityError::NotAKeyFile {
        path: path.to_owned(),
        source,
    })
}

/// Report how well an existing key file is protected right now.
///
/// Separate from reading the key so that a caller can check a key file it did
/// not write -- one copied from a backup or a colleague, which is exactly the
/// case where the mode has been flattened -- and warn before the key is ever
/// used.
pub fn key_file_protection(path: &Path) -> Result<KeyFileProtection, IntegrityError> {
    let metadata = std::fs::metadata(path).map_err(IntegrityError::read(path))?;
    Ok(protection_of(&metadata))
}

/// What a file this crate has just written with [`Restrict::Yes`] is
/// protected by.
#[cfg(unix)]
const fn freshly_written_protection() -> KeyFileProtection {
    KeyFileProtection::OwnerOnly
}

/// On Windows nothing was narrowed, and saying `OwnerOnly` here would be the
/// single most misleading line in the crate.
#[cfg(not(unix))]
const fn freshly_written_protection() -> KeyFileProtection {
    KeyFileProtection::InheritedFromDirectory
}

/// Read the protection out of a file's metadata.
///
/// Only the group and other read/write/execute bits are consulted: the owner
/// bits and the file type say nothing about who else can read it.
#[cfg(unix)]
fn protection_of(metadata: &std::fs::Metadata) -> KeyFileProtection {
    use std::os::unix::fs::PermissionsExt as _;
    let mode = metadata.permissions().mode() & 0o777;
    if mode & 0o077 == 0 {
        KeyFileProtection::OwnerOnly
    } else {
        KeyFileProtection::ReadableByOthers { mode }
    }
}

/// Windows metadata carries no mode to inspect, so the honest answer is the
/// same one [`write_signing_key`] gives.
#[cfg(not(unix))]
fn protection_of(_metadata: &std::fs::Metadata) -> KeyFileProtection {
    KeyFileProtection::InheritedFromDirectory
}

#[cfg(test)]
mod tests;
