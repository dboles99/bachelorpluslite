//! Operations on structured data: validate, format, minify, sort, count.
//!
//! specs.md section 9. Every operation here is a pure `&str -> Result<String>`
//! so it can be tested exhaustively and reused by a future command line
//! without dragging the UI along.
//!
//! Two rules run through all of it:
//!
//! * **Never silently change the user's data.** An operation either produces
//!   the requested result or reports why it could not. Formatting that quietly
//!   drops a duplicate key, or a validate that "fixes" as it goes, is worse
//!   than an error.
//! * **Errors say where.** "Invalid JSON" is not actionable; line and column
//!   are.

#![forbid(unsafe_code)]

use thiserror::Error;

/// Crate identity used by workspace smoke tests and diagnostics.
pub const CRATE_NAME: &str = "bp-data";

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DataError {
    #[error("line {line}, column {column}: {message}")]
    Syntax {
        line: usize,
        column: usize,
        message: String,
    },

    #[error("{0}")]
    Other(String),

    #[error("record {record}: {message}")]
    Record { record: usize, message: String },
}

impl DataError {
    fn from_json(e: &serde_json::Error) -> Self {
        Self::Syntax {
            line: e.line(),
            column: e.column(),
            message: e.to_string(),
        }
    }
}

/// Indentation used when formatting. Two spaces, matching the repository's
/// own conventions and the most common house style for JSON.
const INDENT: &str = "  ";

// --- JSON ---------------------------------------------------------------

/// Check that `text` is valid JSON, reporting where it is not.
pub fn json_validate(text: &str) -> Result<(), DataError> {
    serde_json::from_str::<serde_json::Value>(text)
        .map(|_| ())
        .map_err(|e| DataError::from_json(&e))
}

/// Pretty-print JSON.
pub fn json_format(text: &str) -> Result<String, DataError> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| DataError::from_json(&e))?;
    let mut out = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(INDENT.as_bytes());
    let mut ser = serde_json::Serializer::with_formatter(&mut out, formatter);
    serde::Serialize::serialize(&value, &mut ser).map_err(|e| DataError::Other(e.to_string()))?;
    String::from_utf8(out).map_err(|e| DataError::Other(e.to_string()))
}

/// Strip all optional whitespace from JSON.
pub fn json_minify(text: &str) -> Result<String, DataError> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| DataError::from_json(&e))?;
    serde_json::to_string(&value).map_err(|e| DataError::Other(e.to_string()))
}

/// Pretty-print JSON with object keys in lexicographic order.
///
/// Uses `serde_json`'s `preserve_order` being **off**, which means its map is
/// a `BTreeMap` and already sorted. Kept as a named operation because that is
/// an implementation detail that could change, and because the user asked for
/// sorting rather than for whatever the parser happens to do.
pub fn json_sort_keys(text: &str) -> Result<String, DataError> {
    json_format(text)
}

// --- JSON Lines ---------------------------------------------------------

/// What a pass over a JSONL document found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonLinesReport {
    /// Non-empty lines seen.
    pub records: usize,
    /// Records that did not parse, with the line number and the reason.
    pub malformed: Vec<(usize, String)>,
    /// Blank lines, which are legal separators but worth reporting.
    pub blank_lines: usize,
}

impl JsonLinesReport {
    pub fn is_valid(&self) -> bool {
        self.malformed.is_empty()
    }

    /// One-line summary for the status bar.
    pub fn summary(&self) -> String {
        if self.malformed.is_empty() {
            format!("{} records, all valid", self.records)
        } else {
            format!(
                "{} records, {} malformed (first at line {})",
                self.records,
                self.malformed.len(),
                self.malformed[0].0
            )
        }
    }
}

/// Validate a JSON Lines document, one record per line.
///
/// Streams line by line and never holds more than one record, so a file too
/// large to parse as a whole is still checkable.
pub fn jsonl_validate(text: &str) -> JsonLinesReport {
    let mut report = JsonLinesReport {
        records: 0,
        malformed: Vec::new(),
        blank_lines: 0,
    };

    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            report.blank_lines += 1;
            continue;
        }
        report.records += 1;
        if let Err(e) = serde_json::from_str::<serde_json::Value>(line) {
            // Report every bad record, not just the first: a malformed export
            // usually has a pattern, and one error at a time hides it.
            report.malformed.push((index + 1, e.to_string()));
        }
    }
    report
}

/// Convert JSON Lines to a JSON array.
pub fn jsonl_to_json(text: &str) -> Result<String, DataError> {
    let mut values = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let value: serde_json::Value =
            serde_json::from_str(line).map_err(|e| DataError::Record {
                record: index + 1,
                message: e.to_string(),
            })?;
        values.push(value);
    }
    json_format(&serde_json::to_string(&values).map_err(|e| DataError::Other(e.to_string()))?)
}

/// Convert a JSON array to JSON Lines.
pub fn json_to_jsonl(text: &str) -> Result<String, DataError> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| DataError::from_json(&e))?;
    let serde_json::Value::Array(items) = value else {
        return Err(DataError::Other(
            "JSON Lines needs an array at the top level".to_owned(),
        ));
    };

    let mut out = String::new();
    for item in items {
        out.push_str(&serde_json::to_string(&item).map_err(|e| DataError::Other(e.to_string()))?);
        out.push('\n');
    }
    Ok(out)
}

// --- TOML ---------------------------------------------------------------

pub fn toml_validate(text: &str) -> Result<(), DataError> {
    toml::from_str::<toml::Table>(text)
        .map(|_| ())
        .map_err(|e| DataError::Other(e.message().to_owned()))
}

pub fn toml_format(text: &str) -> Result<String, DataError> {
    let table: toml::Table =
        toml::from_str(text).map_err(|e| DataError::Other(e.message().to_owned()))?;
    toml::to_string_pretty(&table).map_err(|e| DataError::Other(e.to_string()))
}

// --- CSV ----------------------------------------------------------------

/// What a pass over a delimited file found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DelimitedReport {
    pub delimiter: char,
    pub rows: usize,
    pub columns: usize,
    /// Rows whose field count differs from the header's.
    pub ragged: Vec<usize>,
}

impl DelimitedReport {
    pub fn summary(&self) -> String {
        let name = match self.delimiter {
            ',' => "comma",
            '\t' => "tab",
            ';' => "semicolon",
            _ => "custom",
        };
        if self.ragged.is_empty() {
            format!(
                "{} rows x {} columns ({name}-separated)",
                self.rows, self.columns
            )
        } else {
            format!(
                "{} rows x {} columns ({name}-separated), {} ragged (first at row {})",
                self.rows,
                self.columns,
                self.ragged.len(),
                self.ragged[0]
            )
        }
    }
}

/// Guess the delimiter from the header line.
///
/// Whichever candidate appears most often in the first non-empty line. Naive
/// on purpose: a wrong guess here is visible and correctable, and a clever
/// heuristic that is wrong is harder to argue with.
pub fn detect_delimiter(text: &str) -> char {
    let header = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    [',', '\t', ';', '|']
        .into_iter()
        .max_by_key(|d| header.matches(*d).count())
        .filter(|d| header.contains(*d))
        .unwrap_or(',')
}

/// Describe a delimited file: shape, and any rows that do not match it.
///
/// Does not handle quoted fields containing the delimiter -- a real CSV
/// reader belongs here later. Until then this reports shape, and says so.
pub fn delimited_report(text: &str) -> DelimitedReport {
    let delimiter = detect_delimiter(text);
    let mut rows = 0;
    let mut columns = 0;
    let mut ragged = Vec::new();

    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        rows += 1;
        let count = line.split(delimiter).count();
        if rows == 1 {
            columns = count;
        } else if count != columns {
            ragged.push(index + 1);
        }
    }

    DelimitedReport {
        delimiter,
        rows,
        columns,
        ragged,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_json_passes_and_invalid_json_says_where() {
        assert!(json_validate("{\"a\": 1}").is_ok());

        let err = json_validate("{\"a\": }").unwrap_err();
        match err {
            DataError::Syntax { line, column, .. } => {
                assert_eq!(line, 1);
                assert!(column > 0, "a column of 0 would be useless");
            }
            other => panic!("expected a syntax error, got {other:?}"),
        }
    }

    #[test]
    fn syntax_errors_report_the_right_line() {
        let text = "{\n  \"a\": 1,\n  \"b\": oops\n}";
        match json_validate(text).unwrap_err() {
            DataError::Syntax { line, .. } => assert_eq!(line, 3),
            other => panic!("expected a syntax error, got {other:?}"),
        }
    }

    #[test]
    fn format_and_minify_are_inverses_in_meaning() {
        let source = "{\"b\":2,\"a\":[1,2,{\"c\":3}]}";
        let pretty = json_format(source).unwrap();
        assert!(pretty.contains('\n'), "pretty output should have lines");

        let minified = json_minify(&pretty).unwrap();
        assert_eq!(
            minified,
            json_minify(source).unwrap(),
            "round trip must not change the data"
        );
    }

    #[test]
    fn formatting_sorts_keys() {
        let pretty = json_format("{\"z\":1,\"a\":2,\"m\":3}").unwrap();
        let a = pretty.find("\"a\"").unwrap();
        let m = pretty.find("\"m\"").unwrap();
        let z = pretty.find("\"z\"").unwrap();
        assert!(a < m && m < z, "keys out of order:\n{pretty}");
    }

    #[test]
    fn formatting_preserves_numbers_exactly() {
        // Silent numeric mangling is the classic JSON tool bug.
        let source = "{\"big\":12345678901234567890,\"exact\":0.1}";
        let out = json_minify(source).unwrap();
        assert!(out.contains("12345678901234567890"), "got {out}");
        assert!(out.contains("0.1"), "got {out}");
    }

    #[test]
    fn formatting_refuses_rather_than_repairing() {
        // An operation that "fixes" as it formats would silently rewrite data.
        assert!(json_format("{\"a\": }").is_err());
        assert!(json_minify("not json at all").is_err());
    }

    #[test]
    fn jsonl_counts_records_and_finds_every_bad_one() {
        let text = "{\"a\":1}\nnot json\n{\"a\":3}\nalso bad\n";
        let report = jsonl_validate(text);

        assert_eq!(report.records, 4);
        assert_eq!(
            report.malformed.len(),
            2,
            "every bad record, not just the first"
        );
        assert_eq!(report.malformed[0].0, 2);
        assert_eq!(report.malformed[1].0, 4);
        assert!(!report.is_valid());
        assert!(report.summary().contains("line 2"));
    }

    #[test]
    fn jsonl_ignores_blank_lines_but_counts_them() {
        let report = jsonl_validate("{\"a\":1}\n\n{\"a\":2}\n\n");
        assert_eq!(report.records, 2);
        assert_eq!(report.blank_lines, 2);
        assert!(report.is_valid());
    }

    #[test]
    fn jsonl_and_json_convert_both_ways() {
        let jsonl = "{\"a\":1}\n{\"a\":2}\n";
        let json = jsonl_to_json(jsonl).unwrap();
        assert!(json.starts_with('['));

        let back = json_to_jsonl(&json).unwrap();
        assert_eq!(back, jsonl);
    }

    #[test]
    fn jsonl_conversion_names_the_offending_record() {
        match jsonl_to_json("{\"a\":1}\nbroken\n").unwrap_err() {
            DataError::Record { record, .. } => assert_eq!(record, 2),
            other => panic!("expected a record error, got {other:?}"),
        }
    }

    #[test]
    fn json_to_jsonl_needs_an_array() {
        let err = json_to_jsonl("{\"a\":1}").unwrap_err();
        assert!(err.to_string().contains("array"));
    }

    #[test]
    fn toml_validates_and_formats() {
        assert!(toml_validate("a = 1\n").is_ok());
        assert!(toml_validate("a = = 1").is_err());

        let out = toml_format("b=2\na=1\n").unwrap();
        assert!(out.contains("a = 1"), "got {out}");
    }

    #[test]
    fn delimiters_are_detected_from_the_header() {
        assert_eq!(detect_delimiter("a,b,c\n1,2,3"), ',');
        assert_eq!(detect_delimiter("a\tb\tc\n1\t2\t3"), '\t');
        assert_eq!(detect_delimiter("a;b;c"), ';');
        assert_eq!(detect_delimiter("no delimiters here"), ',', "sane default");
    }

    #[test]
    fn delimited_reports_shape_and_ragged_rows() {
        let text = "a,b,c\n1,2,3\n4,5\n6,7,8\n";
        let report = delimited_report(text);

        assert_eq!(report.rows, 4);
        assert_eq!(report.columns, 3);
        assert_eq!(report.ragged, vec![3], "row 3 is short");
        assert!(report.summary().contains("ragged"));
    }

    #[test]
    fn a_clean_table_reports_no_ragged_rows() {
        let report = delimited_report("a,b\n1,2\n3,4\n");
        assert!(report.ragged.is_empty());
        assert_eq!(report.summary(), "3 rows x 2 columns (comma-separated)");
    }

    #[test]
    fn empty_input_does_not_panic_anywhere() {
        assert!(json_validate("").is_err());
        assert_eq!(jsonl_validate("").records, 0);
        assert_eq!(delimited_report("").rows, 0);
        assert!(toml_validate("").is_ok(), "an empty TOML table is valid");
    }
}
