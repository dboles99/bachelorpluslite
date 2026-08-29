# Rosetta R011: Wire a Tested Capability Into the Shell

You are working in the BachelorPad+ Rust workspace.

This rosetta is for the most common kind of work available in this repository
right now: a capability already exists in a library crate, is tested, and is
not reachable from the application. R001 is for designing a feature; this one
is for connecting one that is already built.

Use it when the answer to "where does the logic live?" is "a `bp-*` crate that
already has tests for it."

## Read first

- `docs/architecture/ARCHITECTURE.md` — especially **The editor-view
  boundary**, which describes two views over one document model
- `crates/bp-ui/src/menus.rs` — the action-id map
- the crate you are wiring, including its tests
- the ADR named in the task, if there is one

## The one rule that matters

**Decisions live in library crates; `bp-ui` only connects them.** If you find
yourself writing an `if` about *what the feature should do* inside `bp-ui`,
that logic belongs in the crate, where it can be tested without a window. The
shell is allowed to decide *which* thing to call and *what to show*, and
nothing else.

A change that adds behaviour to `bp-ui` and no tests anywhere is almost always
in the wrong file.

## Traps specific to this codebase

Each of these has already cost somebody a debugging session.

1. **Action ids are ranges, and the ranges are load-bearing.** `menus.rs`
   allocates ids in blocks. The dispatch in `bp-ui/src/lib.rs` matches some of
   them with `contains(&id)`, so an id landing in the wrong block silently
   does something else — the recent-files arm once claimed everything from 60
   to 100. Before you pick a number, read `pub mod action` and pick a free
   block. The test `no_menu_action_id_falls_inside_the_recent_files_window`
   exists to catch this; extend it rather than working around it.

2. **Ids 100–105 are Slint's, conditionally.** Under `TextInput` those are
   handled on the widget and never reach Rust. Under `--editor-view` they all
   reach Rust. `dispatch()` in `app.slint` branches on `use-editor-view`. If
   you add an editor action, decide which side owns it and check
   `only_editor_actions_fall_in_slints_window` still passes.

3. **There are two editor views. A wiring change usually needs both.**
   Anything touching the caret, the selection or the clipboard behaves
   differently under `TextInput` (Slint owns them) and under `EditorSurface`
   (we do). Look at how `select()` and `reveal()` handle this and follow the
   same shape -- the clipboard-history rows were the other worked example
   until ADR-0061 removed them. Missing one path produces a feature that
   works for you and not for the user, depending on a flag.

4. **An element inside a Slint `if` is out of scope for anything outside it.**
   That is why the find bar is collapsed to zero height rather than made
   conditional, and why both editors exist with `visible` toggled. If you need
   to reference an element from a function or `forward-focus`, it cannot be
   inside an `if`.

5. **`AppState::active_text()` allocates the whole document.** The rope is the
   storage, so "the document as a string" is a copy. It is fine for a
   user-initiated one-off — saving, searching, a data operation. It must never
   go on the typing path or into `refresh()`. Use `active_prefix()`,
   `active_has_content()`, or `editor.buffer()` for questions about the
   document that do not need all of it.

6. **`refresh(&ui, &mut state, PushText::Yes | No)`.** `Yes` re-pushes the
   document into the widget and is correct when the document changed
   underneath it — opened, switched, closed. `No` is correct when the user
   typed. Getting it wrong either clones the document on every keystroke or
   leaves the widget showing stale text.

7. **Never hold a `RefCell` borrow across a native dialog.** `rfd` dialogs
   pump events, and a re-entrant callback on a live `borrow_mut()` panics.
   Compute what you need, end the borrow, then show the dialog.

8. **Do not enable rfd's `common-controls-v6`.** It has been tried. The binary
   fails to start with `STATUS_ENTRYPOINT_NOT_FOUND` before `main`. The
   comment in `crates/bp-ui/Cargo.toml` says so; leave it there.

9. **A menu row's callback cannot take the keyboard focus for itself.** Two
   things put the caret back in the editor *after* it returns, and neither is
   reachable from the callback: `dispatch()` in `app.slint` ends with
   `root.focus-editor()`, and closing a `MenuPopup` restores the focus the
   popup took when it opened -- `i-slint-core` calls `close_popup` after it
   has dispatched the click, not before. This cost a session and shipped a
   defect that typed a signing passphrase into the open document. If a row
   opens something that wants the caret, set `pending-focus` and let
   `focus-timer` hand it over on the next tick, the way `focus-passphrase`,
   `focus-goto` and `focus-find` do. Confirm it by driving the window; no
   test in this repository can see it.

10. **The log-hygiene gate stage is real.** A `tracing::*!` macro mentioning an
   identifier that carries document text or secrets fails the build. Log
   *about* a document — its path, its size — never what it contains.

## Constraints

- Windows 10, Windows 11 and Linux are equal targets (ADR-0001)
- core editing never depends on cloud or AI services (ADR-0006)
- no custom cryptography (ADR-0011)
- physical file renames and moves need explicit user approval
- **do not add a dependency** without saying so explicitly and giving the
  reason; dependency choices are ADR material here
- keep the UI thread non-blocking

## House style

The repository has a strong and consistent voice. Match it.

- Comments and doc comments explain **why**, not what. "Keeping the first
  rather than the last preserves the order of what survives" is the register.
  A comment restating the code is worse than no comment.
- Use `--` rather than an em-dash inside code comments.
- Test names are full sentences describing behaviour:
  `fn a_no_op_must_not_create_an_undo_entry()`.
- Assertions carry a message giving the reason when the assert alone is not
  self-explanatory.
- British spelling in prose: behaviour, recognised, initialise.
- Every module has a `//!` header saying what it owns and what it deliberately
  does not.

## Task

Wire: `<CAPABILITY>` from `<CRATE>` into `<MENU or INTERACTION>`.

## Definition of done

From `docs/governance/DEFINITION_OF_DONE.md`, the parts that apply here:

- the capability is reachable from the application
- the library crate has tests for the behaviour; `bp-ui` has tests for the
  wiring decisions it makes (which action maps to what, what the status bar
  says)
- a user-visible error state exists — an operation that can fail says so in
  the status bar rather than doing nothing
- documentation updated: `README.md`'s current-state table if it is
  user-visible, `ROADMAP.md` if it moves a phase
- an ADR if the decision has consequences beyond one file

## The gate

```powershell
./scripts/Invoke-LocalCI.ps1 -Linux
```

This **is** the gate — there is no hosted CI (ADR-0016). Every commit in this
repository has passed it. It runs `fmt`, `check --locked`, `clippy -D
warnings`, `test`, an actual binary launch, and the log-hygiene scan, then the
whole lot again inside WSL.

Two practical notes:

- **Do not edit files while it is running.** The Linux leg re-runs
  `cargo fmt --check` from scratch, so a mid-run edit fails the run for a
  reason that has nothing to do with your change.
- `cargo fmt --all` before you start it, always.

## Required output

1. which crate already holds the behaviour, and which of its tests cover it
2. the action id block you chose and why it is free
3. whether the change needs one editor path or both, and why
4. the code
5. the tests, and where each lives
6. what the user sees when it fails
7. docs updated
8. gate output, both legs
9. explicit remaining risks — particularly anything only a person at the
   keyboard can confirm
