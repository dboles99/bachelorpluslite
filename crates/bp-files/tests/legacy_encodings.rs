//! Files written by other programs open with the text their author typed, and
//! save back byte for byte (ADR-0085, W2-04).
//!
//! **The vectors are literals produced by Python's codecs, not by this
//! build** -- trap 7. A round trip through `load` and `encode` proves only that
//! they agree with each other; these prove they agree with a file Notepad, or
//! anything else, wrote. Add beside them rather than editing one: each is a
//! record of bytes somebody else's encoder produced.

use bp_core::Encoding;
use bp_files::{LineEndingPolicy, encode, load};

/// Written by Python's `cp1252` codec, not by this build.
const WINDOWS_1252: &[u8] = b"Caf\xe9 au lait costs \x804 at the na\xefve gar\xe7on\x92s stall.\x0d\x0aR\xe9sum\xe9 attached.\x0d\x0a";
const WINDOWS_1252_TEXT: &str =
    "Café au lait costs €4 at the naïve garçon’s stall.\r\nRésumé attached.\r\n";

/// Written by Python's `shift_jis` codec, not by this build.
const SHIFT_JIS: &[u8] = b"\x82\xb1\x82\xf1\x82\xc9\x82\xbf\x82\xcd\x81A\x90\xa2\x8aE\x81B\x82\xb1\x82\xea\x82\xcd\x93\xfa\x96{\x8c\xea\x82\xcc\x83e\x83L\x83X\x83g\x83t\x83@\x83C\x83\x8b\x82\xc5\x82\xb7\x81B\x83\x81\x83\x82\x92\xa0\x82\xc5\x95\xdb\x91\xb6\x82\xb5\x82\xdc\x82\xb5\x82\xbd\x81B\x0d\x0a";
const SHIFT_JIS_TEXT: &str =
    "こんにちは、世界。これは日本語のテキストファイルです。メモ帳で保存しました。\r\n";

/// Written by Python's `gbk` codec, not by this build.
const GBK: &[u8] = b"\xc4\xe3\xba\xc3\xa3\xac\xca\xc0\xbd\xe7\xa1\xa3\xd5\xe2\xca\xc7\xd2\xbb\xb8\xf6\xbc\xf2\xcc\xe5\xd6\xd0\xce\xc4\xb5\xc4\xce\xc4\xb1\xbe\xce\xc4\xbc\xfe\xa3\xac\xd3\xc3\xbc\xc7\xca\xc2\xb1\xbe\xb1\xa3\xb4\xe6\xa1\xa3\xce\xd2\xc3\xc7\xd4\xda\xd5\xe2\xc0\xef\xd0\xb4\xb1\xca\xbc\xc7\xa1\xa3\x0d\x0a";
const GBK_TEXT: &str =
    "你好，世界。这是一个简体中文的文本文件，用记事本保存。我们在这里写笔记。\r\n";

/// Written by Python's `utf-16-le` codec, not by this build.
const UTF16LE_NO_BOM: &[u8] = b"G\x00r\x00\xfc\x00\xdf\x00e\x00 \x00a\x00u\x00s\x00 \x00K\x00\xf6\x00l\x00n\x00\x0d\x00\x0a\x00";
const UTF16LE_NO_BOM_TEXT: &str = "Grüße aus Köln\r\n";

/// Written by Python's `utf-16-le` codec, not by this build.
const UTF16LE_NO_BOM_ASCII: &[u8] =
    b"h\x00e\x00l\x00l\x00o\x00,\x00 \x00w\x00o\x00r\x00l\x00d\x00\x0d\x00\x0a\x00";
const UTF16LE_NO_BOM_ASCII_TEXT: &str = "hello, world\r\n";

fn round_trip(bytes: &[u8], text: &str) -> Encoding {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("written-elsewhere.txt");
    std::fs::write(&path, bytes).expect("the fixture");

    let file = load(&path).expect("a file Notepad opens must open");
    assert_eq!(file.text, text, "the text its author typed");
    let saved = encode(&file.text, file.encoding, LineEndingPolicy::Preserve)
        .expect("text read from a file can be written back to it");
    assert_eq!(saved, bytes, "saved back as it came, byte for byte");
    file.encoding
}

#[test]
fn a_windows_1252_file_opens_and_saves_back_as_windows_1252() {
    assert_eq!(
        round_trip(WINDOWS_1252, WINDOWS_1252_TEXT),
        Encoding::Legacy("windows-1252")
    );
}

#[test]
fn a_shift_jis_file_opens_and_saves_back_as_shift_jis() {
    assert_eq!(
        round_trip(SHIFT_JIS, SHIFT_JIS_TEXT),
        Encoding::Legacy("Shift_JIS")
    );
}

#[test]
fn a_gbk_file_opens_and_saves_back_as_gbk() {
    assert_eq!(round_trip(GBK, GBK_TEXT), Encoding::Legacy("GBK"));
}

#[test]
fn utf16_without_a_byte_order_mark_opens_as_text_not_as_nul_separated_letters() {
    assert_eq!(
        round_trip(UTF16LE_NO_BOM, UTF16LE_NO_BOM_TEXT),
        Encoding::Utf16LeNoBom
    );
}

#[test]
fn ascii_utf16_without_a_mark_is_found_before_utf8_validation_accepts_it() {
    // The vector that proves the order: these bytes are valid UTF-8 -- "h",
    // NUL, "e", NUL -- so a UTF-8 check that ran first would open the file
    // with a NUL between every letter.
    assert_eq!(
        round_trip(UTF16LE_NO_BOM_ASCII, UTF16LE_NO_BOM_ASCII_TEXT),
        Encoding::Utf16LeNoBom
    );
}
