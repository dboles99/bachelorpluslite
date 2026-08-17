//! Whole-document line operations (specs.md section 4).
//!
//! Pure `&str -> String`, deliberately: these are the line operations that
//! need no caret, so they are useful before a custom editor view exists and
//! testable without one. Duplicate Line and Move Line Up/Down are the same
//! family but need to know where the caret is, and wait for that view.
//!
//! Two things every operation here preserves, because getting either wrong
//! rewrites every line of someone's file as a side effect of sorting it:
//!
//! * **CRLF.** `bp-files` records the line ending it found but leaves the
//!   text as it was on disk, so the buffer really can hold `\r\n`.
//! * **The final newline, or its absence.** A file that ended without one
//!   still does; a file that ended with one is not silently truncated.

use std::cmp::Reverse;
use std::collections::HashSet;

/// Which way to sort.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Order {
    Ascending,
    Descending,
}

/// How a document separates its lines, and how it ends.
struct Shape<'a> {
    separator: &'a str,
    /// `"\r\n"`, `"\n"`, or `""` when the document ends mid-line.
    trailing: &'a str,
}

impl<'a> Shape<'a> {
    /// A single `\r\n` anywhere makes the document CRLF. Mixed endings have
    /// to converge on one answer, and converging on the Windows one is right
    /// far more often than the reverse: a `\n` inside a CRLF file is usually
    /// damage, whereas a `\r\n` inside an LF file is usually the whole file.
    fn of(text: &'a str) -> Self {
        Self {
            separator: if text.contains("\r\n") { "\r\n" } else { "\n" },
            trailing: if text.ends_with("\r\n") {
                "\r\n"
            } else if text.ends_with('\n') {
                "\n"
            } else {
                ""
            },
        }
    }

    fn rejoin(&self, lines: &[&str]) -> String {
        let mut out = lines.join(self.separator);
        out.push_str(self.trailing);
        out
    }
}

/// Apply `transform` to the document's lines and put them back together.
///
/// `str::lines` strips the `\r` of a CRLF pair, so the transform sees line
/// content rather than line content plus punctuation -- which is what makes
/// `"a\r"` and `"a"` compare equal in a mixed-ending file.
fn on_lines(text: &str, transform: impl FnOnce(&mut Vec<&str>)) -> String {
    let shape = Shape::of(text);
    let mut lines: Vec<&str> = text.lines().collect();
    transform(&mut lines);
    shape.rejoin(&lines)
}

/// Sort the lines.
///
/// Case-insensitively: a user who sorts a list of names does not expect
/// `Zebra` before `apple`, which is what byte order gives. The sort is
/// stable, so lines differing only in case keep the order they were in.
pub fn sort(text: &str, order: Order) -> String {
    on_lines(text, |lines| match order {
        Order::Ascending => lines.sort_by_cached_key(|line| line.to_lowercase()),
        Order::Descending => lines.sort_by_cached_key(|line| Reverse(line.to_lowercase())),
    })
}

/// Remove repeated lines, keeping the first of each.
///
/// Keeping the first rather than the last preserves the order of what
/// survives, so a deduplicated list is still recognisably the original.
/// Comparison is exact -- case and whitespace included -- because a
/// deduplicate that discards a line differing only in case is discarding data
/// the user can no longer see was there.
pub fn remove_duplicates(text: &str) -> String {
    on_lines(text, |lines| {
        let mut seen = HashSet::new();
        lines.retain(|line| seen.insert(*line));
    })
}

/// Reverse the order of the lines.
pub fn reverse(text: &str) -> String {
    on_lines(text, |lines| lines.reverse())
}

/// Strip trailing spaces and tabs from every line.
pub fn trim_trailing_whitespace(text: &str) -> String {
    on_lines(text, |lines| {
        for line in lines.iter_mut() {
            *line = line.trim_end_matches([' ', '\t']);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorting_is_case_insensitive() {
        // Byte order would put every capital first and read as broken.
        assert_eq!(
            sort("banana\nApple\ncherry", Order::Ascending),
            "Apple\nbanana\ncherry"
        );
    }

    #[test]
    fn sorting_descending_reverses_the_order() {
        assert_eq!(
            sort("Apple\nbanana\ncherry", Order::Descending),
            "cherry\nbanana\nApple"
        );
    }

    #[test]
    fn a_trailing_newline_survives_every_operation() {
        // Losing it rewrites the last line of the file for no reason the user
        // asked for, and shows up as a spurious diff.
        assert_eq!(sort("b\na\n", Order::Ascending), "a\nb\n");
        assert_eq!(remove_duplicates("a\na\n"), "a\n");
        assert_eq!(reverse("a\nb\n"), "b\na\n");
        assert_eq!(trim_trailing_whitespace("a  \n"), "a\n");
    }

    #[test]
    fn a_document_ending_mid_line_still_does() {
        assert_eq!(sort("b\na", Order::Ascending), "a\nb");
        assert_eq!(remove_duplicates("a\na"), "a");
        assert_eq!(reverse("a\nb"), "b\na");
    }

    #[test]
    fn crlf_documents_stay_crlf() {
        // `bp-files` leaves the text as it was on disk, so this is not
        // hypothetical: sorting must not convert the whole file to LF.
        assert_eq!(sort("b\r\na\r\n", Order::Ascending), "a\r\nb\r\n");
        assert_eq!(reverse("a\r\nb\r\n"), "b\r\na\r\n");
        assert_eq!(trim_trailing_whitespace("a \r\nb\r\n"), "a\r\nb\r\n");
    }

    #[test]
    fn duplicates_are_removed_keeping_the_first_occurrence() {
        assert_eq!(remove_duplicates("b\na\nb\nc\na"), "b\na\nc");
    }

    #[test]
    fn deduplicating_is_case_sensitive() {
        // Two lines differing only in case are two different lines; throwing
        // one away silently would discard data the user cannot see went.
        assert_eq!(remove_duplicates("Apple\napple"), "Apple\napple");
    }

    #[test]
    fn trimming_leaves_leading_and_interior_whitespace_alone() {
        assert_eq!(
            trim_trailing_whitespace("  indented  \n\tkept\there\t\n"),
            "  indented\n\tkept\there\n"
        );
    }

    #[test]
    fn blank_lines_are_kept_rather_than_tidied_away() {
        // Sorting is not a licence to delete empty lines.
        assert_eq!(sort("b\n\na\n", Order::Ascending), "\na\nb\n");
        assert_eq!(reverse("a\n\nb"), "b\n\na");
    }

    #[test]
    fn an_empty_document_survives_every_operation() {
        for text in ["", "\n", "\r\n"] {
            assert_eq!(sort(text, Order::Ascending), text, "sort on {text:?}");
            assert_eq!(remove_duplicates(text), text, "dedup on {text:?}");
            assert_eq!(reverse(text), text, "reverse on {text:?}");
            assert_eq!(trim_trailing_whitespace(text), text, "trim on {text:?}");
        }
    }

    #[test]
    fn a_single_line_is_returned_unchanged() {
        assert_eq!(sort("only", Order::Ascending), "only");
        assert_eq!(remove_duplicates("only"), "only");
    }

    #[test]
    fn sorting_is_stable_for_lines_differing_only_in_case() {
        assert_eq!(
            sort("beta\nBeta\nalpha", Order::Ascending),
            "alpha\nbeta\nBeta",
            "the pair keeps the order it arrived in"
        );
    }
}
