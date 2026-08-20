//! Where a long line breaks when it is wrapped.
//!
//! This module owns one idea and nothing else: given a line of text and a
//! width, which characters start each visual row. Everything that follows
//! from that -- drawing, the caret, clicks, what Up and Down mean -- is
//! `view`'s job, and all of it is built on the offsets computed here.
//!
//! **A visual row is not a document line.** That is the whole difficulty of
//! word wrap, and the reason this is a module rather than three lines inside
//! a widget: every mapping in `view` was written when the two were the same
//! thing, and each one has to be told otherwise. Keeping the breaking rule in
//! one tested place means there is exactly one answer to "where does this
//! line break", rather than one per function that needs to know.
//!
//! Widths here are **visual columns**, not pixels and not characters. A tab
//! occupies as many columns as it takes to reach the next tab stop, so a line
//! of four tabs is sixteen columns wide at a tab width of four -- and wraps
//! as though it were sixteen characters.

/// How wide a visual row may be, in columns.
///
/// A `usize` rather than an `Option<usize>` with zero meaning off, because
/// the arithmetic has to guard against a zero width anyway -- a width of zero
/// cannot fit even one character, and a loop that never advances is worse
/// than a line that does not wrap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Wrap {
    /// Columns per visual row, or 0 for no wrapping at all.
    pub columns: usize,
}

impl Wrap {
    /// Wrapping switched off: one visual row per document line, which is what
    /// every function in `view` assumed before this module existed.
    pub const OFF: Self = Self { columns: 0 };

    #[must_use]
    pub const fn at(columns: usize) -> Self {
        Self { columns }
    }

    #[must_use]
    pub const fn is_on(self) -> bool {
        self.columns > 0
    }
}

/// Whether a character is a place a line may break *after*.
///
/// Spaces and tabs only. Breaking after a hyphen reads well in prose and
/// badly in the other thing this editor is for -- `--editor-view` would wrap
/// to `--` / `editor-view`, and a path or a flag would come apart at every
/// dash. Prose loses a little; code and data lose nothing.
fn breakable(ch: char) -> bool {
    ch == ' ' || ch == '\t'
}

/// Whether a character may hang past the width instead of forcing a break.
///
/// Spaces, and deliberately not tabs. A run of spaces at the end of a row is
/// invisible, so letting it overflow the margin costs nothing and keeps the
/// continuation flush -- which is what every editor does and what the reader
/// expects. A tab is indentation: it is wide, it is load-bearing in code, and
/// a line of tabs that never wrapped because each one hung would leave the
/// text it indents off the right of the window.
fn hangs(ch: char) -> bool {
    ch == ' '
}

/// The column a character advances to from `visual`.
fn advance(visual: usize, ch: char, tab: usize) -> usize {
    if ch == '\t' {
        (visual / tab + 1) * tab
    } else {
        visual + 1
    }
}

/// The character offsets that start each visual row of `line`.
///
/// Always begins with 0, so the result is never empty and the first row of a
/// line is always its start. With wrapping off, or a line that fits, that is
/// the entire result.
///
/// `line` must not carry its line break; pass the content only.
///
/// Breaks after the last space that fits, so a word is not split when it does
/// not have to be. A word longer than the whole width -- a URL, a long
/// identifier, a line of base64 -- is broken hard at the width instead,
/// because the alternative is a row wider than the window with no way to
/// reach its end.
///
/// Trailing spaces stay on the row they end, rather than opening the next
/// one. Pushing them down would indent the continuation by however many
/// spaces happened to be there, which looks like a bug in the wrap and is
/// not what any editor does.
#[must_use]
pub fn row_starts(line: &str, wrap: Wrap, tab_width: usize) -> Vec<usize> {
    let mut starts = vec![0usize];
    if !wrap.is_on() {
        return starts;
    }
    let width = wrap.columns;
    let tab = tab_width.max(1);

    // Always the column *within the current row*, so after a break it is
    // recomputed from the row's own start rather than carried over.
    let mut visual = 0usize;
    // The offset just after the last breakable character on this row, and
    // `None` until there is one. Just after, not at: breaking after the space
    // is what keeps the space on the row it ends.
    let mut last_break: Option<usize> = None;

    for (index, ch) in line.chars().enumerate() {
        let next = advance(visual, ch, tab);
        let row_start = *starts.last().unwrap_or(&0);

        // `>` not `>=`: a character ending exactly on the width fits. The
        // test is on the character's *end* column, so a tab straddling the
        // edge moves down whole rather than being drawn across the break.
        //
        // `index > row_start` keeps the loop advancing: the first character
        // of a row goes on it whatever its width, because there is no
        // narrower row to move it to.
        if next > width && !hangs(ch) && index > row_start {
            // A break opportunity only helps if it is inside this row and
            // leaves something on it; otherwise break hard, here.
            let at = match last_break {
                Some(at) if at > row_start => at,
                _ => index,
            };
            starts.push(at);
            last_break = None;

            // Characters between the break and here are on the new row
            // already, and count against its width.
            visual = line
                .chars()
                .skip(at)
                .take(index - at)
                .fold(0, |column, c| advance(column, c, tab));
            let next = advance(visual, ch, tab);
            if breakable(ch) {
                last_break = Some(index + 1);
            }
            visual = next;
            continue;
        }

        if breakable(ch) {
            last_break = Some(index + 1);
        }
        visual = next;
    }
    starts
}

/// How many visual rows `line` occupies.
#[must_use]
pub fn row_count(line: &str, wrap: Wrap, tab_width: usize) -> usize {
    row_starts(line, wrap, tab_width).len()
}

/// Which visual row of `line` the character at `char_column` falls on, and
/// where that row starts.
///
/// A caret at the end of a row that was broken at a space belongs to the
/// *next* row, which is where the user sees it after typing that space.
#[must_use]
pub fn row_of(line: &str, char_column: usize, wrap: Wrap, tab_width: usize) -> (usize, usize) {
    let starts = row_starts(line, wrap, tab_width);
    // The last row whose start is at or before the column. `partition_point`
    // rather than a scan because `starts` is sorted by construction and this
    // is called for every frame's caret.
    let row = starts.partition_point(|&start| start <= char_column).max(1) - 1;
    (row, starts[row])
}

/// The character offset one past the end of visual row `row` of `line`.
#[must_use]
pub fn row_end(line: &str, row: usize, wrap: Wrap, tab_width: usize) -> usize {
    let starts = row_starts(line, wrap, tab_width);
    starts
        .get(row + 1)
        .copied()
        .unwrap_or_else(|| line.chars().count())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The text of each visual row, which is what these rules are really
    /// about -- an offset list is hard to read and easy to get subtly wrong.
    fn rows(line: &str, columns: usize, tab_width: usize) -> Vec<String> {
        let starts = row_starts(line, Wrap::at(columns), tab_width);
        let chars: Vec<char> = line.chars().collect();
        starts
            .iter()
            .enumerate()
            .map(|(i, &start)| {
                let end = starts.get(i + 1).copied().unwrap_or(chars.len());
                chars[start..end].iter().collect()
            })
            .collect()
    }

    #[test]
    fn wrapping_off_gives_one_row_however_long_the_line() {
        let long = "x".repeat(500);
        assert_eq!(row_starts(&long, Wrap::OFF, 4), vec![0]);
        assert_eq!(row_count(&long, Wrap::OFF, 4), 1);
    }

    #[test]
    fn a_line_that_fits_is_not_broken() {
        assert_eq!(rows("hello", 10, 4), vec!["hello"]);
        // Exactly the width still fits: the check is on the end column.
        assert_eq!(rows("hello", 5, 4), vec!["hello"]);
    }

    #[test]
    fn a_break_falls_after_a_space_rather_than_inside_a_word() {
        assert_eq!(rows("hello world", 8, 4), vec!["hello ", "world"]);
    }

    #[test]
    fn trailing_spaces_stay_on_the_row_they_end() {
        // Pushing them down would indent the continuation by however many
        // spaces happened to be typed, which reads as a broken wrap.
        let wrapped = rows("ab   cd", 4, 4);
        assert_eq!(wrapped[0], "ab   ", "the spaces belong to the first row");
        assert_eq!(wrapped[1], "cd");
    }

    #[test]
    fn a_word_longer_than_the_width_is_broken_hard() {
        // The alternative is a row wider than the window whose end cannot be
        // reached.
        assert_eq!(rows("abcdefghij", 4, 4), vec!["abcd", "efgh", "ij"]);
    }

    #[test]
    fn a_long_word_after_a_space_breaks_at_the_space_then_hard() {
        assert_eq!(
            rows("go abcdefghij", 4, 4),
            vec!["go ", "abcd", "efgh", "ij"]
        );
    }

    #[test]
    fn a_tab_is_measured_in_columns_not_characters() {
        // Two tabs at width 4 is eight columns, so at a width of 4 they
        // occupy a row each -- even though they are two characters.
        assert_eq!(row_count("\t\t", Wrap::at(4), 4), 2);
        // And at width 8 they fit together.
        assert_eq!(row_count("\t\t", Wrap::at(8), 4), 1);
    }

    #[test]
    fn an_empty_line_is_one_row() {
        assert_eq!(row_starts("", Wrap::at(10), 4), vec![0]);
        assert_eq!(row_count("", Wrap::at(10), 4), 1);
    }

    #[test]
    fn a_width_of_one_still_advances() {
        // The infinite-loop case. Every character has to land somewhere, so
        // a width too narrow to hold one is still one character per row.
        let wrapped = rows("abc", 1, 4);
        assert_eq!(wrapped, vec!["a", "b", "c"]);
    }

    #[test]
    fn a_tab_wider_than_the_row_still_advances() {
        // A tab at width 8 cannot fit in a 4-column row, and there is no
        // smaller piece of it to place. It takes the row anyway.
        let count = row_count("\tx", Wrap::at(2), 8);
        assert!(count >= 2, "the tab and the x cannot share a 2-column row");
        assert!(count < 10, "but it must not loop; got {count}");
    }

    #[test]
    fn the_rows_partition_the_line_exactly() {
        // The property that matters to every caller: no character is lost,
        // duplicated or reordered by wrapping. If this holds, drawing the
        // rows in order draws the line.
        for columns in 1..12usize {
            for line in [
                "",
                "a",
                "hello world",
                "the quick brown fox jumps",
                "\tindented\ttext\there",
                "supercalifragilistic",
                "   leading spaces",
                "trailing   ",
                "a  b  c  d  e",
            ] {
                let joined: String = rows(line, columns, 4).concat();
                assert_eq!(
                    joined, line,
                    "width {columns} lost or altered characters of {line:?}"
                );
            }
        }
    }

    #[test]
    fn row_starts_are_strictly_increasing() {
        // A repeated start is a zero-width row, which means a row that draws
        // nothing and a caret that cannot leave it.
        for columns in 1..12usize {
            for line in [
                "the quick brown fox",
                "\t\t\tdeeply indented",
                "aaaaaaaaaaaaaaaaaaaa",
                "a b  c   d    e",
            ] {
                let starts = row_starts(line, Wrap::at(columns), 4);
                for pair in starts.windows(2) {
                    assert!(
                        pair[1] > pair[0],
                        "width {columns} on {line:?} produced an empty row at {pair:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn no_rows_visible_text_is_wider_than_the_width() {
        // The point of the exercise. Trailing spaces are excluded because
        // they are allowed to hang past the margin -- they draw nothing, so a
        // row that "exceeds" the width only by them does not overflow the
        // window. What must never exceed it is text the reader can see.
        //
        // The two exemptions are real and narrow: a row holding a single
        // character has nowhere narrower to put it, and a tab can be wider
        // than the row on its own.
        for columns in 2..12usize {
            for line in [
                "the quick brown fox jumps over",
                "supercalifragilisticexpialidocious",
                "\tone\ttwo\tthree",
                "a b c d e f g h",
                "trailing spaces   ",
            ] {
                for row in rows(line, columns, 4) {
                    let visible = row.trim_end_matches(' ');
                    let width =
                        super::super::view::visual_column(visible, visible.chars().count(), 4);
                    let single = visible.chars().count() <= 1;
                    assert!(
                        width <= columns || single,
                        "width {columns} on {line:?} produced a {width}-column row {row:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_caret_row_agrees_with_where_the_rows_actually_start() {
        // `row_of` is a lookup into the same list, and this is what says the
        // two never disagree -- a caret on the wrong row is invisible in a
        // test that only checks the drawing.
        let line = "the quick brown fox jumps";
        let wrap = Wrap::at(10);
        let starts = row_starts(line, wrap, 4);

        for column in 0..=line.chars().count() {
            let (row, start) = row_of(line, column, wrap, 4);
            assert_eq!(start, starts[row], "row {row} reported the wrong start");
            assert!(column >= start, "column {column} is before its row starts");
            let end = row_end(line, row, wrap, 4);
            assert!(
                column <= end,
                "column {column} is past the end of row {row} ({end})"
            );
        }
    }

    #[test]
    fn the_caret_at_the_end_of_the_line_is_on_the_last_row() {
        let line = "hello world";
        let wrap = Wrap::at(8);
        let last = row_count(line, wrap, 4) - 1;
        let (row, _) = row_of(line, line.chars().count(), wrap, 4);
        assert_eq!(row, last);
    }
}
