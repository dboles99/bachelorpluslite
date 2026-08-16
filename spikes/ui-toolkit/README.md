# UI Toolkit Spike

Two implementations of the same BachelorPad+ shell, built to decide the UI
toolkit against measurements rather than reputation. See
`docs/decisions/ADR-0015.md` and
`artifacts/ui/ui-toolkit-spike-17AUG2026.md`.

Each shell renders the menu bar from `docs/product/MENU_MAP.md`, a tab strip,
a gutter beside a live editable buffer, and the two-line status bar from
specs.md section 3, with the four themes from ADR-0009 switched by clicking
the theme cell in the status bar.

## Why these are separate workspaces

Each has its own `[workspace]` table, so neither is a member of the root
workspace. A candidate toolkit's dependency tree should not land in
`cargo check --workspace` for every developer on every run, especially when
one of the two is going to be deleted.

The trade-off is that the normal local CI gate does not see them. Use
`./scripts/Invoke-LocalCI.ps1 -IncludeSpikes` to cover them.

## Building and measuring

```powershell
cd spikes/ui-toolkit/slint-shell ; cargo build --release
cd ../egui-shell               ; cargo build --release
cd ../../..
./scripts/Measure-UiSpike.ps1
```

## Measurement contract

Both binaries implement the same contract, which is what makes the numbers
comparable:

- `BPSPIKE_READY_MS=<f64>` is printed once, **after the first frame has been
  rendered** — Slint on `RenderingState::AfterRendering`, egui on its first
  `logic()` call. Getting this wrong is easy and quietly ruins the
  comparison: an earlier version used a zero-duration Slint timer, which
  fires when the event loop starts turning rather than when anything is on
  screen, and reported 13 ms against egui's 890 ms.
- `--measure-exit` quits as soon as that line is printed, so a wall-clock run
  terminates on its own.
- Without the flag the window stays up, so the harness can sample idle RSS.

Run without arguments to just look at it.

## Lifecycle

These are disposable. Once ADR-0015 is accepted, delete the loser and fold
the winner into `bp-ui`. Leaving both is how a spike turns into a second,
unmaintained implementation.
