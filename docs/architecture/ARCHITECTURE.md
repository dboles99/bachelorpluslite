# Architecture

## Dependency rule

```text
UI
 ↓
Core Editor
 ↓
Files / Formats / Search / Storage
 ↓
Optional Semantic / Research Services
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
| `bp-config` | **live** | Settings precedence, config file, recent-files list, recovery from bad input, and **the inventory of what the command line accepts** (`cli`, which renders `--help` and `--version` from it -- ADR-0054). | 1 |
| `bp-ui` | **live** | The Slint application shell (ADR-0015). Split into modules — see below. | 1 |
| `bp-buffer` | **live** | Rope buffer, character indices, line/column maths, and `Access` — whether a document accepts edits and why not. The large-file engine (ADR-0027) was 2,391 of its lines and left under [ADR-0063](../decisions/ADR-0063.md). | 2 |
| `bp-editor` | **live** | Caret, selection, motion, transaction-based undo/redo, line operations, key-to-command mapping, document-to-screen geometry. The editor's storage — see below. | 2 |
| `bp-history` | **live** | Crash-safe recovery journal and autosave checkpoints. | 3 |
| `bp-formats` | **live** | Format detection and profiles (ADR-0008). | 5 |
| `bp-search` | **live** | Literal and regex find/replace, plus recursive cross-file search. | 7 |
| `bp-semantic` | **live** | Deterministic extraction: titles, keywords, summaries, outlines, document statistics, and the questions a document asks (ADR-0046). Layer one only. | 8, 13 |
| `bp-organize` | planned | Projects, topics, tags, related notes, duplicate detection. | 9 |
| `bp-storage` | **live** | SQLite metadata store and migrations (ADR-0019). Documents, tags. Written to on every successful save and read four ways: Related Notes and duplicate detection (ADR-0037), Research Report's aggregates (ADR-0041), and its own summary (ADR-0046). | 9, 13 |
| `bp-security` | **live** | Security profiles resolving to a policy over **six** axes, plus Privacy Mode (ADR-0020). Decides policy; performs none of it. **Two axes have an enforcing reader** — the journal and the metadata store; the other four are reported by the Tools inspector and consulted by nothing (ADR-0059 §4, and they leave under R5). | 14, 16 |
| `bp-crypto` | **live** | The `.bpadx` envelope (ADR-0021): Argon2id, XChaCha20-Poly1305 and AES-256-GCM, chunked with the header and chunk position authenticated. Plus document hashing and detached Ed25519 signatures (specs §15). Composes primitives, implements none. Reached from Security ▸ Encrypt Document and from opening a `.bpadx`; the signing half is not reached yet. | 15, 16 |
| `bp-secrets` | **built, unwired** | Secret scanning (specs §15): AWS/GitHub/GitLab/Slack tokens, PEM blocks, JWTs, connection strings, high-entropy assignments. Reports where a secret is, never what it is. No dependencies. Platform key protection is still planned. | 16 |
| `bp-integrity` | **built, unwired** | `.sig` sidecars, signing-key files and hash manifests over `bp-crypto` (ADR-0026). Says what it cannot enforce per platform rather than implying it did. | 16 |
| `bp-redaction` | **built, unwired** | Irreversible redaction with merging spans, and metadata inspection (ADR-0028). Documents exactly what verification cannot prove. Pure. | 16 |
| `bp-audit` | **built, unwired** | Security audit history (ADR-0024). An event is `Copy`, so it cannot own a secret; documents are named by an opaque id, never a path. The sealed destination has no implementor yet, so it refuses. | 16 |
| `bp-platform` | **built, unwired** | The seam ADR-0001 requires, and it carries rules rather than only traits: path legality per platform (reserved device names, forbidden characters, the length limits each platform counts in its own unit), config/data/cache directories from an injected environment, a capability register, and default-editor registration -- state, plan, artefacts. ADR-0012 is mechanised: a plan that would seize an association or touch Notepad is refused. Two `cfg` attributes in the whole crate, so all 103 tests run on both legs. | 18 |
| `bp-platform-windows` | **not created, deliberately** | Everything it would hold is either a parameterised function in `bp-platform` or blocked: applying `HKCU` keys needs a Win32 call or a new dependency, and DPAPI/Hello wait on the signing-key decision. ADR-0001's own warning about `cfg`-gated code argues against a crate neither CI leg compiles. | 18 |
| `bp-platform-linux` | **not created, deliberately** | Same reasoning. `.desktop` and MIME registration are in `bp-platform` and run on both legs; Secret Service waits on the same decision. | 18 |

## Current dependency edges

```text
bachelorpad ──> bp-config
            ├──> bp-theme
            └──> bp-ui ──┬─> bp-audit      ──> bp-crypto, bp-security
                         ├─> bp-buffer     (the latency probe, and the size classes)
                         ├─> bp-config     ──> bp-platform
                         ├─> bp-core       ──> bp-security
                         ├─> bp-crypto
                         ├─> bp-editor     ──> bp-buffer
                         ├─> bp-files      ──> bp-core, bp-naming, bp-platform
                         ├─> bp-formats
                         ├─> bp-history    ──> bp-crypto, bp-security
                         ├─> bp-integrity  ──> bp-crypto
                         ├─> bp-naming
                         ├─> bp-platform
                         ├─> bp-redaction
                         ├─> bp-search
                         ├─> bp-secrets
                         ├─> bp-security
                         ├─> bp-semantic
                         ├─> bp-theme
                         └─> slint, rfd, arboard
```

**Every library crate is reachable from the shell.** That count was three
unreachable for four sessions — `bp-notebook`, `bp-research` and `bp-storage`,
each a *mode* rather than a menu row — and it reached zero: `bp-storage` left
under ADR-0037, `bp-notebook` and `bp-execution` under ADR-0043, `bp-research`
under ADR-0044.

**Three of those crates no longer exist.**
[ADR-0057](../decisions/ADR-0057.md) removed `bp-notebook` and `bp-execution`
outright and [ADR-0060](../decisions/ADR-0060.md) removed `bp-research`, which
is a second way to reach zero and a worse one to confuse with the first. Of
the four crates named above, **only `bp-storage` reached it by being wired
in and stayed.** The history is kept because the lesson below was learned
while all of them were still here, and because being reachable is what made
removing them a decision about the product rather than a sweep of dead
code.

**Calling any of it a wiring backlog was never accurate**:
each was waiting on a decision about what the mode was for, and the last item
that could honestly be described as wiring — the viewer for a document the
rope does not hold — turned out to be a feature too. (That viewer has since
been removed outright, [ADR-0063](../decisions/ADR-0063.md); the point it
illustrates is about how the work was *sized*, and survives it.) This block is generated
from the manifests rather than maintained by hand; regenerate it after adding
an edge, because a dependency diagram that has drifted is worse than none.

**Eleven crates depend on nothing else in the workspace**: `bp-buffer`,
`bp-crypto`, `bp-formats`, `bp-naming`, `bp-platform`,
`bp-redaction`, `bp-search`, `bp-secrets`, `bp-security`,
`bp-semantic` and `bp-theme`. That is what keeps them cheap to test and
impossible to entangle with the UI toolkit — and it is why all but 292 of the
workspace's 1,446 tests run without a window.

`bp-platform` is on that list for its *real* dependencies and takes
`bp-formats` as a **dev**-dependency, deliberately and one-directionally: it
lets a test assert that every extension the crate offers to register is one
`bp-formats` can identify, so the product never claims a file type it cannot
open. A real dependency would tie the registration table to the parser's
shape, and the two answer different questions about the same extension — "how
do I read this" against "what does the OS call it".

`bp-security` is the one that acquired dependants rather than dependencies:
`bp-core`, `bp-storage`, `bp-history` and `bp-audit` all read
a policy from it (ADR-0020). A crate everything defers to and that defers to
nothing is the right shape for that, and it is why the list above shrank from
an earlier count of eleven without anything going wrong.

Two deliberate non-dependencies:

- `bp-config` does not depend on `bp-theme`. A theme name from a user's file
  has to be recoverable when it is wrong, so it stays a string until the
  application resolves it.
- `bp-semantic` does not depend on `bp-formats`. Extraction reads text; asking
  it what kind of file it is would couple two layers that have no reason to
  know about each other.

## Inside `bp-ui`

The shell was one 2,675-line file and the single-writer bottleneck for every
piece of wiring work. It is now fifteen, split along seams the files already
had as comment banners:

| Module | Lines | Owns |
| --- | ---: | --- |
| `state/security.rs` | 2,771 | Scan, redact, inspect metadata, hash, sign, verify, the security history all six write into, and the profile and Privacy Mode switches that govern them |
| `menus.rs` | 2,575 | Menu contents and the action-id map |
| `state.rs` | 2,500 | `AppState` itself: documents, workspace, opening, saving, reloading, format detection, the gutter and the status labels |
| `lib.rs` | 1,255 | `run_with`, `refresh`, and the Slint callback wiring |
| `dispatch.rs` | 955 | The menu-action match, and the dialogs its arms share |
| `state/research.rs` | 758 | Research Report: aggregate reads over `bp-storage`, worded as insights, each naming the documents behind it and closing with the rules it applied (ADR-0041, ADR-0046); and what the store holds |
| `default_editor.rs` | 754 | File ▸ Set as Default Editor: the report, the consent dialog, the artefacts (ADR-0012) |
| `state/organize.rs` | 585 | Related Notes, duplicate detection and Suggested Folder, over `bp-storage` (ADR-0037, ADR-0048) |
| `editor_view.rs` | 577 | The custom surface: key translation, caret placement, what to draw, and the scroll that serves both a rope and a file |
| `state/inspectors.rs` | 520 | The Tools menu's readouts: the policy in force on all seven axes, the file on disk, and where each setting came from (ADR-0048) |
| `state/encryption.rs` | 519 | The `.bpadx` passphrase flow: what the bar is asking, and what a wrong answer does |
| `audit.rs` | 245 | Which file the security history is, and what a person reading it sees (ADR-0024) |
| `state/questions.rs` | 176 | What the document in front of you *asks*, through `bp_semantic::questions` (ADR-0046) |
| `testpaths.rs` | 148 | **Test-only.** One temp path per test that no *earlier* run can have left behind (ADR-0049) |
| `passphrase.rs` | 141 | What the one-field passphrase bar is currently asking, as a state machine |
| `state/find.rs` | 139 | What the find bar is looking for, which match the user is standing on, and driving a scan of a document served from disk |

**Three modules meet in the Research menu and none knows the other two**,
which is worth knowing before adding to any of them:

- `state/research.rs` reads the *store* and says what the user has been
  writing about over time (ADR-0041);
- `state/citations.rs` reads the *active document* and says what it cites
  (ADR-0044);
- `state/questions.rs` reads the same document and says what it *asks*
  (ADR-0046).

ADR-0044 has why the first two are not one thing, and ADR-0046 has why the
third is not the "Research Question" the menu map named for two years: a
question has a grammar and can be found; which one you are actually asking is
not something the document says.

### Why the four new ones are children of `state` and not siblings

`state.rs` was 6,282 lines -- more than twice the size the whole shell was
when the first split happened -- and the four modules above came out of it in
one pass.

They are `state/*.rs` rather than `bp-ui/src/*.rs`, and that is the reason the
split was possible at all rather than a filing preference. **A child module
can see its parent's private items.** `AppState`'s `editors`, `stamps`,
`gutter_lines` and `match_index` are private to `state`, and they stay that
way: `state::security` reaches them because it is inside `state`. A sibling
module would have needed every field it touched widened to `pub(crate)`, which
is the crate-wide surface the original file's privacy was buying.

What did *not* split is the struct. `AppState` is one set of fields with one
`impl` block per module, because splitting the state would mean deciding which
half of the product owns the active document, and there is no such division.

Two seams the compiler pointed out, which are worth knowing before adding to
either module:

- **The passphrase bar serves signing as well as encryption.**
  `answer_passphrase` lives in `state/encryption.rs` and routes
  `Ask::UnlockKey` into `state/security.rs`, so `create_key_and_sign` and
  `sign_with_stored_key` are `pub(super)` rather than private. One bar, two
  subjects; ADR-0031 is why.
- **`default_signing_key_path` is `pub(super)` and deliberately not
  `pub(crate)`.** `AppState::new` is its only caller. A second place deciding
  where the signing key lives is exactly the defect the `signing_key` field
  exists to prevent.

**`state/security.rs` is the largest file in the crate now**, and that is
recorded rather than hidden. It is one subject -- nothing in it may put a
secret somewhere the user did not ask for it to be -- and half of it is tests
under their own banners. Its seams, if it ever needs them, are secrets and
redaction against hash, sign and verify: different crates behind them
(`bp-secrets`/`bp-redaction` against `bp-crypto`/`bp-integrity`) and different
questions -- "what is in this document" against "is this document what it
was".

`ui/app.slint` took the same treatment for the same reason, and is now eleven
files rather than one 1,189-line one:

| File | Owns |
| --- | --- |
| `types.slint` | The five structs and the `Palette` global — the boundary `slint-build` generates into Rust |
| `menu.slint` | `MenuLabel`, `MenuRow`, `MenuPopup` |
| `editor_surface.slint` | `EditorSurface`: the custom view's drawing, measurement and input |
| `tab.slint` | One tab |
| `find_bar.slint` | Find and replace, including `focus-query` |
| `list_panel.slint` | The bottom panel that lists things you can click. **One component, two uses**: cross-file search results and Organize ▸ Related Notes. It was two near-identical files until ADR-0045 — `diff` with the names normalised showed differences in their comments and nothing else. Its third use, Notebook ▸ Cell Outline, left under ADR-0057 |
| `status_bar.slint` | Both status rows |
| `goto_bar.slint` | Go to Line |
| `passphrase_bar.slint` | The one-field bar every passphrase in this product is typed into — a document's, and a signing key's |
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
| Storage | the rope | the rope |
| Caret and selection | Slint's, unreadable | `bp-editor`'s |
| Undo | Slint's | `bp-editor`'s transactions |
| Status bar | line count | **Ln/Col** |
| Word wrap | yes | yes |
| Input-method composition | yes | **no, and cannot be** |
| Lines drawn | all of them | only the visible ones |

**There was a third column until [ADR-0063](../decisions/ADR-0063.md)** — the
same `EditorSurface` handed a window of a file rather than rows of a rope, for
a document the rope did not hold. `AppState::uses_custom_surface` was the
function that chose, and it had two terms because ADR-0030 made the answer
depend on the active document as well as on the flag. It has one term now, and
the flag decides alone.

**`--editor-view` therefore means something precise**: it selects the surface
that draws a document. It is not a switch between two products, and no
document opens for one user and is refused for another.

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

There is a second rule the tab defect taught, and it is the one a test in this
repository cannot enforce: **the string handed to the toolkit is not the string
the arithmetic indexes.** `bp_editor::view` computes columns as though a tab
reaches the next stop, and Slint draws `\t` as a single glyph — so `text` stays
raw for the arithmetic and `display_text` is what the surface gets. Anything
else whose drawn width differs from its character count has the same shape.

**Parity is settled and not in the custom view's favour.** Word wrap is done —
a document line can occupy several visual rows, Up and Down move by row, and
scrolling anchors to a line *and* a row within it. Input-method composition
cannot be done at all on Slint 1.17.1: `FocusScope` returns
`EventResult::Reject` for `UpdateComposition` and `CommitComposition` in both
its handlers and exposes no callback for either, and `TextInput` is the only
item in the toolkit that consumes them. Without it CJK entry does not work, so
the default stays `TextInput` and the flag stays opt-in. Revisit on a Slint
upgrade; do not spend an afternoon on it before checking that changed.
