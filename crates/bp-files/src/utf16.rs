//! UTF-16 to UTF-8 and back, by hand.
//!
//! Hand-written rather than delegated to `encoding_rs`, deliberately. The
//! whole job is `char::decode_utf16` over an iterator of `u16` assembled in
//! the right byte order, and its inverse `str::encode_utf16`; both are in
//! `core`. A general-purpose encoding library would bring dozens of legacy
//! codecs, a lossy-by-default decoder and an ADR's worth of trust decision,
//! to buy two loops.
//!
//! The design rule here is the same one that made `load` refuse UTF-16 in the
//! first place: **never substitute.** A decoder that emits U+FFFD for damaged
//! input hands the user a document that looks fine, and the damage is written
//! back over the original on the next save. Every input this module cannot
//! represent exactly is an error naming where it went wrong.

use bp_core::Encoding;

/// Byte order of a UTF-16 stream.
///
/// A separate type rather than a `bool` because `decode(bytes, true)` at a
/// call site is unreadable, and reading a UTF-16 file in the wrong order
/// yields plausible-looking CJK rather than an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Endian {
    Little,
    Big,
}

impl Endian {
    /// The byte order an encoding implies, or [`None`] for one that is not
    /// UTF-16.
    pub(crate) const fn of(encoding: Encoding) -> Option<Self> {
        match encoding {
            Encoding::Utf16Le | Encoding::Utf16LeNoBom => Some(Self::Little),
            Encoding::Utf16Be | Encoding::Utf16BeNoBom => Some(Self::Big),
            Encoding::Utf8 | Encoding::Utf8Bom | Encoding::Legacy(_) => None,
        }
    }

    const fn unit(self, pair: [u8; 2]) -> u16 {
        match self {
            Self::Little => u16::from_le_bytes(pair),
            Self::Big => u16::from_be_bytes(pair),
        }
    }

    const fn bytes(self, unit: u16) -> [u8; 2] {
        match self {
            Self::Little => unit.to_le_bytes(),
            Self::Big => unit.to_be_bytes(),
        }
    }
}

/// The two ways a byte stream can fail to be UTF-16.
///
/// Both carry a position, because "this file is not valid UTF-16" is not
/// something a user can act on and "byte 4097 is half a character" is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Utf16Error {
    /// The body is an odd number of bytes, so its last code unit is cut in
    /// half. Always a truncated or corrupt file: UTF-16 is measured in pairs.
    OddByteCount { bytes: usize },
    /// A surrogate code unit with no partner. Nothing in Unicode maps to it,
    /// so there is no correct character to produce -- only U+FFFD, which is
    /// the substitution this module exists to refuse. `at` is the index of
    /// the offending code unit, counted from the start of the body, after any
    /// byte-order mark.
    UnpairedSurrogate { at: usize },
}

/// Decode a UTF-16 body -- byte-order mark already removed -- into a `String`.
///
/// An empty body decodes to an empty string. That is the "lone BOM" case: a
/// file consisting of nothing but a byte-order mark is a valid, empty UTF-16
/// document, not an error, and round-trips back to those same two bytes
/// because the encoding remembers the mark.
pub(crate) fn decode(body: &[u8], endian: Endian) -> Result<String, Utf16Error> {
    if !body.len().is_multiple_of(2) {
        return Err(Utf16Error::OddByteCount { bytes: body.len() });
    }

    // `as_chunks::<2>` rather than `chunks_exact(2)`, which hands back real
    // `[u8; 2]` arrays instead of slices that have to be indexed back into
    // one. The remainder is provably empty -- the length was checked above --
    // and discarding it here says so.
    //
    // Changed because a *newer* clippy than this machine's has
    // `chunks_exact_to_as_chunks`, and the first hosted CI run failed on it
    // (ADR-0073). The local toolchain is 1.97.1 and the runners take the
    // latest stable, so this is the class of finding no local gate can
    // produce: the lint did not exist here.
    let (pairs, _rest) = body.as_chunks::<2>();
    let units = pairs.iter().map(|&pair| endian.unit(pair));

    // One byte of UTF-8 per code unit is the floor, and the common case for
    // the Latin text this editor mostly sees; growing past it is cheap.
    let mut out = String::with_capacity(body.len() / 2);
    // Tracked rather than taken from the iterator: `decode_utf16` consumes
    // one unit for a BMP character and two for a surrogate pair, so the
    // output index is not the input index.
    let mut at = 0usize;
    for decoded in char::decode_utf16(units) {
        match decoded {
            Ok(c) => {
                out.push(c);
                at += c.len_utf16();
            }
            Err(_) => return Err(Utf16Error::UnpairedSurrogate { at }),
        }
    }
    Ok(out)
}

/// Encode text as a UTF-16 body, without a byte-order mark.
///
/// Infallible, and that is not an accident: every `char` in a `String` is a
/// Unicode scalar value, and every scalar value has a UTF-16 form. The
/// asymmetry with [`decode`] is real -- UTF-16 can hold sequences UTF-8
/// cannot, never the reverse.
pub(crate) fn encode(text: &str, endian: Endian) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() * 2);
    for unit in text.encode_utf16() {
        out.extend_from_slice(&endian.bytes(unit));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const BOTH: [Endian; 2] = [Endian::Little, Endian::Big];

    #[test]
    fn byte_order_is_actually_honoured() {
        assert_eq!(encode("hi", Endian::Little), b"h\0i\0");
        assert_eq!(encode("hi", Endian::Big), b"\0h\0i");
    }

    #[test]
    fn an_empty_body_is_an_empty_document_not_an_error() {
        // The lone-BOM case: `load` strips the mark and hands us nothing.
        for endian in BOTH {
            assert_eq!(decode(&[], endian).unwrap(), "");
            assert_eq!(encode("", endian), Vec::<u8>::new());
        }
    }

    #[test]
    fn an_odd_byte_count_is_named_not_padded() {
        for endian in BOTH {
            assert_eq!(
                decode(&[b'h', 0, b'i'], endian).unwrap_err(),
                Utf16Error::OddByteCount { bytes: 3 }
            );
        }
        // One stray byte on its own is the same failure, not an empty file.
        assert_eq!(
            decode(&[0x41], Endian::Little).unwrap_err(),
            Utf16Error::OddByteCount { bytes: 1 }
        );
    }

    #[test]
    fn an_unpaired_surrogate_is_named_with_its_position() {
        // "a" then a lone high surrogate: U+0061, U+D800.
        let bytes = [0x61, 0x00, 0x00, 0xD8];
        assert_eq!(
            decode(&bytes, Endian::Little).unwrap_err(),
            Utf16Error::UnpairedSurrogate { at: 1 },
            "the position must be the code unit index, not the byte index"
        );

        // A lone *low* surrogate is the same refusal.
        let low = [0x00, 0xDC];
        assert_eq!(
            decode(&low, Endian::Little).unwrap_err(),
            Utf16Error::UnpairedSurrogate { at: 0 }
        );
    }

    #[test]
    fn the_position_of_a_surrogate_counts_pairs_as_two() {
        // U+1F600 (two units), then a lone high surrogate at index 2.
        let mut bytes = encode("\u{1F600}", Endian::Little);
        bytes.extend_from_slice(&[0x00, 0xD8]);
        assert_eq!(
            decode(&bytes, Endian::Little).unwrap_err(),
            Utf16Error::UnpairedSurrogate { at: 2 }
        );
    }

    #[test]
    fn a_reversed_high_low_pair_is_two_unpaired_surrogates() {
        // Low then high is not a pair. Caught at the first unit.
        let bytes = [0x00, 0xDC, 0x00, 0xD8];
        assert_eq!(
            decode(&bytes, Endian::Little).unwrap_err(),
            Utf16Error::UnpairedSurrogate { at: 0 }
        );
    }

    #[test]
    fn the_scalar_boundaries_survive_both_orders() {
        // One past each edge of the surrogate hole, and both ends of the
        // range. U+FFFF is the last BMP scalar; U+10000 is the first that
        // needs a pair; U+10FFFF is the last that exists at all.
        let edges = "\u{0}\u{D7FF}\u{E000}\u{FFFF}\u{10000}\u{10FFFF}";
        for endian in BOTH {
            assert_eq!(decode(&encode(edges, endian), endian).unwrap(), edges);
        }
    }

    #[test]
    fn reading_in_the_wrong_order_does_not_silently_look_fine() {
        // Not a guarantee the code makes -- a proof that the `Endian` type is
        // load-bearing. Little-endian "hi" read big-endian is CJK.
        let swapped = decode(&encode("hi", Endian::Little), Endian::Big).unwrap();
        assert_ne!(swapped, "hi");
    }

    proptest! {
        /// **UTF-16 is a faithful container for any text we can hold.**
        /// Encode then decode, in either order, is the identity -- including
        /// astral characters, which are the ones a naive `as u16` cast eats.
        #[test]
        fn encode_then_decode_is_the_identity(text in ".{0,64}") {
            for endian in BOTH {
                prop_assert_eq!(decode(&encode(&text, endian), endian).unwrap(), text.clone());
            }
        }

        /// **The encoded length is exactly two bytes per code unit.** Pins the
        /// invariant `decode`'s odd-byte check depends on: nothing this module
        /// writes can ever be an odd number of bytes.
        #[test]
        fn encoding_always_produces_pairs(text in ".{0,64}") {
            for endian in BOTH {
                let bytes = encode(&text, endian);
                prop_assert_eq!(bytes.len(), text.encode_utf16().count() * 2);
                prop_assert!(bytes.len().is_multiple_of(2));
            }
        }
    }
}
