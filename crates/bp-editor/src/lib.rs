//! Editing operations, caret and selection, undo and redo.
//!
//! This crate owns *what an edit is*, deliberately apart from any widget. It
//! has no UI dependency, so undo semantics are tested directly rather than by
//! driving a window.
//!
//! Undo works on transactions rather than keystrokes. A user who types
//! "hello" and presses undo expects "hello" to disappear, not "o" -- so a run
//! of ordinary typing coalesces into one entry, and anything that is not
//! ordinary typing ends the run.

#![forbid(unsafe_code)]

use std::ops::Range;

use bp_buffer::{Buffer, Position};

/// Crate identity used by workspace smoke tests and diagnostics.
pub const CRATE_NAME: &str = "bp-editor";

/// A single reversible change.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Op {
    Insert {
        at: usize,
        text: String,
    },
    /// Carries the removed text, which is what makes it invertible.
    Remove {
        at: usize,
        text: String,
    },
}

/// One undo step: a group of ops plus where the caret was on each side.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Transaction {
    ops: Vec<Op>,
    cursor_before: usize,
    cursor_after: usize,
}

/// Whether the newest undo entry may still absorb more typing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Coalesce {
    /// A run of ordinary typing is in progress.
    Typing,
    /// The run ended; the next edit starts a new undo entry.
    Closed,
}

/// A document being edited.
#[derive(Debug, Clone)]
pub struct Editor {
    buffer: Buffer,
    cursor: usize,
    /// The other end of the selection. Equal to `cursor` means no selection.
    anchor: usize,
    undo: Vec<Transaction>,
    redo: Vec<Transaction>,
    coalesce: Coalesce,
}

impl Default for Editor {
    fn default() -> Self {
        Self::new("")
    }
}

impl Editor {
    pub fn new(text: &str) -> Self {
        Self {
            buffer: Buffer::from_text(text),
            cursor: 0,
            anchor: 0,
            undo: Vec::new(),
            redo: Vec::new(),
            coalesce: Coalesce::Closed,
        }
    }

    pub fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    pub fn text(&self) -> String {
        self.buffer.to_string()
    }

    pub const fn cursor(&self) -> usize {
        self.cursor
    }

    /// The caret as a 1-based line and column, for the status bar.
    pub fn position(&self) -> Position {
        self.buffer.position_of(self.cursor)
    }

    /// The selected range, or `None` when the caret is a point.
    pub fn selection(&self) -> Option<Range<usize>> {
        if self.anchor == self.cursor {
            None
        } else if self.anchor < self.cursor {
            Some(self.anchor..self.cursor)
        } else {
            Some(self.cursor..self.anchor)
        }
    }

    /// Move the caret, clearing any selection.
    ///
    /// Ends a typing run: text typed, then typed again somewhere else, is two
    /// separate things to undo.
    pub fn set_cursor(&mut self, char_idx: usize) {
        let idx = char_idx.min(self.buffer.len_chars());
        self.cursor = idx;
        self.anchor = idx;
        self.coalesce = Coalesce::Closed;
    }

    /// Set caret and selection anchor together.
    pub fn select(&mut self, anchor: usize, cursor: usize) {
        let max = self.buffer.len_chars();
        self.anchor = anchor.min(max);
        self.cursor = cursor.min(max);
        self.coalesce = Coalesce::Closed;
    }

    pub fn select_all(&mut self) {
        self.select(0, self.buffer.len_chars());
    }

    /// Replace the current selection, if any, with `text`; otherwise insert
    /// at the caret.
    pub fn insert(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let mut ops = Vec::new();
        let cursor_before = self.cursor;

        if let Some(range) = self.selection() {
            let removed = self.buffer.slice(range.clone());
            self.buffer.remove(range.clone());
            ops.push(Op::Remove {
                at: range.start,
                text: removed,
            });
            self.cursor = range.start;
            self.anchor = range.start;
        }

        let at = self.cursor;
        self.buffer.insert(at, text);
        self.cursor = at + text.chars().count();
        self.anchor = self.cursor;
        ops.push(Op::Insert {
            at,
            text: text.to_owned(),
        });

        // A newline ends the run: undo should step back a line at a time,
        // not swallow a paragraph.
        let typing = ops.len() == 1 && !text.contains('\n');
        self.commit(ops, cursor_before, typing);
    }

    /// Backspace. Deletes the selection if there is one.
    pub fn delete_backward(&mut self) {
        if self.selection().is_some() {
            self.delete_selection();
            return;
        }
        if self.cursor == 0 {
            return;
        }
        let at = self.cursor - 1;
        let removed = self.buffer.slice(at..self.cursor);
        self.buffer.remove(at..self.cursor);
        let cursor_before = self.cursor;
        self.cursor = at;
        self.anchor = at;
        // Deletion is not typing: it starts its own undo entry.
        self.commit(vec![Op::Remove { at, text: removed }], cursor_before, false);
    }

    /// Delete forward. Deletes the selection if there is one.
    pub fn delete_forward(&mut self) {
        if self.selection().is_some() {
            self.delete_selection();
            return;
        }
        if self.cursor >= self.buffer.len_chars() {
            return;
        }
        let end = self.cursor + 1;
        let removed = self.buffer.slice(self.cursor..end);
        self.buffer.remove(self.cursor..end);
        self.commit(
            vec![Op::Remove {
                at: self.cursor,
                text: removed,
            }],
            self.cursor,
            false,
        );
    }

    fn delete_selection(&mut self) {
        let Some(range) = self.selection() else {
            return;
        };
        let cursor_before = self.cursor;
        let removed = self.buffer.slice(range.clone());
        self.buffer.remove(range.clone());
        self.cursor = range.start;
        self.anchor = range.start;
        self.commit(
            vec![Op::Remove {
                at: range.start,
                text: removed,
            }],
            cursor_before,
            false,
        );
    }

    /// Record a change, merging into the previous entry when it continues an
    /// uninterrupted run of typing at the caret.
    fn commit(&mut self, ops: Vec<Op>, cursor_before: usize, typing: bool) {
        // Any new edit invalidates the redo branch.
        self.redo.clear();

        // Only merge when this insert continues exactly where the previous one
        // stopped. Typing elsewhere is a separate edit even if nothing else
        // intervened.
        if typing
            && self.coalesce == Coalesce::Typing
            && let Some(last) = self.undo.last_mut()
            && let (
                Some(Op::Insert { at, text }),
                Op::Insert {
                    at: new_at,
                    text: new,
                },
            ) = (last.ops.last_mut(), &ops[0])
            && *at + text.chars().count() == *new_at
        {
            text.push_str(new);
            last.cursor_after = self.cursor;
            return;
        }

        self.undo.push(Transaction {
            ops,
            cursor_before,
            cursor_after: self.cursor,
        });
        self.coalesce = if typing {
            Coalesce::Typing
        } else {
            Coalesce::Closed
        };
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Undo one transaction. Returns `false` when there is nothing to undo.
    pub fn undo(&mut self) -> bool {
        let Some(transaction) = self.undo.pop() else {
            return false;
        };
        // Invert in reverse: later ops were applied on top of earlier ones.
        for op in transaction.ops.iter().rev() {
            match op {
                Op::Insert { at, text } => {
                    self.buffer.remove(*at..*at + text.chars().count());
                }
                Op::Remove { at, text } => self.buffer.insert(*at, text),
            }
        }
        self.cursor = transaction.cursor_before.min(self.buffer.len_chars());
        self.anchor = self.cursor;
        self.redo.push(transaction);
        self.coalesce = Coalesce::Closed;
        true
    }

    /// Redo one transaction. Returns `false` when there is nothing to redo.
    pub fn redo(&mut self) -> bool {
        let Some(transaction) = self.redo.pop() else {
            return false;
        };
        for op in &transaction.ops {
            match op {
                Op::Insert { at, text } => self.buffer.insert(*at, text),
                Op::Remove { at, text } => {
                    self.buffer.remove(*at..*at + text.chars().count());
                }
            }
        }
        self.cursor = transaction.cursor_after.min(self.buffer.len_chars());
        self.anchor = self.cursor;
        self.undo.push(transaction);
        self.coalesce = Coalesce::Closed;
        true
    }

    /// Replace the whole document, as loading a file does.
    ///
    /// Clears history: undoing past a file load into the previous document's
    /// content would be nonsense.
    pub fn reset(&mut self, text: &str) {
        self.buffer = Buffer::from_text(text);
        self.cursor = 0;
        self.anchor = 0;
        self.undo.clear();
        self.redo.clear();
        self.coalesce = Coalesce::Closed;
    }
}

#[cfg(test)]
impl Editor {
    /// Move the caret *without* ending the typing run, so tests can exercise
    /// the contiguity check in `commit` independently of `set_cursor`.
    fn move_caret_without_ending_run(&mut self, idx: usize) {
        self.cursor = idx;
        self.anchor = idx;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_moves_the_caret() {
        let mut e = Editor::new("");
        e.insert("hello");
        assert_eq!(e.text(), "hello");
        assert_eq!(e.cursor(), 5);
        assert_eq!(e.position(), Position::new(1, 6));
    }

    #[test]
    fn typing_a_word_undoes_as_one_step() {
        // The rule users actually expect: undo removes "hello", not "o".
        let mut e = Editor::new("");
        for ch in "hello".chars() {
            e.insert(&ch.to_string());
        }
        assert_eq!(e.text(), "hello");

        assert!(e.undo());
        assert_eq!(e.text(), "");
        assert!(!e.can_undo(), "one entry, not five");
    }

    #[test]
    fn a_newline_ends_the_typing_run() {
        let mut e = Editor::new("");
        e.insert("one");
        e.insert("\n");
        e.insert("two");

        e.undo();
        assert_eq!(e.text(), "one\n");
        e.undo();
        assert_eq!(e.text(), "one");
        e.undo();
        assert_eq!(e.text(), "");
    }

    #[test]
    fn moving_the_caret_ends_the_typing_run() {
        let mut e = Editor::new("");
        e.insert("abc");
        e.set_cursor(0);
        e.insert("X");

        assert_eq!(e.text(), "Xabc");
        e.undo();
        assert_eq!(e.text(), "abc", "the two runs are separate entries");
    }

    #[test]
    fn typing_elsewhere_does_not_merge_even_without_an_explicit_move() {
        // Guards the contiguity check in `commit`, not just the cursor-move
        // path that sets Coalesce::Closed.
        let mut e = Editor::new("abc");
        e.set_cursor(3);
        e.insert("d");
        e.move_caret_without_ending_run(0);
        e.insert("Z");

        assert_eq!(e.text(), "Zabcd");
        e.undo();
        assert_eq!(e.text(), "abcd", "non-contiguous inserts must not merge");
    }

    #[test]
    fn deleting_starts_a_new_entry() {
        let mut e = Editor::new("");
        e.insert("ab");
        e.delete_backward();
        assert_eq!(e.text(), "a");

        e.undo();
        assert_eq!(e.text(), "ab", "undo the delete");
        e.undo();
        assert_eq!(e.text(), "", "then undo the typing");
    }

    #[test]
    fn backspace_at_the_start_does_nothing() {
        let mut e = Editor::new("abc");
        e.set_cursor(0);
        e.delete_backward();
        assert_eq!(e.text(), "abc");
        assert!(!e.can_undo(), "a no-op must not create an undo entry");
    }

    #[test]
    fn delete_forward_at_the_end_does_nothing() {
        let mut e = Editor::new("abc");
        e.set_cursor(3);
        e.delete_forward();
        assert_eq!(e.text(), "abc");
        assert!(!e.can_undo());
    }

    #[test]
    fn typing_over_a_selection_is_one_undo_step() {
        let mut e = Editor::new("hello world");
        e.select(0, 5);
        e.insert("goodbye");
        assert_eq!(e.text(), "goodbye world");

        assert!(e.undo());
        assert_eq!(e.text(), "hello world", "both the delete and the insert");
        assert!(!e.can_undo());
    }

    #[test]
    fn deleting_a_selection_restores_it_exactly() {
        let mut e = Editor::new("hello world");
        e.select(6, 11);
        e.delete_backward();
        assert_eq!(e.text(), "hello ");

        e.undo();
        assert_eq!(e.text(), "hello world");
    }

    #[test]
    fn undo_restores_the_caret() {
        let mut e = Editor::new("abc");
        e.set_cursor(3);
        e.insert("def");
        assert_eq!(e.cursor(), 6);

        e.undo();
        assert_eq!(e.cursor(), 3, "back where the edit started");
    }

    #[test]
    fn redo_replays_the_edit_and_the_caret() {
        let mut e = Editor::new("");
        e.insert("hello");
        e.undo();
        assert!(e.can_redo());

        assert!(e.redo());
        assert_eq!(e.text(), "hello");
        assert_eq!(e.cursor(), 5);
        assert!(!e.can_redo());
    }

    #[test]
    fn a_new_edit_discards_the_redo_branch() {
        let mut e = Editor::new("");
        e.insert("hello");
        e.undo();
        assert!(e.can_redo());

        e.insert("x");
        assert!(!e.can_redo(), "the redo branch is gone once you diverge");
    }

    #[test]
    fn undo_and_redo_report_when_there_is_nothing_to_do() {
        let mut e = Editor::new("abc");
        assert!(!e.undo());
        assert!(!e.redo());
    }

    #[test]
    fn a_full_round_trip_returns_the_original_text() {
        let mut e = Editor::new("start");
        e.set_cursor(5);
        e.insert(" middle");
        e.insert("\n");
        e.insert("end");
        e.select(0, 5);
        e.delete_backward();
        let scrambled = e.text();

        while e.undo() {}
        assert_eq!(e.text(), "start");

        while e.redo() {}
        assert_eq!(e.text(), scrambled);
    }

    #[test]
    fn reset_clears_history() {
        let mut e = Editor::new("old");
        e.set_cursor(3);
        e.insert(" edit");
        e.reset("new document");

        assert_eq!(e.text(), "new document");
        assert_eq!(e.cursor(), 0);
        assert!(!e.can_undo(), "undoing past a file load is nonsense");
        assert!(!e.can_redo());
    }

    #[test]
    fn selection_is_direction_independent() {
        let mut e = Editor::new("hello");
        e.select(4, 1);
        assert_eq!(e.selection(), Some(1..4));
        e.select(1, 4);
        assert_eq!(e.selection(), Some(1..4));
        e.select(2, 2);
        assert_eq!(e.selection(), None);
    }

    #[test]
    fn multibyte_text_undoes_by_character_not_byte() {
        let mut e = Editor::new("");
        e.insert("日本語");
        assert_eq!(e.cursor(), 3);
        e.undo();
        assert_eq!(e.text(), "", "a byte-length undo would leave fragments");
    }

    #[test]
    fn inserting_nothing_is_a_no_op() {
        let mut e = Editor::new("abc");
        e.insert("");
        assert!(!e.can_undo());
    }
}
