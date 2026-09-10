# Testing and the gate

## The gate

```powershell
./scripts/Invoke-LocalCI.ps1 -Linux    # both legs, about 7 minutes
./scripts/Invoke-LocalCI.ps1 -Quick    # what pre-commit runs
./scripts/Invoke-LocalCI.ps1           # the Windows leg in full
```

Every commit in this repository has passed it.

Stages, in order: `fmt`, `check (locked)`, `clippy`, `licences`, `notices`,
`test`, `doctests`, `launch`, `log hygiene`, `fuzz`, and the Linux leg through
WSL.

`.github/workflows/ci.yml` runs the same stages under the same names on
Windows and Ubuntu runners ([ADR-0073](../decisions/ADR-0073.md)). **When the
two disagree, the local one is right.**

Three rules about running it:

- **Background it, and do not edit files while it runs.** The Linux leg
  re-runs `cargo fmt --check` from scratch, so a mid-run edit fails the run
  for a reason unrelated to your change.
- **`cargo fmt --all` before you start it.** Always.
- **Never pipe it through `tail`, `head` or `Select-Object`.** A failure's
  detail is *above* the summary. It happened once and a failing test could not
  be named afterwards.

`BPAD_SKIP_CI=1` bypasses the hooks. That is deliberate: a gate with no escape
hatch gets disabled wholesale the first time it is wrong.

## Writing tests here

**Test names are full sentences.**
`fn a_no_op_must_not_create_an_undo_entry()`. They are read in failure output
by somebody who does not have the file open.

**Use `crate::testpaths` for temporary paths.** Not a process id and a
counter: that is unique within a run and reused by the next process that gets
the same pid. It made the suite flakier the more it was run -- 5,862 leftover
files across 495 pids, and a recycled pid inheriting an earlier run's history
([ADR-0049](../decisions/ADR-0049.md)).

**Never read the real profile directory.** A function that does means
`cargo test` writes there. The fix is a field on `AppState`, not a function.

**Do not let `std::path` answer for a `Platform` parameter.**
`bp_platform::paths::{file_name, join}` exist for this.

## The four traps, and three about verification

Named in `CLAUDE.md`, explained in `DECISIONS.md`. The ones that bite tests:

> **A claim in a comment is not a property of the code.** Ask: what would fail
> if this stopped being true? If nothing, the comment is a wish.

> **Two claims about the same function coexist for as long as nothing asks.**
> When trap 3's answer is *nothing would fail*, the test beside the comment is
> free to say the opposite, and probably does.

> **A round trip through one build says nothing about another build.** Seal
> and open agreeing with each other is weaker than it reads, and it is what
> almost every format test actually asserts. The test that means something is
> a byte vector somebody committed *before* the change.

And the newest one, which cost this project a rename:

> **A test that proves two things agree is evidence about two things.** Before
> trusting a constant as the one home for a value, grep for the value. If the
> count is higher than the number of readers, the constant is a convention
> rather than a mechanism ([ADR-0074](../decisions/ADR-0074.md)).

## What no test can catch

**What the toolkit does with what it is handed.** Five defects have lived in
that seam, including a signing passphrase typed into the open document in
plain text. 1,812 passing tests did not see it.

`project/NEXT_SESSION.md` section 3 has the manual checklist and says which
parts need a person and why. `scripts/Drive-Window.ps1` automates some of it:

```powershell
./scripts/Drive-Window.ps1 -Kill
./scripts/Drive-Window.ps1 -Keys "^g","555{ENTER}"
```

**Say before you drive it.** It takes the keyboard from whoever is at the
machine. On 2026-08-30 it captured nine characters somebody was typing, wrote
them into the document under test, and a defect was nearly reported out of the
contaminated evidence.

And the general form: **when a manual pass produces a surprising result,
suspect the pass before the product.** An automated test that is wrong usually
fails; a manual one that is wrong quietly hands you a finding.

## Fuzzing

`fuzz/` is a separate workspace holding hostile-input harnesses for `bp-files`
and `bp-formats`, plus a corpus. The gate runs its `fmt`, `clippy` and `test`
stages; it is not run continuously.
