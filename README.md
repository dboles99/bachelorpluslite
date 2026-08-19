# BachelorPad+

**BachelorPad+** is a Rust-native semantic text-processing appliance for **Windows 10, Windows 11, and Linux**.

It begins as a fast Notepad-like editor and progressively adds:

- semantic filenames using `Title_DDMMMYYYY.ext`
- full-path and last-save visibility
- rope-based editing and huge-file / memory-mapped modes
- Markdown, YAML, JSON, JSONL/NDJSON, TOML, CSV/TSV, XML and source-code awareness
- semantic organization, related notes, duplicate detection and hybrid search
- Jupyter-compatible notebook behavior and mixed-language execution
- clipboard history and format-aware paste operations
- Light, Dark, Organic and Green themes
- strong local security, encrypted `.bpadx` notes, secure recovery, privacy profiles and secret scanning
- Windows/Linux default-editor integration
- original retro-futurist "text appliance" branding

The product philosophy is:

> **Notepad when you want it. More when you need it.**

## Install the repository scaffold

1. Put the release ZIP and `Install-BachelorPadPlusRepo.ps1` in your Windows Downloads folder.
2. Open PowerShell 7.
3. Run:

```powershell
Set-ExecutionPolicy -Scope Process Bypass
.\Install-BachelorPadPlusRepo.ps1
```

Default destination:

```text
G:\dev2\bachelorpadplus_rust
```

The installer is conservative: it will not silently overwrite an existing repository. It creates a timestamped backup if you explicitly use `-BackupExisting`.

## Development

```powershell
./scripts/Install-GitHooks.ps1   # once per clone
./scripts/Invoke-LocalCI.ps1     # format, check, clippy, test
```

This project has **no hosted CI**. `scripts/Invoke-LocalCI.ps1` is the
authoritative gate and runs from `pre-commit` and `pre-push`. See
[docs/governance/LOCAL_CI.md](docs/governance/LOCAL_CI.md) and ADR-0016.

```powershell
cargo run --release            # the app
cargo run --release -- --editor-view   # with the custom editor view (ADR-0018)
./scripts/Measure-Startup.ps1  # startup and idle memory vs specs.md §22
```

## Current state (2026-08-19)

**17 crates, 700 tests, green on Windows and Linux.** The app opens, edits and
saves atomically, and does rather more than that:

| Area | What works |
| --- | --- |
| Editing | Tabs, four themes plus Follow System, zoom, indentation (tabs or soft tabs, 2/4/8), honest save state, atomic save, undo/cut/copy/paste, sort / deduplicate / reverse / trim lines, duplicate and move lines, go to line |
| Storage | A rope buffer holds every document; whole-document operations are one undo step |
| Safety | Unsaved-changes prompts, external-change detection, crash recovery journal — encrypted and recovered at unlock for encrypted documents ([ADR-0022](docs/decisions/ADR-0022.md)) |
| Files | Open/Save/Save As/Save All/Save a Copy/Reload, recent files, tab context menu, command-line file opening |
| Search | Find and replace with case-sensitive, whole-word and regex toggles, changes shown before they are applied, recursive cross-file search |
| Data | JSON / JSONL / TOML validate, format, minify, convert; RFC 4180 CSV/TSV shape report, conversion to JSON and JSON Lines, column types |
| Semantic | Title, keywords, summary and outline extracted from the document; document statistics; date and time insertion |
| Clipboard | History with kind detection, paste from history, format-aware paste transformations |
| Metadata | A SQLite store with migrations — built and tested, not yet wired in ([ADR-0019](docs/decisions/ADR-0019.md)) |
| Encryption | `.bpadx` documents — Security ▸ Encrypt Document, unlock on open, and saves stay encrypted. Argon2id, XChaCha20-Poly1305 or AES-256-GCM, chunked with position authenticated ([ADR-0021](docs/decisions/ADR-0021.md)) |
| Security | Per-document profiles (Standard / Private / Confidential / Maximum) governing the recovery journal, clipboard history and metadata store ([ADR-0020](docs/decisions/ADR-0020.md)) |

Startup, with the software renderer ([ADR-0017](docs/decisions/ADR-0017.md)):
**35.7 ms to window, 21.9 MB idle**, against targets of 150 ms and 50 MB.
Idle memory rose ~2.7 MB this session; `std-widgets` is imported now, for the
one thing that reports the desktop's light/dark preference.

Configuration precedence: command line (`--theme=`, `--renderer=`, `--log=`,
`--font-size=`, `--tab-width=`, `--indent-spaces=`),
environment (`BACHELORPAD_*`), a TOML file
(`%APPDATA%\bachelorpad\config.toml`, or `$XDG_CONFIG_HOME` on Linux), then
defaults. Broken config warns and falls back; it never stops the editor
starting.

### Known gaps

- **Nobody has typed into the custom editor view.** `--editor-view` gives
  Ln/Col, our own undo, and a view that draws only the lines on screen. Its
  rules are covered by 147 tests in `bp-editor`; the widget has been shown to
  render a frame with a real file open and exit cleanly, and nothing beyond
  that. Whether the caret lands under the pointer is not a thing a test can
  say. Duplicate Line and Move Line Up/Down are enabled only there, so they
  inherit the answer. See [ADR-0018](docs/decisions/ADR-0018.md).
- **Input-method composition does not work under `--editor-view`, and cannot
  on this Slint.** `FocusScope` rejects `UpdateComposition` and
  `CommitComposition` in both its handlers and exposes no callback for either;
  `TextInput` is the only item in the toolkit that consumes them. Without it
  CJK entry does not work at all, so the default stays `TextInput`. **This was
  the second of two parity conditions, and it is now a blocker rather than a
  task** -- see `project/WORK_QUEUE.md`.
- **Word wrap now works under `--editor-view`.** A document line can occupy
  several visual rows: `bp_editor::wrap` decides where they break, the view
  maps rows to characters, Up and Down move by row rather than by line, and
  scrolling is anchored to a line *and* a row within it so a line taller than
  the window can be scrolled through.
- **Drag and drop to open does not work, and cannot yet.** specs §4 wants it.
  On Slint 1.17.1 the winit backend has no file-drop plumbing and
  `DataTransfer` carries only plain text or an image, so there is no channel a
  dropped file's path could arrive through. Blocked on Slint, not on effort.
- **Five features are enabled only under `--editor-view`**: duplicate and move
  line, date and time insertion, and go to line. All need the caret
  `TextInput` does not expose. [MENU_MAP.md](docs/product/MENU_MAP.md) marks
  which rows those are.
- **Little of this has been used in anger.** The tests cover the pieces in
  isolation; the integrated behaviour has had one manual pass, several
  sessions ago.
- **`bp-storage` is still not called by the application.** That is now a
  product decision rather than a security one: ADR-0020 permits recording a
  summary under Standard, and `record_document` honours the policy. What
  remains is ADR-0019's judgement plus a design pass.
- **A plaintext document under Private or Confidential gets no crash
  recovery.** The journal for those profiles is sealed with the document's
  own passphrase ([ADR-0022](docs/decisions/ADR-0022.md)), so a document
  that is not encrypted has no key to use. The status bar says what fixes
  it: encrypt the document.
- **Recovery for an encrypted document is invisible until you open it.** Its
  journal is filed under a digest of its path and can only be read once you
  have unlocked the document, so nothing prompts at startup — deliberately.

[ROADMAP.md](ROADMAP.md) has per-phase status;
[MENU_MAP.md](docs/product/MENU_MAP.md) says which menu rows are real;
[ARCHITECTURE.md](docs/architecture/ARCHITECTURE.md) has the crate map and the
editor-view constraint; [WORK_QUEUE.md](project/WORK_QUEUE.md) lists what is
ready to take and what cannot run in parallel;
[NEXT_SESSION.md](project/NEXT_SESSION.md) is the plan for picking this up
again.

## Governance hierarchy

Where logical and useful, work follows:

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

See `docs/governance/WORK_MODEL.md`.
