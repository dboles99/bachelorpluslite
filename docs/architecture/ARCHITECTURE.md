# Architecture

## Dependency rule

```text
UI
 ↓
Core Editor
 ↓
Files / Formats / Search / Storage
 ↓
Optional Semantic / Research / Notebook Services
 ↓
Security Policy Constraints
```

The UI may call services through typed interfaces. The semantic layer may not
become a prerequisite for basic text editing (ADR-0006). `bp-core` depends on
no other `bp-*` crate, and everything else may depend on it.

## Crate map

This is the intended shape of the workspace. **Only crates marked *live* exist
on disk and are workspace members** — the rest are the plan, not the build.
An empty crate is a compile unit and a merge surface for a structure nothing
has exercised, so a crate gets created when there is something to put in it,
and this table carries the intent until then.

| Crate | Status | Purpose | Phase |
| --- | --- | --- | --- |
| `bp-core` | **live** | Document, Workspace, SaveState, encoding and line endings. Holds no text. | 1 |
| `bp-naming` | **live** | Semantic filename grammar (ADR-0003). Pure: no filesystem, no clock. | 1 |
| `bp-files` | **live** | Atomic save, load, encoding detection, external-change stamps (ADR-0007). | 1, 3 |
| `bp-theme` | **live** | Palettes as data (ADR-0009). Green is the default. | 1, 17 |
| `bp-config` | **live** | Settings precedence, config file, recent-files list, recovery from bad input. | 1 |
| `bp-ui` | **live** | The Slint application shell (ADR-0015). | 1 |
| `bp-buffer` | **live** | Rope buffer, character indices, line/column maths. Not yet the editor's storage — see below. | 2, 4 |
| `bp-editor` | **live** | Caret, selection, edits, transaction-based undo/redo, and whole-document line operations. Only `lines` is wired — see below. | 2 |
| `bp-history` | **live** | Crash-safe recovery journal and autosave checkpoints. | 3 |
| `bp-formats` | **live** | Format detection and profiles (ADR-0008). | 5 |
| `bp-data` | **live** | Structured-data operations: validate, format, minify, convert, report. | 6 |
| `bp-search` | **live** | Literal and regex find/replace, plus recursive cross-file search. | 7 |
| `bp-semantic` | **live** | Deterministic extraction: titles, keywords, summaries, outlines. Layer one only. | 8 |
| `bp-clipboard` | **live** | Clipboard history and kind detection (ADR-0010). In memory only. | 11 |
| `bp-organize` | planned | Projects, topics, tags, related notes, duplicate detection. | 9 |
| `bp-storage` | planned | SQLite metadata store and migrations. | 9 |
| `bp-notebook` | planned | Cell model, `.ipynb` import/export. | 12 |
| `bp-execution` | planned | Runners and execution security. Never auto-runs (ADR-0011). | 12 |
| `bp-research` | planned | Citations, paper metadata, research profile. | 13 |
| `bp-security` | planned | Security profiles, privacy mode, policy propagation (ADR-0011). | 14, 16 |
| `bp-crypto` | planned | `.bpadx` envelope, KDF and AEAD. No custom cryptography. | 15 |
| `bp-secrets` | planned | Platform key protection, secret scanning. | 16 |
| `bp-integrity` | planned | Hashing, signatures, verification. | 16 |
| `bp-redaction` | planned | Redaction and metadata inspection. | 16 |
| `bp-audit` | planned | Security audit history. | 16 |
| `bp-platform` | planned | Platform traits. The seam ADR-0001 requires. | 18 |
| `bp-platform-windows` | planned | DPAPI, Hello, default-app registration, native clipboard history. | 18 |
| `bp-platform-linux` | planned | Secret Service, `.desktop` and MIME registration. | 18 |

## Current dependency edges

```text
bachelorpad ──> bp-config
            ├──> bp-theme
            └──> bp-ui ──┬─> bp-core
                         ├─> bp-files ──> bp-core, bp-naming
                         ├─> bp-buffer   (line counting only, for now)
                         ├─> bp-clipboard
                         ├─> bp-config
                         ├─> bp-data
                         ├─> bp-editor  (line operations only, for now)
                         ├─> bp-formats
                         ├─> bp-history
                         ├─> bp-naming
                         ├─> bp-search
                         ├─> bp-semantic
                         ├─> bp-theme
                         └─> slint, rfd, arboard

bp-editor ──> bp-buffer
```

**Seven crates depend on nothing else in the workspace**: `bp-core`,
`bp-naming`, `bp-theme`, `bp-config`, `bp-buffer`, `bp-formats`, `bp-data`,
`bp-search`, `bp-semantic`, `bp-clipboard` and `bp-history`. That is what
keeps them cheap to test and impossible to entangle with the UI toolkit — and
it is why 280 tests run without a window.

Two deliberate non-dependencies:

- `bp-config` does not depend on `bp-theme`. A theme name from a user's file
  has to be recoverable when it is wrong, so it stays a string until the
  application resolves it.
- `bp-semantic` does not depend on `bp-formats`. Extraction reads text; asking
  it what kind of file it is would couple two layers that have no reason to
  know about each other.

## The editor-view boundary

`bp-buffer` and `bp-editor` exist and are tested. The shell uses
`bp-editor::lines` — the whole-document line operations, which need no caret —
and **none of the rest**. That is deliberate, and it is one cause with three
effects.

Slint's `TextInput` owns its own text and caret. It hands the entire buffer
back on every edit, keeps its own undo stack, and exposes the caret only
through a property marked *"internal, undocumented, only exposed for tests"*.
Consequently:

- the rope cannot become the storage — a rope behind that widget means
  converting to `String` on every push, which is worse than the `String` it
  would replace;
- our undo cannot become the authority — two undo stacks over one document is
  worse than one;
- the status bar cannot show Ln/Col — the caret is not legitimately readable.

All three unblock together when a custom, virtualised editor view owns the
text and routes keys itself. Until then the shell keeps
`HashMap<DocumentId, String>`, which measures fine to roughly 40 MB.

Building the libraries first is the right order: their semantics are provable
without a window, and the widget is the risky part.

| Document | insert via `String` | insert via rope |
| ---: | ---: | ---: |
| 100 KB | 55.8 µs | 0.3 µs |
| 1 MB | 563.7 µs | 0.3 µs |
