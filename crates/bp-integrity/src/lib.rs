//! Hashing, signing and verification as they touch a filesystem
//! (specs.md section 15).
//!
//! **This crate implements no cryptography and re-wraps none.** SHA-256,
//! SHA-512 and Ed25519 all come from `bp-crypto`, which composes them from
//! audited implementations and stops at the edge of its own memory. Every
//! primitive it exports -- [`hash_document`], [`sign_document`],
//! [`verify_document`], [`SigningKey`], [`VerifyingKey`], [`Signature`],
//! [`DocumentHash`] -- is re-exported here unchanged, so a caller needs one
//! dependency rather than two, and gets the identical types either way.
//!
//! What is here is the part `bp-crypto` deliberately left: the story of a
//! file on a disk.
//!
//! * **[`sign_file`] and [`verify_file`]** -- the `.sig` sidecar beside the
//!   document. `bp-crypto` hands back 64 detached bytes and has no opinion
//!   about where they live; this crate names the file, fixes what is in it,
//!   and writes it atomically. See [`sidecar`] for the format and the
//!   reasoning behind it.
//! * **[`Verification`]** -- the verdict, in the five shapes a user has to
//!   tell apart. A missing sidecar is one of them and it *fails*: an unsigned
//!   document is not a verified one, and a check that can be passed by
//!   deleting a file is not a check.
//! * **[`write_signing_key`] and friends** -- key files, which are where this
//!   crate can leak, and where Windows and Linux genuinely differ. See
//!   [`KeyFileProtection`] for exactly what is and is not enforced on each.
//! * **[`Manifest`]** -- digests over a set of files, checked per file,
//!   because a pass/fail over two hundred exhibits does not say which one
//!   moved.
//!
//! ## What this crate does not claim
//!
//! **Whose key it is.** Inherited from `bp-crypto` and worth repeating,
//! because this crate now puts a key *into a file* and a key in a file looks
//! like an identity. It is not one. A signature proves a document was
//! approved by whoever holds one specific key; binding that key to a person
//! needs something neither crate has.
//!
//! **That the set is complete.** A manifest speaks only for the files it
//! lists.
//!
//! **That a key on disk is safe from the machine's administrator.** On
//! neither platform is it.

#![forbid(unsafe_code)]

mod atomic;
mod error;
mod hex;
pub mod keys;
pub mod manifest;
pub mod sidecar;

pub use error::IntegrityError;
pub use keys::{
    KeyFileProtection, SEALED_KEY_EXTENSION, is_sealed_key_file, key_file_protection,
    read_sealed_signing_key, read_signing_key, read_verifying_key, write_sealed_signing_key,
    write_signing_key, write_verifying_key,
};
pub use manifest::{
    FileCheck, FileOutcome, MANIFEST_MAGIC, MalformedManifest, Manifest, ManifestEntry,
    ManifestReport, hash_file,
};
pub use sidecar::{
    Expectation, MalformedSidecar, SIDECAR_MAGIC, SIDECAR_SUFFIX, Sidecar, Verification,
    sidecar_path, sign_file, verify_file,
};

/// `bp-crypto`'s primitives, unchanged and unwrapped.
///
/// Re-exported rather than mirrored so that a `VerifyingKey` produced by this
/// crate is the same type as one produced by `bp-crypto` -- a newtype here
/// would force conversions at every boundary and would be a second place for
/// a key parser to live.
pub use bp_crypto::{
    DocumentHash, HashAlgorithm, SIGNATURE_LEN, SIGNING_KEY_LEN, SignError, Signature, SigningKey,
    VERIFYING_KEY_LEN, VerifyingKey, hash_document, sign_document, verify_document,
};

/// Crate identity used by workspace smoke tests and diagnostics.
pub const CRATE_NAME: &str = "bp-integrity";
