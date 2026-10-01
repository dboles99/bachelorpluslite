//! Encoding and line-ending model.
//!
//! Both are surfaced in the status bar (specs.md section 3) and both are
//! round-trip properties of a document: opening a CRLF file and saving it
//! must not silently convert it to LF.

/// Text encoding of a document as it exists on disk.
///
/// **Every value is something a file can be saved back as** (ADR-0085): a
/// document remembers how it arrived, and Save writes it that way unless the
/// user chooses otherwise. That is why a byte-order mark's presence is part
/// of the value rather than a detail of reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Encoding {
    #[default]
    Utf8,
    /// UTF-8 with a leading byte-order mark. Tracked separately because the
    /// BOM must be written back if it was there when the file was opened.
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    /// UTF-16 found by its pattern rather than by a mark, and saved back
    /// without one: adding a mark to a file that had none changes its first
    /// two bytes, which is the kind of change a tool reading it notices.
    Utf16LeNoBom,
    Utf16BeNoBom,
    /// A legacy encoding, by its name in the WHATWG Encoding Standard --
    /// `windows-1252`, `Shift_JIS`, `GBK` and the rest (ADR-0085).
    ///
    /// A name rather than a type, so this crate depends on no encoding
    /// library: `bp-files` turns the name into an encoder, and the name is
    /// also what the status bar shows. `'static` because every name comes from
    /// that standard's own table.
    Legacy(&'static str),
}

impl Encoding {
    /// Detect from a leading byte-order mark. Absent a BOM this returns
    /// [`Encoding::Utf8`]; it does not attempt statistical detection.
    pub fn detect_bom(bytes: &[u8]) -> Self {
        if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
            Self::Utf8Bom
        } else if bytes.starts_with(&[0xFF, 0xFE]) {
            Self::Utf16Le
        } else if bytes.starts_with(&[0xFE, 0xFF]) {
            Self::Utf16Be
        } else {
            Self::Utf8
        }
    }

    /// The BOM to write back for this encoding, if any.
    pub const fn bom(self) -> &'static [u8] {
        match self {
            Self::Utf8Bom => &[0xEF, 0xBB, 0xBF],
            Self::Utf16Le => &[0xFF, 0xFE],
            Self::Utf16Be => &[0xFE, 0xFF],
            Self::Utf8 | Self::Utf16LeNoBom | Self::Utf16BeNoBom | Self::Legacy(_) => &[],
        }
    }

    /// Status-bar label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Utf8 => "UTF-8",
            Self::Utf8Bom => "UTF-8 BOM",
            Self::Utf16Le => "UTF-16 LE",
            Self::Utf16Be => "UTF-16 BE",
            Self::Utf16LeNoBom => "UTF-16 LE (no BOM)",
            Self::Utf16BeNoBom => "UTF-16 BE (no BOM)",
            // The standard's own name, which is also what Notepad++ and
            // every browser call it -- so a person searching for what their
            // file is in finds the same word.
            Self::Legacy(name) => name,
        }
    }

    /// Whether every character a document can hold can be written in this
    /// encoding. A legacy code page cannot, and Save checks before writing.
    pub const fn is_unicode(self) -> bool {
        !matches!(self, Self::Legacy(_))
    }
}

/// Line-ending convention of a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    Lf,
    CrLf,
}

impl LineEnding {
    /// The convention a new document gets on this platform.
    pub const fn platform_default() -> Self {
        if cfg!(windows) { Self::CrLf } else { Self::Lf }
    }

    /// Detect from content by majority.
    ///
    /// A file with no line break at all yields [`None`], which the caller
    /// should resolve to [`Self::platform_default`]. Reporting a guess for an
    /// empty file would put a wrong value in the status bar.
    pub fn detect(text: &str) -> Option<Self> {
        let crlf = text.matches("\r\n").count();
        // Every CRLF also contains an LF, so subtract to get bare LFs.
        let lf = text.matches('\n').count() - crlf;
        match (crlf, lf) {
            (0, 0) => None,
            _ if crlf >= lf => Some(Self::CrLf),
            _ => Some(Self::Lf),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::CrLf => "\r\n",
        }
    }

    /// Status-bar label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Lf => "LF",
            Self::CrLf => "CRLF",
        }
    }
}

impl Default for LineEnding {
    fn default() -> Self {
        Self::platform_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_utf8_bom() {
        assert_eq!(
            Encoding::detect_bom(&[0xEF, 0xBB, 0xBF, b'h', b'i']),
            Encoding::Utf8Bom
        );
        assert_eq!(Encoding::detect_bom(b"hi"), Encoding::Utf8);
        // A truncated BOM prefix must not be mistaken for one.
        assert_eq!(Encoding::detect_bom(&[0xEF, 0xBB]), Encoding::Utf8);
    }

    #[test]
    fn empty_and_unbroken_text_has_no_detectable_line_ending() {
        assert_eq!(LineEnding::detect(""), None);
        assert_eq!(LineEnding::detect("no breaks here"), None);
    }

    #[test]
    fn detects_by_majority() {
        assert_eq!(LineEnding::detect("a\r\nb\r\nc"), Some(LineEnding::CrLf));
        assert_eq!(LineEnding::detect("a\nb\nc"), Some(LineEnding::Lf));
        // Mixed, LF-dominant.
        assert_eq!(LineEnding::detect("a\r\nb\nc\nd"), Some(LineEnding::Lf));
        // Mixed, CRLF-dominant.
        assert_eq!(LineEnding::detect("a\r\nb\r\nc\nd"), Some(LineEnding::CrLf));
    }

    #[test]
    fn crlf_is_not_double_counted_as_lf() {
        // The subtraction in `detect` is the whole reason this passes.
        assert_eq!(LineEnding::detect("a\r\nb"), Some(LineEnding::CrLf));
    }
}
