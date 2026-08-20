//! Generators shared by the property tests in every module.
//!
//! One place rather than three, because a round-trip property is only worth
//! what its generator covers: if the notebook strategy stops producing
//! multi-byte text or empty cells, three separate copies of it would stop
//! quietly and at different times.

use proptest::prelude::*;
use serde_json::{Map, Value};

use crate::cell::CellKind;
use crate::notebook::Notebook;
use crate::output::{Output, Stream};

/// Any of the eight kinds, so a generated notebook is mixed-language by
/// default rather than by special case.
pub(crate) fn arb_kind() -> impl Strategy<Value = CellKind> {
    prop::sample::select(CellKind::ALL.to_vec())
}

/// Cell text worth splitting: empty strings, newlines at both ends, and
/// multi-byte characters, because those are where an offset-based operation or
/// a line-splitting serialiser goes wrong.
pub(crate) fn arb_source() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(String::new()),
        Just("\n".to_owned()),
        "[a-z ]{0,12}",
        "[a-z]{0,4}\n[a-z]{0,4}\n",
        "(日本|é|👍){0,3}[a-z]{0,4}",
    ]
}

/// Arbitrary JSON, kept to values that survive a text round trip exactly.
/// Floating point is left out on purpose: a property that fails on the last
/// bit of a `f64` would be testing `serde_json`, not this crate.
pub(crate) fn arb_json() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<i32>().prop_map(|n| Value::from(i64::from(n))),
        "[a-z ]{0,8}".prop_map(Value::from),
    ];
    leaf.prop_recursive(3, 12, 3, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..3).prop_map(Value::Array),
            prop::collection::btree_map("[a-z]{1,4}", inner, 0..3)
                .prop_map(|map| Value::Object(map.into_iter().collect())),
        ]
    })
}

/// Passenger metadata, never using the reserved `bachelorpad` key -- that one
/// is documented as ours and as not surviving a save, and generating it would
/// be asserting the opposite.
pub(crate) fn arb_metadata() -> impl Strategy<Value = Map<String, Value>> {
    prop::collection::btree_map(
        prop::sample::select(vec!["tags", "slideshow", "jupyter", "vscode"]),
        arb_json(),
        0..3,
    )
    .prop_map(|map| {
        map.into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect()
    })
}

/// A file shaped like a notebook but written by nobody in particular.
///
/// The point is to reach the import path, which arbitrary JSON never would --
/// nothing random is ever going to produce `nbformat: 4` with a `cells` array,
/// so a property fed plain [`arb_json`] would pass by never getting past the
/// first two checks. Everything here is the right shape and the wrong content:
/// cell types Jupyter never had, source that is a number, outputs that are
/// strings, execution counts, kinds that contradict the cell type, and
/// identifiers two cells both claim.
pub(crate) fn arb_ipynb_json() -> impl Strategy<Value = Value> {
    let cell = (
        prop::sample::select(vec![
            "code", "code", "markdown", "raw", "heading", "", "CODE",
        ]),
        prop_oneof![
            arb_source().prop_map(Value::from),
            prop::collection::vec(arb_source(), 0..3)
                .prop_map(|lines| Value::Array(lines.into_iter().map(Value::from).collect())),
            arb_json(),
        ],
        arb_foreign_cell_metadata(),
        prop::collection::vec(arb_output_json(), 0..3),
        prop_oneof![Just(Value::Null), (1i64..4).prop_map(Value::from)],
        // Ids two cells will fight over, plus ones we cannot use at all.
        prop::sample::select(vec!["1", "1", "0", "a9f", ""]),
    )
        .prop_map(
            |(cell_type, source, metadata, outputs, execution_count, id)| {
                serde_json::json!({
                    "cell_type": cell_type,
                    "source": source,
                    "metadata": metadata,
                    "outputs": outputs,
                    "execution_count": execution_count,
                    "id": id,
                })
            },
        );

    (
        prop::sample::select(vec![Value::from(4), Value::from(3), Value::Null]),
        prop::collection::vec(cell, 0..4),
        arb_metadata(),
    )
        .prop_map(|(nbformat, cells, metadata)| {
            serde_json::json!({
                "nbformat": nbformat,
                "nbformat_minor": 5,
                "cells": cells,
                "metadata": metadata,
            })
        })
}

fn arb_foreign_cell_metadata() -> impl Strategy<Value = Value> {
    prop_oneof![
        arb_metadata().prop_map(Value::Object),
        // Our own key, sometimes naming a kind that contradicts the cell type.
        prop::sample::select(vec!["python", "markdown", "raw", "plain_text", "brainfuck",])
            .prop_map(|kind| serde_json::json!({ "bachelorpad": { "kind": kind } })),
        Just(serde_json::json!({ "vscode": { "languageId": "sql" } })),
        arb_json(),
    ]
}

fn arb_output_json() -> impl Strategy<Value = Value> {
    prop_oneof![
        Just(serde_json::json!({ "output_type": "stream", "name": "stdout", "text": "x\n" })),
        Just(serde_json::json!({ "output_type": "stream", "name": "stderr", "text": ["a", "b"] })),
        Just(serde_json::json!({
            "output_type": "error", "ename": "E", "evalue": "v", "traceback": ["one"]
        })),
        Just(serde_json::json!({
            "output_type": "display_data", "metadata": {},
            "data": { "text/plain": "p", "text/html": "<b/>" }
        })),
        Just(serde_json::json!({ "output_type": "update_display_data", "data": {} })),
        arb_json(),
    ]
}

fn arb_output() -> impl Strategy<Value = Output> {
    prop_oneof![
        (
            prop::sample::select(vec![Stream::Stdout, Stream::Stderr]),
            "[a-z\n]{0,10}"
        )
            .prop_map(|(stream, text)| Output::Text { stream, text }),
        (
            prop::collection::vec("[a-z]{0,3}", 0..3),
            prop::collection::vec(prop::collection::vec("[a-z0-9]{0,3}", 0..3), 0..3),
        )
            .prop_map(|(columns, rows)| Output::Table { columns, rows }),
        arb_json().prop_map(|value| Output::Json { value }),
        "<[a-z]{1,4}/>".prop_map(|html| Output::Html { html }),
        (
            prop::sample::select(vec!["image/png", "image/svg+xml", "application/pdf"]),
            "[A-Za-z0-9+/=]{0,8}",
            prop::option::of("[a-z ]{0,6}"),
        )
            .prop_map(|(media_type, data_base64, alt)| Output::Image {
                media_type: media_type.to_owned(),
                data_base64,
                alt,
            }),
        (
            "[A-Za-z]{0,8}",
            "[a-z ]{0,10}",
            prop::collection::vec("[a-z ]{0,8}", 0..3),
        )
            .prop_map(|(name, message, traceback)| Output::Error {
                name,
                message,
                traceback,
            }),
        ("[a-z.]{1,8}", "[a-z/]{1,8}", "[A-Za-z0-9+/=]{0,8}").prop_map(
            |(name, media_type, data_base64)| Output::File {
                name,
                media_type,
                data_base64,
            }
        ),
    ]
}

type CellSeed = (CellKind, String, bool, Vec<Output>, Map<String, Value>);

fn arb_cell_seed() -> impl Strategy<Value = CellSeed> {
    (
        arb_kind(),
        arb_source(),
        any::<bool>(),
        prop::collection::vec(arb_output(), 0..3),
        arb_metadata(),
    )
}

/// A notebook of between `cells.start` and `cells.end` cells, built through
/// the public API so that a generated notebook is one a caller could actually
/// have produced -- including the rule that only executable cells hold
/// outputs.
pub(crate) fn arb_notebook(cells: std::ops::Range<usize>) -> impl Strategy<Value = Notebook> {
    (
        prop::collection::vec(arb_cell_seed(), cells),
        arb_metadata(),
    )
        .prop_map(|(seeds, metadata)| {
            let mut notebook = Notebook::new();
            *notebook.metadata_mut() = metadata;
            for (kind, source, collapsed, outputs, cell_metadata) in seeds {
                let id = notebook.push(kind, source);
                let cell = notebook.cell_mut(id).expect("just pushed");
                cell.set_collapsed(collapsed);
                *cell.metadata_mut() = cell_metadata;
                if kind.is_executable() {
                    cell.set_outputs(outputs).expect("kind is executable");
                }
            }
            notebook
        })
}
