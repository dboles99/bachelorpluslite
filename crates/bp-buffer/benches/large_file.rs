//! The measurement behind `LARGE_FILE_BYTES` and `HUGE_FILE_BYTES`, and the
//! evidence that the chunked path costs what it claims to.
//!
//! Rosetta R008 asks for realistic sizes, cold and warm runs, CPU and memory,
//! a harness, and metadata. There is no criterion here on purpose: it is not
//! in the workspace dependency set, and criterion's model -- many iterations
//! of a small operation -- is the wrong shape for "open a 256 MiB file
//! once, cold". The thing being measured is a one-shot with a page cache in
//! it, so the harness is a loop with a clock and an explicit cold/warm split.
//!
//! Run with `cargo bench -p bp-buffer`. Redirect stdout into
//! `artifacts/benchmarks/` to keep a run; this program deliberately writes no
//! files outside the temporary directory it cleans up.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use bp_buffer::{INDEX_BUDGET_BYTES, LargeFile, SizeClass, WindowReader};

/// Sizes to measure, in MiB. Chosen around the two thresholds rather than in
/// a tidy progression: the interesting question is where the budget breaks,
/// not what the curve looks like.
const WHOLE_FILE_SIZES_MIB: &[usize] = &[1, 4, 6, 8, 10, 12, 16, 32, 64, 128, 192, 224, 256];

/// The size used for the chunked-path measurements. Above `HUGE_FILE_BYTES`,
/// because that is the regime the chunked path exists for.
const CHUNKED_SIZE_MIB: usize = 256;

/// ~72-byte lines: the shape of prose and of source alike, and short enough
/// that the line index is under pressure rather than trivially small.
const LINE: &str = "the quick brown fox jumps over the lazy dog while the sun sets ok\n";

fn corpus(bytes: usize) -> String {
    let mut s = String::with_capacity(bytes + LINE.len());
    while s.len() < bytes {
        s.push_str(LINE);
    }
    s
}

/// Write exactly `bytes` bytes, ending on a line break.
///
/// Exactly matters: writing in whole blocks makes 1 MiB and 4 MiB the same
/// file, and a threshold measured on a table where two rows are the same
/// file is not a measurement.
fn write_file(path: &Path, bytes: usize) -> u64 {
    let block = corpus(4 * 1024 * 1024);
    let lines = block.len() / LINE.len();
    let block = &block[..lines * LINE.len()];
    let mut file = File::create(path).unwrap();
    let mut written = 0;
    // Whole lines only, and never past the requested size: a file 58 bytes
    // over 8 MiB classifies as Large and puts the row on the wrong side of
    // the very threshold the table is measuring.
    while written + LINE.len() <= bytes {
        let want = ((bytes - written) / LINE.len() * LINE.len()).min(block.len());
        file.write_all(&block.as_bytes()[..want]).unwrap();
        written += want;
    }
    file.sync_all().unwrap();
    std::fs::metadata(path).unwrap().len()
}

/// Resident set of this process, in MiB.
///
/// Shelling out to PowerShell rather than adding a dependency or an `unsafe`
/// block: this is a benchmark, it runs once, and 200 ms of process spawn is
/// noise against the thing being measured.
fn working_set_mib() -> f64 {
    let pid = std::process::id();
    let out = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &format!("(Get-Process -Id {pid}).WorkingSet64"),
        ])
        .output();
    match out {
        Ok(out) => {
            String::from_utf8_lossy(&out.stdout)
                .trim()
                .parse::<f64>()
                .unwrap_or(f64::NAN)
                / (1024.0 * 1024.0)
        }
        // Not Windows, or no PowerShell. The timings are still the point.
        Err(_) => f64::NAN,
    }
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1e3
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
    println!("# bp-buffer large-file benchmark");
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
    println!(
        "date:      {}",
        shell(
            "powershell",
            &["-NoProfile", "-Command", "Get-Date -Format o"]
        )
    );
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

/// What it costs to do it the ordinary way: read the file, check it is text,
/// build the rope. This is the measurement the two thresholds come from.
fn whole_file(dir: &Path) {
    println!("## Whole-file load (the path `SizeClass::Normal` takes)");
    println!(
        "{:>6} {:>10} {:>10} {:>9} {:>9} {:>11} {:>12} {:>8}",
        "MiB", "cold ms", "warm ms", "utf8 ms", "rope ms", "cold tot ms", "resident MiB", "class"
    );

    let base = working_set_mib();
    for mib in WHOLE_FILE_SIZES_MIB {
        let path = dir.join(format!("whole{mib}.txt"));
        let len = write_file(&path, mib * 1024 * 1024);

        // Cold-ish: the file was just written, so this is the first read of
        // it through the ordinary path, virus scanner and all. That is what
        // a user's double-click actually costs.
        let t = Instant::now();
        let bytes = std::fs::read(&path).unwrap();
        let cold = t.elapsed();
        drop(bytes);

        let mut warm = Duration::MAX;
        let mut bytes = Vec::new();
        for _ in 0..3 {
            let t = Instant::now();
            bytes = std::fs::read(&path).unwrap();
            warm = warm.min(t.elapsed());
        }

        let t = Instant::now();
        let text = String::from_utf8(bytes).unwrap();
        let utf8 = t.elapsed();

        let t = Instant::now();
        let rope = ropey::Rope::from_str(&text);
        let rope_ms = t.elapsed();

        drop(text);
        let resident = working_set_mib() - base;
        println!(
            "{mib:>6} {:>10.1} {:>10.1} {:>9.1} {:>9.1} {:>11.1} {:>12.1} {:>8?}   lines={}",
            ms(cold),
            ms(warm),
            ms(utf8),
            ms(rope_ms),
            ms(cold) + ms(utf8) + ms(rope_ms),
            resident,
            SizeClass::of(len),
            rope.len_lines(),
        );
        drop(rope);
        std::fs::remove_file(&path).ok();
    }
    println!();
    println!("The budget: specs.md section 22 allows 150 ms to window and 250 ms to");
    println!("first interaction, so roughly 100 ms is left for getting content up.");
    println!("`LARGE_FILE_BYTES` is the last size whose cold total fits it.");
    println!("`HUGE_FILE_BYTES` is the last size under a 256 MiB resident ceiling,");
    println!("given the rope's measured ~1.16x multiplier over the file.");
    println!();
}

/// What the chunked path costs on a file the ordinary path refuses.
fn chunked(dir: &Path) {
    println!("## Chunked path (the path `SizeClass::Huge` takes)");
    let path = dir.join("huge.txt");
    let len = write_file(&path, CHUNKED_SIZE_MIB * 1024 * 1024);
    let base = working_set_mib();

    let t = Instant::now();
    let mut file = LargeFile::open(&path).unwrap();
    let open = t.elapsed();

    let t = Instant::now();
    let first = file.display_lines(0, 60).unwrap();
    let first_frame = t.elapsed();
    assert_eq!(first.len(), 60);

    // A viewport a long way in, cold: nothing can know where line 2,000,000
    // starts without scanning to it, so this is the honest cost of the first
    // jump. Then the same viewport with the index in hand, which is what
    // every subsequent scroll costs.
    let t = Instant::now();
    let deep = file.display_lines(2_000_000, 60).unwrap();
    let deep_cold = t.elapsed();
    assert_eq!(deep.len(), 60, "line 2,000,000 must exist in this corpus");

    let t = Instant::now();
    let progress = file.index_fully().unwrap();
    let index_all = t.elapsed();

    let t = Instant::now();
    let deep = file.display_lines(2_000_000, 60).unwrap();
    let deep_warm = t.elapsed();
    assert_eq!(deep.len(), 60);
    let resident_after_index = working_set_mib() - base;

    println!(
        "file:                     {} MiB ({len} bytes)",
        len / (1024 * 1024)
    );
    println!("open (metadata only):     {:>9.3} ms", ms(open));
    println!("first 60 lines:           {:>9.3} ms", ms(first_frame));
    println!(
        "60 lines at line 2,000,000:{:>8.3} ms cold (scan), {:.3} ms indexed",
        ms(deep_cold),
        ms(deep_warm)
    );
    println!(
        "index the whole file:     {:>9.1} ms  ({:.2} GiB/s, {} lines, stride {})",
        ms(index_all),
        len as f64 / (1024.0 * 1024.0 * 1024.0) / index_all.as_secs_f64(),
        progress.lines,
        file.index().stride(),
    );
    println!(
        "index budget per call:    {} KiB -> {:.2} ms at that rate",
        INDEX_BUDGET_BYTES / 1024,
        INDEX_BUDGET_BYTES as f64 / len as f64 * ms(index_all),
    );
    println!(
        "resident after indexing:  {resident_after_index:>9.1} MiB   (held: {} KiB)",
        file.resident_bytes() / 1024
    );

    let t = Instant::now();
    let mut reader = WindowReader::open(&path).unwrap();
    let mut windows = 0u64;
    let mut bytes = 0u64;
    let mut peak = 0usize;
    while let Some(w) = reader.next_window().unwrap() {
        bytes += (w.text.len() - w.overlap_bytes) as u64;
        peak = peak.max(reader.resident_bytes() + w.text.capacity());
        windows += 1;
    }
    let stream = t.elapsed();
    assert_eq!(bytes, len);
    println!(
        "stream the whole file:    {:>9.1} ms  ({:.2} GiB/s, {windows} windows)",
        ms(stream),
        len as f64 / (1024.0 * 1024.0 * 1024.0) / stream.as_secs_f64(),
    );
    println!(
        "streaming peak held:      {:>9} KiB   (bound {} KiB, file {} MiB)",
        peak / 1024,
        reader.memory_bound_bytes() / 1024,
        len / (1024 * 1024),
    );
    println!();
    println!("Regression thresholds -- fail the run if any of these is exceeded:");
    println!("  open              <  5 ms   (it is a metadata call; growth means it reads)");
    println!("  first 60 lines    < 50 ms   (one seek and a few chunks, whatever the size)");
    println!("  streaming peak    <  8 MiB  (the stated bound, independent of file size)");

    let mut failures = Vec::new();
    if ms(open) > 5.0 {
        failures.push(format!("open took {:.1} ms", ms(open)));
    }
    if ms(first_frame) > 50.0 {
        failures.push(format!("first frame took {:.1} ms", ms(first_frame)));
    }
    if peak > 8 * 1024 * 1024 {
        failures.push(format!("streaming peak was {} KiB", peak / 1024));
    }
    if failures.is_empty() {
        println!("  all within threshold.");
    } else {
        println!("  REGRESSION: {}", failures.join("; "));
    }
    std::fs::remove_file(&path).ok();
    println!();
}

fn main() {
    metadata();
    let dir: PathBuf = std::env::temp_dir().join("bp_buffer_large_file_bench");
    std::fs::create_dir_all(&dir).unwrap();
    whole_file(&dir);
    chunked(&dir);
    std::fs::remove_dir_all(&dir).ok();
}
