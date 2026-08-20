//! Citations, papers, and what makes a document a research document.
//!
//! specs.md lists Research as one of the menus and Research Note as one of the
//! format profiles. This crate is what those mean: a bibliography that can be
//! read and written without loss ([`bibtex`], [`csl`]), a model of a cited
//! work ([`model`]) and of a paper's own metadata ([`paper`]), a rendering of
//! either into the styles a reader expects ([`style`]), and the settings that
//! separate research mode from ordinary editing ([`profile`]).
//!
//! **Everything here is offline, and that is ADR-0006, not a limitation of the
//! implementation.** A DOI has a grammar, so it can be recognised locally; it
//! has no meaning locally, so it cannot be resolved. The features that would
//! need a service --- fetching a reference from a DOI or an arXiv id,
//! completing a half-typed citation from Crossref or OpenAlex, checking
//! whether a cited paper has been retracted, downloading a PDF --- are
//! *absent*, not stubbed. Adding any of them would put a network call on a
//! path the editor is promised to work without, which needs an ADR
//! superseding ADR-0006 and a human to write it. `bp-research` has no HTTP
//! client and no dependency that implies one, and [`ResearchProfile`] has a
//! test whose whole job is to notice if that changes.
//!
//! The second rule, and the one most of the code is shaped by: **nothing is
//! dropped silently.** A `.bib` file that cannot be understood stops the parse
//! with a line and a column rather than yielding an entry missing a field. An
//! author line that could be read two ways is handed back unsplit rather than
//! split wrongly. A DOI that does not match the grammar is still printed. The
//! failure this guards against is the one nobody notices until a reviewer
//! follows a citation and finds the wrong paper.
//!
//! Pure, like `bp-semantic` and `bp-secrets`: no clock, no filesystem, no
//! network, no configuration. The settings that would otherwise be read from
//! somewhere arrive as a [`ResearchProfile`] parameter.

#![forbid(unsafe_code)]

pub mod bibtex;
pub mod csl;
pub mod model;
pub mod paper;
pub mod profile;
pub mod style;

/// Crate identity used by workspace smoke tests and diagnostics.
pub const CRATE_NAME: &str = "bp-research";

pub use bibtex::{Bibliography, BibtexError, BibtexErrorKind, Position};
pub use model::{Entry, EntryKind, Name, Reference};
pub use paper::{ArxivId, Doi, Identifier, IdentifierError, Located, Paper};
pub use profile::{
    BibliographyOrder, GivenNames, ResearchProfile, Resolution, UnresolvedCitations,
    UnresolvedCitationsError,
};
pub use style::Style;

#[cfg(test)]
mod tests {
    use super::*;

    /// The path a user actually walks: a `.bib` file arrives, a document
    /// cites some of it, and a bibliography comes out the other end. Held
    /// together here because each half is tested in its own module and the
    /// seam between them is where a refactor breaks things.
    #[test]
    fn a_bibliography_goes_from_a_file_to_a_rendered_reference_list() {
        let file = "@string{jstor = {Journal of Storage}}

            @article{smith2019,
              author  = {Smith, Jane Q. and van der Berg, Piet},
              title   = {On {DNA} Caching},
              journal = jstor,
              volume  = {12},
              number  = {3},
              pages   = {101--114},
              year    = 2019,
              doi     = {https://doi.org/10.1000/ABC},
            }

            @book{knuth1984,
              author    = {Knuth, Donald E.},
              title     = {The {TeX}book},
              publisher = {Addison-Wesley},
              year      = 1984,
            }";

        let bibliography = bibtex::parse(file).expect("a real-shaped file parses");
        let profile = ResearchProfile::authoring(Style::Apa);

        let resolution = profile
            .resolve_citations(&["knuth1984", "smith2019"], &bibliography)
            .expect("both keys are in the file");
        assert_eq!(resolution.found.len(), 2);
        assert!(resolution.missing.is_empty());

        let references: Vec<Reference> = resolution
            .found
            .iter()
            .map(|entry| Reference::from_entry(entry))
            .collect();
        let lines = style::render_bibliography(&references, &profile);

        assert_eq!(
            lines,
            vec![
                "Knuth, D. E. (1984). The TeXbook. Addison-Wesley.".to_owned(),
                "Smith, J. Q., & van der Berg, P. (2019). On DNA Caching. Journal of Storage, \
                 12(3), 101-114. https://doi.org/10.1000/abc"
                    .to_owned(),
            ],
            "macros expanded, protection stripped, DOI normalised, sorted by author"
        );
    }

    /// The same file, out through the other format and back, unchanged.
    #[test]
    fn a_bibliography_survives_a_trip_through_csl_json() {
        let bibliography =
            bibtex::parse("@phdthesis{k, author = {Doe, Jane}, title = {A Thesis}, year = 2001}")
                .unwrap();
        let references = bibliography.references();
        assert_eq!(
            csl::from_json(&csl::to_json(&references)).unwrap(),
            references
        );
    }

    /// With research mode off, none of this happens --- ADR-0006's promise
    /// that the editor works with every research feature disabled.
    #[test]
    fn research_mode_off_leaves_a_document_as_plain_text() {
        let bibliography = bibtex::parse("@book{k, author = {Doe, J}, title = {T}}").unwrap();
        let references = bibliography.references();
        let off = ResearchProfile::off();

        assert!(style::render_bibliography(&references, &off).is_empty());
        assert_eq!(style::render_in_text(&references[0], 1, &off), "[k]");
        assert!(off.identifiers_in("10.1000/abc").is_empty());
    }

    #[test]
    fn the_crate_names_itself() {
        assert_eq!(CRATE_NAME, "bp-research");
    }
}
