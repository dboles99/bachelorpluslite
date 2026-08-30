//! The viewer for a document the rope does not hold.
//!
//! ADR-0027 measured that a file past `HUGE_FILE_BYTES` must not reach a rope
//! at all, and `bp_buffer::LargeFile` is what reads one instead: an open
//! handle, a scan buffer and a sparse line index, never the file. This module
//! is the shell's half — where in the document the reader is looking, and what
//! the surface is handed to draw.
//!
//! ## Why this is not an `Editor`
//!
//! There is no rope, so there is no caret, no selection and no undo. Every one
//! of those is a position *in a buffer*, and the buffer is a disk. A document
//! served this way has an entry in `AppState::viewers` and **no entry in
//! `AppState::editors`**, which is the whole of the distinction: everything
//! that reaches for `active_editor()` gets `None` and does nothing, rather
//! than acting on an empty document that looks real.
//!
//! That is a guard and not a coincidence, and it is not the only one. The
//! paths that would do damage rather than nothing — save, save a copy, reload,
//! and anything asking for the document's bytes — refuse by name in
//! `AppState`, because for those "do nothing" is not the same as "do nothing
//! harmful": saving a document whose text reads as empty would write an empty
//! file over two gigabytes.
//!
//! ## Finding something in it
//!
//! ADR-0042. The same document that cannot be held cannot be searched in one
//! pass either, so a scan lives here beside the viewport: `bp_search`'s
//! `StreamSearch` reads the file a window at a time and reports each hit as a
//! document line and a column in it, which is exactly what this view can act
//! on -- it scrolls by line and draws by line.
//!
//! **The scan belongs to the view rather than to the find bar**, so it cannot
//! outlive the document it is about: closing the tab drops the `HugeView` and
//! the scan and its file handle with it. It also means switching tabs away
//! and back resumes the scan rather than restarting it.
//!
//! ## ADR-0030
//!
//! This is drawn by `EditorSurface` in every build, whether or not
//! `--editor-view` was passed. `TextInput` owns its own text and there is no
//! way to hand it a window of a file it does not have; the flag stays a
//! statement about which surface *edits* a document the rope holds, and the
//! one reason it is opt-in — input-method composition — has nothing to say
//! about a surface that accepts no text.

use std::ops::Range;
use std::path::Path;

use bp_buffer::{Access, DisplayLine, LargeFile, LargeFileError};
use bp_search::{Query, StreamHit, StreamSearch, StreamSearchError};

/// A scan of one document for one query, and which hit the reader is on.
#[derive(Debug)]
struct Scan {
    search: StreamSearch,
    /// Index into `search.hits()`. Meaningless while there are none.
    index: usize,
    /// Whether the viewport has already been moved to a hit.
    ///
    /// The *first* hit found moves the view once, which is what the
    /// in-memory find does as a query is typed. Every hit after that is the
    /// reader's to step to -- a view that jumped to each new hit as the scan
    /// turned it up would be unreadable, and would keep moving long after
    /// the reader started reading.
    jumped: bool,
}

/// One huge document, and where in it the reader is looking.
#[derive(Debug)]
pub(crate) struct HugeView {
    file: LargeFile,
    /// 0-based document line at the top of the viewport.
    top: usize,
    /// The lines last handed to the surface.
    ///
    /// Cached because `refresh` runs on every keystroke and on a 1.2 s poll,
    /// and re-reading a screen of a file that has not scrolled is I/O for an
    /// answer already in hand. Keyed by the `(top, count)` it was fetched
    /// for, so a resize invalidates it as surely as a scroll does.
    rows: Vec<DisplayLine>,
    fetched_for: Option<(usize, usize)>,
    /// The scan in flight, if the find bar has asked for one.
    scan: Option<Scan>,
}

impl HugeView {
    /// Open `path` for chunked reading.
    ///
    /// `O(1)`: a `metadata` call and an open handle. No part of the file is
    /// read here, which is the property that makes opening a 2 GB document
    /// cost what opening a 2 KB one does.
    pub(crate) fn open(path: &Path) -> Result<Self, LargeFileError> {
        Ok(Self {
            file: LargeFile::open(path)?,
            top: 0,
            rows: Vec::new(),
            fetched_for: None,
            scan: None,
        })
    }

    /// Size on disk.
    pub(crate) fn len_bytes(&self) -> u64 {
        self.file.len_bytes()
    }

    /// Why this document is read-only, in `bp-buffer`'s own words.
    ///
    /// Asked of the file rather than assembled here, so the sentence the user
    /// reads is the one the crate that made the decision wrote. It says the
    /// size, and it distinguishes itself from a file that is read-only on
    /// disk — which is a different problem with a different way out.
    pub(crate) fn access(&self) -> Access {
        self.file.access()
    }

    /// The 0-based document line at the top of the viewport.
    pub(crate) fn top(&self) -> usize {
        self.top
    }

    /// The lines to draw, at most `count` of them.
    ///
    /// Fewer at the end of the file, which a caller must look at rather than
    /// assume: `display_lines` caps by line count, by line length and by total
    /// bytes, and any of the three can cut a screen short.
    ///
    /// A read failure yields no lines rather than a panic or a stale screen.
    /// The document is on a disk that has just stopped answering; drawing the
    /// previous screen would say it is still there.
    pub(crate) fn rows(&mut self, count: usize) -> &[DisplayLine] {
        let count = count.max(1);
        if self.fetched_for != Some((self.top, count)) {
            self.rows = self.file.display_lines(self.top, count).unwrap_or_default();
            self.fetched_for = Some((self.top, count));
        }
        &self.rows
    }

    /// Scroll by `delta` lines, and report whether the view actually moved.
    ///
    /// **Bounded at both ends, and the bottom edge is found by asking rather
    /// than by counting.** The end of the file is not known until the whole
    /// document has been indexed, which is exactly the walk this viewer
    /// exists to avoid — so a candidate position that yields no lines is one
    /// past the end, and the view stays where it was. That is one seek and
    /// one chunk per refused scroll, against indexing two gigabytes to learn
    /// the same thing.
    pub(crate) fn scroll_by(&mut self, delta: isize, count: usize) -> bool {
        let candidate = if delta < 0 {
            self.top.saturating_sub(delta.unsigned_abs())
        } else {
            self.top.saturating_add(delta.unsigned_abs())
        };
        self.scroll_to(candidate, count)
    }

    /// Put document line `line` at the top, if there is such a line.
    pub(crate) fn scroll_to(&mut self, line: usize, count: usize) -> bool {
        if line == self.top {
            return false;
        }
        let count = count.max(1);
        let Ok(rows) = self.file.display_lines(line, count) else {
            return false;
        };
        // Line 0 always exists in a file large enough to reach this viewer,
        // so an empty answer means the candidate is past the end.
        if rows.is_empty() && line > 0 {
            return false;
        }
        self.top = line;
        self.rows = rows;
        self.fetched_for = Some((line, count));
        true
    }
}

impl HugeView {
    /// Begin scanning this document for `query`, discarding any scan in
    /// flight.
    ///
    /// `O(1)`: an open handle and a compiled pattern, so the find bar may
    /// call this on every keystroke. Nothing is read until the first
    /// [`Self::advance_scan`].
    ///
    /// An empty query clears the scan rather than starting one, which is what
    /// an empty find box means. A pattern that is not a pattern is refused
    /// here -- once, before any of the file is read.
    pub(crate) fn begin_scan(&mut self, query: &Query) -> Result<(), StreamSearchError> {
        self.scan = None;
        if query.is_empty() {
            return Ok(());
        }
        self.scan = Some(Scan {
            search: StreamSearch::start(self.file.path(), query)?,
            index: 0,
            jumped: false,
        });
        Ok(())
    }

    /// Whether a scan is in flight and has more of the file to read.
    pub(crate) fn is_scanning(&self) -> bool {
        self.scan
            .as_ref()
            .is_some_and(|scan| !scan.search.is_finished())
    }

    /// Scan `windows` more windows, and report whether there is more to do.
    ///
    /// **A read failure ends the scan rather than pausing it**, and says so:
    /// the disk has stopped answering for a document that is only ever read
    /// from the disk, so there is nothing to resume. Returning `Ok(false)`
    /// after an error would be indistinguishable from finishing.
    pub(crate) fn advance_scan(
        &mut self,
        windows: usize,
        rows: usize,
    ) -> Result<bool, StreamSearchError> {
        // The borrow of `scan` has to end before the scroll, which needs all
        // of `self`.
        let stepped = {
            let Some(scan) = self.scan.as_mut() else {
                return Ok(false);
            };
            match scan.search.advance(windows) {
                Err(e) => Err(e),
                Ok(_) => {
                    let first = (!scan.jumped)
                        .then(|| scan.search.hits().first().map(|hit| hit.line))
                        .flatten();
                    if first.is_some() {
                        scan.jumped = true;
                    }
                    Ok((!scan.search.is_finished(), first))
                }
            }
        };
        match stepped {
            Err(e) => {
                self.scan = None;
                Err(e)
            }
            Ok((running, first)) => {
                if let Some(line) = first {
                    self.reveal(line, rows);
                }
                Ok(running)
            }
        }
    }

    /// Step to the next or previous hit, wrapping, and move the view to it.
    ///
    /// Returns whether there was a hit to step to. Wrapping is what a find
    /// box does, and it wraps over *what has been found so far*: a scan still
    /// running has an end that keeps moving, and refusing to wrap until it
    /// stops would make the button dead for as long as the file is long.
    pub(crate) fn step_hit(&mut self, forward: bool, rows: usize) -> bool {
        let line = {
            let Some(scan) = self.scan.as_mut() else {
                return false;
            };
            let len = scan.search.hits().len();
            if len == 0 {
                return false;
            }
            scan.index = if forward {
                (scan.index + 1) % len
            } else {
                (scan.index + len - 1) % len
            };
            scan.search.hits().get(scan.index).map(|hit| hit.line)
        };
        match line {
            Some(line) => {
                self.reveal(line, rows);
                true
            }
            None => false,
        }
    }

    /// Put `line` on screen, without moving if it already is.
    ///
    /// A third of a screen above it rather than at the top: a match at the
    /// very first row has no context above it, and context is most of what a
    /// reader of a log wants around a hit.
    fn reveal(&mut self, line: usize, rows: usize) {
        let rows = rows.max(1);
        if (self.top..self.top + self.drawn_or(rows)).contains(&line) {
            return;
        }
        self.scroll_to(line.saturating_sub(rows / 3), rows);
    }

    /// How many rows are actually drawn, for a containment test that must not
    /// claim the end of the file is on screen when it is not.
    fn drawn_or(&self, rows: usize) -> usize {
        match self.fetched_for {
            Some((top, _)) if top == self.top => self.rows.len(),
            _ => rows,
        }
    }

    /// The hit the reader is standing on, if a scan has found one.
    fn current_hit(&self) -> Option<&StreamHit> {
        let scan = self.scan.as_ref()?;
        scan.search.hits().get(scan.index)
    }

    /// Where to draw the highlight: a row of the viewport, and the visual
    /// columns of the match within it.
    ///
    /// Visual columns, because the surface draws tabs expanded and a
    /// character column would put the box somewhere the text is not -- the
    /// same trap the editor's caret fell into, and it goes through the same
    /// function.
    ///
    /// `None` when there is no scan, no hit, the hit is off screen, or the
    /// hit has no column at all -- which is the giant-line case
    /// `StreamHit::column` names. The view still scrolls to it; there is
    /// simply nothing to draw a box around.
    pub(crate) fn highlight(
        &mut self,
        rows: usize,
        tab_width: usize,
    ) -> Option<(usize, Range<usize>)> {
        let hit = self.current_hit()?.clone();
        let column = hit.column?;
        let row = hit.line.checked_sub(self.top)?;
        let line = self.rows(rows).get(row)?;
        let start = bp_editor::view::visual_column(&line.text, column.start, tab_width);
        let end = bp_editor::view::visual_column(&line.text, column.end, tab_width);
        Some((row, start..end))
    }

    /// What the find bar should say about this document's scan.
    ///
    /// `None` when there is no scan, so the caller leaves whatever the bar
    /// already said rather than blanking it.
    pub(crate) fn scan_status(&self) -> Option<String> {
        let scan = self.scan.as_ref()?;
        Some(scan_label(&scan.search, scan.index))
    }
}

/// What the find bar says about a scan of a document served from disk.
///
/// A free function for the same reason as [`viewer_label`]: its test asserts
/// the product's sentence rather than a copy of it.
///
/// **While the scan is running the count is explicitly "so far", and there is
/// no total.** `find_all` can say "1 of 27" the moment it is asked because it
/// has seen the whole document; this has not, and a number that grows while
/// it is labelled a total is a number that was never one. When the scan
/// finishes the wording becomes the in-memory find's exactly, so a reader
/// learns one vocabulary rather than two -- with a `+` when the scan stopped
/// at the cap, because "500 matches" and "at least 500 matches" are different
/// answers.
pub(crate) fn scan_label(search: &StreamSearch, index: usize) -> String {
    let found = search.hits().len();
    if !search.is_finished() {
        let percent = match search.len_bytes() {
            0 => 100,
            len => search.bytes_searched() * 100 / len,
        };
        return if found == 0 {
            format!("searching {percent}%")
        } else {
            format!("searching {percent}% ({found} so far)")
        };
    }
    if found == 0 {
        return "no matches".to_owned();
    }
    let more = if search.truncated() { "+" } else { "" };
    format!("{} of {found}{more}", index + 1)
}

/// What the status bar says about a document being served from disk.
///
/// A free function so its test asserts the *product's* sentence rather than a
/// copy of it, and so both halves come from somewhere that knows: the reason
/// is `Access`'s, and the position is this module's.
///
/// The position is a range rather than a total. A total would need the line
/// count, and the line count needs the whole file indexed — for a 2 GB
/// document that is a walk through 2 GB to put a number in a status bar,
/// which is the exact cost this viewer exists to avoid. Saying which lines are
/// on screen is true, cheap, and answers the question a reader of a huge log
/// actually has.
pub(crate) fn viewer_label(top: usize, drawn: usize) -> String {
    if drawn == 0 {
        return format!("Ln {}", top + 1);
    }
    format!("Ln {}-{}", top + 1, top + drawn)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file of `lines` numbered lines, past the huge threshold only if
    /// asked — most of these are about the paging arithmetic, which does not
    /// care how big the file is.
    fn a_file(lines: usize) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("huge.log");
        let mut text = String::new();
        for n in 1..=lines {
            text.push_str(&format!("line {n}\n"));
        }
        std::fs::write(&path, text).expect("the fixture is written");
        (dir, path)
    }

    #[test]
    fn a_view_starts_at_the_top_and_draws_what_it_was_asked_for() {
        let (_dir, path) = a_file(100);
        let mut view = HugeView::open(&path).expect("the fixture opens");

        assert_eq!(view.top(), 0);
        let rows = view.rows(10);
        assert_eq!(rows.len(), 10);
        assert_eq!(rows[0].text, "line 1");
        assert_eq!(
            rows[0].line, 0,
            "lines are 0-based, and the gutter adds one"
        );
        assert_eq!(rows[9].text, "line 10");
    }

    #[test]
    fn scrolling_moves_the_top_line_and_the_text_follows_it() {
        let (_dir, path) = a_file(100);
        let mut view = HugeView::open(&path).expect("the fixture opens");

        assert!(view.scroll_by(20, 10));
        assert_eq!(view.top(), 20);
        assert_eq!(view.rows(10)[0].text, "line 21");

        assert!(view.scroll_by(-5, 10));
        assert_eq!(view.top(), 15);
        assert_eq!(view.rows(10)[0].text, "line 16");
    }

    #[test]
    fn the_view_cannot_scroll_above_the_first_line() {
        // Saturating rather than wrapping, and asserted rather than assumed:
        // an unsigned top and a signed delta is where a scroll to line
        // 18,446,744,073,709,551,615 comes from.
        let (_dir, path) = a_file(100);
        let mut view = HugeView::open(&path).expect("the fixture opens");

        assert!(!view.scroll_by(-1, 10), "already at the top");
        assert_eq!(view.top(), 0);

        assert!(view.scroll_by(3, 10));
        assert!(view.scroll_by(isize::MIN, 10), "a huge step up lands at 0");
        assert_eq!(view.top(), 0);
    }

    #[test]
    fn the_view_cannot_scroll_past_the_end_of_the_file() {
        // The bound that is found by asking rather than by counting. There is
        // no line 500, so the view stays where it was rather than showing a
        // blank screen and a gutter numbering nothing.
        let (_dir, path) = a_file(100);
        let mut view = HugeView::open(&path).expect("the fixture opens");

        assert!(view.scroll_by(50, 10));
        assert_eq!(view.top(), 50);

        assert!(!view.scroll_by(450, 10), "line 500 does not exist");
        assert_eq!(view.top(), 50, "a refused scroll must not move the view");
        assert_eq!(
            view.rows(10)[0].text,
            "line 51",
            "and must not disturb what is drawn"
        );
    }

    #[test]
    fn a_partial_screen_at_the_end_is_drawn_rather_than_refused() {
        // The distinction the bound above rests on: *no* lines means past the
        // end, *fewer* lines means the end is on screen. Collapsing the two
        // would make the last screenful unreachable.
        let (_dir, path) = a_file(100);
        let mut view = HugeView::open(&path).expect("the fixture opens");

        assert!(view.scroll_by(95, 10));
        let rows = view.rows(10);
        assert!(
            (1..10).contains(&rows.len()),
            "expected a partial screen, got {} rows",
            rows.len()
        );
        assert_eq!(rows[0].text, "line 96");
    }

    #[test]
    fn a_resize_refetches_and_a_redraw_does_not() {
        // The cache is keyed by (top, count) because `refresh` runs on every
        // keystroke and on a 1.2 s poll. Asking twice for the same screen must
        // not read the disk twice; asking for a taller one must.
        let (_dir, path) = a_file(100);
        let mut view = HugeView::open(&path).expect("the fixture opens");

        assert_eq!(view.rows(10).len(), 10);
        let fetched = view.fetched_for;
        assert_eq!(view.rows(10).len(), 10);
        assert_eq!(view.fetched_for, fetched, "a redraw refetched");

        assert_eq!(view.rows(20).len(), 20);
        assert_eq!(
            view.fetched_for,
            Some((0, 20)),
            "a resize must refetch, or the new rows are the old ones"
        );
    }

    #[test]
    fn the_readout_names_the_lines_on_screen_and_not_a_total() {
        // A total needs the line count, and the line count needs the whole
        // file indexed -- which for a 2 GB document is the walk this viewer
        // exists to avoid.
        assert_eq!(viewer_label(0, 48), "Ln 1-48");
        assert_eq!(viewer_label(1_000, 48), "Ln 1001-1048");
        assert_eq!(
            viewer_label(7, 0),
            "Ln 8",
            "an empty screen still has a top"
        );
        assert!(
            !viewer_label(0, 48).contains(" of "),
            "a total would be a promise this viewer cannot keep cheaply"
        );
    }

    /// A file whose lines are given, one per line.
    fn a_file_of(lines: &[&str]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("huge.log");
        let mut text = String::new();
        for line in lines {
            text.push_str(line);
            text.push('\n');
        }
        std::fs::write(&path, text).expect("the fixture is written");
        (dir, path)
    }

    /// Open a view and scan it to the end.
    fn scanned(path: &std::path::Path, pattern: &str, rows: usize) -> HugeView {
        let mut view = HugeView::open(path).expect("the fixture opens");
        view.begin_scan(&Query::literal(pattern))
            .expect("the scan starts");
        while view.advance_scan(64, rows).expect("the scan runs") {}
        view
    }

    #[test]
    fn the_first_hit_a_scan_finds_brings_the_view_to_it() {
        // The live preview a find box does as a query is typed, and the only
        // automatic movement: everything after this is the reader's to step
        // to.
        let (_dir, path) = a_file(400);
        let mut view = HugeView::open(&path).expect("the fixture opens");
        assert_eq!(view.top(), 0);

        view.begin_scan(&Query::literal("line 300"))
            .expect("the scan starts");
        while view.advance_scan(64, 10).expect("the scan runs") {}

        assert!(
            (view.top()..view.top() + 10).contains(&299),
            "line 300 is 0-based line 299, and it must be on screen: top {}",
            view.top()
        );
    }

    #[test]
    fn a_hit_already_on_screen_does_not_move_the_view() {
        // Scrolling to something the reader can already see is movement they
        // did not ask for, and in a log viewer it loses their place.
        let (_dir, path) = a_file(400);
        let mut view = HugeView::open(&path).expect("the fixture opens");
        let _ = view.rows(10);

        view.begin_scan(&Query::literal("line 3"))
            .expect("the scan starts");
        while view.advance_scan(64, 10).expect("the scan runs") {}

        assert_eq!(view.top(), 0, "the first hit is on line 3, already drawn");
    }

    #[test]
    fn stepping_walks_the_hits_and_wraps_in_both_directions() {
        let (_dir, path) = a_file_of(&["needle a", "no", "needle b", "no", "needle c"]);
        let mut view = scanned(&path, "needle", 2);

        // The scan's own jump landed on the first hit.
        assert_eq!(view.scan_status().as_deref(), Some("1 of 3"));

        assert!(view.step_hit(true, 2));
        assert_eq!(view.scan_status().as_deref(), Some("2 of 3"));
        assert!(view.step_hit(true, 2));
        assert_eq!(view.scan_status().as_deref(), Some("3 of 3"));
        assert!(view.step_hit(true, 2), "forward from the last wraps");
        assert_eq!(view.scan_status().as_deref(), Some("1 of 3"));
        assert!(view.step_hit(false, 2), "back from the first wraps");
        assert_eq!(view.scan_status().as_deref(), Some("3 of 3"));
    }

    #[test]
    fn stepping_moves_the_view_to_the_hit() {
        // "line 39" matches line 39 and lines 390-399, which is two clusters
        // far enough apart that stepping between them has to move the view.
        // A pattern whose hits share a screen would pass this without the
        // scroll ever happening.
        let (_dir, path) = a_file(400);
        let mut view = scanned(&path, "line 39", 10);
        let first = view.top();
        assert_ne!(first, 0, "the scan's own jump moved the view to line 39");

        assert!(view.step_hit(true, 10));
        assert_ne!(view.top(), first, "the view followed the step");
    }

    #[test]
    fn stepping_with_nothing_found_does_nothing_rather_than_panicking() {
        // A find box whose query matches nothing still has two arrows on it.
        let (_dir, path) = a_file(50);
        let mut view = scanned(&path, "absent", 10);

        assert!(!view.step_hit(true, 10));
        assert!(!view.step_hit(false, 10));
        assert_eq!(view.top(), 0);
    }

    #[test]
    fn an_empty_query_clears_the_scan_rather_than_starting_one() {
        // What an empty find box means, and it must also clear what the last
        // one found -- a stale highlight over a document nobody is searching
        // is worse than none.
        let (_dir, path) = a_file(50);
        let mut view = scanned(&path, "line", 10);
        assert!(view.scan_status().is_some());

        view.begin_scan(&Query::default()).expect("an empty query");

        assert_eq!(view.scan_status(), None);
        assert!(!view.is_scanning());
        assert_eq!(view.highlight(10, 4), None);
    }

    #[test]
    fn a_bad_pattern_is_refused_before_the_file_is_read() {
        let (_dir, path) = a_file(50);
        let mut view = HugeView::open(&path).expect("the fixture opens");

        let query = Query {
            pattern: "a(b".to_owned(),
            regex: true,
            ..Query::default()
        };
        assert!(view.begin_scan(&query).is_err());
        assert!(!view.is_scanning(), "a refused scan must not look started");
    }

    #[test]
    fn the_highlight_is_the_row_on_screen_and_the_visual_columns_in_it() {
        // Visual columns, not character ones: the surface draws tabs
        // expanded, so a character column would put the box where the text
        // is not. This is the trap the editor's caret fell into, and the
        // highlight goes through the same function it does.
        let (_dir, path) = a_file_of(&["first", "a\tneedle", "third"]);
        let mut view = scanned(&path, "needle", 5);

        assert_eq!(
            view.highlight(5, 4),
            Some((1, 4..10)),
            "row 1 of the viewport; the tab reaches column 4, then six characters"
        );
        assert_eq!(
            view.highlight(5, 8),
            Some((1, 8..14)),
            "a wider tab moves the box with the text"
        );
    }

    #[test]
    fn a_hit_that_is_not_on_screen_is_not_highlighted() {
        // The reader has scrolled away from the hit. Drawing a box at the
        // arithmetic's answer would put it on an unrelated line.
        let (_dir, path) = a_file(400);
        let mut view = scanned(&path, "line 2", 10);
        assert!(view.highlight(10, 4).is_some());

        view.scroll_to(300, 10);

        assert_eq!(view.highlight(10, 4), None);
    }

    #[test]
    fn a_view_with_no_scan_highlights_nothing() {
        let (_dir, path) = a_file(50);
        let mut view = HugeView::open(&path).expect("the fixture opens");

        assert_eq!(view.highlight(10, 4), None);
        assert_eq!(view.scan_status(), None);
        assert!(!view.is_scanning());
    }

    #[test]
    fn a_scan_in_progress_says_so_and_does_not_claim_a_total() {
        // `find_all` can say "1 of 27" at once because it has seen the whole
        // document. This has not, and a number that grows while it is
        // labelled a total is a number that was never one.
        // Two things the fixture has to be, and both are easy to get wrong:
        // more than one window long, so a single tick cannot finish it; and
        // matched by fewer than `MAX_HITS` hits in that first window, or the
        // scan stops at the cap instead and is finished after all.
        let (_dir, path) = a_file(150_000);
        let mut view = HugeView::open(&path).expect("the fixture opens");
        view.begin_scan(&Query::literal("line 199"))
            .expect("the scan starts");

        assert!(view.advance_scan(1, 10).expect("one window"));
        let status = view.scan_status().expect("a scan says something");

        assert!(
            status.starts_with("searching "),
            "expected progress, got {status:?}"
        );
        assert!(
            status.contains("so far"),
            "a running count must be labelled as one, got {status:?}"
        );
        assert!(
            !status.contains(" of "),
            "a total is a promise a running scan cannot keep: {status:?}"
        );
    }

    #[test]
    fn the_readout_finishes_in_the_same_words_the_in_memory_find_uses() {
        // One vocabulary rather than two. `AppState::find` says "1 of 3" and
        // "no matches"; a scan that finished saying something else would make
        // the same bar mean different things on different documents.
        let (_dir, path) = a_file_of(&["needle", "no", "needle"]);
        assert_eq!(
            scanned(&path, "needle", 5).scan_status().as_deref(),
            Some("1 of 2")
        );
        assert_eq!(
            scanned(&path, "absent", 5).scan_status().as_deref(),
            Some("no matches")
        );
    }

    #[test]
    fn a_capped_scan_says_there_are_more_rather_than_a_wrong_total() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("many.log");
        let mut text = String::new();
        for _ in 0..bp_search::MAX_HITS + 50 {
            text.push_str("needle\n");
        }
        std::fs::write(&path, text).expect("the fixture is written");

        let view = scanned(&path, "needle", 10);

        let status = view.scan_status().expect("a scan says something");
        assert!(
            status.ends_with('+'),
            "a capped total must be marked as one, got {status:?}"
        );
        assert!(status.starts_with("1 of "));
    }

    #[test]
    fn a_view_says_why_it_is_read_only_in_bp_buffers_words() {
        // Not this module's sentence. `Access::ReadOnlyBySize` exists so a
        // refusal can say *how* large and distinguish itself from a file that
        // is read-only on disk, and re-wording it here would lose both.
        let (_dir, path) = a_file(10);
        let view = HugeView::open(&path).expect("the fixture opens");

        // The fixture is small, so this is `Editable` -- which is the point:
        // the access comes from the file's own size and not from the fact
        // that a `HugeView` was built over it.
        assert_eq!(
            view.access(),
            bp_buffer::Access::of(view.len_bytes(), false)
        );
    }
}
