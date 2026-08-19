//! Opening files that will not fit the ordinary path.
//!
//! specs.md section 5 asks for large-file detection, read-only initial
//! handling for very large files, chunked loading and streaming search;
//! section 22 says multi-GB files must be "usable in large-file mode". This
//! module is the detection and the chunked reading; [`crate::stream`] is the
//! streaming; [`crate::line_index`] is the index that makes a viewport
//! reachable without a full scan.
//!
//! # Where the thresholds come from
//!
//! Both are measured, on the benchmark corpus in `benches/large_file.rs`
//! (72-byte lines, the shape of prose and source alike). Re-run it to
//! reproduce or to re-derive them on other hardware; these numbers are from
//! a Windows 10 x86-64 workstation, release build, rustc 1.97.1, 2026-08-19:
//!
//! | File | cold read | UTF-8 check | build rope | **total cold** | resident |
//! | ---: | ---: | ---: | ---: | ---: | ---: |
//! | 6 MiB | 65.5 ms | 0.4 ms | 8.8 ms | **74.6 ms** | 7.2 MiB |
//! | 8 MiB | 77.3 ms | 0.5 ms | 12.1 ms | **90.0 ms** | 9.5 MiB |
//! | 10 MiB | 89.6 ms | 0.8 ms | 15.3 ms | **105.7 ms** | 11.8 MiB |
//! | 12 MiB | 108.0 ms | 1.1 ms | 19.2 ms | **128.3 ms** | 14.1 MiB |
//! | 16 MiB | 142.2 ms | 1.7 ms | 25.5 ms | **169.4 ms** | 18.7 MiB |
//! | 192 MiB | 319.1 ms | 24.3 ms | 378.2 ms | **721.6 ms** | 222.9 MiB |
//! | 224 MiB | 342.1 ms | 28.0 ms | 384.0 ms | **754.1 ms** | 260.0 MiB |
//!
//! [`LARGE_FILE_BYTES`] falls out of the top of the table against the budget
//! in specs.md section 22: 150 ms to window, 250 ms to first interaction,
//! leaving about 100 ms for getting content on screen. 8 MiB costs 90 ms of
//! that -- the last size that fits -- and 10 MiB costs 106 ms, which is the
//! budget gone before anything is drawn. So the ordinary path stops at
//! 8 MiB: a measured ceiling that happens to be round, not a round number
//! assumed to be a ceiling.
//!
//! [`HUGE_FILE_BYTES`] falls out of the bottom two rows. The rope costs a
//! measured 1.16x the file on the heap, and that multiplier holds to three
//! digits across every size in the table, so a ceiling on resident memory is
//! a ceiling on file size. 256 MiB resident is as much as a text editor has
//! any business spending on one document -- specs.md section 22 budgets
//! 50 MB idle -- and 1.16x puts that at about 220 MiB of file. 192 MiB is the
//! largest 64 MiB step under it, measured at 222.9 MiB resident. 224 MiB is
//! the first step over, at 260.0 MiB. Past 192 MiB the document is not
//! loaded at all.
//!
//! What the chunked path costs instead, on a 255 MiB file from the same run:
//! opening it 0.19 ms, the first sixty lines 0.35 ms, indexing all 4,067,204
//! lines 181 ms (1.38 GiB/s), and streaming every byte past a matcher 373 ms
//! with a 2,056 KiB peak. The fully indexed reader held 768 KiB at the end of
//! that run -- the index at its 512 KiB cap plus one 256 KiB chunk, which is
//! what `resident_bytes` counts and is not the index alone. Opening is
//! `metadata`; the first frame is one seek and one chunk; neither grows with
//! the file.
//!
//! # Why this does not memory-map, and what it would take to
//!
//! specs.md section 5 lists "memory-mapped read-only mode". This
//! implementation does chunked `read` instead, deliberately, and the
//! decision is written down here rather than silently taken:
//!
//! 1. **It cannot be done without `unsafe`.** Every mapping crate's entry
//!    point (`memmap2::Mmap::map` and equivalents) is an `unsafe fn`, and
//!    this crate has `#![forbid(unsafe_code)]`. Every editing path in the
//!    application goes through `bp-buffer`; trading that guarantee away for
//!    one feature is not a trade one agent should make alone.
//! 2. **The unsoundness is not theoretical here.** A mapping is undefined
//!    behaviour if the file is truncated underneath it -- the next touched
//!    page raises `SIGBUS` on Linux and `EXCEPTION_IN_PAGE_ERROR` on
//!    Windows, neither catchable in safe Rust. This editor already assumes
//!    files change underneath it: `bp-files::watch` exists precisely to
//!    detect it, and ADR-0007 makes external-change detection a v1
//!    requirement. A network share or a removable volume disappearing does
//!    the same thing. So the hazard is one the product design has already
//!    conceded is real.
//! 3. **ADR-0002 already counts mapping as attack surface**, alongside
//!    parsers over untrusted files.
//! 4. **The measured benefit is the smaller half.** Mapping removes a copy
//!    from the read. The read is not where the time goes: at 192 MiB the
//!    cold read is 319 ms against 378 ms to build the rope and 24 ms to
//!    validate, and warm sequential reads here run at 1.7 GiB/s -- already
//!    faster than any consumer of the bytes. Chunked reads sustain
//!    1.38 GiB/s through the line indexer over a 255 MiB file, which is the
//!    same order, and they keep the property that actually matters: opening
//!    a 2 GB file touches kilobytes rather than gigabytes, measured at
//!    0.19 ms whatever the size.
//!
//! What mapping would buy, if a human decides it is worth it: roughly the
//! read half of a cold whole-file scan, and simpler random access for the
//! line-start scan. What it would cost: `unsafe` in the crate every edit
//! passes through, an unsound-by-construction failure mode on external
//! truncation that would need its own mitigation (copy-on-open, or a
//! platform-specific `SIGBUS` handler, neither of which is small), a new
//! third-party dependency, and ADR-0002 revisited. **Left as a stated
//! decision for a human**, not taken here.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::line_index::{LineIndex, LineLocation, MAX_INDEX_HEAP_BYTES};
use crate::stream::{DEFAULT_OVERLAP_BYTES, DEFAULT_WINDOW_BYTES, WindowReader};

/// Above this, a document is handled as a large file.
///
/// Measured, not chosen: see the module docs. 8 MiB is the largest size whose
/// cold open -- read, validate, build the rope -- fits the roughly 100 ms
/// that specs.md section 22 leaves between "window on screen" and "accepting
/// keys". The next step up costs the whole budget again.
///
/// The comparison is strict: a file of exactly this size is still
/// [`SizeClass::Normal`], because the measurement said 8 MiB fits.
pub const LARGE_FILE_BYTES: u64 = 8 * 1024 * 1024;

/// Above this, a document is not loaded into memory at all.
///
/// Measured: the rope costs 1.16x the file, so a 256 MiB ceiling on resident
/// memory for one document lands at about 220 MiB of file, and 192 MiB is the
/// largest 64 MiB step below it. Past this the file is read in chunks and the
/// document opens read-only -- see [`Access::ReadOnlyBySize`].
pub const HUGE_FILE_BYTES: u64 = 192 * 1024 * 1024;

/// Bytes per read when scanning or indexing.
///
/// 256 KiB is comfortably past the point where per-call overhead stops
/// mattering and comfortably short of the point where one read stalls a
/// frame. It is also the granularity at which indexing can be interrupted.
pub const CHUNK_BYTES: usize = 256 * 1024;

/// How much to index in one call when spreading the work across frames.
///
/// Newline scanning here runs at over 1 GiB/s in release, so 4 MiB is a
/// small fraction of a 16 ms frame -- enough that a 2 GB file indexes in a
/// few seconds of background work, little enough that no single call is
/// visible as a stutter.
pub const INDEX_BUDGET_BYTES: u64 = 4 * 1024 * 1024;

/// The longest line the viewport will materialise, in bytes.
///
/// A generated file can hold a single 400 MB line. Rendering it is not
/// possible and pretending to try is how a read-only viewer runs out of
/// memory, so the line is cut and [`DisplayLine::truncated`] says so rather
/// than the UI silently showing a prefix.
pub const MAX_DISPLAY_LINE_BYTES: usize = 64 * 1024;

/// The most lines one [`LargeFile::display_lines`] call will return.
///
/// A viewport is tens of lines. The cap exists so the memory bound is a
/// bound: without it a caller asking for a million lines makes the claim in
/// [`LargeFile::memory_bound_bytes`] false.
pub const MAX_DISPLAY_LINES: usize = 4096;

/// The most text one [`LargeFile::display_lines`] call will return, across
/// all its lines.
///
/// A line cap alone is not a bound -- 4,096 lines of 64 KiB is a quarter of a
/// gigabyte, which is worse than the file the caller was trying to avoid
/// loading. This is the cap that makes the arithmetic come out: the call
/// returns fewer lines than asked for once it is reached, and the caller
/// asks again from where it stopped. That is the same shape as incremental
/// rendering, so it costs nothing to obey.
pub const MAX_DISPLAY_BYTES: usize = 1024 * 1024;

/// What kind of file this is, by size alone.
///
/// Three states rather than a boolean, because the two thresholds mean
/// different things to the caller: one says "warn and defer the expensive
/// whole-document work", the other says "do not load this".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SizeClass {
    /// Load it the ordinary way. Everything under [`LARGE_FILE_BYTES`].
    Normal,
    /// Loadable into a rope, but not for free: expect a visible pause, and
    /// whole-document work -- syntax passes, statistics, cross-file search --
    /// should be deferred or offered rather than done on open.
    Large,
    /// Do not load it. Above [`HUGE_FILE_BYTES`] the document is served from
    /// disk in chunks and opened read-only.
    Huge,
}

impl SizeClass {
    /// Classify a size in bytes.
    ///
    /// Boundaries are exclusive on the way up: exactly [`LARGE_FILE_BYTES`]
    /// is still [`Self::Normal`], because that is the size the measurement
    /// says fits. An off-by-one here moves the class of every file on a
    /// threshold, which is why the direction is stated rather than inferred.
    pub const fn of(bytes: u64) -> Self {
        if bytes > HUGE_FILE_BYTES {
            Self::Huge
        } else if bytes > LARGE_FILE_BYTES {
            Self::Large
        } else {
            Self::Normal
        }
    }

    /// Whether the ordinary whole-file load path should be avoided.
    pub const fn is_large(self) -> bool {
        !matches!(self, Self::Normal)
    }

    /// Whether the file must not be loaded into memory at all.
    pub const fn must_stream(self) -> bool {
        matches!(self, Self::Huge)
    }

    /// A word for the status bar. Empty for [`Self::Normal`], because a
    /// status bar that labels the ordinary case teaches people to ignore it.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Normal => "",
            Self::Large => "Large file",
            Self::Huge => "Huge file",
        }
    }
}

/// Whether a document accepts edits, and if not, why not.
///
/// The two read-only reasons are genuinely different things and collapsing
/// them into a boolean loses the difference the user needs. A file that is
/// read-only on disk becomes editable when its permissions change, and Save
/// As is the way out. A document that is read-only because it is 2 GB stays
/// read-only however the permissions change, and the way out is a different
/// tool or a smaller slice. Telling someone "read-only" without saying which
/// leaves them clicking at permissions that were never the problem.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Access {
    /// Edits are accepted. The default, so that every buffer built before
    /// anyone thought about access is editable rather than mysteriously
    /// inert.
    #[default]
    Editable,
    /// The file on disk forbids writing: permissions, a read-only volume, or
    /// another process holding it.
    ReadOnlyFile,
    /// The document is too large to hold in memory. `bytes` is its size on
    /// disk, so the message can say how large.
    ReadOnlyBySize {
        /// Size on disk, in bytes.
        bytes: u64,
    },
}

impl Access {
    /// Decide access from a size and the file's own permissions.
    ///
    /// Size wins when both apply. It is the reason edits are refused *while
    /// typing*, which is what the user experiences first; permissions only
    /// surface at save time, and a message about them would be answering a
    /// question that has not been asked yet.
    pub const fn of(bytes: u64, file_is_read_only: bool) -> Self {
        if SizeClass::of(bytes).must_stream() {
            Self::ReadOnlyBySize { bytes }
        } else if file_is_read_only {
            Self::ReadOnlyFile
        } else {
            Self::Editable
        }
    }

    /// Whether edits must be refused.
    pub const fn is_read_only(self) -> bool {
        !matches!(self, Self::Editable)
    }

    /// What to tell the user, in one line.
    ///
    /// Phrased as a fact about the document rather than an apology, and it
    /// says the size, because "too large" without a number invites the reply
    /// "it's only 300 megabytes".
    pub fn message(self) -> String {
        match self {
            Self::Editable => String::new(),
            Self::ReadOnlyFile => "Read-only: the file cannot be written.".to_owned(),
            Self::ReadOnlyBySize { bytes } => format!(
                "Read-only: {} is past the {} editing limit.",
                format_bytes(bytes),
                format_bytes(HUGE_FILE_BYTES)
            ),
        }
    }
}

/// Bytes as a person reads them. Binary units, because the thresholds are
/// binary and a status bar that says 192 MB for a 201,326,592-byte limit
/// invites a bug report.
fn format_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * KIB;
    const GIB: u64 = 1024 * MIB;
    if bytes >= GIB {
        format!("{:.1} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.0} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.0} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} bytes")
    }
}

/// What went wrong reading a file in chunks.
#[derive(Debug, Error)]
pub enum LargeFileError {
    /// The read itself failed. Carries the path, because by the time this
    /// reaches a dialog the caller has usually lost track of which of several
    /// open documents it was.
    #[error("cannot read {}: {source}", .path.display())]
    Io {
        /// The file being read.
        path: PathBuf,
        /// The underlying failure.
        #[source]
        source: std::io::Error,
    },

    /// The bytes are not UTF-8. Reported with the offset rather than decoded
    /// lossily, for the reason `bp-files` gives: a lossy decode lets someone
    /// edit and save mangled text over their original.
    #[error("{} is not valid UTF-8 at byte {offset}", .path.display())]
    InvalidUtf8 {
        /// The file being read.
        path: PathBuf,
        /// Byte offset of the first byte that is not valid UTF-8.
        offset: u64,
    },
}

impl LargeFileError {
    /// Attach a path to an I/O failure.
    pub(crate) fn io(path: &Path, source: std::io::Error) -> Self {
        Self::Io {
            path: path.to_owned(),
            source,
        }
    }
}

/// How much of a file has been indexed, for a progress display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexProgress {
    /// Bytes indexed so far.
    pub indexed_bytes: u64,
    /// Bytes in the file.
    pub total_bytes: u64,
    /// Lines found so far. The final line count once `complete`.
    pub lines: usize,
    /// Whether the whole file has been indexed.
    pub complete: bool,
}

impl IndexProgress {
    /// Fraction indexed, 0.0 to 1.0. An empty file is 1.0, not a division by
    /// zero, because "nothing to do" is complete.
    pub fn fraction(&self) -> f32 {
        if self.total_bytes == 0 {
            return 1.0;
        }
        (self.indexed_bytes as f32 / self.total_bytes as f32).clamp(0.0, 1.0)
    }
}

/// One line, ready to draw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayLine {
    /// 0-based line number in the file, matching
    /// [`crate::Buffer::line_start`].
    pub line: usize,
    /// The line's text, without its line break. CRLF is stripped whole, so a
    /// Windows file does not render a stray carriage return at every line
    /// end.
    pub text: String,
    /// True when the line was longer than [`MAX_DISPLAY_LINE_BYTES`] and has
    /// been cut. Surfaced rather than swallowed: a viewer that silently shows
    /// the first 64 KiB of a line is lying about the file.
    pub truncated: bool,
}

/// A file too large to load, read in chunks instead.
///
/// Holds an open handle, a scan buffer and a sparse line index -- a few
/// hundred kilobytes -- and never the file. Opening one is `O(1)`: a
/// `metadata` call. That is the property specs.md section 5 is really asking
/// for when it says a 2 GB file must be openable, and it is the property the
/// tests assert.
#[derive(Debug)]
pub struct LargeFile {
    path: PathBuf,
    file: File,
    len: u64,
    access: Access,
    index: LineIndex,
    scratch: Vec<u8>,
}

impl LargeFile {
    /// Open a file for chunked reading.
    ///
    /// Reads no content -- only `metadata` -- so this is as cheap for 2 GB as
    /// for 2 KB, which is the entire point. Nothing is validated as UTF-8
    /// here either; validation happens per chunk, where it costs what the
    /// chunk costs rather than what the file costs.
    pub fn open(path: &Path) -> Result<Self, LargeFileError> {
        let file = File::open(path).map_err(|source| LargeFileError::io(path, source))?;
        let metadata = file
            .metadata()
            .map_err(|source| LargeFileError::io(path, source))?;
        let len = metadata.len();
        Ok(Self {
            path: path.to_owned(),
            file,
            len,
            access: Access::of(len, metadata.permissions().readonly()),
            index: LineIndex::new(),
            scratch: Vec::new(),
        })
    }

    /// The file being read.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Size on disk when it was opened.
    pub fn len_bytes(&self) -> u64 {
        self.len
    }

    /// Which side of the two thresholds this file falls on.
    pub fn size_class(&self) -> SizeClass {
        SizeClass::of(self.len)
    }

    /// Whether the document accepts edits, and why not if it does not.
    pub fn access(&self) -> Access {
        self.access
    }

    /// The line index, for callers that want to ask it questions directly --
    /// how far it has got, how sparse it has become.
    pub fn index(&self) -> &LineIndex {
        &self.index
    }

    /// Indexing progress as it stands, without doing any more work.
    pub fn progress(&self) -> IndexProgress {
        IndexProgress {
            indexed_bytes: self.index.indexed_bytes(),
            total_bytes: self.len,
            lines: self.index.line_count(),
            complete: self.index.is_complete(),
        }
    }

    /// Bytes of heap this file holds between calls: the index and the scan
    /// buffer. Capacities, not lengths.
    pub fn resident_bytes(&self) -> usize {
        self.index.heap_bytes() + self.scratch.capacity()
    }

    /// The peak this type will not exceed, whatever the file's size.
    ///
    /// Index cap, plus one scan buffer, plus the line currently being
    /// accumulated, plus the largest result [`Self::display_lines`] can
    /// return -- its text and the headers around it, allowing for a `Vec`
    /// that doubled once past its initial capacity. The file's length does
    /// not appear in it, and that is the claim.
    ///
    /// Streaming has its own bound, on
    /// [`WindowReader::memory_bound_bytes`], because the two are configured
    /// separately and adding them would hide which one moved.
    pub const fn memory_bound_bytes() -> usize {
        MAX_INDEX_HEAP_BYTES
            + CHUNK_BYTES
            + MAX_DISPLAY_LINE_BYTES
            + MAX_DISPLAY_BYTES
            + 2 * MAX_DISPLAY_LINES * size_of::<DisplayLine>()
    }

    /// Index a bounded amount more of the file.
    ///
    /// A budget rather than a deadline: a deadline needs a clock, and this
    /// crate does not read one -- that is what lets `bp-buffer` be tested
    /// without a window. The caller converts frames to bytes; see
    /// [`INDEX_BUDGET_BYTES`] for the conversion this repository measured.
    ///
    /// Calling it after the index is complete is free and does nothing, so a
    /// background task can keep calling it without checking first.
    pub fn index_more(&mut self, budget_bytes: u64) -> Result<IndexProgress, LargeFileError> {
        if self.index.is_complete() {
            return Ok(self.progress());
        }
        self.seek(self.index.indexed_bytes())?;
        let mut spent = 0u64;
        while spent < budget_bytes.max(1) {
            let read = self.read_chunk()?;
            if read == 0 {
                self.index.finish();
                break;
            }
            let chunk = std::mem::take(&mut self.scratch);
            self.index.push(&chunk[..read]);
            self.scratch = chunk;
            spent += read as u64;
        }
        Ok(self.progress())
    }

    /// Index the whole file, however long that takes.
    ///
    /// For callers that need a true line count -- a "Ln 42 of 8,134,409"
    /// status bar -- and can afford to block. Everything else should use
    /// [`Self::index_more`].
    pub fn index_fully(&mut self) -> Result<IndexProgress, LargeFileError> {
        while !self.index.is_complete() {
            self.index_more(u64::MAX)?;
        }
        Ok(self.progress())
    }

    /// Index just far enough to know where line `line` is, or to know it does
    /// not exist.
    ///
    /// This is what makes "where does line N start" cheap: scrolling to line
    /// 400 of a 2 GB file reads to line 400 and stops, rather than indexing
    /// two gigabytes to answer a question about the first few kilobytes.
    /// One chunk at a time rather than [`INDEX_BUDGET_BYTES`]: the caller
    /// asked about one line, and spending a frame's budget to answer a
    /// question about the first kilobyte is the very walk this avoids.
    pub fn index_through_line(&mut self, line: usize) -> Result<IndexProgress, LargeFileError> {
        while !self.index.is_complete() && self.index.line_count() <= line {
            self.index_more(CHUNK_BYTES as u64)?;
        }
        Ok(self.progress())
    }

    /// The byte offset at which a 0-based line starts, or `None` if the file
    /// has no such line.
    ///
    /// Indexes as far as it must and no further, then scans forward from the
    /// nearest anchor -- at most one stride of lines, by construction. Takes
    /// `&mut self` because answering may require both, and a method that
    /// pretends to be a getter while doing I/O is how a UI thread ends up
    /// blocked.
    pub fn line_start(&mut self, line: usize) -> Result<Option<u64>, LargeFileError> {
        self.index_through_line(line)?;
        match self.index.locate(line) {
            LineLocation::At(byte) => Ok(Some(byte)),
            LineLocation::After { line: anchor, byte } => {
                self.scan_past_newlines(byte, line - anchor)
            }
            LineLocation::NotYetIndexed | LineLocation::PastEnd => Ok(None),
        }
    }

    /// Read a run of lines for display, starting at a 0-based line.
    ///
    /// `count` is clamped to [`MAX_DISPLAY_LINES`], each line to
    /// [`MAX_DISPLAY_LINE_BYTES`], and the whole result to
    /// [`MAX_DISPLAY_BYTES`]; together those are what make the memory bound
    /// true. Returns fewer lines than asked for at the end of the file or at
    /// any of those caps, and an empty vector when `first` is past the end --
    /// so a caller must look at what it got back rather than assume it got
    /// `count`.
    pub fn display_lines(
        &mut self,
        first: usize,
        count: usize,
    ) -> Result<Vec<DisplayLine>, LargeFileError> {
        let count = count.min(MAX_DISPLAY_LINES);
        if count == 0 {
            return Ok(Vec::new());
        }
        let Some(start) = self.line_start(first)? else {
            return Ok(Vec::new());
        };
        self.seek(start)?;

        let mut out = Vec::with_capacity(count.min(64));
        let mut line = first;
        let mut current: Vec<u8> = Vec::new();
        // Absolute offset of the start of the line being accumulated, and of
        // the next byte to be read. Kept separately because a line may span
        // any number of chunks, and reconstructing one from the other is
        // where an off-by-one in a truncation offset would hide.
        let mut line_start = start;
        let mut pos = start;
        let mut truncated = false;
        let mut returned_bytes = 0usize;

        loop {
            let read = self.read_chunk()?;
            if read == 0 {
                // End of file. The bytes in hand are the last line -- and if
                // there are none, the last line is the empty one after a
                // trailing newline, which the editor's line convention says
                // exists and the gutter has to number.
                out.push(self.finish_line(line, &current, line_start, truncated)?);
                break;
            }
            let chunk = std::mem::take(&mut self.scratch);
            let mut consumed = 0usize;
            while consumed < read {
                let rest = &chunk[consumed..read];
                match rest.iter().position(|b| *b == b'\n') {
                    Some(at) => {
                        Self::extend_capped(&mut current, &rest[..at], &mut truncated);
                        let done = self.finish_line(line, &current, line_start, truncated)?;
                        returned_bytes += done.text.len();
                        out.push(done);
                        current.clear();
                        truncated = false;
                        consumed += at + 1;
                        line_start = pos + consumed as u64;
                        line += 1;
                        if out.len() == count || returned_bytes >= MAX_DISPLAY_BYTES {
                            self.scratch = chunk;
                            return Ok(out);
                        }
                    }
                    None => {
                        Self::extend_capped(&mut current, rest, &mut truncated);
                        consumed = read;
                    }
                }
            }
            pos += read as u64;
            self.scratch = chunk;
        }
        Ok(out)
    }

    /// Stream the file as overlapping text windows, for a matcher.
    ///
    /// `bp-search` decides what a match is; this decides what it gets to look
    /// at. Returns a fresh reader with its own handle, so streaming a search
    /// does not disturb the indexing position -- a background search and a
    /// scrolling viewport are exactly the two things that happen at once.
    pub fn windows(&self) -> Result<WindowReader, LargeFileError> {
        WindowReader::with_config(&self.path, DEFAULT_WINDOW_BYTES, DEFAULT_OVERLAP_BYTES)
    }

    /// Stream with a chosen window and minimum overlap.
    ///
    /// Raise the overlap when searching for something longer than
    /// [`DEFAULT_OVERLAP_BYTES`]; that is the only match length streaming can
    /// lose, and it is stated rather than hidden.
    pub fn windows_with(
        &self,
        window_bytes: usize,
        overlap_bytes: usize,
    ) -> Result<WindowReader, LargeFileError> {
        WindowReader::with_config(&self.path, window_bytes, overlap_bytes)
    }

    /// Append `bytes` to a line being built, stopping at the display cap.
    ///
    /// Keeps consuming the input either way: the rest of the line still has
    /// to be walked to find its end.
    fn extend_capped(current: &mut Vec<u8>, bytes: &[u8], truncated: &mut bool) {
        if current.len() >= MAX_DISPLAY_LINE_BYTES {
            *truncated = true;
            return;
        }
        let room = MAX_DISPLAY_LINE_BYTES - current.len();
        if bytes.len() > room {
            current.extend_from_slice(&bytes[..room]);
            *truncated = true;
        } else {
            current.extend_from_slice(bytes);
        }
    }

    /// Decode one accumulated line.
    ///
    /// A cut at [`MAX_DISPLAY_LINE_BYTES`] can land inside a character, so an
    /// incomplete tail is dropped rather than reported as an error -- the
    /// line is already known to be truncated, and one character more or less
    /// changes nothing. A genuinely invalid sequence is still an error, and
    /// its offset is absolute so the user can be told where.
    fn finish_line(
        &self,
        line: usize,
        bytes: &[u8],
        line_start: u64,
        truncated: bool,
    ) -> Result<DisplayLine, LargeFileError> {
        let body = match std::str::from_utf8(bytes) {
            Ok(text) => text,
            Err(error) => {
                let valid = error.valid_up_to();
                if error.error_len().is_some() || !truncated {
                    return Err(LargeFileError::InvalidUtf8 {
                        path: self.path.clone(),
                        offset: line_start + valid as u64,
                    });
                }
                std::str::from_utf8(&bytes[..valid]).expect("valid_up_to is valid")
            }
        };
        Ok(DisplayLine {
            line,
            text: body.strip_suffix('\r').unwrap_or(body).to_owned(),
            truncated,
        })
    }

    /// From `from`, skip forward past `newlines` line breaks and return the
    /// offset just after the last of them.
    fn scan_past_newlines(
        &mut self,
        from: u64,
        newlines: usize,
    ) -> Result<Option<u64>, LargeFileError> {
        self.seek(from)?;
        let mut at = from;
        let mut remaining = newlines;
        loop {
            let read = self.read_chunk()?;
            if read == 0 {
                return Ok(None);
            }
            let chunk = std::mem::take(&mut self.scratch);
            for (offset, byte) in chunk[..read].iter().enumerate() {
                if *byte == b'\n' {
                    remaining -= 1;
                    if remaining == 0 {
                        self.scratch = chunk;
                        return Ok(Some(at + offset as u64 + 1));
                    }
                }
            }
            at += read as u64;
            self.scratch = chunk;
        }
    }

    fn seek(&mut self, to: u64) -> Result<(), LargeFileError> {
        self.file
            .seek(SeekFrom::Start(to))
            .map_err(|source| LargeFileError::io(&self.path, source))?;
        Ok(())
    }

    /// Read up to [`CHUNK_BYTES`] into the scan buffer, returning how many
    /// bytes arrived. Zero means end of file.
    fn read_chunk(&mut self) -> Result<usize, LargeFileError> {
        self.scratch.resize(CHUNK_BYTES, 0);
        self.file
            .read(&mut self.scratch)
            .map_err(|source| LargeFileError::io(&self.path, source))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn write(dir: &TempDir, name: &str, bytes: &[u8]) -> PathBuf {
        let path = dir.path().join(name);
        let mut file = File::create(&path).unwrap();
        file.write_all(bytes).unwrap();
        file.sync_all().unwrap();
        path
    }

    /// The reference: every line of the text, exactly as an in-memory buffer
    /// would report it, with the line break stripped the same way.
    fn reference_lines(text: &str) -> Vec<String> {
        let buffer = crate::Buffer::from_text(text);
        (0..buffer.len_lines())
            .map(|i| {
                let line = buffer.line(i);
                let line = line.strip_suffix('\n').unwrap_or(&line);
                line.strip_suffix('\r').unwrap_or(line).to_owned()
            })
            .collect()
    }

    #[test]
    fn the_thresholds_classify_their_boundaries_and_one_past() {
        assert_eq!(SizeClass::of(0), SizeClass::Normal);
        assert_eq!(SizeClass::of(LARGE_FILE_BYTES - 1), SizeClass::Normal);
        assert_eq!(
            SizeClass::of(LARGE_FILE_BYTES),
            SizeClass::Normal,
            "exactly 8 MiB is what the measurement said fits"
        );
        assert_eq!(SizeClass::of(LARGE_FILE_BYTES + 1), SizeClass::Large);
        assert_eq!(SizeClass::of(HUGE_FILE_BYTES - 1), SizeClass::Large);
        assert_eq!(SizeClass::of(HUGE_FILE_BYTES), SizeClass::Large);
        assert_eq!(SizeClass::of(HUGE_FILE_BYTES + 1), SizeClass::Huge);
        assert_eq!(SizeClass::of(u64::MAX), SizeClass::Huge);
    }

    #[test]
    fn the_thresholds_are_ordered_and_the_classes_answer_consistently() {
        const { assert!(LARGE_FILE_BYTES < HUGE_FILE_BYTES) };
        assert!(!SizeClass::Normal.is_large());
        assert!(SizeClass::Large.is_large());
        assert!(SizeClass::Huge.is_large());
        assert!(!SizeClass::Large.must_stream());
        assert!(SizeClass::Huge.must_stream());
        assert_eq!(SizeClass::Normal.label(), "", "do not label the ordinary");
    }

    #[test]
    fn read_only_because_huge_is_not_the_same_thing_as_read_only_because_locked() {
        let huge = HUGE_FILE_BYTES + 1;
        assert_eq!(Access::of(0, false), Access::Editable);
        assert_eq!(Access::of(0, true), Access::ReadOnlyFile);
        assert_eq!(
            Access::of(huge, false),
            Access::ReadOnlyBySize { bytes: huge }
        );
        assert_eq!(
            Access::of(huge, true),
            Access::ReadOnlyBySize { bytes: huge },
            "size is what stops the typing, so size is what is reported"
        );
        assert!(!Access::Editable.is_read_only());
        assert!(Access::ReadOnlyFile.is_read_only());
    }

    #[test]
    fn the_user_is_told_which_kind_of_read_only_it_is() {
        assert_eq!(Access::Editable.message(), "");
        let locked = Access::ReadOnlyFile.message();
        assert!(locked.contains("cannot be written"), "{locked}");
        let big = Access::ReadOnlyBySize {
            bytes: 3 * 1024 * 1024 * 1024,
        }
        .message();
        assert!(
            big.contains("3.0 GiB"),
            "the message must say how large: {big}"
        );
        assert!(big.contains("192 MiB"), "and what the limit is: {big}");
        assert_ne!(locked, big, "two reasons, two messages");
    }

    #[test]
    fn opening_reads_nothing() {
        let dir = TempDir::new().unwrap();
        let path = write(&dir, "x.txt", b"hello\nworld\n");
        let file = LargeFile::open(&path).unwrap();
        assert_eq!(file.len_bytes(), 12);
        assert_eq!(
            file.progress().indexed_bytes,
            0,
            "open must not have scanned"
        );
        assert!(!file.progress().complete);
        assert_eq!(file.progress().fraction(), 0.0);
    }

    #[test]
    fn an_empty_file_is_complete_and_has_one_line() {
        let dir = TempDir::new().unwrap();
        let path = write(&dir, "empty.txt", b"");
        let mut file = LargeFile::open(&path).unwrap();
        let progress = file.index_fully().unwrap();
        assert_eq!(progress.lines, 1, "the caret still sits on line 1");
        assert!(progress.complete);
        assert_eq!(progress.fraction(), 1.0, "nothing to do is done");
        assert_eq!(file.line_start(0).unwrap(), Some(0));
        assert_eq!(file.line_start(1).unwrap(), None);
        assert_eq!(
            file.display_lines(0, 10).unwrap(),
            vec![DisplayLine {
                line: 0,
                text: String::new(),
                truncated: false
            }]
        );
    }

    #[test]
    fn every_line_reads_back_exactly_as_an_in_memory_buffer_would() {
        let dir = TempDir::new().unwrap();
        let corpus: &[&str] = &[
            "",
            "\n",
            "a",
            "a\n",
            "one\ntwo\nthree",
            "one\ntwo\nthree\n",
            "\n\n\n",
            "crlf\r\nlines\r\nhere\r\n",
            "日本語\nと🙂\r\nmixed\n",
            "trailing spaces   \nand\ttabs\n",
        ];
        for (n, text) in corpus.iter().enumerate() {
            let path = write(&dir, &format!("c{n}.txt"), text.as_bytes());
            let expected = reference_lines(text);
            let mut file = LargeFile::open(&path).unwrap();

            // Every line, asked for individually and out of order, so the
            // index cannot pass by only ever being walked forwards.
            for start in (0..expected.len()).rev() {
                let got = file.display_lines(start, expected.len()).unwrap();
                let got: Vec<String> = got.into_iter().map(|l| l.text).collect();
                assert_eq!(got, expected[start..], "{text:?} from line {start}");
            }
            assert!(
                file.display_lines(expected.len(), 4).unwrap().is_empty(),
                "one past the last line of {text:?} must be empty"
            );
            assert_eq!(file.index_fully().unwrap().lines, expected.len());
        }
    }

    #[test]
    fn line_starts_agree_with_a_full_scan_including_at_chunk_boundaries() {
        let dir = TempDir::new().unwrap();
        // Lines sized so that line breaks land on, just before and just
        // after multiples of CHUNK_BYTES.
        let mut text = String::new();
        let mut expected = vec![0u64];
        for i in 0..80_000 {
            text.push_str(&format!("{i:width$}\n", width = 7 + i % 3));
            expected.push(text.len() as u64);
        }
        assert!(text.len() > CHUNK_BYTES * 2, "must span several chunks");
        let path = write(&dir, "aligned.txt", text.as_bytes());

        let mut file = LargeFile::open(&path).unwrap();
        for (line, offset) in expected.iter().enumerate() {
            assert_eq!(file.line_start(line).unwrap(), Some(*offset), "line {line}");
        }
        assert_eq!(file.line_start(expected.len()).unwrap(), None);
    }

    #[test]
    fn finding_an_early_line_does_not_index_the_whole_file() {
        let dir = TempDir::new().unwrap();
        let block: String = (0..8_192).map(|i| format!("line {i} of text\n")).collect();
        let path = dir.path().join("long.txt");
        {
            let mut f = File::create(&path).unwrap();
            for _ in 0..64 {
                f.write_all(block.as_bytes()).unwrap();
            }
            f.sync_all().unwrap();
        }
        let len = std::fs::metadata(&path).unwrap().len();

        let mut file = LargeFile::open(&path).unwrap();
        let start = file.line_start(40).unwrap().unwrap();
        assert!(start > 0);
        let indexed = file.progress().indexed_bytes;
        assert!(
            indexed < len / 8,
            "indexed {indexed} of {len} to find line 40 -- that is a full walk"
        );
        assert!(!file.progress().complete);
    }

    #[test]
    fn indexing_can_be_spread_across_calls_without_changing_the_answer() {
        let dir = TempDir::new().unwrap();
        let text: String = (0..20_000).map(|i| format!("row {i}\n")).collect();
        let path = write(&dir, "spread.txt", text.as_bytes());

        let mut one_go = LargeFile::open(&path).unwrap();
        let whole = one_go.index_fully().unwrap();

        let mut dribbled = LargeFile::open(&path).unwrap();
        let mut steps = 0;
        while !dribbled.progress().complete {
            // A byte at a time still advances by a chunk, because a chunk is
            // the granularity of a read; the point is that the caller may
            // stop after any of them.
            dribbled.index_more(1).unwrap();
            steps += 1;
            assert!(steps < 10_000, "indexing must terminate");
        }
        assert!(steps > 1, "the fixture must need more than one call");
        assert_eq!(dribbled.progress(), whole);

        for line in [0usize, 1, 999, 19_999] {
            assert_eq!(
                dribbled.line_start(line).unwrap(),
                one_go.line_start(line).unwrap(),
                "line {line}"
            );
        }
    }

    #[test]
    fn a_file_that_is_one_enormous_line_is_truncated_for_display_and_says_so() {
        let dir = TempDir::new().unwrap();
        let text = "x".repeat(MAX_DISPLAY_LINE_BYTES * 3);
        let path = write(&dir, "one_line.txt", text.as_bytes());

        let mut file = LargeFile::open(&path).unwrap();
        let lines = file.display_lines(0, 4).unwrap();
        assert_eq!(lines.len(), 1, "there is only one line");
        assert!(lines[0].truncated, "and it must admit it was cut");
        assert_eq!(lines[0].text.len(), MAX_DISPLAY_LINE_BYTES);
        assert_eq!(file.index_fully().unwrap().lines, 1);
    }

    #[test]
    fn truncation_never_cuts_a_character_in_half() {
        let dir = TempDir::new().unwrap();
        // Three-byte characters: 64 KiB is not a multiple of three, so the
        // cap lands inside a character.
        let text = "日".repeat(MAX_DISPLAY_LINE_BYTES);
        assert_ne!(MAX_DISPLAY_LINE_BYTES % 3, 0, "the cap must be awkward");
        let path = write(&dir, "cjk_line.txt", text.as_bytes());

        let mut file = LargeFile::open(&path).unwrap();
        let lines = file.display_lines(0, 1).unwrap();
        assert!(lines[0].truncated);
        assert!(lines[0].text.chars().all(|c| c == '日'), "no mojibake");
        assert!(text.starts_with(&lines[0].text), "and a true prefix");
    }

    #[test]
    fn invalid_utf8_in_a_displayed_line_is_an_error_not_mojibake() {
        let dir = TempDir::new().unwrap();
        let path = write(&dir, "bad.txt", b"fine\nbro\xFFken\nfine\n");
        let mut file = LargeFile::open(&path).unwrap();
        assert_eq!(file.display_lines(0, 1).unwrap()[0].text, "fine");
        // Reached two ways -- seeking straight to the bad line, and walking
        // into it from the first line. The offset must be the same both
        // times, which is what pins the running line-start arithmetic.
        for first in [0usize, 1] {
            match file.display_lines(first, 3) {
                Err(LargeFileError::InvalidUtf8 { offset, .. }) => {
                    assert_eq!(offset, 8, "starting from line {first}");
                }
                other => panic!("expected InvalidUtf8 at 8 from line {first}, got {other:?}"),
            }
        }
    }

    #[test]
    fn display_lines_clamps_the_count_it_was_given() {
        let dir = TempDir::new().unwrap();
        let text: String = (0..10).map(|i| format!("{i}\n")).collect();
        let path = write(&dir, "few.txt", text.as_bytes());
        let mut file = LargeFile::open(&path).unwrap();
        assert!(file.display_lines(0, 0).unwrap().is_empty());
        assert_eq!(
            file.display_lines(0, usize::MAX).unwrap().len(),
            11,
            "eleven lines exist; the clamp must not invent more"
        );
    }

    #[test]
    fn display_lines_stops_at_the_byte_cap_rather_than_returning_the_file() {
        let dir = TempDir::new().unwrap();
        // 4,096 lines of 8 KiB is 32 MiB if the line cap were the only cap.
        let line = "y".repeat(8 * 1024);
        let text: String = (0..4_096).map(|_| format!("{line}\n")).collect();
        let path = write(&dir, "wide.txt", text.as_bytes());

        let mut file = LargeFile::open(&path).unwrap();
        let lines = file.display_lines(0, MAX_DISPLAY_LINES).unwrap();
        let returned: usize = lines.iter().map(|l| l.text.len()).sum();
        assert!(
            lines.len() < 4_096,
            "the cap must have stopped it short, got {} lines",
            lines.len()
        );
        assert!(
            returned <= MAX_DISPLAY_BYTES + MAX_DISPLAY_LINE_BYTES,
            "returned {returned} bytes"
        );
        // And the caller can pick up exactly where it stopped.
        let next = file.display_lines(lines.len(), 4).unwrap();
        assert_eq!(next[0].line, lines.len());
        assert_eq!(next[0].text.len(), 8 * 1024);
    }

    #[test]
    fn a_file_that_is_read_only_on_disk_says_so() {
        let dir = TempDir::new().unwrap();
        let path = write(&dir, "locked.txt", b"cannot touch this\n");

        let editable = LargeFile::open(&path).unwrap();
        assert_eq!(editable.access(), Access::Editable);
        assert_eq!(editable.size_class(), SizeClass::Normal);

        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&path, perms).unwrap();

        let locked = LargeFile::open(&path).unwrap();
        assert_eq!(
            locked.access(),
            Access::ReadOnlyFile,
            "the reason must be the file, not its size"
        );

        // Leave it writable so the temporary directory can be removed.
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        std::fs::set_permissions(&path, perms).unwrap();
    }

    #[test]
    fn a_missing_file_is_an_error_naming_it() {
        let dir = TempDir::new().unwrap();
        let error = LargeFile::open(&dir.path().join("nope.txt")).unwrap_err();
        assert!(error.to_string().contains("nope.txt"), "{error}");
    }

    #[test]
    fn memory_stays_bounded_while_indexing_and_displaying_a_large_file() {
        let dir = TempDir::new().unwrap();
        let block: String = (0..16_384).map(|i| format!("line {i} of text\n")).collect();
        let path = dir.path().join("big.txt");
        {
            let mut f = File::create(&path).unwrap();
            for _ in 0..(48 * 1024 * 1024 / block.len() + 1) {
                f.write_all(block.as_bytes()).unwrap();
            }
            f.sync_all().unwrap();
        }
        let len = std::fs::metadata(&path).unwrap().len();

        let mut file = LargeFile::open(&path).unwrap();
        // What the type actually holds, as opposed to the worst case it
        // reserves the right to hold: index plus one chunk.
        let held_bound = MAX_INDEX_HEAP_BYTES + CHUNK_BYTES;
        while !file.progress().complete {
            file.index_more(INDEX_BUDGET_BYTES).unwrap();
            assert!(
                file.resident_bytes() <= held_bound,
                "held {} bytes indexing a {len}-byte file",
                file.resident_bytes()
            );
        }
        assert!(
            (len as usize) > held_bound * 20,
            "the fixture must dwarf the bound"
        );

        // And a viewport anywhere in it costs a viewport, not a file.
        for line in [0usize, 100_000, 500_000, file.progress().lines - 1] {
            let lines = file.display_lines(line, 60).unwrap();
            assert!(!lines.is_empty(), "line {line} must be reachable");
            let held: usize =
                file.resident_bytes() + lines.iter().map(|l| l.text.capacity()).sum::<usize>();
            assert!(held <= LargeFile::memory_bound_bytes(), "held {held}");
        }
    }

    #[test]
    fn the_stated_bound_does_not_depend_on_the_file() {
        // The claim in one assertion: the bound is a function of the
        // configuration, and the configuration has no file in it.
        assert_eq!(
            LargeFile::memory_bound_bytes(),
            MAX_INDEX_HEAP_BYTES
                + CHUNK_BYTES
                + MAX_DISPLAY_LINE_BYTES
                + MAX_DISPLAY_BYTES
                + 2 * MAX_DISPLAY_LINES * size_of::<DisplayLine>()
        );
        assert!(
            LargeFile::memory_bound_bytes() < LARGE_FILE_BYTES as usize,
            "reading a 2 GB file must cost less than loading the smallest \
             file we decline to load ordinarily; it is {} bytes",
            LargeFile::memory_bound_bytes()
        );
    }

    #[test]
    fn streaming_and_the_index_agree_about_line_numbers() {
        let dir = TempDir::new().unwrap();
        let text: String = (0..30_000).map(|i| format!("line {i}\n")).collect();
        let path = write(&dir, "agree.txt", text.as_bytes());

        let mut file = LargeFile::open(&path).unwrap();
        let mut reader = file.windows_with(8192, 128).unwrap();
        let mut windows = 0;
        while let Some(w) = reader.next_window().unwrap() {
            assert_eq!(
                file.line_start(w.first_line).unwrap(),
                Some(w.byte_offset),
                "the index and the stream disagree at window {windows}"
            );
            windows += 1;
        }
        assert!(windows > 10, "only {windows} windows");
    }
}
