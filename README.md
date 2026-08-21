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

## Current state (2026-08-21)

**25 crates, 1,812 tests, green on Windows and Linux.** The app opens, edits and
saves atomically, and does rather more than that:

| Area | What works |
| --- | --- |
| Editing | Tabs, four themes plus Follow System, zoom, indentation (tabs or soft tabs, 2/4/8), honest save state, atomic save, undo/cut/copy/paste, sort / deduplicate / reverse / trim lines, duplicate and move lines, go to line |
| Storage | A rope buffer holds every document; whole-document operations are one undo step |
| Safety | Unsaved-changes prompts, external-change detection, crash recovery journal — encrypted and recovered at unlock for encrypted documents ([ADR-0022](docs/decisions/ADR-0022.md)) |
| Files | Open/Save/Save As/Save All/Save a Copy/Reload, recent files, tab context menu, command-line file opening. A document is classified by size before it is read (ADR-0027): ordinary opens as ever, large opens with the size in the status bar, and a **huge one opens too** — read from disk as you scroll, in the custom surface whichever flag you started with (ADR-0030). A 192 MiB log costs 0.8 MiB more than a small note. UTF-8, UTF-8 with BOM and **UTF-16 LE/BE** all load and save; a truncated or malformed UTF-16 file is refused by name rather than repaired with replacement characters |
| Search | Find and replace with case-sensitive, whole-word and regex toggles, changes shown before they are applied, recursive cross-file search |
| Data | JSON / JSONL / TOML / **YAML** validate, format, minify, convert; RFC 4180 CSV/TSV shape report, conversion to JSON and JSON Lines, column types. YAML refuses deep nesting, alias bombs and duplicate keys in words that say what to do ([ADR-0023](docs/decisions/ADR-0023.md)) |
| Semantic | Title, keywords, summary and outline extracted from the document; document statistics; date and time insertion |
| Clipboard | History with kind detection, paste from history, format-aware paste transformations |
| Metadata | A SQLite store with migrations, written to on every save ([ADR-0019](docs/decisions/ADR-0019.md)) |
| Organize | Related Notes, a collapsible panel of documents sharing tags with the active one; Duplicate Detection, automatic at save and on-demand ([ADR-0037](docs/decisions/ADR-0037.md)) |
| Research | Research Report, reading `bp-storage` into dominant themes, stale clusters, under-connected documents and consolidation candidates — a first insight, not the whole mode ([ADR-0041](docs/decisions/ADR-0041.md)) |
| Notebook | `bp-execution` runs a cell as a fresh subprocess — Python, PowerShell or Shell, no persistent session, no "run all" — but nothing calls it yet: there is no cell-sequence view ([ADR-0040](docs/decisions/ADR-0040.md)) |
| Encryption | `.bpadx` documents — Security ▸ Encrypt Document, unlock on open, and saves stay encrypted. Argon2id, XChaCha20-Poly1305 or AES-256-GCM, chunked with position authenticated ([ADR-0021](docs/decisions/ADR-0021.md)) |
| Security | Per-document profiles (Standard / Private / Confidential / Maximum) governing the recovery journal, clipboard history and metadata store ([ADR-0020](docs/decisions/ADR-0020.md)) |
| Security (phase 16) | Privacy Mode, a session override that can only tighten; Scan for Secrets, which reports where a credential is and never what it is; Redact Found Secrets, as an undoable edit with a consent step; Inspect Metadata; Hash Document; Verify Signature; Sign Document, whose key is sealed in a `.bpadx` envelope under a passphrase rather than protected by file permissions Windows cannot narrow ([ADR-0031](docs/decisions/ADR-0031.md)); Security History, which every row above it writes into (ADR-0024) |

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

Things that do not work, with the reason. Where the reason is "Slint
1.17.1", it was checked in the toolkit's source rather than assumed — the
citation is in `project/WORK_QUEUE.md`, and the point of it is that nobody
should spend an afternoon on these before checking the version changed.

- **Input-method composition does not work under `--editor-view`, and cannot
  on this Slint.** `FocusScope` rejects `UpdateComposition` and
  `CommitComposition` in both its handlers and exposes no callback for either;
  `TextInput` is the only item in the toolkit that consumes them. Without it
  CJK entry does not work at all, so the default stays `TextInput`. **This was
  the second of two parity conditions, and it is now a blocker rather than a
  task** -- see `project/WORK_QUEUE.md`.

- **Drag and drop to open does not work, and cannot yet.** specs §4 wants it.
  On Slint 1.17.1 the winit backend has no file-drop plumbing and
  `DataTransfer` carries only plain text or an image, so there is no channel a
  dropped file's path could arrive through. Blocked on Slint, not on effort.

- **Five features are enabled only under `--editor-view`**: duplicate and move
  line, date and time insertion, and go to line. All need the caret
  `TextInput` does not expose. [MENU_MAP.md](docs/product/MENU_MAP.md) marks
  which rows those are.

- **Three crates are built and unreachable, and this list is counted rather
  than remembered.** `bp-research`, `bp-notebook` and `bp-storage` have zero
  reverse dependencies anywhere in the application: nothing in `bp-ui`, in
  `apps/bachelorpad`, or in any other `bp-*` crate names them. `bp-notebook`
  is reached only by the fuzz harness. That is 9,556 lines defended by 241
  unit tests no user can reach, and it is still the largest thing standing
  between this repository and a product.

  | Crate | Lines | Tests | Reached by |
  | --- | --- | --- | --- |
  | `bp-research` | 5,208 | 161 | nothing |
  | `bp-notebook` | 3,627 | 59 | the fuzz harness only |
  | `bp-storage` | 721 | 21 | nothing, pending a design pass (ADR-0019) |

  **`bp-buffer`'s large-file engine came off this list**, and it was the
  largest thing on it. `SizeClass` and `Access` decide how every document is
  opened; `LargeFile` — the chunked reader — now has the view it was waiting
  for. `bp-audit` came off when Security ▸ Security History shipped, and
  `bp-integrity`, `bp-platform` and `bp-redaction` when Verify Signature, Set
  as Default Editor, redaction and the metadata inspector got rows.

  What is left is genuinely three modes rather than three menus, which is why
  it is what is left.

  Two things also stopped happening on the open path: every open used to read
  the whole file to hand six bytes to `bp_crypto::is_bpadx`, and `load` then
  read it again. A 2 GB document cost 4 GB of I/O before anything reached the
  screen.

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

- **Used in anger three times, and it paid every time.** The first pass found a
  menu bar where twelve of fourteen menus swallowed clicks, and Save As
  defaulting to the process working directory -- which wrote real documents
  into a git checkout. The second, on 2026-08-20, found a tab drawn as a single
  glyph while every column in `bp-editor` was computed as though it reached the
  next tab stop, so the caret on any tab-bearing line sat where the character
  was not. The third, on 2026-08-21, found the worst of them: typing a query
  into Find edited the document. All three are fixed -- the third's fix and
  the crash-recovery defect found the same pass are below, in "Recently
  closed".

  **All three lived in the same seam, and it is the one a test in this
  repository cannot see: what a toolkit does with the string it is handed.**
  1,754 tests did not find the third either -- `AppState::find` was correct
  throughout, and every test of it passed. The defect was entirely in who
  held the focus afterwards.

  Still unclicked, in the order they now matter: **the wheel** and **a window
  resize** -- both inherited by the huge-file viewer, which does nothing but
  scroll, so a wheel going the wrong way there is not a papercut but the
  feature being broken; **reading an enormous document**, which has been
  rendered and measured but not read; **signing one**, which is new and has
  never been clicked; drag-to-select; and **File ▸ Set as Default Editor**,
  the one row that changes state outside this application.
  [NEXT_SESSION.md](project/NEXT_SESSION.md) has the checklist and a four-line
  recipe for a 192 MiB fixture.

### Recently closed, and what each one cost to learn

Kept rather than deleted, because every one of these went stale the same
way — a fix landing without the record moving — and because the lesson in each
is worth more than the fact.

- **Find no longer edits the document while you type the query.** The third
  manual pass, on 2026-08-21, found the worst defect any of the three found:
  Ctrl+F, then type `replicas`, and the `r` reached the find box correctly
  (`1 of 3`), but every character after it went **into the document**, over
  the match the `r` had just selected -- `alpha replicas beta` became
  `alpha eeplicas beta`, the tab went dirty, and the find box still read `r`.
  Reproduced with 1.5 seconds between keystrokes, so it was not a race.

  The cause was one line with a comment that said what it was doing.
  `on_find_changed` ([lib.rs](crates/bp-ui/src/lib.rs)) runs on **every**
  keystroke in the query box and called `dispatch::select`, which ends in
  `editor.focus()` -- "Just take the focus back from the find box", says
  `select-range` in `ui/app.slint`. That is right for Find Next, the other
  caller, where the user does want the caret in the document afterwards. It
  was wrong for the preview that runs while they are still typing the query,
  and the two shared one function.

  **The fix is the split the toolkit's own comment implied it needed**: a
  second, focus-preserving path -- `dispatch::preview_match` in Rust,
  `preview-range` in `ui/app.slint` -- used only by the live-typing preview,
  leaving the three deliberate "jump to it" callers (Find Next/Previous, Go to
  Line, a cross-file search result) on the original focus-stealing one. Both
  the `TextInput` and `--editor-view` branches got the same treatment; the
  first pass's diagnosis of the `--editor-view` half was by reading the code,
  not by driving it, and driving it afterward found nothing further wrong.

  Two agents built this in parallel with an unrelated fix below, in files
  that never overlapped; one of them hit the exact file-collision this
  repository's own working notes warn about mid-run, from the other agent's
  concurrent edits, and caught it itself with an isolated worktree rather
  than reporting a false pass. Confirmed by driving the window again
  afterward: four clean runs against the exact sequence that broke it, at
  both typing speeds.

- **Crash recovery restores the text now, and the encoding and line ending
  with it.** Found in the same pass. `AppState::restore` opened the document
  and inserted the checkpoint's text, but never called `set_line_ending` or
  `set_encoding` the way `open` does, so a recovered document fell back to
  `LineEnding::default()` -- CRLF on Windows. An LF file recovered on Windows
  was rewritten CRLF throughout on the next save, and a UTF-16 document came
  back as UTF-8 -- silently contradicting a test that was already green,
  `saving_a_mixed_document_does_not_rewrite_the_minority_line_break`.

  `bp_history::Checkpoint` now carries `encoding` and `line_ending`,
  `#[serde(default)]` so a journal already on disk still deserializes.
  Recovery re-detects a missing line ending from the text itself, the way the
  rest of the codebase guesses when certainty is not available; a missing
  encoding cannot be recovered the same way, because the checkpoint holds
  already-decoded text, so it falls back to a documented guess rather than a
  claim.

- **A 2 GB file opens, and costs 0.8 MiB.** The last piece of phase 4
  ([ADR-0030](docs/decisions/ADR-0030.md)). A document past the huge threshold
  is served from disk as you scroll: arrow keys, the page keys, Ctrl+Home and
  the wheel move the window, the gutter numbers the *document* rather than the
  screen, and the status bar says which lines are on screen. Measured rather
  than asserted — a few lines of text peaked at 31.3 MiB and a 192 MiB log at
  32.1 MiB.

  It draws in the custom surface in **every** build, not only under
  `--editor-view`. `TextInput` owns its own text and cannot be handed a window
  of a file it does not have; the flag stays a statement about which surface
  *edits* a document the rope holds, and the one reason it is opt-in —
  input-method composition — has nothing to say about a surface that accepts
  no text.

  Save, Save As, Save a Copy and Reload grey with the reason. That is not
  politeness: `text_of` such a document is the empty string, so a Ctrl+S that
  merely did nothing special would write an empty file over two gigabytes and
  report success. Four paths refuse by name, and a test asserts the file's size
  is unchanged rather than trusting the return value.

  **Two things are deliberately not built**, and each is a refusal with a
  reason rather than a gap: Ctrl+End, and a total line count. Both need the
  whole file indexed, which for these documents means reading two gigabytes to
  answer one question while the window is frozen.

- **Signing works, and the key is sealed rather than protected.**
  [ADR-0031](docs/decisions/ADR-0031.md). ADR-0026 had measured a hole it
  could not close: `0600` on Linux, nothing at all on Windows, where narrowing
  a DACL needs Win32 and `unsafe` that `bp-platform` forbids — so
  `is_confirmed_private()` honestly reported "unknown" on half the supported
  platforms. Putting the key inside the envelope encrypted documents already
  use protects the *contents* instead, identically on both platforms, and
  designs nothing new.

  **One ceremony, not one per signature.** The first signature creates the
  key, because what was asked for was a signature; the second finds it and
  asks only to unlock it. The row says which the click will do before you
  click it.

  It still greys, for two reasons that are not about key storage: a signature
  is over the bytes **on disk**, so a document that has never been saved has
  nothing to sign, and one with unsaved changes would get a valid signature
  over the *previous* version — worse than a refusal, because it verifies.

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

- **Word wrap now works under `--editor-view`.** A document line can occupy
  several visual rows: `bp_editor::wrap` decides where they break, the view
  maps rows to characters, Up and Down move by row rather than by line, and
  scrolling is anchored to a line *and* a row within it so a line taller than
  the window can be scrolled through.

- **The crates are tested where they meet, and that is where the defects
  are.** Eleven files of cross-crate tests join load/rope/atomic save, the
  `.bpadx` envelope over a real file, the security profile against all three
  of its dependants, format detection against the parser that then handles
  the file, search against the buffer, journal recovery, the notebook through
  the file layer, the audit log, redaction, signing, and the filename grammar
  against the platform's own rules. Between them they have found seven
  defects that no unit test did. **All seven are fixed**, and there is no
  `#[ignore]`d test left anywhere in the tree. The most serious was
  `bp-naming`, which asked whether a *whole* sanitised title was a Windows
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
