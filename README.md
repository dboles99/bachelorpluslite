# BachelorPlusLite

**BachelorPlusLite** is a lightweight text editor for **Windows 10, Windows 11
and Linux** — a Notepad clone, and deliberately little more.

> **Derived from BachelorPad+.** BachelorPlusLite began as a clone of the
> [BachelorPad+](https://github.com/dboles99/bachelorpadplus_rust) repository,
> and it keeps that project's whole history, its licences and its attribution.
> **The reduction has started, and it is scoped.** Executing anything, and
> notebooks with it, were removed on 2026-08-29
> ([ADR-0057](docs/decisions/ADR-0057.md)). The target is the feature set of
> Windows 11 Notepad plus a note-organising layer — semantic filenames,
> related notes, duplicate detection and what the store knows — which stays
> on purpose.
>
> [ADR-0059](docs/decisions/ADR-0059.md) says what else leaves and in what
> order: research and citations, clipboard history, the structured-data
> operations, huge-file mode, and the security stack — eight crates, taking
> this from 67,316 lines of Rust to roughly 40,000. **What stays is the
> profile model**, narrowed to the two axes that govern something: whether a
> recovery journal is written, and whether anything is recorded about a
> document at all. A smaller editor should not know more about you than the
> larger one did. **The list below still describes more than the target**;
> each line leaves as the capability behind it does.

## What this repository contains

It begins as a fast Notepad-like editor and adds:

- semantic filenames using `Title_DDMMMYYYY.ext`
- full-path and last-save visibility
- rope-based editing and huge-file / memory-mapped modes
- Markdown, YAML, JSON, JSONL/NDJSON, TOML, CSV/TSV, XML and source-code awareness
- semantic organization, related notes, duplicate detection and hybrid search
- clipboard history and format-aware paste operations
- Light, Dark, Organic and Green themes
- strong local security, encrypted `.bpadx` notes, secure recovery, privacy profiles and secret scanning
- Windows/Linux default-editor integration
- original retro-futurist "text appliance" branding

The product philosophy is:

> **Notepad when you want it. More when you need it.**

## Install the repository scaffold

Inherited from BachelorPad+, and kept because the scaffold is the same one.
The installer ships with a BachelorPad+ release; to work on BachelorPlusLite
you clone this repository instead.

1. Put the release ZIP and `Install-BachelorPadPlusRepo.ps1` in your Windows Downloads folder.
2. Open PowerShell 7.
3. Run:

```powershell
Set-ExecutionPolicy -Scope Process Bypass
.\Install-BachelorPadPlusRepo.ps1
```

Default destination:

```text
G:\dev2\bachelorpluslite
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
cargo run --release -- --help  # every flag it accepts
cargo run --release -- --editor-view   # with the custom editor view (ADR-0018)
./scripts/Measure-Startup.ps1  # startup and idle memory vs specs.md §22
./scripts/New-Release.ps1 -Linux       # archives for both targets, with checksums
```

`New-Release.ps1` stages the binary with both licence texts and a `BUILD.txt`
naming the commit, archives it (`.zip` for Windows, `.tar.gz` for Linux, built
inside WSL so the executable bit and the `bpad` symlink survive), and writes a
`SHA256SUMS.txt` that `sha256sum -c` reads. **The archives are unsigned and
the script says so on every run** — signing is deferred and self-signing
refused, for the reason in [ADR-0055](docs/decisions/ADR-0055.md).

## Current state (2026-08-23)

**25 crates, 1,986 tests, green on Windows and Linux.** **Every menu row either does something or is a readout** ([ADR-0048](docs/decisions/ADR-0048.md)) — nothing anywhere says "not implemented yet" any more. The app opens, edits and
saves atomically, and does rather more than that:

| Area | What works |
| --- | --- |
| Editing | Tabs, four themes plus Follow System, zoom, indentation (tabs or soft tabs, 2/4/8), honest save state, atomic save, undo/cut/copy/paste, sort / deduplicate / reverse / trim lines, duplicate and move lines, **go to line in either surface** |
| Storage | A rope buffer holds every document; whole-document operations are one undo step |
| Safety | Unsaved-changes prompts, external-change detection, crash recovery journal — encrypted and recovered at unlock for encrypted documents ([ADR-0022](docs/decisions/ADR-0022.md)) |
| Releases | `scripts/New-Release.ps1 -Linux` produces a `.zip` and a `.tar.gz` carrying the binary, both licences, a `bpad` short spelling and a `BUILD.txt` naming the commit, plus a `SHA256SUMS.txt`. Unsigned, and it says so ([ADR-0055](docs/decisions/ADR-0055.md)) |
| Command line | `bachelorpad notes.txt` opens files; `--line=427 server.log` opens one already scrolled to the line, highlighted, in either editor surface; `--help` lists all thirteen flags and `--version` names the product, its version and its licence ([ADR-0054](docs/decisions/ADR-0054.md)). A mistyped flag, a flag missing its value and a switch given one are each reported at startup rather than discarded |
| Files | Open/Save/Save As/Save All/Save a Copy/Reload, recent files, tab context menu, command-line file opening. A document is classified by size before it is read (ADR-0027): ordinary opens as ever, large opens with the size in the status bar, and a **huge one opens too** — read from disk as you scroll, in the custom surface whichever flag you started with (ADR-0030). A 192 MiB log costs 0.8 MiB more than a small note. UTF-8, UTF-8 with BOM and **UTF-16 LE/BE** all load and save; a truncated or malformed UTF-16 file is refused by name rather than repaired with replacement characters |
| Search | Find and replace with case-sensitive, whole-word and regex toggles, changes shown before they are applied, recursive cross-file search. **Find works in a document too large to hold too** ([ADR-0042](docs/decisions/ADR-0042.md)): the find bar scans it from disk a window at a time, says `searching 62%` rather than claiming a total it cannot know, then jumps to the hit and highlights it. Measured on a 213.5 MiB log — `1 of 1` at line 4,800,001, and `1 of 500+` when a query matches more than the cap |
| Data | JSON / JSONL / TOML / **YAML** validate, format, minify, convert; RFC 4180 CSV/TSV shape report, conversion to JSON and JSON Lines, column types. YAML refuses deep nesting, alias bombs and duplicate keys in words that say what to do ([ADR-0023](docs/decisions/ADR-0023.md)) |
| Semantic | Title, keywords, summary and outline extracted from the document; document statistics; date and time insertion |
| Clipboard | History with kind detection, paste from history, format-aware paste transformations |
| Metadata | A SQLite store with migrations, written to on every save ([ADR-0019](docs/decisions/ADR-0019.md)) |
| Organize | Related Notes, a collapsible panel of documents sharing tags with the active one; Duplicate Detection, automatic at save and on-demand ([ADR-0037](docs/decisions/ADR-0037.md)) |
| Research | Research Report, reading `bp-storage` into dominant themes, stale clusters, under-connected documents and consolidation candidates ([ADR-0041](docs/decisions/ADR-0041.md)) — each insight **naming the documents it is drawn from**, and closing with a section that states every threshold it applied and every number it applied them over, including the truncation it used to leave silent ([ADR-0046](docs/decisions/ADR-0046.md)). Plus **Open Questions**, every question the document asks, at the line it begins on. And **What the Store Holds**, which says what the store has recorded about you and that it never holds the text of a document. The three citation rows that used to sit here left with `bp-research` ([ADR-0060](docs/decisions/ADR-0060.md)) |
| Tools | **Document Inspector** (what is in the document), **Security Inspector** (the policy in force on all seven axes — the Security menu shows three), **File Analysis** (the file, which is a different object from the document) and **Configuration**, which is read-only and says so ([ADR-0048](docs/decisions/ADR-0048.md)) |
| Encryption | `.bpadx` documents — Security ▸ Encrypt Document, unlock on open, and saves stay encrypted. Argon2id, XChaCha20-Poly1305 or AES-256-GCM, chunked with position authenticated ([ADR-0021](docs/decisions/ADR-0021.md)) |
| Security | Per-document profiles (Standard / Private / Confidential / Maximum) governing the recovery journal, clipboard history and metadata store ([ADR-0020](docs/decisions/ADR-0020.md)) |
| Security (phase 16) | Privacy Mode, a session override that can only tighten; Scan for Secrets, which reports where a credential is and never what it is; Redact Found Secrets, as an undoable edit with a consent step; Inspect Metadata; Hash Document; Verify Signature; Sign Document, whose key is sealed in a `.bpadx` envelope under a passphrase rather than protected by file permissions Windows cannot narrow ([ADR-0031](docs/decisions/ADR-0031.md)); Security History, which every row above it writes into (ADR-0024) |

Startup, with the software renderer ([ADR-0017](docs/decisions/ADR-0017.md)):
**35.7 ms to window, 21.9 MB idle**, against targets of 150 ms and 50 MB.
Idle memory rose ~2.7 MB this session; `std-widgets` is imported now, for the
one thing that reports the desktop's light/dark preference.

Configuration precedence: command line, environment (`BACHELORPAD_*`), a TOML
file (`%APPDATA%\bachelorpad\config.toml`, or `$XDG_CONFIG_HOME` on Linux),
then defaults. Broken config warns and falls back; it never stops the editor
starting.

**`bachelorpad --help` lists every flag**, which is why this paragraph no
longer does ([ADR-0054](docs/decisions/ADR-0054.md)). A flag this product does
not accept is now reported rather than dropped -- `--font_size=20` says so
instead of doing nothing.


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

- **Four features are enabled only under `--editor-view`**: duplicate and move
  line, and date and time insertion. They need to know *where the caret is*,
  and `TextInput` will not say. [MENU_MAP.md](docs/product/MENU_MAP.md) marks
  which rows those are.

  **Go to Line left this list on 2026-08-22, and the reason it was on it is
  worth keeping.** It was recorded as needing the caret, like the others. It
  does not: it needs the caret *moved*, which `set-selection-offsets` does and
  which Find Next has always relied on. Reading and moving turned out to be
  different requirements filed under one word. What kept the mistake alive was
  that the move used to be invisible — nothing scrolled the viewport to the
  caret, so a correct jump looked like nothing happening — and fixing that
  (see "Find did not scroll to its match", below) is what made the claim
  checkable at all.

- **No crate is unreachable.** This list was three crates long four sessions
  ago and is now empty: `bp-notebook` and `bp-execution` left it under
  [ADR-0043](docs/decisions/ADR-0043.md), `bp-research` under
  [ADR-0044](docs/decisions/ADR-0044.md). Two of the three have since left the
  workspace altogether ([ADR-0057](docs/decisions/ADR-0057.md)), which is a
  second way for the count to reach zero and not the same claim — the heading
  is kept because the count is worth being able to check rather than remember,
  and because *deleted* and *wired in* have to stay tellable apart.
- **A plaintext document under Private or Confidential gets no crash
  recovery.** The journal for those profiles is sealed with the document's
  own passphrase ([ADR-0022](docs/decisions/ADR-0022.md)), so a document
  that is not encrypted has no key to use. The status bar says what fixes
  it: encrypt the document.

- **Recovery for an encrypted document is invisible until you open it.** Its
  journal is filed under a digest of its path and can only be read once you
  have unlocked the document, so nothing prompts at startup — deliberately.

- **Used in anger four times, and it paid every time.** The first pass found a
  menu bar where twelve of fourteen menus swallowed clicks, and Save As
  defaulting to the process working directory -- which wrote real documents
  into a git checkout. The second, on 2026-08-20, found a tab drawn as a single
  glyph while every column in `bp-editor` was computed as though it reached the
  next tab stop, so the caret on any tab-bearing line sat where the character
  was not. The third, on 2026-08-21, found typing a query into Find edited the
  document. The fourth, later the same day, found the worst of the four:
  `Security ▸ Sign Document`'s passphrase typed itself into the document in
  plain text. All four are fixed -- the last two, and the crash-recovery
  defect found alongside the third, are below in "Recently closed".

  **All four lived in the same seam, and it is the one a test in this
  repository cannot see: what a toolkit does with what it is handed.**
  1,754 tests did not find the third -- `AppState::find` was correct
  throughout, and every test of it passed. 1,812 did not find the fourth
  either: `PassphraseBar`, its `focus-input()` and the Rust call site that
  invokes it are each correct on their own, and the defect was entirely in
  who took the caret back afterwards, and when.

  Still unclicked, in the order they now matter: **File ▸ Set as Default
  Editor**, the one row that changes state outside this application, which
  wants a person's explicit go-ahead rather than a script's click; and
  drag-to-select in the default `TextInput` surface, which uses Slint's own
  selection rather than this codebase's. The wheel, a window resize, reading
  an enormous document, drag-to-select under `--editor-view` and the caret's
  goal column are all checked and clean; signing one is now reachable, since
  the passphrase bar takes the caret.
  [NEXT_SESSION.md](project/NEXT_SESSION.md) has the checklist and a four-line
  recipe for a 192 MiB fixture.

### Recently closed

What has closed since the record last moved, newest first. **The
lesson from each is in [DECISIONS.md](DECISIONS.md)**, which is the one
home for a lesson -- this list is deliberately just the facts, because a
lesson told in two places is a lesson corrected in one.

- Executing anything is removed, and notebooks with it ([ADR-0057](docs/decisions/ADR-0057.md)). Two crates, two menus and one shell module; the first capability to leave.
- A test suite that got flakier the more it was run: temp paths a recycled process id could reuse.
- A clipped menu label, found by driving the window; every one it hit was a greyed row's reason.
- Driving the window is a script now, not plumbing re-derived every session.
- Every menu row now works or is a readout; a 38-row backlog was mostly finished work nobody had pruned.
- A doc comment promised something the code had never done.
- `bp-notebook` and `bp-execution` are reachable.
- Find did not scroll to its match.
- Research mode, and the last unreachable crate.
- A huge document had no keyboard shortcuts at all, and nobody had noticed.
- Find works in a document the rope does not hold.
- A signing passphrase no longer types itself into the document.
- Find no longer edits the document while you type the query.
- A 2 GB file opens, and costs 0.8 MiB.
- Signing works, and the key is sealed rather than protected.
- Somebody has now typed into the custom editor view, and it works.
- Word wrap now works under `--editor-view`.
- The command line answers `--help` and `--version`, and reports a typo.
- `--line=427 server.log` opens the file already at the line.

Every one of these went stale the same way before the record caught up:
a fix landing without the record moving. See [CLAUDE.md](CLAUDE.md) for
which file owns which kind of fact.

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

## Licence

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at
your option — which every crate manifest has claimed since the scaffold
commit and neither file backed until 2026-08-23
([ADR-0054](docs/decisions/ADR-0054.md)).
