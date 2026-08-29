# Working in this repository

BachelorPad+ — a Rust/Slint text editor. *Notepad when you want it. More when
you need it.*

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
   touching `crates/bp-ui/`. It holds ten traps that have each cost somebody a
   session.
4. `project/NEXT_SESSION.md` — what matters most, which is not the same list
   as what is ready.

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

Two things this authority does *not* cover, and both need a human:

- merging to `main`, or opening a pull request into it — that is decision
  **D13**;
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
| `project/DECISIONS_NEEDED.md` | Open questions for a human | Answers — those move to an ADR and the row is deleted |
| `project/WORK_QUEUE.md` | What is ready to take, and collision rules | Why it matters |
| `project/NEXT_SESSION.md` | What matters most, and the manual-pass checklist | Anything a permanent file owns |
| `prompts/rosettas/R011...` | The traps in `bp-ui` | Anything about a specific feature |

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

This **is** CI — there is no hosted CI ([ADR-0016](docs/decisions/ADR-0016.md)).
Every commit in this repository has passed it. Keep it that way.

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

**And three more, all about verification itself** ([ADR-0049](docs/decisions/ADR-0049.md),
[ADR-0050](docs/decisions/ADR-0050.md)):

5. **"Unique" must mean no *earlier* run either.** A temp path from a process
   id and a counter is unique within a run and reused by the next one that
   gets that pid. `crate::testpaths` is the one home.
6. **A claim in the record is not a property of the repository.** D13 sat for
   five sessions on a premise one `git` command disproved. A question that has
   waited several sessions should have its premise checked before it is asked
   again.
7. **A round trip through one build says nothing about another build.** Seal
   and open agreeing with each other is weaker than it reads, and it is what
   almost every format test actually asserts. The test that means something is
   a byte vector somebody committed *before* the change.

**And the one no test can catch:** what a toolkit does with what it is handed.
Five defects have lived in that seam, including a signing passphrase typed
into the open document in plain text. 1,812 passing tests did not see it.
`project/NEXT_SESSION.md` §3 has the manual checklist; some of it needs a
person, and it says which parts and why.
