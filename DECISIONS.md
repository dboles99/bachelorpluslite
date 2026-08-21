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
| BP-ADR-0029 | 2026-08-20 | A line break is `
` or `
`; the rope drops `unicode_lines` | Accepted | [ADR-0029](docs/decisions/ADR-0029.md) |
| BP-ADR-0030 | 2026-08-20 | A huge document opens in the custom surface, read-only, in every build | Accepted, shipped | [ADR-0030](docs/decisions/ADR-0030.md) |
| BP-ADR-0031 | 2026-08-20 | A signing key lives in a sealed `.bpadx` key file, not a permission-protected one | Accepted, shipped | [ADR-0031](docs/decisions/ADR-0031.md) |
| BP-ADR-0032 | 2026-08-20 | The application id is reverse-DNS, and is not the directory name | Accepted, shipped | [ADR-0032](docs/decisions/ADR-0032.md) |
| BP-ADR-0033 | 2026-08-21 | OpenAI's API is the vetted destination for content that leaves the machine | Accepted | [ADR-0033](docs/decisions/ADR-0033.md) |
| BP-ADR-0034 | 2026-08-21 | Default-editor registration stays a `.reg` handoff, never a `winreg` dependency | Accepted, shipped | [ADR-0034](docs/decisions/ADR-0034.md) |
| BP-ADR-0035 | 2026-08-21 | The `.bpadx` validation ceiling is lowered to 256 MiB / 16 iterations | Accepted | [ADR-0035](docs/decisions/ADR-0035.md) |
| BP-ADR-0036 | 2026-08-21 | A long security history stays usable by reading it lazily, not by a faster key | Accepted | [ADR-0036](docs/decisions/ADR-0036.md) |
| BP-ADR-0037 | 2026-08-21 | Phase 9 is a related-notes panel and duplicate detection | Accepted | [ADR-0037](docs/decisions/ADR-0037.md) |
| BP-ADR-0038 | 2026-08-21 | Notebook mode is a lightweight, one-cell-at-a-time utility | Accepted | [ADR-0038](docs/decisions/ADR-0038.md) |
| BP-ADR-0039 | 2026-08-21 | Research mode turns gathered data into design recommendations, offline, on its own data model | Accepted | [ADR-0039](docs/decisions/ADR-0039.md) |
| BP-ADR-0040 | 2026-08-21 | `bp-execution` runs a cell as a fresh subprocess, three languages first | Accepted, shipped | [ADR-0040](docs/decisions/ADR-0040.md) |
| BP-ADR-0041 | 2026-08-21 | Research mode's data is `bp-storage`'s, not a new store | Accepted, shipped | [ADR-0041](docs/decisions/ADR-0041.md) |
| BP-ADR-0042 | 2026-08-21 | Find in a document served from disk is resumable, not threaded | Accepted, shipped | [ADR-0042](docs/decisions/ADR-0042.md) |
| BP-ADR-0043 | 2026-08-21 | Notebook mode arrives as a Run menu over a literate document, not a cell view | Accepted, shipped | [ADR-0043](docs/decisions/ADR-0043.md) |
| BP-ADR-0044 | 2026-08-21 | The Research menu is what a citation crate can honestly do offline | Accepted, shipped | [ADR-0044](docs/decisions/ADR-0044.md) |
| BP-ADR-0045 | 2026-08-21 | A Markdown file is a notebook, and the panel that lists things is one component | Accepted, shipped | [ADR-0045](docs/decisions/ADR-0045.md) |
| BP-ADR-0046 | 2026-08-22 | Research mode's last five planned rows were a paper's structure; two fold into the report, one becomes Open Questions, one becomes What the Store Holds, one is dropped | Accepted, shipped | [ADR-0046](docs/decisions/ADR-0046.md) |

## Decisions needed before the work they block

- ~~**Where a signing key lives**~~ (phase 16). **Answered and built:
  ADR-0031.** The key is stored *sealed*, in the `.bpadx` envelope ADR-0021
  already ships, under a passphrase the user chooses -- contents protected
  rather than permissions. That closes the asymmetry ADR-0026 measured and
  could not fix: `0600` on Linux and nothing on Windows, where narrowing a
  DACL needs Win32 and `unsafe`. The platform keyring stays specs.md section
  15's third route and becomes a second *source of the passphrase* rather than
  a second key format. Security ▸ Sign Document acts.

  **It was the last row greyed for a missing decision rather than a missing
  prerequisite**, and it is worth recording how long that took: three
  sessions, with the reason in its own label the whole time. Nothing about the
  work was hard once the question was answered. That is the argument
  `project/DECISIONS_NEEDED.md` exists to make.
- **What may leave the machine** (phase 8 layer three, phase 10). Generative
  providers and embeddings both imply sending document content somewhere.
  ADR-0006 keeps them optional; ADR-0011 governs what is permitted. Neither
  says which providers are acceptable.
- **What `bp-storage` records, and what the user sees for it** (phase 9).
  Narrowed twice and no longer a yes/no. ADR-0020 settled the security half --
  `record_document` honours the policy, dropping the title under `PathOnly`
  and recording nothing under `Disabled` -- and the product half is answered:
  it is wired **after** a design pass, not before. ADR-0019 stands exactly as
  written. What is still open is the design, which is a question about the
  product rather than about the crate: a related-notes panel, duplicate
  detection, or something else. Recording something before deciding what it is
  for is how a metadata store becomes a liability.
- **specs.md section 22's warm-start target** (75 ms) is still unverified —
  the software renderer's time to first interaction cannot be measured, so
  half of ADR-0017's target has no number behind it.

## Open items

- **"A flipped bit anywhere in a `.sig` never verifies" is false, correctly.**
  `Sidecar::parse` tolerates trailing whitespace so a signature survives being
  mailed and pasted, so flipping bit 0 of the closing newline yields a
  vertical tab and the file still verifies -- the document really is
  unaltered and the signature really does hold. The property with teeth, and
  the one now tested over every byte and every bit, is that a damaged sidecar
  verifies *if and only if* it parses back to the identical key and
  signature. Recorded because the naive version is the one somebody will
  write next.

- ~~**The recent-files list roams on Windows, and it is full of absolute
  paths.**~~ **Fixed**, and all three files moved together, which is why it
  waited for one change owning `bp-config` and `bp-ui`. `recent.toml`, the
  recovery journal and the security history all hung off the config
  directory, which on Windows is `%APPDATA%` and roams between machines --
  and all three hold machine-specific absolute paths, which is exactly what
  `bp_platform::DirKind::roams` warns against. All three now resolve through
  `DirKind::State`.

  Three things worth carrying forward. **A path derived by subtracting
  another path's last component moves when that one does**: the security
  history was `recovery_dir()` with `with_file_name`, so renaming the
  recovery folder would have moved the history silently, and a history that
  moves starts again at sequence one. It asks for the state directory
  directly now. **The rule is separated from the edge that reads the
  environment** -- `recovery_dir_under` and `audit_path_under` take the state
  directory as an argument, the same split `config_path` and `config_path_in`
  make, so both are assertable without a real profile. And **`cargo test` no
  longer creates a recovery directory in the developer's own profile**;
  `recovery_dir()` is `cfg(test)`-redirected to a unique temp path, which is
  the fix the security history already took after it wrote a live history
  into `%APPDATA%` once.

  Nothing migrates the old locations. A recent list of ten paths rebuilds
  itself on the first open; a recovery journal is by definition transient.
  The files left behind in `%APPDATA%\bachelorpad\` are orphaned and can be
  deleted.

- ~~**`Profile::is_data` answers a question it cannot.**~~ **Fixed.** It
  documented itself as "whether the Data menu's operations apply" and said
  yes for `Ini` and `Xml`, which are structured data with no parser in
  `bp-data` -- so the first caller to gate a menu on it would have got a menu
  whose every row failed. The class question is `Profile::is_data_class` and
  says outright that it is not the other one; the menu question is
  `Format::has_data_operations`, asked of the *format*. `Format::ALL` came
  with it, and a test walks it against `bp_ui::menus::data` so the predicate
  and the menu are one truth rather than two that can drift.

  Found by the cross-crate tests and latent for a session, which is the
  argument for the whole tier: nothing was wrong today, and the row that
  would have gone wrong had not been written yet.
- **Neither `bp-platform-windows` nor `bp-platform-linux` was created**, and
  that is a decision rather than an omission. specs §20 and the crate map
  name them. Everything they would hold is either already a parameterised
  function in `bp-platform` -- tested on both legs -- or blocked on D8 and
  the signing-key question. ADR-0001's own warning about `cfg`-gated code
  argues against two crates whose contents each CI leg never compiles.
  Revisit when something genuinely platform-specific has to be linked.

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
- **The custom editor view has now been typed into, and most of it works.**
  A person drove both views on 2026-08-20. Keys arrive, Ctrl+S saves, the
  recovery journal fires on its own, the Security menu reaches its own bottom
  row, and **the caret tracks the pointer**: a click on a line reported that
  line, and a click at the end of `line 30` reported `Ln 30, Col 8`, which is
  exactly its length plus one. Ln/Col -- the whole payoff of the view -- is
  live in the status bar.
  
  What that pass found was **tabs**, and it is fixed: see below. Still
  unchecked are the wheel direction, drag-to-select and behaviour on resize.
  `project/NEXT_SESSION.md` has what is left.

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
- **A file just over the huge threshold reads as past itself.**
  `Access::ReadOnlyBySize`'s message renders both the file's size and the limit
  through `format_bytes`, which rounds to whole MiB above 1 MiB -- so a
  192 MiB + 1 byte document produces *"Read-only: 192 MiB is past the 192 MiB
  editing limit."* Every word of that is true and the sentence looks
  self-contradictory. Only files within rounding distance of the threshold are
  affected, which is why nothing caught it: the tests assert the message names
  the size and blames neither permissions nor a missing viewer, and it does all
  three.

  Found while writing a manual-pass checklist, which is a second argument for
  writing one. Fixing it means either a decimal place at MiB (`192.0 MiB` past
  `192.0 MiB` -- no better), or the exact byte count in parentheses, or
  rounding the *limit* down and the *size* up so the two can never print equal.
  The third is the only one that reads correctly at every size, and it is a
  change to `bp-buffer`'s formatter rather than to the message.

- **The status bar says "Never saved" about a file that was opened from
  disk.** ADR-0005 means "no save has happened in this session", and that is
  what the field holds; what it *reads* as is "this file has never been
  saved", which is false for every document loaded from a path. Found by
  looking at it. A wording change, not a behaviour one.
- **Under `TextInput` a tab is still drawn as one glyph**, because Slint owns
  that rendering and we do not. It has no visible consequence today: that
  view reports a line count rather than Ln/Col, so there is no column readout
  to disagree with the glyphs. It becomes one the moment anything
  column-shaped is offered there.
- **Most of the product has still not been used, and the count keeps making
  the same point.** 1,754 tests cover the pieces, and every manual pass so far
  has found something none of them could. The first found a menu bar where
  twelve of fourteen menus swallowed clicks, and Save As defaulting to the
  process working directory — which wrote real documents into a git checkout.
  The second found a tab drawn as one glyph while every column was computed as
  though it reached the next stop, so the caret on any tab-bearing line sat
  where the character was not, and the error grew with every tab further
  along.

  **Both defects lived in the same seam, and it is the seam a test in this
  repository cannot see: what a toolkit does with what it is handed.** More
  unit tests will not find the third one.

  What is unchecked now is listed in `project/NEXT_SESSION.md`. The wheel and
  the resize moved up that list when the huge-file viewer shipped, because
  scrolling is the whole of what a viewer does.

- **A signing key now exists on the machine of anyone who signs, and
  `cargo test` nearly put one on the developer''s.** It did, once, during the
  change that added the feature — a real Ed25519 private key in
  `%LOCALAPPDATA%`, written by a test run before the guard landed. Deleted,
  and the guard is `AppState`''s `signing_key` field.

  **This is the third time this crate has needed that fix**, after the
  security history (which wrote a live log into `%APPDATA%`) and the recovery
  journal. The shape is always the same: a function that reads the real
  profile directory, called from `AppState::new()` or from an operation any
  test can trigger. The fix is a field rather than a global — and here a
  `cfg(test)` redirect on the *function* would not have been enough, because
  "does a key exist" and the signing that follows must agree on one path.

  Worth a gate stage? Measured rather than assumed, and the answer is not
  obvious: a check that no test writes outside the temp directory would want
  to run the suite under a redirected `%LOCALAPPDATA%`, which is a stage that
  passes for the wrong reason if the redirection fails. Recorded here instead,
  because three occurrences is a pattern and the fourth will not announce
  itself either.

- **Three modes were sized as large and were only undecided**, and that is the
  most expensive mistake in this log. Phases 9, 12 and 13 -- storage,
  notebooks, research -- sat as "built with no way in" for four sessions,
  described each time as *modes* rather than menu rows, with `NEXT_SESSION.md`
  calling a notebook view "the largest unscoped thing in the product" three
  sessions running.

  None of them was large. Each was waiting on a question:

  - Notebooks needed to know what the mode is *for*. ADR-0038's
    no-persistent-session model makes exploratory analysis dishonest and a
    literate document free, because a runbook's examples are self-contained by
    intent. Choose that and no cell view is needed at all -- the notebook
    stays its own JSON in the ordinary editor and the surface is a menu.
  - Research needed two decisions read as two features rather than one
    contradiction: ADR-0039/0041 define synthesis over `bp-storage` and rule
    out the bibliography types, while `MENU_MAP.md` names citation metadata.
    Both are right; they are different features sharing a menu.
  - Storage was the same shape and was settled first, by ADR-0037.

  **Undecided reads like large**, and an estimate made before the question is
  answered measures the question rather than the work. Worth remembering the
  next time something is described as a mode.

- **A refusal outlived the only evidence that could have contradicted it.**
  Go to Line was recorded as needing "the caret `TextInput` does not expose",
  alongside four features that genuinely do. It needed the caret *moved*,
  which `set-selection-offsets` does and which Find Next had always relied on.

  What kept the error alive is the part worth keeping: the move used to be
  *invisible* -- nothing scrolled the viewport to the caret -- so even if
  somebody had removed the guard, a correct jump would have looked like
  nothing happening. The claim only became checkable once that was fixed,
  which happened for an unrelated reason a day earlier.

  Reading a caret and moving one had been filed under one word. So had
  "going somewhere" and "saying where you are" -- the second still does need
  the caret read, which is why the status bar cannot report `Ln`/`Col` under
  `TextInput` and Go to Line now can work anyway.

- **Find never scrolled to its match, and every test of it used a document
  that fitted on one screen.** Four manual passes and three of the same
  session's own runs confirmed Find "worked" on fixtures where the match was
  already visible -- which tested the search and nothing about the viewport.
  It surfaced only because a new feature jumped to a line a hundred below the
  fold and visibly did nothing.

  The methodology finding outlives the fix: **a fixture that fits on one
  screen cannot test anything about scrolling**, and most of this product's
  readouts are about where you are. It is now a row on the manual-pass
  checklist, phrased about the fixture rather than the feature.

- **Two keyboard defects masked each other in the custom surface.**
  `scroll_for_command` returned "handled" for every key it was given,
  including the `Command::Ignore` that `bp_editor::keys::command_for` produces
  for exactly the keys its own comment calls the window's -- so a huge
  document had no shortcuts at all, Ctrl+F among them, since the viewer
  shipped. Behind it, `forward-focus: editor` named the `TextInput`, invisible
  whenever `use-editor-view` is true, so nothing held the keyboard until you
  clicked.

  Fixing the first is what made the second visible: until keys arrived at all,
  there was no way to notice nothing was holding them. **"Handled" is a claim
  with a consequence somewhere else**, and the two `return true`s that caused
  it had been written to mean "there is nothing to do here".

- **A property test found a Windows device name four sessions after it was
  written.** `bp-integrity`'s manifest proptest generates file names from
  `[a-z]{1,8}`; a fresh seed produced `nul`, which Windows will not create as
  a file *or* a directory, so the fixture could not be built and the property
  was never reached.

  The generator now asks `bp_platform::paths::reserved_device_name` rather
  than carrying a list -- a second list is the exact failure `bp-naming`'s
  two-way agreement test exists to stop. **This is the fourth time this
  workspace has been bitten by Win32 device names**, after the sanitiser,
  `atomic_write`, and the env-root property test. The rule has one home; the
  recurring mistake is not asking it.

- **A name that has sat in a plan long enough starts to read like a
  specification.** `docs/product/MENU_MAP.md` listed five Research rows --
  research question, evidence, findings, methods, datasets -- from `9f98b8a`,
  the scaffold commit, where they were part of one sentence describing a menu
  nobody had designed. Four sessions of planning treated them as a backlog.
  They are the section headings of a research *paper*, and ADR-0039 had
  already decided the mode was something else; scoping them took an afternoon
  and resolved all five, two of them by deletion.

  **This is the mirror of "undecided reads like large", and it is the more
  dangerous of the two.** An unscoped mode at least *looks* unscoped, so
  somebody eventually asks. A named row in a table looks decided. Nothing
  about it invites the question, and it can therefore sit in a plan
  indefinitely, being counted.

  The tell, when it comes round again: **ask where the name came from.** If
  the answer is a commit that scaffolded the repository rather than a decision
  that chose it, it is a sketch, and it has been read as a plan ever since.

- **"Inspectable" is a claim about the product, not about the source.**
  ADR-0041 committed Research Report to "a plain aggregate query with a fixed,
  inspectable rule", and every rule was indeed inspectable -- in
  `state/research.rs`. Meanwhile the report scanned the five hundred
  most-recently-seen documents and told the reader nothing about it, so a
  store of nine hundred produced a report that read as complete. Both things
  were true at once for a session: the rule was written down, and the person
  it was written for could not see it.

  The fix was six lines saying what the thresholds are and when the truncation
  bit. The lesson is the same shape as the comment-versus-code trap that
  precedes it: **ask of any honesty commitment, honest to whom?** If the answer
  is "to whoever reads this file", it has not been kept.

- **A comment and a test disagreed inside the same file, and both were read
  as right.** `Store::forget_document`'s doc comment promised "and with it any
  tags that were only on it"; forty lines below, its own test asserted
  `"the tag itself survives; nothing carries it"`. The code did the second.
  Nobody noticed, because every tag-shaped query in `bp-storage` joins
  `document_tags` and so cannot see an orphaned tag -- there was no query whose
  answer depended on which of the two was true, until `Store::summary` had to
  count tags and pick one.

  **Two claims about the same function can coexist for as long as nothing
  asks.** That is a different failure from a comment that is merely wrong: a
  wrong comment is one mistake, and this was two readers each correctly
  describing what they were looking at. The generalisation for this repository
  is that the question "what would fail if this stopped being true?" has a
  second edge -- when the answer is "nothing", the comment is a wish *and* the
  test beside it is free to say the opposite.
