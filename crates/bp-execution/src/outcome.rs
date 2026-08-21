//! What a run actually did, once it is over.

use std::time::Duration;

/// The result of one cell's subprocess, start to finish.
///
/// Every field is private and read through a method, matching the rest of
/// this workspace's style: nothing here is assembled from parts, only
/// produced by [`crate::run`] or [`crate::run_with_timeout`] and handed back
/// whole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOutcome {
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) exit_code: Option<i32>,
    pub(crate) stdout_truncated: bool,
    pub(crate) stderr_truncated: bool,
    pub(crate) timed_out: bool,
    pub(crate) duration: Duration,
}

impl RunOutcome {
    /// What the process wrote to `stdout`, up to the output cap. Never
    /// merged with [`RunOutcome::stderr`] -- a caller may want to tell them
    /// apart, for instance to show one in a different colour.
    ///
    /// Captured as bytes and converted with `String::from_utf8_lossy`: a
    /// process is free to write bytes that are not valid UTF-8, and losing
    /// the whole capture over one bad byte would throw away a lot of good
    /// output for a small amount of noise.
    pub fn stdout(&self) -> &str {
        &self.stdout
    }

    /// What the process wrote to `stderr`, up to the output cap. See
    /// [`RunOutcome::stdout`] for why the two streams are kept apart and how
    /// the bytes were decoded.
    pub fn stderr(&self) -> &str {
        &self.stderr
    }

    /// The process's exit code, if it has one to report.
    ///
    /// `None` covers two different situations `bp-execution` does not try to
    /// tell apart at this layer: the process was killed for exceeding the
    /// timeout (see [`RunOutcome::timed_out`]), or -- on Unix -- it was
    /// terminated by a signal rather than exiting normally, which carries no
    /// exit code at all.
    pub fn exit_code(&self) -> Option<i32> {
        self.exit_code
    }

    /// Whether the exit code says success (`0`). `false` for anything else,
    /// including a killed or signalled process that has no exit code at all
    /// -- there is no code to call successful, so the honest default is no.
    pub fn success(&self) -> bool {
        self.exit_code == Some(0)
    }

    /// Whether `stdout` was cut off by the output cap before the process
    /// finished producing it.
    pub fn stdout_truncated(&self) -> bool {
        self.stdout_truncated
    }

    /// Whether `stderr` was cut off by the output cap before the process
    /// finished producing it.
    pub fn stderr_truncated(&self) -> bool {
        self.stderr_truncated
    }

    /// Whether the process was killed for running past the timeout, rather
    /// than exiting on its own. When this is `true`, [`RunOutcome::exit_code`]
    /// describes how the kill looked to the OS, not anything the cell's own
    /// code decided.
    pub fn timed_out(&self) -> bool {
        self.timed_out
    }

    /// How long the process actually ran, wall-clock, from spawn to exit (or
    /// to being killed).
    pub fn duration(&self) -> Duration {
        self.duration
    }
}
