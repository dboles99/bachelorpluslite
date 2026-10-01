//! Target 5: `bp_files::load` on arbitrary bytes, including the UTF-16 path.
//!
//! `load` is the first thing that touches a file the user picked, before any
//! format has been decided or any parser has run. It decides an encoding from
//! a byte-order mark and decodes accordingly, and the UTF-16 decoder is
//! hand-written -- `bp_files::utf16` says so, and explains why -- which makes
//! it exactly the kind of code this target exists for.
//!
//! Two of its errors are named in the crate docs and both must stay errors:
//!
//! * `TruncatedUtf16` -- an odd number of body bytes, so the last code unit
//!   is cut in half;
//! * `UnpairedSurrogate` -- a surrogate with no partner, where the only
//!   available character is U+FFFD and substituting it "would hand back a
//!   document that looks intact and writes the damage back on the next save".
//!
//! The invariant: **it may return a `LoadError`; it may not panic, abort, or
//! hang.** And one more, because this crate makes a stronger promise than the
//! others: **anything that loads must encode back to exactly the bytes on
//! disk.** "Nothing is ever substituted, so anything that loads is exactly
//! what was on disk and `encode` writes it back byte for byte."

use std::io::Write;
use std::time::Duration;

use bp_files::LineEndingPolicy;
use bp_fuzz::{
    Outcome, Probe, assert_no_violations, corpus, on_app_stack, proptest_config, strategies,
};
use proptest::prelude::*;

/// A directory to write samples into, and one path inside it.
///
/// One file rewritten thousands of times rather than a fresh
/// `NamedTempFile` per case. Creating and deleting a file on Windows costs
/// more than everything this target is actually measuring -- the exhaustive
/// sweep below went from eighty seconds to a few with this one change, and a
/// gate stage's running time is the difference between a check that runs and
/// a check that gets bypassed.
struct Sandbox {
    _dir: tempfile::TempDir,
    path: std::path::PathBuf,
}

impl Sandbox {
    fn new() -> Self {
        let dir = tempfile::TempDir::new().expect("create a temporary directory");
        let path = dir.path().join("sample");
        Self { _dir: dir, path }
    }
}

/// Write `bytes` to the sandbox file, load it, and check the round trip.
///
/// Through a real file rather than a buffer because `load` takes a `&Path`
/// and the encoding decision is made from the bytes on disk. A harness that
/// tested a private decoder directly would be testing something the
/// application does not call.
fn exercise_in(sandbox: &Sandbox, bytes: &[u8]) {
    let mut file = std::fs::File::create(&sandbox.path).expect("create the sample");
    file.write_all(bytes).expect("write the sample");
    file.flush().expect("flush the sample");
    drop(file);

    let Ok(loaded) = bp_files::load(&sandbox.path) else {
        // A refusal is a pass. Which refusal it is belongs to `bp-files`'
        // own tests.
        return;
    };

    // The promise the crate's documentation makes, checked rather than
    // assumed: whatever loaded must write back byte for byte.
    let written = bp_files::encode(&loaded.text, loaded.encoding, LineEndingPolicy::Preserve)
        .expect("text that loaded from an encoding can be written back to it");
    assert_eq!(
        written.as_slice(),
        bytes,
        "a file that loaded did not encode back to itself"
    );
}

#[test]
fn the_corpus_is_survivable() {
    let outcomes: Vec<Outcome> = corpus("files")
        .into_iter()
        .map(|sample| {
            let bytes = sample.bytes.clone();
            let sandbox = Sandbox::new();
            Probe::new().run(sample.name.clone(), move || exercise_in(&sandbox, &bytes))
        })
        .collect();

    assert_no_violations("files corpus", &outcomes);
}

/// Every byte-order mark followed by every short body.
///
/// Exhaustive rather than sampled, because the space is small and every
/// boundary is inside it: an empty body, an odd body, a body that is one
/// unit, a body that is half a surrogate pair. A generator would visit these
/// at random and miss one.
///
/// Run through [`bp_fuzz::inline`] rather than one probe thread each, and
/// sized against the clock. Every case is a real file written and read back,
/// which costs about ten milliseconds here, so the sweep's *shape* is a
/// budget decision: three bytes over the full alphabet everywhere, and the
/// fourth byte -- where a surrogate pair first becomes possible -- only
/// behind the two UTF-16 marks, where it is the only place it can mean
/// anything. Around two thousand cases and twenty seconds. The full
/// four-byte cross product is eight thousand cases and ninety seconds, which
/// is not a thing to put on a pre-commit hook.
#[test]
fn every_short_body_behind_every_byte_order_mark_is_survivable() {
    on_app_stack("files short bodies", BLOCK_BUDGET, || {
        let sandbox = Sandbox::new();
        let marks: [(&str, &[u8], bool); 5] = [
            ("none", &[], false),
            ("utf8", &[0xEF, 0xBB, 0xBF], false),
            ("utf16le", &[0xFF, 0xFE], true),
            ("utf16be", &[0xFE, 0xFF], true),
            // A UTF-16 mark's bytes in the wrong order for the mark that
            // follows: detection takes the first match, so this is the LE
            // branch handed a body that starts with a BE mark.
            ("utf16le then utf16be", &[0xFF, 0xFE, 0xFE, 0xFF], false),
        ];

        // Bytes chosen for the decoder's decision points: ASCII, both halves
        // of a high surrogate, both halves of a low one, and a NUL.
        let alphabet: [u8; 6] = [0x41, 0x00, 0xD8, 0xDC, 0xFF, 0x0A];

        let mut outcomes = Vec::new();
        for (mark_name, mark, utf16) in marks {
            let longest = if utf16 { 4 } else { 3 };
            for len in 0..=longest {
                // The four-byte pass drops to the four bytes that can form a
                // surrogate half or a plain unit; 0xFF and 0x0A add nothing
                // there that three bytes have not already asked.
                let letters = if len == 4 { 4 } else { alphabet.len() };
                let combinations = letters.pow(u32::try_from(len).expect("len fits"));
                for combination in 0..combinations {
                    let mut bytes = mark.to_vec();
                    let mut n = combination;
                    for _ in 0..len {
                        bytes.push(alphabet[n % letters]);
                        n /= letters;
                    }
                    outcomes.push(bp_fuzz::inline(
                        format!("{mark_name} + {bytes:02X?}"),
                        || exercise_in(&sandbox, &bytes),
                    ));
                }
            }
        }

        assert_no_violations("files short bodies", &outcomes);
    });
}

// --- property runs ------------------------------------------------------

const BLOCK_BUDGET: Duration = Duration::from_secs(300);

#[test]
fn arbitrary_bytes_are_survivable() {
    on_app_stack("files arbitrary bytes", BLOCK_BUDGET, || {
        let sandbox = Sandbox::new();
        proptest!(proptest_config(), |(bytes in strategies::arbitrary_bytes())| {
            exercise_in(&sandbox, &bytes);
        });
    });
}

/// Arbitrary bytes behind a byte-order mark.
///
/// The same coverage substitute as everywhere else: `Encoding::detect_bom`
/// looks at three bytes, and a generator producing random ones sends
/// essentially every case down the UTF-8 branch. Prefixing the mark is what
/// gets the hand-written UTF-16 decoder any cases at all.
#[test]
fn arbitrary_bytes_behind_a_byte_order_mark_are_survivable() {
    on_app_stack("files behind a bom", BLOCK_BUDGET, || {
        let sandbox = Sandbox::new();
        proptest!(proptest_config(), |(
            mark in proptest::sample::select(
                &[[0xFFu8, 0xFE], [0xFE, 0xFF]][..],
            ),
            mut body in strategies::arbitrary_bytes(),
        )| {
            let mut bytes = mark.to_vec();
            bytes.append(&mut body);
            exercise_in(&sandbox, &bytes);
        });
    });
}

/// UTF-16 assembled from code units rather than bytes.
///
/// Random bytes behind a mark are mostly valid text by accident -- two random
/// bytes are a BMP character about 96% of the time -- so surrogates,
/// the only interesting case, turn up rarely. This generator draws from a
/// unit alphabet where a third of the draws are surrogate halves.
#[test]
fn utf16_assembled_from_hostile_code_units_is_survivable() {
    on_app_stack("files utf16 units", BLOCK_BUDGET, || {
        let sandbox = Sandbox::new();
        proptest!(proptest_config(), |(
            big_endian in any::<bool>(),
            units in proptest::collection::vec(
                prop_oneof![
                    // High surrogates.
                    0xD800u16..0xDC00,
                    // Low surrogates.
                    0xDC00u16..0xE000,
                    // Everything else.
                    any::<u16>(),
                ],
                0..200,
            ),
            trailing_byte in proptest::option::of(any::<u8>()),
        )| {
            let mut bytes = if big_endian {
                vec![0xFE, 0xFF]
            } else {
                vec![0xFF, 0xFE]
            };
            for unit in units {
                let pair = if big_endian {
                    unit.to_be_bytes()
                } else {
                    unit.to_le_bytes()
                };
                bytes.extend_from_slice(&pair);
            }
            // Sometimes half a unit at the end: the `TruncatedUtf16` case.
            if let Some(byte) = trailing_byte {
                bytes.push(byte);
            }
            exercise_in(&sandbox, &bytes);
        });
    });
}

/// Text through `encode` and back through `load`, in every encoding.
///
/// The other direction of the crate's promise, and stated as the crate states
/// it -- in **bytes**. Anything `encode` writes must load, and encoding what
/// loaded must reproduce the same bytes. That is what "writes it back byte
/// for byte" means and it is what a user's file depends on.
///
/// It is deliberately *not* stated in characters. Text is not preserved
/// across this round trip in one case, and that case has a test of its own
/// below rather than a weaker assertion here.
#[test]
fn anything_encoded_loads_back_to_the_same_bytes() {
    on_app_stack("files encode round trip", BLOCK_BUDGET, || {
        let sandbox = Sandbox::new();
        proptest!(proptest_config(), |(
            text in strategies::arbitrary_text(),
            encoding in proptest::sample::select(
                &[
                    bp_core::Encoding::Utf8,
                    bp_core::Encoding::Utf8Bom,
                    bp_core::Encoding::Utf16Le,
                    bp_core::Encoding::Utf16Be,
                ][..],
            ),
        )| {
            let bytes = bp_files::encode(&text, encoding, LineEndingPolicy::Preserve)
                .expect("a Unicode encoding writes every character");
            std::fs::write(&sandbox.path, &bytes).expect("write the sample");

            let loaded = bp_files::load(&sandbox.path)
                .expect("a file this build encoded must load again");
            let again = bp_files::encode(
                &loaded.text,
                loaded.encoding,
                LineEndingPolicy::Preserve,
            )
            .expect("what loaded can be written back");
            prop_assert_eq!(
                again,
                bytes,
                "encoding what loaded did not reproduce the file"
            );
        });
    });
}

/// The one input where text does not survive the round trip, recorded
/// because it was found here.
///
/// A document whose first character is U+FEFF, saved as UTF-8 **without** a
/// byte-order mark, is written as the three bytes `EF BB BF` -- which are
/// also, exactly, a UTF-8 byte-order mark. `Encoding::detect_bom` cannot tell
/// them apart, because nothing can: the file format has no room for the
/// distinction. So the file loads as `Utf8Bom` with the character gone from
/// the text, and the next save writes it back as a mark.
///
/// The bytes on disk are never wrong -- `encode(load(bytes)) == bytes` holds
/// throughout, which is the promise `bp-files` actually makes and the one the
/// property above checks. What changes silently is the *text*: one character
/// disappears from the document and the encoding label changes underneath the
/// user.
///
/// Recorded rather than fixed. It is not this harness's crate to change,
/// other agents are working in this tree, and "fixing" it means choosing
/// between two lies -- refusing to save a legal document, or writing a
/// leading U+FEFF as an escape nothing else reads. The corpus entry is
/// `corpus/files/utf8-bom-then-zwnbsp.bin`.
#[test]
fn a_leading_zwnbsp_saved_without_a_bom_is_read_back_as_a_bom() {
    let text = "\u{feff}hello";
    let bytes = bp_files::encode(text, bp_core::Encoding::Utf8, LineEndingPolicy::Preserve)
        .expect("UTF-8 writes every character");
    assert_eq!(
        bytes, b"\xEF\xBB\xBFhello",
        "written as three bytes and text"
    );

    let mut file = tempfile::NamedTempFile::new().expect("temporary file");
    file.write_all(&bytes).expect("write");
    file.flush().expect("flush");
    let loaded = bp_files::load(file.path()).expect("load");

    assert_eq!(
        loaded.encoding,
        bp_core::Encoding::Utf8Bom,
        "the leading character was taken for a byte-order mark"
    );
    assert_eq!(loaded.text, "hello", "and so it is gone from the text");

    // The byte-level promise still holds, which is why this is a note rather
    // than a crash.
    let again = bp_files::encode(&loaded.text, loaded.encoding, LineEndingPolicy::Preserve)
        .expect("UTF-8 writes every character");
    assert_eq!(again, bytes, "the file on disk is unchanged either way");
}
