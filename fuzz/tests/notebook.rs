//! Target 3: `bp_notebook::import_ipynb` on arbitrary JSON.
//!
//! An `.ipynb` is a file people are sent, and importing one is a parse of
//! somebody else's JSON into a typed model with a dozen optional fields, each
//! of which can be the wrong type. `ImportWarning` exists precisely because
//! the import is expected to meet input it cannot use; the invariant is that
//! it says so rather than falling over.
//!
//! **It may return an `IpynbError`, and it may return warnings; it may not
//! panic, abort, or hang.**
//!
//! Both entry points are covered: [`bp_notebook::import_ipynb`], which takes
//! a parsed `Value`, and [`bp_notebook::parse_raw_json_view`], which takes
//! text -- the raw-JSON editor writes into the second one, so it meets input
//! a human has just hand-edited into an invalid state.

use std::time::Duration;

use bp_fuzz::{
    Outcome, Probe, assert_no_violations, corpus, on_app_stack, proptest_config, strategies,
};
use proptest::prelude::*;
use serde_json::{Value, json};

fn exercise_value(value: &Value) {
    if let Ok(import) = bp_notebook::import_ipynb(value) {
        // Export what was imported, and import that again. A round trip is
        // where an importer's leniency turns into an exporter's crash: a
        // field accepted as "missing, warned about" has to be writable back
        // as something.
        let exported = bp_notebook::export_ipynb(import.notebook());
        let _ = bp_notebook::import_ipynb(&exported);
        let raw = bp_notebook::raw_json_view(import.notebook());
        let _ = bp_notebook::parse_raw_json_view(&raw);
        let _ = import.warnings();
        let _ = import.is_lossless();
    }
}

fn exercise_text(text: &str) {
    let _ = bp_notebook::parse_raw_json_view(text);
    if let Ok(value) = serde_json::from_str::<Value>(text) {
        exercise_value(&value);
    }
}

#[test]
fn the_corpus_is_survivable() {
    let outcomes: Vec<Outcome> = corpus("notebook")
        .into_iter()
        .map(|sample| {
            let bytes = sample.bytes.clone();
            Probe::new().run(sample.name.clone(), move || {
                // A `.ipynb` that is not UTF-8 never reaches the JSON parser
                // at all, which is one of the things a reader has to
                // survive rather than a case to skip loudly.
                if let Ok(text) = std::str::from_utf8(&bytes) {
                    exercise_text(text);
                }
            })
        })
        .collect();

    assert_no_violations("notebook corpus", &outcomes);
}

// --- generators ---------------------------------------------------------

/// Keys an `.ipynb` reader looks for, plus arbitrary ones.
///
/// The same coverage substitute the other targets use. A generator producing
/// random object keys will never write `"execution_count"`, so the branch
/// reading it is never reached; handed the vocabulary, it reaches all of them
/// in the first few dozen cases.
fn notebook_key() -> impl Strategy<Value = String> {
    prop_oneof![
        9 => proptest::sample::select(
            &[
                "nbformat",
                "nbformat_minor",
                "cells",
                "metadata",
                "cell_type",
                "source",
                "outputs",
                "execution_count",
                "id",
                "attachments",
                "output_type",
                "data",
                "text",
                "name",
                "ename",
                "evalue",
                "traceback",
                "language_info",
                "kernelspec",
                "language",
                "bachelorpad",
                "text/plain",
                "image/png",
            ][..],
        )
        .prop_map(str::to_owned),
        1 => r"(?s).{0,20}".prop_map(String::from),
    ]
}

/// Arbitrary JSON, shallow enough to stay inside `serde_json`'s own 128-level
/// limit -- the deep case is a corpus entry, not a generated one, because
/// depth is a single number rather than a space worth searching.
fn arbitrary_json() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<i64>().prop_map(|n| json!(n)),
        any::<f64>()
            .prop_filter("JSON has no NaN or infinity", |f| f.is_finite())
            .prop_map(|f| json!(f)),
        r"(?s).{0,30}".prop_map(Value::String),
    ];

    leaf.prop_recursive(6, 96, 6, |inner| {
        prop_oneof![
            proptest::collection::vec(inner.clone(), 0..6).prop_map(Value::Array),
            proptest::collection::vec((notebook_key(), inner), 0..6)
                .prop_map(|pairs| { Value::Object(pairs.into_iter().collect()) }),
        ]
    })
}

/// Something that at least gets past the first two checks: an object with a
/// version and a cell array.
///
/// Without this, almost every generated value dies at `NotAnObject` or
/// `NoFormatVersion` and the cell reader -- which is where the interesting
/// code is -- is never entered.
fn notebook_shaped() -> impl Strategy<Value = Value> {
    (
        prop_oneof![
            Just(json!(4)),
            Just(json!(3)),
            any::<i64>().prop_map(|n| json!(n))
        ],
        proptest::collection::vec(arbitrary_json(), 0..8),
        arbitrary_json(),
    )
        .prop_map(|(nbformat, cells, metadata)| {
            json!({
                "nbformat": nbformat,
                "nbformat_minor": 5,
                "cells": cells,
                "metadata": metadata,
            })
        })
}

// --- property runs ------------------------------------------------------

const BLOCK_BUDGET: Duration = Duration::from_secs(300);

#[test]
fn arbitrary_json_values_are_survivable() {
    on_app_stack("notebook arbitrary json", BLOCK_BUDGET, || {
        proptest!(proptest_config(), |(value in arbitrary_json())| {
            exercise_value(&value);
        });
    });
}

#[test]
fn notebook_shaped_json_is_survivable() {
    on_app_stack("notebook shaped json", BLOCK_BUDGET, || {
        proptest!(proptest_config(), |(value in notebook_shaped())| {
            exercise_value(&value);
        });
    });
}

#[test]
fn arbitrary_text_through_the_raw_view_is_survivable() {
    on_app_stack("notebook raw view", BLOCK_BUDGET, || {
        proptest!(proptest_config(), |(text in strategies::arbitrary_text())| {
            exercise_text(&text);
        });
    });
}

#[test]
fn json_punctuation_arranged_at_random_is_survivable() {
    on_app_stack("notebook json salad", BLOCK_BUDGET, || {
        proptest!(proptest_config(), |(text in strategies::json_salad())| {
            exercise_text(&text);
        });
    });
}

/// A real notebook with one field replaced by arbitrary JSON.
///
/// Starts from something that imports cleanly and breaks one thing, so every
/// case reaches the reader for that field instead of dying at the door.
#[test]
fn a_notebook_with_one_field_replaced_is_survivable() {
    on_app_stack("notebook field replacement", BLOCK_BUDGET, || {
        proptest!(proptest_config(), |(
            key in notebook_key(),
            replacement in arbitrary_json(),
            in_cell in any::<bool>(),
        )| {
            let mut value = json!({
                "nbformat": 4,
                "nbformat_minor": 5,
                "metadata": {"language_info": {"name": "python"}},
                "cells": [{
                    "cell_type": "code",
                    "id": "a",
                    "source": "print(1)",
                    "metadata": {},
                    "execution_count": 1,
                    "outputs": [{"output_type": "stream", "name": "stdout", "text": "1\n"}],
                }],
            });

            if in_cell {
                value["cells"][0][&key] = replacement;
            } else {
                value[&key] = replacement;
            }
            exercise_value(&value);
        });
    });
}
