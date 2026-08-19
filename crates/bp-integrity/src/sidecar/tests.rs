//! Tests for the `.sig` sidecar.
//!
//! Every one of these works in a `tempfile` directory. Nothing in this crate
//! may write into the checkout.

use super::*;
use crate::hex;
use bp_crypto::SIGNATURE_LEN;
use proptest::prelude::*;
use std::fs;
use tempfile::{TempDir, tempdir};

const DOCUMENT: &[u8] = b"the quick brown fox jumps over the lazy dog";

fn key() -> SigningKey {
    SigningKey::generate().expect("the OS random source is available in tests")
}

/// Put a document in a temporary directory. Every test uses one of these and
/// none of them touches the working tree.
fn document_in(dir: &TempDir, name: &str, contents: &[u8]) -> PathBuf {
    let path = dir.path().join(name);
    fs::write(&path, contents).unwrap();
    path
}

// --- naming ---------------------------------------------------------------

#[test]
fn the_suffix_is_appended_to_the_whole_file_name() {
    assert_eq!(
        sidecar_path(Path::new("report.docx")).unwrap(),
        Path::new("report.docx.sig")
    );
}

#[test]
fn two_documents_differing_only_by_extension_get_different_sidecars() {
    // Why the extension is kept rather than replaced: replacing it would make
    // exporting a document overwrite the original's signature.
    assert_ne!(
        sidecar_path(Path::new("report.docx")).unwrap(),
        sidecar_path(Path::new("report.pdf")).unwrap()
    );
}

#[test]
fn the_sidecar_stays_in_the_documents_own_directory() {
    let expected = Path::new("case").join("bundle").join("note.txt.sig");
    assert_eq!(
        sidecar_path(&Path::new("case").join("bundle").join("note.txt")).unwrap(),
        expected
    );
}

#[test]
fn a_path_that_names_no_file_is_refused() {
    // What the type permits and the domain does not: there is no file name to
    // append to, so there is nowhere for a signature to go.
    for path in ["..", "/", ""] {
        assert!(
            matches!(
                sidecar_path(Path::new(path)),
                Err(IntegrityError::NotAFilePath(_))
            ),
            "accepted {path:?} as something to sign"
        );
    }
}

// --- the working case ------------------------------------------------------

#[test]
fn a_signed_file_verifies_under_the_key_that_signed_it() {
    let dir = tempdir().unwrap();
    let document = document_in(&dir, "note.txt", DOCUMENT);
    let signing = key();

    let written = sign_file(&document, &signing).unwrap();

    assert_eq!(written, document.with_file_name("note.txt.sig"));
    assert_eq!(
        verify_file(&document, &Expectation::Key(signing.verifying_key())).unwrap(),
        Verification::Verified {
            signer: signing.verifying_key()
        }
    );
}

#[test]
fn an_empty_document_can_be_signed_and_verified() {
    // An empty file is still a document somebody signed. An implementation
    // that special-cased it would be one where a signature over nothing
    // passes for anything.
    let dir = tempdir().unwrap();
    let document = document_in(&dir, "empty.txt", b"");
    let signing = key();

    sign_file(&document, &signing).unwrap();

    assert!(
        verify_file(&document, &Expectation::Key(signing.verifying_key()))
            .unwrap()
            .is_verified()
    );
}

#[test]
fn re_signing_an_unchanged_document_reproduces_the_identical_file() {
    // Deterministic signing plus a canonical encoding: a user who signs twice
    // gets back the file they already had, not a change to review.
    let dir = tempdir().unwrap();
    let document = document_in(&dir, "note.txt", DOCUMENT);
    let signing = key();

    let path = sign_file(&document, &signing).unwrap();
    let first = fs::read(&path).unwrap();
    sign_file(&document, &signing).unwrap();

    assert_eq!(fs::read(&path).unwrap(), first);
}

#[test]
fn signing_leaves_no_temporary_file_beside_the_document() {
    // The atomic write must clean up after itself; a stray temp file next to
    // a document is a file the user will ask about.
    let dir = tempdir().unwrap();
    let document = document_in(&dir, "note.txt", DOCUMENT);
    sign_file(&document, &key()).unwrap();

    let names: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names.len(), 2, "stray file: {names:?}");
}

#[test]
fn a_failed_signing_leaves_any_previous_signature_intact() {
    let dir = tempdir().unwrap();
    let document = document_in(&dir, "note.txt", DOCUMENT);
    let signing = key();
    let path = sign_file(&document, &signing).unwrap();
    let good = fs::read(&path).unwrap();

    // Signing a document that is not there cannot produce a signature, and
    // must not destroy the one that is.
    let missing = dir.path().join("gone.txt");
    assert!(sign_file(&missing, &signing).is_err());

    assert_eq!(fs::read(&path).unwrap(), good);
}

// --- the encoding, which is a promise --------------------------------------

#[test]
fn the_sidecar_carries_bp_cryptos_exact_sixty_four_bytes() {
    // The claim made in the module docs: the framing is added around
    // `bp-crypto`'s signature, not instead of it.
    let dir = tempdir().unwrap();
    let document = document_in(&dir, "note.txt", DOCUMENT);
    let signing = key();

    let path = sign_file(&document, &signing).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    let digits = text.lines().nth(3).unwrap().split_once(": ").unwrap().1;
    let bytes = hex::decode(digits).unwrap();

    assert_eq!(bytes.len(), SIGNATURE_LEN);
    assert_eq!(bytes, sign_document(&signing, DOCUMENT).to_bytes().to_vec());
}

#[test]
fn the_key_in_a_sidecar_is_spelled_the_way_bp_crypto_spells_it() {
    // Two hex encoders in the tree is one chance for them to disagree, and if
    // they ever do, a key written here stops reading back there.
    let signing = key();
    let sidecar = Sidecar::new(signing.verifying_key(), sign_document(&signing, DOCUMENT));

    assert_eq!(
        sidecar.to_text().lines().nth(2).unwrap(),
        format!("key: {}", signing.verifying_key().to_hex())
    );
}

#[test]
fn a_sidecar_is_exactly_four_lines_in_a_fixed_order() {
    let signing = key();
    let sidecar = Sidecar::new(signing.verifying_key(), sign_document(&signing, DOCUMENT));
    let text = sidecar.to_text();
    let lines: Vec<&str> = text.lines().collect();

    assert_eq!(lines.len(), 4);
    assert_eq!(lines[0], SIDECAR_MAGIC);
    assert_eq!(lines[1], "algorithm: ed25519");
    assert!(lines[2].starts_with("key: "));
    assert!(lines[3].starts_with("signature: "));
    assert!(text.ends_with('\n'), "no trailing newline: {text:?}");
}

#[test]
fn a_sidecar_round_trips_through_its_text() {
    // The file contract in ten years: what this build writes, the next one
    // reads back as the same value.
    let signing = key();
    let sidecar = Sidecar::new(signing.verifying_key(), sign_document(&signing, DOCUMENT));

    let parsed = Sidecar::parse(&sidecar.to_text()).unwrap();

    assert_eq!(parsed, sidecar);
    assert_eq!(parsed.verifying_key(), signing.verifying_key());
    assert_eq!(parsed.signature(), sidecar.signature());
}

#[test]
fn windows_line_endings_and_trailing_blank_lines_still_read() {
    // The realistic input: a sidecar that has been through a Windows editor
    // or a mail client. Refusing it would be pedantry at the user's expense.
    let signing = key();
    let sidecar = Sidecar::new(signing.verifying_key(), sign_document(&signing, DOCUMENT));
    let mangled = format!("{}\r\n\r\n", sidecar.to_text().replace('\n', "\r\n"));

    assert_eq!(Sidecar::parse(&mangled).unwrap(), sidecar);
}

#[test]
fn uppercase_digits_are_refused_so_that_one_sidecar_has_one_spelling() {
    let signing = key();
    let sidecar = Sidecar::new(signing.verifying_key(), sign_document(&signing, DOCUMENT));
    let shouted = sidecar.to_text().to_uppercase();

    assert!(Sidecar::parse(&shouted).is_err());
}

#[test]
fn reordering_the_fields_is_refused() {
    let signing = key();
    let text = Sidecar::new(signing.verifying_key(), sign_document(&signing, DOCUMENT)).to_text();
    let lines: Vec<&str> = text.lines().collect();
    let swapped = format!("{}\n{}\n{}\n{}\n", lines[0], lines[1], lines[3], lines[2]);

    assert_eq!(
        Sidecar::parse(&swapped),
        Err(MalformedSidecar::WrongField {
            expected: "key",
            found: "signature".to_owned()
        })
    );
}

#[test]
fn an_extra_field_is_refused_rather_than_ignored() {
    let signing = key();
    let text = Sidecar::new(signing.verifying_key(), sign_document(&signing, DOCUMENT)).to_text();

    assert_eq!(
        Sidecar::parse(&format!("{text}signed-by: someone\n")),
        Err(MalformedSidecar::TrailingContent {
            found: "signed-by: someone".to_owned()
        })
    );
}

#[test]
fn a_sidecar_from_a_future_version_is_refused_by_its_marker() {
    // A v2 file decoded by a v1 reader would fail to verify and look exactly
    // like a tampered document. Refusing it by name is the difference.
    let signing = key();
    let text = Sidecar::new(signing.verifying_key(), sign_document(&signing, DOCUMENT)).to_text();
    let future = text.replace(SIDECAR_MAGIC, "BachelorPad+ signature v2");

    assert!(matches!(
        Sidecar::parse(&future),
        Err(MalformedSidecar::WrongMagic { .. })
    ));
}

#[test]
fn a_different_algorithm_is_refused_rather_than_assumed() {
    let signing = key();
    let text = Sidecar::new(signing.verifying_key(), sign_document(&signing, DOCUMENT)).to_text();
    let other = text.replace("algorithm: ed25519", "algorithm: ed448");

    assert_eq!(
        Sidecar::parse(&other),
        Err(MalformedSidecar::UnknownAlgorithm {
            found: "ed448".to_owned()
        })
    );
}

#[test]
fn a_truncated_sidecar_says_it_is_truncated_rather_than_wrong() {
    // A `.sig` cut short is a copying accident, and telling the user their
    // file is incomplete sends them somewhere useful. "Does not verify" would
    // send them looking for an attacker who does not exist.
    let signing = key();
    let text = Sidecar::new(signing.verifying_key(), sign_document(&signing, DOCUMENT)).to_text();

    for keep in 0..4 {
        let cut: String = text
            .lines()
            .take(keep)
            .map(|line| format!("{line}\n"))
            .collect();
        assert!(
            matches!(
                Sidecar::parse(&cut),
                Err(MalformedSidecar::Truncated { .. })
            ),
            "{keep} lines was not reported as truncated"
        );
    }
}

#[test]
fn a_signature_of_the_wrong_length_is_refused_by_length() {
    // The boundary and one past it, on the field whose length is fixed.
    let signing = key();
    let text = Sidecar::new(signing.verifying_key(), sign_document(&signing, DOCUMENT)).to_text();
    let full = text.lines().nth(3).unwrap().to_owned();

    for (digits, expected) in [
        (SIGNATURE_LEN * 2 - 2, SIGNATURE_LEN - 1),
        (SIGNATURE_LEN * 2 + 2, SIGNATURE_LEN + 1),
    ] {
        let value = &full["signature: ".len()..];
        let resized = if digits < value.len() {
            value[..digits].to_owned()
        } else {
            format!("{value}00")
        };
        let altered = text.replace(&full, &format!("signature: {resized}"));

        assert_eq!(
            Sidecar::parse(&altered),
            Err(MalformedSidecar::BadSignature(
                SignError::WrongSignatureLength(expected)
            ))
        );
    }
}

#[test]
fn a_field_that_is_not_hexadecimal_is_named_as_such() {
    let signing = key();
    let text = Sidecar::new(signing.verifying_key(), sign_document(&signing, DOCUMENT)).to_text();
    let key_line = text.lines().nth(2).unwrap().to_owned();
    let altered = text.replace(&key_line, "key: not hexadecimal at all");

    assert_eq!(
        Sidecar::parse(&altered),
        Err(MalformedSidecar::NotHex { field: "key" })
    );
}

#[test]
fn a_message_about_a_malformed_sidecar_does_not_quote_a_whole_file() {
    // A file that failed to parse can be any file at all, including someone's
    // document. An error that quotes the whole of it is an error that carries
    // it into a bug report.
    let noise = "x".repeat(10_000);

    let Err(MalformedSidecar::WrongMagic { found }) = Sidecar::parse(&noise) else {
        panic!("a wall of text parsed as a sidecar");
    };
    assert!(found.chars().count() <= 41, "quoted {} chars", found.len());
}

// --- failing closed --------------------------------------------------------

#[test]
fn an_unsigned_document_fails_rather_than_being_skipped() {
    let dir = tempdir().unwrap();
    let document = document_in(&dir, "note.txt", DOCUMENT);

    let verdict = verify_file(&document, &Expectation::AnySigner).unwrap();

    assert_eq!(
        verdict,
        Verification::SignatureMissing {
            expected_at: document.with_file_name("note.txt.sig")
        }
    );
    assert!(!verdict.is_verified(), "an unsigned document verified");
}

#[test]
fn deleting_the_signature_turns_a_pass_into_a_failure() {
    // Failing closed, stated as the attack: if removing the sidecar produced
    // a pass, the signature would be decoration.
    let dir = tempdir().unwrap();
    let document = document_in(&dir, "note.txt", DOCUMENT);
    let signing = key();
    let expect = Expectation::Key(signing.verifying_key());

    let path = sign_file(&document, &signing).unwrap();
    assert!(verify_file(&document, &expect).unwrap().is_verified());

    fs::remove_file(&path).unwrap();

    assert!(!verify_file(&document, &expect).unwrap().is_verified());
}

#[test]
fn an_empty_sidecar_file_is_malformed_and_does_not_verify() {
    let dir = tempdir().unwrap();
    let document = document_in(&dir, "note.txt", DOCUMENT);
    let signing = key();
    let path = sign_file(&document, &signing).unwrap();
    fs::write(&path, b"").unwrap();

    let verdict = verify_file(&document, &Expectation::Key(signing.verifying_key())).unwrap();

    assert!(matches!(
        verdict,
        Verification::SignatureMalformed {
            reason: MalformedSidecar::Truncated { .. },
            ..
        }
    ));
    assert!(!verdict.is_verified());
}

// --- the five verdicts, one at a time --------------------------------------

#[test]
fn an_altered_document_does_not_match() {
    let dir = tempdir().unwrap();
    let document = document_in(&dir, "note.txt", DOCUMENT);
    let signing = key();
    sign_file(&document, &signing).unwrap();

    fs::write(&document, b"the quick brown fox jumps over the lazy dot").unwrap();

    assert_eq!(
        verify_file(&document, &Expectation::Key(signing.verifying_key())).unwrap(),
        Verification::DoesNotMatch {
            named_signer: signing.verifying_key()
        }
    );
}

#[test]
fn appending_to_a_signed_document_does_not_match() {
    // The alteration that costs nothing to try: leave the signed bytes alone
    // and add to the end.
    let dir = tempdir().unwrap();
    let document = document_in(&dir, "note.txt", DOCUMENT);
    let signing = key();
    sign_file(&document, &signing).unwrap();

    let mut extended = DOCUMENT.to_vec();
    extended.push(b'!');
    fs::write(&document, &extended).unwrap();

    assert!(
        !verify_file(&document, &Expectation::AnySigner)
            .unwrap()
            .is_verified()
    );
}

#[test]
fn a_document_intact_but_signed_by_another_key_is_its_own_verdict() {
    // The verdict a bare 64-byte `.sig` cannot produce, and the reason the
    // sidecar records a key: "someone else signed this" and "this was
    // tampered with" call for opposite reactions.
    let dir = tempdir().unwrap();
    let document = document_in(&dir, "note.txt", DOCUMENT);
    let colleague = key();
    let expected = key().verifying_key();
    sign_file(&document, &colleague).unwrap();

    assert_eq!(
        verify_file(&document, &Expectation::Key(expected)).unwrap(),
        Verification::SignedByAnotherKey {
            signer: colleague.verifying_key(),
            expected
        }
    );
}

#[test]
fn a_broken_sidecar_is_a_mismatch_and_not_an_accusation_against_the_key_it_names() {
    // The check order. A sidecar that does not hold together internally is
    // unauthenticated text, so the key it names is not evidence, and
    // reporting `SignedByAnotherKey` off the back of it would be repeating
    // whatever an attacker chose to write there.
    let dir = tempdir().unwrap();
    let document = document_in(&dir, "note.txt", DOCUMENT);
    let signing = key();
    let stranger = key().verifying_key();
    let path = sign_file(&document, &signing).unwrap();

    let text = fs::read_to_string(&path).unwrap();
    let key_line = text.lines().nth(2).unwrap().to_owned();
    fs::write(
        &path,
        text.replace(&key_line, &format!("key: {}", stranger.to_hex())),
    )
    .unwrap();

    assert_eq!(
        verify_file(&document, &Expectation::Key(signing.verifying_key())).unwrap(),
        Verification::DoesNotMatch {
            named_signer: stranger
        }
    );
}

#[test]
fn any_signer_reports_who_signed_without_being_told_first() {
    let dir = tempdir().unwrap();
    let document = document_in(&dir, "note.txt", DOCUMENT);
    let signing = key();
    sign_file(&document, &signing).unwrap();

    assert_eq!(
        verify_file(&document, &Expectation::AnySigner).unwrap(),
        Verification::Verified {
            signer: signing.verifying_key()
        }
    );
}

#[test]
fn a_sidecar_from_the_next_document_along_is_not_accepted_for_this_one() {
    // Two signed documents in one folder with their sidecars swapped: a
    // filing accident that must not read as a valid signature.
    let dir = tempdir().unwrap();
    let one = document_in(&dir, "one.txt", b"one");
    let two = document_in(&dir, "two.txt", b"two");
    let signing = key();
    let one_sig = sign_file(&one, &signing).unwrap();
    let two_sig = sign_file(&two, &signing).unwrap();

    let borrowed = fs::read(&two_sig).unwrap();
    fs::write(&one_sig, borrowed).unwrap();

    assert_eq!(
        verify_file(&one, &Expectation::Key(signing.verifying_key())).unwrap(),
        Verification::DoesNotMatch {
            named_signer: signing.verifying_key()
        }
    );
}

#[test]
fn a_sidecar_that_is_not_text_is_reported_as_unreadable_not_as_tampering() {
    let dir = tempdir().unwrap();
    let document = document_in(&dir, "note.txt", DOCUMENT);
    let signing = key();
    let path = sign_file(&document, &signing).unwrap();
    fs::write(&path, [0xff, 0xfe, 0x00, 0x01]).unwrap();

    assert_eq!(
        verify_file(&document, &Expectation::AnySigner).unwrap(),
        Verification::SignatureMalformed {
            path,
            reason: MalformedSidecar::NotText
        }
    );
}

#[test]
fn a_missing_document_is_an_error_rather_than_a_verdict() {
    // There is nothing to have an opinion about. Reporting "not signed" for a
    // file that is not there would send the user to the wrong problem.
    let dir = tempdir().unwrap();

    assert!(matches!(
        verify_file(
            &dir.path().join("never-existed.txt"),
            &Expectation::AnySigner
        ),
        Err(IntegrityError::Read { .. })
    ));
}

// --- what the user is told -------------------------------------------------

#[test]
fn every_verdict_explains_itself_in_terms_a_user_can_act_on() {
    let signer = key().verifying_key();
    let expected = key().verifying_key();

    let verified = Verification::Verified { signer };
    assert!(verified.explain().contains(&signer.to_hex()));

    let missing = Verification::SignatureMissing {
        expected_at: PathBuf::from("case/note.txt.sig"),
    };
    assert!(
        missing.explain().contains("note.txt.sig"),
        "the user is not told which file to look for: {}",
        missing.explain()
    );

    let malformed = Verification::SignatureMalformed {
        path: PathBuf::from("case/note.txt.sig"),
        reason: MalformedSidecar::NotText,
    };
    assert!(malformed.explain().contains("not a text file"));

    let mismatch = Verification::DoesNotMatch {
        named_signer: signer,
    };
    assert!(mismatch.explain().contains("altered"));

    let other = Verification::SignedByAnotherKey { signer, expected };
    let text = other.explain();
    assert!(text.contains(&signer.to_hex()) && text.contains(&expected.to_hex()));
    assert!(
        !text.contains("altered"),
        "an intact document was described as altered: {text}"
    );
}

#[test]
fn only_the_verified_verdict_counts_as_verified() {
    // The whole point of `is_verified` existing: a caller who writes the
    // match themselves and forgets a variant fails open.
    let signer = key().verifying_key();

    assert!(Verification::Verified { signer }.is_verified());
    for verdict in [
        Verification::SignatureMissing {
            expected_at: PathBuf::from("x.sig"),
        },
        Verification::SignatureMalformed {
            path: PathBuf::from("x.sig"),
            reason: MalformedSidecar::NotText,
        },
        Verification::DoesNotMatch {
            named_signer: signer,
        },
        Verification::SignedByAnotherKey {
            signer,
            expected: signer,
        },
    ] {
        assert!(!verdict.is_verified(), "{verdict:?} passed as verified");
    }
}

// --- properties ------------------------------------------------------------

proptest! {
    // File-backed properties, so fewer cases than the default: each one
    // creates a directory, writes two files and does an Ed25519 operation.
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// The guarantee, over every document rather than the one above: what
    /// this crate signs on disk, it verifies from disk.
    #[test]
    fn any_document_signed_on_disk_verifies_from_disk(
        contents in prop::collection::vec(any::<u8>(), 0..2048),
    ) {
        let dir = tempdir().unwrap();
        let document = document_in(&dir, "note.bin", &contents);
        let signing = key();

        sign_file(&document, &signing).unwrap();

        prop_assert_eq!(
            verify_file(&document, &Expectation::Key(signing.verifying_key())).unwrap(),
            Verification::Verified { signer: signing.verifying_key() }
        );
    }

    /// The half that can actually catch something: there is no byte of a
    /// signed document that can be changed and still pass. A byte that could
    /// be is a byte outside what was signed.
    #[test]
    fn changing_any_single_byte_of_a_signed_document_makes_it_fail(
        contents in prop::collection::vec(any::<u8>(), 1..1024),
        position in any::<prop::sample::Index>(),
        flip in 1u8..=255,
    ) {
        let dir = tempdir().unwrap();
        let document = document_in(&dir, "note.bin", &contents);
        let signing = key();
        sign_file(&document, &signing).unwrap();

        let mut altered = contents.clone();
        let index = position.index(altered.len());
        altered[index] ^= flip;
        fs::write(&document, &altered).unwrap();

        prop_assert_eq!(
            verify_file(&document, &Expectation::Key(signing.verifying_key())).unwrap(),
            Verification::DoesNotMatch { named_signer: signing.verifying_key() }
        );
    }

    /// And no digit of the sidecar either. Changing one hex digit of the key
    /// or the signature must never leave a sidecar that still passes --
    /// whether it comes back as malformed or as a mismatch is secondary, but
    /// "verified" is never allowed.
    #[test]
    fn changing_any_single_digit_of_a_sidecar_never_leaves_it_passing(
        contents in prop::collection::vec(any::<u8>(), 0..256),
        line in 2usize..=3,
        position in any::<prop::sample::Index>(),
        replacement in 0u8..16,
    ) {
        let dir = tempdir().unwrap();
        let document = document_in(&dir, "note.bin", &contents);
        let signing = key();
        let path = sign_file(&document, &signing).unwrap();

        let text = fs::read_to_string(&path).unwrap();
        let lines: Vec<String> = text.lines().map(str::to_owned).collect();
        let (label, value) = lines[line].split_once(": ").unwrap();

        let index = position.index(value.len());
        let digit = b"0123456789abcdef"[replacement as usize] as char;
        prop_assume!(value.as_bytes()[index] as char != digit);

        let mut digits: Vec<char> = value.chars().collect();
        digits[index] = digit;
        let rebuilt: String = digits.into_iter().collect();

        let mut replaced = lines.clone();
        replaced[line] = format!("{label}: {rebuilt}");
        fs::write(&path, format!("{}\n", replaced.join("\n"))).unwrap();

        let verdict = verify_file(&document, &Expectation::AnySigner).unwrap();
        prop_assert!(!verdict.is_verified(), "{:?}", verdict);
    }

    /// A sidecar for one document never passes for a different one, however
    /// the two differ.
    #[test]
    fn a_sidecar_never_passes_for_a_document_it_was_not_made_for(
        signed in prop::collection::vec(any::<u8>(), 0..512),
        other in prop::collection::vec(any::<u8>(), 0..512),
    ) {
        prop_assume!(signed != other);

        let dir = tempdir().unwrap();
        let one = document_in(&dir, "one.bin", &signed);
        let two = document_in(&dir, "two.bin", &other);
        let signing = key();
        let one_sig = sign_file(&one, &signing).unwrap();
        let two_sig = sidecar_path(&two).unwrap();
        fs::copy(&one_sig, &two_sig).unwrap();

        prop_assert!(
            !verify_file(&two, &Expectation::AnySigner).unwrap().is_verified()
        );
    }

    /// The encoding round-trips for any document, so a sidecar written by
    /// this build is read back as the same value by the next one.
    #[test]
    fn a_sidecar_round_trips_through_its_text_for_any_document(
        contents in prop::collection::vec(any::<u8>(), 0..512),
    ) {
        let signing = key();
        let sidecar = Sidecar::new(
            signing.verifying_key(),
            sign_document(&signing, &contents),
        );

        prop_assert_eq!(Sidecar::parse(&sidecar.to_text()), Ok(sidecar));
    }
}
