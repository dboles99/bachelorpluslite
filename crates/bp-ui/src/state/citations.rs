//! What the document in front of you cites (ADR-0044).
//!
//! **Two different features share the Research menu**, and reading them as
//! one is what kept it empty. `research.rs` is the other: it reads
//! `bp-storage` and says what the user has been writing about over time
//! (ADR-0039, ADR-0041). This one reads the *active document* and says what
//! is cited in it, through `bp-research`.
//!
//! D12 ruled out bibliography as the foundation of synthesis. It did not rule
//! out bibliography.
//!
//! ## Offline, and named for it
//!
//! Every function here is something `bp-research` can do without a network,
//! because it has no HTTP client and a test whose job is to notice if that
//! changes. Its own words:
//!
//! > A DOI has a grammar, so it can be recognised locally; it has no meaning
//! > locally, so it cannot be resolved.
//!
//! So there is a Find Identifiers and there is no DOI Lookup. Finding one and
//! resolving one are different acts, and a row named for the second would be
//! a promise this product cannot keep.
//!
//! ## Reports rather than panels
//!
//! All three open a dialog, like `Scan for Secrets` before them and for the
//! reason its own arm gives: the status bar elides, and the positions are the
//! part worth reading.

use bp_research::{Identifier, Located, Paper};

use super::AppState;

/// How many identifiers a report names before it stops and says how many
/// more there were.
///
/// The same cap `REPORT_SECTION_LIMIT` (`state/research.rs`) and
/// `SECRETS_IN_SUMMARY` (`state/security.rs`) already apply, for the same
/// reason: a survey paper's reference list must not turn a dialog into a wall
/// of text nobody scrolls.
const IDENTIFIERS_LISTED: usize = 40;

impl AppState {
    /// Research ▸ Citation Metadata: what this document says about itself.
    pub(crate) fn citation_metadata_report(&self) -> String {
        paper_report(&Paper::from_text(&self.active_text()))
    }

    /// Research ▸ Find Identifiers: every DOI and arXiv id in the document.
    pub(crate) fn identifiers_report(&self) -> String {
        identifier_report(&bp_research::paper::find_identifiers(&self.active_text()))
    }

    /// Research ▸ Check Bibliography: read the document as BibTeX.
    pub(crate) fn bibliography_report(&self) -> String {
        bibliography_report(&self.active_text())
    }
}

/// What Citation Metadata says.
///
/// A free function so its test asserts the product's sentences rather than a
/// copy of them -- the same reason `viewer_label` and `run_summary` are ones.
///
/// **A field that was not found is named as not found rather than omitted.**
/// A report that silently drops what it could not read leaves the reader
/// unable to tell "this document has no DOI" from "we did not look".
pub(crate) fn paper_report(paper: &Paper) -> String {
    if paper.is_empty() {
        return "Nothing in this document reads as a paper's own metadata: no \
                title, authors, year, venue, abstract, DOI or arXiv id."
            .to_owned();
    }

    let mut lines = Vec::new();
    lines.push(field("Title", paper.title.as_deref()));

    // The raw author line as well as the split names, and both are the point:
    // `bp-research` hands back the line unsplit whenever splitting it would
    // mean guessing, and dropping that here would throw away the distinction
    // it refused to guess at.
    if paper.authors.is_empty() {
        lines.push(field("Authors", paper.authors_text.as_deref()));
    } else {
        let names: Vec<String> = paper.authors.iter().map(render_name).collect();
        lines.push(format!("Authors: {}", names.join("; ")));
        if let Some(text) = &paper.authors_text {
            lines.push(format!("  as written: {text}"));
        }
    }

    lines.push(field("Year", paper.year.map(|y| y.to_string()).as_deref()));
    lines.push(field("Venue", paper.venue.as_deref()));
    lines.push(field(
        "DOI",
        paper.doi.as_ref().map(bp_research::Doi::as_str),
    ));
    lines.push(field(
        "arXiv",
        paper
            .arxiv
            .as_ref()
            .map(bp_research::ArxivId::to_text)
            .as_deref(),
    ));

    if let Some(abstract_text) = &paper.abstract_text {
        lines.push(String::new());
        lines.push("Abstract:".to_owned());
        lines.push(abstract_text.clone());
    }
    lines.join("\n")
}

/// One `Label: value` line, or one that says the field was not found.
fn field(label: &str, value: Option<&str>) -> String {
    match value {
        Some(value) => format!("{label}: {value}"),
        None => format!("{label}: not found"),
    }
}

/// A name as a citation would print it, without inventing any part of it.
fn render_name(name: &bp_research::Name) -> String {
    let mut out = String::new();
    if let Some(given) = &name.given {
        out.push_str(given);
        out.push(' ');
    }
    if let Some(particle) = &name.particle {
        out.push_str(particle);
        out.push(' ');
    }
    out.push_str(&name.family);
    if let Some(suffix) = &name.suffix {
        out.push_str(", ");
        out.push_str(suffix);
    }
    out
}

/// What Find Identifiers says.
///
/// `line:column` first on every row, because that is what the reader does
/// next: Ctrl+G to it. The canonical form and the URL follow, and the URL is
/// **what the identifier would resolve to**, not something this product
/// fetched -- ADR-0006, and `bp-research` has no client to fetch it with.
pub(crate) fn identifier_report(found: &[Located<Identifier>]) -> String {
    if found.is_empty() {
        return "No DOIs or arXiv identifiers in this document.\n\n\
                A bare number like 2401.01234 is not counted: without an \
                `arXiv:` prefix or an arxiv.org address it is \
                indistinguishable from a version, a price or a date, and \
                guessing would put the wrong paper in a citation."
            .to_owned();
    }

    let mut lines = vec![format!(
        "{} identifier(s). Nothing was looked up: this product does not go \
         to the network (ADR-0006), so the addresses below are where each \
         one points, not what it resolved to.\n",
        found.len()
    )];
    for located in found.iter().take(IDENTIFIERS_LISTED) {
        let (kind, canonical, url) = match &located.value {
            Identifier::Doi(doi) => ("DOI", doi.as_str().to_owned(), doi.url()),
            Identifier::Arxiv(id) => ("arXiv", id.to_text(), id.url()),
        };
        lines.push(format!(
            "{}:{}  {kind} {canonical}\n    {url}",
            located.line, located.column
        ));
    }
    if found.len() > IDENTIFIERS_LISTED {
        lines.push(format!(
            "\n… and {} more, not listed.",
            found.len() - IDENTIFIERS_LISTED
        ));
    }
    lines.join("\n")
}

/// What Check Bibliography says.
///
/// **A parse failure is the report, not an error dialog.** `bp-research`
/// stops at the first thing it cannot read and hands back a line and a
/// column, deliberately -- "a `.bib` file that cannot be understood stops the
/// parse with a line and a column rather than yielding an entry missing a
/// field" -- and that position is the whole value of running this.
///
/// **There is no duplicate-key check here, and there was one for an hour.**
/// A `\cite` of a key that names two entries silently picks one, so it is
/// exactly the defect a bibliography check should find -- and `bp-research`
/// already refuses it *at parse time*, with a message and a position that a
/// check over the parsed entries could not produce, because those entries
/// never exist. The version written here was dead code that would have
/// disagreed with the crate the moment either changed. Counting what a
/// library already decided is how a second, worse answer gets into a
/// product.
pub(crate) fn bibliography_report(source: &str) -> String {
    let bibliography = match bp_research::bibtex::parse(source) {
        Ok(bibliography) => bibliography,
        Err(e) => {
            return format!(
                "This document does not read as BibTeX.\n\n{} at {}.\n\n\
                 The parse stops at the first thing it cannot understand \
                 rather than guessing past it, so there may be more after \
                 this one.",
                e.kind, e.at
            );
        }
    };

    if bibliography.entries.is_empty() {
        return "This reads as BibTeX and contains no entries.".to_owned();
    }

    // Counted by kind rather than listed one per line: a bibliography is
    // hundreds of entries, and what a reader wants from a check is the shape
    // of it plus anything wrong.
    let mut kinds: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for entry in &bibliography.entries {
        *kinds.entry(entry.kind.as_str().to_owned()).or_default() += 1;
    }

    let mut lines = vec![format!("{} entries.\n", bibliography.entries.len())];
    for (kind, count) in &kinds {
        lines.push(format!("  {count} × @{kind}"));
    }
    if !bibliography.macros.is_empty() {
        lines.push(format!(
            "\n{} @string definition(s).",
            bibliography.macros.len()
        ));
    }
    if !bibliography.preamble.is_empty() {
        lines.push(format!(
            "{} @preamble block(s).",
            bibliography.preamble.len()
        ));
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_document_with_no_paper_metadata_says_so_rather_than_printing_a_blank_form() {
        let report = paper_report(&Paper::from_text("just some ordinary prose"));
        assert!(report.contains("Nothing in this document"), "{report}");
    }

    #[test]
    fn a_field_that_was_not_found_is_named_as_not_found() {
        // A report that omits what it could not read leaves the reader unable
        // to tell "no DOI here" from "we did not look for one".
        let paper = Paper::from_text("Title: A Study of Things\n");
        let report = paper_report(&paper);
        assert!(report.contains("A Study of Things"), "{report}");
        assert!(report.contains("DOI: not found"), "{report}");
    }

    #[test]
    fn the_author_line_survives_even_when_the_names_were_split() {
        // `bp-research` keeps the raw line precisely because splitting it can
        // mean guessing, and dropping it here would throw away the
        // distinction it refused to make.
        let paper = Paper::from_text("Title: T\nAuthors: Ada Lovelace and Alan Turing\n");
        let report = paper_report(&paper);
        if !paper.authors.is_empty() {
            assert!(report.contains("as written:"), "{report}");
        }
    }

    #[test]
    fn no_identifiers_says_why_a_bare_number_is_not_one() {
        // The refusal is `bp-research`'s and it is deliberate; a reader who
        // expected 2401.01234 to be found needs to know it was declined
        // rather than missed.
        let report = identifier_report(&[]);
        assert!(report.contains("No DOIs or arXiv"), "{report}");
        assert!(report.contains("2401.01234"), "{report}");
    }

    #[test]
    fn an_identifier_is_reported_with_the_position_a_reader_goes_to_next() {
        let found = bp_research::paper::find_identifiers("intro\nsee doi:10.1000/182 for more\n");
        assert!(!found.is_empty(), "the fixture must contain a DOI");

        let report = identifier_report(&found);
        assert!(report.contains("2:"), "line first: {report}");
        assert!(report.contains("10.1000/182"), "{report}");
    }

    #[test]
    fn the_report_says_nothing_was_looked_up() {
        // ADR-0006. A list of URLs beside identifiers reads like the product
        // fetched them, and it did not and cannot.
        let found = bp_research::paper::find_identifiers("doi:10.1000/182\n");
        let report = identifier_report(&found);
        assert!(report.contains("Nothing was looked up"), "{report}");
    }

    #[test]
    fn a_long_reference_list_is_capped_and_says_how_many_it_did_not_name() {
        let mut text = String::new();
        for n in 0..(IDENTIFIERS_LISTED + 7) {
            text.push_str(&format!("doi:10.1000/{n}\n"));
        }
        let found = bp_research::paper::find_identifiers(&text);
        assert!(found.len() > IDENTIFIERS_LISTED);

        let report = identifier_report(&found);
        assert!(report.contains("and 7 more"), "{report}");
    }

    #[test]
    fn a_document_that_is_not_bibtex_is_reported_with_the_position_that_stopped_it() {
        let report = bibliography_report("@article{missing brace\n");
        assert!(report.contains("does not read as BibTeX"), "{report}");
        assert!(
            report.contains("line"),
            "the position is the point: {report}"
        );
    }

    #[test]
    fn a_bibliography_is_summarised_by_kind_rather_than_listed_entry_by_entry() {
        let source =
            "@article{a, title = {One}}\n@book{b, title = {Two}}\n@article{c, title = {Three}}\n";
        let report = bibliography_report(source);
        assert!(report.contains("3 entries"), "{report}");
        assert!(report.contains("2 × @article"), "{report}");
        assert!(report.contains("1 × @book"), "{report}");
    }

    #[test]
    fn a_duplicate_citation_key_is_refused_by_the_parser_and_the_report_says_where() {
        // Two entries with one key means every \cite of it silently picks
        // one, and which one is not something the writer chose. `bp-research`
        // refuses it during the parse rather than after, so the position is
        // available -- which a check over the parsed entries could never
        // produce, because those entries never exist.
        //
        // This test is here because the first version of `bibliography_report`
        // counted duplicates itself. That code could not run: the parse it
        // ran after never returns a bibliography containing one.
        let source = "@article{same, title = {One}}\n@book{same, title = {Two}}\n";
        let report = bibliography_report(source);

        assert!(report.contains("same"), "{report}");
        assert!(
            report.contains("two entries"),
            "the crate's own words, not a second set: {report}"
        );
        assert!(
            report.contains("line 2"),
            "the position is the point: {report}"
        );
    }
}
