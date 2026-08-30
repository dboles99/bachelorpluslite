//! The recently-opened list.
//!
//! State rather than configuration -- the user never edits it by hand -- so
//! it lives in its own file, in the *state* directory rather than beside
//! `config.toml`, and a corrupt or missing one is simply an empty list.
//! Losing a convenience list is not worth a message, let alone a failure to
//! start.
//!
//! ## Why not beside `config.toml`
//!
//! It used to be, and that was wrong on Windows for a reason
//! [`bp_platform::dirs::DirKind::roams`] states outright: the configuration
//! directory is `%APPDATA%`, which is copied to every machine the user signs
//! into, and **every entry in this file is a machine-specific absolute
//! path**. Roaming it means a second machine offers a File menu of documents
//! on drive letters it does not have, and pays for the copy at every
//! sign-out. `DirKind::State` is the same directory on Linux under XDG's own
//! rules (`$XDG_STATE_HOME`, "data that should persist between restarts but
//! is not important enough for the data directory") and a local one on
//! Windows, which is the whole point.
//!
//! Nothing migrates the old file, and nothing deletes it either: it is a
//! convenience list of at most ten paths and it rebuilds itself the first
//! time anything is opened, so carrying code forward to find it would cost
//! more than it saves. ADR-0070 owns that decision and generalises it to
//! everything a removed feature leaves on a disk.
//!
//! Paths only. This file records *which* documents were opened, never
//! anything about their contents (ADR-0011).

use std::path::{Path, PathBuf};

use bp_platform::dirs::DirKind;
use serde::{Deserialize, Serialize};

use crate::{EnvSnapshot, Platform};

/// How many entries to keep. Long enough to be useful, short enough that the
/// File menu stays a menu.
pub const MAX_RECENT: usize = 10;

const FILE_NAME: &str = "recent.toml";

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recent {
    #[serde(default)]
    paths: Vec<PathBuf>,
}

impl Recent {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }

    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    /// Record a path as most-recently used.
    ///
    /// Moves an existing entry to the front rather than duplicating it, so
    /// reopening a file does not push everything else out of the list.
    pub fn push(&mut self, path: &Path) {
        self.paths.retain(|p| p != path);
        self.paths.insert(0, path.to_path_buf());
        self.paths.truncate(MAX_RECENT);
    }

    /// Drop entries that no longer exist.
    ///
    /// Called before display: offering to open a file that was deleted or is
    /// on an unplugged drive is worse than not offering it.
    pub fn prune_missing(&mut self) {
        self.paths.retain(|p| p.exists());
    }

    pub fn parse(text: &str) -> Self {
        // A corrupt list is an empty list. There is nothing here worth
        // interrupting the user over.
        toml::from_str(text).unwrap_or_default()
    }

    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).unwrap_or_default()
    }
}

/// Where the recent list is stored: in the product's state directory.
///
/// The edge, like [`crate::config_path`]: it reads the environment once and
/// hands it to [`recent_path_in`], which carries the rule.
#[must_use]
pub fn recent_path() -> Option<PathBuf> {
    recent_path_in(Platform::HOST, &EnvSnapshot::from_environment())
}

/// Where the recent list is stored, given a platform and an environment.
///
/// Asks [`bp_platform::dirs`] for [`DirKind::State`] by construction rather
/// than deriving a path from whatever `config_path` returned, so the two
/// cannot drift apart, and so the promise is assertable on both platforms
/// from either leg of CI.
#[must_use]
pub fn recent_path_in(platform: Platform, env: &EnvSnapshot) -> Option<PathBuf> {
    crate::in_product_directory(platform, DirKind::State, env, FILE_NAME)
}

/// Read the recent list, pruning entries that have gone away.
pub fn load_recent() -> Recent {
    let mut recent = recent_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|text| Recent::parse(&text))
        .unwrap_or_default();
    recent.prune_missing();
    recent
}

/// Write the recent list, creating its directory if needed.
///
/// Failures are ignored: not remembering a filename is not worth an error in
/// front of someone who is trying to write.
pub fn save_recent(recent: &Recent) {
    let Some(path) = recent_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, recent.to_toml());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn most_recent_comes_first() {
        let mut r = Recent::new();
        r.push(&p("/a.txt"));
        r.push(&p("/b.txt"));
        assert_eq!(r.paths(), [p("/b.txt"), p("/a.txt")]);
    }

    #[test]
    fn reopening_moves_rather_than_duplicates() {
        let mut r = Recent::new();
        r.push(&p("/a.txt"));
        r.push(&p("/b.txt"));
        r.push(&p("/a.txt"));

        assert_eq!(
            r.paths(),
            [p("/a.txt"), p("/b.txt")],
            "reopening must not push everything else out"
        );
    }

    #[test]
    fn the_list_is_capped() {
        let mut r = Recent::new();
        for i in 0..(MAX_RECENT + 5) {
            r.push(&p(&format!("/f{i}.txt")));
        }
        assert_eq!(r.paths().len(), MAX_RECENT);
        assert_eq!(
            r.paths()[0],
            p(&format!("/f{}.txt", MAX_RECENT + 4)),
            "the newest survives"
        );
    }

    #[test]
    fn a_corrupt_list_is_an_empty_list() {
        // Never a reason to interrupt the user.
        assert_eq!(Recent::parse("this = = not toml"), Recent::new());
        assert_eq!(Recent::parse(""), Recent::new());
    }

    #[test]
    fn it_round_trips_through_toml() {
        let mut r = Recent::new();
        r.push(&p("/notes/One_17AUG2026.txt"));
        r.push(&p("/notes/Two_17AUG2026.md"));

        assert_eq!(Recent::parse(&r.to_toml()), r);
    }

    #[test]
    fn missing_entries_are_pruned() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real.txt");
        std::fs::write(&real, "x").unwrap();

        let mut r = Recent::new();
        r.push(&dir.path().join("gone.txt"));
        r.push(&real);
        assert_eq!(r.paths().len(), 2);

        r.prune_missing();
        assert_eq!(r.paths(), [real], "a deleted file must not be offered");
    }

    #[test]
    fn it_records_paths_and_nothing_else() {
        // ADR-0011: this file says which documents were opened, never
        // anything about what they contain.
        let mut r = Recent::new();
        r.push(&p("/secret/Report_17AUG2026.txt"));
        let toml = r.to_toml();

        assert!(toml.contains("Report_17AUG2026.txt"));
        assert!(
            !toml.to_lowercase().contains("content"),
            "no content field may creep in"
        );
    }

    // --- where the list is kept ----------------------------------------
    //
    // Built from an `EnvSnapshot` here, so these read neither the real
    // environment nor the developer's own configuration directory.

    /// Split a path into (directory, file name) using the *named* platform's
    /// separator.
    ///
    /// `Path::parent` cannot do this job here: it would have to walk a Windows
    /// path while the test runs on Linux, where `\` is an ordinary filename
    /// character and the whole path is therefore one component. Splitting on
    /// the platform's own separator gives both legs the same answer.
    fn split(platform: Platform, path: &str) -> (&str, &str) {
        path.rsplit_once(platform.separator())
            .expect("a resolved path has a directory and a file name")
    }

    #[test]
    fn the_list_sits_where_the_platform_puts_per_machine_state() {
        let env = crate::tests::whole_env();
        assert_eq!(
            crate::tests::text(recent_path_in(Platform::Windows, &env)),
            r"C:\Users\me\AppData\Local\bachelorpad\state\recent.toml",
            "the Windows answer must be spelt with backslashes on either leg"
        );
        assert_eq!(
            crate::tests::text(recent_path_in(Platform::Linux, &env)),
            "/home/me/.local/state/bachelorpad/recent.toml",
            "the Linux answer must be spelt with slashes on either leg"
        );
    }

    #[test]
    fn the_list_is_never_put_somewhere_that_roams() {
        // The defect this file used to have, stated as the rule that catches
        // it rather than as the one path it produced. Every entry in this
        // file is a machine-specific absolute path, so a directory that is
        // copied to the user's other machines is the one place it must not
        // be: the File menu there would offer documents on drive letters
        // that machine does not have.
        let env = crate::tests::whole_env();
        for &platform in Platform::ALL {
            let path = crate::tests::text(recent_path_in(platform, &env));
            let state =
                crate::tests::text(bp_platform::dirs::directory(platform, DirKind::State, &env));
            assert!(
                path.starts_with(&state),
                "{platform:?}: {path:?} is not inside the state directory"
            );
            for kind in DirKind::ALL {
                if *kind == DirKind::State {
                    continue;
                }
                let elsewhere =
                    crate::tests::text(bp_platform::dirs::directory(platform, *kind, &env));
                assert!(
                    !path.starts_with(&elsewhere),
                    "{platform:?}: the recent list must not live in the {} directory",
                    kind.token()
                );
            }
            assert!(
                !DirKind::State.roams(platform),
                "{platform:?}: the directory this file now lives in roams, \
                 which is the thing this move was for"
            );
        }
    }

    #[test]
    fn the_list_and_the_settings_are_two_different_files() {
        // Whichever directories they land in, one must never overwrite the
        // other -- and the file name is the last thing standing between them
        // if they are ever put back together.
        let env = crate::tests::whole_env();
        for &platform in Platform::ALL {
            let recent = crate::tests::text(recent_path_in(platform, &env));
            let config = crate::tests::text(crate::config_path_in(platform, &env));
            assert_ne!(recent, config, "{platform:?}: state overwrote settings");
            let (_, recent_name) = split(platform, &recent);
            let (_, config_name) = split(platform, &config);
            assert_ne!(recent_name, config_name, "{platform:?}");
            assert_eq!(recent_name, FILE_NAME);
        }
    }

    #[test]
    fn a_relative_xdg_state_home_does_not_move_the_list() {
        // Same defect as the config file's, same fix: the recent list must
        // not end up beside the directory the editor happened to be launched
        // from.
        for unusable in crate::tests::UNUSABLE_XDG {
            let env = EnvSnapshot {
                xdg_state_home: Some((*unusable).to_owned()),
                ..crate::tests::whole_env()
            };
            assert_eq!(
                crate::tests::text(recent_path_in(Platform::Linux, &env)),
                "/home/me/.local/state/bachelorpad/recent.toml",
                "{unusable:?} must be ignored"
            );
        }
    }

    #[test]
    fn a_relative_xdg_config_home_no_longer_concerns_the_list_at_all() {
        // It used to decide where this file went. It does not now, and
        // saying so here is what stops the two being quietly rejoined.
        let env = EnvSnapshot {
            xdg_config_home: Some("/etc/xdg".to_owned()),
            ..crate::tests::whole_env()
        };
        assert_eq!(
            crate::tests::text(recent_path_in(Platform::Linux, &env)),
            "/home/me/.local/state/bachelorpad/recent.toml",
            "$XDG_CONFIG_HOME moved a file that is not configuration"
        );
    }

    #[test]
    fn an_empty_environment_yields_no_path_rather_than_a_guess() {
        for &platform in Platform::ALL {
            assert_eq!(recent_path_in(platform, &EnvSnapshot::default()), None);
        }
    }
}
