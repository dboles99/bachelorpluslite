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
/// UTF-16 is recognised only by its byte-order mark. A UTF-16 file without
/// one is indistinguishable from binary without statistical guessing, and a
/// wrong guess is the mangling this crate refuses.
pub fn load(path: &Path) -> Result<LoadedFile, LoadError> {
    let bytes = std::fs::read(path).map_err(|source| LoadError::Read {
        path: path.to_owned(),
        source,
    })?;
    let bytes_on_disk = bytes.len() as u64;
    let encoding = Encoding::detect_bom(&bytes);
    // `Encoding::Utf8` has an empty mark, so this covers all four cases.
    let body = &bytes[encoding.bom().len()..];

    let text = match Endian::of(encoding) {
        Some(endian) => utf16::decode(body, endian).map_err(|e| match e {
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
        None => String::from_utf8(body.to_vec()).map_err(|_| LoadError::InvalidText {
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
    fn rejects_invalid_utf8() {
        let dir = tempdir().unwrap();
        let path = write_bytes(dir.path(), "f.txt", b"ok \xC3\x28 bad");

        assert!(matches!(
            load(&path).unwrap_err(),
            LoadError::InvalidText { .. }
        ));
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
