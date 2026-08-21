//! Searching a document too large to hold, a window at a time.
//!
//! ADR-0042. `bp_buffer::stream` produces overlapping `StreamWindow`s that
//! between them cover a file exactly once; this crate already knows what a
//! match is. What lives here is the join, and the three things neither crate
//! could decide alone.
//!
//! ## It is resumable, and that is what makes it interruptible
//!
//! [`StreamSearch::advance`] scans at most the number of windows it is given
//! and returns. Nothing here spawns a thread, holds a channel, or watches a
//! cancellation flag: **a search is cancelled by not asking it to continue**,
//! and dropping it closes the file handle. A caller driving this from a UI
//! timer keeps its window responsive by choosing a small budget, and a caller
//! in a test scans the whole file in one call by choosing a large one.
//!
//! The budget is counted in windows rather than in milliseconds on purpose. A
//! time budget makes the amount of work depend on the machine, which makes a
//! test of it either flaky or a tautology.
//!
//! ## A hit is a line, and a column in that line
//!
//! The viewer this exists for scrolls by line and draws by line -- it has no
//! caret, because a caret is a position in a rope and there is no rope. A
//! character offset into the document would be a number nothing could use,
//! and computing one means walking every byte before it, which is the cost
//! the whole viewer exists to avoid.
//!
//! ## Two boundary rules, both inherited rather than reinvented
//!
//! - **A match seen twice.** Windows repeat their predecessor's tail so a
//!   match straddling a boundary is still found, which means a match inside
//!   that repeat may be reported twice. [`StreamWindow::is_new_match`] states
//!   the rule -- *end* past the overlap, not *start* -- and this module does
//!   what it says rather than re-deriving it.
//! - **A window that begins mid-line.** Possible only when a single line is
//!   longer than the window, which is a minified bundle or a one-line dump --
//!   and those are exactly the files that reach this code. The line *number*
//!   survives it (`first_line` is the line the window's first byte falls on),
//!   the column does not, and [`StreamHit::column`] is `None` rather than a
//!   guess.

use std::ops::Range;
use std::path::Path;
use std::time::Duration;

use bp_buffer::{LargeFileError, StreamWindow, WindowReader};
use regex::Regex;
use thiserror::Error;

use crate::{MAX_HITS, Query, SearchError, matches_with};

/// The caps and window sizes a scan runs under.
///
/// One struct rather than three constructors, and the window knobs are here
/// rather than hidden because [`Self::overlap_bytes`] is the one thing
/// streaming can lose -- a match longer than it can fall between two windows.
/// A caller searching for something longer than 4 KiB has to be able to say
/// so, and `bp-buffer` clamps whatever it is given rather than refusing it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamLimits {
    /// Stop after this many hits, and report [`StreamSearch::truncated`].
    pub max_hits: usize,
    /// Bytes of file text handed to the matcher at a time.
    pub window_bytes: usize,
    /// Minimum bytes each window repeats of the previous one.
    pub overlap_bytes: usize,
}

impl Default for StreamLimits {
    fn default() -> Self {
        Self {
            max_hits: MAX_HITS,
            window_bytes: bp_buffer::DEFAULT_WINDOW_BYTES,
            overlap_bytes: bp_buffer::DEFAULT_OVERLAP_BYTES,
        }
    }
}

/// What can go wrong scanning a file for a pattern.
///
/// Its own type rather than a variant on [`SearchError`], which derives
/// `PartialEq`: [`LargeFileError`] carries an `std::io::Error`, so widening
/// `SearchError` to hold one would cost every existing caller the ability to
/// compare two errors.
#[derive(Debug, Error)]
pub enum StreamSearchError {
    /// The pattern is not a pattern. Raised by [`StreamSearch::start`],
    /// before any of the file is read.
    #[error(transparent)]
    Pattern(#[from] SearchError),
    /// The file could not be read, or is not UTF-8. Carries the path and,
    /// for bad bytes, the offset.
    #[error(transparent)]
    Read(#[from] LargeFileError),
}

/// One match, placed where a viewer can go to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamHit {
    /// 0-based line of the file the match starts on, matching
    /// `bp_buffer::DisplayLine::line`. Add one for display.
    pub line: usize,
    /// Character range of the match **within that line**, when the start of
    /// the line was inside the window that found the match.
    ///
    /// `None` when it was not -- see the module documentation. A caller that
    /// wants to highlight the match has nothing to highlight; a caller that
    /// wants to scroll to it still has [`Self::line`].
    pub column: Option<Range<usize>>,
}

/// A scan of one file for one pattern, advanced in bounded steps.
///
/// Created by [`Self::start`], driven by [`Self::advance`], and finished when
/// [`Self::is_finished`] says so -- which happens at the end of the file or
/// at [`MAX_HITS`], whichever comes first.
#[derive(Debug)]
pub struct StreamSearch {
    reader: WindowReader,
    /// Compiled once. ADR-0042: per window would be a couple of thousand
    /// compilations of one pattern over a 2 GB file.
    regex: Option<Regex>,
    max_hits: usize,
    hits: Vec<StreamHit>,
    truncated: bool,
    finished: bool,
    windows_scanned: usize,
}

impl StreamSearch {
    /// Open `path` and prepare to scan it for `query`.
    ///
    /// `O(1)`: an open handle and a compiled pattern. No part of the file is
    /// read here, so a find box can start a search on every keystroke without
    /// touching the disk until the first [`Self::advance`].
    ///
    /// **A bad pattern fails here**, which is where a find box wants it --
    /// once, before any reading, rather than on the two thousandth window.
    /// An empty pattern is not an error: it starts a search that is already
    /// finished and found nothing, which is what an empty find box means.
    pub fn start(path: &Path, query: &Query) -> Result<Self, StreamSearchError> {
        Self::start_with(path, query, StreamLimits::default())
    }

    /// Start under chosen [`StreamLimits`].
    ///
    /// `max_hits` is clamped up to one: a cap of zero would be a search that
    /// can never report anything, which is a caller's mistake rather than a
    /// mode worth having. The window and the overlap are clamped by
    /// `bp-buffer`, which owns what those numbers may be.
    pub fn start_with(
        path: &Path,
        query: &Query,
        limits: StreamLimits,
    ) -> Result<Self, StreamSearchError> {
        // Compiled before the file is opened, so a bad pattern costs no I/O
        // at all.
        let regex = if query.is_empty() {
            None
        } else {
            Some(query.compile()?)
        };
        let reader = WindowReader::with_config(path, limits.window_bytes, limits.overlap_bytes)?;
        Ok(Self {
            reader,
            finished: regex.is_none(),
            regex,
            max_hits: limits.max_hits.max(1),
            hits: Vec::new(),
            truncated: false,
            windows_scanned: 0,
        })
    }

    /// Scan at most `windows` more windows.
    ///
    /// Returns the number actually scanned, which is fewer than asked for at
    /// the end of the file or at the hit cap, and zero once
    /// [`Self::is_finished`]. A budget of zero is a no-op rather than an
    /// error, so a caller ticking a timer does not have to special-case it.
    ///
    /// **A read failure ends the search rather than pausing it.** The disk has
    /// stopped answering for a document that is only ever read from the disk;
    /// there is nothing to resume.
    pub fn advance(&mut self, windows: usize) -> Result<usize, StreamSearchError> {
        let mut scanned = 0;
        while scanned < windows && !self.finished {
            let window = match self.reader.next_window() {
                Ok(Some(window)) => window,
                Ok(None) => {
                    self.finished = true;
                    break;
                }
                Err(e) => {
                    self.finished = true;
                    return Err(e.into());
                }
            };
            scanned += 1;
            self.windows_scanned += 1;
            self.take_hits(&window);
        }
        Ok(scanned)
    }

    /// Scan to the end, or to the cap. For tests and for callers with no
    /// window to keep responsive.
    pub fn run_to_completion(&mut self) -> Result<(), StreamSearchError> {
        while !self.finished {
            self.advance(usize::MAX)?;
        }
        Ok(())
    }

    /// Collect this window's matches, in file order.
    fn take_hits(&mut self, window: &StreamWindow) {
        let Some(regex) = &self.regex else { return };
        let found = matches_with(&window.text, regex);
        if found.is_empty() {
            return;
        }
        // **Once per window, not once per match.** A window of a log file can
        // hold tens of thousands of matches, and finding each one's line by
        // walking back to the previous newline is quadratic in the window --
        // which costs nothing on a small fixture and stops a 1 MiB one dead.
        let line_starts = line_start_chars(&window.text);

        for m in found {
            // The rule the overlap exists for, and it is the reader's rule
            // rather than one written twice: a match wholly inside the
            // repeated prefix was already reported against the previous
            // window.
            if !window.is_new_match(&m.range) {
                continue;
            }
            if self.hits.len() == self.max_hits {
                self.truncated = true;
                self.finished = true;
                return;
            }
            self.hits.push(StreamHit {
                line: window.absolute_line(m.line),
                column: column_in_line(window, &line_starts, &m),
            });
        }
    }

    /// The matches found so far, in file order.
    pub fn hits(&self) -> &[StreamHit] {
        &self.hits
    }

    /// Whether the scan has reached the end of the file or the hit cap.
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// Whether the scan stopped at the cap rather than at the end.
    ///
    /// Surfaced rather than swallowed, for the reason
    /// [`crate::FileSearchReport::truncated`] gives: "500 matches" and "at
    /// least 500 matches" mean different things to someone deciding whether
    /// to refine a query.
    pub fn truncated(&self) -> bool {
        self.truncated
    }

    /// Bytes of the file scanned so far. Rises to [`Self::len_bytes`] unless
    /// the scan stops at the cap.
    pub fn bytes_searched(&self) -> u64 {
        self.reader.bytes_delivered()
    }

    /// Size of the file when the scan started.
    pub fn len_bytes(&self) -> u64 {
        self.reader.len_bytes()
    }

    /// Windows scanned so far, for a caller budgeting its own ticks.
    pub fn windows_scanned(&self) -> usize {
        self.windows_scanned
    }

    /// The longest match this scan is guaranteed to find across a window
    /// boundary, in bytes.
    ///
    /// Worth exposing for the same reason `bp-buffer` exposes it: it is the
    /// one thing streaming can lose, and a caller searching for something
    /// longer should know rather than discover.
    pub fn guaranteed_match_bytes(&self) -> usize {
        self.reader.overlap_bytes()
    }
}

/// A budget that keeps a window responsive.
///
/// Not used by this crate -- it is the number a UI driving [`StreamSearch`]
/// wants, and it lives beside the thing it is about rather than as a constant
/// in a shell. One window is 1 MiB, so this is ~4 MiB of scanning between
/// redraws: enough to finish a 192 MiB log in about fifty ticks, and small
/// enough that no single call reads more than a moment's worth of disk.
pub const WINDOWS_PER_TICK: usize = 4;

/// How often a caller should tick [`StreamSearch::advance`].
///
/// Here for the same reason as [`WINDOWS_PER_TICK`], and paired with it: the
/// two together are the scan rate, and separating them across two crates is
/// how one gets changed without the other.
pub const STREAM_TICK: Duration = Duration::from_millis(16);

/// Character offset at which each line of `text` begins.
///
/// Characters and not bytes: a match's range is in characters, and
/// subtracting a byte offset from it would put a highlight several places to
/// the right on any line holding a multi-byte character before the match.
fn line_start_chars(text: &str) -> Vec<usize> {
    let mut starts = vec![0usize];
    for (index, ch) in text.chars().enumerate() {
        if ch == '\n' {
            starts.push(index + 1);
        }
    }
    starts
}

/// The character range of a match within its own line.
///
/// `m.line` is 1-based within the window, which is an *index* into
/// `line_starts` once one is subtracted -- so this is a lookup rather than a
/// search, and the window is walked once for all of its matches rather than
/// once for each.
///
/// `None` for a match on the window's first line when that line began before
/// the window did. That is the case `starts_mid_line` names: the line's start
/// was never in this window, so its column cannot be computed from one, and
/// the two dishonest alternatives are to guess a column or to drop the hit.
fn column_in_line(
    window: &StreamWindow,
    line_starts: &[usize],
    m: &crate::Match,
) -> Option<Range<usize>> {
    if m.line == 1 && window.starts_mid_line {
        return None;
    }
    let start = *line_starts.get(m.line.checked_sub(1)?)?;
    Some((m.range.start.checked_sub(start)?)..(m.range.end.checked_sub(start)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Windows small enough that a fixture of a few tens of kilobytes spans
    /// many of them.
    ///
    /// Every boundary property here -- the dedupe, the line arithmetic, the
    /// mid-line column -- is about what happens *between* windows, so a test
    /// using the 1 MiB default would need a megabyte of fixture to assert
    /// anything at all, and would assert it once. `bp-buffer` clamps the
    /// window up to `MIN_WINDOW_BYTES`, so this is the smallest real one.
    fn small() -> StreamLimits {
        StreamLimits {
            max_hits: usize::MAX,
            window_bytes: bp_buffer::MIN_WINDOW_BYTES,
            overlap_bytes: 64,
        }
    }

    fn a_file(text: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("served.log");
        std::fs::write(&path, text).expect("the fixture is written");
        (dir, path)
    }

    fn numbered(lines: usize) -> String {
        let mut text = String::new();
        for n in 1..=lines {
            text.push_str(&format!("line {n}\n"));
        }
        text
    }

    /// Run a scan to the end and return what it found.
    fn scan_with(path: &std::path::Path, query: &Query, limits: StreamLimits) -> StreamSearch {
        let mut search = StreamSearch::start_with(path, query, limits).expect("the scan starts");
        search.run_to_completion().expect("the scan finishes");
        search
    }

    fn hits(path: &std::path::Path, pattern: &str) -> Vec<StreamHit> {
        scan_with(path, &Query::literal(pattern), StreamLimits::default())
            .hits()
            .to_vec()
    }

    #[test]
    fn a_hit_is_the_documents_own_line_number_and_the_column_within_it() {
        let (_dir, path) = a_file("alpha\nbeta needle gamma\ndelta\n");

        let found = hits(&path, "needle");

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 1, "0-based, as DisplayLine::line is");
        assert_eq!(
            found[0].column,
            Some(5..11),
            "characters within the line, not within the file or the window"
        );
    }

    #[test]
    fn a_match_on_the_first_line_has_a_column_and_not_a_missing_one() {
        // The two ways a match can have no newline before it are the first
        // line of the file and a window cut inside a giant line. Confusing
        // them would lose the column of every hit on line 1 of every
        // document.
        let (_dir, path) = a_file("needle here\nsecond\n");

        let found = hits(&path, "needle");

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 0);
        assert_eq!(found[0].column, Some(0..6));
    }

    #[test]
    fn columns_are_counted_in_characters_and_not_in_bytes() {
        // A byte column would put a highlight several places to the right of
        // the match on any line with non-ASCII before it, and every
        // character in this prefix is three bytes.
        let (_dir, path) = a_file("\u{65e5}\u{672c}\u{8a9e} needle\n");

        let found = hits(&path, "needle");

        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].column,
            Some(4..10),
            "four characters precede it, not twelve bytes"
        );
    }

    #[test]
    fn every_match_in_a_multi_window_file_is_found_exactly_once() {
        // The property the overlap exists for, from both sides at once: a
        // boundary defect shows up either as a lost hit or as a doubled one,
        // and an exact count catches both.
        let mut text = String::new();
        for n in 0..2_000 {
            text.push_str(&format!("line {n} padding padding padding\n"));
        }
        let wanted = text.matches("padding").count();
        let (_dir, path) = a_file(&text);

        let search = scan_with(&path, &Query::literal("padding"), small());

        assert!(
            search.windows_scanned() > 5,
            "the fixture must span several windows or this asserts nothing: {} scanned",
            search.windows_scanned()
        );
        assert_eq!(search.hits().len(), wanted);
    }

    #[test]
    fn a_line_number_stays_right_across_every_window_boundary() {
        // The arithmetic that goes wrong quietly. Each line contains its own
        // number, so a hit's reported line can be checked against the text
        // rather than against a count kept alongside it -- an off-by-one at
        // the first boundary would pass a count-only assertion.
        let mut text = String::new();
        for n in 0..2_000 {
            text.push_str(&format!("row {n} marker padding padding\n"));
        }
        let (_dir, path) = a_file(&text);

        let search = scan_with(&path, &Query::literal("marker"), small());

        assert_eq!(search.hits().len(), 2_000);
        for (expected, hit) in search.hits().iter().enumerate() {
            assert_eq!(hit.line, expected, "line drifted at hit {expected}");
            let digits = expected.to_string().len();
            assert_eq!(
                hit.column,
                Some((4 + digits + 1)..(4 + digits + 1 + 6)),
                "column drifted at hit {expected}"
            );
        }
    }

    #[test]
    fn a_match_straddling_a_window_boundary_is_still_found_and_not_twice() {
        // Windows end on line breaks wherever the text allows, so the only
        // way a match straddles one is a line longer than half a window --
        // which is the minified bundle and the one-line dump this viewer
        // exists for. Needles every hundred characters of one long line put
        // several of them across boundaries without this test having to know
        // where the boundaries are.
        let mut line = String::new();
        for _ in 0..400 {
            line.push_str(&"x".repeat(94));
            line.push_str("needle");
        }
        let (_dir, path) = a_file(&format!("first\n{line}\nlast\n"));

        let search = scan_with(&path, &Query::literal("needle"), small());

        assert!(search.windows_scanned() > 5, "the line must span windows");
        assert_eq!(
            search.hits().len(),
            400,
            "no needle lost to a boundary and none reported twice"
        );
        assert!(
            search.hits().iter().all(|h| h.line == 1),
            "every one of them is on the same line of the file"
        );
    }

    #[test]
    fn hits_arrive_in_file_order() {
        let mut text = String::new();
        for n in 0..2_000 {
            text.push_str(&format!("line {n} needle\n"));
        }
        let (_dir, path) = a_file(&text);

        let search = scan_with(&path, &Query::literal("needle"), small());

        let lines: Vec<usize> = search.hits().iter().map(|h| h.line).collect();
        assert!(
            lines.windows(2).all(|pair| pair[0] < pair[1]),
            "a scan that reads windows in order must report hits in order"
        );
        assert_eq!(lines.first(), Some(&0));
    }

    #[test]
    fn a_scan_advances_only_as_far_as_its_budget() {
        // The property the whole design rests on: a call does bounded work
        // and comes back, so whatever called it still owns its thread.
        let (_dir, path) = a_file(&numbered(4_000));

        let mut search =
            StreamSearch::start_with(&path, &Query::literal("line"), small()).expect("it starts");

        assert_eq!(search.advance(1).expect("one window"), 1);
        assert_eq!(search.windows_scanned(), 1);
        assert!(!search.is_finished(), "one window is not the whole file");
        let after_one = search.bytes_searched();
        assert!(after_one > 0);
        assert!(
            after_one < search.len_bytes(),
            "a budgeted call must not have read the file"
        );

        assert_eq!(search.advance(2).expect("two more"), 2);
        assert_eq!(search.windows_scanned(), 3);
        assert!(search.bytes_searched() > after_one, "the scan moved on");
    }

    #[test]
    fn a_budget_of_zero_does_nothing_and_is_not_an_error() {
        // A caller ticking a timer should not have to special-case it.
        let (_dir, path) = a_file(&numbered(10));
        let mut search =
            StreamSearch::start(&path, &Query::literal("line")).expect("the scan starts");

        assert_eq!(search.advance(0).expect("a no-op"), 0);
        assert_eq!(search.windows_scanned(), 0);
        assert!(!search.is_finished());
    }

    #[test]
    fn advancing_a_finished_scan_scans_nothing() {
        let (_dir, path) = a_file(&numbered(10));
        let mut search = scan_with(&path, &Query::literal("line"), StreamLimits::default());

        assert!(search.is_finished());
        assert_eq!(search.advance(100).expect("nothing left"), 0);
    }

    #[test]
    fn a_scan_stops_at_the_cap_and_says_it_did() {
        // "5 matches" and "at least 5 matches" are different answers, and a
        // capped scan must not keep reading to build a list nobody asked for.
        let (_dir, path) = a_file(&numbered(4_000));

        let limits = StreamLimits {
            max_hits: 5,
            ..small()
        };
        let search = scan_with(&path, &Query::literal("line"), limits);

        assert_eq!(search.hits().len(), 5);
        assert!(search.truncated(), "the cap must be reported, not hidden");
        assert!(search.is_finished());
        assert!(
            search.bytes_searched() < search.len_bytes(),
            "a capped scan stops reading rather than finishing the file"
        );
    }

    #[test]
    fn a_scan_that_reaches_the_end_is_finished_and_not_truncated() {
        let (_dir, path) = a_file(&numbered(10));

        let search = scan_with(&path, &Query::literal("line"), StreamLimits::default());

        assert_eq!(search.hits().len(), 10);
        assert!(search.is_finished());
        assert!(!search.truncated());
        assert_eq!(
            search.bytes_searched(),
            search.len_bytes(),
            "a completed scan has seen the whole file"
        );
    }

    #[test]
    fn an_empty_pattern_finds_nothing_rather_than_everything() {
        // What an empty find box means. Matching at every position would
        // report a hit per character of a two-gigabyte file.
        let (_dir, path) = a_file(&numbered(10));

        let search = scan_with(&path, &Query::default(), StreamLimits::default());

        assert!(search.hits().is_empty());
        assert!(search.is_finished(), "there is nothing to look for");
        assert_eq!(
            search.bytes_searched(),
            0,
            "and nothing to read to find out"
        );
    }

    #[test]
    fn a_bad_pattern_fails_at_the_start_and_reads_nothing() {
        // Where a find box wants the error: once, before the disk is touched,
        // rather than on the two thousandth window of a 2 GB file.
        let (_dir, path) = a_file(&numbered(10));

        let query = Query {
            pattern: "a(b".to_owned(),
            regex: true,
            ..Query::default()
        };
        let error = StreamSearch::start(&path, &query).expect_err("a bad pattern is refused");

        assert!(
            matches!(error, StreamSearchError::Pattern(_)),
            "got {error:?}"
        );
    }

    #[test]
    fn a_file_that_is_not_there_fails_at_the_start() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let error = StreamSearch::start(&dir.path().join("absent.log"), &Query::literal("x"))
            .expect_err("a missing file is refused");

        assert!(matches!(error, StreamSearchError::Read(_)), "got {error:?}");
    }

    #[test]
    fn bytes_that_are_not_text_end_the_scan_as_an_error() {
        // A scan reading a file it cannot decode must fail rather than
        // silently stop, which would look exactly like "no more matches".
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("binary.log");
        std::fs::write(&path, [b'a', b'\n', 0xff, 0xfe, b'\n']).expect("the fixture is written");

        let mut search = StreamSearch::start(&path, &Query::literal("a")).expect("it starts");
        let error = search.advance(10).expect_err("bad bytes are refused");

        assert!(matches!(error, StreamSearchError::Read(_)), "got {error:?}");
        assert!(
            search.is_finished(),
            "a scan that failed must not look resumable"
        );
    }

    #[test]
    fn the_query_options_reach_the_scan() {
        // The scan compiles the query itself rather than calling `find_all`,
        // so every option has to be checked here too: a streaming search that
        // quietly ignored the case toggle is exactly the defect `find_query`
        // exists to prevent one level up.
        let (_dir, path) = a_file("Needle\nneedle\nneedles\n");

        assert_eq!(
            hits(&path, "needle").len(),
            3,
            "case-insensitive by default"
        );

        let sensitive = Query {
            pattern: "needle".to_owned(),
            case_sensitive: true,
            ..Query::default()
        };
        assert_eq!(
            scan_with(&path, &sensitive, StreamLimits::default())
                .hits()
                .len(),
            2,
            "case-sensitive skips `Needle`"
        );

        let whole = Query {
            pattern: "needle".to_owned(),
            whole_word: true,
            ..Query::default()
        };
        assert_eq!(
            scan_with(&path, &whole, StreamLimits::default())
                .hits()
                .len(),
            2,
            "whole-word skips `needles`"
        );

        let expression = Query {
            pattern: "n..dle".to_owned(),
            regex: true,
            ..Query::default()
        };
        assert_eq!(
            scan_with(&path, &expression, StreamLimits::default())
                .hits()
                .len(),
            3,
            "the regex toggle is what stops the pattern being escaped"
        );
    }

    #[test]
    fn a_streaming_scan_agrees_with_find_all_about_what_a_match_is() {
        // The property that keeps two search paths from drifting. Both go
        // through `matches_with`, and this is what says so from outside.
        let mut text = String::new();
        for n in 0..1_500 {
            text.push_str(&format!("line {n}: alpha beta ALPHA\n"));
        }
        let (_dir, path) = a_file(&text);
        let query = Query::literal("alpha");

        let whole = crate::find_all(&text, &query).expect("the in-memory search runs");
        let search = scan_with(&path, &query, small());

        assert!(search.windows_scanned() > 1, "one window proves nothing");
        assert_eq!(search.hits().len(), whole.len());
        // `find_all` reports a 1-based line within the text; a hit reports a
        // 0-based line of the file. The mapping between them is the assertion.
        let streamed: Vec<usize> = search.hits().iter().map(|h| h.line).collect();
        let in_memory: Vec<usize> = whole.iter().map(|m| m.line - 1).collect();
        assert_eq!(streamed, in_memory);
    }

    #[test]
    fn a_hit_inside_a_giant_line_reports_the_line_and_admits_it_has_no_column() {
        // The case `StreamHit::column`'s `Option` exists for, and it is not
        // hypothetical: the needle sits well inside a line longer than the
        // window, so the window that finds it began inside that line and
        // never saw where it started.
        let filler = "x".repeat(3 * bp_buffer::MIN_WINDOW_BYTES);
        let (_dir, path) = a_file(&format!("first line\n{filler}needle{filler}\n"));

        let search = scan_with(&path, &Query::literal("needle"), small());

        assert_eq!(search.hits().len(), 1);
        assert_eq!(search.hits()[0].line, 1, "the line number survives");
        assert_eq!(
            search.hits()[0].column,
            None,
            "the column does not, and is not guessed"
        );
    }

    #[test]
    fn the_overlap_this_scan_relies_on_is_stated_rather_than_assumed() {
        // The one thing streaming can lose is a match longer than the
        // overlap, and a caller cannot weigh that without being told what it
        // is.
        let (_dir, path) = a_file(&numbered(10));
        let search = StreamSearch::start(&path, &Query::literal("line")).expect("it starts");

        assert!(
            search.guaranteed_match_bytes() >= 4096,
            "longer than anything anyone types into a find box"
        );
    }

    #[test]
    fn the_tick_budget_is_a_scan_rate_that_finishes_a_large_log_promptly() {
        // WINDOWS_PER_TICK and STREAM_TICK mean nothing apart, which is why they
        // live together. This pins the rate they describe rather than either
        // number alone.
        let ticks_per_second = 1_000 / STREAM_TICK.as_millis();
        let per_second =
            WINDOWS_PER_TICK as u128 * ticks_per_second * bp_buffer::DEFAULT_WINDOW_BYTES as u128;

        assert!(
            per_second >= 128 * 1024 * 1024,
            "a 192 MiB log should take a second or two, not a minute: {per_second} B/s"
        );
    }
}
