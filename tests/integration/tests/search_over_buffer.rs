//! Seam: `bp-search`'s matches against `bp-buffer`'s rope and `bp-editor`'s
//! positions.
//!
//! Three crates, three ways of naming a place in a document, and no test
//! anywhere that they name the same place. `bp-search` works in character
//! offsets over a `&str`; `bp-buffer` works in character offsets over a rope;
//! `bp-editor` turns those into a 1-based line and column for the status bar
//! and the "go to line" box. Each is internally consistent. The defect lives
//! between them, and the two document features that expose it are the two
//! this editor explicitly supports: multi-byte characters and CRLF.
//!
//! What a user sees when this breaks is a find that highlights the wrong
//! text, or a results panel whose line numbers scroll the editor to the wrong
//! place -- both of which look like the search being "a bit off" rather than
//! like a bug with a cause.
//!
//! The fixtures are built from a small alphabet spelled out below, so nothing
//! that could belong to anybody reaches a failure report.

mod common;

use common::no_files;

use bp_buffer::{Buffer, Position};
use bp_editor::Editor;
use bp_search::{Query, count, find_all, next_match, plan_replace_all, previous_match};
use proptest::prelude::*;

/// Tokens a document is assembled from.
///
/// `é` is two bytes, `中` three and `😀` four, so any place a byte offset was
/// used where a character offset was meant lands inside one of them. `\r\n`
/// is here because a CRLF document is two characters per line break and one
/// line break, and something has to reconcile that.
const TOKENS: &[&str] = &[
    "needle", "a", " ", "é", "中", "😀", "\n", "\r\n", "haystack", "\t",
];

fn document() -> impl Strategy<Value = String> {
    proptest::collection::vec(proptest::sample::select(TOKENS), 0..60)
        .prop_map(|parts| parts.concat())
}

/// The patterns worth searching for in the documents above.
fn pattern() -> impl Strategy<Value = String> {
    proptest::sample::select(&["needle", "é", "中", "😀", "a", " ", "haystack", "e"][..])
        .prop_map(str::to_owned)
}

proptest! {
    #![proptest_config(no_files(256))]

    /// **A match's range names the same characters in the rope that it names
    /// in the string.**
    ///
    /// This is the byte-versus-character property. `bp-search` documents its
    /// offsets as character offsets precisely so they can be handed to the
    /// editor to select a match; if either side ever counted bytes, a
    /// document containing `é` would slice somewhere else -- and on the rope
    /// side, in the middle of a character.
    #[test]
    fn a_match_names_the_same_characters_in_the_rope(
        text in document(),
        pattern in pattern(),
    ) {
        let buffer = Buffer::from_text(&text);
        let matches = find_all(&text, &Query::literal(&pattern)).expect("literal query");

        for m in &matches {
            prop_assert!(
                m.range.end <= buffer.len_chars(),
                "a match ends at character {} of a {} character buffer",
                m.range.end,
                buffer.len_chars()
            );
            prop_assert_eq!(
                buffer.slice(m.range.clone()),
                m.text.clone(),
                "the rope holds something else at characters {}..{}",
                m.range.start,
                m.range.end
            );
        }
    }

    /// The count and the list agree. Two implementations of "how many", and
    /// the status bar shows one while the panel lists the other.
    #[test]
    fn counting_agrees_with_listing(text in document(), pattern in pattern()) {
        let query = Query::literal(&pattern);
        prop_assert_eq!(
            count(&text, &query).expect("count"),
            find_all(&text, &query).expect("find").len()
        );
    }

    /// **A match's reported line is the line the buffer puts it on**, for
    /// documents whose line breaks are LF and CRLF.
    ///
    /// Restricted to those two deliberately: the unrestricted version of this
    /// property fails, and is below under `#[ignore]` with the reason.
    #[test]
    fn a_matchs_line_number_agrees_with_the_buffer(
        parts in proptest::collection::vec(
            proptest::sample::select(&["needle", "a", "é", "中", "😀", "\n", "\r\n", " "][..]),
            0..60,
        ),
        pattern in pattern(),
    ) {
        let text: String = parts.concat();
        let buffer = Buffer::from_text(&text);

        for m in find_all(&text, &Query::literal(&pattern)).expect("find") {
            prop_assert_eq!(
                m.line,
                buffer.position_of(m.range.start).line,
                "search says line {} for a match at character {}, the buffer says line {}",
                m.line,
                m.range.start,
                buffer.position_of(m.range.start).line
            );
        }
    }

    /// **Going to a match's line and column puts the caret on the match.**
    ///
    /// The journey a "find in files" result actually makes: search reports a
    /// line, the editor is asked for it, and the caret has to land on the
    /// text the user was shown. `go_to_position` clamps rather than refusing,
    /// so an off-by-one here does not error -- it silently lands elsewhere,
    /// which is why it needs asserting rather than trusting.
    #[test]
    fn going_to_a_matchs_position_lands_on_the_match(
        parts in proptest::collection::vec(
            proptest::sample::select(&["needle", "a", "é", "中", "😀", "\n", "\r\n", " "][..]),
            0..60,
        ),
        pattern in pattern(),
    ) {
        let text: String = parts.concat();
        let buffer = Buffer::from_text(&text);
        let mut editor = Editor::new(&text);

        for m in find_all(&text, &Query::literal(&pattern)).expect("find") {
            let position = buffer.position_of(m.range.start);
            let in_range = editor.go_to_position(Position::new(position.line, position.column));
            prop_assert!(in_range, "a position the buffer produced was out of range");
            prop_assert_eq!(
                editor.cursor(),
                m.range.start,
                "the caret landed at character {} instead of {}",
                editor.cursor(),
                m.range.start
            );
        }
    }

    /// **`next_match` and `previous_match` reach every match and no other
    /// place.** Stepping forward from each match's own start lands on a real
    /// match, and stepping forward from the last one wraps to the first.
    #[test]
    fn stepping_through_matches_stays_on_matches(
        text in document(),
        pattern in pattern(),
    ) {
        let matches = find_all(&text, &Query::literal(&pattern)).expect("find");
        prop_assume!(!matches.is_empty());

        for m in &matches {
            let forward = next_match(&matches, m.range.start).expect("a match");
            prop_assert_eq!(forward.range.start, m.range.start);

            let back = previous_match(&matches, m.range.start);
            let back = back.expect("a match");
            prop_assert!(
                matches.iter().any(|other| other.range == back.range),
                "stepping back left the set of matches"
            );
        }

        let last = matches.last().expect("a match");
        let wrapped = next_match(&matches, last.range.end + 1).expect("a match");
        prop_assert_eq!(
            wrapped.range.start,
            matches[0].range.start,
            "the search did not wrap to the first match"
        );
    }

    /// **A replace plan applied through the rope produces the document the
    /// plan promised.**
    ///
    /// The preview the user confirms is `ReplacePlan::apply`'s string; what
    /// the editor actually does is remove and insert character ranges in the
    /// rope. Two implementations of the same edit, and the only reason they
    /// agree is that the offsets mean the same thing in both. Applied back to
    /// front so that earlier ranges are still valid after a later edit.
    #[test]
    fn a_replace_plan_applied_to_the_rope_matches_its_own_preview(
        text in document(),
        pattern in pattern(),
        replacement in proptest::sample::select(&["X", "", "ﬁ", "two words", "😀😀"][..]),
    ) {
        let query = Query::literal(&pattern);
        let plan = plan_replace_all(&text, &query, replacement).expect("plan");
        let expected = plan.replacements.clone();
        let promised = plan.apply();

        let mut buffer = Buffer::from_text(&text);
        for change in expected.iter().rev() {
            prop_assert_eq!(
                buffer.slice(change.range.clone()),
                change.before.clone(),
                "the plan describes text the rope does not hold at {}..{}",
                change.range.start,
                change.range.end
            );
            buffer.remove(change.range.clone());
            buffer.insert(change.range.start, &change.after);
        }

        prop_assert_eq!(
            buffer.to_string(),
            promised,
            "applying the plan to the rope produced a different document from its own preview"
        );
    }
}

// --- named cases ---------------------------------------------------------

#[test]
fn a_match_after_multi_byte_text_is_selected_correctly() {
    // Eight characters, seventeen bytes, before the match. A byte offset here
    // would select from the middle of the run.
    let text = "é中😀 é中😀 needle";
    let buffer = Buffer::from_text(text);

    let m = &find_all(text, &Query::literal("needle")).expect("find")[0];

    assert_eq!(m.range.start, text.chars().count() - "needle".len());
    assert_eq!(buffer.slice(m.range.clone()), "needle");
    assert_eq!(buffer.position_of(m.range.start), Position::new(1, 9));
}

#[test]
fn a_match_on_the_third_line_of_a_crlf_document_is_reported_as_line_three() {
    let text = "one\r\ntwo\r\nneedle\r\nfour\r\n";
    let buffer = Buffer::from_text(text);

    let m = &find_all(text, &Query::literal("needle")).expect("find")[0];

    assert_eq!(m.line, 3, "search disagrees with the document");
    assert_eq!(buffer.position_of(m.range.start), Position::new(3, 1));
    assert_eq!(buffer.slice(m.range.clone()), "needle");
}

#[test]
fn searching_an_empty_document_finds_nothing_rather_than_everything() {
    assert!(
        find_all("", &Query::literal("needle"))
            .expect("find")
            .is_empty()
    );
    // And an empty pattern is an empty find box, not a match at every offset.
    assert!(
        find_all("some text", &Query::literal(""))
            .expect("find")
            .is_empty()
    );
}

// --- a disagreement between the crates, left failing ---------------------

/// **DEFECT.** `bp-search` and `bp-buffer` do not agree on what a line is.
///
/// `bp_search::Offsets` counts line starts by looking for `'\n'` and nothing
/// else. `bp_buffer::Buffer` is a `ropey::Rope` built with ropey's default
/// features, which include `unicode_lines`: ropey treats a bare `\r`, `\x0B`,
/// `\x0C`, U+0085, U+2028 and U+2029 as line breaks too.
///
/// So for a document containing any of those, a find reports one line number
/// and the editor's caret reports another. The user sees a result panel that
/// scrolls to the wrong line, and "go to line 12" from a search result lands
/// somewhere else. A bare `\r` is not exotic: it is what a file written on a
/// classic Mac, or by a program that emitted a progress bar, contains.
///
/// The same split exists *inside* `bp-buffer`, between its two public answers
/// to "how many lines": the free `line_count` counts `'\n'`, and
/// `Buffer::len_lines` asks the rope. `bp-ui`'s status bar uses the first for
/// one editor view and the second for the other, so the same document reports
/// two different line counts depending on which view is showing it.
///
/// Fixing it is a decision, not a patch -- either `bp-search` adopts ropey's
/// line-break set, or `bp-buffer` builds its rope without `unicode_lines` --
/// so this is left named rather than papered over. Run with
/// `cargo test -p bp-integration-tests -- --ignored` to see it.
#[test]
#[ignore = "known defect: bp-search counts only \\n as a line break, bp-buffer's rope counts every Unicode line break"]
fn search_and_the_buffer_agree_on_lines_containing_any_line_break() {
    for (text, breaks) in [
        ("one\rtwo\rneedle", "a bare carriage return"),
        ("one\u{2028}two\u{2028}needle", "U+2028 LINE SEPARATOR"),
        ("one\u{0085}two\u{0085}needle", "U+0085 NEXT LINE"),
        ("one\x0Btwo\x0Bneedle", "a vertical tab"),
    ] {
        let buffer = Buffer::from_text(text);
        let m = &find_all(text, &Query::literal("needle")).expect("find")[0];

        assert_eq!(
            m.line,
            buffer.position_of(m.range.start).line,
            "search and the buffer disagree about a document separated by {breaks}"
        );
        assert_eq!(
            bp_buffer::line_count(text),
            buffer.len_lines(),
            "bp-buffer's own two line counts disagree about {breaks}"
        );
    }
}
