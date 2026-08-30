//! The Tools menu's readouts: three questions about the thing in front of
//! you that none of the other menus answers (ADR-0048).
//!
//! **What separates these from Document Inspector**, which lives in
//! `state.rs` and came first: that one answers *what is in this document* --
//! words, lines, format, encoding. These three answer questions about
//! everything around it.
//!
//! | Row | Its subject |
//! | --- | --- |
//! | Security Inspector | The *policy* in force, on all seven axes, and where each axis got its answer |
//! | File Analysis | The *file on disk*, which is not the same object as the document |
//! | Configuration | The *settings*, and where each one came from |
//!
//! ## Why none of them is a settings editor
//!
//! Every setting this product has is already editable where it applies --
//! theme and zoom in View, indentation and encoding in Format, the security
//! profile in Security. What none of those says is *where a value came from*
//! when the user did not choose it in this session, which is the question a
//! config file, four environment variables and a command line make worth
//! asking. Configuration reads; it does not write, and it says so.

use bp_security::{Clipboard, Embeddings, Metadata, Network, Recovery, TemporaryFiles, Zeroise};

use super::AppState;

impl AppState {
    /// Tools ▸ Security Inspector: the policy actually in force, axis by
    /// axis.
    ///
    /// **The Security menu shows three of these seven and the rest are
    /// invisible**, which is the gap this fills: `Policy` has seven axes,
    /// the menu had room for recovery, clipboard and network, and a user had
    /// no way to discover that embeddings, temporary files and zeroising are
    /// governed at all.
    ///
    /// Reports the policy *under Privacy Mode*, not the document's own, for
    /// the reason `menus::security` gives about its own readouts: showing the
    /// unclamped policy would tell somebody their clipboard is kept while
    /// Privacy Mode is discarding it. Both are named, so a clamped axis shows
    /// its own answer as well as the one that overrode it.
    pub(crate) fn security_inspector_report(&self) -> String {
        let own = self.security();
        let policy = own.policy_under(self.privacy);
        let unclamped = own.policy();

        let mut lines = vec![
            format!("Profile: {}", own.name()),
            format!(
                "Privacy Mode: {}",
                if self.privacy.is_on() { "on" } else { "off" }
            ),
            String::new(),
            "In force, on every axis this product governs:".to_owned(),
        ];

        // Each axis names the profile's own answer too when Privacy Mode has
        // overridden it. A readout that silently reported the clamped value
        // would make Privacy Mode look like it had changed the document.
        let mut axis = |name: &str, now: String, own: String| {
            if now == own {
                lines.push(format!("- {name}: {now}"));
            } else {
                lines.push(format!("- {name}: {now}  (profile alone says: {own})"));
            }
        };

        axis(
            "Recovery journal",
            recovery(policy.recovery),
            recovery(unclamped.recovery),
        );
        axis(
            "Clipboard history",
            clipboard(policy.clipboard),
            clipboard(unclamped.clipboard),
        );
        axis(
            "Metadata store",
            metadata(policy.metadata),
            metadata(unclamped.metadata),
        );
        axis(
            "Embeddings",
            embeddings(policy.embeddings),
            embeddings(unclamped.embeddings),
        );
        axis(
            "Leaves this machine",
            network(policy.network),
            network(unclamped.network),
        );
        axis(
            "Temporary files",
            temporary(policy.temporary_files),
            temporary(unclamped.temporary_files),
        );
        axis(
            "Zeroise on close",
            zeroise(policy.zeroise),
            zeroise(unclamped.zeroise),
        );

        lines.push(String::new());
        lines.push(
            "A profile can only be tightened by Privacy Mode, never loosened \
             (ADR-0020)."
                .to_owned(),
        );
        lines.join("\n")
    }

    /// Tools ▸ File Analysis: the file on disk, which is a different object
    /// from the document.
    ///
    /// **The distinction is the whole row.** Document Inspector counts what
    /// is in the buffer; this reports what is on the filesystem, and the two
    /// disagree exactly when it matters -- an unsaved edit, a file changed
    /// underneath you, a document that has never been written at all.
    pub(crate) fn file_analysis_report(&self) -> String {
        let Some(doc) = self.workspace.active() else {
            return "There is no document to analyse.".to_owned();
        };
        let Some(path) = doc.path() else {
            return "This document has never been saved, so there is no file \
                    to analyse. Document Inspector describes what is in the \
                    buffer."
                .to_owned();
        };

        let mut lines = vec![format!("Path: {}", path.display())];

        match std::fs::metadata(path) {
            Ok(meta) => {
                lines.push(format!("Size on disk: {}", super::human_bytes(meta.len())));
                // The size class is a real behavioural fork rather than a
                // statistic: it decides whether the rope holds the document
                // or the viewer streams it (ADR-0027, ADR-0030).
                let class = bp_buffer::SizeClass::of(meta.len());
                lines.push(format!(
                    "Size class: {} — {}",
                    class.label(),
                    if class.must_stream() {
                        "read from disk as you scroll, read-only"
                    } else {
                        "held in memory and editable"
                    }
                ));
                lines.push(format!(
                    "Read-only on disk: {}",
                    if meta.permissions().readonly() {
                        "yes"
                    } else {
                        "no"
                    }
                ));
            }
            Err(error) => {
                // Named rather than swallowed: a path that will not stat is
                // the interesting case, not the boring one.
                lines.push(format!("The file could not be read: {error}"));
            }
        }

        lines.push(String::new());
        lines.push(format!("Detected format: {}", self.format().label()));
        lines.push(format!("Encoding: {}", doc.encoding().label()));
        lines.push(format!("Line ending: {}", doc.line_ending().label()));

        lines.push(String::new());
        lines.push(format!(
            "Unsaved changes in this tab: {}",
            if doc.is_dirty() { "yes" } else { "no" }
        ));
        lines.push(format!(
            "Changed on disk since it was opened: {}",
            match self.disk_state() {
                Some(bp_files::DiskState::Modified) => "yes",
                Some(bp_files::DiskState::Missing) => "the file is gone",
                Some(bp_files::DiskState::Unchanged) => "no",
                // No stamp means nothing has been read or written through
                // this tab, so there is nothing to compare against -- which
                // is not the same answer as "no".
                None => "not known",
            }
        ));
        lines.join("\n")
    }

    /// Note ▸ Recovery Checkpoints (ADR-0048): what the crash-recovery
    /// journal is holding for this document, and what the profile does with
    /// it.
    ///
    /// **Not "Revision History", which `MENU_MAP.md` named until 2026-08-22.**
    /// `bp-history` is a crash-recovery journal, not a version store: it holds
    /// the *pending* checkpoint for work not yet on disk and discards it the
    /// moment a save succeeds. A row called Revision History would promise
    /// successive versions to go back to, which this product does not keep
    /// and would be a storage decision rather than a menu row.
    ///
    /// What it *can* honestly answer is the question the status bar only
    /// hints at, and which is invisible for an encrypted document until it is
    /// unlocked: is there a checkpoint, how old is it, and would there be one
    /// at all under this profile.
    pub(crate) fn recovery_report(&self) -> String {
        let policy = self.security().policy_under(self.privacy);
        let mut lines = vec![format!(
            "Under this document's profile, the recovery journal is: {}",
            recovery(policy.recovery)
        )];

        // Named before the count, because "none" means two entirely different
        // things depending on it -- nothing to recover, or nothing is ever
        // written. A reader who is not told which will assume the first.
        if policy.recovery == Recovery::Disabled {
            lines.push(String::new());
            lines.push(
                "So there is nothing to list, and there never will be while                  this profile applies. A plaintext document under a strict                  profile gets no recovery at all -- encrypt it, and the                  journal is sealed with its passphrase instead."
                    .to_owned(),
            );
            return lines.join(
                "
",
            );
        }

        let pending = self.journal.pending();
        let mine: Vec<&bp_history::Checkpoint> = self
            .workspace
            .active()
            .and_then(bp_core::Document::path)
            .map(|path| {
                pending
                    .iter()
                    .map(|(_, checkpoint)| checkpoint)
                    .filter(|checkpoint| checkpoint.path.as_deref() == Some(path))
                    .collect()
            })
            .unwrap_or_default();

        lines.push(String::new());
        if mine.is_empty() {
            lines.push(
                "There is no pending checkpoint for this document, which                  means its work is on disk. A checkpoint is written while                  there are unsaved edits and discarded the moment a save                  succeeds."
                    .to_owned(),
            );
        } else {
            let now = bp_history::now_unix();
            for checkpoint in &mine {
                lines.push(format!(
                    "- {} — written {} ago",
                    checkpoint.name,
                    human_duration(now.saturating_sub(checkpoint.written_at))
                ));
            }
        }

        let sealed = self.journal.sealed_count();
        if sealed > 0 {
            lines.push(String::new());
            lines.push(format!(
                "{sealed} sealed checkpoint{} also exist for encrypted                  documents. They are filed under a digest of the path and can                  only be read once that document is unlocked, so none of them                  can be listed here by name.",
                if sealed == 1 { "" } else { "s" }
            ));
        }

        lines.push(String::new());
        lines.push(format!("Journal: {}", self.journal.location().display()));
        lines.join(
            "
",
        )
    }

    /// Tools ▸ Configuration: the settings in force, and where each came
    /// from.
    ///
    /// **Read-only, and the doc comment above says why.** Everything here is
    /// already editable in the menu it belongs to; what nothing else answers
    /// is where a value came from when the user did not pick it this session.
    pub(crate) fn configuration_report(&self) -> String {
        let mut lines = vec![
            "In force now:".to_owned(),
            format!("- Theme: {}", self.theme.name()),
            format!("- Font size: {} pt", self.font_size),
            format!(
                "- Indentation: {}",
                if self.indent.spaces {
                    format!("{} spaces", self.indent.width)
                } else {
                    format!("tab, {} columns wide", self.indent.width)
                }
            ),
            String::new(),
        ];

        // The file is named whether or not it exists, and the two cases read
        // differently on purpose: "no file" and "a file we could not find"
        // are the same sentence to somebody wondering why their setting did
        // not apply.
        match bp_config::config_path() {
            Some(path) if path.exists() => {
                lines.push(format!("Configuration file: {}", path.display()));
            }
            Some(path) => {
                lines.push(format!(
                    "Configuration file: {} — not created yet, so every value \
                     above came from a default, the environment, the command \
                     line, or this session.",
                    path.display()
                ));
            }
            None => {
                lines.push(
                    "Configuration file: none — this environment does not say \
                     where the user profile is, so no file is read and no \
                     path is guessed at."
                        .to_owned(),
                );
            }
        }

        lines.push(String::new());
        lines.push(
            "Later sources win: defaults, then the file, then BPAD_* \
             environment variables, then the command line. A change made from \
             a menu applies to this session and is not written back."
                .to_owned(),
        );
        lines.join("\n")
    }
}

/// "12 seconds", "4 minutes", "2 hours" -- a rough age, for a readout where
/// the point is whether the checkpoint is recent rather than exactly when.
///
/// Rounds down and stops at hours: a journal entry older than a day means the
/// application has been open a very long time with unsaved work, and the
/// difference between 30 and 40 hours is not what the reader needs.
fn human_duration(seconds: u64) -> String {
    let plural = |n: u64, unit: &str| format!("{n} {unit}{}", if n == 1 { "" } else { "s" });
    match seconds {
        0..=89 => plural(seconds, "second"),
        90..=5399 => plural(seconds / 60, "minute"),
        _ => plural(seconds / 3600, "hour"),
    }
}

// Each axis in the user's words rather than the enum's. Free functions so
// the wording is asserted by a test without a policy to build first, the
// same shape `menus::describe_recovery` already has for the three axes the
// Security menu shows.

fn recovery(value: Recovery) -> String {
    match value {
        Recovery::Plaintext => "kept, unencrypted",
        Recovery::Encrypted => "kept, encrypted with the document's passphrase",
        Recovery::Disabled => "never written",
    }
    .to_owned()
}

fn clipboard(value: Clipboard) -> String {
    match value {
        Clipboard::Persistent => "kept between sessions",
        Clipboard::InMemory => "kept in memory only",
        Clipboard::Disabled => "not kept",
    }
    .to_owned()
}

fn metadata(value: Metadata) -> String {
    match value {
        Metadata::Summary => "a summary recorded: path, title and tags",
        Metadata::PathOnly => "the path only",
        Metadata::Disabled => "nothing recorded",
    }
    .to_owned()
}

fn embeddings(value: Embeddings) -> String {
    match value {
        Embeddings::Cloud => "permitted, including a cloud provider",
        Embeddings::Local => "permitted, on this machine only",
        Embeddings::None => "never computed",
    }
    .to_owned()
}

fn network(value: Network) -> String {
    match value {
        Network::Allowed => "permitted",
        Network::Denied => "never",
    }
    .to_owned()
}

fn temporary(value: TemporaryFiles) -> String {
    match value {
        TemporaryFiles::Allowed => "permitted",
        TemporaryFiles::Denied => "never written",
    }
    .to_owned()
}

fn zeroise(value: Zeroise) -> String {
    match value {
        Zeroise::Off => "buffers are dropped normally",
        Zeroise::On => "buffers are overwritten when a document closes",
    }
    .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_security_inspector_names_all_seven_axes() {
        let report = AppState::new().security_inspector_report();
        for axis in [
            "Recovery journal",
            "Clipboard history",
            "Metadata store",
            "Embeddings",
            "Leaves this machine",
            "Temporary files",
            "Zeroise on close",
        ] {
            assert!(report.contains(axis), "{axis} is missing from {report}");
        }
    }

    #[test]
    fn privacy_mode_shows_both_the_clamped_answer_and_the_profiles_own() {
        let mut state = AppState::new();
        state.privacy = bp_security::Privacy::On;
        let report = state.security_inspector_report();
        assert!(report.contains("Privacy Mode: on"), "{report}");
        assert!(
            report.contains("profile alone says:"),
            "an axis Privacy Mode overrode must show what it overrode, or the \
             readout reads as though the document itself had changed: {report}"
        );
    }

    #[test]
    fn an_unclamped_axis_is_not_annotated_with_itself() {
        let report = AppState::new().security_inspector_report();
        assert!(
            !report.contains("profile alone says:"),
            "with Privacy Mode off nothing is overridden, and saying so for \
             every axis would bury the ones that are: {report}"
        );
    }

    #[test]
    fn file_analysis_distinguishes_never_saved_from_unreadable() {
        let report = AppState::new().file_analysis_report();
        assert!(
            report.contains("never been saved"),
            "a document with no path has no file, which is not the same as a \
             file that failed to read: {report}"
        );
    }

    #[test]
    fn file_analysis_reports_the_file_rather_than_the_buffer() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("note.txt");
        std::fs::write(&path, "hello").expect("write");

        let mut state = AppState::new();
        state.open(path.clone());

        let report = state.file_analysis_report();
        assert!(report.contains("note.txt"), "{report}");
        assert!(report.contains("Size on disk"), "{report}");
        assert!(
            report.contains("Changed on disk since it was opened: no"),
            "{report}"
        );
    }

    #[test]
    fn an_age_reads_in_the_largest_unit_that_still_says_something() {
        assert_eq!(human_duration(1), "1 second");
        assert_eq!(human_duration(45), "45 seconds");
        assert_eq!(human_duration(90), "1 minute");
        assert_eq!(human_duration(3600), "60 minutes");
        assert_eq!(human_duration(7200), "2 hours");
    }

    #[test]
    fn a_profile_that_writes_no_journal_says_so_rather_than_listing_nothing() {
        let mut state = AppState::new();
        state.privacy = bp_security::Privacy::On;
        let report = state.recovery_report();
        assert!(
            report.contains("never will be"),
            "'no checkpoints' and 'checkpoints are never written' are              different answers and a reader will assume the first: {report}"
        );
    }

    #[test]
    fn an_ordinary_profile_reports_no_pending_work_rather_than_a_refusal() {
        let report = AppState::new().recovery_report();
        assert!(report.contains("no pending checkpoint"), "{report}");
        assert!(report.contains("Journal:"), "{report}");
    }

    #[test]
    fn configuration_names_the_file_even_when_there_is_not_one() {
        let report = AppState::new().configuration_report();
        assert!(report.contains("Configuration file:"), "{report}");
        assert!(
            report.contains("Later sources win"),
            "the precedence is the question this row exists to answer: {report}"
        );
    }

    #[test]
    fn configuration_says_it_does_not_write() {
        assert!(
            AppState::new()
                .configuration_report()
                .contains("is not written back"),
            "a row called Configuration that silently discarded a change would \
             be worse than no row"
        );
    }
}
