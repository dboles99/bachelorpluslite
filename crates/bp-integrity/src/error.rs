//! The one error type for everything this crate does to a filesystem.

use std::io;
use std::path::{Path, PathBuf};

use bp_crypto::SignError;

/// Something went wrong *reaching* a file, as opposed to something being
/// wrong *with* it.
///
/// The split is deliberate and it is the whole reason this type is small.
/// A signature that does not hold is not an error here -- it is a
/// [`crate::Verification`] verdict, because it is an answer the caller asked
/// for and must act on. An error is the case where no answer could be
/// obtained at all: the disk refused, the path was not a file, the key file
/// held something that is not a key. Folding the two together is how a
/// verification failure ends up in a `?` chain and gets logged instead of
/// shown.
#[derive(Debug, thiserror::Error)]
pub enum IntegrityError {
    /// The path names no file, so there is nothing to sign and nowhere to put
    /// a sidecar beside it. `..`, `/` and `C:\` all land here.
    #[error("{} does not name a file, so it cannot be signed or verified", .0.display())]
    NotAFilePath(PathBuf),

    #[error("cannot read {}: {source}", .path.display())]
    Read {
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

    /// The bytes in a key file are not a key. Separate from [`Self::Read`]
    /// because the file was perfectly readable: the user pointed us at the
    /// wrong file, or at a key that was truncated in transit, and only they
    /// can fix that.
    #[error("{} does not contain a usable key: {source}", .path.display())]
    NotAKeyFile {
        path: PathBuf,
        #[source]
        source: SignError,
    },

    /// A manifest is text, and this one was not text.
    #[error("{} is not valid UTF-8, so it is not a manifest this crate wrote", .path.display())]
    NotText { path: PathBuf },

    /// A manifest on disk did not parse. The reason travels with it because
    /// "malformed" alone sends a user to a hex editor.
    #[error("{} is not a valid manifest: {source}", .path.display())]
    MalformedManifestFile {
        path: PathBuf,
        #[source]
        source: crate::MalformedManifest,
    },

    /// A path that cannot be written into a manifest line, or read back out
    /// of one. Named separately from a parse failure because the caller
    /// caused this one and can choose a different path.
    #[error("{reason}: {path}")]
    UnusablePath { path: String, reason: &'static str },

    /// A sealed key file could not be opened: the wrong passphrase, or a file
    /// that is damaged.
    ///
    /// **Deliberately does not say which**, and that is `bp-crypto`'s
    /// decision showing through rather than vagueness here: an authenticated
    /// envelope cannot distinguish a wrong key from altered bytes, because
    /// the tag check fails identically for both. A message that guessed would
    /// be wrong half the time, and the half it got wrong is the half where
    /// somebody retypes a correct passphrase for ten minutes.
    #[error("cannot unlock {}: {source}", .path.display())]
    SealedKey {
        path: PathBuf,
        #[source]
        source: bp_crypto::CryptoError,
    },
}

impl IntegrityError {
    /// Wrap an I/O failure with the file it happened to.
    ///
    /// `io::Error` on its own says "permission denied" and not which of the
    /// four files involved denied it, which is the difference between an
    /// error a user can act on and one they report to us.
    pub(crate) fn read(path: &Path) -> impl FnOnce(io::Error) -> Self + use<'_> {
        |source| Self::Read {
            path: path.to_owned(),
            source,
        }
    }

    /// The writing half of [`Self::read`].
    pub(crate) fn write(path: &Path) -> impl FnOnce(io::Error) -> Self + use<'_> {
        |source| Self::Write {
            path: path.to_owned(),
            source,
        }
    }
}
