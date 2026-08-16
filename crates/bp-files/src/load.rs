//! Reading a file into a document.

use std::io;
use std::path::{Path, PathBuf};

use bp_core::{Encoding, LineEnding};
use thiserror::Error;

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

    #[error("{} is {}, which this build cannot open yet", .path.display(), .encoding.label())]
    UnsupportedEncoding { path: PathBuf, encoding: Encoding },
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
/// UTF-16 is detected and reported as [`LoadError::UnsupportedEncoding`]
/// rather than being decoded. Refusing is deliberate: a lossy decode would
/// let the user edit and save mangled text over their original.
pub fn load(path: &Path) -> Result<LoadedFile, LoadError> {
    let bytes = std::fs::read(path).map_err(|source| LoadError::Read {
        path: path.to_owned(),
        source,
    })?;
    let bytes_on_disk = bytes.len() as u64;
    let encoding = Encoding::detect_bom(&bytes);

    let body = match encoding {
        Encoding::Utf8 => &bytes[..],
        Encoding::Utf8Bom => &bytes[encoding.bom().len()..],
        Encoding::Utf16Le | Encoding::Utf16Be => {
            return Err(LoadError::UnsupportedEncoding {
                path: path.to_owned(),
                encoding,
            });
        }
    };

    let text = String::from_utf8(body.to_vec()).map_err(|_| LoadError::InvalidText {
        path: path.to_owned(),
        encoding,
    })?;

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
    fn refuses_utf16_rather_than_mangling_it() {
        let dir = tempdir().unwrap();
        let path = write_bytes(dir.path(), "e.txt", b"\xFF\xFEh\0i\0");

        let err = load(&path).unwrap_err();

        assert!(matches!(
            err,
            LoadError::UnsupportedEncoding {
                encoding: Encoding::Utf16Le,
                ..
            }
        ));
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
