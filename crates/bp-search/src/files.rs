//! Searching across files on disk.
//!
//! specs.md section 5 asks for high-performance recursive search. This is the
//! correctness-first version: a plain recursive walk with the guards that
//! actually matter in practice.
//!
//! Those guards are the substance. A naive recursive grep in a developer's
//! home directory reads gigabytes of `target/`, chokes on a binary, and
//! returns fifty thousand hits nobody scrolls through. Each limit here exists
//! because leaving it out makes the feature unusable rather than merely slow.

use std::path::{Path, PathBuf};

use crate::{Query, SearchError, find_all};

/// Directories skipped entirely.
///
/// Build output and dependency trees: enormous, machine-generated, and never
/// what someone means by "search my notes".
const SKIP_DIRS: &[&str] = &[
    ".git",
    ".svn",
    ".hg",
    "target",
    "node_modules",
    ".venv",
    "venv",
    "__pycache__",
    ".mypy_cache",
    ".pytest_cache",
    "dist",
    "build",
    ".next",
    ".cache",
];

/// Files larger than this are skipped.
///
/// A multi-megabyte file in a text search is nearly always a database, a log
/// dump or a minified bundle.
pub const MAX_FILE_BYTES: u64 = 5 * 1024 * 1024;

/// How much of a file to inspect for NUL bytes before deciding it is binary.
const BINARY_SNIFF: usize = 8192;

/// Stop after this many hits.
///
/// A result list nobody can read is the same as no result list, and the walk
/// should not keep burning I/O to build one.
pub const MAX_HITS: usize = 500;

/// Directory depth limit, as a guard against pathological trees and symlink
/// loops that the standard library will happily follow forever.
pub const MAX_DEPTH: usize = 24;

/// One match, in one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileHit {
    pub path: PathBuf,
    /// 1-based line number.
    pub line: usize,
    /// The whole line, trimmed, for display.
    pub preview: String,
    /// Character offset of the match within the file.
    pub offset: usize,
}

/// What a search found, and whether it ran out of room.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSearchReport {
    pub hits: Vec<FileHit>,
    pub files_searched: usize,
    pub files_skipped: usize,
    /// True when the walk stopped at [`MAX_HITS`].
    ///
    /// Surfaced rather than swallowed: "500 results" and "at least 500
    /// results" mean different things to someone deciding whether to refine
    /// their query.
    pub truncated: bool,
}

impl FileSearchReport {
    pub fn summary(&self) -> String {
        let count = if self.truncated {
            format!("{}+ matches", self.hits.len())
        } else {
            format!("{} matches", self.hits.len())
        };
        format!("{count} in {} files searched", self.files_searched)
    }
}

/// Search every text file under `root`.
pub fn search_dir(root: &Path, query: &Query) -> Result<FileSearchReport, SearchError> {
    let mut report = FileSearchReport {
        hits: Vec::new(),
        files_searched: 0,
        files_skipped: 0,
        truncated: false,
    };
    if query.is_empty() {
        return Ok(report);
    }

    walk(root, 0, query, &mut report)?;
    Ok(report)
}

fn walk(
    dir: &Path,
    depth: usize,
    query: &Query,
    report: &mut FileSearchReport,
) -> Result<(), SearchError> {
    if depth > MAX_DEPTH || report.truncated {
        return Ok(());
    }
    // An unreadable directory is skipped, not fatal: one permission error
    // must not abandon the rest of the search.
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(());
    };

    for entry in entries.filter_map(Result::ok) {
        if report.truncated {
            return Ok(());
        }
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };

        if file_type.is_dir() {
            let skip = path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|name| SKIP_DIRS.contains(&name) || name.starts_with('.'));
            if !skip {
                walk(&path, depth + 1, query, report)?;
            }
            continue;
        }

        // Symlinks are not followed: a link back up the tree turns a walk
        // into an infinite one.
        if !file_type.is_file() {
            continue;
        }

        match search_file(&path, query, report) {
            Ok(true) => report.files_searched += 1,
            Ok(false) => report.files_skipped += 1,
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// Search one file. Returns whether it was actually searched.
fn search_file(
    path: &Path,
    query: &Query,
    report: &mut FileSearchReport,
) -> Result<bool, SearchError> {
    let Ok(metadata) = std::fs::metadata(path) else {
        return Ok(false);
    };
    if metadata.len() > MAX_FILE_BYTES {
        return Ok(false);
    }

    let Ok(bytes) = std::fs::read(path) else {
        return Ok(false);
    };
    if is_binary(&bytes) {
        return Ok(false);
    }
    let Ok(text) = String::from_utf8(bytes) else {
        return Ok(false);
    };

    for m in find_all(&text, query)? {
        if report.hits.len() >= MAX_HITS {
            report.truncated = true;
            return Ok(true);
        }
        let preview = text
            .lines()
            .nth(m.line - 1)
            .unwrap_or_default()
            .trim()
            .chars()
            .take(200)
            .collect();
        report.hits.push(FileHit {
            path: path.to_path_buf(),
            line: m.line,
            preview,
            offset: m.range.start,
        });
    }
    Ok(true)
}

/// A NUL byte in the first few kilobytes means binary.
///
/// Crude and standard. Text files do not contain NUL; the formats that do are
/// exactly the ones a text search should not be reading.
fn is_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(BINARY_SNIFF).any(|b| *b == 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write(dir: &Path, name: &str, contents: &str) {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, contents).unwrap();
    }

    #[test]
    fn finds_matches_across_files() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "a.txt", "hello world\nnothing here");
        write(dir.path(), "b.txt", "another hello");

        let report = search_dir(dir.path(), &Query::literal("hello")).unwrap();

        assert_eq!(report.hits.len(), 2);
        assert_eq!(report.files_searched, 2);
        assert!(!report.truncated);
    }

    #[test]
    fn reports_the_line_and_a_preview() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "a.txt", "one\ntwo\n   needle here   \nfour");

        let report = search_dir(dir.path(), &Query::literal("needle")).unwrap();

        assert_eq!(report.hits[0].line, 3);
        assert_eq!(report.hits[0].preview, "needle here", "trimmed for display");
    }

    #[test]
    fn recurses_into_subdirectories() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "deep/deeper/c.txt", "hello down here");

        let report = search_dir(dir.path(), &Query::literal("hello")).unwrap();
        assert_eq!(report.hits.len(), 1);
    }

    #[test]
    fn build_output_and_dot_directories_are_skipped() {
        // The difference between a usable feature and one that reads a
        // gigabyte of target/ before answering.
        let dir = TempDir::new().unwrap();
        write(dir.path(), "keep.txt", "hello");
        write(dir.path(), "target/generated.txt", "hello");
        write(dir.path(), "node_modules/dep.txt", "hello");
        write(dir.path(), ".git/config", "hello");
        write(dir.path(), ".hidden/notes.txt", "hello");

        let report = search_dir(dir.path(), &Query::literal("hello")).unwrap();

        assert_eq!(report.hits.len(), 1);
        assert!(report.hits[0].path.ends_with("keep.txt"));
    }

    #[test]
    fn binary_files_are_skipped() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "text.txt", "hello");
        std::fs::write(dir.path().join("blob.bin"), b"hello\0\0\0binary").unwrap();

        let report = search_dir(dir.path(), &Query::literal("hello")).unwrap();

        assert_eq!(report.hits.len(), 1, "the NUL byte marks it binary");
        assert_eq!(report.files_skipped, 1);
    }

    #[test]
    fn oversized_files_are_skipped() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "small.txt", "hello");
        let big = format!("hello{}", "x".repeat(MAX_FILE_BYTES as usize));
        write(dir.path(), "big.txt", &big);

        let report = search_dir(dir.path(), &Query::literal("hello")).unwrap();

        assert_eq!(report.hits.len(), 1);
        assert_eq!(report.files_skipped, 1);
    }

    #[test]
    fn truncation_is_reported_rather_than_hidden() {
        // "500 results" and "at least 500 results" mean different things to
        // someone deciding whether to refine their query.
        let dir = TempDir::new().unwrap();
        let many = "hello\n".repeat(MAX_HITS + 50);
        write(dir.path(), "many.txt", &many);

        let report = search_dir(dir.path(), &Query::literal("hello")).unwrap();

        assert_eq!(report.hits.len(), MAX_HITS);
        assert!(report.truncated);
        assert!(report.summary().contains('+'), "{}", report.summary());
    }

    #[test]
    fn an_empty_query_searches_nothing() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "a.txt", "hello");

        let report = search_dir(dir.path(), &Query::literal("")).unwrap();
        assert!(report.hits.is_empty());
        assert_eq!(report.files_searched, 0, "no walk at all");
    }

    #[test]
    fn a_missing_directory_is_empty_not_an_error() {
        let dir = TempDir::new().unwrap();
        let report = search_dir(&dir.path().join("nope"), &Query::literal("x")).unwrap();
        assert!(report.hits.is_empty());
    }

    #[test]
    fn a_bad_regex_is_reported_once_rather_than_per_file() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "a.txt", "text");
        write(dir.path(), "b.txt", "text");

        let bad = Query {
            regex: true,
            ..Query::literal("(unclosed")
        };
        assert!(matches!(
            search_dir(dir.path(), &bad),
            Err(SearchError::BadPattern(_))
        ));
    }

    #[test]
    fn regex_and_case_options_carry_across_files() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "a.txt", "Value: 42");
        write(dir.path(), "b.txt", "value: seven");

        let numbers = Query {
            regex: true,
            ..Query::literal(r"value: \d+")
        };
        let report = search_dir(dir.path(), &numbers).unwrap();
        assert_eq!(report.hits.len(), 1, "case-insensitive by default");
        assert!(report.hits[0].path.ends_with("a.txt"));
    }
}
