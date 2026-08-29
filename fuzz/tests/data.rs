//! Target 3: `bp_formats::sniff` and `detect`, over the data corpus.
//!
//! **This file targeted `bp_data` until ADR-0062 removed that crate**, and it
//! is narrowed rather than deleted because `sniff` was always here too and is
//! covered nowhere else. Its name still says `data` -- that names the corpus
//! it reads (`corpus/data/`: JSON, JSON Lines, TOML and delimited text), not
//! the crate it exercises, and renaming a file needs approval that a sweep
//! should not assume.
//!
//! **`sniff` is exactly the shape this harness is for.** It is two hundred
//! lines of prefix matching over the first kilobyte of a document, it is
//! called on *every* file the editor opens before anything else looks at it,
//! and it is **total**: it returns a `Format`, never a `Result`. A total
//! function's only possible failure is a panic.
//!
//! What ADR-0062 took away is worth naming, because a shorter file reads like
//! a smaller risk and this one is not. `sniff` used to have a *consumer that
//! could disagree with it* -- `bp-data` either parsed a document as whatever
//! `sniff` called it, or refused, and a cross-crate test asserted the two
//! agreed. That test found a real defect: `sniff` counted lines beginning
//! with `{` and called a pretty-printed JSON array "JSON Lines". **Nothing
//! can ask that question now.** `sniff`'s answer reaches a status-bar label
//! and a syntax profile, so a wrong answer is now cosmetic rather than a
//! syntax error on a valid file -- but it is also unfalsifiable, and this
//! harness only proves it does not crash.
//!
//! The invariant, unchanged: **it may return anything; it may not panic,
//! abort, or hang.**

use std::time::Duration;

use bp_fuzz::{
    Outcome, Probe, assert_no_violations, corpus, on_app_stack, proptest_config, strategies,
};
use proptest::prelude::*;

/// Both entry points, over the same text.
///
/// `detect` is called twice: once with no path, which is the pasted-buffer
/// case that falls straight through to `sniff`, and once with an extension
/// nothing claims, which is the `report.json.bak` case. A path whose
/// extension *is* known never reaches `sniff` at all, so it is not the
/// interesting input here.
fn exercise(text: &str) {
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

// --- property runs ------------------------------------------------------

const BLOCK_BUDGET: Duration = Duration::from_secs(300);

#[test]
fn arbitrary_text_is_survivable() {
    on_app_stack("sniff arbitrary text", BLOCK_BUDGET, || {
        proptest!(proptest_config(), |(text in strategies::arbitrary_text())| {
            exercise(&text);
        });
    });
}

/// The three salads, because `sniff` decides by prefix.
///
/// Punctuation arranged at random is what defeats a prefix matcher, and each
/// generator produces the leading characters of a different branch: `{`/`[`
/// for the JSON arms, `[section]`/`key =` for TOML, and delimiters for the
/// tabular one. Kept as three rather than folded into `arbitrary_text`
/// because a random string almost never begins with any of them, which is the
/// coverage-feedback substitute this suite uses throughout.
#[test]
fn punctuation_arranged_at_random_is_survivable() {
    on_app_stack("sniff salads", BLOCK_BUDGET, || {
        proptest!(proptest_config(), |(text in prop_oneof![
            strategies::json_salad(),
            strategies::toml_salad(),
            strategies::csv_salad(),
        ])| {
            exercise(&text);
        });
    });
}

/// A first kilobyte is all `sniff` reads, so the boundary is worth a probe.
///
/// Not a survival property alone: it also pins that a document is classified
/// by its opening, so appending megabytes to a file cannot change what it is
/// called. That is the property the status bar depends on and nothing else
/// asserts now that the parser which used to disagree with `sniff` is gone.
#[test]
fn what_follows_the_first_kilobyte_cannot_change_the_verdict() {
    on_app_stack("sniff prefix", BLOCK_BUDGET, || {
        proptest!(proptest_config(), |(
            head in strategies::json_salad(),
            tail in strategies::arbitrary_text(),
        )| {
            // Only meaningful once the head alone fills the window sniff
            // reads; a shorter head means the tail is genuinely part of the
            // prefix and may legitimately change the answer.
            if head.len() >= 1024 {
                prop_assert_eq!(
                    bp_formats::sniff(&head),
                    bp_formats::sniff(&format!("{head}{tail}")),
                    "appending past the first kilobyte changed the verdict"
                );
            }
        });
    });
}
