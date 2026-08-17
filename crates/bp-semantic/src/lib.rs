//! Deterministic extraction: titles, keywords, summaries, outlines.
//!
//! specs.md section 10 describes three layers — deterministic extraction,
//! local statistical models, and optional generative providers. **This crate
//! is layer one and only layer one.** It reads the document in front of it
//! and reports what is demonstrably there.
//!
//! That constraint is ADR-0006, not modesty: core behaviour must work with
//! every semantic and cloud feature disabled, so naming a note cannot depend
//! on a model being loaded or a network being up. Everything here is a pure
//! function of the text, which also means every rule is testable and every
//! answer explainable — the user can see *why* their note got that title.

#![forbid(unsafe_code)]

/// Crate identity used by workspace smoke tests and diagnostics.
pub const CRATE_NAME: &str = "bp-semantic";

/// Longest title we will propose. Long enough to be descriptive, short
/// enough to leave room for the date and extension inside a filename.
pub const MAX_TITLE_CHARS: usize = 60;

/// Words too common to describe anything.
///
/// Deliberately small and English-only. A large list would silently discard
/// terms that matter in a technical note, and a wrong keyword is worse than a
/// dull one.
const STOPWORDS: &[&str] = &[
    "a", "an", "the", "and", "or", "but", "if", "then", "else", "for", "of", "to", "in", "on",
    "at", "by", "with", "from", "as", "is", "are", "was", "were", "be", "been", "being", "it",
    "its", "this", "that", "these", "those", "there", "here", "we", "you", "they", "he", "she",
    "i", "not", "no", "yes", "do", "does", "did", "have", "has", "had", "will", "would", "can",
    "could", "should", "may", "might", "must", "so", "than", "too", "very", "just", "also", "into",
    "over", "under", "about", "after", "before", "when", "while", "how", "what", "why", "which",
    "who", "all", "any", "some", "more", "most", "other", "such", "only", "own", "same",
];

/// A heading found in a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    /// 1 for `#`, 2 for `##`, and so on.
    pub level: usize,
    pub text: String,
    /// 1-based line the heading is on.
    pub line: usize,
}

/// Propose a human-readable title for a document.
///
/// In order of preference: the first Markdown heading, then the first
/// non-empty line. Returns `None` for a document with nothing in it — an
/// empty note has no title, and inventing one would be worse than admitting
/// that.
pub fn suggest_title(text: &str) -> Option<String> {
    let from_heading = outline(text).into_iter().next().map(|h| h.text);
    let candidate = from_heading.or_else(|| {
        text.lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .map(clean_line)
    })?;

    let title = tidy(&candidate);
    (!title.is_empty()).then_some(title)
}

/// Strip the punctuation and markup that decorate a line but do not describe
/// it, so a title taken from prose reads like a title.
fn clean_line(line: &str) -> String {
    let line = line
        .trim_start_matches(['#', '>', '-', '*', '=', '/', ';'])
        .trim();
    // Common comment and front-matter openers.
    let line = line
        .trim_start_matches("//")
        .trim_start_matches("--")
        .trim_start_matches("<!--")
        .trim_end_matches("-->")
        .trim();
    line.to_owned()
}

/// Collapse whitespace and truncate on a word boundary.
fn tidy(text: &str) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= MAX_TITLE_CHARS {
        return collapsed;
    }

    // Cut at the last space inside the budget so a title never ends
    // mid-word; fall back to a hard cut for text with no spaces at all.
    let budget: String = collapsed.chars().take(MAX_TITLE_CHARS).collect();
    match budget.rfind(' ') {
        Some(space) if space > MAX_TITLE_CHARS / 2 => budget[..space].trim_end().to_owned(),
        _ => budget.trim_end().to_owned(),
    }
}

/// Markdown headings, in document order.
///
/// ATX style (`#`) only; setext underlining is ambiguous with horizontal
/// rules and prose, and guessing wrong puts nonsense in the outline.
pub fn outline(text: &str) -> Vec<Heading> {
    text.lines()
        .enumerate()
        .filter_map(|(index, raw)| {
            let line = raw.trim_start();
            if !line.starts_with('#') {
                return None;
            }
            let level = line.chars().take_while(|c| *c == '#').count();
            if level > 6 {
                return None;
            }
            // A heading needs a space after its hashes; `#hashtag` is not one.
            let rest = line[level..].strip_prefix(' ')?.trim();
            (!rest.is_empty()).then(|| Heading {
                level,
                text: rest.trim_end_matches('#').trim().to_owned(),
                line: index + 1,
            })
        })
        .collect()
}

/// The most frequent meaningful words, most common first.
///
/// Frequency only — no weighting, no corpus. It is crude, and being crude is
/// the point: the user can see why each word was chosen.
pub fn keywords(text: &str, limit: usize) -> Vec<String> {
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for word in text.split(|c: char| !c.is_alphanumeric() && c != '-' && c != '_') {
        let word = word.trim_matches(['-', '_']).to_lowercase();
        // Two letters is the shortest that carries meaning ("id", "os");
        // one-letter tokens are noise.
        if word.chars().count() < 2 || STOPWORDS.contains(&word.as_str()) {
            continue;
        }
        // Skip bare numbers: a note full of dates should not be "keyworded"
        // with them.
        if word.chars().all(|c| c.is_numeric()) {
            continue;
        }
        *counts.entry(word).or_default() += 1;
    }

    let mut ranked: Vec<(String, usize)> = counts.into_iter().collect();
    // Frequency first, then alphabetically, so the result is stable rather
    // than dependent on hash order.
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    ranked.into_iter().take(limit).map(|(w, _)| w).collect()
}

/// The first paragraph, collapsed and truncated.
///
/// A paragraph rather than the first N characters: cutting mid-sentence
/// produces a summary that reads like a fault.
pub fn summary(text: &str, max_chars: usize) -> String {
    let mut paragraph = String::new();
    for line in text.lines() {
        let trimmed = line.trim();
        // Skip headings, so the summary is the prose under the title rather
        // than the title again.
        if trimmed.is_empty() || trimmed.starts_with('#') {
            if paragraph.is_empty() {
                continue;
            }
            break;
        }
        if !paragraph.is_empty() {
            paragraph.push(' ');
        }
        paragraph.push_str(trimmed);
    }

    let collapsed = paragraph.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= max_chars {
        return collapsed;
    }
    let budget: String = collapsed.chars().take(max_chars).collect();
    match budget.rfind(' ') {
        Some(space) if space > max_chars / 2 => format!("{}…", budget[..space].trim_end()),
        _ => format!("{}…", budget.trim_end()),
    }
}

/// What a document is made of, for the status bar and the inspector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Statistics {
    pub characters: usize,
    /// Characters excluding whitespace.
    pub characters_no_whitespace: usize,
    pub words: usize,
    pub lines: usize,
    pub paragraphs: usize,
}

/// Count characters, words, lines and paragraphs in `text`.
///
/// The shell hands this the whole document -- after every edit, for the
/// status bar -- so this is one pass over the characters rather than
/// `chars().count()`, `split_whitespace().count()`, `lines().count()` and a
/// separate paragraph scan run back to back over the same text.
pub fn statistics(text: &str) -> Statistics {
    let mut characters = 0usize;
    let mut characters_no_whitespace = 0usize;
    let mut words = 0usize;
    // A document is at least one line even when it is empty -- the caret
    // has to be somewhere -- so this starts at one, not zero.
    let mut lines = 1usize;
    let mut paragraphs = 0usize;

    // A word is a maximal run of alphanumeric characters. A hyphen or
    // apostrophe extends the current word only when it sits between two
    // alphanumerics, so "state-of-the-art" and "don't" each count once;
    // anything else (leading, trailing, or doubled) is ordinary punctuation
    // and ends the word instead of gluing it to whatever follows. Telling
    // those apart needs one character of memory -- was the previous
    // character such a candidate joiner -- rather than lookahead, which is
    // what keeps this a single pass.
    #[derive(PartialEq)]
    enum Word {
        Outside,
        Inside,
        /// Just saw a possible joiner right after `Inside`; not confirmed
        /// until we see what comes next.
        Joining,
    }
    let mut word = Word::Outside;

    // A paragraph is a run of non-blank lines; a line that is empty or
    // whitespace-only separates paragraphs rather than belonging to one.
    // Several blank lines in a row still separate exactly two paragraphs,
    // not several -- a gap is a gap, and counting each blank line would
    // turn "two gaps between three paragraphs" into a number nobody asked
    // for, so `in_paragraph` only flips once per run either way.
    let mut line_has_content = false;
    let mut in_paragraph = false;

    for c in text.chars() {
        // `\r` is line-ending encoding, not content: dropping it here
        // unconditionally (not only as half of "\r\n") means the remaining
        // character stream for a CRLF document is identical to its LF
        // equivalent, so every field below agrees between the two, not
        // just the line count.
        if c == '\r' {
            continue;
        }

        characters += 1;
        if !c.is_whitespace() {
            characters_no_whitespace += 1;
        }

        if c.is_alphanumeric() {
            if word == Word::Outside {
                words += 1;
            }
            word = Word::Inside;
        } else if matches!(c, '-' | '\'' | '\u{2019}') && word == Word::Inside {
            word = Word::Joining;
        } else {
            word = Word::Outside;
        }

        if c == '\n' {
            // Must agree with `bp_buffer::line_count`'s convention: a
            // trailing newline starts a new, empty line, so "a\n" is two
            // lines, not the one `str::lines()` would report. The two
            // implementations must never disagree, or the editor's gutter
            // and this crate's status bar would number the document
            // differently.
            lines += 1;
            if line_has_content {
                if !in_paragraph {
                    paragraphs += 1;
                }
                in_paragraph = true;
            } else {
                in_paragraph = false;
            }
            line_has_content = false;
        } else if !c.is_whitespace() {
            line_has_content = true;
        }
    }
    // The final line has no trailing '\n' to trigger the check above.
    if line_has_content && !in_paragraph {
        paragraphs += 1;
    }

    Statistics {
        characters,
        characters_no_whitespace,
        words,
        lines,
        paragraphs,
    }
}

impl Statistics {
    /// One line for the status bar.
    pub fn summary(&self) -> String {
        format!(
            "{} word{}, {} line{}",
            self.words,
            if self.words == 1 { "" } else { "s" },
            self.lines,
            if self.lines == 1 { "" } else { "s" },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_markdown_heading_becomes_the_title() {
        let text = "# Rust Migration Notes\n\nSome prose here.";
        assert_eq!(suggest_title(text).as_deref(), Some("Rust Migration Notes"));
    }

    #[test]
    fn the_first_line_is_the_fallback() {
        let text = "Meeting with the editors\n\nMore detail below.";
        assert_eq!(
            suggest_title(text).as_deref(),
            Some("Meeting with the editors")
        );
    }

    #[test]
    fn leading_decoration_is_stripped() {
        assert_eq!(suggest_title("// TODO list").as_deref(), Some("TODO list"));
        assert_eq!(
            suggest_title("> A quotation").as_deref(),
            Some("A quotation")
        );
        assert_eq!(suggest_title("- a bullet").as_deref(), Some("a bullet"));
    }

    #[test]
    fn an_empty_document_has_no_title() {
        // Inventing one would be worse than admitting there is none.
        assert_eq!(suggest_title(""), None);
        assert_eq!(suggest_title("   \n\n  \t"), None);
        assert_eq!(
            suggest_title("###"),
            None,
            "decoration alone is not a title"
        );
    }

    #[test]
    fn long_titles_are_cut_on_a_word_boundary() {
        let text = "The quick brown fox jumps over the lazy dog and keeps on running for miles";
        let title = suggest_title(text).unwrap();

        assert!(title.chars().count() <= MAX_TITLE_CHARS);
        assert!(!title.ends_with(' '));
        assert!(
            text.starts_with(&title),
            "a truncated title must still be a prefix of the source"
        );
        assert!(!title.ends_with("runn"), "must not stop mid-word: {title}");
    }

    #[test]
    fn a_title_with_no_spaces_still_fits() {
        let text = "x".repeat(200);
        let title = suggest_title(&text).unwrap();
        assert_eq!(title.chars().count(), MAX_TITLE_CHARS);
    }

    #[test]
    fn whitespace_is_collapsed() {
        assert_eq!(
            suggest_title("Spaced    out\ttitle").as_deref(),
            Some("Spaced out title")
        );
    }

    #[test]
    fn outline_finds_headings_with_levels_and_lines() {
        let text = "# One\n\ntext\n\n## Two\n\n### Three\n";
        let outline = outline(text);

        assert_eq!(outline.len(), 3);
        assert_eq!(
            outline[0],
            Heading {
                level: 1,
                text: "One".to_owned(),
                line: 1
            }
        );
        assert_eq!(outline[1].level, 2);
        assert_eq!(outline[1].line, 5);
        assert_eq!(outline[2].text, "Three");
    }

    #[test]
    fn a_hashtag_is_not_a_heading() {
        // The space after the hashes is what makes it a heading.
        assert!(outline("#hashtag not a heading").is_empty());
        assert_eq!(outline("# real heading").len(), 1);
    }

    #[test]
    fn closing_hashes_are_trimmed() {
        assert_eq!(outline("## Middle ##")[0].text, "Middle");
    }

    #[test]
    fn seven_hashes_is_not_a_heading() {
        assert!(outline("####### too deep").is_empty());
        assert_eq!(outline("###### six is fine").len(), 1);
    }

    #[test]
    fn keywords_rank_by_frequency_and_skip_noise() {
        let text = "rust rust rust the the the editor editor migration 2026 2026 2026";
        let keywords = keywords(text, 3);

        assert_eq!(keywords[0], "rust", "most frequent first");
        assert!(!keywords.contains(&"the".to_owned()), "stopword");
        assert!(
            !keywords.iter().any(|k| k == "2026"),
            "a bare number describes nothing"
        );
    }

    #[test]
    fn keywords_are_stable_rather_than_hash_ordered() {
        // Equal frequencies must not reorder between runs.
        let text = "alpha beta gamma alpha beta gamma";
        let first = keywords(text, 3);
        for _ in 0..5 {
            assert_eq!(keywords(text, 3), first);
        }
        assert_eq!(first, vec!["alpha", "beta", "gamma"]);
    }

    #[test]
    fn the_stopword_list_is_english_and_lowercase() {
        // Guards against a stray word from another language creeping in —
        // it would silently drop a term that matters in someone's notes.
        for word in STOPWORDS {
            assert!(
                word.chars().all(|c| c.is_ascii_lowercase()),
                "{word:?} is not lowercase ASCII"
            );
        }
        let mut sorted = STOPWORDS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), STOPWORDS.len(), "duplicate stopword");
    }

    #[test]
    fn keywords_are_case_insensitive() {
        let keywords = keywords("Rust rust RUST editor", 1);
        assert_eq!(keywords, vec!["rust"]);
    }

    #[test]
    fn summary_takes_the_first_paragraph_not_the_heading() {
        let text = "# Title\n\nThe first paragraph explains things.\nStill the same paragraph.\n\nA second paragraph.";
        let summary = summary(text, 200);

        assert!(summary.starts_with("The first paragraph"));
        assert!(summary.contains("Still the same paragraph"));
        assert!(!summary.contains("second paragraph"), "stops at the break");
        assert!(!summary.contains("Title"), "the heading is not the summary");
    }

    #[test]
    fn a_long_summary_is_elided_on_a_word_boundary() {
        let text = "word ".repeat(100);
        let summary = summary(&text, 40);

        assert!(summary.ends_with('…'));
        assert!(summary.chars().count() <= 41, "budget plus the ellipsis");
        assert!(!summary.contains("wor…"), "must not cut mid-word");
    }

    #[test]
    fn an_empty_document_summarises_to_nothing() {
        assert_eq!(summary("", 100), "");
        assert_eq!(summary("# Only a heading", 100), "");
    }

    #[test]
    fn extraction_never_panics_on_odd_input() {
        for text in [
            "",
            "\n\n\n",
            "###",
            "\u{0}\u{1}",
            "日本語のテキスト",
            &"x".repeat(10_000),
        ] {
            let _ = suggest_title(text);
            let _ = outline(text);
            let _ = keywords(text, 5);
            let _ = summary(text, 50);
            let _ = statistics(text);
        }
    }

    #[test]
    fn an_empty_document_has_one_empty_line_and_nothing_else() {
        let stats = statistics("");
        assert_eq!(stats.characters, 0);
        assert_eq!(stats.characters_no_whitespace, 0);
        assert_eq!(stats.words, 0, "no words in nothing");
        assert_eq!(stats.lines, 1, "the caret has to be somewhere");
        assert_eq!(stats.paragraphs, 0, "no content, no paragraph");
    }

    #[test]
    fn a_single_word_counts_as_one_of_everything_that_matters() {
        let stats = statistics("hello");
        assert_eq!(stats.characters, 5);
        assert_eq!(stats.words, 1);
        assert_eq!(stats.lines, 1, "no newline at all");
        assert_eq!(stats.paragraphs, 1, "one line of content is one paragraph");
    }

    #[test]
    fn a_trailing_newline_still_counts_as_a_line() {
        // str::lines() would say one; the caret can sit on the second,
        // empty line, and the gutter has to number it.
        let stats = statistics("a\n");
        assert_eq!(stats.lines, 2, "str::lines() says 1, and is wrong here");
    }

    #[test]
    fn crlf_documents_count_the_same_as_their_lf_equivalent() {
        let lf = "first paragraph, two lines,\nstill one paragraph.\n\nsecond paragraph.\n";
        let crlf =
            "first paragraph, two lines,\r\nstill one paragraph.\r\n\r\nsecond paragraph.\r\n";

        assert_eq!(
            statistics(lf),
            statistics(crlf),
            "\\r is encoding, not content"
        );
    }

    #[test]
    fn multibyte_text_counts_characters_not_bytes() {
        let text = "日本語";
        let stats = statistics(text);
        assert_eq!(text.len(), 9, "three characters, nine UTF-8 bytes");
        assert_eq!(
            stats.characters, 3,
            "must count characters, not the byte length"
        );
        assert_eq!(stats.characters_no_whitespace, 3);
    }

    #[test]
    fn hyphenated_and_apostrophised_words_count_once_each() {
        let stats = statistics("state-of-the-art don't rock'n'roll");
        assert_eq!(
            stats.words, 3,
            "a joining hyphen or apostrophe must not split a word in two"
        );
    }

    #[test]
    fn punctuation_that_is_not_a_joiner_still_splits_words() {
        // A hyphen or apostrophe only glues a word together when it sits
        // between two alphanumerics; leading, trailing or doubled ones are
        // ordinary punctuation.
        let stats = statistics("'quoted' word--word -dash- end-");
        assert_eq!(stats.words, 5, "quoted, word, word, dash, end");
    }

    #[test]
    fn several_consecutive_blank_lines_are_still_one_separator() {
        let one_gap = statistics("first paragraph\n\nsecond paragraph");
        let many_gaps = statistics("first paragraph\n\n\n\n\nsecond paragraph");

        assert_eq!(one_gap.paragraphs, 2);
        assert_eq!(
            many_gaps.paragraphs, 2,
            "a run of blank lines is one separator, not several"
        );
    }

    #[test]
    fn a_document_of_only_whitespace_has_no_words_or_paragraphs() {
        let stats = statistics("   \n\t\n   ");
        assert_eq!(stats.words, 0);
        assert_eq!(stats.paragraphs, 0, "blank lines are not a paragraph");
        assert_eq!(stats.characters_no_whitespace, 0);
    }

    #[test]
    fn summary_reads_as_one_line_for_the_status_bar() {
        assert_eq!(statistics("hello").summary(), "1 word, 1 line");
        assert_eq!(statistics("hello\nworld").summary(), "2 words, 2 lines");
    }
}
