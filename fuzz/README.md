# Hostile-input harnesses

Phase 19 asked for "fuzz targets for parsers, encrypted envelopes, notebook
import and malformed inputs". This is those targets — but read the next
section before you describe them to anyone, because they are not fuzzing.

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

ADR-0023 measured YAML's nesting limit against **the 1 MiB main thread**,
which is the stack the application parses on. libtest gives a test thread
considerably more, so a cap validated on the default stack proves nothing
about the shipped binary. Every probe runs on a thread pinned to
`bp_fuzz::MAIN_THREAD_STACK`.

Run the suite in **debug** for the depth probes. Release builds use fewer
bytes per frame — ADR-0023 measured 500–1000 levels there against 280–290
unoptimised — so debug is the harsher question.

## Running it

This is a **standalone cargo workspace**, not a member of the root one, for
the same reason `spikes/` are: it carries dependencies that have no business
in the shipped application's graph. `.gitignore` already anticipates it
("any standalone workspace (a spike, a fuzz target) has its own target").

```powershell
cd fuzz
cargo test                          # the whole suite, ~2 minutes
cargo test --test yaml              # one target
cargo test --release --test yaml    # faster; weaker as a depth probe

# A deliberate soak. The default case count is sized for a gate, not a
# fuzzing session.
$env:BP_FUZZ_CASES=100000; cargo test --release --test yaml

# Regenerate the corpus. Only needed if a format changed -- see below.
cargo run --bin seed-corpus

# The pre-authentication KDF cost measurement, which is ignored by default
# because it allocates a gigabyte.
cargo test --release --test envelope -- --ignored --nocapture
```

`BP_FUZZ_CASES` sets how many cases each `proptest` block runs; the default
is 2,000.

### The gate does not run this yet

`Invoke-LocalCI.ps1`'s `spikes` stage globs `spikes/*/` only, so **nothing in
the gate reaches this directory** — not fmt, not clippy, not test. Wiring it
in needs a change to that script, and R008 and the repository's convention
are explicit that a change to the gate is its own commit and never a
passenger on someone else's. It is left undone on purpose. Until then, this
suite runs by hand.

Whoever wires it in: note that `-IncludeSpikes` currently runs only `fmt` and
`clippy` over a standalone workspace. This one has tests worth running, which
is a difference the switch does not currently express.

## The targets

Priority order, which is also value order.

### 1. `tests/yaml.rs` — `bp_data`'s YAML entry points

The highest-value target here, and the reason the directory exists.
ADR-0023 caps nesting at 128 levels and alias expansion at 1,000,000 nodes
because `saphyr`'s loader recurses one stack frame per level and **a 200-byte
input crashed the process**. Those two caps *are* the mitigation; this asks
whether they hold.

Covers `yaml_validate`, `yaml_document_count`, `yaml_format`, `yaml_minify`,
`yaml_to_json` and `json_to_yaml` — all of them, because the guard runs in
`scan_yaml` before a tree is built and every other entry point goes on to
build one. Depth is walked in single steps from 0 to 140 and then out to
100,000, in flow *and* block style; the alias bomb is walked from 2 levels to
100.

### 2. `tests/envelope.rs` — `bp_crypto::open`

An envelope parser reads a header before it has a key, so it acts on whatever
an attacker wrote before it can authenticate anything — `header.rs` says so
itself. Covers `open` under three passphrases (right, wrong, empty),
`is_bpadx` and `stable_name`, plus:

- **every** prefix of a real document (all truncations);
- **every** single-byte corruption of a real document, at three bit masks —
  exhaustive, not sampled, because "no flipped byte produces plaintext" is a
  total claim;
- arbitrary bytes behind a valid magic, so the header parser is actually
  reached rather than rejected in the first comparison;
- a real document with a random slice overwritten, which is the highest-yield
  generator in the file;
- `seal`/`open` round trips over arbitrary options.

### 3. `tests/notebook.rs` — `bp_notebook::import_ipynb`

Arbitrary JSON through both entry points: `import_ipynb` (a parsed `Value`)
and `parse_raw_json_view` (text, which is what the raw-JSON editor writes
into, so it meets input a human has just hand-edited into an invalid state).
Anything that imports is exported and imported again — a field accepted as
"missing, warned about" has to be writable back as something.

Object keys are drawn from the `.ipynb` vocabulary nine times in ten. A
generator producing random keys never writes `"execution_count"`, so the
branch reading it is never entered; this is the substitute for coverage
feedback and it is used in every target here.

### 4. `tests/data.rs` — JSON, JSONL, TOML, CSV, and `bp_formats::sniff`

Every `&str` entry point in `bp_data` except the YAML ones, run over the same
input regardless of what the input looks like — a CSV reader handed JSON is
what happens when a user picks the wrong menu item. Includes the YAML corpus
pointed at these readers, nesting depth for the formats with their own limits
(`serde_json`'s 128, which ADR-0023 chose the YAML cap to match), extreme
table shapes (10,000 columns; 20,000 rows; a 50,000-character unterminated
quoted field), and a round-trip property: whatever `json_format`,
`json_minify` and `json_sort_keys` write must parse back.

`bp_formats::sniff` is here because it is total — it returns a `Format`,
never a `Result` — which means a panic is its only possible failure, and it
runs on every file the editor opens.

### 5. `tests/files.rs` — `bp_files::load`

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
provoke. Re-running the seeder is **not** idempotent for `corpus/envelope/` —
`seal` draws a fresh random salt per document — so only re-run it if a format
actually changed.

The pathological entries, which came from the ADRs and crate docs rather than
being invented here:

| Entry | Source |
| --- | --- |
| `yaml/nesting-at-cap.yaml`, `nesting-over-cap.yaml` | ADR-0023's 128 |
| `yaml/nesting-block-384.yaml` | the nested block mappings ADR-0023 measured dying at 290 |
| `yaml/nesting-flow-100000.yaml` | the depth ADR-0023 says the event stream reads without touching the stack |
| `yaml/billion-laughs.yaml` | `bp_data`'s own `an_alias_bomb_is_refused_before_it_is_expanded` — 10^10 nodes, under a kilobyte, ten levels deep |
| `yaml/billion-laughs-x2.yaml` | the same at twenty levels: 10^20, which overflows a non-saturating `u64` |
| `yaml/duplicate-key.yaml` | ADR-0023's "a duplicate mapping key is an error, not a merge" |
| `yaml/implicit-key-over-1024.yaml` | the 1025-character key `yaml_format` refuses because the emitter would write something that will not parse back |
| `yaml/recursive-alias.yaml` | an alias to the anchor being defined |
| `envelope/truncated-body.bpadx`, `truncated-header.bpadx` | a partial copy or an interrupted sync |
| `envelope/flipped-ciphertext-byte.bpadx`, `flipped-header-byte.bpadx` | ADR-0021 authenticates the header as additional data, so both must refuse |
| `envelope/kdf-cost-raised.bpadx`, `kdf-cost-absurd.bpadx` | one inside the format's cost ceiling, one past it |
| `files/utf16le-odd-length.bin` | `LoadError::TruncatedUtf16` — an odd body, so the last code unit is cut in half |
| `files/utf16le-unpaired-high-surrogate.bin` and three siblings | `LoadError::UnpairedSurrogate`, in both byte orders and at end-of-file |
| `files/utf16le-lone-bom.bin` | a file that is nothing but a mark: a valid empty document, not an error |
| `files/utf8-bom-then-zwnbsp.bin` | found by this harness — see below |

Plus, in each directory, the ordinary and the merely awkward: empty files,
comments only, merge keys, complex keys, `.nan`/`.inf`, overlong UTF-8, lone
surrogate escapes in JSON, ragged and quote-damaged CSV, notebooks with
duplicate cell ids and outputs of the wrong shape.

## What was found

**No panic. No abort. No hang.** Across the corpus, the exhaustive sweeps and
several hundred thousand property cases per target, every one of the five
targets held its invariant. In particular ADR-0023's two caps hold: nothing
between 0 and 100,000 levels of YAML nesting reached a stack overflow on a
1 MiB stack in a debug build, in either flow or block style, and the alias
bomb is refused at every size from 2 levels to 100.

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

### A document's declared KDF cost is paid before anything is authenticated

`bp-crypto`. `open` parses the header, derives a key at the cost the header
declares, and only then checks a tag. It cannot be otherwise — a tag check
needs a key — but it means the cost written in an **unauthenticated** header
is paid in full before the reader can say the document is rubbish. A single
flipped bit in the cost field of a real document is enough.

`KdfParams::validate` is the bound, and it is doing its job: 4 GiB is refused
in microseconds. Measured on this machine (release, `--ignored`):

```text
declared cost       8 KiB ->   46.5µs to refuse
declared cost    1024 KiB ->    1.5ms to refuse
declared cost   65536 KiB ->  124.2ms to refuse
declared cost  524288 KiB ->     1.1s to refuse
declared cost 1048576 KiB ->     2.1s to refuse
64 MiB at   1 passes ->  125.1ms to refuse
64 MiB at   8 passes ->  643.6ms to refuse
64 MiB at  64 passes ->     4.7s to refuse
declared cost 4 GiB -> refused immediately
```

The ceiling `validate` permits is 1 GiB × 64 passes × 64 lanes. Argon2's time
is linear in passes, so that corner is roughly **75 seconds and a gigabyte of
resident memory per attempt**, on an unauthenticated header, before the user
is told the file is damaged. That is a bound rather than an unbounded
denial-of-service, which is what ADR-0021 set out to achieve, and it is
written down here because "generous but finite" is easier to review with the
number attached. Test:
`envelope::the_declared_kdf_cost_is_paid_before_anything_is_authenticated`.

### The corpus is hostile to the editor, too

Not a defect in anything this repository ships, and recorded because it costs
somebody a confusing hour otherwise.

`corpus/yaml/billion-laughs.yaml` is 570 bytes and it kills the YAML language
server that Google's Cloud Code extension for VS Code runs. That server
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

What works is not installing the extension, or disabling it for this
workspace — it is a GCP, Kubernetes and Skaffold extension, and the only match
for any of those words in this tree is this paragraph. VS Code keeps
per-workspace extension enablement in its own state rather than in `.vscode/`,
so that cannot be committed for everyone; it is one action per clone, from the
Extensions view.

Two things generalise beyond one extension. **Any tool that walks the tree
looking for YAML is a candidate** — the `.claude/worktrees/` copies multiply
every hit, and the same corpus is reachable from editors, indexers and search
tools that were never told what this directory is. And the caps ADR-0023
argues for are not belt-and-braces: this is what the same input does to a
mature parser written by somebody else that does not have them.

## Deliberately not covered

- **Anything requiring nightly or a sanitizer.** No ASan, no MSan, no
  libFuzzer. Stated at the top, repeated here because it is the largest gap.
- **`bp-crypto`'s cryptographic claims.** Nothing here asserts that a
  document *cannot* be opened without the passphrase, only that trying does
  not crash. Confidentiality is a property of the primitives and of ADR-0021's
  construction; a survival harness is the wrong instrument for it.
- **`bp_crypto::sign`.** Ed25519 signatures landed alongside the envelope and
  have no target here. They should get one.
- **The recovery journal (ADR-0022), `bp-secrets`, and the security profile
  (ADR-0020).** All parse or deserialise stored state, all are reachable from
  a file on disk, none is covered.
- **`bp_files::save`**, `atomic_write` and `resolve_in_dir` — the write half.
  Hostile *paths* rather than hostile bytes, which is a different target with
  different setup, and it is missing.
- **`bp-config`, `bp-history`, `bp-clipboard`, `bp-search`, `bp-redaction`,
  `bp-integrity`.** Not surveyed at all. Several read files.
- **Concurrency.** Every probe is single-threaded. Nothing here would find a
  race, and `bp-files`' watch/reload path has one to be found or ruled out.
- **Memory growth as a failure.** A hang is detected by a clock; an input
  that allocates steadily without ever finishing would be caught only if it
  overran the budget. There is no allocation ceiling in this harness.
- **Which error comes back.** Every target checks that a refusal *happens*,
  never that it is the right variant. That is each crate's own business and
  each crate tests it.
