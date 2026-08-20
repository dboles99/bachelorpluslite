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

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use saphyr::{LoadableYamlNode, Mapping, Scalar, ScalarStyle, Yaml, YamlEmitter};
use saphyr_parser::{Event, Parser};
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

    fn from_yaml(e: &saphyr::ScanError) -> Self {
        Self::at(e.marker(), e.info().to_owned())
    }

    /// A syntax error at a `saphyr` marker.
    ///
    /// `Marker::col` counts from zero whatever its documentation says, and
    /// `serde_json` counts columns from one. Both end up in the same status
    /// bar pointing at the same editor, so the YAML side is the one that has
    /// to move: a column off by one sends the user to the wrong character.
    fn at(mark: &saphyr::Marker, message: String) -> Self {
        Self::Syntax {
            line: mark.line(),
            column: mark.col() + 1,
            message,
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

// --- YAML ---------------------------------------------------------------

/// Deepest nesting a YAML document may have before `bp-data` refuses it.
///
/// Not a taste judgement. `saphyr_parser`'s loader walks a document through
/// `load_node` -> `load_sequence` -> `load_node` mutual recursion, one stack
/// frame per level, and a stack overflow aborts the process instead of
/// returning an error the user could be shown. Measured on the 1 MiB main
/// thread: a chain of nested block mappings survives 280 levels and dies at
/// 290 in an unoptimised build, 500 and 1000 in a release one. The input
/// that does it is 200 bytes, so file size is no protection.
///
/// 128 is the limit `serde_json` already enforces on the JSON path beside
/// this one, which means both formats refuse at the same depth and the shell
/// has one sentence to say rather than two. It leaves better than a factor
/// of two against the tightest build measured.
pub const MAX_YAML_NESTING: usize = 128;

/// Most nodes one YAML document may expand to.
///
/// An alias is expanded into a copy of whatever it points at, so a few lines
/// can name a structure vastly larger than the file. The classic form is the
/// "billion laughs" document: ten anchors, each a sequence of ten references
/// to the one before, is 10^10 nodes from about 200 bytes. Nesting alone does
/// not catch it -- that document is ten levels deep -- so the expanded size
/// is counted separately, on the same pass.
pub const MAX_YAML_NODES: usize = 1_000_000;

/// Longest key a flow mapping may write in the short `key: value` form.
///
/// YAML 1.2 caps an implicit key at 1024 characters. Measured in bytes here,
/// which can only be an over-count for multi-byte text and so can only ever
/// send a key to the always-legal explicit form early.
const MAX_IMPLICIT_KEY: usize = 1024;

/// One open collection while `scan_yaml` walks the event stream.
struct YamlFrame<'input> {
    /// Nodes accumulated so far, this collection included, with every alias
    /// counted at the size it expands to rather than as one reference.
    nodes: usize,
    /// The anchor this collection is being defined under, or 0 for none.
    anchor: usize,
    /// Keys already seen, for a mapping.
    keys: HashSet<Scalar<'input>>,
    /// For a mapping, whether the next node completed is a key rather than a
    /// value. `None` for a sequence, where every node is an element.
    next_is_key: Option<bool>,
}

/// What one pass over a YAML event stream found, before any tree was built.
struct YamlScan {
    documents: usize,
}

/// Walk the parser's events, refusing what would be unsafe to build a tree
/// for and counting the documents on the way.
///
/// All of this has to happen *before* `Yaml::load_from_str`, not after. The
/// two things it refuses -- nesting deep enough to overflow the stack, and
/// alias expansion large enough to exhaust memory -- both go wrong while the
/// tree is being built, where there is no error left to return. The event
/// stream is safe to walk because `Parser`'s iterator is a state machine over
/// an explicit `Vec`, not recursion: it reads 100,000 levels of nesting
/// without touching the stack.
///
/// It also catches the one thing the loader gets silently wrong. A duplicate
/// mapping key overwrites the earlier value on the way into the map, so by
/// the time there is a tree the first value is gone and there is nothing left
/// to report. Only the events still know both were there.
fn scan_yaml(text: &str) -> Result<YamlScan, DataError> {
    let mut stack: Vec<YamlFrame<'_>> = Vec::new();
    let mut anchors: HashMap<usize, usize> = HashMap::new();
    let mut documents = 0;

    for event in Parser::new_from_str(text) {
        let (event, span) = event.map_err(|e| DataError::from_yaml(&e))?;

        match event {
            Event::DocumentStart(_) => {
                documents += 1;
                // An anchor does not cross a document boundary; the parser
                // clears its own table here too.
                anchors.clear();
            }
            Event::SequenceStart(anchor, _) => {
                open_collection(&mut stack, anchor, None, span.start)?;
            }
            Event::MappingStart(anchor, _) => {
                open_collection(&mut stack, anchor, Some(true), span.start)?;
            }
            Event::SequenceEnd | Event::MappingEnd => {
                let frame = stack
                    .pop()
                    .expect("the parser closes every collection it opened");
                if frame.anchor > 0 {
                    anchors.insert(frame.anchor, frame.nodes);
                }
                close_node(&mut stack, frame.nodes)?;
            }
            Event::Scalar(value, style, anchor, tag) => {
                if anchor > 0 {
                    anchors.insert(anchor, 1);
                }
                check_duplicate_key(&mut stack, value, style, tag.is_some(), span.start)?;
                close_node(&mut stack, 1)?;
            }
            Event::Alias(anchor) => {
                // An alias whose anchor is unknown loads as a single
                // `BadValue`, so it costs one node rather than nothing.
                let nodes = anchors.get(&anchor).copied().unwrap_or(1);
                close_node(&mut stack, nodes)?;
            }
            Event::DocumentEnd | Event::StreamStart | Event::StreamEnd | Event::Nothing => {}
        }
    }

    Ok(YamlScan { documents })
}

/// Push a frame for a sequence (`next_is_key` `None`) or a mapping.
fn open_collection<'input>(
    stack: &mut Vec<YamlFrame<'input>>,
    anchor: usize,
    next_is_key: Option<bool>,
    at: saphyr::Marker,
) -> Result<(), DataError> {
    if stack.len() >= MAX_YAML_NESTING {
        return Err(DataError::at(
            &at,
            format!(
                "nested more than {MAX_YAML_NESTING} levels deep, which BachelorPad+ will not build a tree for"
            ),
        ));
    }
    stack.push(YamlFrame {
        nodes: 1,
        anchor,
        keys: HashSet::new(),
        next_is_key,
    });
    Ok(())
}

/// Refuse a mapping key that has already been used at this level.
///
/// The key is resolved the same way the loader will resolve it, so `1` and
/// `'1'` stay two keys and `1` and `1` become one. A key carrying a tag is
/// left alone: the loader keeps the tag as part of the key's identity and
/// `Scalar` does not, so comparing without it would report two distinct keys
/// as a duplicate.
fn check_duplicate_key<'input>(
    stack: &mut [YamlFrame<'input>],
    value: Cow<'input, str>,
    style: ScalarStyle,
    tagged: bool,
    at: saphyr::Marker,
) -> Result<(), DataError> {
    let Some(frame) = stack.last_mut() else {
        return Ok(());
    };
    if frame.next_is_key != Some(true) || tagged {
        return Ok(());
    }

    let shown = value.to_string();
    if let Some(scalar) = Scalar::parse_from_cow_and_metadata(value, style, None)
        && !frame.keys.insert(scalar)
    {
        return Err(DataError::at(
            &at,
            format!(
                "duplicate key {shown:?}: the later value would silently replace the earlier one"
            ),
        ));
    }
    Ok(())
}

/// Account for a node that has just finished: charge its size to whatever
/// contains it, and move that mapping's key/value turn on.
fn close_node(stack: &mut [YamlFrame<'_>], nodes: usize) -> Result<(), DataError> {
    let Some(frame) = stack.last_mut() else {
        // A document's root node. Nothing contains it, but it still has to
        // fit in memory.
        return check_node_budget(nodes);
    };
    frame.nodes += nodes;
    if let Some(next_is_key) = frame.next_is_key.as_mut() {
        *next_is_key = !*next_is_key;
    }
    check_node_budget(frame.nodes)
}

/// Every node is charged to the collection holding it, so checking each
/// collection as it grows catches an over-large document wherever the size
/// actually accumulates.
fn check_node_budget(nodes: usize) -> Result<(), DataError> {
    if nodes > MAX_YAML_NODES {
        return Err(DataError::Other(format!(
            "this YAML document expands to more than {MAX_YAML_NODES} nodes, which is more than BachelorPad+ will hold in memory at once"
        )));
    }
    Ok(())
}

/// Parse every document in `text`, once `scan_yaml` has said it is safe to.
fn load_yaml(text: &str) -> Result<Vec<Yaml<'_>>, DataError> {
    scan_yaml(text)?;
    Yaml::load_from_str(text).map_err(|e| DataError::from_yaml(&e))
}

/// Check that `text` is valid YAML, reporting where it is not.
///
/// Every document in the stream is checked, not only the first: a file whose
/// third document is malformed is not a valid file.
pub fn yaml_validate(text: &str) -> Result<(), DataError> {
    load_yaml(text).map(|_| ())
}

/// How many documents `text` holds.
///
/// A YAML file carries a stream of documents separated by `---`, and the
/// count is worth showing: an operation that behaves differently for one
/// document and for three should not be where the user first learns there
/// are three. An empty file, and one that is only comments, hold none.
pub fn yaml_document_count(text: &str) -> Result<usize, DataError> {
    Ok(scan_yaml(text)?.documents)
}

/// Pretty-print YAML, every document in the file.
///
/// Each document is emitted under its own `---` marker, so a file of three
/// documents comes back as a file of three documents. Taking only the first
/// -- the shape a `&str -> Result<String>` signature invites -- would quietly
/// delete the rest.
///
/// Two things do not survive, and the caller has to say so before offering
/// this. **Comments are lost**: `saphyr` parses YAML into data, and a comment
/// is not data, so there is nothing to put back. **Aliases are expanded**:
/// the loader resolves `*name` into a copy of what it points at, so the
/// output repeats the value rather than referring to it. Both preserve what
/// the document *means* while changing what it says, which is the one place
/// this module's "never silently change the user's data" rule needs a
/// warning rather than an error.
pub fn yaml_format(text: &str) -> Result<String, DataError> {
    emit_block(load_yaml(text)?)
}

/// Emit documents in block style, each under its own `---` marker.
///
/// Split from `yaml_format` so the round-trip property can drive it with
/// documents it built itself. Going through the public `&str` entry point
/// would only ever test the emitter against documents the parser had already
/// produced, which is the half of the pair that is not in question.
fn emit_block(documents: Vec<Yaml<'_>>) -> Result<String, DataError> {
    let mut out = String::new();
    for document in documents {
        check_block_keys(&document)?;
        let document = respell_non_finite(document);
        let mut piece = String::new();
        YamlEmitter::new(&mut piece)
            .dump(&document)
            .map_err(|e| DataError::Other(e.to_string()))?;
        out.push_str(&piece);
        out.push('\n');
    }
    Ok(out)
}

/// Rewrite YAML in flow style, one line per document.
///
/// YAML's answer to minifying. Whitespace is structural in block style, so
/// there is nothing to strip; the compact form is a different syntax rather
/// than the same syntax with the spaces taken out. Flow style is that form,
/// and it is what `[1, 2, 3]` already is.
///
/// Every string is quoted whether it needs to be or not. `123`, `true` and
/// `null` are all perfectly good strings and all mean something else
/// unquoted, so quoting unconditionally is a rule with no exceptions to get
/// wrong, at the cost of two characters in a form written for machines.
pub fn yaml_minify(text: &str) -> Result<String, DataError> {
    emit_flow(&load_yaml(text)?)
}

/// Emit documents in flow style, one line each. Split from `yaml_minify` for
/// the same reason as `emit_block`.
fn emit_flow(documents: &[Yaml<'_>]) -> Result<String, DataError> {
    let mut out = String::new();
    for (index, document) in documents.iter().enumerate() {
        // The first document needs no marker; every one after it does, or
        // they would run together into a single malformed document.
        if index > 0 {
            out.push_str("---\n");
        }
        write_flow(document, &mut out)?;
        out.push('\n');
    }
    Ok(out)
}

/// Convert YAML to JSON.
///
/// A YAML stream holds any number of documents and JSON holds one root value,
/// so the two line up only when there is a single document -- or none, for
/// which `null` is JSON's spelling of the same nothing. More than one is
/// refused rather than wrapped in an array: an array of documents is a
/// different shape from the document the user was looking at, and inventing
/// it would hide the mismatch instead of naming it.
pub fn yaml_to_json(text: &str) -> Result<String, DataError> {
    let documents = load_yaml(text)?;
    let value = match documents.as_slice() {
        [] => serde_json::Value::Null,
        [document] => yaml_to_value(document)?,
        many => {
            return Err(DataError::Other(format!(
                "this file holds {} YAML documents and JSON has a single root value; convert them one at a time",
                many.len()
            )));
        }
    };
    json_format(&serde_json::to_string(&value).map_err(|e| DataError::Other(e.to_string()))?)
}

/// Convert JSON to YAML.
///
/// The pair to `yaml_to_json`, and the direction that cannot lose anything:
/// every JSON value has a YAML spelling. The reverse is not true, which is
/// why `yaml_to_json` has a list of refusals and this does not.
pub fn json_to_yaml(text: &str) -> Result<String, DataError> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| DataError::from_json(&e))?;
    let mut out = String::new();
    YamlEmitter::new(&mut out)
        .dump(&value_to_yaml(value)?)
        .map_err(|e| DataError::Other(e.to_string()))?;
    out.push('\n');
    Ok(out)
}

/// Refuse a document `YamlEmitter` would write in a form nothing can read
/// back.
///
/// A block mapping writes its keys implicitly -- `key: value` -- and YAML
/// stops an implicit key at 1024 characters. `saphyr` 0.0.12 writes a longer
/// one anyway, and the file it produces fails to parse on the next open.
/// Found by the round-trip property, not by anyone thinking of it.
///
/// Refusing is the honest answer rather than the convenient one: `yaml_minify`
/// writes the same document correctly, using the explicit `? key : value`
/// form, so there is somewhere to send the user.
fn check_block_keys(node: &Yaml<'_>) -> Result<(), DataError> {
    match node {
        Yaml::Sequence(items) => items.iter().try_for_each(check_block_keys),
        Yaml::Mapping(entries) => {
            for (key, value) in entries {
                // A collection key already goes out in the explicit form, so
                // the implicit-key limit does not reach it.
                if matches!(key, Yaml::Value(_)) {
                    let length = flow_of(key).len();
                    if length > MAX_IMPLICIT_KEY {
                        return Err(DataError::Other(format!(
                            "a mapping key of {length} characters cannot be written in block style; YAML stops an implicit key at {MAX_IMPLICIT_KEY}"
                        )));
                    }
                }
                check_block_keys(key)?;
                check_block_keys(value)?;
            }
            Ok(())
        }
        Yaml::Tagged(_, inner) => check_block_keys(inner),
        Yaml::Value(_) | Yaml::Representation(..) | Yaml::Alias(_) | Yaml::BadValue => Ok(()),
    }
}

/// Hand the emitter a spelling of every non-finite float that YAML reads back.
///
/// `saphyr` 0.0.12's emitter writes Rust's `inf`, `-inf` and `NaN`, none of
/// which YAML reads as a number: the next open turns the float into the
/// *string* `"inf"`. A `Representation` node is written verbatim, so putting
/// YAML's own `.inf`, `-.inf` and `.nan` there instead is what stops
/// formatting from quietly changing a value's type.
///
/// Recursion is safe here because `scan_yaml` has already refused anything
/// nested deeper than `MAX_YAML_NESTING`.
fn respell_non_finite(node: Yaml<'_>) -> Yaml<'_> {
    match node {
        Yaml::Value(Scalar::FloatingPoint(value)) if !value.0.is_finite() => Yaml::Representation(
            Cow::Borrowed(yaml_float_spelling(value.0)),
            ScalarStyle::Plain,
            None,
        ),
        Yaml::Sequence(items) => {
            Yaml::Sequence(items.into_iter().map(respell_non_finite).collect())
        }
        Yaml::Mapping(entries) => Yaml::Mapping(
            entries
                .into_iter()
                .map(|(key, value)| (respell_non_finite(key), respell_non_finite(value)))
                .collect(),
        ),
        Yaml::Tagged(tag, inner) => Yaml::Tagged(tag, Box::new(respell_non_finite(*inner))),
        other => other,
    }
}

/// YAML's spelling of a non-finite float.
///
/// Only ever called for one, and it would answer `.inf` for a finite value
/// rather than complain -- the callers all test `is_finite` first because
/// each has a better thing to write in that case, not because this would
/// stop them.
fn yaml_float_spelling(value: f64) -> &'static str {
    if value.is_nan() {
        ".nan"
    } else if value.is_sign_positive() {
        ".inf"
    } else {
        "-.inf"
    }
}

/// Write one document in YAML flow style, on a single line.
///
/// Recursion is bounded by `scan_yaml` having already refused anything deeper
/// than `MAX_YAML_NESTING`.
fn write_flow(node: &Yaml<'_>, out: &mut String) -> Result<(), DataError> {
    match node {
        Yaml::Value(scalar) => write_flow_scalar(scalar, out),
        Yaml::Sequence(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_flow(item, out)?;
            }
            out.push(']');
        }
        Yaml::Mapping(entries) => {
            out.push('{');
            for (index, (key, value)) in entries.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                let mut rendered = String::new();
                write_flow(key, &mut rendered)?;
                // A flow mapping takes the short `key: value` form only when
                // the key is one scalar short enough to be an implicit key.
                // Anything else -- a collection as a key, or a string past
                // YAML's 1024-character implicit-key limit -- needs the
                // explicit `? key : value` form, which is always legal but
                // costs four characters nothing else needs.
                if matches!(key, Yaml::Value(_)) && rendered.len() <= MAX_IMPLICIT_KEY {
                    out.push_str(&rendered);
                } else {
                    out.push_str("? ");
                    out.push_str(&rendered);
                    out.push(' ');
                }
                out.push_str(": ");
                write_flow(value, out)?;
            }
            out.push('}');
        }
        Yaml::Tagged(tag, inner) => {
            out.push_str(&format!("{tag} "));
            write_flow(inner, out)?;
        }
        Yaml::Representation(value, style, tag) => {
            // Unreachable from `load_yaml`, which leaves scalar parsing on.
            // Kept honest rather than unreachable!(): the loader is one
            // setting away from producing these.
            return Err(DataError::Other(format!(
                "unresolved scalar {value:?} ({style:?}, {tag:?})"
            )));
        }
        Yaml::Alias(anchor) => {
            return Err(DataError::Other(format!(
                "alias {anchor} was never resolved"
            )));
        }
        Yaml::BadValue => {
            return Err(DataError::Other(
                "a value whose tag does not match its contents, such as `!!int foo`".to_owned(),
            ));
        }
    }
    Ok(())
}

/// Write a scalar in flow style.
///
/// Strings go out double-quoted with JSON's escapes, which are a subset of
/// YAML's own double-quoted escapes. Borrowing them rather than hand-rolling
/// the rules is the point: a string holding a newline, a quote or a control
/// character is where a hand-written escaper goes wrong, and `serde_json`
/// already gets all three right.
fn write_flow_scalar(scalar: &Scalar<'_>, out: &mut String) {
    match scalar {
        Scalar::Null => out.push_str("null"),
        Scalar::Boolean(value) => out.push_str(if *value { "true" } else { "false" }),
        Scalar::Integer(value) => out.push_str(&value.to_string()),
        Scalar::FloatingPoint(value) => {
            let value = value.0;
            if value.is_finite() {
                out.push_str(&value.to_string());
                // Rust prints a whole number without a point and YAML would
                // read that back as an integer.
                if value.fract() == 0.0 {
                    out.push_str(".0");
                }
            } else {
                out.push_str(yaml_float_spelling(value));
            }
        }
        Scalar::String(value) => {
            out.push_str(&serde_json::Value::String(value.to_string()).to_string());
        }
    }
}

/// Map one YAML node to the JSON value that means the same thing.
///
/// The refusals are all cases where JSON has nothing to say and a converter
/// would otherwise have to guess: a non-string mapping key (JSON has only
/// string keys, so `1:` and `'1':` would collide under one name), a tag
/// (JSON carries no type information), a non-finite float (JSON has no
/// `.inf`), and a value the parser could not resolve.
fn yaml_to_value(node: &Yaml<'_>) -> Result<serde_json::Value, DataError> {
    Ok(match node {
        Yaml::Value(scalar) => scalar_to_value(scalar)?,
        Yaml::Sequence(items) => serde_json::Value::Array(
            items
                .iter()
                .map(yaml_to_value)
                .collect::<Result<Vec<_>, _>>()?,
        ),
        Yaml::Mapping(entries) => {
            let mut object = serde_json::Map::new();
            for (key, value) in entries {
                let Yaml::Value(Scalar::String(name)) = key else {
                    return Err(DataError::Other(format!(
                        "a JSON object needs string keys, and this mapping has the key {}",
                        flow_of(key)
                    )));
                };
                object.insert(name.to_string(), yaml_to_value(value)?);
            }
            serde_json::Value::Object(object)
        }
        Yaml::Tagged(tag, _) => {
            return Err(DataError::Other(format!("JSON cannot carry the tag {tag}")));
        }
        Yaml::Representation(..) | Yaml::Alias(_) | Yaml::BadValue => {
            return Err(DataError::Other(
                "a value the parser could not resolve, such as `!!int foo`".to_owned(),
            ));
        }
    })
}

/// A node rendered in flow style for an error message, or a placeholder if
/// even that fails. An error about a key is useless without the key.
fn flow_of(node: &Yaml<'_>) -> String {
    let mut out = String::new();
    match write_flow(node, &mut out) {
        Ok(()) => out,
        Err(_) => "<unprintable>".to_owned(),
    }
}

/// Map one YAML scalar to its JSON value.
///
/// `Number::from_f64` is the whole reason this returns a `Result`: it refuses
/// a non-finite float, which is JSON saying it has no way to write `.inf`
/// rather than an error worth hiding behind a `null`.
fn scalar_to_value(scalar: &Scalar<'_>) -> Result<serde_json::Value, DataError> {
    Ok(match scalar {
        Scalar::Null => serde_json::Value::Null,
        Scalar::Boolean(value) => serde_json::Value::Bool(*value),
        Scalar::Integer(value) => serde_json::Value::Number((*value).into()),
        Scalar::FloatingPoint(value) => serde_json::Number::from_f64(value.0)
            .map(serde_json::Value::Number)
            .ok_or_else(|| {
                DataError::Other(format!(
                    "JSON has no way to write {}",
                    yaml_float_spelling(value.0)
                ))
            })?,
        Scalar::String(value) => serde_json::Value::String(value.to_string()),
    })
}

/// Map one JSON value to the YAML node that means the same thing.
fn value_to_yaml(value: serde_json::Value) -> Result<Yaml<'static>, DataError> {
    Ok(match value {
        serde_json::Value::Null => Yaml::Value(Scalar::Null),
        serde_json::Value::Bool(value) => Yaml::Value(Scalar::Boolean(value)),
        serde_json::Value::Number(number) => {
            if let Some(value) = number.as_i64() {
                Yaml::Value(Scalar::Integer(value))
            } else if number.is_f64() {
                // `as_f64` would also answer for an integer too large for
                // `i64`, silently rounding it. Only take it when the number
                // was written as a float in the first place.
                Yaml::Value(Scalar::FloatingPoint(
                    number
                        .as_f64()
                        .expect("a number that reports itself as f64 has one")
                        .into(),
                ))
            } else {
                return Err(DataError::Other(format!(
                    "{number} does not fit YAML's 64-bit signed integer"
                )));
            }
        }
        serde_json::Value::String(value) => Yaml::Value(Scalar::String(Cow::Owned(value))),
        serde_json::Value::Array(items) => Yaml::Sequence(
            items
                .into_iter()
                .map(value_to_yaml)
                .collect::<Result<Vec<_>, _>>()?,
        ),
        serde_json::Value::Object(entries) => {
            let mut mapping = Mapping::new();
            for (name, value) in entries {
                mapping.insert(
                    Yaml::Value(Scalar::String(Cow::Owned(name))),
                    value_to_yaml(value)?,
                );
            }
            Yaml::Mapping(mapping)
        }
    })
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
    use proptest::prelude::*;

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
    fn valid_yaml_passes_and_invalid_yaml_says_where() {
        assert!(yaml_validate("a: 1\n").is_ok());

        match yaml_validate("a: [1, 2\nb: 3\n").unwrap_err() {
            DataError::Syntax { line, column, .. } => {
                assert_eq!(line, 2);
                assert!(column > 0, "a column of 0 would be useless");
            }
            other => panic!("expected a syntax error, got {other:?}"),
        }
    }

    #[test]
    fn a_yaml_error_reports_the_column_from_one_like_the_json_one_does() {
        // Both errors reach the same status bar and point at the same
        // editor. `saphyr` counts columns from zero, so an unadjusted column
        // would put the caret one character to the left of the fault.
        //
        // The 129th `[` is the one refused, and it is the 129th character.
        let text = "[".repeat(MAX_YAML_NESTING + 1);
        match yaml_validate(&text).unwrap_err() {
            DataError::Syntax { line, column, .. } => {
                assert_eq!(line, 1);
                assert_eq!(column, MAX_YAML_NESTING + 1);
            }
            other => panic!("expected a syntax error, got {other:?}"),
        }

        // And the same for an error `saphyr` raises itself: the `:` is the
        // fourth character of "  b: 2".
        match yaml_validate("a: 1\n  b: 2\n").unwrap_err() {
            DataError::Syntax { line, column, .. } => {
                assert_eq!(line, 2);
                assert_eq!(column, 4);
            }
            other => panic!("expected a syntax error, got {other:?}"),
        }
    }

    #[test]
    fn a_duplicate_yaml_key_is_refused_rather_than_silently_collapsed() {
        // `saphyr` overwrites the earlier value on the way into the map, so
        // by the time there is a tree the first value is gone. Formatting
        // such a file would delete a line and report success.
        let err = yaml_validate("a: 1\nb: 2\na: 3\n").unwrap_err();
        match err {
            DataError::Syntax {
                line, ref message, ..
            } => {
                assert_eq!(line, 3);
                assert!(message.contains("duplicate key"), "got {message}");
                assert!(message.contains('a'), "the key itself must be named");
            }
            other => panic!("expected a syntax error, got {other:?}"),
        }

        assert!(yaml_format("{a: 1, a: 2}").is_err(), "flow style too");
    }

    #[test]
    fn a_quoted_key_and_an_unquoted_one_are_not_duplicates() {
        // `1` is an integer key and `'1'` is a string key. Reporting them as
        // a duplicate would refuse a valid file.
        assert!(yaml_validate("1: x\n'1': y\n").is_ok());
    }

    #[test]
    fn empty_and_comment_only_yaml_hold_no_documents() {
        for text in ["", "   \n\n", "# just a note\n# and another\n"] {
            assert!(yaml_validate(text).is_ok(), "{text:?} should be valid");
            assert_eq!(yaml_document_count(text).unwrap(), 0, "{text:?}");
            assert_eq!(yaml_format(text).unwrap(), "", "{text:?}");
            assert_eq!(yaml_minify(text).unwrap(), "", "{text:?}");
        }
    }

    #[test]
    fn every_document_in_a_multi_document_file_survives_formatting() {
        // Taking only the first document is the shape a `&str ->
        // Result<String>` signature invites, and it would delete the rest.
        let text = "a: 1\n---\nb: 2\n---\nc: 3\n";
        assert_eq!(yaml_document_count(text).unwrap(), 3);

        let formatted = yaml_format(text).unwrap();
        assert_eq!(yaml_document_count(&formatted).unwrap(), 3);
        assert!(formatted.contains("a: 1"), "got {formatted}");
        assert!(formatted.contains("c: 3"), "got {formatted}");

        let minified = yaml_minify(text).unwrap();
        assert_eq!(yaml_document_count(&minified).unwrap(), 3);
    }

    #[test]
    fn a_later_document_being_malformed_fails_the_whole_file() {
        match yaml_validate("a: 1\n---\nb: [1\n").unwrap_err() {
            DataError::Syntax { line, .. } => assert!(line >= 3, "line {line}"),
            other => panic!("expected a syntax error, got {other:?}"),
        }
    }

    #[test]
    fn yaml_to_json_refuses_a_file_of_several_documents() {
        // JSON has one root value. An array of documents would be a
        // different shape from the document the user was looking at.
        let err = yaml_to_json("a: 1\n---\nb: 2\n").unwrap_err();
        assert!(err.to_string().contains('2'), "say how many: {err}");

        assert_eq!(yaml_to_json("a: 1\n").unwrap(), "{\n  \"a\": 1\n}");
        assert_eq!(
            yaml_to_json("").unwrap(),
            "null",
            "an empty stream is JSON's nothing"
        );
    }

    #[test]
    fn yaml_to_json_refuses_what_json_cannot_say() {
        // Each of these has no JSON spelling, and guessing one would change
        // the data: `1:` and `'1':` would collide under a single name, a tag
        // would be dropped, and there is no JSON `.inf`.
        assert!(yaml_to_json("1: x\n").is_err(), "non-string key");
        assert!(yaml_to_json("a: !custom 3\n").is_err(), "tag");
        assert!(yaml_to_json("a: .inf\n").is_err(), "non-finite float");
    }

    #[test]
    fn yaml_and_json_convert_both_ways() {
        let json = "{\n  \"a\": [\n    1,\n    2\n  ],\n  \"b\": null\n}";
        let yaml = json_to_yaml(json).unwrap();
        assert_eq!(yaml_to_json(&yaml).unwrap(), json);
    }

    #[test]
    fn an_alias_is_expanded_rather_than_dropped() {
        // `saphyr` resolves `*name` into a copy while loading, so the output
        // repeats the value. The value is preserved; the reference is not.
        let formatted = yaml_format("a: &x [1, 2]\nb: *x\n").unwrap();
        assert!(!formatted.contains('*'), "no alias survives: {formatted}");
        assert_eq!(
            formatted.matches("- 1").count(),
            2,
            "both copies present: {formatted}"
        );
    }

    #[test]
    fn a_non_finite_float_stays_a_float_through_formatting() {
        // `saphyr`'s emitter writes Rust's `inf` and `NaN`, which YAML reads
        // back as the strings "inf" and "NaN". A float that becomes a string
        // on the next open is exactly the silent change this module forbids.
        let text = "a: .inf\nb: -.inf\nc: .nan\n";
        for out in [yaml_format(text).unwrap(), yaml_minify(text).unwrap()] {
            assert!(out.contains(".inf"), "got {out}");
            assert!(out.contains("-.inf"), "got {out}");
            assert!(out.contains(".nan"), "got {out}");
            assert_eq!(load_yaml(&out).unwrap(), load_yaml(text).unwrap());
        }
    }

    #[test]
    fn minifying_yaml_produces_flow_style_that_parses_back() {
        let minified = yaml_minify("a:\n  b:\n    - 1\n    - c: d\n").unwrap();

        assert_eq!(minified.lines().count(), 1, "one line: {minified}");
        assert!(minified.starts_with('{'), "flow style: {minified}");
        assert_eq!(
            load_yaml(&minified).unwrap(),
            load_yaml("a:\n  b:\n    - 1\n    - c: d\n").unwrap()
        );
    }

    #[test]
    fn a_minified_string_is_quoted_even_when_it_need_not_be() {
        // `123`, `true` and `null` are all valid strings and all mean
        // something else unquoted.
        let minified = yaml_minify("a: '123'\nb: 'true'\nc: 'null'\n").unwrap();
        assert!(minified.contains("\"123\""), "got {minified}");
        assert!(minified.contains("\"true\""), "got {minified}");
        assert!(minified.contains("\"null\""), "got {minified}");
    }

    #[test]
    fn yaml_nested_past_the_limit_is_refused_instead_of_crashing() {
        // `saphyr_parser`'s loader recurses once per level, and a stack
        // overflow aborts the process rather than returning an error. 200
        // bytes is enough to do it, so file size is no protection.
        // 500 levels of block mappings in 125 KB. Without the limit this
        // test aborts the whole test binary rather than failing.
        let mut block = String::new();
        for indent in 0..500 {
            block.push_str(&" ".repeat(indent));
            block.push_str("a:\n");
        }
        match yaml_validate(&block).unwrap_err() {
            DataError::Syntax { ref message, .. } => {
                assert!(message.contains("nested"), "got {message}");
            }
            other => panic!("expected a syntax error, got {other:?}"),
        }

        // Flow style is refused too, though `saphyr`'s own scanner also caps
        // flow nesting at 255, so past that its message arrives first.
        let flow = format!("{}{}", "[".repeat(500), "]".repeat(500));
        assert!(yaml_validate(&flow).is_err());
    }

    #[test]
    fn yaml_nested_up_to_the_limit_is_still_accepted() {
        // One past the boundary is refused; the boundary itself is not.
        let ok = format!(
            "{}{}",
            "[".repeat(MAX_YAML_NESTING),
            "]".repeat(MAX_YAML_NESTING)
        );
        assert!(yaml_validate(&ok).is_ok());

        let over = format!(
            "{}{}",
            "[".repeat(MAX_YAML_NESTING + 1),
            "]".repeat(MAX_YAML_NESTING + 1)
        );
        assert!(yaml_validate(&over).is_err());
    }

    #[test]
    fn a_key_too_long_for_block_style_is_refused_rather_than_written_unreadable() {
        // YAML stops an implicit key at 1024 characters and `saphyr`'s
        // emitter writes a longer one anyway, producing a file that will not
        // parse on the next open. Found by the round-trip property.
        // The input has to spell the key explicitly for the same reason the
        // output would have to.
        let text = format!("? \"{}\"\n: v\n", "x".repeat(MAX_IMPLICIT_KEY + 1));
        assert!(yaml_validate(&text).is_ok(), "the input itself is fine");

        let err = yaml_format(&text).unwrap_err();
        assert!(err.to_string().contains("1024"), "got {err}");

        // Flow style writes it correctly, with the explicit `? key : value`
        // form, so the refusal has somewhere to send the user.
        let minified = yaml_minify(&text).unwrap();
        assert_eq!(load_yaml(&minified).unwrap(), load_yaml(&text).unwrap());
    }

    #[test]
    fn an_alias_bomb_is_refused_before_it_is_expanded() {
        // Ten anchors, each ten references to the one before: 10^10 nodes
        // from a few hundred bytes, and only ten levels deep, so the nesting
        // limit does not see it.
        let mut bomb = String::from("a0: &a0 [x, x, x, x, x, x, x, x, x, x]\n");
        for level in 1..10 {
            bomb.push_str(&format!("a{level}: &a{level} ["));
            for reference in 0..10 {
                if reference > 0 {
                    bomb.push_str(", ");
                }
                bomb.push_str(&format!("*a{}", level - 1));
            }
            bomb.push_str("]\n");
        }
        assert!(bomb.len() < 1000, "the input is tiny: {} bytes", bomb.len());

        let err = yaml_validate(&bomb).unwrap_err();
        assert!(err.to_string().contains("nodes"), "got {err}");
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

    // --- YAML round trip, as a property ---------------------------------
    //
    // The examples above each pin one case somebody thought of. This pins
    // the rule they are all instances of: whatever a document holds,
    // writing it out and reading it back gives the same document. Three
    // faults were found by turning it on -- `.inf` becoming the string
    // "inf", a whole-number float losing its point and coming back an
    // integer, and a long key needing the explicit flow form -- and none of
    // them were on anyone's list of cases to write.

    /// Floats worth generating: the ordinary ones, and the three YAML spells
    /// differently from Rust.
    fn arb_float() -> impl Strategy<Value = f64> {
        prop_oneof![
            10 => any::<f64>().prop_filter("finite", |f| f.is_finite()),
            1 => Just(f64::INFINITY),
            1 => Just(f64::NEG_INFINITY),
            1 => Just(f64::NAN),
            1 => Just(0.0),
            1 => Just(-0.0),
        ]
    }

    /// Strings worth generating: arbitrary text, plus the ones that mean
    /// something else when they are not quoted.
    fn arb_string() -> impl Strategy<Value = String> {
        prop_oneof![
            6 => any::<String>(),
            1 => prop::sample::select(vec![
                String::new(),
                "true".to_owned(),
                "null".to_owned(),
                "~".to_owned(),
                "123".to_owned(),
                "1.5".to_owned(),
                "- item".to_owned(),
                "a: b".to_owned(),
                "#comment".to_owned(),
                "line\nbreak".to_owned(),
                "trailing  ".to_owned(),
                "  leading".to_owned(),
                "\"quoted\"".to_owned(),
                "'single'".to_owned(),
                "\t\r\u{0}".to_owned(),
                "x".repeat(2000),
            ]),
        ]
    }

    /// An arbitrary YAML document.
    ///
    /// Built as a tree rather than as text, so the property tests the
    /// emitter against the parser. Generating text instead would only ever
    /// present the emitter with documents the parser had already produced,
    /// which is the half of the pair that is not in question.
    fn arb_document() -> impl Strategy<Value = Yaml<'static>> {
        let leaf = prop_oneof![
            Just(Yaml::Value(Scalar::Null)),
            any::<bool>().prop_map(|b| Yaml::Value(Scalar::Boolean(b))),
            any::<i64>().prop_map(|i| Yaml::Value(Scalar::Integer(i))),
            arb_float().prop_map(|f| Yaml::Value(Scalar::FloatingPoint(f.into()))),
            arb_string().prop_map(|s| Yaml::Value(Scalar::String(Cow::Owned(s)))),
        ];

        leaf.prop_recursive(4, 48, 6, |inner| {
            prop_oneof![
                prop::collection::vec(inner.clone(), 0..6).prop_map(Yaml::Sequence),
                prop::collection::vec((inner.clone(), inner), 0..6).prop_map(|entries| {
                    // Inserting builds the same collapsing the loader would
                    // do, so the generated document never holds a duplicate
                    // key that the reload could not reproduce.
                    let mut mapping = Mapping::new();
                    for (key, value) in entries {
                        mapping.insert(key, value);
                    }
                    Yaml::Mapping(mapping)
                }),
            ]
        })
    }

    proptest! {
        #[test]
        fn writing_yaml_out_and_reading_it_back_gives_the_same_documents(
            documents in prop::collection::vec(arb_document(), 0..4)
        ) {
            // Flow style must never refuse: it has an explicit form for
            // every key.
            let flow = emit_flow(&documents).unwrap();
            prop_assert_eq!(load_yaml(&flow).unwrap(), documents.clone(), "flow style");

            // Block style may refuse, but only for the one thing YAML cannot
            // write that way. Pinning the reason keeps the block half of this
            // property from quietly stopping.
            match emit_block(documents.clone()) {
                Ok(block) => {
                    prop_assert_eq!(load_yaml(&block).unwrap(), documents, "block style");
                }
                Err(e) => prop_assert!(
                    e.to_string().contains("implicit key"),
                    "block style refused for an unexpected reason: {}",
                    e
                ),
            }
        }
    }
}
