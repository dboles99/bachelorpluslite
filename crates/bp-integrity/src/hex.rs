//! Lowercase hexadecimal, because the artefacts this crate writes are text.
//!
//! `bp-crypto` spells verifying keys in hex too, but keeps its encoder
//! private, and the two must agree byte-for-byte -- so this one is written to
//! the same rule (lowercase, no separators) and pinned by a test that
//! round-trips a real key through both.

use std::fmt::Write as _;

/// Encode bytes as lowercase hex with no separators.
pub(crate) fn encode(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut out, byte| {
        // Writing into a `String` cannot fail.
        let _ = write!(out, "{byte:02x}");
        out
    })
}

/// Decode hex, rejecting anything that is not exactly hex digits.
///
/// Deliberately stricter than [`bp_crypto::VerifyingKey::from_hex`], which
/// tolerates the whitespace of a key pasted out of a wrapped email. A field
/// inside a file *this crate wrote* has no excuse for stray characters, and
/// accepting them would mean two spellings of the same sidecar.
///
/// Digits are decoded by hand rather than via `u8::from_str_radix`, which
/// accepts a leading `+` and would read `"+f"` as a byte.
pub(crate) fn decode(text: &str) -> Option<Vec<u8>> {
    const fn nibble(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            _ => None,
        }
    }

    let digits = text.as_bytes();
    if digits.is_empty() || !digits.len().is_multiple_of(2) {
        return None;
    }
    digits
        .chunks_exact(2)
        .map(|pair| Some((nibble(pair[0])? << 4) | nibble(pair[1])?))
        .collect()
}
