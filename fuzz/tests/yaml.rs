//! Target 1: `bp_data`'s YAML entry points.
//!
//! The highest-value target in the repository, and the reason this directory
//! exists at all. ADR-0023 caps YAML nesting at 128 levels and alias
//! expansion at 1,000,000 nodes, and it does so because the underlying
//! loader "walks a document through `load_node` -> `load_sequence` ->
//! `load_node` mutual recursion, one stack frame per level, and a stack
//! overflow aborts the process instead of returning an error the user could
//! be shown". A 200-byte input did that.
//!
//! Those two caps *are* the mitigation. Everything here exists to ask
//! whether they hold.
//!
//! The invariant, for every function below: **it may return an error; it may
//! not panic, abort, or hang.**
//!
//! Every probe runs on a [`bp_fuzz::MAIN_THREAD_STACK`]-sized thread. That is
//! not decoration: ADR-0023's depth numbers were measured on the 1 MiB main
//! thread, libtest gives a test thread considerably more, and a cap validated
//! against the roomier stack would prove nothing about the binary a user
//! runs.

use std::time::Duration;

use bp_fuzz::{
    Outcome, Probe, assert_no_violations, corpus, on_app_stack, proptest_config, strategies,
};
use proptest::prelude::*;

/// Run one YAML string through every entry point `bp_data` exposes for it.
///
/// All of them, not just `yaml_validate`: the cap is enforced in `scan_yaml`
/// before a tree is built, and `yaml_format`, `yaml_minify` and `yaml_to_json`
/// all go on to build one. A guard that protected only the validator would
/// leave every other door open.
fn exercise(text: &str) {
    let _ = bp_data::yaml_validate(text);
    let _ = bp_data::yaml_document_count(text);
    let _ = bp_data::yaml_format(text);
    let _ = bp_data::yaml_minify(text);
    let _ = bp_data::yaml_to_json(text);
    // The other direction, on the same input: `json_to_yaml` emits YAML, and
    // an emitter recurses over the tree just as a loader does.
    let _ = bp_data::json_to_yaml(text);
}

#[test]
fn the_corpus_is_survivable() {
    let outcomes: Vec<Outcome> = corpus("yaml")
        .into_iter()
        .map(|sample| {
            let Some(text) = sample.text() else {
                panic!("{} is not UTF-8; the YAML corpus is text", sample.name);
            };
            let owned = text.to_owned();
            Probe::new().run(sample.name.clone(), move || exercise(&owned))
        })
        .collect();

    assert_no_violations("yaml corpus", &outcomes);
}

/// The claim ADR-0023 makes, tested as a claim rather than as an example.
///
/// Not "129 levels is refused" -- `bp_data` already tests that -- but "no
/// depth at all kills the process". A cap enforced by a counter is only worth
/// what the counter is worth, and the failure mode it guards against is an
/// abort, which no assertion inside the process can report after the fact.
#[test]
fn no_nesting_depth_survives_into_a_stack_overflow() {
    // Around the cap in single steps, then far past it. 100,000 is the depth
    // ADR-0023 says the *event stream* reads without touching the stack; if
    // the guard is ever reordered after the tree build, this is the entry
    // that stops being a test failure and starts being a crashed test binary.
    let depths: Vec<usize> = (0..=140)
        .chain([200, 512, 1_000, 10_000, 100_000])
        .collect();

    let outcomes: Vec<Outcome> = depths
        .into_iter()
        .map(|depth| {
            let flow = strategies::nested("[", "]", depth);
            Probe::new().run(format!("flow depth {depth}"), move || exercise(&flow))
        })
        .collect();

    assert_no_violations("yaml nesting depth", &outcomes);
}

/// The same question in block style, which is the form ADR-0023 measured.
#[test]
fn no_block_mapping_depth_survives_into_a_stack_overflow() {
    let outcomes: Vec<Outcome> = [130usize, 300, 1_000, 4_096, 20_000]
        .into_iter()
        .map(|depth| {
            let mut text = String::new();
            for level in 0..depth {
                text.push_str(&"  ".repeat(level));
                text.push_str("a:\n");
            }
            Probe::new().run(format!("block depth {depth}"), move || exercise(&text))
        })
        .collect();

    assert_no_violations("yaml block nesting depth", &outcomes);
}

/// The alias cap, which the nesting cap cannot see.
///
/// Ten levels deep and under a kilobyte, so depth is no defence; the node
/// count is the only thing standing between this input and an allocation the
/// machine cannot serve. Levels beyond about 20 also ask whether the counter
/// saturates rather than overflowing -- 10^20 does not fit in a `u64`.
#[test]
fn an_alias_bomb_is_refused_at_every_size() {
    let outcomes: Vec<Outcome> = [2usize, 5, 10, 15, 20, 40, 100]
        .into_iter()
        .map(|levels| {
            let mut bomb = String::from("a0: &a0 [x, x, x, x, x, x, x, x, x, x]\n");
            for level in 1..levels {
                bomb.push_str(&format!("a{level}: &a{level} ["));
                for reference in 0..10 {
                    if reference > 0 {
                        bomb.push_str(", ");
                    }
                    bomb.push_str(&format!("*a{}", level - 1));
                }
                bomb.push_str("]\n");
            }
            Probe::new().run(format!("alias bomb, {levels} levels"), move || {
                exercise(&bomb);
            })
        })
        .collect();

    assert_no_violations("yaml alias bomb", &outcomes);
}

// --- property runs ------------------------------------------------------

/// A budget for a whole `proptest` block.
///
/// Bounded on purpose: this is a gate stage, not a fuzzing session, and a
/// stage that can run for an hour is a stage people learn to skip.
const BLOCK_BUDGET: Duration = Duration::from_secs(300);

#[test]
fn arbitrary_text_is_survivable() {
    on_app_stack("yaml arbitrary text", BLOCK_BUDGET, || {
        proptest!(proptest_config(), |(text in strategies::arbitrary_text())| {
            exercise(&text);
        });
    });
}

#[test]
fn yaml_punctuation_arranged_at_random_is_survivable() {
    on_app_stack("yaml salad", BLOCK_BUDGET, || {
        proptest!(proptest_config(), |(text in strategies::yaml_salad())| {
            exercise(&text);
        });
    });
}

/// Salad wrapped in nesting, so the generator spends its cases *inside* a
/// deep document rather than only around shallow ones.
#[test]
fn nested_yaml_salad_is_survivable() {
    on_app_stack("yaml nested salad", BLOCK_BUDGET, || {
        proptest!(proptest_config(), |(depth in 0usize..300, inner in strategies::yaml_salad())| {
            let text = format!("{}{inner}{}", "[".repeat(depth), "]".repeat(depth));
            exercise(&text);
        });
    });
}
