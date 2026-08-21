# BachelorPad+ Full-Product Roadmap

This is not an MVP roadmap. Each phase contributes to the full v1 target.

Status as of **2026-08-21**. Three words, meaning three different things:

- **Done** — the phase's work is finished.
- **Started** — a real, tested slice exists *and is reachable from the
  application*. Not that the phase is finished.
- **Built, unwired** — the library work exists and is tested, but nothing in
  the application calls it. Used where that is a deliberate decision rather
  than an unfinished job, and the reason is always recorded.

| # | Phase | Status | Tasks | Crates |
| --- | --- | --- | --- | --- |
| 1 | Foundation and workspace | **Done** | [01-foundation](project/tasks/01-foundation/) | `bp-core`, `bp-config`, `bp-theme`, `bp-ui` |
| 2 | Core editor | **Started** | [02-core-editor](project/tasks/02-core-editor/) | `bp-buffer`, `bp-editor` |
| 3 | File safety and recovery | **Done** | [03-file-safety](project/tasks/03-file-safety/) | `bp-files`, `bp-history` |
| 4 | Large-file engine | **Done** | [04-large-files](project/tasks/04-large-files/) | `bp-buffer` |
| 5 | Format registry and parser framework | **Started** | [05-formats](project/tasks/05-formats/) | `bp-formats` |
| 6 | Structured formats | **Started** | [05-formats](project/tasks/05-formats/) | `bp-data` |
| 7 | Search engine | **Started** | [06-search](project/tasks/06-search/) | `bp-search` |
| 8 | Semantic foundation and naming | **Started** | [07-semantic](project/tasks/07-semantic/) | `bp-semantic`, `bp-naming` |
| 9 | Organization and related-note graph | **Built, unwired** | [07-semantic](project/tasks/07-semantic/) | `bp-storage` |
| 10 | Semantic/hybrid search | Not started | [07-semantic](project/tasks/07-semantic/) | — |
| 11 | Clipboard system | **Started** | [08-clipboard](project/tasks/08-clipboard/) | `bp-clipboard` |
| 12 | Notebook/execution system | **Built, unwired** | [09-notebook](project/tasks/09-notebook/) | `bp-notebook` |
| 13 | Research mode | **Started** | [10-research](project/tasks/10-research/) | `bp-research`, `bp-storage`, `bp-semantic` |
| 14 | Security foundation | **Started** | [11-security](project/tasks/11-security/) | `bp-security` |
| 15 | Encrypted `.bpadx` documents | **Started** | [11-security](project/tasks/11-security/) | `bp-crypto` |
| 16 | Advanced security | **Started** | [11-security](project/tasks/11-security/) | `bp-secrets`, `bp-redaction`, `bp-audit`, `bp-integrity` |
| 17 | Themes/personality/accessibility | **Started** | [12-themes-brand](project/tasks/12-themes-brand/) | `bp-theme` |
| 18 | Windows/Linux platform integration | **Started** | [13-platform](project/tasks/13-platform/) | `bp-platform` |
| 19 | Hardening, fuzzing and benchmarks | **Started** | [14-hardening](project/tasks/14-hardening/) | `fuzz/` (standalone, and now gated) |
| 20 | Packaging, signing, release and upgrade testing | Not started | [14-hardening](project/tasks/14-hardening/) | — |

## Two numbering schemes, reconciled

The phase numbers above are canonical — they are what `specs.md`, the ADRs and
the application's own "not implemented yet (phase N)" menu rows refer to.

`project/tasks/` uses fourteen directories rather than twenty, because several
roadmap phases share a task folder (all three security phases live under
`11-security`, for example). The mapping is the **Tasks** column. The
directories are deliberately *not* renumbered: they are a coarser grouping of
the same plan, not a competing one, and renaming them would break every link
that points at them for no gain.

## Where the work actually is

Phases 1, 3 and 4 are complete. **Every other phase now has a tested slice
reachable in the product**, and phases 9, 12 and 13 — the three that were
"built with no way in" for four sessions — joined them across 2026-08-21/22.

**No crate in this workspace is unreachable.** That sentence has been the
headline of this section since the fourth session, when it was three crates,
9,556 lines and 241 unit tests behind code no user could reach. `bp-storage`
left under [ADR-0037](docs/decisions/ADR-0037.md), `bp-notebook` and
`bp-execution` under [ADR-0043](docs/decisions/ADR-0043.md), and
`bp-research` under [ADR-0044](docs/decisions/ADR-0044.md).

**What actually unblocked the last three was not effort.** Each had been
sized as a *mode* — a large, unscoped thing — and each turned out to be
waiting on a question rather than on work:

- **Phase 12, notebooks.** Sized as a cell-sequence view for three sessions.
  The blocking question was what the mode is *for*, and ADR-0038's
  no-persistent-session model answers it: a literate document, whose examples
  are self-contained by intent. Pick that and the view is not needed at all —
  the notebook stays its own JSON in the ordinary editor, and the surface is
  a menu. [ADR-0043](docs/decisions/ADR-0043.md).
- **Phase 13, research.** Blocked on reading two decisions as a
  contradiction. ADR-0039/0041 define the mode as synthesis over `bp-storage`
  and say the bibliography types are not what it is built on, while
  `MENU_MAP.md` names citation metadata. They are two features sharing a
  menu, not one feature with two definitions.
  [ADR-0044](docs/decisions/ADR-0044.md).

  **Its remaining backlog went the same way, one session later, and in the
  opposite direction.** The five rows `MENU_MAP.md` still listed — research
  question, evidence, findings, methods, datasets — were the sections of a
  research *paper*, written into the file by the scaffold commit and never
  elaborated. Sized as work they looked like five features; asked what each
  was *for*, two became part of Research Report, one became **Open
  Questions**, one became **What the Store Holds**, and one was dropped as a
  second name for the report itself. [ADR-0046](docs/decisions/ADR-0046.md).
  So the corollary of "undecided reads like large" is worth writing down too:
  **a name that has sat in a plan long enough starts to read like a
  specification.** Nobody ever wrote these five as one.
- **Phase 9, storage.** Settled first, by ADR-0037, and the same shape: a
  decision about presentation rather than a body of work.

The lesson is recorded because the estimate was the error:
**undecided reads like large.**

**Phase 4 is finished, including the piece it deliberately left out.** Find
over a document served from disk shipped under
[ADR-0042](docs/decisions/ADR-0042.md) — a resumable scan rather than a
threaded one, measured on a 213.5 MiB log: `searching 86%` while it ran, then
`1 of 1` at line 4,800,001. Ctrl+End and a total line count remain refusals
with reasons rather than gaps.

**Cross-crate tests found seven defects, and all seven are fixed.** Until the
fourth session every test in the repository tested one crate. Eleven files of
cross-crate tests found that `bp-search` and `bp-buffer` disagree about what a
line is, that `bp-formats` identified a pretty-printed JSON array as JSON
Lines, that the encoder deciding encodings and line endings was `pub(crate)`
inside the shell where no library test could reach it, that YAML had no way
in, that `bp-notebook` runs a cell whose language it could not map through the
kernel's interpreter, that `bp-files` never applies the extended-length path
prefix -- and, worst of the seven, that `bp-naming` would generate a filename
Windows treats as a device. That last one is now fixed: the sanitiser tests
the stem before the first dot, the way Win32 does, so `con.txt` becomes
`con File.txt` rather than a save that reports success while writing to the
console -- and `bp-files` refuses such a name outright, which closes the
route sanitising never covered, the name a user types into a Save dialog.
The last of the seven was a decision rather than a defect and is now
[ADR-0029](docs/decisions/ADR-0029.md): a line break in this product is `
`
or `
` and nothing else. **There is no `#[ignore]`d test left anywhere in
the tree.** `DECISIONS.md` has the evidence for each.

**Phase 4's measurements, kept because they are the claim.** The open path
classifies from metadata before a byte is read, and all three classes have
somewhere to go: an ordinary document opens as it always did, a large one
opens and edits with its size in the status bar, and a huge one is served
from disk as the reader scrolls
([ADR-0030](docs/decisions/ADR-0030.md)). A few lines of text peaked at
31.3 MiB and a 192 MiB log at 32.1 MiB — a document 240 times the size costs
0.8 MiB more.

It draws in the custom surface in *every* build, not only under
`--editor-view`, because `TextInput` owns its own text and cannot be handed a
window of a file it does not have. The flag is now precisely a statement about
which surface *edits* a document the rope holds.

Two things in that phase are deliberately not built and each is a refusal with
a reason rather than a gap: **Ctrl+End** and **a total line count**. Both need
the whole file indexed, which for these documents means reading two gigabytes
to answer one question while the window is frozen. Find is no longer on that
list: ADR-0042 built it, and it is resumable for exactly the reason those two
are refused — a window must not be frozen while a file is read.

Two whole-file reads also came off the open path. `bp_crypto::is_bpadx` needs
six bytes and was being handed the entire file, which `bp_files::load` then
read again.

**Phase 19's harnesses are inside the gate now, which they were not.**
`fuzz/` declares its own `[workspace]`, so every `--workspace` command in
`Invoke-LocalCI.ps1` walked straight past it: five harnesses feeding hostile
input to `bp-crypto`, `bp-data`, `bp-files`, `bp-formats` and `bp-notebook`,
formatted, linted and run by nobody, while this table called the phase
*Started* on the strength of them. Two stages now run them, on both legs,
and they are green -- 42 tests, one `#[ignore]`d for cost with the reason on
it. The gate is about two and a half minutes per leg longer, which is what
that assurance costs.

**The item that gated three sessions is no longer gating.** The rope is the
editor's storage, `bp-editor` owns caret, motion, undo and the line
operations, and a custom editor view exists behind `--editor-view`
(ADR-0018). Phase 4's large-file work is unblocked either way, because the
storage question is settled independently of which view draws.

**The parity question is settled, and not in the view's favour.** Word wrap
is done: a document line can occupy several visual rows, Up and Down move by
row, and scrolling is anchored to a line *and* a row within it. Input-method
composition cannot be done at all on Slint 1.17.1 — `FocusScope` rejects
`UpdateComposition` and `CommitComposition` and exposes no callback for
either, and `TextInput` is the only item in the toolkit that consumes them.
Without it CJK entry does not work, so `--editor-view` stays opt-in and the
default stays `TextInput`. `project/WORK_QUEUE.md` records the evidence.

**Signing shipped, and with it the last row that was greyed for a missing
decision rather than a missing prerequisite.** ADR-0031 puts the Ed25519
signing key inside the `.bpadx` envelope, sealed under a passphrase, which
closes the Windows/Linux asymmetry ADR-0026 measured and could not fix. Sign
Document acts; Verify Signature already did.

**Security profiles (phase 14) are wired.** ADR-0020 defines what each of
Standard, Private, Confidential and Maximum permits across seven axes, and the
named profiles are checked to be monotonic — each at least as restrictive as
the one before, on every axis. All three dependants now read the policy: the
recovery journal refuses rather than writing plaintext (and deletes what a
looser profile already wrote), clipboard history stops recording and is
cleared, and `bp-storage`'s `record_document` drops the title under `PathOnly`
and records nothing under `Disabled`. The Security menu sets the profile and
states what it permits; the status bar shows anything other than the default.

Two limits are deliberate and visible. Profiles requiring an encrypted journal
cannot be honoured until `bp-crypto` (phase 15), so recovery is refused rather
than downgraded — the menu says "off until encryption ships". And `bp-storage`
is still not called by the application: that is ADR-0019's product decision,
which the profile model makes revisitable rather than reversible by hand.
