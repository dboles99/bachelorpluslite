# UI Toolkit Spike — Slint vs egui, and the renderer question

**Task:** `project/tasks/01-foundation/02-ui-prototype.md`
**Date:** 2026-08-17
**Host:** Windows 10 Pro 19045, rustc 1.97.1
**Decisions:** ADR-0015 (toolkit), ADR-0017 (renderer and targets)
**Raw data:** `artifacts/benchmarks/ui-toolkit-20260817-062916.json` (spike),
`artifacts/benchmarks/startup-20260817-070513.json` (real application)

## What was built

The same shell twice: a window with the 14-item menu bar from
`docs/product/MENU_MAP.md`, a tab strip, a line-number gutter beside a live
editable text buffer, and the two-line status bar from specs.md section 3.
Both carried the same four themes with identical palettes.

Both spikes have since been deleted. The winner became `bp-ui`, and the
startup instrumentation moved into the real application — a benchmark fixture
measures the fixture and drifts from the product the moment either changes.

## Toolkit comparison

| Metric | Slint 1.17 | egui/eframe 0.36 |
| --- | ---: | ---: |
| Time to window | 98 ms | 197 ms |
| `main()` → first frame | 442 ms | 878 ms |
| Idle working set | 64.4 MB | 153.8 MB |
| Idle private bytes | 70.8 MB | 359.8 MB |
| Release binary | 10.4 MB | 12.2 MB |

Identical release profiles (`opt-level = 3`, `lto`, `codegen-units = 1`,
`strip`). Slint wins on every axis. → ADR-0015.

## The renderer finding

The first pass concluded that neither toolkit came close to the specs.md
section 22 targets. That conclusion was wrong, for two reasons.

**The metric was conflated.** "Startup" was one number covering two separately
observable events. Measured apart, on the real application:

| Renderer | Time to window | → first frame | Idle working | Idle private |
| --- | ---: | ---: | ---: | ---: |
| GPU (femtovg) | 129 ms | 411 ms | 62.7 MB | 69.1 MB |
| Software | **36 ms** | not measurable | **19.2 MB** | **6.4 MB** |

**The renderer was the wrong one.** Slint ships `renderer-software` in its
default features, selectable at runtime. Almost all of the GPU path's cost is
graphics backend and driver initialisation — which a text editor does not need.
With CPU rasterisation the application meets the window and RAM targets with
room to spare. → ADR-0017.

## Two measurement mistakes worth recording

Both produced numbers that looked plausible and were meaningless. Both are the
reason this report leads with method rather than results.

**A first-frame hook that fired before the first frame.** The Slint spike
initially timed a zero-duration `Timer`, which fires when the event loop starts
turning, not when anything is drawn. It reported **13 ms** against egui's
890 ms — a 68× "win" that was measuring a different event. Replacing it with
`RenderingState::AfterRendering` gave 442 ms.

**A crash that looked like a speed record.** Slint's software renderer has no
rendering notifier, and the spike called `.expect()` on it. Every run panicked
before drawing, and the harness dutifully recorded **~92 ms** — apparently
beating the 150 ms target. It was time-to-crash. The app now prints
`BPSPIKE_READY_UNAVAILABLE`, and the harness leaves the column blank. A blank
is honest; a zero would not be.

A related trap in the harness itself: the probe that detects the missing hook
used shell redirection, which captures nothing from a release binary built with
`windows_subsystem = "windows"`. It saw no marker, concluded the hook existed,
and timed the very lie it was there to catch. It now launches exactly like a
measurement run.

## Typing latency

`bachelorpad --latency-probe`, release build, Windows. Measures the
application's state path per keystroke — taking the edited text from the
widget, storing it, marking the document modified, refreshing the gutter. It
excludes rasterisation and presentation.

| Document | p50 | p95 | max |
| ---: | ---: | ---: | ---: |
| 1 KB | 0.6 µs | 0.7 µs | 1.4 µs |
| 10 KB | 4.0 µs | 4.1 µs | 9.0 µs |
| 100 KB | 37.8 µs | 39.1 µs | 93.9 µs |
| 1 MB | 376 µs | 387 µs | 857 µs |

Linear in document size, and 1 MB costs 2% of a 16 ms frame. The linearity is
inherent, not a bug in the probe: Slint's `edited` callback hands back the
entire buffer, so a keystroke copies the document. Extrapolating, the frame
budget is reached near 40 MB — which is what `bp-buffer` is for, and why it is
a phase-4 concern rather than a phase-1 one.

One avoidable cost was removed along the way: the gutter string was rebuilt on
every keystroke, allocating proportionally to the document even when no line
was added or removed. It is now rebuilt only when the line count changes.

## A third mistake: a manifest that stopped the app launching

Enabling rfd's `common-controls-v6` feature — which embeds a side-by-side
manifest dependency on ComCtl32 v6 for prettier task dialogs — made the binary
fail to start at all, with `STATUS_ENTRYPOINT_NOT_FOUND` (0xC0000139) before
`main`. It was caught only because the latency probe produced no output; a
window-only smoke test would have caught it too, but a `cargo build` and
`cargo test` run will not, since neither loads the linked executable.

## Measurement caveats

- Time to window is measured externally, from process spawn until the process
  owns a main window. It is the cross-renderer comparable clock. It does *not*
  mean content is drawn.
- **Time to first interaction is unverified under the software renderer.**
  Half of the ADR-0017 target has no measurement behind it.
- "Cold" is a first launch after a build, not post-reboot. Real cold start is
  worse.
- Typing latency and scroll smoothness under CPU rasterisation are unmeasured.
  ADR-0017 names this as its revert condition.
- **Linux timing is unmeasured.** The shell builds, links and runs on Linux
  (verified under WSLg, after installing `libxkbcommon-x11`), but WSLg's
  compositor makes startup numbers unrepresentative. Needs a native machine.

## Qualitative notes

**Slint.** Themes are a `global Palette` fed from Rust, so `bp-ui/ui/app.slint`
contains no colour literals and a user theme uses the same mechanism the
built-ins do — exactly what ADR-0009 asks for. Markup lives in `.slint` files
compiled by a build script: a real separation, at the cost of a second language
and a codegen step. The generated code also needs `unsafe`, so `bp-ui` uses
`deny(unsafe_code)` rather than `forbid`.

**egui.** Immediate mode, very fast to iterate on. Theming goes through
`Visuals`, a fixed set of toolkit-defined slots rather than an open palette;
the four themes fit, but fully custom surfaces mean hand-painting.

**API churn** is a live risk for egui. Version 0.36 broke four APIs used by a
shell this small: `App::update` removed in favour of a required `App::ui`,
panels moved from `&Context` to `&mut Ui`, `TopBottomPanel` folded into a
unified `Panel`, and `TextEdit::frame` retyped from `bool` to `Frame`. All four
broke this spike, written against the prior release. Slint 1.17 compiled
against an API stable since 1.0.

## Conclusion

Slint, with the software renderer, on measurements, memory and theming fit.
The product now meets its window and idle-RAM targets; what remains unproven
is time to first interaction and typing latency under CPU rasterisation.
