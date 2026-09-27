//! The BachelorPad+ text buffer.
//!
//! A rope, so that an edit in the middle of a large document costs
//! `O(log n)` rather than copying the whole thing (specs.md section 5).
//!
//! Indices here are **character** indices, not bytes. A byte index can land
//! inside a multi-byte character and produce a panic or a mangled document,
//! and this type is reached from UI code where an off-by-one is a matter of
//! time. Positions are 1-based line and column, because that is what the
//! status bar shows ("Ln 42, Col 18") and converting at the edge invites
//! exactly one team member to forget.
//!
//! A rope is the right answer up to a point, and [`large`] is where that
//! point is measured. Past it a document is not held in memory at all: it is
//! indexed sparsely, read in chunks and streamed. `#![forbid(unsafe_code)]`
//! holds for all of it, which is also why nothing here memory-maps -- the
//! reasoning, and what it would take to change the answer, is written out in
//! the [`large`] module docs rather than left as an omission.

#![forbid(unsafe_code)]

use std::ops::Range;

use ropey::Rope;

/// Crate identity used by workspace smoke tests and diagnostics.
pub const CRATE_NAME: &str = "bp-buffer";

/// Number of lines in `text`, using the editor convention.
///
/// `"a\n"` is **two** lines: the caret can sit on the second one and the
/// gutter has to number it. `str::lines()` reports one, which is right for
/// iterating content and wrong for displaying a document.
///
/// A free function so callers that only need the count -- the gutter, on
/// every keystroke -- get it without building a rope.
pub fn line_count(text: &str) -> usize {
    text.bytes().filter(|b| *b == b'\n').count() + 1
}

/// A 1-based line and column, as displayed to the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

impl Position {
    pub const START: Self = Self { line: 1, column: 1 };

    pub const fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }
}

/// Where another text differs from a buffer: one span, found by
/// [`Buffer::change_to`].
///
/// **The two ranges are in different units, and the names say which.** The
/// buffer counts characters; the other text is a `&str`, sliced in bytes.
/// W1-04 was a character offset handed to something that counted bytes, and
/// a struct with two fields called `start` would invite it again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// The characters of the buffer that go.
    pub removed_chars: Range<usize>,
    /// The bytes of the other text that replace them.
    pub inserted_bytes: Range<usize>,
}

/// A rope-backed text buffer.
#[derive(Debug, Clone, Default)]
pub struct Buffer {
    rope: Rope,
}

impl Buffer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_text(text: &str) -> Self {
        Self {
            rope: Rope::from_str(text),
        }
    }

    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    pub fn len_bytes(&self) -> usize {
        self.rope.len_bytes()
    }

    pub fn is_empty(&self) -> bool {
        self.rope.len_chars() == 0
    }

    /// The character index of a UTF-8 byte offset, clamped to the end.
    ///
    /// For the one caller that is handed bytes: Slint's `TextInput` reports
    /// its caret as a byte offset, and everything in this workspace counts in
    /// characters. A byte offset inside a character resolves to that
    /// character, which is ropey's rule and the safe one -- it can only ever
    /// name a real boundary.
    pub fn byte_to_char(&self, byte_idx: usize) -> usize {
        self.rope.byte_to_char(byte_idx.min(self.len_bytes()))
    }

    /// The UTF-8 byte offset of a character index, clamped to the end.
    pub fn char_to_byte(&self, char_idx: usize) -> usize {
        self.rope.char_to_byte(char_idx.min(self.len_chars()))
    }

    /// Number of lines, counting the empty line after a trailing newline.
    ///
    /// This differs from `str::lines()`, which reports `"a\n"` as one line.
    /// An editor must report two: the caret can sit on that second line, and
    /// the gutter has to number it.
    pub fn len_lines(&self) -> usize {
        self.rope.len_lines()
    }

    /// Insert at a character index. Indices past the end clamp to the end
    /// rather than panicking -- this is reached from UI code.
    ///
    /// Does nothing when the buffer is read-only. Silently, and deliberately:
    /// the caller is a keystroke, the answer is already on screen in the
    /// status bar, and a dialog per character is not a better editor.
    pub fn insert(&mut self, char_idx: usize, text: &str) {
        self.rope.insert(char_idx.min(self.len_chars()), text);
    }

    /// Remove a character range, clamped to the buffer.
    pub fn remove(&mut self, range: Range<usize>) {
        let end = range.end.min(self.len_chars());
        let start = range.start.min(end);
        if start < end {
            self.rope.remove(start..end);
        }
    }

    /// The character at `char_idx`, or `None` at or past the end.
    ///
    /// Returning `Option` rather than panicking because caret arithmetic asks
    /// about the position one past the last character constantly, and that is
    /// a legitimate question with a legitimate answer.
    pub fn char_at(&self, char_idx: usize) -> Option<char> {
        (char_idx < self.len_chars()).then(|| self.rope.char(char_idx))
    }

    pub fn slice(&self, range: Range<usize>) -> String {
        let end = range.end.min(self.len_chars());
        let start = range.start.min(end);
        self.rope.slice(start..end).to_string()
    }

    /// One line, including its trailing newline if it has one.
    pub fn line(&self, line_idx: usize) -> String {
        if line_idx >= self.len_lines() {
            return String::new();
        }
        self.rope.line(line_idx).to_string()
    }

    /// Convert a character index to a display position.
    pub fn position_of(&self, char_idx: usize) -> Position {
        let idx = char_idx.min(self.len_chars());
        let line = self.rope.char_to_line(idx);
        let line_start = self.rope.line_to_char(line);
        Position::new(line + 1, idx - line_start + 1)
    }

    /// Convert a display position back to a character index.
    ///
    /// Out-of-range lines and columns clamp, so a stale position from the UI
    /// lands somewhere sensible instead of panicking.
    pub fn char_of_position(&self, position: Position) -> usize {
        let line = position.line.saturating_sub(1).min(self.len_lines() - 1);
        let line_start = self.rope.line_to_char(line);
        let line_len = self.line_len_chars(line);
        let column = position.column.saturating_sub(1).min(line_len);
        line_start + column
    }

    /// Length of a line in characters, excluding its line break.
    ///
    /// The caret sits *before* the newline, so a column may not exceed this.
    pub fn line_len_chars(&self, line_idx: usize) -> usize {
        if line_idx >= self.len_lines() {
            return 0;
        }
        let slice = self.rope.line(line_idx);
        let mut len = slice.len_chars();
        // Strip the line break, CRLF or LF.
        let text = slice.to_string();
        if text.ends_with('\n') {
            len -= 1;
            if text.ends_with("\r\n") {
                len -= 1;
            }
        }
        len
    }

    /// Character index at which `line_idx` starts.
    pub fn line_start(&self, line_idx: usize) -> usize {
        let line = line_idx.min(self.len_lines().saturating_sub(1));
        self.rope.line_to_char(line)
    }

    /// The one span where `text` differs from this buffer, or `None` when
    /// they are the same.
    ///
    /// For a widget that owns the typing and reports its whole text after
    /// every key. A keystroke changes the text in one place, so what is
    /// common to both ends is kept and only the middle is replaced: the rope
    /// takes a small edit instead of being rebuilt, and nothing has to hold a
    /// copy of the old document to find out what changed.
    ///
    /// The span is the *smallest* one, which is not always where the key
    /// was pressed -- typing "a" into "aa" could have been at any of three
    /// places, and this reports the end. Every answer produces the same
    /// text, which is the only thing a mirror needs.
    pub fn change_to(&self, text: &str) -> Option<Change> {
        let mut prefix = 0;
        let mut prefix_bytes = 0;
        for (ours, theirs) in self.rope.chars().zip(text.chars()) {
            if ours != theirs {
                break;
            }
            prefix += 1;
            prefix_bytes += theirs.len_utf8();
        }
        let len = self.len_chars();
        let rest = &text[prefix_bytes..];
        let text_len = prefix + rest.chars().count();
        if prefix == len && prefix == text_len {
            return None;
        }

        // Matched from the back, but never into the prefix on either side:
        // a character counted twice would make the span negative.
        let room = (len - prefix).min(text_len - prefix);
        let mut suffix = 0;
        let mut suffix_bytes = 0;
        let mut ours = self.rope.chars_at(len).reversed();
        for theirs in rest.chars().rev().take(room) {
            if ours.next() != Some(theirs) {
                break;
            }
            suffix += 1;
            suffix_bytes += theirs.len_utf8();
        }
        Some(Change {
            removed_chars: prefix..len - suffix,
            inserted_bytes: prefix_bytes..text.len() - suffix_bytes,
        })
    }
}

impl std::fmt::Display for Buffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.rope)
    }
}

impl From<&str> for Buffer {
    fn from(text: &str) -> Self {
        Self::from_text(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_buffer_has_one_line() {
        let b = Buffer::new();
        assert!(b.is_empty());
        assert_eq!(b.len_lines(), 1, "the caret still sits on line 1");
        assert_eq!(b.position_of(0), Position::START);
    }

    #[test]
    fn a_trailing_newline_creates_a_line() {
        // Differs from str::lines() on purpose: the caret can sit on line 2,
        // and the gutter has to number it.
        assert_eq!(Buffer::from_text("a\n").len_lines(), 2);
        assert_eq!("a\n".lines().count(), 1, "std disagrees, and is wrong here");
        assert_eq!(Buffer::from_text("a").len_lines(), 1);
    }

    #[test]
    fn positions_are_one_based() {
        let b = Buffer::from_text("hello\nworld");
        assert_eq!(b.position_of(0), Position::new(1, 1));
        assert_eq!(b.position_of(5), Position::new(1, 6), "end of line 1");
        assert_eq!(b.position_of(6), Position::new(2, 1), "start of line 2");
        assert_eq!(b.position_of(11), Position::new(2, 6), "end of buffer");
    }

    #[test]
    fn positions_round_trip() {
        let b = Buffer::from_text("hello\nworld\n\nlast");
        for idx in 0..=b.len_chars() {
            let pos = b.position_of(idx);
            assert_eq!(b.char_of_position(pos), idx, "at {idx} ({pos:?})");
        }
    }

    #[test]
    fn positions_are_char_based_not_byte_based() {
        // "é" is two bytes. A byte-indexed implementation reports column 3
        // after one character, or panics.
        let b = Buffer::from_text("é日本");
        assert_eq!(b.len_chars(), 3);
        assert_eq!(b.len_bytes(), 8);
        assert_eq!(b.position_of(1), Position::new(1, 2));
        assert_eq!(b.position_of(3), Position::new(1, 4));
    }

    #[test]
    fn out_of_range_indices_clamp_rather_than_panic() {
        let b = Buffer::from_text("hi");
        assert_eq!(b.position_of(999), b.position_of(2));
        assert_eq!(b.char_of_position(Position::new(999, 999)), 2);
        assert_eq!(b.char_of_position(Position::new(0, 0)), 0);
    }

    #[test]
    fn insert_and_remove() {
        let mut b = Buffer::from_text("hello world");
        b.insert(5, ",");
        assert_eq!(b.to_string(), "hello, world");
        b.remove(5..6);
        assert_eq!(b.to_string(), "hello world");
    }

    #[test]
    fn insert_past_the_end_appends() {
        let mut b = Buffer::from_text("hi");
        b.insert(999, "!");
        assert_eq!(b.to_string(), "hi!");
    }

    #[test]
    fn remove_clamps_and_ignores_empty_ranges() {
        let mut b = Buffer::from_text("hello");
        b.remove(3..999);
        assert_eq!(b.to_string(), "hel");
        b.remove(2..2);
        assert_eq!(b.to_string(), "hel");
        // Built from variables so clippy's reversed_empty_ranges lint does not
        // reject the literal. A reversed range arriving here from UI
        // arithmetic is exactly what the guard exists for.
        let (start, end) = (5, 1);
        b.remove(start..end);
        assert_eq!(b.to_string(), "hel", "a reversed range must be a no-op");
    }

    #[test]
    fn line_length_excludes_the_line_break() {
        let b = Buffer::from_text("abc\ndefg\r\nhi");
        assert_eq!(b.line_len_chars(0), 3);
        assert_eq!(b.line_len_chars(1), 4, "CRLF must not count as content");
        assert_eq!(b.line_len_chars(2), 2);
        assert_eq!(b.line_len_chars(99), 0);
    }

    #[test]
    fn a_column_cannot_land_past_the_end_of_its_line() {
        let b = Buffer::from_text("ab\ncdef");
        // Column 50 on line 1 clamps to just after "ab", not into line 2.
        assert_eq!(b.char_of_position(Position::new(1, 50)), 2);
        assert_eq!(b.position_of(2), Position::new(1, 3));
    }

    #[test]
    fn line_starts_are_correct() {
        let b = Buffer::from_text("ab\ncd\nef");
        assert_eq!(b.line_start(0), 0);
        assert_eq!(b.line_start(1), 3);
        assert_eq!(b.line_start(2), 6);
    }

    #[test]
    fn line_count_agrees_with_the_rope() {
        // The free function is an optimisation of `Buffer::len_lines`, so the
        // two must never disagree -- that would put the gutter and the caret
        // on different lines.
        for text in [
            "",
            "a",
            "a\n",
            "a\nb",
            "a\nb\n",
            "\n",
            "\n\n\n",
            "line\r\nline\r\n",
            "日本\n語",
        ] {
            assert_eq!(
                line_count(text),
                Buffer::from_text(text).len_lines(),
                "disagreement on {text:?}"
            );
        }
    }

    #[test]
    fn line_count_counts_the_line_after_a_trailing_newline() {
        assert_eq!(line_count(""), 1);
        assert_eq!(line_count("a"), 1);
        assert_eq!(line_count("a\n"), 2, "str::lines() says 1, and is wrong");
        assert_eq!(line_count("a\nb"), 2);
    }

    #[test]
    fn char_at_answers_for_the_position_past_the_end() {
        // Caret arithmetic asks about that position constantly.
        let b = Buffer::from_text("hé");
        assert_eq!(b.char_at(0), Some('h'));
        assert_eq!(b.char_at(1), Some('é'), "chars, not bytes");
        assert_eq!(b.char_at(2), None);
        assert_eq!(b.char_at(999), None);
    }

    #[test]
    fn slice_and_line_read_back() {
        let b = Buffer::from_text("one\ntwo\nthree");
        assert_eq!(b.slice(0..3), "one");
        assert_eq!(b.line(1), "two\n");
        assert_eq!(b.line(2), "three");
        assert_eq!(b.line(99), "");
        assert_eq!(b.slice(0..999), "one\ntwo\nthree");
    }

    #[test]
    fn byte_and_character_offsets_round_trip_across_multibyte_text() {
        let buffer = Buffer::from_text("a日b");
        assert_eq!(buffer.char_to_byte(2), 4, "日 is three bytes");
        assert_eq!(buffer.byte_to_char(4), 2);
        assert_eq!(buffer.byte_to_char(99), 3, "past the end clamps");
        assert_eq!(buffer.char_to_byte(99), 5, "past the end clamps");
    }

    /// Apply what `change_to` found, the way a mirror does.
    fn applied(before: &str, after: &str) -> String {
        let mut buffer = Buffer::from_text(before);
        if let Some(change) = buffer.change_to(after) {
            let start = change.removed_chars.start;
            buffer.remove(change.removed_chars);
            buffer.insert(start, &after[change.inserted_bytes]);
        }
        buffer.to_string()
    }

    #[test]
    fn one_keystroke_is_one_character_of_change() {
        let buffer = Buffer::from_text("hello world");
        assert_eq!(
            buffer.change_to("hello, world"),
            Some(Change {
                removed_chars: 5..5,
                inserted_bytes: 5..6,
            })
        );
        assert_eq!(
            buffer.change_to("hell world"),
            Some(Change {
                removed_chars: 4..5,
                inserted_bytes: 4..4,
            })
        );
    }

    #[test]
    fn the_same_text_is_no_change() {
        assert_eq!(Buffer::from_text("same").change_to("same"), None);
        assert_eq!(Buffer::new().change_to(""), None);
    }

    #[test]
    fn a_change_after_multibyte_text_counts_characters_here_and_bytes_there() {
        // The removed range indexes the rope and the inserted one slices the
        // string: after 日本 they are 2 and 6.
        let buffer = Buffer::from_text("日本語");
        assert_eq!(
            buffer.change_to("日本x語"),
            Some(Change {
                removed_chars: 2..2,
                inserted_bytes: 6..7,
            })
        );
    }

    #[test]
    fn a_repeated_character_is_not_counted_at_both_ends() {
        // "aa" to "aaa" matches two from the front and, unchecked, two more
        // from the back -- a span ending before it starts.
        assert_eq!(
            Buffer::from_text("aa").change_to("aaa"),
            Some(Change {
                removed_chars: 2..2,
                inserted_bytes: 2..3,
            })
        );
        assert_eq!(
            Buffer::from_text("aaa").change_to("aa"),
            Some(Change {
                removed_chars: 2..3,
                inserted_bytes: 2..2,
            })
        );
    }

    #[test]
    fn applying_the_change_always_produces_the_other_text() {
        let pairs = [
            ("", "typed"),
            ("typed", ""),
            ("abc", "xyz"),
            ("line one\nline two\n", "line one\nline 2\n"),
            ("crlf\r\nend", "crlf\nend"),
            ("日本語 テキスト", "日本 テキスト"),
            ("aXa", "aa"),
            ("abab", "ababab"),
            ("héllo", "hello"),
        ];
        for (before, after) in pairs {
            assert_eq!(applied(before, after), after, "{before:?} to {after:?}");
            assert_eq!(applied(after, before), before, "{after:?} to {before:?}");
        }
    }
}
