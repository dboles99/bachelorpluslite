//! What the find bar is currently looking for, and where it got to.
//!
//! `bp-search` decides what a match *is*. This module owns the two things it
//! deliberately does not: which options the bar has switched on, and which of
//! the matches the user is standing on.
//!
//! ## Why the query is built in one place
//!
//! [`find_query`] exists so that every search in the product agrees about the
//! options. A find that honours the regex toggle and a Replace All that
//! quietly does not is worse than neither honouring it, and that is not a
//! defect `bp-search` can catch -- `case_sensitive` and `whole_word` are both
//! `bool`, so swapping them at a call site compiles and passes every test in
//! that crate. The test below is what stands in for a type.

use super::AppState;

impl AppState {
    /// Recompute matches for `query` against the active document.
    ///
    /// Returns the range to select, if there is one.
    pub(crate) fn find(&mut self, query: &bp_search::Query) -> Option<std::ops::Range<usize>> {
        self.match_index = 0;
        if query.is_empty() {
            self.matches.clear();
            self.find_status.clear();
            return None;
        }
        let text = self.active_text();
        match bp_search::find_all(&text, query) {
            Ok(found) => {
                self.matches = found;
                self.find_status = if self.matches.is_empty() {
                    "no matches".to_owned()
                } else {
                    format!("1 of {}", self.matches.len())
                };
                self.matches.first().map(|m| m.range.clone())
            }
            Err(e) => {
                // A half-typed regex is the normal case while typing, so this
                // reports rather than alarms.
                self.matches.clear();
                self.find_status = e.to_string();
                None
            }
        }
    }

    /// Step to the next or previous match, wrapping.
    pub(crate) fn step_match(&mut self, forward: bool) -> Option<std::ops::Range<usize>> {
        if self.matches.is_empty() {
            return None;
        }
        let len = self.matches.len();
        self.match_index = if forward {
            (self.match_index + 1) % len
        } else {
            (self.match_index + len - 1) % len
        };
        self.find_status = format!("{} of {len}", self.match_index + 1);
        Some(self.matches[self.match_index].range.clone())
    }
}

/// Build the query the find bar currently describes.
///
/// One function rather than three `Query` literals, because every place that
/// searches has to agree about the options -- a find that honours the regex
/// toggle and a Replace All that quietly does not is worse than neither
/// honouring it.
///
/// What `bp-search` cannot catch is a toggle landing in the wrong field:
/// `case_sensitive` and `whole_word` are both `bool`, so swapping them
/// compiles, passes every test in that crate, and gives the user a whole-word
/// switch that changes the case rules. That is what the test below is for.
pub(crate) fn find_query(
    pattern: &str,
    case_sensitive: bool,
    whole_word: bool,
    regex: bool,
) -> bp_search::Query {
    bp_search::Query {
        pattern: pattern.to_owned(),
        case_sensitive,
        whole_word,
        regex,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_find_option_reaches_its_own_field_and_no_other() {
        // Set one at a time, so a pair swapped in the constructor fails here
        // rather than compiling into a switch that changes the wrong thing.
        let case = find_query("x", true, false, false);
        assert!(case.case_sensitive);
        assert!(!case.whole_word, "case sensitivity must not imply words");
        assert!(!case.regex, "case sensitivity must not imply a pattern");

        let word = find_query("x", false, true, false);
        assert!(word.whole_word);
        assert!(!word.case_sensitive);
        assert!(!word.regex);

        let re = find_query("x", false, false, true);
        assert!(re.regex);
        assert!(!re.case_sensitive);
        assert!(!re.whole_word);
    }

    #[test]
    fn all_options_off_is_the_literal_query_the_find_box_used_to_build() {
        // The default the bar opens with. Everything searched before the
        // toggles existed went through `Query::literal`, so this is the
        // guarantee that adding them changed nothing for anyone who does not
        // touch them.
        assert_eq!(
            find_query("a.b", false, false, false),
            bp_search::Query::literal("a.b")
        );
    }

    #[test]
    fn the_regex_toggle_is_what_stops_a_pattern_being_escaped() {
        // The behaviour is `bp-search`'s and tested there; what this pins is
        // that the toggle reaches it at all. `a.b` matching `axb` is only
        // possible when `regex` arrived as true, so this fails if the flag is
        // dropped on the way through.
        let literal = find_query("a.b", false, false, false);
        let pattern = find_query("a.b", false, false, true);

        assert!(bp_search::find_all("axb", &literal).unwrap().is_empty());
        assert_eq!(bp_search::find_all("axb", &pattern).unwrap().len(), 1);
    }
}
