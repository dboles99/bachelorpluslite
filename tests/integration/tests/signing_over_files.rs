//! Seam: `bp-integrity` over `bp-crypto` over `bp-files`.
//!
//! `bp-crypto` signs byte arrays and has no opinion about where the 64
//! detached bytes live. `bp-integrity` decides that: a `.sig` sidecar beside
//! the document, a key file, a manifest over a set. `bp-files` is what put
//! the document on the disk in the first place. Each is tested alone; the
//! join -- *sign what is actually on the disk, and fail closed on every way
//! that can go wrong* -- is tested nowhere.
//!
//! The properties are the ones a recipient of a signed document depends on:
//!
//! * a file `bp-files` wrote, signed and then verified, verifies;
//! * one flipped bit anywhere in the document never verifies, and one
//!   anywhere in the sidecar verifies *only* if it changed nothing the
//!   sidecar actually asserts;
//! * deleting the sidecar is a *failure*, not a pass -- the whole design
//!   rests on this and it is the variant a hand-written `match` forgets;
//! * signing with key A and demanding key B is a **different verdict** from
//!   a document that was tampered with (ADR-0026), which is the entire
//!   reason the sidecar records the signer's key at all;
//! * a signing key survives a round trip through a key file, byte-exactly
//!   enough that signatures made either side of it interchange;
//! * a manifest over a directory says **which** file changed.
//!
//! No Argon2id is involved here: signing is Ed25519 and hashing is SHA-2, so
//! nothing in this file pays a key-derivation cost, and the bit-flip sweeps
//! can afford to cover every byte and every bit rather than sample them.
//!
//! Every document in this file is a fixed constant, so nothing generated
//! reaches a failure message.

use std::path::PathBuf;

use bp_files::{SaveOptions, atomic_write};
use bp_integrity::{
    Expectation, FileOutcome, HashAlgorithm, Manifest, Sidecar, SigningKey, Verification,
    read_signing_key, sidecar_path, sign_file, verify_file, write_signing_key,
};
use tempfile::{TempDir, tempdir};

/// A document on disk, written the way a save writes one.
struct Signed {
    _dir: TempDir,
    document: PathBuf,
    sidecar: PathBuf,
    key: SigningKey,
}

/// Write `contents` with the real save path and sign the file that resulted.
///
/// `atomic_write` rather than `fs::write`, for the same reason the encryption
/// seam uses it: a signature must cover the bytes a recipient will receive,
/// and the bytes a recipient receives are the ones the save path left behind.
fn write_and_sign(contents: &[u8]) -> Signed {
    let dir = tempdir().expect("temp dir");
    let document = dir.path().join("statement.txt");
    atomic_write(&document, contents, SaveOptions::default()).expect("atomic_write");

    let key = SigningKey::generate().expect("generate");
    let sidecar = sign_file(&document, &key).expect("sign_file");
    assert_eq!(
        sidecar,
        sidecar_path(&document).expect("sidecar_path"),
        "sign_file wrote its sidecar somewhere verify_file will not look"
    );

    Signed {
        _dir: dir,
        document,
        sidecar,
        key,
    }
}

// --- the good case, end to end ------------------------------------------

#[test]
fn a_file_written_by_the_save_path_signs_and_verifies() {
    let signed = write_and_sign(b"the agreed text\r\nwith a NUL: \0 and a high byte: \xff\n");

    let verdict = verify_file(
        &signed.document,
        &Expectation::Key(signed.key.verifying_key()),
    )
    .expect("verify");
    assert!(
        verdict.is_verified(),
        "a freshly signed file did not verify: {}",
        verdict.explain()
    );
}

#[test]
fn the_sidecar_is_a_separate_file_and_signing_leaves_the_document_alone() {
    let contents = b"unchanged bytes\n";
    let signed = write_and_sign(contents);

    assert!(signed.sidecar.exists(), "no sidecar was written");
    assert_ne!(signed.sidecar, signed.document);
    assert!(
        std::fs::read(&signed.document).expect("read") == contents,
        "signing altered the document it was signing"
    );
}

// --- failing closed ------------------------------------------------------

#[test]
fn deleting_the_sidecar_is_a_failure_and_not_a_pass() {
    // The variant the whole design rests on. A check that can be passed by
    // deleting a file is not a check, so this asserts both halves: the
    // verdict is the specific one, and `is_verified` is false.
    let signed = write_and_sign(b"the agreed text\n");
    std::fs::remove_file(&signed.sidecar).expect("remove sidecar");

    let verdict = verify_file(
        &signed.document,
        &Expectation::Key(signed.key.verifying_key()),
    )
    .expect("verify");
    assert!(!verdict.is_verified(), "an unsigned document verified");
    match verdict {
        Verification::SignatureMissing { expected_at } => {
            assert_eq!(expected_at, signed.sidecar);
        }
        other => panic!("expected SignatureMissing, got {other:?}"),
    }

    // And the permissive expectation must not rescue it either: "any signer"
    // relaxes *whose* key, never *whether* there is one.
    let verdict = verify_file(&signed.document, &Expectation::AnySigner).expect("verify");
    assert!(
        !verdict.is_verified(),
        "AnySigner turned a missing signature into a pass"
    );
}

#[test]
fn an_empty_sidecar_is_malformed_rather_than_an_accusation() {
    let signed = write_and_sign(b"the agreed text\n");
    atomic_write(&signed.sidecar, b"", SaveOptions::default()).expect("truncate sidecar");

    let verdict = verify_file(&signed.document, &Expectation::AnySigner).expect("verify");
    assert!(!verdict.is_verified());
    assert!(
        matches!(verdict, Verification::SignatureMalformed { .. }),
        "an empty sidecar is a copying accident, not tampering: {verdict:?}"
    );
}

#[test]
fn signing_with_one_key_and_demanding_another_is_its_own_verdict() {
    // ADR-0026's distinction, and the reason the sidecar carries a key at
    // all. A bare 64-byte `.sig` cannot tell these two apart, and telling a
    // user "this document has been altered" when the truth is "this is from a
    // colleague you were not expecting" sends them hunting an attacker who
    // does not exist.
    let signed = write_and_sign(b"the agreed text\n");
    let other = SigningKey::generate().expect("generate");

    let verdict =
        verify_file(&signed.document, &Expectation::Key(other.verifying_key())).expect("verify");
    assert!(!verdict.is_verified(), "the wrong key still verified");

    match verdict {
        Verification::SignedByAnotherKey { signer, expected } => {
            assert_eq!(signer, signed.key.verifying_key());
            assert_eq!(expected, other.verifying_key());
        }
        other => panic!(
            "an intact document signed by another key must not be reported as tampering; got \
             {other:?}"
        ),
    }

    // The same file, tampered with, is the *other* verdict. Asserting both in
    // one test is the point: it is their difference that is under test, and a
    // crate that collapsed them would still pass either assertion alone.
    let mut bytes = std::fs::read(&signed.document).expect("read");
    bytes[0] ^= 1;
    atomic_write(&signed.document, &bytes, SaveOptions::default()).expect("write");

    let verdict =
        verify_file(&signed.document, &Expectation::Key(other.verifying_key())).expect("verify");
    assert!(
        matches!(verdict, Verification::DoesNotMatch { .. }),
        "an altered document must be DoesNotMatch whatever key was asked for; got {verdict:?}"
    );
}

// --- key files -----------------------------------------------------------

#[test]
fn a_signing_key_survives_a_round_trip_through_a_key_file() {
    let dir = tempdir().expect("temp dir");
    let path = dir.path().join("signing.key");

    let key = SigningKey::generate().expect("generate");
    write_signing_key(&path, &key).expect("write_signing_key");
    let reloaded = read_signing_key(&path).expect("read_signing_key");

    assert_eq!(
        reloaded.verifying_key(),
        key.verifying_key(),
        "the key file did not come back as the same key"
    );

    // The interchange that matters: a signature made before the round trip
    // must verify against the key read after it, and the other way round.
    let document = dir.path().join("statement.txt");
    atomic_write(&document, b"the agreed text\n", SaveOptions::default()).expect("write");

    sign_file(&document, &key).expect("sign with the original");
    assert!(
        verify_file(&document, &Expectation::Key(reloaded.verifying_key()))
            .expect("verify")
            .is_verified()
    );

    sign_file(&document, &reloaded).expect("sign with the reloaded key");
    assert!(
        verify_file(&document, &Expectation::Key(key.verifying_key()))
            .expect("verify")
            .is_verified()
    );
}

#[test]
fn a_key_file_that_is_one_byte_short_is_refused_rather_than_padded() {
    // The failure this guards against only ever surfaces at the recipient: a
    // key read short signs perfectly well and matches nothing anybody has
    // ever verified against.
    let dir = tempdir().expect("temp dir");
    let path = dir.path().join("signing.key");
    let key = SigningKey::generate().expect("generate");
    write_signing_key(&path, &key).expect("write");

    let mut bytes = std::fs::read(&path).expect("read");
    bytes.pop();
    atomic_write(&path, &bytes, SaveOptions::default()).expect("truncate");

    assert!(
        read_signing_key(&path).is_err(),
        "a truncated key file was accepted"
    );
}

// --- manifests -----------------------------------------------------------

/// Three files in one directory, and the manifest that covers them.
fn bundle() -> (TempDir, Manifest) {
    let dir = tempdir().expect("temp dir");
    for (name, body) in [
        ("exhibit-a.txt", "first\n"),
        ("exhibit-b.txt", "second\n"),
        ("exhibit-c.txt", "third\n"),
    ] {
        atomic_write(
            &dir.path().join(name),
            body.as_bytes(),
            SaveOptions::default(),
        )
        .expect("write");
    }
    let manifest = Manifest::build(
        dir.path(),
        &["exhibit-a.txt", "exhibit-b.txt", "exhibit-c.txt"],
        HashAlgorithm::Sha256,
    )
    .expect("build");
    (dir, manifest)
}

#[test]
fn a_manifest_names_which_file_changed_not_merely_that_one_did() {
    let (dir, manifest) = bundle();

    // Round trip through a real file first: the report has to come from a
    // manifest that was written and read back, not one still in memory.
    let manifest_path = dir.path().join("MANIFEST");
    manifest.write(&manifest_path).expect("write manifest");
    let manifest = Manifest::read(&manifest_path).expect("read manifest");

    assert!(
        manifest.check(dir.path()).all_match(),
        "an untouched bundle failed"
    );

    atomic_write(
        &dir.path().join("exhibit-b.txt"),
        b"second, edited\n",
        SaveOptions::default(),
    )
    .expect("edit");

    let report = manifest.check(dir.path());
    assert!(!report.all_match());

    let failures: Vec<_> = report.failures().collect();
    assert_eq!(
        failures.len(),
        1,
        "one file changed and {} were reported; summary: {}",
        failures.len(),
        report.summarise()
    );
    assert_eq!(failures[0].path, "exhibit-b.txt");
    assert!(
        matches!(failures[0].outcome, FileOutcome::Differs { .. }),
        "an edited file must be Differs, not {:?}",
        failures[0].outcome
    );
    assert!(
        report.summarise().contains('1'),
        "the summary hides the count: {}",
        report.summarise()
    );
}

#[test]
fn a_deleted_file_is_missing_rather_than_altered() {
    // Three ways to fail rather than one, because only one of them is
    // evidence of tampering and escalating the other two is how a
    // permissions problem becomes a security incident.
    let (dir, manifest) = bundle();
    std::fs::remove_file(dir.path().join("exhibit-c.txt")).expect("remove");

    let report = manifest.check(dir.path());
    let failures: Vec<_> = report.failures().collect();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].path, "exhibit-c.txt");
    assert_eq!(failures[0].outcome, FileOutcome::Missing);
}

/// A document with a bit of everything in it, so a bit flip lands in text,
/// in a NUL and in a high byte over the course of the sweep.
const AWKWARD: &[u8] = b"clause 1\r\nclause 2\n\x00\xff\xfe end\n";

#[test]
fn one_flipped_bit_anywhere_in_the_document_never_verifies() {
    // Every byte, every bit. Deterministic rather than generated: the reach
    // here comes from covering every offset, which a fixed document already
    // does, and a `proptest` shrink over a file on disk buys nothing but
    // minutes.
    let signed = write_and_sign(AWKWARD);
    let expect = Expectation::Key(signed.key.verifying_key());

    for index in 0..AWKWARD.len() {
        for bit in 0..8u8 {
            let mut damaged = AWKWARD.to_vec();
            damaged[index] ^= 1 << bit;
            std::fs::write(&signed.document, &damaged).expect("damage");

            let verdict = verify_file(&signed.document, &expect).expect("verify");
            assert!(
                !verdict.is_verified(),
                "byte {index} of {} with bit {bit} flipped still verified",
                AWKWARD.len(),
            );
            assert!(
                matches!(verdict, Verification::DoesNotMatch { .. }),
                "an altered document is tampering, not a malformed sidecar: {verdict:?}"
            );
        }
    }
}

#[test]
fn a_flipped_bit_in_the_sidecar_can_only_pass_if_it_changed_nothing() {
    // The naive property -- "a flipped bit in the sidecar never verifies" --
    // is false, and it is worth saying why rather than weakening it away. A
    // sidecar is *text*, deliberately, and its parser deliberately tolerates
    // trailing whitespace so that a signature can survive being mailed and
    // pasted. Flipping bit 0 of the closing newline yields a vertical tab,
    // which is still trailing whitespace, so the file still holds the same
    // key and the same 64 signature bytes and still verifies. That is
    // correct: the document really is unaltered and the signature really
    // does hold.
    //
    // So the property that actually has teeth is the equivalence. A damaged
    // sidecar verifies **if and only if** it parses back to the very same
    // key and signature. Anything else -- a flipped hex digit accepted, a
    // key silently truncated, a parser that skipped a field -- breaks it.
    let signed = write_and_sign(b"the agreed text\n");
    let original = std::fs::read(&signed.sidecar).expect("read sidecar");
    let intact = Sidecar::parse(std::str::from_utf8(&original).expect("a sidecar is text"))
        .expect("parse the sidecar this crate just wrote");

    for index in 0..original.len() {
        for bit in 0..8u8 {
            let mut damaged = original.clone();
            damaged[index] ^= 1 << bit;
            std::fs::write(&signed.sidecar, &damaged).expect("damage");

            let same_evidence = std::str::from_utf8(&damaged)
                .ok()
                .and_then(|text| Sidecar::parse(text).ok())
                == Some(intact);

            for expect in [
                Expectation::Key(signed.key.verifying_key()),
                Expectation::AnySigner,
            ] {
                let verdict = verify_file(&signed.document, &expect).expect("verify");
                assert_eq!(
                    verdict.is_verified(),
                    same_evidence,
                    "sidecar byte {index} of {} with bit {bit} flipped verified as {} under                      {expect:?}, but the evidence it holds {} the evidence that was written",
                    original.len(),
                    verdict.is_verified(),
                    if same_evidence {
                        "matches"
                    } else {
                        "differs from"
                    },
                );
            }
        }
    }
}
