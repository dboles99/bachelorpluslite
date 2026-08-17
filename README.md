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

Current state: the app opens, edits, and saves atomically on Windows and
Linux. The core model (`bp-core`), filename grammar (`bp-naming`), atomic save
path (`bp-files`), themes (`bp-theme`) and the Slint shell (`bp-ui`) are
implemented and tested — 103 tests, green on both platforms.

Startup, with the software renderer ([ADR-0017](docs/decisions/ADR-0017.md)):
**36 ms to window, 19.2 MB idle**, against targets of 150 ms and 50 MB.

Configuration comes from, in precedence order: command line
(`--theme=`, `--renderer=`, `--log=`), environment (`BACHELORPAD_*`), a TOML
file (`%APPDATA%\bachelorpad\config.toml`, or `$XDG_CONFIG_HOME` on Linux),
then defaults. Broken config warns and falls back; it never stops the editor
starting.

Known gaps: keyboard shortcut delivery is wired but unverified, cursor
position shows a line count rather than Ln/Col, and text is held in a `String`
rather than a rope (measured fine to ~40 MB). See
[project/tasks/01-foundation/](project/tasks/01-foundation/).

The crate map — including the ~22 crates not yet created — is in
[ARCHITECTURE.md](docs/architecture/ARCHITECTURE.md).

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
