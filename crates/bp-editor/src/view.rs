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
}
