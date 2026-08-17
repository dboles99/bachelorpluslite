//! Line operations, checked from outside the crate.
//!
//! These live here rather than in the unit tests on purpose. They were
//! written against the public API by someone reviewing the implementation
//! rather than writing it, and they found two defects the unit tests did not:
//! a line move that merged two lines on any document without a trailing
//! newline, and a caret sitting *between* the `\r` and `\n` of a CRLF pair,
//! from which moving a line invented a blank line at the top of the document.
//!
//! The involution property below is the reason both were caught. Moving a
//! line down and then up has to return the buffer to precisely its original
//! bytes, for every document shape and from every caret position -- which is
//! a far stronger claim than any number of worked examples, and much shorter
//! to write.

use bp_editor::{Editor, Motion};

/// The exact case that was broken: a document with no trailing newline.
#[test]
fn a_line_move_does_not_merge_lines() {
    let mut e = Editor::new("AAA\nBBB");
    e.set_cursor(0);
    e.move_line_down();
    assert_eq!(e.text(), "BBB\nAAA", "lines must not merge");

    let mut e = Editor::new("AAA\nBBB");
    e.set_cursor(4);
    e.move_line_up();
    assert_eq!(e.text(), "BBB\nAAA");
}

/// Moving a line down and back must return the exact original bytes.
#[test]
fn moving_a_line_is_an_involution() {
    let documents = [
        "AAA\nBBB",
        "AAA\nBBB\n",
        "AAA\r\nBBB",
        "AAA\r\nBBB\r\n",
        "one\ntwo\nthree",
        "one\ntwo\nthree\n",
        "\n\n",
        "日本\nlatin",
        "a\n\nb",
    ];

    let mut failures = Vec::new();
    for original in documents {
        for start in 0..original.chars().count() {
            let mut e = Editor::new(original);
            e.set_cursor(start);
            let line = e.position().line;
            if line >= e.buffer().len_lines() {
                continue;
            }
            e.move_line_down();
            e.move_line_up();
            if e.text() != original {
                failures.push(format!(
                    "{original:?} from offset {start} -> {:?}",
                    e.text()
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "not an involution:\n  {}",
        failures.join("\n  ")
    );
}

#[test]
fn duplicating_a_line_without_a_trailing_newline() {
    let mut e = Editor::new("only");
    e.set_cursor(2);
    e.duplicate_line();
    assert_eq!(e.text(), "only\nonly");

    let mut e = Editor::new("a\nb");
    e.set_cursor(0);
    e.duplicate_line();
    assert_eq!(e.text(), "a\na\nb");
}

#[test]
fn duplicating_preserves_crlf() {
    let mut e = Editor::new("a\r\nb\r\n");
    e.set_cursor(0);
    e.duplicate_line();
    assert_eq!(e.text(), "a\r\na\r\nb\r\n");
}

#[test]
fn every_line_operation_undoes_in_one_step() {
    for op in 0..3 {
        let original = "one\ntwo\nthree";
        let mut e = Editor::new(original);
        e.set_cursor(5);
        match op {
            0 => e.duplicate_line(),
            1 => e.move_line_up(),
            _ => e.move_line_down(),
        }
        assert_ne!(e.text(), original, "op {op} did nothing");
        assert!(e.undo(), "op {op} left nothing to undo");
        assert_eq!(e.text(), original, "op {op} did not undo cleanly");
        assert!(!e.can_undo(), "op {op} was more than one undo step");
    }
}

#[test]
fn no_ops_at_the_edges_leave_no_undo_entry() {
    let mut e = Editor::new("one\ntwo");
    e.set_cursor(0);
    e.move_line_up();
    assert_eq!(e.text(), "one\ntwo");
    assert!(!e.can_undo(), "a no-op created an undo entry");

    let mut e = Editor::new("one\ntwo");
    e.set_cursor(5);
    e.move_line_down();
    assert_eq!(e.text(), "one\ntwo");
    assert!(!e.can_undo());
}

#[test]
fn word_selection_covers_the_word_under_the_offset() {
    let mut e = Editor::new("alpha beta");
    e.select_word_at(7);
    assert_eq!(e.selection(), Some(6..10), "beta");

    e.select_word_at(5);
    assert_eq!(e.selection(), Some(5..6), "the space is its own run");

    // Must not panic at the very end of the document.
    e.select_word_at(10);
    e.select_word_at(999);
}

#[test]
fn the_caret_never_sits_inside_a_crlf_pair() {
    // Half a line break is a position on no line at all, and every operation
    // that reasons in whole lines then works from a boundary that is one
    // character out.
    let mut e = Editor::new("a\r\nb");
    e.set_cursor(2);
    assert_eq!(e.cursor(), 1, "snapped back to the end of the line");

    e.select(2, 2);
    assert_eq!(e.selection(), None);
    assert_eq!(e.cursor(), 1);
}

#[test]
fn arrows_step_over_a_crlf_pair_in_one_press() {
    // It is one line break, so it is one step -- in both directions.
    let mut e = Editor::new("a\r\nb");
    e.set_cursor(1);
    e.move_caret(Motion::Right, false);
    assert_eq!(e.cursor(), 3, "past the whole break, not into it");

    e.move_caret(Motion::Left, false);
    assert_eq!(e.cursor(), 1, "and back again");
}

#[test]
fn every_caret_position_in_a_crlf_document_is_reachable_and_stable() {
    let mut e = Editor::new("one\r\ntwo\r\nthree");
    e.set_cursor(0);

    let mut seen = vec![0];
    for _ in 0..40 {
        e.move_caret(Motion::Right, false);
        seen.push(e.cursor());
    }
    assert_eq!(*seen.last().unwrap(), e.buffer().len_chars());

    // Walking back must retrace exactly the same positions.
    let mut backwards = vec![e.cursor()];
    for _ in 0..40 {
        e.move_caret(Motion::Left, false);
        backwards.push(e.cursor());
    }
    backwards.reverse();
    backwards.dedup();
    let mut forwards = seen;
    forwards.dedup();
    assert_eq!(forwards, backwards, "the walk is not reversible");
}

#[test]
fn line_selection_takes_the_break_with_it() {
    let mut e = Editor::new("one\ntwo\n");
    e.select_line_at(1);
    assert_eq!(e.selection(), Some(0..4), "including the newline");

    let mut e = Editor::new("a\r\nb");
    e.select_line_at(0);
    assert_eq!(e.selection(), Some(0..3), "CRLF is two characters");
}
