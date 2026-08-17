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
cargo run --release          # the app
./scripts/Measure-Startup.ps1  # startup and idle memory vs specs.md §22
```

## Current state (2026-08-17)

**14 crates, 327 tests, green on Windows and Linux.** The app opens, edits and
saves atomically, and does rather more than that:

| Area | What works |
| --- | --- |
| Editing | Tabs, four themes, honest save state, atomic save, undo/cut/copy/paste, sort / deduplicate / reverse / trim lines |
| Safety | Unsaved-changes prompts, external-change detection, crash recovery journal |
| Files | Open/Save/Save As/Save All/Reload, recent files, command-line file opening |
| Search | Find and replace (literal or regex) with the changes shown before they are applied, recursive cross-file search |
| Data | JSON / JSONL / TOML validate, format, minify, convert; RFC 4180 CSV/TSV shape report |
| Semantic | Title, keywords, summary and outline extracted from the document |
| Clipboard | History with kind detection, paste from history |

Startup, with the software renderer ([ADR-0017](docs/decisions/ADR-0017.md)):
**36 ms to window, 19.2 MB idle**, against targets of 150 ms and 50 MB.

Configuration precedence: command line (`--theme=`, `--renderer=`, `--log=`),
environment (`BACHELORPAD_*`), a TOML file
(`%APPDATA%\bachelorpad\config.toml`, or `$XDG_CONFIG_HOME` on Linux), then
defaults. Broken config warns and falls back; it never stops the editor
starting.

### Known gaps

- **Cursor position shows a line count, not Ln/Col.** Slint's `TextInput`
  owns the caret and exposes it only through an internal, undocumented
  property. This also blocks the rope buffer becoming the editor's storage and
  our own undo becoming the authority — all three need a custom editor view.
- **Little of this has been used in anger.** The tests cover the pieces in
  isolation; the integrated behaviour has had one manual pass.
- **Two features hold sensitive data without a security profile governing
  them**: the recovery journal writes unsaved text to disk in plaintext, and
  clipboard history keeps copied secrets in memory. Both are waiting on
  phase 14.

[ROADMAP.md](ROADMAP.md) has per-phase status;
[ARCHITECTURE.md](docs/architecture/ARCHITECTURE.md) has the crate map and the
editor-view constraint; [NEXT_SESSION.md](project/NEXT_SESSION.md) is the plan
for picking this up again.

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
