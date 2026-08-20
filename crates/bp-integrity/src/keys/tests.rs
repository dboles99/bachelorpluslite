//! Tests for key files.
//!
//! All of them in a `tempfile` directory: a test that writes a private key
//! into the checkout is a test that commits one.

use super::*;
use bp_crypto::{SIGNING_KEY_LEN, SignError, VERIFYING_KEY_LEN, sign_document};
use proptest::prelude::*;
use std::fs;
use tempfile::tempdir;

const DOCUMENT: &[u8] = b"the quick brown fox jumps over the lazy dog";

fn key() -> SigningKey {
    SigningKey::generate().expect("the OS random source is available in tests")
}

// --- the signing key -------------------------------------------------------

#[test]
fn a_signing_key_round_trips_through_a_file_and_signs_identically() {
    // The guarantee that matters in ten years: a key stored today loads
    // tomorrow and produces the same signatures, not merely a valid key.
    let dir = tempdir().unwrap();
    let path = dir.path().join("signing.key");
    let original = key();

    write_signing_key(&path, &original).unwrap();
    let reloaded = read_signing_key(&path).unwrap();

    assert_eq!(reloaded.verifying_key(), original.verifying_key());
    assert_eq!(
        sign_document(&reloaded, DOCUMENT),
        sign_document(&original, DOCUMENT)
    );
}

#[test]
fn a_key_file_is_exactly_the_thirty_two_seed_bytes() {
    // The stated stable encoding. A header, a newline or a hex spelling here
    // would make every key this build writes unreadable to the next one and
    // to every other Ed25519 tool.
    let dir = tempdir().unwrap();
    let path = dir.path().join("signing.key");
    let signing = key();

    write_signing_key(&path, &signing).unwrap();

    let raw = fs::read(&path).unwrap();
    assert_eq!(raw.len(), SIGNING_KEY_LEN);
    assert_eq!(raw, signing.to_bytes().to_vec());
}

#[test]
fn a_key_file_of_the_wrong_length_is_refused_rather_than_padded() {
    // The boundary and one past it. A key read one byte short signs perfectly
    // well and matches nothing anyone has ever verified against, and the
    // failure only surfaces at the recipient.
    let dir = tempdir().unwrap();

    for length in [0, SIGNING_KEY_LEN - 1, SIGNING_KEY_LEN + 1] {
        let path = dir.path().join(format!("{length}.key"));
        fs::write(&path, vec![0u8; length]).unwrap();

        match read_signing_key(&path) {
            Err(IntegrityError::NotAKeyFile { source, .. }) => {
                assert_eq!(source, SignError::WrongSigningKeyLength(length));
            }
            other => panic!("a {length}-byte file was accepted as a key: {other:?}"),
        }
    }
}

#[test]
fn a_key_file_with_a_trailing_newline_is_refused() {
    // The realistic accident: a key written out by a shell redirect, or
    // opened and saved by an editor that adds a final newline.
    let dir = tempdir().unwrap();
    let path = dir.path().join("signing.key");
    let mut bytes = key().to_bytes().to_vec();
    bytes.push(b'\n');
    fs::write(&path, &bytes).unwrap();

    assert!(matches!(
        read_signing_key(&path),
        Err(IntegrityError::NotAKeyFile { .. })
    ));
}

#[test]
fn a_key_file_that_is_not_there_is_a_read_error_and_names_the_path() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("nowhere.key");

    let Err(error) = read_signing_key(&path) else {
        panic!("read a key that does not exist");
    };
    assert!(matches!(error, IntegrityError::Read { .. }));
    assert!(
        error.to_string().contains("nowhere.key"),
        "the message does not say which file: {error}"
    );
}

#[test]
fn writing_a_key_leaves_no_temporary_file_behind() {
    // A temporary file left beside a key file is a second copy of the secret.
    let dir = tempdir().unwrap();
    write_signing_key(&dir.path().join("signing.key"), &key()).unwrap();

    let names: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names.len(), 1, "stray file: {names:?}");
}

#[test]
fn a_failed_key_write_leaves_the_previous_key_intact() {
    // Rotation must not be able to destroy the only copy of an identity.
    let dir = tempdir().unwrap();
    let path = dir.path().join("nonexistent-folder").join("signing.key");

    assert!(matches!(
        write_signing_key(&path, &key()),
        Err(IntegrityError::Write { .. })
    ));
    assert!(!path.exists());
}

#[test]
fn rewriting_a_key_replaces_it_completely() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("signing.key");
    let first = key();
    let second = key();

    write_signing_key(&path, &first).unwrap();
    write_signing_key(&path, &second).unwrap();

    assert_eq!(
        read_signing_key(&path).unwrap().verifying_key(),
        second.verifying_key()
    );
    assert_eq!(fs::read(&path).unwrap().len(), SIGNING_KEY_LEN);
}

// --- the verifying key -----------------------------------------------------

#[test]
fn a_verifying_key_file_is_hex_text_that_round_trips() {
    // The opposite encoding to the private half, for the opposite reason: a
    // public key exists to be pasted into an email.
    let dir = tempdir().unwrap();
    let path = dir.path().join("signing.pub");
    let verifying = key().verifying_key();

    write_verifying_key(&path, &verifying).unwrap();

    let text = fs::read_to_string(&path).unwrap();
    assert_eq!(text, format!("{}\n", verifying.to_hex()));
    assert_eq!(text.trim().len(), VERIFYING_KEY_LEN * 2);
    assert_eq!(read_verifying_key(&path).unwrap(), verifying);
}

#[test]
fn a_verifying_key_pasted_across_two_lines_still_reads() {
    // Refusing wrapped text teaches users to retype keys by hand, which is
    // how a key acquires a wrong digit.
    let dir = tempdir().unwrap();
    let path = dir.path().join("pasted.pub");
    let verifying = key().verifying_key();
    let text = verifying.to_hex();
    fs::write(&path, format!("  {}\n  {}\n", &text[..32], &text[32..])).unwrap();

    assert_eq!(read_verifying_key(&path).unwrap(), verifying);
}

#[test]
fn a_public_key_file_that_is_not_a_key_is_refused_by_name() {
    let dir = tempdir().unwrap();

    let garbage = dir.path().join("garbage.pub");
    fs::write(&garbage, b"this is a note, not a key").unwrap();
    assert!(matches!(
        read_verifying_key(&garbage),
        Err(IntegrityError::NotAKeyFile { .. })
    ));

    // Well-formed digits, half a key: a specific message beats "not hex".
    let half = dir.path().join("half.pub");
    fs::write(&half, &key().verifying_key().to_hex()[..32]).unwrap();
    match read_verifying_key(&half) {
        Err(IntegrityError::NotAKeyFile { source, .. }) => {
            assert_eq!(source, SignError::WrongVerifyingKeyLength(16));
        }
        other => panic!("half a key was accepted: {other:?}"),
    }
}

#[test]
fn a_verifying_key_file_is_not_a_signing_key_file() {
    // The two encodings are different on purpose, and the length check is
    // what stops a user handing out their private key by picking the wrong
    // file -- or loading their public one and signing with it.
    let dir = tempdir().unwrap();
    let public = dir.path().join("signing.pub");
    write_verifying_key(&public, &key().verifying_key()).unwrap();

    assert!(matches!(
        read_signing_key(&public),
        Err(IntegrityError::NotAKeyFile { .. })
    ));
}

// --- what the platform actually enforces -----------------------------------

#[test]
fn writing_a_key_reports_the_protection_the_platform_actually_gave() {
    // Reported rather than assumed. On Windows the honest answer is
    // "inherited", and a caller that is told `OwnerOnly` there would show a
    // padlock this crate did not earn.
    let dir = tempdir().unwrap();
    let path = dir.path().join("signing.key");

    let protection = write_signing_key(&path, &key()).unwrap();

    assert_eq!(protection, key_file_protection(&path).unwrap());
    if cfg!(unix) {
        assert_eq!(protection, KeyFileProtection::OwnerOnly);
        assert!(protection.is_confirmed_private());
    } else {
        assert_eq!(protection, KeyFileProtection::InheritedFromDirectory);
        assert!(
            !protection.is_confirmed_private(),
            "unknown protection was reported as confirmed"
        );
    }
}

#[test]
fn every_protection_tells_the_user_something_they_can_do() {
    for protection in [
        KeyFileProtection::OwnerOnly,
        KeyFileProtection::ReadableByOthers { mode: 0o644 },
        KeyFileProtection::InheritedFromDirectory,
    ] {
        let text = protection.describe();
        assert!(!text.is_empty(), "{protection:?} says nothing");
    }
    assert!(
        KeyFileProtection::ReadableByOthers { mode: 0o644 }
            .describe()
            .contains("0644"),
        "the user is not told what the mode actually is"
    );
    assert!(
        KeyFileProtection::InheritedFromDirectory
            .describe()
            .contains("cannot"),
        "Windows is described as though something was enforced"
    );
}

#[test]
fn asking_about_a_key_file_that_is_not_there_is_an_error() {
    let dir = tempdir().unwrap();
    assert!(matches!(
        key_file_protection(&dir.path().join("nowhere.key")),
        Err(IntegrityError::Read { .. })
    ));
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::os::unix::fs::PermissionsExt as _;

    #[test]
    fn a_key_file_is_owner_only_from_the_moment_it_has_bytes_in_it() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("signing.key");

        write_signing_key(&path, &key()).unwrap();

        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "key file is mode {mode:04o}");
    }

    #[test]
    fn a_key_file_that_arrived_world_readable_is_reported_as_such() {
        // The realistic case: a key restored from a tar archive or checked
        // out of a repository, both of which flatten the mode.
        let dir = tempdir().unwrap();
        let path = dir.path().join("loose.key");
        fs::write(&path, key().to_bytes().as_ref()).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();

        assert_eq!(
            key_file_protection(&path).unwrap(),
            KeyFileProtection::ReadableByOthers { mode: 0o644 }
        );
    }

    #[test]
    fn rewriting_a_loose_key_file_narrows_it_instead_of_inheriting_it() {
        // The one behaviour this crate deliberately does not take from
        // `bp-files`: a key that arrived world-readable must not stay
        // world-readable simply because it was rotated in place.
        let dir = tempdir().unwrap();
        let path = dir.path().join("loose.key");
        fs::write(&path, key().to_bytes().as_ref()).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o666)).unwrap();

        write_signing_key(&path, &key()).unwrap();

        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn a_group_readable_key_is_not_reported_as_private() {
        // Only the group and other bits matter. A key nobody but the owner
        // can read is the only one this crate calls private.
        let dir = tempdir().unwrap();
        let path = dir.path().join("group.key");
        fs::write(&path, key().to_bytes().as_ref()).unwrap();

        for mode in [0o640, 0o604, 0o660, 0o700] {
            fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
            let protection = key_file_protection(&path).unwrap();
            assert_eq!(
                protection.is_confirmed_private(),
                mode & 0o077 == 0,
                "mode {mode:04o} was judged {protection:?}"
            );
        }
    }

    #[test]
    fn a_public_key_file_is_widened_rather_than_left_owner_only() {
        // A public key only its owner can read cannot do its job -- and the
        // temporary file underneath is `0600`, so leaving it alone would
        // produce exactly that.
        let dir = tempdir().unwrap();
        let path = dir.path().join("signing.pub");

        write_verifying_key(&path, &key().verifying_key()).unwrap();

        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o644, "a public key was written as {mode:04o}");
    }
}

// --- properties ------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// Every 32-byte string is a valid seed, and every one of them has to
    /// survive the round trip through a file byte for byte -- not merely come
    /// back as *a* key.
    #[test]
    fn any_seed_round_trips_through_a_key_file(seed in prop::array::uniform32(any::<u8>())) {
        let dir = tempdir().unwrap();
        let path = dir.path().join("signing.key");
        let original = SigningKey::from_bytes(&seed).unwrap();

        write_signing_key(&path, &original).unwrap();
        let reloaded = read_signing_key(&path).unwrap();

        prop_assert_eq!(reloaded.to_bytes().to_vec(), seed.to_vec());
        prop_assert_eq!(reloaded.verifying_key(), original.verifying_key());
    }

    /// Any file that is not exactly the seed length is refused. A silent pad
    /// or truncate here would produce a working key that verifies nowhere.
    #[test]
    fn a_file_of_any_other_length_is_never_read_as_a_key(
        bytes in prop::collection::vec(any::<u8>(), 0..96),
    ) {
        prop_assume!(bytes.len() != SIGNING_KEY_LEN);

        let dir = tempdir().unwrap();
        let path = dir.path().join("maybe.key");
        fs::write(&path, &bytes).unwrap();

        prop_assert!(read_signing_key(&path).is_err());
    }

    /// The public half survives the text round trip for any key.
    #[test]
    fn any_verifying_key_round_trips_through_its_file(
        seed in prop::array::uniform32(any::<u8>()),
    ) {
        let dir = tempdir().unwrap();
        let path = dir.path().join("signing.pub");
        let verifying = SigningKey::from_bytes(&seed).unwrap().verifying_key();

        write_verifying_key(&path, &verifying).unwrap();

        prop_assert_eq!(read_verifying_key(&path).unwrap(), verifying);
    }
}

// --- the sealed key file (ADR-0031) ----------------------------------------
//
// A cheap KDF is not available here: `SealOptions` carries the OWASP baseline
// and `bp-crypto` keeps `derive` private, so each of these pays one real
// Argon2id derivation per seal and per open. That is ~45 ms, which is why
// there are a handful of these rather than a property test over generated
// passphrases.

const PASSPHRASE: &str = "correct horse battery staple";

#[test]
fn a_sealed_key_round_trips_and_signs_identically() {
    // The same guarantee the plain key file has to make, through an envelope:
    // a key stored today loads tomorrow and produces the same signatures, not
    // merely a valid key.
    let dir = tempdir().unwrap();
    let path = dir.path().join("signing.bpadx");
    let original = key();

    write_sealed_signing_key(&path, &original, PASSPHRASE).unwrap();
    let reloaded = read_sealed_signing_key(&path, PASSPHRASE).unwrap();

    assert_eq!(
        sign_document(&original, DOCUMENT).to_bytes(),
        sign_document(&reloaded, DOCUMENT).to_bytes(),
        "the reloaded key is a different key"
    );
}

#[test]
fn the_seed_is_not_in_the_file() {
    // The whole point, and the one assertion that would catch the envelope
    // being bypassed by a well-meaning "simplification". Checked against the
    // actual bytes rather than against the format, because a format that
    // happened to store the seed in a header would satisfy any structural
    // test.
    let dir = tempdir().unwrap();
    let path = dir.path().join("signing.bpadx");
    let original = key();

    write_sealed_signing_key(&path, &original, PASSPHRASE).unwrap();
    let on_disk = fs::read(&path).unwrap();
    let seed = original.to_bytes();

    assert!(
        !on_disk.windows(SIGNING_KEY_LEN).any(|w| w == seed.as_ref()),
        "the raw seed is sitting in the sealed file"
    );
    assert!(
        on_disk.len() > SIGNING_KEY_LEN,
        "a sealed file is the seed plus a header and a tag"
    );
}

#[test]
fn the_wrong_passphrase_is_refused_and_does_not_say_it_was_the_passphrase() {
    // `bp-crypto`'s decision showing through, and it is the right one: an
    // authenticated envelope cannot distinguish a wrong key from altered
    // bytes, because the tag check fails identically for both. A message that
    // guessed would be wrong half the time, and the half it got wrong is the
    // half where somebody retypes a correct passphrase for ten minutes.
    let dir = tempdir().unwrap();
    let path = dir.path().join("signing.bpadx");
    write_sealed_signing_key(&path, &key(), PASSPHRASE).unwrap();

    let error = read_sealed_signing_key(&path, "wrong horse").expect_err("must refuse");
    assert!(matches!(error, IntegrityError::SealedKey { .. }));

    let message = error.to_string();
    assert!(
        message.contains("signing.bpadx"),
        "the message must name the file: {message}"
    );
}

#[test]
fn a_damaged_sealed_key_is_refused() {
    // Same refusal as the wrong passphrase, deliberately. What must not
    // happen is a key coming back at all.
    let dir = tempdir().unwrap();
    let path = dir.path().join("signing.bpadx");
    write_sealed_signing_key(&path, &key(), PASSPHRASE).unwrap();

    let mut bytes = fs::read(&path).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0x01;
    fs::write(&path, &bytes).unwrap();

    assert!(matches!(
        read_sealed_signing_key(&path, PASSPHRASE),
        Err(IntegrityError::SealedKey { .. })
    ));
}

#[test]
fn an_encrypted_document_is_not_accepted_as_a_key() {
    // `bp-crypto` seals anything, so the envelope opening proves nothing
    // about what came out. A key read from the wrong length signs perfectly
    // well and matches nothing anyone has verified against -- and the failure
    // surfaces only at the recipient, which is the worst place for it.
    let dir = tempdir().unwrap();
    let path = dir.path().join("notes.bpadx");
    let sealed = bp_crypto::seal(
        b"these are notes, not a key",
        PASSPHRASE,
        bp_crypto::SealOptions::default(),
    )
    .unwrap();
    fs::write(&path, &sealed).unwrap();

    let error = read_sealed_signing_key(&path, PASSPHRASE).expect_err("must refuse");
    assert!(
        matches!(error, IntegrityError::NotAKeyFile { .. }),
        "the envelope opened, so this is not a SealedKey failure: {error}"
    );
}

#[test]
fn a_sealed_key_is_recognised_without_reading_it() {
    // Six bytes, not a file. Used to tell "there is no key yet, offer to make
    // one" from "there is a key, ask for its passphrase" -- different
    // questions with different first words.
    let dir = tempdir().unwrap();
    let sealed = dir.path().join("signing.bpadx");
    let plain = dir.path().join("plain.key");
    let missing = dir.path().join("nothing.bpadx");

    write_sealed_signing_key(&sealed, &key(), PASSPHRASE).unwrap();
    write_signing_key(&plain, &key()).unwrap();

    assert!(is_sealed_key_file(&sealed));
    assert!(
        !is_sealed_key_file(&plain),
        "a raw seed file has no magic and must not be taken for a sealed one"
    );
    assert!(!is_sealed_key_file(&missing), "a missing file is not a key");
}

#[test]
fn a_file_shorter_than_the_magic_is_not_a_sealed_key() {
    // The short-read trap, the same one the document open path has: `read`
    // may return fewer bytes than asked for without being at the end, so this
    // uses `read_exact` -- and a file genuinely shorter than the magic must
    // answer `false` rather than fail.
    let dir = tempdir().unwrap();
    for (name, contents) in [("empty", ""), ("tiny", "BP")] {
        let path = dir.path().join(name);
        fs::write(&path, contents).unwrap();
        assert!(!is_sealed_key_file(&path), "{name}");
    }
}

#[test]
fn an_empty_passphrase_is_refused_rather_than_sealing_with_nothing() {
    // `bp-crypto` refuses it, and this asserts the refusal reaches the
    // caller. A key file "sealed" under an empty passphrase is a key file in
    // clear with extra steps.
    let dir = tempdir().unwrap();
    let path = dir.path().join("signing.bpadx");

    assert!(matches!(
        write_sealed_signing_key(&path, &key(), ""),
        Err(IntegrityError::SealedKey { .. })
    ));
    assert!(
        !path.exists(),
        "a refused seal must not leave a file behind"
    );
}

#[test]
fn the_file_mode_is_still_narrowed_where_the_platform_allows_it() {
    // Two locks are not worse than one. The envelope is what makes this safe
    // on Windows, where nothing can be narrowed; on Linux the mode is still
    // set, because a key file that is also unreadable by other local users is
    // strictly better than one that is merely unreadable.
    let dir = tempdir().unwrap();
    let path = dir.path().join("signing.bpadx");

    let reported = write_sealed_signing_key(&path, &key(), PASSPHRASE).unwrap();
    assert_eq!(reported, key_file_protection(&path).unwrap());
}
