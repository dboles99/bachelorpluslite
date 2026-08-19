//! Seam: `bp-files` load and atomic save, `bp-core`'s encoding and
//! line-ending model, and `bp-buffer`'s rope, joined into one open-edit-save-
//! reload cycle.
//!
//! Each of the three is tested alone and none of them has ever seen the other
//! two. The bug this exists to catch is the classic one for a text editor:
//! **a save that silently rewrites bytes the user never touched.** A CRLF
//! file that comes back LF, a BOM that quietly disappears, a file that gains
//! a trailing newline it never had. Every one of those is invisible in the
//! editor and obvious in a diff, a signature check or a build.
//!
//! The pipeline exercised here is exactly the one the shell runs:
//!
//! ```text
//! bp_files::load -> bp_buffer::Buffer -> edit -> render -> atomic_write -> load
//! ```
//!
//! One deliberate gap, and it is a finding rather than an omission. `bp-files`
//! has no encoding-aware writer: its only writer is `atomic_write(&[u8])`. The
//! function that turns a document plus its `Encoding` and `LineEnding` into
//! bytes is `bp_ui::state::encode`, which is `pub(crate)` inside the Slint
//! shell. So the *decision* cannot be reached from any integration test, and
//! what is checked below is that the pipeline is capable of being lossless
//! and that `bp-core`'s model describes the bytes correctly -- not that the
//! shell's encoder uses it correctly. See the report accompanying these
//! tests.

mod common;

use common::{Doc, no_files};

use bp_buffer::Buffer;
use bp_core::{Encoding, LineEnding};
use bp_files::{LoadedFile, SaveOptions, atomic_write, load};
use proptest::prelude::*;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Write `bytes` into a fresh temporary directory and hand back the path.
///
/// A `TempDir` is returned with it and must be held: dropping it deletes the
/// directory. Tests never write into the checkout -- one that did has already
/// cost this project real documents.
fn written(bytes: &[u8]) -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("document.txt");
    atomic_write(&path, bytes, SaveOptions::default()).expect("atomic_write");
    (dir, path)
}

/// The bytes `bp-core`'s model says a loaded file occupies on disk.
///
/// This is the whole of the model: the encoding contributes a byte-order
/// mark or nothing, and the text contributes itself. Nothing normalises
/// anything, which is the point.
fn as_bytes_on_disk(file: &LoadedFile) -> Vec<u8> {
    let mut out = file.encoding.bom().to_vec();
    out.extend_from_slice(file.text.as_bytes());
    out
}

/// Save the text of a document back over itself, as the shell's Save does.
fn save_over(path: &Path, encoding: Encoding, text: &str) {
    let mut bytes = encoding.bom().to_vec();
    bytes.extend_from_slice(text.as_bytes());
    atomic_write(path, &bytes, SaveOptions::default()).expect("atomic_write");
}

// --- generators ---------------------------------------------------------

/// Document text built from tokens rather than from arbitrary characters.
///
/// The tokens are chosen for the things that break a round trip: both line
/// breaks, a bare carriage return, multi-byte characters of two and three
/// bytes, a tab, and a trailing-newline decision made independently of the
/// body. Nothing user-specific ever reaches a failure report, and the
/// alphabet is visible right here, so `Doc`'s redacting `Debug` costs no
/// diagnostic value.
fn document() -> impl Strategy<Value = Doc> {
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
        0..40,
    )
    .prop_map(|parts| Doc(parts.concat()))
}

/// Text using one line-ending convention throughout, which is what a real
/// file almost always is.
///
/// Yields the convention alongside the text so a property can assert what
/// detection should have said.
fn uniform_document() -> impl Strategy<Value = (LineEnding, Doc)> {
    (
        prop_oneof![Just(LineEnding::Lf), Just(LineEnding::CrLf)],
        proptest::collection::vec(
            prop_oneof![Just("a"), Just("word "), Just("é"), Just("中"), Just("\t")],
            1..24,
        ),
    )
        .prop_map(|(ending, parts)| (ending, Doc(parts.join(ending.as_str()))))
}

// --- the properties -----------------------------------------------------

proptest! {
    #![proptest_config(no_files(256))]

    /// **Loading loses nothing.** The bytes on disk are exactly the encoding's
    /// byte-order mark followed by the text that was handed back, so a caller
    /// holding a `LoadedFile` holds everything needed to reproduce the file.
    ///
    /// This is the property that goes red the moment `load` normalises a line
    /// ending, drops a bare carriage return, or forgets which BOM it saw.
    #[test]
    fn loading_is_lossless(doc in document(), bom in any::<bool>()) {
        let mut bytes = if bom { Encoding::Utf8Bom.bom().to_vec() } else { Vec::new() };
        bytes.extend_from_slice(doc.as_bytes());
        let (_dir, path) = written(&bytes);

        let file = load(&path).expect("load");

        prop_assert_eq!(file.bytes_on_disk, bytes.len() as u64);
        prop_assert!(
            as_bytes_on_disk(&file) == bytes,
            "the model does not reproduce the {} bytes that were read",
            bytes.len()
        );
    }

    /// **The rope loses nothing.** Text through `Buffer` and back out is the
    /// same text, carriage returns, astral characters and all.
    #[test]
    fn the_rope_is_lossless(doc in document()) {
        prop_assert!(Buffer::from_text(doc.as_str()).to_string() == doc.0);
    }

    /// **Open, save, reload is byte-exact when nothing was edited.**
    ///
    /// A save with no edits must produce the file that was opened. Anything
    /// else is a program that modifies documents merely by being pointed at
    /// them, which is how an editor loses an hour of somebody's work to a
    /// version-control diff nobody can explain.
    #[test]
    fn a_save_with_no_edit_reproduces_the_file(doc in document(), bom in any::<bool>()) {
        let mut bytes = if bom { Encoding::Utf8Bom.bom().to_vec() } else { Vec::new() };
        bytes.extend_from_slice(doc.as_bytes());
        let (_dir, path) = written(&bytes);

        let opened = load(&path).expect("load");
        let buffer = Buffer::from_text(&opened.text);
        save_over(&path, opened.encoding, &buffer.to_string());
        let reloaded = load(&path).expect("reload");

        prop_assert!(reloaded == opened, "the document changed by being saved");
        prop_assert!(
            std::fs::read(&path).expect("read") == bytes,
            "the {} bytes on disk changed by being saved",
            bytes.len()
        );
    }

    /// **An edit through the rope lands at the character index it was given,
    /// and nothing else moves.**
    ///
    /// Character indices, not bytes: a byte index into a rope holding `é` or
    /// `😀` either panics or splits a character, and the reloaded file is
    /// where that shows up.
    #[test]
    fn an_edit_lands_exactly_where_it_was_asked_to(
        doc in document(),
        bom in any::<bool>(),
        position in 0.0f64..1.0,
    ) {
        let mut bytes = if bom { Encoding::Utf8Bom.bom().to_vec() } else { Vec::new() };
        bytes.extend_from_slice(doc.as_bytes());
        let (_dir, path) = written(&bytes);

        let opened = load(&path).expect("load");
        let chars = opened.text.chars().count();
        // Every index including one past the last character: appending is a
        // legitimate edit and the end is where an off-by-one lives.
        let at = ((chars + 1) as f64 * position) as usize;
        let at = at.min(chars);

        let insert = "MARK é 😀";
        let expected: String = opened
            .text
            .chars()
            .take(at)
            .chain(insert.chars())
            .chain(opened.text.chars().skip(at))
            .collect();

        let mut buffer = Buffer::from_text(&opened.text);
        buffer.insert(at, insert);
        prop_assert!(
            buffer.to_string() == expected,
            "the rope put a {} character insertion at character {} of {} somewhere else",
            insert.chars().count(),
            at,
            chars
        );

        save_over(&path, opened.encoding, &buffer.to_string());
        let reloaded = load(&path).expect("reload");

        prop_assert!(reloaded.text == expected, "the edited document did not survive the save");
        prop_assert_eq!(reloaded.encoding, opened.encoding, "the encoding changed");
    }

    /// **A file with no trailing newline does not gain one, and a file with
    /// one does not lose it.**
    ///
    /// Called out separately from byte-exactness because it is the single
    /// most common way an editor annoys a build system, and because it is
    /// the property most likely to be "fixed" into existence by a helpful
    /// change to a save path.
    #[test]
    fn the_trailing_newline_is_neither_invented_nor_removed(doc in document()) {
        let (_dir, path) = written(doc.as_bytes());
        let opened = load(&path).expect("load");
        let had_one = doc.as_str().ends_with('\n');

        save_over(&path, opened.encoding, &Buffer::from_text(&opened.text).to_string());
        let reloaded = load(&path).expect("reload");

        prop_assert_eq!(
            reloaded.text.ends_with('\n'),
            had_one,
            "the trailing newline was invented or removed"
        );
    }

    /// **A uniformly CRLF file stays CRLF; a uniformly LF file stays LF.**
    ///
    /// Both directions, because a conversion in either one is data loss, and
    /// the detection has to agree with the bytes: `bp-core` reports the
    /// convention the status bar shows, so a report that disagreed with the
    /// file would have the user choosing a save format on a false premise.
    #[test]
    fn a_uniform_line_ending_survives_open_edit_and_save((ending, doc) in uniform_document()) {
        let (_dir, path) = written(doc.as_bytes());
        let opened = load(&path).expect("load");

        // Detection only reports a convention when there is a break to see.
        if doc.as_str().contains('\n') {
            prop_assert_eq!(
                opened.line_ending,
                ending,
                "the line ending was detected as the other convention"
            );
        }

        let mut buffer = Buffer::from_text(&opened.text);
        buffer.insert(0, "edited ");
        save_over(&path, opened.encoding, &buffer.to_string());
        let reloaded = load(&path).expect("reload");

        prop_assert_eq!(
            reloaded.text.matches("\r\n").count(),
            doc.as_str().matches("\r\n").count(),
            "the number of CRLF breaks changed across a save"
        );
        prop_assert_eq!(
            reloaded.text.matches('\n').count(),
            doc.as_str().matches('\n').count(),
            "the number of line breaks changed across a save"
        );
    }
}

// --- the boundary cases, named --------------------------------------------

#[test]
fn a_byte_order_mark_survives_a_save() {
    let bytes = b"\xEF\xBB\xBFwith a mark\r\n";
    let (_dir, path) = written(bytes);

    let opened = load(&path).expect("load");
    assert_eq!(opened.encoding, Encoding::Utf8Bom);
    assert_eq!(
        opened.text, "with a mark\r\n",
        "the BOM is not buffer content"
    );

    save_over(&path, opened.encoding, &opened.text);

    assert_eq!(std::fs::read(&path).expect("read"), bytes);
}

#[test]
fn a_file_without_a_byte_order_mark_does_not_gain_one() {
    let bytes = b"no mark here\n";
    let (_dir, path) = written(bytes);

    let opened = load(&path).expect("load");
    assert_eq!(opened.encoding, Encoding::Utf8);

    save_over(&path, opened.encoding, &opened.text);

    assert_eq!(std::fs::read(&path).expect("read"), bytes);
}

#[test]
fn the_empty_document_round_trips() {
    let (_dir, path) = written(b"");

    let opened = load(&path).expect("load");
    assert_eq!(opened.text, "");
    assert_eq!(opened.bytes_on_disk, 0);
    // No break to detect, so the platform's convention is reported rather
    // than a guess.
    assert_eq!(opened.line_ending, LineEnding::platform_default());

    save_over(
        &path,
        opened.encoding,
        &Buffer::from_text(&opened.text).to_string(),
    );
    assert!(std::fs::read(&path).expect("read").is_empty());
}

#[test]
fn a_mixed_line_ending_document_is_not_converted_by_the_pipeline() {
    // The lines are deliberately unequal: `LineEnding::detect` resolves this
    // by majority, and the majority answer must not become an instruction to
    // rewrite the minority.
    let bytes = b"crlf\r\nlf one\nlf two\nlf three\ncrlf\r\n";
    let (_dir, path) = written(bytes);

    let opened = load(&path).expect("load");
    assert_eq!(opened.line_ending, LineEnding::Lf, "three LF, two CRLF");

    save_over(
        &path,
        opened.encoding,
        &Buffer::from_text(&opened.text).to_string(),
    );

    assert_eq!(
        std::fs::read(&path).expect("read"),
        bytes,
        "a mixed document was rewritten to one convention"
    );
}

#[test]
fn utf16_is_refused_rather_than_decoded_into_the_rope() {
    // The refusal is the feature. A lossy decode would let the user edit and
    // then save mangled text over the original, which is the one failure a
    // "cannot open this yet" message cannot cause.
    let (_dir, path) = written(b"\xFF\xFEh\0i\0");
    assert!(load(&path).is_err());
}
