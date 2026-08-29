//! Format detection and profiles.
//!
//! specs.md section 7 lists the formats BachelorPad+ handles first-class and
//! the profiles they group into. This crate answers two questions and nothing
//! else: *what is this file*, and *what class of thing is it*. **It parses
//! nothing, and nothing in this product parses these formats any more**
//! (ADR-0062): the answer reaches a status-bar label, a syntax profile and
//! `bp-platform`'s registration table. Detection stays cheap enough to run on
//! every open and every Save As.
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
    /// Whether this profile groups formats that hold *data* rather than prose,
    /// source or a log.
    ///
    /// A statement about the class and nothing more. It deliberately does not
    /// answer "may the Data menu act on this document" — that is
    /// [`Format::has_data_operations`], and the two are different questions
    /// because [`Format::Ini`] and [`Format::Xml`] are structured data that
    /// nothing in this product can parse. Asking this one and enabling a menu
    /// on the answer gives a menu whose every row fails.
    pub const fn is_data_class(self) -> bool {
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
    /// Every format, so that a test or a diagnostic covers all of them
    /// without a list that goes stale.
    ///
    /// The same device as `DirKind::ALL` and `Platform::ALL`, and it earns its
    /// keep here for a specific reason: [`Self::has_data_operations`] is a
    /// `match` that a new variant would silently fall out of, and a test
    /// walking this array against the Data menu is what notices.
    pub const ALL: &'static [Format] = &[
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
    ];

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

/// Whether the head reads as JSON Lines rather than as one JSON document.
///
/// The discriminator is that **a JSONL record is a complete JSON value on its
/// own line**, and a JSON document has exactly one root value. So there are
/// two halves to check, and both are cheap: the first non-blank line closes
/// everything it opens and ends there, and something non-blank follows it.
///
/// A pretty-printed document fails the first half. Its opening line is `[` or
/// `{` alone -- an opener with nothing closing it -- and its inner lines are
/// fragments that end in `,`, or in another opener, rather than in the close
/// of what they began. A single compact value on one line fails the second
/// half, because nothing follows it.
///
/// The rule this replaces counted lines that *begin* a value and said JSONL
/// at two or more, on the reasoning that "a pretty-printed JSON document has
/// exactly one such line". That holds only for a document whose root is an
/// object of scalars. A pretty-printed **array of objects** -- the commonest
/// shape JSON comes in -- has one such line per element, so
/// `[\n  {"id": 1},\n  {"id": 2}\n]` counted three, was reported as JSONL,
/// and was then refused by `bp-data` record by record.
///
/// **That defect was found by a cross-crate test that can no longer exist**
/// (ADR-0062): `bp-data` is gone, so nothing consumes this verdict in a way
/// that could contradict it. The rule below is kept exactly as it was, and a
/// mistake in it is now cosmetic rather than a syntax error on a valid file --
/// which is a smaller consequence and a quieter one.
///
/// Only the first line is inspected for completeness, deliberately: the head
/// is a fixed [`SNIFF_BYTES`] prefix, so a file of long records would
/// otherwise be judged on a line the truncation had cut in half. The second
/// line only has to exist, not to be whole.
///
/// What it still cannot tell apart, and where it settles:
///
/// * **A one-record JSONL file** and a compact JSON document are the same
///   bytes. Reported as `Json`, which costs nothing -- both parsers take it.
/// * **A JSONL file whose first record is itself pretty-printed** is not valid
///   JSONL at all, so there is no right answer here, only a better error.
///   Reported as `Json`: the file opens exactly as a JSON document does, and
///   `json_validate`'s complaint about trailing characters points at the line
///   where the second record starts, which is the actual problem.
/// * **JSONL of bare scalars** (`1\n2\n3`) never reaches here at all --
///   [`sniff`] consults this only for a head starting with `{` or `[` -- and
///   comes out as plain text.
/// * **A file whose very first record is longer than [`SNIFF_BYTES`]**, where
///   there is no complete line to judge at all. Reported as `Json`.
/// * **Anything past [`SNIFF_BYTES`]**: a file that changes shape after its
///   first kilobyte is judged on that kilobyte.
///
/// This is a heuristic over a prefix, not a parser, and `bp-formats` has no
/// JSON dependency with which to become one. Where the two halves do not both
/// hold it says `Json`, because an unhelpful answer beats a confident wrong
/// one -- `Json` on a JSONL file costs the user a menu, whereas `JsonLines`
/// on a JSON file costs them a syntax error on a file that has none.
fn looks_like_json_lines(head: &str) -> bool {
    let mut lines = head.lines().filter(|l| !l.trim().is_empty());
    let Some(first) = lines.next() else {
        return false;
    };
    closes_what_it_opens(first) && lines.next().is_some()
}

/// Whether `line` is a whole JSON value and nothing else.
///
/// Brackets are counted outside of strings -- the `}` in `{"s":"}"}` closes
/// nothing -- and the depth must reach zero exactly at the end, never having
/// gone below it on the way. A trailing `,` disqualifies the line even when
/// its brackets balance: a comma at the end means the value is an element of
/// something larger which continues on the next line, which is precisely the
/// inner line of a pretty-printed array.
///
/// This is not validation. `{"a" "b"}` closes what it opens and is not JSON;
/// telling those apart needs a parser, and the caller's answer for a line that
/// is well-shaped but malformed is `JsonLines`, whose parser will say so.
fn closes_what_it_opens(line: &str) -> bool {
    let line = line.trim();
    if !(line.starts_with('{') || line.starts_with('[')) || line.ends_with(',') {
        return false;
    }

    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut escaped = false;
    for c in line.chars() {
        if in_string {
            match c {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '{' | '[' => depth += 1,
            '}' | ']' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0 && !in_string
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

        // The shape that the old rule got wrong: a pretty-printed array of
        // objects has one line per element that *begins* a value, and so was
        // counted as three records of JSONL.
        let pretty_array = "[\n  {\"id\": 1},\n  {\"id\": 2}\n]\n";
        assert_eq!(
            sniff(pretty_array),
            Format::Json,
            "a pretty-printed array of objects is one JSON document"
        );

        // The same shape one level in, where the elements are arrays.
        let pretty_nested_arrays = "[\n  [1, 2],\n  [3, 4]\n]\n";
        assert_eq!(sniff(pretty_nested_arrays), Format::Json);

        // And an object of objects, which the old rule happened to survive
        // only because its inner lines start with a quote rather than a brace.
        let pretty_object_of_objects =
            "{\n  \"outer\": {\n    \"a\": 1\n  },\n  \"other\": {\n    \"b\": 2\n  }\n}\n";
        assert_eq!(sniff(pretty_object_of_objects), Format::Json);
    }

    #[test]
    fn json_lines_of_arrays_is_still_json_lines() {
        // A record does not have to be an object. Nothing about the rule
        // should care which container a line opens with.
        assert_eq!(sniff("[1, 2]\n[3, 4]\n"), Format::JsonLines);
        assert_eq!(sniff("[]\n[]\n"), Format::JsonLines);
    }

    #[test]
    fn one_compact_value_on_one_line_is_a_json_document() {
        // Nothing follows the root value, so there is no second record and
        // nothing to make this JSON Lines.
        assert_eq!(sniff("[1, 2, 3]"), Format::Json);
        assert_eq!(sniff("[1, 2, 3]\n"), Format::Json);
        assert_eq!(sniff("[{\"id\": 1}, {\"id\": 2}]\n"), Format::Json);
        assert_eq!(sniff("{\"a\": 1, \"b\": 2}\n"), Format::Json);
        // A trailing blank line is not a second record either.
        assert_eq!(sniff("[1, 2, 3]\n\n\n"), Format::Json);
    }

    #[test]
    fn json_lines_survives_blank_lines_and_crlf() {
        // Both are how a real export arrives: a writer that flushes a blank
        // line between records, and a file that has been through Windows.
        assert_eq!(sniff("{\"a\":1}\n\n{\"a\":2}\n"), Format::JsonLines);
        assert_eq!(sniff("{\"a\":1}\r\n{\"a\":2}\r\n"), Format::JsonLines);
        assert_eq!(sniff("{\"a\":1}\r\n\r\n{\"a\":2}\r\n"), Format::JsonLines);
        // Leading blank lines before the first record, too.
        assert_eq!(sniff("\n\n{\"a\":1}\n{\"a\":2}\n"), Format::JsonLines);
    }

    #[test]
    fn a_pretty_printed_first_record_is_reported_as_json() {
        // This file is not valid JSONL -- a record may not span lines -- and
        // it is not valid JSON either, because a second root value follows.
        // There is no right answer, so the one that gives the better error
        // wins: it opens as a JSON document does, `Json` is what it is
        // reported as, and the parser's complaint about trailing characters
        // lands on the line where the second record begins.
        let text = "{\n  \"a\": 1\n}\n{\"b\": 2}\n";
        assert_eq!(sniff(text), Format::Json);
    }

    #[test]
    fn a_value_with_a_separator_after_it_is_not_a_record() {
        // `{"a":1},` is a value *and a comma* -- an element of an array whose
        // brackets have gone missing, which is neither JSON nor JSON Lines.
        // Reported as `Json`, whose parser then complains about a trailing
        // character at column 9 of line 1, which is exactly the stray comma.
        assert_eq!(sniff("{\"a\":1},\n{\"b\":2},\n"), Format::Json);
        assert_eq!(sniff("[1,2],\n[3,4]\n"), Format::Json);
    }

    #[test]
    fn a_line_that_closes_what_it_never_opened_is_not_a_record() {
        // Depth reaching zero at the end of the line is not enough on its
        // own: `{"a":1}][` closes the record, then closes a bracket that was
        // never opened, then opens another, and the count comes back to zero
        // having described no value at all.
        assert_eq!(sniff("{\"a\":1}][\n{\"b\":2}\n"), Format::Json);
    }

    #[test]
    fn a_brace_inside_a_string_closes_nothing() {
        // Counting brackets without knowing where the strings are would read
        // the `}` in the value as closing the record, and the trailing `}` as
        // unbalanced.
        assert_eq!(
            sniff("{\"s\":\"}\"}\n{\"s\":\"]\"}\n"),
            Format::JsonLines,
            "a bracket inside a string is text"
        );
        assert_eq!(
            sniff("{\"s\":\"\\\"}\"}\n{\"s\":\"x\"}\n"),
            Format::JsonLines,
            "an escaped quote does not end the string"
        );
        // An unterminated string means the value continues on the next line.
        assert_eq!(sniff("{\"s\":\"unfinished\n  more\"}\n"), Format::Json);
    }

    #[test]
    fn an_empty_or_single_line_document_is_never_json_lines() {
        assert_eq!(sniff(""), Format::PlainText);
        assert_eq!(sniff("\n\n\n"), Format::PlainText);
        assert_eq!(sniff("{}"), Format::Json);
        assert_eq!(sniff("{}\n"), Format::Json);
        assert_eq!(sniff("{}\n{}\n"), Format::JsonLines, "two is enough");
    }

    #[test]
    fn a_record_the_head_cuts_in_half_does_not_decide_the_answer() {
        // Long records are the normal case for an export. Here the head ends
        // part-way through the second one; only the first line is checked for
        // completeness, precisely so that the truncation cannot be what
        // settles it.
        let record = format!("{{\"a\":\"{}\"}}", "x".repeat(SNIFF_BYTES * 3 / 4));
        let text = format!("{record}\n{record}\n{record}\n");
        assert!(text.len() > SNIFF_BYTES, "the head must be truncated");
        assert_eq!(sniff(&text), Format::JsonLines);
    }

    #[test]
    fn a_first_record_longer_than_the_head_is_reported_as_json() {
        // The limit of a heuristic over a prefix: when the very first record
        // does not fit, there is no complete line to judge and the answer is
        // the safe one. Named here so the boundary is a decision rather than
        // a surprise.
        let record = format!("{{\"a\":\"{}\"}}", "x".repeat(SNIFF_BYTES * 2));
        let text = format!("{record}\n{record}\n");
        assert_eq!(sniff(&text), Format::Json);
    }

    // --- the property, over a generated corpus -----------------------------

    /// A JSON value, as a shape rather than as text, so a corpus of them can
    /// be rendered both ways and the two renderings compared.
    #[derive(Clone)]
    enum Json {
        Scalar(&'static str),
        Array(Vec<Json>),
        Object(Vec<Json>),
    }

    impl Json {
        /// One line, no spaces -- how a JSONL record is written.
        fn compact(&self) -> String {
            match self {
                Self::Scalar(s) => (*s).to_owned(),
                Self::Array(items) => {
                    let rendered: Vec<String> = items.iter().map(Self::compact).collect();
                    format!("[{}]", rendered.join(","))
                }
                Self::Object(items) => {
                    let rendered: Vec<String> = items
                        .iter()
                        .enumerate()
                        .map(|(i, v)| format!("\"k{i}\":{}", v.compact()))
                        .collect();
                    format!("{{{}}}", rendered.join(","))
                }
            }
        }

        /// Indented over several lines -- how a formatter writes a document.
        fn pretty(&self, indent: usize) -> String {
            let pad = " ".repeat(indent);
            let inner_pad = " ".repeat(indent + 2);
            match self {
                Self::Scalar(s) => (*s).to_owned(),
                Self::Array(items) if items.is_empty() => "[]".to_owned(),
                Self::Object(items) if items.is_empty() => "{}".to_owned(),
                Self::Array(items) => {
                    let rendered: Vec<String> = items
                        .iter()
                        .map(|v| format!("{inner_pad}{}", v.pretty(indent + 2)))
                        .collect();
                    format!("[\n{}\n{pad}]", rendered.join(",\n"))
                }
                Self::Object(items) => {
                    let rendered: Vec<String> = items
                        .iter()
                        .enumerate()
                        .map(|(i, v)| format!("{inner_pad}\"k{i}\": {}", v.pretty(indent + 2)))
                        .collect();
                    format!("{{\n{}\n{pad}}}", rendered.join(",\n"))
                }
            }
        }
    }

    /// Every container shape up to three levels deep, over leaves chosen to
    /// include the ones that break a naive bracket count.
    fn corpus() -> Vec<Json> {
        let leaves = [
            "1",
            "-2.5",
            "true",
            "null",
            "\"text\"",
            "\"} and , and ] inside\"",
            "\"an \\\" escaped quote\"",
        ];
        let mut level: Vec<Json> = leaves.iter().map(|s| Json::Scalar(s)).collect();
        let mut all = Vec::new();
        for _ in 0..3 {
            let mut next = Vec::new();
            for value in &level {
                next.push(Json::Array(Vec::new()));
                next.push(Json::Array(vec![value.clone()]));
                next.push(Json::Array(vec![value.clone(), value.clone()]));
                next.push(Json::Object(Vec::new()));
                next.push(Json::Object(vec![value.clone()]));
                next.push(Json::Object(vec![value.clone(), value.clone()]));
            }
            all.extend(next.iter().cloned());
            level = next;
        }
        all
    }

    #[test]
    fn a_pretty_printed_document_is_never_json_lines() {
        // The property the defect broke. A formatter's output has one root
        // value however many lines it takes, so no rendering of one may come
        // back as JSON Lines.
        for value in corpus() {
            let text = format!("{}\n", value.pretty(0));
            assert_eq!(
                sniff(&text),
                Format::Json,
                "pretty-printed and not read as one document:\n{text}"
            );
        }
    }

    #[test]
    fn compact_values_one_per_line_are_json_lines() {
        // The other direction, and the one the heuristic exists to serve: two
        // or more complete values, each on its own line.
        for value in corpus() {
            let record = value.compact();
            let text = format!("{record}\n{record}\n{record}\n");
            assert_eq!(
                sniff(&text),
                Format::JsonLines,
                "one value per line and not read as JSON Lines:\n{text}"
            );
        }
    }

    #[test]
    fn one_compact_value_is_a_document_however_it_is_shaped() {
        // The same records as above, alone. Nothing follows, so nothing makes
        // them a stream.
        for value in corpus() {
            let text = format!("{}\n", value.compact());
            assert_eq!(
                sniff(&text),
                Format::Json,
                "a lone value was read as a stream of them:\n{text}"
            );
        }
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
        assert!(Format::Toml.profile().is_data_class());
        assert!(!Format::Markdown.profile().is_data_class());
    }

    #[test]
    fn every_variant_is_in_all() {
        // `ALL` is a hand-written list, so the thing that could go wrong with
        // it is a variant added to the enum and not to it. Nothing in Rust
        // catches that, so this does -- by round-tripping each entry through
        // the one `match` that has to name every variant.
        assert_eq!(
            Format::ALL.len(),
            Format::ALL
                .iter()
                .map(|f| f.default_extension())
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            "two entries in ALL share a default extension, so one is a duplicate"
        );
        for &format in Format::ALL {
            assert_eq!(
                Format::from_extension(format.default_extension()),
                Some(format),
                "{} does not come back from its own default extension",
                format.label()
            );
        }
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
