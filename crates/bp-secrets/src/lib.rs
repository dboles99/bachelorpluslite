//! Secret scanning: find credentials in a document, and never hold one.
//!
//! specs.md section 15 lists secret scanning beside encryption and redaction,
//! and section 24 names "secret persistence" as a threat category. This crate
//! is the detector for that threat: given the text of a document, it reports
//! *where* a credential appears.
//!
//! Two decisions shape everything below.
//!
//! **A finding never carries the secret.** [`Finding`] holds a position, a
//! length and a classification, and nothing else. A struct that carried the
//! matched text would be a new place secrets live, and once it exists someone
//! will `Debug`-print a `Vec` of them into a log -- which is exactly the leak
//! the scan was meant to prevent. Callers that need the text already have the
//! document in front of them.
//!
//! **A false positive is worse than a miss.** A scanner that flags every long
//! string gets switched off within a day, and a switched-off scanner catches
//! nothing. Every rule here is anchored on something a credential has and an
//! ordinary string does not: a vendor prefix, a JWT header that decodes to
//! JSON with an `alg` member, a password sitting in a URI's userinfo.
//! [`Confidence`] lets a caller be stricter still.
//!
//! Pure, like `bp-semantic`: no clock, no IO, no configuration. One pass over
//! each line, and no heap allocation at all for a line that yields nothing.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

/// Crate identity used by workspace smoke tests and diagnostics.
pub const CRATE_NAME: &str = "bp-secrets";

/// What kind of credential a finding is.
///
/// `#[non_exhaustive]` because vendors keep minting new token formats, and
/// adding one must not be a breaking change for the shell. Prefer
/// [`SecretKind::label`] over matching, unless you genuinely need to treat
/// one kind differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SecretKind {
    /// An AWS access key ID (`AKIA...` and its siblings).
    AwsAccessKeyId,
    /// A 40-character AWS secret access key, named as such by its assignment.
    AwsSecretAccessKey,
    /// A GitHub personal access, OAuth, app, refresh or fine-grained token.
    GitHubToken,
    /// A GitLab personal, deploy, runner, agent or build token.
    GitLabToken,
    /// A Slack bot, user, app, legacy or refresh token.
    SlackToken,
    /// The opening marker of a PEM private-key block.
    PrivateKeyBlock,
    /// A JSON Web Token whose header decodes to a JOSE header.
    JsonWebToken,
    /// A password inside a connection string or credentialed URI.
    ConnectionStringPassword,
    /// A high-entropy value assigned to a secret-sounding name.
    GenericSecretAssignment,
}

impl SecretKind {
    /// A short human-readable name, for a UI list or a status message.
    ///
    /// Lives here so the shell never has to match on the enum, which is what
    /// makes `#[non_exhaustive]` free rather than annoying.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::AwsAccessKeyId => "AWS access key ID",
            Self::AwsSecretAccessKey => "AWS secret access key",
            Self::GitHubToken => "GitHub token",
            Self::GitLabToken => "GitLab token",
            Self::SlackToken => "Slack token",
            Self::PrivateKeyBlock => "private key block",
            Self::JsonWebToken => "JSON Web Token",
            Self::ConnectionStringPassword => "connection string password",
            Self::GenericSecretAssignment => "possible secret",
        }
    }

    /// How specific the rule that produced this kind is.
    ///
    /// Used only to settle overlaps: a JWT assigned to `api_token` is one
    /// finding, and it should be reported as the JWT rather than as an
    /// anonymous high-entropy string.
    const fn specificity(self) -> u8 {
        match self {
            Self::PrivateKeyBlock => 4,
            Self::AwsAccessKeyId
            | Self::AwsSecretAccessKey
            | Self::GitHubToken
            | Self::GitLabToken
            | Self::SlackToken
            | Self::JsonWebToken => 3,
            Self::ConnectionStringPassword => 2,
            Self::GenericSecretAssignment => 1,
        }
    }
}

/// How sure the scanner is.
///
/// Ordered, so a caller can write `finding.confidence >= Confidence::Medium`
/// and mean it. The UI is expected to show everything and let the user
/// filter; anything automatic -- refusing to save, redacting on export --
/// should demand `High`, because the cost of being wrong there is the user
/// losing text they wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Confidence {
    /// Looks random and is named like a secret, but nothing vouches for it.
    Low,
    /// Long and random enough that an innocent explanation is a stretch.
    Medium,
    /// A recognised credential format, or a password in a place only a
    /// password goes.
    High,
}

/// Where a credential is, never what it is.
///
/// `line` and `column` are 1-based and match `bp_buffer::Position`, so a
/// caller can jump the caret straight to a finding. `length` is in
/// **characters**, not bytes, for the same reason: the column it extends from
/// is a character column.
///
/// The span covers the credential material itself wherever that is a single
/// run of text -- the value inside the quotes, the password inside the URI --
/// so a redaction feature can replace exactly it. The one exception is
/// [`SecretKind::PrivateKeyBlock`], where the span covers the
/// `-----BEGIN ... PRIVATE KEY-----` marker and the key body continues to the
/// matching `END` line; one line, column and length cannot describe a
/// multi-line block, and stretching the type to do so would complicate every
/// other kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Finding {
    pub kind: SecretKind,
    pub line: usize,
    pub column: usize,
    pub length: usize,
    pub confidence: Confidence,
}

/// Scan `text` for credentials, in document order.
///
/// Findings are sorted by line and then column, with overlaps already
/// resolved: one stretch of text yields at most one finding, classified by
/// the most specific rule that matched it.
///
/// Linear in the length of the input. Every backwards or forwards walk is
/// bounded by a constant -- a key name cannot exceed [`MAX_KEY_CHARS`], and a
/// value longer than [`MAX_VALUE_BYTES`] is a blob rather than a credential
/// -- so no input makes this quadratic and no pattern can backtrack, which is
/// the reason the matching here is hand-written rather than a regex set.
#[must_use]
pub fn scan(text: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let mut raw = Vec::new();

    for (index, line) in text.lines().enumerate() {
        raw.clear();
        scan_known_formats(line, &mut raw);
        scan_assignments(line, &mut raw);
        if raw.is_empty() {
            continue;
        }
        resolve_overlaps(&mut raw);
        emit(line, index + 1, &raw, &mut findings);
    }

    findings
}

// --- limits ------------------------------------------------------------

/// Longest key name we will look back over for an assignment. Nothing anyone
/// calls a password is longer, and the bound is what keeps the backwards walk
/// from turning the scan quadratic.
pub const MAX_KEY_CHARS: usize = 64;

/// Longest assigned value we will consider a candidate. Real credentials are
/// far shorter; past this it is a base64 blob, a certificate body or minified
/// code, and flagging those is how a scanner earns its way into the "off"
/// setting.
pub const MAX_VALUE_BYTES: usize = 512;

/// Shortest value that can be a generic secret. Below this the entropy signal
/// is meaningless: a twelve-character string cannot look random enough to be
/// told apart from a word.
const MIN_GENERIC_VALUE_BYTES: usize = 16;

/// Distinct characters a candidate must contain. `ghp_xxxxxxxxxxxx...` and
/// `AKIA0000000000000000` are documentation and redaction, not credentials.
const MIN_DISTINCT_CHARS: usize = 6;

/// A normalised key never outgrows this: [`MAX_KEY_CHARS`] characters plus an
/// underscore inserted before each camel-case hump.
const NORMALISED_KEY_CAP: usize = MAX_KEY_CHARS * 2;

// --- vocabulary --------------------------------------------------------

/// Key names that mean "the value is a credential".
///
/// Deliberately excludes the bare word `auth`, which would match `author`,
/// `authors` and `authority` in every file carrying a licence header. The
/// authorisation names that matter are spelled out in full instead.
const SECRET_KEY_WORDS: &[&str] = &[
    "secret",
    "token",
    "password",
    "passwd",
    "passphrase",
    "pwd",
    "apikey",
    "api_key",
    "accesskey",
    "access_key",
    "privatekey",
    "private_key",
    "signingkey",
    "signing_key",
    "credential",
    "authorization",
];

/// Key endings that describe a secret rather than being one.
///
/// `token_url`, `api_key_id` and `password_hash` all contain a secret word
/// and none of them holds a live credential. Without this list the scanner
/// flags every OAuth configuration block in the world.
const DENIED_KEY_ENDINGS: &[&str] = &[
    "_id",
    "_url",
    "_uri",
    "_endpoint",
    "_host",
    "_hostname",
    "_port",
    "_path",
    "_dir",
    "_file",
    "_filename",
    "_name",
    "_type",
    "_kind",
    "_class",
    "_length",
    "_len",
    "_size",
    "_count",
    "_ttl",
    "_expiry",
    "_expires",
    "_at",
    "_on",
    "_enabled",
    "_disabled",
    "_required",
    "_header",
    "_prefix",
    "_suffix",
    "_pattern",
    "_regex",
    "_format",
    "_version",
    "_field",
    "_column",
    "_env",
    "_var",
    "_hash",
    "_digest",
    "_algorithm",
    "_alg",
    "_scheme",
    "_method",
    "_label",
    "_description",
    "_message",
    "_error",
];

/// Fragments that mean a value is a stand-in.
///
/// AWS's own documentation key is `AKIAIOSFODNN7EXAMPLE`; without `example`
/// here the scanner cries wolf on half the tutorials on the internet. The
/// risk of the reverse -- a genuine random key happening to contain one of
/// these -- is roughly one in three thousand for a four-character fragment in
/// a forty-character base62 string, which is a price worth paying when a
/// miss costs less than a false alarm.
const PLACEHOLDER_FRAGMENTS: &[&str] = &[
    "example",
    "sample",
    "your",
    "changeme",
    "change_me",
    "change-me",
    "placeholder",
    "dummy",
    "fake",
    "notreal",
    "redact",
    "todo",
    "insert",
    "replace",
    "xxxx",
    "aaaa",
    "0000",
    "1234",
    "abcdef",
    "secret",
    // Covers `password`, `passwd` and the `user:pass@host` that every
    // connection-string example on the internet is written with.
    "pass",
    "token",
    "apikey",
    "api_key",
    "here",
    "test",
    "demo",
    "invalid",
    "hunter2",
    "deadbeef",
];

/// AWS access key ID prefixes. Twenty characters, all upper case, and the
/// four-letter prefix is what makes the rule safe to call `High`.
const AWS_KEY_ID_PREFIXES: &[&str] = &[
    "AKIA", "ASIA", "ABIA", "ACCA", "AGPA", "AIDA", "AIPA", "ANPA", "ANVA", "AROA", "APKA",
];

/// GitHub token prefixes and the shortest tail each must carry.
const GITHUB_PREFIXES: &[(&str, usize)] = &[
    ("ghp_", 36),
    ("gho_", 36),
    ("ghu_", 36),
    ("ghs_", 36),
    ("ghr_", 36),
    ("github_pat_", 40),
];

/// GitLab token prefixes: personal, deploy, runner, CI build, agent,
/// incoming-mail, feed and service-account.
const GITLAB_PREFIXES: &[&str] = &[
    "glpat-", "gldt-", "glrt-", "glcbt-", "glagent-", "glimt-", "glft-", "glsoat-", "glptt-",
];

/// Slack token prefixes: bot, user, app, legacy, refresh, config.
const SLACK_PREFIXES: &[&str] = &[
    "xoxb-", "xoxp-", "xoxa-", "xoxs-", "xoxr-", "xoxe-", "xoxo-",
];

/// URI schemes where a password in the userinfo is unambiguously a
/// credential rather than something a documentation writer typed.
const CREDENTIALED_SCHEMES: &[&str] = &[
    "postgres",
    "postgresql",
    "mysql",
    "mariadb",
    "mongodb",
    "mongodb+srv",
    "redis",
    "rediss",
    "amqp",
    "amqps",
    "ftp",
    "ftps",
    "sftp",
    "ssh",
    "smtp",
    "smtps",
    "imap",
    "imaps",
    "ldap",
    "ldaps",
    "mssql",
    "sqlserver",
    "clickhouse",
    "cassandra",
    "nats",
    "mqtt",
    "jdbc",
    "odbc",
];

/// Words that only appear in a connection string, used to tell
/// `Password=hunter2;` in an ODBC string from `password = "..."` in code.
const CONNECTION_STRING_MARKERS: &[&str] = &[
    "server=",
    "data source=",
    "initial catalog=",
    "database=",
    "driver=",
    "dsn=",
    "user id=",
    "uid=",
    "host=",
    "port=",
    "integrated security=",
    "trusted_connection=",
];

// --- internals ---------------------------------------------------------

/// A finding still expressed in byte offsets within its line.
#[derive(Debug, Clone, Copy)]
struct Raw {
    kind: SecretKind,
    start: usize,
    len: usize,
    confidence: Confidence,
}

/// Convert a line's raw findings into public ones.
///
/// Byte offsets become character columns. The common case -- an ASCII line --
/// needs no conversion at all; a line with multi-byte characters is walked
/// once with a cursor rather than once per finding, so a line carrying many
/// findings still costs a single pass.
fn emit(line: &str, line_number: usize, raw: &[Raw], out: &mut Vec<Finding>) {
    let ascii = line.is_ascii();
    let mut cursor_byte = 0usize;
    let mut cursor_char = 0usize;

    for found in raw {
        let (column, length) = if ascii {
            (found.start + 1, found.len)
        } else {
            cursor_char += line[cursor_byte..found.start].chars().count();
            cursor_byte = found.start;
            (
                cursor_char + 1,
                line[found.start..found.start + found.len].chars().count(),
            )
        };

        out.push(Finding {
            kind: found.kind,
            line: line_number,
            column,
            length,
            confidence: found.confidence,
        });
    }
}

/// Keep one finding per stretch of text, preferring the most specific rule.
///
/// Leaves the survivors sorted by start offset, which [`emit`] relies on.
///
/// The accepted spans live in a map keyed by start offset so that testing a
/// candidate costs two range lookups rather than a walk over everything
/// accepted so far. On an ordinary line the difference is nothing; on a
/// ten-megabyte single line carrying tens of thousands of findings -- a
/// minified bundle, a one-line JSON log -- it is the difference between
/// linear and quadratic.
fn resolve_overlaps(raw: &mut Vec<Raw>) {
    raw.sort_by(|a, b| {
        b.kind
            .specificity()
            .cmp(&a.kind.specificity())
            .then(b.len.cmp(&a.len))
            .then(a.start.cmp(&b.start))
    });

    let mut accepted: BTreeMap<usize, (usize, Raw)> = BTreeMap::new();
    for candidate in raw.iter() {
        let end = candidate.start + candidate.len;
        let clashes_left = accepted
            .range(..=candidate.start)
            .next_back()
            .is_some_and(|(_, (kept_end, _))| *kept_end > candidate.start);
        let clashes_right = accepted
            .range(candidate.start..)
            .next()
            .is_some_and(|(kept_start, _)| *kept_start < end);

        if !clashes_left && !clashes_right {
            accepted.insert(candidate.start, (end, *candidate));
        }
    }

    // A `BTreeMap` iterates in key order, so the survivors come out sorted by
    // start offset with no second sort.
    *raw = accepted.into_values().map(|(_, kept)| kept).collect();
}

/// A byte that can be part of an identifier, for boundary tests. A prefix
/// rule must not fire in the middle of a word: `myghp_token` is a variable
/// name, not a GitHub token.
const fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn at_word_boundary(bytes: &[u8], index: usize) -> bool {
    index == 0 || !is_word_byte(bytes[index - 1])
}

/// Count distinct bytes. A cheap guard against redacted and templated values,
/// which repeat one character.
fn distinct_bytes(value: &[u8]) -> usize {
    let mut seen = [false; 256];
    let mut count = 0usize;
    for byte in value {
        if !seen[*byte as usize] {
            seen[*byte as usize] = true;
            count += 1;
        }
    }
    count
}

/// Shannon entropy in bits per character.
///
/// Random base64 lands near 5.5 bits, hexadecimal near 3.9, English words
/// near 2.5. The thresholds in [`classify_generic`] are set from those.
fn shannon_entropy(value: &[u8]) -> f64 {
    if value.is_empty() {
        return 0.0;
    }
    let mut counts = [0u32; 256];
    for byte in value {
        counts[*byte as usize] += 1;
    }
    let total = value.len() as f64;
    counts
        .iter()
        .filter(|count| **count > 0)
        .map(|count| {
            let p = f64::from(*count) / total;
            -p * p.log2()
        })
        .sum()
}

/// Does this value read as a stand-in rather than a credential?
fn is_placeholder(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    PLACEHOLDER_FRAGMENTS
        .iter()
        .any(|fragment| lower.contains(fragment))
}

/// The guard every recognised-format rule shares: enough variety to be a real
/// key, and no documentation marker in it.
fn looks_like_a_real_value(value: &str) -> bool {
    distinct_bytes(value.as_bytes()) >= MIN_DISTINCT_CHARS && !is_placeholder(value)
}

// --- recognised formats ------------------------------------------------

/// One pass over the line, dispatching on the first byte of each rule.
///
/// Dispatching this way keeps the per-position cost constant no matter how
/// many formats are supported, which is what lets the whole scan stay linear.
fn scan_known_formats(line: &str, out: &mut Vec<Raw>) {
    let bytes = line.as_bytes();
    let mut i = 0usize;

    while i < bytes.len() {
        let matched = match bytes[i] {
            b'A' => match_aws_key_id(line, i),
            b'g' => match_github(line, i).or_else(|| match_gitlab(line, i)),
            b'x' => match_slack(line, i),
            b'e' => match_jwt(line, i),
            b'-' => match_pem_marker(line, i),
            b':' => match_credentialed_uri(line, i),
            _ => None,
        };

        match matched {
            Some(found) => {
                let next = found.start + found.len;
                out.push(found);
                i = next.max(i + 1);
            }
            None => i += 1,
        }
    }
}

/// The longest run of bytes from `start` that all satisfy `allowed`.
fn run_length(bytes: &[u8], start: usize, allowed: fn(u8) -> bool) -> usize {
    let mut end = start;
    while end < bytes.len() && allowed(bytes[end]) {
        end += 1;
    }
    end - start
}

const fn is_base62(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
}

const fn is_base64url(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_'
}

const fn is_token_tail(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_'
}

const AWS_KEY_ID_LEN: usize = 20;

fn match_aws_key_id(line: &str, at: usize) -> Option<Raw> {
    let bytes = line.as_bytes();
    if !at_word_boundary(bytes, at) {
        return None;
    }
    let prefix = AWS_KEY_ID_PREFIXES
        .iter()
        .find(|prefix| line[at..].starts_with(**prefix))?;

    // Exactly twenty characters, all upper-case alphanumeric, with nothing
    // word-like immediately after: a longer run is a base64 blob that happens
    // to open with those four letters.
    let end = at + AWS_KEY_ID_LEN;
    if end > bytes.len() {
        return None;
    }
    let tail = &bytes[at + prefix.len()..end];
    if tail
        .iter()
        .any(|b| !b.is_ascii_uppercase() && !b.is_ascii_digit())
    {
        return None;
    }
    if end < bytes.len() && is_word_byte(bytes[end]) {
        return None;
    }
    if !looks_like_a_real_value(&line[at..end]) {
        return None;
    }

    Some(Raw {
        kind: SecretKind::AwsAccessKeyId,
        start: at,
        len: AWS_KEY_ID_LEN,
        confidence: Confidence::High,
    })
}

fn match_github(line: &str, at: usize) -> Option<Raw> {
    let bytes = line.as_bytes();
    if !at_word_boundary(bytes, at) {
        return None;
    }
    let (prefix, minimum) = GITHUB_PREFIXES
        .iter()
        .find(|(prefix, _)| line[at..].starts_with(prefix))?;

    // Fine-grained tokens carry underscores in the body; classic ones do not.
    let allowed: fn(u8) -> bool = if *prefix == "github_pat_" {
        is_token_tail
    } else {
        is_base62
    };
    let body = run_length(bytes, at + prefix.len(), allowed);
    if body < *minimum {
        return None;
    }

    let len = prefix.len() + body;
    if !looks_like_a_real_value(&line[at..at + len]) {
        return None;
    }

    Some(Raw {
        kind: SecretKind::GitHubToken,
        start: at,
        len,
        confidence: Confidence::High,
    })
}

fn match_gitlab(line: &str, at: usize) -> Option<Raw> {
    let bytes = line.as_bytes();
    if !at_word_boundary(bytes, at) {
        return None;
    }
    let prefix = GITLAB_PREFIXES
        .iter()
        .find(|prefix| line[at..].starts_with(**prefix))?;

    let body = run_length(bytes, at + prefix.len(), is_token_tail);
    if body < 20 {
        return None;
    }

    let len = prefix.len() + body;
    if !looks_like_a_real_value(&line[at..at + len]) {
        return None;
    }

    Some(Raw {
        kind: SecretKind::GitLabToken,
        start: at,
        len,
        confidence: Confidence::High,
    })
}

fn match_slack(line: &str, at: usize) -> Option<Raw> {
    let bytes = line.as_bytes();
    if !at_word_boundary(bytes, at) {
        return None;
    }
    let prefix = SLACK_PREFIXES
        .iter()
        .find(|prefix| line[at..].starts_with(**prefix))?;

    let body = run_length(bytes, at + prefix.len(), is_token_tail);
    if body < 10 {
        return None;
    }

    let len = prefix.len() + body;
    if !looks_like_a_real_value(&line[at..at + len]) {
        return None;
    }

    Some(Raw {
        kind: SecretKind::SlackToken,
        start: at,
        len,
        confidence: Confidence::High,
    })
}

/// Decode unpadded base64url.
///
/// Only ever called on a JWT header, which is short. Returning `None` for
/// anything malformed is the point: it is what turns "three dot-separated
/// base64ish runs", which minified code produces by accident, into "a JOSE
/// header", which nothing produces by accident.
fn decode_base64url(segment: &str) -> Option<Vec<u8>> {
    if segment.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(segment.len() * 3 / 4);
    let mut accumulator = 0u32;
    let mut bits = 0u32;

    for byte in segment.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => return None,
        };
        accumulator = (accumulator << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((accumulator >> bits) & 0xFF) as u8);
        }
    }

    Some(out)
}

/// Longest JWT header we will decode. A JOSE header carrying a whole
/// certificate chain is not something a text editor needs to recognise, and
/// the bound keeps the work per position constant.
const MAX_JWT_HEADER_BYTES: usize = 1024;

fn match_jwt(line: &str, at: usize) -> Option<Raw> {
    let bytes = line.as_bytes();
    if !at_word_boundary(bytes, at) || !line[at..].starts_with("eyJ") {
        return None;
    }

    let header_len = run_length(bytes, at, is_base64url);
    if !(8..=MAX_JWT_HEADER_BYTES).contains(&header_len) {
        return None;
    }
    let mut cursor = at + header_len;
    if bytes.get(cursor) != Some(&b'.') {
        return None;
    }
    cursor += 1;

    let payload_len = run_length(bytes, cursor, is_base64url);
    if payload_len < 8 {
        return None;
    }
    cursor += payload_len;
    if bytes.get(cursor) != Some(&b'.') {
        return None;
    }
    cursor += 1;

    // The signature may be empty (`alg: none`), but a fourth segment means
    // this is something else -- JWE has five -- and guessing is how a scanner
    // starts flagging version strings.
    cursor += run_length(bytes, cursor, is_base64url);
    if bytes.get(cursor) == Some(&b'.') {
        return None;
    }

    // The header must actually be a JOSE header. Every JWS header carries an
    // `alg` member because the specification requires it, so this one check
    // removes essentially every accidental match.
    let header = decode_base64url(&line[at..at + header_len])?;
    let header = std::str::from_utf8(&header).ok()?;
    if !header.starts_with('{') || !header.contains("\"alg\"") {
        return None;
    }

    Some(Raw {
        kind: SecretKind::JsonWebToken,
        start: at,
        len: cursor - at,
        confidence: Confidence::High,
    })
}

/// Longest `-----BEGIN ...-----` marker we will look for a closing dash run
/// in. Real ones are well under forty characters.
const MAX_PEM_MARKER_BYTES: usize = 96;

fn match_pem_marker(line: &str, at: usize) -> Option<Raw> {
    if !line[at..].starts_with("-----BEGIN") {
        return None;
    }
    let body_start = at + "-----BEGIN".len();
    let window_end = (body_start + MAX_PEM_MARKER_BYTES).min(line.len());
    let window = line.get(body_start..window_end)?;
    let close = window.find("-----")?;

    // `CERTIFICATE` and `PUBLIC KEY` blocks are meant to be published.
    // Flagging them is the fastest way to teach a user to ignore this tool.
    if !window[..close].contains("PRIVATE KEY") {
        return None;
    }

    Some(Raw {
        kind: SecretKind::PrivateKeyBlock,
        start: at,
        len: body_start + close + "-----".len() - at,
        confidence: Confidence::High,
    })
}

const fn is_scheme_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'+' || byte == b'.' || byte == b'-'
}

/// Longest authority we will read out of a URI. Past this it is not a host.
const MAX_AUTHORITY_BYTES: usize = 512;

/// Match `scheme://user:password@host`, anchored on the `://`.
///
/// The userinfo cannot contain `/`, so stopping the search for `@` at the
/// first slash is what keeps `http://host:8080/a@b` from reading as a
/// credential.
fn match_credentialed_uri(line: &str, at: usize) -> Option<Raw> {
    let bytes = line.as_bytes();
    if !line[at..].starts_with("://") {
        return None;
    }

    let mut scheme_start = at;
    while scheme_start > 0
        && is_scheme_byte(bytes[scheme_start - 1])
        && at - scheme_start < MAX_KEY_CHARS
    {
        scheme_start -= 1;
    }
    if scheme_start == at || !bytes[scheme_start].is_ascii_alphabetic() {
        return None;
    }
    let scheme = line[scheme_start..at].to_ascii_lowercase();

    let authority_start = at + "://".len();
    let authority_end = (authority_start + MAX_AUTHORITY_BYTES).min(bytes.len());
    let mut cursor = authority_start;
    let mut at_sign = None;
    while cursor < authority_end {
        match bytes[cursor] {
            b'/' | b'?' | b'#' | b'"' | b'\'' | b'`' | b'<' | b'>' | b' ' | b'\t' => break,
            b'@' => at_sign = Some(cursor),
            _ => {}
        }
        cursor += 1;
    }
    let at_sign = at_sign?;

    let userinfo = &line[authority_start..at_sign];
    let colon = userinfo.find(':')?;
    let password = &userinfo[colon + 1..];
    if password.len() < 3 || password.len() > MAX_VALUE_BYTES || is_placeholder(password) {
        return None;
    }

    // A scheme that carries data carries real credentials; `http` is reported
    // a notch lower because a URL in prose is far more often an illustration.
    // An unrecognised scheme is not reported at all.
    let confidence = if CREDENTIALED_SCHEMES.contains(&scheme.as_str()) {
        Confidence::High
    } else if scheme == "http" || scheme == "https" {
        Confidence::Medium
    } else {
        return None;
    };

    Some(Raw {
        kind: SecretKind::ConnectionStringPassword,
        start: authority_start + colon + 1,
        len: password.len(),
        confidence,
    })
}

// --- assignments -------------------------------------------------------

const fn is_key_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-' || byte == b'.'
}

/// Is there an assignment separator starting at `index`, and where does it
/// end?
///
/// `:` only counts when whitespace or a quote follows, which is what tells
/// YAML and JSON apart from `http://`, `12:30` and a Rust path.
fn separator_at(bytes: &[u8], index: usize) -> Option<usize> {
    match bytes[index] {
        b'=' => {
            let previous = if index == 0 { b' ' } else { bytes[index - 1] };
            if matches!(
                previous,
                b'=' | b'!' | b'<' | b'>' | b'+' | b'-' | b'*' | b'/' | b'%'
            ) {
                return None;
            }
            match bytes.get(index + 1) {
                Some(b'=') => None,
                // Ruby's hash rocket, and nothing else.
                Some(b'>') => Some(index + 2),
                _ => Some(index + 1),
            }
        }
        b':' => match bytes.get(index + 1) {
            Some(b' ' | b'\t' | b'"' | b'\'') => Some(index + 1),
            Some(b'=') => Some(index + 2),
            _ => None,
        },
        _ => None,
    }
}

/// The key name immediately left of a separator, if there is one.
fn key_before(line: &str, separator_start: usize) -> Option<&str> {
    let bytes = line.as_bytes();
    let mut end = separator_start;
    while end > 0 && matches!(bytes[end - 1], b' ' | b'\t') {
        end -= 1;
    }
    if end > 0 && matches!(bytes[end - 1], b'"' | b'\'') {
        end -= 1;
    }
    let mut start = end;
    while start > 0 && is_key_byte(bytes[start - 1]) && end - start < MAX_KEY_CHARS {
        start -= 1;
    }
    (start < end).then(|| &line[start..end])
}

const fn is_value_terminator(byte: u8) -> bool {
    matches!(
        byte,
        b' ' | b'\t'
            | b','
            | b';'
            | b')'
            | b']'
            | b'}'
            | b'&'
            | b'#'
            | b'"'
            | b'\''
            | b'`'
            | b'|'
            | b'<'
            | b'>'
            | b'\\'
    )
}

/// The value immediately right of a separator: its byte offset and its text.
///
/// Handles a quoted value and a bare one. Both searches are bounded, so a
/// line full of unterminated quotes costs the same as any other line.
fn value_after(line: &str, separator_end: usize) -> Option<(usize, &str)> {
    let bytes = line.as_bytes();
    let mut start = separator_end;
    while start < bytes.len() && matches!(bytes[start], b' ' | b'\t') {
        start += 1;
    }
    if start >= bytes.len() {
        return None;
    }

    if matches!(bytes[start], b'"' | b'\'') {
        let quote = bytes[start];
        start += 1;
        let limit = (start + MAX_VALUE_BYTES + 1).min(bytes.len());
        let mut end = start;
        while end < limit && bytes[end] != quote {
            end += 1;
        }
        // An unterminated quote inside the budget is not a value we can read.
        if end >= limit || end == start {
            return None;
        }
        return Some((start, &line[start..end]));
    }

    let limit = (start + MAX_VALUE_BYTES + 1).min(bytes.len());
    let mut end = start;
    while end < limit && !is_value_terminator(bytes[end]) {
        end += 1;
    }
    (end > start).then(|| (start, &line[start..end]))
}

/// Fold a key name into one shape: lower case, underscore-separated, with
/// camel-case humps split so `apiKeyUrl` and `api_key_url` are denied by the
/// same rule.
///
/// Writes into a caller-owned buffer because this runs once per candidate
/// assignment; allocating here would show up in a scan of a large file.
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

fn key_names_a_secret(key: &str) -> bool {
    if DENIED_KEY_ENDINGS
        .iter()
        .any(|ending| key.ends_with(ending))
    {
        return false;
    }
    SECRET_KEY_WORDS.iter().any(|word| key.contains(word))
}

/// Is this line an ODBC or ADO.NET style connection string?
///
/// Checked only after a password-shaped key has already matched, and only
/// over the head of the line, so the lower-casing never costs more than a
/// constant.
fn looks_like_a_connection_string(line: &str) -> bool {
    let mut cut = line.len().min(4096);
    while !line.is_char_boundary(cut) {
        cut -= 1;
    }
    let head = line[..cut].to_ascii_lowercase();
    if !head.contains(';') {
        return false;
    }
    CONNECTION_STRING_MARKERS
        .iter()
        .any(|marker| head.contains(marker))
}

fn scan_assignments(line: &str, out: &mut Vec<Raw>) {
    let bytes = line.as_bytes();
    let mut i = 0usize;

    while i < bytes.len() {
        let Some(separator_end) = separator_at(bytes, i) else {
            i += 1;
            continue;
        };
        let Some((start, value)) = value_after(line, separator_end) else {
            i = separator_end;
            continue;
        };

        if let Some(key) = key_before(line, i)
            && let Some((kind, confidence)) = classify_assignment(line, key, value)
        {
            out.push(Raw {
                kind,
                start,
                len: value.len(),
                confidence,
            });
        }

        // Never rescan inside a value already read: that is what keeps a line
        // of a thousand assignments linear rather than squared.
        i = (start + value.len()).max(separator_end);
    }
}

/// Decide what, if anything, an assignment is.
///
/// The cheap disqualifications come first so that the overwhelming majority
/// of assignments in a source file -- `let x = compute()` -- cost only a few
/// byte comparisons.
fn classify_assignment(line: &str, raw_key: &str, value: &str) -> Option<(SecretKind, Confidence)> {
    if value.len() < 4 || value.len() > MAX_VALUE_BYTES {
        return None;
    }
    // A credential is printable ASCII with no spaces. Anything else is prose,
    // a path, an interpolation or a byte string.
    if value.bytes().any(|b| !(0x21..=0x7E).contains(&b)) {
        return None;
    }
    if value.starts_with('[') || value.starts_with('{') || value.starts_with('(') {
        return None;
    }
    // `$` means a shell or template interpolation, and a URI is the other
    // rule's business.
    if value.contains('$') || value.contains("://") {
        return None;
    }

    let mut buffer = [0u8; NORMALISED_KEY_CAP];
    let len = normalise_key(raw_key, &mut buffer);
    let key = std::str::from_utf8(&buffer[..len]).ok()?;
    if !key_names_a_secret(key) {
        return None;
    }

    // `Password=hunter2;` in a connection string needs no entropy argument:
    // the surrounding `Server=...;Database=...;` is the evidence, and a
    // database password is often short and memorable.
    let password_key = key.contains("password") || key.contains("passwd") || key.contains("pwd");
    if password_key && !is_placeholder(value) && looks_like_a_connection_string(line) {
        return Some((SecretKind::ConnectionStringPassword, Confidence::High));
    }

    classify_generic(key, value)
}

/// The last resort: no vendor prefix, no structure, just a name that says
/// "secret" and a value that looks random.
///
/// The three tiers exist so a caller can choose its own appetite. Nothing
/// here is `High` on the strength of the name alone -- that needs a value
/// long and random enough that an innocent explanation is a stretch.
fn classify_generic(key: &str, value: &str) -> Option<(SecretKind, Confidence)> {
    if value.len() < MIN_GENERIC_VALUE_BYTES {
        return None;
    }
    if distinct_bytes(value.as_bytes()) < MIN_DISTINCT_CHARS {
        return None;
    }
    if is_placeholder(value) {
        return None;
    }

    // At least two of the three classes. A run of one class is a word, a hash
    // label or a base32 identifier far more often than it is a key.
    let classes = u8::from(value.bytes().any(|b| b.is_ascii_lowercase()))
        + u8::from(value.bytes().any(|b| b.is_ascii_uppercase()))
        + u8::from(value.bytes().any(|b| b.is_ascii_digit()));
    if classes < 2 {
        return None;
    }

    let entropy = shannon_entropy(value.as_bytes());
    let confidence = if value.len() >= 32 && entropy >= 4.5 {
        Confidence::High
    } else if value.len() >= 20 && entropy >= 4.0 {
        Confidence::Medium
    } else if entropy >= 3.5 {
        Confidence::Low
    } else {
        return None;
    };

    // An AWS secret key has no prefix to anchor on -- it is forty base64
    // characters and nothing else -- so the name has to do the anchoring, and
    // only a name that says AWS makes that safe.
    let aws = key.contains("aws")
        && value.len() == 40
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'/' || b == b'+' || b == b'=');
    if aws {
        return Some((SecretKind::AwsSecretAccessKey, Confidence::High));
    }

    Some((SecretKind::GenericSecretAssignment, confidence))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Fabricated credentials, well-formed but never issued. They deliberately
    // avoid the words in PLACEHOLDER_FRAGMENTS, because the scanner is
    // supposed to ignore anything that reads like documentation and these
    // fixtures need to reach the rules under test.
    const AWS_KEY_ID: &str = "AKIA3G7QVHBRN2WPKZ5F";
    const AWS_SECRET: &str = "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";
    const GITHUB_TOKEN: &str = "ghp_R2d9KpXvA7mQzL3nB8wYtE6sJ1uH0cVfNgD4";
    const GITLAB_TOKEN: &str = "glpat-7fKq2mZxR9vLpN3wBtY6";
    const SLACK_TOKEN: &str = "xoxb-5839274016-5839274016482-Rq7ZmXvT3nB9wYtE6sJ1uHpK";
    const JWT: &str = concat!(
        "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.",
        "eyJzdWIiOiI5OTg4Nzc2NjU1IiwibmFtZSI6IkEgQiIsImlhdCI6MTUxNjIzOTAyMn0.",
        "SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c"
    );

    /// The single finding a fixture is expected to produce.
    ///
    /// Deliberately does not print the scanned text on failure: even in a
    /// test, a scanner that puts credentials in its output is teaching the
    /// wrong habit.
    fn single(text: &str) -> Finding {
        let findings = scan(text);
        assert_eq!(findings.len(), 1, "expected one finding, got {findings:?}");
        findings[0]
    }

    /// The 1-based character column a needle starts at, computed rather than
    /// counted by hand so the expectations cannot drift.
    fn column_of(line: &str, needle: &str) -> usize {
        let byte = line.find(needle).expect("needle is in the line");
        line[..byte].chars().count() + 1
    }

    #[test]
    fn an_empty_document_has_no_findings() {
        assert!(scan("").is_empty());
        assert!(scan("\n\n\n").is_empty());
        assert!(scan("   \t  ").is_empty());
    }

    #[test]
    fn an_aws_access_key_id_is_found_at_its_exact_position() {
        let line = format!("aws_access_key_id = {AWS_KEY_ID}");
        let found = single(&line);

        assert_eq!(found.kind, SecretKind::AwsAccessKeyId);
        assert_eq!(found.confidence, Confidence::High);
        assert_eq!(found.line, 1);
        assert_eq!(found.column, column_of(&line, AWS_KEY_ID));
        assert_eq!(found.length, 20, "an access key ID is always twenty");
    }

    #[test]
    fn an_aws_secret_key_is_found_only_when_the_name_says_aws() {
        let named = format!("AWS_SECRET_ACCESS_KEY={AWS_SECRET}");
        assert_eq!(single(&named).kind, SecretKind::AwsSecretAccessKey);

        // Forty base64 characters on their own are a hash, a slice of a
        // certificate, or nothing at all. Without the name there is no case.
        assert!(
            scan(AWS_SECRET).is_empty(),
            "a bare forty-character string is not evidence of anything"
        );
    }

    #[test]
    fn github_gitlab_and_slack_tokens_are_found_by_their_prefixes() {
        for (token, kind) in [
            (GITHUB_TOKEN, SecretKind::GitHubToken),
            (GITLAB_TOKEN, SecretKind::GitLabToken),
            (SLACK_TOKEN, SecretKind::SlackToken),
        ] {
            let line = format!("curl -H \"X-Auth: {token}\" https://example.invalid");
            let found = single(&line);
            assert_eq!(found.kind, kind);
            assert_eq!(found.confidence, Confidence::High);
            assert_eq!(found.column, column_of(&line, token));
            assert_eq!(found.length, token.chars().count());
        }
    }

    #[test]
    fn a_vendor_prefix_inside_a_word_is_not_a_token() {
        // `myghp_...` is a variable name. Firing here is how a scanner starts
        // flagging identifiers.
        let line = format!("let myghp_{} = 1;", &GITHUB_TOKEN[4..]);
        let findings = scan(&line);
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn a_jwt_is_found_and_reported_as_a_jwt_not_as_a_long_string() {
        let line = format!("Authorization: Bearer {JWT}");
        let found = single(&line);

        assert_eq!(found.kind, SecretKind::JsonWebToken);
        assert_eq!(found.confidence, Confidence::High);
        assert_eq!(found.column, column_of(&line, JWT));
        assert_eq!(found.length, JWT.len());
    }

    #[test]
    fn three_dot_separated_runs_are_not_a_jwt_unless_the_header_decodes() {
        // `eyJhello` decodes to `{"azYh`, which is JSON-shaped and still has
        // no `alg` member. Every JWS header has one; that is the whole test.
        // Under a secret-sounding name the generic rule may still have a view
        // of it, and that is fine -- what must not happen is the scanner
        // claiming to have recognised a token format it has not.
        let findings = scan("api_token = \"eyJhello.eyJworld.eyJagain\"");
        assert!(
            findings.iter().all(|f| f.kind != SecretKind::JsonWebToken),
            "{findings:?}"
        );
        assert!(scan("const chunk = \"eyJhello.eyJworld.eyJagain\";").is_empty());

        // A fourth segment is something else entirely.
        let four = format!("{JWT}.eyJmb3Vy");
        let findings = scan(&four);
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn a_private_key_block_is_flagged_but_a_certificate_is_not() {
        let marker = "-----BEGIN RSA PRIVATE KEY-----";
        let text = format!("{marker}\nMIIEowIBAAKCAQEAx7Vn9Qm2\n-----END RSA PRIVATE KEY-----\n");
        let found = single(&text);

        assert_eq!(found.kind, SecretKind::PrivateKeyBlock);
        assert_eq!(found.line, 1);
        assert_eq!(found.column, 1);
        assert_eq!(found.length, marker.chars().count());

        for public in [
            "-----BEGIN CERTIFICATE-----",
            "-----BEGIN PUBLIC KEY-----",
            "-----BEGIN CERTIFICATE REQUEST-----",
        ] {
            assert!(scan(public).is_empty(), "{public} is meant to be published");
        }
    }

    #[test]
    fn every_pem_private_key_flavour_is_recognised() {
        for marker in [
            "-----BEGIN PRIVATE KEY-----",
            "-----BEGIN RSA PRIVATE KEY-----",
            "-----BEGIN EC PRIVATE KEY-----",
            "-----BEGIN DSA PRIVATE KEY-----",
            "-----BEGIN OPENSSH PRIVATE KEY-----",
            "-----BEGIN ENCRYPTED PRIVATE KEY-----",
            "-----BEGIN PGP PRIVATE KEY BLOCK-----",
        ] {
            assert_eq!(single(marker).kind, SecretKind::PrivateKeyBlock, "{marker}");
        }
    }

    #[test]
    fn a_password_in_a_database_uri_is_flagged_and_the_span_covers_only_it() {
        let password = "Rk7mZq3XvB9n";
        let line = format!("DATABASE_URL=postgres://svcuser:{password}@db.internal:5432/orders");
        let found = single(&line);

        assert_eq!(found.kind, SecretKind::ConnectionStringPassword);
        assert_eq!(found.confidence, Confidence::High);
        assert_eq!(found.column, column_of(&line, password));
        assert_eq!(
            found.length,
            password.len(),
            "the span is the password, not the whole URI"
        );
    }

    #[test]
    fn a_uri_without_a_password_is_not_a_credential() {
        for line in [
            "https://api.example.invalid/v1/repos?state=open&per_page=100",
            "ssh://git@github.com/octocat/hello-world.git",
            "http://localhost:8080/inbox/a@b",
            "postgres://svcuser@db.internal:5432/orders",
        ] {
            assert!(scan(line).is_empty(), "{line}");
        }
    }

    #[test]
    fn an_odbc_connection_string_password_needs_no_entropy_argument() {
        // `Server=...;Database=...;` is the evidence. A database password is
        // often short and memorable, and demanding randomness here would miss
        // every one of them.
        let line = "Server=tcp:db01;Database=Orders;User Id=svc;Password=Rk7mZq3XvB9n;";
        let found = single(line);

        assert_eq!(found.kind, SecretKind::ConnectionStringPassword);
        assert_eq!(found.confidence, Confidence::High);
        assert_eq!(found.column, column_of(line, "Rk7mZq3XvB9n"));
    }

    #[test]
    fn a_high_entropy_value_under_a_secret_name_is_reported_by_confidence_tier() {
        // Sixteen characters is the floor: enough for entropy to mean
        // something, not enough to be sure of it.
        assert_eq!(
            single("api_key = \"Xk92mQvRt7LpZn3B\"").confidence,
            Confidence::Low
        );
        assert_eq!(
            single("api_key = \"Xk92mQvRt7LpZn3BwYs6\"").confidence,
            Confidence::Medium
        );

        let found = single("client_secret = \"Xk92mQvRt7LpZn3BwYs6HjD4fG8kMc1Vb5N\"");
        assert_eq!(found.kind, SecretKind::GenericSecretAssignment);
        assert_eq!(found.confidence, Confidence::High);
    }

    #[test]
    fn a_value_one_character_short_of_the_floor_is_not_reported() {
        assert_eq!(MIN_GENERIC_VALUE_BYTES, 16);
        assert!(scan("api_key = \"Xk92mQvRt7LpZn3\"").is_empty(), "fifteen");
        assert_eq!(scan("api_key = \"Xk92mQvRt7LpZn3B\"").len(), 1, "sixteen");
    }

    #[test]
    fn a_secret_recognised_by_two_rules_is_reported_once_by_the_specific_one() {
        let line = format!("GITHUB_TOKEN={GITHUB_TOKEN}");
        assert_eq!(
            single(&line).kind,
            SecretKind::GitHubToken,
            "not an anonymous high-entropy string"
        );

        let line = format!("api_token = \"{JWT}\"");
        assert_eq!(single(&line).kind, SecretKind::JsonWebToken);
    }

    /// Text that must never be flagged.
    ///
    /// This is the corpus that decides whether the feature survives contact
    /// with a real document. Every entry is something that turns up in
    /// ordinary files and looks, to a naive scanner, exactly like a secret.
    const NOT_SECRETS: &[&str] = &[
        // A base64-encoded image: the classic long-string false positive.
        "const logo = \"data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==\";",
        // Git object names, which are exactly the length people expect a
        // secret to be.
        "commit = a94a8fe5ccb19ba61c4c0873d391e987982fbbd3",
        "Fixes: 3f2b1c8d9e0a4b5c6d7e8f9a0b1c2d3e4f5a6b7c",
        "checksum = \"3f2b1c8d9e0a4b5c6d7e8f9a0b1c2d3e4f5a6b7c8d9e0f1a2b3c4d5e6f7a8b9c\"",
        "sha256 = \"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\"",
        // A UUID, in the place UUIDs actually appear.
        "session_id = \"550e8400-e29b-41d4-a716-446655440000\"",
        "  550e8400-e29b-41d4-a716-446655440000",
        // A long URL with query parameters.
        "https://api.github.com/repos/octocat/hello-world/issues?state=open&per_page=100&sort=created",
        // Minified JavaScript.
        "!function(e,t){\"object\"==typeof exports?t(exports):t(e.lib={})}(this,function(e){\"use strict\";e.version=\"4.17.21\"});",
        // A lockfile integrity hash.
        "  \"integrity\": \"sha512-uJvDkwl0DRcpUE6RxTfIaKmXdVCLnT8AbCJVFYFqLMwXm8Qb5rDLPuMTVLQeaAiFYuFqKZ3q1fXNQxKlPvJqfw==\",",
        // Documentation placeholders, which outnumber real secrets by orders
        // of magnitude.
        "api_key = \"your-api-key-here\"",
        "AWS_SECRET_ACCESS_KEY=<your secret access key>",
        "aws_access_key_id = AKIAIOSFODNN7EXAMPLE",
        "GITHUB_TOKEN=ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
        "export API_TOKEN=$API_TOKEN",
        "password = \"hunter2\"",
        "postgres://user:pass@localhost:5432/mydb",
        // Configuration that names a secret without holding one.
        "token_url = \"https://login.example.invalid/oauth2/v2.0/token\"",
        "private_key_path = \"/etc/ssl/private/server.key\"",
        "secret_name = \"prod-database-credentials\"",
        "password_hash = \"$2b$12$Kj8mQvRt7LpZn3BwYs6HjD4fG8kMc1Vb5NxAe0Tu\"",
        "token_type: \"Bearer\"",
        "client_id = \"9f8e7d6c5b4a39281706f5e4d3c2b1a0\"",
        "  \"secretRef\": { \"name\": \"db-credentials\", \"key\": \"password\" },",
        // Markup and prose that mention the words.
        "  <input type=\"password\" name=\"password\" autocomplete=\"current-password\">",
        "The API key is stored in the operating system keychain, never in the note.",
        "| password | the passphrase used to unlock the document |",
        "last_password_change: 2026-08-19T14:32:07+01:00",
        // Ordinary Rust, which is why `auth` is not a secret-name word.
        "    let authors = vec![\"Daniel Boles\"];",
        "/// Returns the author of the document.",
        "    let authority = Authority::from_str(\"login.example.invalid\")?;",
        // Windows paths and CSS, which both contain colons.
        "path = \"C:\\\\Users\\\\danie\\\\AppData\\\\Local\"",
        "  --brand-amber: #ffb347;",
        // A Slack link, not a Slack token.
        "See https://example.slack.com/archives/C01234567/p1699999999",
        // A base64 blob whose prefix is JWT-shaped and whose header is not.
        "const chunk = \"eyJhello.eyJworld.eyJagain\";",
    ];

    #[test]
    fn the_false_positive_corpus_produces_nothing_at_all() {
        for line in NOT_SECRETS {
            let findings = scan(line);
            assert!(findings.is_empty(), "flagged {line}\n  as {findings:?}");
        }
        // And the whole corpus as one document, in case a rule only fires
        // with a neighbour on the line above.
        let document = NOT_SECRETS.join("\n");
        let findings = scan(&document);
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn findings_arrive_sorted_by_line_and_then_column() {
        let text = format!(
            "nothing here\napi_key = \"Xk92mQvRt7LpZn3BwYs6\"\nid {AWS_KEY_ID} and {GITHUB_TOKEN}\n"
        );
        let findings = scan(&text);

        assert_eq!(findings.len(), 3);
        assert_eq!(findings[0].line, 2);
        assert_eq!(findings[1].line, 3);
        assert_eq!(findings[2].line, 3);
        assert!(findings[1].column < findings[2].column);
        assert_eq!(findings[1].kind, SecretKind::AwsAccessKeyId);
        assert_eq!(findings[2].kind, SecretKind::GitHubToken);
    }

    #[test]
    fn a_crlf_document_reports_the_same_positions_as_its_lf_equivalent() {
        let lf = "first\napi_key = \"Xk92mQvRt7LpZn3BwYs6\"\n";
        let crlf = lf.replace('\n', "\r\n");
        assert_eq!(scan(lf), scan(&crlf), "\\r is encoding, not content");
    }

    #[test]
    fn columns_count_characters_rather_than_bytes() {
        // A column that counted bytes would put the caret in the wrong place
        // on every line with an emoji or an accent in it.
        let line = "\u{1f511} api_key = \"Xk92mQvRt7LpZn3BwYs6\"";
        let found = single(line);

        assert_eq!(found.column, column_of(line, "Xk92"));
        assert_eq!(found.length, 20);
        assert!(
            found.column < line.find("Xk92").expect("needle") + 1,
            "the byte offset is larger, which is exactly the bug"
        );
    }

    #[test]
    fn a_finding_never_points_outside_the_line_it_is_on() {
        let text = format!("\u{e9}\u{e9}\u{e9} {GITHUB_TOKEN} \u{e9}\n{AWS_KEY_ID}\n");
        for found in scan(&text) {
            let line = text.lines().nth(found.line - 1).expect("line exists");
            assert!(found.column >= 1);
            assert!(
                found.column - 1 + found.length <= line.chars().count(),
                "{found:?} runs off the end of its line"
            );
        }
    }

    #[test]
    fn scanning_the_same_text_twice_gives_the_same_answer() {
        let text = format!("{GITHUB_TOKEN} {AWS_KEY_ID} {SLACK_TOKEN}");
        let first = scan(&text);
        for _ in 0..5 {
            assert_eq!(scan(&text), first, "the scan reads no clock and no state");
        }
    }

    #[test]
    fn every_kind_has_a_label_and_confidence_is_ordered_low_to_high() {
        for kind in [
            SecretKind::AwsAccessKeyId,
            SecretKind::AwsSecretAccessKey,
            SecretKind::GitHubToken,
            SecretKind::GitLabToken,
            SecretKind::SlackToken,
            SecretKind::PrivateKeyBlock,
            SecretKind::JsonWebToken,
            SecretKind::ConnectionStringPassword,
            SecretKind::GenericSecretAssignment,
        ] {
            assert!(!kind.label().is_empty(), "{kind:?}");
        }

        assert!(Confidence::Low < Confidence::Medium);
        assert!(Confidence::Medium < Confidence::High);
    }

    #[test]
    fn the_vocabulary_lists_are_lower_case_and_free_of_duplicates() {
        // A stray upper-case entry would silently never match, and a
        // duplicate is a sign someone edited the list twice.
        for (name, list) in [
            ("secret words", SECRET_KEY_WORDS),
            ("denied endings", DENIED_KEY_ENDINGS),
            ("placeholders", PLACEHOLDER_FRAGMENTS),
            ("schemes", CREDENTIALED_SCHEMES),
            ("connection markers", CONNECTION_STRING_MARKERS),
        ] {
            for entry in list {
                assert!(
                    entry.chars().all(|c| !c.is_ascii_uppercase()),
                    "{name}: {entry} would never match"
                );
            }
            let mut sorted = list.to_vec();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(sorted.len(), list.len(), "{name} has a duplicate");
        }

        for ending in DENIED_KEY_ENDINGS {
            assert!(ending.starts_with('_'), "{ending} is not a word ending");
        }
    }

    #[test]
    fn entropy_separates_random_strings_from_words() {
        assert!(shannon_entropy(b"") < 0.001, "nothing has no entropy");
        assert!(shannon_entropy(b"aaaaaaaaaaaaaaaa") < 0.001, "one symbol");
        assert!(shannon_entropy(b"the quick brown fox jumps") < 4.5);
        assert!(shannon_entropy(b"Xk92mQvRt7LpZn3BwYs6HjD4fG8kMc1Vb5N") > 4.5);
    }

    #[test]
    fn base64url_decoding_refuses_what_is_not_base64url() {
        assert_eq!(decode_base64url("eyJ9").as_deref(), Some(&b"{\"}"[..]));
        assert!(decode_base64url("not base64!").is_none(), "space and bang");
        assert!(decode_base64url("eyJhb").is_none(), "a length of 4n+1");
        assert!(decode_base64url("").is_some(), "nothing decodes to nothing");
    }

    #[test]
    fn a_line_of_a_thousand_assignments_is_scanned_once_not_a_thousand_times() {
        // Not a timing assertion -- a correctness one. If the value scan did
        // not skip past what it had already read, this line would be walked
        // repeatedly and the same secret reported more than once.
        let line = "k = \"Xk92mQvRt7LpZn3BwYs6\", ".repeat(1000);
        assert!(scan(&line).is_empty(), "`k` names nothing");

        let line = "api_key = \"Xk92mQvRt7LpZn3BwYs6\", ".repeat(100);
        assert_eq!(scan(&line).len(), 100, "one finding each, no duplicates");
    }

    #[test]
    fn odd_input_is_scanned_without_panicking() {
        for text in [
            "",
            "\n",
            "=",
            ":",
            "-----BEGIN",
            "eyJ",
            "://",
            "a=\"",
            "\u{0}\u{1}\u{7f}",
            "\u{65e5}\u{672c}\u{8a9e}=\u{30c6}\u{30ad}\u{30b9}\u{30c8}",
            &"=".repeat(10_000),
            &"a".repeat(10_000),
        ] {
            let _ = scan(text);
        }
    }
}
