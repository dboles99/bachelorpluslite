//! Format detection and profiles.
//!
//! specs.md section 7 lists the formats BachelorPad+ handles first-class and
//! the profiles they group into. This crate answers two questions and nothing
//! else: *what is this file*, and *what class of thing is it*. It parses
//! nothing -- `bp-data` does that -- so detection stays cheap enough to run
//! on every open and every Save As.
//!
//! Extension first, content second. An extension is the user's stated
//! intent and is right nearly always; content sniffing exists for the file
//! with no extension at all, and for the one named `.txt` that is obviously
//! JSON.

#![forbid(unsafe_code)]

use std::path::Path;

/// Crate identity used by workspace smoke tests and diagnostics.
pub const CRATE_NAME: &str = "bp-formats";

/// How much of a file to look at when sniffing. Enough to see a root token
/// and a line or two, cheap enough to do on every open.
pub const SNIFF_BYTES: usize = 1024;

/// The class of thing a format is, which decides what the UI offers.
///
/// specs.md section 7. A profile rather than a per-format switch: "can this
/// be pretty-printed" is a question about structured data in general, not
/// about JSON specifically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Profile {
    PlainText,
    Markdown,
    StructuredData,
    TabularData,
    SourceCode,
    Notebook,
    Log,
}

impl Profile {
    /// Whether the Data menu's operations apply.
    pub const fn is_data(self) -> bool {
        matches!(self, Self::StructuredData | Self::TabularData)
    }
}

/// A format BachelorPad+ recognises.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    PlainText,
    Markdown,
    Json,
    JsonLines,
    Yaml,
    Toml,
    Ini,
    Xml,
    Html,
    Css,
    Csv,
    Tsv,
    Rust,
    Python,
    PowerShell,
    Shell,
    Sql,
    JavaScript,
    TypeScript,
    Log,
    Notebook,
}

impl Format {
    /// Short label for the status bar.
    pub const fn label(self) -> &'static str {
        match self {
            Self::PlainText => "TXT",
            Self::Markdown => "Markdown",
            Self::Json => "JSON",
            Self::JsonLines => "JSONL",
            Self::Yaml => "YAML",
            Self::Toml => "TOML",
            Self::Ini => "INI",
            Self::Xml => "XML",
            Self::Html => "HTML",
            Self::Css => "CSS",
            Self::Csv => "CSV",
            Self::Tsv => "TSV",
            Self::Rust => "Rust",
            Self::Python => "Python",
            Self::PowerShell => "PowerShell",
            Self::Shell => "Shell",
            Self::Sql => "SQL",
            Self::JavaScript => "JavaScript",
            Self::TypeScript => "TypeScript",
            Self::Log => "LOG",
            Self::Notebook => "Notebook",
        }
    }

    pub const fn profile(self) -> Profile {
        match self {
            Self::PlainText => Profile::PlainText,
            Self::Markdown => Profile::Markdown,
            Self::Json | Self::JsonLines | Self::Yaml | Self::Toml | Self::Ini | Self::Xml => {
                Profile::StructuredData
            }
            Self::Csv | Self::Tsv => Profile::TabularData,
            Self::Html
            | Self::Css
            | Self::Rust
            | Self::Python
            | Self::PowerShell
            | Self::Shell
            | Self::Sql
            | Self::JavaScript
            | Self::TypeScript => Profile::SourceCode,
            Self::Log => Profile::Log,
            Self::Notebook => Profile::Notebook,
        }
    }

    /// The extension a new file of this format gets.
    pub const fn default_extension(self) -> &'static str {
        match self {
            Self::PlainText => "txt",
            Self::Markdown => "md",
            Self::Json => "json",
            Self::JsonLines => "jsonl",
            Self::Yaml => "yaml",
            Self::Toml => "toml",
            Self::Ini => "ini",
            Self::Xml => "xml",
            Self::Html => "html",
            Self::Css => "css",
            Self::Csv => "csv",
            Self::Tsv => "tsv",
            Self::Rust => "rs",
            Self::Python => "py",
            Self::PowerShell => "ps1",
            Self::Shell => "sh",
            Self::Sql => "sql",
            Self::JavaScript => "js",
            Self::TypeScript => "ts",
            Self::Log => "log",
            Self::Notebook => "ipynb",
        }
    }

    /// Match a file extension, case-insensitively.
    pub fn from_extension(ext: &str) -> Option<Self> {
        Some(
            match ext.trim_start_matches('.').to_ascii_lowercase().as_str() {
                "txt" | "text" => Self::PlainText,
                "md" | "markdown" => Self::Markdown,
                "json" => Self::Json,
                "jsonl" | "ndjson" => Self::JsonLines,
                "yaml" | "yml" => Self::Yaml,
                "toml" => Self::Toml,
                "ini" | "cfg" | "conf" => Self::Ini,
                "xml" => Self::Xml,
                "html" | "htm" => Self::Html,
                "css" => Self::Css,
                "csv" => Self::Csv,
                "tsv" | "tab" => Self::Tsv,
                "rs" => Self::Rust,
                "py" | "pyw" => Self::Python,
                "ps1" | "psm1" | "psd1" => Self::PowerShell,
                "sh" | "bash" | "zsh" => Self::Shell,
                "sql" => Self::Sql,
                "js" | "mjs" | "cjs" => Self::JavaScript,
                "ts" | "tsx" => Self::TypeScript,
                "log" => Self::Log,
                "ipynb" => Self::Notebook,
                _ => return None,
            },
        )
    }
}

/// Detect from a path and the start of the content.
///
/// The extension wins when it is recognised: it is what the user said the
/// file is, and second-guessing that is how an editor ends up "helpfully"
/// treating a `.txt` note as source code.
///
/// Content sniffing covers the rest: no extension, or one we do not know.
pub fn detect(path: Option<&Path>, content: &str) -> Format {
    if let Some(format) = path
        .and_then(Path::extension)
        .and_then(|e| e.to_str())
        .and_then(Format::from_extension)
    {
        return format;
    }
    sniff(content)
}

/// Guess a format from content alone.
///
/// Conservative: anything unrecognised is plain text, because being wrong
/// about a note is worse than being unhelpful about it.
pub fn sniff(content: &str) -> Format {
    let head: String = content.chars().take(SNIFF_BYTES).collect();
    let trimmed = head.trim_start();

    if trimmed.is_empty() {
        return Format::PlainText;
    }

    if trimmed.starts_with("#!") {
        let first_line = trimmed.lines().next().unwrap_or_default();
        if first_line.contains("python") {
            return Format::Python;
        }
        if first_line.contains("pwsh") || first_line.contains("powershell") {
            return Format::PowerShell;
        }
        return Format::Shell;
    }

    if trimmed.starts_with("<?xml") {
        return Format::Xml;
    }
    if trimmed.starts_with("<!DOCTYPE html") || trimmed.starts_with("<html") {
        return Format::Html;
    }

    // A JSON document has a single root value; JSON Lines has one per line.
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return if looks_like_json_lines(trimmed) {
            Format::JsonLines
        } else {
            Format::Json
        };
    }

    if trimmed.starts_with("---\n") || trimmed.starts_with("---\r\n") {
        return Format::Yaml;
    }

    Format::PlainText
}

/// Two or more lines that each independently start a JSON object or array.
///
/// A pretty-printed JSON document has exactly one such line; a JSONL file has
/// one per record.
fn looks_like_json_lines(head: &str) -> bool {
    head.lines()
        .filter(|l| !l.trim().is_empty())
        .take(4)
        .filter(|l| {
            let t = l.trim_start();
            t.starts_with('{') || t.starts_with('[')
        })
        .count()
        >= 2
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn extensions_map_to_formats() {
        assert_eq!(Format::from_extension("json"), Some(Format::Json));
        assert_eq!(Format::from_extension(".JSON"), Some(Format::Json));
        assert_eq!(Format::from_extension("yml"), Some(Format::Yaml));
        assert_eq!(Format::from_extension("ndjson"), Some(Format::JsonLines));
        assert_eq!(Format::from_extension("wat"), None);
    }

    #[test]
    fn the_extension_beats_the_content() {
        // A note that happens to start with a brace is still a note. An
        // editor that overrides the user's stated intent is a nuisance.
        assert_eq!(
            detect(Some(&p("note.txt")), "{\"looks\": \"like json\"}"),
            Format::PlainText
        );
    }

    #[test]
    fn content_decides_when_there_is_no_usable_extension() {
        assert_eq!(detect(Some(&p("dump")), "{\"a\":1}"), Format::Json);
        assert_eq!(detect(None, "#!/bin/bash\necho hi"), Format::Shell);
        assert_eq!(
            detect(Some(&p("f.unknown")), "<?xml version=\"1.0\"?>"),
            Format::Xml
        );
    }

    #[test]
    fn shebangs_distinguish_interpreters() {
        assert_eq!(sniff("#!/usr/bin/env python3\n"), Format::Python);
        assert_eq!(sniff("#!/usr/bin/env pwsh\n"), Format::PowerShell);
        assert_eq!(sniff("#!/bin/sh\n"), Format::Shell);
    }

    #[test]
    fn json_lines_is_told_apart_from_json() {
        let json = "{\n  \"a\": 1,\n  \"b\": 2\n}";
        let jsonl = "{\"a\":1}\n{\"a\":2}\n{\"a\":3}\n";

        assert_eq!(sniff(json), Format::Json, "one root value");
        assert_eq!(sniff(jsonl), Format::JsonLines, "one per line");
    }

    #[test]
    fn unrecognised_content_stays_plain_text() {
        // Being unhelpful beats being wrong about someone's note.
        assert_eq!(sniff("Just some prose, really."), Format::PlainText);
        assert_eq!(sniff(""), Format::PlainText);
        assert_eq!(sniff("   \n\n  "), Format::PlainText);
    }

    #[test]
    fn profiles_group_formats_sensibly() {
        assert_eq!(Format::Json.profile(), Profile::StructuredData);
        assert_eq!(Format::Csv.profile(), Profile::TabularData);
        assert_eq!(Format::Rust.profile(), Profile::SourceCode);
        assert_eq!(Format::Notebook.profile(), Profile::Notebook);
        assert!(Format::Toml.profile().is_data());
        assert!(!Format::Markdown.profile().is_data());
    }

    #[test]
    fn every_format_round_trips_through_its_own_extension() {
        // A format whose default extension does not map back to it would make
        // "Save As" change the document's type.
        for format in [
            Format::PlainText,
            Format::Markdown,
            Format::Json,
            Format::JsonLines,
            Format::Yaml,
            Format::Toml,
            Format::Ini,
            Format::Xml,
            Format::Html,
            Format::Css,
            Format::Csv,
            Format::Tsv,
            Format::Rust,
            Format::Python,
            Format::PowerShell,
            Format::Shell,
            Format::Sql,
            Format::JavaScript,
            Format::TypeScript,
            Format::Log,
            Format::Notebook,
        ] {
            assert_eq!(
                Format::from_extension(format.default_extension()),
                Some(format),
                "{} does not round-trip",
                format.label()
            );
        }
    }

    #[test]
    fn sniffing_a_huge_line_stays_bounded() {
        // Detection runs on every open; it must not scale with file size.
        let huge = format!("{{\"a\":\"{}\"}}", "x".repeat(5_000_000));
        assert_eq!(sniff(&huge), Format::Json);
    }

    #[test]
    fn multibyte_content_does_not_split_a_character() {
        // `take(SNIFF_BYTES)` over chars, not bytes -- slicing bytes here
        // would panic on a boundary.
        let text = "日本語".repeat(2000);
        assert_eq!(sniff(&text), Format::PlainText);
    }
}
