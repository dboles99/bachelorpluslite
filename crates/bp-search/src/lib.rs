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

use std::ops::Range;

use regex::{Regex, RegexBuilder};
use thiserror::Error;

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

/// Find every match, in document order.
///
/// Returns an empty list for an empty pattern rather than matching
/// everywhere, which is what an empty find box should do.
pub fn find_all(text: &str, query: &Query) -> Result<Vec<Match>, SearchError> {
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let regex = query.compile()?;

    // Byte offset -> char offset, computed once in a single pass rather than
    // re-counting from the start for every match.
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

    Ok(regex
        .find_iter(text)
        .map(|m| {
            let start = byte_to_char[m.start()];
            let end = byte_to_char[m.end()];
            let line = line_starts.partition_point(|s| *s <= start);
            Match {
                range: start..end,
                line,
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

/// Replace every match, returning the new text and how many were replaced.
///
/// For a regex query the replacement honours capture groups (`$1`); for a
/// literal query it is inserted verbatim, so a replacement containing `$`
/// does not silently vanish.
pub fn replace_all(
    text: &str,
    query: &Query,
    replacement: &str,
) -> Result<(String, usize), SearchError> {
    if query.is_empty() {
        return Ok((text.to_owned(), 0));
    }
    let regex = query.compile()?;
    let count = regex.find_iter(text).count();

    let replaced = if query.regex {
        regex.replace_all(text, replacement).into_owned()
    } else {
        // `NoExpand` stops `$1` in the replacement being treated as a group
        // reference when the user meant a literal dollar sign.
        regex
            .replace_all(text, regex::NoExpand(replacement))
            .into_owned()
    };
    Ok((replaced, count))
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
