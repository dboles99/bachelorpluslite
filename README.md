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

## Current state (2026-08-20)

**24 crates, 1,686 tests, green on Windows and Linux.** The app opens, edits and
saves atomically, and does rather more than that:

| Area | What works |
| --- | --- |
| Editing | Tabs, four themes plus Follow System, zoom, indentation (tabs or soft tabs, 2/4/8), honest save state, atomic save, undo/cut/copy/paste, sort / deduplicate / reverse / trim lines, duplicate and move lines, go to line |
| Storage | A rope buffer holds every document; whole-document operations are one undo step |
| Safety | Unsaved-changes prompts, external-change detection, crash recovery journal — encrypted and recovered at unlock for encrypted documents ([ADR-0022](docs/decisions/ADR-0022.md)) |
| Files | Open/Save/Save As/Save All/Save a Copy/Reload, recent files, tab context menu, command-line file opening. UTF-8, UTF-8 with BOM and **UTF-16 LE/BE** all load and save; a truncated or malformed UTF-16 file is refused by name rather than repaired with replacement characters |
| Search | Find and replace with case-sensitive, whole-word and regex toggles, changes shown before they are applied, recursive cross-file search |
| Data | JSON / JSONL / TOML / **YAML** validate, format, minify, convert; RFC 4180 CSV/TSV shape report, conversion to JSON and JSON Lines, column types. YAML refuses deep nesting, alias bombs and duplicate keys in words that say what to do ([ADR-0023](docs/decisions/ADR-0023.md)) |
| Semantic | Title, keywords, summary and outline extracted from the document; document statistics; date and time insertion |
| Clipboard | History with kind detection, paste from history, format-aware paste transformations |
| Metadata | A SQLite store with migrations — built and tested, not yet wired in ([ADR-0019](docs/decisions/ADR-0019.md)) |
| Encryption | `.bpadx` documents — Security ▸ Encrypt Document, unlock on open, and saves stay encrypted. Argon2id, XChaCha20-Poly1305 or AES-256-GCM, chunked with position authenticated ([ADR-0021](docs/decisions/ADR-0021.md)) |
| Security | Per-document profiles (Standard / Private / Confidential / Maximum) governing the recovery journal, clipboard history and metadata store ([ADR-0020](docs/decisions/ADR-0020.md)) |
| Security (phase 16) | Privacy Mode, a session override that can only tighten; Scan for Secrets, which reports where a credential is and never what it is; Redact Found Secrets, as an undoable edit with a consent step; Inspect Metadata; Hash Document; Verify Signature; Security History, which every row above it writes into (ADR-0024). Signing is greyed with the reason on the row — there is nowhere to keep a key yet |

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

- **Somebody has now typed into the custom editor view, and it works.**
  `--editor-view` gives Ln/Col, our own undo, and a view that draws only the
  lines on screen. A person drove it on 2026-08-20: keys arrive, Ctrl+S saves
  (checked by reading the file back off the disk), and **the caret tracks the
  pointer** -- a click at the end of `line 30` reported `Ln 30, Col 8`,
  exactly its length plus one. The wheel direction, drag-to-select and
  resize behaviour are still unchecked. See [ADR-0018](docs/decisions/ADR-0018.md).

  **That pass found a defect 177 tests could not**, and it is fixed: a tab
  was drawn as a single glyph while every column in `bp-editor` was computed
  as though it reached the next tab stop, so the caret on any tab-bearing
  line sat where the character was not. `view::expand_tabs` and
  `VisualRow::display_text` close it -- the row keeps its characters for the
  arithmetic, and the toolkit is handed the appearance.

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
- **The crates are tested where they meet, and that is where the defects
  are.** Eleven files of cross-crate tests join load/rope/atomic save, the
  `.bpadx` envelope over a real file, the security profile against all three
  of its dependants, format detection against the parser that then handles
  the file, search against the buffer, journal recovery, the notebook through
  the file layer, the audit log, redaction, signing, and the filename grammar
  against the platform's own rules. Between them they have found seven
  defects that no unit test did. **Five are fixed.** The most serious is the
  newest: `bp-naming` asked whether a *whole* sanitised title was a Windows
  device name, while Win32 asks only about the stem before the first dot --
  so `con.txt`, `aux.log` and `NUL.dat` passed untouched, and on Windows
  saving to one of those writes **the device**, reports success, and the
  document is gone. The sanitiser now tests the stem, its list gained
  `CONIN$` and `CONOUT$`, and a property compares its verdict against
  `bp-platform`'s stem by stem in both directions so the two lists cannot
  drift apart quietly.

  **The other half of that hole was a name nobody sanitised.** `bp-naming`
  defends the name the product *suggests*; the one a user types over the top
  of it in a Save dialog had never been checked by anything. `bp-files`
  depends on `bp-platform` now and `atomic_write` refuses a device name
  outright -- the only refusal in that module that is checked rather than
  attempted, because attempting it does not fail: the open succeeds, the
  read-back verifies against the console, and the status bar reports a
  document that is nowhere. The platform is a parameter rather than a `cfg`,
  so `con.txt` stays an ordinary file on Linux and both CI legs execute both
  rule sets.

  **There is no `#[ignore]`d test left anywhere in the tree.** All seven are
  closed. The last was a decision rather than a defect and is now
  [ADR-0029](docs/decisions/ADR-0029.md): a line break in this product is
  `
` or `
` and nothing else, so `bp-buffer` builds its rope without
  ropey's `unicode_lines` -- a break set that was never chosen, only
  inherited from writing `ropey = "1"`, and that disagreed with
  `bp-core::LineEnding`, `bp-files::encode`, `bp-search` and two of
  `bp-buffer`'s own helpers. Find-in-files and the caret now agree on every
  document this product can save, and the status bar reports one line count
  rather than a different one per editor view.

- **Three crates and one module are built and unreachable, and this list is
  now counted rather than remembered.** `bp-research`, `bp-notebook` and
  `bp-storage` have zero reverse dependencies anywhere in the application:
  nothing in `bp-ui`, in `apps/bachelorpad`, or in any other `bp-*` crate
  names them. `bp-notebook` is reached only by the fuzz harness. `bp-audit`
  came off this list when Security ▸ Security History shipped. Add `bp-buffer`'s
  large-file engine -- depended on, but `LargeFile` and `SizeClass` appear
  nowhere outside their own crate, so opening a 2 GB file still loads 2 GB --
  and it is over ten thousand lines defended by 237 unit tests that no user
  can reach. It is still the largest thing standing between this
  repository and a product.

  | Crate | Lines | Tests | Reached by |
  | --- | --- | --- | --- |
  | `bp-research` | 5,208 | 161 | nothing |
  | `bp-notebook` | 3,514 | 55 | the fuzz harness only |
  | `bp-buffer` ▸ `large.rs` | 1,198 | — | nothing |
  | `bp-storage` | 721 | 21 | nothing, by decision (ADR-0019) |

  `bp-integrity`, `bp-platform` and `bp-redaction` came off this list --
  Verify Signature, Set as Default Editor, redaction and the metadata
  inspector all have rows now.
- **Still barely used in anger.** The tests cover the pieces and now the
  seams; a person driving the application has done so once, several sessions
  ago. The four rows added to the Security menu this session have never been
  clicked.
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
