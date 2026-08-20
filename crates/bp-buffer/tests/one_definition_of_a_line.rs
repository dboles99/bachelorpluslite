//! ADR-0029: the crate's two answers to "how many lines" must be one answer.
//!
//! `line_count` counts `'\n'` over a `&str`; `Buffer::len_lines` asks the
//! rope. They are the same question, and they disagreed for every document
//! containing a bare `\r`, `\x0B`, `\x0C`, U+0085, U+2028 or U+2029 --
//! because ropey's default features break on all of those and `line_count`
//! never has. `bp-ui`'s status bar reads the first under one editor view and
//! the second under the other, so one document reported two line counts
//! depending on which view was showing it.
//!
//! A property rather than examples, because an example-based test has to
//! guess which separator somebody's file uses, and the whole defect was about
//! the separators nobody thought to guess.

use bp_buffer::{Buffer, line_count};
use proptest::prelude::*;

/// The characters that used to be line breaks to the rope and never to
/// `line_count`, plus the two that legitimately are.
///
/// Written as escapes rather than as literals so that an editor, a diff or a
/// terminal cannot quietly normalise the fixture into something that would
/// pass for the wrong reason -- which is exactly the class of character these
/// are.
const SEPARATORS: &[&str] = &[
    "\n",       // a line break, and the only one this product writes
    "\r\n",     // a line break, written on Windows
    "\r",       // data, per bp-files: "a bare carriage return is data"
    "\u{000B}", // vertical tab
    "\u{000C}", // form feed
    "\u{0085}", // NEXT LINE
    "\u{2028}", // LINE SEPARATOR
    "\u{2029}", // PARAGRAPH SEPARATOR
];

/// Text built from ordinary words joined by an arbitrary mix of the
/// separators above.
fn text_with_mixed_separators() -> impl Strategy<Value = String> {
    prop::collection::vec(("[a-z]{0,6}", prop::sample::select(SEPARATORS)), 0..12).prop_map(
        |parts| {
            let mut out = String::new();
            for (word, separator) in parts {
                out.push_str(&word);
                out.push_str(separator);
            }
            out
        },
    )
}

proptest! {
    /// **The property ADR-0029 exists to make true.**
    #[test]
    fn the_two_line_counts_are_one_line_count(text in text_with_mixed_separators()) {
        let buffer = Buffer::from_text(&text);
        prop_assert_eq!(
            line_count(&text),
            buffer.len_lines(),
            "the crate's two answers disagree about {:?}",
            text
        );
    }

    /// And the position of every character agrees with the same count.
    ///
    /// A line total that matches while the *positions* do not would leave the
    /// gutter right and the caret wrong, which is the half a count cannot see.
    #[test]
    fn no_position_names_a_line_the_count_does_not_have(text in text_with_mixed_separators()) {
        let buffer = Buffer::from_text(&text);
        let lines = line_count(&text);
        for index in 0..=buffer.len_chars() {
            let position = buffer.position_of(index);
            prop_assert!(
                position.line >= 1 && position.line <= lines,
                "character {index} of {text:?} is on line {} of {lines}",
                position.line
            );
        }
    }
}

#[test]
fn the_separators_that_are_not_breaks_are_carried_as_data() {
    // The decision, stated as the behaviour a user meets. `bp-files` calls a
    // bare carriage return data; after ADR-0029 the buffer agrees, so a
    // document separated by one is a single line rather than several.
    for separator in [
        "\r", "\u{000B}", "\u{000C}", "\u{0085}", "\u{2028}", "\u{2029}",
    ] {
        let text = format!("one{separator}two{separator}three");
        let buffer = Buffer::from_text(&text);
        assert_eq!(
            buffer.len_lines(),
            1,
            "{separator:?} must not break a line -- ADR-0029"
        );
        assert_eq!(line_count(&text), 1, "{separator:?}");
    }
}

#[test]
fn the_two_separators_that_are_breaks_still_are() {
    for separator in ["\n", "\r\n"] {
        let text = format!("one{separator}two{separator}three");
        let buffer = Buffer::from_text(&text);
        assert_eq!(buffer.len_lines(), 3, "{separator:?} is a line break");
        assert_eq!(line_count(&text), 3, "{separator:?}");
    }
}
