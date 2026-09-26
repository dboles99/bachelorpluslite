# Working in this repository

BachelorPad+ Lite — a Rust/Slint text editor. *Notepad when you want it. More
when you need it.*

**Public since 2026-09-10, GPL-3.0-only, version 0.9.5.** The product name is
`BachelorPad+ Lite` and lives in one place, `bp_platform::DISPLAY_NAME`
([ADR-0074](docs/decisions/ADR-0074.md)) — do not spell it out anywhere, and
read that ADR before you are tempted to, because the last two renames both
reached some of the sites and not all of them.

**This file is the entry point, and it is deliberately short.** Everything it
says is a rule you need before you touch anything; everything else is one link
away. If this file grows past a screen or two it has stopped doing its job.

---

## Start here, in this order

1. **Ask the open questions before doing anything else.** See "How a session
   opens", below. This is the rule most often skipped and it is the one that
   costs the most.
2. `project/WORK_QUEUE.md` — what is *ready*. Take the top item.
3. `prompts/rosettas/R011_WIRE_CAPABILITY_INTO_SHELL.md` — **required** before
   touching `crates/bp-ui/`. It holds the traps that have each cost somebody a
   session.
4. `project/NEXT_SESSION.md` — what matters most, which is not the same list
   as what is ready.

> **Items 2 and 4 live in a private repository**, and have since this one went
> public on 2026-09-10:
> [dboles99/bachelorpluslite-planning](https://github.com/dboles99/bachelorpluslite-planning).
> `project/` and the marketing plan moved there because they hold commercial
> strategy and open questions rather than reasoning. **Everything that
> explains why the software is the way it is stayed here** — every ADR, every
> lesson in `DECISIONS.md`, the whole manual.
>
> Roughly seventy-five references to those filenames remain in the ADRs and
> were deliberately left alone. *"The checklist is in
> `project/NEXT_SESSION.md` section 3"* is a true statement about how this
> project was run, and rewriting seventy-five of them to say something vaguer
> would damage the record to hide a filename. Only the fourteen actual links
> were changed, because a link that 404s is a different thing from a
> reference that names something.
>
> **If you are working on this from outside, you do not need them.** They
> schedule work; they do not explain it.

---

## Standing authorities

**You do not need to ask for any of these.** They were granted once, on
2026-08-22, precisely so that a session spends its time on work rather than on
round trips.

### 1. Take the top ready item and finish it

Do not ask which item, and do not ask whether to start. If the queue is empty,
say so and stop — that is a real result and it means the next work needs a
decision, not effort.

Take the *whole* item, including its documentation. "Whoever finishes an item
updates the record as part of it" is not optional; a fix that lands without
the record moving is how every stale sentence in this repository got there.

### 2. Commit and push once the full gate is green

```powershell
./scripts/Invoke-LocalCI.ps1 -Linux    # ~7 min. Background it.
```

Green on **both legs** is the whole of the permission. Commit on the current
feature branch, push, and report. Do not ask first.

**Opening a pull request into `main` is covered too**, and this file said
otherwise for six sessions. D13 asked whether `main` gets the work and by what
route; [ADR-0053](docs/decisions/ADR-0053.md) answered it on 2026-08-22 --
**yes, by pull request, and an agent may open it.** This section went on
citing D13 as open, which is trap 4 exactly: two claims about the same act,
coexisting because nothing asked.

Two things this authority does *not* cover, and both need a human:

- **clicking merge.** It is the one act in this project that changes what a
  fresh clone gets, and ADR-0053 keeps it deliberately;
- `git push --force`, a rebase of pushed history, or anything that rewrites a
  commit somebody may already have.

### 3. Delete or rename a planned feature, with an ADR

If a row in `docs/product/MENU_MAP.md` would promise something this product
cannot keep, **say so and remove it.** Write the ADR; the ADR is the whole
point, because the decision is more valuable than the row would have been.

Two rows have gone this way and both were right:

- **DOI Lookup** — finding an identifier and resolving one are different acts,
  and only the first works offline (ADR-0006). [ADR-0044](docs/decisions/ADR-0044.md).
- **Research question, evidence, findings, methods, datasets** — five rows
  that were the section headings of a research *paper*, written into the plan
  by the scaffold commit and never designed. [ADR-0046](docs/decisions/ADR-0046.md).

> **A name that has sat in a plan long enough starts to read like a
> specification.** The tell: ask where the name came from. If the answer is
> the commit that scaffolded the repository rather than a decision that chose
> it, it is a sketch that has been read as a plan ever since.

---

## How a session opens

`project/DECISIONS_NEEDED.md` is a queue of questions only Daniel can answer.
Its top section is written to be pasted straight into a message.

**Ask all of it in your first response, then start work anyway.** Do not wait
for the answer, and do not stop halfway through an item to ask one thing. The
whole reason the file exists is that across five sessions, work has been
blocked by decisions far more often than by effort — and the reason questions
sit unanswered is that a session opens by *reading* rather than by *asking*.

If a question turns out to have an obvious default, it is not a question. Pick
the default, say which you picked, and delete the row.

---

## Where each fact lives

**One home per kind of fact.** A fact repeated in four files is a fact that
will be updated in three. When you need to mention something owned elsewhere,
**link to it — do not restate it.**

| File | Owns | Does *not* hold |
| --- | --- | --- |
| `README.md` | What works today, for a reader who has never seen the project | Lessons, plans, or why a thing was decided |
| `ROADMAP.md` | Phase status, and what is blocked on what | Anything about a single feature |
| `DECISIONS.md` | The ADR index, **and every lesson learned** | Anything not yet decided |
| `docs/decisions/ADR-*.md` | One decision each, with its reasoning | Status of the work that implements it |
| `docs/product/MENU_MAP.md` | Every menu row and its state | Implementation detail |
| `docs/architecture/ARCHITECTURE.md` | Crates, modules, sizes, seams | Feature descriptions |
| `project/DECISIONS_NEEDED.md` *(private repo)* | Open questions for a human | Answers — those move to an ADR and the row is deleted |
| `project/WORK_QUEUE.md` *(private repo)* | What is ready to take, and collision rules | Why it matters |
| `project/NEXT_SESSION.md` *(private repo)* | What matters most, and the manual-pass checklist | Anything a permanent file owns |
| `docs/product/MARKETING_PLAN.md` *(private repo)* | Positioning, channels, and what is not decided | Anything about how the software works |
| `prompts/rosettas/R011...` | The traps in `bp-ui` | Anything about a specific feature |
| `docs/user/`, `docs/tutorials/`, `docs/developer/` | The documentation, as source | Anything a generated page holds |
| `docs/generated/`, `app-help/`, `wiki/` | **Nothing. Generated** ([ADR-0075](docs/decisions/ADR-0075.md)) | Edits — the gate reverts them |
| `site/` | The website and the waitlist ([ADR-0076](docs/decisions/ADR-0076.md)) | Anything about the product a doc page owns |
| `CONTRIBUTING.md`, `SECURITY.md` | How an outsider contributes, and how to report a vulnerability | Lessons, or why a thing was decided |

**Lessons go in `DECISIONS.md` and nowhere else.** README may name one in a
clause and link; it must not retell it.

---

## The gate

```powershell
./scripts/Invoke-LocalCI.ps1 -Linux     # the real one, both legs, ~7 min
./scripts/Invoke-LocalCI.ps1 -Quick     # what pre-commit and pre-push run
./scripts/Install-GitHooks.ps1          # once per clone
./scripts/Drive-Window.ps1 -Kill        # launch the app and photograph it
```

**This is the gate, and it is authoritative.** Every commit in this repository
has passed it. Keep it that way.

**There is hosted CI as well, since 2026-09-10**, and this file said otherwise
for one line. [ADR-0016](docs/decisions/ADR-0016.md) opened *"GitHub Actions
is not available to this project"*; one `gh api` call disproved it, and
[ADR-0073](docs/decisions/ADR-0073.md) restored it. That is **trap 6 inside an
accepted ADR**, which is where a false premise is safest — settled is how
something stops being re-read.

The two gates have two jobs and must not drift:

| | Runs | Is the gate for |
| --- | --- | --- |
| `Invoke-LocalCI.ps1` | pre-commit, pre-push, by hand | Everything before a push. **Yours to run** |
| `.github/workflows/ci.yml` | push, pull request | A pull request from somebody who is not Daniel |
| `release.yml`, `docs.yml`, `site.yml` | a `v*` tag, `docs/**`, `site/**` | Releases, generated-doc staleness, the website |

**When the two disagree, the local one is right.** Hosted CI does the three
things a local gate structurally cannot: judge a stranger's PR, build on a
machine that has never built this (trap 7), and run the RustSec advisory
database, which needs a network the local gate is not allowed to assume.

- **Background it, and do not edit files while it runs.** The Linux leg
  re-runs `cargo fmt --check` from scratch, so a mid-run edit fails the run
  for a reason that has nothing to do with your change.
- `cargo fmt --all` before you start it, always.
- **Never pipe the gate through `tail`, `head` or `Select-Object`.** A
  failure's detail is *above* the summary, so truncating the output discards
  the only thing a red run is worth reading. It happened once, and a failing
  test could not be named afterwards.
- The hooks run `-Quick` only. **The full `-Linux` leg is yours to run**, and
  authority 2 depends on it — a push whose Linux leg was never run is a push
  that skipped half the gate.

---

## Non-negotiables

- Windows 10, Windows 11 and Linux are equal targets ([ADR-0001](docs/decisions/ADR-0001.md))
- Core editing never depends on cloud or AI services ([ADR-0006](docs/decisions/ADR-0006.md))
- No custom cryptography — compose vetted primitives ([ADR-0011](docs/decisions/ADR-0011.md))
- **This product executes nothing** ([ADR-0057](docs/decisions/ADR-0057.md)).
  It used to say "notebook content never auto-runs"; there is nothing left to
  auto-run, and a rule that cannot fail reads as live when it is vacuous
- File renames and moves need explicit user approval
- **No new dependency without saying so and giving the reason.** Dependency
  choices are ADR material here.
- Keep the UI thread non-blocking
- **GPL-3.0-only, never `-or-later`** ([ADR-0071](docs/decisions/ADR-0071.md)).
  Slint's grant is to version 3 and no other, so "or later" would offer terms
  this project has not been granted. A new dependency must pass
  `cargo deny check licenses`, and `THIRD-PARTY-NOTICES.md` is generated —
  regenerate it when the graph moves
- **Windows and Linux only.** macOS is declined with its cost written down
  ([ADR-0072](docs/decisions/ADR-0072.md)), and it would *compile* today,
  which is why the refusal needs a record rather than a silence
- **`docs/` is the one documentation source**
  ([ADR-0075](docs/decisions/ADR-0075.md)). `docs/generated/`, `app-help/` and
  `wiki/` are generated from it and gate-checked; editing them is a change the
  next run reverts

## House style

The repository has a strong and consistent voice. Match it.

- Comments explain **why**, not what. A comment restating the code is worse
  than no comment.
- `--` rather than an em dash inside code comments.
- Test names are full sentences: `fn a_no_op_must_not_create_an_undo_entry()`.
- British spelling in prose: behaviour, recognised, initialise.
- Every module has a `//!` header saying what it owns and what it deliberately
  does not.

## The traps this repository springs repeatedly

Named here, explained in `DECISIONS.md`:

1. **A function reading the real profile directory means `cargo test` writes
   there.** The fix is a field on `AppState`, not a function.
2. **Code taking a `Platform` parameter must not let `std::path` answer for
   it.** `bp_platform::paths::{file_name, join}` exist for this.
3. **A claim in a comment is not a property of the code.** Ask: what would
   fail if this stopped being true? If nothing, the comment is a wish.
4. **Two claims about the same function coexist for as long as nothing asks.**
   Trap 3 from the other side — when trap 3's answer is "nothing would fail",
   the test beside the comment is free to say the opposite, and probably does.

   **And its sharpest form, which cost a rename twice**
   ([ADR-0074](docs/decisions/ADR-0074.md)): *a test that proves two things
   agree is evidence about two things.* `DISPLAY_NAME` had a green test
   proving a **pair** agreed, a doc comment claiming three sites, and the name
   written out at **eight** — one of them a `.slint` literal no Rust test can
   see. **Before trusting a constant as the one home for a value, grep for the
   value.** If the count exceeds the number of readers, the constant is a
   convention rather than a mechanism.

**And three more, all about verification itself** ([ADR-0049](docs/decisions/ADR-0049.md),
[ADR-0050](docs/decisions/ADR-0050.md)):

5. **"Unique" must mean no *earlier* run either.** A temp path from a process
   id and a counter is unique within a run and reused by the next one that
   gets that pid. `crate::testpaths` is the one home.
6. **A claim in the record is not a property of the repository.** D13 sat for
   five sessions on a premise one `git` command disproved. A question that has
   waited several sessions should have its premise checked before it is asked
   again.

   **And an *accepted ADR* is where a false premise is safest**, because
   settled is how something stops being re-read — ADR-0016 rested on "GitHub
   Actions is not available to this project" and one `gh api` call disproved
   it ([ADR-0073](docs/decisions/ADR-0073.md)). **An ADR whose reasoning rests
   on an external fact should name the command that would test it.**
7. **A round trip through one build says nothing about another build.** Seal
   and open agreeing with each other is weaker than it reads, and it is what
   almost every format test actually asserts. The test that means something is
   a byte vector somebody committed *before* the change.

**And the one no test can catch:** what a toolkit does with what it is handed.
Five defects have lived in that seam, including a signing passphrase typed
into the open document in plain text. 1,812 passing tests did not see it.
`project/NEXT_SESSION.md` §3 has the manual checklist; some of it needs a
person, and it says which parts and why.
