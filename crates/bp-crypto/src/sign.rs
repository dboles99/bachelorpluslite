//! Document hashing and detached Ed25519 signatures (specs.md section 15).
//!
//! **This module composes cryptography; it implements none.** SHA-256 and
//! SHA-512 come from `sha2`, Ed25519 from `ed25519-dalek`, randomness from the
//! OS. ADR-0011 draws the line exactly there: choosing which primitive answers
//! which question is ours to do, and writing one is not.
//!
//! ## A hash and a signature answer different questions
//!
//! A hash answers *are these two files the same*. Anyone can compute one, so
//! anyone can also compute a matching digest for a document they substituted;
//! a hash on its own says nothing about who produced the bytes. It is still
//! the right tool when the digest travels by a channel the reader already
//! trusts -- read down a telephone, printed on a covering letter, quoted in an
//! email the recipient can already authenticate.
//!
//! A signature answers *did the holder of this key approve these exact bytes*,
//! which is the question a hash cannot reach.
//!
//! ## Detached, not embedded
//!
//! [`sign_document`] returns bytes to file beside the document -- the `.sig`
//! convention -- rather than wrapping the document in a new container. A
//! signed `.docx` has to remain a `.docx`: embedding would make signing a
//! destructive edit, and would oblige every reader of every format this
//! product supports to learn to unwrap us first. The cost is that the two
//! files can be separated, which is a filing problem rather than a security
//! one, because a missing signature fails closed.
//!
//! ## Key and signature formats, which are stable
//!
//! Keys outlive builds, so the encodings are stated here rather than left to
//! be inferred from whatever this build happens to write:
//!
//! * a **signing key** is its 32-byte Ed25519 seed (RFC 8032 section 5.1.5),
//!   the value the expanded scalar and the public key are derived from;
//! * a **verifying key** is the 32-byte compressed Edwards point of RFC 8032
//!   section 5.1.5 -- what every other Ed25519 implementation calls a public
//!   key;
//! * a **signature** is the 64-byte `R || s` of RFC 8032 section 5.1.6.
//!
//! All three are raw bytes with no magic, length prefix or version, unlike the
//! `.bpadx` envelope next door. The envelope is ours and can grow; these three
//! are fixed-length values pinned by a standard that will not be revised, and
//! adding a wrapper would only make keys this product wrote unreadable to
//! every other tool. Verifying keys additionally have a hexadecimal spelling
//! ([`VerifyingKey::to_hex`]) because a public key has to survive being pasted
//! into an email.
//!
//! ## One thing this cannot do
//!
//! **Tell you whose key it is.** [`verify_document`] proves that a signature
//! was made by the holder of one specific verifying key; it has no opinion on
//! whether that key belongs to the name on the document. Binding a key to a
//! person needs something this crate does not have, and implying otherwise
//! would be exactly the sort of reassuring lie ADR-0020 refuses elsewhere.

use ed25519_dalek::Signer;
use rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256, Sha512};
use std::fmt;
use zeroize::Zeroizing;

/// Length of a signing key in bytes. Taken from the library rather than
/// written out, so the constant cannot drift from what is actually parsed.
pub const SIGNING_KEY_LEN: usize = ed25519_dalek::SECRET_KEY_LENGTH;

/// Length of a verifying key in bytes.
pub const VERIFYING_KEY_LEN: usize = ed25519_dalek::PUBLIC_KEY_LENGTH;

/// Length of a detached signature in bytes -- and therefore the exact size of
/// a `.sig` file, which is worth knowing before opening one.
pub const SIGNATURE_LEN: usize = ed25519_dalek::SIGNATURE_LENGTH;

/// What went wrong signing, verifying or reading a key.
///
/// Separate from [`crate::CryptoError`] because the two describe different
/// worlds: that one is about a document that will not open, this one about a
/// claim about a document that does not hold. Folding them together would
/// produce an enum where half the variants are unreachable from either entry
/// point, and a user shown "wrong passphrase" for a bad signature.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SignError {
    /// The one answer a user acts on, and deliberately vague about which of
    /// the three causes applies: the difference is not knowable from the
    /// bytes, and guessing would be an accusation.
    #[error(
        "this signature does not match the document -- the document has been \
         altered, the signature has been altered, or it was signed by a \
         different key"
    )]
    DoesNotVerify,

    #[error("a signing key is exactly {SIGNING_KEY_LEN} bytes; this one is {0}")]
    WrongSigningKeyLength(usize),

    #[error("a verifying key is exactly {VERIFYING_KEY_LEN} bytes; this one is {0}")]
    WrongVerifyingKeyLength(usize),

    #[error("a signature is exactly {SIGNATURE_LEN} bytes; this one is {0}")]
    WrongSignatureLength(usize),

    #[error("this is the right length for a verifying key but is not a usable one")]
    NotAVerifyingKey,

    #[error("this is not a verifying key written in hexadecimal")]
    MalformedKeyText,

    #[error("could not read the system random number generator: {0}")]
    NoRandomness(String),
}

/// Which digest was taken.
///
/// Recorded alongside the bytes rather than assumed, so a digest can never be
/// compared against one of the other kind by accident -- the failure that
/// makes a checksum look wrong when both files are in fact identical.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HashAlgorithm {
    /// The default. Short enough that a user will actually read it to the end
    /// when comparing two screens, and what `sha256sum` and every download
    /// page already print.
    #[default]
    Sha256,
    /// Offered because some compliance regimes name it specifically, and
    /// because it is the faster of the two on 64-bit hardware. Twice the
    /// digits to compare by eye, which is why it is not the default.
    Sha512,
}

impl HashAlgorithm {
    /// The spelling a user recognises, matching the way the standard names it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Sha256 => "SHA-256",
            Self::Sha512 => "SHA-512",
        }
    }

    /// Digest length in bytes.
    #[must_use]
    pub const fn digest_len(self) -> usize {
        match self {
            Self::Sha256 => 32,
            Self::Sha512 => 64,
        }
    }
}

/// A digest of a document, with the algorithm that produced it.
///
/// Equality is the whole point of the type: two `DocumentHash` values compare
/// equal exactly when the same algorithm produced the same digest, so `a == b`
/// is the answer to "are these two documents the same file". A SHA-256 digest
/// never equals a SHA-512 one, whatever the bytes.
///
/// The comparison is an ordinary one, not constant-time, because a digest is
/// not a secret: it is computed from public bytes and shown to the user on
/// purpose. Timing here reveals only what the attacker could compute anyway.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentHash {
    algorithm: HashAlgorithm,
    bytes: Vec<u8>,
}

impl DocumentHash {
    /// Which algorithm produced this, so the user can be told.
    #[must_use]
    pub const fn algorithm(&self) -> HashAlgorithm {
        self.algorithm
    }

    /// The raw digest, for a caller that has to write it somewhere binary.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Lowercase hexadecimal with no separators.
    ///
    /// The machine-comparable form, and deliberately byte-for-byte what
    /// `sha256sum` and `certutil` print, so a user can paste ours next to
    /// theirs and see one string or two.
    #[must_use]
    pub fn to_hex(&self) -> String {
        hex(&self.bytes)
    }

    /// The form to put in front of a user: the algorithm's name, then the
    /// digest in groups of four.
    ///
    /// A person asked to compare two 64-character strings loses their place;
    /// asked to compare sixteen groups of four, they do not. There is
    /// deliberately no `Display` implementation choosing between this and
    /// [`Self::to_hex`] on the caller's behalf, because a checksum with spaces
    /// in it and a heading without them are both wrong in ways nobody notices
    /// until a user is already confused.
    #[must_use]
    pub fn to_display(&self) -> String {
        let digits = self.to_hex();
        let mut out = String::with_capacity(digits.len() + digits.len() / 4 + 8);
        out.push_str(self.algorithm.name());
        out.push_str("  ");
        for (index, digit) in digits.chars().enumerate() {
            if index > 0 && index % 4 == 0 {
                out.push(' ');
            }
            out.push(digit);
        }
        out
    }
}

/// Take a digest of a document.
///
/// The whole document at once: a caller holding bytes too large for memory has
/// a streaming problem this crate does not solve today, and pretending to
/// solve it with a chunked API that still buffers would be worse than the
/// honest signature.
#[must_use]
pub fn hash_document(document: &[u8], algorithm: HashAlgorithm) -> DocumentHash {
    let bytes = match algorithm {
        HashAlgorithm::Sha256 => Sha256::digest(document).to_vec(),
        HashAlgorithm::Sha512 => Sha512::digest(document).to_vec(),
    };
    DocumentHash { algorithm, bytes }
}

/// The private half of a signing identity.
///
/// Held as `ed25519-dalek`'s own type, which zeroizes on drop; the seed is
/// only ever exposed through [`Self::to_bytes`], and then inside `Zeroizing`,
/// so a caller who writes it to a file does not also leave it in freed memory.
pub struct SigningKey(ed25519_dalek::SigningKey);

impl fmt::Debug for SigningKey {
    /// The public half only.
    ///
    /// `ed25519-dalek` already redacts its secret, but restating it here means
    /// a later change to the wrapped type -- or a derived `Debug` added
    /// thoughtlessly to this one -- cannot quietly start printing a private
    /// key into a log file.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SigningKey")
            .field("verifying_key", &self.verifying_key().to_hex())
            .finish_non_exhaustive()
    }
}

impl SigningKey {
    /// Generate a new signing identity from the operating system's CSPRNG.
    ///
    /// The seed is drawn with `try_fill_bytes` and the key built from it,
    /// rather than through `ed25519-dalek`'s own `generate`, for one reason:
    /// `generate` takes the infallible `fill_bytes`, which **panics** if the
    /// OS random source is unavailable. A machine that cannot produce
    /// randomness should tell the user so, not take the application down.
    pub fn generate() -> Result<Self, SignError> {
        let mut seed = Zeroizing::new([0u8; SIGNING_KEY_LEN]);
        OsRng
            .try_fill_bytes(&mut seed[..])
            .map_err(|e| SignError::NoRandomness(e.to_string()))?;
        Ok(Self(ed25519_dalek::SigningKey::from_bytes(&seed)))
    }

    /// Read a signing key from its stored 32-byte seed.
    ///
    /// Every 32-byte string is a valid seed, so the only way this fails is on
    /// length -- and that is worth its own error rather than a silent pad or
    /// truncate, because a key read one byte short is a key that signs
    /// perfectly well and matches nothing anyone has ever verified against.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SignError> {
        if bytes.len() != SIGNING_KEY_LEN {
            return Err(SignError::WrongSigningKeyLength(bytes.len()));
        }
        let mut seed = Zeroizing::new([0u8; SIGNING_KEY_LEN]);
        seed.copy_from_slice(bytes);
        Ok(Self(ed25519_dalek::SigningKey::from_bytes(&seed)))
    }

    /// The 32-byte seed, for storing the key.
    ///
    /// `Zeroizing` rather than discipline: the caller is about to write this
    /// to a file or a keyring, and every path out of that code -- including
    /// the ones that return early on an I/O error -- has to leave nothing
    /// behind.
    #[must_use]
    pub fn to_bytes(&self) -> Zeroizing<[u8; SIGNING_KEY_LEN]> {
        Zeroizing::new(self.0.to_bytes())
    }

    /// The public half, derived rather than stored.
    ///
    /// Storing only the seed means the two halves cannot be filed apart and
    /// later paired wrongly -- a mismatch that produces signatures nobody can
    /// verify and no error at the time it is made.
    #[must_use]
    pub fn verifying_key(&self) -> VerifyingKey {
        VerifyingKey(self.0.verifying_key())
    }
}

/// The public half of a signing identity: what a recipient needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerifyingKey(ed25519_dalek::VerifyingKey);

impl VerifyingKey {
    /// Read a verifying key from its 32 stored bytes.
    ///
    /// Unlike a seed, not every 32-byte string is one: the encoding is a point
    /// on a curve, and most strings are not on it. Refusing here rather than
    /// at verification time is the difference between "that is not a key" and
    /// "that signature is bad", and only one of those is true.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SignError> {
        let point: [u8; VERIFYING_KEY_LEN] = bytes
            .try_into()
            .map_err(|_| SignError::WrongVerifyingKeyLength(bytes.len()))?;
        ed25519_dalek::VerifyingKey::from_bytes(&point)
            .map(Self)
            .map_err(|_| SignError::NotAVerifyingKey)
    }

    /// The 32 bytes, for storing or sending the key.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; VERIFYING_KEY_LEN] {
        self.0.to_bytes()
    }

    /// The key as lowercase hexadecimal.
    ///
    /// A public key travels between people -- in an email, a README, a message
    /// -- and raw bytes do not survive that journey. This spelling belongs
    /// here rather than in the shell so that every part of the product writes
    /// a key the same way; a public key formatted two ways is a public key
    /// users believe they have two of.
    ///
    /// There is deliberately no such method on [`SigningKey`]. A private key
    /// turned into a `String` is a private key in an allocation nobody wipes,
    /// and offering the convenience would be inviting exactly that.
    #[must_use]
    pub fn to_hex(&self) -> String {
        hex(&self.to_bytes())
    }

    /// Read a verifying key from its hexadecimal spelling.
    ///
    /// Whitespace anywhere is ignored, because the realistic input is a key
    /// pasted out of an email that wrapped it across two lines, and refusing
    /// that teaches users to retype keys by hand.
    pub fn from_hex(text: &str) -> Result<Self, SignError> {
        let bytes = unhex(text).ok_or(SignError::MalformedKeyText)?;
        Self::from_bytes(&bytes)
    }
}

/// A detached signature: the entire contents of a `.sig` file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Signature(ed25519_dalek::Signature);

impl Signature {
    /// Read a signature from the 64 bytes of a `.sig` file.
    ///
    /// The length check is the reason this returns a `Result`: a truncated
    /// signature file is a filing accident, and telling the user their file is
    /// incomplete is actionable in a way that "this signature does not match"
    /// is not -- the latter would send them looking for a tampered document
    /// that does not exist.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SignError> {
        let raw: [u8; SIGNATURE_LEN] = bytes
            .try_into()
            .map_err(|_| SignError::WrongSignatureLength(bytes.len()))?;
        Ok(Self(ed25519_dalek::Signature::from_bytes(&raw)))
    }

    /// The 64 bytes to write to a `.sig` file.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; SIGNATURE_LEN] {
        self.0.to_bytes()
    }
}

/// Sign a document.
///
/// Cannot fail, and takes no randomness: Ed25519 derives its per-signature
/// nonce from the key and the message, so signing the same bytes twice gives
/// the same signature. That determinism is a feature rather than a leak -- it
/// is what removes the catastrophic nonce-reuse failure that sinks careless
/// ECDSA implementations -- and it also means a user who re-signs a document
/// gets a file identical to the one they already have, instead of a spurious
/// change to review.
#[must_use]
pub fn sign_document(key: &SigningKey, document: &[u8]) -> Signature {
    Signature(key.0.sign(document))
}

/// Verify a detached signature over a document.
///
/// Uses `ed25519-dalek`'s strict verification, which additionally rejects
/// small-order keys and non-canonical signature points. The permissive check
/// is enough to answer "did this key sign this", but here a signature is
/// evidence: strict verification is what makes a signature bind to exactly one
/// key and one document, rather than being a value that can be made to verify
/// under several. The cost is that a handful of pathological keys no honest
/// generator produces are refused.
///
/// Every failure is the same [`SignError::DoesNotVerify`], because the bytes
/// do not distinguish an altered document from an altered signature from the
/// wrong key, and inventing a distinction would be telling the user something
/// that is not known.
pub fn verify_document(
    key: &VerifyingKey,
    document: &[u8],
    signature: &Signature,
) -> Result<(), SignError> {
    key.0
        .verify_strict(document, &signature.0)
        .map_err(|_| SignError::DoesNotVerify)
}

/// Lowercase hexadecimal, the one spelling used everywhere in this module.
fn hex(bytes: &[u8]) -> String {
    use fmt::Write as _;
    bytes.iter().fold(String::new(), |mut out, byte| {
        // Writing to a `String` cannot fail, so there is nothing to handle.
        let _ = write!(out, "{byte:02x}");
        out
    })
}

/// Hexadecimal back to bytes, ignoring whitespace.
///
/// Digits are decoded by hand rather than through `u8::from_str_radix`, which
/// accepts a leading `+` and would quietly read "+f" as a byte. A key parser
/// that accepts things that are not keys is a key parser that lets a mangled
/// paste through.
fn unhex(text: &str) -> Option<Vec<u8>> {
    const fn nibble(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            _ => None,
        }
    }

    let digits: Vec<u8> = text
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    if !digits.len().is_multiple_of(2) {
        return None;
    }
    digits
        .chunks_exact(2)
        .map(|pair| Some((nibble(pair[0])? << 4) | nibble(pair[1])?))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const DOCUMENT: &[u8] = b"the quick brown fox jumps over the lazy dog";

    fn key() -> SigningKey {
        SigningKey::generate().expect("the OS random source is available in tests")
    }

    // --- hashing ---------------------------------------------------------

    #[test]
    fn each_algorithm_reproduces_the_published_answer_for_a_known_input() {
        // The one test that would catch this module hashing something other
        // than the document -- a length prefix, a trailing newline, the wrong
        // algorithm entirely. Vectors are the standard ones for "abc".
        assert_eq!(
            hash_document(b"abc", HashAlgorithm::Sha256).to_hex(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hash_document(b"abc", HashAlgorithm::Sha512).to_hex(),
            "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a\
             2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
        );
    }

    #[test]
    fn an_empty_document_hashes_to_the_published_empty_digest() {
        // Nothing special-cased on the way in: the empty document must give
        // the same digest every other tool gives it, or a user comparing
        // against `sha256sum` sees a mismatch that is ours.
        assert_eq!(
            hash_document(b"", HashAlgorithm::Sha256).to_hex(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn each_algorithm_produces_a_digest_of_its_stated_length() {
        for algorithm in [HashAlgorithm::Sha256, HashAlgorithm::Sha512] {
            let hash = hash_document(DOCUMENT, algorithm);
            assert_eq!(
                hash.as_bytes().len(),
                algorithm.digest_len(),
                "{} disagreed with its own digest_len",
                algorithm.name()
            );
        }
    }

    #[test]
    fn the_same_document_hashes_the_same_way_and_a_changed_one_does_not() {
        for algorithm in [HashAlgorithm::Sha256, HashAlgorithm::Sha512] {
            let original = hash_document(DOCUMENT, algorithm);
            assert_eq!(original, hash_document(DOCUMENT, algorithm));

            let mut altered = DOCUMENT.to_vec();
            altered[0] ^= 0x01;
            assert_ne!(
                original,
                hash_document(&altered, algorithm),
                "{} matched a document it should not have",
                algorithm.name()
            );
        }
    }

    #[test]
    fn digests_of_different_algorithms_never_compare_equal() {
        // The reason the algorithm is carried alongside the bytes. Comparing a
        // SHA-256 digest against a SHA-512 one is a mistake, and answering
        // "not equal" is the only honest result -- answering "equal" for a
        // shared prefix would be worse than useless.
        let short = hash_document(DOCUMENT, HashAlgorithm::Sha256);
        let long = hash_document(DOCUMENT, HashAlgorithm::Sha512);

        assert_ne!(short, long);
    }

    #[test]
    fn a_digest_shown_to_a_user_names_its_algorithm_and_is_grouped_in_fours() {
        let shown = hash_document(b"abc", HashAlgorithm::Sha256).to_display();

        assert!(shown.starts_with("SHA-256  "), "unnamed algorithm: {shown}");
        assert!(
            shown.contains("ba78 16bf 8f01 cfea"),
            "not grouped: {shown}"
        );
        // Same digits, so a user comparing the shown form against a pasted
        // checksum is comparing the same value.
        let plain: String = shown.chars().filter(char::is_ascii_hexdigit).collect();
        assert!(plain.ends_with(&hash_document(b"abc", HashAlgorithm::Sha256).to_hex()));
    }

    // --- signing: the working case ---------------------------------------

    #[test]
    fn a_signature_verifies_under_the_key_that_made_it() {
        let signing = key();
        let signature = sign_document(&signing, DOCUMENT);

        assert_eq!(
            verify_document(&signing.verifying_key(), DOCUMENT, &signature),
            Ok(())
        );
    }

    #[test]
    fn a_signature_over_an_empty_document_verifies_and_only_under_its_own_key() {
        // An empty document is still a document somebody signed, and an
        // implementation that special-cased it would be one where a signature
        // over nothing verifies under anything.
        let signing = key();
        let other = key();
        let signature = sign_document(&signing, b"");

        assert_eq!(
            verify_document(&signing.verifying_key(), b"", &signature),
            Ok(())
        );
        assert_eq!(
            verify_document(&other.verifying_key(), b"", &signature),
            Err(SignError::DoesNotVerify)
        );
    }

    #[test]
    fn a_detached_signature_is_the_same_size_whatever_the_document() {
        // What "detached" means in practice: the signature does not carry the
        // document, so it cannot be mistaken for a copy of it.
        let signing = key();
        let large = vec![b'x'; 100_000];

        assert_eq!(sign_document(&signing, b"").to_bytes().len(), SIGNATURE_LEN);
        assert_eq!(
            sign_document(&signing, &large).to_bytes().len(),
            SIGNATURE_LEN
        );
    }

    #[test]
    fn signing_the_same_document_twice_gives_the_same_signature() {
        // Ed25519 is deterministic by design, which is what removes the
        // nonce-reuse failure mode. A random signature here would mean a
        // nonce came from somewhere it should not have.
        let signing = key();

        assert_eq!(
            sign_document(&signing, DOCUMENT),
            sign_document(&signing, DOCUMENT)
        );
    }

    #[test]
    fn two_generated_keys_are_different() {
        // A fixed seed would make every user of this product share one
        // identity, and every signature forgeable by any of them.
        assert_ne!(key().verifying_key(), key().verifying_key());
    }

    // --- signing: every failure path -------------------------------------

    #[test]
    fn a_signature_does_not_verify_under_a_different_key() {
        let signing = key();
        let stranger = key();
        let signature = sign_document(&signing, DOCUMENT);

        assert_eq!(
            verify_document(&stranger.verifying_key(), DOCUMENT, &signature),
            Err(SignError::DoesNotVerify)
        );
    }

    #[test]
    fn a_signature_made_by_a_different_keypair_is_refused_for_this_document() {
        // The same failure seen from the other side: not "my key against your
        // signature" but "your signature offered as mine". A signature is
        // evidence about one key, and swapping the signature must not produce
        // evidence about another.
        let mine = key();
        let theirs = key();
        let forged = sign_document(&theirs, DOCUMENT);

        assert_eq!(
            verify_document(&mine.verifying_key(), DOCUMENT, &forged),
            Err(SignError::DoesNotVerify)
        );
    }

    #[test]
    fn an_altered_document_does_not_verify() {
        let signing = key();
        let signature = sign_document(&signing, DOCUMENT);

        let mut altered = DOCUMENT.to_vec();
        let last = altered.len() - 1;
        altered[last] ^= 0x01;

        assert_eq!(
            verify_document(&signing.verifying_key(), &altered, &signature),
            Err(SignError::DoesNotVerify)
        );
    }

    #[test]
    fn a_document_with_bytes_appended_does_not_verify() {
        // The alteration that costs nothing to attempt: leave the signed bytes
        // alone and add to the end. A length that was not covered would let
        // this through.
        let signing = key();
        let signature = sign_document(&signing, DOCUMENT);

        let mut extended = DOCUMENT.to_vec();
        extended.push(b'!');

        assert_eq!(
            verify_document(&signing.verifying_key(), &extended, &signature),
            Err(SignError::DoesNotVerify)
        );
    }

    #[test]
    fn an_altered_signature_does_not_verify() {
        let signing = key();
        let signature = sign_document(&signing, DOCUMENT);

        let mut raw = signature.to_bytes();
        raw[0] ^= 0x01;
        let tampered = Signature::from_bytes(&raw).unwrap();

        assert_eq!(
            verify_document(&signing.verifying_key(), DOCUMENT, &tampered),
            Err(SignError::DoesNotVerify)
        );
    }

    #[test]
    fn every_byte_of_a_signature_is_load_bearing() {
        // The property behind the single-bit test above: there is no byte of a
        // signature that can be changed and still accepted. A byte that could
        // be is a byte carrying nothing, which is where a verifier grows a
        // hole -- and it is how signature malleability is spotted.
        let signing = key();
        let verifying = signing.verifying_key();
        let signature = sign_document(&signing, DOCUMENT);
        let original = signature.to_bytes();

        for index in 0..SIGNATURE_LEN {
            let mut raw = original;
            raw[index] ^= 0xff;
            let altered = Signature::from_bytes(&raw).unwrap();

            assert_eq!(
                verify_document(&verifying, DOCUMENT, &altered),
                Err(SignError::DoesNotVerify),
                "flipping byte {index} of the signature still verified"
            );
        }
    }

    #[test]
    fn a_truncated_signature_is_refused_as_incomplete_rather_than_as_a_mismatch() {
        // A `.sig` file cut short is a copying accident. Reporting it as a
        // failed verification would send the user hunting for a tampered
        // document that does not exist.
        let signing = key();
        let signature = sign_document(&signing, DOCUMENT);
        let raw = signature.to_bytes();

        assert_eq!(
            Signature::from_bytes(&raw[..SIGNATURE_LEN - 1]),
            Err(SignError::WrongSignatureLength(SIGNATURE_LEN - 1))
        );
        assert_eq!(
            Signature::from_bytes(&[]),
            Err(SignError::WrongSignatureLength(0))
        );
    }

    #[test]
    fn a_signature_with_bytes_appended_is_refused_by_length() {
        let signing = key();
        let mut raw = sign_document(&signing, DOCUMENT).to_bytes().to_vec();
        raw.push(0);

        assert_eq!(
            Signature::from_bytes(&raw),
            Err(SignError::WrongSignatureLength(SIGNATURE_LEN + 1))
        );
    }

    #[test]
    fn a_signing_key_of_the_wrong_length_is_refused_rather_than_padded() {
        // Padding or truncating would produce a key that signs perfectly well
        // and matches nothing anyone has ever verified against -- a failure
        // that only surfaces at the recipient.
        //
        // Compared by unwrapping the error rather than the whole `Result`,
        // because `SigningKey` deliberately has no `PartialEq`: comparing two
        // private keys with `==` is a byte comparison that leaks through
        // timing, and the operation has no honest use here.
        assert_eq!(
            SigningKey::from_bytes(&[0u8; SIGNING_KEY_LEN - 1]).unwrap_err(),
            SignError::WrongSigningKeyLength(SIGNING_KEY_LEN - 1)
        );
        assert_eq!(
            SigningKey::from_bytes(&[0u8; SIGNING_KEY_LEN + 1]).unwrap_err(),
            SignError::WrongSigningKeyLength(SIGNING_KEY_LEN + 1)
        );
        assert_eq!(
            SigningKey::from_bytes(&[]).unwrap_err(),
            SignError::WrongSigningKeyLength(0)
        );
    }

    #[test]
    fn a_verifying_key_of_the_wrong_length_is_refused() {
        assert_eq!(
            VerifyingKey::from_bytes(&[0u8; VERIFYING_KEY_LEN - 1]),
            Err(SignError::WrongVerifyingKeyLength(VERIFYING_KEY_LEN - 1))
        );
        assert_eq!(
            VerifyingKey::from_bytes(&[0u8; VERIFYING_KEY_LEN + 1]),
            Err(SignError::WrongVerifyingKeyLength(VERIFYING_KEY_LEN + 1))
        );
    }

    #[test]
    fn bytes_of_the_right_length_that_are_not_a_key_are_refused_as_such() {
        // Distinct from a length problem and from a bad signature: the user
        // pasted something that is not a key at all, and can act on that.
        //
        // A verifying key is a compressed curve point, and most 32-byte
        // strings do not decompress to one -- 0x02 repeated is one that does
        // not. The constant is pinned rather than searched for, so this test
        // asserts a fact about Curve25519 rather than about whatever the
        // search happened to find today.
        assert_eq!(
            VerifyingKey::from_bytes(&[0x02u8; VERIFYING_KEY_LEN]),
            Err(SignError::NotAVerifyingKey)
        );
    }

    #[test]
    fn a_degenerate_key_cannot_verify_anything() {
        // The all-zero encoding is a legitimate point but a small-order one:
        // signatures under it verify against many documents. Strict
        // verification is what refuses it, whether or not parsing did.
        let signing = key();
        let signature = sign_document(&signing, DOCUMENT);

        match VerifyingKey::from_bytes(&[0u8; VERIFYING_KEY_LEN]) {
            Err(e) => assert_eq!(e, SignError::NotAVerifyingKey),
            Ok(degenerate) => assert_eq!(
                verify_document(&degenerate, DOCUMENT, &signature),
                Err(SignError::DoesNotVerify),
                "a small-order key was allowed to verify"
            ),
        }
    }

    // --- serialisation, because keys outlive builds -----------------------

    #[test]
    fn a_signing_key_round_trips_through_its_bytes_and_signs_identically() {
        // The guarantee that matters in ten years: a key stored today loads
        // tomorrow and produces the same signatures, not merely a valid key.
        let original = key();
        let stored = original.to_bytes();
        let reloaded = SigningKey::from_bytes(stored.as_ref()).unwrap();

        assert_eq!(reloaded.verifying_key(), original.verifying_key());
        assert_eq!(
            sign_document(&reloaded, DOCUMENT),
            sign_document(&original, DOCUMENT)
        );
    }

    #[test]
    fn a_verifying_key_round_trips_through_its_bytes_and_through_hexadecimal() {
        let verifying = key().verifying_key();

        assert_eq!(
            VerifyingKey::from_bytes(&verifying.to_bytes()),
            Ok(verifying)
        );
        assert_eq!(VerifyingKey::from_hex(&verifying.to_hex()), Ok(verifying));
        assert_eq!(verifying.to_hex().len(), VERIFYING_KEY_LEN * 2);
    }

    #[test]
    fn a_key_pasted_across_two_lines_still_reads() {
        // The realistic input. Refusing wrapped text teaches users to retype
        // keys by hand, which is how a key gets a wrong digit in it.
        let verifying = key().verifying_key();
        let text = verifying.to_hex();
        let wrapped = format!("  {}\n  {}\n", &text[..32], &text[32..]);

        assert_eq!(VerifyingKey::from_hex(&wrapped), Ok(verifying));
    }

    #[test]
    fn text_that_is_not_hexadecimal_is_refused_by_name() {
        for text in [
            "not hexadecimal at all",
            // An odd number of digits: something was lost in the paste.
            "abc",
            // `u8::from_str_radix` would accept this pair as 0x0f.
            "+f0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20",
        ] {
            assert_eq!(
                VerifyingKey::from_hex(text),
                Err(SignError::MalformedKeyText),
                "accepted {text:?} as a key"
            );
        }
    }

    #[test]
    fn hexadecimal_of_the_wrong_length_is_refused_as_a_length_problem() {
        // Well-formed digits, wrong number of them: the user pasted half a
        // key, and saying so is more use than "not hexadecimal".
        assert_eq!(
            VerifyingKey::from_hex("aabb"),
            Err(SignError::WrongVerifyingKeyLength(2))
        );
    }

    #[test]
    fn a_signature_round_trips_through_its_detached_bytes() {
        // The `.sig` file contract: what is written is what is read.
        let signing = key();
        let signature = sign_document(&signing, DOCUMENT);
        let filed = signature.to_bytes();

        assert_eq!(Signature::from_bytes(&filed), Ok(signature));
        assert_eq!(filed.len(), SIGNATURE_LEN);
        assert_eq!(
            verify_document(
                &signing.verifying_key(),
                DOCUMENT,
                &Signature::from_bytes(&filed).unwrap()
            ),
            Ok(())
        );
    }

    #[test]
    fn a_signing_key_never_prints_its_private_half() {
        // `Debug` output ends up in logs and panic messages. A key that leaks
        // there is a key that leaks everywhere.
        let signing = key();
        let printed = format!("{signing:?}");
        let secret = hex(signing.to_bytes().as_ref());

        assert!(
            !printed.contains(&secret),
            "the seed was printed: {printed}"
        );
        assert!(printed.contains(&signing.verifying_key().to_hex()));
    }

    // --- properties -------------------------------------------------------

    proptest! {
        /// The guarantee, over every document rather than the one above:
        /// what this module signs, it verifies.
        #[test]
        fn any_document_verifies_under_the_key_that_signed_it(
            document in prop::collection::vec(any::<u8>(), 0..2048),
        ) {
            let signing = key();
            let signature = sign_document(&signing, &document);

            prop_assert_eq!(
                verify_document(&signing.verifying_key(), &document, &signature),
                Ok(())
            );
        }

        /// The other half, and the one that can actually catch something:
        /// there is no byte of a document that can be changed without the
        /// signature refusing it. A byte that could be is a byte outside what
        /// was signed.
        #[test]
        fn altering_any_single_byte_of_a_document_breaks_its_signature(
            document in prop::collection::vec(any::<u8>(), 1..2048),
            position in any::<prop::sample::Index>(),
            flip in 1u8..=255,
        ) {
            let signing = key();
            let signature = sign_document(&signing, &document);

            let mut altered = document.clone();
            let index = position.index(altered.len());
            altered[index] ^= flip;

            prop_assert_eq!(
                verify_document(&signing.verifying_key(), &altered, &signature),
                Err(SignError::DoesNotVerify)
            );
        }

        /// Hashing has the same shape: a digest identifies the bytes it was
        /// taken over, and nothing else.
        #[test]
        fn altering_any_single_byte_of_a_document_changes_its_digest(
            document in prop::collection::vec(any::<u8>(), 1..2048),
            position in any::<prop::sample::Index>(),
            flip in 1u8..=255,
        ) {
            let mut altered = document.clone();
            let index = position.index(altered.len());
            altered[index] ^= flip;

            prop_assert_ne!(
                hash_document(&document, HashAlgorithm::Sha256),
                hash_document(&altered, HashAlgorithm::Sha256)
            );
        }
    }
}
