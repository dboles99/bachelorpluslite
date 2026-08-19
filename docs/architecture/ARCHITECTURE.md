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
| `bp-naming` | **live** | Semantic filename grammar (ADR-0003), and the date and time formats the Insert menu writes into documents. Pure: no filesystem, no clock. | 1 |
| `bp-files` | **live** | Atomic save, load, encoding detection, external-change stamps (ADR-0007). | 1, 3 |
| `bp-theme` | **live** | Palettes as data (ADR-0009). Green is the default. | 1, 17 |
| `bp-config` | **live** | Settings precedence, config file, recent-files list, recovery from bad input. | 1 |
| `bp-ui` | **live** | The Slint application shell (ADR-0015). Split into modules — see below. | 1 |
| `bp-buffer` | **live** | Rope buffer, character indices, line/column maths. | 2, 4 |
| `bp-editor` | **live** | Caret, selection, motion, transaction-based undo/redo, line operations, key-to-command mapping, document-to-screen geometry. The editor's storage — see below. | 2 |
| `bp-history` | **live** | Crash-safe recovery journal and autosave checkpoints. | 3 |
| `bp-formats` | **live** | Format detection and profiles (ADR-0008). | 5 |
| `bp-data` | **live** | Structured-data operations: validate, format, minify, convert, report. | 6 |
| `bp-search` | **live** | Literal and regex find/replace, plus recursive cross-file search. | 7 |
| `bp-semantic` | **live** | Deterministic extraction: titles, keywords, summaries, outlines, document statistics. Layer one only. | 8 |
| `bp-clipboard` | **live** | Clipboard history, kind detection and format-aware paste transformations (ADR-0010). In memory only. | 11 |
| `bp-organize` | planned | Projects, topics, tags, related notes, duplicate detection. | 9 |
| `bp-storage` | **live** | SQLite metadata store and migrations (ADR-0019). Documents, tags. Nothing writes to it yet. | 9 |
| `bp-notebook` | planned | Cell model, `.ipynb` import/export. | 12 |
| `bp-execution` | planned | Runners and execution security. Never auto-runs (ADR-0011). | 12 |
| `bp-research` | planned | Citations, paper metadata, research profile. | 13 |
| `bp-security` | **live** | Security profiles resolving to a policy over seven axes (ADR-0020). Decides policy; performs none of it. Nothing consults it yet. | 14, 16 |
| `bp-crypto` | **built, unwired** | The `.bpadx` envelope (ADR-0021): Argon2id, XChaCha20-Poly1305 and AES-256-GCM, chunked with the header and chunk position authenticated. Composes primitives, implements none. Reached from Security ▸ Encrypt Document and from opening a `.bpadx`. | 15 |
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
                         ├─> bp-buffer   (the latency probe only)
                         ├─> bp-clipboard
                         ├─> bp-config
                         ├─> bp-data
                         ├─> bp-editor ──> bp-buffer
                         ├─> bp-formats
                         ├─> bp-history
                         ├─> bp-naming
                         ├─> bp-search
                         ├─> bp-semantic
                         ├─> bp-theme
                         └─> slint, rfd, arboard
```

**Eleven crates depend on nothing else in the workspace**: `bp-core`,
`bp-naming`, `bp-theme`, `bp-config`, `bp-buffer`, `bp-formats`, `bp-data`,
`bp-search`, `bp-semantic`, `bp-storage` and `bp-clipboard`. That is what keeps them cheap
to test and impossible to entangle with the UI toolkit — and it is why 542
tests run without a window.

Two deliberate non-dependencies:

- `bp-config` does not depend on `bp-theme`. A theme name from a user's file
  has to be recoverable when it is wrong, so it stays a string until the
  application resolves it.
- `bp-semantic` does not depend on `bp-formats`. Extraction reads text; asking
  it what kind of file it is would couple two layers that have no reason to
  know about each other.

## Inside `bp-ui`

The shell was one 2,675-line file and the single-writer bottleneck for every
piece of wiring work. It is now five, split along seams the file already had
as comment banners:

| Module | Owns |
| --- | --- |
| `state.rs` | `AppState`: documents, workspace, saving, reloading, format detection, status labels, find matches |
| `editor_view.rs` | The custom surface: key translation, caret placement, what to draw, `TAB_WIDTH` |
| `dispatch.rs` | The menu-action match, and the dialogs its arms share |
| `menus.rs` | Menu contents and the action-id map |
| `lib.rs` | `run_with`, `refresh`, and the Slint callback wiring |

`ui/app.slint` took the same treatment for the same reason, and is now seven
files rather than one 1,189-line one:

| File | Owns |
| --- | --- |
| `types.slint` | The five structs and the `Palette` global — the boundary `slint-build` generates into Rust |
| `menu.slint` | `MenuLabel`, `MenuRow`, `MenuPopup` |
| `editor_surface.slint` | `EditorSurface`: the custom view's drawing, measurement and input |
| `tab.slint` | One tab |
| `find_bar.slint` | Find and replace, including `focus-query` |
| `results_panel.slint` | Cross-file search results |
| `status_bar.slint` | Both status rows |
| `app.slint` | The window's properties and callbacks, the shortcut bindings, the menu bar and its popups, both editor views, the tab strip, and the layout |

Three things stayed in `app.slint` deliberately, and they are what a wiring
item may still have to serialise on:

- **The menu bar and its popups.** A `PopupWindow`'s position is relative to
  its parent element, and the popups are placed from their labels' `x`. Moving
  both into a component changes the coordinate parent — a visual change
  nothing in the gate can catch.
- **Both editor views**, because `dispatch`, `select-range` and
  `forward-focus` name `editor` and `surface` directly, and a parent cannot
  name an element inside a child component.
- **The tab strip**, a `for` over `root.tabs` and three lines of layout.

Two rules the modules run on:

- **Action ids are blocks, and the dispatch matches ranges.** An id in the
  wrong block silently does something else. Every range arm is bounded at both
  ends; the unbounded one that used to exist would have swallowed any id
  allocated above 300.
- **`bp-ui` connects; it does not decide.** An `if` in here about what a
  feature should *do* belongs in a library crate, where it can be tested
  without a window.

## The editor-view boundary

**The rope is the storage.** `bp-ui` holds
`HashMap<DocumentId, bp_editor::Editor>`, whichever view is drawing. That was
the change ADR-0018 made, and it is what unblocked phase 2.

| Document | insert via `String` | insert via rope |
| ---: | ---: | ---: |
| 100 KB | 55.8 µs | 0.3 µs |
| 1 MB | 563.7 µs | 0.3 µs |

There are two views over it:

| | `TextInput` (default) | `EditorSurface` (`--editor-view`) |
| --- | --- | --- |
| Caret and selection | Slint's, unreadable | `bp-editor`'s |
| Undo | Slint's | `bp-editor`'s transactions |
| Status bar | line count | **Ln/Col** |
| Word wrap | yes | not yet |
| Input-method composition | yes | not yet |
| Lines drawn | all of them | only the visible ones |

`TextInput` owns its own text, caret and undo stack and exposes the caret only
through a property marked *"internal, undocumented, only exposed for tests"*,
which is why text arriving from it lands in the rope as a single undoable
replacement rather than as keystrokes.

The custom surface exists to own none of those things either. Every decision
is in `bp-editor` — motion in `Editor`, chords in `keys`, document-to-screen
arithmetic in `view` — and the widget draws what it is handed and reports rows,
columns and key text back. That split is what lets an editor's behaviour be
checked by `cargo test`; what is left needing a person is genuinely visual.

The rule the boundary runs on: **`bp-editor` deals in rows and visual columns,
never pixels.** The toolkit knows its own font, so it converts. One place for
the two to disagree instead of two.

Word wrap is the parity item with real depth: it makes a visual line differ
from a document line, which every function in `bp_editor::view` currently
assumes away.
