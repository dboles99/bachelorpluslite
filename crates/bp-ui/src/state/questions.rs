//! What the document in front of you *asks* (ADR-0046).
//!
//! The third feature in the Research menu, and it is neither of the other
//! two. `research.rs` reads the *store* and says what the user has been
//! writing about over time; `citations.rs` reads the *document* and says what
//! it cites; this reads the same document and says what it has not settled.
//!
//! ## Why it is not called "Research Question"
//!
//! `docs/product/MENU_MAP.md` named that row from the day the repository was
//! scaffolded, out of a paper's IMRaD structure. A research question in that
//! sense is *declared* -- it is a thing the author states and the work is
//! then judged against -- and nothing in this product holds one: not
//! `bp-storage`'s schema, which knows a path, a title and tags, and not the
//! text, which never says which of its questions is *the* question.
//!
//! So the row is named for the act it can actually perform, which is the
//! same correction "DOI Lookup" got:
//!
//! > A question has a grammar, so it can be recognised locally; whether it is
//! > the question you are asking is not something the document says.
//!
//! Open Questions surfaces every question the document asks and lets the
//! reader decide which ones are still open. That is a synthesis act under
//! ADR-0039 -- reading gathered material and arranging it -- and it needs
//! neither a schema nor a model.
//!
//! ## Where the rule lives
//!
//! In `bp_semantic::questions`, with the fence handling, the sentence
//! splitting and the Markdown trimming, because that is a decision about text
//! and this crate only decides what to show. `bp-semantic` is layer one of
//! specs.md section 10 -- deterministic extraction -- and every rule in it is
//! explainable to the user, which is exactly what a report like this needs.

use bp_semantic::Question;

use super::AppState;

/// How many questions the report names before it stops and says how many
/// more there were.
///
/// The same cap `IDENTIFIERS_LISTED` (`state/citations.rs`) applies, and for
/// the same reason: a long working note can ask a great many things, and a
/// dialog nobody scrolls to the bottom of has answered nothing.
const QUESTIONS_LISTED: usize = 40;

impl AppState {
    /// Research ▸ Open Questions: every question this document asks.
    pub(crate) fn open_questions_report(&self) -> String {
        question_report(&bp_semantic::questions(&self.active_text()))
    }
}

/// What Open Questions says.
///
/// A free function so its test asserts the product's sentences rather than a
/// copy of them, which is the rule every report in this crate follows.
///
/// **The line number leads each row**, because what the reader does next is
/// Ctrl+G to it. Find Identifiers used to lead the same way and is the reason
/// this one does; it left with `bp-research` (ADR-0060) and the reason did
/// not, because it was about the reader rather than about that row.
pub(crate) fn question_report(found: &[Question]) -> String {
    if found.is_empty() {
        return "This document asks nothing.\n\n\
                A question is a sentence ending in a question mark. Code \
                inside a fenced block is skipped, so a `?` that belongs to a \
                language is not counted as one."
            .to_owned();
    }

    let mut lines = vec![format!(
        "{} question{} in this document. Which of them are still open is not \
         something the document says -- that is the reading.\n",
        found.len(),
        if found.len() == 1 { "" } else { "s" }
    )];
    for question in found.iter().take(QUESTIONS_LISTED) {
        lines.push(format!("{}:  {}", question.line, question.text));
    }
    if found.len() > QUESTIONS_LISTED {
        lines.push(format!(
            "\n… and {} more, not listed.",
            found.len() - QUESTIONS_LISTED
        ));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn question(line: usize, text: &str) -> Question {
        Question {
            text: text.to_owned(),
            line,
        }
    }

    #[test]
    fn a_document_that_asks_nothing_says_so_and_says_what_it_skipped() {
        let report = question_report(&[]);
        assert!(
            report.starts_with("This document asks nothing."),
            "{report}"
        );
        assert!(
            report.contains("fenced block is skipped"),
            "a reader whose questions are all in code has to be told why \
             none were found: {report}"
        );
    }

    #[test]
    fn every_question_is_listed_with_the_line_to_go_to() {
        let report = question_report(&[
            question(3, "Should the tokeniser follow?"),
            question(11, "What breaks if it does not?"),
        ]);
        assert!(report.contains("2 questions"), "{report}");
        assert!(
            report.contains("3:  Should the tokeniser follow?"),
            "{report}"
        );
        assert!(
            report.contains("11:  What breaks if it does not?"),
            "{report}"
        );
    }

    #[test]
    fn one_question_is_not_reported_as_one_questions() {
        assert!(
            question_report(&[question(1, "Why?")]).contains("1 question in"),
            "the plural has to follow the count, as every other report here does"
        );
    }

    #[test]
    fn the_report_never_claims_to_know_which_question_is_the_real_one() {
        let report = question_report(&[question(1, "Why a rope?")]);
        assert!(
            report.contains("still open is not something the document says"),
            "naming this row Open Questions rather than Research Question is \
             the decision, and the report has to carry it: {report}"
        );
    }

    #[test]
    fn a_long_list_is_capped_and_says_how_many_it_did_not_name() {
        let many: Vec<Question> = (1..=QUESTIONS_LISTED + 7)
            .map(|line| question(line, "Is this one?"))
            .collect();
        let report = question_report(&many);
        assert!(report.contains("and 7 more, not listed."), "{report}");
    }

    #[test]
    fn the_report_reads_the_active_document() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("notes.md");
        std::fs::write(&path, "Notes.\n\nShould we ship it?\n").expect("write");

        let mut state = AppState::new();
        state.open(path);

        let report = state.open_questions_report();
        assert!(report.contains("Should we ship it?"), "{report}");
        assert!(
            report.contains("3:"),
            "line 3 is where it is, and the line is what the reader does next: {report}"
        );
    }
}
