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
//! ## The output cap is enforced while reading, not after
//!
//! [`read_capped`] stops pulling bytes off a pipe the moment it has
//! [`OUTPUT_CAP_BYTES`], rather than reading to completion and truncating a
//! buffer afterward. A `print` loop that never terminates must never get the
//! chance to be read to completion in the first place -- that is the whole
//! point of a cap.

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::thread;
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
    run_impl(language, language.candidates(), source, timeout, gesture)
}

/// The real implementation, taking the candidate interpreter list as an
/// explicit parameter rather than deriving it from `language` internally.
///
/// This split exists for one reason: it lets a test exercise "no candidate
/// resolves" (see [`ExecutionError::InterpreterNotFound`]) through the real
/// spawn path end to end, by handing it a candidate list guaranteed to
/// resolve nothing -- without inventing a fourth, fictitious [`Language`]
/// variant, and without mutating the real `PATH` for the whole test process
/// (which a parallel test run would race on).
fn run_impl(
    language: Language,
    candidates: &[&'static str],
    source: &str,
    timeout: Duration,
    gesture: UserGesture,
) -> Result<RunOutcome, ExecutionError> {
    let _ = gesture; // Consumed by value: proves a caller held one to give up.

    let interpreter = resolve(candidates).ok_or_else(|| ExecutionError::InterpreterNotFound {
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

    let start = Instant::now();
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if start.elapsed() >= timeout {
                    timed_out = true;
                    // Kill, then reap: leaving a killed child unwaited is a
                    // zombie process on Unix.
                    let _ = child.kill();
                    break child.wait().ok();
                }
                thread::sleep(POLL_INTERVAL);
            }
            // Cannot observe whether the process has exited. Rare and
            // OS-level, but the run was genuinely attempted and may already
            // have produced output worth keeping -- report it as an outcome
            // with an unknown exit code rather than discarding everything
            // captured so far.
            Err(_) => break None,
        }
    };
    let duration = start.elapsed();

    // No extra wait needed here: by the time the process has exited or been
    // killed-and-reaped above, the OS has closed its end of every pipe, so
    // each reader thread is at (or moments from) EOF regardless of how much
    // of its cap it had used.
    let _ = stdin_writer.join();
    let (stdout_bytes, stdout_truncated) = stdout_reader.join().unwrap_or_default();
    let (stderr_bytes, stderr_truncated) = stderr_reader.join().unwrap_or_default();

    Ok(RunOutcome {
        stdout: String::from_utf8_lossy(&stdout_bytes).into_owned(),
        stderr: String::from_utf8_lossy(&stderr_bytes).into_owned(),
        exit_code: status.and_then(|s| s.code()),
        stdout_truncated,
        stderr_truncated,
        timed_out,
        duration,
    })
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

    #[test]
    fn an_unresolvable_interpreter_is_reported_as_not_found_not_a_panic() {
        let err = run_impl(
            Language::Python,
            NO_SUCH_INTERPRETER,
            "print(1)",
            DEFAULT_TIMEOUT,
            gesture(),
        )
        .unwrap_err();

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
