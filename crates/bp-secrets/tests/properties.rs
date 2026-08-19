//! Property tests for the secret scanner.
//!
//! The unit tests inside the crate pin down what each rule recognises. These
//! check the two things that must hold for *every* input, including the ones
//! nobody thought of: the scan must not panic, and a finding must point at
//! text that is actually there.
//!
//! The second is the one that matters in practice. A `Finding` is handed
//! straight to the editor to move the caret and highlight a span, so a
//! position off the end of a line is a panic in the shell rather than a wrong
//! answer in a library.

use bp_secrets::{Confidence, Finding, scan};
use proptest::prelude::*;

/// Fragments of text a document is assembled from. A generator of pure noise
/// almost never produces anything a rule fires on, so the property would
/// exercise the "no findings" path for ever; this mixture makes real matches
/// common while still leaving room for nonsense.
///
/// Every credential here is fabricated.
const FRAGMENTS: &[&str] = &[
    "AKIA3G7QVHBRN2WPKZ5F",
    "ghp_R2d9KpXvA7mQzL3nB8wYtE6sJ1uH0cVfNgD4",
    "glpat-7fKq2mZxR9vLpN3wBtY6",
    "xoxb-5839274016-5839274016482-Rq7ZmXvT3nB9wYtE6sJ1uHpK",
    "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiI5OTg4Nzc2NjU1In0.SflKxwRJSMeKKF2QT4fw",
    "-----BEGIN RSA PRIVATE KEY-----",
    "-----BEGIN CERTIFICATE-----",
    "api_key = \"Xk92mQvRt7LpZn3BwYs6\"",
    "AWS_SECRET_ACCESS_KEY=wJalrXUtnFEMI/K7MDENG/bPxRfiCYzKvQ2nR8dT",
    "postgres://svcuser:Rk7mZq3XvB9n@db.internal:5432/orders",
    "Server=tcp:db01;Database=Orders;Password=Rk7mZq3XvB9n;",
    "commit = a94a8fe5ccb19ba61c4c0873d391e987982fbbd3",
    "session_id = \"550e8400-e29b-41d4-a716-446655440000\"",
    // Multi-byte text, so the byte-to-character conversion is exercised.
    "\u{1f511} \u{65e5}\u{672c}\u{8a9e} caf\u{e9}",
    "\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}",
    // The awkward shapes: separators and quotes with nothing behind them.
    "=",
    "::",
    "a=\"",
    "-----BEGIN",
    "eyJ",
    "://",
    "",
    "   ",
];

/// Documents built from the fragments above, joined by real separators.
fn any_document() -> impl Strategy<Value = String> {
    prop::collection::vec(
        (
            prop::sample::select(FRAGMENTS),
            prop::sample::select(&[" ", "\n", "\r\n", "\t", ", ", "\"", "'", ""][..]),
        ),
        0..24,
    )
    .prop_map(|parts| {
        parts
            .into_iter()
            .map(|(fragment, joiner)| format!("{fragment}{joiner}"))
            .collect()
    })
}

/// The invariant, checked the same way whatever produced the document.
fn assert_findings_are_inside(text: &str, findings: &[Finding]) -> Result<(), TestCaseError> {
    let lines: Vec<&str> = text.lines().collect();

    for found in findings {
        prop_assert!(found.line >= 1, "line numbers are 1-based: {found:?}");
        prop_assert!(
            found.line <= lines.len(),
            "{found:?} is past the last of {} lines",
            lines.len()
        );

        let line = lines[found.line - 1];
        let characters = line.chars().count();

        prop_assert!(found.column >= 1, "columns are 1-based: {found:?}");
        prop_assert!(found.length >= 1, "an empty span points at nothing");
        prop_assert!(
            found.column - 1 + found.length <= characters,
            "{found:?} runs off the end of a {characters}-character line"
        );
    }

    Ok(())
}

proptest! {
    /// The scan must survive anything, including bytes no editor would
    /// produce. A panic here is a crash in the shell.
    #[test]
    fn scanning_arbitrary_text_never_panics(text in "(?s).{0,2000}") {
        let _ = scan(&text);
    }

    /// Every finding points inside the text it was found in.
    #[test]
    fn every_finding_lands_inside_the_text_it_came_from(text in any_document()) {
        assert_findings_are_inside(&text, &scan(&text))?;
    }

    /// The same, for text that was never meant to be scanned.
    #[test]
    fn every_finding_lands_inside_arbitrary_text(text in "(?s).{0,2000}") {
        assert_findings_are_inside(&text, &scan(&text))?;
    }

    /// Findings arrive in document order, which the UI relies on to walk them
    /// with "next secret" without sorting first.
    #[test]
    fn findings_are_in_document_order_and_never_overlap(text in any_document()) {
        let findings = scan(&text);
        for pair in findings.windows(2) {
            let (first, second) = (pair[0], pair[1]);
            prop_assert!(
                (first.line, first.column) < (second.line, second.column),
                "{first:?} then {second:?}"
            );
            if first.line == second.line {
                prop_assert!(
                    first.column + first.length <= second.column,
                    "{first:?} overlaps {second:?}"
                );
            }
        }
    }

    /// Nothing in the crate reads a clock, a file or a global, so two scans
    /// of the same text are the same scan.
    #[test]
    fn scanning_is_deterministic(text in any_document()) {
        prop_assert_eq!(scan(&text), scan(&text));
    }

    /// Prefixing a document with whole lines moves every finding down by
    /// exactly that many lines and leaves the columns alone. This is what
    /// makes a finding usable as a caret target after an edit above it.
    #[test]
    fn adding_lines_above_shifts_every_finding_by_that_many_lines(
        text in any_document(),
        above in 0usize..5,
    ) {
        let before = scan(&text);
        let shifted = scan(&format!("{}{text}", "prelude\n".repeat(above)));

        prop_assert_eq!(shifted.len(), before.len());
        for (original, moved) in before.iter().zip(shifted.iter()) {
            prop_assert_eq!(moved.line, original.line + above);
            prop_assert_eq!(moved.column, original.column);
            prop_assert_eq!(moved.length, original.length);
            prop_assert_eq!(moved.kind, original.kind);
        }
    }

    /// A caller that filters on confidence gets a subset, never a different
    /// set. Cheap to state, and it pins the meaning of the ordering.
    #[test]
    fn filtering_by_confidence_only_ever_removes_findings(text in any_document()) {
        let all = scan(&text);
        let strict: Vec<_> = all
            .iter()
            .copied()
            .filter(|f| f.confidence >= Confidence::High)
            .collect();

        prop_assert!(strict.len() <= all.len());
        for found in &strict {
            prop_assert!(all.contains(found));
        }
    }
}
