//! Reading a file into a document.

use std::io;
use std::path::{Path, PathBuf};

use bp_core::{Encoding, LineEnding};
use thiserror::Error;

use crate::utf16::{self, Endian, Utf16Error};

/// Why a file could not be turned into a document.
///
/// Every variant names the file and says what is wrong with it rather than
/// what the code was doing. "os error 32" and "invalid utf-16" are not things
/// a user can act on; "byte 4097 is half a character" is.
#[derive(Debug, Error)]
pub enum LoadError {
    #[error("cannot read {}: {source}", .path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("{} is not valid {}", .path.display(), .encoding.label())]
    InvalidText { path: PathBuf, encoding: Encoding },

    /// A legacy file whose bytes would not survive being saved back.
    ///
    /// Some code pages give one character two encodings -- Shift-JIS has NEC
    /// and IBM codes for the same kanji -- and an encoder can write only one
    /// of them. A file using the other would come back changed in bytes
    /// nobody edited, which is the silent alteration ADR-0085 exists to
    /// prevent; so it is refused, before anything can be saved over it.
    #[error(
        "{} reads as {} but would not save back unchanged -- some of its characters have two encodings in it and this program can write only one; it was not opened, so nothing in it has changed",
        .path.display(), .encoding.label()
    )]
    WouldNotSaveBack { path: PathBuf, encoding: Encoding },

    /// The body of a UTF-16 file is an odd number of bytes, so its last code
    /// unit is cut in half. Refused rather than dropping the stray byte: a
    /// file this size is truncated or corrupt, and opening it would invite
    /// the user to save the truncation back over whatever survived.
    #[error(
        "{} is {} but its text is {bytes} bytes -- an odd length, so the file is truncated or is not really {}",
        .path.display(), .encoding.label(), .encoding.label()
    )]
    TruncatedUtf16 {
        path: PathBuf,
        encoding: Encoding,
        bytes: usize,
    },

    /// A surrogate code unit with no partner. There is no character to
    /// produce for it -- only U+FFFD, and substituting one would hand back a
    /// document that looks intact and writes the damage back on the next
    /// save.
    #[error(
        "{} is {} but has an unpaired surrogate at character {at}, so it is not valid text",
        .path.display(), .encoding.label()
    )]
    UnpairedSurrogate {
        path: PathBuf,
        encoding: Encoding,
        /// Index of the offending code unit within the text, after the
        /// byte-order mark.
        at: usize,
    },
}

/// A file read from disk, with the properties the status bar reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedFile {
    pub text: String,
    /// Encoding as found on disk. Saving must write it back the same way --
    /// opening a BOM'd file and saving it without the BOM is data loss as far
    /// as some downstream tools are concerned.
    pub encoding: Encoding,
    pub line_ending: LineEnding,
    /// Size on disk, before BOM stripping or decoding.
    pub bytes_on_disk: u64,
}

/// Read a text file, detecting its encoding and line endings.
///
/// UTF-16 in either byte order is decoded, not refused. Windows tools have
/// emitted UTF-16 with a byte-order mark for decades, so refusing meant a
/// Windows editor that could not open a large class of Windows files. What
/// the original refusal was protecting against -- a lossy decode the user
/// then saves back over their original -- is protected against instead by
/// [`LoadError::TruncatedUtf16`] and [`LoadError::UnpairedSurrogate`]:
/// nothing is ever substituted, so anything that loads is exactly what was on
/// disk and [`crate::encode`] writes it back byte for byte.
///
/// **The encoding is found in this order** (ADR-0085): a byte-order mark;
/// then the UTF-16 pattern, an unmarked file whose zeros fall in every other
/// byte; then valid UTF-8; then a legacy code page guessed by `chardetng`.
/// The UTF-16 check comes before the UTF-8 one because ASCII-only UTF-16 is
/// valid UTF-8 -- `h`, NUL, `e`, NUL -- and opened as UTF-8 it has a NUL
/// between every letter. A file that is none of these, or that the guessed
/// encoding cannot decode without substituting, is refused.
pub fn load(path: &Path) -> Result<LoadedFile, LoadError> {
    let bytes = read(path)?;
    let encoding = unmarked(&bytes).unwrap_or_else(|| Encoding::detect_bom(&bytes));
    decode(path, &bytes, encoding)
}

/// Read a text file as `encoding`, whatever it looks like (Format > Reopen
/// As).
///
/// The correction for a guess that was wrong -- `chardetng` reading a short
/// Central European file as Western, say -- and the same refusals as
/// [`load`]: bytes the encoding cannot decode, or would not write back the
/// same, are refused rather than substituted. A byte-order mark the file has
/// is honoured within the family asked for, so "UTF-16 LE" on a marked file
/// keeps the mark and on an unmarked one does not add one.
pub fn load_as(path: &Path, encoding: Encoding) -> Result<LoadedFile, LoadError> {
    let bytes = read(path)?;
    let found = Encoding::detect_bom(&bytes);
    let encoding = match encoding {
        Encoding::Utf8 | Encoding::Utf8Bom if found == Encoding::Utf8Bom => Encoding::Utf8Bom,
        Encoding::Utf8 | Encoding::Utf8Bom => Encoding::Utf8,
        Encoding::Utf16Le | Encoding::Utf16LeNoBom if found == Encoding::Utf16Le => {
            Encoding::Utf16Le
        }
        Encoding::Utf16Le | Encoding::Utf16LeNoBom => Encoding::Utf16LeNoBom,
        Encoding::Utf16Be | Encoding::Utf16BeNoBom if found == Encoding::Utf16Be => {
            Encoding::Utf16Be
        }
        Encoding::Utf16Be | Encoding::Utf16BeNoBom => Encoding::Utf16BeNoBom,
        legacy @ Encoding::Legacy(_) => legacy,
    };
    decode(path, &bytes, encoding)
}

fn read(path: &Path) -> Result<Vec<u8>, LoadError> {
    std::fs::read(path).map_err(|source| LoadError::Read {
        path: path.to_owned(),
        source,
    })
}

/// `bytes` as text in `encoding`, which has already been decided.
fn decode(path: &Path, bytes: &[u8], encoding: Encoding) -> Result<LoadedFile, LoadError> {
    let bytes_on_disk = bytes.len() as u64;
    // An unmarked encoding has an empty mark, so this covers every case.
    let body = &bytes[encoding.bom().len()..];

    let text = match (encoding, Endian::of(encoding)) {
        (Encoding::Legacy(name), _) => {
            let text = crate::legacy::decode(body, name).ok_or_else(|| LoadError::InvalidText {
                path: path.to_owned(),
                encoding,
            })?;
            // Asked of every legacy file rather than of the encodings known
            // to have duplicates: the property is what `encode` promises, and
            // a second list of exceptions would be the thing that goes stale.
            if crate::legacy::encode(&text, name).as_deref() != Ok(body) {
                return Err(LoadError::WouldNotSaveBack {
                    path: path.to_owned(),
                    encoding,
                });
            }
            text
        }
        (_, Some(endian)) => utf16::decode(body, endian).map_err(|e| match e {
            Utf16Error::OddByteCount { bytes } => LoadError::TruncatedUtf16 {
                path: path.to_owned(),
                encoding,
                bytes,
            },
            Utf16Error::UnpairedSurrogate { at } => LoadError::UnpairedSurrogate {
                path: path.to_owned(),
                encoding,
                at,
            },
        })?,
        (_, None) => String::from_utf8(body.to_vec()).map_err(|_| LoadError::InvalidText {
            path: path.to_owned(),
            encoding,
        })?,
    };

    let line_ending = LineEnding::detect(&text).unwrap_or_default();

    Ok(LoadedFile {
        text,
        encoding,
        line_ending,
        bytes_on_disk,
    })
}

/// The encoding of a file with no byte-order mark, when it is not UTF-8:
/// UTF-16 by its pattern, or a legacy code page by a guess. [`None`] for a
/// file with a mark, or one that is valid UTF-8.
fn unmarked(bytes: &[u8]) -> Option<Encoding> {
    if !Encoding::detect_bom(bytes).bom().is_empty() {
        return None;
    }
    if let Some(endian) = crate::legacy::utf16_pattern(bytes) {
        let encoding = match endian {
            Endian::Little => Encoding::Utf16LeNoBom,
            Endian::Big => Encoding::Utf16BeNoBom,
        };
        // The pattern is a reason to try, not a verdict: zeros in the right
        // places with an unpaired surrogate among them are not UTF-16.
        if utf16::decode(bytes, endian).is_ok() {
            return Some(encoding);
        }
    }
    if std::str::from_utf8(bytes).is_ok() {
        return None;
    }
    Some(crate::legacy::guess(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SaveOptions, atomic_write};
    use tempfile::tempdir;

    fn write_bytes(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let path = dir.join(name);
        atomic_write(&path, bytes, SaveOptions::default()).unwrap();
        path
    }

    #[test]
    fn reads_plain_utf8() {
        let dir = tempdir().unwrap();
        let path = write_bytes(dir.path(), "a.txt", b"hello\nworld\n");

        let f = load(&path).unwrap();

        assert_eq!(f.text, "hello\nworld\n");
        assert_eq!(f.encoding, Encoding::Utf8);
        assert_eq!(f.line_ending, LineEnding::Lf);
        assert_eq!(f.bytes_on_disk, 12);
    }

    #[test]
    fn strips_a_utf8_bom_but_remembers_it() {
        let dir = tempdir().unwrap();
        let path = write_bytes(dir.path(), "b.txt", b"\xEF\xBB\xBFhello");

        let f = load(&path).unwrap();

        assert_eq!(f.text, "hello", "the BOM must not appear in the buffer");
        assert_eq!(
            f.encoding,
            Encoding::Utf8Bom,
            "but it must be remembered, so saving writes it back"
        );
        assert_eq!(f.bytes_on_disk, 8);
    }

    #[test]
    fn detects_crlf() {
        let dir = tempdir().unwrap();
        let path = write_bytes(dir.path(), "c.txt", b"a\r\nb\r\n");

        assert_eq!(load(&path).unwrap().line_ending, LineEnding::CrLf);
    }

    #[test]
    fn an_empty_file_gets_the_platform_line_ending() {
        let dir = tempdir().unwrap();
        let path = write_bytes(dir.path(), "d.txt", b"");

        let f = load(&path).unwrap();

        assert_eq!(f.text, "");
        assert_eq!(f.line_ending, LineEnding::platform_default());
    }

    #[test]
    fn decodes_utf16_in_both_byte_orders() {
        let dir = tempdir().unwrap();

        let le = write_bytes(dir.path(), "e-le.txt", b"\xFF\xFEh\0i\0");
        let f = load(&le).unwrap();
        assert_eq!(f.text, "hi", "the BOM must not appear in the buffer");
        assert_eq!(f.encoding, Encoding::Utf16Le);
        assert_eq!(f.bytes_on_disk, 6);

        let be = write_bytes(dir.path(), "e-be.txt", b"\xFE\xFF\0h\0i");
        let f = load(&be).unwrap();
        assert_eq!(f.text, "hi");
        assert_eq!(f.encoding, Encoding::Utf16Be);
    }

    #[test]
    fn a_lone_utf16_byte_order_mark_is_an_empty_document() {
        let dir = tempdir().unwrap();
        let path = write_bytes(dir.path(), "e2.txt", b"\xFF\xFE");

        let f = load(&path).unwrap();

        assert_eq!(f.text, "");
        assert_eq!(
            f.encoding,
            Encoding::Utf16Le,
            "an empty UTF-16 file is still a UTF-16 file, and saving it must \
             write the mark back"
        );
        assert_eq!(f.line_ending, LineEnding::platform_default());
    }

    #[test]
    fn refuses_a_truncated_utf16_file_rather_than_dropping_the_stray_byte() {
        let dir = tempdir().unwrap();
        let path = write_bytes(dir.path(), "e3.txt", b"\xFF\xFEh\0i");

        let err = load(&path).unwrap_err();

        assert!(matches!(
            err,
            LoadError::TruncatedUtf16 {
                encoding: Encoding::Utf16Le,
                bytes: 3,
                ..
            }
        ));
        assert!(err.to_string().contains("e3.txt"));
    }

    #[test]
    fn refuses_an_unpaired_surrogate_rather_than_substituting_u_fffd() {
        let dir = tempdir().unwrap();
        // "a" then a lone high surrogate.
        let path = write_bytes(dir.path(), "e4.txt", b"\xFF\xFEa\0\x00\xD8");

        let err = load(&path).unwrap_err();

        assert!(matches!(
            err,
            LoadError::UnpairedSurrogate {
                encoding: Encoding::Utf16Le,
                at: 1,
                ..
            }
        ));
        assert!(err.to_string().contains("e4.txt"));
    }

    #[test]
    fn utf16_line_endings_are_detected_after_decoding() {
        let dir = tempdir().unwrap();
        // "a\r\nb" little-endian.
        let path = write_bytes(dir.path(), "e5.txt", b"\xFF\xFEa\0\r\0\n\0b\0");

        let f = load(&path).unwrap();

        assert_eq!(f.text, "a\r\nb");
        assert_eq!(f.line_ending, LineEnding::CrLf);
    }

    #[test]
    fn bytes_that_are_not_utf8_are_read_as_a_legacy_encoding_rather_than_refused() {
        // This test asserted the opposite until ADR-0085, pinning a refusal
        // the manual called a decision and no ADR had made. These bytes are
        // a valid Windows-1252 file, and that is what Notepad shows.
        let dir = tempdir().unwrap();
        let path = write_bytes(dir.path(), "f.txt", b"ok \xC3\x28 bad");

        //
        // *Which* code page is a guess, and on four bytes an honest one:
        // `chardetng` says Windows-1250 here. That is why the guess is shown
        // in the status bar and Format > Reopen As exists. What must hold
        // whatever it guesses is that nothing is lost.
        let file = load(&path).unwrap();
        assert!(
            matches!(file.encoding, Encoding::Legacy(_)),
            "{:?}",
            file.encoding
        );
        let saved = crate::encode(&file.text, file.encoding, crate::LineEndingPolicy::Preserve)
            .expect("what was read can be written back");
        assert_eq!(saved, b"ok \xC3\x28 bad");
    }

    #[test]
    fn a_legacy_file_that_would_not_save_back_unchanged_is_refused() {
        // 0xED 0x40 is the NEC-selected code for U+7E8A in Shift-JIS; the
        // encoder writes the IBM code, 0xFA 0x5C, for the same character.
        // Enough ordinary Japanese around it that the guess is Shift-JIS.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(
            b"\x82\xb1\x82\xea\x82\xcd\x93\xfa\x96{\x8c\xea\x82\xcc\x83e\x83L\x83X\x83g\x82\xc5\x82\xb7\x81B",
        );
        bytes.extend_from_slice(b"\xed\x40");
        bytes.extend_from_slice(b"\x82\xc5\x82\xb7\x81B\r\n");
        let dir = tempdir().unwrap();
        let path = write_bytes(dir.path(), "nec.txt", &bytes);

        match load(&path) {
            Err(LoadError::WouldNotSaveBack { encoding, .. }) => {
                assert_eq!(encoding, Encoding::Legacy("Shift_JIS"));
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn reopening_as_another_encoding_reads_the_same_bytes_differently() {
        // The correction for a wrong guess: `chardetng` reads these four
        // bytes as Windows-1250, and the person knows it is Windows-1252.
        let dir = tempdir().unwrap();
        let path = write_bytes(dir.path(), "f.txt", b"ok \xC3\x28 bad");

        let file = load_as(&path, Encoding::Legacy("windows-1252")).unwrap();
        assert_eq!(file.encoding, Encoding::Legacy("windows-1252"));
        assert_eq!(file.text, "ok \u{C3}( bad");
    }

    #[test]
    fn reopening_as_utf8_refuses_bytes_that_are_not_utf8() {
        let dir = tempdir().unwrap();
        let path = write_bytes(dir.path(), "f.txt", b"ok \xC3\x28 bad");
        assert!(matches!(
            load_as(&path, Encoding::Utf8),
            Err(LoadError::InvalidText { .. })
        ));
    }

    #[test]
    fn reopening_as_utf16_keeps_a_mark_the_file_has_and_adds_none_it_lacks() {
        let dir = tempdir().unwrap();
        let marked = write_bytes(dir.path(), "m.txt", b"\xFF\xFEh\0i\0");
        let unmarked = write_bytes(dir.path(), "u.txt", b"h\0i\0");
        assert_eq!(
            load_as(&marked, Encoding::Utf16LeNoBom).unwrap().encoding,
            Encoding::Utf16Le
        );
        assert_eq!(
            load_as(&unmarked, Encoding::Utf16Le).unwrap().encoding,
            Encoding::Utf16LeNoBom
        );
    }

    #[test]
    fn reports_a_missing_file_clearly() {
        let dir = tempdir().unwrap();
        let err = load(&dir.path().join("nope.txt")).unwrap_err();

        assert!(matches!(err, LoadError::Read { .. }));
        assert!(err.to_string().contains("nope.txt"));
    }

    #[test]
    fn round_trips_through_save() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("g.txt");
        let original = "line one\nline two\n";

        atomic_write(&path, original.as_bytes(), SaveOptions::default()).unwrap();

        assert_eq!(load(&path).unwrap().text, original);
    }
}
