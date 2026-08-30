//! Target 2: `bp_crypto::open` on hostile bytes.
//!
//! The classic target. An envelope parser reads a header before it has a key,
//! which means it acts on whatever an attacker wrote before it can
//! authenticate anything at all -- `header.rs` says so itself: "everything
//! here is **untrusted input** until a chunk authenticates".
//!
//! The invariant: **it may return a `CryptoError`; it may not panic, abort,
//! or hang.** Every malformed input is a named error. `CannotOpen` for the
//! one failure that cannot be diagnosed further, and something more specific
//! for everything that can.
//!
//! Two things are deliberately *not* asserted here. Whether the right error
//! variant comes back is `bp-crypto`'s own business and it tests it; and
//! nothing here tries to open a document it should not be able to, because
//! that is a cryptographic claim and a survival harness is the wrong
//! instrument for it.

use std::time::Duration;

use bp_crypto::{CryptoError, KdfParams, SealOptions, Suite};
use bp_fuzz::{
    Outcome, Probe, assert_no_violations, corpus, on_app_stack, proptest_config, strategies,
};
use proptest::prelude::*;

/// The passphrase the corpus documents were sealed under. See
/// `src/bin/seed-corpus.rs`.
const PASSPHRASE: &str = "corpus";

/// The cheapest cost the format allows, for documents this test seals itself.
const CHEAP: KdfParams = KdfParams {
    memory_kib: 8,
    iterations: 1,
    lanes: 1,
};

fn cheap_options() -> SealOptions {
    SealOptions {
        suite: Suite::XChaCha20Poly1305,
        kdf: CHEAP,
        chunk_size: 1024,
    }
}

/// Try to open `bytes` under the passphrases a real reader would use: the
/// right one, a wrong one, and an empty one.
///
/// Three rather than one because they reach different code. The right
/// passphrase runs the chunk loop to the end; a wrong one fails at the first
/// tag; an empty one is refused before any work happens at all.
fn exercise(bytes: &[u8]) {
    let _ = bp_crypto::open(bytes, PASSPHRASE);
    let _ = bp_crypto::open(bytes, "not the passphrase");
    let _ = bp_crypto::open(bytes, "");
    // Cheap and total, and the thing the shell calls before it decides
    // whether a file is even a candidate.
    let _ = bp_crypto::is_bpadx(bytes);
    let _ = bp_crypto::stable_name(bytes);
}

#[test]
fn the_corpus_is_survivable() {
    let outcomes: Vec<Outcome> = corpus("envelope")
        .into_iter()
        .map(|sample| {
            let bytes = sample.bytes.clone();
            Probe::new().run(sample.name.clone(), move || exercise(&bytes))
        })
        .collect();

    assert_no_violations("envelope corpus", &outcomes);
}

/// Every prefix of a real document, which is every way a copy can be cut
/// short.
///
/// A truncation is not an exotic attack -- it is an interrupted sync, a full
/// disk, a half-finished download. Each one must be an error, and the ones
/// inside the header are the interesting half because the length checks there
/// are hand-written.
#[test]
fn every_truncation_of_a_real_document_is_an_error() {
    let sealed = bp_crypto::seal(b"a document\n", PASSPHRASE, cheap_options()).expect("seal");

    let outcomes: Vec<Outcome> = (0..sealed.len())
        .map(|cut| {
            let prefix = sealed[..cut].to_vec();
            Probe::new().run(format!("truncated to {cut} bytes"), move || {
                let opened = bp_crypto::open(&prefix, PASSPHRASE);
                assert!(
                    opened.is_err(),
                    "a document cut to {cut} bytes opened; truncation is not authentication"
                );
            })
        })
        .collect();

    assert_no_violations("envelope truncation", &outcomes);
}

/// Every single-byte corruption of a real document.
///
/// ADR-0021 authenticates the header as additional data on every chunk, so
/// the claim is total: **no** flipped byte anywhere in the file may produce
/// plaintext. This walks all of them rather than sampling, because the file
/// is 96 bytes and a claim that holds for a sample is not the claim.
///
/// The KDF cost field is walked too, and it is why this test carries its own
/// budget: a flip there is honoured before anything is authenticated, so a
/// single bit can turn an 8 KiB derive into a 512 MiB one. That is not a
/// defect this test asserts against -- the bound is documented and finite --
/// but it is the reason the budget is generous, and it is written up in
/// `fuzz/README.md`.
#[test]
fn no_single_byte_flip_ever_produces_plaintext() {
    let sealed = bp_crypto::seal(b"a document\n", PASSPHRASE, cheap_options()).expect("seal");

    let outcomes: Vec<Outcome> = (0..sealed.len())
        .flat_map(|at| [0x01u8, 0x80, 0xFF].into_iter().map(move |mask| (at, mask)))
        .map(|(at, mask)| {
            let mut damaged = sealed.clone();
            damaged[at] ^= mask;
            Probe::new().budget(Duration::from_secs(60)).run(
                format!("byte {at} xor {mask:#04x}"),
                move || {
                    let opened = bp_crypto::open(&damaged, PASSPHRASE);
                    assert!(
                        opened.is_err(),
                        "flipping byte {at} by {mask:#04x} produced plaintext"
                    );
                },
            )
        })
        .collect();

    assert_no_violations("envelope bit flips", &outcomes);
}

/// A document that is nothing but a plausible header.
///
/// Every combination of the header's own enumerations, with no body at all.
/// The reader has to survive being handed a header it half-understands and
/// then nothing.
#[test]
fn a_header_with_no_body_is_an_error() {
    let mut outcomes = Vec::new();

    for suite in [Suite::XChaCha20Poly1305, Suite::Aes256Gcm] {
        let sealed = bp_crypto::seal(
            b"x",
            PASSPHRASE,
            SealOptions {
                suite,
                ..cheap_options()
            },
        )
        .expect("seal");

        // The header runs to the first chunk-length prefix. Found by cutting
        // rather than by arithmetic on a private constant.
        for cut in 40..sealed.len().min(60) {
            let prefix = sealed[..cut].to_vec();
            outcomes.push(
                Probe::new().run(format!("{suite:?} header cut at {cut}"), move || {
                    let opened = bp_crypto::open(&prefix, PASSPHRASE);
                    assert!(opened.is_err(), "a headerless body opened");
                }),
            );
        }
    }

    assert_no_violations("envelope header only", &outcomes);
}

// --- property runs ------------------------------------------------------

const BLOCK_BUDGET: Duration = Duration::from_secs(300);

/// Arbitrary bytes.
///
/// Honest about its own weakness: the six-byte magic means virtually every
/// generated input is rejected in the first comparison, so this run is
/// checking that the *rejection* is total rather than exploring the parser.
/// The salad and mutation runs below are what actually get inside.
#[test]
fn arbitrary_bytes_are_survivable() {
    on_app_stack("envelope arbitrary bytes", BLOCK_BUDGET, || {
        proptest!(proptest_config(), |(bytes in strategies::arbitrary_bytes())| {
            exercise(&bytes);
        });
    });
}

/// Arbitrary bytes behind a valid magic, so the header parser is actually
/// reached.
///
/// This is the substitute for coverage feedback: rather than waiting for a
/// generator to produce `BPADX\0` by chance -- which it will not, in any run
/// anyone would sit through -- the magic is prefixed and the generator's
/// budget is spent on the fields behind it.
#[test]
fn arbitrary_bytes_behind_the_magic_are_survivable() {
    on_app_stack("envelope past the magic", BLOCK_BUDGET, || {
        proptest!(proptest_config(), |(mut bytes in strategies::arbitrary_bytes())| {
            let mut document = b"BPADX\0".to_vec();
            document.append(&mut bytes);
            // Version 1 is the only one this build reads, so pin it: a random
            // version is rejected two bytes in and everything past it is
            // never looked at.
            if document.len() >= 8 {
                document[6..8].copy_from_slice(&1u16.to_le_bytes());
            }
            let _ = bp_crypto::open(&document, PASSPHRASE);
        });
    });
}

/// A real document with a random slice of it replaced by random bytes.
///
/// The highest-yield generator in this file. It starts from something the
/// parser accepts and damages it, so every case is a document that gets some
/// distance in before it goes wrong -- which is where a parser's bugs live.
///
/// The KDF cost field is pinned back to cheap after each mutation. Not to
/// hide anything: the pre-authentication cost is written up in the README and
/// walked deliberately by
/// [`no_single_byte_flip_ever_produces_plaintext`]. It is pinned because a
/// generator free to set a 256 MiB cost spends the whole run in Argon2 instead
/// of in the parser this target is aimed at.
#[test]
fn a_damaged_real_document_is_survivable() {
    let sealed = bp_crypto::seal(&vec![b'x'; 3000], PASSPHRASE, cheap_options()).expect("seal");
    let len = sealed.len();

    on_app_stack("envelope mutation", BLOCK_BUDGET, move || {
        proptest!(proptest_config(), |(
            at in 0usize..len,
            patch in proptest::collection::vec(any::<u8>(), 1..64),
        )| {
            let mut damaged = sealed.clone();
            let end = (at + patch.len()).min(len);
            damaged[at..end].copy_from_slice(&patch[..end - at]);
            // Offsets 10..22 are memory_kib, iterations and lanes.
            damaged[10..14].copy_from_slice(&CHEAP.memory_kib.to_le_bytes());
            damaged[14..18].copy_from_slice(&CHEAP.iterations.to_le_bytes());
            damaged[18..22].copy_from_slice(&CHEAP.lanes.to_le_bytes());

            let opened = bp_crypto::open(&damaged, PASSPHRASE);
            // A patch can, very occasionally, write back the bytes that were
            // already there. Anything else must fail.
            if damaged != sealed {
                prop_assert!(opened.is_err(), "a damaged document opened");
            }
        });
    });
}

/// What a document's own declared Argon2id cost buys an attacker.
///
/// Not a defect, and not asserted as one -- it is the format working as
/// ADR-0021 designed it, and `KdfParams::validate` is the bound that keeps it
/// finite. It is measured here because the measurement is the only way to say
/// how large "finite" is, and because the ordering is not obvious from the
/// outside: a reader has to derive a key before it can check a tag, so the
/// cost written in an unauthenticated header is **paid before anything is
/// authenticated**. The header is authenticated -- as additional data on
/// every chunk -- but only afterwards, by which time the work is done.
///
/// The consequence: opening a hostile `.bpadx` and typing any passphrase at
/// all costs whatever the file asked for, up to the ceiling
/// `KdfParams::validate` sets, before it can report that the file is rubbish.
/// A single flipped bit in the cost field of a real document is enough to do
/// it.
///
/// **That ceiling is 256 MiB, 16 passes, 64 lanes** -- ADR-0035 lowered it
/// from 1 GiB and 64 passes. This test walked the old one for a session after
/// the change, which made three of its rows measure a refusal rather than the
/// work: the numbers were real and were measuring the wrong thing. The rows
/// below are pinned to `KdfParams::validate`'s actual bounds, and the last
/// one is one KiB past them.
///
/// `#[ignore]`d because it allocates hundreds of megabytes and takes seconds,
/// which is not a thing to put on a gate. Run it deliberately:
///
/// ```text
/// cargo test --release --test envelope -- --ignored --nocapture
/// ```
#[test]
#[ignore = "allocates hundreds of MiB and takes seconds; run deliberately"]
fn the_declared_kdf_cost_is_paid_before_anything_is_authenticated() {
    let sealed = bp_crypto::seal(b"a document\n", PASSPHRASE, cheap_options()).expect("seal");

    // Offset 10 is the stored memory cost: magic 6 + version 2 + suite 1 +
    // kdf id 1.
    const MEMORY_KIB_AT: usize = 10;

    for memory_kib in [8u32, 1024, 64 * 1024, 256 * 1024] {
        let mut document = sealed.clone();
        document[MEMORY_KIB_AT..MEMORY_KIB_AT + 4].copy_from_slice(&memory_kib.to_le_bytes());

        let started = std::time::Instant::now();
        // The wrong passphrase, deliberately: this is the cost of *failing*
        // to open, which is what an attacker's file actually inflicts.
        let result = bp_crypto::open(&document, "not the passphrase");
        let elapsed = started.elapsed();

        println!(
            "declared cost {:>7} KiB -> {:>8.1?} to refuse ({})",
            memory_kib,
            elapsed,
            result.err().map_or_else(
                || "opened, which cannot happen".to_string(),
                |e| e.to_string()
            ),
        );
    }

    // Iterations, the other multiplier, measured at a memory cost small
    // enough to keep this test tolerable. Argon2's time is linear in the
    // pass count, so the ceiling -- 256 MiB at 16 passes -- is the product of
    // the two worst rows printed here and is not measured directly, because
    // measuring it means sitting through it.
    for iterations in [1u32, 8, 16] {
        let mut document = sealed.clone();
        document[MEMORY_KIB_AT..MEMORY_KIB_AT + 4].copy_from_slice(&(64u32 * 1024).to_le_bytes());
        document[MEMORY_KIB_AT + 4..MEMORY_KIB_AT + 8].copy_from_slice(&iterations.to_le_bytes());

        let started = std::time::Instant::now();
        let _ = bp_crypto::open(&document, "not the passphrase");
        println!(
            "64 MiB at {iterations:>3} passes -> {:>8.1?} to refuse",
            started.elapsed()
        );
    }

    // Past the ceiling, refused without doing the work. This is the half that
    // is a control rather than an observation, so it is the half asserted --
    // and it is one KiB past rather than sixteen times past, because a bound
    // is only demonstrated at its edge. A test that refuses 4 GiB says
    // nothing about where the refusal actually starts.
    let mut absurd = sealed;
    absurd[MEMORY_KIB_AT..MEMORY_KIB_AT + 4].copy_from_slice(&(256u32 * 1024 + 1).to_le_bytes());
    let started = std::time::Instant::now();
    let refused =
        bp_crypto::open(&absurd, PASSPHRASE).expect_err("one KiB past the ceiling must be refused");
    assert!(
        matches!(refused, CryptoError::UnreasonableCost { .. }),
        "got {refused}"
    );
    assert!(
        started.elapsed() < Duration::from_millis(100),
        "a cost past the ceiling must be refused without paying it"
    );
    println!("declared cost 256 MiB + 1 KiB -> refused immediately: {refused}");
}

/// Arbitrary seal options round-tripped, so `seal` is not left untested while
/// `open` gets all the attention.
///
/// A writer that can be made to produce a document its own reader rejects is
/// the same defect as a reader that crashes, arriving a day later.
#[test]
fn anything_that_seals_can_be_opened_again() {
    on_app_stack("envelope round trip", BLOCK_BUDGET, || {
        proptest!(proptest_config(), |(
            plaintext in proptest::collection::vec(any::<u8>(), 0..8192),
            passphrase in r"(?s).{0,64}",
            chunk_size in 0u32..(20 * 1024 * 1024),
            aes in any::<bool>(),
        )| {
            let options = SealOptions {
                suite: if aes { Suite::Aes256Gcm } else { Suite::XChaCha20Poly1305 },
                kdf: CHEAP,
                chunk_size,
            };
            match bp_crypto::seal(&plaintext, &passphrase, options) {
                Ok(sealed) => {
                    let opened = bp_crypto::open(&sealed, &passphrase)
                        .expect("a document this build sealed must open again");
                    prop_assert_eq!(opened.as_slice(), plaintext.as_slice());
                }
                Err(e) => {
                    // Every refusal must be one of the two the inputs above
                    // can legitimately provoke. Anything else means `seal`
                    // failed for a reason it did not name.
                    prop_assert!(
                        matches!(
                            e,
                            CryptoError::EmptyPassphrase | CryptoError::UnreasonableChunkSize(_)
                        ),
                        "seal refused with {e}"
                    );
                }
            }
        });
    });
}

/// The one property nothing else in this repository checks: **a document
/// sealed by an earlier build still opens.**
///
/// Every other round-trip test in the workspace -- and the property test just
/// above -- seals and opens with the *same* build. That proves the two halves
/// agree with each other; it cannot prove either agrees with what is already
/// on somebody's disk. A change that altered the header layout, the additional
/// data, the nonce construction or the Argon2id invocation *coherently on both
/// sides* would pass the entire gate and silently orphan every document ever
/// written.
///
/// ADR-0021 opens by saying a document written today has to open in ten years.
/// This is the test that would fail if that stopped being true.
///
/// The vectors are the three well-formed envelopes already in the corpus.
/// They were written once by `seed-corpus`, are checked into git, and are the
/// reason `fuzz/README.md` says re-running the seeder is not idempotent here:
/// **regenerating them is what this test exists to make somebody think twice
/// about.** If it fails, the format changed -- decide whether that was meant
/// before reaching for the seeder.
#[test]
fn a_document_sealed_by_an_earlier_build_still_opens() {
    // Plaintexts from `src/bin/seed-corpus.rs`. Written out rather than
    // imported so that a change to the seeder cannot quietly move the goal
    // posts this test is here to hold still.
    // `corpus` names an entry by its target directory as well as its file, so
    // these are the names as reported, not as they appear on disk.
    let expected: [(&str, Vec<u8>); 3] = [
        ("envelope/sealed.bpadx", b"a small document\n".to_vec()),
        ("envelope/sealed-aes.bpadx", b"a small document\n".to_vec()),
        ("envelope/sealed-multichunk.bpadx", vec![b'x'; 4096]),
    ];

    for (name, plaintext) in expected {
        let sample = corpus("envelope")
            .into_iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("corpus/{name} is missing from a checked-out tree"));

        let opened = bp_crypto::open(&sample.bytes, PASSPHRASE).unwrap_or_else(|e| {
            panic!(
                "{name} was sealed by an earlier build and no longer opens: {e}. \
                 The format changed. That is either a deliberate version bump -- \
                 in which case this build must still read version 1 -- or a \
                 regression that has orphaned every existing document."
            )
        });

        assert_eq!(
            opened.as_slice(),
            plaintext.as_slice(),
            "{name} opened, but to different bytes than it was sealed with"
        );
    }
}

/// A wrong passphrase against a real document, asserted rather than merely
/// survived.
///
/// `exercise` above hands the corpus a wrong passphrase and discards the
/// result, because a survival harness asserts survival. The cryptographic
/// claim -- that it does not open -- belongs somewhere, and the vectors are
/// here.
#[test]
fn an_earlier_builds_document_does_not_open_under_the_wrong_passphrase() {
    let sample = corpus("envelope")
        .into_iter()
        .find(|s| s.name == "envelope/sealed.bpadx")
        .expect("corpus/envelope/sealed.bpadx");

    assert_eq!(
        bp_crypto::open(&sample.bytes, "not the passphrase"),
        Err(CryptoError::CannotOpen)
    );
}
