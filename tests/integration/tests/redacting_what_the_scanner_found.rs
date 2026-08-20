//! Seam: `bp-redaction` composed with `bp-secrets`.
//!
//! These two crates deliberately do not depend on each other, and both are
//! right to: one finds credentials and holds none, the other destroys text
//! and knows nothing about what text is worth destroying. The shell is what
//! joins them, and joining them requires a **unit conversion** --
//! `bp_secrets::Finding` reports a 1-based line and a **character** column
//! because that is what a caret is moved to, while `bp_redaction::Span` is in
//! **bytes** because that is what a `&str` can be sliced at.
//!
//! ADR-0028 is explicit that a span in the wrong unit destroys the wrong text
//! *and reports success*, which is the worst failure this product has
//! available: a document handed over with a black rectangle drawn next to the
//! credential rather than over it. On a pure-ASCII line the two units agree
//! and every example test passes; the disagreement starts at the first
//! accented letter.
//!
//! So the properties here are all about text where the units differ:
//!
//! * the converted span **slices exactly the credential** out of the
//!   document -- the assertion that goes red the moment either crate's
//!   coordinates drift;
//! * nothing was **widened**, because widening is `bp-redaction`'s own
//!   documented symptom of a caller measuring in the wrong unit;
//! * redacting what was found removes the credential, and `verify` confirms
//!   nothing survived;
//!
//! for documents with non-ASCII text before the secret, with CRLF endings,
//! and with the secret on the first and on the last line.
//!
//! **Nothing in this file is a real credential.** Both fixtures are
//! syntactically valid and cryptographically meaningless; they exist because
//! `bp-secrets` will not report a value that fails its own "does this look
//! like a real value" test, so a fixture of `AKIA0000...` would be silently
//! skipped and every assertion below would pass vacuously. Each test
//! therefore asserts the scan actually found something first.
//!
//! Nothing here prints document content; see `common`.

mod common;

use common::{Doc, no_files};

use bp_redaction::{PLACEHOLDER, Replacement, Span, redact, verify};
use bp_secrets::{Finding, SecretKind, scan};
use proptest::prelude::*;

/// A syntactically valid AWS access key ID that is not one. Twenty upper-case
/// alphanumerics after a recognised prefix, with enough distinct characters
/// that `bp-secrets` does not dismiss it as documentation.
const AWS_KEY: &str = "AKIAQ7XV2M9TDCF4RJ81";

/// A syntactically valid GitHub token that is not one: `ghp_` and 36 more.
const GITHUB_TOKEN: &str = "ghp_9fK2mQ7xT4vB1nZ6yR3sW8dJ5cH0gL2pA4eU";

// --- the conversion under test ------------------------------------------
//
// This is the shell's join, written out here rather than imported because it
// is `bp-ui`-private. Getting it wrong is the whole risk, so it is short and
// it is the same shape the shell uses: line offsets that put back the `\r`
// `str::lines` drops, and a character-to-byte walk within the line.

/// Byte offset and text of every line, split exactly as `str::lines` splits.
///
/// `bp_secrets` numbers its findings by enumerating `text.lines()`, so this
/// has to agree with it line for line -- including that `lines` drops the
/// carriage return of a CRLF ending, which shifts every byte offset after it
/// if it is not added back.
fn line_spans(text: &str) -> Vec<(usize, &str)> {
    let mut spans = Vec::new();
    let mut offset = 0usize;
    for line in text.lines() {
        spans.push((offset, line));
        offset += line.len();
        if text[offset..].starts_with('\r') {
            offset += 1;
        }
        if text[offset..].starts_with('\n') {
            offset += 1;
        }
    }
    spans
}

/// The byte offset of character `index`, or the end of the line past it.
fn byte_of_char(line: &str, index: usize) -> usize {
    line.char_indices()
        .nth(index)
        .map_or(line.len(), |(offset, _)| offset)
}

/// Turn a scan into byte spans, the way the shell does.
///
/// A `PrivateKeyBlock` is skipped: `bp-secrets` documents that its finding
/// covers the `-----BEGIN` marker only, so redacting that span would remove
/// the label and leave the key body behind under a `[REDACTED]` marker --
/// exactly the failure ADR-0028 is written against. No fixture here produces
/// one; the arm is present so that the conversion in this file is the same
/// conversion the product performs.
fn spans_of(findings: &[Finding], text: &str) -> Vec<Span<'static>> {
    let lines = line_spans(text);
    findings
        .iter()
        .filter(|f| f.kind != SecretKind::PrivateKeyBlock)
        .filter_map(|f| {
            let (offset, line) = *lines.get(f.line.checked_sub(1)?)?;
            let first = f.column.saturating_sub(1);
            let start = byte_of_char(line, first);
            let end = byte_of_char(line, first.saturating_add(f.length));
            (start < end).then(|| Span::labelled(offset + start, offset + end, f.kind.label()))
        })
        .collect()
}

// --- the property, in one place -----------------------------------------

/// Scan `text`, convert, redact, verify -- and assert every step held.
///
/// Returns nothing: everything it has to say it says by failing, and a
/// document is not something to hand back to a test that might print it.
fn scan_then_redact_removes(text: &str, secret: &str) {
    let findings = scan(text);
    assert!(
        !findings.is_empty(),
        "the fixture was not detected at all, so nothing below is being tested"
    );

    let spans = spans_of(&findings, text);
    assert_eq!(
        spans.len(),
        findings.len(),
        "a finding did not survive the conversion into a byte span"
    );

    // The sharpest assertion in the file: the converted span must name
    // exactly the credential and not a byte more or less. Everything after
    // this would still pass if the span were off by two bytes and happened to
    // swallow the secret anyway.
    for span in &spans {
        assert!(
            text.is_char_boundary(span.start) && text.is_char_boundary(span.end),
            "a converted span falls inside a character: {}..{} of {} bytes",
            span.start,
            span.end,
            text.len(),
        );
        assert_eq!(
            &text[span.start..span.end],
            secret,
            "the converted span named {} bytes that are not the credential (at {}..{})",
            span.end - span.start,
            span.start,
            span.end,
        );
    }

    let redacted = redact(text, &spans, Replacement::Placeholder).expect("redact");

    // `widened` is `bp-redaction`'s own name for "the caller measured in the
    // wrong unit". A correct conversion never triggers it.
    for applied in &redacted.applied {
        assert!(
            !applied.widened,
            "a span had to be widened to reach a character boundary, which is the \
             documented symptom of an offset computed in the wrong unit"
        );
    }

    assert!(
        !redacted.text.contains(secret),
        "the credential is still in the redacted document"
    );
    assert!(
        redacted.text.contains(PLACEHOLDER.trim_end_matches(']')),
        "nothing was marked as redacted, so the redaction is invisible to a reader"
    );

    let verification = verify(text, &spans, &redacted.text).expect("verify");
    assert_eq!(verification.checked, spans.len());
    assert!(
        verification.is_clean(),
        "{} of {} redactions left their text in the document",
        verification.surviving.len(),
        verification.checked,
    );
}

/// Assemble a document: `before` lines, the secret line, `after` lines.
fn document(before: &[&str], secret_line: &str, after: &[&str], eol: &str) -> String {
    let mut out = String::new();
    for line in before {
        out.push_str(line);
        out.push_str(eol);
    }
    out.push_str(secret_line);
    for line in after {
        out.push_str(eol);
        out.push_str(line);
    }
    out
}

// --- the cases ----------------------------------------------------------

#[test]
fn a_purely_ascii_document_is_the_case_that_never_broke() {
    // The control. If this is the only test that exists, the seam looks fine
    // and is not.
    let text = document(
        &["notes"],
        &format!("aws_access_key_id = {AWS_KEY}"),
        &["done"],
        "\n",
    );
    scan_then_redact_removes(&text, AWS_KEY);
}

#[test]
fn non_ascii_before_the_secret_on_the_same_line() {
    // Where the units part company. Every character before the credential on
    // its own line costs one column and two, three or four bytes, so a
    // conversion that treats the column as an offset lands short of the
    // credential by exactly the number of extra bytes.
    for prefix in [
        "Führungsnotizen: ",
        "研究ノート ",
        "clé — ",
        "🎼🎻 ",
        "naïve café résumé — Sitzung 2 — ",
    ] {
        let text = document(
            &["heading"],
            &format!("{prefix}aws_access_key_id = {AWS_KEY}"),
            &["done"],
            "\n",
        );
        scan_then_redact_removes(&text, AWS_KEY);
    }
}

#[test]
fn non_ascii_on_the_lines_before_the_secret() {
    // The other half of the same arithmetic: earlier *lines* shift the line
    // offset rather than the column, and a converter that computed line
    // offsets by counting characters instead of bytes fails here and nowhere
    // else.
    let text = document(
        &["Führungsnotizen", "研究ノート", "🎼 Takt 4"],
        &format!("token = {GITHUB_TOKEN}"),
        &["ende"],
        "\n",
    );
    scan_then_redact_removes(&text, GITHUB_TOKEN);
}

#[test]
fn crlf_endings_with_non_ascii_before_the_secret() {
    // `str::lines` drops the carriage return, so every line offset after the
    // first is one byte short unless it is put back. With non-ASCII lines
    // above as well, both corrections have to be right at once.
    let text = document(
        &["Führungsnotizen", "研究ノート"],
        &format!("clé — aws_access_key_id = {AWS_KEY}"),
        &["ende", "Schluß"],
        "\r\n",
    );
    scan_then_redact_removes(&text, AWS_KEY);
}

#[test]
fn the_secret_on_the_first_line() {
    for eol in ["\n", "\r\n"] {
        let text = document(
            &[],
            &format!("clé — aws_access_key_id = {AWS_KEY}"),
            &["Führungsnotizen", "ende"],
            eol,
        );
        scan_then_redact_removes(&text, AWS_KEY);
    }
}

#[test]
fn the_secret_on_the_last_line_with_and_without_a_final_newline() {
    for eol in ["\n", "\r\n"] {
        let unterminated = document(
            &["Führungsnotizen", "研究ノート"],
            &format!("clé — token = {GITHUB_TOKEN}"),
            &[],
            eol,
        );
        scan_then_redact_removes(&unterminated, GITHUB_TOKEN);

        let terminated = format!("{unterminated}{eol}");
        scan_then_redact_removes(&terminated, GITHUB_TOKEN);
    }
}

#[test]
fn two_secrets_on_one_non_ascii_line_are_both_removed() {
    // Two spans on one line is where an implementation that applied
    // redactions one after another shifts the second offset. `bp-redaction`
    // resolves every span against the original before copying a byte, and
    // this is the seam-level check that the shell hands it offsets that let
    // it.
    let text = format!("Führungsnotizen — key = {AWS_KEY} and token = {GITHUB_TOKEN}\nende\n");

    let findings = scan(&text);
    assert_eq!(findings.len(), 2, "the fixture must produce two findings");

    let spans = spans_of(&findings, &text);
    assert_eq!(&text[spans[0].start..spans[0].end], AWS_KEY);
    assert_eq!(&text[spans[1].start..spans[1].end], GITHUB_TOKEN);

    let redacted = redact(&text, &spans, Replacement::Placeholder).expect("redact");
    assert!(!redacted.text.contains(AWS_KEY));
    assert!(!redacted.text.contains(GITHUB_TOKEN));
    assert!(
        verify(&text, &spans, &redacted.text)
            .expect("verify")
            .is_clean()
    );
}

#[test]
fn removing_and_masking_take_the_credential_out_just_as_placeholder_does() {
    // The three replacement modes differ in what they leave behind and must
    // not differ in what they take away. `Mask` is the one worth pinning: a
    // mask written *over* text rather than *instead of* it is the classic
    // redaction failure, and it is invisible unless something looks.
    let text = format!("clé — aws_access_key_id = {AWS_KEY}\n");
    let spans = spans_of(&scan(&text), &text);
    assert_eq!(spans.len(), 1);

    for replacement in [
        Replacement::Remove,
        Replacement::Placeholder,
        Replacement::Mask {
            fill: '\u{2588}',
            width: bp_redaction::MaskWidth::MatchOriginal,
        },
        Replacement::Mask {
            fill: '*',
            width: bp_redaction::MaskWidth::Fixed(8),
        },
    ] {
        let redacted = redact(&text, &spans, replacement).expect("redact");
        assert!(
            !redacted.text.contains(AWS_KEY),
            "{replacement:?} left the credential in the document"
        );
        assert!(
            verify(&text, &spans, &redacted.text)
                .expect("verify")
                .is_clean(),
            "{replacement:?} did not survive verification"
        );
    }
}

proptest! {
    #![proptest_config(no_files(64))]

    /// The whole property over generated non-ASCII surroundings.
    ///
    /// The alphabet is deliberately free of anything the scanner could match
    /// -- no ASCII alphanumeric runs, no `=` and no `:` -- so the only
    /// finding is the fixture, and a failure is a failure of the conversion
    /// rather than of the generator.
    #[test]
    fn the_credential_is_removed_whatever_surrounds_it(
        before in proptest::collection::vec("[éüßö研究ノト🎼🎻—  ]{0,24}", 0..4),
        prefix in "[éüßö研究ノト🎼🎻—  ]{0,24}",
        after in proptest::collection::vec("[éüßö研究ノト🎼🎻—  ]{0,24}", 0..4),
        crlf in any::<bool>(),
    ) {
        let before: Vec<&str> = before.iter().map(String::as_str).collect();
        let after: Vec<&str> = after.iter().map(String::as_str).collect();
        let text = Doc(document(
            &before,
            &format!("{prefix}aws_access_key_id = {AWS_KEY}"),
            &after,
            if crlf { "\r\n" } else { "\n" },
        ));

        // `scan_then_redact_removes` asserts rather than returns, so a
        // failure here reports the generated shape through `Doc` -- sizes,
        // never text.
        scan_then_redact_removes(text.as_str(), AWS_KEY);
    }
}
