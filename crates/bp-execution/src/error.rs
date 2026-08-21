//! What can go wrong before a cell's own code ever runs.
//!
//! This enum's whole business is "we could not even attempt this run" -- an
//! interpreter that is not there, or an OS that refused to start or manage
//! the process. A cell whose code raises, panics, segfaults or exits
//! non-zero is not an error from this crate's point of view: that is a
//! *successful* run that happened to produce bad news, and it comes back as
//! an `Ok(RunOutcome)` whose exit code and `stderr` say so. Getting that
//! distinction backwards -- reporting a Python exception the same way as "no
//! Python installed" -- would misdirect a user toward reinstalling an
//! interpreter that was never the problem.

use crate::language::Language;

/// Something prevented a run from starting at all.
#[derive(Debug, thiserror::Error)]
pub enum ExecutionError {
    /// None of a language's candidate interpreter binaries resolved on
    /// `PATH`. `tried` names exactly what was attempted, in order, so the
    /// eventual UI-side message can tell a user what to install rather than
    /// just "no interpreter."
    #[error(
        "no {language} interpreter found on PATH (tried: {})",
        .tried.join(", ")
    )]
    InterpreterNotFound {
        /// The language that could not be run.
        language: Language,
        /// The candidate binary names that were tried, in the order they
        /// were tried.
        tried: Vec<&'static str>,
    },

    /// The interpreter resolved (it answered to a bare `--version` probe)
    /// but the OS refused to start it as a real child process, or something
    /// went wrong managing it afterward (for instance: it could not be
    /// killed after the timeout, or its exit status could not be read).
    /// Distinct from [`ExecutionError::InterpreterNotFound`] because this is
    /// a run that got further before failing -- worth telling apart when
    /// deciding what to tell a user.
    #[error("the interpreter process could not be started or managed: {0}")]
    Process(#[source] std::io::Error),
}
