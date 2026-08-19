//! Where every line starts, learned a chunk at a time and never all at once.
//!
//! The obvious line index is `Vec<u64>` with one entry per line. On the 2 GB
//! file specs.md section 5 asks us to open, that is 25 million entries and
//! 200 MB of index -- more than the memory budget for the whole document, to
//! answer a question the user asks about forty lines at a time.
//!
//! So the index is **sparse**: it keeps the start of every `stride`-th line,
//! and doubles `stride` (discarding every other anchor) whenever it would
//! outgrow its cap. That makes the index's memory a constant the file size
//! cannot move, and turns "where does line N start" into "here is a known
//! offset at most `stride` lines before it, scan from there". A bounded scan
//! is the price of a bounded index, and it is the right trade: the scan is
//! sequential I/O over a few kilobytes, and the alternative is an index
//! larger than the memory bound it was supposed to protect.
//!
//! It is also **incremental**. `push` takes consecutive chunks in file order,
//! so indexing can be spread across frames rather than blocking the first
//! one, and every question can be answered against the prefix indexed so far.
//!
//! Chunk boundaries are the defect to hunt here, and one fact makes this half
//! safe: `\n` is `0x0A`, and no byte of a multi-byte UTF-8 sequence is ever
//! below `0x80`. A newline therefore cannot hide inside a character, and
//! counting newlines over raw bytes cannot be confused by a chunk that splits
//! one. Decoding is a different matter -- see [`crate::stream`].

use std::ops::Range;

/// Anchors kept before the index halves its resolution.
///
/// 65,536 anchors is 512 KiB of `u64`, chosen so the index is a rounding
/// error against the several-megabyte streaming budget while still giving
/// exact answers -- stride 1 -- for any file under ~65,000 lines, which is
/// most of what anyone actually opens.
pub const MAX_ANCHORS: usize = 64 * 1024;

/// Bytes of heap the index will never exceed, for the default cap.
///
/// Stated as a constant because "bounded memory" is only a claim until there
/// is a number attached to it, and a number nothing asserts against is a
/// comment.
pub const MAX_INDEX_HEAP_BYTES: usize = MAX_ANCHORS * size_of::<u64>();

/// What the index knows about where a line begins.
///
/// Four answers rather than `Option<u64>`, because "I have not looked that
/// far yet" and "there is no such line" lead to opposite actions -- index
/// more, or tell the user -- and collapsing them into `None` is how a
/// viewport ends up permanently blank at the bottom of a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineLocation {
    /// The exact byte offset at which the line starts.
    At(u64),
    /// Not anchored. `line` is the nearest earlier line whose offset is
    /// known; scan forward from `byte`, counting newlines, to reach the one
    /// asked for. Never more than `stride` lines of scanning.
    After {
        /// The 0-based line the offset belongs to.
        line: usize,
        /// Byte offset at which that line starts.
        byte: u64,
    },
    /// The scan has not reached this line yet. Index more and ask again.
    NotYetIndexed,
    /// There is no such line. Only ever returned once the index is complete,
    /// because before that the honest answer is [`Self::NotYetIndexed`].
    PastEnd,
}

/// A sparse, incrementally built map from line number to byte offset.
///
/// Byte offsets, not character offsets: a character offset cannot be known
/// without decoding everything before it, which is the whole cost this type
/// exists to avoid.
#[derive(Debug, Clone)]
pub struct LineIndex {
    /// `anchors[k]` is the byte offset at which line `k * stride` starts.
    /// `anchors[0]` is always 0 -- line 0 starts at the beginning.
    anchors: Vec<u64>,
    stride: usize,
    max_anchors: usize,
    /// Line starts discovered so far, using the editor convention: a
    /// trailing newline creates a line, so `"a\n"` is two.
    lines: usize,
    indexed_bytes: u64,
    complete: bool,
}

impl Default for LineIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl LineIndex {
    /// An index over nothing, with the default memory cap.
    pub fn new() -> Self {
        Self::with_max_anchors(MAX_ANCHORS)
    }

    /// An index with a tighter cap.
    ///
    /// Exists so a caller that opens twenty documents can spend less per
    /// document, and so tests can force the halving path at a size a person
    /// can enumerate by hand rather than at 65,536 anchors. Clamped to at
    /// least two, because halving a one-entry index cannot make progress.
    pub fn with_max_anchors(max_anchors: usize) -> Self {
        let max_anchors = max_anchors.max(2);
        let mut anchors = Vec::with_capacity(max_anchors.min(1024));
        anchors.push(0);
        Self {
            anchors,
            stride: 1,
            max_anchors,
            lines: 1,
            indexed_bytes: 0,
            complete: false,
        }
    }

    /// Index the next consecutive chunk of the file.
    ///
    /// Chunks must arrive in file order and without gaps -- the index tracks
    /// absolute offsets and has no way to detect a skipped region, so a
    /// caller that seeks between calls produces a plausible-looking index
    /// that is wrong everywhere after the seek. Chunks may split a line or a
    /// multi-byte character freely; newline counting is byte-level and cannot
    /// be fooled by either.
    ///
    /// Ignored once [`Self::finish`] has been called, so a double-finish in a
    /// background task cannot corrupt the index.
    pub fn push(&mut self, chunk: &[u8]) {
        if self.complete {
            return;
        }
        for (offset, byte) in chunk.iter().enumerate() {
            if *byte == b'\n' {
                let start = self.indexed_bytes + offset as u64 + 1;
                self.record(start);
            }
        }
        self.indexed_bytes += chunk.len() as u64;
    }

    /// Declare that the whole file has been pushed.
    ///
    /// Until this is called the index refuses to say a line does not exist,
    /// because it cannot tell "past the end" from "not read yet".
    pub fn finish(&mut self) {
        self.complete = true;
    }

    /// Lines discovered so far, on the editor's convention that a trailing
    /// newline creates one. Equals the file's line count once complete.
    pub fn line_count(&self) -> usize {
        self.lines
    }

    /// Bytes fed in so far. A progress numerator; the file's length is the
    /// denominator.
    pub fn indexed_bytes(&self) -> u64 {
        self.indexed_bytes
    }

    /// Whether the whole file has been indexed.
    pub fn is_complete(&self) -> bool {
        self.complete
    }

    /// Lines between consecutive anchors. 1 means every line is exact; it
    /// doubles each time the index halves itself. Exposed because it is the
    /// bound on how far a caller may have to scan, and a caller budgeting
    /// work per frame needs that number.
    pub fn stride(&self) -> usize {
        self.stride
    }

    /// Heap the anchors currently occupy, in bytes.
    ///
    /// The honest measurement for the bounded-memory claim: capacity, not
    /// length, because a `Vec` that grew and shrank still holds the larger
    /// buffer.
    pub fn heap_bytes(&self) -> usize {
        self.anchors.capacity() * size_of::<u64>()
    }

    /// Where line `line` starts, exactly or nearly.
    ///
    /// 0-based, matching [`crate::Buffer::line_start`]; the 1-based numbers
    /// are a display concern and converting at the edge is the rule
    /// everywhere else in this crate.
    pub fn locate(&self, line: usize) -> LineLocation {
        if line == 0 {
            return LineLocation::At(0);
        }
        if line >= self.lines {
            return if self.complete {
                LineLocation::PastEnd
            } else {
                LineLocation::NotYetIndexed
            };
        }
        let anchor = line / self.stride;
        let anchor_line = anchor * self.stride;
        let byte = self.anchors[anchor];
        if anchor_line == line {
            LineLocation::At(byte)
        } else {
            LineLocation::After {
                line: anchor_line,
                byte,
            }
        }
    }

    /// The byte range a line occupies, including its line break, when both
    /// ends happen to be anchored.
    ///
    /// Returns `None` rather than guessing whenever either end needs a scan,
    /// because a range that is sometimes exact and sometimes approximate is
    /// a range nobody can safely use. Callers that need it always should ask
    /// [`crate::LargeFile`], which can afford the I/O.
    pub fn exact_range(&self, line: usize) -> Option<Range<u64>> {
        let start = match self.locate(line) {
            LineLocation::At(byte) => byte,
            _ => return None,
        };
        match self.locate(line + 1) {
            LineLocation::At(end) => Some(start..end),
            _ => None,
        }
    }

    /// Record that a new line starts at `start`, keeping the anchor only if
    /// it is the next one the current stride calls for.
    ///
    /// The `line / stride == anchors.len()` half is belt and braces: with the
    /// halving in [`Self::halve`] as written, the next line whose index is a
    /// multiple of the new stride is exactly the next anchor slot, so the
    /// modulo alone would do. It is here so that the invariant
    /// -- *`anchors[k]` is the start of line `k * stride`* -- holds by local
    /// inspection rather than by an argument about arithmetic somewhere else.
    fn record(&mut self, start: u64) {
        let line = self.lines;
        if line.is_multiple_of(self.stride) && line / self.stride == self.anchors.len() {
            self.anchors.push(start);
            if self.anchors.len() >= self.max_anchors {
                self.halve();
            }
        }
        self.lines += 1;
    }

    /// Throw away every other anchor and double the stride.
    ///
    /// In place, so the `Vec`'s capacity -- which is what
    /// [`Self::heap_bytes`] reports and what the memory bound is stated
    /// against -- never grows past the cap.
    fn halve(&mut self) {
        let mut write = 0;
        for read in (0..self.anchors.len()).step_by(2) {
            self.anchors[write] = self.anchors[read];
            write += 1;
        }
        self.anchors.truncate(write);
        self.stride *= 2;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reference implementation: walk the whole thing, keep everything.
    /// Deliberately the stupidest possible version, because a property test
    /// against a clever reference tests two implementations of the same bug.
    fn full_scan(text: &[u8]) -> Vec<u64> {
        let mut starts = vec![0u64];
        for (i, b) in text.iter().enumerate() {
            if *b == b'\n' {
                starts.push(i as u64 + 1);
            }
        }
        starts
    }

    fn index_in_chunks(text: &[u8], chunk: usize) -> LineIndex {
        let mut index = LineIndex::new();
        for part in text.chunks(chunk.max(1)) {
            index.push(part);
        }
        index.finish();
        index
    }

    /// Resolve a location the way a caller would, using the reference table
    /// to stand in for the forward scan `LargeFile` does with real I/O.
    fn resolve(index: &LineIndex, line: usize, reference: &[u64]) -> Option<u64> {
        match index.locate(line) {
            LineLocation::At(byte) => Some(byte),
            LineLocation::After { line: anchor, byte } => {
                // The scan a caller would perform: `line - anchor` newlines
                // forward from `byte`. Modelled here against the reference so
                // the test checks the *anchor*, which is what the index owns.
                assert!(anchor < line, "an anchor must be strictly earlier");
                assert_eq!(byte, reference[anchor], "anchor offset is wrong");
                assert!(
                    line - anchor < index.stride(),
                    "scan distance must be under one stride"
                );
                Some(reference[line])
            }
            LineLocation::NotYetIndexed | LineLocation::PastEnd => None,
        }
    }

    #[test]
    fn the_index_agrees_with_a_full_scan_at_every_line_and_every_chunk_size() {
        let corpus: &[&str] = &[
            "",
            "\n",
            "a",
            "a\n",
            "a\nb",
            "a\nb\n",
            "\n\n\n",
            "one\ntwo\nthree",
            "crlf\r\nlines\r\nhere\r\n",
            "日本語\nと\r\nemoji 🙂 mixed\n",
            "no trailing newline at all, just one very long single line here",
        ];
        for text in corpus {
            let bytes = text.as_bytes();
            let reference = full_scan(bytes);
            for chunk in 1..=bytes.len().max(1) + 2 {
                let index = index_in_chunks(bytes, chunk);
                assert_eq!(
                    index.line_count(),
                    reference.len(),
                    "line count for {text:?} at chunk {chunk}"
                );
                for (line, expected) in reference.iter().enumerate() {
                    assert_eq!(
                        resolve(&index, line, &reference),
                        Some(*expected),
                        "line {line} of {text:?} at chunk {chunk}"
                    );
                }
                assert_eq!(
                    index.locate(reference.len()),
                    LineLocation::PastEnd,
                    "one past the last line of {text:?} at chunk {chunk}"
                );
            }
        }
    }

    #[test]
    fn an_empty_file_still_has_line_zero() {
        let mut index = LineIndex::new();
        index.finish();
        assert_eq!(index.line_count(), 1, "the caret sits on line 1");
        assert_eq!(index.locate(0), LineLocation::At(0));
        assert_eq!(index.locate(1), LineLocation::PastEnd);
    }

    #[test]
    fn a_file_with_no_trailing_newline_has_no_phantom_last_line() {
        let mut index = LineIndex::new();
        index.push(b"a\nb");
        index.finish();
        assert_eq!(index.line_count(), 2);
        assert_eq!(index.locate(1), LineLocation::At(2));
        assert_eq!(index.locate(2), LineLocation::PastEnd);
    }

    #[test]
    fn a_trailing_newline_creates_a_line_starting_at_the_end() {
        let mut index = LineIndex::new();
        index.push(b"a\n");
        index.finish();
        assert_eq!(index.line_count(), 2, "str::lines() says 1, and is wrong");
        assert_eq!(
            index.locate(1),
            LineLocation::At(2),
            "the empty last line starts at EOF"
        );
    }

    #[test]
    fn an_unfinished_index_says_so_rather_than_claiming_the_line_is_missing() {
        let mut index = LineIndex::new();
        index.push(b"a\nb\n");
        assert_eq!(index.locate(1), LineLocation::At(2));
        assert_eq!(
            index.locate(9),
            LineLocation::NotYetIndexed,
            "before finish, absence is ignorance, not proof"
        );
        index.finish();
        assert_eq!(index.locate(9), LineLocation::PastEnd);
    }

    #[test]
    fn halving_keeps_every_anchor_it_reports_correct() {
        // Eight anchors forces three halvings over 200 lines, so the stride
        // changes mid-file several times -- the case where a bare modulo
        // instead of the "next expected anchor" test goes wrong.
        let text: String = (0..200).map(|i| format!("line {i}\n")).collect();
        let bytes = text.as_bytes();
        let reference = full_scan(bytes);

        let mut index = LineIndex::with_max_anchors(8);
        for part in bytes.chunks(7) {
            index.push(part);
        }
        index.finish();

        assert!(index.stride() > 1, "the cap must actually have bitten");
        assert_eq!(index.line_count(), reference.len());
        for (line, expected) in reference.iter().enumerate() {
            assert_eq!(
                resolve(&index, line, &reference),
                Some(*expected),
                "line {line} after halving to stride {}",
                index.stride()
            );
        }
    }

    #[test]
    fn the_index_never_outgrows_its_cap() {
        let mut index = LineIndex::with_max_anchors(16);
        for i in 0..50_000u32 {
            index.push(format!("{i}\n").as_bytes());
            assert!(
                index.heap_bytes() <= 16 * size_of::<u64>(),
                "heap {} bytes at line {i}",
                index.heap_bytes()
            );
        }
        index.finish();
        assert_eq!(index.line_count(), 50_001);
    }

    #[test]
    fn the_default_cap_bounds_the_heap_for_a_file_far_larger_than_it() {
        let mut index = LineIndex::new();
        // 400,000 lines: six times the anchor cap, so halving has happened.
        for i in 0..400_000u32 {
            index.push(format!("{i}\n").as_bytes());
        }
        index.finish();
        assert!(index.stride() >= 8, "stride is {}", index.stride());
        assert!(
            index.heap_bytes() <= MAX_INDEX_HEAP_BYTES,
            "heap {} exceeds the stated bound {MAX_INDEX_HEAP_BYTES}",
            index.heap_bytes()
        );
    }

    #[test]
    fn exact_range_refuses_to_guess() {
        let mut index = LineIndex::with_max_anchors(2);
        for _ in 0..8 {
            index.push(b"ab\n");
        }
        index.finish();
        assert!(index.stride() > 1);
        // Line 0 and the next anchor are exact; a line between them is not,
        // and must come back as None rather than a nearby-ish range.
        assert!(index.exact_range(0).is_some() || index.exact_range(1).is_none());
        let unanchored = (0..index.line_count())
            .find(|l| !matches!(index.locate(*l), LineLocation::At(_)))
            .expect("halving must leave some line unanchored");
        assert_eq!(index.exact_range(unanchored), None);
    }

    #[test]
    fn pushes_after_finish_are_ignored() {
        let mut index = LineIndex::new();
        index.push(b"a\n");
        index.finish();
        index.push(b"b\nc\n");
        assert_eq!(index.line_count(), 2, "finish means finished");
        assert_eq!(index.indexed_bytes(), 2);
    }
}
