//! The citation record, the person names in it, and the view styles render.
//!
//! Two types, and the split between them is the whole design.
//!
//! [`Entry`] is the **record of truth**: a type, a key, and a map of fields
//! exactly as they were written. It knows nothing about what a field means,
//! which is why nothing can be lost in it -- an `@dataset` entry with a
//! `langid` field survives a parse and an emit unchanged even though this
//! build has never heard of either. specs.md section 7 lists Research Note as
//! a format profile; a format profile that silently rewrites the user's
//! bibliography is not one.
//!
//! [`Reference`] is a **lossy view for rendering**, built from an `Entry` by
//! deciding what `journal` versus `booktitle` versus `journaltitle` means and
//! by stripping the brace protection that belongs to BibTeX rather than to
//! the reader. Losing things is its job. It is safe only because the `Entry`
//! it came from is still there, holding everything.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::bibtex::strip_protection;

/// What kind of thing is being cited.
///
/// `Other` is not a fallback, it is the point: BibTeX's type name is an open
/// vocabulary that biblatex, Zotero and every journal's export template have
/// each extended. Mapping an unrecognised `@dataset` onto `Misc` would throw
/// away the one word that said what the thing was, and the user would never
/// be told. Construct through [`EntryKind::parse`] so the canonical spelling
/// is decided in one place.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum EntryKind {
    Article,
    Book,
    Booklet,
    Conference,
    InBook,
    InCollection,
    InProceedings,
    Manual,
    MastersThesis,
    Misc,
    PhdThesis,
    Proceedings,
    TechReport,
    Unpublished,
    /// A type this build does not know, kept verbatim in lower case.
    Other(String),
}

impl EntryKind {
    /// Read a type name, case-insensitively, the way BibTeX does.
    ///
    /// `@ARTICLE`, `@Article` and `@article` are one type; treating them as
    /// three would split a bibliography into duplicates that only differ by
    /// how the exporting tool felt about capitals.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let lower = text.trim().to_ascii_lowercase();
        match lower.as_str() {
            "article" => Self::Article,
            "book" => Self::Book,
            "booklet" => Self::Booklet,
            "conference" => Self::Conference,
            "inbook" => Self::InBook,
            "incollection" => Self::InCollection,
            "inproceedings" => Self::InProceedings,
            "manual" => Self::Manual,
            "mastersthesis" => Self::MastersThesis,
            "misc" => Self::Misc,
            "phdthesis" => Self::PhdThesis,
            "proceedings" => Self::Proceedings,
            "techreport" => Self::TechReport,
            "unpublished" => Self::Unpublished,
            _ => Self::Other(lower),
        }
    }

    /// The canonical lower-case spelling, which is what gets emitted.
    ///
    /// Lower case rather than the file's original case because a round trip
    /// has to converge: normalising once on the way in means emitting and
    /// reparsing is the identity from then on.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Article => "article",
            Self::Book => "book",
            Self::Booklet => "booklet",
            Self::Conference => "conference",
            Self::InBook => "inbook",
            Self::InCollection => "incollection",
            Self::InProceedings => "inproceedings",
            Self::Manual => "manual",
            Self::MastersThesis => "mastersthesis",
            Self::Misc => "misc",
            Self::PhdThesis => "phdthesis",
            Self::Proceedings => "proceedings",
            Self::TechReport => "techreport",
            Self::Unpublished => "unpublished",
            Self::Other(name) => name,
        }
    }

    /// Whether this kind normally appears inside a larger publication.
    ///
    /// Styles need to know: an article's container is italicised and a book's
    /// title is, and getting that backwards is the most visible way a
    /// bibliography looks wrong.
    #[must_use]
    pub fn is_part_of_a_container(&self) -> bool {
        matches!(
            self,
            Self::Article
                | Self::Conference
                | Self::InBook
                | Self::InCollection
                | Self::InProceedings
        )
    }
}

/// One bibliographic record, exactly as written.
///
/// Field names are lower-cased on the way in because BibTeX matches them
/// case-insensitively; `Author` and `author` in one entry are the same field,
/// and a model that kept both would let one of them be silently ignored by
/// whatever consumed it. A `BTreeMap` rather than a `Vec` of pairs so that
/// emission has one order, always -- a bibliography that reorders itself
/// between saves is a diff nobody can read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub kind: EntryKind,
    /// The citation key: what `\cite{...}` names. Never lower-cased -- keys
    /// are case-sensitive in every tool that consumes them.
    pub key: String,
    pub fields: BTreeMap<String, String>,
}

impl Entry {
    /// An entry with a type and a key and nothing else.
    ///
    /// `@misc{key}` with no fields is legal BibTeX, so the empty entry has to
    /// be constructible or the model would be narrower than the format.
    #[must_use]
    pub fn new(kind: EntryKind, key: impl Into<String>) -> Self {
        Self {
            kind,
            key: key.into(),
            fields: BTreeMap::new(),
        }
    }

    /// A field's raw value, looked up case-insensitively.
    ///
    /// Raw: brace protection intact. Callers that want to show it to a human
    /// want [`crate::bibtex::strip_protection`] first, and callers writing it
    /// back to a `.bib` want it exactly as it is.
    #[must_use]
    pub fn field(&self, name: &str) -> Option<&str> {
        self.fields
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }

    /// Set a field, normalising the name the way the parser does.
    ///
    /// Exists so callers cannot get a differently-cased duplicate into
    /// `fields` by inserting directly and defeat the guarantee above.
    pub fn set_field(&mut self, name: &str, value: impl Into<String>) -> Option<String> {
        self.fields.insert(name.to_ascii_lowercase(), value.into())
    }

    /// The `author` field, split into people.
    ///
    /// Empty when there is no author field, which is different from a field
    /// that failed to parse -- names cannot fail to parse, only to be
    /// present. See [`Name::parse_list`].
    #[must_use]
    pub fn authors(&self) -> Vec<Name> {
        self.field("author")
            .map(Name::parse_list)
            .unwrap_or_default()
    }

    /// The `editor` field, split into people. Same contract as
    /// [`Entry::authors`]; separate because a style prints them differently.
    #[must_use]
    pub fn editors(&self) -> Vec<Name> {
        self.field("editor")
            .map(Name::parse_list)
            .unwrap_or_default()
    }

    /// A four-digit year from `year`, or failing that from a biblatex `date`.
    ///
    /// `None` for `year = {in press}` or `{forthcoming}`: those are real
    /// values a style should print verbatim, and inventing a number for them
    /// would misdate the work.
    #[must_use]
    pub fn year(&self) -> Option<i32> {
        self.field("year")
            .and_then(first_four_digit_year)
            .or_else(|| self.field("date").and_then(first_four_digit_year))
    }
}

/// The first standalone four-digit number in `text`, if it could be a year.
///
/// "Standalone" matters: `10.1234/xyz` contains `1234` and is a DOI prefix,
/// not a date. Requiring the run to be exactly four digits long stops the
/// scan reading a year out of the middle of a longer number.
pub(crate) fn first_four_digit_year(text: &str) -> Option<i32> {
    let bytes = text.as_bytes();
    let mut start = 0usize;
    while start < bytes.len() {
        if !bytes[start].is_ascii_digit() {
            start += 1;
            continue;
        }
        let mut end = start;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
        if end - start == 4 {
            // Unwrap is total: four ASCII digits always parse, and 9999 fits.
            let year: i32 = text[start..end].parse().ok()?;
            if (1000..=9999).contains(&year) {
                return Some(year);
            }
        }
        start = end;
    }
    None
}

/// One person, split the way BibTeX splits names.
///
/// Four parts because a bibliography style needs them separately: APA wants
/// `Family, G.`, MLA wants `Family, Given`, and both want "de la Cruz" to
/// alphabetise under C while printing the particle. A single display string
/// cannot answer any of those questions, and a style that guessed would sort
/// half a bibliography under D.
///
/// Only `family` is required, because it is the only part every name has --
/// an organisation, a mononym and a pen name all have one and nothing else.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Name {
    /// Given names, in the order written. `None` rather than `""` so a caller
    /// cannot render "Smith, ." for a mononym.
    pub given: Option<String>,
    /// The "von" part: `de`, `van der`, `ó`. Kept apart from the family name
    /// because styles disagree about whether it sorts and whether it
    /// capitalises.
    pub particle: Option<String>,
    /// The family name, or the whole name for anything that does not split.
    pub family: String,
    /// `Jr`, `III`, `Sr`. Never merged into `given`: it follows the family
    /// name, not the given one.
    pub suffix: Option<String>,
}

impl Name {
    /// Read one name in any of BibTeX's three written forms.
    ///
    /// `First von Last`, `von Last, First`, and `von Last, Jr, First`. Which
    /// one it is, is decided by counting top-level commas -- exactly as
    /// BibTeX does, because a name file written for BibTeX has to mean here
    /// what it means there.
    ///
    /// `None` for blank input. An empty string is not a person, and returning
    /// a `Name` with an empty family would put a stray comma in every
    /// bibliography that had a trailing `and`.
    ///
    /// Expects brace-balanced text, which is what [`crate::bibtex::parse`]
    /// produces. An unbalanced brace is kept verbatim in the family name
    /// rather than repaired --- there is no honest guess at what the writer
    /// meant --- so such a name does not survive [`Name::to_bibtex`]
    /// unchanged.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }

        // A name wrapped entirely in braces is a literal: `{World Health
        // Organization}` is one organisation, not a Ms. Organization with two
        // given names. This is the escape hatch BibTeX gives for names its
        // rules would mangle, and honouring it is what makes the rules safe.
        if let Some(inner) = fully_braced(text) {
            let inner = inner.trim();
            return (!inner.is_empty()).then(|| Self {
                given: None,
                particle: None,
                family: inner.to_owned(),
                suffix: None,
            });
        }

        let parts = split_top_level(text, b',');
        let name = match parts.len() {
            1 => Self::from_first_von_last(parts[0]),
            2 => {
                let mut name = Self::from_von_last(parts[0])?;
                name.given = optional_part(parts[1]);
                name
            }
            // Three or more: BibTeX takes the first three and complains about
            // the rest. Joining the tail into the given names loses nothing
            // and keeps every character the user typed.
            _ => {
                let mut name = Self::from_von_last(parts[0])?;
                name.suffix = optional_part(parts[1]);
                name.given = optional_part(&parts[2..].join(", "));
                name
            }
        };

        // A family name is the one part every person has, so text that
        // produces none of it -- `A {}`, a line of punctuation -- named
        // nobody. Returning a `Name` with an empty family would put a bare
        // comma in the bibliography where a person should be.
        (!name.family.trim().is_empty()).then_some(name)
    }

    /// Split an `author` or `editor` field into people.
    ///
    /// Splits on the word `and` between spaces, at brace depth zero, which is
    /// BibTeX's separator. Depth matters: `{Smith and Sons}` is a publisher
    /// with a conjunction in its name, and splitting it would invent a second
    /// author called Sons.
    #[must_use]
    pub fn parse_list(text: &str) -> Vec<Self> {
        split_on_and(text)
            .iter()
            .filter_map(|p| Self::parse(p))
            .collect()
    }

    /// Write the name back in the `von Last, Jr, First` form.
    ///
    /// That form rather than `First von Last` because it is the only one that
    /// is unambiguous: it says where the family name starts instead of asking
    /// the reader to infer it from capitalisation. Parsing the result returns
    /// this exact `Name`, which is the property the round-trip test checks.
    ///
    /// A part containing a comma, or the word `and`, is wrapped in braces --
    /// otherwise it would re-read as a second name or a suffix.
    #[must_use]
    pub fn to_bibtex(&self) -> String {
        // Nothing but a family name: emit it braced, so a family name that
        // happens to contain spaces or a lower-case first word ("de Gaulle"
        // as a whole) is not re-split into parts it never had.
        if self.given.is_none() && self.particle.is_none() && self.suffix.is_none() {
            return format!("{{{}}}", self.family);
        }

        let mut left = String::new();
        if let Some(particle) = &self.particle {
            left.push_str(particle);
            left.push(' ');
        }
        left.push_str(&brace_family(&self.family));

        let given = self.given.as_deref().unwrap_or("");
        match &self.suffix {
            Some(suffix) => format!("{left}, {}, {}", brace_part(suffix), brace_part(given)),
            None => format!("{left}, {}", brace_part(given)),
        }
    }

    /// `Given particle Family, Suffix` -- the way a name is written in prose.
    ///
    /// `initials` for the same reason as [`Name::display_family_first`]: IEEE
    /// writes `J. Q. Smith` and MLA writes `Jane Q. Smith`, and the only
    /// difference is this flag.
    #[must_use]
    pub fn display_given_first(&self, initials: bool) -> String {
        let mut out = String::new();
        let given = match (&self.given, initials) {
            (Some(given), true) => Some(initialise(given)),
            (Some(given), false) => Some(given.clone()),
            (None, _) => None,
        };
        push_part(&mut out, given.as_deref());
        push_part(&mut out, self.particle.as_deref());
        push_part(&mut out, Some(&self.family));
        if let Some(suffix) = &self.suffix {
            out.push_str(", ");
            out.push_str(suffix);
        }
        out
    }

    /// `particle Family, Given, Suffix` -- the way a name is written in a
    /// bibliography, where it has to alphabetise.
    ///
    /// `initials` because styles split on it and nothing else: APA and IEEE
    /// abbreviate given names, MLA and Chicago spell them out. One flag here
    /// beats four nearly identical renderers.
    #[must_use]
    pub fn display_family_first(&self, initials: bool) -> String {
        let mut out = String::new();
        push_part(&mut out, self.particle.as_deref());
        push_part(&mut out, Some(&self.family));

        let given = match (&self.given, initials) {
            (Some(given), true) => Some(initialise(given)),
            (Some(given), false) => Some(given.clone()),
            (None, _) => None,
        };
        if let Some(given) = given {
            out.push_str(", ");
            out.push_str(&given);
        }
        if let Some(suffix) = &self.suffix {
            out.push_str(", ");
            out.push_str(suffix);
        }
        out
    }

    /// What a reader sees in an in-text citation and what the bibliography
    /// sorts on: the particle and family name, nothing else.
    #[must_use]
    pub fn sort_key(&self) -> String {
        let mut out = String::new();
        push_part(&mut out, self.particle.as_deref());
        push_part(&mut out, Some(&self.family));
        out
    }

    /// `First von Last`: no commas at all, so every token is a candidate and
    /// the case of each one is the only evidence about where the family name
    /// begins.
    fn from_first_von_last(text: &str) -> Self {
        let tokens = split_whitespace_top_level(text);
        if tokens.len() <= 1 {
            return Self {
                given: None,
                particle: None,
                family: unwrap_family(&tokens.join(" ")),
                suffix: None,
            };
        }

        // The last token is always part of the family name, so a lower-case
        // token in final position is a surname ("cummings"), not a particle.
        let lowercase: Vec<usize> = (0..tokens.len() - 1)
            .filter(|i| begins_lowercase(tokens[*i]))
            .collect();

        let (given, particle, family) = match (lowercase.first(), lowercase.last()) {
            (Some(&first), Some(&last)) => (
                &tokens[..first],
                Some(tokens[first..=last].join(" ")),
                tokens[last + 1..].join(" "),
            ),
            _ => (
                &tokens[..tokens.len() - 1],
                None,
                tokens[tokens.len() - 1].to_owned(),
            ),
        };

        Self {
            given: optional_part(&given.join(" ")),
            particle,
            family: unwrap_family(&family),
            suffix: None,
        }
    }

    /// `von Last` -- the part before the first comma.
    ///
    /// The particle runs from the start to the *last* lower-case token before
    /// the final one, not merely over a leading run of them. That is BibTeX's
    /// own rule, and matching it is what makes `Ludwig van der {Y} Berg` split
    /// the same way whether it was written with a comma or without: two rules
    /// that disagreed would make writing a name back out change who it was.
    fn from_von_last(text: &str) -> Option<Self> {
        let text = text.trim();
        if let Some(inner) = fully_braced(text) {
            let inner = inner.trim();
            return (!inner.is_empty()).then(|| Self {
                given: None,
                particle: None,
                family: inner.to_owned(),
                suffix: None,
            });
        }

        let tokens = split_whitespace_top_level(text);
        if tokens.is_empty() {
            return None;
        }
        // Stop one short of the end: a name that is nothing but lower-case
        // words still has a family name, and it is the last word.
        let particle_len = (0..tokens.len() - 1)
            .rfind(|i| begins_lowercase(tokens[*i]))
            .map_or(0, |last| last + 1);

        Some(Self {
            given: None,
            particle: (particle_len > 0).then(|| tokens[..particle_len].join(" ")),
            family: unwrap_family(&tokens[particle_len..].join(" ")),
            suffix: None,
        })
    }
}

/// Append a space-separated part, without a leading space on an empty string.
fn push_part(out: &mut String, part: Option<&str>) {
    let Some(part) = part else { return };
    if part.is_empty() {
        return;
    }
    if !out.is_empty() {
        out.push(' ');
    }
    out.push_str(part);
}

/// `Ada Byron` becomes `A. B.`
///
/// Splits on spaces and hyphens and keeps the hyphen, because `Jean-Luc`
/// abbreviates to `J.-L.` in every style that abbreviates at all, and `J.` on
/// its own is a different person.
fn initialise(given: &str) -> String {
    let mut out = String::new();
    for (index, word) in given.split_inclusive(['-', ' ']).enumerate() {
        let separator = word.chars().next_back().filter(|c| *c == '-' || *c == ' ');
        let core = word.trim_end_matches(['-', ' ']);
        // A "word" of pure punctuation has no initial; skipping it is what
        // keeps `J. , .` out of a rendered bibliography.
        let Some(first) = core.chars().find(|c| c.is_alphanumeric()) else {
            continue;
        };
        if index > 0 && !out.ends_with('-') {
            out.push(' ');
        }
        out.push(first);
        out.push('.');
        if separator == Some('-') {
            out.push('-');
        }
    }
    out
}

/// `Some(inner)` when `text` is exactly one brace group and nothing else.
fn fully_braced(text: &str) -> Option<&str> {
    let bytes = text.as_bytes();
    if bytes.first() != Some(&b'{') || bytes.last() != Some(&b'}') || bytes.len() < 2 {
        return None;
    }
    let mut depth = 0usize;
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 1,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                // Closed before the end: two groups side by side, not one.
                if depth == 0 && index + 1 != bytes.len() {
                    return None;
                }
            }
            _ => {}
        }
        index += 1;
    }
    (depth == 0).then(|| &text[1..text.len() - 1])
}

/// Drop the braces around a family name that was written as one group.
///
/// `Jean {de la Fontaine}` means "all three words are the family name"; the
/// braces did their job during splitting and are noise afterwards.
fn unwrap_family(text: &str) -> String {
    match fully_braced(text) {
        Some(inner) => inner.trim().to_owned(),
        None => text.to_owned(),
    }
}

/// `None` for a part that names nobody, and unwrap a part that is entirely
/// braced -- the braces were put there by [`Name::to_bibtex`] to protect a
/// comma, and taking them off is what closes the round trip.
///
/// A part with no letter or digit in it is discarded rather than kept: `, ,`
/// left over from a line of stray commas is punctuation, not a given name,
/// and rendering it would print `Smith, ., .`
fn optional_part(text: &str) -> Option<String> {
    let text = text.trim();
    let text = fully_braced(text).map_or(text, str::trim);
    (!text.is_empty() && text.chars().any(char::is_alphanumeric)).then(|| text.to_owned())
}

/// Wrap a given name or a suffix whose punctuation would re-read as structure.
fn brace_part(part: &str) -> String {
    let ambiguous =
        part.contains(',') || split_on_and(part).len() > 1 || fully_braced(part).is_some();
    if ambiguous {
        format!("{{{part}}}")
    } else {
        part.to_owned()
    }
}

/// The same for a family name, which also has to survive being re-split.
///
/// Any whitespace at all is enough: `Wang Wei` written back bare would be read
/// as a given name and a family name, and `de la Cruz` as a particle and a
/// family name. The braces say "all of this is the family name", which is
/// what it already was.
fn brace_family(part: &str) -> String {
    if part.contains(char::is_whitespace) {
        format!("{{{part}}}")
    } else {
        brace_part(part)
    }
}

/// Whether a name token's first meaningful character is lower case.
///
/// A token opening with `{` or `\` counts as upper case, following BibTeX:
/// both are the user asserting control over the token, and the point of
/// asserting it is to stop the von-detection guessing.
fn begins_lowercase(token: &str) -> bool {
    match token.chars().next() {
        Some('{' | '\\') => false,
        Some(c) => c.is_lowercase(),
        None => false,
    }
}

/// Split on a separator byte that is outside every brace group.
fn split_top_level(text: &str, separator: u8) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'\\' {
            index += escaped_pair_len(text, index);
            continue;
        }
        match bytes[index] {
            b'{' => depth += 1,
            b'}' => depth = depth.saturating_sub(1),
            b if b == separator && depth == 0 => {
                parts.push(&text[start..index]);
                start = index + 1;
            }
            _ => {}
        }
        index += 1;
    }
    parts.push(&text[start.min(text.len())..]);
    parts
}

/// How far past a backslash the character it suspends ends.
///
/// A backslash takes the *character* after it, not the byte: skipping two
/// bytes would leave the cursor inside a multi-byte character, and the next
/// slice taken from it would panic. `\` at the very end suspends nothing.
fn escaped_pair_len(text: &str, backslash: usize) -> usize {
    let after = backslash + 1;
    match text.get(after..).and_then(|rest| rest.chars().next()) {
        Some(c) => 1 + c.len_utf8(),
        None => 1,
    }
}

/// Split on whitespace outside every brace group.
fn split_whitespace_top_level(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = None;
    let mut index = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'\\' {
            // Both the backslash and what it suspends belong to whichever
            // token is open, so an escaped space never splits a name.
            start.get_or_insert(index);
            index += escaped_pair_len(text, index);
            continue;
        }
        match byte {
            b'{' => depth += 1,
            b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
        if depth == 0 && byte.is_ascii_whitespace() {
            if let Some(begin) = start.take() {
                parts.push(&text[begin..index]);
            }
        } else {
            start.get_or_insert(index);
        }
        index += 1;
    }
    if let Some(begin) = start {
        parts.push(&text[begin..]);
    }
    parts
}

/// Split a name list on the separator word `and`, at brace depth zero.
fn split_on_and(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => {
                index += escaped_pair_len(text, index);
                continue;
            }
            b'{' => depth += 1,
            b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
        // The separator is the whole word, whitespace on both sides. `Anderson`
        // and `Rand` both contain `and`, and neither is a separator.
        if depth == 0
            && bytes[index].is_ascii_whitespace()
            && text[index..].trim_start().starts_with("and")
        {
            let after_space = index + (text[index..].len() - text[index..].trim_start().len());
            let after_and = after_space + 3;
            let ends_word = text
                .as_bytes()
                .get(after_and)
                .is_none_or(u8::is_ascii_whitespace);
            if ends_word {
                parts.push(&text[start..index]);
                start = after_and;
                index = after_and;
                continue;
            }
        }
        index += 1;
    }
    parts.push(&text[start.min(text.len())..]);
    parts
}

/// What a style renders: one publication, with the field names already
/// decided and the BibTeX punctuation already gone.
///
/// Deliberately lossy. Every `Option` here is a question a style asks; a
/// field no style asks about is not represented, and the [`Entry`] still has
/// it. Building this is the step where `journal`, `journaltitle` and
/// `booktitle` stop being three names for one idea.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reference {
    /// The citation key, so a rendered entry can be traced back.
    pub key: String,
    pub kind: EntryKind,
    pub authors: Vec<Name>,
    pub editors: Vec<Name>,
    pub title: Option<String>,
    /// The journal, book or proceedings this appeared in.
    pub container: Option<String>,
    /// Publisher, institution, school or organisation -- whichever the entry
    /// had. Styles print one "who released this" slot, so the model has one.
    pub publisher: Option<String>,
    pub year: Option<i32>,
    pub volume: Option<String>,
    pub issue: Option<String>,
    /// Page range, verbatim. Not split into first and last: `e0234567` and
    /// `1--12, 15` are both real, and normalising them would print a page
    /// range the article does not have.
    pub pages: Option<String>,
    /// The DOI as text. Normalised when it is recognisable and kept verbatim
    /// when it is not, because a reader can act on a malformed DOI and cannot
    /// act on one that was quietly deleted.
    pub doi: Option<String>,
    pub url: Option<String>,
    pub note: Option<String>,
}

impl Reference {
    /// Decide what an entry's fields mean.
    ///
    /// The order inside each `or_else` chain is the decision: `journal` beats
    /// `journaltitle` beats `booktitle` because an entry carrying two of them
    /// is a converted file where the first is the one the converter wrote.
    #[must_use]
    pub fn from_entry(entry: &Entry) -> Self {
        let text = |name: &str| {
            entry
                .field(name)
                .map(strip_protection)
                .filter(|v| !v.is_empty())
        };
        let any = |names: &[&str]| names.iter().find_map(|n| text(n));

        Self {
            key: entry.key.clone(),
            kind: entry.kind.clone(),
            authors: entry.authors(),
            editors: entry.editors(),
            title: text("title"),
            container: any(&["journal", "journaltitle", "booktitle", "series"]),
            publisher: any(&["publisher", "institution", "school", "organization"]),
            year: entry.year(),
            volume: text("volume"),
            issue: any(&["number", "issue"]),
            pages: text("pages").map(|p| normalise_dashes(&p)),
            doi: text("doi")
                .map(|d| crate::paper::Doi::parse(&d).map_or(d, |doi| doi.into_string())),
            url: text("url"),
            note: text("note"),
        }
    }

    /// Whoever this work should be attributed to: the authors, or the editors
    /// when there are no authors (an edited volume has no other answer).
    #[must_use]
    pub fn attributed_to(&self) -> &[Name] {
        if self.authors.is_empty() {
            &self.editors
        } else {
            &self.authors
        }
    }

    /// What a bibliography sorts this under.
    ///
    /// Family name, then year, then title -- the conventional order, and
    /// total, so sorting is stable rather than dependent on the input order.
    #[must_use]
    pub fn sort_key(&self) -> (String, i32, String) {
        let people = self.attributed_to();
        let author = people.first().map(Name::sort_key).unwrap_or_default();
        (
            author.to_lowercase(),
            self.year.unwrap_or(i32::MAX),
            self.title.clone().unwrap_or_default().to_lowercase(),
        )
    }
}

/// `1--12` becomes `1-12`.
///
/// LaTeX writes an en dash as two hyphens; a reader outside LaTeX sees a typo.
/// Only runs of hyphens are touched, so a page number containing one is safe.
fn normalise_dashes(pages: &str) -> String {
    let mut out = String::with_capacity(pages.len());
    let mut previous_hyphen = false;
    for c in pages.chars() {
        if c == '-' {
            if !previous_hyphen {
                out.push('-');
            }
            previous_hyphen = true;
        } else {
            out.push(c);
            previous_hyphen = false;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn an_unknown_entry_type_is_kept_rather_than_flattened() {
        // The failure mode this guards: @dataset silently becoming @misc.
        assert_eq!(
            EntryKind::parse("dataset"),
            EntryKind::Other("dataset".to_owned())
        );
        assert_eq!(EntryKind::parse("DataSet").as_str(), "dataset");
    }

    #[test]
    fn entry_types_are_case_insensitive() {
        assert_eq!(EntryKind::parse("ARTICLE"), EntryKind::Article);
        assert_eq!(EntryKind::parse("  Article "), EntryKind::Article);
    }

    #[test]
    fn field_names_are_case_insensitive() {
        let mut entry = Entry::new(EntryKind::Article, "k");
        entry.set_field("Author", "Smith, J.");
        assert_eq!(entry.field("AUTHOR"), Some("Smith, J."));
        assert_eq!(entry.fields.len(), 1, "one field, not two spellings of one");
    }

    #[test]
    fn a_year_that_is_not_a_number_is_not_invented() {
        let mut entry = Entry::new(EntryKind::Article, "k");
        entry.set_field("year", "in press");
        assert_eq!(entry.year(), None);

        entry.set_field("year", "2019");
        assert_eq!(entry.year(), Some(2019));
    }

    #[test]
    fn a_year_comes_from_a_biblatex_date_when_there_is_no_year() {
        let mut entry = Entry::new(EntryKind::Article, "k");
        entry.set_field("date", "2021-05-04");
        assert_eq!(entry.year(), Some(2021));
    }

    #[test]
    fn a_long_number_is_not_read_as_a_year() {
        let mut entry = Entry::new(EntryKind::Article, "k");
        entry.set_field("year", "20211");
        assert_eq!(entry.year(), None, "five digits is not a year");
    }

    #[test]
    fn the_three_written_forms_of_a_name_agree() {
        let plain = Name::parse("Donald E. Knuth").unwrap();
        let comma = Name::parse("Knuth, Donald E.").unwrap();
        assert_eq!(plain, comma);
        assert_eq!(plain.family, "Knuth");
        assert_eq!(plain.given.as_deref(), Some("Donald E."));
    }

    #[test]
    fn a_particle_is_found_by_its_lower_case() {
        let name = Name::parse("Ludwig van Beethoven").unwrap();
        assert_eq!(name.given.as_deref(), Some("Ludwig"));
        assert_eq!(name.particle.as_deref(), Some("van"));
        assert_eq!(name.family, "Beethoven");

        let long = Name::parse("Vincent van der Berg").unwrap();
        assert_eq!(long.particle.as_deref(), Some("van der"));
        assert_eq!(long.family, "Berg");
    }

    #[test]
    fn the_particle_runs_to_the_last_lower_case_word_either_way_round() {
        // The two written forms have to agree, or writing a name back out
        // would change who it was. BibTeX's rule is "up to the last lower-case
        // token", not "the leading run of them".
        let plain = Name::parse("Piet van {Q} der Berg").unwrap();
        let comma = Name::parse("van {Q} der Berg, Piet").unwrap();
        assert_eq!(plain, comma);
        assert_eq!(plain.particle.as_deref(), Some("van {Q} der"));
        assert_eq!(plain.family, "Berg");
    }

    #[test]
    fn a_lower_case_surname_is_not_mistaken_for_a_particle() {
        // The last token is always family, whatever its case.
        let name = Name::parse("e e cummings").unwrap();
        assert_eq!(name.family, "cummings");
        assert_eq!(name.particle.as_deref(), Some("e e"));
    }

    #[test]
    fn a_suffix_is_the_middle_of_three_parts() {
        let name = Name::parse("von Neumann, Jr, John").unwrap();
        assert_eq!(name.particle.as_deref(), Some("von"));
        assert_eq!(name.family, "Neumann");
        assert_eq!(name.suffix.as_deref(), Some("Jr"));
        assert_eq!(name.given.as_deref(), Some("John"));
    }

    #[test]
    fn a_braced_name_is_one_organisation() {
        let name = Name::parse("{World Health Organization}").unwrap();
        assert_eq!(name.family, "World Health Organization");
        assert_eq!(name.given, None);
        assert_eq!(name.particle, None, "\"Health\" is not a particle here");
    }

    #[test]
    fn braces_hold_a_multi_word_family_name_together() {
        let name = Name::parse("Jean {de la Fontaine}").unwrap();
        assert_eq!(name.given.as_deref(), Some("Jean"));
        assert_eq!(name.family, "de la Fontaine");
        assert_eq!(name.particle, None, "the braces overrode von-detection");
    }

    #[test]
    fn text_with_no_family_name_in_it_names_nobody() {
        // The type allows a Name with an empty family; the domain does not.
        assert_eq!(Name::parse("A {}"), None);
        assert_eq!(Name::parse(", , ,"), None);
        assert_eq!(Name::parse("Smith, , ,").unwrap().given, None);
    }

    #[test]
    fn a_family_name_with_a_space_in_it_is_written_back_protected() {
        // Bare, `Wang Wei` would read as a given name and a family name.
        let name = Name::parse("{Wang Wei}, Q.").unwrap();
        assert_eq!(name.family, "Wang Wei");
        assert_eq!(name.to_bibtex(), "{Wang Wei}, Q.");
        assert_eq!(Name::parse(&name.to_bibtex()).unwrap(), name);
    }

    #[test]
    fn an_unbalanced_brace_is_kept_rather_than_repaired() {
        // There is no honest guess at what was meant, so the text survives as
        // written and the name simply does not split.
        let name = Name::parse("Smith}").unwrap();
        assert_eq!(name.family, "Smith}");
        assert_eq!(name.given, None);
    }

    #[test]
    fn a_lone_backslash_suspends_nothing_and_splits_nothing() {
        // A backslash takes the character after it, not the byte; two bytes
        // would land the cursor inside a multi-byte character.
        assert_eq!(Name::parse("Smith\\").unwrap().family, "Smith\\");
        let escaped = Name::parse("Jean\\ Luc Picard").unwrap();
        assert_eq!(
            escaped.given.as_deref(),
            Some("Jean\\ Luc"),
            "an escaped space does not separate two names"
        );
        assert_eq!(Name::parse("\\日 Smith").unwrap().family, "Smith");
    }

    #[test]
    fn an_empty_name_is_not_a_person() {
        assert_eq!(Name::parse(""), None);
        assert_eq!(Name::parse("   \t "), None);
        assert_eq!(Name::parse("{}"), None);
    }

    #[test]
    fn a_name_list_splits_on_the_word_and_only() {
        let names = Name::parse_list("Smith, John and Anderson, Kim and Rand, Ayn");
        assert_eq!(names.len(), 3);
        assert_eq!(names[1].family, "Anderson", "\"and\" inside a word");
        assert_eq!(names[2].family, "Rand", "\"and\" at the end of a word");
    }

    #[test]
    fn a_conjunction_inside_braces_does_not_split_a_name() {
        let names = Name::parse_list("{Smith and Sons} and Doe, Jane");
        assert_eq!(names.len(), 2, "the braced conjunction is part of a name");
        assert_eq!(names[0].family, "Smith and Sons");
    }

    #[test]
    fn an_empty_author_field_yields_no_authors() {
        assert!(Name::parse_list("").is_empty());
        assert!(Name::parse_list("   and   ").is_empty());
    }

    #[test]
    fn given_names_abbreviate_to_initials() {
        let name = Name::parse("Knuth, Donald Ervin").unwrap();
        assert_eq!(name.display_family_first(true), "Knuth, D. E.");
        assert_eq!(name.display_family_first(false), "Knuth, Donald Ervin");
    }

    #[test]
    fn a_hyphenated_given_name_keeps_its_hyphen_when_abbreviated() {
        let name = Name::parse("Picard, Jean-Luc").unwrap();
        assert_eq!(
            name.display_family_first(true),
            "Picard, J.-L.",
            "J. alone would be a different person"
        );
    }

    #[test]
    fn a_mononym_renders_without_a_stray_comma() {
        let name = Name::parse("{Aristotle}").unwrap();
        assert_eq!(name.display_family_first(true), "Aristotle");
        assert_eq!(name.display_given_first(false), "Aristotle");
    }

    #[test]
    fn a_reference_prefers_journal_over_booktitle() {
        let mut entry = Entry::new(EntryKind::Article, "k");
        entry.set_field("journal", "Nature");
        entry.set_field("booktitle", "Proceedings of Something");
        assert_eq!(
            Reference::from_entry(&entry).container.as_deref(),
            Some("Nature")
        );
    }

    #[test]
    fn a_reference_strips_protection_braces_for_display() {
        let mut entry = Entry::new(EntryKind::Article, "k");
        entry.set_field("title", "The {DNA} of {TeX}");
        assert_eq!(
            Reference::from_entry(&entry).title.as_deref(),
            Some("The DNA of TeX")
        );
    }

    #[test]
    fn a_latex_page_dash_becomes_a_plain_one() {
        let mut entry = Entry::new(EntryKind::Article, "k");
        entry.set_field("pages", "101--114");
        assert_eq!(
            Reference::from_entry(&entry).pages.as_deref(),
            Some("101-114")
        );
    }

    #[test]
    fn an_unrecognisable_doi_is_kept_rather_than_dropped() {
        let mut entry = Entry::new(EntryKind::Article, "k");
        entry.set_field("doi", "not a doi at all");
        assert_eq!(
            Reference::from_entry(&entry).doi.as_deref(),
            Some("not a doi at all"),
            "a reader can act on a wrong DOI and not on a missing one"
        );
    }

    #[test]
    fn a_doi_written_as_a_url_is_normalised() {
        let mut entry = Entry::new(EntryKind::Article, "k");
        entry.set_field("doi", "https://doi.org/10.1000/XYZ");
        assert_eq!(
            Reference::from_entry(&entry).doi.as_deref(),
            Some("10.1000/xyz")
        );
    }

    #[test]
    fn an_entry_with_no_authors_is_attributed_to_its_editors() {
        let mut entry = Entry::new(EntryKind::Book, "k");
        entry.set_field("editor", "Doe, Jane");
        let reference = Reference::from_entry(&entry);
        assert!(reference.authors.is_empty());
        assert_eq!(reference.attributed_to().len(), 1);
    }

    #[test]
    fn an_entry_with_nothing_in_it_becomes_an_empty_reference() {
        let entry = Entry::new(EntryKind::Misc, "k");
        let reference = Reference::from_entry(&entry);
        assert_eq!(reference.title, None);
        assert_eq!(reference.year, None);
        assert!(reference.attributed_to().is_empty());
    }

    proptest! {
        /// Emitting a parsed name and reparsing it must return the same name.
        /// Stated over source text rather than over generated `Name` values
        /// because a hand-built `Name` can hold a particle that begins with a
        /// capital, which no written form can express.
        #[test]
        fn a_parsed_name_survives_being_written_and_read_again(
            source in r"([A-Za-z,. -]|\{[A-Za-z ]{0,5}\}){0,20}"
        ) {
            let Some(name) = Name::parse(&source) else { return Ok(()) };
            let written = name.to_bibtex();
            let reparsed = Name::parse(&written);
            prop_assert_eq!(reparsed.as_ref(), Some(&name), "wrote {:?}", written);
        }

        /// A name list survives the same trip, separators included.
        #[test]
        fn a_name_list_survives_being_written_and_read_again(
            sources in proptest::collection::vec(
                r"([A-Za-z,. -]|\{[A-Za-z ]{0,5}\}){0,10}",
                0..4,
            )
        ) {
            let names: Vec<Name> = sources.iter().filter_map(|s| Name::parse(s)).collect();
            let written: Vec<String> = names.iter().map(Name::to_bibtex).collect();
            prop_assert_eq!(Name::parse_list(&written.join(" and ")), names);
        }

        /// Reading a type name and writing it back is a fixed point.
        #[test]
        fn an_entry_type_name_normalises_once(source in "[A-Za-z]{0,20}") {
            let kind = EntryKind::parse(&source);
            prop_assert_eq!(EntryKind::parse(kind.as_str()), kind);
        }

        /// Nothing in here panics, whatever the field values look like.
        #[test]
        fn building_a_reference_never_panics(value in ".{0,60}") {
            let mut entry = Entry::new(EntryKind::parse(&value), "key");
            for field in ["author", "title", "year", "pages", "doi", "journal"] {
                entry.set_field(field, value.clone());
            }
            let _ = Reference::from_entry(&entry);
        }
    }
}
