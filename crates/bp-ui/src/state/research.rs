//! Research mode's synthesis layer (ADR-0041, extended by ADR-0046): plain,
//! honest aggregate reads over `bp-storage`'s existing schema, worded as
//! insights rather than bare statistics.
//!
//! ## Three of `MENU_MAP.md`'s five planned names live here
//!
//! ADR-0046 scoped them. **Evidence** is not a row: it is the documents each
//! insight is drawn from, named on the insight itself, because a claim and
//! what backs it are one thing to read. **Methods** and **datasets** are not
//! rows either: they are the closing section, which states every threshold
//! this module applies and every number it applied them over -- the
//! denominator an aggregate needs as much as a percentage does. **Findings**
//! was dropped as a second name for what the four insights already are, and
//! **research question** became a document reading, in `state/questions.rs`.
//!
//! No new store and no new field -- `AppState::store` is the same
//! `Option<bp_storage::Store>` `organize.rs` already added for ADR-0037.
//! This module only reads it, through `tag_frequency` (added here, in
//! `bp-storage`) plus the query functions Organize already reads through
//! (`documents_tagged`, `related_to`, `recent_documents`). Every "insight"
//! below is a fixed, inspectable rule -- ADR-0041 is explicit this is not a
//! recommendation engine with a model behind it.

use bp_storage::{DocumentRecord, Store};

use super::AppState;

/// How many of the most-recently-seen documents Research ▸ Research Report
/// scans when looking for under-connected documents.
///
/// A chosen default, not derived: `bp-storage` has no "every document"
/// query, only `recent_documents(limit)` (ADR-0037). This report is a small
/// aggregate read run on a menu click, not a hot path, so scanning the most
/// recent few hundred documents rather than however many the store holds is
/// the right trade -- clarity over a fancier SQL query, the same call ADR-0041
/// itself makes for stale clusters.
const SCAN_LIMIT: usize = 500;

/// A tag's documents count as a "stale cluster" once none of them have been
/// touched (`last_seen`) in this many seconds.
///
/// A chosen default (30 days), not derived: ADR-0041 names "a long time"
/// without a number, and this is where that gets one, measured against
/// [`bp_history::now_unix`] -- the same clock `record_for_organize` already
/// reads for `seen_at`.
const STALE_SECONDS: i64 = 30 * 24 * 60 * 60;

/// How many documents sharing one tag makes it a "consolidation candidate".
///
/// A chosen default, not derived: ADR-0041 says "several... in large
/// numbers" without a number; three is where a shared tag stops looking
/// incidental and starts looking like a real cluster.
const CONSOLIDATION_MIN: usize = 3;

/// How many items each section of the report shows before it stops naming
/// them and just says how many more there were.
///
/// The same cap `SECRETS_IN_SUMMARY` (`state/security.rs`) already applies
/// to a status readout, for the same reason: a store with hundreds of
/// documents must not turn a report into an unreadable wall of text.
const REPORT_SECTION_LIMIT: usize = 5;

/// How many documents an insight names as the evidence behind it before it
/// stops and says how many more there were.
///
/// Smaller than [`REPORT_SECTION_LIMIT`] on purpose: this sits *inside* a
/// bullet rather than on a line of its own, and a tag carried by forty
/// documents would otherwise make one bullet longer than the rest of the
/// report put together.
const EVIDENCE_PER_BULLET: usize = 3;

/// Seconds in a day, for saying how long a `first_seen`-to-`last_seen` span
/// is without reaching for a calendar.
///
/// Days rather than dates: rendering a date needs a timezone and a format,
/// and neither is a question this report has any business answering. "143
/// days" is what the reader wanted anyway -- how much history the insights
/// above were drawn from, not which afternoon it started on.
const SECONDS_PER_DAY: i64 = 24 * 60 * 60;

impl AppState {
    /// Research ▸ Research Report (ADR-0041): the four insight shapes it
    /// names, each a plain aggregate over `bp-storage`'s schema -- dominant
    /// themes, stale clusters, under-connected documents, consolidation
    /// candidates.
    ///
    /// Always a string, never `Option`, matching the discipline
    /// `duplicate_detection_report` already holds itself to: a section that
    /// found nothing says so in words rather than vanishing, so the report
    /// stays honest about what it looked for as well as what it found.
    pub(crate) fn research_report(&self) -> String {
        let Some(store) = &self.store else {
            return "There is no store to research yet.".to_owned();
        };

        let frequency = store.tag_frequency().unwrap_or_default();
        let now = i64::try_from(bp_history::now_unix()).unwrap_or(i64::MAX);
        let summary = store.summary().unwrap_or_default();

        [
            dominant_themes_section(store, &frequency),
            stale_clusters_section(store, &frequency, now),
            under_connected_section(store),
            consolidation_candidates_section(store, &frequency),
            how_this_was_read_section(&summary),
        ]
        .join("\n\n")
    }

    /// Research ▸ What the Store Holds (ADR-0046): the store's own contents,
    /// asked directly rather than inferred from a report about them.
    ///
    /// The same numbers `how_this_was_read_section` carries, and that is the
    /// point: this is that section reachable on its own, for the reader whose
    /// question is "what has this recorded about me?" rather than "what have
    /// I been writing about?". The store is written to on every successful
    /// save without anyone being asked, so being able to see all of it in one
    /// row is not really a research feature -- it is the courtesy Security ▸
    /// Inspect Metadata already extends about a single file, extended to the
    /// one database this product keeps about all of them.
    pub(crate) fn store_contents_report(&self) -> String {
        let Some(store) = &self.store else {
            return "There is no store on this machine.".to_owned();
        };
        store_contents(&store.summary().unwrap_or_default())
    }
}

/// `"s"` for anything but exactly one, so a count and a plural can be
/// written in one `format!` without a branch at every call site.
fn plural(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

/// A document's filename, never its full path -- the same trim every report
/// in this crate already applies to a `DocumentRecord` (`organize.rs`'s
/// `filename_of`, `dispatch.rs`'s `filename`), each module keeping its own
/// copy of a helper too small to be worth sharing across a crate boundary.
fn filename_of(path: &str) -> &str {
    std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
}

/// Render `items` as a bulleted section under `heading`, each item through
/// `line`, capped at [`REPORT_SECTION_LIMIT`] with an "...and N more" tail --
/// the shared shape all four sections use so the cap and the wording of
/// "how many more" cannot drift between them.
fn bulleted_section<T>(
    heading: &str,
    empty: &str,
    items: &[T],
    line: impl Fn(&T) -> String,
) -> String {
    if items.is_empty() {
        return format!("{heading}\n{empty}");
    }
    let mut section = heading.to_owned();
    section.push('\n');
    for item in items.iter().take(REPORT_SECTION_LIMIT) {
        section.push_str("- ");
        section.push_str(&line(item));
        section.push('\n');
    }
    let rest = items.len().saturating_sub(REPORT_SECTION_LIMIT);
    if rest > 0 {
        section.push_str(&format!("...and {rest} more\n"));
    }
    section.trim_end().to_owned()
}

/// The evidence behind an insight about `tag`: the documents it is drawn
/// from, named.
///
/// **ADR-0046's "evidence", and it is a suffix rather than a row.** An
/// aggregate the reader cannot check is asking to be taken on trust, and a
/// claim and what backs it are one thing to read rather than two rows to
/// cross-reference. Naming the documents turns "four documents mention Rust"
/// into something a reader can go and open.
///
/// Queried per bullet rather than up front, and only for the bullets that
/// survive [`REPORT_SECTION_LIMIT`]: `bulleted_section` calls this from
/// inside its `take`, so a store with two hundred tags runs five queries
/// here, not two hundred.
fn evidence_for(store: &Store, tag: &str) -> String {
    named_documents(&store.documents_tagged(tag).unwrap_or_default())
}

/// `documents`, named and capped -- or nothing at all when there are none,
/// so a caller can append this to a bullet unconditionally.
fn named_documents(documents: &[DocumentRecord]) -> String {
    if documents.is_empty() {
        return String::new();
    }
    let names: Vec<&str> = documents
        .iter()
        .take(EVIDENCE_PER_BULLET)
        .map(|document| filename_of(&document.path))
        .collect();
    let rest = documents.len().saturating_sub(EVIDENCE_PER_BULLET);
    if rest > 0 {
        format!(": {}, and {rest} more", names.join(", "))
    } else {
        format!(": {}", names.join(", "))
    }
}

/// The most frequent tags across every recorded document.
fn dominant_themes_section(store: &Store, frequency: &[(String, usize)]) -> String {
    bulleted_section(
        "Dominant themes:",
        "Nothing has been tagged yet.",
        frequency,
        |(tag, count)| {
            format!(
                "{tag} ({count} document{}){}",
                plural(*count),
                evidence_for(store, tag)
            )
        },
    )
}

/// Tags whose documents have all gone untouched for [`STALE_SECONDS`].
///
/// Runs over every tag `tag_frequency` returns, not just the top
/// [`REPORT_SECTION_LIMIT`] shown for dominant themes -- a tag can be rare
/// and stale at once, and this section would otherwise never see it.
fn stale_clusters_section(store: &Store, frequency: &[(String, usize)], now: i64) -> String {
    let stale: Vec<(String, usize)> = frequency
        .iter()
        .filter(|(tag, _)| {
            let documents = store.documents_tagged(tag).unwrap_or_default();
            !documents.is_empty()
                && documents
                    .iter()
                    .all(|document| now - document.last_seen >= STALE_SECONDS)
        })
        .cloned()
        .collect();

    bulleted_section(
        "Stale clusters:",
        "Nothing looked stale.",
        &stale,
        |(tag, count)| {
            format!(
                "{tag} — {count} note{} untouched for over 30 days{}",
                plural(*count),
                evidence_for(store, tag)
            )
        },
    )
}

/// Documents sharing no tag with anything else in the store, among the
/// [`SCAN_LIMIT`] most recently seen.
fn under_connected_section(store: &Store) -> String {
    let recent = store.recent_documents(SCAN_LIMIT).unwrap_or_default();
    let under_connected: Vec<DocumentRecord> = recent
        .into_iter()
        .filter(|document| {
            store
                .related_to(document.id, 1)
                .unwrap_or_default()
                .is_empty()
        })
        .collect();

    bulleted_section(
        "Under-connected documents:",
        "Every recorded document shares a tag with at least one other.",
        &under_connected,
        |document| {
            document
                .title
                .clone()
                .unwrap_or_else(|| filename_of(&document.path).to_owned())
        },
    )
}

/// Tags carried by [`CONSOLIDATION_MIN`] or more documents -- worded as a
/// recommendation, per ADR-0041, rather than a bare statistic.
fn consolidation_candidates_section(store: &Store, frequency: &[(String, usize)]) -> String {
    let candidates: Vec<(String, usize)> = frequency
        .iter()
        .filter(|(_, count)| *count >= CONSOLIDATION_MIN)
        .cloned()
        .collect();

    bulleted_section(
        "Consolidation candidates:",
        "No tag is shared by enough documents to suggest consolidating.",
        &candidates,
        |(tag, count)| {
            format!(
                "{tag} — {count} documents share this tag; consider consolidating{}",
                evidence_for(store, tag)
            )
        },
    )
}

/// ADR-0046's **methods** and **datasets**, in one closing section: every
/// threshold the four insights above applied, and every number they applied
/// it over.
///
/// **This is the section that turns a silent limit into a stated one.** Until
/// it existed, a store of nine hundred documents got an under-connected
/// section drawn from the five hundred most recent and said nothing about the
/// four hundred it never looked at -- so a reader had no way to tell a report
/// that found nothing from a report that did not look. The rule was always
/// inspectable in `research.rs`; it was not inspectable from the window,
/// which is where the reader is.
fn how_this_was_read_section(summary: &bp_storage::StoreSummary) -> String {
    let mut lines = vec!["How this was read:".to_owned()];

    lines.push(format!("- {}", documents_line(summary)));
    lines.push(format!("- {}", tags_line(summary)));
    if let Some(span) = span_days(summary) {
        lines.push(format!("- {span}"));
    }

    let untagged = summary.documents.saturating_sub(summary.tagged_documents);
    if untagged > 0 {
        lines.push(format!(
            "- {untagged} document{} carr{} no tag, so no tag-shaped insight above can see {}",
            plural(untagged),
            if untagged == 1 { "ies" } else { "y" },
            if untagged == 1 { "it" } else { "them" },
        ));
    }

    lines.push(format!(
        "- A cluster is stale when nothing in it has been touched for {} days.",
        STALE_SECONDS / SECONDS_PER_DAY
    ));
    lines.push(format!(
        "- A tag is a consolidation candidate at {CONSOLIDATION_MIN} documents."
    ));

    // The truncation, said out loud. `recent_documents(SCAN_LIMIT)` is the
    // nearest thing this crate has to "every document", and a reader who is
    // not told the limit will read a short list as a complete one.
    if summary.documents > SCAN_LIMIT {
        lines.push(format!(
            "- Under-connected documents were looked for among the {SCAN_LIMIT} most recently seen only, not all {}.",
            summary.documents
        ));
    }
    lines.push(format!(
        "- Each section above names at most {REPORT_SECTION_LIMIT} items, and each item at most {EVIDENCE_PER_BULLET} documents."
    ));

    lines.join("\n")
}

/// What Research ▸ What the Store Holds says.
///
/// A free function over the summary rather than a method, so its test asserts
/// the product's sentences without needing a store to put documents into --
/// the same reason `paper_report` and `viewer_label` are ones.
fn store_contents(summary: &bp_storage::StoreSummary) -> String {
    if summary.documents == 0 {
        return "The store is empty: nothing has been recorded in it yet. A document is recorded when you save it.".to_owned();
    }

    let mut lines = vec![
        "What the store holds:".to_owned(),
        format!("- {}", documents_line(summary)),
        format!("- {}", tags_line(summary)),
    ];
    if let Some(span) = span_days(summary) {
        lines.push(format!("- {span}"));
    }
    lines.push(String::new());
    // Said here rather than left to be inferred, because this row exists to
    // answer "what has this recorded about me?" and the answer is only
    // reassuring if it is complete. `bp-storage` has a test of its own named
    // for it -- `the_store_holds_no_document_content`.
    lines.push(
        "The store keeps a path, a title and extracted keywords for each document. It never holds the text of one.".to_owned(),
    );
    lines.join("\n")
}

/// "12 documents recorded, 9 of them tagged" -- the denominator both the
/// report's closing section and the store-contents row open with, written
/// once so the two cannot word it differently.
fn documents_line(summary: &bp_storage::StoreSummary) -> String {
    format!(
        "{} document{} recorded, {} of them tagged",
        summary.documents,
        plural(summary.documents),
        summary.tagged_documents
    )
}

/// "5 tags in use".
fn tags_line(summary: &bp_storage::StoreSummary) -> String {
    format!("{} tag{} in use", summary.tags, plural(summary.tags))
}

/// "covering 143 days", or `None` when the store cannot say -- an empty
/// store, or one whose whole history fits inside a day.
///
/// Nothing rather than "covering 0 days", which reads as a fault rather than
/// as a store that started this morning.
fn span_days(summary: &bp_storage::StoreSummary) -> Option<String> {
    let (first, last) = (summary.first_seen?, summary.last_seen?);
    let days = last.saturating_sub(first) / SECONDS_PER_DAY;
    if days < 1 {
        return None;
    }
    Some(format!(
        "covering {days} day{}",
        plural(usize::try_from(days).unwrap_or(usize::MAX))
    ))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn a_store_with_nothing_recorded_says_so_in_every_section() {
        let state = AppState::new();
        let report = state.research_report();

        assert!(report.contains("Nothing has been tagged yet."), "{report}");
        assert!(report.contains("Nothing looked stale."), "{report}");
        assert!(
            report.contains("Every recorded document shares a tag with at least one other."),
            "{report}"
        );
        assert!(
            report.contains("No tag is shared by enough documents to suggest consolidating."),
            "{report}"
        );
    }

    /// A state with one saved document, and the path it was saved to.
    fn a_saved_document(text: &str) -> (tempfile::TempDir, AppState, PathBuf) {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("report.txt");
        std::fs::write(&path, text).expect("write");

        let mut state = AppState::new();
        state.open(path.clone());
        (dir, state, path)
    }

    #[test]
    fn dominant_themes_names_the_most_frequent_tag_first() {
        let (dir, mut state, path) =
            a_saved_document("rust rust rust rust notes notes budget budget budget budget");
        let id = state.workspace.active_id().unwrap();
        state.save_document(id, Some(path));

        let second_path = dir.path().join("second.txt");
        std::fs::write(&second_path, "rust rust rust rust cooking").unwrap();
        state.open(second_path.clone());
        let second_id = state.workspace.active_id().unwrap();
        state.save_document(second_id, Some(second_path));

        let report = state.research_report();
        let themes_start = report.find("Dominant themes:").expect("a themes section");
        let first_bullet = report[themes_start..].lines().nth(1).unwrap_or_default();
        assert!(
            first_bullet.to_ascii_lowercase().contains("rust"),
            "got {first_bullet:?} in {report}"
        );
    }

    #[test]
    fn a_tag_shared_by_three_or_more_documents_is_a_consolidation_candidate() {
        let (dir, mut state, path) = a_saved_document("shared shared shared shared");
        let id = state.workspace.active_id().unwrap();
        state.save_document(id, Some(path));

        for name in ["b.txt", "c.txt"] {
            let p = dir.path().join(name);
            std::fs::write(&p, "shared shared shared shared").unwrap();
            state.open(p.clone());
            let doc_id = state.workspace.active_id().unwrap();
            state.save_document(doc_id, Some(p));
        }

        let report = state.research_report();
        assert!(report.contains("consider consolidating"), "got {report}");
    }

    #[test]
    fn two_documents_sharing_no_tag_are_both_under_connected() {
        let (dir, mut state, path) = a_saved_document("alpha alpha alpha alpha");
        let id = state.workspace.active_id().unwrap();
        state.save_document(id, Some(path));

        let second_path = dir.path().join("second.txt");
        std::fs::write(&second_path, "beta beta beta beta").unwrap();
        state.open(second_path.clone());
        let second_id = state.workspace.active_id().unwrap();
        state.save_document(second_id, Some(second_path));

        let report = state.research_report();
        assert!(
            !report.contains("Every recorded document shares a tag with at least one other."),
            "got {report}"
        );
    }

    #[test]
    fn a_tag_whose_only_document_was_just_saved_is_not_stale() {
        let (_dir, mut state, path) = a_saved_document("fresh fresh fresh fresh");
        let id = state.workspace.active_id().unwrap();
        state.save_document(id, Some(path));

        assert!(state.research_report().contains("Nothing looked stale."),);
    }

    #[test]
    fn a_missing_store_says_there_is_nothing_to_research() {
        let mut state = AppState::new();
        state.store = None;
        assert_eq!(
            state.research_report(),
            "There is no store to research yet."
        );
    }

    // --- ADR-0046: evidence, methods and datasets ----------------------

    #[test]
    fn an_insight_names_the_documents_it_is_drawn_from() {
        let (dir, mut state, path) = a_saved_document("shared shared shared shared");
        let id = state.workspace.active_id().unwrap();
        state.save_document(id, Some(path));

        let second_path = dir.path().join("second.txt");
        std::fs::write(&second_path, "shared shared shared shared").unwrap();
        state.open(second_path.clone());
        let second_id = state.workspace.active_id().unwrap();
        state.save_document(second_id, Some(second_path));

        let report = state.research_report();
        assert!(
            report.contains("report.txt") && report.contains("second.txt"),
            "an aggregate the reader cannot check is asking to be taken on \
             trust: {report}"
        );
    }

    #[test]
    fn evidence_is_capped_and_says_how_many_it_did_not_name() {
        let documents: Vec<DocumentRecord> = (0..EVIDENCE_PER_BULLET + 4)
            .map(|n| DocumentRecord {
                id: n as i64,
                path: format!("/notes/n{n}.md"),
                title: None,
                first_seen: 0,
                last_seen: 0,
            })
            .collect();
        let named = named_documents(&documents);
        assert!(named.contains("and 4 more"), "got {named}");
        assert_eq!(
            named.matches(".md").count(),
            EVIDENCE_PER_BULLET,
            "one bullet must not be longer than the rest of the report"
        );
    }

    #[test]
    fn an_insight_with_no_documents_behind_it_appends_nothing() {
        assert_eq!(named_documents(&[]), "");
    }

    #[test]
    fn the_report_states_every_threshold_it_applied() {
        let report = AppState::new().research_report();
        assert!(report.contains("How this was read:"), "{report}");
        assert!(
            report.contains("touched for 30 days"),
            "the stale rule is a chosen number and the reader cannot check \
             the insight without it: {report}"
        );
        assert!(
            report.contains("consolidation candidate at 3 documents"),
            "{report}"
        );
        assert!(report.contains("names at most 5 items"), "{report}");
    }

    #[test]
    fn the_report_states_what_it_read_the_thresholds_over() {
        let (_dir, mut state, path) = a_saved_document("alpha alpha alpha alpha");
        let id = state.workspace.active_id().unwrap();
        state.save_document(id, Some(path));

        let report = state.research_report();
        assert!(
            report.contains("1 document recorded, 1 of them tagged"),
            "the denominator an aggregate needs: {report}"
        );
    }

    #[test]
    fn a_store_smaller_than_the_scan_limit_does_not_claim_to_have_truncated() {
        let report = AppState::new().research_report();
        assert!(
            !report.contains("most recently seen only"),
            "a limit that did not bite must not be reported as though it did: \
             {report}"
        );
    }

    #[test]
    fn a_store_larger_than_the_scan_limit_says_so() {
        let summary = bp_storage::StoreSummary {
            documents: SCAN_LIMIT + 400,
            tagged_documents: SCAN_LIMIT,
            tags: 12,
            first_seen: None,
            last_seen: None,
        };
        let section = how_this_was_read_section(&summary);
        assert!(
            section.contains("most recently seen only, not all 900"),
            "a reader who is not told the limit reads a short list as a \
             complete one: {section}"
        );
    }

    #[test]
    fn untagged_documents_are_named_as_invisible_rather_than_left_out() {
        let summary = bp_storage::StoreSummary {
            documents: 10,
            tagged_documents: 4,
            tags: 3,
            first_seen: None,
            last_seen: None,
        };
        let section = how_this_was_read_section(&summary);
        assert!(
            section.contains("6 documents carry no tag"),
            "every insight above is tag-shaped, so this is why one found \
             nothing: {section}"
        );
    }

    #[test]
    fn one_untagged_document_is_written_in_the_singular() {
        let summary = bp_storage::StoreSummary {
            documents: 2,
            tagged_documents: 1,
            tags: 1,
            first_seen: None,
            last_seen: None,
        };
        assert!(
            how_this_was_read_section(&summary).contains("1 document carries no tag"),
            "{}",
            how_this_was_read_section(&summary)
        );
    }

    #[test]
    fn a_span_shorter_than_a_day_is_not_reported_as_zero_days() {
        let summary = bp_storage::StoreSummary {
            documents: 1,
            tagged_documents: 1,
            tags: 1,
            first_seen: Some(1_000),
            last_seen: Some(2_000),
        };
        assert_eq!(
            span_days(&summary),
            None,
            "'covering 0 days' reads as a fault rather than as a store that \
             started this morning"
        );
    }

    #[test]
    fn a_span_of_several_days_is_reported_in_days() {
        let summary = bp_storage::StoreSummary {
            documents: 2,
            tagged_documents: 2,
            tags: 1,
            first_seen: Some(0),
            last_seen: Some(SECONDS_PER_DAY * 143),
        };
        assert_eq!(span_days(&summary).as_deref(), Some("covering 143 days"));
    }

    #[test]
    fn an_empty_store_says_it_is_empty_and_says_what_would_fill_it() {
        let report = store_contents(&bp_storage::StoreSummary::default());
        assert!(report.contains("The store is empty"), "{report}");
        assert!(
            report.contains("recorded when you save it"),
            "an empty store is otherwise indistinguishable from a broken one: \
             {report}"
        );
    }

    #[test]
    fn what_the_store_holds_says_it_never_holds_the_text() {
        let summary = bp_storage::StoreSummary {
            documents: 12,
            tagged_documents: 9,
            tags: 5,
            first_seen: Some(0),
            last_seen: Some(SECONDS_PER_DAY * 30),
        };
        let report = store_contents(&summary);
        assert!(
            report.contains("12 documents recorded, 9 of them tagged"),
            "{report}"
        );
        assert!(report.contains("5 tags in use"), "{report}");
        assert!(report.contains("covering 30 days"), "{report}");
        assert!(
            report.contains("never holds the text"),
            "this row exists to answer 'what has this recorded about me?', \
             and the answer is only reassuring if it is complete: {report}"
        );
    }

    #[test]
    fn a_missing_store_says_there_is_none_rather_than_that_it_is_empty() {
        let mut state = AppState::new();
        state.store = None;
        assert_eq!(
            state.store_contents_report(),
            "There is no store on this machine.",
            "no store and an empty store are different answers"
        );
    }

    #[test]
    fn the_report_and_the_store_contents_row_word_the_counts_identically() {
        let summary = bp_storage::StoreSummary {
            documents: 12,
            tagged_documents: 9,
            tags: 5,
            first_seen: None,
            last_seen: None,
        };
        let line = documents_line(&summary);
        assert!(how_this_was_read_section(&summary).contains(&line));
        assert!(store_contents(&summary).contains(&line));
    }
}
