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
//! ## ADR-0030
//!
//! This is drawn by `EditorSurface` in every build, whether or not
//! `--editor-view` was passed. `TextInput` owns its own text and there is no
//! way to hand it a window of a file it does not have; the flag stays a
//! statement about which surface *edits* a document the rope holds, and the
//! one reason it is opt-in — input-method composition — has nothing to say
//! about a surface that accepts no text.

use std::path::Path;

use bp_buffer::{Access, DisplayLine, LargeFile, LargeFileError};

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
