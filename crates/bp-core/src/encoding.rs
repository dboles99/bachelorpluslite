//! Encoding and line-ending model.
//!
//! Both are surfaced in the status bar (specs.md section 3) and both are
//! round-trip properties of a document: opening a CRLF file and saving it
//! must not silently convert it to LF.

/// Text encoding of a document as it exists on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Encoding {
    #[default]
    Utf8,
    /// UTF-8 with a leading byte-order mark. Tracked separately because the
    /// BOM must be written back if it was there when the file was opened.
    Utf8Bom,
    Utf16Le,
    Utf16Be,
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
            Self::Utf8 => &[],
            Self::Utf8Bom => &[0xEF, 0xBB, 0xBF],
            Self::Utf16Le => &[0xFF, 0xFE],
            Self::Utf16Be => &[0xFE, 0xFF],
        }
    }

    /// Status-bar label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Utf8 => "UTF-8",
            Self::Utf8Bom => "UTF-8 BOM",
            Self::Utf16Le => "UTF-16 LE",
            Self::Utf16Be => "UTF-16 BE",
        }
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
