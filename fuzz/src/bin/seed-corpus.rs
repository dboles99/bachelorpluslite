//! Write the checked-in corpus.
//!
//! `cargo run --bin seed-corpus`
//!
//! The corpus is committed, not generated at test time, because a corpus is
//! evidence: an input that once crashed something has to survive in the
//! repository, or the regression it guards is only guarded until somebody
//! rewrites the generator. This program exists so the generated entries stay
//! *auditable* -- every one below says in a comment where the input came from
//! and what it is meant to provoke -- and so they can be rebuilt if a format
//! changes.
//!
//! It is not idempotent for `corpus/envelope/`: `bp_crypto::seal` draws a
//! fresh random salt and nonce prefix per document, so re-seeding rewrites
//! those files with different bytes meaning the same thing. Only re-run it if
//! the envelope format actually changed.

use std::fs;
use std::path::Path;

use bp_crypto::{KdfParams, SealOptions, Suite};

/// Argon2id cost for the corpus documents: the floor the format allows.
///
/// The default (19 MiB, t=2) is right for a real document and wrong for a
/// corpus opened dozens of times per test run -- and the target here is the
/// *parser*, not the KDF. 8 KiB is `KdfParams::validate`'s minimum.
const CHEAP: KdfParams = KdfParams {
    memory_kib: 8,
    iterations: 1,
    lanes: 1,
};

/// The passphrase every sealed corpus document is under.
///
/// Written down on purpose: these documents contain nothing, and a corpus
/// nobody can open is harder to reason about than one anybody can.
pub const PASSPHRASE: &str = "corpus";

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus");
    data(&root.join("data"));
    files(&root.join("files"));
    envelope(&root.join("envelope"));
    println!("corpus written under {}", root.display());
}

fn put(dir: &Path, name: &str, bytes: impl AsRef<[u8]>) {
    fs::create_dir_all(dir).expect("create corpus directory");
    let path = dir.join(name);
    fs::write(&path, bytes.as_ref()).expect("write corpus entry");
    println!("  {} ({} bytes)", path.display(), bytes.as_ref().len());
}

// --- JSON, JSONL, TOML, CSV --------------------------------------------

fn data(dir: &Path) {
    // serde_json enforces 128 levels of its own; ADR-0023 picked the YAML cap
    // to match it so both formats refuse at the same depth. One past it here.
    put(
        dir,
        "nesting-over-cap.json",
        format!("{}{}", "[".repeat(129), "]".repeat(129)),
    );
    // Deep enough that an unguarded recursive descent cannot survive.
    put(
        dir,
        "nesting-100000.json",
        format!("{}{}", "[".repeat(100_000), "]".repeat(100_000)),
    );
    put(dir, "truncated.json", "{\"a\": [1, 2, {\"b\": ");
    put(dir, "duplicate-keys.json", "{\"a\": 1, \"a\": 2, \"a\": 3}");

    // A \uD800 escape with no low surrogate after it: valid JSON syntax that
    // cannot become a Rust `String`.
    put(dir, "lone-surrogate-escape.json", "{\"a\": \"\\uD800\"}");
    put(
        dir,
        "huge-exponent.json",
        "[1e309, -1e309, 1e-400, 12345678901234567890123]",
    );
    put(dir, "bare-nan.json", "[NaN, Infinity]");
    put(dir, "empty-keys.json", "{\"\":{\"\":{\"\":{\"\":1}}}}");
    put(dir, "bom-prefixed.json", "\u{feff}{\"a\": 1}");

    put(
        dir,
        "mixed.jsonl",
        "{\"a\":1}\nnot json\n{\"b\":2}\n\n[1,2]\n",
    );
    put(
        dir,
        "blank-and-crlf.jsonl",
        "{\"a\":1}\r\n\r\n{\"b\":2}\r\n",
    );
    put(dir, "single-scalar.jsonl", "42\n\"text\"\nnull\n");

    put(dir, "duplicate-key.toml", "a = 1\na = 2\n");
    put(dir, "deep-tables.toml", deep_toml(200));
    put(
        dir,
        "odd-values.toml",
        "a = 1979-05-27T07:32:00-08:00\nb = inf\nc = nan\nd = 0x7FFFFFFFFFFFFFFF\ne = \"\"\"multi\nline\"\"\"\n",
    );
    put(dir, "unclosed-table.toml", "[a\nb = 1\n");

    put(dir, "ragged.csv", "a,b,c\n1,2,3\n4,5\n6,7,8,9\n");
    put(dir, "quoted-comma.csv", "name,city\n\"Doe, Jane\",Leeds\n");
    put(
        dir,
        "bare-quote.csv",
        "a,b\nun\"quoted,2\n\"unterminated,3\n",
    );
    put(dir, "crlf-no-final-newline.csv", "a,b\r\n1,2\r\n3,4");
    put(dir, "embedded-newline.csv", "a,b\n\"line\none\",2\n");
    put(dir, "one-cell.csv", "x");
    put(dir, "all-delimiters.csv", "a,b;c\td|e\n1,2;3\t4|5\n");

    // Every cell empty: the shape a column-type report has to classify with
    // no evidence at all.
    put(dir, "empty-columns.csv", "a,b,c\n,,\n,,\n");
}

fn deep_toml(levels: usize) -> String {
    let mut out = String::new();
    let mut path = String::from("a");
    for _ in 0..levels {
        out.push_str(&format!("[{path}]\nv = 1\n"));
        path.push_str(".a");
    }
    out
}

// --- files: encodings on disk ------------------------------------------

fn files(dir: &Path) {
    const LE: [u8; 2] = [0xFF, 0xFE];
    const BE: [u8; 2] = [0xFE, 0xFF];

    // An odd number of body bytes: the last code unit is cut in half.
    // `LoadError::TruncatedUtf16` names it, and naming it is the point --
    // dropping the stray byte would invite the user to save the truncation
    // back over whatever survived.
    let mut odd = LE.to_vec();
    odd.extend_from_slice(&[0x41, 0x00, 0x42, 0x00, 0x43]);
    put(dir, "utf16le-odd-length.bin", odd);

    // A high surrogate with no low one after it. There is no character to
    // produce but U+FFFD, and substituting is what `bp_files::utf16` exists
    // to refuse.
    let mut lone_le = LE.to_vec();
    lone_le.extend_from_slice(&[0x41, 0x00, 0x00, 0xD8, 0x42, 0x00]);
    put(dir, "utf16le-unpaired-high-surrogate.bin", lone_le);

    // A low surrogate first, with nothing to pair backwards to.
    let mut low_le = LE.to_vec();
    low_le.extend_from_slice(&[0x00, 0xDC, 0x41, 0x00]);
    put(dir, "utf16le-unpaired-low-surrogate.bin", low_le);

    // The same fault in the other byte order, which is a different branch.
    let mut lone_be = BE.to_vec();
    lone_be.extend_from_slice(&[0x00, 0x41, 0xD8, 0x00, 0x00, 0x42]);
    put(dir, "utf16be-unpaired-high-surrogate.bin", lone_be);

    // A high surrogate as the very last unit: the truncation case, which
    // needs a different check from a surrogate followed by a non-surrogate.
    let mut trailing = LE.to_vec();
    trailing.extend_from_slice(&[0x41, 0x00, 0x00, 0xD8]);
    put(dir, "utf16le-surrogate-at-eof.bin", trailing);

    // Nothing but a byte-order mark. A valid, empty UTF-16 document -- not an
    // error -- and it must round-trip back to those two bytes.
    put(dir, "utf16le-lone-bom.bin", LE);
    put(dir, "utf16be-lone-bom.bin", BE);

    // Well-formed, so the corpus is not all failures: a surrogate pair for
    // U+1F600 followed by a CRLF.
    let mut good = LE.to_vec();
    good.extend_from_slice(&[0x3D, 0xD8, 0x00, 0xDE, 0x0D, 0x00, 0x0A, 0x00]);
    put(dir, "utf16le-astral-and-crlf.bin", good);

    put(dir, "utf8-bom.txt", b"\xEF\xBB\xBFhello\n");
    put(dir, "utf8-bom-only.txt", b"\xEF\xBB\xBF");

    // A byte-order mark followed by a genuine U+FEFF character. Found by
    // `files::a_leading_zwnbsp_saved_without_a_bom_is_read_back_as_a_bom`:
    // the two are the same three bytes, so a document whose first character
    // is U+FEFF, saved as UTF-8 without a mark, loads back with that
    // character gone and its encoding relabelled. The bytes on disk are
    // never wrong; the text silently loses a character. See that test for
    // why it is recorded rather than fixed.
    put(
        dir,
        "utf8-bom-then-zwnbsp.bin",
        b"\xEF\xBB\xBF\xEF\xBB\xBFhello\n",
    );

    // Continuation bytes with no lead byte, and a truncated three-byte
    // sequence at the end.
    put(dir, "invalid-utf8.bin", b"ok \x80\x81 then \xE2\x82");

    // An overlong encoding of '/': valid-looking, and rejected by Rust.
    put(dir, "overlong-utf8.bin", b"a\xC0\xAFb");
    put(dir, "nul-and-controls.bin", b"a\x00b\x01\x02\x7Fc\n");
    put(dir, "mixed-line-endings.txt", b"a\r\nb\nc\rd\n");
    put(dir, "no-final-newline.txt", b"one line");
    put(dir, "empty.txt", Vec::new());
}

// --- .bpadx envelopes ---------------------------------------------------

/// Where the header stops and the first chunk's length prefix begins.
///
/// Recomputed from the format rather than imported: `Header::parse` is
/// crate-private, and a hard-coded 48 would silently point at the wrong byte
/// the day a field is added. Magic 6 + version 2 + suite 1 + kdf id 1 + cost
/// 12 + salt 16 + chunk size 4 + prefix length 1, then the prefix itself.
const PREFIX_LEN_AT: usize = 42;

/// Offset of the stored Argon2id memory cost. Magic 6 + version 2 + suite 1 +
/// kdf id 1.
const MEMORY_KIB_AT: usize = 10;

fn header_end(sealed: &[u8]) -> usize {
    PREFIX_LEN_AT + 1 + usize::from(sealed[PREFIX_LEN_AT])
}

fn envelope(dir: &Path) {
    let options = SealOptions {
        suite: Suite::XChaCha20Poly1305,
        kdf: CHEAP,
        chunk_size: 1024,
    };
    let sealed =
        bp_crypto::seal(b"a small document\n", PASSPHRASE, options).expect("seal a document");
    put(dir, "sealed.bpadx", &sealed);

    // The same document under the other suite: a different nonce width, so a
    // different branch of the header parser.
    let aes = bp_crypto::seal(
        b"a small document\n",
        PASSPHRASE,
        SealOptions {
            suite: Suite::Aes256Gcm,
            ..options
        },
    )
    .expect("seal under AES");
    put(dir, "sealed-aes.bpadx", &aes);

    // Multi-chunk, so the chunk loop runs more than once and the last-chunk
    // flag in the additional data is exercised.
    let big = bp_crypto::seal(&vec![b'x'; 4096], PASSPHRASE, options).expect("seal a big one");
    put(dir, "sealed-multichunk.bpadx", &big);

    // Truncated mid-ciphertext: the shape a partial copy or an interrupted
    // sync leaves behind.
    put(dir, "truncated-body.bpadx", &sealed[..sealed.len() - 5]);
    // Truncated inside the header, before the nonce prefix.
    put(dir, "truncated-header.bpadx", &sealed[..20]);
    put(dir, "magic-only.bpadx", b"BPADX\0");
    put(dir, "empty.bpadx", Vec::new());

    // One flipped byte in the ciphertext. Must be `CannotOpen` -- the
    // authentication tag catches it -- and must never be a panic.
    let mut flipped = sealed.clone();
    let last = flipped.len() - 3;
    flipped[last] ^= 0x01;
    put(dir, "flipped-ciphertext-byte.bpadx", &flipped);

    // A flipped byte in the *header*. ADR-0021 authenticates the header as
    // additional data on every chunk, so this must fail to open rather than
    // quietly change how the document is read.
    let mut flipped_header = sealed.clone();
    flipped_header[30] ^= 0x01;
    put(dir, "flipped-header-byte.bpadx", &flipped_header);

    let at = header_end(&sealed);

    // The declared chunk length raised past what follows: the reader is told
    // to expect more bytes than exist.
    let mut long_chunk = sealed.clone();
    long_chunk[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    put(dir, "chunk-length-overstated.bpadx", &long_chunk);

    // A declared chunk length of zero: too small to hold an authentication
    // tag, let alone a chunk.
    let mut zero_chunk = sealed.clone();
    zero_chunk[at..at + 4].copy_from_slice(&0u32.to_le_bytes());
    put(dir, "chunk-length-zero.bpadx", &zero_chunk);

    // The stored Argon2id cost raised to 64 MiB. Inside the format's bounds,
    // so it is honoured -- and honoured *before* anything is authenticated,
    // because a key has to exist before a tag can be checked. See
    // `fuzz/README.md`: this is the entry behind the pre-authentication cost
    // note.
    let mut costly = sealed.clone();
    costly[MEMORY_KIB_AT..MEMORY_KIB_AT + 4].copy_from_slice(&(64u32 * 1024).to_le_bytes());
    put(dir, "kdf-cost-raised.bpadx", &costly);

    // Past the format's ceiling: must be refused by name, not attempted.
    let mut absurd = sealed.clone();
    absurd[MEMORY_KIB_AT..MEMORY_KIB_AT + 4].copy_from_slice(&(2u32 * 1024 * 1024).to_le_bytes());
    put(dir, "kdf-cost-absurd.bpadx", &absurd);

    // A version this build does not know, and a suite tag it does not know.
    let mut future = sealed.clone();
    future[6..8].copy_from_slice(&999u16.to_le_bytes());
    put(dir, "future-version.bpadx", &future);

    let mut suite = sealed.clone();
    suite[8] = 200;
    put(dir, "unknown-suite.bpadx", &suite);

    // Plain text with no magic at all: the everyday "this is not a .bpadx"
    // case, which must be a named refusal rather than a guess.
    put(dir, "not-an-envelope.bpadx", b"just some text\n");
}
