//! What a document says about its author that its author did not write.
//!
//! specs.md section 15 asks for a metadata inspector, and section 24 names
//! "metadata leakage" as a threat category on its own. The threat is not that
//! a document contains a secret -- that is `bp-secrets`' subject -- but that
//! it identifies someone: who wrote it, on what machine, under what account,
//! for which organisation, and which copy of it this is.
//!
//! **A finding is a position, not a value.** [`MetadataFinding`] carries byte
//! offsets and a classification, on the same argument `bp-secrets` makes about
//! its `Finding`: a struct that carried the matched text would become a new
//! place a person's name and address live, and the caller already has the
//! document in front of it. The offsets are *bytes*, not line and column,
//! precisely so that [`MetadataFinding::span`] hands straight to
//! [`redact`](crate::redact) -- inspection and destruction are meant to
//! compose without a conversion step in between, because a conversion step is
//! where an off-by-one turns into a redaction of the wrong words.
//!
//! **Only what text can show.** [`inspect`] reads a `&str` and nothing else.
//! The heavier metadata -- a `.docx` author field, a PDF producer string, EXIF
//! GPS coordinates -- lives in containers that need a ZIP reader, an XML
//! parser or an image parser to open, and this crate adds no dependency for
//! any of them. [`Container`] is how that boundary is *reported* rather than
//! hidden: it names, per format, what is there and what a build would have to
//! gain to see it. An inspector that quietly said "no metadata found" about a
//! `.docx` would be worse than no inspector, because the user would believe
//! it.
//!
//! **The filesystem is outside this crate entirely.** Modification times,
//! ownership, extended attributes and alternate data streams identify a
//! document too, and reading them is impure. That is the shell's to do, and
//! [`Container::PlainText`] says so rather than implying a plain text file
//! carries nothing.
//!
//! Same discipline as the rest: a false positive teaches the user to ignore
//! the panel, and an ignored panel finds nothing. Every rule below is anchored
//! on something an ordinary sentence does not contain.

use crate::Span;

/// A category of identifying information a document can carry.
///
/// `#[non_exhaustive]` because this list grows with every format and habit we
/// learn about, and growing it must not break the shell. Prefer
/// [`MetadataKind::label`] to matching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum MetadataKind {
    /// A person, named by a field that exists to name one: `author`,
    /// `Signed-off-by`, `last_modified_by`.
    PersonName,
    /// A company, department or team.
    OrganizationName,
    /// An email address, wherever it appears.
    EmailAddress,
    /// The account name inside a filesystem path -- the `danie` of
    /// `C:\Users\danie\...`. Pasting a path is the most common way a document
    /// discloses who was at the keyboard.
    LocalAccount,
    /// A machine, workstation or device name.
    MachineName,
    /// Software that touched the document: a generator, a producer, a mail
    /// client, a user agent.
    ToolFingerprint,
    /// A full date and time. Says when, and -- when it carries a numeric UTC
    /// offset -- roughly where.
    Timestamp,
    /// A run of characters with no visible glyph: zero-width spaces, joiners,
    /// bidirectional controls. Nothing a person types on purpose, and the
    /// standard way to fingerprint which copy of a document leaked.
    InvisibleCharacter,
}

impl MetadataKind {
    /// A short human-readable name, for a panel row or a redaction label.
    ///
    /// Lives here so the shell never matches on the enum, which is what makes
    /// `#[non_exhaustive]` free rather than annoying. Also what
    /// [`MetadataFinding::span`] uses as its label, so these words can end up
    /// inside a document as `[REDACTED: author name]`: they are written to be
    /// read by whoever receives the redacted copy.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::PersonName => "author name",
            Self::OrganizationName => "organisation",
            Self::EmailAddress => "email address",
            Self::LocalAccount => "account name",
            Self::MachineName => "machine name",
            Self::ToolFingerprint => "software fingerprint",
            Self::Timestamp => "timestamp",
            Self::InvisibleCharacter => "invisible characters",
        }
    }
}

/// How far a finding goes towards identifying somebody.
///
/// Ordered, so `finding.exposure >= Exposure::Identifying` is a filter a
/// caller can write and mean. The UI is expected to show everything and let
/// the user choose; anything automatic should demand the top tier, because
/// automatically deleting a timestamp out of somebody's notes is a bug they
/// will not forgive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Exposure {
    /// Narrows the field. When the document was written, with what, roughly
    /// where -- enough to correlate against something else, not enough alone.
    Circumstantial,
    /// Names a machine, an account, an organisation, or a particular copy.
    Identifying,
    /// Names a person, or an address that reaches one.
    Attributable,
}

/// Where identifying information is, never what it says.
///
/// `start..end` are byte offsets into the inspected text, half-open and
/// always on character boundaries, so `&text[finding.start..finding.end]` is
/// the value and [`MetadataFinding::span`] is the redaction of it. `line` is
/// 1-based and there only for display -- it matches `bp_buffer::Position`'s
/// convention so a caller can put it in a list without arithmetic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MetadataFinding {
    /// What kind of identifying information this is.
    pub kind: MetadataKind,
    /// First byte of the value.
    pub start: usize,
    /// One past the last byte of the value.
    pub end: usize,
    /// 1-based line the value starts on, for display.
    pub line: usize,
    /// How identifying it is.
    pub exposure: Exposure,
}

impl MetadataFinding {
    /// The redaction span for this finding, labelled by its kind.
    ///
    /// The whole reason findings are in byte offsets: inspecting and then
    /// redacting is one line of caller code, with no unit conversion in
    /// between to get wrong. The label is [`MetadataKind::label`] rather than
    /// the value, so a `[REDACTED: email address]` marker describes what was
    /// taken out without putting it back.
    #[must_use]
    pub const fn span(&self) -> Span<'static> {
        Span::labelled(self.start, self.end, self.kind.label())
    }
}

/// A shape a document arrives in, and what its metadata costs to read.
///
/// This exists so that "we cannot see it" is something the product can *say*.
/// A metadata inspector that reports nothing for a `.docx` is not neutral: the
/// user reads it as an all-clear and sends the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Container {
    /// Plain text, Markdown, source, YAML, JSON, CSV -- anything whose bytes
    /// are the document. [`inspect`] sees everything the *content* holds; the
    /// filesystem entry around it does not belong to this crate.
    PlainText,
    /// Office Open XML: `.docx`, `.xlsx`, `.pptx`. A ZIP of XML parts.
    OfficeOpenXml,
    /// OpenDocument: `.odt`, `.ods`, `.odp`. Also a ZIP of XML parts.
    OpenDocument,
    /// PDF.
    Pdf,
    /// Rich Text Format.
    RichText,
    /// A raster image, carrying EXIF, XMP or PNG text chunks.
    Image,
}

impl Container {
    /// A short human-readable name.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::PlainText => "plain text",
            Self::OfficeOpenXml => "Office Open XML",
            Self::OpenDocument => "OpenDocument",
            Self::Pdf => "PDF",
            Self::RichText => "Rich Text Format",
            Self::Image => "image",
        }
    }

    /// What this container holds that reading its text cannot reach.
    ///
    /// Empty for [`Container::PlainText`], which is the only honest answer
    /// for a format whose bytes are its content -- and the reason this returns
    /// a slice rather than an `Option`: "nothing hidden" and "we did not look"
    /// must not be the same value.
    #[must_use]
    pub const fn hidden(self) -> &'static [MetadataKind] {
        match self {
            Self::PlainText => &[],
            Self::OfficeOpenXml | Self::OpenDocument => &[
                MetadataKind::PersonName,
                MetadataKind::OrganizationName,
                MetadataKind::ToolFingerprint,
                MetadataKind::Timestamp,
            ],
            Self::Pdf => &[
                MetadataKind::PersonName,
                MetadataKind::ToolFingerprint,
                MetadataKind::Timestamp,
            ],
            Self::RichText => &[MetadataKind::PersonName, MetadataKind::ToolFingerprint],
            Self::Image => &[
                MetadataKind::ToolFingerprint,
                MetadataKind::Timestamp,
                MetadataKind::MachineName,
            ],
        }
    }

    /// What a build would have to gain before [`inspect`] could see
    /// [`Container::hidden`], or `None` when it already sees everything.
    ///
    /// Deliberately a sentence rather than a crate name: the decision to take
    /// a ZIP reader and an XML parser into a product that promises to open
    /// documents from strangers is a security decision with an owner, and it
    /// is not this crate's to make quietly. Report it, and let it be argued.
    #[must_use]
    pub const fn requires(self) -> Option<&'static str> {
        match self {
            Self::PlainText => None,
            Self::OfficeOpenXml => {
                Some("a ZIP reader and an XML parser, for docProps/core.xml and docProps/app.xml")
            }
            Self::OpenDocument => Some("a ZIP reader and an XML parser, for meta.xml"),
            Self::Pdf => Some("a PDF parser, for the document information dictionary and XMP"),
            Self::RichText => Some("an RTF parser, for the \\info group"),
            Self::Image => Some("an EXIF and XMP reader"),
        }
    }
}

/// Find identifying information in `text`, in document order.
///
/// Findings are sorted by position and never overlap: one stretch of text
/// yields at most one finding, and where two rules both match, the one that
/// starts earlier and covers more wins -- an email address inside an
/// `author:` line is reported once, as the author line, because that is the
/// span a user would want to remove.
///
/// Sees only what the text shows; see [`Container`] for what it does not.
///
/// Linear in the length of the input. Every lookahead and lookbehind is
/// bounded by a constant, so no document makes this quadratic.
#[must_use]
pub fn inspect(text: &str) -> Vec<MetadataFinding> {
    let mut found = Vec::new();

    scan_identity_keys(text, &mut found);
    scan_email_addresses(text, &mut found);
    scan_account_paths(text, &mut found);
    scan_timestamps(text, &mut found);
    scan_invisible_runs(text, &mut found);

    if found.is_empty() {
        return found;
    }

    // Longest-first at the same start, so the greedy sweep below keeps the
    // span that covers the most rather than an arbitrary one.
    found.sort_by_key(|finding| (finding.start, std::cmp::Reverse(finding.end)));

    let mut kept: Vec<MetadataFinding> = Vec::with_capacity(found.len());
    let mut consumed = 0usize;
    for mut finding in found {
        if finding.start < consumed {
            continue;
        }
        consumed = finding.end;
        finding.line = line_of(text, finding.start);
        kept.push(finding);
    }
    kept
}

// --- shared helpers ----------------------------------------------------

/// The 1-based line a byte offset is on.
///
/// Computed once per surviving finding rather than carried through every rule,
/// because the rules work in byte offsets and a line number is only ever
/// wanted for display.
fn line_of(text: &str, offset: usize) -> usize {
    text[..offset].bytes().filter(|byte| *byte == b'\n').count() + 1
}

/// Push a finding, with the line left to be filled in by [`inspect`].
fn push(
    out: &mut Vec<MetadataFinding>,
    kind: MetadataKind,
    start: usize,
    end: usize,
    exposure: Exposure,
) {
    if start < end {
        out.push(MetadataFinding {
            kind,
            start,
            end,
            line: 0,
            exposure,
        });
    }
}

const fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Does a value look like something a person actually wrote, rather than a
/// template hole, a null or a shrug?
///
/// Without this the inspector reports `author: ${AUTHOR}` and
/// `creator: unknown` as disclosures, and a panel full of those is a panel
/// nobody reads.
fn is_a_real_value(value: &str, quoted: bool) -> bool {
    if value.len() < 2 || value.len() > MAX_VALUE_BYTES {
        return false;
    }
    if !value.chars().any(char::is_alphanumeric) {
        return false;
    }
    if value.starts_with('$') || value.starts_with('<') || value.starts_with('{') {
        return false;
    }
    // Quoting is the author saying "this is data". Without it, punctuation
    // that only appears in code is the giveaway: `let owner =
    // repository.owner();` matches the `owner` key and is not a disclosure of
    // anybody. Angle brackets are deliberately not on this list, because
    // `Daniel Boles <daniel@example.invalid>` is the conventional way an
    // authorship field names a person.
    if !quoted && value.contains(['(', ')', '{', '}', ';']) {
        return false;
    }
    !matches!(
        value.to_ascii_lowercase().as_str(),
        "unknown" | "none" | "null" | "nil" | "n/a" | "na" | "tbd" | "todo" | "anonymous"
    )
}

/// Longest value a metadata field can have before it stops being a field and
/// starts being a paragraph. A name, a company or a producer string is far
/// shorter, and the bound keeps every scan constant-time per position.
const MAX_VALUE_BYTES: usize = 256;

// --- rule: fields that exist to name somebody --------------------------

/// Normalised key names that identify their value, and what it is.
///
/// Every entry is a name whose *only* meaning is the one wanted here. Bare
/// `host` is deliberately absent: it names a server far more often than a
/// workstation, and a rule that flags `host: localhost` is a rule the user
/// turns off. `tool` and `application` are absent for the same reason.
const IDENTITY_KEYS: &[(&str, MetadataKind, Exposure)] = &[
    ("author", MetadataKind::PersonName, Exposure::Attributable),
    ("authors", MetadataKind::PersonName, Exposure::Attributable),
    ("creator", MetadataKind::PersonName, Exposure::Attributable),
    (
        "created_by",
        MetadataKind::PersonName,
        Exposure::Attributable,
    ),
    (
        "modified_by",
        MetadataKind::PersonName,
        Exposure::Attributable,
    ),
    (
        "last_modified_by",
        MetadataKind::PersonName,
        Exposure::Attributable,
    ),
    ("owner", MetadataKind::PersonName, Exposure::Attributable),
    (
        "maintainer",
        MetadataKind::PersonName,
        Exposure::Attributable,
    ),
    ("reviewer", MetadataKind::PersonName, Exposure::Attributable),
    (
        "signed_off_by",
        MetadataKind::PersonName,
        Exposure::Attributable,
    ),
    (
        "co_authored_by",
        MetadataKind::PersonName,
        Exposure::Attributable,
    ),
    ("contact", MetadataKind::PersonName, Exposure::Attributable),
    (
        "company",
        MetadataKind::OrganizationName,
        Exposure::Identifying,
    ),
    (
        "organization",
        MetadataKind::OrganizationName,
        Exposure::Identifying,
    ),
    (
        "organisation",
        MetadataKind::OrganizationName,
        Exposure::Identifying,
    ),
    (
        "department",
        MetadataKind::OrganizationName,
        Exposure::Identifying,
    ),
    (
        "employer",
        MetadataKind::OrganizationName,
        Exposure::Identifying,
    ),
    ("hostname", MetadataKind::MachineName, Exposure::Identifying),
    (
        "host_name",
        MetadataKind::MachineName,
        Exposure::Identifying,
    ),
    (
        "computer_name",
        MetadataKind::MachineName,
        Exposure::Identifying,
    ),
    (
        "machine_name",
        MetadataKind::MachineName,
        Exposure::Identifying,
    ),
    (
        "device_name",
        MetadataKind::MachineName,
        Exposure::Identifying,
    ),
    (
        "workstation",
        MetadataKind::MachineName,
        Exposure::Identifying,
    ),
    (
        "generator",
        MetadataKind::ToolFingerprint,
        Exposure::Circumstantial,
    ),
    (
        "producer",
        MetadataKind::ToolFingerprint,
        Exposure::Circumstantial,
    ),
    (
        "created_with",
        MetadataKind::ToolFingerprint,
        Exposure::Circumstantial,
    ),
    (
        "user_agent",
        MetadataKind::ToolFingerprint,
        Exposure::Circumstantial,
    ),
    (
        "x_mailer",
        MetadataKind::ToolFingerprint,
        Exposure::Circumstantial,
    ),
];

/// Longest key we will look back over. Nothing in [`IDENTITY_KEYS`] is close,
/// and the bound is what keeps the backwards walk from making the scan
/// quadratic on a line of ten thousand colons.
const MAX_KEY_BYTES: usize = 32;

/// A normalised key never outgrows this: [`MAX_KEY_BYTES`] plus an underscore
/// per camel-case hump.
const NORMALISED_KEY_CAP: usize = MAX_KEY_BYTES * 2;

/// Fold a key into one shape -- lower case, underscore-separated, camel-case
/// humps split -- so `lastModifiedBy`, `last-modified-by` and
/// `Last_Modified_By` are one entry in the table rather than three.
fn normalise_key(raw: &str, buffer: &mut [u8; NORMALISED_KEY_CAP]) -> usize {
    let mut len = 0usize;
    let mut previous_was_lower_or_digit = false;

    for byte in raw.bytes() {
        if len + 2 > NORMALISED_KEY_CAP {
            break;
        }
        if byte.is_ascii_uppercase() && previous_was_lower_or_digit {
            buffer[len] = b'_';
            len += 1;
        }
        previous_was_lower_or_digit = byte.is_ascii_lowercase() || byte.is_ascii_digit();
        buffer[len] = if byte == b'-' || byte == b'.' {
            b'_'
        } else {
            byte.to_ascii_lowercase()
        };
        len += 1;
    }

    len
}

const fn is_key_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-' || byte == b'.'
}

/// One field per line, taken at the first `:` or `=` on it.
///
/// One per line rather than one per separator because these keys sit at the
/// head of a line in every format that carries them -- YAML front matter, a
/// git trailer, an INI section, a JSON object printed one member per line --
/// and looking further along the line only finds prose containing a colon.
fn scan_identity_keys(text: &str, out: &mut Vec<MetadataFinding>) {
    let mut offset = 0usize;

    for raw_line in text.split_inclusive('\n') {
        let line = raw_line.trim_end_matches(['\n', '\r']);
        if let Some((kind, exposure, start, end)) = identity_field(line) {
            push(out, kind, offset + start, offset + end, exposure);
        }
        offset += raw_line.len();
    }
}

/// The identifying field on this line, as `(kind, exposure, value start, value
/// end)` in line-relative bytes.
fn identity_field(line: &str) -> Option<(MetadataKind, Exposure, usize, usize)> {
    let bytes = line.as_bytes();
    let separator = bytes.iter().position(|byte| matches!(byte, b':' | b'='))?;

    let key = key_before(line, separator)?;
    let mut buffer = [0u8; NORMALISED_KEY_CAP];
    let len = normalise_key(key, &mut buffer);
    let key = std::str::from_utf8(&buffer[..len]).ok()?;

    let (_, kind, exposure) = IDENTITY_KEYS
        .iter()
        .find(|(candidate, _, _)| *candidate == key)?;

    let (start, value, quoted) = value_after(line, separator + 1)?;
    is_a_real_value(value, quoted).then_some((*kind, *exposure, start, start + value.len()))
}

/// The key name immediately left of a separator, if there is one.
fn key_before(line: &str, separator: usize) -> Option<&str> {
    let bytes = line.as_bytes();
    let mut end = separator;
    while end > 0 && matches!(bytes[end - 1], b' ' | b'\t') {
        end -= 1;
    }
    if end > 0 && matches!(bytes[end - 1], b'"' | b'\'') {
        end -= 1;
    }
    let mut start = end;
    while start > 0 && is_key_byte(bytes[start - 1]) && end - start < MAX_KEY_BYTES {
        start -= 1;
    }
    (start < end).then(|| &line[start..end])
}

/// The value to the right of a separator: its offset in the line, its text,
/// and whether it was quoted.
///
/// Handles a quoted value and a bare one; a bare value runs to the end of the
/// line with trailing punctuation and comment markers trimmed off. Whether it
/// was quoted is returned rather than discarded because it is evidence: an
/// author who wrote quotes around a value was writing data.
fn value_after(line: &str, after_separator: usize) -> Option<(usize, &str, bool)> {
    let bytes = line.as_bytes();
    let mut start = after_separator;
    while start < bytes.len() && matches!(bytes[start], b' ' | b'\t') {
        start += 1;
    }
    if start >= bytes.len() {
        return None;
    }

    // Every limit below is `start` plus a constant, which is an arithmetic
    // offset and not necessarily a character boundary. Slicing a `&str` at one
    // of those panics, and a panic from a metadata panel is a crash in the
    // user's face over a document that merely had an ideograph in the wrong
    // place.
    let cap = |mut limit: usize| {
        limit = limit.min(line.len());
        while !line.is_char_boundary(limit) {
            limit -= 1;
        }
        limit
    };

    if matches!(bytes[start], b'"' | b'\'') {
        let quote = bytes[start];
        start += 1;
        let limit = cap(start + MAX_VALUE_BYTES + 1);
        let mut end = start;
        while end < limit && bytes[end] != quote {
            end += 1;
        }
        if end >= limit {
            return None;
        }
        return (end > start).then(|| (start, &line[start..end], true));
    }

    let limit = cap(start + MAX_VALUE_BYTES + 1);
    let mut end = limit;
    // `-->` closes an HTML comment and `,` ends a JSON member; neither is part
    // of the name.
    if let Some(close) = line[start..limit].find("-->") {
        end = start + close;
    }
    while end > start && matches!(bytes[end - 1], b' ' | b'\t' | b',' | b';') {
        end -= 1;
    }
    (end > start).then(|| (start, &line[start..end], false))
}

// --- rule: email addresses ---------------------------------------------

const fn is_local_part_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'%' | b'+' | b'-' | b'\'')
}

const fn is_domain_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-')
}

/// Anchored on `@`, which is what makes this cheap and what makes it safe: a
/// bare handle (`@someone`) has no local part and is skipped, and a domain
/// with no dot or a one-letter last label is not a domain.
fn scan_email_addresses(text: &str, out: &mut Vec<MetadataFinding>) {
    let bytes = text.as_bytes();

    for at in 0..bytes.len() {
        if bytes[at] != b'@' {
            continue;
        }

        let mut start = at;
        while start > 0 && is_local_part_byte(bytes[start - 1]) && at - start < 64 {
            start -= 1;
        }
        if start == at || bytes[start] == b'.' {
            continue;
        }

        let mut end = at + 1;
        while end < bytes.len() && is_domain_byte(bytes[end]) && end - at < 256 {
            end += 1;
        }
        // A trailing dot is punctuation at the end of a sentence.
        while end > at + 1 && bytes[end - 1] == b'.' {
            end -= 1;
        }

        let domain = &text[at + 1..end];
        let Some((_, tld)) = domain.rsplit_once('.') else {
            continue;
        };
        if tld.len() < 2 || !tld.bytes().all(|byte| byte.is_ascii_alphabetic()) {
            continue;
        }

        push(
            out,
            MetadataKind::EmailAddress,
            start,
            end,
            Exposure::Attributable,
        );
    }
}

// --- rule: account names inside filesystem paths -----------------------

const fn is_separator(byte: u8) -> bool {
    byte == b'/' || byte == b'\\'
}

/// Path segments whose next segment is a user account name.
const ACCOUNT_ROOTS: &[&str] = &["users", "home"];

/// Segments that follow an account root without naming a person.
const NOT_ACCOUNTS: &[&str] = &["public", "default", "shared", "all", "guest"];

/// Find `C:\Users\<name>`, `/home/<name>`, `/Users/<name>` and their
/// double-backslash forms, and report `<name>`.
///
/// The span is the account name alone, not the whole path, for the same
/// reason `bp-secrets` spans the password inside a URI rather than the URI: it
/// is the part that identifies somebody, and a document that legitimately
/// refers to a file should not have the reference destroyed to hide the
/// account it sits under. The rest of the path is the caller's judgement.
///
/// The root must be at the start of a path -- after a drive letter, or after
/// whitespace, a quote or a bracket -- so that `src/users/list.rs` is a
/// directory of source code rather than a disclosure.
fn scan_account_paths(text: &str, out: &mut Vec<MetadataFinding>) {
    let bytes = text.as_bytes();

    for start in 0..bytes.len() {
        if !is_separator(bytes[start]) {
            continue;
        }

        // Compared as bytes rather than as a `&str`: `start + 1 + len` is an
        // arithmetic offset, and slicing a `&str` at one that lands inside a
        // multi-byte character panics. `/\u{a1}\u{800}` is enough to do it.
        let Some(root) = ACCOUNT_ROOTS.iter().find(|root| {
            bytes.len() >= start + 1 + root.len()
                && bytes[start + 1..start + 1 + root.len()].eq_ignore_ascii_case(root.as_bytes())
        }) else {
            continue;
        };
        let after_root = start + 1 + root.len();
        if after_root >= bytes.len() || !is_separator(bytes[after_root]) {
            continue;
        }

        // Walk back over a run of separators, so `C:\\Users\\danie` -- a path
        // as it appears inside a JSON, Rust or shell string -- is recognised
        // as readily as `C:\Users\danie`.
        let mut root_start = start;
        while root_start > 0 && is_separator(bytes[root_start - 1]) {
            root_start -= 1;
        }
        let anchored = root_start == 0
            || (bytes[root_start - 1] == b':'
                && (root_start < 2
                    || !is_word_byte(bytes[root_start - 2])
                    || bytes[root_start - 2].is_ascii_alphabetic()))
            || matches!(
                bytes[root_start - 1],
                b' ' | b'\t' | b'\n' | b'\r' | b'"' | b'\'' | b'`' | b'(' | b'[' | b'=' | b'<'
            );
        if !anchored {
            continue;
        }

        let mut name_start = after_root;
        while name_start < bytes.len() && is_separator(bytes[name_start]) {
            name_start += 1;
        }
        let mut name_end = name_start;
        while name_end < bytes.len()
            && !is_separator(bytes[name_end])
            && !bytes[name_end].is_ascii_whitespace()
            && !matches!(
                bytes[name_end],
                b'"' | b'\'' | b'`' | b')' | b']' | b',' | b';'
            )
            && name_end - name_start < MAX_VALUE_BYTES
        {
            name_end += 1;
        }
        // The length bound above can stop in the middle of a character.
        while name_end > name_start && !text.is_char_boundary(name_end) {
            name_end -= 1;
        }
        if name_end == name_start {
            continue;
        }

        let name = &text[name_start..name_end];
        if NOT_ACCOUNTS
            .iter()
            .any(|excluded| name.eq_ignore_ascii_case(excluded))
        {
            continue;
        }

        push(
            out,
            MetadataKind::LocalAccount,
            name_start,
            name_end,
            Exposure::Identifying,
        );
    }
}

// --- rule: timestamps --------------------------------------------------

fn digits_at(bytes: &[u8], at: usize, count: usize) -> bool {
    at + count <= bytes.len() && bytes[at..at + count].iter().all(u8::is_ascii_digit)
}

fn two_digit_value(bytes: &[u8], at: usize) -> u32 {
    u32::from(bytes[at] - b'0') * 10 + u32::from(bytes[at + 1] - b'0')
}

/// Match ISO 8601 date-times: `2026-08-19T14:32:07+01:00` and its variants.
///
/// Date-times only, never bare dates. A date on its own is as likely to be
/// content as metadata -- a deadline, a quotation, a note about a meeting --
/// and flagging every one of them buries the findings that matter. A full
/// timestamp is machine-written, and its numeric UTC offset says roughly where
/// the machine was; that is why this is worth reporting at all, and why it is
/// reported at the lowest tier.
fn scan_timestamps(text: &str, out: &mut Vec<MetadataFinding>) {
    let bytes = text.as_bytes();
    let mut at = 0usize;

    while at < bytes.len() {
        if !digits_at(bytes, at, 4)
            || (at > 0 && is_word_byte(bytes[at - 1]))
            || bytes.get(at + 4) != Some(&b'-')
            || !digits_at(bytes, at + 5, 2)
            || bytes.get(at + 7) != Some(&b'-')
            || !digits_at(bytes, at + 8, 2)
        {
            at += 1;
            continue;
        }

        let month = two_digit_value(bytes, at + 5);
        let day = two_digit_value(bytes, at + 8);
        if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
            at += 1;
            continue;
        }

        // The date must be followed by a time, or this is a date and not a
        // timestamp.
        let mut end = at + 10;
        if !matches!(bytes.get(end), Some(b'T' | b't' | b' '))
            || !digits_at(bytes, end + 1, 2)
            || bytes.get(end + 3) != Some(&b':')
            || !digits_at(bytes, end + 4, 2)
        {
            at += 1;
            continue;
        }
        end += 6;

        if bytes.get(end) == Some(&b':') && digits_at(bytes, end + 1, 2) {
            end += 3;
        }
        if bytes.get(end) == Some(&b'.') {
            let mut fraction = end + 1;
            while fraction < bytes.len() && bytes[fraction].is_ascii_digit() {
                fraction += 1;
            }
            if fraction > end + 1 {
                end = fraction;
            }
        }
        if matches!(bytes.get(end), Some(b'Z' | b'z')) {
            end += 1;
        } else if matches!(bytes.get(end), Some(b'+' | b'-')) && digits_at(bytes, end + 1, 2) {
            let offset_end = if bytes.get(end + 3) == Some(&b':') && digits_at(bytes, end + 4, 2) {
                end + 6
            } else if digits_at(bytes, end + 3, 2) {
                end + 5
            } else {
                end + 3
            };
            end = offset_end;
        }

        push(
            out,
            MetadataKind::Timestamp,
            at,
            end,
            Exposure::Circumstantial,
        );
        at = end;
    }
}

// --- rule: characters with no glyph ------------------------------------

/// Characters that occupy a position and show nothing.
///
/// Zero-width spaces and joiners are the standard way to fingerprint a
/// document per recipient: a few of them, in a pattern, identify which copy
/// leaked. The bidirectional controls are the Trojan Source family, which make
/// text read differently from how it is stored. A person types none of these
/// on purpose in a note, and a document that contains a run of them is a
/// document somebody prepared.
const fn is_invisible(character: char) -> bool {
    matches!(character,
        '\u{00ad}'                  // soft hyphen
        | '\u{180e}'                // Mongolian vowel separator
        | '\u{200b}'..='\u{200f}'   // zero-width space, joiners, LRM/RLM
        | '\u{202a}'..='\u{202e}'   // bidi embeddings and overrides
        | '\u{2060}'..='\u{2064}'   // word joiner, invisible operators
        | '\u{2066}'..='\u{2069}'   // bidi isolates
        | '\u{feff}'                // zero-width no-break space
    )
}

/// One finding per run, not per character: a fingerprint is a pattern of many,
/// and a panel with four hundred rows in it is a panel nobody reads.
///
/// A byte-order mark at offset zero is skipped. There it is an encoding
/// artefact that half the world's editors write, and reporting it would put a
/// finding on a large share of perfectly ordinary files. The same character
/// anywhere else was put there on purpose.
fn scan_invisible_runs(text: &str, out: &mut Vec<MetadataFinding>) {
    let mut run: Option<(usize, usize)> = None;

    for (at, character) in text.char_indices() {
        let counts = is_invisible(character) && !(at == 0 && character == '\u{feff}');
        match (&mut run, counts) {
            (Some(open), true) => open.1 = at + character.len_utf8(),
            (Some(open), false) => {
                push(
                    out,
                    MetadataKind::InvisibleCharacter,
                    open.0,
                    open.1,
                    Exposure::Identifying,
                );
                run = None;
            }
            (None, true) => run = Some((at, at + character.len_utf8())),
            (None, false) => {}
        }
    }

    if let Some(open) = run {
        push(
            out,
            MetadataKind::InvisibleCharacter,
            open.0,
            open.1,
            Exposure::Identifying,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Replacement, redact, verify};

    /// The single finding a fixture is expected to produce.
    fn single(text: &str) -> MetadataFinding {
        let found = inspect(text);
        assert_eq!(found.len(), 1, "expected one finding, got {found:?}");
        found[0]
    }

    /// The text a finding covers, for tests only. Nothing in the public API
    /// hands this back.
    fn value<'t>(text: &'t str, finding: &MetadataFinding) -> &'t str {
        &text[finding.start..finding.end]
    }

    #[test]
    fn an_author_field_names_a_person() {
        for line in [
            "author: Daniel Boles",
            "Author = Daniel Boles",
            "  \"author\": \"Daniel Boles\",",
            "lastModifiedBy: Daniel Boles",
            "Signed-off-by: Daniel Boles",
        ] {
            let found = single(line);
            assert_eq!(found.kind, MetadataKind::PersonName, "{line}");
            assert_eq!(found.exposure, Exposure::Attributable, "{line}");
            assert_eq!(value(line, &found), "Daniel Boles", "{line}");
            assert_eq!(found.line, 1);
        }
    }

    #[test]
    fn a_company_is_identifying_and_a_generator_is_only_circumstantial() {
        let found = single("company: Northwind Traders Ltd");
        assert_eq!(found.kind, MetadataKind::OrganizationName);
        assert_eq!(found.exposure, Exposure::Identifying);

        let found = single("generator: BachelorPad+ 0.1.0");
        assert_eq!(found.kind, MetadataKind::ToolFingerprint);
        assert_eq!(found.exposure, Exposure::Circumstantial);
        assert!(found.exposure < Exposure::Identifying);
    }

    #[test]
    fn a_field_holding_a_template_hole_or_a_shrug_is_not_a_disclosure() {
        for line in [
            "author: ${AUTHOR}",
            "author: <name here>",
            "author: unknown",
            "author: N/A",
            "author:",
            "author: \"\"",
        ] {
            assert!(inspect(line).is_empty(), "{line}");
        }
    }

    #[test]
    fn an_email_address_is_found_and_a_handle_is_not() {
        let text = "write to daniel.boles+notes@example.co.uk about it";
        let found = single(text);
        assert_eq!(found.kind, MetadataKind::EmailAddress);
        assert_eq!(value(text, &found), "daniel.boles+notes@example.co.uk");

        for line in [
            "mention @danielboles in the issue",
            "the @media query",
            "a@b",
            "user@localhost",
            "trailing user@example.com.",
        ] {
            let found = inspect(line);
            let addresses = found
                .iter()
                .filter(|f| f.kind == MetadataKind::EmailAddress)
                .count();
            if line.starts_with("trailing") {
                assert_eq!(addresses, 1, "{line}");
                assert_eq!(value(line, &found[0]), "user@example.com");
            } else {
                assert_eq!(addresses, 0, "{line}");
            }
        }
    }

    #[test]
    fn a_user_path_names_the_account_and_not_the_whole_path() {
        for (text, account) in [
            ("C:\\Users\\danie\\AppData\\Local", "danie"),
            ("\"C:\\\\Users\\\\danie\\\\notes.txt\"", "danie"),
            ("/home/danie/projects/notes.md", "danie"),
            ("/Users/danie/Documents", "danie"),
        ] {
            let found = inspect(text);
            let account_names: Vec<_> = found
                .iter()
                .filter(|f| f.kind == MetadataKind::LocalAccount)
                .collect();
            assert_eq!(account_names.len(), 1, "{text}: {found:?}");
            assert_eq!(value(text, account_names[0]), account, "{text}");
            assert_eq!(account_names[0].exposure, Exposure::Identifying);
        }
    }

    #[test]
    fn a_directory_that_happens_to_be_called_users_is_not_an_account() {
        for text in [
            "src/users/list.rs",
            "app\\users\\model.rs",
            "the users guide",
            "/home/",
            "C:\\Users\\Public\\Documents",
            "/Users/Shared/data",
        ] {
            assert!(
                inspect(text)
                    .iter()
                    .all(|f| f.kind != MetadataKind::LocalAccount),
                "{text}"
            );
        }
    }

    #[test]
    fn a_full_timestamp_is_reported_and_a_bare_date_is_not() {
        let text = "last_save 2026-08-19T14:32:07+01:00 done";
        let found = single(text);
        assert_eq!(found.kind, MetadataKind::Timestamp);
        assert_eq!(found.exposure, Exposure::Circumstantial);
        assert_eq!(value(text, &found), "2026-08-19T14:32:07+01:00");

        for line in [
            "due 2026-08-19 at the latest",
            "version 2026-13-01T00:00:00Z",
            "part number 1234-56-78",
            "2026-08-19T14",
        ] {
            assert!(
                inspect(line)
                    .iter()
                    .all(|f| f.kind != MetadataKind::Timestamp),
                "{line}"
            );
        }
    }

    #[test]
    fn every_timestamp_shape_is_matched_end_to_end() {
        for stamp in [
            "2026-08-19T14:32Z",
            "2026-08-19T14:32:07Z",
            "2026-08-19 14:32:07",
            "2026-08-19T14:32:07.123456+0530",
            "2026-08-19t14:32:07-05:00",
        ] {
            let found = single(stamp);
            assert_eq!(value(stamp, &found), stamp, "{stamp}");
        }
    }

    #[test]
    fn a_run_of_invisible_characters_is_one_finding() {
        let text = "visible\u{200b}\u{200b}\u{200d}\u{2060}text";
        let found = single(text);

        assert_eq!(found.kind, MetadataKind::InvisibleCharacter);
        assert_eq!(found.exposure, Exposure::Identifying);
        assert_eq!(
            value(text, &found).chars().count(),
            4,
            "one finding, not four"
        );
    }

    #[test]
    fn a_leading_byte_order_mark_is_encoding_and_anywhere_else_is_a_fingerprint() {
        assert!(inspect("\u{feff}ordinary file").is_empty());
        assert_eq!(
            single("ordinary\u{feff}file").kind,
            MetadataKind::InvisibleCharacter
        );
    }

    #[test]
    fn a_finding_at_the_end_of_the_text_is_not_dropped() {
        // The boundary the run-scanner is most likely to get wrong: a run that
        // is still open when the text ends.
        let text = "trailing\u{200b}\u{200b}";
        let found = single(text);
        assert_eq!(found.end, text.len());
    }

    #[test]
    fn findings_are_sorted_and_never_overlap() {
        let text = "author: Daniel Boles <daniel@example.com>\npath: /home/danie/notes.md\n";
        let found = inspect(text);

        for pair in found.windows(2) {
            assert!(pair[0].end <= pair[1].start, "{:?}", found);
        }
        // The author line wins over the address inside it: one row in the
        // panel, and the span a user would actually want to remove.
        assert_eq!(found[0].kind, MetadataKind::PersonName);
        assert_eq!(value(text, &found[0]), "Daniel Boles <daniel@example.com>");
        assert_eq!(found[1].kind, MetadataKind::LocalAccount);
        assert_eq!(found[1].line, 2, "the line is for display and is 1-based");
    }

    /// Text that must never produce a finding.
    ///
    /// The corpus that decides whether the panel survives contact with a real
    /// document.
    const NOT_METADATA: &[&str] = &[
        "The author of the report has not been named.",
        "let owner = repository.owner();",
        "host: localhost",
        "port = 8080",
        "database=orders",
        "// TODO: ask the maintainer",
        "src/users/mod.rs",
        "a href=\"mailto\"",
        "meeting on 2026-08-19",
        "1.2.3-alpha",
        "SELECT * FROM users WHERE id = 1",
        "email addresses are stored in the keychain",
        "contact:",
        "@media (prefers-color-scheme: dark) {",
        "https://example.invalid/users/42",
        "0.0.0.0:8080",
    ];

    #[test]
    fn the_false_positive_corpus_produces_nothing_at_all() {
        for line in NOT_METADATA {
            let found = inspect(line);
            assert!(found.is_empty(), "flagged {line}\n  as {found:?}");
        }
        let document = NOT_METADATA.join("\n");
        let found = inspect(&document);
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn a_finding_redacts_cleanly_through_its_own_span() {
        // The composition the byte offsets exist for: inspect, map to spans,
        // redact, verify -- with no unit conversion anywhere in between.
        let text = "author: Daniel Boles\nmachine: none\npath: /home/danie/notes.md";
        let spans: Vec<_> = inspect(text).iter().map(MetadataFinding::span).collect();
        assert_eq!(spans.len(), 2);

        let redacted = redact(text, &spans, Replacement::Placeholder).expect("valid spans");
        assert_eq!(
            redacted.text,
            "author: [REDACTED: author name]\nmachine: none\npath: /home/[REDACTED: account name]/notes.md"
        );
        assert!(
            verify(text, &spans, &redacted.text)
                .expect("valid spans")
                .is_clean()
        );
    }

    #[test]
    fn an_empty_document_carries_no_metadata() {
        assert!(inspect("").is_empty());
        assert!(inspect("\n\n\n").is_empty());
        assert!(inspect("   \t  ").is_empty());
    }

    #[test]
    fn every_kind_has_a_label_and_exposure_is_ordered() {
        for kind in [
            MetadataKind::PersonName,
            MetadataKind::OrganizationName,
            MetadataKind::EmailAddress,
            MetadataKind::LocalAccount,
            MetadataKind::MachineName,
            MetadataKind::ToolFingerprint,
            MetadataKind::Timestamp,
            MetadataKind::InvisibleCharacter,
        ] {
            assert!(!kind.label().is_empty(), "{kind:?}");
            // Labels reach the document through `Span::labelled`, so one
            // containing a bracket would have to be sanitised out.
            assert!(!kind.label().contains(['[', ']']), "{kind:?}");
        }

        assert!(Exposure::Circumstantial < Exposure::Identifying);
        assert!(Exposure::Identifying < Exposure::Attributable);
    }

    #[test]
    fn every_container_that_hides_metadata_says_what_reading_it_would_cost() {
        for container in [
            Container::PlainText,
            Container::OfficeOpenXml,
            Container::OpenDocument,
            Container::Pdf,
            Container::RichText,
            Container::Image,
        ] {
            assert!(!container.label().is_empty());
            assert_eq!(
                container.hidden().is_empty(),
                container.requires().is_none(),
                "{container:?}: a container either hides something and says \
                 what it would take to read it, or hides nothing"
            );
        }

        // Plain text is the only one this crate sees all of, and even there
        // the filesystem entry around the bytes is somebody else's problem.
        assert!(Container::PlainText.hidden().is_empty());
        assert!(Container::PlainText.requires().is_none());
        assert!(
            Container::OfficeOpenXml
                .requires()
                .expect("a docx needs a reader")
                .contains("ZIP")
        );
    }

    #[test]
    fn the_key_table_is_normalised_and_free_of_duplicates() {
        // An entry that is not already in normalised form would silently never
        // match, and a duplicate is a sign the table was edited twice.
        let mut buffer = [0u8; NORMALISED_KEY_CAP];
        for (key, _, _) in IDENTITY_KEYS {
            let len = normalise_key(key, &mut buffer);
            assert_eq!(
                std::str::from_utf8(&buffer[..len]).expect("ascii"),
                *key,
                "{key} is not in normalised form"
            );
            assert!(
                key.len() <= MAX_KEY_BYTES,
                "{key} is longer than we look back"
            );
        }

        let mut keys: Vec<&str> = IDENTITY_KEYS.iter().map(|(key, _, _)| *key).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), IDENTITY_KEYS.len(), "duplicate key");
    }

    #[test]
    fn a_path_separator_followed_by_multi_byte_text_is_not_sliced_in_half() {
        // Found by the property tests. `/` starts the account-path rule, which
        // then compared the next five bytes against "users" -- and those five
        // bytes end inside a three-byte character, which is a panic on the
        // slice rather than a wrong answer. Compared as bytes ever since.
        for text in [
            "/\u{a1}\u{800}",
            "\\u{4e2d}\u{6587}",
            "//\u{1f511}",
            "/hom\u{e9}",
        ] {
            let _ = inspect(text);
        }
    }

    #[test]
    fn odd_input_is_inspected_without_panicking() {
        for text in [
            "",
            "\n",
            ":",
            "=",
            "@",
            "@.",
            "/",
            "\\",
            "//////",
            "author:",
            "\u{0}\u{1}\u{7f}",
            "\u{200b}",
            "\u{feff}",
            "\u{65e5}\u{672c}\u{8a9e}: \u{30c6}\u{30ad}\u{30b9}\u{30c8}",
            "author: \u{1f511}\u{1f511}\u{1f511}",
            "2026-08-19T",
            &"@".repeat(5_000),
            &"/home/".repeat(2_000),
            &"a".repeat(10_000),
        ] {
            let found = inspect(text);
            for finding in found {
                assert!(text.is_char_boundary(finding.start), "{finding:?}");
                assert!(text.is_char_boundary(finding.end), "{finding:?}");
            }
        }
    }
}
