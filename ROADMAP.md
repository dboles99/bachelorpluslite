# BachelorPad+ Full-Product Roadmap

This is not an MVP roadmap. Each phase contributes to the full v1 target.

Status as of **2026-08-29**. Four words, meaning four different things:

- **Done** — the phase's work is finished.
- **Started** — a real, tested slice exists *and is reachable from the
  application*. Not that the phase is finished.
- **Built, unwired** — the library work exists and is tested, but nothing in
  the application calls it. Used where that is a deliberate decision rather
  than an unfinished job, and the reason is always recorded.
  **No phase carries it any more**, as of 2026-08-22. It is kept because the
  state is a real one and will recur, and because the count of phases in it —
  three, for four sessions — was this project's most useful single number
  while it was not zero.
- **Removed** — the phase's capability was built, shipped, and then taken out
  again, with an ADR saying why. Four phases carry it: 12 entire, under
  [ADR-0057](docs/decisions/ADR-0057.md), 13 in part, under
  [ADR-0060](docs/decisions/ADR-0060.md), 11 under
  [ADR-0061](docs/decisions/ADR-0061.md) and 6 under
  [ADR-0062](docs/decisions/ADR-0062.md) — research mode's citation half left
  and its synthesis half stayed, which is why the row reads *in part* and
  names the crates that remain. It is a status rather than a deleted
  row because a phase number is what `specs.md` and every ADR refer to, and a
  table that renumbers around a removal breaks all of them.

| # | Phase | Status | Tasks | Crates |
| --- | --- | --- | --- | --- |
| 1 | Foundation and workspace | **Done** | [01-foundation](project/tasks/01-foundation/) | `bp-core`, `bp-config`, `bp-theme`, `bp-ui` |
| 2 | Core editor | **Started** | [02-core-editor](project/tasks/02-core-editor/) | `bp-buffer`, `bp-editor` |
| 3 | File safety and recovery | **Done** | [03-file-safety](project/tasks/03-file-safety/) | `bp-files`, `bp-history` |
| 4 | Large-file engine | **Done** | [04-large-files](project/tasks/04-large-files/) | `bp-buffer` |
| 5 | Format registry and parser framework | **Started** | [05-formats](project/tasks/05-formats/) | `bp-formats` |
| 6 | Structured formats | **Removed** | [ADR-0062](docs/decisions/ADR-0062.md) | — |
| 7 | Search engine | **Started** | [06-search](project/tasks/06-search/) | `bp-search` |
| 8 | Semantic foundation and naming | **Started** | [07-semantic](project/tasks/07-semantic/) | `bp-semantic`, `bp-naming` |
| 9 | Organization and related-note graph | **Started** | [07-semantic](project/tasks/07-semantic/) | `bp-storage` |
| 10 | Semantic/hybrid search | Not started, **decided** | [07-semantic](project/tasks/07-semantic/) | — |
| 11 | Clipboard system | **Removed** | [ADR-0061](docs/decisions/ADR-0061.md) | — |
| 12 | Notebook/execution system | **Removed** | [ADR-0057](docs/decisions/ADR-0057.md) | — |
| 13 | Research mode | **Removed**, in part | [ADR-0060](docs/decisions/ADR-0060.md) | `bp-storage`, `bp-semantic` |
| 14 | Security foundation | **Started** | [11-security](project/tasks/11-security/) | `bp-security` |
| 15 | Encrypted `.bpadx` documents | **Started** | [11-security](project/tasks/11-security/) | `bp-crypto` |
| 16 | Advanced security | **Started** | [11-security](project/tasks/11-security/) | `bp-secrets`, `bp-redaction`, `bp-audit`, `bp-integrity` |
| 17 | Themes/personality/accessibility | **Started** | [12-themes-brand](project/tasks/12-themes-brand/) | `bp-theme` |
| 18 | Windows/Linux platform integration | **Started** | [13-platform](project/tasks/13-platform/) | `bp-platform` |
| 19 | Hardening, fuzzing and benchmarks | **Started** | [14-hardening](project/tasks/14-hardening/) | `fuzz/` (standalone, and now gated) |
| 20 | Packaging, signing, release and upgrade testing | **Started** | [14-hardening](project/tasks/14-hardening/) | — |

**Phase 20 started on 2026-08-23, and what it found is worth knowing before
anybody sizes the rest of it.** The inventory came first, as ADR-0048
recommends, and turned up four things in a product 1,972 passing tests deep:
the binary could not say its own version, a mistyped flag was discarded in
silence, every manifest claimed a licence the repository did not contain, and
`specs.md` advertised three flags that did not exist -- one of which made
`bachelorpad --line 427 server.log` try to open a file called `427`.

All four are fixed ([ADR-0054](docs/decisions/ADR-0054.md)), and the artefact
followed the same day: `scripts/New-Release.ps1` builds, stages, archives and
checksums both targets, and both archives have been extracted and run.
**Signing is deferred and self-signing refused outright**
([ADR-0055](docs/decisions/ADR-0055.md)) -- so the archives are unsigned and
say so, every run, in the same words.

Upgrade testing followed it: `what_an_earlier_build_wrote.rs` holds committed
literals for the four files this product leaves on a disk -- `config.toml`,
`recent.toml`, the security history and the recovery journal -- each of which
had only write-then-read-back coverage, which is trap 7 exactly.

What is left of phase 20 is an icon and a `.desktop` file (a build-time
resource, so a dependency ADR), and a decision about whether there is an
installer at all.

**Phase 10 was decided on 2026-08-23 without being started, which is a state
this table needs a word for.** D16 asked what should compute an embedding;
the answer is all three sources as choices, with the profile as a ceiling and
`Cloud` behind a per-use gesture ([ADR-0056](docs/decisions/ADR-0056.md)) --
and the axis carrying those three values had existed since ADR-0020. The
phase is now **three queue items rather than one large thing**, listed in
`project/WORK_QUEUE.md`, and the first of them needs neither provider to
exist.

## Two numbering schemes, reconciled

The phase numbers above are canonical — they are what `specs.md` and the ADRs
refer to.

**They are no longer what any menu row refers to.** Until 2026-08-22 a greyed
row read "not implemented yet (phase 19)" and the number pointed here;
[ADR-0048](docs/decisions/ADR-0048.md) removed every such row, so a phase
number is now a planning device rather than something a user can see.

`project/tasks/` uses fourteen directories rather than twenty, because several
roadmap phases share a task folder (all three security phases live under
`11-security`, for example). The mapping is the **Tasks** column. The
directories are deliberately *not* renumbered: they are a coarser grouping of
the same plan, not a competing one, and renaming them would break every link
that points at them for no gain.

## Where the work actually is

Phases 1, 3 and 4 are complete. **Every remaining phase now has a tested slice
reachable in the product**, and phases 9, 12 and 13 — the three that were
"built with no way in" for four sessions — joined them across 2026-08-21/22.
Phase 12 has since left altogether ([ADR-0057](docs/decisions/ADR-0057.md)),
which does not undo that: it was reachable for a week before it was removed,
and being reachable is what made the removal a decision about the product
rather than a tidy-up of dead code.

**No crate in this workspace is unreachable.** That sentence has been the
headline of this section since the fourth session, when it was three crates,
9,556 lines and 241 unit tests behind code no user could reach. `bp-storage`
left under [ADR-0037](docs/decisions/ADR-0037.md), `bp-notebook` and
`bp-execution` under [ADR-0043](docs/decisions/ADR-0043.md), and
`bp-research` under [ADR-0044](docs/decisions/ADR-0044.md).

**Three of those four crates have since been deleted** — `bp-notebook` and
`bp-execution` under [ADR-0057](docs/decisions/ADR-0057.md), `bp-research`
under [ADR-0060](docs/decisions/ADR-0060.md). The sentence still holds and
means less than it did: reaching zero by wiring and reaching it by deletion
are different achievements, and only `bp-storage` reached it the first way and
stayed.

**What actually unblocked the last three was not effort.** Each had been sized
as a *mode* -- a large, unscoped thing -- and each turned out to be waiting on
a question rather than on work: phase 12 on what a notebook is *for*
([ADR-0043](docs/decisions/ADR-0043.md)), phase 13 on whether two decisions
were a contradiction ([ADR-0044](docs/decisions/ADR-0044.md)), and phase 9 on
presentation ([ADR-0037](docs/decisions/ADR-0037.md)). Research mode's last
five planned rows went the same way one session later, in the opposite
direction ([ADR-0046](docs/decisions/ADR-0046.md)).

**The lesson from all four -- "undecided reads like large", and its mirror --
is in [DECISIONS.md](DECISIONS.md)**, which is the one home for a lesson. It
is named here rather than retold because it decides whether a phase can be
estimated at all, which is this file's subject.

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
and they are green -- 39 tests, one `#[ignore]`d for cost with the reason on
it. There are four harnesses rather than five: the notebook target went with
the crate it protected ([ADR-0057](docs/decisions/ADR-0057.md)), and it is
named above because it is part of why this stage exists. The gate is about two and a half minutes per leg longer, which is what
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
Standard, Private, Confidential and Maximum permits across six axes, and the
named profiles are checked to be monotonic — each at least as restrictive as
the one before, on every axis. **Two of the six have a dependant that reads
them**, and both do: the recovery journal refuses rather than writing
plaintext (and deletes what a looser profile already wrote), and
`bp-storage`'s `record_document` drops the title under `PathOnly` and records
nothing under `Disabled`. There were seven axes and three dependants until
[ADR-0061](docs/decisions/ADR-0061.md) removed the clipboard and its axis
together; the other four axes are reported by the Tools inspector and
consulted by nothing ([ADR-0059](docs/decisions/ADR-0059.md) §4). The Security menu sets the profile and
states what it permits; the status bar shows anything other than the default.

Two limits are deliberate and visible. Profiles requiring an encrypted journal
cannot be honoured until `bp-crypto` (phase 15), so recovery is refused rather
than downgraded — the menu says "off until encryption ships". And `bp-storage`
is still not called by the application: that is ADR-0019's product decision,
which the profile model makes revisitable rather than reversible by hand.
