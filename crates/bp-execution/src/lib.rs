//! Running one notebook cell, as a fresh subprocess, and nothing more.
//!
//! ADR-0040 decides this crate's whole shape. `bp-notebook` already builds
//! `RunRequest` -- a language and some source text, produced only from a
//! `UserGesture` no parsed file can construct. `bp-execution` is the crate
//! its own module docs name as the thing that would consume a `RunRequest`
//! and actually run it: effectively, `run(request.language(),
//! request.source(), gesture)`, once someone writes that call. It does not
//! know what a notebook or a cell is, does not decide when a run is allowed,
//! and is not wired into `bp-notebook` or `bp-ui` by this crate itself --
//! that is later, separate work. This crate is tested and correct standalone
//! first, the same way `bp-notebook` was before anything called it.
//!
//! ## The execution model: one process, then gone
//!
//! **A run is one subprocess, started fresh for this call and torn down
//! before it returns.** No kernel, no persistent interpreter, no session
//! held open between calls. This is the direct reading of "no persistent
//! kernel session" (ADR-0038): nothing is still running when the next call
//! comes in, so nothing from one run is visible to the next. A variable set
//! in one call's Python source is gone by the time another call runs more
//! Python -- there is no live interpreter left holding it. That is a real,
//! visible limitation, not an oversight, and a persistent-session model is a
//! different, larger decision this crate does not make.
//!
//! ## Three languages, deliberately not five
//!
//! [`Language`] has three variants -- Python, PowerShell, POSIX shell --
//! covering the `CellKind` kinds that share one shape: an interpreter binary
//! already on the machine, fed source on `stdin`, producing text output and
//! an exit code. Rust (a compile step with its own error class) and SQL (no
//! interpreter to invoke at all, only a database nothing has chosen) are
//! named as deferred future work by ADR-0040, not stubbed here.
//!
//! ## No sandboxing. None. Stated plainly so nothing overstates it
//!
//! `bp-execution` runs the given source with the same privileges as this
//! process, on this machine, with network and filesystem access exactly as
//! unrestricted as they would be for anything else this user runs. There is
//! no container, no restricted token, no network-denied namespace, and no
//! attempt at one. What stands between an opened notebook and code running
//! on this machine is exactly one thing -- [`UserGesture`], required by value
//! by the only two functions here that start a process ([`run`] and
//! [`run_with_timeout`]) -- and a user reading a cell before running it.
//! That is the whole of the guarantee. Building real sandboxing is a
//! materially larger, platform-specific undertaking ADR-0040 explicitly
//! leaves to a future decision, and pretending otherwise here would be worse
//! than not building the feature.
//!
//! ## What every run gets
//!
//! * A wall-clock timeout ([`DEFAULT_TIMEOUT`], overridable via
//!   [`run_with_timeout`]) after which the process is killed and reaped --
//!   not merely abandoned.
//! * A cap on captured output ([`OUTPUT_CAP_BYTES`] per stream, `stdout` and
//!   `stderr` independently), enforced while reading rather than after, so a
//!   `print` loop that never ends cannot be read to completion before the
//!   cap applies.
//! * [`RunOutcome`] says honestly whether either of those happened.
//!
//! ## What "successful" means here
//!
//! [`ExecutionError`] means only "this run could not even be attempted" --
//! no interpreter found, or an OS-level failure starting or managing the
//! process. A cell whose own code raises, panics or exits non-zero is a
//! *successful* run: it comes back `Ok(RunOutcome)`, with the bad news
//! visible in [`RunOutcome::exit_code`] and [`RunOutcome::stderr`]. Treating
//! a runtime error as an [`ExecutionError`] would misdirect a user toward
//! fixing their environment when the problem was in their code.

#![forbid(unsafe_code)]

mod error;
mod gesture;
mod language;
mod outcome;
mod run;

pub use error::ExecutionError;
pub use gesture::UserGesture;
pub use language::Language;
pub use outcome::RunOutcome;
pub use run::{DEFAULT_TIMEOUT, OUTPUT_CAP_BYTES, run, run_with_timeout};

/// Crate identity used by workspace smoke tests and diagnostics.
pub const CRATE_NAME: &str = "bp-execution";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_crate_names_itself() {
        assert_eq!(CRATE_NAME, "bp-execution");
    }
}
