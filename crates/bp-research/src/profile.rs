//! What "research mode" is, expressed as settings rather than as a mood.
//!
//! specs.md lists Research as a menu and Research Note as a format profile, so
//! there is a mode; this is what it consists of. The test that decided every
//! field below: **would an ordinary note behave differently because of it?**
//! A citation key renders as a formatted reference instead of staying literal;
//! a DOI in the text gets offered instead of being ordinary prose; a citation
//! with no matching entry stops an export instead of passing silently. Those
//! are research mode. Spell check and word wrap are not, and are not here.
//!
//! Every axis has a reader in this crate --- `bp-security` makes the same
//! point about its policy, and it is the same failure being avoided: a
//! setting nothing consults reads as a promise while being decoration.
//!
//! **[`ResearchProfile::default`] is off.** ADR-0006 requires the editor to
//! work with every research feature disabled, and a default that switched them
//! on would make the plain editor's behaviour depend on this crate. Off also
//! means the shell can hold a profile for every document without deciding
//! anything.
//!
//! Nothing here can reach the network, and there is a test whose whole job is
//! to notice if a field is ever added that could.

use serde::{Deserialize, Serialize};

use crate::bibtex::Bibliography;
use crate::model::Entry;
use crate::paper::{Identifier, Located, find_identifiers};
use crate::style::Style;

/// Whether given names are spelled out or reduced to initials.
///
/// A separate axis from [`Style`] because venues override their own style
/// here more often than anywhere else: an APA-shaped house style that spells
/// names out in full is a common instruction, and the alternative to this
/// field is a fifth style that differs from APA in one respect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum GivenNames {
    /// Whatever the chosen style does. The right answer almost always.
    #[default]
    StyleDefault,
    Full,
    Initials,
}

/// The order a rendered bibliography comes out in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum BibliographyOrder {
    /// By author, then year, then title. What author-date styles require.
    #[default]
    Alphabetical,
    /// The order the document cited them in. What numbered styles require,
    /// and the reason this is a setting rather than a consequence of the
    /// style: a numbered style with an alphabetical list is a real house
    /// convention, not a mistake.
    AsCited,
}

/// What happens when a citation names an entry that does not exist.
///
/// Two values, not three: a "warn" that the caller may ignore and an "ignore"
/// are the same setting from this crate's side, and the difference belongs to
/// whoever displays the result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum UnresolvedCitations {
    /// Resolve what can be resolved and hand back the rest, named.
    #[default]
    Report,
    /// Refuse the whole resolution. For an export: a paper that ships with a
    /// dangling citation is a paper that has to be reissued.
    Refuse,
}

/// The settings that make an ordinary document a research document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResearchProfile {
    /// The master switch ADR-0006 requires. With this off, citations stay the
    /// literal keys the user typed and no document is scanned.
    pub enabled: bool,
    /// The style citations and the bibliography render in.
    pub style: Style,
    /// Whether DOIs and arXiv identifiers in a document are offered to the
    /// user. Off is a real preference: in a document *about* identifiers,
    /// every one of them lighting up is noise.
    pub detect_identifiers: bool,
    /// Whether a rendered reference ends with its DOI. Off for venues whose
    /// house style has no room for one, and for a printed document where a
    /// forty-character URL is a line of ink.
    pub include_doi: bool,
    pub given_names: GivenNames,
    /// Override for how many authors are listed before "et al.". `None`
    /// takes the style's own limit, which is the answer unless a venue has
    /// said otherwise.
    pub author_limit: Option<usize>,
    pub bibliography_order: BibliographyOrder,
    pub unresolved_citations: UnresolvedCitations,
}

impl Default for ResearchProfile {
    fn default() -> Self {
        Self::off()
    }
}

impl ResearchProfile {
    /// Research mode off: what every document gets until somebody asks.
    #[must_use]
    pub const fn off() -> Self {
        Self {
            enabled: false,
            style: Style::Apa,
            detect_identifiers: false,
            include_doi: true,
            given_names: GivenNames::StyleDefault,
            author_limit: None,
            bibliography_order: BibliographyOrder::Alphabetical,
            unresolved_citations: UnresolvedCitations::Report,
        }
    }

    /// Reading somebody else's work: identifiers are offered, and a citation
    /// that does not resolve is reported rather than fatal, because the
    /// bibliography being read is not the reader's to fix.
    #[must_use]
    pub const fn reading(style: Style) -> Self {
        Self {
            enabled: true,
            style,
            detect_identifiers: true,
            ..Self::off()
        }
    }

    /// Writing one's own: the same, but a dangling citation stops an export.
    #[must_use]
    pub const fn authoring(style: Style) -> Self {
        Self {
            enabled: true,
            style,
            detect_identifiers: true,
            unresolved_citations: UnresolvedCitations::Refuse,
            ..Self::off()
        }
    }

    /// How many authors this profile lists before "et al.".
    ///
    /// A limit of zero is a value the type allows and the domain does not ---
    /// "et al." with nobody in front of it names no one --- so it is raised to
    /// one rather than honoured.
    #[must_use]
    pub fn effective_author_limit(&self) -> usize {
        self.author_limit
            .map_or_else(|| self.style.author_limit(), |limit| limit.max(1))
    }

    /// Whether given names come out as initials under this profile.
    #[must_use]
    pub fn use_initials(&self) -> bool {
        match self.given_names {
            GivenNames::StyleDefault => self.style.uses_initials(),
            GivenNames::Full => false,
            GivenNames::Initials => true,
        }
    }

    /// The identifiers this profile permits offering for `text`.
    ///
    /// The gate lives here rather than in the caller so that "research mode
    /// is off" cannot be forgotten at one of several call sites.
    #[must_use]
    pub fn identifiers_in(&self, text: &str) -> Vec<Located<Identifier>> {
        if self.enabled && self.detect_identifiers {
            find_identifiers(text)
        } else {
            Vec::new()
        }
    }

    /// Match citation keys against a bibliography.
    ///
    /// Duplicate keys in `keys` resolve once each: a document may cite the
    /// same work five times, and a bibliography that listed it five times
    /// would be wrong. Order follows first citation, which is what
    /// [`BibliographyOrder::AsCited`] then depends on.
    ///
    /// # Errors
    ///
    /// [`UnresolvedCitations::Refuse`] and at least one key with no entry.
    pub fn resolve_citations<'a>(
        &self,
        keys: &[&str],
        bibliography: &'a Bibliography,
    ) -> Result<Resolution<'a>, UnresolvedCitationsError> {
        let mut resolution = Resolution::default();
        for key in keys {
            match bibliography.entry(key) {
                Some(entry) => {
                    if !resolution.found.iter().any(|e| e.key == entry.key) {
                        resolution.found.push(entry);
                    }
                }
                None => {
                    let key = (*key).to_owned();
                    if !resolution.missing.contains(&key) {
                        resolution.missing.push(key);
                    }
                }
            }
        }

        if self.unresolved_citations == UnresolvedCitations::Refuse
            && !resolution.missing.is_empty()
        {
            return Err(UnresolvedCitationsError(resolution.missing));
        }
        Ok(resolution)
    }
}

/// What a document's citation keys turned out to be.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Resolution<'a> {
    /// The entries found, in order of first citation, each once.
    pub found: Vec<&'a Entry>,
    /// Keys with no entry, in order of first citation, each once. Never
    /// discarded: a citation that resolves to nothing is the single most
    /// common way a bibliography goes wrong, and it has to be visible.
    pub missing: Vec<String>,
}

/// Citations that name nothing.
///
/// Carries every missing key rather than the first, because a user fixing
/// their bibliography wants the whole list, not one round trip per typo.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("the bibliography has no entry for: {}", .0.join(", "))]
pub struct UnresolvedCitationsError(pub Vec<String>);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bibtex;

    fn bibliography() -> Bibliography {
        bibtex::parse("@article{a, title = {A}}\n@book{b, title = {B}}").unwrap()
    }

    #[test]
    fn the_default_profile_is_off() {
        // ADR-0006: the plain editor must not acquire behaviour from here.
        let profile = ResearchProfile::default();
        assert!(!profile.enabled);
        assert!(!profile.detect_identifiers);
        assert_eq!(profile, ResearchProfile::off());
    }

    #[test]
    fn a_disabled_profile_scans_nothing() {
        let text = "See 10.1000/xyz for details.";
        assert!(ResearchProfile::off().identifiers_in(text).is_empty());
        assert_eq!(
            ResearchProfile::reading(Style::Apa)
                .identifiers_in(text)
                .len(),
            1
        );
    }

    #[test]
    fn identifier_detection_can_be_switched_off_on_its_own() {
        let mut profile = ResearchProfile::reading(Style::Apa);
        profile.detect_identifiers = false;
        assert!(profile.identifiers_in("10.1000/xyz").is_empty());
    }

    #[test]
    fn an_author_limit_of_zero_is_raised_rather_than_honoured() {
        // "et al." with nobody in front of it names no one.
        let mut profile = ResearchProfile::authoring(Style::Apa);
        profile.author_limit = Some(0);
        assert_eq!(profile.effective_author_limit(), 1);
    }

    #[test]
    fn an_author_limit_of_none_defers_to_the_style() {
        let profile = ResearchProfile::authoring(Style::Mla);
        assert_eq!(profile.effective_author_limit(), Style::Mla.author_limit());
    }

    #[test]
    fn given_names_may_override_the_style_in_either_direction() {
        let mut profile = ResearchProfile::authoring(Style::Apa);
        assert!(profile.use_initials(), "APA abbreviates");
        profile.given_names = GivenNames::Full;
        assert!(!profile.use_initials());

        let mut chicago = ResearchProfile::authoring(Style::ChicagoAuthorDate);
        assert!(!chicago.use_initials(), "Chicago spells names out");
        chicago.given_names = GivenNames::Initials;
        assert!(chicago.use_initials());
    }

    #[test]
    fn resolving_citations_finds_each_entry_once_in_citation_order() {
        let bibliography = bibliography();
        let resolution = ResearchProfile::reading(Style::Apa)
            .resolve_citations(&["b", "a", "b"], &bibliography)
            .unwrap();
        let keys: Vec<&str> = resolution.found.iter().map(|e| e.key.as_str()).collect();
        assert_eq!(keys, vec!["b", "a"], "first citation decides the order");
        assert!(resolution.missing.is_empty());
    }

    #[test]
    fn a_reporting_profile_hands_back_the_keys_it_could_not_find() {
        let bibliography = bibliography();
        let resolution = ResearchProfile::reading(Style::Apa)
            .resolve_citations(&["a", "ghost", "ghost"], &bibliography)
            .unwrap();
        assert_eq!(resolution.found.len(), 1);
        assert_eq!(resolution.missing, vec!["ghost".to_owned()]);
    }

    #[test]
    fn an_authoring_profile_refuses_and_lists_every_missing_key() {
        let bibliography = bibliography();
        let error = ResearchProfile::authoring(Style::Apa)
            .resolve_citations(&["a", "ghost", "phantom"], &bibliography)
            .expect_err("must refuse");
        assert_eq!(error.0, vec!["ghost".to_owned(), "phantom".to_owned()]);
        assert_eq!(
            error.to_string(),
            "the bibliography has no entry for: ghost, phantom",
            "the whole list, so the fix is one pass"
        );
    }

    #[test]
    fn citing_nothing_resolves_to_nothing_without_complaint() {
        let bibliography = bibliography();
        let resolution = ResearchProfile::authoring(Style::Apa)
            .resolve_citations(&[], &bibliography)
            .unwrap();
        assert_eq!(resolution, Resolution::default());
    }

    #[test]
    fn resolving_against_an_empty_bibliography_reports_every_key() {
        let empty = Bibliography::default();
        let resolution = ResearchProfile::reading(Style::Apa)
            .resolve_citations(&["a"], &empty)
            .unwrap();
        assert_eq!(resolution.missing, vec!["a".to_owned()]);
    }

    #[test]
    fn no_research_setting_can_reach_the_network() {
        // ADR-0006 is binding and this crate has no HTTP in it, which is easy
        // to keep true today and easy to lose later. This fails the moment a
        // field appears whose name implies a lookup, which is the point at
        // which somebody has to write an ADR instead of a struct field.
        let serialised = serde_json::to_string(&ResearchProfile::authoring(Style::Apa))
            .expect("a profile serialises");
        for word in [
            "url", "http", "fetch", "resolver", "lookup", "online", "sync", "remote", "api",
            "server", "endpoint", "download", "crossref", "provider",
        ] {
            assert!(
                !serialised.to_lowercase().contains(word),
                "a research setting named for {word:?} would be a network dependency: {serialised}"
            );
        }
    }

    #[test]
    fn a_profile_survives_being_stored_and_read_back() {
        // A profile travels with a document, so it has to serialise.
        let profile = ResearchProfile::authoring(Style::Ieee);
        let json = serde_json::to_string(&profile).unwrap();
        assert_eq!(
            serde_json::from_str::<ResearchProfile>(&json).unwrap(),
            profile
        );
    }
}
