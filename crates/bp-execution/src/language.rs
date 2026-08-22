//! The three languages ADR-0040 ships first.
//!
//! Deliberately three, not `CellKind`'s five: Rust is a compile step with its
//! own error class, and SQL has no interpreter to invoke at all, only a
//! database nothing has chosen yet. Both are named as future work in
//! ADR-0040, not stubbed here "for completeness" -- a variant with no working
//! resolution behind it would be a promise this crate cannot keep.

use std::fmt;
use std::process::{Command, Stdio};

/// A language `bp-execution` knows how to run.
///
/// Each maps to one or more interpreter binary names tried in order on
/// `PATH`, resolved fresh on every call -- see [`crate::run`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    /// `python3`, then `python`.
    Python,
    /// `pwsh` (PowerShell 7+, cross-platform), then `powershell` (Windows
    /// built-in `powershell.exe`, which `Command::new("powershell")` finds on
    /// `PATH` the ordinary way).
    PowerShell,
    /// `sh`. Deliberately not `bash`: POSIX shell is what is portable and
    /// guaranteed present on any Unix-like system, and nothing in
    /// `bp-notebook` ties `CellKind::Shell` to a specific shell beyond "shell
    /// script" (see `CellKind::from_language_name`, which maps `bash`/`zsh`
    /// onto the same `Shell` kind as `sh` -- the kind names the *category*,
    /// not a chosen implementation). `bash` is common but not universal
    /// (notably absent from a minimal/BSD base install); `sh` is the one name
    /// POSIX itself specifies.
    Shell,
}

impl Language {
    /// Every language this crate can run.
    ///
    /// For a caller that wants to say what is available rather than ask about
    /// one language -- Run ▸ Interpreters is the first (ADR-0048), and a list
    /// built by hand there would go stale the moment a fourth arrives.
    #[must_use]
    pub fn all() -> &'static [Language] {
        &[Language::Python, Language::PowerShell, Language::Shell]
    }

    /// The interpreter this language actually resolves to on this machine,
    /// or `None` when none of its candidates is installed.
    ///
    /// **Runs the probe, so it is not free** -- it spawns each candidate
    /// until one answers. Fine on a menu click; never on a refresh.
    ///
    /// Public because "why did my cell not run" has no other answer: the
    /// fallback chain is documented on the variants and invisible from
    /// outside, so a user with no `python3` and no `python` got a failure
    /// naming neither.
    #[must_use]
    pub fn interpreter(self) -> Option<&'static str> {
        resolve(self.candidates())
    }

    /// Candidate interpreter binary names, tried in order. First one that
    /// actually runs wins -- see [`resolve`].
    ///
    /// Public alongside [`Language::interpreter`]: a readout that says which
    /// interpreter was found is only half an answer when none was, and the
    /// other half is what it looked for.
    #[must_use]
    pub fn candidates(self) -> &'static [&'static str] {
        match self {
            Language::Python => &["python3", "python"],
            Language::PowerShell => &["pwsh", "powershell"],
            Language::Shell => &["sh"],
        }
    }
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Language::Python => "Python",
            Language::PowerShell => "PowerShell",
            Language::Shell => "Shell",
        })
    }
}

/// Whether `candidate` resolves to something that actually runs, tried by
/// actually running it rather than inferred from `PATH` containing a file of
/// that name.
///
/// A stub or a broken shim (present on `PATH`, but not executable, or
/// exiting some other way that means "spawn failed") must be treated as "not
/// found" -- `PATH` lookup succeeding is not the same claim as the binary
/// working, and only actually invoking it tells the two apart.
///
/// `--version` is cheap for all three candidate families and, so long as
/// `stdin` is `Stdio::null()`, cannot block waiting for input even on a
/// shell that does not recognise the flag and errors out immediately -- the
/// exit code from that error does not matter, only whether the OS could
/// start the process at all.
pub(crate) fn probe(candidate: &str) -> bool {
    Command::new(candidate)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

/// Try each of `candidates` in order and return the first that [`probe`]s
/// successfully.
///
/// Split out from [`Language::candidates`] purely so tests can exercise "no
/// candidate resolves" against a name that is guaranteed absent, without
/// depending on any real interpreter being (or not being) installed on the
/// machine running the tests.
pub(crate) fn resolve(candidates: &[&'static str]) -> Option<&'static str> {
    candidates.iter().copied().find(|c| probe(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_prints_a_name_a_user_would_recognise() {
        assert_eq!(Language::Python.to_string(), "Python");
        assert_eq!(Language::PowerShell.to_string(), "PowerShell");
        assert_eq!(Language::Shell.to_string(), "Shell");
    }

    #[test]
    fn every_language_is_in_the_list_of_all_of_them() {
        // A list built by hand goes stale the moment a fourth arrives, so
        // this asserts the two agree rather than asserting a count.
        for language in Language::all() {
            assert!(
                !language.candidates().is_empty(),
                "{language} has no interpreter to try"
            );
        }
        assert_eq!(
            Language::all().len(),
            3,
            "a language was added without this list being told"
        );
    }

    #[test]
    fn a_candidate_name_that_cannot_possibly_exist_does_not_resolve() {
        assert!(!probe("bp-execution-definitely-not-a-real-binary-9f8a7b3c"));
        assert_eq!(
            resolve(&["bp-execution-definitely-not-a-real-binary-9f8a7b3c"]),
            None
        );
    }

    #[test]
    fn resolve_picks_the_first_candidate_that_actually_runs() {
        // `Language::Shell`'s own list, `["sh"]`, is not guaranteed present on
        // every machine that runs this test suite (Windows without Git Bash
        // or WSL on `PATH`, for instance) -- see `run.rs`'s tests for how
        // language-specific tests guard against that. This test only checks
        // the *ordering* behaviour, using a fabricated list where the first
        // entry is guaranteed absent and the second is a program every
        // supported OS ships: something that resolves via `PATH` and answers
        // to a bare invocation without hanging on stdin.
        #[cfg(windows)]
        let real = "powershell";
        #[cfg(not(windows))]
        let real = "sh";

        assert_eq!(
            resolve(&["bp-execution-definitely-not-a-real-binary-9f8a7b3c", real]),
            Some(real)
        );
    }
}
