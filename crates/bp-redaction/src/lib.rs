//! Redaction that destroys text, and inspection of the metadata a document
//! carries without being asked (specs.md section 15; "metadata leakage" is
//! one of section 24's threat categories).
//!
//! **A redaction here removes bytes.** [`redact`] returns a new `String` with
//! the named text gone out of it. Nothing in this crate draws a black
//! rectangle, sets a background colour, applies a style or attaches an
//! annotation. The failure that has embarrassed governments, law firms and
//! newspapers is always the same one: a "redaction" that covers text the
//! reader can select, copy, or recover by deleting a layer. There is nothing
//! to peel off here because there is nothing underneath -- the only way to
//! read what was redacted is to have kept the original.
//!
//! **What this crate cannot destroy, and must not be described as
//! destroying.** It rewrites one string. The file already on disk, the undo
//! stack, the recovery journal (ADR-0022), revision history, the clipboard,
//! the search index and every copy already sent are untouched, and each of
//! them is a place the original still lives. A shell that redacts a buffer
//! and saves over the original has dealt with exactly one of those. Telling
//! the user "redacted" without telling them that is the same lie the black
//! rectangle tells.
//!
//! **Spans come from somewhere else.** Finding is a different job from
//! destroying, so [`Span`]s are a parameter. `bp-secrets` finds credentials,
//! [`inspect`] finds identifying metadata, and the user finds the rest by
//! selecting it. This crate deliberately does *not* depend on `bp-secrets`:
//! it would gain nothing and would bind two release cycles together, and a
//! redaction engine that could only destroy what one particular scanner
//! recognises is a redaction engine the user cannot point at their own name.
//!
//! **Byte offsets, and what happens when they are wrong.** A [`Span`] is a
//! half-open range of *bytes*, because that is the only unit in which a
//! caller can name a boundary Rust will refuse to slice at, and refusing to
//! slice there is a panic in the user's face. Spans that split a character
//! are widened to whole characters; spans that overlap or touch are merged;
//! spans past the end of the text are refused, because offsets measured
//! against text that has since changed are offsets pointing at the wrong
//! words, and a confident redaction of the wrong words is worse than an
//! error message. [`redact`] lists the whole rule set in one place.
//!
//! Pure, like `bp-secrets` and `bp-security`: no clock, no IO, no
//! configuration, no randomness. The same text and the same spans give the
//! same output on every machine, which is what makes a redaction something a
//! second person can check.

#![forbid(unsafe_code)]

mod metadata;

pub use metadata::{Container, Exposure, MetadataFinding, MetadataKind, inspect};

/// Crate identity used by workspace smoke tests and diagnostics.
pub const CRATE_NAME: &str = "bp-redaction";

/// The marker [`Replacement::Placeholder`] leaves when a redaction carries no
/// label.
///
/// Public so a caller can find previous redactions in a document -- to count
/// them, to refuse to sign a document that still contains one, or to warn
/// before publishing. A labelled redaction is this word followed by `: ` and
/// the labels, still inside the same brackets, so `text.contains("[REDACTED")`
/// finds both.
pub const PLACEHOLDER: &str = "[REDACTED]";

/// A stretch of the document to destroy, named in bytes, plus what it was.
///
/// Half-open: `start..end`, so an empty span is `start == end` and a span
/// covering everything is `0..text.len()`. Bytes rather than characters
/// because a caller computing offsets from a parser, a search or a scanner
/// has byte offsets, and converting to characters first would be a second
/// place for an off-by-one to live.
///
/// `label` says *what kind of thing* was here -- "AWS key", "client name" --
/// and it must never be the text itself. It is written into the document by
/// [`Replacement::Placeholder`], so a label carrying the secret would put the
/// secret back. This type cannot enforce that; the caller can, by passing the
/// name of the rule that matched rather than what it matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span<'a> {
    /// First byte to destroy.
    pub start: usize,
    /// One past the last byte to destroy.
    pub end: usize,
    /// What was here, in words. Never the text that was here.
    pub label: Option<&'a str>,
}

impl Span<'_> {
    /// A span with nothing to say about what it covers.
    ///
    /// The common case: a user dragged a selection, and the only honest label
    /// for it is none at all.
    #[must_use]
    pub const fn new(start: usize, end: usize) -> Self {
        Self {
            start,
            end,
            label: None,
        }
    }

    /// The number of bytes named, before any widening or merging.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    /// Does this span name nothing?
    ///
    /// Worth asking before building a UI message: an empty span is accepted
    /// and then ignored, so "1 redaction" would be a lie.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.start >= self.end
    }
}

impl<'a> Span<'a> {
    /// A span that knows what it covers, for [`Replacement::Placeholder`].
    ///
    /// Separate constructor rather than an argument to [`Span::new`] because
    /// most spans have no label and a trailing `None` at every call site is
    /// noise that hides the ones that do.
    #[must_use]
    pub const fn labelled(start: usize, end: usize, label: &'a str) -> Self {
        Self {
            start,
            end,
            label: Some(label),
        }
    }
}

/// What is left where the text was.
///
/// Three modes, because they answer three different questions and no one of
/// them is right twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Replacement {
    /// Take the text out and close the gap.
    ///
    /// The document reads as if the words were never written. Right when the
    /// redaction should not be visible at all -- a key pasted into a note by
    /// accident, a paragraph the author decided against -- and wrong when the
    /// reader is entitled to know something was withheld, because it hides
    /// the withholding as well as the text.
    Remove,

    /// Replace the text with a run of one character.
    ///
    /// Right where the document's shape matters -- a table, a fixed-column
    /// listing, a screenshot-like block -- or where a visible bar is the
    /// convention the reader expects. It leaves a mark without naming what
    /// was there, which is either discretion or an unanswered question
    /// depending on the reader.
    ///
    /// `fill` is written verbatim: a newline or a control character here will
    /// damage the document's structure, and this crate will not stop you.
    Mask {
        /// The character to repeat. `'\u{2588}'` (a full block) and `'*'` are
        /// the conventional choices.
        fill: char,
        /// How many of them.
        width: MaskWidth,
    },

    /// Replace the text with [`PLACEHOLDER`], or `[REDACTED: label]` when the
    /// merged span carries labels.
    ///
    /// Right for a document that will be read by someone who needs to know a
    /// redaction happened, and often what kind -- a disclosure, a shared
    /// note, a bug report. The cost is exactly that: `[REDACTED: AWS key]`
    /// tells the reader an AWS key was there, which is information the empty
    /// gap of [`Replacement::Remove`] does not give away. Choose the label
    /// with that in mind.
    Placeholder,
}

/// How wide a [`Replacement::Mask`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MaskWidth {
    /// Always this many characters, whatever was removed.
    ///
    /// The safe choice: the mark tells the reader that something was here and
    /// nothing else. `Fixed(0)` is legal and produces no mark at all, which
    /// is [`Replacement::Remove`] with extra steps -- it is allowed rather
    /// than rejected because a caller computing the width from a setting
    /// should not have to special-case zero, but it is worth knowing that it
    /// leaves the redaction invisible.
    Fixed(usize),

    /// One `fill` per character removed.
    ///
    /// Keeps columns aligned, and discloses the length of what was removed.
    /// For a name that is nearly nothing; for a password, a PIN or a
    /// short answer from a fixed set of options it can be most of the secret.
    /// Counts *characters*, not bytes, so an accented letter or an ideograph
    /// costs one mask character rather than two or three.
    MatchOriginal,
}

/// Why a set of spans could not be applied.
///
/// Both variants mean the caller's offsets do not describe the text it passed,
/// which is nearly always one bug: the text changed after the spans were
/// computed. Neither is recoverable by guessing, and guessing is the thing to
/// avoid -- redacting a clamped or reversed range would destroy text nobody
/// named while reporting success.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RedactionError {
    /// A span whose end is before its start.
    #[error("a redaction was asked for from byte {start} to byte {end}, which is before it")]
    Inverted {
        /// The span's start.
        start: usize,
        /// The span's end, which is smaller.
        end: usize,
    },

    /// A span reaching past the end of the text.
    #[error(
        "a redaction was asked for up to byte {end} of a document that is {length} bytes long; \
         these positions were measured against different text, so none of them can be trusted"
    )]
    PastEnd {
        /// Where the span ended.
        end: usize,
        /// How long the text actually is.
        length: usize,
    },
}

/// One redaction as it was actually carried out.
///
/// Deliberately holds no removed text. It is the audit record -- something a
/// caller may log, show in a panel or write to the security history in
/// specs.md section 21 -- and a record that carried what it destroyed would
/// be a new place the secret lives, which is the argument `bp-secrets` makes
/// about `Finding` for the same reason.
///
/// `start..end` index the original text and `output_start..output_end` index
/// the redacted text, so a caller can map a highlight from one to the other
/// without re-deriving anything. Both are on character boundaries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    /// First byte destroyed, in the original text, after widening and merging.
    pub start: usize,
    /// One past the last byte destroyed, in the original text.
    pub end: usize,
    /// Where the replacement begins in the redacted text.
    pub output_start: usize,
    /// One past where the replacement ends in the redacted text.
    pub output_end: usize,
    /// Whether the span had to grow to reach character boundaries.
    ///
    /// True means more was destroyed than the caller named. Harmless when it
    /// happens -- the extra is the remainder of a character the span had cut
    /// in half -- but worth surfacing, because it is also the symptom of a
    /// caller computing offsets in the wrong unit.
    pub widened: bool,
    /// The labels of the spans that merged into this one, joined with `", "`.
    ///
    /// `None` when none of them carried one.
    pub label: Option<String>,
}

/// The result of a redaction: the new text, and what was done to produce it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Redacted {
    /// The document with the spans destroyed.
    pub text: String,
    /// One entry per redaction, in document order, after merging. Shorter
    /// than the span list whenever spans overlapped, touched or were empty.
    pub applied: Vec<Applied>,
}

/// The result of checking that a redaction worked.
///
/// See [`verify`] for what this does and does not establish; the distinction
/// is the whole value of the type, and a caller that reports `is_clean` as
/// "the secret is gone" has overstated it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verification {
    /// How many redactions were checked. Matches `Redacted::applied.len()`.
    pub checked: usize,
    /// Indices into [`Redacted::applied`] whose removed text still occurs
    /// somewhere in the output.
    ///
    /// Indices rather than text, for the same reason [`Applied`] carries
    /// none: a report that quotes the secret it failed to remove is a leak
    /// wearing the clothes of a safety feature.
    pub surviving: Vec<usize>,
}

impl Verification {
    /// Did every redaction's text disappear from the output?
    ///
    /// A `true` here is a necessary condition for a good redaction, not a
    /// sufficient one -- read [`verify`].
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.surviving.is_empty()
    }
}

/// Destroy `spans` in `text`, leaving `replacement` behind.
///
/// The returned [`Redacted::text`] is a fresh `String`; the input is borrowed
/// and unchanged, so a caller that wants the original gone must also drop it.
///
/// # The rules the spans are put through
///
/// 1. **An inverted span is an error.** `end < start` cannot be honoured any
///    way that is not a guess.
/// 2. **A span past the end of the text is an error.** It proves the offsets
///    were measured against different text, which makes every other span in
///    the set suspect too.
/// 3. **An empty span is ignored.** It names no text, so there is nothing to
///    destroy and nothing to announce. It is dropped *before* widening, so
///    that a zero-width position inside a multi-byte character cannot grow
///    into the destruction of a character nobody named.
/// 4. **A span that splits a character is widened to whole characters.**
///    Outwards, never inwards: the direction that destroys more, because
///    leaving a fragment of what was named is the failure that matters, and
///    slicing at the split byte would panic instead.
/// 5. **Spans that overlap or merely touch are merged.** Applying them one
///    after another is where the classic bug lives -- the first replacement
///    moves every later offset -- and rejecting them would mean failing at
///    exactly the moment two rules agree there is something to remove.
///    Merging also makes the output independent of how the caller happened to
///    split its findings: one span, or the same span cut in two, redacts
///    identically.
/// 6. **Order does not matter.** Spans are sorted first, so a caller need not.
/// 7. **Labels of merged spans are all kept**, in first-seen order and
///    without duplicates, because dropping all but one would misdescribe the
///    redaction in the document itself.
///
/// Linear in the length of the text plus `n log n` in the number of spans,
/// with one pass over the text and no re-scanning: the offsets are all
/// resolved against the original before a single byte is copied, which is the
/// reason overlapping spans cannot shift each other.
///
/// # Errors
///
/// [`RedactionError`] if any span is inverted or reaches past the end of
/// `text`. Nothing is redacted in that case -- a partial redaction reported as
/// a failure is the worst of both.
pub fn redact(
    text: &str,
    spans: &[Span<'_>],
    replacement: Replacement,
) -> Result<Redacted, RedactionError> {
    let planned = plan(text, spans)?;

    let mut out = String::with_capacity(text.len());
    let mut applied = Vec::with_capacity(planned.len());
    let mut cursor = 0usize;

    for redaction in &planned {
        out.push_str(&text[cursor..redaction.start]);
        let output_start = out.len();
        let label = join_labels(&redaction.labels);
        write_replacement(
            &mut out,
            &text[redaction.start..redaction.end],
            label.as_deref(),
            replacement,
        );
        applied.push(Applied {
            start: redaction.start,
            end: redaction.end,
            output_start,
            output_end: out.len(),
            widened: redaction.widened,
            label,
        });
        cursor = redaction.end;
    }
    out.push_str(&text[cursor..]);

    Ok(Redacted { text: out, applied })
}

/// Check that nothing `spans` named survives in `output`.
///
/// Pass the *same* `original` and `spans` that were given to [`redact`], and
/// the text it returned. This re-derives what was removed from the original
/// rather than taking it as an argument, so that no caller has to hold the
/// secret in a variable in order to check for it.
///
/// # What a clean result proves
///
/// That the exact byte sequence occupying each redacted span in the original
/// does not occur anywhere in the output. For a credential, a name or an
/// address -- text that is meant to be matched literally -- that is the
/// question worth asking, and it catches the whole family of real failures:
/// a mask written over the top without deleting, a replacement applied at a
/// stale offset, a copy of the value elsewhere in the same document, a mode
/// that turned out to be a no-op.
///
/// # What it does not prove
///
/// * **Not that the information is gone.** Only the literal bytes are
///   searched for. The same secret base64-encoded, percent-encoded, in
///   another case, split across a line break, or described in a sentence is
///   invisible to this check.
/// * **Not that a survivor is a failure.** If the same text also appears
///   somewhere the caller did not name -- the key pasted twice, a name in the
///   title as well as the body -- it is reported as surviving even though
///   every named span was destroyed. That is the correct alarm to raise, but
///   it is a reason to look, not a verdict.
/// * **Not that the spans were the right spans.** A caller that named the
///   wrong bytes gets a clean verification of a redaction that removed the
///   wrong text. Nothing here can know what should have been removed.
/// * **Nothing about the file.** The original document, earlier versions, the
///   recovery journal, revision history, the undo stack, the clipboard,
///   backups and any copy already sent are all outside this string and all
///   still hold the original.
///
/// # Errors
///
/// [`RedactionError`], on the same terms as [`redact`]. If `redact` succeeded
/// with these arguments, this cannot fail.
pub fn verify(
    original: &str,
    spans: &[Span<'_>],
    output: &str,
) -> Result<Verification, RedactionError> {
    let planned = plan(original, spans)?;

    // A plan never contains an empty redaction -- empty spans are dropped and
    // merging only grows ranges -- which matters here, because every string
    // contains the empty string and a single empty span would make every
    // verification fail for ever.
    let surviving = planned
        .iter()
        .enumerate()
        .filter(|(_, redaction)| output.contains(&original[redaction.start..redaction.end]))
        .map(|(index, _)| index)
        .collect();

    Ok(Verification {
        checked: planned.len(),
        surviving,
    })
}

// --- internals ---------------------------------------------------------

/// A span after validation, widening and merging: what will actually happen.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Planned<'a> {
    start: usize,
    end: usize,
    widened: bool,
    labels: Vec<&'a str>,
}

/// Apply the rules listed on [`redact`] and return the redactions in
/// document order.
fn plan<'a>(text: &str, spans: &[Span<'a>]) -> Result<Vec<Planned<'a>>, RedactionError> {
    let mut prepared: Vec<Planned<'a>> = Vec::with_capacity(spans.len());

    for span in spans {
        if span.end < span.start {
            return Err(RedactionError::Inverted {
                start: span.start,
                end: span.end,
            });
        }
        if span.end > text.len() {
            return Err(RedactionError::PastEnd {
                end: span.end,
                length: text.len(),
            });
        }
        // Rule 3, and it must come before rule 4: widening a zero-width
        // position that happens to sit inside a character would destroy a
        // character the caller never named.
        if span.start == span.end {
            continue;
        }

        // Rule 4. Both walks terminate: byte 0 and `text.len()` are always
        // character boundaries.
        let mut start = span.start;
        while !text.is_char_boundary(start) {
            start -= 1;
        }
        let mut end = span.end;
        while !text.is_char_boundary(end) {
            end += 1;
        }

        prepared.push(Planned {
            start,
            end,
            widened: start != span.start || end != span.end,
            labels: span.label.into_iter().collect(),
        });
    }

    // Rule 6. The label is part of the sort key only to break a tie between
    // two spans covering the identical range: without it the merged label
    // would read in whichever order the caller happened to pass them, and
    // "the order does not matter" would be true of the text and false of the
    // audit record.
    prepared.sort_by(|left, right| {
        (left.start, left.end, left.labels.first()).cmp(&(
            right.start,
            right.end,
            right.labels.first(),
        ))
    });

    // Rule 5, in one pass over the sorted list. `<=` rather than `<` is what
    // makes touching spans merge as well as overlapping ones.
    let mut merged: Vec<Planned<'a>> = Vec::with_capacity(prepared.len());
    for planned in prepared {
        match merged.last_mut() {
            Some(previous) if planned.start <= previous.end => {
                previous.end = previous.end.max(planned.end);
                previous.widened |= planned.widened;
                // Rule 7.
                for label in planned.labels {
                    if !previous.labels.contains(&label) {
                        previous.labels.push(label);
                    }
                }
            }
            _ => merged.push(planned),
        }
    }

    Ok(merged)
}

/// Strip a label down to something that cannot forge or break a placeholder.
///
/// Brackets and control characters are removed and runs of whitespace are
/// collapsed, because a label reaches the document verbatim: a `]` in one
/// would let a rule name -- or a filename, or anything else a caller passes
/// through -- close the marker early and write arbitrary text outside it, and
/// a newline in one would split a line and move every position after it.
/// A label left with nothing in it is treated as no label at all.
fn clean_label(label: &str) -> Option<String> {
    let mut out = String::with_capacity(label.len());
    for word in label.split_whitespace() {
        let word: String = word
            .chars()
            .filter(|c| !matches!(c, '[' | ']') && !c.is_control())
            .collect();
        if word.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&word);
    }
    (!out.is_empty()).then_some(out)
}

/// Join a merged redaction's labels into the one string that describes it.
fn join_labels(labels: &[&str]) -> Option<String> {
    let cleaned: Vec<String> = labels
        .iter()
        .filter_map(|label| clean_label(label))
        .collect();
    (!cleaned.is_empty()).then(|| cleaned.join(", "))
}

/// Write what stands in for `removed`.
///
/// `removed` is read only to count characters; it is never copied into the
/// output, which is the property the whole crate exists to provide.
fn write_replacement(out: &mut String, removed: &str, label: Option<&str>, mode: Replacement) {
    match mode {
        Replacement::Remove => {}
        Replacement::Mask { fill, width } => {
            let count = match width {
                MaskWidth::Fixed(fixed) => fixed,
                MaskWidth::MatchOriginal => removed.chars().count(),
            };
            out.extend(std::iter::repeat_n(fill, count));
        }
        Replacement::Placeholder => match label {
            Some(label) => {
                out.push_str("[REDACTED: ");
                out.push_str(label);
                out.push(']');
            }
            None => out.push_str(PLACEHOLDER),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLOCK: Replacement = Replacement::Mask {
        fill: '\u{2588}',
        width: MaskWidth::MatchOriginal,
    };

    /// Redact and unwrap, for the many tests where the spans are obviously
    /// valid and the interesting part is the text that comes out.
    fn text_of(text: &str, spans: &[Span<'_>], mode: Replacement) -> String {
        redact(text, spans, mode).expect("valid spans").text
    }

    #[test]
    fn removing_a_span_deletes_exactly_it_and_closes_the_gap() {
        let text = "the key is AKIA1234 and nothing else";
        let start = text.find("AKIA1234").expect("needle");
        let span = Span::new(start, start + "AKIA1234".len());

        let redacted = redact(text, &[span], Replacement::Remove).expect("valid");
        assert_eq!(redacted.text, "the key is  and nothing else");
        assert_eq!(redacted.applied.len(), 1);
        assert_eq!(redacted.applied[0].start, start);
        assert_eq!(redacted.applied[0].output_start, start);
        assert_eq!(
            redacted.applied[0].output_end, start,
            "a removal occupies no output"
        );
        assert!(!redacted.applied[0].widened);
    }

    #[test]
    fn the_three_modes_differ_only_in_what_is_left_behind() {
        let text = "name: Daniel";
        let span = Span::labelled(6, 12, "author");

        assert_eq!(text_of(text, &[span], Replacement::Remove), "name: ");
        assert_eq!(
            text_of(text, &[span], BLOCK),
            "name: \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}"
        );
        assert_eq!(
            text_of(
                text,
                &[span],
                Replacement::Mask {
                    fill: '*',
                    width: MaskWidth::Fixed(3),
                }
            ),
            "name: ***"
        );
        assert_eq!(
            text_of(text, &[span], Replacement::Placeholder),
            "name: [REDACTED: author]"
        );
        assert_eq!(
            text_of(text, &[Span::new(6, 12)], Replacement::Placeholder),
            format!("name: {PLACEHOLDER}"),
            "an unlabelled span gets the bare marker"
        );
    }

    #[test]
    fn a_mask_width_counts_characters_rather_than_bytes() {
        // Three characters, seven bytes. A byte count here would draw seven
        // blocks over three columns and wreck any layout the mask was chosen
        // to preserve.
        let text = "x\u{e9}\u{4e2d}\u{1f600}y";
        let redacted = text_of(text, &[Span::new(1, text.len() - 1)], BLOCK);
        assert_eq!(redacted, "x\u{2588}\u{2588}\u{2588}y");
    }

    #[test]
    fn a_zero_width_mask_leaves_no_mark_at_all() {
        // Legal, and worth pinning: a caller reading the width from a setting
        // must not have to special-case zero, but the result is a redaction
        // the reader cannot see happened.
        let text = "before secret after";
        let masked = text_of(
            text,
            &[Span::new(7, 13)],
            Replacement::Mask {
                fill: '*',
                width: MaskWidth::Fixed(0),
            },
        );
        assert_eq!(masked, "before  after");
        assert_eq!(
            masked,
            text_of(text, &[Span::new(7, 13)], Replacement::Remove)
        );
    }

    #[test]
    fn overlapping_spans_become_one_redaction_over_their_union() {
        let text = "0123456789";
        let redacted = redact(
            text,
            &[Span::new(2, 6), Span::new(4, 8)],
            Replacement::Placeholder,
        )
        .expect("valid");

        assert_eq!(redacted.applied.len(), 1, "{:?}", redacted.applied);
        assert_eq!((redacted.applied[0].start, redacted.applied[0].end), (2, 8));
        assert_eq!(redacted.text, format!("01{PLACEHOLDER}89"));
    }

    #[test]
    fn a_span_wholly_inside_another_disappears_into_it() {
        let text = "0123456789";
        let redacted = redact(
            text,
            &[Span::new(1, 9), Span::new(3, 4)],
            Replacement::Remove,
        )
        .expect("valid");

        assert_eq!(redacted.applied.len(), 1);
        assert_eq!((redacted.applied[0].start, redacted.applied[0].end), (1, 9));
        assert_eq!(redacted.text, "09");
    }

    #[test]
    fn touching_spans_merge_and_a_one_byte_gap_does_not() {
        let text = "0123456789";

        // The boundary: 2..4 and 4..6 share an edge and merge.
        let touching = redact(
            text,
            &[Span::new(2, 4), Span::new(4, 6)],
            Replacement::Placeholder,
        )
        .expect("valid");
        assert_eq!(touching.applied.len(), 1);
        assert_eq!(touching.text, format!("01{PLACEHOLDER}6789"));

        // One past it: 2..4 and 5..6 leave byte 4 alone and stay two.
        let apart = redact(
            text,
            &[Span::new(2, 4), Span::new(5, 6)],
            Replacement::Placeholder,
        )
        .expect("valid");
        assert_eq!(apart.applied.len(), 2);
        assert_eq!(apart.text, format!("01{PLACEHOLDER}4{PLACEHOLDER}6789"));
    }

    #[test]
    fn splitting_a_span_in_two_changes_nothing() {
        // The reason adjacency merges: the caller's arbitrary decision about
        // where one finding ends and the next begins must not be visible in
        // the output.
        let text = "alpha bravo charlie";
        let whole = text_of(text, &[Span::new(6, 11)], Replacement::Placeholder);
        let split = text_of(
            text,
            &[Span::new(6, 8), Span::new(8, 11)],
            Replacement::Placeholder,
        );
        assert_eq!(whole, split);
    }

    #[test]
    fn spans_out_of_order_give_the_same_answer_as_spans_in_order() {
        let text = "one two three four";
        let ordered = [Span::new(0, 3), Span::new(4, 7), Span::new(8, 13)];
        let jumbled = [Span::new(8, 13), Span::new(0, 3), Span::new(4, 7)];

        assert_eq!(
            redact(text, &ordered, Replacement::Placeholder).expect("valid"),
            redact(text, &jumbled, Replacement::Placeholder).expect("valid")
        );
    }

    #[test]
    fn an_empty_span_names_nothing_and_so_redacts_nothing() {
        let text = "unchanged";
        for at in [0, 4, text.len()] {
            let redacted =
                redact(text, &[Span::new(at, at)], Replacement::Placeholder).expect("valid");
            assert_eq!(redacted.text, text, "at {at}");
            assert!(redacted.applied.is_empty(), "at {at}");
        }
    }

    #[test]
    fn an_empty_span_inside_a_character_does_not_grow_into_one() {
        // The reason empty spans are dropped before widening: byte 1 is
        // inside the two-byte 'e-acute', and widening 1..1 would have
        // destroyed a character nobody asked about.
        let text = "\u{e9}x";
        let redacted = redact(text, &[Span::new(1, 1)], Replacement::Remove).expect("valid");
        assert_eq!(redacted.text, text);
        assert!(redacted.applied.is_empty());
    }

    #[test]
    fn a_span_splitting_a_character_widens_outwards_to_whole_characters() {
        // Four bytes, one character. A span of 1..3 names the middle of it,
        // which Rust will not slice; widening inwards would leave a lone
        // surrogate-ish fragment of the very thing being destroyed.
        let text = "a\u{1f511}b";
        let redacted = redact(text, &[Span::new(2, 4)], Replacement::Remove).expect("valid");

        assert_eq!(redacted.text, "ab");
        assert_eq!(redacted.applied.len(), 1);
        assert_eq!((redacted.applied[0].start, redacted.applied[0].end), (1, 5));
        assert!(
            redacted.applied[0].widened,
            "the caller is told it got more than it named"
        );
    }

    #[test]
    fn a_span_past_the_end_is_refused_and_says_why() {
        let text = "short";
        let error = redact(text, &[Span::new(0, 99)], Replacement::Remove).expect_err("past end");

        assert_eq!(error, RedactionError::PastEnd { end: 99, length: 5 });
        let message = error.to_string();
        assert!(message.contains("99") && message.contains('5'), "{message}");
        assert!(
            message.contains("measured against different text"),
            "the message has to point at the real cause: {message}"
        );

        // The boundary itself is fine, and one past it is not.
        assert!(redact(text, &[Span::new(0, 5)], Replacement::Remove).is_ok());
        assert!(redact(text, &[Span::new(0, 6)], Replacement::Remove).is_err());
    }

    #[test]
    fn an_inverted_span_is_refused_rather_than_guessed_at() {
        let error =
            redact("0123456789", &[Span::new(7, 3)], Replacement::Remove).expect_err("inverted");
        assert_eq!(error, RedactionError::Inverted { start: 7, end: 3 });
    }

    #[test]
    fn one_bad_span_redacts_nothing_at_all() {
        // A partial redaction reported as a failure is the worst of both: the
        // document is damaged and the caller believes it is untouched.
        let text = "alpha bravo";
        let spans = [Span::new(0, 5), Span::new(0, 99)];
        assert!(redact(text, &spans, Replacement::Remove).is_err());
    }

    #[test]
    fn a_label_cannot_forge_or_break_a_placeholder() {
        let text = "0123456789";
        let hostile = "evil] and \u{0}\n\n  more   words";
        let redacted = text_of(
            text,
            &[Span::labelled(2, 8, hostile)],
            Replacement::Placeholder,
        );

        assert_eq!(redacted, "01[REDACTED: evil and more words]89");
        assert_eq!(
            redacted.matches(']').count(),
            1,
            "a bracket in a label must not close the marker early"
        );
        assert!(!redacted.contains('\n'), "a label cannot add a line");
    }

    #[test]
    fn a_label_with_nothing_in_it_is_no_label() {
        let redacted = text_of(
            "0123456789",
            &[Span::labelled(2, 8, "  [] \t ")],
            Replacement::Placeholder,
        );
        assert_eq!(redacted, format!("01{PLACEHOLDER}89"));
    }

    #[test]
    fn merged_labels_are_listed_once_each_in_the_order_they_arrived() {
        let text = "0123456789";
        let spans = [
            Span::labelled(2, 5, "AWS key"),
            Span::labelled(4, 7, "GitHub token"),
            Span::labelled(6, 8, "AWS key"),
        ];
        let redacted = redact(text, &spans, Replacement::Placeholder).expect("valid");

        assert_eq!(redacted.text, "01[REDACTED: AWS key, GitHub token]89");
        assert_eq!(
            redacted.applied[0].label.as_deref(),
            Some("AWS key, GitHub token"),
            "the audit record says both, not one"
        );
    }

    #[test]
    fn redacting_the_whole_document_leaves_nothing_of_it() {
        let text = "every last word of it, \u{4e2d}\u{6587} included";
        let whole = [Span::new(0, text.len())];

        assert_eq!(text_of(text, &whole, Replacement::Remove), "");
        assert_eq!(
            text_of(text, &whole, Replacement::Placeholder),
            PLACEHOLDER,
            "nothing but the marker"
        );

        let masked = text_of(text, &whole, BLOCK);
        assert!(masked.chars().all(|c| c == '\u{2588}'), "{masked}");
        assert_eq!(masked.chars().count(), text.chars().count());
    }

    #[test]
    fn an_empty_document_and_an_empty_span_list_are_both_fine() {
        assert_eq!(text_of("", &[], Replacement::Remove), "");
        assert_eq!(text_of("", &[Span::new(0, 0)], Replacement::Remove), "");
        assert_eq!(
            text_of("unchanged", &[], Replacement::Placeholder),
            "unchanged"
        );
    }

    #[test]
    fn applied_ranges_point_at_the_replacement_in_the_output() {
        let text = "alpha bravo charlie delta";
        let spans = [Span::new(6, 11), Span::new(20, 25)];
        let redacted = redact(text, &spans, Replacement::Placeholder).expect("valid");

        for entry in &redacted.applied {
            assert_eq!(
                &redacted.text[entry.output_start..entry.output_end],
                PLACEHOLDER
            );
        }
    }

    #[test]
    fn verification_is_clean_when_the_secret_is_gone() {
        let text = "token=ghp_R2d9KpXvA7mQzL3nB8wYtE6sJ1uH0cVfNgD4 done";
        let spans = [Span::new(6, 46)];
        let redacted = redact(text, &spans, Replacement::Placeholder).expect("valid");
        let checked = verify(text, &spans, &redacted.text).expect("valid");

        assert!(checked.is_clean());
        assert_eq!(checked.checked, 1);
        assert_eq!(checked.surviving, Vec::<usize>::new());
    }

    #[test]
    fn verification_catches_a_redaction_that_only_covered_the_text() {
        // The failure the whole feature exists for: something that looks
        // redacted and still carries the value.
        let text = "password: hunter2000";
        let spans = [Span::new(10, 20)];
        let pretend = "password: \u{2588}\u{2588}\u{2588} (hunter2000)";

        let checked = verify(text, &spans, pretend).expect("valid");
        assert!(!checked.is_clean());
        assert_eq!(checked.surviving, vec![0]);
    }

    #[test]
    fn verification_names_which_redaction_survived_not_merely_that_one_did() {
        // Three redactions, and the one that failed is neither the first nor
        // the last. A check that stopped after finding an answer, or that only
        // looked at the first span, would report a clean document here.
        let text = "one ALPHA two BRAVO three CHARLIE four";
        let spans = [Span::new(4, 9), Span::new(14, 19), Span::new(26, 33)];
        let half_done = "one  two BRAVO three  four";

        let checked = verify(text, &spans, half_done).expect("valid");
        assert_eq!(checked.checked, 3);
        assert_eq!(checked.surviving, vec![1], "the middle one, by index");
        assert!(!checked.is_clean());
    }

    #[test]
    fn verification_reports_every_survivor_when_nothing_was_redacted_at_all() {
        // The degenerate case a caller can reach by passing the original text
        // back by mistake: every redaction must be reported, not just one.
        let text = "one ALPHA two BRAVO three CHARLIE four";
        let spans = [Span::new(4, 9), Span::new(14, 19), Span::new(26, 33)];

        let checked = verify(text, &spans, text).expect("valid");
        assert_eq!(checked.surviving, vec![0, 1, 2]);
    }

    #[test]
    fn verification_cannot_tell_a_second_copy_from_a_failure() {
        // Documented limitation, pinned as a test so it cannot quietly change
        // into a claim the crate does not support. The redaction succeeded;
        // the value is still in the document because it was written twice.
        let text = "key: SECRETVALUE\nbackup key: SECRETVALUE";
        let spans = [Span::new(5, 16)];
        let redacted = redact(text, &spans, Replacement::Remove).expect("valid");

        assert_eq!(redacted.text, "key: \nbackup key: SECRETVALUE");
        let checked = verify(text, &spans, &redacted.text).expect("valid");
        assert!(
            !checked.is_clean(),
            "surviving means look, not necessarily failed"
        );
    }

    #[test]
    fn verification_says_nothing_about_a_re_encoded_secret() {
        // The other half of the limitation: base64 of the same value is not
        // searched for, and a clean result here does not mean the information
        // is gone.
        let text = "key: SECRET base64: U0VDUkVU";
        let spans = [Span::new(5, 11)];
        let redacted = redact(text, &spans, Replacement::Remove).expect("valid");
        assert!(
            verify(text, &spans, &redacted.text)
                .expect("valid")
                .is_clean()
        );
        assert!(redacted.text.contains("U0VDUkVU"));
    }

    #[test]
    fn verification_of_an_empty_span_list_is_clean_rather_than_meaningless() {
        let checked = verify("anything", &[], "anything").expect("valid");
        assert_eq!(checked.checked, 0);
        assert!(
            checked.is_clean(),
            "nothing was asked for and nothing failed"
        );
    }

    #[test]
    fn a_plan_never_contains_an_empty_redaction() {
        // The invariant `verify` leans on: every string contains the empty
        // string, so one empty range in a plan would make verification always
        // report a survivor.
        let text = "a\u{e9}b\u{1f511}c";
        let spans = [
            Span::new(0, 0),
            Span::new(3, 3),
            Span::new(2, 2),
            Span::new(1, 2),
        ];
        for redaction in plan(text, &spans).expect("valid") {
            assert!(redaction.start < redaction.end, "{redaction:?}");
        }
    }

    #[test]
    fn odd_input_is_redacted_without_panicking() {
        for text in [
            "",
            "\n",
            "\u{0}\u{1}\u{7f}",
            "\u{feff}leading mark",
            "\u{1f511}\u{1f511}\u{1f511}",
            "\u{65e5}\u{672c}\u{8a9e}",
        ] {
            for start in 0..=text.len() {
                for end in 0..=text.len() {
                    let span = Span::labelled(start, end, "label");
                    let _ = redact(text, &[span], Replacement::Placeholder);
                    let _ = redact(text, &[span], Replacement::Remove);
                    let _ = redact(text, &[span], BLOCK);
                }
            }
        }
    }
}
