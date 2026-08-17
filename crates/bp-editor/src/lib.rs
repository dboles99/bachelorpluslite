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

pub mod keys;
pub mod lines;
pub mod view;

pub use keys::{Command, Key, Modifiers};

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

/// Where to move the caret. Expressed as intent, so the view can say "page
/// down" without the editor needing to know how tall the window is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    Up,
    Down,
    WordLeft,
    WordRight,
    LineStart,
    LineEnd,
    DocumentStart,
    DocumentEnd,
    /// A screenful, in lines. The view knows how many; the editor does not.
    PageUp(usize),
    PageDown(usize),
}

/// What kind of character this is, for word-wise movement.
///
/// Three classes rather than two: moving across `foo.bar` should stop at the
/// dot, which is what every editor does and what two classes cannot express.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    Whitespace,
    Word,
    Punctuation,
}

fn class(ch: char) -> Class {
    if ch.is_whitespace() {
        Class::Whitespace
    } else if ch.is_alphanumeric() || ch == '_' {
        Class::Word
    } else {
        Class::Punctuation
    }
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
    /// Column to aim for while moving vertically.
    ///
    /// Without it, going down through a short line and back up lands in the
    /// wrong place: the caret would take the short line's column with it and
    /// never recover the one the user started from.
    goal_column: Option<usize>,
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
            goal_column: None,
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

    /// Put the caret at the start of a 1-based line number, for the "go to
    /// line" command.
    ///
    /// Just `go_to_position` with column 1: the column clamp there already
    /// gives the right answer for "the start of the line" once column is 1,
    /// so there is no separate arithmetic here to keep in step with it.
    pub fn go_to_line(&mut self, line: usize) -> bool {
        self.go_to_position(Position::new(line, 1))
    }

    /// Put the caret at a 1-based line and column, for the "go to line"
    /// command.
    ///
    /// Both are 1-based, matching what the status bar shows and what a user
    /// types into a "go to" box. Zero is treated the same as one rather than
    /// rejected: a user who types 0, or a caller passing through some
    /// uninitialised default, means "the start", not "an error" -- and since
    /// every document has at least one line and every line has at least
    /// column 1, that request is always satisfiable, which is why it always
    /// counts as in range rather than being reported as a clamp.
    ///
    /// A line or column past the end of the document clamps rather than
    /// refusing: typing 9999 into a 42-line document means "the end", not
    /// "do nothing". The column clamps to the line's own length in
    /// characters, excluding its line break -- which is also what keeps the
    /// caret out of a CRLF pair, since the clamp never reaches as far as the
    /// break in the first place. `set_cursor` is still what assigns the
    /// caret rather than the field directly, so that guarantee holds even if
    /// this arithmetic is ever wrong.
    ///
    /// Returns whether the request was fully in range, so a caller can still
    /// say "there are only 42 lines" while the caret has already moved
    /// somewhere sensible.
    pub fn go_to_position(&mut self, position: Position) -> bool {
        let last_line = self.buffer.len_lines();
        let requested_line = position.line.max(1);
        let line_in_range = requested_line <= last_line;
        let line_idx = requested_line.min(last_line) - 1;

        let line_len = self.buffer.line_len_chars(line_idx);
        let requested_column = position.column.max(1);
        let column_in_range = requested_column - 1 <= line_len;
        let column_offset = (requested_column - 1).min(line_len);

        self.set_cursor(self.buffer.line_start(line_idx) + column_offset);
        line_in_range && column_in_range
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
        let idx = self.caret_position(char_idx);
        self.cursor = idx;
        self.anchor = idx;
        self.coalesce = Coalesce::Closed;
        self.goal_column = None;
    }

    /// Set caret and selection anchor together.
    pub fn select(&mut self, anchor: usize, cursor: usize) {
        self.anchor = self.caret_position(anchor);
        self.cursor = self.caret_position(cursor);
        self.coalesce = Coalesce::Closed;
        self.goal_column = None;
    }

    /// The nearest index a caret may legitimately occupy.
    ///
    /// Clamps past the end, as everything reached from UI code here does, and
    /// additionally refuses to sit *between* the `\r` and the `\n` of a CRLF
    /// pair. That position looks harmless and is not: the two characters are
    /// one line break, so a caret inside them belongs to no line, and any
    /// operation that reasons in whole lines -- moving one, duplicating one,
    /// selecting one -- then works from a line boundary that is half a
    /// character out. Moving a line from there used to invent a blank line at
    /// the top of the document.
    ///
    /// It snaps backwards, to just before the `\r`, because that is the end
    /// of the line's content -- the same place `Motion::LineEnd` stops and
    /// the same convention `line_len_chars` uses.
    fn caret_position(&self, char_idx: usize) -> usize {
        let idx = char_idx.min(self.buffer.len_chars());
        let splits_a_pair = idx > 0
            && self.buffer.char_at(idx) == Some('\n')
            && self.buffer.char_at(idx - 1) == Some('\r');
        if splits_a_pair { idx - 1 } else { idx }
    }

    pub fn select_all(&mut self) {
        self.select(0, self.buffer.len_chars());
    }

    /// Select the run of same-class characters around `char_idx`, for a
    /// double-click.
    ///
    /// Whitespace is a class like any other here: double-clicking the gap
    /// between two words selects the gap, rather than falling through to
    /// one of the words on either side of it.
    pub fn select_word_at(&mut self, char_idx: usize) {
        let idx = char_idx.min(self.buffer.len_chars());
        // At the very end of the document there is no character *at* idx to
        // classify, so fall back to the one just behind it.
        let kind = self
            .buffer
            .char_at(idx)
            .or_else(|| idx.checked_sub(1).and_then(|i| self.buffer.char_at(i)))
            .map(class);
        let Some(kind) = kind else {
            // Nothing to select: an empty document.
            self.set_cursor(idx);
            return;
        };

        let mut start = idx;
        while start > 0
            && self
                .buffer
                .char_at(start - 1)
                .is_some_and(|ch| class(ch) == kind)
        {
            start -= 1;
        }
        let mut end = idx;
        let len = self.buffer.len_chars();
        while end < len && self.buffer.char_at(end).is_some_and(|ch| class(ch) == kind) {
            end += 1;
        }
        self.select(start, end);
    }

    /// Select a whole line, including its trailing newline if it has one,
    /// for a triple-click.
    ///
    /// The newline is part of the selection so that typing over it replaces
    /// the line cleanly rather than leaving an empty one behind.
    pub fn select_line_at(&mut self, char_idx: usize) {
        let idx = char_idx.min(self.buffer.len_chars());
        let line = self.buffer.position_of(idx).line - 1;
        let start = self.buffer.line_start(line);
        let content_end = start + self.buffer.line_len_chars(line);
        let end = content_end + self.line_break_len(content_end);
        self.select(start, end);
    }

    /// Move the caret, extending the selection when `select` is set.
    ///
    /// An unshifted Left or Right with a selection collapses to its near
    /// edge rather than stepping from the caret. That is what every editor
    /// does, and the alternative loses a character off the end of the
    /// selection every time.
    pub fn move_caret(&mut self, motion: Motion, select: bool) {
        let vertical = matches!(
            motion,
            Motion::Up | Motion::Down | Motion::PageUp(_) | Motion::PageDown(_)
        );

        let target = match (self.selection(), select, motion) {
            (Some(range), false, Motion::Left) => range.start,
            (Some(range), false, Motion::Right) => range.end,
            _ => self.target_of(motion),
        };

        // Every motion above already produces a legitimate position; this is
        // the one place they all pass through, so it is also the cheapest
        // place to be sure of it.
        let target = self.caret_position(target);
        self.cursor = target;
        if !select {
            self.anchor = target;
        }
        // Any deliberate movement ends a typing run: text typed, then typed
        // again somewhere else, is two separate things to undo.
        self.coalesce = Coalesce::Closed;
        if !vertical {
            self.goal_column = None;
        }
    }

    /// Where `motion` lands, as a character index.
    fn target_of(&mut self, motion: Motion) -> usize {
        let end = self.buffer.len_chars();
        match motion {
            // A CRLF pair is one line break and therefore one step, in
            // either direction. Treating it as two puts the caret between the
            // carriage return and the newline, which is a position on no
            // line at all.
            Motion::Left => match self.cursor.checked_sub(2) {
                Some(before)
                    if self.buffer.char_at(before) == Some('\r')
                        && self.buffer.char_at(before + 1) == Some('\n') =>
                {
                    before
                }
                _ => self.cursor.saturating_sub(1),
            },
            Motion::Right => {
                let step = usize::from(
                    self.buffer.char_at(self.cursor) == Some('\r')
                        && self.buffer.char_at(self.cursor + 1) == Some('\n'),
                ) + 1;
                (self.cursor + step).min(end)
            }
            Motion::WordLeft => self.word_boundary_before(self.cursor),
            Motion::WordRight => self.word_boundary_after(self.cursor),
            Motion::LineStart => {
                let position = self.buffer.position_of(self.cursor);
                self.buffer.line_start(position.line - 1)
            }
            Motion::LineEnd => {
                let line = self.buffer.position_of(self.cursor).line - 1;
                self.buffer.line_start(line) + self.buffer.line_len_chars(line)
            }
            Motion::DocumentStart => 0,
            Motion::DocumentEnd => end,
            Motion::Up => self.vertical(-1),
            Motion::Down => self.vertical(1),
            Motion::PageUp(rows) => self.vertical(-isize::try_from(rows).unwrap_or(1)),
            Motion::PageDown(rows) => self.vertical(isize::try_from(rows).unwrap_or(1)),
        }
    }

    /// Move `delta` lines, keeping the column the user is aiming for.
    fn vertical(&mut self, delta: isize) -> usize {
        let position = self.buffer.position_of(self.cursor);
        let line = position.line - 1;
        // Remember the column on the first vertical move of a run, so passing
        // through a short line does not truncate it permanently.
        let goal = *self.goal_column.get_or_insert(position.column - 1);

        let last = self.buffer.len_lines().saturating_sub(1);
        let target_line = line.saturating_add_signed(delta).min(last);

        self.buffer.line_start(target_line) + goal.min(self.buffer.line_len_chars(target_line))
    }

    /// The start of the word before `from`, as Ctrl+Left means it.
    ///
    /// Skips any whitespace immediately behind the caret, then the whole run
    /// of whatever class it lands in.
    fn word_boundary_before(&self, from: usize) -> usize {
        let mut index = from;
        while index > 0
            && self
                .buffer
                .char_at(index - 1)
                .is_some_and(char::is_whitespace)
        {
            index -= 1;
        }
        let Some(kind) = index
            .checked_sub(1)
            .and_then(|i| self.buffer.char_at(i))
            .map(class)
        else {
            return index;
        };
        while index > 0
            && self
                .buffer
                .char_at(index - 1)
                .is_some_and(|ch| class(ch) == kind)
        {
            index -= 1;
        }
        index
    }

    /// The start of the word after `from`, as Ctrl+Right means it.
    fn word_boundary_after(&self, from: usize) -> usize {
        let end = self.buffer.len_chars();
        let mut index = from;
        if let Some(kind) = self.buffer.char_at(index).map(class) {
            while index < end
                && self
                    .buffer
                    .char_at(index)
                    .is_some_and(|ch| class(ch) == kind)
            {
                index += 1;
            }
        }
        while index < end && self.buffer.char_at(index).is_some_and(char::is_whitespace) {
            index += 1;
        }
        index
    }

    /// Delete the word before the caret, as Ctrl+Backspace does.
    pub fn delete_word_backward(&mut self) {
        if self.selection().is_some() {
            self.delete_selection();
            return;
        }
        let start = self.word_boundary_before(self.cursor);
        if start < self.cursor {
            self.select(start, self.cursor);
            self.delete_selection();
        }
    }

    /// Delete the word after the caret, as Ctrl+Delete does.
    pub fn delete_word_forward(&mut self) {
        if self.selection().is_some() {
            self.delete_selection();
            return;
        }
        let end = self.word_boundary_after(self.cursor);
        if end > self.cursor {
            self.select(self.cursor, end);
            self.delete_selection();
        }
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

    /// Delete the selection, if there is one.
    pub fn delete_selection(&mut self) {
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

    /// Duplicate the caret's line, or every line a selection touches, into a
    /// copy placed directly below.
    ///
    /// The caret's new position is its old offset from the start of the
    /// span, carried across unchanged. That is what lands it on the same
    /// line and column of the copy without recomputing anything, even when
    /// the span is several lines and the caret was not on the first one.
    pub fn duplicate_line(&mut self) {
        let cursor_before = self.cursor;
        let (first, last) = self.line_span();
        let span_start = self.buffer.line_start(first);
        let content_end = self.buffer.line_start(last) + self.buffer.line_len_chars(last);
        let break_len = self.line_break_len(content_end);
        let offset = self.cursor - span_start;

        let (at, text) = if break_len > 0 {
            (
                content_end + break_len,
                self.buffer.slice(span_start..content_end + break_len),
            )
        } else {
            // The span's last line has no break of its own: it is the final
            // line of the document, so the copy needs one to land on a line
            // of its own instead of running on from the original.
            let mut text = String::from("\n");
            text.push_str(&self.buffer.slice(span_start..content_end));
            (content_end, text)
        };
        let lead_in = if break_len > 0 { 0 } else { 1 };

        self.buffer.insert(at, &text);
        self.cursor = at + lead_in + offset;
        self.anchor = self.cursor;
        self.commit(vec![Op::Insert { at, text }], cursor_before, false);
    }

    /// Swap the caret's line with the one above, as every editor's Alt+Up
    /// does. Does nothing at the first line, since there is nothing above it
    /// to swap with.
    pub fn move_line_up(&mut self) {
        let line = self.buffer.position_of(self.cursor).line - 1;
        if line == 0 {
            return;
        }
        self.swap_lines(line - 1, line);
    }

    /// Swap the caret's line with the one below. Does nothing at the last
    /// line -- including the phantom empty line after a trailing newline,
    /// which is as much a line as any other for caret purposes elsewhere in
    /// this file, and swapping into it is what turns "the last real line"
    /// into a normal, undoable move rather than a permanent no-op.
    pub fn move_line_down(&mut self) {
        let line = self.buffer.position_of(self.cursor).line - 1;
        if line + 1 >= self.buffer.len_lines() {
            return;
        }
        self.swap_lines(line, line + 1);
    }

    /// Swap two adjacent lines, keeping the caret on its own text at the
    /// same column.
    ///
    /// Takes the pair rather than a direction because "up" and "down" are
    /// the same operation once the two line indices are known -- the caret
    /// is always in `first` or `second`, whichever call it came from.
    ///
    /// Each line's break travels with its own *slot*, not with the text
    /// that started in it. Gluing the two raw line slices together instead
    /// would carry `first`'s break along with `first`'s text to the new
    /// position, but `second`'s slot might have none (it is the end of the
    /// document) or a different one (mixed CRLF and LF) -- either way that
    /// merges two lines into one or invents a break that was never there.
    fn swap_lines(&mut self, first: usize, second: usize) {
        let cursor_before = self.cursor;
        let start = self.buffer.line_start(first);
        let first_content_end = start + self.buffer.line_len_chars(first);
        let first_break_len = self.line_break_len(first_content_end);
        let middle = first_content_end + first_break_len;
        let second_content_end = middle + self.buffer.line_len_chars(second);
        let second_break_len = self.line_break_len(second_content_end);
        let end = second_content_end + second_break_len;

        let first_content = self.buffer.slice(start..first_content_end);
        let first_break = self.buffer.slice(first_content_end..middle);
        let second_content = self.buffer.slice(middle..second_content_end);
        let second_break = self.buffer.slice(second_content_end..end);

        let mut swapped = second_content.clone();
        swapped.push_str(&first_break);
        swapped.push_str(&first_content);
        swapped.push_str(&second_break);

        // Every piece keeps its own length; only their order changes. Find
        // which piece the caret was in, then place it at that piece's new
        // start plus however far into the piece it was.
        let second_content_len = second_content.chars().count();
        let first_break_len_chars = first_break.chars().count();
        let first_content_len = first_content.chars().count();
        let new_first_content_start = start + second_content_len + first_break_len_chars;
        let new_first_break_start = start + second_content_len;
        let new_second_break_start = new_first_content_start + first_content_len;

        let new_cursor = if self.cursor < middle {
            if self.cursor <= first_content_end {
                new_first_content_start + (self.cursor - start)
            } else {
                new_first_break_start + (self.cursor - first_content_end)
            }
        } else if self.cursor <= second_content_end {
            start + (self.cursor - middle)
        } else {
            new_second_break_start + (self.cursor - second_content_end)
        };

        let removed = self.buffer.slice(start..end);
        self.buffer.remove(start..end);
        self.buffer.insert(start, &swapped);

        self.cursor = new_cursor;
        self.anchor = new_cursor;

        self.commit(
            vec![
                Op::Remove {
                    at: start,
                    text: removed,
                },
                Op::Insert {
                    at: start,
                    text: swapped,
                },
            ],
            cursor_before,
            false,
        );
    }

    /// The 0-based first and last line touched by the caret, or by a
    /// selection.
    ///
    /// A selection ending exactly at the start of a line has not reached
    /// into that line -- the newline it selected belongs to the line above
    /// -- so that line is excluded. Otherwise dragging to just before the
    /// next line would duplicate or affect a line the user never touched.
    fn line_span(&self) -> (usize, usize) {
        let (from, to) = match self.selection() {
            Some(range) => (range.start, range.end),
            None => (self.cursor, self.cursor),
        };
        let first = self.buffer.position_of(from).line - 1;
        let mut last = self.buffer.position_of(to).line - 1;
        if last > first && to == self.buffer.line_start(last) {
            last -= 1;
        }
        (first, last)
    }

    /// Length of the line break starting at `at`: 0, 1 for `\n`, or 2 for
    /// `\r\n`. Mirrors the pairing `Buffer::line_len_chars` strips, so the
    /// two never disagree about where a line actually ends.
    fn line_break_len(&self, at: usize) -> usize {
        match (self.buffer.char_at(at), self.buffer.char_at(at + 1)) {
            (Some('\r'), Some('\n')) => 2,
            (Some('\n'), _) => 1,
            _ => 0,
        }
    }

    /// Record a change, merging into the previous entry when it continues an
    /// uninterrupted run of typing at the caret.
    fn commit(&mut self, ops: Vec<Op>, cursor_before: usize, typing: bool) {
        // Any new edit invalidates the redo branch.
        self.redo.clear();
        // An edit re-establishes where the caret is; the column a vertical
        // run was aiming for no longer means anything.
        self.goal_column = None;

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

    /// Carry out a command, reporting whether the document changed.
    ///
    /// The clipboard commands are not here: the OS clipboard belongs to the
    /// shell, and an editor that reached for it would drag a platform
    /// dependency into a crate that is tested without one. They come back
    /// `false` and the shell handles them.
    pub fn apply(&mut self, command: &Command) -> bool {
        match command {
            Command::Insert(text) => {
                self.insert(text);
                true
            }
            Command::DeleteBackward => {
                self.delete_backward();
                true
            }
            Command::DeleteForward => {
                self.delete_forward();
                true
            }
            Command::DeleteWordBackward => {
                self.delete_word_backward();
                true
            }
            Command::DeleteWordForward => {
                self.delete_word_forward();
                true
            }
            Command::Undo => self.undo(),
            Command::Redo => self.redo(),
            Command::Move { motion, select } => {
                self.move_caret(*motion, *select);
                false
            }
            Command::SelectAll => {
                self.select_all();
                false
            }
            Command::Copy | Command::Cut | Command::Paste | Command::Ignore => false,
        }
    }

    /// Replace the whole document as a single undoable edit.
    ///
    /// What a data operation, Replace All or a line operation does: the
    /// document is rewritten wholesale, and undo must put back what was there
    /// in one press rather than unpicking it. Distinct from [`reset`], which
    /// is a *new* document and discards the history.
    ///
    /// Returns whether anything actually changed, so a no-op operation does
    /// not mark a clean document dirty.
    pub fn replace_all_text(&mut self, text: &str) -> bool {
        if self.buffer.len_chars() == text.chars().count() && self.text() == text {
            return false;
        }
        let cursor = self.cursor;
        self.select_all();
        if text.is_empty() {
            self.delete_selection();
        } else {
            self.insert(text);
        }
        // Keep the caret where it was where that still makes sense: formatting
        // a document should not scroll the user back to the top.
        self.set_cursor(cursor.min(self.buffer.len_chars()));
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
        self.goal_column = None;
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
    fn replacing_the_whole_document_is_one_undo_step() {
        // What Format JSON, Replace All and Sort Lines each do. Unpicking it
        // keystroke by keystroke would be useless.
        let mut e = Editor::new("one\ntwo\nthree");
        e.set_cursor(5);
        assert!(e.replace_all_text("ONE\nTWO\nTHREE"));

        assert_eq!(e.text(), "ONE\nTWO\nTHREE");
        assert_eq!(e.cursor(), 5, "the caret stays where the user left it");

        assert!(e.undo());
        assert_eq!(e.text(), "one\ntwo\nthree");
        assert!(!e.can_undo(), "one entry, not two");
    }

    #[test]
    fn replacing_a_document_with_itself_changes_nothing() {
        // Otherwise a data operation that found nothing to do would still
        // mark a saved document unsaved.
        let mut e = Editor::new("unchanged");
        assert!(!e.replace_all_text("unchanged"));
        assert!(!e.can_undo());
    }

    #[test]
    fn a_document_can_be_replaced_with_nothing() {
        let mut e = Editor::new("something");
        assert!(e.replace_all_text(""));
        assert_eq!(e.text(), "");
        assert_eq!(e.cursor(), 0);

        e.undo();
        assert_eq!(e.text(), "something");
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

    // --- caret movement -------------------------------------------------

    #[test]
    fn arrows_step_one_character_and_stop_at_the_ends() {
        let mut e = Editor::new("ab");
        e.move_caret(Motion::Left, false);
        assert_eq!(e.cursor(), 0, "already at the start");

        e.move_caret(Motion::Right, false);
        e.move_caret(Motion::Right, false);
        e.move_caret(Motion::Right, false);
        assert_eq!(e.cursor(), 2, "already at the end");
    }

    #[test]
    fn an_unshifted_arrow_collapses_a_selection_to_its_near_edge() {
        // Stepping from the caret instead would lose a character off the end
        // of the selection every time.
        let mut e = Editor::new("hello world");
        e.select(2, 7);
        e.move_caret(Motion::Left, false);
        assert_eq!(e.cursor(), 2);
        assert_eq!(e.selection(), None);

        e.select(2, 7);
        e.move_caret(Motion::Right, false);
        assert_eq!(e.cursor(), 7);
    }

    #[test]
    fn a_shifted_arrow_extends_from_the_anchor() {
        let mut e = Editor::new("hello");
        e.set_cursor(2);
        e.move_caret(Motion::Right, true);
        e.move_caret(Motion::Right, true);

        assert_eq!(e.selection(), Some(2..4));
    }

    #[test]
    fn vertical_movement_remembers_the_column_across_a_short_line() {
        // The classic defect: down through "x" then back up must return to
        // column 6, not to column 2.
        let mut e = Editor::new("long line\nx\nanother line");
        e.set_cursor(5);
        assert_eq!(e.position(), Position::new(1, 6));

        e.move_caret(Motion::Down, false);
        assert_eq!(
            e.position(),
            Position::new(2, 2),
            "clamped to the short line"
        );

        e.move_caret(Motion::Down, false);
        assert_eq!(
            e.position(),
            Position::new(3, 6),
            "the goal column survived"
        );
    }

    #[test]
    fn a_horizontal_move_forgets_the_goal_column() {
        let mut e = Editor::new("long line\nx\nanother line");
        e.set_cursor(5);
        e.move_caret(Motion::Down, false);
        e.move_caret(Motion::Left, false);
        e.move_caret(Motion::Down, false);

        assert_eq!(
            e.position(),
            Position::new(3, 1),
            "column 1 is where the caret actually was"
        );
    }

    #[test]
    fn vertical_movement_stops_at_the_first_and_last_line() {
        let mut e = Editor::new("one\ntwo");
        e.set_cursor(1);
        e.move_caret(Motion::Up, false);
        assert_eq!(e.position(), Position::new(1, 2), "no line above");

        e.move_caret(Motion::Down, false);
        e.move_caret(Motion::Down, false);
        assert_eq!(e.position(), Position::new(2, 2), "no line below");
    }

    #[test]
    fn home_and_end_work_on_the_caret_s_own_line() {
        let mut e = Editor::new("first\nsecond line");
        e.set_cursor(9);

        e.move_caret(Motion::LineStart, false);
        assert_eq!(e.cursor(), 6);
        e.move_caret(Motion::LineEnd, false);
        assert_eq!(e.cursor(), 17, "before the end of the buffer, not past it");
    }

    #[test]
    fn end_stops_before_the_line_break() {
        let mut e = Editor::new("ab\ncd");
        e.set_cursor(0);
        e.move_caret(Motion::LineEnd, false);
        assert_eq!(e.cursor(), 2, "on the newline would put it on line 2");
    }

    #[test]
    fn word_movement_stops_at_punctuation() {
        // Two classes would skip the dot and land past `bar`.
        let mut e = Editor::new("foo.bar baz");
        e.set_cursor(0);

        e.move_caret(Motion::WordRight, false);
        assert_eq!(e.cursor(), 3, "end of foo");
        e.move_caret(Motion::WordRight, false);
        assert_eq!(e.cursor(), 4, "past the dot");
        e.move_caret(Motion::WordRight, false);
        assert_eq!(e.cursor(), 8, "past bar and its trailing space");
    }

    #[test]
    fn word_movement_backwards_skips_trailing_whitespace_first() {
        let mut e = Editor::new("alpha beta");
        e.set_cursor(10);

        e.move_caret(Motion::WordLeft, false);
        assert_eq!(e.cursor(), 6, "start of beta");
        e.move_caret(Motion::WordLeft, false);
        assert_eq!(e.cursor(), 0, "start of alpha, over the space");
    }

    #[test]
    fn word_movement_terminates_at_the_document_edges() {
        let mut e = Editor::new("   ");
        e.set_cursor(3);
        e.move_caret(Motion::WordLeft, false);
        assert_eq!(e.cursor(), 0);

        e.move_caret(Motion::WordRight, false);
        assert_eq!(e.cursor(), 3);
    }

    #[test]
    fn a_page_is_as_many_lines_as_the_view_says() {
        let mut e = Editor::new("1\n2\n3\n4\n5\n6\n7\n8\n9\n10");
        e.set_cursor(0);

        e.move_caret(Motion::PageDown(4), false);
        assert_eq!(e.position().line, 5);
        e.move_caret(Motion::PageUp(2), false);
        assert_eq!(e.position().line, 3);

        e.move_caret(Motion::PageDown(500), false);
        assert_eq!(e.position().line, 10, "clamped to the last line");
    }

    #[test]
    fn document_start_and_end_go_to_the_edges() {
        let mut e = Editor::new("a\nb\nc");
        e.move_caret(Motion::DocumentEnd, false);
        assert_eq!(e.cursor(), 5);
        e.move_caret(Motion::DocumentStart, false);
        assert_eq!(e.cursor(), 0);
    }

    #[test]
    fn deleting_a_word_backwards_removes_one_word_per_press() {
        let mut e = Editor::new("alpha beta gamma");
        e.set_cursor(16);

        e.delete_word_backward();
        assert_eq!(e.text(), "alpha beta ");
        e.delete_word_backward();
        assert_eq!(e.text(), "alpha ");

        e.undo();
        assert_eq!(e.text(), "alpha beta ", "one press, one undo step");
    }

    #[test]
    fn deleting_a_word_forwards_removes_the_word_and_its_trailing_space() {
        let mut e = Editor::new("alpha beta");
        e.set_cursor(0);
        e.delete_word_forward();
        assert_eq!(e.text(), "beta");
    }

    #[test]
    fn deleting_a_word_with_a_selection_deletes_the_selection() {
        let mut e = Editor::new("alpha beta gamma");
        e.select(0, 5);
        e.delete_word_backward();
        assert_eq!(e.text(), " beta gamma");
    }

    #[test]
    fn deleting_a_word_at_the_edges_does_nothing() {
        let mut e = Editor::new("word");
        e.set_cursor(0);
        e.delete_word_backward();
        assert_eq!(e.text(), "word");
        assert!(!e.can_undo(), "a no-op must not create an undo entry");

        e.set_cursor(4);
        e.delete_word_forward();
        assert_eq!(e.text(), "word");
        assert!(!e.can_undo());
    }

    #[test]
    fn movement_is_by_character_in_multibyte_text() {
        let mut e = Editor::new("日本語");
        e.move_caret(Motion::Right, false);
        assert_eq!(e.cursor(), 1);
        assert_eq!(e.position(), Position::new(1, 2));

        e.move_caret(Motion::DocumentEnd, false);
        assert_eq!(e.cursor(), 3);
    }

    #[test]
    fn movement_in_an_empty_document_stays_put() {
        let mut e = Editor::new("");
        for motion in [
            Motion::Left,
            Motion::Right,
            Motion::Up,
            Motion::Down,
            Motion::WordLeft,
            Motion::WordRight,
            Motion::LineStart,
            Motion::LineEnd,
            Motion::DocumentStart,
            Motion::DocumentEnd,
            Motion::PageUp(10),
            Motion::PageDown(10),
        ] {
            e.move_caret(motion, false);
            assert_eq!(e.cursor(), 0, "{motion:?} moved in an empty document");
        }
    }

    // --- go to line -------------------------------------------------------

    #[test]
    fn go_to_line_moves_the_caret_to_an_ordinary_line() {
        let mut e = Editor::new("one\ntwo\nthree");
        assert!(e.go_to_line(2), "line 2 exists");
        assert_eq!(e.position(), Position::new(2, 1));
    }

    #[test]
    fn go_to_line_reaches_the_first_and_last_line() {
        let mut e = Editor::new("one\ntwo\nthree");
        assert!(e.go_to_line(3));
        assert_eq!(e.position(), Position::new(3, 1));

        assert!(e.go_to_line(1));
        assert_eq!(e.position(), Position::new(1, 1));
    }

    #[test]
    fn line_zero_means_the_first_line_not_an_error() {
        // A user who types 0 means "the start", the same as typing 1 -- so
        // it is treated as equivalent rather than refused.
        let mut e = Editor::new("one\ntwo");
        assert!(
            e.go_to_line(0),
            "0 is a valid way to ask for the first line"
        );
        assert_eq!(e.position(), Position::new(1, 1));
    }

    #[test]
    fn a_line_past_the_end_clamps_to_the_last_line_and_reports_out_of_range() {
        let mut e = Editor::new("one\ntwo\nthree");
        assert!(
            !e.go_to_line(9999),
            "asking for line 9999 of a 3-line document is out of range"
        );
        assert_eq!(e.position(), Position::new(3, 1), "still moved to the end");
    }

    #[test]
    fn go_to_line_in_an_empty_document_stays_on_the_only_line() {
        let mut e = Editor::new("");
        assert!(e.go_to_line(1));
        assert_eq!(e.position(), Position::new(1, 1));
        assert!(!e.go_to_line(2), "an empty document has only one line");
    }

    #[test]
    fn go_to_position_on_a_crlf_line_does_not_land_inside_the_break() {
        let mut e = Editor::new("one\r\ntwo\r\nthree");
        // Column 50 is far past "one"'s three characters: clamping must stop
        // before the \r, not walk into the break looking for more column to
        // give.
        assert!(
            !e.go_to_position(Position::new(1, 50)),
            "\"one\" has no column 50"
        );
        assert_eq!(e.cursor(), 3, "clamped to just after the e, before the \\r");

        let cursor = e.cursor();
        let splits_the_pair = e.buffer().char_at(cursor) == Some('\n')
            && cursor.checked_sub(1).and_then(|i| e.buffer().char_at(i)) == Some('\r');
        assert!(
            !splits_the_pair,
            "the caret must never sit between \\r and \\n"
        );
    }

    #[test]
    fn go_to_line_treats_the_phantom_final_line_as_a_real_line() {
        // A trailing newline creates an empty final line the caret can sit
        // on, as Buffer::len_lines documents -- so "go to the last line"
        // must reach it, not stop one short at the last line with visible
        // text on it.
        let mut e = Editor::new("one\ntwo\n");
        assert!(
            e.go_to_line(3),
            "the phantom line after the trailing newline"
        );
        assert_eq!(e.position(), Position::new(3, 1));
        assert_eq!(e.cursor(), e.buffer().len_chars());
    }

    #[test]
    fn go_to_position_counts_columns_by_character_not_byte() {
        let mut e = Editor::new("日本語\nabc");
        assert!(e.go_to_position(Position::new(1, 2)));
        assert_eq!(e.cursor(), 1, "between 日 and 本, by character offset");
    }

    #[test]
    fn go_to_position_clamps_a_column_past_the_end_of_the_line() {
        let mut e = Editor::new("hi\nlonger line");
        assert!(
            !e.go_to_position(Position::new(1, 100)),
            "\"hi\" has no column 100"
        );
        assert_eq!(
            e.position(),
            Position::new(1, 3),
            "clamped to just after the i"
        );
    }

    #[test]
    fn go_to_line_lands_on_the_requested_line_for_every_line_in_range() {
        // The check that actually pins the behaviour: whatever go_to_line
        // does internally, asking for line n and reading the line back must
        // always give n back, for every n a document actually has.
        let documents = [
            "one\ntwo\nthree",
            "one\ntwo\nthree\n",
            "solo",
            "",
            "a\r\nb\r\nc",
            "\n\n\n",
            "日本\nlatin\n語",
        ];
        for text in documents {
            let mut e = Editor::new(text);
            let last_line = e.buffer().len_lines();
            for n in 1..=last_line {
                assert!(e.go_to_line(n), "{text:?}: line {n} of {last_line}");
                assert_eq!(e.position().line, n, "{text:?}: go_to_line({n})");
            }
        }
    }

    // --- duplicate line --------------------------------------------------

    #[test]
    fn duplicating_a_line_copies_it_directly_below() {
        let mut e = Editor::new("one\ntwo\nthree");
        e.set_cursor(5); // column 2 of "two"
        e.duplicate_line();
        assert_eq!(e.text(), "one\ntwo\ntwo\nthree");
        assert_eq!(
            e.position(),
            Position::new(3, 2),
            "caret lands on the copy, same column"
        );
    }

    #[test]
    fn duplicating_a_line_is_one_undo_step() {
        let mut e = Editor::new("one\ntwo\nthree");
        e.set_cursor(5);
        e.duplicate_line();
        assert!(e.undo());
        assert_eq!(e.text(), "one\ntwo\nthree");
        assert!(!e.can_undo(), "one entry, not two");
    }

    #[test]
    fn duplicating_a_multi_line_selection_duplicates_the_whole_span() {
        // The selection only reaches partway into each line, but the whole
        // of both lines is what gets duplicated.
        let mut e = Editor::new("one\ntwo\nthree\nfour");
        e.select(5, 10);
        e.duplicate_line();
        assert_eq!(e.text(), "one\ntwo\nthree\ntwo\nthree\nfour");
        assert_eq!(
            e.position(),
            Position::new(5, 3),
            "caret follows the same character onto the copy"
        );
    }

    #[test]
    fn duplicating_a_selection_that_stops_at_the_next_lines_start_excludes_it() {
        // A selection ending exactly at the start of the next line has not
        // reached into it -- only the fully-selected line is duplicated.
        let mut e = Editor::new("one\ntwo\nthree");
        e.select(0, 4);
        e.duplicate_line();
        assert_eq!(e.text(), "one\none\ntwo\nthree");
    }

    #[test]
    fn duplicating_the_last_line_without_a_trailing_newline_adds_one() {
        let mut e = Editor::new("one\ntwo");
        e.set_cursor(5); // column 2 of "two"
        e.duplicate_line();
        assert_eq!(e.text(), "one\ntwo\ntwo");
        assert_eq!(e.position(), Position::new(3, 2), "caret on the new copy");
    }

    #[test]
    fn duplicating_a_line_with_a_crlf_ending_keeps_it_intact() {
        let mut e = Editor::new("one\r\ntwo\r\nthree");
        e.set_cursor(0);
        e.duplicate_line();
        assert_eq!(
            e.text(),
            "one\r\none\r\ntwo\r\nthree",
            "the break must not be split into a lone \\r or \\n"
        );
    }

    #[test]
    fn duplicating_the_only_line_in_a_single_line_document() {
        let mut e = Editor::new("solo");
        e.set_cursor(2);
        e.duplicate_line();
        assert_eq!(e.text(), "solo\nsolo");
        assert_eq!(e.position(), Position::new(2, 3));
    }

    #[test]
    fn duplicating_a_line_with_multibyte_text_keeps_the_caret_aligned() {
        let mut e = Editor::new("日本語\nabc");
        e.set_cursor(1); // between 日 and 本
        e.duplicate_line();
        assert_eq!(e.text(), "日本語\n日本語\nabc");
        assert_eq!(e.cursor(), 5, "same character offset into the copy");
    }

    // --- move line up / down ----------------------------------------------

    #[test]
    fn moving_a_line_up_swaps_it_with_the_line_above() {
        let mut e = Editor::new("one\ntwo\nthree");
        e.set_cursor(5); // column 2 of "two"
        e.move_line_up();
        assert_eq!(e.text(), "two\none\nthree");
        assert_eq!(
            e.position(),
            Position::new(1, 2),
            "the caret follows its line up, keeping its column"
        );
    }

    #[test]
    fn moving_a_line_down_swaps_it_with_the_line_below() {
        let mut e = Editor::new("one\ntwo\nthree");
        e.set_cursor(1); // column 2 of "one"
        e.move_line_down();
        assert_eq!(e.text(), "two\none\nthree");
        assert_eq!(e.position(), Position::new(2, 2));
    }

    #[test]
    fn moving_the_middle_line_of_three_swaps_with_its_neighbour() {
        let mut up = Editor::new("one\ntwo\nthree");
        up.set_cursor(5); // on "two"
        up.move_line_up();
        assert_eq!(up.text(), "two\none\nthree");

        let mut down = Editor::new("one\ntwo\nthree");
        down.set_cursor(5);
        down.move_line_down();
        assert_eq!(down.text(), "one\nthree\ntwo");
    }

    #[test]
    fn moving_a_line_up_at_the_top_does_nothing() {
        let mut e = Editor::new("one\ntwo");
        e.set_cursor(1);
        e.move_line_up();
        assert_eq!(e.text(), "one\ntwo");
        assert!(!e.can_undo(), "a no-op must not create an undo entry");
    }

    #[test]
    fn moving_a_line_down_at_the_bottom_does_nothing() {
        let mut e = Editor::new("one\ntwo");
        e.set_cursor(5); // on "two", the last line
        e.move_line_down();
        assert_eq!(e.text(), "one\ntwo");
        assert!(!e.can_undo());
    }

    #[test]
    fn moving_the_only_line_in_a_document_does_nothing_either_way() {
        let mut e = Editor::new("solo");
        e.set_cursor(2);
        e.move_line_up();
        e.move_line_down();
        assert_eq!(e.text(), "solo");
        assert!(!e.can_undo());
    }

    #[test]
    fn moving_the_last_real_line_down_swaps_with_the_trailing_empty_line() {
        // A trailing newline creates an empty final line the caret can sit
        // on, as `Buffer::len_lines` documents -- so it counts as a real
        // line to swap with, not a phantom that makes "two" already the
        // last line. The swap pushes "two" past it, so the document no
        // longer ends in a newline.
        let mut e = Editor::new("one\ntwo\n");
        e.set_cursor(5); // on "two"
        e.move_line_down();
        assert_eq!(e.text(), "one\n\ntwo");
    }

    #[test]
    fn moving_a_line_without_a_trailing_newline_does_not_invent_one() {
        // Gluing the two line slices together verbatim would carry the
        // first line's break along with its text and leave the last line
        // without one of its own. The swap must instead give each line
        // whichever break belongs to its new neighbour.
        let mut down = Editor::new("AAA\nBBB");
        down.set_cursor(1);
        down.move_line_down();
        assert_eq!(down.text(), "BBB\nAAA");

        let mut up = Editor::new("AAA\nBBB");
        up.set_cursor(5);
        up.move_line_up();
        assert_eq!(up.text(), "BBB\nAAA");
    }

    #[test]
    fn moving_a_line_preserves_crlf_line_endings_both_ways() {
        let mut down = Editor::new("AAA\r\nBBB\r\n");
        down.set_cursor(0);
        down.move_line_down();
        assert_eq!(down.text(), "BBB\r\nAAA\r\n");

        let mut up = Editor::new("AAA\r\nBBB\r\n");
        up.set_cursor(7); // on "BBB"
        up.move_line_up();
        assert_eq!(up.text(), "BBB\r\nAAA\r\n");
    }

    #[test]
    fn moving_a_line_preserves_crlf_when_the_last_line_has_no_break() {
        let mut e = Editor::new("AAA\r\nBBB");
        e.set_cursor(0);
        e.move_line_down();
        assert_eq!(e.text(), "BBB\r\nAAA");
    }

    #[test]
    fn moving_a_line_keeps_its_column_even_when_the_new_neighbour_is_shorter() {
        let mut e = Editor::new("a\nbcdef");
        e.set_cursor(5); // column 4 of "bcdef"
        e.move_line_up();
        assert_eq!(e.text(), "bcdef\na");
        assert_eq!(
            e.position(),
            Position::new(1, 4),
            "the caret's own column, not clamped to the line it swapped with"
        );
    }

    #[test]
    fn moving_a_line_with_multibyte_text_keeps_the_caret_aligned() {
        let mut e = Editor::new("日本語\nabc");
        e.set_cursor(1); // between 日 and 本
        e.move_line_down();
        assert_eq!(e.text(), "abc\n日本語");
        assert_eq!(e.cursor(), 5, "offset 1 into the moved line, by character");
    }

    #[test]
    fn moving_a_line_up_is_one_undo_step() {
        let mut e = Editor::new("one\ntwo\nthree");
        e.set_cursor(5);
        e.move_line_up();
        assert!(e.undo());
        assert_eq!(e.text(), "one\ntwo\nthree");
        assert!(!e.can_undo(), "one entry, not two");
    }

    #[test]
    fn moving_a_line_down_is_one_undo_step() {
        let mut e = Editor::new("one\ntwo\nthree");
        e.set_cursor(1);
        e.move_line_down();
        assert!(e.undo());
        assert_eq!(e.text(), "one\ntwo\nthree");
        assert!(!e.can_undo());
    }

    #[test]
    fn moving_a_line_down_then_back_up_restores_the_document_exactly() {
        // The strongest check available: whatever the swap does internally,
        // doing it and then undoing it by hand (not via `undo`, via the
        // inverse move) must reproduce the original bytes exactly.
        for text in [
            "one\ntwo\nthree",
            "one\ntwo\nthree\n",
            "AAA\nBBB",
            "AAA\r\nBBB\r\n",
            "AAA\r\nBBB",
            "just one line",
        ] {
            let mut e = Editor::new(text);
            e.set_cursor(0);
            e.move_line_down();
            e.move_line_up();
            assert_eq!(e.text(), text, "round trip corrupted {text:?}");
        }
    }

    // --- word and line selection -------------------------------------------

    #[test]
    fn double_clicking_a_word_selects_it() {
        let mut e = Editor::new("hello world");
        e.select_word_at(7); // inside "world"
        assert_eq!(e.selection(), Some(6..11));
    }

    #[test]
    fn double_clicking_whitespace_selects_the_whitespace_run() {
        let mut e = Editor::new("hello   world");
        e.select_word_at(6); // the middle of three spaces
        assert_eq!(e.selection(), Some(5..8));
    }

    #[test]
    fn double_clicking_punctuation_selects_the_punctuation_run() {
        let mut e = Editor::new("foo!!bar");
        e.select_word_at(4);
        assert_eq!(e.selection(), Some(3..5));
    }

    #[test]
    fn double_clicking_at_the_end_of_the_document_selects_the_last_word() {
        let mut e = Editor::new("hello world");
        e.select_word_at(11); // past the last character
        assert_eq!(e.selection(), Some(6..11));
    }

    #[test]
    fn double_clicking_an_empty_document_selects_nothing() {
        let mut e = Editor::new("");
        e.select_word_at(0);
        assert_eq!(e.selection(), None);
        assert_eq!(e.cursor(), 0);
    }

    #[test]
    fn triple_clicking_a_line_selects_it_including_the_newline() {
        let mut e = Editor::new("one\ntwo\nthree");
        e.select_line_at(5); // somewhere on "two"
        assert_eq!(e.selection(), Some(4..8));
        assert_eq!(e.buffer().slice(4..8), "two\n");
    }

    #[test]
    fn triple_clicking_the_last_line_without_a_trailing_newline_selects_just_the_text() {
        let mut e = Editor::new("one\ntwo\nthree");
        e.select_line_at(10); // somewhere on "three"
        assert_eq!(e.selection(), Some(8..13));
    }

    #[test]
    fn triple_clicking_a_crlf_line_includes_the_full_break() {
        let mut e = Editor::new("one\r\ntwo\r\nthree");
        e.select_line_at(0);
        assert_eq!(e.selection(), Some(0..5));
        assert_eq!(e.buffer().slice(0..5), "one\r\n");
    }
}
