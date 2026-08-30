//! Hostile-input harnesses for the parsers BachelorPad+ points at untrusted
//! files.
//!
//! # This is not fuzzing
//!
//! Say it plainly, because a security control described as more than it is
//! is worse than one that is absent: **there is no coverage-guided fuzzer
//! here.** `cargo-fuzz` needs a nightly toolchain and libFuzzer, and neither
//! is installed on the machine this repository is gated on. A gate stage
//! nobody can run is worse than no stage (ADR-0016 makes the local gate the
//! only gate), so this is what everyone can actually run: a checked-in
//! corpus of the inputs already known to be pathological, plus `proptest`
//! generating structured and unstructured input around them.
//!
//! What that keeps: the invariant, the corpus, the regression value of both,
//! and a stage in `cargo test` that fails on the day one of these parsers
//! starts panicking.
//!
//! What it does not give you, and the fuzz targets would: no coverage
//! feedback, so nothing walks towards an unexplored branch; no corpus
//! minimisation; no automatic crash minimisation; no sanitizers (ASan,
//! MSan); no persistent corpus growing across runs. Random and property
//! input finds shallow bugs. A guided fuzzer finds the ones behind three
//! conditionals. Do not read a green run here as "these parsers have been
//! fuzzed".
//!
//! # The invariant
//!
//! Every target asserts exactly one thing, and it is the same one:
//!
//! > **The function may return an error. It may not panic, abort, or hang.**
//!
//! An error is a designed outcome that reaches the user as a sentence. A
//! panic in a parser reachable from "open this file" is a crash on a file
//! somebody was sent, and an abort or a hang is the same crash with a worse
//! diagnosis.
//!
//! # How each of those three is actually detected
//!
//! * **Panic** -- [`catch_unwind`](std::panic::catch_unwind) around every
//!   call, so one bad input is a named failure with its input reported rather
//!   than an anonymous test abort, and the rest of the corpus still runs.
//! * **Abort** -- cannot be caught by anything, by definition. A stack
//!   overflow kills the process and the test binary dies. That is the
//!   detection: the run reports a crashed test binary, which is the honest
//!   signal, because that is exactly what the application would do.
//! * **Hang** -- [`Probe::budget`] runs corpus entries on their own thread
//!   and gives up waiting after a budget, so a wedged parser fails the run
//!   instead of holding the gate open forever. The worker thread is leaked
//!   when that happens; there is no way to kill a thread in std, and a
//!   leaked thread in a test binary that is about to fail is the cheaper
//!   half of the trade.
//!
//! # Stack size is part of the harness, not an accident
//!
//! ADR-0023 measured the YAML nesting limit against **the 1 MiB main
//! thread**, which is the stack the application actually parses on. libtest
//! runs each test on a spawned thread with a considerably larger one, so a
//! depth that would kill the shipped binary can pass a test run on the
//! default stack and prove nothing. [`Probe::stack_size`] pins the worker to
//! [`MAIN_THREAD_STACK`] so the recursion probes are asked the question the
//! application will be asked.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

pub mod strategies;

/// The stack the application parses on.
///
/// Windows gives the main thread 1 MiB by default and ADR-0023's depth
/// measurements were taken there. Probing on anything larger measures a
/// thread the user never runs.
pub const MAIN_THREAD_STACK: usize = 1024 * 1024;

/// How long one input may take before it is treated as a hang.
///
/// Generous on purpose. Argon2id at a document's own declared cost is
/// legitimately slow, and a slow machine under a cold cache is not a defect.
/// Anything past this is not "slow", it is stuck.
pub const DEFAULT_BUDGET: Duration = Duration::from_secs(20);

/// How an input broke the invariant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Violation {
    /// The call unwound. `message` is the panic payload if it was a string.
    Panic { message: String },
    /// The call did not return inside its budget.
    Hang { budget: Duration },
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Panic { message } => write!(f, "PANICKED: {message}"),
            Self::Hang { budget } => write!(f, "HUNG: no return within {budget:?}"),
        }
    }
}

/// One input's verdict, kept so a whole corpus can be reported at once.
#[derive(Debug)]
pub struct Outcome {
    pub name: String,
    pub elapsed: Duration,
    pub violation: Option<Violation>,
}

/// How to run one input.
///
/// A builder rather than four functions because the two knobs -- stack size
/// and time budget -- are the interesting part of each target and belong at
/// the call site where a reader can see which question is being asked.
#[derive(Debug, Clone, Copy)]
pub struct Probe {
    stack_size: usize,
    budget: Duration,
}

impl Default for Probe {
    fn default() -> Self {
        Self {
            stack_size: MAIN_THREAD_STACK,
            budget: DEFAULT_BUDGET,
        }
    }
}

impl Probe {
    /// A probe on the application's stack, with the default budget.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Run on a stack of this size instead of [`MAIN_THREAD_STACK`].
    #[must_use]
    pub fn stack_size(mut self, bytes: usize) -> Self {
        self.stack_size = bytes;
        self
    }

    /// Give up on an input after this long.
    #[must_use]
    pub fn budget(mut self, budget: Duration) -> Self {
        self.budget = budget;
        self
    }

    /// Run `body` on its own thread, catching a panic and timing out a hang.
    ///
    /// The return value of `body` is discarded on purpose. Every target here
    /// asks whether a call *survives* an input, never what it computed --
    /// checking the answer as well would be a correctness test, which the
    /// crates already have.
    pub fn run<F>(self, name: impl Into<String>, body: F) -> Outcome
    where
        F: FnOnce() + Send + 'static,
    {
        let name = name.into();
        let (tx, rx) = mpsc::channel();
        let started = Instant::now();

        let spawned = thread::Builder::new()
            .name(format!("probe:{name}"))
            .stack_size(self.stack_size)
            .spawn(move || {
                let result = catch_unwind(AssertUnwindSafe(body));
                // A closed receiver means the probe already gave up and
                // moved on. Nothing to report to, and nothing wrong.
                let _ = tx.send(result.map_err(|payload| panic_message(&*payload)));
            });

        let handle = match spawned {
            Ok(handle) => handle,
            Err(e) => {
                return Outcome {
                    name,
                    elapsed: started.elapsed(),
                    violation: Some(Violation::Panic {
                        message: format!("could not spawn a probe thread: {e}"),
                    }),
                };
            }
        };

        match rx.recv_timeout(self.budget) {
            Ok(Ok(())) => {
                // Only join once the body has reported; joining first would
                // block forever on a hang, which is the case being detected.
                let _ = handle.join();
                Outcome {
                    name,
                    elapsed: started.elapsed(),
                    violation: None,
                }
            }
            Ok(Err(message)) => {
                let _ = handle.join();
                Outcome {
                    name,
                    elapsed: started.elapsed(),
                    violation: Some(Violation::Panic { message }),
                }
            }
            Err(_) => Outcome {
                name,
                elapsed: started.elapsed(),
                // The thread is left running. std cannot kill one, and the
                // run is failing anyway.
                violation: Some(Violation::Hang {
                    budget: self.budget,
                }),
            },
        }
    }
}

/// Run one input on the current thread, catching a panic but not a hang.
///
/// For sweeps of thousands of inputs, where [`Probe::run`]'s thread per input
/// costs more than the check is worth. It gives up hang detection to get
/// there -- the caller is expected to be inside an [`on_app_stack`] block,
/// which puts one budget around the whole sweep instead of one around each
/// input.
pub fn inline<F>(name: impl Into<String>, body: F) -> Outcome
where
    F: FnOnce(),
{
    let started = Instant::now();
    let violation = catch_unwind(AssertUnwindSafe(body))
        .err()
        .map(|payload| Violation::Panic {
            message: panic_message(&*payload),
        });

    Outcome {
        name: name.into(),
        elapsed: started.elapsed(),
        violation,
    }
}

/// Turn a panic payload into something printable.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "<non-string panic payload>".to_string()
    }
}

/// Fail the test if any outcome violated the invariant, naming every one.
///
/// All of them, not the first: a run that reports one input and stops turns
/// a survey into a sequence of single-bug sessions, which is the same reason
/// `Invoke-LocalCI.ps1` runs every stage after one has failed.
///
/// # Panics
///
/// Deliberately, when an input panicked or hung -- that is how a `#[test]`
/// reports a failure.
pub fn assert_no_violations(target: &str, outcomes: &[Outcome]) {
    let broken: Vec<&Outcome> = outcomes.iter().filter(|o| o.violation.is_some()).collect();

    if broken.is_empty() {
        return;
    }

    let mut report = format!(
        "{}: {} of {} inputs broke the invariant \
         (may error; may not panic, abort or hang)\n",
        target,
        broken.len(),
        outcomes.len()
    );
    for outcome in broken {
        let violation = outcome
            .violation
            .as_ref()
            .expect("filtered to the violations");
        report.push_str(&format!(
            "  {} after {:?}: {violation}\n",
            outcome.name, outcome.elapsed
        ));
    }
    panic!("{report}");
}

/// How many cases each `proptest` block runs.
///
/// The default is chosen for the gate: `Invoke-LocalCI.ps1` runs on every
/// commit, and a stage that takes ten minutes is a stage people learn to
/// bypass. `BP_FUZZ_CASES` raises it for a deliberate soak -- an afternoon at
/// 200,000 is the closest this harness gets to what a real fuzzer does
/// overnight, and it is the right thing to run before a release rather than
/// before a commit.
///
/// # Panics
///
/// If `BP_FUZZ_CASES` is set to something that is not a number. Silently
/// ignoring it would mean a soak that quietly ran 2,000 cases and reported
/// nothing wrong.
#[must_use]
pub fn cases() -> u32 {
    match std::env::var("BP_FUZZ_CASES") {
        Ok(raw) => raw
            .parse()
            .unwrap_or_else(|e| panic!("BP_FUZZ_CASES={raw} is not a number: {e}")),
        Err(_) => 2_000,
    }
}

/// The `proptest` configuration every block here uses.
///
/// `failure_persistence` is off. Proptest's default writes a regression file
/// beside the test source on a failure, and a corpus that appears as an
/// untracked artefact of whoever ran the suite last is not a corpus. When a
/// case fails here, the response is to add it to `corpus/` by name, through
/// `seed-corpus.rs`, where the next reader can see what it is for.
#[must_use]
pub fn proptest_config() -> proptest::test_runner::Config {
    proptest::test_runner::Config {
        cases: cases(),
        failure_persistence: None,
        ..proptest::test_runner::Config::default()
    }
}

/// Run a whole block of work on the application's stack and fail the test if
/// it panics or hangs.
///
/// This is how the `proptest` blocks are run. Proptest catches and *shrinks*
/// a panicking case itself, which produces a far better report than anything
/// here could -- a minimal input rather than whichever one happened to be
/// generated. So the block is left to do that, and this wrapper's job is only
/// to make sure it does it on a 1 MiB stack, the size the application parses
/// on, rather than the roomier one libtest hands a test thread.
///
/// # Panics
///
/// When the block panics or overruns `budget`.
pub fn on_app_stack<F>(name: &str, budget: Duration, body: F)
where
    F: FnOnce() + Send + 'static,
{
    let outcome = Probe::new().budget(budget).run(name, body);
    assert_no_violations(name, &[outcome]);
}

/// Root of the checked-in corpus.
#[must_use]
pub fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus")
}

/// One corpus entry: its name, for reporting, and its bytes.
#[derive(Debug, Clone)]
pub struct Sample {
    pub name: String,
    pub bytes: Vec<u8>,
}

impl Sample {
    /// The entry as text, if it is UTF-8.
    ///
    /// Some entries are deliberately not -- an invalid-UTF-8 file is one of
    /// the inputs being tested -- so this returns [`None`] rather than
    /// substituting, which is the same rule `bp-files` follows.
    #[must_use]
    pub fn text(&self) -> Option<&str> {
        std::str::from_utf8(&self.bytes).ok()
    }
}

/// Read every file in one corpus directory, sorted so a run is reproducible.
///
/// # Panics
///
/// If the directory is missing or unreadable. The corpus is checked in; its
/// absence is a broken checkout, not a condition to tolerate quietly.
#[must_use]
pub fn corpus(target: &str) -> Vec<Sample> {
    let dir = corpus_root().join(target);
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("corpus directory {} is unreadable: {e}", dir.display()))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    entries.sort();

    assert!(
        !entries.is_empty(),
        "corpus directory {} is empty",
        dir.display()
    );

    entries
        .into_iter()
        .map(|path| Sample {
            name: format!(
                "{target}/{}",
                path.file_name().unwrap_or_default().to_string_lossy()
            ),
            bytes: std::fs::read(&path)
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display())),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    //! The harness testing itself.
    //!
    //! A detector nobody has ever seen fire is indistinguishable from one
    //! that cannot. Every target in `tests/` reports "no violations", and
    //! that sentence is only worth something if these three pass -- they are
    //! what makes a green run evidence rather than a tautology.

    use super::*;

    #[test]
    fn a_clean_run_reports_no_violation() {
        let outcome = Probe::new().run("fine", || {});
        assert!(outcome.violation.is_none());
    }

    #[test]
    fn a_panic_is_caught_and_its_message_kept() {
        let outcome = Probe::new().run("panics", || panic!("the message"));
        assert_eq!(
            outcome.violation,
            Some(Violation::Panic {
                message: "the message".to_string()
            })
        );
    }

    #[test]
    fn a_hang_is_detected_rather_than_waited_out() {
        let budget = Duration::from_millis(150);
        // Not an infinite loop: this thread is leaked when the probe gives
        // up, and a test suite that leaks a spinning core is a worse
        // neighbour than one that leaks a sleeping thread.
        let outcome = Probe::new()
            .budget(budget)
            .run("hangs", || thread::sleep(Duration::from_secs(30)));

        assert_eq!(outcome.violation, Some(Violation::Hang { budget }));
        assert!(
            outcome.elapsed < Duration::from_secs(5),
            "the probe waited {:?}, which is not giving up",
            outcome.elapsed
        );
    }

    #[test]
    fn every_violation_is_named_not_just_the_first() {
        let outcomes = vec![
            Probe::new().run("ok", || {}),
            Probe::new().run("first", || panic!("one")),
            Probe::new().run("second", || panic!("two")),
        ];

        let report = catch_unwind(AssertUnwindSafe(|| {
            assert_no_violations("self-test", &outcomes);
        }))
        .expect_err("should have failed");
        let report = panic_message(&*report);

        assert!(report.contains("first"), "got {report}");
        assert!(report.contains("second"), "got {report}");
        assert!(report.contains("2 of 3"), "got {report}");
    }

    #[test]
    fn the_corpus_is_where_it_is_expected_to_be() {
        for target in ["yaml", "data", "files", "envelope"] {
            assert!(!corpus(target).is_empty(), "corpus/{target} has no entries");
        }
    }
}
