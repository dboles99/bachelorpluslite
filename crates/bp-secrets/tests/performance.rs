//! Timing floor for the scanner.
//!
//! specs.md section 22 wants large documents to stay responsive, and a scan
//! that runs on save or on a keystroke is only affordable if it is linear.
//! The bound below is deliberately loose -- a shared build machine under load
//! is nothing like a developer's laptop, and a tight bound would fail for
//! reasons that have nothing to do with this crate. It is set to catch the
//! failure that actually matters: someone adding a rule that rescans, and
//! turning a linear scan into a quadratic one.
//!
//! Measured on the development machine (Windows, stable toolchain), on a
//! document of prose, configuration and scattered credentials:
//!
//! | input | `--release` | unoptimised |
//! | ----- | ----------- | ----------- |
//! | 1 MB  | 16 ms       | 136 ms      |
//! | 2 MB  | 31 ms       | 266 ms      |
//! | 4 MB  | 64 ms       | 533 ms      |
//! | 8 MB  | 131 ms      | 1.07 s      |
//! | 10 MB | 175 ms      | 1.32 s      |
//! | 10 MB, all on one line | 176 ms | 1.33 s |
//!
//! Four times the bytes costs four times the time, and folding the whole
//! document onto one line costs nothing extra -- which is the shape being
//! defended here. Writing these tests found a real defect: overlap resolution
//! compared each candidate against every finding already accepted, which is
//! quadratic in findings-per-line and made the one-line case six times slower
//! than the same bytes split over many. The bound below is loose enough to
//! survive a loaded build machine and tight enough that the regression would
//! have failed it.

use std::time::{Duration, Instant};

use bp_secrets::scan;

/// Enough text that a quadratic scan cannot hide.
const TARGET_BYTES: usize = 10 * 1024 * 1024;

/// The generous bound described above.
const BUDGET: Duration = Duration::from_secs(10);

/// A document that looks like the ones people actually keep: mostly prose and
/// configuration, with a handful of real credentials scattered through it, and
/// plenty of the long strings a naive scanner would choke on.
fn large_document() -> String {
    let block = concat!(
        "# Deployment notes\n",
        "\n",
        "The staging cluster is rebuilt every Monday from the manifest in\n",
        "infra/staging.yaml. Rotate credentials before, not after.\n",
        "\n",
        "commit = a94a8fe5ccb19ba61c4c0873d391e987982fbbd3\n",
        "session_id = \"550e8400-e29b-41d4-a716-446655440000\"\n",
        "logo = \"data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAA",
        "AfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==\"\n",
        "let x = 1; let y = 2; let z = compute(x, y); assert_eq!(z, 3);\n",
        "aws_access_key_id = AKIA3G7QVHBRN2WPKZ5F\n",
        "DATABASE_URL=postgres://svcuser:Rk7mZq3XvB9n@db.internal:5432/orders\n",
        "\n",
    );

    let mut document = String::with_capacity(TARGET_BYTES + block.len());
    while document.len() < TARGET_BYTES {
        document.push_str(block);
    }
    document
}

#[test]
fn ten_megabytes_are_scanned_well_inside_the_budget() {
    let document = large_document();
    assert!(document.len() >= TARGET_BYTES);

    let started = Instant::now();
    let findings = scan(&document);
    let elapsed = started.elapsed();

    assert!(
        !findings.is_empty(),
        "a scan that finds nothing is a fast scan for the wrong reason"
    );
    assert!(
        elapsed < BUDGET,
        "scanning {} bytes took {elapsed:?}, over the {BUDGET:?} budget",
        document.len()
    );
}

#[test]
fn a_single_enormous_line_costs_no_more_than_the_same_bytes_split_over_many() {
    // A minified bundle, a one-line JSON log or a base64 blob arrives as one
    // line, and puts every finding in the document into a single line's
    // working set. Anything that compares findings against each other, or
    // walks backwards without a bound, turns quadratic exactly here.
    let split = large_document();
    let joined = split.replace('\n', " ");

    let started = Instant::now();
    let over_many = scan(&split).len();
    let many_lines = started.elapsed();

    let started = Instant::now();
    let on_one = scan(&joined).len();
    let one_line = started.elapsed();

    assert_eq!(on_one, over_many, "the same bytes hold the same secrets");
    assert!(one_line < BUDGET, "one line took {one_line:?}");

    // The two are within noise of each other when the scan is linear. The
    // quadratic version this test was written against was six times slower.
    assert!(
        one_line < many_lines * 4 + Duration::from_millis(50),
        "one line took {one_line:?} against {many_lines:?} for the same bytes"
    );
}

#[test]
fn pathological_punctuation_does_not_make_the_scan_quadratic() {
    // Each of these is a shape that tempts a scanner into rescanning: a
    // separator with nothing after it, an unterminated quote, a run of
    // dashes that never becomes a PEM marker.
    for pattern in ["a=", "k=\"", "-----", "eyJ.", "://", "x=y "] {
        let document = pattern.repeat(TARGET_BYTES / pattern.len() / 4);

        let started = Instant::now();
        let _ = scan(&document);
        let elapsed = started.elapsed();

        assert!(
            elapsed < BUDGET,
            "{pattern:?} repeated to {} bytes took {elapsed:?}",
            document.len()
        );
    }
}
