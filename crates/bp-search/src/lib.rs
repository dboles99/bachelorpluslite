//! Find and replace within a document.
//!
//! specs.md section 6 of the search requirements: exact and regular
//! expression matching, case sensitivity, whole-word matching, and a replace
//! that can be previewed before it is applied.
//!
//! Offsets are **character** indices, not bytes, because they are handed
//! straight to the editor to select a match. A byte offset would land inside
//! a multi-byte character and select the wrong thing -- or nothing.

#![forbid(unsafe_code)]

use std::fmt::Write as _;
use std::ops::Range;

use regex::{Regex, RegexBuilder};
use thiserror::Error;

mod files;
pub use files::{FileHit, FileSearchReport, MAX_FILE_BYTES, MAX_HITS, search_dir};

/// Crate identity used by workspace smoke tests and diagnostics.
pub const CRATE_NAME: &str = "bp-search";

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SearchError {
    #[error("not a valid regular expression: {0}")]
    BadPattern(String),
}

/// What to look for and how.
///
/// The defaults are the find-box defaults: literal text, case-insensitive,
/// not word-bounded. Case-insensitive is what people expect from a find box;
/// the surprising direction is the other one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    pub pattern: String,
    pub case_sensitive: bool,
    pub whole_word: bool,
    /// Treat the pattern as a regular expression rather than literal text.
    pub regex: bool,
}

impl Query {
    pub fn literal(pattern: &str) -> Self {
        Self {
            pattern: pattern.to_owned(),
            ..Self::default()
        }
    }

    pub fn is_empty(&self) -> bool {
        self.pattern.is_empty()
    }

    /// Build the regex this query means.
    ///
    /// A literal query is escaped, so searching for `a.b` finds `a.b` and not
    /// `axb`. That is the whole difference between a find box and a footgun.
    fn compile(&self) -> Result<Regex, SearchError> {
        let base = if self.regex {
            self.pattern.clone()
        } else {
            regex::escape(&self.pattern)
        };
        let pattern = if self.whole_word {
            format!(r"\b(?:{base})\b")
        } else {
            base
        };

        RegexBuilder::new(&pattern)
            .case_insensitive(!self.case_sensitive)
            .build()
            .map_err(|e| SearchError::BadPattern(e.to_string()))
    }
}

/// Where a match is, in character offsets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub range: Range<usize>,
    /// 1-based line the match starts on, for reporting.
    pub line: usize,
    /// The matched text.
    pub text: String,
}

/// Byte offsets to character offsets, and character offsets to line numbers.
///
/// Built once per pass rather than re-counting from the start of the document
/// for every match, which is the difference between linear and quadratic on a
/// document with many hits.
struct Offsets {
    byte_to_char: Vec<usize>,
    /// Character offset at which each line starts.
    line_starts: Vec<usize>,
}

impl Offsets {
    fn of(text: &str) -> Self {
        let mut byte_to_char = vec![0usize; text.len() + 1];
        let mut chars = 0;
        for (byte_index, _) in text.char_indices() {
            byte_to_char[byte_index] = chars;
            chars += 1;
        }
        byte_to_char[text.len()] = chars;

        let mut line_starts = vec![0usize];
        for (index, ch) in text.char_indices() {
            if ch == '\n' {
                line_starts.push(byte_to_char[index] + 1);
            }
        }

        Self {
            byte_to_char,
            line_starts,
        }
    }

    fn char_at(&self, byte: usize) -> usize {
        self.byte_to_char[byte]
    }

    /// 1-based line containing a character offset.
    fn line_at(&self, char_offset: usize) -> usize {
        self.line_starts
            .partition_point(|start| *start <= char_offset)
    }
}

/// Find every match, in document order.
///
/// Returns an empty list for an empty pattern rather than matching
/// everywhere, which is what an empty find box should do.
pub fn find_all(text: &str, query: &Query) -> Result<Vec<Match>, SearchError> {
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let regex = query.compile()?;
    let offsets = Offsets::of(text);

    Ok(regex
        .find_iter(text)
        .map(|m| {
            let start = offsets.char_at(m.start());
            Match {
                range: start..offsets.char_at(m.end()),
                line: offsets.line_at(start),
                text: m.as_str().to_owned(),
            }
        })
        .collect())
}

/// Count matches without materialising them.
pub fn count(text: &str, query: &Query) -> Result<usize, SearchError> {
    if query.is_empty() {
        return Ok(0);
    }
    Ok(query.compile()?.find_iter(text).count())
}

/// The match at or after `from`, wrapping to the start.
///
/// Wrapping is what a find box does; stopping at the end and making the user
/// press Home first is not.
pub fn next_match(matches: &[Match], from: usize) -> Option<&Match> {
    matches
        .iter()
        .find(|m| m.range.start >= from)
        .or_else(|| matches.first())
}

/// The match before `from`, wrapping to the end.
pub fn previous_match(matches: &[Match], from: usize) -> Option<&Match> {
    matches
        .iter()
        .rev()
        .find(|m| m.range.start < from)
        .or_else(|| matches.last())
}

/// One replacement a plan would make.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Replacement {
    /// Where in the *original* document, in character offsets.
    pub range: Range<usize>,
    /// 1-based line the match starts on.
    pub line: usize,
    /// The text that would be removed.
    pub before: String,
    /// The text that would take its place, with any capture groups already
    /// expanded. What the user sees here is literally what gets written.
    pub after: String,
}

/// What a replace would do, worked out before anything changes.
///
/// specs.md section 6 wants a replace that can be previewed. Building the new
/// document and the list of changes in the same pass is what makes the
/// preview trustworthy: there is no second implementation that could disagree
/// with the first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplacePlan {
    /// Every replacement, in document order.
    pub replacements: Vec<Replacement>,
    replaced: String,
}

impl ReplacePlan {
    pub fn count(&self) -> usize {
        self.replacements.len()
    }

    pub fn is_empty(&self) -> bool {
        self.replacements.is_empty()
    }

    /// The document as it would be once applied.
    pub fn apply(self) -> String {
        self.replaced
    }

    /// One line describing the plan, for a confirmation prompt.
    pub fn summary(&self) -> String {
        match self.replacements.len() {
            0 => "Nothing to replace.".to_owned(),
            1 => {
                let only = &self.replacements[0];
                format!("Replace 1 match, on line {}.", only.line)
            }
            n => {
                let first = self.replacements[0].line;
                let last = self.replacements[n - 1].line;
                format!("Replace {n} matches, on lines {first} to {last}.")
            }
        }
    }

    /// The first `limit` replacements rendered as `Ln 12  before → after`.
    ///
    /// Truncated because a preview of 40,000 replacements is not a preview.
    /// The last line says how many were not shown rather than leaving the
    /// user to infer it from the count.
    pub fn preview(&self, limit: usize) -> String {
        let mut out = String::new();
        for change in self.replacements.iter().take(limit) {
            let _ = writeln!(
                out,
                "Ln {}  {} → {}",
                change.line,
                elide(&change.before),
                elide(&change.after)
            );
        }
        if let Some(hidden) = self
            .replacements
            .len()
            .checked_sub(limit)
            .filter(|n| *n > 0)
        {
            let _ = writeln!(out, "… and {hidden} more");
        }
        out
    }
}

/// A single-line, length-bounded rendering of a match for the preview list.
///
/// A match containing newlines would otherwise break the one-change-per-line
/// layout, and a very long one would push the arrow off the panel.
fn elide(text: &str) -> String {
    const MAX: usize = 60;
    let flat = text.replace('\n', "⏎").replace('\r', "");
    if flat.chars().count() <= MAX {
        return flat;
    }
    let kept: String = flat.chars().take(MAX - 1).collect();
    format!("{kept}…")
}

/// Work out what replacing every match would do, without doing it.
///
/// For a regex query the replacement honours capture groups (`$1`); for a
/// literal query it is inserted verbatim, so a replacement containing `$`
/// does not silently vanish.
pub fn plan_replace_all(
    text: &str,
    query: &Query,
    replacement: &str,
) -> Result<ReplacePlan, SearchError> {
    if query.is_empty() {
        return Ok(ReplacePlan {
            replacements: Vec::new(),
            replaced: text.to_owned(),
        });
    }
    let regex = query.compile()?;
    let offsets = Offsets::of(text);

    let mut replacements = Vec::new();
    let mut replaced = String::with_capacity(text.len());
    let mut copied_to = 0;

    for captures in regex.captures_iter(text) {
        // Group 0 is the whole match, which always exists.
        let whole = captures.get(0).expect("group 0 is the match itself");

        let mut after = String::new();
        if query.regex {
            captures.expand(replacement, &mut after);
        } else {
            // A literal query takes the replacement verbatim: `$1` there means
            // a dollar and a one, not a group reference that expands to
            // nothing.
            after.push_str(replacement);
        }

        replaced.push_str(&text[copied_to..whole.start()]);
        replaced.push_str(&after);
        copied_to = whole.end();

        let start = offsets.char_at(whole.start());
        replacements.push(Replacement {
            range: start..offsets.char_at(whole.end()),
            line: offsets.line_at(start),
            before: whole.as_str().to_owned(),
            after,
        });
    }
    replaced.push_str(&text[copied_to..]);

    Ok(ReplacePlan {
        replacements,
        replaced,
    })
}

/// Replace every match, returning the new text and how many were replaced.
pub fn replace_all(
    text: &str,
    query: &Query,
    replacement: &str,
) -> Result<(String, usize), SearchError> {
    let plan = plan_replace_all(text, query, replacement)?;
    let count = plan.count();
    Ok((plan.apply(), count))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(pattern: &str) -> Query {
        Query::literal(pattern)
    }

    #[test]
    fn finds_every_occurrence_in_order() {
        let matches = find_all("one two one", &q("one")).unwrap();
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].range, 0..3);
        assert_eq!(matches[1].range, 8..11);
    }

    #[test]
    fn an_empty_pattern_matches_nothing() {
        // An empty find box must not select the whole document.
        assert!(find_all("anything", &q("")).unwrap().is_empty());
        assert_eq!(count("anything", &q("")).unwrap(), 0);
    }

    #[test]
    fn matching_is_case_insensitive_by_default() {
        assert_eq!(find_all("Hello hello HELLO", &q("hello")).unwrap().len(), 3);

        let sensitive = Query {
            case_sensitive: true,
            ..q("hello")
        };
        assert_eq!(find_all("Hello hello HELLO", &sensitive).unwrap().len(), 1);
    }

    #[test]
    fn a_literal_query_does_not_smuggle_in_regex() {
        // The difference between a find box and a footgun.
        let matches = find_all("axb and a.b", &q("a.b")).unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].text, "a.b");
    }

    #[test]
    fn regex_mode_is_opt_in_and_reports_bad_patterns() {
        let re = Query {
            regex: true,
            ..q(r"\d+")
        };
        assert_eq!(find_all("a1 b22 c333", &re).unwrap().len(), 3);

        let bad = Query {
            regex: true,
            ..q("(unclosed")
        };
        assert!(matches!(
            find_all("text", &bad),
            Err(SearchError::BadPattern(_))
        ));
    }

    #[test]
    fn whole_word_matching_respects_boundaries() {
        let whole = Query {
            whole_word: true,
            ..q("cat")
        };
        let matches = find_all("cat concatenate cat.", &whole).unwrap();
        assert_eq!(matches.len(), 2, "concatenate must not match");
    }

    #[test]
    fn offsets_are_characters_not_bytes() {
        // Handed straight to the editor to select; a byte offset would select
        // the wrong text or split a character.
        let text = "日本語 cat";
        let matches = find_all(text, &q("cat")).unwrap();
        assert_eq!(matches[0].range, 4..7);
        assert_eq!(text.chars().skip(4).take(3).collect::<String>(), "cat");
    }

    #[test]
    fn line_numbers_are_one_based_and_correct() {
        let text = "alpha\nbeta\ngamma beta";
        let matches = find_all(text, &q("beta")).unwrap();
        assert_eq!(matches[0].line, 2);
        assert_eq!(matches[1].line, 3);
    }

    #[test]
    fn next_and_previous_wrap() {
        let text = "a x a x a";
        let matches = find_all(text, &q("a")).unwrap();
        assert_eq!(matches.len(), 3);

        assert_eq!(next_match(&matches, 0).unwrap().range, 0..1);
        assert_eq!(next_match(&matches, 1).unwrap().range, 4..5);
        assert_eq!(
            next_match(&matches, 9).unwrap().range,
            0..1,
            "past the end wraps to the first"
        );
        assert_eq!(
            previous_match(&matches, 0).unwrap().range,
            8..9,
            "before the start wraps to the last"
        );
    }

    #[test]
    fn next_and_previous_on_no_matches_is_none() {
        assert!(next_match(&[], 0).is_none());
        assert!(previous_match(&[], 0).is_none());
    }

    #[test]
    fn replace_all_reports_how_many_it_changed() {
        let (text, n) = replace_all("one two one", &q("one"), "1").unwrap();
        assert_eq!(text, "1 two 1");
        assert_eq!(n, 2);
    }

    #[test]
    fn a_literal_replacement_containing_a_dollar_survives() {
        // `$1` in a literal replacement means a dollar and a one, not a
        // capture group that expands to nothing.
        let (text, n) = replace_all("price here", &q("price"), "$1.00").unwrap();
        assert_eq!(text, "$1.00 here");
        assert_eq!(n, 1);
    }

    #[test]
    fn a_regex_replacement_can_use_capture_groups() {
        let re = Query {
            regex: true,
            ..q(r"(\w+)@(\w+)")
        };
        let (text, n) = replace_all("a@b", &re, "$2@$1").unwrap();
        assert_eq!(text, "b@a");
        assert_eq!(n, 1);
    }

    #[test]
    fn replacing_nothing_leaves_the_text_alone() {
        let (text, n) = replace_all("unchanged", &q("absent"), "x").unwrap();
        assert_eq!(text, "unchanged");
        assert_eq!(n, 0);

        let (text, n) = replace_all("unchanged", &q(""), "x").unwrap();
        assert_eq!(
            text, "unchanged",
            "an empty pattern must not touch anything"
        );
        assert_eq!(n, 0);
    }

    #[test]
    fn a_plan_says_what_it_would_change_before_changing_it() {
        let text = "one two\none three";
        let plan = plan_replace_all(text, &q("one"), "1").unwrap();

        assert_eq!(plan.count(), 2);
        assert_eq!(plan.replacements[0].range, 0..3);
        assert_eq!(plan.replacements[0].line, 1);
        assert_eq!(plan.replacements[1].line, 2);
        assert_eq!(plan.replacements[0].before, "one");
        assert_eq!(plan.replacements[0].after, "1");

        assert_eq!(text, "one two\none three", "planning must not mutate");
        assert_eq!(plan.apply(), "1 two\n1 three");
    }

    #[test]
    fn the_preview_shows_expanded_capture_groups_not_the_template() {
        // Showing the user `$2@$1` would be showing them the recipe rather
        // than the result, which is not a preview.
        let re = Query {
            regex: true,
            ..q(r"(\w+)@(\w+)")
        };
        let plan = plan_replace_all("a@b", &re, "$2@$1").unwrap();

        assert_eq!(plan.replacements[0].after, "b@a");
        assert_eq!(plan.apply(), "b@a");
    }

    #[test]
    fn what_the_plan_shows_is_what_applying_produces() {
        // The property that makes a preview worth trusting: splicing the
        // listed replacements into the original reproduces the applied text.
        let text = "alpha beta alpha gamma alpha";
        let plan = plan_replace_all(text, &q("alpha"), "X").unwrap();

        let mut spliced = String::new();
        let mut copied = 0;
        for change in &plan.replacements {
            spliced.extend(text.chars().skip(copied).take(change.range.start - copied));
            spliced.push_str(&change.after);
            copied = change.range.end;
        }
        spliced.extend(text.chars().skip(copied));

        assert_eq!(spliced, plan.apply());
    }

    #[test]
    fn an_empty_plan_still_carries_the_original_document() {
        let plan = plan_replace_all("unchanged", &q("absent"), "x").unwrap();
        assert!(plan.is_empty());
        assert_eq!(plan.summary(), "Nothing to replace.");
        assert_eq!(plan.apply(), "unchanged");

        let plan = plan_replace_all("unchanged", &q(""), "x").unwrap();
        assert!(plan.is_empty(), "an empty pattern plans nothing");
        assert_eq!(plan.apply(), "unchanged");
    }

    #[test]
    fn the_summary_names_the_lines_involved() {
        let plan = plan_replace_all("a\nb\na", &q("a"), "z").unwrap();
        assert_eq!(plan.summary(), "Replace 2 matches, on lines 1 to 3.");

        let one = plan_replace_all("only here", &q("only"), "z").unwrap();
        assert_eq!(one.summary(), "Replace 1 match, on line 1.");
    }

    #[test]
    fn the_preview_truncates_and_says_how_much_it_hid() {
        let text = "x\n".repeat(10);
        let preview = plan_replace_all(&text, &q("x"), "y").unwrap().preview(3);

        assert_eq!(preview.lines().count(), 4, "three changes and a tail");
        assert!(preview.starts_with("Ln 1  x → y"), "got {preview}");
        assert!(preview.ends_with("… and 7 more\n"), "got {preview}");
    }

    #[test]
    fn the_preview_keeps_one_change_to_one_line() {
        // A multi-line match would otherwise break the layout, and a very
        // long one would push the arrow off the panel.
        let re = Query {
            regex: true,
            ..q(r"(?s)a.*c")
        };
        let preview = plan_replace_all("a\nb\nc", &re, "z").unwrap().preview(10);
        assert_eq!(preview.lines().count(), 1);
        assert!(preview.contains("a⏎b⏎c"), "got {preview}");

        let long = "q".repeat(200);
        let preview = plan_replace_all(&long, &q(&long), "z").unwrap().preview(10);
        assert_eq!(preview.lines().count(), 1);
        assert!(preview.contains('…'), "got {preview}");
    }

    #[test]
    fn searching_a_large_document_stays_reasonable() {
        // The byte-to-char map is built once per search, not per match.
        let text = "needle in a haystack. ".repeat(50_000);
        let matches = find_all(&text, &q("needle")).unwrap();
        assert_eq!(matches.len(), 50_000);
        assert_eq!(matches[1].range.start, 22);
    }

    #[test]
    fn empty_text_finds_nothing_without_panicking() {
        assert!(find_all("", &q("anything")).unwrap().is_empty());
        assert_eq!(replace_all("", &q("a"), "b").unwrap(), (String::new(), 0));
    }
}
