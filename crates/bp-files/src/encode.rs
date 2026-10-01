//! Turning a document back into bytes: the inverse of [`load`].
//!
//! [`load`]: crate::load
//!
//! This lives here, next to `load`, because it is the same decision seen from
//! the other side. While it lived in the Slint shell as a `pub(crate)`
//! function, no test outside a window could reach the code that decides what
//! actually lands on the user's disk -- the cross-crate tests could prove the
//! *model* lossless and could not touch the writer. The property that makes
//! the pair worth having is stated and tested below: write then load returns
//! what you wrote.

use std::borrow::Cow;

use bp_core::{Encoding, LineEnding};

use crate::utf16::{self, Endian};

/// What [`encode`] does with the line breaks the text already has.
///
/// A choice rather than a fixed behaviour because the two callers want
/// opposite things and both are right. Saving a file the user opened should
/// return the bytes they were given; applying an explicit "convert to CRLF"
/// should rewrite every break. Folding those into one silent rule -- which is
/// what the shell did -- means a mixed-ending document is quietly rewritten
/// on every save, and nothing can tell that it happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineEndingPolicy {
    /// Write the text's line breaks through unchanged, byte for byte.
    ///
    /// The default, because it is the only policy under which `encode` is a
    /// true inverse of `load` for every document. A file that was mixed when
    /// it was opened is still mixed when it is saved.
    #[default]
    Preserve,
    /// Rewrite every line break to this convention.
    ///
    /// **This is lossy on purpose, and only for the minority convention in a
    /// mixed document.** An LF break in a document saved as CRLF becomes
    /// CRLF; there is no way to get it back. Use
    /// [`LineEndingSurvey::breaks_rewritten_by`] before calling to find out
    /// how many breaks that will be -- zero for the overwhelmingly common
    /// case of a document that already uses one convention throughout.
    ///
    /// A bare carriage return is data, not a break, and is never rewritten.
    /// That has one visible consequence, reported by
    /// [`LineEndingSurvey::converges_to`]: converting `"\r\r\n"` to LF leaves
    /// the bare `\r` in front of the new `\n`, which reads back as CRLF.
    ConvertTo(LineEnding),
}

/// A count of the line-break conventions present in a document.
///
/// Exists so that "saving this will converge it" is something a caller can
/// *know* rather than infer. `bp_core::LineEnding::detect` answers "which
/// convention is this document", by majority, and throws the minority away in
/// the process; the status bar needs no more than that. Deciding whether to
/// warn before a save needs the counts themselves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LineEndingSurvey {
    /// Bare line feeds -- `\n` not preceded by `\r`.
    pub lf: usize,
    /// Carriage-return line-feed pairs.
    pub crlf: usize,
    /// Carriage returns with no line feed after them.
    ///
    /// Counted and reported, never treated as a line break: `LineEnding` has
    /// no variant for classic Mac endings, so calling a bare `\r` a break
    /// would mean converting it to something, and converting a `\r` that was
    /// really data -- in a paste from a terminal, say -- is exactly the
    /// silent rewrite this module exists to avoid. They pass through
    /// untouched under every policy.
    pub lone_cr: usize,
    /// Bare carriage returns that sit immediately before a line break, as in
    /// `"\r\r\n"` -- the fingerprint of a document some other tool has
    /// already converted once.
    ///
    /// A subset of [`lone_cr`](Self::lone_cr), broken out because it is the
    /// one thing that stops a conversion to LF from converging: rewriting the
    /// break to `\n` leaves the untouched `\r` sitting in front of it, which
    /// on reload is a CRLF again. See [`converges_to`](Self::converges_to).
    pub cr_before_break: usize,
}

impl LineEndingSurvey {
    /// Count the line breaks in `text`.
    ///
    /// One pass, byte-wise. `\r` and `\n` are ASCII, so they cannot occur
    /// inside a multi-byte UTF-8 sequence and a byte scan cannot land in the
    /// middle of a character.
    pub fn of(text: &str) -> Self {
        let mut survey = Self::default();
        let bytes = text.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'\n' => {
                    survey.lf += 1;
                    i += 1;
                }
                b'\r' if bytes.get(i + 1) == Some(&b'\n') => {
                    survey.crlf += 1;
                    i += 2;
                }
                b'\r' => {
                    survey.lone_cr += 1;
                    // `\r\r\n`: this bare CR abuts the CRLF after it.
                    if bytes.get(i + 1) == Some(&b'\r') && bytes.get(i + 2) == Some(&b'\n') {
                        survey.cr_before_break += 1;
                    }
                    i += 1;
                }
                _ => i += 1,
            }
        }
        survey
    }

    /// Whether both conventions occur, so some break disagrees with whatever
    /// the document is declared to be.
    ///
    /// A lone `\r` never makes a document mixed, because it is not a break.
    pub const fn is_mixed(self) -> bool {
        self.lf > 0 && self.crlf > 0
    }

    /// Total number of line breaks, of either convention.
    pub const fn breaks(self) -> usize {
        self.lf + self.crlf
    }

    /// The convention the document is, by majority.
    ///
    /// Contractually identical to `bp_core::LineEnding::detect` -- a property
    /// test pins the two together, because two answers to "what is this file"
    /// in one program is a defect waiting to be found by a user. [`None`] for
    /// a document with no line break at all, which the caller resolves to
    /// `LineEnding::platform_default`.
    pub const fn majority(self) -> Option<LineEnding> {
        if self.crlf == 0 && self.lf == 0 {
            None
        } else if self.crlf >= self.lf {
            // Ties go to CRLF, matching `detect`.
            Some(LineEnding::CrLf)
        } else {
            Some(LineEnding::Lf)
        }
    }

    /// How many line breaks [`LineEndingPolicy::ConvertTo`] would rewrite.
    ///
    /// Zero means the conversion is a no-op and `encode` is an exact inverse
    /// of `load` under either policy. Anything above zero is the number of
    /// the user's line breaks that saving is about to change, which is the
    /// number a warning would want to quote.
    pub const fn breaks_rewritten_by(self, target: LineEnding) -> usize {
        match target {
            LineEnding::Lf => self.crlf,
            LineEnding::CrLf => self.lf,
        }
    }

    /// Whether [`LineEndingPolicy::ConvertTo`] will actually leave the
    /// document using only `target`.
    ///
    /// Almost always yes, and false in exactly one situation: converting to
    /// LF a document that contains `"\r\r\n"`. The bare carriage return is
    /// data, so it stays; the `\r\n` after it becomes `\n`; and the two are
    /// then adjacent, which *is* a CRLF. The crate will not delete a byte the
    /// user typed in order to make its own postcondition come true, so it
    /// reports the situation instead. Saving such a document as LF repeatedly
    /// consumes one carriage return each time, which is why a caller that
    /// converts should check this rather than assume.
    pub const fn converges_to(self, target: LineEnding) -> bool {
        match target {
            LineEnding::Lf => self.cr_before_break == 0,
            // Converting to CRLF cannot fail this way: every `\n` it writes
            // already has its own `\r`, so an extra one in front is still
            // just a bare CR followed by a CRLF.
            LineEnding::CrLf => true,
        }
    }
}

/// Turn document text into the bytes to write to disk.
///
/// The exact inverse of [`load`](crate::load) under
/// [`LineEndingPolicy::Preserve`]: for any text this crate can hold and any
/// `encoding`, writing these bytes and loading them back returns the same
/// text and the same encoding. Under
/// [`LineEndingPolicy::ConvertTo`] it is an inverse only when
/// [`LineEndingSurvey::breaks_rewritten_by`] is zero -- deliberately, and
/// that asymmetry is pinned by test rather than left to be discovered.
///
/// **Fallible for exactly one reason** (ADR-0085): a legacy code page cannot
/// hold every character a document can, and a document opened in
/// Windows-1252 may since have had an `α` typed into it. That is refused,
/// naming the character, rather than written as `&#945;` -- which is what
/// the encoder would do if asked, and a second way to lose text. Every
/// Unicode encoding still succeeds for every `String`; this was infallible
/// until there was something to report, and an error with no reachable
/// variant would have been a branch no test could exercise.
///
/// The byte-order mark is written whenever the encoding carries one, which is
/// what makes reopening the file report the encoding it was opened with. One
/// consequence worth knowing: a document whose *first character* is U+FEFF
/// and whose encoding is plain [`Encoding::Utf8`] produces bytes that are
/// indistinguishable from a UTF-8 BOM, so reloading reports `Utf8Bom` and one
/// character shorter. The bytes are still exact; only the model shifts. That
/// is a property of BOM sniffing, not of this function, and no encoder can
/// avoid it.
pub fn encode(
    text: &str,
    encoding: Encoding,
    line_endings: LineEndingPolicy,
) -> Result<Vec<u8>, Unencodable> {
    let body = match line_endings {
        LineEndingPolicy::Preserve => Cow::Borrowed(text),
        LineEndingPolicy::ConvertTo(target) => match convert(text, target) {
            Some(converted) => Cow::Owned(converted),
            // Nothing to change: keep the borrow rather than copying the
            // whole document, which is the common case for every save.
            None => Cow::Borrowed(text),
        },
    };

    if let Encoding::Legacy(name) = encoding {
        return crate::legacy::encode(&body, name).map_err(|character| Unencodable {
            character,
            line: body
                .find(character)
                .map_or(1, |at| body[..at].matches('\n').count() + 1),
            encoding,
        });
    }

    let mut out = encoding.bom().to_vec();
    match Endian::of(encoding) {
        Some(endian) => out.extend_from_slice(&utf16::encode(&body, endian)),
        None => out.extend_from_slice(body.as_bytes()),
    }
    Ok(out)
}

/// A character the document's encoding has no form for, found at save.
///
/// Carries what a person can act on: which character, where, and that a
/// Unicode encoding would hold it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error(
    "'{character}' on line {line} cannot be written in {} -- choose Format > Encoding > UTF-8 to keep it",
    .encoding.label()
)]
pub struct Unencodable {
    pub character: char,
    /// One-based, as every line number a person sees in this product is.
    pub line: usize,
    pub encoding: Encoding,
}

/// Rewrite every line break to `target`, or [`None`] if there is nothing to
/// rewrite.
///
/// A single left-to-right pass rather than the `replace("\r\n", "\n")` then
/// `replace('\n', "\r\n")` pair the shell used. The two are *behaviourally*
/// identical -- `str::replace` is itself a single non-overlapping left-to-
/// right scan, so it handles `"\r\r\n"` exactly the way this does, and every
/// property below passes against either. The reason for one pass is cost: the
/// pair allocates a whole intermediate copy of the document on the way to
/// CRLF, on every save. The [`None`] return is the same economy -- a
/// conversion with nothing to convert should not copy the document at all.
fn convert(text: &str, target: LineEnding) -> Option<String> {
    let survey = LineEndingSurvey::of(text);
    if survey.breaks_rewritten_by(target) == 0 {
        return None;
    }

    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len() + survey.breaks());
    let mut copied = 0;
    let mut i = 0;
    while i < bytes.len() {
        let width = match bytes[i] {
            b'\n' => 1,
            b'\r' if bytes.get(i + 1) == Some(&b'\n') => 2,
            _ => {
                i += 1;
                continue;
            }
        };
        out.push_str(&text[copied..i]);
        out.push_str(target.as_str());
        i += width;
        copied = i;
    }
    out.push_str(&text[copied..]);
    Some(out)
}

#[cfg(test)]
mod tests {
    /// `encode`, for the encodings that cannot fail: every Unicode one holds
    /// every character a `String` can.
    fn unicode(text: &str, encoding: Encoding, line_endings: LineEndingPolicy) -> Vec<u8> {
        super::encode(text, encoding, line_endings)
            .expect("a Unicode encoding writes every character")
    }

    use super::*;
    use crate::{SaveOptions, atomic_write, load};
    use proptest::prelude::*;
    use tempfile::TempDir;

    const ENCODINGS: [Encoding; 4] = [
        Encoding::Utf8,
        Encoding::Utf8Bom,
        Encoding::Utf16Le,
        Encoding::Utf16Be,
    ];

    /// Write bytes into a fresh temporary directory and load them back.
    ///
    /// Never the checkout: every test here writes a file, and a test that
    /// writes into the working tree has already cost this project documents.
    fn round_trip(bytes: &[u8]) -> (TempDir, crate::LoadedFile) {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("document.txt");
        atomic_write(&path, bytes, SaveOptions::default()).expect("atomic_write");
        let loaded = load(&path).expect("load");
        (dir, loaded)
    }

    // --- generators ------------------------------------------------------

    /// Text built from the tokens that break round trips: both line breaks, a
    /// bare carriage return, two- and three-byte characters, an astral one,
    /// and a tab. The alphabet is visible here, so a failure report leaks
    /// nothing about any real document.
    fn document() -> impl Strategy<Value = String> {
        proptest::collection::vec(
            prop_oneof![
                Just("a"),
                Just("word "),
                Just("\n"),
                Just("\r\n"),
                Just("\r"),
                Just("é"),
                Just("中"),
                Just("\t"),
                Just("😀"),
            ],
            0..30,
        )
        .prop_map(|parts| parts.concat())
    }

    fn encoding() -> impl Strategy<Value = Encoding> {
        prop_oneof![
            Just(Encoding::Utf8),
            Just(Encoding::Utf8Bom),
            Just(Encoding::Utf16Le),
            Just(Encoding::Utf16Be),
        ]
    }

    fn line_ending() -> impl Strategy<Value = LineEnding> {
        prop_oneof![Just(LineEnding::Lf), Just(LineEnding::CrLf)]
    }

    // --- the properties --------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(128))]

        /// **The headline property: `encode` is the inverse of `load`.** For
        /// any text, in any encoding this crate supports, under the
        /// preserving policy, writing and reading back returns the same text,
        /// the same encoding, and the line ending the text actually has.
        ///
        /// This is the property that could not exist while the encoder lived
        /// in the shell, and the reason for moving it.
        #[test]
        fn writing_then_loading_returns_what_was_written(
            text in document(),
            encoding in encoding(),
        ) {
            // The one documented exception: leading U+FEFF in plain UTF-8 is
            // a BOM once it is bytes. The generator cannot produce it, but
            // saying so here is cheaper than a reader wondering.
            prop_assume!(!text.starts_with('\u{FEFF}'));

            let bytes = unicode(&text, encoding, LineEndingPolicy::Preserve);
            let (_dir, loaded) = round_trip(&bytes);

            prop_assert_eq!(&loaded.text, &text);
            prop_assert_eq!(loaded.encoding, encoding);
            prop_assert_eq!(
                loaded.line_ending,
                LineEnding::detect(&text).unwrap_or_default()
            );
            prop_assert_eq!(loaded.bytes_on_disk, bytes.len() as u64);
        }

        /// **The survey agrees with `bp_core`.** Two answers to "what line
        /// ending is this file" in one program is a defect waiting for a
        /// user to find it.
        #[test]
        fn majority_agrees_with_line_ending_detect(text in document()) {
            prop_assert_eq!(
                LineEndingSurvey::of(&text).majority(),
                LineEnding::detect(&text)
            );
        }

        /// **Converting converges, and changes nothing else.** After a
        /// conversion every break is the target, and the number of breaks and
        /// of bare carriage returns is exactly what it was: a conversion may
        /// rewrite a break, never add, drop or merge one.
        #[test]
        fn converting_converges_without_losing_breaks(
            text in document(),
            target in line_ending(),
        ) {
            let before = LineEndingSurvey::of(&text);
            let bytes = unicode(&text, Encoding::Utf8, LineEndingPolicy::ConvertTo(target));
            let (_dir, loaded) = round_trip(&bytes);
            let after = LineEndingSurvey::of(&loaded.text);

            // Nothing but line-break characters may move: strip every `\r`
            // and `\n` from both and what is left must be identical, so a
            // conversion can never eat a letter, a tab or an emoji.
            let strip = |s: &str| s.replace(['\r', '\n'], "");
            prop_assert_eq!(strip(&loaded.text), strip(&text), "a non-break character moved");
            prop_assert_eq!(after.breaks(), before.breaks(), "a break was added or lost");

            // Convergence is claimed only where `converges_to` claims it, and
            // *wherever* it claims it. Checking both halves is what stops the
            // predicate from becoming a rubber stamp that always says no.
            prop_assert_eq!(
                after.breaks_rewritten_by(target) == 0,
                before.converges_to(target),
                "converges_to mispredicted the outcome"
            );

            if before.converges_to(target) {
                prop_assert_eq!(after.lone_cr, before.lone_cr, "a bare CR was touched");
                if before.breaks() > 0 {
                    prop_assert_eq!(loaded.line_ending, target);
                }
            }
        }

        /// **The asymmetry, pinned as an equivalence.** Converting is an
        /// inverse of `load` *exactly* when there was nothing to convert. Not
        /// "usually", not "unless mixed" -- the survey predicts it, both
        /// directions, every time.
        ///
        /// This is the honest statement of the behaviour the shell had and
        /// could not describe: a save under `ConvertTo` rewrites the minority
        /// convention, and `breaks_rewritten_by` says in advance how many.
        #[test]
        fn converting_is_lossless_exactly_when_nothing_needs_converting(
            text in document(),
            target in line_ending(),
        ) {
            let predicted = LineEndingSurvey::of(&text).breaks_rewritten_by(target);
            let bytes = unicode(&text, Encoding::Utf8, LineEndingPolicy::ConvertTo(target));
            let (_dir, loaded) = round_trip(&bytes);

            prop_assert_eq!(
                loaded.text == text,
                predicted == 0,
                "survey predicted {} rewrites; text {} unchanged",
                predicted,
                if loaded.text == text { "was" } else { "was not" }
            );
        }

        /// **Converting is idempotent.** Saving a converged document again
        /// must be a no-op, or a document would drift a little further from
        /// what the user wrote on every save -- the failure mode nobody
        /// notices until a diff is unreadable.
        #[test]
        fn converting_twice_is_converting_once(
            text in document(),
            target in line_ending(),
        ) {
            // Only where the conversion converges. Where it does not -- a
            // document holding "\r\r\n", converted to LF -- a second pass
            // does change it again, and that drift is stated on
            // `converges_to` and pinned by example, not hidden by a weaker
            // property here.
            prop_assume!(LineEndingSurvey::of(&text).converges_to(target));

            let policy = LineEndingPolicy::ConvertTo(target);
            let once = unicode(&text, Encoding::Utf8, policy);
            let text_once = String::from_utf8(once.clone()).expect("utf8");
            let twice = unicode(&text_once, Encoding::Utf8, policy);
            prop_assert_eq!(twice, once);
        }

        /// **The survey partitions the text.** Every `\r` and `\n` in the
        /// document is accounted for by exactly one counter, so no break can
        /// be invisible to a caller deciding whether to warn.
        #[test]
        fn the_survey_accounts_for_every_carriage_return_and_line_feed(text in document()) {
            let s = LineEndingSurvey::of(&text);
            prop_assert_eq!(
                s.lf + s.crlf,
                text.matches('\n').count(),
                "line feeds unaccounted for"
            );
            prop_assert_eq!(
                s.crlf + s.lone_cr,
                text.matches('\r').count(),
                "carriage returns unaccounted for"
            );
        }
    }

    // --- examples, boundaries and failure modes --------------------------

    #[test]
    fn the_empty_document_is_just_the_byte_order_mark() {
        for encoding in ENCODINGS {
            let bytes = unicode("", encoding, LineEndingPolicy::Preserve);
            assert_eq!(bytes, encoding.bom(), "{}", encoding.label());

            let (_dir, loaded) = round_trip(&bytes);
            assert_eq!(loaded.text, "");
            assert_eq!(loaded.encoding, encoding);
            assert_eq!(loaded.line_ending, LineEnding::platform_default());
        }
    }

    #[test]
    fn utf16_writes_the_byte_order_mark_it_promises() {
        assert_eq!(
            unicode("hi", Encoding::Utf16Le, LineEndingPolicy::Preserve),
            b"\xFF\xFEh\0i\0"
        );
        assert_eq!(
            unicode("hi", Encoding::Utf16Be, LineEndingPolicy::Preserve),
            b"\xFE\xFF\0h\0i"
        );
    }

    #[test]
    fn a_utf8_bom_is_written_back_and_a_plain_utf8_file_gains_nothing() {
        assert_eq!(
            unicode("hi", Encoding::Utf8Bom, LineEndingPolicy::Preserve),
            b"\xEF\xBB\xBFhi"
        );
        assert_eq!(
            unicode("hi", Encoding::Utf8, LineEndingPolicy::Preserve),
            b"hi"
        );
    }

    #[test]
    fn preserving_leaves_a_mixed_document_mixed() {
        // Three LF, two CRLF: the majority answer must not become an
        // instruction to rewrite the minority.
        let text = "crlf\r\nlf one\nlf two\nlf three\ncrlf\r\n";
        let bytes = unicode(text, Encoding::Utf8, LineEndingPolicy::Preserve);

        assert_eq!(bytes, text.as_bytes());
        let survey = LineEndingSurvey::of(text);
        assert!(survey.is_mixed());
        assert_eq!(survey.majority(), Some(LineEnding::Lf));
        assert_eq!(survey.breaks_rewritten_by(LineEnding::Lf), 2);
        assert_eq!(survey.breaks_rewritten_by(LineEnding::CrLf), 3);
    }

    #[test]
    fn converting_a_mixed_document_rewrites_exactly_the_predicted_breaks() {
        let text = "crlf\r\nlf one\nlf two\nlf three\ncrlf\r\n";
        let converted = unicode(
            text,
            Encoding::Utf8,
            LineEndingPolicy::ConvertTo(LineEnding::Lf),
        );
        assert_eq!(
            String::from_utf8(converted).unwrap(),
            "crlf\nlf one\nlf two\nlf three\ncrlf\n",
            "the two CRLF the survey named are the two that changed"
        );
    }

    #[test]
    fn a_bare_carriage_return_is_data_not_a_line_break() {
        let text = "a\rb\nc";
        for target in [LineEnding::Lf, LineEnding::CrLf] {
            let out = unicode(text, Encoding::Utf8, LineEndingPolicy::ConvertTo(target));
            let out = String::from_utf8(out).unwrap();
            assert_eq!(out, format!("a\rb{}c", target.as_str()));
        }
        let survey = LineEndingSurvey::of(text);
        assert_eq!(survey.lone_cr, 1);
        assert_eq!(survey.cr_before_break, 0, "this CR is nowhere near a break");
        assert!(!LineEndingSurvey::of("a\rb").is_mixed());
    }

    #[test]
    fn a_carriage_return_abutting_a_break_is_the_one_case_that_cannot_converge() {
        // "\r\r\n" is a bare CR -- data -- followed by a CRLF. Converting to
        // LF rewrites the break and leaves the CR, and the two are then
        // adjacent, which is a CRLF again. The only alternative is deleting a
        // byte the user typed, which this crate will not do; so it reports
        // the situation instead of pretending.
        let survey = LineEndingSurvey::of("a\r\r\nb");
        assert_eq!(survey.crlf, 1);
        assert_eq!(survey.lone_cr, 1);
        assert_eq!(survey.cr_before_break, 1);
        assert!(!survey.converges_to(LineEnding::Lf));
        assert!(survey.converges_to(LineEnding::CrLf));

        let out = unicode(
            "a\r\r\nb",
            Encoding::Utf8,
            LineEndingPolicy::ConvertTo(LineEnding::Lf),
        );
        assert_eq!(String::from_utf8(out).unwrap(), "a\r\nb");

        // And the drift a caller heeding `converges_to` avoids: converting
        // again eats the carriage return the first pass left behind.
        let again = unicode(
            "a\r\nb",
            Encoding::Utf8,
            LineEndingPolicy::ConvertTo(LineEnding::Lf),
        );
        assert_eq!(String::from_utf8(again).unwrap(), "a\nb");

        // Converting the same document to CRLF is a clean no-op, which is why
        // `converges_to` answers per target rather than per document.
        let out = unicode(
            "a\r\r\nb",
            Encoding::Utf8,
            LineEndingPolicy::ConvertTo(LineEnding::CrLf),
        );
        assert_eq!(String::from_utf8(out).unwrap(), "a\r\r\nb");
    }

    #[test]
    fn an_ordinary_document_converges_to_either_convention() {
        for text in ["a\nb\r\nc", "a\nb", "a\r\nb", "no breaks", "", "a\rb"] {
            let survey = LineEndingSurvey::of(text);
            assert!(survey.converges_to(LineEnding::Lf), "{text:?}");
            assert!(survey.converges_to(LineEnding::CrLf), "{text:?}");
        }
    }

    #[test]
    fn an_empty_survey_has_no_majority_and_nothing_to_rewrite() {
        let s = LineEndingSurvey::of("");
        assert_eq!(s, LineEndingSurvey::default());
        assert_eq!(s.majority(), None);
        assert!(!s.is_mixed());
        assert_eq!(s.breaks_rewritten_by(LineEnding::Lf), 0);
        assert_eq!(s.breaks_rewritten_by(LineEnding::CrLf), 0);
    }

    #[test]
    fn a_tie_goes_to_crlf_just_as_detect_does() {
        let text = "a\r\nb\nc";
        assert_eq!(
            LineEndingSurvey::of(text).majority(),
            Some(LineEnding::CrLf)
        );
        assert_eq!(LineEnding::detect(text), Some(LineEnding::CrLf));
    }

    #[test]
    fn astral_characters_survive_every_encoding() {
        // The character a naive UTF-16 cast truncates, and the one a
        // surrogate-pair bug splits.
        let text = "😀 é 中\n";
        for encoding in ENCODINGS {
            let bytes = unicode(text, encoding, LineEndingPolicy::Preserve);
            let (_dir, loaded) = round_trip(&bytes);
            assert_eq!(loaded.text, text, "{}", encoding.label());
            assert_eq!(loaded.encoding, encoding);
        }
    }

    #[test]
    fn a_leading_zero_width_no_break_space_in_plain_utf8_is_read_back_as_a_bom() {
        // The documented limitation, tested so it is a known behaviour rather
        // than a surprise. The bytes are exact; the model shifts.
        let text = "\u{FEFF}hi";
        let bytes = unicode(text, Encoding::Utf8, LineEndingPolicy::Preserve);
        assert_eq!(bytes, "\u{FEFF}hi".as_bytes());

        let (_dir, loaded) = round_trip(&bytes);
        assert_eq!(loaded.encoding, Encoding::Utf8Bom);
        assert_eq!(loaded.text, "hi");

        // In UTF-16 it round-trips exactly, because the mark is not ambiguous
        // with the first character there.
        let bytes = unicode(text, Encoding::Utf16Le, LineEndingPolicy::Preserve);
        let (_dir, loaded) = round_trip(&bytes);
        assert_eq!(loaded.encoding, Encoding::Utf16Le);
        assert_eq!(loaded.text, text);
    }

    #[test]
    fn the_default_policy_preserves() {
        assert_eq!(LineEndingPolicy::default(), LineEndingPolicy::Preserve);
    }
}
