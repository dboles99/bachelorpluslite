//! The `.bpadx` header: what a reader needs before it can try a passphrase.
//!
//! Everything here is **untrusted input** until a chunk authenticates. The
//! header is read before any key exists, so a reader that acts on it
//! credulously is acting on whatever an attacker wrote. Two consequences run
//! through this module:
//!
//! * every field is range-checked on the way in, and the checks are the point
//!   rather than defensive decoration -- a stored Argon2id cost of 64 GiB is a
//!   denial-of-service, not a strong document;
//! * the header's own bytes are authenticated later, as additional data on
//!   every chunk (ADR-0021), so an edit to any field below makes the whole
//!   document fail to open rather than silently changing how it is read.

use crate::{CryptoError, Suite};

/// File magic. Six bytes, ending in a NUL so a text editor showing the file
/// stops at something obviously binary rather than running on into the
/// ciphertext.
pub(crate) const MAGIC: &[u8; 6] = b"BPADX\0";

/// The format version this build writes.
pub(crate) const VERSION: u16 = 1;

/// Salt length. 16 bytes is the Argon2 recommendation; longer buys nothing
/// against a per-file random salt.
pub(crate) const SALT_LEN: usize = 16;

/// Default plaintext bytes per chunk.
///
/// 64 KiB: large enough that the 16-byte tag per chunk is noise (0.02%), small
/// enough that a reader never needs much memory and a damaged region stays
/// small.
pub const DEFAULT_CHUNK_SIZE: u32 = 64 * 1024;

/// Bounds on the chunk size a file may declare.
///
/// The upper bound is what stops a hostile header making a reader allocate a
/// gigabyte before it has authenticated anything at all.
pub(crate) const MIN_CHUNK_SIZE: u32 = 1024;
pub(crate) const MAX_CHUNK_SIZE: u32 = 16 * 1024 * 1024;

/// Argon2id cost, stored per document so it can be raised later without
/// orphaning documents written today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KdfParams {
    /// Memory cost, in KiB.
    pub memory_kib: u32,
    /// Iterations.
    pub iterations: u32,
    /// Parallelism lanes.
    pub lanes: u32,
}

impl Default for KdfParams {
    /// The OWASP baseline (ADR-0021): 19 MiB, t=2, p=1. Around 50-100 ms on
    /// an ordinary machine, so opening a document feels immediate.
    fn default() -> Self {
        Self {
            memory_kib: 19 * 1024,
            iterations: 2,
            lanes: 1,
        }
    }
}

impl KdfParams {
    /// Refuse costs that would be an attack on the reader rather than a
    /// strong document.
    ///
    /// The upper bounds are generous -- a legitimate document may well be
    /// stronger than the baseline -- but finite. A file asking for 64 GiB is
    /// not a document anyone wrote.
    pub(crate) fn validate(self) -> Result<(), CryptoError> {
        // Argon2's own minimum is 8 KiB per lane; below that it refuses
        // anyway, and refusing here names the reason.
        let sane = self.memory_kib >= 8
            && self.memory_kib <= 1024 * 1024
            && self.iterations >= 1
            && self.iterations <= 64
            && self.lanes >= 1
            && self.lanes <= 64;

        if sane {
            Ok(())
        } else {
            Err(CryptoError::UnreasonableCost {
                memory_kib: self.memory_kib,
                iterations: self.iterations,
                lanes: self.lanes,
            })
        }
    }
}

/// Everything a reader learns before it has a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub version: u16,
    pub suite: Suite,
    pub kdf: KdfParams,
    pub salt: [u8; SALT_LEN],
    pub chunk_size: u32,
    /// The random part of every chunk's nonce. The chunk index supplies the
    /// rest, so a nonce cannot repeat within a file; a fresh salt per file
    /// means the key differs between files, so it cannot repeat across them
    /// either.
    pub nonce_prefix: Vec<u8>,
}

impl Header {
    /// Serialise, exactly as it appears at the start of the file.
    ///
    /// This is also what goes into every chunk's additional data, which is why
    /// it is one function: two encodings that could differ by a byte would
    /// mean a file that writes and never reads back.
    pub(crate) fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(48);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&self.version.to_le_bytes());
        out.push(self.suite.tag());
        // KDF identifier. One byte, so a future Argon2 successor is a value
        // rather than a format break.
        out.push(1);
        out.extend_from_slice(&self.kdf.memory_kib.to_le_bytes());
        out.extend_from_slice(&self.kdf.iterations.to_le_bytes());
        out.extend_from_slice(&self.kdf.lanes.to_le_bytes());
        out.extend_from_slice(&self.salt);
        out.extend_from_slice(&self.chunk_size.to_le_bytes());
        // Length-prefixed: the two suites have different nonce widths, and a
        // reader that inferred the width from the suite would have to be
        // changed for every future suite.
        out.push(u8::try_from(self.nonce_prefix.len()).unwrap_or(0));
        out.extend_from_slice(&self.nonce_prefix);
        out
    }

    /// Parse a header, returning it and how many bytes it occupied.
    ///
    /// Every failure names what was wrong. "Could not open" tells a user
    /// nothing they can act on, and the difference between "this is not a
    /// BachelorPad+ document" and "this was written by a newer version"
    /// decides what they do next.
    pub(crate) fn parse(bytes: &[u8]) -> Result<(Self, usize), CryptoError> {
        let mut at = 0usize;

        let magic = take(bytes, &mut at, MAGIC.len())?;
        if magic != MAGIC {
            return Err(CryptoError::NotBpadx);
        }

        let version = u16::from_le_bytes(take_array::<2>(bytes, &mut at)?);
        if version != VERSION {
            return Err(CryptoError::UnsupportedVersion {
                found: version,
                supported: VERSION,
            });
        }

        let suite = Suite::from_tag(take_array::<1>(bytes, &mut at)?[0])?;

        let kdf_id = take_array::<1>(bytes, &mut at)?[0];
        if kdf_id != 1 {
            return Err(CryptoError::UnsupportedKdf(kdf_id));
        }

        let kdf = KdfParams {
            memory_kib: u32::from_le_bytes(take_array::<4>(bytes, &mut at)?),
            iterations: u32::from_le_bytes(take_array::<4>(bytes, &mut at)?),
            lanes: u32::from_le_bytes(take_array::<4>(bytes, &mut at)?),
        };
        kdf.validate()?;

        let salt = take_array::<SALT_LEN>(bytes, &mut at)?;

        let chunk_size = u32::from_le_bytes(take_array::<4>(bytes, &mut at)?);
        if !(MIN_CHUNK_SIZE..=MAX_CHUNK_SIZE).contains(&chunk_size) {
            return Err(CryptoError::UnreasonableChunkSize(chunk_size));
        }

        let prefix_len = usize::from(take_array::<1>(bytes, &mut at)?[0]);
        if prefix_len != suite.nonce_prefix_len() {
            return Err(CryptoError::Corrupt);
        }
        let nonce_prefix = take(bytes, &mut at, prefix_len)?.to_vec();

        Ok((
            Self {
                version,
                suite,
                kdf,
                salt,
                chunk_size,
                nonce_prefix,
            },
            at,
        ))
    }
}

fn take<'a>(bytes: &'a [u8], at: &mut usize, len: usize) -> Result<&'a [u8], CryptoError> {
    let end = at.checked_add(len).ok_or(CryptoError::Truncated)?;
    let slice = bytes.get(*at..end).ok_or(CryptoError::Truncated)?;
    *at = end;
    Ok(slice)
}

fn take_array<const N: usize>(bytes: &[u8], at: &mut usize) -> Result<[u8; N], CryptoError> {
    let slice = take(bytes, at, N)?;
    let mut out = [0u8; N];
    out.copy_from_slice(slice);
    Ok(out)
}
