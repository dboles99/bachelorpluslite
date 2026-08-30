//! Mapping between the document and the screen.
//!
//! A custom editor view has to answer four questions on every frame: which
//! lines to draw, where the caret goes, what the selection covers, and which
//! character the user just clicked on. All four are arithmetic over the
//! buffer and a few font measurements, so all four live here rather than in
//! the widget -- where they would be untestable and, being arithmetic, wrong.
//!
//! Columns come in two kinds and confusing them is the classic defect:
//!
//! * a **character column** indexes the buffer;
//! * a **visual column** is where the character appears, after a tab has
//!   expanded to the next tab stop.
//!
//! They differ the moment a line contains a tab, and every function here says
//! which it means.

use std::ops::Range;

use bp_buffer::Buffer;

/// What the view is drawn with.
///
/// Monospace only: `advance` is one number rather than a per-glyph
/// measurement, which is what makes the mapping arithmetic instead of a text
/// layout pass. specs.md section 4 does not ask for proportional text, and a
/// gutter that lines up with its content is worth more than one that could.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    /// Width of one character, in pixels.
    pub advance: f32,
    /// Distance between the tops of two consecutive lines, in pixels.
    pub line_height: f32,
    /// Columns a tab advances to a multiple of.
    pub tab_width: usize,
}

impl Default for Metrics {
    fn default() -> Self {
        Self {
            advance: 8.0,
            line_height: 18.0,
            tab_width: 4,
        }
    }
}

impl Metrics {
    /// Measurements arrive from the toolkit, which can report zero before the
    /// first layout pass. Dividing by that produces an infinity that becomes
    /// `usize::MAX` on the way to a line index, so it is clamped here once
    /// rather than guarded at each of the four call sites.
    fn advance(self) -> f32 {
        if self.advance > 0.0 {
            self.advance
        } else {
            1.0
        }
    }

    fn line_height(self) -> f32 {
        if self.line_height > 0.0 {
            self.line_height
        } else {
            1.0
        }
    }

    fn tab_width(self) -> usize {
        self.tab_width.max(1)
    }
}

/// A position in the view, in pixels relative to its top-left corner.
///
/// `y` can be negative: the caret being above the visible area is exactly
/// what the caller needs to know in order to scroll to it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

/// One line's worth of selection highlight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    /// Row within the viewport; 0 is the first visible line.
    pub row: usize,
    /// Visual columns the highlight covers.
    pub columns: Range<usize>,
}

/// Strip the line break from a line read out of the buffer.
fn content(line: &str) -> &str {
    line.strip_suffix('\n')
        .map_or(line, |l| l.strip_suffix('\r').unwrap_or(l))
}

/// Where the character at `char_column` appears, in visual columns.
///
/// Counting characters would put the caret in the wrong place on any line
/// containing a tab, and the gutter and the text would disagree.
pub fn visual_column(line: &str, char_column: usize, tab_width: usize) -> usize {
    let tab = tab_width.max(1);
    let mut visual = 0;

    for ch in line.chars().take(char_column) {
        visual = if ch == '\t' {
            (visual / tab + 1) * tab
        } else {
            visual + 1
        };
    }
    visual
}

/// `line` with every tab replaced by the spaces it stands for.
///
/// The counterpart to [`visual_column`], and it exists because a manual pass
/// found the two disagreeing on screen. The surface places the caret and the
/// selection at `visual_column x advance`, so a tab **drawn** as a single
/// glyph -- which is what a toolkit does with `\t` -- puts every caret on
/// that line somewhere the character is not, and the further along the line
/// the worse it gets.
///
/// Stops are counted from the start of `line`. For a row that is the left
/// edge of the screen, which is the same basis [`visual_column`] and
/// [`char_column`] use; they have to agree or the caret and the glyphs part
/// company again.
///
/// Spaces rather than a rendered tab because the width has to be *ours*: the
/// toolkit's idea of a tab stop is its own, and the whole reason the mapping
/// here is arithmetic rather than a layout pass is that nothing else gets to
/// decide how wide a column is.
#[must_use]
pub fn expand_tabs(line: &str, tab_width: usize) -> String {
    if !line.contains('\t') {
        return line.to_owned();
    }
    let tab = tab_width.max(1);
    let mut out = String::with_capacity(line.len() + tab);
    let mut visual = 0;
    for ch in line.chars() {
        if ch == '\t' {
            let stop = (visual / tab + 1) * tab;
            out.extend(std::iter::repeat_n(' ', stop - visual));
            visual = stop;
        } else {
            out.push(ch);
            visual += 1;
        }
    }
    out
}

/// Which character sits at `target`, in visual columns.
///
/// The inverse of [`visual_column`]. A click inside the whitespace a tab
/// expands to lands on whichever end of it is nearer, because a caret cannot
/// sit in the middle of a tab and pretending otherwise puts it somewhere the
/// user did not click.
pub fn char_column(line: &str, target: usize, tab_width: usize) -> usize {
    let tab = tab_width.max(1);
    let mut visual = 0;

    for (index, ch) in line.chars().enumerate() {
        if visual >= target {
            return index;
        }
        let next = if ch == '\t' {
            (visual / tab + 1) * tab
        } else {
            visual + 1
        };
        if next > target {
            let into = target - visual;
            let remaining = next - target;
            return if into >= remaining { index + 1 } else { index };
        }
        visual = next;
    }
    line.chars().count()
}

/// The character offset at a viewport row and visual column.
///
/// The primitive the widget actually uses: a toolkit that knows its own font
/// can divide pixels by the advance itself, and keeping pixels on that side
/// of the boundary means there is one place where the two could disagree
/// rather than two.
///
/// Clamped in both directions, because a drag that leaves the window still
/// has to select something sensible.
pub fn offset_at_cell(
    buffer: &Buffer,
    first_line: usize,
    row: usize,
    column: usize,
    tab_width: usize,
) -> usize {
    let line = first_line
        .saturating_add(row)
        .min(buffer.len_lines().saturating_sub(1));
    let text = buffer.line(line);

    buffer.line_start(line) + char_column(content(&text), column, tab_width)
}

/// The caret's viewport row and visual column.
///
/// The row is signed: negative means above the visible area, which is what
/// tells the caller to scroll rather than to draw.
pub fn caret_cell(
    buffer: &Buffer,
    first_line: usize,
    char_idx: usize,
    tab_width: usize,
) -> (isize, usize) {
    let position = buffer.position_of(char_idx);
    let line = position.line - 1;
    let text = buffer.line(line);

    let row = isize::try_from(line).unwrap_or(isize::MAX)
        - isize::try_from(first_line).unwrap_or(isize::MAX);
    (
        row,
        visual_column(content(&text), position.column - 1, tab_width),
    )
}

/// The character offset the point `(x, y)` falls on.
pub fn offset_at(buffer: &Buffer, metrics: Metrics, first_line: usize, point: Point) -> usize {
    // `as` saturates, so a row beyond any real document lands on the last
    // line rather than wrapping to the first.
    let row = (point.y / metrics.line_height()).floor().max(0.0) as usize;
    let column = (point.x / metrics.advance()).round().max(0.0) as usize;

    offset_at_cell(buffer, first_line, row, column, metrics.tab_width())
}

/// Where the caret sits for a character offset.
pub fn caret_point(buffer: &Buffer, metrics: Metrics, first_line: usize, char_idx: usize) -> Point {
    let (row, column) = caret_cell(buffer, first_line, char_idx, metrics.tab_width());

    Point {
        x: column as f32 * metrics.advance(),
        y: row as f32 * metrics.line_height(),
    }
}

/// The first visible line that brings `line` into view, moving as little as
/// possible.
///
/// Scrolling further than necessary loses the reader's place; this keeps the
/// caret on screen and otherwise leaves the view alone.
pub fn reveal(first_line: usize, rows: usize, line: usize) -> usize {
    let rows = rows.max(1);
    if line < first_line {
        line
    } else if line >= first_line + rows {
        line + 1 - rows
    } else {
        first_line
    }
}

/// How many lines fit in a view `height` pixels tall.
pub fn rows_for(metrics: Metrics, height: f32) -> usize {
    if height <= 0.0 {
        return 1;
    }
    // Ceil rather than floor: a partly-visible last line is still drawn, and
    // failing to draw it leaves a strip of background where text should be.
    ((height / metrics.line_height()).ceil() as usize).max(1)
}

/// The lines to draw, as `(line index, text)` without line breaks.
pub fn visible_lines(buffer: &Buffer, first_line: usize, rows: usize) -> Vec<(usize, String)> {
    let last = buffer.len_lines();
    (first_line..(first_line + rows).min(last))
        .map(|line| (line, content(&buffer.line(line)).to_owned()))
        .collect()
}

/// The selection highlight, one span per visible line it covers.
///
/// A line whose break is inside the selection gets one extra column of
/// highlight, so a multi-line selection reads as covering the newline rather
/// than stopping raggedly at each line's last character.
pub fn selection_spans(
    buffer: &Buffer,
    selection: &Range<usize>,
    metrics: Metrics,
    first_line: usize,
    rows: usize,
) -> Vec<Span> {
    if selection.is_empty() || rows == 0 {
        return Vec::new();
    }
    let tab = metrics.tab_width();
    let first_selected = buffer.position_of(selection.start).line - 1;
    let last_selected = buffer.position_of(selection.end).line - 1;

    let from = first_selected.max(first_line);
    let to = last_selected.min(first_line + rows - 1);
    if from > to {
        return Vec::new();
    }

    (from..=to)
        .map(|line| {
            let start_of_line = buffer.line_start(line);
            let text = buffer.line(line);
            let text = content(&text);
            let len = text.chars().count();

            let from_char = selection.start.saturating_sub(start_of_line).min(len);
            let start = if line == first_selected {
                visual_column(text, from_char, tab)
            } else {
                0
            };
            let end = if line == last_selected {
                visual_column(text, selection.end - start_of_line, tab)
            } else {
                // The line break is selected too.
                visual_column(text, len, tab) + 1
            };

            Span {
                row: line - first_line,
                columns: start..end.max(start),
            }
        })
        .collect()
}

// --- wrapped layout ------------------------------------------------------
//
// Everything above assumes one visual row per document line. Everything below
// does not, and the two must agree exactly when wrapping is off -- which is
// what `the_wrapped_mapping_matches_the_unwrapped_one_when_wrapping_is_off`
// exists to say.
//
// Tab stops are measured **within a visual row**, not from the document
// line's start. A continuation row is drawn from the left edge, so its tabs
// advance from there; measuring them from the line start would draw a tab one
// width and click it as another. That choice is also what lets every function
// below reuse `visual_column` and `char_column` unchanged, on the row's own
// text -- there is no second column implementation to keep in step.

use crate::wrap::{self, Wrap};

/// How the document is laid out on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    pub wrap: Wrap,
    pub tab_width: usize,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            wrap: Wrap::OFF,
            tab_width: 4,
        }
    }
}

impl Layout {
    fn tab(self) -> usize {
        self.tab_width.max(1)
    }
}

/// A scroll position, as a visual row rather than a document line.
///
/// Two fields rather than one global row index, because a global index would
/// have to be computed by laying out every line above it -- an O(document)
/// cost on every frame, on the one operation that has to stay cheap. Anchored
/// to a line and an offset within it, scrolling costs only what is on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Anchor {
    pub line: usize,
    /// Which wrapped row of that line, 0 for the first.
    pub sub_row: usize,
}

impl Anchor {
    #[must_use]
    pub const fn at(line: usize, sub_row: usize) -> Self {
        Self { line, sub_row }
    }
}

/// One row as it appears on screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisualRow {
    /// The document line it belongs to.
    pub line: usize,
    /// Which wrapped row of that line. 0 means this row carries the line's
    /// number in the gutter; higher means it is a continuation and does not.
    pub sub_row: usize,
    /// Character offset within the line where this row starts.
    pub start: usize,
    pub text: String,
}

impl VisualRow {
    /// Whether this row should show a line number.
    ///
    /// A continuation is the same line, and numbering it again would say the
    /// document is longer than it is.
    #[must_use]
    pub const fn is_continuation(&self) -> bool {
        self.sub_row > 0
    }

    /// The row as it should be *drawn*.
    ///
    /// [`text`](Self::text) is the row's characters, which is what the column
    /// arithmetic in this module indexes; this is its appearance, which is
    /// what a toolkit paints. They differ only in tabs, and that difference
    /// is the whole point -- see [`expand_tabs`].
    ///
    /// A method rather than a second field, because the two must not be
    /// allowed to drift: a field would be set once at construction and then
    /// be wrong the moment the tab width changed under it.
    #[must_use]
    pub fn display_text(&self, tab_width: usize) -> String {
        expand_tabs(&self.text, tab_width)
    }
}

/// How many visual rows a document line occupies.
#[must_use]
pub fn rows_in_line(buffer: &Buffer, line: usize, layout: Layout) -> usize {
    if line >= buffer.len_lines() {
        return 1;
    }
    let text = buffer.line(line);
    wrap::row_count(content(&text), layout.wrap, layout.tab())
}

/// Move `delta` visual rows from `at`, stopping at the document's ends.
///
/// The primitive scrolling and vertical motion are both built on, so that
/// "one row down" means the same thing to the wheel and to the Down key.
#[must_use]
pub fn step_row(buffer: &Buffer, at: Anchor, delta: isize, layout: Layout) -> Anchor {
    let last_line = buffer.len_lines().saturating_sub(1);
    let mut anchor = Anchor {
        line: at.line.min(last_line),
        sub_row: at
            .sub_row
            .min(rows_in_line(buffer, at.line.min(last_line), layout) - 1),
    };

    for _ in 0..delta.unsigned_abs() {
        if delta > 0 {
            if anchor.sub_row + 1 < rows_in_line(buffer, anchor.line, layout) {
                anchor.sub_row += 1;
            } else if anchor.line < last_line {
                anchor.line += 1;
                anchor.sub_row = 0;
            } else {
                break;
            }
        } else if anchor.sub_row > 0 {
            anchor.sub_row -= 1;
        } else if anchor.line > 0 {
            anchor.line -= 1;
            anchor.sub_row = rows_in_line(buffer, anchor.line, layout) - 1;
        } else {
            break;
        }
    }
    anchor
}

/// The rows to draw, starting at `anchor`.
#[must_use]
pub fn visible_rows(
    buffer: &Buffer,
    anchor: Anchor,
    rows: usize,
    layout: Layout,
) -> Vec<VisualRow> {
    let mut out = Vec::with_capacity(rows);
    let mut at = anchor;
    let last_line = buffer.len_lines().saturating_sub(1);

    for _ in 0..rows {
        if at.line > last_line {
            break;
        }
        let text = buffer.line(at.line);
        let text = content(&text);
        let starts = wrap::row_starts(text, layout.wrap, layout.tab());
        let Some(&start) = starts.get(at.sub_row) else {
            break;
        };
        let end = starts
            .get(at.sub_row + 1)
            .copied()
            .unwrap_or_else(|| text.chars().count());

        out.push(VisualRow {
            line: at.line,
            sub_row: at.sub_row,
            start,
            text: text.chars().take(end).skip(start).collect(),
        });

        let next = step_row(buffer, at, 1, layout);
        if next == at {
            break;
        }
        at = next;
    }
    out
}

/// Which visual row a character offset sits on, and where in it.
///
/// Returns the anchor of that row plus the visual column within it, so the
/// caller can ask both "which row" and "where on it" from one walk.
#[must_use]
pub fn row_at_offset(buffer: &Buffer, char_idx: usize, layout: Layout) -> (Anchor, usize) {
    let position = buffer.position_of(char_idx);
    let line = position.line - 1;
    let text = buffer.line(line);
    let text = content(&text);
    let column = position.column - 1;

    let (sub_row, start) = wrap::row_of(text, column, layout.wrap, layout.tab());
    let row_text: String = text.chars().skip(start).collect();
    let visual = visual_column(&row_text, column.saturating_sub(start), layout.tab());

    (Anchor::at(line, sub_row), visual)
}

/// The caret's viewport row and visual column, with wrapping applied.
///
/// The row is signed, and negative means above the visible area -- the same
/// contract as [`caret_cell`], so the caller's "scroll or draw" logic does not
/// change with wrapping.
#[must_use]
pub fn caret_row_cell(
    buffer: &Buffer,
    anchor: Anchor,
    char_idx: usize,
    layout: Layout,
) -> (isize, usize) {
    let (caret, visual) = row_at_offset(buffer, char_idx, layout);
    (rows_between(buffer, anchor, caret, layout), visual)
}

/// How many visual rows separate two anchors; negative if `to` is above
/// `from`.
///
/// Walks the lines between them rather than laying out the whole document.
/// The cost is proportional to the distance, which for a caret against the
/// viewport is at most a screenful -- except immediately after a jump, which
/// is a one-off.
#[must_use]
pub fn rows_between(buffer: &Buffer, from: Anchor, to: Anchor, layout: Layout) -> isize {
    let sign = if (to.line, to.sub_row) < (from.line, from.sub_row) {
        -1
    } else {
        1
    };
    let (low, high) = if sign < 0 { (to, from) } else { (from, to) };

    let mut rows: isize = 0;
    for line in low.line..high.line {
        rows += isize::try_from(rows_in_line(buffer, line, layout)).unwrap_or(isize::MAX);
    }
    rows += isize::try_from(high.sub_row).unwrap_or(0);
    rows -= isize::try_from(low.sub_row).unwrap_or(0);
    rows * sign
}

/// The character offset at a viewport row and visual column, with wrapping.
///
/// Clamped in both directions, because a drag that leaves the window still has
/// to select something sensible.
#[must_use]
pub fn offset_at_row_cell(
    buffer: &Buffer,
    anchor: Anchor,
    row: usize,
    column: usize,
    layout: Layout,
) -> usize {
    let at = step_row(
        buffer,
        anchor,
        isize::try_from(row).unwrap_or(isize::MAX),
        layout,
    );
    offset_in_row(buffer, at, column, layout)
}

/// The character offset at a visual column of one particular row.
///
/// Split out from [`offset_at_row_cell`] because vertical motion already
/// knows which row it wants and must not re-derive it by counting from the
/// viewport -- the caret can be off screen, and a negative row index is not
/// something that function can take.
#[must_use]
pub fn offset_in_row(buffer: &Buffer, at: Anchor, column: usize, layout: Layout) -> usize {
    let text = buffer.line(at.line);
    let text = content(&text);
    let starts = wrap::row_starts(text, layout.wrap, layout.tab());
    let start = starts.get(at.sub_row).copied().unwrap_or(0);
    let end = starts
        .get(at.sub_row + 1)
        .copied()
        .unwrap_or_else(|| text.chars().count());

    let row_text: String = text.chars().take(end).skip(start).collect();
    let within = char_column(&row_text, column, layout.tab());

    buffer.line_start(at.line) + start + within
}

/// The first visible row that brings `char_idx` into view, moving as little as
/// possible.
#[must_use]
pub fn reveal_row(
    buffer: &Buffer,
    anchor: Anchor,
    rows: usize,
    char_idx: usize,
    layout: Layout,
) -> Anchor {
    let rows = isize::try_from(rows.max(1)).unwrap_or(1);
    let (caret, _) = row_at_offset(buffer, char_idx, layout);
    let offset = rows_between(buffer, anchor, caret, layout);

    if offset < 0 {
        caret
    } else if offset >= rows {
        step_row(buffer, caret, 1 - rows, layout)
    } else {
        anchor
    }
}

/// The selection highlight, one span per visible *row* it covers.
#[must_use]
pub fn selection_row_spans(
    buffer: &Buffer,
    selection: &Range<usize>,
    anchor: Anchor,
    rows: usize,
    layout: Layout,
) -> Vec<Span> {
    if selection.is_empty() || rows == 0 {
        return Vec::new();
    }
    visible_rows(buffer, anchor, rows, layout)
        .into_iter()
        .enumerate()
        .filter_map(|(index, row)| {
            let row_start = buffer.line_start(row.line) + row.start;
            let chars = row.text.chars().count();
            let row_end = row_start + chars;

            // Does the selection touch this row at all?
            if selection.end < row_start || selection.start > row_end {
                return None;
            }
            let from = selection.start.saturating_sub(row_start).min(chars);
            let to = selection.end.saturating_sub(row_start).min(chars);

            let start = visual_column(&row.text, from, layout.tab());
            let mut end = visual_column(&row.text, to, layout.tab());
            // The line break is selected too, so the row that ends a
            // selected line gets one extra column -- but only the row that
            // actually carries the break, not every wrapped row of it.
            let carries_break =
                row_end == buffer.line_start(row.line) + buffer.line_len_chars(row.line);
            if selection.end > row_end && carries_break {
                end += 1;
            }

            // A selection that begins exactly where a row ends covers none of
            // it. Emitting a zero-width span would put an invisible rectangle
            // on a row the user has not selected -- harmless to look at, and
            // wrong the moment anything counts the spans.
            if end <= start {
                return None;
            }

            Some(Span {
                row: index,
                columns: start..end,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: Metrics = Metrics {
        advance: 10.0,
        line_height: 20.0,
        tab_width: 4,
    };

    fn point(x: f32, y: f32) -> Point {
        Point { x, y }
    }

    // --- tabs on screen ---------------------------------------------------

    #[test]
    fn a_tab_expands_to_the_next_stop_and_not_to_a_fixed_width() {
        // The distinction that matters: a tab is not "four spaces", it is
        // "however many spaces reach the next multiple of four".
        assert_eq!(expand_tabs("a\tb", 4), "a   b");
        assert_eq!(expand_tabs("abc\td", 4), "abc d");
        assert_eq!(expand_tabs("abcd\te", 4), "abcd    e");
        assert_eq!(expand_tabs("\tx", 4), "    x");
    }

    #[test]
    fn consecutive_tabs_each_reach_their_own_stop() {
        assert_eq!(expand_tabs("\t\tx", 4), "        x");
        assert_eq!(expand_tabs("a\t\tb", 4), "a       b");
    }

    #[test]
    fn a_line_without_tabs_is_returned_unchanged() {
        assert_eq!(expand_tabs("nothing to expand", 4), "nothing to expand");
        assert_eq!(expand_tabs("", 4), "");
    }

    #[test]
    fn a_tab_width_of_zero_is_treated_as_one() {
        // The same clamp `Layout::tab` applies. A width of zero would divide
        // by zero on the stop calculation, and a caller that read one out of
        // a config file should get a usable editor rather than a panic.
        assert_eq!(expand_tabs("a\tb", 0), "a b");
    }

    #[test]
    fn the_drawn_row_is_as_wide_as_the_column_arithmetic_thinks_it_is() {
        // **The property the caret depends on.** The surface puts the caret
        // at `visual_column x advance`, so for every position in the line the
        // number of characters drawn before it must equal the visual column
        // the arithmetic computes for it. Where these disagree, every caret
        // on a line containing a tab lands where the character is not -- and
        // that is exactly what a person found by looking at the screen.
        for line in [
            "a\tb",
            "\tindented",
            "a\t\tb",
            "abcd\te",
            "no tabs here",
            "\t",
            "trailing\t",
            "MULTIBYTE\tAFTER",
        ] {
            for width in [1usize, 2, 4, 8] {
                for i in 0..=line.chars().count() {
                    let prefix: String = line.chars().take(i).collect();
                    assert_eq!(
                        expand_tabs(&prefix, width).chars().count(),
                        visual_column(line, i, width),
                        "line {line:?} at char {i} with tab width {width}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_row_the_surface_draws_expands_its_tabs() {
        // The seam: `text` stays raw because the column arithmetic in this
        // module indexes it by character, and `display_text` is what a
        // toolkit gets.
        let buffer = Buffer::from_text("a\tb\n");
        let layout = Layout {
            wrap: Wrap::OFF,
            tab_width: 4,
        };
        let rows = visible_rows(&buffer, Anchor::default(), 1, layout);
        assert_eq!(rows[0].text, "a\tb", "the row must keep its characters");
        assert_eq!(rows[0].display_text(4), "a   b", "and draw them expanded");
    }

    #[test]
    fn a_tab_advances_to_the_next_stop_not_by_one() {
        assert_eq!(visual_column("\tx", 1, 4), 4);
        assert_eq!(visual_column("\tx", 2, 4), 5);
        assert_eq!(visual_column("ab\tc", 3, 4), 4, "from column 2 to 4");
        assert_eq!(visual_column("abcd\te", 5, 4), 8, "a full tab from a stop");
    }

    #[test]
    fn visual_and_character_columns_round_trip() {
        for line in ["plain text", "\tindented", "a\tb\tc", "\t\t", ""] {
            let count = line.chars().count();
            for char_col in 0..=count {
                let visual = visual_column(line, char_col, 4);
                assert_eq!(
                    char_column(line, visual, 4),
                    char_col,
                    "{line:?} at char column {char_col}"
                );
            }
        }
    }

    #[test]
    fn a_click_inside_a_tab_lands_on_the_nearer_end() {
        // A caret cannot sit in the middle of a tab.
        assert_eq!(char_column("\tx", 1, 4), 0, "nearer the start");
        assert_eq!(char_column("\tx", 3, 4), 1, "nearer the end");
        assert_eq!(char_column("\tx", 2, 4), 1, "the midpoint rounds forward");
    }

    #[test]
    fn a_click_past_the_end_of_a_line_lands_at_its_end() {
        assert_eq!(char_column("abc", 99, 4), 3);
        assert_eq!(char_column("", 5, 4), 0);
    }

    #[test]
    fn clicking_maps_to_the_character_under_the_pointer() {
        let buffer = Buffer::from_text("hello\nworld");

        assert_eq!(offset_at(&buffer, M, 0, point(0.0, 0.0)), 0);
        assert_eq!(offset_at(&buffer, M, 0, point(21.0, 0.0)), 2, "rounds");
        assert_eq!(offset_at(&buffer, M, 0, point(0.0, 20.0)), 6, "line 2");
        assert_eq!(offset_at(&buffer, M, 0, point(30.0, 25.0)), 9);
    }

    #[test]
    fn clicking_below_the_last_line_lands_on_it_rather_than_panicking() {
        let buffer = Buffer::from_text("one\ntwo");
        assert_eq!(offset_at(&buffer, M, 0, point(0.0, 9_000.0)), 4);
        assert_eq!(offset_at(&buffer, M, 0, point(-50.0, -50.0)), 0);
    }

    #[test]
    fn clicking_accounts_for_the_scroll_position() {
        let buffer = Buffer::from_text("one\ntwo\nthree\nfour");
        // Row 0 of the viewport is document line 2 when scrolled by two.
        assert_eq!(offset_at(&buffer, M, 2, point(0.0, 0.0)), 8);
    }

    #[test]
    fn zero_metrics_do_not_produce_a_nonsense_line() {
        // The toolkit can report zero before the first layout pass, and
        // dividing by it lands on usize::MAX rather than line 0.
        let zero = Metrics {
            advance: 0.0,
            line_height: 0.0,
            tab_width: 0,
        };
        let buffer = Buffer::from_text("one\ntwo");
        assert!(offset_at(&buffer, zero, 0, point(5.0, 5.0)) <= buffer.len_chars());
    }

    #[test]
    fn cells_and_pixels_agree() {
        // The pixel functions are wrappers; if they ever stop agreeing with
        // the cells the caret and the click land in different places.
        let buffer = Buffer::from_text("one\n\ttwo\nthree");
        for offset in 0..=buffer.len_chars() {
            let (row, column) = caret_cell(&buffer, 0, offset, M.tab_width);
            let point = caret_point(&buffer, M, 0, offset);

            assert_eq!(point.x, column as f32 * M.advance, "x at {offset}");
            assert_eq!(point.y, row as f32 * M.line_height, "y at {offset}");
            assert_eq!(
                offset_at(&buffer, M, 0, point),
                offset_at_cell(&buffer, 0, row as usize, column, M.tab_width),
                "round trip at {offset}"
            );
        }
    }

    #[test]
    fn a_caret_above_the_viewport_reports_a_negative_row() {
        let buffer = Buffer::from_text("a\nb\nc\nd");
        assert_eq!(caret_cell(&buffer, 2, 0, 4).0, -2);
        assert_eq!(caret_cell(&buffer, 0, 6, 4).0, 3);
    }

    #[test]
    fn a_cell_below_the_document_lands_on_the_last_line() {
        let buffer = Buffer::from_text("one\ntwo");
        assert_eq!(offset_at_cell(&buffer, 0, 900, 0, 4), 4);
        assert_eq!(
            offset_at_cell(&buffer, 0, 0, 900, 4),
            3,
            "past the line end"
        );
    }

    #[test]
    fn the_caret_sits_where_its_character_is_drawn() {
        let buffer = Buffer::from_text("hello\nworld");

        assert_eq!(caret_point(&buffer, M, 0, 0), point(0.0, 0.0));
        assert_eq!(caret_point(&buffer, M, 0, 3), point(30.0, 0.0));
        assert_eq!(caret_point(&buffer, M, 0, 6), point(0.0, 20.0));
    }

    #[test]
    fn the_caret_reports_a_negative_offset_when_scrolled_past() {
        // Which is how the caller knows it needs to scroll up.
        let buffer = Buffer::from_text("a\nb\nc");
        assert_eq!(caret_point(&buffer, M, 2, 0).y, -40.0);
    }

    #[test]
    fn the_caret_follows_an_expanded_tab() {
        let buffer = Buffer::from_text("\tx");
        assert_eq!(caret_point(&buffer, M, 0, 1).x, 40.0, "one tab stop in");
    }

    #[test]
    fn revealing_a_line_scrolls_as_little_as_possible() {
        assert_eq!(reveal(10, 5, 12), 10, "already visible; do not move");
        assert_eq!(reveal(10, 5, 3), 3, "scroll up to it");
        assert_eq!(reveal(10, 5, 20), 16, "scroll down just enough");
        assert_eq!(reveal(10, 5, 14), 10, "the last visible row");
        assert_eq!(reveal(10, 5, 15), 11, "one past it");
    }

    #[test]
    fn a_partly_visible_last_line_still_counts_as_a_row() {
        assert_eq!(rows_for(M, 100.0), 5);
        assert_eq!(rows_for(M, 105.0), 6, "the sliver is still drawn");
        assert_eq!(rows_for(M, 0.0), 1, "never zero");
    }

    #[test]
    fn only_the_visible_lines_are_built() {
        // The whole point of the exercise: a 100,000-line document costs the
        // same to draw as a 10-line one.
        let buffer = Buffer::from_text(&"line\n".repeat(100_000));
        let visible = visible_lines(&buffer, 50_000, 3);

        assert_eq!(visible.len(), 3);
        assert_eq!(visible[0].0, 50_000);
        assert_eq!(visible[0].1, "line");
    }

    #[test]
    fn asking_for_more_lines_than_exist_returns_what_there_is() {
        let buffer = Buffer::from_text("one\ntwo");
        assert_eq!(visible_lines(&buffer, 0, 50).len(), 2);
        assert!(visible_lines(&buffer, 99, 5).is_empty());
    }

    #[test]
    fn a_selection_within_one_line_is_one_span() {
        let buffer = Buffer::from_text("hello world");
        let spans = selection_spans(&buffer, &(2..5), M, 0, 10);

        assert_eq!(
            spans,
            vec![Span {
                row: 0,
                columns: 2..5
            }]
        );
    }

    #[test]
    fn a_multi_line_selection_covers_each_line_break() {
        // Without the extra column the highlight stops raggedly at the end of
        // each line and does not read as continuous.
        let buffer = Buffer::from_text("one\ntwo\nthree");
        let spans = selection_spans(&buffer, &(1..9), M, 0, 10);

        assert_eq!(
            spans,
            vec![
                Span {
                    row: 0,
                    columns: 1..4
                },
                Span {
                    row: 1,
                    columns: 0..4
                },
                Span {
                    row: 2,
                    columns: 0..1
                },
            ]
        );
    }

    #[test]
    fn a_selection_is_clipped_to_the_visible_rows() {
        let buffer = Buffer::from_text("a\nb\nc\nd\ne\nf");
        let spans = selection_spans(&buffer, &(0..11), M, 2, 2);

        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].row, 0, "rows are relative to the viewport");
        assert_eq!(spans[1].row, 1);
    }

    #[test]
    fn a_selection_entirely_off_screen_draws_nothing() {
        let buffer = Buffer::from_text("a\nb\nc\nd");
        assert!(selection_spans(&buffer, &(0..1), M, 3, 1).is_empty());
        assert!(selection_spans(&buffer, &(6..7), M, 0, 1).is_empty());
    }

    #[test]
    fn an_empty_selection_draws_nothing() {
        let buffer = Buffer::from_text("hello");
        assert!(selection_spans(&buffer, &(3..3), M, 0, 10).is_empty());
    }

    #[test]
    fn a_selection_across_a_tab_is_measured_in_visual_columns() {
        let buffer = Buffer::from_text("\tabc");
        let spans = selection_spans(&buffer, &(0..2), M, 0, 10);

        assert_eq!(spans[0].columns, 0..5, "the tab is four columns wide");
    }

    // --- wrapped layout ---------------------------------------------------

    const OFF: Layout = Layout {
        wrap: Wrap::OFF,
        tab_width: 4,
    };

    fn wrapped(columns: usize) -> Layout {
        Layout {
            wrap: Wrap::at(columns),
            tab_width: 4,
        }
    }

    #[test]
    fn the_wrapped_mapping_matches_the_unwrapped_one_when_wrapping_is_off() {
        // The invariant that makes wrapping safe to add: with it off, every
        // wrapped function must agree with the one it replaces, character for
        // character. Anything else is a regression for the default.
        let buffer = Buffer::from_text("one\n\ttwo two\nthree\n\nlast line here");

        for first in 0..buffer.len_lines() {
            let anchor = Anchor::at(first, 0);

            let old: Vec<(usize, String)> = visible_lines(&buffer, first, 3);
            let new: Vec<(usize, String)> = visible_rows(&buffer, anchor, 3, OFF)
                .into_iter()
                .map(|r| (r.line, r.text))
                .collect();
            assert_eq!(old, new, "visible lines differ at {first}");

            for idx in 0..buffer.len_chars() {
                assert_eq!(
                    caret_cell(&buffer, first, idx, 4),
                    caret_row_cell(&buffer, anchor, idx, OFF),
                    "caret differs at offset {idx} from line {first}"
                );
            }
            for row in 0..4 {
                for column in 0..12 {
                    assert_eq!(
                        offset_at_cell(&buffer, first, row, column, 4),
                        offset_at_row_cell(&buffer, anchor, row, column, OFF),
                        "click differs at row {row} column {column} from line {first}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_wrapped_line_becomes_several_rows_carrying_one_line_number() {
        let buffer = Buffer::from_text("hello world again\nshort");
        let rows = visible_rows(&buffer, Anchor::default(), 5, wrapped(8));

        assert_eq!(rows[0].text, "hello ");
        assert!(!rows[0].is_continuation(), "the first row is numbered");
        assert_eq!(rows[1].text, "world ");
        assert!(rows[1].is_continuation(), "and its continuations are not");
        assert_eq!(rows[2].text, "again");
        assert!(rows[2].is_continuation());
        assert_eq!(rows[3].line, 1, "then the next document line");
        assert!(!rows[3].is_continuation());
    }

    #[test]
    fn scrolling_can_start_part_way_through_a_wrapped_line() {
        // The reason the anchor is a line *and* a sub-row: a wrapped line
        // taller than the window has to be scrollable through, and a
        // document-line anchor could only jump over it.
        let buffer = Buffer::from_text("hello world again\nshort");
        let rows = visible_rows(&buffer, Anchor::at(0, 1), 2, wrapped(8));

        assert_eq!(rows[0].text, "world ");
        assert_eq!(rows[0].sub_row, 1);
        assert_eq!(rows[1].text, "again");
    }

    #[test]
    fn stepping_rows_crosses_line_boundaries_and_stops_at_the_ends() {
        let buffer = Buffer::from_text("hello world\nshort\nx");
        let layout = wrapped(8);

        // Down through the wrapped first line into the second.
        assert_eq!(
            step_row(&buffer, Anchor::at(0, 0), 1, layout),
            Anchor::at(0, 1)
        );
        assert_eq!(
            step_row(&buffer, Anchor::at(0, 1), 1, layout),
            Anchor::at(1, 0)
        );
        // Up from a line start lands on the *last* row of the line above.
        assert_eq!(
            step_row(&buffer, Anchor::at(1, 0), -1, layout),
            Anchor::at(0, 1)
        );

        // Clamped rather than wrapped, in both directions.
        assert_eq!(
            step_row(&buffer, Anchor::at(0, 0), -5, layout),
            Anchor::at(0, 0)
        );
        let bottom = step_row(&buffer, Anchor::at(0, 0), 500, layout);
        assert_eq!(bottom, Anchor::at(2, 0));
    }

    #[test]
    fn a_click_on_a_continuation_row_lands_in_the_right_half_of_the_line() {
        // The defect this prevents is silent and specific: clicking the
        // second row of a wrapped line would otherwise resolve against the
        // line's start and land in the first row's text.
        let buffer = Buffer::from_text("hello world again");
        let layout = wrapped(8);

        // Row 1 is "world ", starting at character 6. Column 0 of it.
        let offset = offset_at_row_cell(&buffer, Anchor::default(), 1, 0, layout);
        assert_eq!(offset, 6);
        assert_eq!(
            offset_at_row_cell(&buffer, Anchor::default(), 2, 0, layout),
            12
        );
    }

    #[test]
    fn the_caret_and_the_click_agree_on_every_offset_of_a_wrapped_line() {
        // The involution that caught two real defects in the unwrapped
        // mapping: turn an offset into a cell and back, and get the offset
        // you started with.
        let buffer =
            Buffer::from_text("the quick brown fox jumps over\nsecond line\n\tindented text here");

        for columns in [6usize, 9, 14, 20] {
            let layout = wrapped(columns);
            let anchor = Anchor::default();
            for idx in 0..buffer.len_chars() {
                let (row, column) = caret_row_cell(&buffer, anchor, idx, layout);
                let Ok(row) = usize::try_from(row) else {
                    continue;
                };
                let back = offset_at_row_cell(&buffer, anchor, row, column, layout);
                assert_eq!(
                    back, idx,
                    "width {columns}: offset {idx} became row {row} column {column} and came back {back}"
                );
            }
        }
    }

    #[test]
    fn revealing_the_caret_scrolls_by_rows_not_by_lines() {
        // A wrapped line taller than the viewport is the case a line-based
        // reveal cannot express: the caret is on the line already showing,
        // and still off screen.
        let buffer = Buffer::from_text("aaaa bbbb cccc dddd eeee ffff");
        let layout = wrapped(5);
        let end = buffer.len_chars();

        let anchor = reveal_row(&buffer, Anchor::default(), 2, end, layout);
        let (row, _) = caret_row_cell(&buffer, anchor, end, layout);
        assert!(
            (0..2).contains(&row),
            "the caret should be inside the two visible rows; got {row}"
        );
    }

    #[test]
    fn revealing_a_caret_already_on_screen_does_not_scroll() {
        // Scrolling further than necessary loses the reader's place.
        let buffer = Buffer::from_text("hello world again\nsecond");
        let layout = wrapped(8);
        let anchor = Anchor::at(0, 1);

        assert_eq!(reveal_row(&buffer, anchor, 3, 8, layout), anchor);
    }

    #[test]
    fn a_selection_over_a_wrapped_line_is_highlighted_on_each_row_it_covers() {
        let buffer = Buffer::from_text("hello world again");
        let layout = wrapped(8);
        // "world" -- offsets 6..11, entirely inside the second visual row.
        let spans = selection_row_spans(&buffer, &(6..11), Anchor::default(), 5, layout);

        assert_eq!(spans.len(), 1, "one row is covered; got {spans:?}");
        assert_eq!(spans[0].row, 1);
        assert_eq!(spans[0].columns, 0..5);
    }

    #[test]
    fn a_selection_spanning_a_wrap_covers_both_rows() {
        let buffer = Buffer::from_text("hello world again");
        let layout = wrapped(8);
        // From inside "hello" to inside "world", across the break.
        let spans = selection_row_spans(&buffer, &(2..9), Anchor::default(), 5, layout);

        assert_eq!(spans.len(), 2, "got {spans:?}");
        assert_eq!(spans[0].row, 0);
        assert_eq!(spans[1].row, 1);
        assert_eq!(spans[1].columns.start, 0, "the second row starts selected");
    }

    #[test]
    fn only_the_row_carrying_the_line_break_is_extended_by_one() {
        // The break is drawn once. Extending every wrapped row of a selected
        // line would draw a ragged extra column down the whole paragraph.
        let buffer = Buffer::from_text("hello world again\nsecond");
        let layout = wrapped(8);
        let rows = visible_rows(&buffer, Anchor::default(), 6, layout);
        let spans = selection_row_spans(&buffer, &(0..20), Anchor::default(), 6, layout);

        // Extended means the span reaches past the row's own text, which is
        // the only way to tell a break-column apart from a fully selected
        // row that happens to be the full width.
        let extended: Vec<usize> = spans
            .iter()
            .filter(|s| {
                let row = &rows[s.row];
                s.columns.end > visual_column(&row.text, row.text.chars().count(), 4)
            })
            .map(|s| s.row)
            .collect();
        assert_eq!(
            extended,
            vec![2],
            "only the last row of line 0 carries the break; rows {rows:?} spans {spans:?}"
        );
    }
}
