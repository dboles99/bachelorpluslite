# Rosetta R014: Land It

Last step. The one that gets skipped, because the code already works.

## Gate

```powershell
cargo fmt --all                      # always, before starting the gate
./scripts/Invoke-LocalCI.ps1 -Linux  # this IS the gate (ADR-0016)
```

- **Do not edit files while it runs.** The Linux leg re-runs `cargo fmt
  --check` from scratch and a mid-run edit fails it for an unrelated reason.
- **Read the summary table, not the tail.** `test` and `launch` can pass while
  `clippy` has already failed. Reporting green off the last few lines has
  happened and was wrong.
- **The Linux leg is not a formality.** It has caught a `#[cfg(unix)]` test
  the Windows leg never compiles. Both legs, every time.
- A `Invoke-LocalCI.ps1` change is its own commit, never a passenger. It is
  the thing that validates everything else.

## Record

Update what is now untrue. Each of these has been left stale before:

| File | When |
| --- | --- |
| `README.md` | user-visible behaviour changed; the crate and test counts; the known-gaps list |
| `ROADMAP.md` | a phase moved between Not started / Started / Built, unwired / Done |
| `docs/product/MENU_MAP.md` | any menu row changed state |
| `project/WORK_QUEUE.md` | an item was taken, finished, or turned out to be blocked |
| `project/DECISIONS_NEEDED.md` | a question was answered, or a new one appeared |
| `DECISIONS.md` + `docs/decisions/` | the decision has consequences beyond one file |

**Take the test count from the gate, not from arithmetic.** Adding up what you
think you added has been wrong.

**"Blocked" means checked, not assumed.** Two items were recorded as blocked
this way — drag-and-drop and input-method composition — and both name the file
and function in the dependency that proves it. A blocker without evidence is a
guess that will cost somebody an afternoon.

**And checked means the question was the right one.** Drag-and-drop was
checked, and the check was true -- the winit backend really has no file-drop
plumbing -- and it was still not a blocker, because the application can take
winit's events directly ([ADR-0081](../../docs/decisions/ADR-0081.md)). Before
recording a dependency as blocking something, search its public API for the
*noun*, not for the mechanism you expected: `grep -rn StyledText` would have
found the rich-text item that D14 was answered as lacking.

## Commit

Say *why*, at whatever length that takes. This repository's log is a design
record: the trap avoided, the trade made, the thing that turned out to be
false. Include the gate result and the test count.

## Done when

- the gate passed on both legs;
- the working tree is clean;
- nothing in the table above is stale;
- anything only a person at a keyboard can confirm is stated as such, rather
  than reported as working.
