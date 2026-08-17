//! The recently-opened list.
//!
//! State rather than configuration -- the user never edits it by hand -- so
//! it lives in its own file beside `config.toml` and a corrupt or missing one
//! is simply an empty list. Losing a convenience list is not worth a message,
//! let alone a failure to start.
//!
//! Paths only. This file records *which* documents were opened, never
//! anything about their contents (ADR-0011).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

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

/// Where the recent list is stored, beside the config file.
pub fn recent_path() -> Option<PathBuf> {
    Some(crate::config_path()?.with_file_name(FILE_NAME))
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
}
