//! CSL JSON: the other format worth supporting, and why only this one.
//!
//! BibTeX is the format bibliographies are *stored* in. CSL JSON is the format
//! they are *passed* in --- it is what Zotero exports, what Pandoc consumes,
//! and what every citation processor speaks. Supporting it costs one struct
//! and buys the user an offline path from their reference manager into a
//! document and back out to a typesetter, with no service in the middle. That
//! is a good trade, and it is the same trade ADR-0006 asks for everywhere
//! else: interoperate by understanding the file, not by calling an API.
//!
//! **RIS is deliberately not here.** It looks like the easiest format of the
//! three --- two-letter tags, one per line --- and it is the hardest to be
//! correct in: EndNote, Zotero, ProQuest and Scopus disagree about what `T2`,
//! `M3` and `AN` mean, so a faithful reader has to pick a dialect and be
//! silently wrong about the others. Picking one is a decision with a
//! defensible answer and no obviously right one, which makes it ADR material
//! rather than something to settle inside a module.
//!
//! CSL's type vocabulary is coarser than BibTeX's --- `incollection` and
//! `inbook` are both `chapter`, `phdthesis` and `mastersthesis` are both
//! `thesis` --- so a plain mapping would quietly promote one to the other on
//! every round trip. A `bp-bibtex-type` member is written alongside the
//! standard `type` to keep the original; CSL processors ignore members they do
//! not know, so the file stays valid for everyone else.

use serde::{Deserialize, Serialize};

use crate::model::{EntryKind, Name, Reference};

/// Why some JSON is not a CSL bibliography.
///
/// The `serde_json` message is passed through rather than replaced: it carries
/// a line and a column, which is what the user needs to fix the file, and
/// nothing this crate could say instead would be more useful.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CslError {
    #[error("this is not CSL JSON (a list of citation records): {0}")]
    Malformed(String),
}

/// Write references as CSL JSON.
///
/// Pretty-printed, because the result is a file a person may open, diff and
/// commit; the bytes saved by minifying are worth less than a readable diff.
#[must_use]
pub fn to_json(references: &[Reference]) -> String {
    let items: Vec<Item> = references.iter().map(Item::from_reference).collect();
    // Serialising a `Vec` of plain structs cannot fail; the fallback keeps the
    // signature honest without inventing an error case callers must handle.
    serde_json::to_string_pretty(&items).unwrap_or_else(|_| "[]".to_owned())
}

/// Read a CSL JSON bibliography.
///
/// # Errors
///
/// [`CslError::Malformed`] if the text is not a JSON list of records with the
/// members CSL requires. An item with no `id` is refused rather than given a
/// generated one: the id is what a document cites, and inventing it would
/// produce a bibliography whose keys change every time it is imported.
pub fn from_json(text: &str) -> Result<Vec<Reference>, CslError> {
    let items: Vec<Item> =
        serde_json::from_str(text).map_err(|error| CslError::Malformed(error.to_string()))?;
    Ok(items.iter().map(Item::to_reference).collect())
}

/// One CSL record. Absent members are omitted rather than written as `null`,
/// which is what every other tool in this ecosystem does.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Item {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    /// The BibTeX entry type this came from. Non-standard, namespaced, and
    /// the reason a round trip through CSL does not flatten `phdthesis` into
    /// `thesis` permanently.
    #[serde(rename = "bp-bibtex-type", skip_serializing_if = "Option::is_none")]
    bibtex_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(rename = "container-title", skip_serializing_if = "Option::is_none")]
    container_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    publisher: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    volume: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    issue: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    page: Option<String>,
    #[serde(rename = "DOI", skip_serializing_if = "Option::is_none")]
    doi: Option<String>,
    #[serde(rename = "URL", skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    author: Vec<CslName>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    editor: Vec<CslName>,
    #[serde(skip_serializing_if = "Option::is_none")]
    issued: Option<CslDate>,
}

/// A person, in CSL's spelling.
///
/// `literal` is CSL's way of saying "this is one indivisible name", and it is
/// exactly what a brace-wrapped BibTeX name means, so the two map onto each
/// other without loss.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct CslName {
    #[serde(skip_serializing_if = "Option::is_none")]
    family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    given: Option<String>,
    #[serde(
        rename = "non-dropping-particle",
        skip_serializing_if = "Option::is_none"
    )]
    particle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    suffix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    literal: Option<String>,
}

/// CSL dates are nested lists so that a range or a partial date fits in the
/// same member. Only the year is read: month and day are not in [`Reference`],
/// and inventing a precision the source did not have would be a lie about the
/// record.
///
/// The parts are `Value` rather than numbers because exporters in the wild
/// write `"2020"` as often as `2020`, and refusing half of the real files over
/// a JSON type would make this reader useless.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CslDate {
    #[serde(rename = "date-parts")]
    date_parts: Vec<Vec<serde_json::Value>>,
}

impl Item {
    fn from_reference(reference: &Reference) -> Self {
        Self {
            id: reference.key.clone(),
            kind: csl_type(&reference.kind).to_owned(),
            bibtex_type: Some(reference.kind.as_str().to_owned()),
            title: reference.title.clone(),
            container_title: reference.container.clone(),
            publisher: reference.publisher.clone(),
            volume: reference.volume.clone(),
            issue: reference.issue.clone(),
            page: reference.pages.clone(),
            doi: reference.doi.clone(),
            url: reference.url.clone(),
            note: reference.note.clone(),
            author: reference.authors.iter().map(CslName::from_name).collect(),
            editor: reference.editors.iter().map(CslName::from_name).collect(),
            issued: reference.year.map(|year| CslDate {
                date_parts: vec![vec![serde_json::Value::from(year)]],
            }),
        }
    }

    fn to_reference(&self) -> Reference {
        Reference {
            key: self.id.clone(),
            kind: match &self.bibtex_type {
                Some(kind) => EntryKind::parse(kind),
                None => kind_from_csl(&self.kind),
            },
            authors: self.author.iter().map(CslName::to_name).collect(),
            editors: self.editor.iter().map(CslName::to_name).collect(),
            title: self.title.clone(),
            container: self.container_title.clone(),
            publisher: self.publisher.clone(),
            year: self.issued.as_ref().and_then(CslDate::year),
            volume: self.volume.clone(),
            issue: self.issue.clone(),
            pages: self.page.clone(),
            doi: self.doi.clone(),
            url: self.url.clone(),
            note: self.note.clone(),
        }
    }
}

impl CslDate {
    fn year(&self) -> Option<i32> {
        let part = self.date_parts.first()?.first()?;
        part.as_i64()
            .or_else(|| part.as_str()?.trim().parse::<i64>().ok())
            .and_then(|year| i32::try_from(year).ok())
    }
}

impl CslName {
    fn from_name(name: &Name) -> Self {
        // A name with nothing but a family part is an organisation or a
        // mononym; CSL calls that `literal`, and saying so keeps a processor
        // from printing "Organization, W. H."
        if name.given.is_none() && name.particle.is_none() && name.suffix.is_none() {
            return Self {
                literal: Some(name.family.clone()),
                ..Self::default()
            };
        }
        Self {
            family: Some(name.family.clone()),
            given: name.given.clone(),
            particle: name.particle.clone(),
            suffix: name.suffix.clone(),
            literal: None,
        }
    }

    fn to_name(&self) -> Name {
        if let Some(literal) = &self.literal {
            return Name {
                given: None,
                particle: None,
                family: literal.clone(),
                suffix: None,
            };
        }
        Name {
            given: self.given.clone(),
            particle: self.particle.clone(),
            family: self.family.clone().unwrap_or_default(),
            suffix: self.suffix.clone(),
        }
    }
}

/// The nearest standard CSL type, for processors that only read `type`.
fn csl_type(kind: &EntryKind) -> &str {
    match kind {
        EntryKind::Article => "article-journal",
        EntryKind::Book | EntryKind::Proceedings => "book",
        EntryKind::Booklet => "pamphlet",
        EntryKind::InBook | EntryKind::InCollection => "chapter",
        EntryKind::Conference | EntryKind::InProceedings => "paper-conference",
        EntryKind::Manual | EntryKind::TechReport => "report",
        EntryKind::MastersThesis | EntryKind::PhdThesis => "thesis",
        EntryKind::Misc => "document",
        EntryKind::Unpublished => "manuscript",
        // An unknown BibTeX type keeps its own name: a CSL processor will
        // treat it generically, and the alternative is deciding on the user's
        // behalf that their `@dataset` was really a `@misc`.
        EntryKind::Other(name) => name,
    }
}

/// The reverse, for a file that came from somewhere other than here.
///
/// Lossy in the directions CSL is coarser --- every `chapter` reads back as
/// `incollection` --- which is why [`Item::bibtex_type`] exists and is
/// preferred when present.
fn kind_from_csl(kind: &str) -> EntryKind {
    match kind {
        "article-journal" | "article" | "article-magazine" | "article-newspaper" => {
            EntryKind::Article
        }
        "book" => EntryKind::Book,
        "pamphlet" => EntryKind::Booklet,
        "chapter" => EntryKind::InCollection,
        "paper-conference" => EntryKind::InProceedings,
        "report" => EntryKind::TechReport,
        "thesis" => EntryKind::PhdThesis,
        "manuscript" => EntryKind::Unpublished,
        "document" | "" => EntryKind::Misc,
        other => EntryKind::Other(other.to_ascii_lowercase()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bibtex;
    use proptest::prelude::*;

    fn references(source: &str) -> Vec<Reference> {
        bibtex::parse(source).unwrap().references()
    }

    #[test]
    fn a_record_carries_the_members_a_processor_reads() {
        let json = to_json(&references(
            "@article{smith2019, author = {Smith, Jane}, title = {On Caching},
              journal = {Storage}, volume = {12}, number = {3}, pages = {1--9},
              year = 2019, doi = {10.1000/abc}}",
        ));
        for expected in [
            "\"id\": \"smith2019\"",
            "\"type\": \"article-journal\"",
            "\"container-title\": \"Storage\"",
            "\"DOI\": \"10.1000/abc\"",
            "\"date-parts\"",
        ] {
            assert!(json.contains(expected), "{expected} missing from {json}");
        }
    }

    #[test]
    fn an_organisation_becomes_a_literal_name() {
        let json = to_json(&references(
            "@techreport{who, author = {{World Health Organization}}, title = {Report}}",
        ));
        assert!(
            json.contains("\"literal\": \"World Health Organization\""),
            "{json}"
        );
        assert!(!json.contains("\"family\""), "{json}");
    }

    #[test]
    fn a_thesis_keeps_which_kind_of_thesis_it_was() {
        // CSL calls both "thesis"; without the extension member a masters
        // thesis would silently become a doctorate on the way back.
        let original = references("@mastersthesis{k, title = {T}, school = {S}}");
        let returned = from_json(&to_json(&original)).unwrap();
        assert_eq!(returned[0].kind, EntryKind::MastersThesis);
        assert_eq!(returned, original);
    }

    #[test]
    fn a_file_from_another_tool_reads_without_the_extension_member() {
        let returned = from_json(
            r#"[{"id":"x","type":"paper-conference","title":"T",
                "author":[{"family":"Doe","given":"Jane"}],
                "issued":{"date-parts":[[2020]]}}]"#,
        )
        .unwrap();
        assert_eq!(returned.len(), 1);
        assert_eq!(returned[0].kind, EntryKind::InProceedings);
        assert_eq!(returned[0].year, Some(2020));
        assert_eq!(returned[0].authors[0].family, "Doe");
    }

    #[test]
    fn a_year_written_as_a_string_is_still_a_year() {
        // Real exporters write both, and refusing one halves the useful files.
        let returned =
            from_json(r#"[{"id":"x","type":"book","issued":{"date-parts":[["1999"]]}}]"#).unwrap();
        assert_eq!(returned[0].year, Some(1999));
    }

    #[test]
    fn a_date_with_no_parts_is_no_year_rather_than_a_wrong_one() {
        let returned =
            from_json(r#"[{"id":"x","type":"book","issued":{"date-parts":[]}}]"#).unwrap();
        assert_eq!(returned[0].year, None);
    }

    #[test]
    fn an_empty_bibliography_round_trips_as_an_empty_list() {
        assert_eq!(to_json(&[]), "[]");
        assert_eq!(from_json("[]").unwrap(), Vec::new());
    }

    #[test]
    fn malformed_json_is_refused_with_the_position_in_it() {
        let error = from_json("{ not a list }").expect_err("must fail");
        let message = error.to_string();
        assert!(message.contains("not CSL JSON"), "{message}");
        assert!(message.contains("line 1"), "{message}");
    }

    #[test]
    fn a_record_with_no_id_is_refused_rather_than_given_one() {
        // A generated key would change on every import, and every citation in
        // the document would stop resolving.
        assert!(from_json(r#"[{"type":"book","title":"T"}]"#).is_err());
    }

    #[test]
    fn an_unknown_csl_type_keeps_its_name() {
        let returned = from_json(r#"[{"id":"x","type":"dataset"}]"#).unwrap();
        assert_eq!(returned[0].kind, EntryKind::Other("dataset".to_owned()));
    }

    proptest! {
        /// Writing references as CSL JSON and reading them back returns the
        /// same references.
        #[test]
        fn references_survive_a_round_trip_through_csl(
            keys in proptest::collection::vec("[a-z][a-z0-9]{0,5}", 0..4),
            author in "[A-Za-z ,]{0,20}",
            title in "[A-Za-z ]{0,20}",
            year in proptest::option::of(1000i32..3000),
        ) {
            let mut seen = std::collections::BTreeSet::new();
            let references: Vec<Reference> = keys
                .iter()
                .filter(|key| seen.insert((*key).clone()))
                .map(|key| {
                    let mut entry = crate::model::Entry::new(EntryKind::Article, key.clone());
                    entry.set_field("author", author.clone());
                    entry.set_field("title", title.clone());
                    if let Some(year) = year {
                        entry.set_field("year", year.to_string());
                    }
                    Reference::from_entry(&entry)
                })
                .collect();

            prop_assert_eq!(from_json(&to_json(&references)).unwrap(), references);
        }

        /// So does every entry type, including the ones CSL cannot tell apart.
        ///
        /// Drawn from the canonical names rather than from random letters:
        /// the types that collide in CSL are `mastersthesis` against
        /// `phdthesis` and `inbook` against `incollection`, and a generator
        /// that never spells those never tests the thing that breaks.
        #[test]
        fn every_entry_type_survives_a_round_trip_through_csl(
            kind in proptest::sample::select(vec![
                "article", "book", "booklet", "conference", "inbook",
                "incollection", "inproceedings", "manual", "mastersthesis",
                "misc", "phdthesis", "proceedings", "techreport",
                "unpublished", "dataset", "software",
            ]),
        ) {
            let entry = crate::model::Entry::new(EntryKind::parse(kind), "k");
            let reference = Reference::from_entry(&entry);
            let returned = from_json(&to_json(std::slice::from_ref(&reference))).unwrap();
            prop_assert_eq!(returned, vec![reference]);
        }

        /// Reading arbitrary text either produces references or an error that
        /// says what was wrong; it never panics.
        #[test]
        fn reading_arbitrary_text_never_panics(text in r#"[\[\]{}",:a-z0-9 ]{0,60}"#) {
            match from_json(&text) {
                Ok(references) => prop_assert!(references.len() < 100),
                Err(error) => prop_assert!(!error.to_string().is_empty()),
            }
        }
    }
}
