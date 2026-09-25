//! The profile a document carries, and the session override above it.
//!
//! **This is what is left of `state/security.rs`**, which was 2,771 lines and
//! held scan, redact, inspect, hash, sign, verify and the security history
//! they all wrote into. ADR-0064 removed every one of those; what survives is
//! the pair of switches that *governed* them, and they survive because they
//! govern two things that are still here — whether the recovery journal is
//! written, and what the metadata store may record.
//!
//! It is named for what it does rather than kept under the old name. A module
//! called `security` holding four profiles and a toggle would be the same
//! overclaim the menu made.

use bp_core::Document;

use super::{AppState, checkpoint_encoding_of, checkpoint_line_ending_of, doc_path};

impl AppState {
    /// The active document's policy, under Privacy Mode.
    ///
    /// Falls back to the default when there is no active document, so callers
    /// never have to choose a policy for themselves -- picking one at a call
    /// site is how a permissive default gets applied to a document that asked
    /// for something stricter.
    pub(crate) fn policy(&self) -> bp_security::Policy {
        // Through `policy_under`, always. Reading a document's own policy
        // directly is how a caller ends up outside Privacy Mode's reach.
        self.security().policy_under(self.privacy)
    }

    pub(crate) fn security(&self) -> bp_security::Security {
        self.workspace
            .active()
            .map_or_else(bp_security::Security::default, Document::security)
    }

    /// Give the active document a profile, and make the world match it.
    ///
    /// Setting a profile is not only a note on the document: tightening one
    /// obliges us to remove what the looser one already permitted. **The
    /// journal is the whole of that now** -- the clipboard history was the
    /// other half and left under ADR-0061 -- and it is handled by asking for
    /// a checkpoint immediately, because a refused checkpoint deletes the
    /// file a looser profile wrote. Waiting for the autosave interval would
    /// leave this morning's plaintext on disk for up to a minute after the
    /// user asked for it to stop.
    pub(crate) fn set_security(&mut self, security: bp_security::Security) {
        let Some(id) = self.workspace.active_id() else {
            return;
        };
        let Some(doc) = self.workspace.get_mut(id) else {
            return;
        };
        let previous = doc.set_security(security);
        if previous == security {
            return;
        }
        let encoding = doc.encoding();
        let line_ending = doc.line_ending();
        let policy = security.policy_under(self.privacy);

        let entry = bp_history::Checkpoint {
            path: doc_path(&self.workspace, id),
            name: self.display_name(id),
            text: self.text_of(id).to_owned(),
            written_at: bp_history::now_unix(),
            encoding: checkpoint_encoding_of(encoding),
            line_ending: Some(checkpoint_line_ending_of(line_ending)),
        };
        let path = entry.path.clone();
        if let Ok(bp_history::Written::Refused(refusal)) =
            self.journal.checkpoint(id.get(), &entry, policy.recovery)
            && let Some(message) = refusal.notice()
        {
            self.error = Some(message.to_owned());
        }
        // An earlier run's checkpoint of the same file is the same plaintext
        // under a profile that now forbids it. The refused checkpoint above
        // only reaches this run's own (ADR-0084).
        if policy.recovery == bp_security::Recovery::Disabled
            && let Some(path) = path
        {
            self.forget_earlier_runs(|checkpoint| checkpoint.path.as_deref() == Some(&path));
        }
    }

    /// Forget what earlier runs left in the journal, where `which` says so.
    /// Returns how many were forgotten.
    ///
    /// Only ever for a privacy control the user has just tightened. A
    /// recovery question left unanswered keeps the work (ADR-0084); asking
    /// for no plaintext on disk is an answer.
    fn forget_earlier_runs(&mut self, which: impl Fn(&bp_history::Checkpoint) -> bool) -> usize {
        let doomed: Vec<bp_history::Entry> = self
            .journal
            .left_behind()
            .into_iter()
            .filter(|(_, checkpoint)| which(checkpoint))
            .map(|(entry, _)| entry)
            .collect();
        if self.journal.forget(&doomed).is_err() {
            return 0;
        }
        self.superseded.retain(|entry| !doomed.contains(entry));
        doomed.len()
    }

    /// Turn Privacy Mode on or off, and make the world match.
    ///
    /// Switching it on has to *act*, not merely be recorded: a mode that only
    /// governed future writes would leave the journals gathered a moment ago
    /// exactly where they were, which is the opposite of what somebody
    /// switching it on wants.
    pub(crate) fn set_privacy(&mut self, privacy: bp_security::Privacy) {
        if self.privacy == privacy {
            return;
        }
        self.privacy = privacy;
        if !privacy.is_on() {
            // Turning it off restores each document's own profile. Nothing to
            // clean up -- the clamp only ever removed permissions.
            return;
        }
        // Every document, not only the active one: the mode is session-wide,
        // and a journal left behind for a background tab is exactly what it
        // was switched on to prevent.
        self.checkpoint_all();
        // And what earlier runs left: switching it on is asking for no
        // plaintext on disk, and the manual promises journals already written
        // are removed -- not only this run's.
        let earlier = self.forget_earlier_runs(|_| true);
        self.error = Some(if earlier == 0 {
            "Privacy Mode on -- journals removed".to_owned()
        } else {
            format!("Privacy Mode on -- journals removed, including {earlier} an earlier run left")
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_document_carries_the_default_profile() {
        assert_eq!(AppState::new().security(), bp_security::Security::default());
    }

    #[test]
    fn each_tab_keeps_its_own_profile() {
        // Two tabs open side by side can be governed differently, and the
        // stricter one must not be relaxed by the other being open.
        let mut state = AppState::new();
        state.set_security(bp_security::Security::Named(
            bp_security::Profile::Confidential,
        ));
        let strict = state.workspace.active_id().unwrap();

        state.new_document();
        assert_eq!(
            state.security(),
            bp_security::Security::default(),
            "a new document is not governed by the last one"
        );

        state.workspace.set_active(strict);
        assert_eq!(
            state.policy().metadata,
            bp_security::Metadata::Disabled,
            "switching back restores the stricter document's policy"
        );
    }

    #[test]
    fn setting_the_same_profile_again_changes_nothing() {
        // `refresh` rebuilds menus constantly, so a no-op that acted would
        // act every time a menu was opened.
        let mut state = AppState::new();
        state.edit("work".to_owned());

        state.set_security(bp_security::Security::default());

        assert!(state.error.is_none(), "a no-op must say nothing");
    }

    #[test]
    fn privacy_mode_removes_journals_already_written() {
        // Switching it on has to *act*, not merely be recorded. A mode that
        // only governed future writes would leave everything gathered a
        // moment ago exactly where it was.
        //
        // The clipboard history was the other half of this until ADR-0061 and
        // the sealed journal a third until ADR-0064. The plaintext journal is
        // now the only already-written state Privacy Mode clears, which makes
        // this test more load-bearing than it was, not less.
        let dir = tempfile::tempdir().expect("temp dir");
        let mut state = AppState::new();
        state.journal = bp_history::Journal::new(dir.path().to_path_buf());
        state.edit("unsaved work".to_owned());
        state.checkpoint_all();
        assert_eq!(state.journal.pending().len(), 1);

        state.set_privacy(bp_security::Privacy::On);

        assert!(state.journal.pending().is_empty(), "the journal survived");
    }

    #[test]
    fn a_profile_that_forbids_a_journal_deletes_the_one_already_there() {
        // The half of `set_security` that is not bookkeeping. Tightening a
        // profile has to remove what the looser one wrote, and it has to
        // happen now rather than at the next autosave -- otherwise the
        // plaintext the user just asked to stop sits on disk for a minute
        // more.
        let dir = tempfile::tempdir().expect("temp dir");
        let mut state = AppState::new();
        state.journal = bp_history::Journal::new(dir.path().to_path_buf());
        state.edit("unsaved work".to_owned());
        state.checkpoint_all();
        assert_eq!(state.journal.pending().len(), 1);

        state.set_security(bp_security::Security::Named(
            bp_security::Profile::Confidential,
        ));

        assert!(
            state.journal.pending().is_empty(),
            "tightening a profile left the journal a looser one wrote"
        );
    }

    fn earlier_run_left(dir: &std::path::Path, path: &str, text: &str) {
        let entry = bp_history::Checkpoint {
            path: Some(std::path::PathBuf::from(path)),
            name: path.to_owned(),
            text: text.to_owned(),
            written_at: bp_history::now_unix(),
            encoding: bp_history::CheckpointEncoding::Utf8,
            line_ending: None,
        };
        bp_history::Journal::new(dir.to_path_buf())
            .checkpoint(1, &entry, bp_security::Recovery::Plaintext)
            .expect("an earlier run's checkpoint");
    }

    #[test]
    fn privacy_mode_also_removes_what_an_earlier_run_left() {
        // Found in review of PR #6. Each run now removes only its own
        // checkpoints, so turning Privacy Mode on after answering Not Now left
        // the earlier run's plaintext on disk -- and the manual says journals
        // already written are removed. Asking for no plaintext is an answer
        // to the recovery question, where a dismissed dialog is not.
        let dir = tempfile::tempdir().expect("temp dir");
        earlier_run_left(dir.path(), "/notes/a.txt", "left behind");
        let mut state = AppState::new();
        state.journal = bp_history::Journal::new(dir.path().to_path_buf());
        assert_eq!(state.journal.left_behind().len(), 1);

        state.set_privacy(bp_security::Privacy::On);

        assert!(
            state.journal.pending().is_empty(),
            "an earlier run's plaintext survived"
        );
        assert!(
            state
                .error
                .as_deref()
                .is_some_and(|e| e.contains("1 an earlier run left")),
            "the status must say what was removed: {:?}",
            state.error
        );
    }

    #[test]
    fn tightening_a_profile_removes_an_earlier_runs_checkpoint_of_that_file_only() {
        let dir = tempfile::tempdir().expect("temp dir");
        earlier_run_left(dir.path(), "/notes/secret.txt", "this file");
        earlier_run_left(dir.path(), "/notes/other.txt", "another file");
        let mut state = AppState::new();
        state.journal = bp_history::Journal::new(dir.path().to_path_buf());
        // The open document stands for /notes/secret.txt.
        let id = state.workspace.open_path(
            std::path::PathBuf::from("/notes/secret.txt"),
            super::super::now(),
        );
        state.editors.insert(id, bp_editor::Editor::new("text"));

        state.set_security(bp_security::Security::Named(
            bp_security::Profile::Confidential,
        ));

        let left: Vec<String> = state
            .journal
            .left_behind()
            .into_iter()
            .map(|(_, c)| c.text)
            .collect();
        assert_eq!(
            left,
            ["another file"],
            "only the tightened file's checkpoint goes"
        );
    }
}
