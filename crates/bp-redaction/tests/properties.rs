//! Property tests for redaction and metadata inspection.
//!
//! The unit tests inside the crate pin down what each rule does. These check
//! the things that must hold for *every* input, including the ones nobody
//! thought of -- and the first of them is the point of the whole crate: after
//! a redaction, the text that was named is not in the output. Not usually, not
//! for the spans somebody wrote a test for: for every span in range.
//!
//! Two generators do the work.
//!
//! * [`distinct_text`] builds a string whose characters are all different, so
//!   that *every* substring of it occurs exactly once. That is what makes
//!   "the removed text is gone" a decidable question: on ordinary text a
//!   second, untouched copy of the same words would be indistinguishable from
//!   a failed redaction, which is precisely the limitation `verify` documents.
//! * [`awkward_text`] and arbitrary strings supply the shapes that break
//!   slicing: multi-byte characters, lone control bytes, text that is nothing
//!   but combining marks.

use bp_redaction::{
    Exposure, MaskWidth, MetadataFinding, Redacted, RedactionError, Replacement, Span, inspect,
    redact, verify,
};
use proptest::prelude::*;

/// Every mode, so no property is only ever checked against the easy one.
///
/// `Fixed(0)` is deliberately in the list: it is the mode that leaves no trace
/// at all, and a property that quietly relied on the replacement being visible
/// would fail here.
const MODES: &[Replacement] = &[
    Replacement::Remove,
    Replacement::Placeholder,
    Replacement::Mask {
        fill: '*',
        width: MaskWidth::Fixed(3),
    },
    Replacement::Mask {
        fill: '\u{2588}',
        width: MaskWidth::MatchOriginal,
    },
    Replacement::Mask {
        fill: '#',
        width: MaskWidth::Fixed(0),
    },
];

/// Labels a generated span may carry.
///
/// Upper case and spaces only, sharing not one character with the pool
/// [`distinct_text`] draws from, so that writing a label into the output can
/// never reintroduce a character the redaction removed. If it could, the
/// "it is gone" property would fail for a reason that has nothing to do with
/// redaction.
const LABELS: &[&str] = &["AWS KEY", "GITHUB TOKEN", "CLIENT NAME"];

/// Characters that are all different from each other, and none of which
/// appears in any replacement this crate can write.
const DISTINCT_POOL: &str = "abcdefghijklmnopqr\u{e9}\u{f1}\u{3b1}\u{3c9}\u{4e2d}\u{1f511}";

/// A string in which every character occurs exactly once, so every substring
/// of it is unique.
fn distinct_text() -> impl Strategy<Value = String> {
    let pool: Vec<char> = DISTINCT_POOL.chars().collect();
    Just(pool)
        .prop_shuffle()
        .prop_map(|characters| characters.into_iter().collect())
}

/// Text assembled from the shapes that break byte arithmetic.
fn awkward_text() -> impl Strategy<Value = String> {
    let fragments = &[
        "",
        " ",
        "\n",
        "\r\n",
        "a",
        "\u{e9}",
        "\u{4e2d}\u{6587}",
        "\u{1f511}",
        "e\u{301}",
        "\u{0}\u{7f}",
        "\u{feff}",
        "\u{200b}\u{200d}",
        "author: Daniel Boles",
        "/home/danie/notes.md",
        "2026-08-19T14:32:07+01:00",
        "daniel@example.invalid",
    ][..];

    prop::collection::vec(prop::sample::select(fragments), 0..24).prop_map(|parts| parts.concat())
}

/// Every byte offset in `text` that a span may legally name.
fn boundaries(text: &str) -> Vec<usize> {
    (0..=text.len())
        .filter(|at| text.is_char_boundary(*at))
        .collect()
}

/// Some text, and some spans measured against it -- always in range, so
/// `redact` must succeed, but free to split characters, overlap, touch,
/// nest, be empty and arrive out of order.
fn text_and_spans() -> impl Strategy<Value = (String, Vec<(usize, usize, Option<usize>)>)> {
    awkward_text().prop_flat_map(|text| {
        let len = text.len();
        let spans =
            prop::collection::vec((0..=len, 0..=len, prop::option::of(0..LABELS.len())), 0..8)
                .prop_map(|raw| {
                    raw.into_iter()
                        .map(|(a, b, label)| (a.min(b), a.max(b), label))
                        .collect::<Vec<_>>()
                });
        (Just(text), spans)
    })
}

fn build<'a>(raw: &[(usize, usize, Option<usize>)]) -> Vec<Span<'a>> {
    raw.iter()
        .map(|(start, end, label)| match label {
            Some(index) => Span::labelled(*start, *end, LABELS[*index]),
            None => Span::new(*start, *end),
        })
        .collect()
}

/// Everything a redaction leaves untouched, in order -- taken once from the
/// original and once from the output, so the two can be compared.
fn untouched(text: &str, ranges: &[(usize, usize)]) -> String {
    let mut kept = String::new();
    let mut cursor = 0usize;
    for (start, end) in ranges {
        kept.push_str(&text[cursor..*start]);
        cursor = *end;
    }
    kept.push_str(&text[cursor..]);
    kept
}

/// The structural invariants of a result, checked wherever one is produced.
fn assert_result_is_sane(text: &str, redacted: &Redacted) -> Result<(), TestCaseError> {
    let mut previous_end = 0usize;
    for entry in &redacted.applied {
        prop_assert!(entry.start < entry.end, "an empty redaction: {entry:?}");
        prop_assert!(entry.end <= text.len(), "{entry:?} runs off the original");
        prop_assert!(
            entry.start >= previous_end,
            "{entry:?} overlaps or precedes the one before it"
        );
        previous_end = entry.end;

        prop_assert!(text.is_char_boundary(entry.start), "{entry:?}");
        prop_assert!(text.is_char_boundary(entry.end), "{entry:?}");
        prop_assert!(entry.output_start <= entry.output_end, "{entry:?}");
        prop_assert!(
            entry.output_end <= redacted.text.len(),
            "{entry:?} runs off the output"
        );
        prop_assert!(
            redacted.text.is_char_boundary(entry.output_start),
            "{entry:?}"
        );
        prop_assert!(
            redacted.text.is_char_boundary(entry.output_end),
            "{entry:?}"
        );
    }

    // Everything that was not redacted survived, in the same order, byte for
    // byte. This is the other half of "the redaction did what it said": not
    // only is the named text gone, nothing else moved.
    let source: Vec<(usize, usize)> = redacted
        .applied
        .iter()
        .map(|entry| (entry.start, entry.end))
        .collect();
    let output: Vec<(usize, usize)> = redacted
        .applied
        .iter()
        .map(|entry| (entry.output_start, entry.output_end))
        .collect();
    prop_assert_eq!(
        untouched(text, &source),
        untouched(&redacted.text, &output),
        "the text outside the redactions changed"
    );

    Ok(())
}

proptest! {
    /// The property the crate exists for, checked at every span in range:
    /// redact a stretch of text, and its content is not in what comes back.
    ///
    /// The text has no repeated character, so every substring of it is unique
    /// and a survivor can only be a survivor.
    #[test]
    fn redacting_any_span_removes_its_content(text in distinct_text()) {
        let offsets = boundaries(&text);
        for (index, start) in offsets.iter().enumerate() {
            for end in &offsets[index + 1..] {
                let spans = [Span::new(*start, *end)];
                for mode in MODES {
                    let redacted = redact(&text, &spans, *mode).expect("in range");
                    prop_assert!(
                        !redacted.text.contains(&text[*start..*end]),
                        "{start}..{end} survived {mode:?}"
                    );
                    prop_assert!(
                        verify(&text, &spans, &redacted.text).expect("in range").is_clean(),
                        "{start}..{end} was reported as surviving {mode:?}"
                    );
                    assert_result_is_sane(&text, &redacted)?;
                }
            }
        }
    }

    /// The other side of verification: when nothing was redacted, *every*
    /// redaction must be reported as surviving, by index.
    ///
    /// Handing the original text back as the output is the shape a real
    /// mistake takes -- a mode that turned out to be a no-op, an output
    /// variable that was never reassigned -- and a check that stopped at the
    /// first survivor, or only ever looked at the first span, would still
    /// report a document as verified.
    #[test]
    fn verification_reports_every_redaction_when_none_of_them_happened(
        text in distinct_text(),
    ) {
        let offsets = boundaries(&text);
        if offsets.len() < 7 {
            return Ok(());
        }
        let spans: Vec<Span<'_>> = offsets
            .chunks_exact(3)
            .map(|chunk| Span::new(chunk[0], chunk[1]))
            .collect();

        let checked = verify(&text, &spans, &text).expect("in range");
        prop_assert_eq!(checked.checked, spans.len());
        prop_assert_eq!(
            checked.surviving,
            (0..spans.len()).collect::<Vec<_>>(),
            "every span's text is still there, and every one must be named"
        );
    }

    /// Redacting everything leaves nothing of the original -- not a character
    /// of it, whichever mode was used.
    #[test]
    fn redacting_the_whole_document_leaves_none_of_it(text in distinct_text()) {
        let whole = [Span::new(0, text.len())];
        for mode in MODES {
            let redacted = redact(&text, &whole, *mode).expect("in range");
            for character in text.chars() {
                prop_assert!(
                    !redacted.text.contains(character),
                    "{character:?} survived {mode:?} in {:?}", redacted.text
                );
            }
        }
    }

    /// Cutting a span in two at any interior point changes nothing. This is
    /// what the adjacency-merge rule buys: where one finding ends and the next
    /// begins is the caller's arbitrary decision, and it must not be visible.
    #[test]
    fn splitting_a_span_anywhere_gives_the_same_output(text in awkward_text()) {
        let offsets = boundaries(&text);
        if offsets.len() < 3 {
            return Ok(());
        }
        let (start, end) = (offsets[0], *offsets.last().expect("non-empty"));
        let whole = [Span::labelled(start, end, LABELS[0])];

        for cut in &offsets[1..offsets.len() - 1] {
            let split = [
                Span::labelled(start, *cut, LABELS[0]),
                Span::labelled(*cut, end, LABELS[0]),
            ];
            for mode in MODES {
                prop_assert_eq!(
                    redact(&text, &whole, *mode).expect("in range"),
                    redact(&text, &split, *mode).expect("in range"),
                    "cut at {}", cut
                );
            }
        }
    }

    /// The order the spans arrive in is not information. Reversing them --
    /// which is the worst case for a sort that is not doing its job -- gives
    /// the identical result, including the audit records.
    #[test]
    fn the_order_of_the_spans_does_not_matter((text, raw) in text_and_spans()) {
        let forwards = build(&raw);
        let mut backwards = forwards.clone();
        backwards.reverse();

        for mode in MODES {
            prop_assert_eq!(
                redact(&text, &forwards, *mode).expect("in range"),
                redact(&text, &backwards, *mode).expect("in range")
            );
        }
    }

    /// Whatever the spans, the result holds together: the redactions are in
    /// order, do not overlap, sit on character boundaries, point at real
    /// ranges of the output, and leave everything else exactly where it was.
    #[test]
    fn a_result_is_structurally_sound_whatever_the_spans((text, raw) in text_and_spans()) {
        let spans = build(&raw);
        for mode in MODES {
            let redacted = redact(&text, &spans, *mode).expect("in range");
            assert_result_is_sane(&text, &redacted)?;
        }
    }

    /// No span the caller named is left partly intact. Every non-empty one
    /// lies wholly inside one redaction -- which is what merging must never
    /// break, and the direction of failure that matters: a redaction that
    /// covers more than was asked is untidy, one that covers less is a leak.
    #[test]
    fn every_span_asked_for_is_covered_by_a_redaction((text, raw) in text_and_spans()) {
        let spans = build(&raw);
        let redacted = redact(&text, &spans, Replacement::Remove).expect("in range");

        for span in &spans {
            if span.is_empty() {
                continue;
            }
            prop_assert!(
                redacted
                    .applied
                    .iter()
                    .any(|entry| entry.start <= span.start && span.end <= entry.end),
                "{span:?} is not inside any of {:?}", redacted.applied
            );
        }
    }

    /// Removal takes out exactly the bytes it says it did, and no others.
    #[test]
    fn removal_shortens_the_text_by_exactly_what_it_removed((text, raw) in text_and_spans()) {
        let spans = build(&raw);
        let redacted = redact(&text, &spans, Replacement::Remove).expect("in range");
        let removed: usize = redacted
            .applied
            .iter()
            .map(|entry| entry.end - entry.start)
            .sum();

        prop_assert_eq!(redacted.text.len(), text.len() - removed);
    }

    /// Feeding the redactions back in as spans changes nothing. A plan that
    /// was not already fully merged would grow or shift here.
    #[test]
    fn a_plan_is_already_settled((text, raw) in text_and_spans()) {
        let redacted = redact(&text, &build(&raw), Replacement::Remove).expect("in range");
        let again: Vec<Span<'_>> = redacted
            .applied
            .iter()
            .map(|entry| Span::new(entry.start, entry.end))
            .collect();

        let repeated = redact(&text, &again, Replacement::Remove).expect("in range");
        prop_assert_eq!(&repeated.text, &redacted.text);
        prop_assert_eq!(repeated.applied.len(), redacted.applied.len());
    }

    /// Nothing here reads a clock, a file or a global, so two redactions of
    /// the same text are the same redaction.
    #[test]
    fn redaction_is_deterministic((text, raw) in text_and_spans()) {
        let spans = build(&raw);
        for mode in MODES {
            prop_assert_eq!(
                redact(&text, &spans, *mode).expect("in range"),
                redact(&text, &spans, *mode).expect("in range")
            );
        }
    }

    /// Arbitrary bytes and arbitrary offsets, including offsets past the end
    /// and offsets in the wrong order. Every one of them must come back as a
    /// result or an error, never as a panic: a span that split a character
    /// would panic on the slice, and that is a crash in the user's face.
    #[test]
    fn arbitrary_text_and_arbitrary_offsets_never_panic(
        text in "(?s).{0,300}",
        raw in prop::collection::vec((0usize..400, 0usize..400), 0..8),
    ) {
        let spans: Vec<Span<'_>> = raw
            .iter()
            .map(|(start, end)| Span::labelled(*start, *end, LABELS[0]))
            .collect();

        for mode in MODES {
            match redact(&text, &spans, *mode) {
                Ok(redacted) => {
                    assert_result_is_sane(&text, &redacted)?;
                    // If it could be redacted it can be verified.
                    prop_assert!(verify(&text, &spans, &redacted.text).is_ok());
                }
                Err(error) => {
                    // The only two reasons, and both name the offending
                    // numbers so the message can be shown to a person.
                    let message = error.to_string();
                    prop_assert!(!message.is_empty());
                    match error {
                        RedactionError::Inverted { start, end } => prop_assert!(end < start),
                        RedactionError::PastEnd { end, length } => {
                            prop_assert!(end > length);
                            prop_assert_eq!(length, text.len());
                        }
                    }
                }
            }
        }
    }

    /// Inspection must survive anything, and every finding it produces must
    /// point at real, sliceable text -- because a caller is about to slice it.
    #[test]
    fn every_metadata_finding_lands_inside_the_text_it_came_from(text in awkward_text()) {
        let found = inspect(&text);
        let mut previous_end = 0usize;

        for finding in &found {
            prop_assert!(finding.start < finding.end, "{finding:?} is empty");
            prop_assert!(finding.end <= text.len(), "{finding:?} runs off the end");
            prop_assert!(text.is_char_boundary(finding.start), "{finding:?}");
            prop_assert!(text.is_char_boundary(finding.end), "{finding:?}");
            prop_assert!(
                finding.start >= previous_end,
                "{finding:?} overlaps the one before it"
            );
            previous_end = finding.end;

            prop_assert!(finding.line >= 1, "lines are 1-based: {finding:?}");
            prop_assert!(
                finding.line <= text.lines().count().max(1),
                "{finding:?} is past the last line"
            );
        }
    }

    /// The same, for text that was never meant to be inspected.
    #[test]
    fn inspecting_arbitrary_text_never_panics(text in "(?s).{0,600}") {
        for finding in inspect(&text) {
            prop_assert!(text.is_char_boundary(finding.start), "{finding:?}");
            prop_assert!(text.is_char_boundary(finding.end), "{finding:?}");
        }
    }

    /// Inspection and redaction compose: the spans `inspect` produces are
    /// spans `redact` accepts, and the values it found are gone afterwards.
    /// This is the whole reason findings are in byte offsets.
    #[test]
    fn what_inspection_finds_can_always_be_redacted(text in awkward_text()) {
        let found = inspect(&text);
        let spans: Vec<Span<'_>> = found.iter().map(MetadataFinding::span).collect();

        for mode in MODES {
            let redacted = redact(&text, &spans, *mode).expect("findings are in range");
            assert_result_is_sane(&text, &redacted)?;
            // Findings never overlap, so none is ever partly redacted. Two
            // that happen to touch do merge into one, which is why this is a
            // containment check rather than a count.
            for span in &spans {
                prop_assert!(
                    redacted
                        .applied
                        .iter()
                        .any(|entry| entry.start <= span.start && span.end <= entry.end),
                    "{span:?} is not inside any of {:?}", redacted.applied
                );
            }
        }
    }

    /// Filtering by exposure gives a subset, never a different set. Cheap to
    /// state, and it pins the meaning of the ordering a UI filter relies on.
    #[test]
    fn filtering_by_exposure_only_ever_removes_findings(text in awkward_text()) {
        let all = inspect(&text);
        let strict: Vec<_> = all
            .iter()
            .copied()
            .filter(|finding| finding.exposure >= Exposure::Identifying)
            .collect();

        prop_assert!(strict.len() <= all.len());
        for finding in &strict {
            prop_assert!(all.contains(finding));
        }
    }

    /// Inspection reads no clock and no state either.
    #[test]
    fn inspection_is_deterministic(text in awkward_text()) {
        prop_assert_eq!(inspect(&text), inspect(&text));
    }
}
