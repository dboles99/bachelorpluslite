//! Running one language's interpreter as a fresh subprocess.
//!
//! ADR-0040's execution model in one function: spawn, feed `source` on
//! `stdin`, capture `stdout`/`stderr` separately, enforce a wall-clock
//! timeout and an output cap, tear the process down, and hand back what
//! happened. No session, no state carried between calls, no sandboxing
//! beyond [`crate::UserGesture`] -- see the crate root docs for what that
//! last point does and does not mean.
//!
//! ## Concurrency, and why
//!
//! `source` goes to the child on `stdin`; `stdout` and `stderr` come back on
//! two more pipes. All three are OS pipes with a bounded buffer (a handful
//! of KiB), and this crate does not control how an interpreter schedules its
//! own reads and writes against them. A cell that prints a lot before (or
//! without) fully reading `stdin` can fill the `stdout` pipe while this
//! process is still blocked writing `stdin`'s -- and if nothing is reading
//! `stdout` yet, that is a deadlock: both sides waiting on a pipe the other
//! side would have to drain first before either can make progress.
//!
//! So writing `stdin` and reading each of `stdout`/`stderr` happen on three
//! separate threads, running concurrently with each other and with this
//! function's own wait loop. Nothing here assumes an interpreter reads all
//! of `stdin` before writing any output, which `Command::output()` (write
//! everything first, *then* read) would have to assume to be safe.
//!
//! ## Blocking and resumable are the same run, driven two ways
//!
//! [`run`] and [`run_with_timeout`] block until the cell is done, which is
//! what a test or a batch caller wants. A window cannot afford that: a cell
//! that sleeps for the whole timeout would freeze it for the whole timeout,
//! and the point of a timeout is to bound a hang, not to schedule one.
//!
//! So the spawn and the waiting are separated. [`Run::start`] does everything
//! up to and including the three threads, and [`Run::poll`] asks once,
//! without blocking, whether the process has exited or the deadline has
//! passed. `run_with_timeout` is then written in terms of those two, so there
//! is exactly one implementation of what a run *is* and no chance of the two
//! shapes drifting on the timeout, the cap or the teardown.
//!
//! ## The output cap is enforced while reading, not after
//!
//! [`read_capped`] stops pulling bytes off a pipe the moment it has
//! [`OUTPUT_CAP_BYTES`], rather than reading to completion and truncating a
//! buffer afterward. A `print` loop that never terminates must never get the
//! chance to be read to completion in the first place -- that is the whole
//! point of a cap.

use std::io::{Read, Write};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::error::ExecutionError;
use crate::gesture::UserGesture;
use crate::language::{Language, resolve};
use crate::outcome::RunOutcome;

/// How long a run is allowed before it is killed, unless a caller asks for a
/// different bound via [`run_with_timeout`].
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// How many bytes of `stdout` (and, independently, of `stderr`) are kept.
/// Past this, that stream is truncated and [`RunOutcome`] says so.
pub const OUTPUT_CAP_BYTES: usize = 1024 * 1024;

/// How often the wait loop checks whether the process has exited or the
/// timeout has elapsed. Small enough that even a short timeout (as tests use)
/// is respected promptly; large enough not to spin the CPU.
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Run `source` as `language`, with [`DEFAULT_TIMEOUT`].
///
/// Requires a [`UserGesture`] by value: this is one of exactly two functions
/// in this crate that can start a process (the other is
/// [`run_with_timeout`]), and both demand one for the same reason
/// `bp-notebook`'s `request_run*` family does -- see the crate root docs.
///
/// Returns `Err` only when the run could not be attempted at all -- no
/// interpreter found, or an OS-level failure starting or managing the
/// process. A cell whose own code fails is a *successful* run: it comes back
/// as `Ok`, with the failure visible in [`RunOutcome::exit_code`] and
/// [`RunOutcome::stderr`].
pub fn run(
    language: Language,
    source: &str,
    gesture: UserGesture,
) -> Result<RunOutcome, ExecutionError> {
    run_with_timeout(language, source, DEFAULT_TIMEOUT, gesture)
}

/// Run `source` as `language`, killing the process if it is still running
/// after `timeout`.
///
/// The `gesture` requirement and the `Ok`-means-ran-successfully contract are
/// identical to [`run`]'s, which simply calls this with [`DEFAULT_TIMEOUT`].
/// Exposed directly so a caller -- or a test -- that needs a different bound
/// does not have to invent a second entry point.
pub fn run_with_timeout(
    language: Language,
    source: &str,
    timeout: Duration,
    gesture: UserGesture,
) -> Result<RunOutcome, ExecutionError> {
    block_on(Run::start_impl(
        language,
        language.candidates(),
        source,
        timeout,
        gesture,
    )?)
}

/// Drive a [`Run`] to its end on this thread, sleeping between checks.
///
/// The blocking half of the crate, and the only place that sleeps. Written
/// over [`Run::poll`] rather than beside it so that the two shapes cannot
/// come to disagree about when a run is over.
fn block_on(mut run: Run) -> Result<RunOutcome, ExecutionError> {
    loop {
        match run.poll() {
            Progress::Finished(outcome) => return Ok(outcome),
            Progress::Running(waiting) => {
                run = waiting;
                thread::sleep(POLL_INTERVAL);
            }
        }
    }
}

/// A run in flight, asked about rather than waited on.
///
/// **The type is what stops a finished run being polled again**, which is the
/// whole reason [`Self::poll`] takes `self` and hands the run back only while
/// it is still going: the reader threads are joined exactly once, when the
/// outcome is built, and there is no state in which this holds a
/// `JoinHandle` that has already been joined.
///
/// Dropping one leaves the child running. [`Self::stop`] is how a caller ends
/// a cell it no longer wants, and it returns whatever the process managed to
/// produce first rather than discarding it.
#[derive(Debug)]
pub struct Run {
    child: Child,
    start: Instant,
    timeout: Duration,
    stdin_writer: JoinHandle<()>,
    stdout_reader: JoinHandle<(Vec<u8>, bool)>,
    stderr_reader: JoinHandle<(Vec<u8>, bool)>,
}

/// What one [`Run::poll`] found.
#[derive(Debug)]
pub enum Progress {
    /// Still going. The run is handed back so the caller can ask again.
    Running(Run),
    /// Over -- exited on its own, or killed at the deadline.
    Finished(RunOutcome),
}

impl Run {
    /// Spawn the interpreter and start feeding it, with [`DEFAULT_TIMEOUT`].
    ///
    /// Requires a [`UserGesture`] by value for the same reason [`run`] does:
    /// this starts a process, and the gesture is the one greppable line where
    /// a person is asserted to have asked for it.
    pub fn start(
        language: Language,
        source: &str,
        gesture: UserGesture,
    ) -> Result<Self, ExecutionError> {
        Self::start_with_timeout(language, source, DEFAULT_TIMEOUT, gesture)
    }

    /// Spawn with a chosen deadline.
    pub fn start_with_timeout(
        language: Language,
        source: &str,
        timeout: Duration,
        gesture: UserGesture,
    ) -> Result<Self, ExecutionError> {
        Self::start_impl(language, language.candidates(), source, timeout, gesture)
    }

    /// How long this run has been going. For a caller that shows progress.
    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }

    /// Ask once whether the run is over. Never blocks.
    ///
    /// A run is over when the process exits *or* when the deadline passes, and
    /// the second case kills and reaps before collecting -- leaving a killed
    /// child unwaited is a zombie on Unix.
    pub fn poll(mut self) -> Progress {
        let status = match self.child.try_wait() {
            Ok(Some(status)) => Some(status),
            Ok(None) => {
                if self.start.elapsed() < self.timeout {
                    return Progress::Running(self);
                }
                let _ = self.child.kill();
                let status = self.child_wait();
                return Progress::Finished(self.collect(status, true));
            }
            // Cannot observe whether the process has exited. Rare and
            // OS-level, but the run was genuinely attempted and may already
            // have produced output worth keeping -- report it as an outcome
            // with an unknown exit code rather than discarding everything.
            Err(_) => None,
        };
        Progress::Finished(self.collect(status, false))
    }

    /// End the run now, and keep whatever it produced.
    ///
    /// What the Stop the caller offers actually does. A cell that printed for
    /// ten seconds and was then stopped has ten seconds of output worth
    /// showing, and throwing it away would make Stop feel like a failure
    /// rather than a decision.
    pub fn stop(mut self) -> RunOutcome {
        let _ = self.child.kill();
        let status = self.child_wait();
        // `timed_out` stays false: this ended because somebody said so, and
        // reporting it as a timeout would blame the cell for the user's
        // decision.
        self.collect(status, false)
    }

    /// Reap the child after a kill, so it does not become a zombie.
    fn child_wait(&mut self) -> Option<ExitStatus> {
        self.child.wait().ok()
    }

    /// Join the readers and build the outcome. Consumes, because a
    /// `JoinHandle` can only be joined once.
    ///
    /// No extra wait is needed first: by the time the process has exited or
    /// been killed-and-reaped, the OS has closed its end of every pipe, so
    /// each reader thread is at (or moments from) EOF regardless of how much
    /// of its cap it had used.
    fn collect(self, status: Option<ExitStatus>, timed_out: bool) -> RunOutcome {
        let duration = self.start.elapsed();
        let _ = self.stdin_writer.join();
        let (stdout_bytes, stdout_truncated) = self.stdout_reader.join().unwrap_or_default();
        let (stderr_bytes, stderr_truncated) = self.stderr_reader.join().unwrap_or_default();

        RunOutcome {
            stdout: String::from_utf8_lossy(&stdout_bytes).into_owned(),
            stderr: String::from_utf8_lossy(&stderr_bytes).into_owned(),
            exit_code: status.and_then(|s| s.code()),
            stdout_truncated,
            stderr_truncated,
            timed_out,
            duration,
        }
    }
}

impl Run {
    /// The real spawn, taking the candidate interpreter list as an explicit
    /// parameter rather than deriving it from `language` internally.
    ///
    /// This split exists for one reason: it lets a test exercise "no candidate
    /// resolves" (see [`ExecutionError::InterpreterNotFound`]) through the real
    /// spawn path end to end, by handing it a candidate list guaranteed to
    /// resolve nothing -- without inventing a fourth, fictitious [`Language`]
    /// variant, and without mutating the real `PATH` for the whole test process
    /// (which a parallel test run would race on).
    fn start_impl(
        language: Language,
        candidates: &[&'static str],
        source: &str,
        timeout: Duration,
        gesture: UserGesture,
    ) -> Result<Self, ExecutionError> {
        let _ = gesture; // Consumed by value: proves a caller held one to give up.

        let interpreter =
            resolve(candidates).ok_or_else(|| ExecutionError::InterpreterNotFound {
                language,
                tried: candidates.to_vec(),
            })?;

        let mut child = Command::new(interpreter)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(ExecutionError::Process)?;

        // Infallible: all three were just requested as `Stdio::piped()` above.
        let mut stdin = child.stdin.take().expect("stdin was requested as piped");
        let stdout = child.stdout.take().expect("stdout was requested as piped");
        let stderr = child.stderr.take().expect("stderr was requested as piped");

        let source = source.to_owned();
        let stdin_writer = thread::spawn(move || {
            // A script that never reads stdin, or exits before we finish
            // writing, closes its end early -- `write_all` then fails with a
            // broken-pipe error. That is not this crate's failure to report: the
            // process still ran, and its exit code and stderr say what
            // happened. `stdin` is dropped when this closure returns, closing
            // our end, which is how the interpreter is told "that's all of it."
            let _ = stdin.write_all(source.as_bytes());
        });
        let stdout_reader = thread::spawn(move || read_capped(stdout));
        let stderr_reader = thread::spawn(move || read_capped(stderr));

        Ok(Self {
            child,
            start: Instant::now(),
            timeout,
            stdin_writer,
            stdout_reader,
            stderr_reader,
        })
    }
}

/// Read `reader` to EOF or [`OUTPUT_CAP_BYTES`], whichever comes first.
/// Returns the bytes captured and whether the cap cut it short.
fn read_capped(mut reader: impl Read) -> (Vec<u8>, bool) {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        if buf.len() >= OUTPUT_CAP_BYTES {
            return (buf, true);
        }
        match reader.read(&mut chunk) {
            Ok(0) => return (buf, false), // EOF: the stream ended on its own.
            Ok(n) => {
                let room = OUTPUT_CAP_BYTES - buf.len();
                let take = n.min(room);
                buf.extend_from_slice(&chunk[..take]);
                if take < n {
                    return (buf, true);
                }
            }
            // A read error here (e.g. the pipe went away because the
            // process was just killed) is not the cap's doing and not this
            // crate's failure to report -- treat it as a clean end of
            // stream, same as EOF.
            Err(_) => return (buf, false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gesture() -> UserGesture {
        UserGesture::from_user_command()
    }

    /// A candidate list guaranteed to resolve nothing, for exercising
    /// [`ExecutionError::InterpreterNotFound`] without depending on any real
    /// interpreter being present or absent on the test machine.
    const NO_SUCH_INTERPRETER: &[&str] = &["bp-execution-definitely-not-a-real-binary-9f8a7b3c"];

    /// `python3`, then `python` -- whichever actually runs, or `None` if
    /// neither does. Every Python-dependent test calls this once and skips
    /// (prints and returns) rather than fails when it is `None`: this crate's
    /// job is to run Python correctly *when Python is there to run*, and a
    /// CI machine or a contributor's laptop without Python installed is not
    /// a bp-execution defect. The only precedent for shelling out found
    /// elsewhere in this workspace is `bp-buffer`'s benchmark harness, which
    /// tolerates a missing PowerShell by reporting `NaN` rather than by
    /// skipping outright -- not directly applicable to a pass/fail test, so
    /// this is a fresh judgment call, documented here rather than left
    /// implicit.
    fn python() -> Option<Language> {
        resolve(Language::Python.candidates()).map(|_| Language::Python)
    }

    #[test]
    fn user_gesture_has_exactly_one_constructor_and_no_way_around_it() {
        // The type exists to make `from_user_command` the one, greppable,
        // deliberately-named call site. The compile_fail doctests on
        // `UserGesture` prove it cannot be cloned or defaulted; this test
        // just proves the constructor itself works and produces a usable
        // value.
        let _ = UserGesture::from_user_command();
    }

    #[test]
    fn a_trivial_python_script_runs_and_its_stdout_is_captured() {
        let Some(language) = python() else {
            eprintln!("skipping: no python3/python found on PATH");
            return;
        };
        let outcome = run(language, "print(\"hello\")", gesture()).unwrap();
        // `trim_end`, not an exact match: Python's stdout is line-buffered
        // text mode, and on Windows that means a trailing "\r\n" rather than
        // "\n" -- a platform detail of how the interpreter itself writes
        // text, not something bp-execution transforms.
        assert_eq!(outcome.stdout().trim_end(), "hello");
        assert_eq!(outcome.stderr(), "");
        assert_eq!(outcome.exit_code(), Some(0));
        assert!(outcome.success());
        assert!(!outcome.timed_out());
        assert!(!outcome.stdout_truncated());
        assert!(!outcome.stderr_truncated());
    }

    #[test]
    fn a_script_that_raises_is_a_successful_run_with_the_failure_in_its_exit_code_and_stderr() {
        // The most important test in this module: getting this backwards --
        // treating a Python exception as an `ExecutionError` -- would be a
        // real design defect, not a missing case. `bp-execution` could not
        // even attempt the run is the only thing `ExecutionError` means; a
        // cell whose code blew up is not that.
        let Some(language) = python() else {
            eprintln!("skipping: no python3/python found on PATH");
            return;
        };
        let outcome = run(
            language,
            "import sys\nprint('before', file=sys.stderr)\nraise ValueError('boom')",
            gesture(),
        )
        .unwrap();
        assert_ne!(outcome.exit_code(), Some(0));
        assert!(!outcome.success());
        assert!(
            outcome.stderr().contains("ValueError"),
            "stderr was: {}",
            outcome.stderr()
        );
        assert!(outcome.stderr().contains("before"));
        assert_eq!(
            outcome.stdout(),
            "",
            "the exception went to stderr, not stdout"
        );
        assert!(!outcome.timed_out(), "a raised exception is not a timeout");
    }

    #[test]
    fn a_script_that_loops_forever_is_killed_once_the_timeout_elapses() {
        let Some(language) = python() else {
            eprintln!("skipping: no python3/python found on PATH");
            return;
        };
        let short = Duration::from_millis(200);
        let started = Instant::now();
        let outcome =
            run_with_timeout(language, "while True:\n    pass", short, gesture()).unwrap();
        assert!(outcome.timed_out());
        // Generous upper bound: the poll interval and process teardown add
        // some slack, but this must not run anywhere near the crate's
        // 30-second default.
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "took {:?}, the timeout should have cut this off promptly",
            started.elapsed()
        );
    }

    #[test]
    fn output_far_past_the_cap_is_truncated_and_the_outcome_says_so() {
        let Some(language) = python() else {
            eprintln!("skipping: no python3/python found on PATH");
            return;
        };
        // Each line is 101 bytes (100 'x' plus '\n'); comfortably more than
        // OUTPUT_CAP_BYTES lines guarantees crossing the cap even accounting
        // for interpreter startup overhead in stdout.
        let script = "import sys\nfor _ in range(50000):\n    sys.stdout.write('x' * 100 + '\\n')";
        let outcome =
            run_with_timeout(language, script, Duration::from_secs(20), gesture()).unwrap();
        assert!(outcome.stdout_truncated());
        assert!(outcome.stdout().len() <= OUTPUT_CAP_BYTES);
    }

    /// Poll a run to its end, counting how many times it said it was still
    /// going. The count is the point: a resumable run that never once
    /// answered "still running" would be a blocking one wearing a different
    /// signature.
    fn drive(run: Run) -> (RunOutcome, usize) {
        let mut run = run;
        let mut still_running = 0;
        loop {
            match run.poll() {
                Progress::Finished(outcome) => return (outcome, still_running),
                Progress::Running(waiting) => {
                    run = waiting;
                    still_running += 1;
                    thread::sleep(POLL_INTERVAL);
                }
            }
        }
    }

    #[test]
    fn a_resumable_run_reaches_the_same_outcome_the_blocking_one_does() {
        // The property that makes one implementation safe to have two shapes:
        // `run_with_timeout` is written over `poll`, so a difference here
        // would mean the blocking path had grown its own idea of what a run
        // is.
        let Some(language) = python() else {
            println!("no python interpreter; skipping");
            return;
        };

        let blocking = run(language, "print('same')", gesture()).expect("the blocking run starts");
        let started = Run::start(language, "print('same')", gesture()).expect("the run starts");
        let (resumable, _) = drive(started);

        assert_eq!(resumable.stdout(), blocking.stdout());
        assert_eq!(resumable.exit_code(), blocking.exit_code());
        assert!(!resumable.timed_out());
    }

    #[test]
    fn a_run_that_takes_a_while_says_it_is_still_going_rather_than_blocking() {
        // The whole reason this API exists. A window polls this between
        // frames, so `poll` must return while the cell is still working --
        // if it only ever came back at the end, the window would freeze for
        // as long as the cell took.
        let Some(language) = python() else {
            println!("no python interpreter; skipping");
            return;
        };

        let started = Run::start(
            language,
            "import time\ntime.sleep(0.4)\nprint('done')",
            gesture(),
        )
        .expect("the run starts");
        let (outcome, still_running) = drive(started);

        assert!(
            still_running > 0,
            "poll never once reported the run in flight, so it blocked"
        );
        assert_eq!(outcome.stdout().trim(), "done");
        assert!(!outcome.timed_out());
    }

    #[test]
    fn stopping_a_run_keeps_what_it_had_already_printed() {
        // What Stop has to mean for it to be a decision rather than a
        // failure: a cell that printed for a while and was then stopped has
        // output worth showing, and discarding it would punish the user for
        // changing their mind.
        let Some(language) = python() else {
            println!("no python interpreter; skipping");
            return;
        };

        // **The cell says when it has printed, and the test waits for that
        // rather than for a number of milliseconds.** The first version of
        // this polled 400 times at 10ms and carried a comment claiming it did
        // not depend on interpreter startup being fast; four seconds is
        // exactly such a dependency, and it failed on the Linux leg of a
        // loaded machine, where the failure read as "stop lost the output"
        // rather than "Python had not started yet".
        //
        // A marker file is the observable signal. `stdout` cannot be one:
        // this crate only hands it back when the run is over, which is the
        // very thing being tested.
        let marker = std::env::temp_dir().join(format!(
            "bp-execution-stop-{}-{:?}.marker",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_file(&marker);
        let script = format!(
            "import sys, time\nsys.stdout.write('before stop\\n')\nsys.stdout.flush()\nopen(r'{}', 'w').close()\ntime.sleep(30)",
            marker.display()
        );

        let mut run = Run::start(language, &script, gesture()).expect("the run starts");
        let deadline = Instant::now() + Duration::from_secs(60);
        while !marker.exists() {
            assert!(
                Instant::now() < deadline,
                "the interpreter never signalled that it had printed, so this \
                 test never got as far as what it is about"
            );
            match run.poll() {
                Progress::Running(waiting) => run = waiting,
                Progress::Finished(_) => panic!("a 30-second sleep should not have finished"),
            }
            thread::sleep(POLL_INTERVAL);
        }

        let outcome = run.stop();
        let _ = std::fs::remove_file(&marker);

        assert!(
            outcome.stdout().contains("before stop"),
            "stopped run lost its output: {:?}",
            outcome.stdout()
        );
        assert!(
            !outcome.timed_out(),
            "a run the user stopped must not be reported as a timeout -- that blames the cell for their decision"
        );
    }

    #[test]
    fn a_resumable_run_still_honours_its_deadline() {
        // The timeout is the reader's protection against a hung cell, and it
        // has to survive the run being driven from outside rather than from
        // the loop that used to own it.
        let Some(language) = python() else {
            println!("no python interpreter; skipping");
            return;
        };

        let started = Run::start_with_timeout(
            language,
            "while True:\n    pass",
            Duration::from_millis(300),
            gesture(),
        )
        .expect("the run starts");
        let (outcome, _) = drive(started);

        assert!(outcome.timed_out(), "the deadline did not end the run");
    }

    #[test]
    fn an_unresolvable_interpreter_is_reported_as_not_found_not_a_panic() {
        let err = Run::start_impl(
            Language::Python,
            NO_SUCH_INTERPRETER,
            "print(1)",
            DEFAULT_TIMEOUT,
            gesture(),
        )
        .expect_err("an interpreter that does not exist cannot be started");

        let text = err.to_string();
        assert!(text.contains("Python"), "{text}");
        assert!(
            text.contains("bp-execution-definitely-not-a-real-binary"),
            "{text}"
        );

        match err {
            ExecutionError::InterpreterNotFound { language, tried } => {
                assert_eq!(language, Language::Python);
                assert_eq!(tried, NO_SUCH_INTERPRETER.to_vec());
            }
            other => panic!("expected InterpreterNotFound, got {other:?}"),
        }
    }
}
