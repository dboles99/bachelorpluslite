//! The `.bpadx` encrypted document format (ADR-0021).
//!
//! **This crate composes cryptography; it implements none.** Argon2id comes
//! from `argon2`, the AEADs from `chacha20poly1305` and `aes-gcm`, randomness
//! from the OS. ADR-0011 draws the line exactly there: designing the envelope
//! is ours to do, and writing a cipher is not.
//!
//! It also decides no policy. Whether a document *should* be encrypted is
//! `bp-security`'s question (ADR-0020); neither crate depends on the other.
//!
//! ## The shape of a document
//!
//! ```text
//! header  magic, version, suite, KDF cost, salt, chunk size, nonce prefix
//! chunk 0 u32 length, then ciphertext+tag
//! chunk 1 ...
//! ```
//!
//! Each chunk is sealed separately, and its additional authenticated data is
//! **the whole header, the chunk's index, and whether it is the last**. Each
//! of those three prevents a specific attack, and leaving any one out gives a
//! format that still decrypts perfectly while being silently wrong:
//!
//! * without the header, the suite or the KDF cost can be edited to downgrade
//!   the document;
//! * without the index, chunks can be reordered or duplicated and every one
//!   still authenticates;
//! * without the last-chunk flag, the file can be truncated and the remainder
//!   still authenticates -- data loss that looks like a successful decrypt.
//!
//! ## Being reviewed
//!
//! ADR-0011 permits composing primitives and forbids implementing them, which
//! makes the *arrangement* below the part nobody here can check by re-reading
//! it. `docs/architecture/BPADX_ENVELOPE_REVIEW.md` is written for somebody
//! who has not seen this repository: the byte layout, the four call sites, a
//! self-audit against fourteen standard pitfalls with the test behind each,
//! and the ten questions a reviewer is asked. Four of the fourteen are
//! conceded rather than mitigated -- no key commitment, no domain separation,
//! no padding, no streaming API.
//!
//! ## One thing this cannot do
//!
//! **Tell a wrong passphrase from a corrupted file.** Both are an
//! authentication failure, and distinguishing them would mean trusting
//! something outside the authenticated data. [`CryptoError::CannotOpen`] says
//! both, deliberately.

#![forbid(unsafe_code)]

use aes_gcm::Aes256Gcm;
use aes_gcm::aead::{Aead, KeyInit, Payload};
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::XChaCha20Poly1305;
use zeroize::{Zeroize, Zeroizing};

mod header;
mod sign;

pub use header::{DEFAULT_CHUNK_SIZE, Header, KdfParams};
use header::{MAGIC, MAX_CHUNK_SIZE, SALT_LEN, VERSION};
pub use sign::{
    DocumentHash, HashAlgorithm, SIGNATURE_LEN, SIGNING_KEY_LEN, SignError, Signature, SigningKey,
    VERIFYING_KEY_LEN, VerifyingKey, hash_document, sign_document, verify_document,
};

/// Crate identity used by workspace smoke tests and diagnostics.
pub const CRATE_NAME: &str = "bp-crypto";

/// Which AEAD sealed a document.
///
/// Recorded per document rather than fixed, so the one not chosen today
/// remains available without a format break (ADR-0021).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Suite {
    /// The default. A 192-bit nonce makes a random per-file prefix safe
    /// without ceremony, and it is constant-time in software on any CPU --
    /// including those without AES instructions, where a software AES leaks
    /// timing.
    #[default]
    XChaCha20Poly1305,
    /// Hardware-accelerated almost everywhere, and the answer to compliance
    /// questions. The 96-bit nonce leaves less room, which the chunk counter
    /// accounts for.
    Aes256Gcm,
}

impl Suite {
    pub(crate) const fn tag(self) -> u8 {
        match self {
            Self::XChaCha20Poly1305 => 1,
            Self::Aes256Gcm => 2,
        }
    }

    pub(crate) fn from_tag(tag: u8) -> Result<Self, CryptoError> {
        match tag {
            1 => Ok(Self::XChaCha20Poly1305),
            2 => Ok(Self::Aes256Gcm),
            other => Err(CryptoError::UnsupportedSuite(other)),
        }
    }

    pub(crate) const fn nonce_len(self) -> usize {
        match self {
            Self::XChaCha20Poly1305 => 24,
            Self::Aes256Gcm => 12,
        }
    }

    /// How many random bytes lead each nonce. The remaining eight carry the
    /// chunk index, so a nonce cannot repeat within a file.
    pub(crate) const fn nonce_prefix_len(self) -> usize {
        self.nonce_len() - 8
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::XChaCha20Poly1305 => "XChaCha20-Poly1305",
            Self::Aes256Gcm => "AES-256-GCM",
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CryptoError {
    #[error("this is not a BachelorPad+ encrypted document")]
    NotBpadx,

    #[error(
        "this document was written by a newer BachelorPad+ (format {found}, \
         this build understands {supported})"
    )]
    UnsupportedVersion { found: u16, supported: u16 },

    #[error("unknown encryption suite {0}; this document needs a newer BachelorPad+")]
    UnsupportedSuite(u8),

    #[error("unknown key-derivation method {0}; this document needs a newer BachelorPad+")]
    UnsupportedKdf(u8),

    /// The one failure that cannot be diagnosed further. See the crate docs.
    #[error("wrong passphrase, or this document has been altered or damaged")]
    CannotOpen,

    #[error("this document is incomplete -- it was truncated or only partly copied")]
    Truncated,

    #[error("this document's structure is damaged")]
    Corrupt,

    #[error(
        "this document asks for an unreasonable amount of work to open \
         ({memory_kib} KiB, {iterations} passes, {lanes} lanes) and was refused"
    )]
    UnreasonableCost {
        memory_kib: u32,
        iterations: u32,
        lanes: u32,
    },

    #[error("this document declares an unreasonable chunk size ({0} bytes) and was refused")]
    UnreasonableChunkSize(u32),

    #[error("the passphrase is empty")]
    EmptyPassphrase,

    #[error("could not read the system random number generator: {0}")]
    NoRandomness(String),

    #[error("key derivation failed: {0}")]
    Kdf(String),
}

/// How a document should be sealed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SealOptions {
    pub suite: Suite,
    pub kdf: KdfParams,
    pub chunk_size: u32,
}

impl Default for SealOptions {
    fn default() -> Self {
        Self {
            suite: Suite::default(),
            kdf: KdfParams::default(),
            chunk_size: DEFAULT_CHUNK_SIZE,
        }
    }
}

/// Bytes an AEAD adds to a chunk: the authentication tag.
///
/// The same for both suites -- Poly1305 and GCM both produce 16 bytes -- which
/// is why one constant serves. A suite with a different tag size would need
/// this to move onto `Suite`.
const TAG_OVERHEAD: usize = 16;

/// A derived key, wiped when it goes out of scope.
///
/// `Zeroizing` rather than discipline: an early return that forgot to clear
/// this would leave the document's key in freed memory, and there are several
/// early returns below.
type Key = Zeroizing<[u8; 32]>;

fn derive(passphrase: &str, salt: &[u8], params: KdfParams) -> Result<Key, CryptoError> {
    params.validate()?;
    let argon = Argon2::new(
        Algorithm::Argon2id,
        Version::V0x13,
        Params::new(params.memory_kib, params.iterations, params.lanes, Some(32))
            .map_err(|e| CryptoError::Kdf(e.to_string()))?,
    );

    let mut key = Zeroizing::new([0u8; 32]);
    argon
        .hash_password_into(passphrase.as_bytes(), salt, key.as_mut())
        .map_err(|e| CryptoError::Kdf(e.to_string()))?;
    Ok(key)
}

fn random(bytes: &mut [u8]) -> Result<(), CryptoError> {
    getrandom::fill(bytes).map_err(|e| CryptoError::NoRandomness(e.to_string()))
}

/// The additional authenticated data for one chunk.
///
/// Header, index, last-chunk flag. All three, always -- see the crate docs for
/// what each one prevents.
fn aad(header_bytes: &[u8], index: u64, last: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(header_bytes.len() + 9);
    out.extend_from_slice(header_bytes);
    out.extend_from_slice(&index.to_be_bytes());
    out.push(u8::from(last));
    out
}

/// The nonce for one chunk: the file's random prefix, then the index.
fn nonce(prefix: &[u8], index: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(prefix.len() + 8);
    out.extend_from_slice(prefix);
    out.extend_from_slice(&index.to_be_bytes());
    out
}

fn seal_chunk(
    suite: Suite,
    key: &Key,
    nonce: &[u8],
    aad: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let payload = Payload {
        msg: plaintext,
        aad,
    };
    match suite {
        Suite::XChaCha20Poly1305 => XChaCha20Poly1305::new(key.as_ref().into())
            .encrypt(nonce.into(), payload)
            .map_err(|_| CryptoError::CannotOpen),
        Suite::Aes256Gcm => Aes256Gcm::new(key.as_ref().into())
            .encrypt(nonce.into(), payload)
            .map_err(|_| CryptoError::CannotOpen),
    }
}

fn open_chunk(
    suite: Suite,
    key: &Key,
    nonce: &[u8],
    aad: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let payload = Payload {
        msg: ciphertext,
        aad,
    };
    match suite {
        Suite::XChaCha20Poly1305 => XChaCha20Poly1305::new(key.as_ref().into())
            .decrypt(nonce.into(), payload)
            .map_err(|_| CryptoError::CannotOpen),
        Suite::Aes256Gcm => Aes256Gcm::new(key.as_ref().into())
            .decrypt(nonce.into(), payload)
            .map_err(|_| CryptoError::CannotOpen),
    }
}

/// Encrypt `plaintext` into a `.bpadx` document.
///
/// An empty passphrase is refused rather than accepted as a weak one: a
/// document that opens with no passphrase is not encrypted in any sense the
/// user would recognise, and offering it would be the lie ADR-0020 forbids
/// elsewhere.
pub fn seal(
    plaintext: &[u8],
    passphrase: &str,
    options: SealOptions,
) -> Result<Vec<u8>, CryptoError> {
    if passphrase.is_empty() {
        return Err(CryptoError::EmptyPassphrase);
    }
    options.kdf.validate()?;
    if !(header::MIN_CHUNK_SIZE..=MAX_CHUNK_SIZE).contains(&options.chunk_size) {
        return Err(CryptoError::UnreasonableChunkSize(options.chunk_size));
    }

    let mut salt = [0u8; SALT_LEN];
    random(&mut salt)?;
    let mut nonce_prefix = vec![0u8; options.suite.nonce_prefix_len()];
    random(&mut nonce_prefix)?;

    let header = Header {
        version: VERSION,
        suite: options.suite,
        kdf: options.kdf,
        salt,
        chunk_size: options.chunk_size,
        nonce_prefix,
    };
    let header_bytes = header.to_bytes();
    let key = derive(passphrase, &header.salt, header.kdf)?;

    let mut out = header_bytes.clone();
    let size = options.chunk_size as usize;

    // An empty document still gets one chunk. Without it there would be
    // nothing to authenticate, so an empty `.bpadx` would open under any
    // passphrase at all.
    let chunks: Vec<&[u8]> = if plaintext.is_empty() {
        vec![&[]]
    } else {
        plaintext.chunks(size).collect()
    };
    let last_index = chunks.len() - 1;

    for (index, chunk) in chunks.iter().enumerate() {
        let i = index as u64;
        let sealed = seal_chunk(
            options.suite,
            &key,
            &nonce(&header.nonce_prefix, i),
            &aad(&header_bytes, i, index == last_index),
            chunk,
        )?;
        let len = u32::try_from(sealed.len()).map_err(|_| CryptoError::Corrupt)?;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&sealed);
    }
    Ok(out)
}

/// Decrypt a `.bpadx` document.
///
/// The plaintext is returned in a `Zeroizing` wrapper: this is document
/// content that was encrypted at rest, and handing it back in a plain `Vec`
/// would leave it in freed memory the moment the caller drops it.
pub fn open(document: &[u8], passphrase: &str) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
    let (header, header_len) = Header::parse(document)?;
    let header_bytes = &document[..header_len];
    let key = derive(passphrase, &header.salt, header.kdf)?;

    // Wrapped and sized up front, both deliberately.
    //
    // `Zeroizing` from the start rather than only on the returned value:
    // growing a plain `Vec` reallocates, and each reallocation copies the
    // plaintext accumulated so far into a new buffer and frees the old one
    // *unwiped*. Wrapping only at the end would leave every earlier
    // allocation of a multi-chunk document lying in freed memory, which is
    // precisely the guarantee this function's documentation makes.
    //
    // Reserving the plaintext's maximum size means there are no reallocations
    // to leak through in the first place. It is bounded by the file's own
    // length, so a hostile header cannot use it to demand memory.
    let mut out: Zeroizing<Vec<u8>> = Zeroizing::new(Vec::with_capacity(
        document.len().saturating_sub(header_len),
    ));
    let mut at = header_len;
    let mut index: u64 = 0;

    // What a single chunk may decrypt to, from the header the chunks
    // themselves authenticate. Without this the declared chunk size is
    // validated, authenticated, and then never used for anything -- which a
    // reader would reasonably assume constrains the framing.
    let max_ciphertext = (header.chunk_size as usize).saturating_add(TAG_OVERHEAD);

    loop {
        if at == document.len() {
            // Ran out of chunks without one claiming to be the last: the file
            // was cut short. Distinct from a damaged chunk, and the user can
            // act on the difference.
            return Err(CryptoError::Truncated);
        }
        // `checked_add` rather than `+`, matching `header::take`. `len` is
        // attacker-controlled up to `u32::MAX`, and on a 32-bit target a bare
        // addition wraps -- panicking in debug, and in release yielding a
        // bogus slice that happens to be in range.
        let end = at.checked_add(4).ok_or(CryptoError::Truncated)?;
        let len_bytes: [u8; 4] = document
            .get(at..end)
            .ok_or(CryptoError::Truncated)?
            .try_into()
            .map_err(|_| CryptoError::Truncated)?;
        at = end;

        let len = u32::from_le_bytes(len_bytes) as usize;
        if len > max_ciphertext {
            // Longer than the header says a chunk can be. The header is
            // authenticated, so this is either damage or a forgery attempt,
            // and either way there is nothing to decrypt.
            return Err(CryptoError::Corrupt);
        }
        let end = at.checked_add(len).ok_or(CryptoError::Truncated)?;
        // A hostile length must not make this allocate before anything has
        // authenticated. The slice is bounded by the file itself.
        let ciphertext = document.get(at..end).ok_or(CryptoError::Truncated)?;
        at = end;

        let last = at == document.len();
        match open_chunk(
            header.suite,
            &key,
            &nonce(&header.nonce_prefix, index),
            &aad(header_bytes, index, last),
            ciphertext,
        ) {
            Ok(mut chunk) => {
                out.extend_from_slice(&chunk);
                chunk.zeroize();
            }
            Err(e) => {
                // `out` wipes itself on drop, but doing it here says so at the
                // point it matters rather than relying on the reader knowing.
                out.zeroize();
                return Err(e);
            }
        }

        if last {
            return Ok(out);
        }
        index += 1;
    }
}

/// A stable, filesystem-safe name derived from arbitrary bytes.
///
/// For naming recovery-journal files after the document they belong to,
/// without writing the path down (ADR-0022). SHA-256 truncated to 32 hex
/// characters -- ample against accidental collision, and short enough to stay
/// a sane filename.
///
/// **This is not a secret.** Anyone holding the recovery directory can test a
/// guessed path against it, so it hides *which* documents have unsaved work
/// only from someone who cannot guess their paths. That is the trade the
/// alternative -- writing the path in clear beside the ciphertext -- loses
/// outright.
///
/// Not used for key derivation anywhere. Keys come from Argon2id.
#[must_use]
pub fn stable_name(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    digest.iter().take(16).map(|b| format!("{b:02x}")).collect()
}

/// How many bytes [`is_bpadx`] needs to reach its answer.
///
/// Exposed so a caller can read a *header* rather than a file. [`is_bpadx`]
/// is cheap, and says so — but a caller that reaches for `std::fs::read` to
/// obtain its argument has made it the most expensive call in the program,
/// and one did: opening a document read the whole of it to look at six bytes,
/// and then `bp_files::load` read the whole of it again. On a 2 GB file that
/// is 4 GB of I/O before anything is on screen.
pub const MAGIC_LEN: usize = MAGIC.len();

/// Whether `bytes` begins with the `.bpadx` magic.
///
/// Cheap enough to call on every file open, so the shell can route a document
/// to the passphrase prompt instead of showing the user its ciphertext. Read
/// [`MAGIC_LEN`] bytes to call it; do not read the file.
///
/// A short slice is not encrypted rather than unknown: a file with fewer than
/// [`MAGIC_LEN`] bytes cannot carry the magic, so `false` is the whole truth
/// about it.
#[must_use]
pub fn is_bpadx(bytes: &[u8]) -> bool {
    bytes.starts_with(MAGIC)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deliberately cheap key derivation.
    ///
    /// The real cost is 19 MiB and ~50 ms; multiplied across this module that
    /// is minutes of gate time for no extra coverage, because the cost
    /// parameter is not what any of these tests are about. The one test that
    /// *is* about cost uses real values.
    fn cheap() -> KdfParams {
        KdfParams {
            memory_kib: 8,
            iterations: 1,
            lanes: 1,
        }
    }

    fn options(suite: Suite) -> SealOptions {
        SealOptions {
            suite,
            kdf: cheap(),
            chunk_size: header::MIN_CHUNK_SIZE,
        }
    }

    const PASS: &str = "correct horse battery staple";

    #[test]
    fn a_document_round_trips_through_both_suites() {
        for suite in [Suite::XChaCha20Poly1305, Suite::Aes256Gcm] {
            let text = b"the quick brown fox";
            let sealed = seal(text, PASS, options(suite)).unwrap();

            assert!(is_bpadx(&sealed), "{} lost the magic", suite.name());
            assert_eq!(
                open(&sealed, PASS).unwrap().as_slice(),
                text,
                "{} did not round trip",
                suite.name()
            );
        }
    }

    #[test]
    fn the_ciphertext_does_not_contain_the_plaintext() {
        // The most basic thing a reviewer should be able to check, and the
        // one that would catch a catastrophic wiring mistake instantly.
        let text = b"SECRETSECRETSECRET";
        let sealed = seal(text, PASS, options(Suite::default())).unwrap();

        assert!(
            !sealed.windows(text.len()).any(|w| w == text),
            "the plaintext is sitting in the output"
        );
    }

    #[test]
    fn sealing_the_same_document_twice_gives_different_bytes() {
        // A fresh salt and nonce prefix each time. Identical output would mean
        // one of them was fixed, which makes the key reusable across documents
        // and the nonces repeat with it.
        let a = seal(b"same", PASS, options(Suite::default())).unwrap();
        let b = seal(b"same", PASS, options(Suite::default())).unwrap();

        assert_ne!(a, b, "output is deterministic; a salt or nonce is fixed");
    }

    #[test]
    fn a_wrong_passphrase_does_not_open_the_document() {
        let sealed = seal(b"secret", PASS, options(Suite::default())).unwrap();

        assert_eq!(
            open(&sealed, "wrong passphrase"),
            Err(CryptoError::CannotOpen)
        );
    }

    #[test]
    fn an_empty_passphrase_is_refused_rather_than_treated_as_a_weak_one() {
        // A document that opens with no passphrase is not encrypted in any
        // sense the user would recognise.
        assert_eq!(
            seal(b"secret", "", options(Suite::default())),
            Err(CryptoError::EmptyPassphrase)
        );
    }

    #[test]
    fn a_flipped_ciphertext_bit_is_caught() {
        for suite in [Suite::XChaCha20Poly1305, Suite::Aes256Gcm] {
            let mut sealed = seal(b"secret document", PASS, options(suite)).unwrap();
            let last = sealed.len() - 1;
            sealed[last] ^= 0x01;

            assert_eq!(
                open(&sealed, PASS),
                Err(CryptoError::CannotOpen),
                "{} accepted altered ciphertext",
                suite.name()
            );
        }
    }

    #[test]
    fn editing_the_suite_in_the_header_is_caught() {
        // The header is authenticated as additional data on every chunk, so an
        // edit to it cannot silently change how the document is read. A header
        // that were only *read* could be downgraded.
        let mut sealed = seal(b"secret", PASS, options(Suite::XChaCha20Poly1305)).unwrap();
        // Byte 8: magic (6) + version (2), then the suite tag.
        assert_eq!(sealed[8], Suite::XChaCha20Poly1305.tag());
        sealed[8] = Suite::Aes256Gcm.tag();

        // Rejected either as a structural mismatch (the nonce width no longer
        // matches the declared suite) or as a failed authentication. Both are
        // refusals; what matters is that it does not open.
        assert!(open(&sealed, PASS).is_err(), "a downgraded header opened");
    }

    #[test]
    fn editing_the_kdf_cost_in_the_header_is_caught() {
        // Lowering the cost would let an attacker who copied the file brute
        // force it more cheaply, if the reader honoured the edit.
        let mut sealed = seal(b"secret", PASS, options(Suite::default())).unwrap();
        // magic 6 + version 2 + suite 1 + kdf id 1 = 10, then memory_kib.
        // The document was sealed at `cheap()`'s 8 KiB, so the edit has to be
        // to a *different* legal value -- writing 8 back would test nothing,
        // which is what the first version of this test did.
        assert_eq!(u32::from_le_bytes(sealed[10..14].try_into().unwrap()), 8);
        sealed[10..14].copy_from_slice(&64u32.to_le_bytes());

        assert!(open(&sealed, PASS).is_err(), "an edited cost was honoured");
    }

    /// Split a sealed document into its header and its framed chunks.
    fn split(sealed: &[u8]) -> (Vec<u8>, Vec<Vec<u8>>) {
        let (_, header_len) = Header::parse(sealed).unwrap();
        let mut chunks = Vec::new();
        let mut at = header_len;
        while at < sealed.len() {
            let len = u32::from_le_bytes(sealed[at..at + 4].try_into().unwrap()) as usize;
            chunks.push(sealed[at..at + 4 + len].to_vec());
            at += 4 + len;
        }
        (sealed[..header_len].to_vec(), chunks)
    }

    #[test]
    fn reordering_chunks_is_caught() {
        // Without the chunk index in the additional data, every reordered
        // chunk would still authenticate and the document would open with its
        // contents rearranged -- corruption that looks like success.
        let text: Vec<u8> = (0..4096u32).flat_map(u32::to_le_bytes).collect();
        let sealed = seal(&text, PASS, options(Suite::default())).unwrap();
        let (header, chunks) = split(&sealed);
        assert!(
            chunks.len() >= 3,
            "need several chunks; got {}",
            chunks.len()
        );

        let mut swapped = header;
        swapped.extend_from_slice(&chunks[1]);
        swapped.extend_from_slice(&chunks[0]);
        for chunk in &chunks[2..] {
            swapped.extend_from_slice(chunk);
        }

        assert_eq!(open(&swapped, PASS), Err(CryptoError::CannotOpen));
    }

    #[test]
    fn duplicating_a_chunk_is_caught() {
        let text: Vec<u8> = (0..4096u32).flat_map(u32::to_le_bytes).collect();
        let sealed = seal(&text, PASS, options(Suite::default())).unwrap();
        let (header, chunks) = split(&sealed);

        let mut doubled = header;
        doubled.extend_from_slice(&chunks[0]);
        for chunk in &chunks {
            doubled.extend_from_slice(chunk);
        }

        assert!(open(&doubled, PASS).is_err());
    }

    #[test]
    fn truncating_the_document_is_caught() {
        // The attack the last-chunk flag exists for. Without it the remaining
        // chunks all authenticate and the document opens short -- silent data
        // loss presented as a successful decrypt.
        let text: Vec<u8> = (0..4096u32).flat_map(u32::to_le_bytes).collect();
        let sealed = seal(&text, PASS, options(Suite::default())).unwrap();
        let (header, chunks) = split(&sealed);

        let mut cut = header;
        for chunk in &chunks[..chunks.len() - 1] {
            cut.extend_from_slice(chunk);
        }

        assert_eq!(
            open(&cut, PASS),
            Err(CryptoError::CannotOpen),
            "a truncated document must not open with its remaining chunks"
        );
    }

    #[test]
    fn a_document_cut_mid_chunk_is_reported_as_incomplete() {
        // Distinct from damage: the user can act on "incomplete" by finding a
        // better copy.
        let sealed = seal(b"secret document", PASS, options(Suite::default())).unwrap();
        let cut = &sealed[..sealed.len() - 5];

        assert_eq!(open(cut, PASS), Err(CryptoError::Truncated));
    }

    #[test]
    fn a_file_that_is_not_bpadx_is_named_rather_than_guessed_at() {
        assert!(!is_bpadx(b"just some text"));
        assert_eq!(open(b"just some text", PASS), Err(CryptoError::NotBpadx));
    }

    #[test]
    fn a_newer_format_version_is_refused_by_name() {
        // "Could not open" tells the user nothing they can act on; "written by
        // a newer BachelorPad+" tells them to update.
        let mut sealed = seal(b"secret", PASS, options(Suite::default())).unwrap();
        sealed[6] = 99;
        sealed[7] = 0;

        assert_eq!(
            open(&sealed, PASS),
            Err(CryptoError::UnsupportedVersion {
                found: 99,
                supported: VERSION,
            })
        );
    }

    #[test]
    fn an_unknown_suite_is_refused_by_name() {
        let mut sealed = seal(b"secret", PASS, options(Suite::default())).unwrap();
        sealed[8] = 77;

        assert_eq!(open(&sealed, PASS), Err(CryptoError::UnsupportedSuite(77)));
    }

    #[test]
    fn an_absurd_work_factor_is_refused_rather_than_attempted() {
        // A hostile header asking for 64 GiB is a denial-of-service, not a
        // strong document. Refusing is the difference between a message and a
        // machine that stops responding.
        let mut sealed = seal(b"secret", PASS, options(Suite::default())).unwrap();
        let absurd = (64u32 * 1024 * 1024).to_le_bytes();
        sealed[10..14].copy_from_slice(&absurd);

        assert!(matches!(
            open(&sealed, PASS),
            Err(CryptoError::UnreasonableCost { .. })
        ));
    }

    #[test]
    fn a_memory_cost_that_was_valid_under_the_old_ceiling_is_now_refused() {
        // ADR-0035 lowered the memory ceiling from 1 GiB to 256 MiB. 512 MiB
        // was legal under the old bound and is refused under the new one --
        // catching an accidental reversion of this change.
        let mut sealed = seal(b"secret", PASS, options(Suite::default())).unwrap();
        let tightened = (512u32 * 1024).to_le_bytes(); // 512 MiB, above new 256 MiB ceiling
        sealed[10..14].copy_from_slice(&tightened);

        assert!(matches!(
            open(&sealed, PASS),
            Err(CryptoError::UnreasonableCost { .. })
        ));
    }

    #[test]
    fn an_absurd_chunk_size_is_refused() {
        let mut sealed = seal(b"secret", PASS, options(Suite::default())).unwrap();
        // magic 6 + version 2 + suite 1 + kdf 1 + cost 12 + salt 16 = 38.
        let absurd = u32::MAX.to_le_bytes();
        sealed[38..42].copy_from_slice(&absurd);

        assert!(matches!(
            open(&sealed, PASS),
            Err(CryptoError::UnreasonableChunkSize(_))
        ));
    }

    #[test]
    fn an_empty_document_is_still_authenticated() {
        // It gets one chunk holding nothing. Without it there would be nothing
        // to authenticate, and an empty `.bpadx` would open under any
        // passphrase at all.
        let sealed = seal(b"", PASS, options(Suite::default())).unwrap();

        assert!(open(&sealed, PASS).unwrap().is_empty());
        assert_eq!(open(&sealed, "wrong"), Err(CryptoError::CannotOpen));
    }

    #[test]
    fn a_document_spanning_many_chunks_round_trips_exactly() {
        let text: Vec<u8> = (0..50_000u32).map(|i| (i % 251) as u8).collect();
        let sealed = seal(&text, PASS, options(Suite::default())).unwrap();

        assert_eq!(open(&sealed, PASS).unwrap().as_slice(), text.as_slice());
    }

    #[test]
    fn a_document_around_one_chunk_long_round_trips() {
        // The boundary where "is this the last chunk" is decided.
        let size = header::MIN_CHUNK_SIZE as usize;
        for len in [size - 1, size, size + 1] {
            let text = vec![b'x'; len];
            let sealed = seal(&text, PASS, options(Suite::default())).unwrap();
            assert_eq!(
                open(&sealed, PASS).unwrap().as_slice(),
                text.as_slice(),
                "failed at {len} bytes"
            );
        }
    }

    #[test]
    fn the_default_work_factor_is_the_owasp_baseline() {
        // The one test that uses the real cost. ADR-0021 pins these numbers,
        // and lowering them silently would be a security change disguised as a
        // performance one.
        let kdf = KdfParams::default();
        assert_eq!(kdf.memory_kib, 19 * 1024);
        assert_eq!(kdf.iterations, 2);
        assert_eq!(kdf.lanes, 1);

        // And it genuinely works end to end at that cost, not only at the
        // cheap one the rest of this module uses.
        let sealed = seal(b"real cost", PASS, SealOptions::default()).unwrap();
        assert_eq!(open(&sealed, PASS).unwrap().as_slice(), b"real cost");
    }

    #[test]
    fn the_stored_parameters_are_what_a_reader_uses() {
        // The reason cost lives in the envelope: a document written at one
        // cost must open on a build whose default is another.
        let sealed = seal(
            b"written cheaply",
            PASS,
            SealOptions {
                kdf: cheap(),
                ..SealOptions::default()
            },
        )
        .unwrap();

        let (header, _) = Header::parse(&sealed).unwrap();
        assert_eq!(header.kdf, cheap());
        assert_eq!(open(&sealed, PASS).unwrap().as_slice(), b"written cheaply");
    }

    #[test]
    fn every_byte_of_a_document_is_load_bearing() {
        // The property behind all the targeted tests above: there is no byte
        // that can be changed without the document refusing to open. If one
        // existed it would be a byte outside the authenticated data, which is
        // exactly where a format grows a hole.
        let sealed = seal(b"hello", PASS, options(Suite::default())).unwrap();

        for index in 0..sealed.len() {
            let mut altered = sealed.clone();
            altered[index] ^= 0xff;
            assert!(
                open(&altered, PASS).is_err(),
                "flipping byte {index} of {} still opened the document",
                sealed.len()
            );
        }
    }
}
