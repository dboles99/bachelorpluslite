//! Seam: `bp_formats::sniff`'s verdict against a parser that can contradict
//! it.
//!
//! **This restores a property [ADR-0062](../../../docs/decisions/ADR-0062.md)
//! removed**, and the reason it is worth restoring is in that ADR:
//!
//! > Removing a caller does not only remove work. It can remove the only
//! > thing that could contradict a claim.
//!
//! `sniff` guesses a document's format from its first kilobyte and runs on
//! every file the editor opens. Until `bp-data` left, whatever it guessed had
//! to be parseable *as that*, and a cross-crate test said so. **That test
//! found a real defect**: `sniff` counted lines beginning with `{`, called two
//! or more of them JSON Lines, and so classified a pretty-printed array of
//! objects — the commonest shape JSON comes in — as a format its own parser
//! would reject.
//!
//! ## Why this is a test and not a feature
//!
//! `serde_json` is already a workspace dependency and already a
//! dev-dependency here, so **this adds nothing to the shipped binary**. The
//! Data menu is gone and is not coming back
//! ([ADR-0062](../../../docs/decisions/ADR-0062.md)); what was worth keeping
//! was never the menu, it was the *contradiction* — something that could
//! demonstrate `sniff` was wrong.
//!
//! ## What is checkable and what is not
//!
//! `sniff` reads a prefix and guesses. It is not validation and does not
//! claim to be, so "this text starts with `{` and is not JSON" is not a
//! defect — `Format::Json` is the right guess for a malformed JSON file, and
//! the user gets a syntax-highlighting profile rather than an error.
//!
//! **The falsifiable claim is narrower and is the one that broke**: for a
//! document that *is* valid JSON, `sniff` must not classify it as something a
//! parser then refuses. Everything below is that claim.

mod common;

use bp_formats::{Format, sniff};
use common::no_files;
use proptest::prelude::*;
use serde_json::Value;

/// Whether the parser `sniff`'s verdict implies will accept `text`.
///
/// `Ok(())` for a format with no parser behind it -- this asks whether a
/// verdict is *contradicted*, and a verdict nothing can check is not.
fn the_implied_parser_accepts(format: Format, text: &str) -> Result<(), String> {
    match format {
        Format::Json => serde_json::from_str::<Value>(text)
            .map(|_| ())
            .map_err(|e| e.to_string()),
        Format::JsonLines => {
            for (index, line) in text.lines().enumerate() {
                if line.trim().is_empty() {
                    continue;
                }
                serde_json::from_str::<Value>(line)
                    .map_err(|e| format!("line {}: {e}", index + 1))?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

// --- the shapes that broke it -------------------------------------------

/// The three fixtures the original test carried, kept verbatim.
///
/// They are a record of the defect, not an expectation of the rule: each is a
/// pretty-printed JSON document whose inner lines begin with `{` or `[`, which
/// is what the old line-counting rule mistook for JSON Lines.
const PRETTY_PRINTED: [&str; 3] = [
    "[\n  {\"id\": 1},\n  {\"id\": 2}\n]\n",
    "[\n  [1, 2],\n  [3, 4]\n]\n",
    "{\n  \"outer\": {\n    \"a\": 1\n  },\n  \"other\": {\n    \"b\": 2\n  }\n}\n",
];

#[test]
fn a_pretty_printed_json_document_is_not_called_json_lines() {
    for text in PRETTY_PRINTED {
        assert!(
            serde_json::from_str::<Value>(text).is_ok(),
            "the fixture is not valid JSON"
        );

        let sniffed = sniff(text);
        assert_eq!(
            the_implied_parser_accepts(sniffed, text),
            Ok(()),
            "sniff called a valid JSON document {} and a parser refused it",
            sniffed.label()
        );
    }
}

#[test]
fn a_real_json_lines_document_is_called_json_lines() {
    // The other direction, and it has to hold too: a rule tightened until it
    // never says JsonLines would pass the test above and be useless.
    let text = "{\"a\": 1}\n{\"a\": 2}\n{\"a\": 3}\n";
    assert_eq!(sniff(text), Format::JsonLines);
    assert_eq!(the_implied_parser_accepts(Format::JsonLines, text), Ok(()));
}

#[test]
fn a_single_json_value_on_one_line_is_a_json_document() {
    // One line cannot be JSON Lines: `looks_like_json_lines` requires a
    // second. Worth pinning because the boundary is where an off-by-one in
    // that rule would land.
    let text = "{\"a\": 1}\n";
    assert_eq!(sniff(text), Format::Json);
    assert_eq!(the_implied_parser_accepts(Format::Json, text), Ok(()));
}

// --- the property, over generated documents ------------------------------

/// A JSON value, deep enough to be worth pretty-printing.
///
/// Leaves first, then three levels of arrays and objects around them --
/// `prop_recursive`'s shape rather than a hand-rolled depth counter, so the
/// generator shrinks properly when a case fails.
fn json_value() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<i32>().prop_map(Value::from),
        "[ -~]{0,12}".prop_map(Value::from),
    ];
    leaf.prop_recursive(3, 16, 4, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..4).prop_map(Value::from),
            prop::collection::vec(("[a-z]{1,6}", inner), 0..4)
                .prop_map(|pairs| { Value::Object(pairs.into_iter().collect()) }),
        ]
    })
}

proptest! {
    #![proptest_config(no_files(256))]

    /// **Whatever `sniff` calls a valid JSON document, a parser must accept
    /// it as that.**
    ///
    /// Both spellings, because the defect was specific to one: `to_string`
    /// puts everything on one line and cannot trip the JSON Lines rule, while
    /// `to_string_pretty` is what produced the inner lines beginning with `{`.
    #[test]
    fn a_valid_json_document_is_classified_as_something_that_parses(value in json_value()) {
        for text in [
            serde_json::to_string(&value).expect("compact"),
            serde_json::to_string_pretty(&value).expect("pretty"),
        ] {
            let sniffed = sniff(&text);
            prop_assert_eq!(
                the_implied_parser_accepts(sniffed, &text),
                Ok(()),
                "sniff called this {} and a parser refused it:\n{}",
                sniffed.label(),
                text
            );
        }
    }
}
