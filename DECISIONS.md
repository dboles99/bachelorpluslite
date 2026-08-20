# BachelorPad+ Decision Log

This file is the human-readable decision register. Every row has a formal ADR
in `docs/decisions/` under the same number: `BP-ADR-0007` is `ADR-0007.md`.
Adding a decision means adding both.

| ID | Date | Decision | Status | ADR |
| --- | --- | --- | --- | --- |
| BP-ADR-0001 | 2026-08-16 | Target Windows 10, Windows 11 and Linux | Accepted | [ADR-0001](docs/decisions/ADR-0001.md) |
| BP-ADR-0002 | 2026-08-16 | Rust is the primary implementation language | Accepted | [ADR-0002](docs/decisions/ADR-0002.md) |
| BP-ADR-0003 | 2026-08-16 | Default semantic filename uses `Title_DDMMMYYYY.ext` | Accepted | [ADR-0003](docs/decisions/ADR-0003.md) |
| BP-ADR-0004 | 2026-08-16 | Creation date is the default filename date | Accepted | [ADR-0004](docs/decisions/ADR-0004.md) |
| BP-ADR-0005 | 2026-08-16 | Status bar exposes last successful disk save and full path | Accepted | [ADR-0005](docs/decisions/ADR-0005.md) |
| BP-ADR-0006 | 2026-08-16 | Core editing must work without semantic/cloud features | Accepted | [ADR-0006](docs/decisions/ADR-0006.md) |
| BP-ADR-0007 | 2026-08-16 | Add full Rust performance/safety feature set | Accepted | [ADR-0007](docs/decisions/ADR-0007.md) |
| BP-ADR-0008 | 2026-08-16 | Add structured formats and Jupyter-style notebook capability | Accepted | [ADR-0008](docs/decisions/ADR-0008.md) |
| BP-ADR-0009 | 2026-08-16 | Add Light, Dark, Organic and Green themes | Accepted | [ADR-0009](docs/decisions/ADR-0009.md) |
| BP-ADR-0010 | 2026-08-16 | Add clipboard history and format-aware clipboard behavior | Accepted | [ADR-0010](docs/decisions/ADR-0010.md) |
| BP-ADR-0011 | 2026-08-16 | Add comprehensive security and cryptography subsystem | Accepted | [ADR-0011](docs/decisions/ADR-0011.md) |
| BP-ADR-0012 | 2026-08-16 | Use OS-supported default-editor registration, never patch Notepad | Accepted | [ADR-0012](docs/decisions/ADR-0012.md) |
| BP-ADR-0013 | 2026-08-16 | Use original retro-futurist appliance branding without copying existing IP | Accepted | [ADR-0013](docs/decisions/ADR-0013.md) |
| BP-ADR-0014 | 2026-08-16 | Use project→task→step→prompts/rosettas→artifacts→metadata→database→note governance | Accepted | [ADR-0014](docs/decisions/ADR-0014.md) |
| BP-ADR-0015 | 2026-08-17 | Adopt Slint as the UI toolkit | Accepted | [ADR-0015](docs/decisions/ADR-0015.md) |
| BP-ADR-0016 | 2026-08-17 | CI is local-first; GitHub Actions is dormant | Accepted | [ADR-0016](docs/decisions/ADR-0016.md) |
| BP-ADR-0017 | 2026-08-17 | Default to the software renderer; split the startup target | Accepted | [ADR-0017](docs/decisions/ADR-0017.md) |
| BP-ADR-0018 | 2026-08-17 | Write a custom editor view; make the rope the storage; ship the view opt-in | Accepted | [ADR-0018](docs/decisions/ADR-0018.md) |
| BP-ADR-0019 | 2026-08-17 | SQLite via bundled `rusqlite`; append-only migrations; metadata never content | Accepted | [ADR-0019](docs/decisions/ADR-0019.md) |
| BP-ADR-0020 | 2026-08-19 | Security profiles resolve to a policy; named profiles are monotonic | Accepted | [ADR-0020](docs/decisions/ADR-0020.md) |
| BP-ADR-0021 | 2026-08-19 | `.bpadx`: two AEADs versioned in the envelope, chunked, position authenticated | Accepted | [ADR-0021](docs/decisions/ADR-0021.md) |
| BP-ADR-0022 | 2026-08-19 | The recovery journal is sealed with the document's passphrase and recovered at unlock | Accepted | [ADR-0022](docs/decisions/ADR-0022.md) |
| BP-ADR-0023 | 2026-08-19 | YAML uses `saphyr`; nesting, alias expansion and duplicate keys are bounded before a tree exists | Accepted | [ADR-0023](docs/decisions/ADR-0023.md) |
| BP-ADR-0024 | 2026-08-19 | An audit event is `Copy`, so it cannot carry a secret; the log's destination resolves from the existing policy axes | Accepted | [ADR-0024](docs/decisions/ADR-0024.md) |
| BP-ADR-0025 | 2026-08-19 | Notebook kinds ride in namespaced `.ipynb` metadata; a run needs a `UserGesture` no parsed file can produce | Accepted | [ADR-0025](docs/decisions/ADR-0025.md) |
| BP-ADR-0026 | 2026-08-19 | A `.sig` sidecar appends to the whole file name and records the signer's key; Windows enforces no key-file permission | Accepted | [ADR-0026](docs/decisions/ADR-0026.md) |
| BP-ADR-0027 | 2026-08-19 | Large-file thresholds are measured; the line index is sparse; memory mapping is declined with reasons | Accepted | [ADR-0027](docs/decisions/ADR-0027.md) |
| BP-ADR-0028 | 2026-08-19 | Redaction merges overlapping and adjacent spans, resolves every offset against the original, and states what verification cannot prove | Accepted | [ADR-0028](docs/decisions/ADR-0028.md) |

## Decisions needed before the work they block

- **Where a signing key lives** (phase 16). `bp-crypto` signs and verifies,
  and both are tested; Verify Signature is reachable and Sign Document is
  greyed, because an Ed25519 signing key has to be kept somewhere and this
  product has no key store, no platform-keyring integration (Windows DPAPI,
  Linux Secret Service -- phase 18) and no decision on file permissions for a
  key sitting on disk. specs.md section 15 asks for all three. The row states
  the gap rather than a key appearing in the config directory because a menu
  row needed one.
- **What may leave the machine** (phase 8 layer three, phase 10). Generative
  providers and embeddings both imply sending document content somewhere.
  ADR-0006 keeps them optional; ADR-0011 governs what is permitted. Neither
  says which providers are acceptable.
- **Whether `bp-storage` may record extracted titles before phase 14 exists**
  (phase 9). ADR-0019 currently says no, and that is why the crate is built
  and unreachable. Everything phase 9 wants -- related notes, duplicate
  detection -- needs something recorded about each document, so this decision
  gates the phase rather than merely delaying it. Reversing it is allowed;
  doing so by accident is not.
- **specs.md section 22's warm-start target** (75 ms) is still unverified —
  the software renderer's time to first interaction cannot be measured, so
  half of ADR-0017's target has no number behind it.

## Open items

- **`bp-files` never applies the `\\?\` extended-length prefix**, so a name
  `bp-naming` is willing to generate can be unwritable. A filename at
  `SemanticName::to_filename`'s own documented 255-byte maximum fails inside
  any directory whose path pushes the total past 259 -- the system temp
  directory alone does it. `bp-platform` has both the diagnosis
  (`paths::needs_extended_length_prefix`, `path_problems`) and the fix
  (`paths::to_extended_length`). `bp-files` depends on `bp-platform` now --
  it asks the same crate whether a name the user typed is a device -- so what
  was two problems is one: the edge exists and the call does not.
  The error compounds it: `SaveError` blames a read-only file or another
  program for what is `os error 3`, sending the user to look at the wrong
  thing.
- **`bp-notebook` silently runs unidentified code through the wrong
  interpreter, and ADR-0025 says the opposite.** `ipynb.rs::infer_kind`
  computes `stated.or(notebook_language)`, which conflates "the cell said
  nothing about its language", where falling back to the kernelspec is right,
  with "the cell named a language we cannot map", where it is a guess. No
  warning distinguishes them. A cell another tool tagged
  `vscode.languageId: "brainfuck"` imports as Python and appears in
  `request_run_all` with the user told nothing. ADR-0025 states that such a
  cell "becomes `CellKind::Raw` and an `ImportWarning::UnknownCodeLanguage`
  says so" -- and that holds only for notebooks carrying no kernelspec, which
  almost no real `.ipynb` is. `CellKind::from_language_name`'s own doc gives
  the stakes: guessing wrong means offering to run someone's text through the
  wrong interpreter.
- **"A flipped bit anywhere in a `.sig` never verifies" is false, correctly.**
  `Sidecar::parse` tolerates trailing whitespace so a signature survives being
  mailed and pasted, so flipping bit 0 of the closing newline yields a
  vertical tab and the file still verifies -- the document really is
  unaltered and the signature really does hold. The property with teeth, and
  the one now tested over every byte and every bit, is that a damaged sidecar
  verifies *if and only if* it parses back to the identical key and
  signature. Recorded because the naive version is the one somebody will
  write next.

- **The recent-files list roams on Windows, and it is full of absolute
  paths.** `recent.toml` sits beside `config.toml` in the config directory,
  which on Windows is `%APPDATA%` and therefore roams between machines --
  and a recent list is machine-specific absolute paths, which is exactly what
  `bp_platform::DirKind::roams` warns against. `bp-platform` names the
  recent-files list as `DirKind::State` for this reason. It was not moved
  when `bp-config` started delegating, deliberately: it is a product decision
  with a user-visible effect rather than a deduplication, and `bp-ui`'s
  `recovery_dir` derives its own path from `config_path`, so moving one
  without the other would scatter the product's files. Both want doing
  together, by something that owns both crates.
- **Neither `bp-platform-windows` nor `bp-platform-linux` was created**, and
  that is a decision rather than an omission. specs §20 and the crate map
  name them. Everything they would hold is either already a parameterised
  function in `bp-platform` -- tested on both legs -- or blocked on D8 and
  the signing-key question. ADR-0001's own warning about `cfg`-gated code
  argues against two crates whose contents each CI leg never compiles.
  Revisit when something genuinely platform-specific has to be linked.

- **`bp-search` and `bp-buffer` do not agree what a line is, and this is
  now a decision rather than a bug report.** `bp_search::Offsets` counts
  `'
'`. `bp_buffer::Buffer` is a `ropey::Rope` built with ropey's default
  features, which include `unicode_lines`, so ropey also breaks on a bare
  `
`, ``, ``, U+0085, U+2028 and U+2029. For `"one
two
needle"`
  search reports line 1 and the buffer reports line 3, so a find-in-files
  result scrolls the editor to the wrong line in any file carrying a bare CR.
  The same split exists inside `bp-buffer` — the free `line_count` counts
  newlines, `Buffer::len_lines` asks the rope — and `bp-ui` uses one for the
  `TextInput` status bar and the other for the `EditorSurface` one, so the
  same document reports two line counts depending on which view draws.
  Either `bp-search` adopts ropey's break set or `bp-buffer` builds its rope
  without `unicode_lines`; one line definition has to win, and that is an
  ADR. Found by the cross-crate tests, pinned as an ignored test naming it.
- **Time to first interaction is unmeasurable under the software renderer.**
  Slint exposes no rendering notifier there, so half of the ADR-0017 startup
  target has no measurement behind it. A hole, not a pass.
- **Scroll smoothness under CPU rasterisation is unmeasured**, and needs a
  capture rig. This is the standing revert condition on ADR-0017. The
  application's own typing-path latency *is* now measured and is not a
  problem: 376 µs p50 at 1 MB against a ~16 ms frame budget.
- **Comparative startup timing on Linux is unmeasured.** The shell builds and
  runs there, but WSLg's compositor makes timing unrepresentative. Needs a
  native Linux machine.
- ~~Keyboard shortcut delivery is unverified.~~ **Confirmed working**
  (2026-08-17, manual test). `KeyBinding` in a wrapping `FocusScope` matches
  during the capture phase, so the focused `TextInput` no longer swallows
  Ctrl+S.
- **The three dependants now read the security profile.** The recovery
  journal refuses rather than writing plaintext, and deletes what a looser
  profile already wrote; clipboard history stops recording and is cleared;
  and `bp-storage`'s `record_document` drops the title under `PathOnly` and
  records nothing under `Disabled`. Privacy Mode clamps all of it for the
  session without replacing the document's profile, so leaving it restores
  what the document had rather than the default.

  What ADR-0020 required to fail loudly no longer has to. The two profiles
  wanting an encrypted recovery journal are honoured, because `bp-crypto`
  ships and ADR-0022 seals the journal with the document's own passphrase.
  The remaining hole is narrower, and is the next item: a *plaintext*
  document under those profiles has no key, so it gets no journal at all.
- **The custom editor view has never been typed into.** Its rules are covered
  by 147 tests in `bp-editor`; the widget has been verified to exactly one
  standard, that it renders a frame with a real file open without panicking.
  Whether keys arrive, whether the caret lands under the pointer and which
  way the wheel scrolls are all open. `project/NEXT_SESSION.md` has the
  checklist.

  This used to carry "nothing depends on the answers yet". **That is no
  longer true.** Duplicate Line and Move Line Up/Down are enabled only under
  `--editor-view`, and Go to Line will be, so features now inherit whatever
  that caret does. The default is still `TextInput`, so nothing is *broken*
  by the answer being bad -- but a growing set of the product is unusable
  until somebody gives it.
- **A clipboard menu row can act on a different entry than the one it
  names.** Row ids are decoded against the clipboard history as it is at the
  moment of the click, not as it was when the menu was built, and a 1.2 s
  poll timer can add an entry in between. Predates the paste-transformation
  rows and applies equally to plain paste. Fixing it properly means freezing
  a snapshot of the history while a menu is open, which is a design question
  rather than a patch.
- **Most of the product has not been used.** 542 tests cover the pieces in
  isolation. One manual pass found two defects no test caught: a menu bar
  where twelve of fourteen menus swallowed clicks, and Save As defaulting to
  the process working directory, which wrote real documents into a git
  checkout. Integrated behaviour needs exercising, not more unit tests.
