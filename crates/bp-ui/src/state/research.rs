//! Research mode's synthesis layer (ADR-0041): plain, honest aggregate
//! reads over `bp-storage`'s existing schema, worded as insights rather than
//! bare statistics.
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

        [
            dominant_themes_section(&frequency),
            stale_clusters_section(store, &frequency, now),
            under_connected_section(store),
            consolidation_candidates_section(&frequency),
        ]
        .join("\n\n")
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

/// The most frequent tags across every recorded document.
fn dominant_themes_section(frequency: &[(String, usize)]) -> String {
    bulleted_section(
        "Dominant themes:",
        "Nothing has been tagged yet.",
        frequency,
        |(tag, count)| format!("{tag} ({count} document{})", plural(*count)),
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
                "{tag} — {count} note{} untouched for over 30 days",
                plural(*count)
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
fn consolidation_candidates_section(frequency: &[(String, usize)]) -> String {
    let candidates: Vec<(String, usize)> = frequency
        .iter()
        .filter(|(_, count)| *count >= CONSOLIDATION_MIN)
        .cloned()
        .collect();

    bulleted_section(
        "Consolidation candidates:",
        "No tag is shared by enough documents to suggest consolidating.",
        &candidates,
        |(tag, count)| format!("{tag} — {count} documents share this tag; consider consolidating"),
    )
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
}
