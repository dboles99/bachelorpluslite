# The `.bpadx` envelope, prepared for outside review

> **D4 is moot, and this document is history rather than a request.**
> [ADR-0064](../decisions/ADR-0064.md) removed encryption on 2026-08-30, and
> with it `bp-crypto` -- the crate this brief asks a reviewer to read. There is
> no envelope to review, no `.bpadx` this build can open, and **nothing to send
> anybody.**
>
> It is kept, unedited below this line, for one reason: the three live
> questions it raises are the design questions any future envelope would have
> to answer, and they were expensive to find. If encryption ever returns, it
> starts here rather than from nothing -- and it starts by answering them,
> because they were never answered, only asked.
>
> Everything below describes code that existed at commit `f238b67` and does
> not exist now. Read it in the past tense.

**This document existed to make D4 cheap.** ADR-0011 forbids implementing
cryptography and permits composing it; ADR-0021 is the composition. Every
primitive here is vetted RustCrypto, and the *arrangement* of them is this
project's own design — which is exactly the part an author cannot check by
re-reading their own work. D4 was answered *yes, before phase 15 trusts real
documents to it* five sessions ago and has sat unstarted since.

A session cannot discharge D4: a reviewer inside it is not an outside one.
What it can do is remove every hour a reviewer would otherwise spend
reconstructing the format from source. That is what follows — the composition
written out, the pitfalls audited with each answer pointing at the test that
holds it, and the questions stated explicitly so a reviewer is not left
guessing what is being asked.

**Read [ADR-0021](../decisions/ADR-0021.md) first if you want the reasoning.
This is the artefact.**

---

## 1. What is in scope, and what is not

**In scope.** The envelope: how a key is derived, how a document is framed,
what is authenticated, what a reader does with a header it has not yet
authenticated. Roughly 1,080 lines across two files:

| File | Lines | Owns |
| --- | --- | --- |
| [crates/bp-crypto/src/lib.rs](../../crates/bp-crypto/src/lib.rs) | 870 | `seal`, `open`, the AEAD suites, the AAD and nonce construction |
| [crates/bp-crypto/src/header.rs](../../crates/bp-crypto/src/header.rs) | 213 | The wire header and every bound placed on it |

Roughly half of `lib.rs` is its own tests, which are named throughout §5.

**Not in scope, and please do not spend time on them.**

- **The primitives.** `argon2`, `chacha20poly1305`, `aes-gcm`, `ed25519-dalek`
  and `getrandom` are used as published. If one of them is wrong, this project
  is wrong along with everyone else using them, and that is not a finding this
  review can act on.
- **Policy.** Whether a given document *should* be encrypted is `bp-security`'s
  question (ADR-0020). `bp-crypto` performs cryptography and decides nothing;
  neither crate depends on the other.
- **Signatures.** `sign.rs` is Ed25519 in raw RFC 8032 encodings, deliberately
  *not* wrapped in this envelope, and reviewed on its own terms if at all. It
  touches the envelope at exactly one point — a signing key's seed is sealed
  by it — and that point is §4 and question 3.

---

## 2. The composition

### 2.1 The wire format

Little-endian throughout except the two counters, which are big-endian. All
offsets are from the start of the file.

```text
off  size  field
  0     6  magic, "BPADX\0"          -- ends in NUL so a text editor stops here
  6     2  version, u16              -- 1; a reader refuses anything else by name
  8     1  suite tag                 -- 1 = XChaCha20-Poly1305, 2 = AES-256-GCM
  9     1  KDF id                    -- 1 = Argon2id, version 0x13
 10     4  Argon2id memory cost, KiB
 14     4  Argon2id iterations
 18     4  Argon2id lanes
 22    16  salt
 38     4  chunk size, plaintext bytes per chunk
 42     1  nonce prefix length, N
 43     N  nonce prefix             -- N = 16 (XChaCha) or 4 (GCM); see 2.3

then, repeating to end of file:
        4  sealed chunk length, u32  -- ciphertext plus the 16-byte tag
        L  ciphertext || tag
```

The header is therefore **59 bytes** under XChaCha20-Poly1305 and **47** under
AES-256-GCM. It is fixed-length for a given suite; there are no optional
fields and no extension area.

### 2.2 The key

```text
key = Argon2id(password = passphrase as UTF-8,
               salt     = the 16 header bytes at offset 22,
               m, t, p  = the three header fields at offsets 10, 14, 18,
               outlen   = 32)
```

The 32 bytes are used **directly** as the AEAD key. There is no HKDF step, no
label, no pepper, and no application-specific string mixed in. The default
cost is the OWASP baseline: 19 MiB, t = 2, p = 1.

The cost lives in the file rather than the code so that raising it later
strengthens new documents without orphaning old ones. That is the whole reason
the field exists, and it is also the reason a reader has to treat it as hostile
input — see §5, P7.

### 2.3 The nonce

```text
nonce(i) = nonce_prefix || i as u64, big-endian
```

The prefix is fresh OS randomness per `seal` call. Its width is whatever the
suite has left after the 8-byte counter: **16 random bytes** for
XChaCha20-Poly1305's 192-bit nonce, **4** for AES-256-GCM's 96-bit one. A
reader refuses a header whose declared prefix length is not exactly the
suite's.

Within one file the counter cannot repeat, because it is the chunk index.
Across files, **it is the key that differs, not the nonce** — every `seal`
draws a fresh 16-byte salt, so two documents under the same passphrase have
different Argon2id outputs. This matters and §5, P2 states it plainly: the
whole of the cross-file `(key, nonce)` guarantee rests on the salt.

### 2.4 The chunks, and what is authenticated

The plaintext is split into fixed-size segments — 64 KiB by default, and the
header's declared size is bounded to 1 KiB … 16 MiB. Each is sealed on its
own. An **empty document still gets one chunk**, sealed over zero bytes;
without it there would be nothing to authenticate and an empty `.bpadx` would
open under any passphrase at all.

Each chunk's additional authenticated data is three things, always:

```text
aad(i) = header_bytes || i as u64, big-endian || (i == last) as u8
```

Each of the three prevents a specific attack, and dropping any one leaves a
format that still decrypts perfectly while being silently wrong:

| Component | What its absence would allow |
| --- | --- |
| the whole header | Editing the suite or the KDF cost to downgrade the document |
| the chunk index | Reordering or duplicating chunks, each still authenticating |
| the last-chunk flag | Truncating the file, the remainder still authenticating — data loss presented as a successful decrypt |

One detail worth a reviewer's eye because it is easy to get wrong and is
right here: on the **write** side `header_bytes` is the serialisation; on the
**read** side it is `document[..header_len]`, the raw bytes as they were found
on disk. Those are identical by construction — the parser consumes a
fixed-length header — but taking the raw bytes on read means there is no
canonicalisation gap for a re-serialisation to open up.

### 2.5 Reading, and the bounds a reader applies before it trusts anything

`open` parses, derives, then walks chunks. Everything it checks before a tag
has ever been verified:

| Check | Bound |
| --- | --- |
| magic | exactly `BPADX\0`, else `NotBpadx` |
| version | exactly 1, else `UnsupportedVersion { found, supported }` |
| suite tag | 1 or 2, else `UnsupportedSuite(tag)` |
| KDF id | exactly 1, else `UnsupportedKdf(id)` |
| Argon2id memory | 8 KiB … 256 MiB, else `UnreasonableCost` |
| Argon2id iterations | 1 … 16, else `UnreasonableCost` |
| Argon2id lanes | 1 … 64, else `UnreasonableCost` |
| chunk size | 1 KiB … 16 MiB, else `UnreasonableChunkSize` |
| nonce prefix length | exactly the suite's, else `Corrupt` |
| every length read | `checked_add`, then a bounds-checked slice, else `Truncated` |
| each sealed chunk length | ≤ chunk size + 16, else `Corrupt` |
| end of file | must fall exactly at the end of a chunk sealed as last |

The KDF ceiling was lowered from 1 GiB and 64 iterations by
[ADR-0035](../decisions/ADR-0035.md); it is still 13× and 8× the OWASP
baseline this build writes.

### 2.6 What the format deliberately does not hide

Stated so nobody assumes otherwise:

- **The plaintext's length, exactly.** There is no padding. Every chunk but
  the last is full, and each carries its own length, so the plaintext size is
  recoverable to the byte from the file size.
- **Everything in the header.** Suite, cost, salt, chunk size and nonce prefix
  are all public by construction — a reader needs them before it has a key.
- **That the file is a BachelorPad+ encrypted document**, from six bytes.
- **Nothing about the passphrase.** There is no verifier blob and no key check
  value. The first chunk's tag is the only test, which is why a wrong
  passphrase and a damaged file are the same error.

---

## 3. What one failure deliberately cannot say

`CryptoError::CannotOpen` is *"wrong passphrase, or this document has been
altered or damaged"*, and it says both because no honest implementation can
tell them apart. Distinguishing them would mean trusting something outside the
authenticated data — which is another way of saying it would mean adding
something an attacker can forge.

Every other failure names itself, because the user can act on the difference
between "this is not a BachelorPad+ document" and "this was written by a newer
version".

---

## 4. Where the envelope is used, which is part of the review

A composition is only as sound as its uses. There are four, and the third is
the one worth looking hardest at.

| Caller | What it seals | Under what key |
| --- | --- | --- |
| [bp-ui/src/state/encryption.rs](../../crates/bp-ui/src/state/encryption.rs) | The document itself | The passphrase the user typed |
| [bp-history/src/lib.rs](../../crates/bp-history/src/lib.rs) | A recovery checkpoint, as JSON, path and name inside the ciphertext | The document's own passphrase (ADR-0022) |
| [bp-audit/src/passphrase.rs](../../crates/bp-audit/src/passphrase.rs) | **One whole envelope per audit line**, hex-encoded onto one line of an append-only log | The document's own passphrase (ADR-0024) |
| [bp-integrity/src/keys.rs](../../crates/bp-integrity/src/keys.rs) | An Ed25519 signing key's 32-byte seed | A passphrase for the key (ADR-0031) |

**`bp-audit` is the one that stresses the design.** An audit log is appended
to, many times, and re-sealing the whole file per append would be O(n²) bytes
and one Argon2id derivation per event over a growing file — and it would
destroy the append-only property the crate is built on. So each record gets
its own complete envelope.

The argument that makes this safe is in that module's header and is repeated
here because a reviewer should attack it: **a fresh salt per line means a
fresh key per line**, so a `(key, nonce)` pair cannot repeat across lines
however the nonces fall, and within a line the chunk counter never leaves
zero because a record is one chunk. The price is one Argon2id derivation per
appended line. The alternative — a session key with a durable nonce counter —
buys a cheap derivation by taking on a counter that must survive process
restarts, in a log that deliberately keeps no state.

**`bp-integrity` is the one where the envelope's lack of a content type shows.**
Nothing in the format says what is inside it, so pointing
`read_sealed_signing_key` at an encrypted *document* opens it successfully and
hands back prose. The guard is a length check: what comes out must be 32
bytes, or it is `NotAKeyFile`. See question 3.

---

## 5. Self-audit against the standard composition pitfalls

Each row is a claim, and each claim names the test that would fail if it
stopped being true. Where nothing tests it, the row says so — a claim with no
test behind it is a claim a reviewer should read the code for, and pretending
otherwise would waste their time.

Tests marked **[fuzz]** live in [fuzz/tests/envelope.rs](../../fuzz/tests/envelope.rs)
and run in the gate on both legs; the rest are `bp-crypto`'s own.

### P1 — Nonce reuse within a document

**Structural.** The prefix is constant per file and the suffix is the chunk
index, which is monotone. Not asserted directly, because a nonce is not
observable from outside `seal`; what *is* asserted is that a repeated index is
rejected rather than accepted.

`reordering_chunks_is_caught` · `duplicating_a_chunk_is_caught` ·
`a_document_spanning_many_chunks_round_trips_exactly`

### P2 — Nonce reuse across documents

**This is the sharpest point in the design and it deserves the most attention.**

Nonce prefixes *will* collide. Under AES-256-GCM the prefix is 4 random bytes:
any given pair of documents shares one with probability 2⁻³², and across about
2¹⁶ = 65,536 documents a shared prefix is more likely than not. That is not a
reuse, because the key differs: a fresh 16-byte salt per `seal` gives a
different Argon2id output even for an identical passphrase and an identical
plaintext.

So the entire cross-file `(key, nonce)` guarantee rests on the salt — 128 bits,
birthday-bounded at 2⁶⁴ documents under one passphrase — and **not at all** on
the nonce prefix. Question 1 asks whether that is the right place to put it.

`sealing_the_same_document_twice_gives_different_bytes`

### P3 — Reordering and duplication

Caught by the chunk index in the AAD.

`reordering_chunks_is_caught` · `duplicating_a_chunk_is_caught`

### P4 — Truncation

Caught by the last-chunk flag in the AAD, and by the reader requiring the file
to end exactly where a chunk sealed as last ends.

**With one honest wrinkle.** A file cut *mid-chunk* is `Truncated`, which the
user can act on by finding a better copy. A file cut exactly at a chunk
*boundary* is `CannotOpen` — the surviving final chunk was sealed with
`last = false` and fails to authenticate against an AAD that now says `true`.
It is detected either way; it is only named less helpfully in the second case.
Question 7.

`truncating_the_document_is_caught` · `a_document_cut_mid_chunk_is_reported_as_incomplete` ·
`every_truncation_of_a_real_document_is_an_error` **[fuzz]** — every prefix of
a real document, not a sample

### P5 — Header tampering and downgrade

The whole header is in every chunk's AAD, so no field can be edited without
every chunk failing.

`editing_the_suite_in_the_header_is_caught` ·
`editing_the_kdf_cost_in_the_header_is_caught` ·
`every_byte_of_a_document_is_load_bearing` — walks *every* byte position ·
`no_single_byte_flip_ever_produces_plaintext` **[fuzz]**

Authenticated is not the same as trusted-before-use. See P7.

### P6 — What a wrong passphrase reveals

Nothing beyond the header, which is public anyway. Cost is constant: Argon2id
at the declared parameters, then one failed tag check. There is no early exit
that depends on the passphrase.

`a_wrong_passphrase_does_not_open_the_document` ·
`an_empty_passphrase_is_refused_rather_than_treated_as_a_weak_one` (on seal;
`open` will attempt an empty passphrase, and simply fails to authenticate)

### P7 — Work performed before anything is authenticated

**Unavoidable and bounded, not eliminated.** A tag check needs a key, so the
Argon2id cost written in an *unauthenticated* header is paid in full before the
reader can say the file is rubbish. A single flipped bit in the cost field of a
real document is enough to trigger it.

`KdfParams::validate` is the bound: 256 MiB × 16 passes × 64 lanes. **That
corner has been measured, in release, at ~5.56 s**
([ADR-0035](../decisions/ADR-0035.md), which lowered it from a ceiling
allowing ~75 s). The measurement is the only way to say how large "finite" is,
which is why it is a number rather than an assertion — see `fuzz/README.md`
for the sweep it comes from.

`an_absurd_work_factor_is_refused_rather_than_attempted` ·
`a_memory_cost_that_was_valid_under_the_old_ceiling_is_now_refused` ·
`the_declared_kdf_cost_is_paid_before_anything_is_authenticated` **[fuzz]**,
`#[ignore]`d and run deliberately

What that sweep does *not* isolate is the lane multiplier on its own: it walks
memory and passes, and ADR-0035's 5.56 s covers all three together. Question 5
asks whether 64 lanes is the right bound, not whether the corner is bounded.

### P8 — Memory amplification from a hostile header

Every allocation a reader makes is bounded by the *file's* own length rather
than by anything the header claims. The output buffer is reserved from
`document.len()`; a chunk's ciphertext is a slice of the file; a declared chunk
length longer than the header permits is `Corrupt` before anything is read.

`an_absurd_chunk_size_is_refused` · `arbitrary_bytes_are_survivable` **[fuzz]** ·
`arbitrary_bytes_behind_the_magic_are_survivable` **[fuzz]** ·
`a_header_with_no_body_is_an_error` **[fuzz]** ·
`the_corpus_is_survivable` **[fuzz]**

### P9 — Key commitment

**Absent, unmitigated, and not tested.** Neither AEAD is key-committing and
the envelope carries no commitment value. A ciphertext can therefore be
crafted that authenticates under more than one key, which is the shape of a
partitioning-oracle attack against a passphrase — it turns a user who tries
passphrases and reports success or failure into an oracle that eliminates many
guesses per attempt.

Whether that matters for a desktop text editor is a judgement, and it is
exactly the judgement an outside reviewer is better placed to make than the
author. **Question 2, and the one I would most want answered.**

### P10 — Domain separation

**Absent.** Nothing in the header — and therefore nothing in the AAD — says
what kind of thing is inside the envelope. Four different kinds of secret share
the construction (§4), and to `open` they are indistinguishable. The only
guard anywhere is `read_sealed_signing_key`'s length check. Question 3.

### P11 — Plaintext length

Not hidden, deliberately, and stated in §2.6 so nobody assumes it is.

### P12 — Key and plaintext in memory

A code-reading claim, not a tested one — safe Rust gives no way to assert on
freed memory — so please read it as one:

- the derived key is `Zeroizing<[u8; 32]>`, chosen over discipline because
  `open` has several early returns;
- the plaintext accumulates into a `Zeroizing<Vec<u8>>` **whose capacity is
  reserved up front**, so there are no reallocations to leave unwiped copies
  of earlier chunks behind — wrapping only the returned value would have left
  every intermediate allocation of a multi-chunk document in freed memory;
- each decrypted chunk is zeroized after it is copied out;
- on a failed chunk the accumulator is zeroized explicitly, at the point it
  matters, rather than relying on the reader knowing that drop would.

`#![forbid(unsafe_code)]` is on the crate.

### P13 — Longevity: does a document written today still open?

**Tested as of this document, and it was not before.** Every other round-trip
test in the workspace — including the property test — seals and opens with the
*same build*. That proves the two halves agree with each other and cannot prove
either agrees with what is already on somebody's disk. A change that altered
the layout, the AAD, the nonce construction or the Argon2id invocation
*coherently on both sides* would have passed the whole gate and silently
orphaned every existing document. For a format whose ADR opens by saying a
document written today has to open in ten years, that was the one property
nothing checked.

Three well-formed envelopes were already checked into `fuzz/corpus/envelope/`,
sealed under a known passphrase with known plaintext, and nothing asserted that
any of them opened.

`a_document_sealed_by_an_earlier_build_still_opens` **[fuzz]** ·
`an_earlier_builds_document_does_not_open_under_the_wrong_passphrase` **[fuzz]**

### P14 — What the API does not do

`seal` takes a whole plaintext and returns a whole document; `open` takes a
whole document and returns a whole plaintext. **ADR-0021 gives "a document
larger than memory can be written and read" as one of three reasons for
chunking, and that is true of the format and not of the API.** The framing
permits streaming; nothing implements it. Said here so a reviewer does not go
looking for a streaming reader to review.

---

## 6. The questions a reviewer is asked

Answering any subset is useful. They are roughly in the order I would want
them answered.

1. **Nonce uniqueness rests entirely on the salt** (P2). Sixteen random bytes
   per document, with AES-256-GCM contributing only 4 random nonce bytes of
   its own. Is 128 bits of salt the right place for the whole of that burden,
   or should GCM's prefix be doing some of the work — and does the answer
   change for `bp-audit`, which derives once per log line?
2. **Key commitment** (P9). Should the envelope carry one? What is the
   realistic exposure of a passphrase-based, non-committing AEAD in a desktop
   editor where the user is the only oracle, and what would the cheapest
   adequate fix be?
3. **Domain separation** (P10). A document, a recovery checkpoint, an audit
   line and an Ed25519 signing seed share one construction with nothing
   distinguishing them. Is `read_sealed_signing_key`'s length check enough, or
   does a purpose byte belong in the header — and therefore in every chunk's
   AAD?
4. **The Argon2id output is used directly as the AEAD key** (§2.2), with no
   HKDF and no label. Fine, or a real weakness?
5. **The pre-authentication cost ceiling** (P7): 256 MiB × 16 passes × 64
   lanes, honoured before anything authenticates and measured at ~5.56 s at
   the corner. Is that the right ceiling — and is 64 lanes in particular, which
   no sweep isolates from the other two multipliers?
6. **The AAD encoding** (§2.4): `header || u64 big-endian || u8`. The header is
   fixed-length for a given suite, so there is no length-prefix ambiguity that
   I can see. Confirm or refute.
7. **Truncation at a chunk boundary reports `CannotOpen`** (P4) rather than
   `Truncated`. Is conflating it with a wrong passphrase right, or should a
   reader distinguish "the last chunk does not claim to be last" from "the tag
   failed"?
8. **The error taxonomy as a side channel.** `NotBpadx`, `UnsupportedVersion`,
   `UnsupportedSuite`, `UnsupportedKdf`, `UnreasonableCost`,
   `UnreasonableChunkSize`, `Truncated` and `Corrupt` are all decided before a
   key exists or from the framing; `CannotOpen` is the only one a tag decides.
   Does that partition leak anything?
9. **The four call sites** (§4), and `bp-audit` above all: does one envelope
   per appended log line, under one passphrase with a fresh salt each time,
   break an assumption the envelope makes?
10. **The upgrade story.** Version 1 is written and only version 1 is read; a
    reader that meets version 2 refuses by name rather than guessing. Is
    refusing right, and is a single `u16` at offset 6 enough of a hinge?

---

## 7. Known and conceded before you start

Not defects to find; positions taken, so a reviewer can disagree with the
position rather than rediscover the fact.

- **A wrong passphrase and a damaged file are one error** (§3), on purpose.
- **The plaintext's length is not hidden** (§2.6, P11). No padding.
- **Platform key protection does not exist.** No DPAPI, no Windows Hello, no
  Secret Service. Passphrase only, and ADR-0011 already fixes those as a
  convenience layer over the passphrase rather than a replacement.
- **There is no streaming API** (P14).
- **A verified signature says a key signed these bytes and nothing about whose
  key it is.** There is no trust store, and any UI rendering "signature valid"
  as "this is from Alice" would be lying. ADR-0021 says so; it is repeated
  because it is the kind of thing that gets designed back in.
- **`stable_name` is not a secret.** SHA-256 of a path, truncated to 32 hex
  characters, naming recovery-journal files. Anyone holding the recovery
  directory can test a guessed path against it. Not used for key derivation
  anywhere.

---

## 8. Running it

```powershell
cargo test -p bp-crypto                       # the envelope's own tests
cd fuzz; cargo test                           # survival, corpus, golden vectors
cd fuzz; cargo test --release --test envelope -- --ignored --nocapture
                                              # the pre-authentication cost measurement
./scripts/Invoke-LocalCI.ps1 -Linux           # everything, both platforms, ~7 min
```

There is no hosted CI ([ADR-0016](../decisions/ADR-0016.md)); the script above
*is* it, and it runs the fuzz workspace on both legs.

The corpus in `fuzz/corpus/envelope/` is checked in and the three well-formed
entries are golden vectors now. **Re-running `seed-corpus` regenerates them**,
which is the one thing that would make P13's test pass vacuously — the seeder's
own header says it is not idempotent here, and `fuzz/README.md` says why.
