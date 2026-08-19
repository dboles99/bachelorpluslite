//! Seam: `bp-formats` says what a document is, and `bp-data` then has to
//! parse it.
//!
//! Neither crate depends on the other -- deliberately, and it is why they are
//! both cheap to test. It is also why nothing checks that they agree. The
//! product's behaviour is the composition: detection picks the format, the
//! status bar shows it, the Data menu's operations are the ones for it, and
//! `bp-data` is handed the text. A disagreement is not a caught error, it is
//! a user told their file is JSON Lines and then shown a syntax error on
//! line 1 of a perfectly good JSON document.
//!
//! Two directions are worth separating, because they behave differently:
//!
//! * **by extension**, which is what `detect` uses when it recognises one,
//!   and which is the user's stated intent;
//! * **by sniffing**, which is what happens for a file with no extension or
//!   an unknown one -- and which is a guess, so it is where a disagreement
//!   between the two crates actually lives.
//!
//! ADR-0023's three YAML bounds are exercised here too, through the same
//! join: a document is detected as YAML and then has to be refused by the
//! parser rather than taking the process down with it.

mod common;

use common::no_files;

use std::path::PathBuf;

use bp_data::{
    DataError, delimited_report, json_validate, jsonl_validate, toml_validate, yaml_document_count,
    yaml_validate,
};
use bp_formats::{Format, detect, sniff};
use proptest::prelude::*;

/// Whether `bp-data` accepts `text` as the format `bp-formats` claims it is.
///
/// `Ok(())` for the formats `bp-data` has no parser for, so this function
/// asserts nothing it cannot back up. Which those are is itself listed in
/// [`formats_bp_data_cannot_parse_are_named_here`].
fn bp_data_accepts(format: Format, text: &str) -> Result<(), String> {
    match format {
        Format::Json => json_validate(text).map_err(|e: DataError| e.to_string()),
        Format::JsonLines => {
            let report = jsonl_validate(text);
            if report.is_valid() {
                Ok(())
            } else {
                Err(report.summary())
            }
        }
        Format::Yaml => yaml_validate(text).map_err(|e| e.to_string()),
        Format::Toml => toml_validate(text).map_err(|e| e.to_string()),
        Format::Csv | Format::Tsv => delimited_report(text)
            .map(|_| ())
            .map_err(|e| e.to_string()),
        // Everything else is text as far as `bp-data` is concerned.
        _ => Ok(()),
    }
}

// --- generators of genuinely valid documents ------------------------------

fn json_scalar() -> impl Strategy<Value = String> {
    proptest::sample::select(&["1", "-2.5", "true", "false", "null", "\"text\"", "\"é中\""][..])
        .prop_map(str::to_owned)
}

/// A JSON value, nested a few levels, in compact form.
fn json_value() -> impl Strategy<Value = String> {
    json_scalar().prop_recursive(3, 16, 4, |inner| {
        prop_oneof![
            proptest::collection::vec(inner.clone(), 0..4)
                .prop_map(|items| format!("[{}]", items.join(","))),
            proptest::collection::vec(inner, 0..4).prop_map(|items| {
                let fields: Vec<String> = items
                    .into_iter()
                    .enumerate()
                    .map(|(i, v)| format!("\"k{i}\":{v}"))
                    .collect();
                format!("{{{}}}", fields.join(","))
            }),
        ]
    })
}

/// A JSON Lines document: one compact JSON value per line.
fn jsonl_document() -> impl Strategy<Value = String> {
    proptest::collection::vec(json_value(), 1..6).prop_map(|records| {
        let mut out = records.join("\n");
        out.push('\n');
        out
    })
}

fn toml_document() -> impl Strategy<Value = String> {
    proptest::collection::vec(
        proptest::sample::select(
            &[
                "title = \"a note\"",
                "count = 3",
                "enabled = true",
                "ratio = 1.5",
                "items = [1, 2, 3]",
            ][..],
        ),
        0..5,
    )
    .prop_map(|lines| {
        // Distinct keys, so TOML's own -- correct -- refusal of a duplicate
        // key is not mistaken for the two crates disagreeing.
        let mut out = String::new();
        for (index, line) in lines.into_iter().enumerate() {
            let (key, rest) = line.split_once(" = ").expect("a key");
            out.push_str(&format!("{key}{index} = {rest}\n"));
        }
        out
    })
}

fn yaml_document() -> impl Strategy<Value = String> {
    proptest::collection::vec(
        proptest::sample::select(
            &[
                "title: a note",
                "count: 3",
                "enabled: true",
                "items:\n  - one\n  - two",
                "nested:\n  inner: value",
            ][..],
        ),
        1..4,
    )
    .prop_map(|blocks| {
        // Distinct top-level keys, so a duplicate-key refusal is not what is
        // being measured here.
        let mut out = String::new();
        for (index, block) in blocks.into_iter().enumerate() {
            let (key, rest) = block.split_once(':').expect("a key");
            out.push_str(&format!("{key}{index}:{rest}\n"));
        }
        out
    })
}

fn delimited_document(delimiter: char) -> impl Strategy<Value = String> {
    proptest::collection::vec(
        proptest::collection::vec(
            proptest::sample::select(&["a", "1", "", "\"quoted, field\"", "é中"][..]),
            1..5,
        ),
        1..6,
    )
    .prop_map(move |rows| {
        let mut out = String::new();
        for row in rows {
            out.push_str(&row.join(&delimiter.to_string()));
            out.push('\n');
        }
        out
    })
}

// --- the extension path ---------------------------------------------------

proptest! {
    #![proptest_config(no_files(256))]

    /// **A document saved under a format's own extension is detected as that
    /// format, and `bp-data` parses it.**
    ///
    /// The extension is the user's stated intent and `detect` honours it over
    /// the content, so this is the path a real document takes. What it checks
    /// across the seam is that the two crates' idea of each format is the
    /// same one -- that `bp-formats::Json` means what `bp_data::json_validate`
    /// accepts, and so on for all six.
    #[test]
    fn a_valid_document_under_its_own_extension_is_detected_and_parsed(
        (format, text) in prop_oneof![
            json_value().prop_map(|t| (Format::Json, t)),
            jsonl_document().prop_map(|t| (Format::JsonLines, t)),
            toml_document().prop_map(|t| (Format::Toml, t)),
            yaml_document().prop_map(|t| (Format::Yaml, t)),
            delimited_document(',').prop_map(|t| (Format::Csv, t)),
            delimited_document('\t').prop_map(|t| (Format::Tsv, t)),
        ],
    ) {
        let path = PathBuf::from(format!("note.{}", format.default_extension()));
        prop_assert_eq!(
            detect(Some(&path), &text),
            format,
            "the extension did not decide the format"
        );
        prop_assert!(
            format.profile().is_data(),
            "a structured format whose profile does not offer the Data menu"
        );
        let accepted = bp_data_accepts(format, &text);
        prop_assert!(
            accepted.is_ok(),
            "a valid {} document was rejected by bp-data: {}",
            format.label(),
            accepted.unwrap_err()
        );
    }

    /// A JSON Lines document is not also a valid single JSON document once it
    /// has more than one record, and vice versa. The distinction the two
    /// crates make has to be the same distinction, or a conversion between
    /// them silently drops records.
    #[test]
    fn jsonl_and_json_stay_distinguishable(records in proptest::collection::vec(json_value(), 2..6)) {
        let jsonl = format!("{}\n", records.join("\n"));
        prop_assert!(jsonl_validate(&jsonl).is_valid());
        prop_assert!(
            json_validate(&jsonl).is_err(),
            "several records ran together parsed as one JSON document"
        );
    }
}

// --- the sniffing path, where the crates disagree -------------------------

/// **DEFECT.** `bp_formats::sniff` calls a pretty-printed JSON array of
/// objects "JSON Lines", and `bp-data` then refuses it.
///
/// `looks_like_json_lines` counts lines that begin with `{` or `[` and calls
/// it JSON Lines at two or more. Its doc comment says "a pretty-printed JSON
/// document has exactly one such line", which is only true of a document
/// whose root is an object of scalars. A pretty-printed **array of objects**
/// -- the single most common shape of JSON anybody has -- has one such line
/// per element:
///
/// ```text
/// [
///   {"id": 1},
///   {"id": 2}
/// ]
/// ```
///
/// Sniffing is reached whenever the file has no extension or an unrecognised
/// one (`report.json.bak`, `data`, a pasted buffer that has never been
/// saved). The user is then told the file is JSONL, and `jsonl_validate`
/// reports a syntax error on line 1 of a file that is perfectly good JSON.
///
/// Left failing rather than narrowed, because the fix is a judgement about
/// the heuristic -- requiring the *first* non-blank line to be a complete
/// JSON value would do it -- and belongs to whoever owns `bp-formats`.
#[test]
#[ignore = "known defect: bp_formats::sniff reports pretty-printed JSON arrays as JSON Lines, which bp-data rejects"]
fn sniffing_agrees_with_the_parser_that_then_handles_the_file() {
    for text in [
        "[\n  {\"id\": 1},\n  {\"id\": 2}\n]\n",
        "[\n  [1, 2],\n  [3, 4]\n]\n",
        "{\n  \"outer\": {\n    \"a\": 1\n  },\n  \"other\": {\n    \"b\": 2\n  }\n}\n",
    ] {
        // Every one of these is valid JSON.
        assert!(json_validate(text).is_ok(), "the fixture is not valid JSON");

        let sniffed = sniff(text);
        assert_eq!(
            bp_data_accepts(sniffed, text),
            Ok(()),
            "sniff called a valid JSON document {} and bp-data refused it",
            sniffed.label()
        );
    }
}

#[test]
fn sniffing_a_single_json_value_agrees_with_the_parser() {
    // The half of the heuristic that does hold, kept so the ignored test
    // above is understood as narrow rather than as "sniffing is broken".
    for text in [
        "{\"a\": 1, \"b\": 2}",
        "[1, 2, 3]",
        "{\n  \"a\": 1,\n  \"b\": 2\n}\n",
    ] {
        let sniffed = sniff(text);
        assert_eq!(sniffed, Format::Json, "not detected as JSON");
        assert_eq!(bp_data_accepts(sniffed, text), Ok(()));
    }
}

#[test]
fn sniffing_json_lines_agrees_with_the_parser() {
    let text = "{\"id\": 1}\n{\"id\": 2}\n{\"id\": 3}\n";
    assert_eq!(sniff(text), Format::JsonLines);
    assert_eq!(bp_data_accepts(Format::JsonLines, text), Ok(()));
}

#[test]
fn sniffing_a_yaml_document_marker_agrees_with_the_parser() {
    let text = "---\ntitle: a note\ncount: 3\n";
    assert_eq!(sniff(text), Format::Yaml);
    assert_eq!(bp_data_accepts(Format::Yaml, text), Ok(()));
}

#[test]
fn an_empty_document_is_plain_text_and_offends_nobody() {
    assert_eq!(sniff(""), Format::PlainText);
    assert_eq!(detect(None, ""), Format::PlainText);
    assert_eq!(bp_data_accepts(Format::PlainText, ""), Ok(()));
}

/// The formats `bp-formats` groups as structured or tabular data but that
/// `bp-data` cannot parse at all.
///
/// Not a failure -- neither crate promises them -- but a trap with a name.
/// `Profile::is_data` documents itself as "whether the Data menu's operations
/// apply", and it answers yes for both of these. The first caller to use it
/// to enable a menu gets a menu whose every item fails.
#[test]
fn formats_bp_data_cannot_parse_are_named_here() {
    for format in [Format::Ini, Format::Xml] {
        assert!(
            format.profile().is_data(),
            "{} is no longer grouped as data; update this test",
            format.label()
        );
    }
    // And the ones that are grouped as data and *are* parseable, so that a
    // format added to the group without a parser changes this list.
    for format in [
        Format::Json,
        Format::JsonLines,
        Format::Yaml,
        Format::Toml,
        Format::Csv,
        Format::Tsv,
    ] {
        assert!(
            format.profile().is_data(),
            "{} left the group",
            format.label()
        );
    }
}

// --- ADR-0023: the three YAML bounds, reached through detection ------------

/// A chain of `depth` nested flow sequences: `[[[...]]]`.
fn nested_sequences(depth: usize) -> String {
    format!("{}{}", "[".repeat(depth), "]".repeat(depth))
}

#[test]
fn yaml_nesting_is_accepted_at_the_limit_and_refused_one_past_it() {
    let path = PathBuf::from("deep.yaml");

    let at_limit = nested_sequences(128);
    assert_eq!(detect(Some(&path), &at_limit), Format::Yaml);
    assert!(
        yaml_validate(&at_limit).is_ok(),
        "128 levels is the documented limit and must be accepted"
    );

    let past_limit = nested_sequences(129);
    let err = yaml_validate(&past_limit).expect_err("129 levels must be refused");
    let message = err.to_string();
    assert!(
        message.contains("128"),
        "the refusal must say what the limit is: {message}"
    );
}

#[test]
fn alias_expansion_is_refused_rather_than_exhausting_memory() {
    // The "billion laughs" shape: eight anchors, each ten references to the
    // one before. Ten levels deep at most, so nesting alone does not catch
    // it -- the expanded node count is what does.
    let mut text = String::from("a0: &a0 [x, x, x, x, x, x, x, x, x, x]\n");
    for level in 1..8 {
        let previous = level - 1;
        let refs: Vec<String> = (0..10).map(|_| format!("*a{previous}")).collect();
        text.push_str(&format!("a{level}: &a{level} [{}]\n", refs.join(", ")));
    }

    let err = yaml_validate(&text).expect_err("an alias bomb must be refused");
    assert!(
        err.to_string().contains("nodes"),
        "the refusal must name what was too large: {err}"
    );
}

#[test]
fn a_duplicate_mapping_key_is_refused_rather_than_silently_overwritten() {
    // The loader drops the first value on the way into the map, so by the
    // time there is a tree there is nothing left to report. This is the one
    // bound whose absence would be invisible.
    let err = yaml_validate("title: first\ntitle: second\n")
        .expect_err("a duplicate key must be refused");
    let message = err.to_string();
    assert!(
        message.contains("duplicate"),
        "the refusal must say what is wrong: {message}"
    );
    assert!(
        yaml_validate("title: first\nsubtitle: second\n").is_ok(),
        "distinct keys must still be accepted"
    );
}

#[test]
fn a_multi_document_yaml_file_is_counted_as_several() {
    let text = "---\na: 1\n---\nb: 2\n---\nc: 3\n";
    let path = PathBuf::from("stream.yaml");
    assert_eq!(detect(Some(&path), text), Format::Yaml);
    assert_eq!(yaml_document_count(text).expect("count"), 3);
    assert!(yaml_validate(text).is_ok());
}
