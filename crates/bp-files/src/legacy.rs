//! Files that are not UTF-8 and do not say what they are (ADR-0085).
//!
//! Two kinds, and both are what Notepad has always opened:
//!
//! * **UTF-16 without a byte-order mark**, found by its pattern. Mostly Latin
//!   text in UTF-16 has a zero in every other byte, and -- the reason this
//!   check runs before UTF-8 validation -- ASCII-only UTF-16 *is* valid UTF-8,
//!   `h`, NUL, `e`, NUL, so a file checked the other way round opens with a
//!   NUL between every letter.
//! * **A legacy code page** -- Windows-1252, Shift-JIS, GBK -- guessed by
//!   `chardetng` once the bytes have failed to be UTF-8, and decoded and
//!   encoded by `encoding_rs`. The guess is shown in the status bar like any
//!   other encoding, so a wrong one is visible rather than silently applied.
//!
//! What this module deliberately does not do is substitute. A byte sequence
//! the guessed encoding cannot decode refuses the file rather than putting
//! U+FFFD in it, and a character the encoding cannot hold refuses the save
//! rather than writing `&#945;` into a text file -- both are ways a document
//! that looks intact writes damage back to disk.

use bp_core::Encoding;
use chardetng::{EncodingDetector, Iso2022JpDetection, Utf8Detection};

use crate::utf16::Endian;

/// Which byte order an unmarked file's zeros say it is in, if they say.
///
/// A zero in at least half the code units' high bytes, and almost none in
/// the low ones. Latin text in UTF-16 meets that easily; UTF-8 text never
/// does, because UTF-8 has no reason to contain a zero byte at all. CJK text
/// in UTF-16 has few zeros and is not found this way -- `chardetng` does not
/// detect UTF-16 either, so such a file still needs its mark, which is what
/// Windows has always written.
pub(crate) fn utf16_pattern(bytes: &[u8]) -> Option<Endian> {
    if bytes.len() < 2 || !bytes.len().is_multiple_of(2) {
        return None;
    }
    let units = bytes.len() / 2;
    let (mut even, mut odd) = (0usize, 0usize);
    for (i, byte) in bytes.iter().enumerate() {
        if *byte == 0 {
            if i.is_multiple_of(2) {
                even += 1;
            } else {
                odd += 1;
            }
        }
    }
    // "Almost none" rather than none: U+0100 to U+01FF put a zero in the low
    // byte, and one `Ā` should not stop a file being recognised.
    if odd * 2 >= units && even * 10 <= odd {
        Some(Endian::Little)
    } else if even * 2 >= units && odd * 10 <= even {
        Some(Endian::Big)
    } else {
        None
    }
}

/// The legacy encoding `bytes` are most likely in, once they are known not
/// to be UTF-8.
pub(crate) fn guess(bytes: &[u8]) -> Encoding {
    // ISO-2022-JP is denied because it is seven-bit: a file in it is valid
    // UTF-8 and never reaches this function, so allowing it could only ever
    // produce a wrong guess.
    let mut detector = EncodingDetector::new(Iso2022JpDetection::Deny);
    detector.feed(bytes, true);
    Encoding::Legacy(detector.guess(None, Utf8Detection::Deny).name())
}

/// `bytes` decoded as the legacy encoding `name`, or [`None`] if any of them
/// is not a character in it.
pub(crate) fn decode(bytes: &[u8], name: &str) -> Option<String> {
    let encoding = encoding_rs::Encoding::for_label(name.as_bytes())?;
    encoding
        .decode_without_bom_handling_and_without_replacement(bytes)
        .map(std::borrow::Cow::into_owned)
}

/// `text` encoded as the legacy encoding `name`, or the first character it
/// has no form for.
pub(crate) fn encode(text: &str, name: &str) -> Result<Vec<u8>, char> {
    let Some(encoding) = encoding_rs::Encoding::for_label(name.as_bytes()) else {
        // Unreachable from a name this crate produced, and still not a
        // reason to write UTF-8 under a legacy label: refuse on the first
        // character there is, or succeed on an empty document.
        return text.chars().next().map_or(Ok(Vec::new()), Err);
    };
    let (bytes, _, had_unmappable) = encoding.encode(text);
    if !had_unmappable {
        return Ok(bytes.into_owned());
    }
    // Found again a character at a time, only on the failure path: the
    // whole-text call above is what every successful save pays.
    let mut buffer = [0u8; 4];
    Err(text
        .chars()
        .find(|c| encoding.encode(c.encode_utf8(&mut buffer)).2)
        .unwrap_or(char::REPLACEMENT_CHARACTER))
}

/// The encoding a person or a recovery checkpoint names, if it is one this
/// product can read and write.
///
/// Accepts any label the Encoding Standard does -- `latin1`, `sjis`, `cp936`
/// -- and answers with the standard's own name, so two spellings of one
/// encoding are one value. UTF-8 and UTF-16 answer as themselves rather than
/// as a legacy name.
#[must_use]
pub fn named(label: &str) -> Option<Encoding> {
    let encoding = encoding_rs::Encoding::for_label(label.trim().as_bytes())?;
    Some(if encoding == encoding_rs::UTF_8 {
        Encoding::Utf8
    } else if encoding == encoding_rs::UTF_16LE {
        Encoding::Utf16Le
    } else if encoding == encoding_rs::UTF_16BE {
        Encoding::Utf16Be
    } else if encoding == encoding_rs::REPLACEMENT || encoding == encoding_rs::X_USER_DEFINED {
        // Neither is an encoding a file is written in: one decodes
        // everything to U+FFFD, the other is a browser's escape hatch.
        return None;
    } else {
        Encoding::Legacy(encoding.name())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latin_utf16_is_found_by_its_zeros_in_either_order() {
        assert_eq!(utf16_pattern(b"h\0i\0"), Some(Endian::Little));
        assert_eq!(utf16_pattern(b"\0h\0i"), Some(Endian::Big));
    }

    #[test]
    fn utf8_text_has_no_utf16_pattern() {
        assert_eq!(utf16_pattern("plain text, and café".as_bytes()), None);
        assert_eq!(utf16_pattern(b""), None);
        assert_eq!(utf16_pattern(b"odd"), None);
    }

    #[test]
    fn a_character_with_no_form_in_the_encoding_is_named_rather_than_escaped() {
        // encoding_rs would write `&#945;` for it, into a text file.
        assert_eq!(encode("alpha is α", "windows-1252"), Err('α'));
        assert_eq!(encode("café", "windows-1252"), Ok(b"caf\xe9".to_vec()));
    }

    #[test]
    fn a_byte_with_no_character_refuses_rather_than_substituting() {
        // 0x81 0x20 is not a Shift-JIS character: a lead byte with no trail.
        assert_eq!(decode(b"\x81\x20", "Shift_JIS"), None);
    }

    #[test]
    fn any_spelling_of_an_encoding_is_that_encoding() {
        assert_eq!(named("latin1"), Some(Encoding::Legacy("windows-1252")));
        assert_eq!(named("sjis"), Some(Encoding::Legacy("Shift_JIS")));
        assert_eq!(named(" UTF-8 "), Some(Encoding::Utf8));
        assert_eq!(named("replacement"), None);
        assert_eq!(named("not an encoding"), None);
    }
}
