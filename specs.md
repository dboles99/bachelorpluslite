# BachelorPad+ v1 Master Specification

## 1. Product definition

BachelorPad+ is a cross-platform Rust-native semantic text-processing appliance for:

- Windows 10
- Windows 11
- Linux

It must remain usable as a very fast plain-text editor when all semantic, notebook, research, clipboard-history, and cloud-connected features are disabled.

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
- Research Note

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

Required:

- optional notebook mode
- plain text, Markdown, Python, Rust, PowerShell, SQL, Shell and Raw cells
- run selection
- run cell / all / above / below
- stop execution
- split/merge/move/duplicate/collapse cells
- mixed-language notebooks
- Jupyter `.ipynb` import/export
- raw notebook JSON view
- output types: text, tables, JSON, HTML, images, charts, errors, files

Security rule: never auto-run an opened or pasted notebook/cell.

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

Context menu includes:

- Paste
- Paste Special
- Clipboard History
- Windows native clipboard history where available
- BachelorPad+ local clipboard panel
- pinned items
- search
- type detection
- format-aware paste transformations

Potential types:

- plain text
- Markdown
- JSON
- YAML
- URL
- path
- code
- structured data

Persistent clipboard history is opt-in.

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
- secure clipboard
- timed clipboard clearing
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
- notebook cell colors
- semantic indicators

Themes should be data-driven and user-editable.

## 17. Retro-futurist brand

The optional BachelorPad+ personality mode may use original appliance terminology such as:

- Semantic Filing Apparatus
- Automatic Document Identification
- Recall Engine
- Clipboard Retention Chamber
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
Notebook
Organize
Research
Run
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
bpad file.txt
bachelorpad --line 427 server.log
bachelorpad --readonly huge.log
bachelorpad --large-file dataset.jsonl
```

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
- clipboard items
- notebook cells
- artifacts
- security events
- signatures
- integrity checks
- settings

Sensitive metadata follows the document security profile.

## 22. Performance targets

Targets, not guarantees:

- cold startup < 150 ms where practical
- warm startup < 75 ms where practical
- idle RAM < 50 MB target
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
- clipboard leakage
- plaintext recovery
- cloud transmission
- secret persistence
- notebook code execution
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
