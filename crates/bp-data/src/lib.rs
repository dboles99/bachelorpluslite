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

use std::collections::HashSet;

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
    /// Lines on which a row whose field count differs from the header's
    /// begins.
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

/// One parsed row of a delimited file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// 1-based line the row begins on.
    ///
    /// Not the row's ordinal: a quoted field may contain newlines, so row 3
    /// can begin on line 7.
    pub line: usize,
    pub fields: Vec<String>,
}

/// The first non-empty record, as raw text.
///
/// Not `lines().next()`: a quoted field may contain newlines, so the first
/// record can span several lines. A quote toggles the state, which handles a
/// doubled `""` correctly by toggling twice.
fn first_record(text: &str) -> &str {
    let mut in_quotes = false;
    let mut start = 0;

    for (index, ch) in text.char_indices() {
        match ch {
            '"' => in_quotes = !in_quotes,
            '\n' if !in_quotes => {
                let record = &text[start..index];
                if !record.trim().is_empty() {
                    return record;
                }
                start = index + 1;
            }
            _ => {}
        }
    }
    &text[start..]
}

/// Occurrences of `delimiter` that are not inside a quoted field.
fn count_outside_quotes(record: &str, delimiter: char) -> usize {
    let mut in_quotes = false;
    let mut count = 0;

    for ch in record.chars() {
        match ch {
            '"' => in_quotes = !in_quotes,
            c if c == delimiter && !in_quotes => count += 1,
            _ => {}
        }
    }
    count
}

/// Guess the delimiter from the header record.
///
/// Whichever candidate appears most often outside quotes. Quoted content is
/// excluded because a single `"Doe, Jane"` in a semicolon-separated header is
/// otherwise enough to pick the wrong delimiter for the whole file.
///
/// Still deliberately naive beyond that: a wrong guess is visible and
/// correctable, and a clever heuristic that is wrong is harder to argue with.
pub fn detect_delimiter(text: &str) -> char {
    let header = first_record(text);
    [',', '\t', ';', '|']
        .into_iter()
        .map(|d| (d, count_outside_quotes(header, d)))
        .max_by_key(|(_, count)| *count)
        .filter(|(_, count)| *count > 0)
        .map_or(',', |(d, _)| d)
}

/// Parse a delimited document, honouring RFC 4180 quoting.
///
/// A quoted field may contain the delimiter, newlines, and doubled quotes
/// (`""`) standing for one literal quote. Splitting on the delimiter -- which
/// is what this used to do -- gets all three wrong, and gets them wrong
/// silently: a row reported as ragged because one field contained a comma
/// sends the user looking for a defect in their data.
pub fn delimited_rows(text: &str, delimiter: char) -> Result<Vec<Row>, DataError> {
    // The reader takes a single byte. Every delimiter `detect_delimiter` can
    // return is ASCII, but this is public and a caller may not be.
    if !delimiter.is_ascii() {
        return Err(DataError::Other(format!(
            "delimiter {delimiter:?} is not a single-byte character"
        )));
    }

    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter as u8)
        // Every row is data here. This describes a file rather than binding
        // fields to names, and treating the first row as headers would drop
        // it from the count.
        .has_headers(false)
        // Ragged rows are what the report exists to surface, not an error
        // that should stop the pass at the first one.
        .flexible(true)
        .from_reader(text.as_bytes());

    let mut rows = Vec::new();
    for result in reader.records() {
        let record = result.map_err(|e| DataError::Other(e.to_string()))?;
        rows.push(Row {
            line: record.position().map_or(0, csv::Position::line) as usize,
            fields: record.iter().map(str::to_owned).collect(),
        });
    }
    Ok(rows)
}

/// Describe a delimited file: shape, and any rows that do not match it.
pub fn delimited_report(text: &str) -> Result<DelimitedReport, DataError> {
    let delimiter = detect_delimiter(text);
    let rows = delimited_rows(text, delimiter)?;
    let columns = rows.first().map_or(0, |row| row.fields.len());

    Ok(DelimitedReport {
        delimiter,
        rows: rows.len(),
        columns,
        ragged: rows
            .iter()
            .skip(1)
            .filter(|row| row.fields.len() != columns)
            .map(|row| row.line)
            .collect(),
    })
}

// --- CSV: JSON conversion and column typing ------------------------------

/// Resolve header cells to distinct JSON object keys.
///
/// Two things can otherwise make two columns collide under one key: an empty
/// header cell, and a header name repeated later in the row. Either would
/// silently drop a column's data when the second value overwrites the
/// first in the JSON object -- exactly what the module doc forbids. The
/// rule: an empty cell becomes `_<column>` (1-based), and a name already
/// used gets its own column number appended. Both branches are naive about
/// a pathological header (a literal `_2` column sitting next to an empty
/// second cell can still collide), but every generated key is traceable
/// back to a column number, which matters more than covering every
/// adversarial header.
fn resolve_header_keys(header: &[String]) -> Vec<String> {
    let mut used = HashSet::new();
    let mut keys = Vec::with_capacity(header.len());

    for (index, cell) in header.iter().enumerate() {
        let column = index + 1;
        let key = if cell.is_empty() {
            format!("_{column}")
        } else if used.contains(cell) {
            format!("{cell}_{column}")
        } else {
            cell.clone()
        };
        used.insert(key.clone());
        keys.push(key);
    }
    keys
}

/// Map delimited rows to JSON objects keyed by the header row.
///
/// The first row is the header; every row after it becomes one object.
/// Fields are always emitted as JSON strings, never numbers or booleans --
/// see `delimited_to_json` for why -- so this only has to decide which key
/// each field goes under.
fn rows_to_objects(rows: &[Row]) -> Vec<serde_json::Value> {
    let Some(header) = rows.first() else {
        return Vec::new();
    };
    let keys = resolve_header_keys(&header.fields);

    rows.iter()
        .skip(1)
        .map(|row| {
            let mut object = serde_json::Map::new();
            for (index, key) in keys.iter().enumerate() {
                // A short row is missing data, not owed a guess at it: null
                // says "not present" without inventing content.
                let value = match row.fields.get(index) {
                    Some(field) => serde_json::Value::String(field.clone()),
                    None => serde_json::Value::Null,
                };
                object.insert(key.clone(), value);
            }
            // A long row has fields the header never named. They are still
            // the user's data, so they are kept -- under the same
            // `_<column>` scheme as an empty header cell, rather than
            // dropped on the floor.
            for (index, field) in row.fields.iter().enumerate().skip(keys.len()) {
                object.insert(
                    format!("_{}", index + 1),
                    serde_json::Value::String(field.clone()),
                );
            }
            serde_json::Value::Object(object)
        })
        .collect()
}

/// Convert a delimited document to a JSON array, one object per row.
///
/// The first row supplies the field names; see `resolve_header_keys` for how
/// a blank or repeated one is handled. Every value is emitted as a JSON
/// string, never a number or boolean, no matter how numeric it looks: a
/// part number like `007` would lose its leading zeros, and a big enough
/// integer loses precision, the moment it is read back as a JSON number.
/// `column_types` reports what a column looks like without changing what is
/// actually stored.
pub fn delimited_to_json(text: &str) -> Result<String, DataError> {
    let rows = delimited_rows(text, detect_delimiter(text))?;
    let values = serde_json::Value::Array(rows_to_objects(&rows));
    json_format(&serde_json::to_string(&values).map_err(|e| DataError::Other(e.to_string()))?)
}

/// Convert a delimited document to JSON Lines, one compact object per row.
///
/// Same header-to-key mapping as `delimited_to_json`; the two must always
/// describe the same data for the same input, which is what
/// `delimited_to_json_and_jsonl_agree_on_the_same_input` tests.
pub fn delimited_to_jsonl(text: &str) -> Result<String, DataError> {
    let rows = delimited_rows(text, detect_delimiter(text))?;
    let mut out = String::new();
    for object in rows_to_objects(&rows) {
        out.push_str(&serde_json::to_string(&object).map_err(|e| DataError::Other(e.to_string()))?);
        out.push('\n');
    }
    Ok(out)
}

/// The narrowest type every non-empty value in a column fits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnKind {
    Integer,
    Float,
    Boolean,
    /// No non-empty value to judge -- not the same as `Text`, which claims
    /// to have looked at values and found no narrower fit.
    Empty,
    Text,
}

/// What a column of a delimited file looks like.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnReport {
    pub name: String,
    pub kind: ColumnKind,
    /// Non-empty values.
    pub filled: usize,
    /// Empty values, including a row too short to reach this column.
    pub missing: usize,
}

impl ColumnReport {
    /// One-line summary for the status bar, in the same style as
    /// `DelimitedReport::summary`.
    pub fn summary(&self) -> String {
        let kind = match self.kind {
            ColumnKind::Integer => "integer",
            ColumnKind::Float => "float",
            ColumnKind::Boolean => "boolean",
            ColumnKind::Empty => "empty",
            ColumnKind::Text => "text",
        };
        if self.missing == 0 {
            format!("{}: {kind} ({} filled)", self.name, self.filled)
        } else {
            format!(
                "{}: {kind} ({} filled, {} missing)",
                self.name, self.filled, self.missing
            )
        }
    }
}

/// Whether `value` is written as an optionally-signed run of digits.
///
/// Not a `parse::<i64>` check: a column of large IDs is still an integer
/// column even if one value overflows `i64`, and what matters here is what
/// the text looks like, not whether it fits one particular machine width.
fn looks_like_integer(value: &str) -> bool {
    let digits = value.strip_prefix('-').unwrap_or(value);
    !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())
}

/// Whether `value` reads as `true`/`false`, case-insensitively.
///
/// Deliberately not `0`/`1`: those are already valid, unambiguous integers,
/// and guessing that a column of them means booleans would be exactly the
/// kind of clever-but-wrong inference this module avoids elsewhere.
fn looks_like_boolean(value: &str) -> bool {
    value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("false")
}

/// Whether `value` parses as a float and is not one of the word-like values
/// (`inf`, `nan`, ...) that `f64::from_str` also happens to accept.
///
/// A CSV cell that says "nan" almost never means the IEEE value; it means
/// someone typed the letters n-a-n. Reporting that column as `Float` would
/// be a clever misreading of the data, not a description of it.
fn looks_like_float(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    let word = lower.trim_start_matches(['+', '-']);
    if matches!(word, "inf" | "infinity" | "nan") {
        return false;
    }
    value.parse::<f64>().is_ok()
}

/// The narrowest `ColumnKind` that every value in `values` fits.
///
/// Checked narrowest first: every integer also parses as a float, so
/// checking float first would report `Float` for a plain integer column.
fn column_kind(values: &[&str]) -> ColumnKind {
    if values.is_empty() {
        ColumnKind::Empty
    } else if values.iter().all(|v| looks_like_integer(v)) {
        ColumnKind::Integer
    } else if values.iter().all(|v| looks_like_boolean(v)) {
        ColumnKind::Boolean
    } else if values.iter().all(|v| looks_like_float(v)) {
        ColumnKind::Float
    } else {
        ColumnKind::Text
    }
}

/// Report each header column's inferred type and how filled in it is.
pub fn column_types(text: &str) -> Result<Vec<ColumnReport>, DataError> {
    let rows = delimited_rows(text, detect_delimiter(text))?;
    let Some(header) = rows.first() else {
        return Ok(Vec::new());
    };
    let keys = resolve_header_keys(&header.fields);
    let data_rows = &rows[1..];

    Ok(keys
        .into_iter()
        .enumerate()
        .map(|(index, name)| {
            let cells: Vec<&str> = data_rows
                .iter()
                .map(|row| row.fields.get(index).map_or("", String::as_str))
                .collect();
            let non_empty: Vec<&str> = cells.iter().copied().filter(|c| !c.is_empty()).collect();
            let missing = cells.len() - non_empty.len();

            ColumnReport {
                name,
                kind: column_kind(&non_empty),
                filled: non_empty.len(),
                missing,
            }
        })
        .collect())
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
    fn a_quoted_field_does_not_vote_for_its_own_delimiter() {
        // Two semicolons outside quotes beat three commas inside one field.
        assert_eq!(detect_delimiter(r#"a;"x,y,z,w";c"#), ';');
    }

    #[test]
    fn delimited_reports_shape_and_ragged_rows() {
        let text = "a,b,c\n1,2,3\n4,5\n6,7,8\n";
        let report = delimited_report(text).unwrap();

        assert_eq!(report.rows, 4);
        assert_eq!(report.columns, 3);
        assert_eq!(report.ragged, vec![3], "row 3 is short");
        assert!(report.summary().contains("ragged"));
    }

    #[test]
    fn a_clean_table_reports_no_ragged_rows() {
        let report = delimited_report("a,b\n1,2\n3,4\n").unwrap();
        assert!(report.ragged.is_empty());
        assert_eq!(report.summary(), "3 rows x 2 columns (comma-separated)");
    }

    #[test]
    fn a_comma_inside_a_quoted_field_is_not_a_new_column() {
        // The defect this reader replaces: splitting on the delimiter made
        // row 2 look ragged and sent the user hunting a fault in clean data.
        let text = "name,city\n\"Doe, Jane\",Leeds\n";
        let report = delimited_report(text).unwrap();

        assert_eq!(report.columns, 2);
        assert!(report.ragged.is_empty(), "clean data must report clean");

        let rows = delimited_rows(text, ',').unwrap();
        assert_eq!(rows[1].fields, vec!["Doe, Jane", "Leeds"]);
    }

    #[test]
    fn a_doubled_quote_is_one_literal_quote() {
        let rows = delimited_rows("a\n\"He said \"\"hi\"\"\"\n", ',').unwrap();
        assert_eq!(rows[1].fields, vec![r#"He said "hi""#]);
    }

    #[test]
    fn a_newline_inside_a_quoted_field_does_not_start_a_row() {
        // Two rows across three lines -- and the line numbers must follow the
        // file, not the row count, or "ragged at row 3" points at the wrong
        // place in the editor.
        let text = "a,b\n\"line one\nline two\",x\n4\n";
        let rows = delimited_rows(text, ',').unwrap();

        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1].fields[0], "line one\nline two");
        assert_eq!(rows[1].line, 2);
        assert_eq!(rows[2].line, 4, "the short row is on line 4, not line 3");

        assert_eq!(delimited_report(text).unwrap().ragged, vec![4]);
    }

    #[test]
    fn a_non_ascii_delimiter_is_refused_rather_than_truncated() {
        assert!(delimited_rows("a\u{00A7}b", '\u{00A7}').is_err());
    }

    #[test]
    fn empty_input_does_not_panic_anywhere() {
        assert!(json_validate("").is_err());
        assert_eq!(jsonl_validate("").records, 0);
        assert_eq!(delimited_report("").unwrap().rows, 0);
        assert!(toml_validate("").is_ok(), "an empty TOML table is valid");
    }

    #[test]
    fn delimited_to_json_maps_header_names_to_values() {
        let text = "name,age\nAda,36\nGrace,85\n";
        let json = delimited_to_json(text).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(value[0]["name"], "Ada");
        assert_eq!(value[0]["age"], "36", "values stay strings, never numbers");
        assert_eq!(value[1]["name"], "Grace");
        assert!(json.contains('\n'), "output should be pretty-printed");
    }

    #[test]
    fn delimited_to_jsonl_emits_one_compact_object_per_line() {
        let text = "name,age\nAda,36\nGrace,85\n";
        let jsonl = delimited_to_jsonl(text).unwrap();
        let lines: Vec<&str> = jsonl.lines().collect();

        assert_eq!(lines.len(), 2);
        assert!(jsonl.ends_with('\n'), "trailing newline");
        assert!(!lines[0].contains('\n'), "each object is compact, one line");

        let first: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(first["name"], "Ada");
    }

    #[test]
    fn delimited_to_json_and_jsonl_agree_on_the_same_input() {
        let text = "a,b\n1,2\n3,4\n";
        let json = delimited_to_json(text).unwrap();
        let jsonl = delimited_to_jsonl(text).unwrap();

        let from_json: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
        let from_jsonl: Vec<serde_json::Value> = jsonl
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();

        assert_eq!(
            from_json, from_jsonl,
            "the two conversions must describe the same data"
        );
    }

    #[test]
    fn a_quoted_field_with_the_delimiter_and_a_newline_survives_conversion() {
        let text = "name,note\n\"Doe, Jane\",\"line one\nline two\"\n";
        let json = delimited_to_json(text).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(value[0]["name"], "Doe, Jane");
        assert_eq!(value[0]["note"], "line one\nline two");
    }

    #[test]
    fn a_row_longer_than_the_header_keeps_its_extra_fields() {
        let text = "a,b\n1,2,3,4\n";
        let json = delimited_to_json(text).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(value[0]["a"], "1");
        assert_eq!(value[0]["b"], "2");
        assert_eq!(
            value[0]["_3"], "3",
            "an extra field keeps its column number as the key"
        );
        assert_eq!(value[0]["_4"], "4");
    }

    #[test]
    fn a_row_shorter_than_the_header_gets_nulls_for_the_rest() {
        let text = "a,b,c\n1\n";
        let json = delimited_to_json(text).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(value[0]["a"], "1");
        assert!(
            value[0]["b"].is_null(),
            "missing field must be null, not dropped or guessed"
        );
        assert!(value[0]["c"].is_null());
    }

    #[test]
    fn duplicate_and_empty_header_names_do_not_collide() {
        let text = "a,,a\n1,2,3\n";
        let json = delimited_to_json(text).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        let object = value[0].as_object().unwrap();

        assert_eq!(
            object.len(),
            3,
            "three columns must give three distinct keys, got {object:?}"
        );
        assert_eq!(value[0]["a"], "1", "first column keeps the plain name");
        assert_eq!(
            value[0]["_2"], "2",
            "empty header cell gets a generated key"
        );
        assert_eq!(
            value[0]["a_3"], "3",
            "repeated header name gets its column number appended"
        );
    }

    #[test]
    fn a_leading_zero_survives_the_round_trip() {
        // The classic CSV-to-JSON bug: coercing "007" to a number drops the
        // zeros that made it a valid part number in the first place.
        let text = "code\n007\n";
        let json = delimited_to_json(text).unwrap();
        assert!(
            json.contains("\"007\""),
            "a leading zero must stay a string, got {json}"
        );

        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            value[0]["code"],
            serde_json::Value::String("007".to_owned())
        );
    }

    #[test]
    fn column_types_infers_the_narrowest_kind_that_fits_every_value() {
        let text = "n,f,b,e,t,m\n1,1.5,true,,hello,1\n2,2.0,false,,world,x\n";
        let reports = column_types(text).unwrap();
        let kind_of = |name: &str| reports.iter().find(|r| r.name == name).unwrap().kind;

        assert_eq!(kind_of("n"), ColumnKind::Integer);
        assert_eq!(kind_of("f"), ColumnKind::Float);
        assert_eq!(kind_of("b"), ColumnKind::Boolean);
        assert_eq!(kind_of("e"), ColumnKind::Empty);
        assert_eq!(kind_of("t"), ColumnKind::Text);
        assert_eq!(
            kind_of("m"),
            ColumnKind::Text,
            "a mix of number-like and text values falls back to Text"
        );

        let n = reports.iter().find(|r| r.name == "n").unwrap();
        assert!(
            !n.summary().contains("missing"),
            "a fully filled column should not mention missing values, got {}",
            n.summary()
        );
    }

    #[test]
    fn column_types_counts_filled_and_missing_values() {
        let text = "a,b\n1,\n,2\n";
        let reports = column_types(text).unwrap();

        let a = reports.iter().find(|r| r.name == "a").unwrap();
        assert_eq!(a.filled, 1);
        assert_eq!(a.missing, 1);
        assert!(a.summary().contains("missing"), "got {}", a.summary());

        let b = reports.iter().find(|r| r.name == "b").unwrap();
        assert_eq!(b.filled, 1);
        assert_eq!(b.missing, 1);
    }

    #[test]
    fn empty_input_produces_empty_results_for_the_new_conversions() {
        assert_eq!(delimited_to_json("").unwrap(), "[]");
        assert_eq!(delimited_to_jsonl("").unwrap(), "");
        assert!(column_types("").unwrap().is_empty());
    }
}
