//! Identifiers with a shape, and the metadata of a document that is a paper.
//!
//! ADR-0006 draws the line this module lives on. A DOI can be *recognised*
//! offline --- it has a grammar --- and it cannot be *resolved* offline,
//! because resolving one means asking doi.org. So this module recognises, and
//! stops. Nothing here fetches anything; [`Doi::url`] returns text for the
//! user to copy into their own browser, and is the closest this crate comes
//! to the network.
//!
//! The second rule is [`Paper`]'s: **a wrong author is worse than no author.**
//! Everything recovered from plain text here is recovered from something the
//! document *states* --- an identifier with a grammar, or a labelled header
//! line --- never from a guess about which line looks like a title or which
//! commas separate people. Where the evidence is ambiguous, the raw text is
//! handed back untouched ([`Paper::authors_text`]) instead of a plausible
//! wrong answer, so the user is asked rather than misinformed.

use serde::{Deserialize, Serialize};

use crate::model::{Entry, Name};

/// Why a string is not the identifier it was offered as.
///
/// Both messages describe the shape that was expected, because the user is
/// looking at their own typo and needs to know what it should have looked
/// like.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum IdentifierError {
    #[error(
        "\"{0}\" is not a DOI: a DOI is \"10.\", a registrant number of at \
         least four digits, a slash, and a suffix"
    )]
    NotADoi(String),

    #[error(
        "\"{0}\" is not an arXiv identifier: those look like 2401.01234 or \
         math.GT/0309136, optionally followed by a version such as v2"
    )]
    NotAnArxivId(String),
}

/// A Digital Object Identifier, in its canonical form.
///
/// A newtype rather than a `String` so that "this has been checked against
/// the grammar" is carried by the type, and lower-cased on the way in because
/// DOIs are case-insensitive by specification --- two records differing only
/// in case are one work, and a bibliography that lists both is wrong.
///
/// The stored form never carries a `doi:` prefix or an https wrapper; those
/// are ways of *writing* a DOI, and keeping them would make equality depend
/// on which website the user copied from.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Doi(String);

impl Doi {
    /// Recognise a DOI, however it was written down.
    ///
    /// # Errors
    ///
    /// [`IdentifierError::NotADoi`] when the text does not have the shape.
    /// Recognising loosely and storing wrongly would be worse than refusing:
    /// a malformed DOI in a bibliography is a reference nobody can follow.
    pub fn parse(text: &str) -> Result<Self, IdentifierError> {
        let trimmed = text.trim();
        let body = strip_doi_prefix(trimmed);
        match doi_body_len(body) {
            Some(len) if len == body.len() => Ok(Self(body.to_ascii_lowercase())),
            _ => Err(IdentifierError::NotADoi(trimmed.to_owned())),
        }
    }

    /// The canonical `10.xxxx/yyyy` text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The canonical text, owned.
    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }

    /// The resolver address, as a string for the user to open themselves.
    ///
    /// This crate never opens it. ADR-0006 forbids the editor depending on a
    /// service, and a DOI that silently phoned home would be exactly that;
    /// producing the text and letting the user decide keeps the decision
    /// where it belongs.
    #[must_use]
    pub fn url(&self) -> String {
        format!("https://doi.org/{}", self.0)
    }
}

impl std::fmt::Display for Doi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for Doi {
    type Error = IdentifierError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<Doi> for String {
    fn from(value: Doi) -> Self {
        value.0
    }
}

/// An arXiv identifier, with its version if one was given.
///
/// The version is kept separate because it is a different claim: `2401.01234`
/// is the paper and `2401.01234v2` is one revision of it. A citation of the
/// paper should not silently become a citation of whichever revision the user
/// happened to download.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ArxivId {
    /// The identifier without its version: `2401.01234`, `math.GT/0309136`.
    pub id: String,
    pub version: Option<u32>,
}

impl ArxivId {
    /// Recognise an arXiv identifier, with or without an `arXiv:` prefix or
    /// an `arxiv.org/abs/` wrapper.
    ///
    /// # Errors
    ///
    /// [`IdentifierError::NotAnArxivId`] when the text does not have either
    /// of the two shapes arXiv has used.
    pub fn parse(text: &str) -> Result<Self, IdentifierError> {
        let trimmed = text.trim();
        let body = strip_arxiv_prefix(trimmed);
        match arxiv_body(body) {
            Some((id, len)) if len == body.len() => Ok(id),
            _ => Err(IdentifierError::NotAnArxivId(trimmed.to_owned())),
        }
    }

    /// The identifier including its version, if it has one.
    #[must_use]
    pub fn to_text(&self) -> String {
        match self.version {
            Some(version) => format!("{}v{version}", self.id),
            None => self.id.clone(),
        }
    }

    /// The abstract page, as text for the user to open themselves. See
    /// [`Doi::url`] for why nothing here follows it.
    #[must_use]
    pub fn url(&self) -> String {
        format!("https://arxiv.org/abs/{}", self.to_text())
    }
}

impl std::fmt::Display for ArxivId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_text())
    }
}

impl TryFrom<String> for ArxivId {
    type Error = IdentifierError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<ArxivId> for String {
    fn from(value: ArxivId) -> Self {
        value.to_text()
    }
}

/// Something recognised in a document, and where it was.
///
/// Modelled on `bp_secrets::Finding`: 1-based line and character column, and
/// a length in characters, so the shell can put the caret on it or underline
/// it. Unlike a secret, the value itself is carried --- a DOI is a citation,
/// not a credential, and the whole point is to offer it to the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Located<T> {
    pub value: T,
    pub line: usize,
    pub column: usize,
    /// Length of the text as it was written, in characters --- which is not
    /// the length of `value`, because the canonical form drops prefixes and
    /// changes case.
    pub length: usize,
}

/// One of the identifiers this crate can recognise in running text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Identifier {
    Doi(Doi),
    Arxiv(ArxivId),
}

/// Find every DOI and arXiv identifier in a document, in reading order.
///
/// Linear, one pass, no backtracking --- the same discipline as
/// `bp-secrets`, and for the same reason: this runs over whatever the user
/// just opened, and a pathological input must not become a hang.
///
/// **Bare arXiv numbers are not detected.** `2401.01234` on its own is
/// indistinguishable from a version number, a price or a date, and offering
/// the user a citation for it would be guessing. An `arXiv:` prefix or an
/// arxiv.org address is required, because that is the document asserting what
/// the number is.
#[must_use]
pub fn find_identifiers(text: &str) -> Vec<Located<Identifier>> {
    let mut found = Vec::new();
    for (index, line) in text.lines().enumerate() {
        scan_line(line, index + 1, &mut found);
    }
    found
}

fn scan_line(line: &str, line_number: usize, found: &mut Vec<Located<Identifier>>) {
    let mut offset = 0usize;
    let mut column = 1usize;
    while offset < line.len() {
        if let Some((value, end)) = identifier_at(line, offset) {
            let length = line[offset..end].chars().count();
            found.push(Located {
                value,
                line: line_number,
                column,
                length,
            });
            column += length;
            offset = end;
            continue;
        }
        let width = line[offset..].chars().next().map_or(1, char::len_utf8);
        offset += width;
        column += 1;
    }
}

/// An identifier starting exactly at `offset`, and where it ends.
fn identifier_at(line: &str, offset: usize) -> Option<(Identifier, usize)> {
    if !starts_a_token(line, offset) {
        return None;
    }
    let rest = &line[offset..];

    if let Some(len) = doi_body_len(rest) {
        let text = trim_trailing_punctuation(&rest[..len]);
        if let Ok(doi) = Doi::parse(text) {
            return Some((Identifier::Doi(doi), offset + text.len()));
        }
    }

    let prefix = arxiv_prefix_len(rest)?;
    let (id, len) = arxiv_body(rest[prefix..].trim_start())?;
    // The trimmed whitespace between `arXiv:` and the number counts towards
    // the span, or the highlight would stop short of the identifier.
    let skipped = rest[prefix..].len() - rest[prefix..].trim_start().len();
    Some((Identifier::Arxiv(id), offset + prefix + skipped + len))
}

/// Whether a match may begin here: an identifier does not start in the middle
/// of a word, and `3.10.1000/x` is a version number, not a DOI.
fn starts_a_token(line: &str, offset: usize) -> bool {
    if offset == 0 {
        return true;
    }
    line[..offset]
        .chars()
        .next_back()
        .is_none_or(|c| !c.is_alphanumeric() && c != '.' && c != '-')
}

/// Drop whichever of the several ways of writing a DOI is in front.
fn strip_doi_prefix(text: &str) -> &str {
    const PREFIXES: &[&str] = &[
        "https://doi.org/",
        "http://doi.org/",
        "https://dx.doi.org/",
        "http://dx.doi.org/",
        "doi.org/",
        "doi:",
        "DOI:",
    ];
    for prefix in PREFIXES {
        // `get` rather than a slice: the text may be arbitrary UTF-8, and
        // `prefix.len()` can land inside a multi-byte character.
        if text
            .get(..prefix.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
        {
            return text[prefix.len()..].trim_start();
        }
    }
    text
}

/// The byte length of the DOI at the start of `text`, if there is one.
///
/// The suffix runs to the first whitespace or angle bracket. It is
/// deliberately greedy: DOI suffixes legitimately contain brackets, semicolons
/// and full stops, so the trailing punctuation of the sentence has to be
/// removed afterwards ([`trim_trailing_punctuation`]) rather than avoided
/// here.
fn doi_body_len(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    if !text.starts_with("10.") {
        return None;
    }
    let mut index = 3usize;
    let mut digits = 0usize;
    while index < bytes.len() && (bytes[index].is_ascii_digit() || bytes[index] == b'.') {
        if bytes[index].is_ascii_digit() {
            digits += 1;
        }
        index += 1;
    }
    // Four digits is the registered minimum; three would match a decimal
    // number followed by a path.
    if digits < 4 || bytes.get(index) != Some(&b'/') {
        return None;
    }
    index += 1;
    let suffix_start = index;
    while index < bytes.len()
        && !bytes[index].is_ascii_whitespace()
        && !matches!(bytes[index], b'<' | b'>' | b'"')
    {
        index += 1;
    }
    (index > suffix_start).then_some(index)
}

/// Remove the punctuation that ended the sentence rather than the DOI.
///
/// The classic failure: `... see 10.1000/xyz.` yields a DOI ending in a full
/// stop that resolves to nothing. Closing brackets are only dropped when they
/// have no opener inside the suffix, because `10.1000/(a)b)` is not the same
/// identifier as `10.1000/(a)b`.
fn trim_trailing_punctuation(text: &str) -> &str {
    let mut end = text.len();
    loop {
        let slice = &text[..end];
        let Some(last) = slice.chars().next_back() else {
            return slice;
        };
        let drop = match last {
            '.' | ',' | ';' | ':' | '"' | '\'' => true,
            ')' => slice.matches('(').count() < slice.matches(')').count(),
            ']' => slice.matches('[').count() < slice.matches(']').count(),
            '}' => slice.matches('{').count() < slice.matches('}').count(),
            _ => false,
        };
        if !drop {
            return slice;
        }
        end -= last.len_utf8();
    }
}

/// The length of an `arXiv:` or `arxiv.org/abs/` marker at the start.
fn arxiv_prefix_len(text: &str) -> Option<usize> {
    const PREFIXES: &[&str] = &[
        "https://arxiv.org/abs/",
        "http://arxiv.org/abs/",
        "arxiv.org/abs/",
        "arxiv:",
    ];
    PREFIXES
        .iter()
        .find(|prefix| {
            text.get(..prefix.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
        })
        .map(|prefix| prefix.len())
}

/// Same, but tolerant of the identifier being handed over on its own.
fn strip_arxiv_prefix(text: &str) -> &str {
    match arxiv_prefix_len(text) {
        Some(len) => text[len..].trim_start(),
        None => text,
    }
}

/// Parse either arXiv identifier scheme, returning it and its byte length.
///
/// Both schemes are supported because arXiv changed format in April 2007 and
/// every paper from before then still cites the old one; recognising only the
/// new form would fail on two decades of references.
fn arxiv_body(text: &str) -> Option<(ArxivId, usize)> {
    let bytes = text.as_bytes();

    // New style: YYMM.NNNNN
    let leading_digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
    if leading_digits == 4 && bytes.get(4) == Some(&b'.') {
        let tail = bytes[5..].iter().take_while(|b| b.is_ascii_digit()).count();
        if (4..=5).contains(&tail) {
            let end = 5 + tail;
            let (version, version_len) = parse_version(&text[end..]);
            return Some((
                ArxivId {
                    id: text[..end].to_owned(),
                    version,
                },
                end + version_len,
            ));
        }
        return None;
    }

    // Old style: archive(.subject)/YYMMNNN, where the archive name may itself
    // contain a hyphen (`cond-mat.stat-mech`).
    let mut index = 0usize;
    while index < bytes.len()
        && (bytes[index].is_ascii_alphabetic() || matches!(bytes[index], b'-' | b'.'))
    {
        index += 1;
    }
    if index == 0 || bytes.get(index) != Some(&b'/') {
        return None;
    }
    let digits = bytes[index + 1..]
        .iter()
        .take_while(|b| b.is_ascii_digit())
        .count();
    if digits != 7 {
        return None;
    }
    let end = index + 1 + digits;
    let (version, version_len) = parse_version(&text[end..]);
    Some((
        ArxivId {
            id: text[..end].to_ascii_lowercase(),
            version,
        },
        end + version_len,
    ))
}

/// A trailing `v12`, and how many bytes it took.
fn parse_version(text: &str) -> (Option<u32>, usize) {
    let bytes = text.as_bytes();
    if bytes.first() != Some(&b'v') {
        return (None, 0);
    }
    let digits = bytes[1..].iter().take_while(|b| b.is_ascii_digit()).count();
    if digits == 0 {
        return (None, 0);
    }
    match text[1..1 + digits].parse() {
        Ok(version) => (Some(version), 1 + digits),
        Err(_) => (None, 0),
    }
}

/// How far into a document a labelled header is still a header.
///
/// A `Title:` forty lines down is prose quoting a form, not this document's
/// title. The bound is what stops a header scan turning into a guess about
/// the whole document.
pub const HEADER_SCAN_LINES: usize = 40;

/// What a document that is a paper says about itself.
///
/// Every field is optional because every field is genuinely absent from some
/// real paper, and because a `Paper` is built from evidence rather than from
/// a schema: a document with only a DOI in it produces a `Paper` with only a
/// DOI in it, which is exactly as much as is known.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Paper {
    pub title: Option<String>,
    /// The authors, where they could be separated without guessing.
    pub authors: Vec<Name>,
    /// The author line exactly as written, whenever there was one.
    ///
    /// Kept even when `authors` was filled in, and *especially* when it was
    /// not: `Smith, J., Doe, A.` cannot be split into people without deciding
    /// whether the first comma separates a surname from an initial or one
    /// person from the next, and both readings are common. Handing the raw
    /// line back lets the user settle it; inventing an answer would put a
    /// person who does not exist into a citation.
    pub authors_text: Option<String>,
    pub doi: Option<Doi>,
    pub arxiv: Option<ArxivId>,
    pub year: Option<i32>,
    /// Journal, conference or publisher --- wherever it appeared.
    pub venue: Option<String>,
    /// The abstract. Named with a suffix because `abstract` is a Rust keyword.
    pub abstract_text: Option<String>,
}

impl Paper {
    /// Read a paper's metadata out of a bibliographic entry.
    ///
    /// The reliable direction: an `Entry` has already said which text is the
    /// title and which is the author, so nothing here is inferred.
    #[must_use]
    pub fn from_entry(entry: &Entry) -> Self {
        let text = |name: &str| {
            entry
                .field(name)
                .map(crate::bibtex::strip_protection)
                .filter(|value| !value.is_empty())
        };
        let venue = [
            "journal",
            "journaltitle",
            "booktitle",
            "publisher",
            "school",
        ]
        .iter()
        .find_map(|name| text(name));

        Self {
            title: text("title"),
            authors: entry.authors(),
            authors_text: text("author"),
            doi: text("doi").and_then(|value| Doi::parse(&value).ok()),
            arxiv: text("eprint")
                .or_else(|| text("archiveprefix"))
                .and_then(|value| ArxivId::parse(&value).ok()),
            year: entry.year(),
            venue,
            abstract_text: text("abstract"),
        }
    }

    /// Read what a plain-text document states about itself.
    ///
    /// Two sources, and no third: labelled header lines (`Title:`, `Authors:`,
    /// `DOI:` ...) in the first [`HEADER_SCAN_LINES`] lines, and identifiers
    /// anywhere in the body. There is deliberately no "the first line is
    /// probably the title" rule --- `bp-semantic` offers that as a *title
    /// suggestion*, which is a different and revocable claim from asserting a
    /// paper's published title.
    ///
    /// Markdown decoration around a label is tolerated (`## Title:`,
    /// `**Authors:**`) because a research note is usually written in Markdown
    /// and the decoration is not part of what the line says.
    #[must_use]
    pub fn from_text(text: &str) -> Self {
        let mut paper = Self::default();
        let lines: Vec<&str> = text.lines().take(HEADER_SCAN_LINES).collect();

        let mut index = 0usize;
        while index < lines.len() {
            let Some((label, value)) = labelled_line(lines[index]) else {
                index += 1;
                continue;
            };
            index += 1;

            match label.as_str() {
                "title" => set_once(&mut paper.title, non_empty(value)),
                "author" | "authors" => {
                    if paper.authors_text.is_none() {
                        paper.authors = split_people(value);
                        paper.authors_text = non_empty(value);
                    }
                }
                "doi" => set_once(&mut paper.doi, Doi::parse(value).ok()),
                "arxiv" => set_once(&mut paper.arxiv, ArxivId::parse(value).ok()),
                "year" | "date" | "published" => {
                    set_once(&mut paper.year, crate::model::first_four_digit_year(value));
                }
                "venue" | "journal" | "conference" | "booktitle" | "in" => {
                    set_once(&mut paper.venue, non_empty(value));
                }
                "abstract" => {
                    // An abstract is a paragraph, so it keeps going until a
                    // blank line or the next label -- otherwise every
                    // abstract in the world would be truncated at its first
                    // line break.
                    let mut collected = vec![value.trim().to_owned()];
                    while let Some(next) = lines.get(index) {
                        if next.trim().is_empty() || labelled_line(next).is_some() {
                            break;
                        }
                        collected.push(next.trim().to_owned());
                        index += 1;
                    }
                    let joined = collected.join(" ").trim().to_owned();
                    set_once(&mut paper.abstract_text, non_empty(&joined));
                }
                _ => {}
            }
        }

        // Identifiers anywhere in the document, but never overriding one the
        // header stated: a DOI in the header is this paper's, a DOI in the
        // body may belong to something it cites.
        for found in find_identifiers(text) {
            match found.value {
                Identifier::Doi(doi) => set_once(&mut paper.doi, Some(doi)),
                Identifier::Arxiv(id) => set_once(&mut paper.arxiv, Some(id)),
            }
        }

        paper
    }

    /// Whether anything at all was recovered.
    ///
    /// The shell needs this to decide whether to offer a Research Note header
    /// at all; offering an empty one on every plain document would be noise.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

fn set_once<T>(slot: &mut Option<T>, value: Option<T>) {
    if slot.is_none() {
        *slot = value;
    }
}

fn non_empty(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// Split an author line only where the separator is unambiguous.
///
/// `;` first, then the BibTeX `and`. A line separated only by commas is left
/// alone --- see [`Paper::authors_text`] for why guessing there is worse than
/// declining.
fn split_people(value: &str) -> Vec<Name> {
    if value.contains(';') {
        return value.split(';').filter_map(Name::parse).collect();
    }
    let names = Name::parse_list(value);
    if names.len() > 1 {
        return names;
    }
    // One name, and it may be one person written `Family, Given` or three
    // people written with commas. One comma can only be the former.
    if value.matches(',').count() > 1 {
        return Vec::new();
    }
    names
}

/// A `Label: value` line, with Markdown decoration removed.
///
/// The colon has to be near the start: a sentence of prose containing a colon
/// is not a header, and without the bound `The point is: research is hard`
/// would become a field called "the point is".
fn labelled_line(line: &str) -> Option<(String, &str)> {
    const MAX_LABEL_CHARS: usize = 20;

    let trimmed = line.trim_start_matches(['#', '>', '-', '*', '_', ' ', '\t']);
    let colon = trimmed.find(':')?;
    let (label, rest) = trimmed.split_at(colon);
    if label.chars().count() > MAX_LABEL_CHARS {
        return None;
    }
    let label = label.trim_matches(['*', '_', ' ', '\t']).to_lowercase();
    if label.is_empty() || !label.chars().all(|c| c.is_alphabetic() || c == ' ') {
        return None;
    }
    // Both ends: `**Authors:** Smith` closes its bold marker in the value.
    Some((label, rest[1..].trim_matches(['*', '_', ' ', '\t'])))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn a_doi_is_recognised_however_it_was_written() {
        let canonical = Doi::parse("10.1000/xyz123").unwrap();
        for written in [
            "doi:10.1000/xyz123",
            "DOI: 10.1000/xyz123",
            "https://doi.org/10.1000/xyz123",
            "http://dx.doi.org/10.1000/XYZ123",
            "  10.1000/xyz123  ",
        ] {
            assert_eq!(Doi::parse(written).unwrap(), canonical, "{written}");
        }
    }

    #[test]
    fn a_doi_is_case_insensitive_because_the_specification_says_so() {
        assert_eq!(
            Doi::parse("10.1000/AbC").unwrap(),
            Doi::parse("10.1000/abc").unwrap()
        );
    }

    #[test]
    fn things_that_are_not_dois_are_refused_with_the_shape_explained() {
        for text in [
            "",
            "10.100/x",  // three digits
            "10.1000",   // no suffix
            "10.1000/",  // empty suffix
            "11.1000/x", // wrong directory
            "not a doi",
            "10.1000/x y", // whitespace inside
        ] {
            let error = Doi::parse(text).expect_err(text);
            assert!(error.to_string().contains("is not a DOI"), "{text}");
        }
    }

    #[test]
    fn a_doi_offers_a_url_but_never_follows_it() {
        // ADR-0006: this is text for the user, not a request.
        assert_eq!(
            Doi::parse("10.1000/x").unwrap().url(),
            "https://doi.org/10.1000/x"
        );
    }

    #[test]
    fn both_arxiv_schemes_are_recognised() {
        let modern = ArxivId::parse("arXiv:2401.01234").unwrap();
        assert_eq!(modern.id, "2401.01234");
        assert_eq!(modern.version, None);

        let versioned = ArxivId::parse("2401.01234v2").unwrap();
        assert_eq!(versioned.version, Some(2));

        let legacy = ArxivId::parse("math.GT/0309136").unwrap();
        assert_eq!(legacy.id, "math.gt/0309136");

        let hyphenated = ArxivId::parse("cond-mat.stat-mech/0309136v1").unwrap();
        assert_eq!(hyphenated.version, Some(1));
    }

    #[test]
    fn an_arxiv_url_is_recognised_and_reduced_to_the_identifier() {
        let id = ArxivId::parse("https://arxiv.org/abs/2401.01234v3").unwrap();
        assert_eq!(id.id, "2401.01234");
        assert_eq!(id.to_text(), "2401.01234v3");
        assert_eq!(id.url(), "https://arxiv.org/abs/2401.01234v3");
    }

    #[test]
    fn a_version_is_a_different_claim_from_the_paper() {
        let paper = ArxivId::parse("2401.01234").unwrap();
        let revision = ArxivId::parse("2401.01234v2").unwrap();
        assert_ne!(paper, revision);
        assert_eq!(paper.id, revision.id);
    }

    #[test]
    fn things_that_are_not_arxiv_identifiers_are_refused() {
        for text in [
            "",
            "240.01234",
            "2401.012",
            "2401.0123456",
            "math/123",
            "hello",
        ] {
            assert!(ArxivId::parse(text).is_err(), "{text} should not parse");
        }
    }

    #[test]
    fn a_doi_in_a_sentence_loses_the_sentences_punctuation() {
        // The classic bug: a trailing full stop that resolves to nothing.
        let found = find_identifiers("See 10.1000/xyz123. Next sentence.");
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].value,
            Identifier::Doi(Doi::parse("10.1000/xyz123").unwrap())
        );
        assert_eq!(found[0].column, 5);
        assert_eq!(found[0].length, "10.1000/xyz123".len());
    }

    #[test]
    fn a_doi_keeps_brackets_that_belong_to_it() {
        let found = find_identifiers("(see 10.1000/a(b)c).");
        assert_eq!(found.len(), 1);
        let Identifier::Doi(doi) = &found[0].value else {
            panic!("expected a DOI")
        };
        assert_eq!(doi.as_str(), "10.1000/a(b)c", "the inner pair is balanced");
    }

    #[test]
    fn a_version_number_is_not_mistaken_for_a_doi() {
        assert!(find_identifiers("upgrade 3.10.1000/2 to").is_empty());
    }

    #[test]
    fn a_bare_number_is_not_offered_as_an_arxiv_paper() {
        // Guessing here would put a random citation into a document.
        assert!(find_identifiers("version 2401.01234 shipped").is_empty());
        assert_eq!(find_identifiers("arXiv:2401.01234").len(), 1);
    }

    #[test]
    fn identifiers_report_where_they_are_in_characters() {
        let text = "one\n日本語 arXiv:2401.01234 tail";
        let found = find_identifiers(text);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 2);
        assert_eq!(found[0].column, 5, "three characters, nine bytes");
        assert_eq!(found[0].length, "arXiv:2401.01234".chars().count());
    }

    #[test]
    fn a_document_with_no_identifiers_yields_none() {
        assert!(find_identifiers("").is_empty());
        assert!(find_identifiers("just some prose, 10 percent of it").is_empty());
    }

    #[test]
    fn a_labelled_header_is_read_field_by_field() {
        let paper = Paper::from_text(
            "# Notes on caching\n\
             Title: An Empirical Study of Caching\n\
             Authors: Smith, Jane and Doe, John\n\
             Year: 2019\n\
             Journal: Transactions on Storage\n\
             DOI: 10.1000/abc\n\
             \n\
             Body text.",
        );
        assert_eq!(
            paper.title.as_deref(),
            Some("An Empirical Study of Caching")
        );
        assert_eq!(paper.authors.len(), 2);
        assert_eq!(paper.authors[0].family, "Smith");
        assert_eq!(paper.year, Some(2019));
        assert_eq!(paper.venue.as_deref(), Some("Transactions on Storage"));
        assert_eq!(paper.doi.as_ref().map(Doi::as_str), Some("10.1000/abc"));
    }

    #[test]
    fn markdown_decoration_around_a_label_is_ignored() {
        let paper = Paper::from_text("**Title:** Something\n> Year: 2020");
        assert_eq!(paper.title.as_deref(), Some("Something"));
        assert_eq!(paper.year, Some(2020));
    }

    #[test]
    fn a_comma_separated_author_line_is_handed_back_rather_than_guessed_at() {
        let paper = Paper::from_text("Authors: Smith, J., Doe, A., Roe, R.");
        assert!(
            paper.authors.is_empty(),
            "three people or one, and nothing in the line says which"
        );
        assert_eq!(
            paper.authors_text.as_deref(),
            Some("Smith, J., Doe, A., Roe, R.")
        );
    }

    #[test]
    fn one_comma_is_a_single_person_written_family_first() {
        let paper = Paper::from_text("Author: Smith, Jane");
        assert_eq!(paper.authors.len(), 1);
        assert_eq!(paper.authors[0].family, "Smith");
        assert_eq!(paper.authors[0].given.as_deref(), Some("Jane"));
    }

    #[test]
    fn semicolons_separate_people_unambiguously() {
        let paper = Paper::from_text("Authors: Smith, J.; Doe, A.; Roe, R.");
        assert_eq!(paper.authors.len(), 3);
        assert_eq!(paper.authors[2].family, "Roe");
    }

    #[test]
    fn an_abstract_runs_to_the_blank_line() {
        let paper = Paper::from_text(
            "Abstract: We show that\n\
             caching is useful.\n\
             \n\
             1. Introduction",
        );
        assert_eq!(
            paper.abstract_text.as_deref(),
            Some("We show that caching is useful.")
        );
    }

    #[test]
    fn an_abstract_stops_at_the_next_label() {
        let paper = Paper::from_text("Abstract: Short.\nYear: 2001");
        assert_eq!(paper.abstract_text.as_deref(), Some("Short."));
        assert_eq!(paper.year, Some(2001));
    }

    #[test]
    fn the_first_occurrence_of_a_label_wins() {
        let paper = Paper::from_text("Title: First\nTitle: Second");
        assert_eq!(paper.title.as_deref(), Some("First"));
    }

    #[test]
    fn a_header_doi_beats_one_found_in_the_body() {
        // A DOI in the body may belong to something this paper cites.
        let paper = Paper::from_text("DOI: 10.1000/mine\n\nWe build on 10.2000/theirs.");
        assert_eq!(paper.doi.as_ref().map(Doi::as_str), Some("10.1000/mine"));
    }

    #[test]
    fn a_body_identifier_is_used_when_no_header_states_one() {
        let paper = Paper::from_text("Some notes about arXiv:2401.01234 and friends.");
        assert_eq!(
            paper.arxiv.as_ref().map(ArxivId::to_text),
            Some("2401.01234".to_owned())
        );
    }

    #[test]
    fn prose_with_a_colon_is_not_a_header_field() {
        let paper = Paper::from_text("The whole point is: research notes are documents too.");
        assert!(paper.is_empty(), "a sentence is not a labelled field");
    }

    #[test]
    fn a_document_that_says_nothing_about_itself_yields_an_empty_paper() {
        assert!(Paper::from_text("").is_empty());
        assert!(Paper::from_text("Just some prose.\n\nMore prose.").is_empty());
    }

    #[test]
    fn a_title_is_never_guessed_from_the_first_line() {
        // bp-semantic suggests titles; asserting a published title is a
        // different and much stronger claim.
        let paper = Paper::from_text("An Empirical Study of Caching\n\nBy someone.");
        assert_eq!(paper.title, None);
    }

    #[test]
    fn a_paper_from_an_entry_takes_the_fields_it_is_given() {
        let bibliography =
            crate::bibtex::parse("@article{k, title = {The {DNA} Study}, author = {Doe, Jane}, journal = {Nature}, year = 1998, doi = {10.1000/x}}")
                .unwrap();
        let paper = Paper::from_entry(&bibliography.entries[0]);
        assert_eq!(paper.title.as_deref(), Some("The DNA Study"));
        assert_eq!(paper.venue.as_deref(), Some("Nature"));
        assert_eq!(paper.year, Some(1998));
        assert_eq!(paper.doi.as_ref().map(Doi::as_str), Some("10.1000/x"));
    }

    proptest! {
        /// Parsing a DOI is a fixed point: the canonical form parses to itself.
        #[test]
        fn a_doi_canonicalises_once(
            registrant in "[0-9]{4,9}",
            suffix in r"[A-Za-z0-9./()_-]{1,20}",
        ) {
            let text = format!("10.{registrant}/{suffix}");
            let Ok(doi) = Doi::parse(&text) else { return Ok(()) };
            prop_assert_eq!(Doi::parse(doi.as_str()).unwrap(), doi);
        }

        /// A DOI dropped into prose is recovered exactly, wherever it lands.
        #[test]
        fn a_doi_in_prose_is_recovered_intact(
            registrant in "[0-9]{4,7}",
            suffix in "[a-z0-9]{1,12}",
            before in "[a-z ]{0,10}",
        ) {
            let doi = format!("10.{registrant}/{suffix}");
            // A space between, because an identifier never starts inside a
            // word -- which is its own test, above.
            let text = format!("{before} {doi}, and more.");
            let found = find_identifiers(&text);
            prop_assert_eq!(found.len(), 1);
            prop_assert_eq!(&found[0].value, &Identifier::Doi(Doi::parse(&doi).unwrap()));
        }

        /// An arXiv identifier survives being written and read again.
        #[test]
        fn an_arxiv_identifier_round_trips(
            yymm in "[0-9]{4}",
            number in "[0-9]{4,5}",
            version in proptest::option::of(1u32..30),
        ) {
            let id = ArxivId { id: format!("{yymm}.{number}"), version };
            prop_assert_eq!(ArxivId::parse(&id.to_text()).unwrap(), id);
        }

        /// Scanning arbitrary text never panics and never reports a position
        /// outside the document.
        #[test]
        fn scanning_arbitrary_text_is_safe(text in r"[a-zA-Z0-9 .:/@\n-]{0,120}") {
            let line_count = text.lines().count().max(1);
            for found in find_identifiers(&text) {
                prop_assert!(found.line >= 1 && found.line <= line_count);
                prop_assert!(found.column >= 1);
                prop_assert!(found.length >= 1);
            }
            let _ = Paper::from_text(&text);
        }
    }
}
