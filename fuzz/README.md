# Hostile-input harnesses

Phase 19 asked for "fuzz targets for parsers, encrypted envelopes, notebook
import and malformed inputs". **Two of those five harnesses remain**, and the reduction is why:
[ADR-0057](../docs/decisions/ADR-0057.md) removed `bp-notebook` and its
`import_ipynb` target, [ADR-0062](../docs/decisions/ADR-0062.md) removed
`bp-data` with the YAML target and corpus, and
[ADR-0064](../docs/decisions/ADR-0064.md) removed `bp-crypto` with the
envelope target and its three golden vectors.

**A corpus is evidence about a parser, and there is no parser left for any of
those to be evidence about.** What remains — `bp_formats::sniff` and
`bp_files::load` — is every parser this product still points at a file
somebody else wrote.

**Losing the envelope target is the largest single reduction in assurance
here**, and it is worth saying rather than leaving to be inferred from a
shorter file: it did exhaustive single-byte corruption, every prefix of a real
document, and the only assertion anywhere about a document *this build did not
write*.

Read the next section before you describe any of this to anyone, because they
are not fuzzing.

## This is not fuzzing

**There is no coverage-guided fuzzer here.** `cargo-fuzz` needs a nightly
toolchain and libFuzzer, and this machine has neither:

```text
$ cargo fuzz --version
error: no such command: `fuzz`
$ rustup toolchain list
stable-x86_64-pc-windows-msvc (active, default)
1.97.1-x86_64-pc-windows-msvc
```

Installing a nightly toolchain to satisfy a directory name would have made
the gate depend on a toolchain nobody in this project runs, and ADR-0016
makes the local gate the *only* gate. A stage nobody can run is worse than no
stage. So this is the version everybody can run: a checked-in corpus of the
inputs already known to be pathological, plus `proptest` generating
structured and unstructured input around them.

**What that keeps:** the invariant, the corpus, the regression value of both,
and a suite that fails on the day one of these parsers starts panicking.

**What it does not give you, and a real fuzzer would:**

- no coverage feedback, so nothing walks towards an unexplored branch — every
  generator here is blind, and the "salad" strategies exist only to
  *approximate* what coverage feedback would have found for free;
- no corpus minimisation, and no automatic crash minimisation beyond
  `proptest`'s shrinking;
- no sanitizers — no ASan, no MSan, so a memory error in a dependency's
  `unsafe` block goes unseen unless it happens to trip a Rust bounds check;
- no persistent corpus growing across runs. What is in `corpus/` is what
  somebody put there on purpose.

Random and property input finds shallow bugs. A guided fuzzer finds the ones
behind three conditionals. **Do not read a green run here as "these parsers
have been fuzzed."** If a nightly toolchain ever becomes acceptable, the
corpus and the invariant transfer to `cargo-fuzz` unchanged; only the driver
would be rewritten.

## The invariant

Every target asserts one thing, and it is the same one everywhere:

> **The function may return an error. It may not panic, abort, or hang.**

An error is a designed outcome that reaches the user as a sentence. A panic
in a parser reachable from "open this file" is a crash on a file somebody was
sent; an abort or a hang is that crash with a worse diagnosis.

How each is detected:

| | |
| --- | --- |
| **Panic** | `catch_unwind` round every call, so one bad input is a named failure with its input reported and the rest of the corpus still runs. |
| **Abort** | Uncatchable by definition. A stack overflow kills the process and the test binary dies — which *is* the signal, because it is exactly what the application would do. |
| **Hang** | Corpus entries run on their own thread with a time budget; overrunning it fails the run rather than holding the gate open. The wedged thread is leaked, because std cannot kill one. |

### Stack size is part of the harness

Every probe runs on a thread pinned to `bp_fuzz::MAIN_THREAD_STACK` — **the
1 MiB main thread**, which is the stack the application parses on. libtest
gives a test thread considerably more, so a limit validated on the default
stack proves nothing about the shipped binary.

This was ADR-0023's rule and it outlived that ADR. The depth probes it was
written for went with `bp-data` (ADR-0062); the pinning stays, because
`bp_files::load` recurses on hostile input too and the reasoning was never
about YAML.

## Running it

This is a **standalone cargo workspace**, not a member of the root one, for
the same reason `spikes/` are: it carries dependencies that have no business
in the shipped application's graph. `.gitignore` already anticipates it
("any standalone workspace (a spike, a fuzz target) has its own target").

```powershell
cd fuzz
cargo test                            # the whole suite, ~2 minutes
cargo test --test files            # one target
cargo test --release --test files  # faster

# A deliberate soak. The default case count is sized for a gate, not a
# fuzzing session.
$env:BP_FUZZ_CASES=100000; cargo test --release --test files

# Regenerate the corpus. Only needed if a format changed -- see below.
cargo run --bin seed-corpus

# The pre-authentication KDF cost measurement, which is ignored by default
# because it allocates a gigabyte.
```

`BP_FUZZ_CASES` sets how many cases each `proptest` block runs; the default
is 2,000.

### The gate runs this

It did not, for as long as this section said so. `Invoke-LocalCI.ps1`'s
`spikes` stage globs `spikes/*/` only, so nothing in the gate reached this
directory — not fmt, not clippy, not test — while `ROADMAP.md` called phase
19 *Started* on the strength of what lives here.

Two stages now do, on the full run and both legs, and deliberately **not**
behind `-IncludeSpikes`: a spike is a prototype the product does not depend
on, and these are tests of shipped crates against input designed to break
them. They cost about two and a half minutes per leg, which is why they are
out of `-Quick` and why pre-push rather than pre-commit is where they land.

## The targets

Priority order, which is also value order.

### 1. `tests/data.rs` — `bp_formats::sniff` and `detect`

**Narrowed by [ADR-0062](../docs/decisions/ADR-0062.md), not deleted.** It
covered every `&str` entry point in `bp_data`; `sniff` was here too, and is
covered nowhere else. The filename now names the corpus it reads
(`corpus/data/`) rather than the crate it exercises.

`sniff` is total — it returns a `Format`, never a `Result` — so a panic is its
only possible failure, and it runs on every file the editor opens before
anything else looks at it. Two hundred lines of prefix matching over the first
kilobyte.

**What the narrowing cost is worth reading before trusting a green run.**
`sniff` used to have a consumer that could contradict it: `bp-data` had to
parse a document as whatever `sniff` called it, and a cross-crate test
asserted the two agreed. *That test found a real defect* — `sniff` counted
lines beginning with `{` and classified a pretty-printed JSON array as JSON
Lines. Nothing can ask that question now. So this target gained one property
that is not survival — **appending text past the first kilobyte must not
change the verdict** — which is checkable without a parser and is the most
that can honestly be asserted.

### 2. `tests/files.rs` — `bp_files::load`

The first thing that touches a file the user picked. The UTF-16 decoder is
hand-written, so it is exactly what this is for. Covers arbitrary bytes,
arbitrary bytes behind each byte-order mark, UTF-16 assembled from a code-unit
alphabet where a third of the draws are surrogate halves, and a near-
exhaustive sweep of every short body behind every mark.

It also checks the promise the crate's own documentation makes: **anything
that loads must encode back to exactly the bytes on disk.** That held
throughout. See "What was found" for the one place where the *text* does not.

## The corpus

`corpus/<target>/`, checked in, written by `src/bin/seed-corpus.rs`. Every
entry has a comment there saying where it came from and what it is meant to
provoke. It is idempotent now: `corpus/envelope/` was the one directory it was
not, because `seal` drew a fresh salt per document, and that directory is
gone.

**The three golden vectors are gone.** `envelope/sealed.bpadx`,
`sealed-aes.bpadx` and `sealed-multichunk.bpadx` were sealed once under a
known passphrase and asserted to still open, and they were the only test in
the repository that said anything about a document *this* build did not write
([ADR-0050](../docs/decisions/ADR-0050.md)). They went with `bp-crypto` under
[ADR-0064](../docs/decisions/ADR-0064.md). Nothing here replaces them, and
nothing can: the claim needed a format to make it about.

The pathological entries, which came from the ADRs and crate docs rather than
being invented here:

| Entry | Source |
| --- | --- |
| `files/utf16le-odd-length.bin` | `LoadError::TruncatedUtf16` — an odd body, so the last code unit is cut in half |
| `files/utf16le-unpaired-high-surrogate.bin` and three siblings | `LoadError::UnpairedSurrogate`, in both byte orders and at end-of-file |
| `files/utf16le-lone-bom.bin` | a file that is nothing but a mark: a valid empty document, not an error |
| `files/utf8-bom-then-zwnbsp.bin` | found by this harness — see below |

Plus, in each directory, the ordinary and the merely awkward: empty files,
comments only, overlong UTF-8, lone surrogate escapes in JSON, and ragged and
quote-damaged CSV.

**`corpus/yaml/` is gone**, with the eight pathological entries this table
used to lead with ([ADR-0062](../docs/decisions/ADR-0062.md)). They were
evidence about `bp-data`'s YAML loader and there is no loader; keeping a
corpus that crash-loops any language server reading workspace YAML, for a
parser this product no longer ships, is not a trade worth making. `corpus/data/`
stays and feeds `sniff`.

## What was found

**No panic. No abort. No hang.** Across the corpus, the exhaustive sweeps and
several hundred thousand property cases per target, every one of the targets
held its invariant.

**There were five targets when that was written and there are three.** Neither
removed target — notebook import (ADR-0057) or YAML (ADR-0062) — is among the
ones that found something, so this section loses no finding. What it does lose
is a claim: ADR-0023's two caps held under every probe, nothing between 0 and
100,000 levels of YAML nesting reached a stack overflow on a 1 MiB stack, and
the alias bomb was refused at every size from 2 to 100. **That was true and is
now unrepeatable**, because the code it was true of has been deleted. It is
recorded here as history rather than as assurance.

Two observations that are not crashes and are recorded rather than fixed.
Neither crate was touched — other agents are working in this tree.

### A leading U+FEFF saved as UTF-8 without a mark is read back as a mark

`bp-files`. A document whose first character is U+FEFF, saved as
`Encoding::Utf8` (no byte-order mark), is written as `EF BB BF` — which is
also, exactly, a UTF-8 byte-order mark. `Encoding::detect_bom` cannot tell
them apart, because nothing can. The file loads as `Utf8Bom` with the
character **gone from the text**, and the next save writes it back as a mark.

The bytes on disk are never wrong: `encode(load(bytes)) == bytes` held for
every input tried, which is the promise `bp-files` actually makes. What
changes silently is the text — one character disappears and the encoding
label changes underneath the user, in a crate whose stated rule is "never
substitute".

Recorded because "fix" means choosing between two lies: refusing to save a
legal document, or writing a leading U+FEFF as something no other tool reads.
Test:
`files::a_leading_zwnbsp_saved_without_a_bom_is_read_back_as_a_bom`.
Corpus: `files/utf8-bom-then-zwnbsp.bin`.

### The corpus was hostile to the editor, too

**This hazard is gone with the corpus** ([ADR-0062](../docs/decisions/ADR-0062.md)
deleted `corpus/yaml/`), and the account is kept because the *shape* recurs:
any committed corpus is input somebody's tooling will read without being told
what it is. Read it as a warning about the next one, not a live problem.

`corpus/yaml/billion-laughs.yaml` was 570 bytes and it killed the YAML
language server that Google's Cloud Code extension for VS Code runs. That server
defaults to `cloudcode.yaml.yamlFileMatcher = "**/*.yaml"` — the whole
workspace — and expands aliases eagerly, with no equivalent of ADR-0023's cap.
Opening the workspace is enough. It climbs to the ~4 GB Node heap ceiling in
about 50 seconds and aborts:

```text
FATAL ERROR: Ineffective mark-compacts near heap limit
             Allocation failed - JavaScript heap out of memory
[20292] 50336 ms: Incremental Mark-Compact (reduce) 3997.0 (4000.4) -> 3992.9 MB
Server process exited with code 134.
```

Then it restarts, reopens the file and does it again — 21 times in one window
before anyone looked. The user-visible symptom is three notifications that say
nothing about YAML: *Restarting server failed*, *couldn't create connection to
server*, *Pending response rejected since connection got disposed*. Nothing
points at this directory, which is why it is written down here.

Confirmed by driving that extension's own bundled server directly, capped at a
1 GiB heap, with a single `didOpen` and no other input:

| Document | Result |
| --- | --- |
| `corpus/yaml/billion-laughs.yaml` (570 bytes) | exit 134 after 12.3 s |
| a four-line Kubernetes Pod manifest | survived 45 s, no crash |

**Narrowing that matcher does not fix it, and this repository briefly claimed
it did.** Setting `cloudcode.yaml.yamlFileMatcher` to a glob this tree has no
files for reaches the server — it lands in
`SettingsRegistry.instance.yamlFilePattern` — but a window opened with the
setting in place still aborted four times in four minutes. The pattern governs
which files are matched against schemas, not which are parsed, and the log says
`Server initialization failed`: it dies during startup, before the setting
spares it anything. There is no checked-in setting that prevents this.

What worked was not installing the extension, or disabling it for this
workspace. VS Code keeps per-workspace extension enablement in its own state
rather than in `.vscode/`, so that could not be committed for everyone; it was
one action per clone.

**Three things generalise past the extension and past the file.** Any tool
that walks the tree looking for a format a corpus contains is a candidate —
the `.claude/worktrees/` copies multiply every hit, and a corpus is reachable
from editors, indexers and search tools that were never told what the
directory is. The caps ADR-0023 argued for were not belt-and-braces: this is
what the same input does to a mature parser that lacks them.

And the third is the one this session added. On the morning of 2026-08-29 the
answer here was **"the corpus is untouched and the listing is narrowed
instead"**, because the corpus was evidence about a parser this product
shipped. By the afternoon ADR-0062 had removed that parser, and the same file
went from *valuable and awkward* to *purely a hazard* without changing a byte.
**What a committed artefact is worth is a function of what still reads it**,
and that is not a property of the artefact.

## Deliberately not covered

- **Anything requiring nightly or a sanitizer.** No ASan, no MSan, no
  libFuzzer. Stated at the top, repeated here because it is the largest gap.
- **Nothing cryptographic.** `bp-crypto` and `bp-integrity` are gone
  ([ADR-0064](../docs/decisions/ADR-0064.md)), so there is no envelope, no
  signature and no key material anywhere in this product.
- **The recovery journal and the privacy profile (ADR-0020).** Both parse or
  deserialise stored state, both are reachable from a file on disk, neither is
  covered.
- **`bp_files::save`**, `atomic_write` and `resolve_in_dir` — the write half.
  Hostile *paths* rather than hostile bytes, which is a different target with
  different setup, and it is missing.
- **`bp-config`, `bp-history`, `bp-search`, `bp-storage`.** Not surveyed at
  all. Several read files.
- **Concurrency.** Every probe is single-threaded. Nothing here would find a
  race, and `bp-files`' watch/reload path has one to be found or ruled out.
- **Memory growth as a failure.** A hang is detected by a clock; an input
  that allocates steadily without ever finishing would be caught only if it
  overran the budget. There is no allocation ceiling in this harness.
- **Which error comes back.** Every target checks that a refusal *happens*,
  never that it is the right variant. That is each crate's own business and
  each crate tests it.
