//! Temp paths for tests that a *previous* run cannot have left behind.
//!
//! Test-only, and it exists because two places invented the same scheme and
//! both got it subtly wrong in the same way.
//!
//! ## The defect this was extracted to fix
//!
//! `audit::audit_path` and `state::security::default_signing_key_path` each
//! built a per-test temp name from the process id and a counter, with a doc
//! comment explaining that the pid is there "so two runs at once cannot
//! collide". That is true and it is not the dangerous case.
//!
//! **The dangerous case is two runs that are not at once.** Nothing ever
//! deleted these files, and Windows recycles process ids from a small space,
//! so a test process eventually starts with a pid some earlier run already
//! used -- and its "unique" path then names that run's leftover file. For the
//! audit log that means opening a history whose sequence numbers already
//! start at one, which `AuditLog` correctly refuses; for the signing key it
//! means `begin_signing` finding a key sealed under a passphrase this test
//! does not know.
//!
//! It was found by a test failing one run in five *in isolation*, with 5,862
//! leftover files across 495 distinct pids in the temp directory. **The suite
//! got flakier the more it was run**, which is the worst shape a flake can
//! have: the machine that runs the tests most is the one that trusts them
//! least.
//!
//! ## What makes a name unique here
//!
//! A nanosecond timestamp as well as the pid and the counter, so a recycled
//! pid cannot reproduce an earlier name; the file is removed first anyway, so
//! a name that somehow repeats still starts empty. And a sweep, once per
//! process, of anything this module made that is more than an hour old --
//! because a scheme that only stops *collisions* still leaves the temp
//! directory growing by a file per test for ever.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// Every name this module makes starts with this, which is what makes the
/// sweep safe: it deletes only files it could have created itself.
const PREFIX: &str = "bpad-test-";

/// How old a leftover has to be before the sweep takes it.
///
/// An hour rather than immediately, because a *concurrent* run's files are
/// live and deleting one would break a passing test to tidy up after a
/// finished one. No test run takes an hour.
const STALE_SECONDS: u64 = 60 * 60;

/// A temp path nothing else is using, and no earlier run can have left
/// behind.
///
/// `kind` names what it is for and becomes part of the filename, so a failing
/// run points at a file somebody can go and read.
pub(crate) fn unique(kind: &str, extension: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);

    sweep_once();

    // The clock as well as the pid: a recycled pid cannot also reproduce the
    // nanosecond an earlier run started at.
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());

    let path = std::env::temp_dir().join(format!(
        "{PREFIX}{kind}-{}-{stamp}-{n}.{extension}",
        std::process::id()
    ));
    // Belt as well as braces. The name above should be unrepeatable; if it
    // ever repeats, the caller still gets an empty file rather than somebody
    // else's history or somebody else's key.
    let _ = std::fs::remove_file(&path);
    path
}

/// Delete this module's leftovers from finished runs. Once per process.
///
/// Best-effort throughout: a file another process is holding open simply
/// stays, and a temp directory that will not enumerate is not a reason to
/// fail a test that has nothing to do with it.
fn sweep_once() {
    static SWEPT: std::sync::Once = std::sync::Once::new();
    SWEPT.call_once(|| {
        let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if !name.starts_with(PREFIX) {
                continue;
            }
            let old = entry
                .metadata()
                .and_then(|meta| meta.modified())
                .and_then(|when| {
                    when.elapsed()
                        .map_err(|_| std::io::Error::other("modified in the future"))
                })
                .is_ok_and(|age| age.as_secs() > STALE_SECONDS);
            if old {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_paths_are_never_the_same() {
        assert_ne!(unique("thing", "log"), unique("thing", "log"));
    }

    #[test]
    fn a_path_names_what_it_is_for_so_a_failure_can_be_looked_at() {
        let path = unique("security-history", "log");
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        assert!(name.starts_with(PREFIX), "{name}");
        assert!(name.contains("security-history"), "{name}");
        assert!(name.ends_with(".log"), "{name}");
    }

    #[test]
    fn a_returned_path_does_not_exist_yet() {
        // The property the whole module is for: whatever the name, the caller
        // is handed somewhere empty. A stale file here is somebody else's
        // history or somebody else's signing key.
        let path = unique("collision", "log");
        std::fs::write(&path, b"a previous run was here").expect("write");

        // Same inputs, and it must still come back clean.
        let again = unique("collision", "log");
        assert!(
            !again.exists(),
            "a path handed out must never already hold a previous run's file"
        );
        let _ = std::fs::remove_file(&path);
    }
}
