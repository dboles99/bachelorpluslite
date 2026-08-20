//! Shared helpers for the cross-crate tests.
//!
//! Two things live here, and both exist because of a hard rule on these
//! tests rather than for convenience.
//!
//! * **Nothing may print document content or a passphrase.** `proptest`
//!   prints the shrunk input of a failing case, so a generated document
//!   handed to it directly would end up in a log. [`Doc`] and [`Pass`] carry
//!   the value and refuse to render it, which keeps the failure report to
//!   sizes and offsets -- enough to act on, and nothing that leaks.
//! * **Key derivation must be cheap here.** `bp_crypto`'s defaults are the
//!   OWASP baseline, 19 MiB and two passes, which is right for a document and
//!   ruinous for a property test that opens a file a hundred times.
//!   [`cheap`] is the same envelope with the cost turned down; it changes
//!   only the KDF parameters, so every framing, nonce and AAD rule under test
//!   is the shipping one.

#![allow(dead_code)]

use std::fmt;

use bp_crypto::{KdfParams, SealOptions};
use proptest::test_runner::{Config, FileFailurePersistence};

/// A `proptest` configuration that writes nothing into the checkout.
///
/// `proptest`'s default is to save a failing seed in a `.proptest-regressions`
/// file beside the test source. That is a good default and the wrong one
/// here: these tests run against a working tree several agents are editing,
/// and a test that writes into the repository has already caused a real
/// incident on this project. Failures are reported, not filed.
pub fn no_files(cases: u32) -> Config {
    Config {
        cases,
        failure_persistence: Some(Box::new(FileFailurePersistence::Off)),
        ..Config::default()
    }
}

/// Document content that will never be printed.
#[derive(Clone, PartialEq, Eq)]
pub struct Doc(pub String);

impl fmt::Debug for Doc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "<document: {} bytes, {} chars>",
            self.0.len(),
            self.0.chars().count()
        )
    }
}

impl Doc {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

/// Raw document bytes that will never be printed.
#[derive(Clone, PartialEq, Eq)]
pub struct Bytes(pub Vec<u8>);

impl fmt::Debug for Bytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<{} bytes>", self.0.len())
    }
}

impl Bytes {
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }
}

/// A passphrase that will never be printed, not even its length.
#[derive(Clone, PartialEq, Eq)]
pub struct Pass(pub String);

impl fmt::Debug for Pass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<passphrase>")
    }
}

impl Pass {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// `bp_crypto`'s shipping envelope with the key derivation turned down to
/// the minimum the crate itself accepts.
///
/// Only [`KdfParams`] changes. The suite, the chunk framing, the nonce
/// construction and the additional authenticated data are all the defaults,
/// so what these tests exercise is the format that ships.
pub fn cheap() -> SealOptions {
    SealOptions {
        kdf: KdfParams {
            memory_kib: 8,
            iterations: 1,
            lanes: 1,
        },
        ..SealOptions::default()
    }
}

/// The same, with a chunk size small enough that a test can straddle a chunk
/// boundary without writing megabytes.
pub fn cheap_with_chunk(chunk_size: u32) -> SealOptions {
    SealOptions {
        chunk_size,
        ..cheap()
    }
}
