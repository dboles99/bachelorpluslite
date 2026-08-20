//! Seam: `bp-crypto` sealing a document that `bp-files` writes and reads.
//!
//! Both halves are covered in isolation -- `bp-crypto` seals and opens byte
//! arrays, `bp-files` writes and verifies byte arrays -- and neither has ever
//! seen the other. What joins them is that a sealed document is *binary*,
//! while every `bp-files` test is text, and that a real save goes through
//! `atomic_write`'s read-back verification with those bytes.
//!
//! The properties here are the ones a user would notice being broken:
//!
//! * what comes back out is byte-for-byte what went in, at every size,
//!   including the chunk boundary and either side of it, and including the
//!   empty document;
//! * a wrong passphrase fails, and yields nothing;
//! * a single altered bit anywhere in the file fails, rather than opening
//!   part of the document.
//!
//! Nothing here prints document content or a passphrase; see `common`.

mod common;

use common::{Bytes, Pass, cheap, cheap_with_chunk, no_files};

use bp_crypto::{CryptoError, SealOptions, is_bpadx, open, seal};
use bp_files::{SaveOptions, atomic_write, load};
use proptest::prelude::*;
use tempfile::tempdir;

/// The smallest chunk size the format permits, so a test can cross a chunk
/// boundary in kilobytes rather than in megabytes. The framing rules under
/// test are the same at every size.
const SMALL_CHUNK: u32 = 1024;

/// Seal, write with the real save path, read back, open.
///
/// `atomic_write` rather than `fs::write` deliberately: the save the product
/// performs verifies by reading the file back, and a verification that
/// mis-handled a NUL or a high byte would fail here and nowhere else.
fn round_trip(plaintext: &[u8], passphrase: &str, options: SealOptions) -> Vec<u8> {
    let dir = tempdir().expect("temp dir");
    let path = dir.path().join("document.bpadx");

    let sealed = seal(plaintext, passphrase, options).expect("seal");
    atomic_write(&path, &sealed, SaveOptions::default()).expect("atomic_write");

    let from_disk = std::fs::read(&path).expect("read back");
    assert!(
        from_disk == sealed,
        "the file on disk is not the bytes that were sealed"
    );
    open(&from_disk, passphrase).expect("open").to_vec()
}

/// Deterministic filler, so a failure names a size rather than a document.
fn filler(size: usize) -> Vec<u8> {
    (0..size).map(|i| (i % 251) as u8).collect()
}

// --- byte-exactness -----------------------------------------------------

#[test]
fn the_empty_document_survives_the_round_trip() {
    // Not a formality: an empty document has no plaintext to authenticate,
    // and `seal` writes one empty chunk precisely so that an empty `.bpadx`
    // does not open under any passphrase at all.
    assert!(round_trip(b"", "correct horse", cheap()).is_empty());
}

#[test]
fn every_size_around_a_chunk_boundary_survives() {
    let chunk = SMALL_CHUNK as usize;
    for size in [
        0,
        1,
        chunk - 1,
        chunk,
        chunk + 1,
        2 * chunk - 1,
        2 * chunk,
        2 * chunk + 1,
    ] {
        let plaintext = filler(size);
        let out = round_trip(&plaintext, "correct horse", cheap_with_chunk(SMALL_CHUNK));
        assert_eq!(
            out.len(),
            size,
            "a {size}-byte document came back {} bytes long",
            out.len()
        );
        assert!(out == plaintext, "a {size}-byte document came back altered");
    }
}

#[test]
fn an_encrypted_document_leaves_no_plaintext_on_disk() {
    let dir = tempdir().expect("temp dir");
    let path = dir.path().join("secret.bpadx");
    let needle = "SENTINEL-6f2a9c1b";
    let document = format!("notes\n{needle}\nmore notes\n");

    let sealed = seal(document.as_bytes(), "correct horse", cheap()).expect("seal");
    atomic_write(&path, &sealed, SaveOptions::default()).expect("write");

    let on_disk = std::fs::read(&path).expect("read");
    assert!(
        !on_disk
            .windows(needle.len())
            .any(|w| w == needle.as_bytes()),
        "the sealed file contains its own plaintext"
    );
    assert!(is_bpadx(&on_disk), "the file does not carry the magic");

    // And the shell's routing question: a `.bpadx` must not be mistaken for
    // text and shown to the user as ciphertext. `load` refusing it outright is
    // the better answer and the usual one -- ciphertext is rarely valid UTF-8
    // -- but either way the plaintext must not appear.
    if let Ok(file) = load(&path) {
        assert!(
            !file.text.contains(needle),
            "loading a sealed file as text produced its plaintext"
        );
    }
}

// --- failing closed -----------------------------------------------------

#[test]
fn a_wrong_passphrase_yields_nothing() {
    let sealed = seal(b"the document", "correct horse", cheap()).expect("seal");

    let err = open(&sealed, "correct horst").expect_err("a wrong passphrase must not open it");
    assert_eq!(err, CryptoError::CannotOpen);
}

#[test]
fn an_empty_passphrase_is_refused_rather_than_accepted_as_a_weak_one() {
    assert_eq!(
        seal(b"the document", "", cheap()).unwrap_err(),
        CryptoError::EmptyPassphrase
    );
}

#[test]
fn a_document_that_is_not_bpadx_is_named_as_such() {
    let dir = tempdir().expect("temp dir");
    let path = dir.path().join("plain.txt");
    atomic_write(&path, b"just a note\n", SaveOptions::default()).expect("write");

    let bytes = std::fs::read(&path).expect("read");
    assert!(!is_bpadx(&bytes));
    assert_eq!(
        open(&bytes, "correct horse").unwrap_err(),
        CryptoError::NotBpadx
    );
}

#[test]
fn truncating_the_file_is_rejected_at_every_length() {
    let sealed = seal(
        &filler(3000),
        "correct horse",
        cheap_with_chunk(SMALL_CHUNK),
    )
    .expect("seal");

    for cut in 0..sealed.len() {
        assert!(
            open(&sealed[..cut], "correct horse").is_err(),
            "a file cut to {cut} of {} bytes opened",
            sealed.len()
        );
    }
}

proptest! {
    #![proptest_config(no_files(24))]

    /// The round trip is byte-exact for any content and any passphrase.
    #[test]
    fn sealing_and_opening_returns_the_same_bytes(
        plaintext in proptest::collection::vec(any::<u8>(), 0..4096).prop_map(Bytes),
        passphrase in "[ -~]{1,64}".prop_map(Pass),
    ) {
        let out = round_trip(
            plaintext.as_slice(),
            passphrase.as_str(),
            cheap_with_chunk(SMALL_CHUNK),
        );
        prop_assert_eq!(out.len(), plaintext.as_slice().len());
        prop_assert!(out == plaintext.0, "the document came back altered");
    }

    /// A different passphrase never opens the document, and never returns a
    /// partial one.
    #[test]
    fn a_different_passphrase_never_opens_it(
        plaintext in proptest::collection::vec(any::<u8>(), 0..512).prop_map(Bytes),
        right in "[ -~]{1,32}".prop_map(Pass),
        wrong in "[ -~]{1,32}".prop_map(Pass),
    ) {
        prop_assume!(right != wrong);
        let sealed = seal(plaintext.as_slice(), right.as_str(), cheap()).expect("seal");
        prop_assert!(open(&sealed, wrong.as_str()).is_err());
    }

    /// Appending to a sealed document does not extend it.
    #[test]
    fn trailing_bytes_are_rejected(
        extra in proptest::collection::vec(any::<u8>(), 1..32).prop_map(Bytes),
    ) {
        let mut sealed = seal(b"the document", "correct horse", cheap()).expect("seal");
        sealed.extend_from_slice(extra.as_slice());
        prop_assert!(open(&sealed, "correct horse").is_err());
    }
}

proptest! {
    // Every byte of the file is opened once, so a single case is already
    // several hundred key derivations. The case count is small on purpose;
    // what gives this its reach is that each case covers every byte offset.
    #![proptest_config(no_files(6))]

    /// One flipped bit anywhere in the file -- header, length prefix, chunk
    /// or tag -- makes the whole document refuse to open. Not "opens with one
    /// bad chunk", and not "opens with the rest": nothing comes back.
    #[test]
    fn one_flipped_bit_anywhere_makes_it_refuse(
        plaintext in proptest::collection::vec(any::<u8>(), 0..400).prop_map(Bytes),
        bit in 0u8..8,
    ) {
        let sealed = seal(
            plaintext.as_slice(),
            "correct horse",
            cheap_with_chunk(SMALL_CHUNK),
        )
        .expect("seal");
        for index in 0..sealed.len() {
            let mut damaged = sealed.clone();
            damaged[index] ^= 1 << bit;
            prop_assert!(
                open(&damaged, "correct horse").is_err(),
                "byte {} of {} with bit {} flipped still opened",
                index,
                sealed.len(),
                bit,
            );
        }
    }
}
