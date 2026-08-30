// A benchmark is scaffolding, and `clippy.toml`'s `allow-unwrap-in-tests`
// does not reach it -- that setting recognises `#[test]` functions and
// `#[cfg(test)]` blocks, and a bench target is neither. Same intent as
// `bp-buffer`'s: a fixture that cannot be built has nothing to measure.
#![allow(clippy::unwrap_used, clippy::panic)]

//! What a frame of scrolling costs, and whether it costs more further down.
//!
//! ## Why this exists
//!
//! [`Anchor`]'s own doc comment makes a performance claim:
//!
//! > Two fields rather than one global row index, because a global index
//! > would have to be computed by laying out every line above it -- an
//! > O(document) cost on every frame, on the one operation that has to stay
//! > cheap. Anchored to a line and an offset within it, **scrolling costs
//! > only what is on screen.**
//!
//! Ask `CLAUDE.md`'s trap 3 of that sentence -- *what would fail if it
//! stopped being true?* -- and until this file the answer was **nothing**. A
//! change that made `visible_rows` count from line 0 would have passed every
//! test in the workspace and both gate legs, and shown up only as an editor
//! that got slower the further you scrolled.
//!
//! ## What it measures, and what it cannot
//!
//! [ADR-0017](../../../docs/decisions/ADR-0017.md) chose the software
//! renderer and left a revert condition: *"if scroll smoothness or the
//! renderer's contribution to input latency regresses perceptibly, the
//! default returns to the platform renderer"*. That condition has never been
//! checkable, for two reasons and only one of them is fixable here.
//!
//! **Fixable, and fixed:** the application's own per-frame work. Building the
//! rows a frame draws is `bp-editor`'s job, it happens on the UI thread, and
//! it is measurable without a window.
//!
//! **Not fixable here:** what the compositor then does with those rows.
//! Rasterisation cost and end-to-end input latency need a capture rig this
//! environment does not have, exactly as ADR-0017 said. **This benchmark does
//! not measure smoothness**, and a green run here is not evidence that
//! scrolling looks smooth -- only that the half this process controls is not
//! where a regression came from.
//!
//! Saying which half is measured is the point. "Perceptibly" cannot fail a
//! run; a number can.
//!
//! Run with `cargo bench -p bp-editor`. Redirect stdout into
//! `artifacts/benchmarks/` to keep a run; this program writes no files.

use std::process::Command;
use std::time::{Duration, Instant};

use bp_buffer::Buffer;
use bp_editor::view::{self, Anchor, Layout};
use bp_editor::wrap::Wrap;

/// Document sizes, in lines. Spread over three orders of magnitude because
/// the claim under test is that the numbers do **not** move with this column.
/// A tidy progression would hide that; a wide one makes it obvious.
const SIZES: &[usize] = &[1_000, 10_000, 100_000, 1_000_000];

/// A 72-byte line: the shape of prose and of source alike.
const SHORT: &str = "the quick brown fox jumps over the lazy dog while the sun sets ok";

/// Rows in a viewport. 50 is a maximised window at this line height; the cost
/// is linear in this and that is the intended shape.
const ROWS: usize = 50;

/// The frame budget ADR-0017 measures against, in milliseconds.
const FRAME_BUDGET_MS: f64 = 16.0;

/// What one frame's row-building may cost before it is a regression.
///
/// **A tenth of the frame budget, not the whole of it.** The compositor,
/// layout and the rest of the application share that 16 ms, and this is one
/// step of one of them. A threshold set at the budget would only fail once
/// the editor was already unusable.
const ROW_BUILD_BUDGET_MS: f64 = 1.6;

fn corpus(lines: usize, line: &str) -> Buffer {
    let mut s = String::with_capacity(lines * (line.len() + 1));
    for _ in 0..lines {
        s.push_str(line);
        s.push('\n');
    }
    Buffer::from(s.as_str())
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// Time `f` over enough repetitions to be worth reading, and return the
/// per-call cost.
///
/// A warm-up pass first, discarded: the first call through a fresh rope pays
/// for cache misses that the second does not, and reporting that as the cost
/// of a frame would overstate every steady-state scroll by the one frame
/// nobody notices.
fn per_call<T>(reps: u32, mut f: impl FnMut() -> T) -> Duration {
    let _ = f();
    let started = Instant::now();
    for _ in 0..reps {
        std::hint::black_box(f());
    }
    started.elapsed() / reps
}

fn shell(program: &str, args: &[&str]) -> String {
    Command::new(program)
        .args(args)
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_owned())
}

fn metadata() {
    println!("# bp-editor scroll benchmark");
    println!(
        "commit:    {}",
        shell("git", &["rev-parse", "--short", "HEAD"])
    );
    println!("toolchain: {}", shell("rustc", &["--version"]));
    println!(
        "os:        {} {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    println!(
        "machine:   {}",
        std::env::var("COMPUTERNAME")
            .or_else(|_| std::env::var("HOSTNAME"))
            .unwrap_or_else(|_| "unknown".to_owned())
    );
    // `date` on Linux, PowerShell on Windows. Neither is present on both, and
    // a benchmark whose metadata block panics has measured nothing.
    let date = if cfg!(windows) {
        shell(
            "powershell",
            &["-NoProfile", "-Command", "Get-Date -Format o"],
        )
    } else {
        shell("date", &["-Is"])
    };
    println!("date:      {date}");
    println!(
        "profile:   {}",
        if cfg!(debug_assertions) {
            "debug -- numbers are NOT comparable to the recorded thresholds"
        } else {
            "release"
        }
    );
    println!();
}

/// One row for the table below: what a frame costs at some depth into a
/// document.
struct Row {
    lines: usize,
    label: &'static str,
    build: Duration,
    step_one: Duration,
    step_page: Duration,
}

fn measure(lines: usize, line: &str, layout: Layout) -> Vec<Row> {
    let buffer = corpus(lines, line);

    // Top, middle and end. **The end is the one that matters**: if anything
    // in the path walks from line 0, this column grows with the size column
    // and the claim is refuted.
    let places: [(&'static str, usize); 3] = [
        ("top", 0),
        ("middle", lines / 2),
        ("end", lines.saturating_sub(ROWS + 1)),
    ];

    places
        .into_iter()
        .map(|(label, at)| {
            let anchor = Anchor::at(at, 0);
            Row {
                lines,
                label,
                build: per_call(200, || view::visible_rows(&buffer, anchor, ROWS, layout)),
                // One wheel notch.
                step_one: per_call(200, || view::step_row(&buffer, anchor, 1, layout)),
                // A page: what Page Down asks for, and the largest single
                // jump an ordinary scroll makes.
                step_page: per_call(200, || {
                    view::step_row(&buffer, anchor, ROWS as isize, layout)
                }),
            }
        })
        .collect()
}

fn table(title: &str, note: &str, rows: &[Row]) -> Vec<String> {
    println!("## {title}");
    println!();
    println!("{note}");
    println!();
    println!("| lines | at | build {ROWS} rows | step 1 row | step {ROWS} rows |");
    println!("| --- | --- | --- | --- | --- |");

    let mut failures = Vec::new();
    for row in rows {
        println!(
            "| {:>9} | {:<6} | {:>9.1} µs | {:>9.1} µs | {:>9.1} µs |",
            row.lines,
            row.label,
            row.build.as_secs_f64() * 1e6,
            row.step_one.as_secs_f64() * 1e6,
            row.step_page.as_secs_f64() * 1e6,
        );
        if ms(row.build) > ROW_BUILD_BUDGET_MS {
            failures.push(format!(
                "{} lines at {}: building {ROWS} rows took {:.2} ms",
                row.lines,
                row.label,
                ms(row.build)
            ));
        }
    }
    println!();
    failures
}

/// The claim, stated as a ratio rather than a time.
///
/// Times vary with the machine and are worthless as a cross-machine
/// threshold; a *ratio between two columns of the same run* is not. If
/// scrolling costs only what is on screen, the largest document costs about
/// what the smallest does, and the ratio sits near 1.
fn independence(rows: &[Row]) -> Vec<String> {
    let at_end = |lines: usize| {
        rows.iter()
            .find(|r| r.lines == lines && r.label == "end")
            .map(|r| r.build)
    };
    let (Some(small), Some(large)) = (at_end(SIZES[0]), at_end(SIZES[SIZES.len() - 1])) else {
        return vec!["could not find both ends of the size range".to_owned()];
    };

    let ratio = large.as_secs_f64() / small.as_secs_f64().max(f64::EPSILON);
    println!(
        "Cost at the end of a {} line document vs a {} line one: **{ratio:.2}x**",
        SIZES[SIZES.len() - 1],
        SIZES[0]
    );
    println!();

    // Generous, deliberately. The claim is "does not scale with the
    // document", not "is identical" -- a rope's depth grows with its
    // contents, so a bigger document does cost a little more to index into,
    // and the difference between logarithmic and linear is what this catches.
    // At these sizes a linear walk would be four orders of magnitude, not
    // four times.
    if ratio > 4.0 {
        vec![format!(
            "building a frame at the end of a {}-line document cost {ratio:.2}x \
             what it cost in a {}-line one -- something in the scroll path is \
             walking the document",
            SIZES[SIZES.len() - 1],
            SIZES[0]
        )]
    } else {
        Vec::new()
    }
}

fn main() {
    metadata();

    println!(
        "Frame budget {FRAME_BUDGET_MS:.0} ms; this measures **one step of one part** of a \
         frame, so the threshold below is a tenth of it ({ROW_BUILD_BUDGET_MS} ms)."
    );
    println!();

    let unwrapped: Vec<Row> = SIZES
        .iter()
        .flat_map(|&n| measure(n, SHORT, Layout::default()))
        .collect();
    let mut failures = table(
        "Unwrapped",
        "One visual row per document line -- the default surface's shape.",
        &unwrapped,
    );

    let wrapped_layout = Layout {
        wrap: Wrap::at(40),
        tab_width: 4,
    };
    let wrapped: Vec<Row> = SIZES
        .iter()
        .flat_map(|&n| measure(n, SHORT, wrapped_layout))
        .collect();
    failures.extend(table(
        "Wrapped at 40 columns",
        "Each 64-character line becomes two rows, so `wrap::row_starts` runs \
         for every line drawn. This is the more expensive path and the one \
         `--editor-view` takes.",
        &wrapped,
    ));

    println!("## Does the cost depend on document size?");
    println!();
    failures.extend(independence(&unwrapped));

    println!("## Thresholds");
    println!();
    println!(
        "  build {ROWS} rows   < {ROW_BUILD_BUDGET_MS} ms   (a tenth of a {FRAME_BUDGET_MS:.0} ms frame)"
    );
    println!("  size independence  < 4x     (largest document vs smallest, same run)");
    println!();
    if failures.is_empty() {
        println!("  all within threshold.");
    } else {
        for failure in &failures {
            println!("  REGRESSION: {failure}");
        }
    }
    println!();
    println!(
        "**Not measured here:** rasterisation and end-to-end input latency, which \
         are the other half of ADR-0017's revert condition and need a capture rig."
    );
}
