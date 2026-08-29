# BachelorPad+ v1 Master Specification

## 1. Product definition

BachelorPad+ is a cross-platform Rust-native semantic text-processing appliance for:

- Windows 10
- Windows 11
- Linux

It must remain usable as a very fast plain-text editor when all semantic, research and cloud-connected features are disabled.

Canonical positioning:

> **BachelorPad+ — Notepad when you want it. More when you need it.**

Brand direction: original retro-futurist consumer appliance, drawing from mid-century and 1970s visions of future domestic technology, exaggerated convenience products, industrial labels, status lamps, chunky panels, and deadpan appliance humor. It must not copy names, logos, props, characters, typography, dialogue, fictional companies, or distinctive assets from any existing entertainment property.

## 2. Default filename grammar

```text
<Semantic Title>_<DDMMMYYYY>.<extension>
```

Example:

```text
Rust Migration Notes - UE AI Editor Suite_16AUG2026.txt
```

Rules:

- date is creation date by default
- month is uppercase English abbreviation
- semantic title is human-readable
- invalid filesystem characters are sanitized
- collision suffix `_02`, `_03`, ...
- intentional revision suffix `_v2`, `_v3`, ...
- semantic rename and physical move always require explicit user approval

## 3. Status bar

The status bar can expose:

- save state
- last successful disk save time
- last recovery checkpoint
- full path
- format
- encoding
- line endings
- line/column
- zoom
- validation state
- record count
- semantic-index state
- security state
- theme

Example:

```text
✓ Saved 8:05 PM │ Markdown │ UTF-8 │ CRLF │ Ln 42, Col 18 │ Semantic: Current │ G:\Notes\BachelorPad Research_16AUG2026.md
```

Unsaved:

```text
● Unsaved │ Recovery 12 sec ago │ Last disk save 8:01 PM │ Markdown │ G:\Notes\...
```

A recovery checkpoint must never be shown as a successful disk save.

## 4. Core editor

Required:

- tabs
- new/open/save/save-as/save-copy/save-all
- drag-and-drop
- command-line opening
- open multiple files in tabs
- rope or comparable large-text buffer
- undo/redo
- structural undo where practical
- multi-cursor
- column selection
- line numbers
- word wrap
- folding
- indentation guides
- duplicate/move/sort/remove-duplicate lines
- date/time insertion
- go-to-line
- zoom

## 5. Large files and Rust-specific performance

Required:

- large-file detection
- memory-mapped read-only mode
- streaming search
- incremental rendering
- read-only initial handling for very large files
- parallel multi-file analysis
- non-blocking background operations
- high-performance recursive search
- streaming structured-data processing
- content-addressed revision history where practical
- compression for historical/recovery data

## 6. File safety

Required:

- atomic save
- flush and verification path
- external-change watcher
- compare/reload/keep-mine/save-copy flow
- crash-safe edit journal
- session recovery
- autosave/recovery checkpoints
- revision history
- hashing/integrity verification

## 7. Formats

First-class support:

- TXT
- Markdown
- YAML/YML
- JSON
- JSONL/NDJSON
- TOML
- INI
- CSV/TSV
- XML
- HTML/CSS
- Rust
- Python
- PowerShell
- Shell
- SQL
- JavaScript/TypeScript
- LOG
- IPYNB

Format profiles:

- Plain Text
- Markdown
- Structured Data
- Tabular Data
- Source Code
- Notebook
- Log

*Research Note* was on this list and was never built -- no `Profile` in
`bp-formats` ever carried it -- and the capability it would have grouped
formats for left under [ADR-0060](docs/decisions/ADR-0060.md). Removed rather
than left to read as planned.

## 8. Markdown

- source/preview/split
- outline
- heading operations
- links/images/tables/checklists/code blocks
- citations/footnotes
- table of contents
- heading normalization
- export HTML/PDF

## 9. Structured data

### YAML
- validate
- format
- sort keys
- duplicate-key detection
- outline/folding
- JSON/TOML conversion

### JSON
- raw/tree/split
- validate
- pretty/minify
- sort keys
- structural selection
- JSONPath-like copying

### JSONL / NDJSON
- streaming validation
- record count
- malformed-record detection
- field extraction
- schema inference
- deduplication
- statistics
- filter/sort/query
- table view
- JSON/CSV conversion

### CSV / TSV
- delimiter detection
- table/raw views
- column typing
- filtering/sorting
- statistics
- missing values
- duplicate rows
- JSON/JSONL conversion

## 10. Semantic layer

Three layers:

1. deterministic extraction
2. local statistical/NLP models
3. optional generative model providers

Capabilities:

- title generation
- filename generation
- document classification
- topics
- keywords
- entities
- project inference
- summaries
- tags
- related notes
- duplicate and near-duplicate detection
- folder suggestions
- semantic search
- hybrid lexical + semantic search
- semantic diff

Semantic features must be optional. The editor may not depend on cloud AI.

## 11. Organization model

Canonical work model:

```text
project
  -> task
    -> step
      -> prompts/rosettas
        -> artifacts
          -> metadata
            -> database
              -> note
```

For document organization:

```text
workspace
  -> project
    -> topic
      -> document
        -> section
          -> cell
            -> artifact
```

Virtual organization must not require moving physical files.

## 12. Notebook and execution

**Deleted, in full.** [ADR-0057](docs/decisions/ADR-0057.md) removed the
ability to execute anything, and notebooks with it: `bp-notebook`,
`bp-execution`, the Notebook and Run menus, and the panel a cell's output was
drawn in. A text editor that runs code is a different product, and this one is
being reduced to the feature set of Windows 11 Notepad plus a note-organising
layer.

What this section asked for that **survives**: `.ipynb` is still a recognised
file type. `bp-formats` identifies one and `bp-platform` still offers to
register for it -- a Jupyter notebook is JSON, JSON is text, and opening one
is not a claim this product cannot keep. Section 7's `IPYNB` and `Notebook`
rows are those, and they stay.

The security rule this section carried -- *never auto-run an opened or pasted
notebook cell* -- is **removed rather than kept**. It was enforced by a
`UserGesture` no parsed file could construct, that mechanism worked, and it
now has nothing to guard: nothing auto-runs because nothing runs. A constraint
that cannot fail reads as live when it is vacuous, which is the trap
`CLAUDE.md` numbers 3.

## 13. Rust-aware commands

When applicable:

- Rust scratchpad
- cargo check
- cargo test
- cargo fmt
- cargo clippy
- run current crate
- open Cargo.toml

BachelorPad+ remains a text appliance, not a full IDE.

## 14. Clipboard

**Deleted, less one row.** [ADR-0061](docs/decisions/ADR-0061.md) removed
`bp-clipboard` -- the history, pinning, kind detection and the format-aware
paste transformations -- along with the `Clipboard` policy axis that decided
whether a history could persist. Windows Notepad has no clipboard history and
neither does a note-organising layer.

**Cut, Copy and Paste survive and always did**, as `bp_editor::Command` values
over the OS clipboard. They shared a word with the history and nothing else.
*Paste Special* was this section's name for the transformations and goes with
them.

The rule this section ended on -- *persistent clipboard history is opt-in* --
is removed rather than kept, by ADR-0057's rule: there is no history to make
opt-in, and a constraint that cannot fail reads as live when it is vacuous.

## 15. Security and cryptography

Required security concepts:

- `.bpadx` encrypted file format
- passphrase protection
- Argon2id KDF
- established AEAD encryption implementation
- platform-neutral encrypted format
- Windows DPAPI/key protection where appropriate
- Windows Hello-assisted unlock where supported
- Linux secret-service/keyring integration where supported
- per-document security profiles: Standard, Private, Confidential, Maximum, Custom
- Privacy Mode
- encrypted recovery
- encrypted edit journals
- encrypted revision history
- secret scanning
- auto-lock
- document hashing
- digital signatures
- redaction
- metadata inspector
- security audit history
- semantic privacy controls
- security inspector
- sensitive-memory wrappers/zeroization where practical

No custom cryptography.

## 16. Themes

Built-ins:

- Light
- Dark
- Organic
- Green
- Follow System

Customizable:

- editor
- menus
- tabs
- status bar
- borders
- accent
- syntax colors
- semantic indicators

Themes should be data-driven and user-editable.

## 17. Retro-futurist brand

The optional BachelorPad+ personality mode may use original appliance terminology such as:

- Semantic Filing Apparatus
- Automatic Document Identification
- Recall Engine
- Emergency Recovery System
- Cryptographic Containment
- Heavy-Duty Text Intake

A Standard and Minimal personality mode must also exist.

## 18. Main menus

```text
File
Edit
View
Insert
Format
Data
Note
Organize
Research
Security
Tools
Help
```

Menus are context-sensitive.

## 19. Default editor integration

### Windows 10/11

BachelorPad+ registers supported file types and capabilities and guides the user through supported Windows default-app selection. Do not replace or patch `notepad.exe`.

Association presets:

- Notepad Replacement
- Text + Notes
- Text + Structured Data
- Developer
- Everything Supported
- Custom

### Linux

Register a `.desktop` application and supported MIME types; use standards-based MIME/default-app mechanisms.

CLI:

```text
bachelorpad file.txt
bachelorpad --line=427 server.log
bachelorpad --help
```

**`bp_config::cli::FLAGS` is the home for what the command line accepts**, and
`--help` renders from it, so this block deliberately shows shapes rather than
a list -- a second list is a list that goes stale (ADR-0054).

Two flags this section used to show are gone, and neither was a decision when
it was written:

- `--large-file` is **deleted**. ADR-0027 and ADR-0030 make size decide how a
  document opens, from metadata, before a byte is read; a flag forcing it is
  either a no-op or a worse answer than the automatic one.
- `--readonly` is **not a flag**. Read-only is a property of a document, not
  of an invocation -- the huge-document viewer is already read-only and
  Security > Lock Document already exists. It wants a design pass, and
  `project/WORK_QUEUE.md` has it.

The `bpad` short spelling this block also showed is **kept and unbuilt**: it
is a second name for the same executable, which is something an installer
creates rather than something the product does. It belongs to phase 20 with
the rest of the artefact, and is listed there rather than here.

## 20. Cross-platform architecture

Shared Rust core with platform adapters:

- bp-platform
- bp-platform-windows
- bp-platform-linux

Windows-only services must not leak into the portable core.

## 21. Local metadata

SQLite recommended for:

- documents
- versions
- projects
- topics
- entities
- tags
- relationships
- semantic metadata
- embeddings
- filename candidates
- file events
- recovery sessions
- artifacts
- security events
- signatures
- integrity checks
- settings

Sensitive metadata follows the document security profile.

## 22. Performance targets

Targets, not guarantees.

Startup is two separately observable events, and conflating them hid a
129 ms window with a 411 ms blank interior behind a single passing number.
See ADR-0017.

- time to window < 150 ms cold — window on screen, themed, titled
- time to first interaction < 250 ms cold — content drawn, accepting keys
- warm startup < 75 ms where practical
- idle RAM < 50 MB target

Measured on the real application by `scripts/Measure-Startup.ps1`. As of
2026-08-17, with the software renderer: 36 ms to window, 19.2 MB idle. Time to
first interaction is **not yet measurable** under that renderer.

Throughput and responsiveness:

- 1 MB text: effectively instant
- 10 MB: near-instant
- 100 MB: comfortably usable
- multi-GB: usable in large-file mode
- typing latency: imperceptible
- background save: non-blocking
- core network requirement: none

## 23. Testing

Required:

- unit
- integration
- property
- fuzzing
- security tests
- performance benchmarks
- crash/recovery tests
- malformed parser inputs
- corrupted ciphertext
- wrong-passphrase and authentication-failure tests
- large-file tests
- default-association integration tests

## 24. Threat-model categories

- stolen device
- malicious files
- local-user exposure
- plaintext recovery
- cloud transmission
- secret persistence
- plugin/integration abuse
- metadata leakage
- file corruption
- external modifications

## 25. Definition of done

BachelorPad+ v1 is complete when it delivers the features above as a coherent cross-platform product while retaining the simple path:

```text
Open
  -> Type
    -> Ctrl+S
      -> Safe
```
