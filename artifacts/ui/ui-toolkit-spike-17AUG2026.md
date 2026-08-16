# UI Toolkit Spike — Slint vs egui

**Task:** `project/tasks/01-foundation/02-ui-prototype.md`
**Date:** 2026-08-17
**Host:** Windows 10 Pro 19045, rustc 1.97.1
**Raw data:** `artifacts/benchmarks/ui-toolkit-20260817-060024.json`
**Harness:** `scripts/Measure-UiSpike.ps1`

## What was built

The same shell twice, in `spikes/ui-toolkit/`: a window with the 14-item menu
bar from `docs/product/MENU_MAP.md`, a tab strip, a line-number gutter beside
a live editable text buffer, and the two-line status bar from specs.md
section 3. Both carry the same four themes (Light, Dark, Organic, Green) with
identical palettes, switched at runtime by clicking the theme cell.

Both are standalone Cargo workspaces so the main workspace does not inherit a
GUI dependency tree for a toolkit that is still a candidate.

## Results

| Metric | Slint 1.17 | egui/eframe 0.36 | Target (specs.md §22) |
| --- | ---: | ---: | ---: |
| Cold launch, wall clock | 953 ms | 1412 ms | < 150 ms |
| Warm launch, wall clock (median of 6) | 588 ms | 1156 ms | < 75 ms |
| Warm launch, wall clock (best) | 578 ms | 1149 ms | — |
| `main()` → first frame rendered | 443 ms | 878 ms | — |
| Idle working set | 61.8 MB | 153.9 MB | < 50 MB |
| Idle private bytes | 68.6 MB | 361.1 MB | — |
| Release binary | 10.4 MB | 12.2 MB | — |

Release profiles are identical (`opt-level = 3`, `lto = true`,
`codegen-units = 1`, `strip = true`). Slint is faster and lighter on every
axis: roughly 2× on startup, roughly 2.5× on idle working set, 5× on private
bytes.

## The finding that matters most

**Neither toolkit comes close to the startup targets in specs.md section 22.**
The winner misses cold start by ~6× and warm start by ~8×. Idle RAM is 1.2×
over target for Slint and 3× over for egui.

The bulk of the time is graphics backend and driver initialisation, not
application code — the gap between process spawn and `main()` reaching a
rendered frame is where it goes, and it is roughly constant across runs.

This is a spec problem as much as an implementation problem. Three options,
none of which should be picked without measuring first:

1. **Try Slint's software renderer.** It removes GPU/driver init entirely.
   Likely a large win on startup for a text editor that does not need GPU
   compositing, at some cost in scrolling smoothness.
2. **Revise the targets.** 150 ms cold is a Notepad-class number, and Notepad
   is a thin shell over a native OS control with no independent renderer. A
   themed, GPU-rendered, cross-platform toolkit does not start that fast.
3. **Split the target.** Time-to-window and time-to-interactive are different
   promises; a shell that paints in 150 ms and finishes wiring behind it is a
   different engineering problem from one that does everything up front.

Recommendation: run (1) as a follow-up spike before touching the targets,
because it is cheap and could change the answer.

## Measurement caveats

- Wall clock is process spawn to process exit, with the shell quitting as
  soon as it has rendered one frame. It includes process teardown and
  `Start-Process` overhead, so treat it as an upper bound.
- "Cold" is a first launch after a build, not a post-reboot launch. The OS
  file cache is warm for shared system DLLs. Real cold start is worse.
- Both shells report at the same point: Slint on
  `RenderingState::AfterRendering`, egui on its first `logic()` call, in both
  cases after the graphics backend has drawn. An earlier version of this
  spike used a zero-duration Slint timer, which fires when the event loop
  starts turning rather than when anything is on screen; it reported 13 ms
  against egui's 890 ms and the two numbers were measuring different events.
  Those figures are not in this table.
- One machine, one GPU, Windows only. **Linux is not measured** — the WSL
  distro has no Rust toolchain. BP-ADR-0001 makes Linux first-class, so this
  is a real gap in the evidence, not a formality.

## Qualitative notes

**Slint.** Themes are a `global Palette` with computed properties; binding
`Palette.ink` into a widget restyles it wherever it appears, including the
text input primitive. Markup lives in `.slint` files compiled by a build
script, so the layout is separate from the Rust. The separation is real but
means two languages and a build-script step in the loop.

**egui.** Immediate mode: the whole UI is a function of state, re-run each
frame. Easy to reason about and very fast to iterate on. Theming goes through
`Visuals`, which is a fixed set of slots rather than an open palette — the
four themes fit, but ADR-0009's "restyle every surface" requirement pushes
against the grain, and fully custom surfaces mean painting them by hand.

**API churn** is a live risk for egui: 0.36 removed `App::update` in favour
of a required `App::ui`, moved every panel to take `&mut Ui` instead of
`&Context`, folded `TopBottomPanel` into a unified `Panel`, and changed
`TextEdit::frame` from `bool` to `Frame`. All four broke this spike, which
was written against the prior API. egui ships breaking releases roughly
quarterly. Slint 1.17 compiled against a 1.x API that has been stable since
1.0.

## Conclusion

Slint on measurements, on memory, and on theming fit. egui is the better
prototyping experience and would be the right answer for a tool where UI
iteration speed dominates — but this product's constraints are startup time,
idle footprint, and total control over four themes, and Slint is ahead on all
three.

Recorded as ADR-0015 (Proposed).
