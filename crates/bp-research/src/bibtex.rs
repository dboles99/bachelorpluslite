//! Reading and writing `.bib` files.
//!
//! BibTeX looks like a config format and is not one. `@string` macros,
//! concatenation with `#`, two interchangeable delimiter pairs, values that
//! may be braced or quoted or bare, brace groups that nest inside quoted
//! strings, backslash escapes that suspend all of it, and free text between
//! entries that is a comment by omission rather than by syntax. A hand-rolled
//! "split on commas and equals" reader gets most files right and mangles the
//! rest quietly, which is the worst available outcome for a bibliography: a
//! dropped `pages` field is not noticed until a reviewer looks it up.
//!
//! So this is a real scanner, and the rule it is built on is that **nothing
//! is discarded silently**. Anything this parser cannot resolve --- an
//! undefined macro, a field given twice, a brace that never closes --- stops
//! the parse and produces a message with a line and a column, so the user
//! fixes their file instead of finding out later that a reference is wrong.
//! [`BibtexError::at`] is there so the shell can put the caret on it.
//!
//! What is *deliberately* not done here:
//!
//! * **LaTeX is not interpreted.** `\"{o}` stays `\"{o}`. Rendering TeX
//!   markup into Unicode is a large table with real ambiguity in it, and a
//!   half-done version silently changes people's names.
//! * **Free text between entries, and `@comment` blocks, are dropped on
//!   re-emit.** They carry no bibliographic data; keeping their exact
//!   position would mean modelling the file rather than its contents.
//! * **Field order and original case are not preserved.** Names are
//!   lower-cased and fields emit alphabetically, so that a file this crate
//!   writes twice is byte-identical twice.
//!
//! Hand-written, no regex and no parser generator, for the same reason as
//! `bp-secrets`: one linear pass with no backtracking, so a pathological
//! `.bib` cannot become a hang.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::model::{Entry, EntryKind, Reference};

/// Where in the source something is, 1-based, in characters rather than
/// bytes --- because it is fed to a caret, and a caret sits between
/// characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

impl std::fmt::Display for Position {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}, column {}", self.line, self.column)
    }
}

/// A parse failure, with the place it happened.
///
/// One struct rather than an enum of variants each carrying a line and a
/// column, so that a caller can reach the position without matching on the
/// reason --- which is what the shell actually needs to do.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("{at}: {kind}")]
pub struct BibtexError {
    pub at: Position,
    pub kind: BibtexErrorKind,
}

/// Why a `.bib` file could not be read.
///
/// The messages are written to be shown to the person whose file it is: they
/// say what was expected, not what the scanner's state machine was doing.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BibtexErrorKind {
    #[error("expected an entry type after the \"@\", such as @article")]
    MissingEntryType,

    #[error("@{kind} has to be followed by {{ or (")]
    MissingEntryBody { kind: String },

    #[error("this entry has no citation key -- the name a \\cite would use")]
    MissingKey,

    #[error(
        "\"{key}\" cannot be a citation key: a key runs from the opening brace \
         to the first comma and may not contain spaces, braces or quotes"
    )]
    MalformedKey { key: String },

    #[error("\"{key}\" is the key of two entries, and a citation key has to name one work")]
    DuplicateKey { key: String },

    #[error("expected a field name, such as \"author =\"")]
    ExpectedFieldName,

    #[error("the field \"{field}\" has to be followed by =")]
    ExpectedEquals { field: String },

    #[error(
        "expected a value here: text in braces, text in quotes, a number, \
         or the name of an @string"
    )]
    ExpectedValue,

    #[error("expected a comma before the next field, or the end of the entry")]
    ExpectedFieldSeparator,

    #[error(
        "entry \"{key}\" gives \"{field}\" twice; BibTeX would keep one and \
         discard the other, so which one is meant has to be decided in the file"
    )]
    DuplicateField { key: String, field: String },

    #[error("this {{ is never closed")]
    UnclosedBrace,

    #[error("this \" is never closed")]
    UnclosedQuote,

    #[error("this }} closes a group that was never opened")]
    UnbalancedBrace,

    #[error(
        "\"{name}\" is not a defined @string, so this value cannot be resolved; \
         define it above, or write the text out in full"
    )]
    UnknownMacro { name: String },

    #[error("the file ends in the middle of an entry")]
    UnexpectedEnd,
}

/// Everything a `.bib` file holds that means something.
///
/// The macros and the preamble are kept even though every entry value has
/// already been expanded against them: they are the user's file, and a
/// round trip that quietly deleted the `@string` block would look like data
/// loss the next time they opened it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Bibliography {
    pub entries: Vec<Entry>,
    /// `@string` definitions, keyed by their lower-cased name.
    pub macros: BTreeMap<String, String>,
    /// `@preamble` blocks, in the order they appeared.
    pub preamble: Vec<String>,
}

impl Bibliography {
    /// The entry a citation key names.
    ///
    /// Linear, and deliberately: a bibliography is hundreds of entries, not
    /// millions, and an index would be a second copy of the keys to keep
    /// correct. Keys are unique --- [`BibtexErrorKind::DuplicateKey`]
    /// guarantees it --- so the first match is the only match.
    #[must_use]
    pub fn entry(&self, key: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.key == key)
    }

    /// Every entry as the view a style renders.
    #[must_use]
    pub fn references(&self) -> Vec<Reference> {
        self.entries.iter().map(Reference::from_entry).collect()
    }
}

/// The month abbreviations every BibTeX style defines.
///
/// Predefined here because `month = jan` is in a large fraction of real `.bib`
/// files and refusing it as an unknown macro would make this parser useless
/// on them. A `@string` in the file overrides these, which is also what
/// BibTeX does.
const BUILTIN_MACROS: &[(&str, &str)] = &[
    ("jan", "January"),
    ("feb", "February"),
    ("mar", "March"),
    ("apr", "April"),
    ("may", "May"),
    ("jun", "June"),
    ("jul", "July"),
    ("aug", "August"),
    ("sep", "September"),
    ("oct", "October"),
    ("nov", "November"),
    ("dec", "December"),
];

/// Read a `.bib` file.
///
/// An empty or entry-free input is an empty [`Bibliography`], not an error:
/// a new bibliography starts out with nothing in it, and refusing to open it
/// would be a refusal to start.
///
/// # Errors
///
/// Any construct that cannot be resolved without guessing. See
/// [`BibtexErrorKind`] --- every variant exists because resolving it by
/// guessing would put a wrong reference in someone's paper.
pub fn parse(source: &str) -> Result<Bibliography, BibtexError> {
    let mut scanner = Scanner::new(source);
    let mut bibliography = Bibliography::default();

    while scanner.skip_to_entry() {
        let at = scanner.position();
        scanner.bump(); // the '@'
        scanner.skip_whitespace();
        let name = scanner.take_while(|b| b.is_ascii_alphabetic());
        if name.is_empty() {
            return Err(BibtexError {
                at,
                kind: BibtexErrorKind::MissingEntryType,
            });
        }

        match name.to_ascii_lowercase().as_str() {
            // `@comment` has no agreed body syntax --- JabRef writes a braced
            // group, plain BibTeX treats the rest of the line as prose --- so
            // both are skipped rather than parsed.
            "comment" => scanner.skip_comment(),
            "string" => {
                let (macro_name, value) = scanner.parse_string(&bibliography.macros)?;
                // Last definition wins, as in BibTeX. Not an error: a file
                // that redefines a month is doing something legal.
                bibliography.macros.insert(macro_name, value);
            }
            "preamble" => {
                let value = scanner.parse_delimited_value(&bibliography.macros, &name)?;
                bibliography.preamble.push(value);
            }
            _ => {
                let entry = scanner.parse_entry(EntryKind::parse(&name), &bibliography.macros)?;
                if let Some(existing) = bibliography.entry(&entry.key) {
                    return Err(BibtexError {
                        at,
                        kind: BibtexErrorKind::DuplicateKey {
                            key: existing.key.clone(),
                        },
                    });
                }
                bibliography.entries.push(entry);
            }
        }
    }

    Ok(bibliography)
}

/// Write a bibliography back out.
///
/// Reading the result returns an equal [`Bibliography`] --- that is the
/// property this file is tested on, and the reason values are always emitted
/// braced rather than quoted: braces nest, so any value the parser can
/// produce can be written back inside them without escaping.
#[must_use]
pub fn emit(bibliography: &Bibliography) -> String {
    let mut out = String::new();

    for (name, value) in &bibliography.macros {
        let _ = writeln!(out, "@string{{{name} = {{{value}}}}}");
    }
    for text in &bibliography.preamble {
        let _ = writeln!(out, "@preamble{{{{{text}}}}}");
    }
    if !bibliography.macros.is_empty() || !bibliography.preamble.is_empty() {
        out.push('\n');
    }

    for (index, entry) in bibliography.entries.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        out.push_str(&emit_entry(entry));
    }
    out
}

/// Write one entry.
///
/// Fields come out in alphabetical order and every entry ends with a trailing
/// comma, because the point of a stable layout is that a bibliography edited
/// in one place produces a one-line diff.
#[must_use]
pub fn emit_entry(entry: &Entry) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "@{}{{{},", entry.kind.as_str(), entry.key);
    for (name, value) in &entry.fields {
        let _ = writeln!(out, "  {name} = {{{value}}},");
    }
    out.push_str("}\n");
    out
}

/// Remove the braces that only exist to protect capitalisation.
///
/// `{DNA}` in a title tells BibTeX not to lower-case those letters; a reader
/// should never see the braces. But the braces after a control sequence are
/// its *argument* --- dropping them from `\"{o}` changes what it means --- so
/// those are kept, along with the command itself. LaTeX is not otherwise
/// interpreted here; see the module docs for why.
#[must_use]
pub fn strip_protection(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = String::with_capacity(value.len());
    let mut index = 0usize;

    while index < bytes.len() {
        match bytes[index] {
            b'\\' => {
                let start = index;
                index += 1;
                if index < bytes.len() && bytes[index].is_ascii_alphabetic() {
                    // A control word runs to the end of its letters.
                    while index < bytes.len() && bytes[index].is_ascii_alphabetic() {
                        index += 1;
                    }
                } else if index < bytes.len() {
                    // A control symbol is exactly one character: `\&`, `\"`.
                    index += char_len(value, index);
                }
                out.push_str(&value[start..index]);
                if bytes.get(index) == Some(&b'{') {
                    let end = brace_group_end(bytes, index);
                    out.push_str(&value[index..end]);
                    index = end;
                }
            }
            b'{' | b'}' => index += 1,
            _ => {
                let len = char_len(value, index);
                out.push_str(&value[index..index + len]);
                index += len;
            }
        }
    }
    out
}

/// One past the `}` matching the `{` at `start`, or the end if there is none.
fn brace_group_end(bytes: &[u8], start: usize) -> usize {
    let mut depth = 0usize;
    let mut index = start;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 1,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return index + 1;
                }
            }
            _ => {}
        }
        index += 1;
    }
    bytes.len()
}

/// Length in bytes of the character starting at `index`.
fn char_len(text: &str, index: usize) -> usize {
    text[index..].chars().next().map_or(1, char::len_utf8)
}

/// Bytes that may appear in a field name, an entry key or a bare value.
///
/// BibTeX's own rule, by exclusion: everything except whitespace and the ten
/// characters that mean something structural. Written as a deny-list because
/// the allow-list would have to include every letter of every alphabet ---
/// field names in the wild are ASCII, keys are not.
fn is_token_byte(byte: u8) -> bool {
    !byte.is_ascii_whitespace()
        && !matches!(
            byte,
            b'"' | b'#' | b'%' | b'\'' | b'(' | b')' | b',' | b'=' | b'{' | b'}'
        )
}

/// A cursor over the source that knows where it is.
struct Scanner<'a> {
    text: &'a str,
    bytes: &'a [u8],
    offset: usize,
    line: usize,
    column: usize,
}

impl<'a> Scanner<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            text,
            bytes: text.as_bytes(),
            offset: 0,
            line: 1,
            column: 1,
        }
    }

    fn position(&self) -> Position {
        Position {
            line: self.line,
            column: self.column,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.offset).copied()
    }

    /// Advance one character, keeping the line and column honest. Multi-byte
    /// characters move the column by one, not by their byte length.
    fn bump(&mut self) -> Option<char> {
        let c = self.text[self.offset..].chars().next()?;
        self.offset += c.len_utf8();
        if c == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(c)
    }

    fn skip_whitespace(&mut self) {
        while self.peek().is_some_and(|b| b.is_ascii_whitespace()) {
            self.bump();
        }
    }

    fn take_while(&mut self, accept: impl Fn(u8) -> bool) -> String {
        let start = self.offset;
        while self.peek().is_some_and(&accept) {
            self.bump();
        }
        self.text[start..self.offset].to_owned()
    }

    /// Skip everything up to the next `@`, and say whether one was found.
    ///
    /// Text outside entries is not part of the format --- BibTeX ignores it,
    /// and so do we. A line whose first non-blank character is `%` is skipped
    /// whole: `%` is not a comment in BibTeX, but generations of users and
    /// tools have written it as one, and an `@` inside such a line is far
    /// more likely to be a commented-out entry than a live one.
    fn skip_to_entry(&mut self) -> bool {
        loop {
            match self.peek() {
                None => return false,
                Some(b'@') => return true,
                Some(b'%') => {
                    while self.peek().is_some_and(|b| b != b'\n') {
                        self.bump();
                    }
                }
                Some(_) => {
                    self.bump();
                }
            }
        }
    }

    /// `@comment` in either of the two shapes the world writes it in.
    fn skip_comment(&mut self) {
        self.skip_whitespace();
        match self.peek() {
            Some(b'{') => {
                let end = brace_group_end(self.bytes, self.offset);
                while self.offset < end && self.bump().is_some() {}
            }
            Some(b'(') => {
                while self.peek().is_some_and(|b| b != b')') {
                    self.bump();
                }
                self.bump();
            }
            _ => {
                while self.peek().is_some_and(|b| b != b'\n') {
                    self.bump();
                }
            }
        }
    }

    fn error(&self, at: Position, kind: BibtexErrorKind) -> BibtexError {
        BibtexError { at, kind }
    }

    /// The delimiter pair opening a body, `{}` or `()`.
    fn open_body(&mut self, kind: &str) -> Result<u8, BibtexError> {
        self.skip_whitespace();
        let at = self.position();
        match self.peek() {
            Some(b'{') => {
                self.bump();
                Ok(b'}')
            }
            Some(b'(') => {
                self.bump();
                Ok(b')')
            }
            _ => Err(self.error(
                at,
                BibtexErrorKind::MissingEntryBody {
                    kind: kind.to_owned(),
                },
            )),
        }
    }

    /// `@string{ name = value }`.
    fn parse_string(
        &mut self,
        macros: &BTreeMap<String, String>,
    ) -> Result<(String, String), BibtexError> {
        let close = self.open_body("string")?;
        self.skip_whitespace();
        let at = self.position();
        let name = self.take_while(is_token_byte);
        if name.is_empty() {
            return Err(self.error(at, BibtexErrorKind::ExpectedFieldName));
        }
        self.skip_whitespace();
        let at = self.position();
        if self.peek() != Some(b'=') {
            return Err(self.error(
                at,
                BibtexErrorKind::ExpectedEquals {
                    field: name.clone(),
                },
            ));
        }
        self.bump();
        let value = self.parse_value(macros)?;
        self.skip_whitespace();
        let at = self.position();
        if self.peek() != Some(close) {
            return Err(self.error(at, BibtexErrorKind::ExpectedFieldSeparator));
        }
        self.bump();
        Ok((name.to_ascii_lowercase(), value))
    }

    /// `@preamble{ value }` --- a body that is one value and nothing else.
    fn parse_delimited_value(
        &mut self,
        macros: &BTreeMap<String, String>,
        kind: &str,
    ) -> Result<String, BibtexError> {
        let close = self.open_body(kind)?;
        let value = self.parse_value(macros)?;
        self.skip_whitespace();
        let at = self.position();
        if self.peek() != Some(close) {
            return Err(self.error(at, BibtexErrorKind::ExpectedFieldSeparator));
        }
        self.bump();
        Ok(value)
    }

    fn parse_entry(
        &mut self,
        kind: EntryKind,
        macros: &BTreeMap<String, String>,
    ) -> Result<Entry, BibtexError> {
        let close = self.open_body(kind.as_str())?;
        self.skip_whitespace();

        let at = self.position();
        let key = self.take_while(|b| b != b',' && b != close && b != b'\n');
        let key = key.trim().to_owned();
        if key.is_empty() {
            return Err(self.error(at, BibtexErrorKind::MissingKey));
        }
        if !key.bytes().all(is_token_byte) {
            return Err(self.error(at, BibtexErrorKind::MalformedKey { key }));
        }

        let mut entry = Entry::new(kind, key);
        match self.peek() {
            Some(b) if b == close => {
                self.bump();
                return Ok(entry);
            }
            Some(b',') => {
                self.bump();
            }
            _ => {
                return Err(self.error(at, BibtexErrorKind::UnexpectedEnd));
            }
        }

        loop {
            self.skip_whitespace();
            let at = self.position();
            match self.peek() {
                None => return Err(self.error(at, BibtexErrorKind::UnexpectedEnd)),
                Some(b) if b == close => {
                    self.bump();
                    return Ok(entry);
                }
                Some(_) => {}
            }

            let name = self.take_while(is_token_byte);
            if name.is_empty() {
                return Err(self.error(at, BibtexErrorKind::ExpectedFieldName));
            }
            let name = name.to_ascii_lowercase();

            self.skip_whitespace();
            let equals_at = self.position();
            if self.peek() != Some(b'=') {
                return Err(self.error(equals_at, BibtexErrorKind::ExpectedEquals { field: name }));
            }
            self.bump();

            let value = self.parse_value(macros)?;
            if entry.fields.insert(name.clone(), value).is_some() {
                return Err(self.error(
                    at,
                    BibtexErrorKind::DuplicateField {
                        key: entry.key.clone(),
                        field: name,
                    },
                ));
            }

            self.skip_whitespace();
            let at = self.position();
            match self.peek() {
                Some(b',') => {
                    self.bump();
                }
                Some(b) if b == close => {
                    self.bump();
                    return Ok(entry);
                }
                Some(_) => return Err(self.error(at, BibtexErrorKind::ExpectedFieldSeparator)),
                None => return Err(self.error(at, BibtexErrorKind::UnexpectedEnd)),
            }
        }
    }

    /// One value, possibly several pieces joined with `#`.
    ///
    /// Concatenation is resolved here rather than kept in the model: the
    /// pieces are an authoring convenience, and a `Reference` that had to
    /// know about them would push BibTeX's syntax into every style.
    fn parse_value(&mut self, macros: &BTreeMap<String, String>) -> Result<String, BibtexError> {
        let mut out = String::new();
        loop {
            self.skip_whitespace();
            let at = self.position();
            match self.peek() {
                Some(b'{') => out.push_str(&self.braced_body()?),
                Some(b'"') => out.push_str(&self.quoted_body()?),
                Some(b) if is_token_byte(b) => {
                    let token = self.take_while(is_token_byte);
                    // A bare number is itself; a bare word is a macro name.
                    // That is BibTeX's rule, and it is why `year = 2020`
                    // works without anybody defining `2020`.
                    if token.bytes().all(|b| b.is_ascii_digit()) {
                        out.push_str(&token);
                    } else {
                        let key = token.to_ascii_lowercase();
                        let resolved = macros.get(&key).map(String::as_str).or_else(|| {
                            BUILTIN_MACROS
                                .iter()
                                .find(|(name, _)| *name == key)
                                .map(|(_, value)| *value)
                        });
                        match resolved {
                            Some(value) => out.push_str(value),
                            None => {
                                return Err(
                                    self.error(at, BibtexErrorKind::UnknownMacro { name: token })
                                );
                            }
                        }
                    }
                }
                _ => return Err(self.error(at, BibtexErrorKind::ExpectedValue)),
            }

            self.skip_whitespace();
            if self.peek() == Some(b'#') {
                self.bump();
            } else {
                return Ok(out);
            }
        }
    }

    /// A `{...}` value, returned without its outer braces.
    ///
    /// Inner braces are kept: they are the case protection a style needs, and
    /// [`strip_protection`] is where they come off. A backslash suspends the
    /// next character entirely, which is how `\{` fails to open a group.
    fn braced_body(&mut self) -> Result<String, BibtexError> {
        let opened = self.position();
        self.bump(); // the '{'
        let mut depth = 1usize;
        let mut out = String::new();

        while let Some(byte) = self.peek() {
            match byte {
                b'\\' => {
                    out.push('\\');
                    self.bump();
                    if let Some(c) = self.bump() {
                        out.push(c);
                    }
                }
                b'{' => {
                    depth += 1;
                    out.push('{');
                    self.bump();
                }
                b'}' => {
                    depth -= 1;
                    self.bump();
                    if depth == 0 {
                        return Ok(out);
                    }
                    out.push('}');
                }
                _ => {
                    if let Some(c) = self.bump() {
                        out.push(c);
                    }
                }
            }
        }
        Err(self.error(opened, BibtexErrorKind::UnclosedBrace))
    }

    /// A `"..."` value, returned without its quotes.
    ///
    /// A `"` inside a brace group does not end the value --- that is the trap
    /// that catches naive readers, and the reason `title = "The {"} Problem"`
    /// is a legal thing someone has written.
    fn quoted_body(&mut self) -> Result<String, BibtexError> {
        let opened = self.position();
        self.bump(); // the '"'
        let mut depth = 0usize;
        let mut out = String::new();

        while let Some(byte) = self.peek() {
            match byte {
                b'\\' => {
                    out.push('\\');
                    self.bump();
                    if let Some(c) = self.bump() {
                        out.push(c);
                    }
                }
                b'{' => {
                    depth += 1;
                    out.push('{');
                    self.bump();
                }
                b'}' => {
                    if depth == 0 {
                        let at = self.position();
                        return Err(self.error(at, BibtexErrorKind::UnbalancedBrace));
                    }
                    depth -= 1;
                    out.push('}');
                    self.bump();
                }
                b'"' if depth == 0 => {
                    self.bump();
                    return Ok(out);
                }
                _ => {
                    if let Some(c) = self.bump() {
                        out.push(c);
                    }
                }
            }
        }
        Err(self.error(opened, BibtexErrorKind::UnclosedQuote))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn one(source: &str) -> Entry {
        let bibliography = parse(source).expect("should parse");
        assert_eq!(bibliography.entries.len(), 1, "expected exactly one entry");
        bibliography.entries.into_iter().next().unwrap()
    }

    fn kind_of(source: &str) -> BibtexErrorKind {
        parse(source).expect_err("should have failed").kind
    }

    #[test]
    fn an_empty_file_is_an_empty_bibliography() {
        // Refusing to open a new bibliography would be a refusal to start.
        assert_eq!(parse("").unwrap(), Bibliography::default());
        assert_eq!(parse("   \n\n\t").unwrap(), Bibliography::default());
    }

    #[test]
    fn a_plain_entry_reads_its_type_key_and_fields() {
        let entry = one("@article{knuth1984,\n  author = {Knuth, D.},\n  year = 1984,\n}");
        assert_eq!(entry.kind, EntryKind::Article);
        assert_eq!(entry.key, "knuth1984");
        assert_eq!(entry.field("author"), Some("Knuth, D."));
        assert_eq!(entry.field("year"), Some("1984"));
    }

    #[test]
    fn an_entry_may_have_no_fields_at_all() {
        let entry = one("@misc{lonely}");
        assert!(entry.fields.is_empty());
        assert_eq!(entry.key, "lonely");
    }

    #[test]
    fn parentheses_delimit_an_entry_as_well_as_braces() {
        let entry = one("@article(k, title = {T})");
        assert_eq!(entry.field("title"), Some("T"));
    }

    #[test]
    fn nested_braces_survive_intact() {
        let entry = one("@article{k, title = {The {DNA} of {{TeX}}}}");
        assert_eq!(
            entry.field("title"),
            Some("The {DNA} of {{TeX}}"),
            "protection braces belong to the value, not to the syntax"
        );
    }

    #[test]
    fn a_quote_inside_braces_does_not_end_a_quoted_value() {
        let entry = one(r#"@article{k, title = "The {"} problem"}"#);
        assert_eq!(entry.field("title"), Some(r#"The {"} problem"#));
    }

    #[test]
    fn a_comma_or_an_at_sign_inside_a_value_is_just_text() {
        let entry = one("@article{k, note = {a, b @ c}, year = 2000}");
        assert_eq!(entry.field("note"), Some("a, b @ c"));
        assert_eq!(entry.field("year"), Some("2000"));
    }

    #[test]
    fn an_escaped_brace_does_not_open_a_group() {
        let entry = one(r"@article{k, title = {100\% \{literal\}}}");
        assert_eq!(entry.field("title"), Some(r"100\% \{literal\}"));
    }

    #[test]
    fn string_macros_are_expanded_and_concatenated() {
        let entry = one("@string{acm = {ACM Press}}\n\
             @article{k, publisher = acm # { and friends}}");
        assert_eq!(entry.field("publisher"), Some("ACM Press and friends"));
    }

    #[test]
    fn a_string_macro_may_be_referenced_case_insensitively() {
        let entry = one("@string{ACM = {ACM}}\n@article{k, publisher = acm}");
        assert_eq!(entry.field("publisher"), Some("ACM"));
    }

    #[test]
    fn month_abbreviations_are_predefined() {
        // Refusing these would make the parser useless on real files.
        let entry = one("@article{k, month = jan}");
        assert_eq!(entry.field("month"), Some("January"));
    }

    #[test]
    fn a_string_definition_overrides_a_builtin_month() {
        let entry = one("@string{jan = {Januar}}\n@article{k, month = jan}");
        assert_eq!(entry.field("month"), Some("Januar"));
    }

    #[test]
    fn an_undefined_macro_stops_the_parse_and_names_itself() {
        // The alternative is an empty publisher field nobody notices.
        let kind = kind_of("@article{k, publisher = acm}");
        assert_eq!(
            kind,
            BibtexErrorKind::UnknownMacro {
                name: "acm".to_owned()
            }
        );
        assert!(kind.to_string().contains("not a defined @string"));
    }

    #[test]
    fn a_field_given_twice_is_refused_rather_than_halved() {
        // The failure mode this whole file is designed against.
        let error = parse("@article{k, title = {A}, Title = {B}}").expect_err("must fail");
        assert_eq!(
            error.kind,
            BibtexErrorKind::DuplicateField {
                key: "k".to_owned(),
                field: "title".to_owned()
            }
        );
        assert!(error.to_string().contains("twice"), "{error}");
        assert!(error.at.line >= 1);
    }

    #[test]
    fn two_entries_may_not_share_a_citation_key() {
        assert_eq!(
            kind_of("@article{k, title = {A}}\n@book{k, title = {B}}"),
            BibtexErrorKind::DuplicateKey {
                key: "k".to_owned()
            }
        );
    }

    #[test]
    fn free_text_and_comments_between_entries_are_ignored() {
        let bibliography = parse(
            "This file was generated by hand.\n\
             @comment{jabref-meta: databaseType:bibtex;}\n\
             % @article{commented, title = {Not real}}\n\
             @article{real, title = {Real}}",
        )
        .unwrap();
        assert_eq!(bibliography.entries.len(), 1);
        assert_eq!(bibliography.entries[0].key, "real");
    }

    #[test]
    fn a_preamble_is_kept() {
        let bibliography = parse(r"@preamble{ {\newcommand{\noopsort}[1]{}} }").unwrap();
        assert_eq!(bibliography.preamble.len(), 1);
        assert!(bibliography.preamble[0].contains("noopsort"));
    }

    #[test]
    fn an_unclosed_brace_reports_where_it_opened() {
        let error = parse("@article{k,\n  title = {never ends\n").expect_err("must fail");
        assert_eq!(error.kind, BibtexErrorKind::UnclosedBrace);
        assert_eq!(
            error.at.line, 2,
            "the line the opening brace is on, not the last line"
        );
        assert!(error.to_string().starts_with("line 2, column"));
    }

    #[test]
    fn an_unclosed_quote_is_reported_as_such() {
        assert_eq!(
            kind_of("@article{k, title = \"never ends\n"),
            BibtexErrorKind::UnclosedQuote
        );
    }

    #[test]
    fn a_missing_equals_names_the_field_it_belongs_to() {
        assert_eq!(
            kind_of("@article{k, title {A}}"),
            BibtexErrorKind::ExpectedEquals {
                field: "title".to_owned()
            }
        );
    }

    #[test]
    fn an_entry_without_a_key_is_refused() {
        assert_eq!(
            kind_of("@article{, title = {A}}"),
            BibtexErrorKind::MissingKey
        );
    }

    #[test]
    fn a_key_with_a_space_in_it_is_refused_and_quoted_back() {
        let kind = kind_of("@article{not a key, title = {A}}");
        assert_eq!(
            kind,
            BibtexErrorKind::MalformedKey {
                key: "not a key".to_owned()
            }
        );
        assert!(kind.to_string().contains("not a key"));
    }

    #[test]
    fn an_at_sign_with_no_type_after_it_is_refused() {
        assert_eq!(kind_of("@ {k}"), BibtexErrorKind::MissingEntryType);
        assert_eq!(kind_of("@"), BibtexErrorKind::MissingEntryType);
    }

    #[test]
    fn an_entry_type_with_no_body_is_refused() {
        assert_eq!(
            kind_of("@article"),
            BibtexErrorKind::MissingEntryBody {
                kind: "article".to_owned()
            }
        );
    }

    #[test]
    fn a_missing_comma_between_fields_is_refused() {
        assert_eq!(
            kind_of("@article{k, title = {A} year = 2000}"),
            BibtexErrorKind::ExpectedFieldSeparator
        );
    }

    #[test]
    fn a_field_with_no_value_is_refused() {
        assert_eq!(
            kind_of("@article{k, title = , year = 2000}"),
            BibtexErrorKind::ExpectedValue
        );
    }

    #[test]
    fn a_file_that_stops_mid_entry_says_so() {
        assert_eq!(
            kind_of("@article{k, title = {A},"),
            BibtexErrorKind::UnexpectedEnd
        );
    }

    #[test]
    fn an_unknown_entry_type_parses_and_keeps_its_name() {
        let entry = one("@dataset{k, title = {A}}");
        assert_eq!(entry.kind, EntryKind::Other("dataset".to_owned()));
        assert!(emit_entry(&entry).starts_with("@dataset{k,"));
    }

    #[test]
    fn positions_count_characters_not_bytes() {
        // A caret sits between characters; a byte offset would land it in the
        // middle of a multi-byte one.
        let error = parse("@article{k, title = {日本語}, x }").expect_err("must fail");
        assert_eq!(error.at.line, 1);
        assert_eq!(error.at.column, 30, "three characters, nine bytes");
    }

    #[test]
    fn emitting_produces_a_stable_layout() {
        let entry = one("@article{k, year = 2000, author = {Doe, J}}");
        assert_eq!(
            emit_entry(&entry),
            "@article{k,\n  author = {Doe, J},\n  year = {2000},\n}\n",
            "alphabetical, braced, trailing comma"
        );
    }

    #[test]
    fn strip_protection_removes_case_braces() {
        assert_eq!(strip_protection("The {DNA} of {TeX}"), "The DNA of TeX");
        assert_eq!(strip_protection("{{Deeply}} nested"), "Deeply nested");
    }

    #[test]
    fn strip_protection_keeps_the_argument_of_a_command() {
        // Dropping these braces changes what TeX would print.
        assert_eq!(strip_protection(r#"{\"{o}}rn"#), r#"\"{o}rn"#);
        assert_eq!(strip_protection(r"{\textbf{bold}}"), r"\textbf{bold}");
        assert_eq!(strip_protection(r"100\% \{literal\}"), r"100\% \{literal\}");
    }

    #[test]
    fn strip_protection_leaves_ordinary_text_alone() {
        assert_eq!(strip_protection(""), "");
        assert_eq!(strip_protection("plain"), "plain");
        assert_eq!(strip_protection("日本語"), "日本語");
    }

    proptest! {
        /// Emitting a parsed bibliography and reading it again returns the
        /// same bibliography --- for every input the parser accepts.
        #[test]
        fn a_parsed_file_survives_being_written_and_read_again(
            source in r#"(@[a-z]{1,8}\{[a-z0-9]{1,6}(,[a-z]{1,6} *= *(\{[a-zA-Z0-9 {}]{0,10}\}|"[a-zA-Z0-9 ]{0,10}"|[0-9]{1,4})){0,3},?\} *){0,3}"#
        ) {
            let Ok(first) = parse(&source) else { return Ok(()) };
            let written = emit(&first);
            let second = parse(&written)
                .map_err(|e| TestCaseError::fail(format!("re-reading {written:?}: {e}")))?;
            prop_assert_eq!(second, first);
        }

        /// The same property from the other end: any bibliography this crate
        /// can hold can be written and read back unchanged.
        #[test]
        fn an_entry_built_in_memory_survives_a_round_trip(
            key in "[a-zA-Z][a-zA-Z0-9:._-]{0,10}",
            values in proptest::collection::vec(r"[a-zA-Z0-9 .,;:@%#!?'/*+_-]{0,30}", 0..4),
        ) {
            let mut entry = Entry::new(EntryKind::Article, key);
            for (index, value) in values.iter().enumerate() {
                entry.set_field(&format!("f{index}"), value.clone());
            }
            let bibliography = Bibliography { entries: vec![entry], ..Bibliography::default() };
            prop_assert_eq!(parse(&emit(&bibliography)).unwrap(), bibliography);
        }

        /// Macros and preambles come back too, not just entries.
        #[test]
        fn macros_and_preambles_survive_a_round_trip(
            name in "[a-z]{1,6}",
            value in "[a-zA-Z0-9 ]{0,20}",
            preamble in "[a-zA-Z0-9 ]{0,20}",
        ) {
            let mut bibliography = Bibliography::default();
            bibliography.macros.insert(name, value);
            bibliography.preamble.push(preamble);
            prop_assert_eq!(parse(&emit(&bibliography)).unwrap(), bibliography);
        }

        /// No input makes the parser panic or hang: it either produces a
        /// bibliography or an error with a position in it.
        #[test]
        fn parsing_arbitrary_text_never_panics(source in r#"[@{}()",#%\\a-z0-9 =\n]{0,80}"#) {
            match parse(&source) {
                Ok(_) => {}
                Err(error) => {
                    prop_assert!(error.at.line >= 1);
                    prop_assert!(error.at.column >= 1);
                    prop_assert!(!error.to_string().is_empty());
                }
            }
        }

        /// Stripping protection deletes braces and nothing else.
        #[test]
        fn strip_protection_only_ever_removes_braces(value in r"[a-zA-Z{}\\ ]{0,40}") {
            let stripped = strip_protection(&value);
            prop_assert_eq!(
                stripped.replace(['{', '}'], ""),
                value.replace(['{', '}'], "")
            );
        }
    }
}
