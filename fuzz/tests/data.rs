//! Target 4: `bp_data`'s JSON, JSONL, TOML and delimited-text entry points,
//! and `bp_formats::sniff`.
//!
//! Everything `bp_data` exposes that takes a `&str` from a file somebody
//! else wrote. YAML has its own file -- it is the one with a documented crash
//! behind it -- and these are the rest.
//!
//! `bp_formats::sniff` is here rather than in a file of its own because it is
//! two hundred lines of prefix matching over the first kilobyte of a
//! document, it is called on *every* file the editor opens before anything
//! else looks at it, and it is total: it returns a `Format`, never a
//! `Result`. A total function's only possible failure is a panic, which makes
//! it exactly the shape this harness is for and a very short thing to test.
//!
//! The invariant: **each may return an error; none may panic, abort, or
//! hang.**

use std::time::Duration;

use bp_fuzz::{
    Outcome, Probe, assert_no_violations, corpus, on_app_stack, proptest_config, strategies,
};
use proptest::prelude::*;

/// Every `&str` entry point in the crate except the YAML ones.
///
/// Run over the same input regardless of what the input looks like. A CSV
/// reader handed JSON is not a mistake to guard against here -- it is what
/// happens when a user picks the wrong menu item, and it must produce a
/// sentence rather than a crash.
fn exercise(text: &str) {
    let _ = bp_data::json_validate(text);
    let _ = bp_data::json_format(text);
    let _ = bp_data::json_minify(text);
    let _ = bp_data::json_sort_keys(text);
    let _ = bp_data::json_to_jsonl(text);

    let report = bp_data::jsonl_validate(text);
    let _ = report.is_valid();
    let _ = report.summary();
    let _ = bp_data::jsonl_to_json(text);

    let _ = bp_data::toml_validate(text);
    let _ = bp_data::toml_format(text);

    let delimiter = bp_data::detect_delimiter(text);
    let _ = bp_data::delimited_rows(text, delimiter);
    // A delimiter the text does not use, and one that is not punctuation at
    // all: both are reachable, because the UI lets the user pick.
    let _ = bp_data::delimited_rows(text, '\u{1F600}');
    if let Ok(report) = bp_data::delimited_report(text) {
        let _ = report.summary();
    }
    let _ = bp_data::delimited_to_json(text);
    let _ = bp_data::delimited_to_jsonl(text);
    if let Ok(columns) = bp_data::column_types(text) {
        for column in &columns {
            let _ = column.summary();
        }
    }

    // Total, so the only thing it can do wrong is panic.
    let _ = bp_formats::sniff(text);
    let _ = bp_formats::detect(None, text);
    let _ = bp_formats::detect(Some(std::path::Path::new("x.unknown-extension")), text);
}

#[test]
fn the_corpus_is_survivable() {
    let outcomes: Vec<Outcome> = corpus("data")
        .into_iter()
        .map(|sample| {
            let Some(text) = sample.text() else {
                panic!("{} is not UTF-8; the data corpus is text", sample.name);
            };
            let owned = text.to_owned();
            Probe::new().run(sample.name.clone(), move || exercise(&owned))
        })
        .collect();

    assert_no_violations("data corpus", &outcomes);
}

/// The YAML corpus, run through the *other* readers.
///
/// Free coverage, and not a contrivance: a user who opens a YAML file and
/// hits "validate JSON" is the ordinary case this catches. The pathological
/// entries -- the alias bomb, the 100,000-level nesting -- are the ones worth
/// pointing at a reader that has never seen them.
#[test]
fn the_yaml_corpus_is_survivable_through_the_other_readers() {
    let outcomes: Vec<Outcome> = corpus("yaml")
        .into_iter()
        .filter_map(|sample| {
            let owned = sample.text()?.to_owned();
            Some(
                Probe::new().run(format!("as data: {}", sample.name), move || {
                    exercise(&owned);
                }),
            )
        })
        .collect();

    assert_no_violations("yaml corpus through data readers", &outcomes);
}

/// Nesting depth, for the formats that have their own limits.
///
/// `serde_json` enforces 128 levels -- ADR-0023 chose the YAML cap to match
/// it so both formats refuse at the same depth and "the shell has one
/// sentence to say rather than two". `toml` has its own. Neither claim is
/// this repository's, which is exactly why they are worth a probe: a
/// dependency's limit is a dependency's to change.
#[test]
fn no_nesting_depth_survives_into_a_stack_overflow() {
    let depths: Vec<usize> = (120..=136).chain([200, 1_000, 10_000, 100_000]).collect();

    let outcomes: Vec<Outcome> = depths
        .into_iter()
        .flat_map(|depth| {
            [
                ("json array", strategies::nested("[", "]", depth)),
                ("json object", strategies::nested("{\"a\":", "}", depth)),
                ("toml key", format!("{}v = 1\n", "a.".repeat(depth))),
            ]
            .map(|(shape, text)| {
                Probe::new().run(format!("{shape} depth {depth}"), move || exercise(&text))
            })
        })
        .collect();

    assert_no_violations("data nesting depth", &outcomes);
}

/// A very wide table and a very tall one.
///
/// Depth is not the only unbounded dimension in a document. `column_types`
/// keeps a report per column and `delimited_report` a record per row, so the
/// shape of the input decides the shape of the allocation.
#[test]
fn extreme_table_shapes_are_survivable() {
    let wide = format!(
        "{}\n{}\n",
        vec!["c"; 10_000].join(","),
        vec!["1"; 10_000].join(",")
    );
    let tall = format!("a,b\n{}", "1,2\n".repeat(20_000));
    // One field, enormous, and unterminated -- the quoting state machine with
    // nowhere to stop.
    let unterminated = format!("a,b\n\"{}\n", "x".repeat(50_000));

    let outcomes: Vec<Outcome> = [
        ("wide", wide),
        ("tall", tall),
        ("unterminated quote", unterminated),
    ]
    .into_iter()
    .map(|(name, text)| Probe::new().run(name, move || exercise(&text)))
    .collect();

    assert_no_violations("data table shapes", &outcomes);
}

// --- property runs ------------------------------------------------------

const BLOCK_BUDGET: Duration = Duration::from_secs(300);

#[test]
fn arbitrary_text_is_survivable() {
    on_app_stack("data arbitrary text", BLOCK_BUDGET, || {
        proptest!(proptest_config(), |(text in strategies::arbitrary_text())| {
            exercise(&text);
        });
    });
}

#[test]
fn json_punctuation_arranged_at_random_is_survivable() {
    on_app_stack("data json salad", BLOCK_BUDGET, || {
        proptest!(proptest_config(), |(text in strategies::json_salad())| {
            exercise(&text);
        });
    });
}

#[test]
fn toml_punctuation_arranged_at_random_is_survivable() {
    on_app_stack("data toml salad", BLOCK_BUDGET, || {
        proptest!(proptest_config(), |(text in strategies::toml_salad())| {
            exercise(&text);
        });
    });
}

#[test]
fn delimited_punctuation_arranged_at_random_is_survivable() {
    on_app_stack("data csv salad", BLOCK_BUDGET, || {
        proptest!(proptest_config(), |(
            text in strategies::csv_salad(),
            delimiter in proptest::sample::select(&[',', ';', '\t', '|', '"', '\n', 'a', '\u{1F600}'][..]),
        )| {
            let _ = bp_data::delimited_rows(&text, delimiter);
            let _ = bp_data::delimited_report(&text);
            let _ = bp_data::column_types(&text);
            let _ = bp_formats::sniff(&text);
        });
    });
}

/// Whatever `json_format` writes must parse back.
///
/// Not a survival property but a round-trip one, and it belongs here because
/// a formatter that emits something its own reader rejects is how a user
/// loses a file: they format, they save, and the next open fails. `bp_data`
/// found exactly this in the YAML emitter -- a key over 1024 characters that
/// "will not parse on the next open" -- so the same question is worth asking
/// of the others.
#[test]
fn anything_these_writers_emit_can_be_read_back() {
    on_app_stack("data round trip", BLOCK_BUDGET, || {
        proptest!(proptest_config(), |(text in strategies::json_salad())| {
            if let Ok(formatted) = bp_data::json_format(&text) {
                prop_assert!(
                    bp_data::json_validate(&formatted).is_ok(),
                    "json_format wrote something json_validate rejects"
                );
            }
            if let Ok(minified) = bp_data::json_minify(&text) {
                prop_assert!(
                    bp_data::json_validate(&minified).is_ok(),
                    "json_minify wrote something json_validate rejects"
                );
            }
            if let Ok(sorted) = bp_data::json_sort_keys(&text) {
                prop_assert!(
                    bp_data::json_validate(&sorted).is_ok(),
                    "json_sort_keys wrote something json_validate rejects"
                );
            }
        });
    });
}
