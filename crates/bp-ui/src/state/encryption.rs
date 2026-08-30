//! Opening, unlocking and sealing an encrypted document.
//!
//! `bp-crypto` owns the `.bpadx` envelope (ADR-0021). This module owns the
//! part of it a person experiences: the passphrase bar's two questions, and
//! what a wrong answer does.
//!
//! ## One rule, and everything here follows from it
//!
//! **An encrypted file does not become a tab until it is unlocked.** A tab
//! nobody can read looks exactly like an empty document, and the next Ctrl+S
//! would write that emptiness over the real one -- so [`open_maybe_encrypted`]
//! reads a header, puts the question up, and opens nothing until [`unlock`]
//! answers it.
//!
//! [`open_maybe_encrypted`]: AppState::open_maybe_encrypted
//! [`unlock`]: AppState::unlock
//!
//! ## Where the passphrase lives, and why
//!
//! In `AppState::passphrases`, for the session, so that saving a `.bpadx`
//! does not ask again on every Ctrl+S -- which would train the user to type it
//! reflexively, which is worse than holding it. An entry there *is* the test
//! for "this document is encrypted"; there is no second flag to fall out of
//! step with it.
//!
//! `crate::passphrase::Ask` is the question the bar is currently putting, and
//! it is the only state this flow keeps between two keystrokes.

use std::path::{Path, PathBuf};

use bp_core::DocumentId;
use bp_files::{SaveOptions, atomic_write};

use super::{AppState, now, read_header};

impl AppState {
    /// Whether `id` is an encrypted document.
    pub(crate) fn is_encrypted(&self, id: DocumentId) -> bool {
        self.passphrases.contains_key(&id)
    }

    /// Begin opening `path`, asking for a passphrase if it is encrypted.
    ///
    /// Read as bytes rather than through `bp_files::load`, which decodes text
    /// and would mangle ciphertext on the way in.
    ///
    /// An encrypted file does **not** become a tab until it is unlocked. A tab
    /// nobody can read is worse than no tab: it looks like an empty document,
    /// and saving it would write emptiness over the real one.
    pub(crate) fn open_maybe_encrypted(&mut self, path: PathBuf) -> bool {
        match read_header(&path) {
            Ok(bytes) if bp_crypto::is_bpadx(&bytes) => {
                self.ask = Some(crate::passphrase::Ask::Unlock(path));
                self.passphrase_status.clear();
                true
            }
            // Not encrypted, or unreadable -- either way `open` handles it and
            // reports properly.
            _ => {
                self.open(path);
                false
            }
        }
    }

    /// Answer whatever the passphrase bar was asking.
    ///
    /// Returns whether the bar should stay open: a wrong passphrase keeps it
    /// up with a message, because closing it would make a typo look like a
    /// refusal to open the file at all.
    pub(crate) fn answer_passphrase(&mut self, entered: &str) -> bool {
        use crate::passphrase::Ask;
        let Some(ask) = self.ask.take() else {
            return false;
        };

        match ask {
            Ask::Unlock(path) => self.unlock(path, entered),
            Ask::Set { id, target } => {
                if entered.is_empty() {
                    // `bp_crypto::seal` refuses this too, but saying so here
                    // means the user is told before typing it twice.
                    self.passphrase_status = "a passphrase is required".to_owned();
                    self.ask = Some(Ask::Set { id, target });
                    return true;
                }
                self.passphrase_status.clear();
                self.ask = Some(Ask::Confirm {
                    id,
                    target,
                    first: zeroize::Zeroizing::new(entered.to_owned()),
                });
                true
            }
            Ask::Confirm { id, target, first } => {
                if first.as_str() != entered {
                    // Back to the first question, not the second. Asking to
                    // confirm again would compare against a passphrase the
                    // user may have already decided was the mistake.
                    self.passphrase_status = "those did not match -- start again".to_owned();
                    self.ask = Some(Ask::Set { id, target });
                    return true;
                }
                self.encrypt(id, &target, &first)
            }

            Ask::SetKey { id } => {
                if entered.is_empty() {
                    self.passphrase_status = "a passphrase is required".to_owned();
                    self.ask = Some(Ask::SetKey { id });
                    return true;
                }
                self.passphrase_status.clear();
                self.ask = Some(Ask::ConfirmKey {
                    id,
                    first: zeroize::Zeroizing::new(entered.to_owned()),
                });
                true
            }
            Ask::ConfirmKey { id, first } => {
                if first.as_str() != entered {
                    // The same recovery as a document''s, and it matters more
                    // here: a key sealed under a passphrase the user did not
                    // mean to type is a key they cannot open, and nothing
                    // says so until the next time they try to sign.
                    self.passphrase_status = "those did not match -- start again".to_owned();
                    self.ask = Some(Ask::SetKey { id });
                    return true;
                }
                self.create_key_and_sign(id, &first)
            }
            Ask::UnlockKey { id } => self.sign_with_stored_key(id, entered),
        }
    }

    /// Decrypt `path` and open it as a tab.
    fn unlock(&mut self, path: PathBuf, passphrase: &str) -> bool {
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) => {
                self.error = Some(e.to_string());
                return false;
            }
        };

        match bp_crypto::open(&bytes, passphrase) {
            Ok(plain) => {
                // Lossy rather than refusing: a document that decrypted and
                // authenticated is the user's own text, and refusing to show
                // it because one byte is not UTF-8 would strand it inside a
                // file only this program can open.
                let text = String::from_utf8_lossy(&plain).into_owned();
                let id = self.workspace.open_path(path.clone(), now());

                // Unsaved work from a crashed session, if there is any. This
                // is the only moment it can be read: the journal is sealed
                // with this passphrase, which nothing had until now. Recovery
                // therefore never needs a prompt of its own -- a program
                // asking for a passphrase unprompted is the habit that makes
                // phishing work.
                let recovered = self.journal.sealed_pending(&path, passphrase);
                let text = match &recovered {
                    Some(checkpoint) => checkpoint.text.clone(),
                    None => text,
                };

                self.editors.insert(id, bp_editor::Editor::new(&text));
                self.passphrases
                    .insert(id, zeroize::Zeroizing::new(passphrase.to_owned()));
                self.mark_in_step(id, &path);
                self.passphrase_status.clear();

                if recovered.is_some() {
                    // Marked modified, because what is on screen is *not*
                    // what is in the file. Leaving it clean would let the
                    // user close the tab and lose the recovered work without
                    // being asked.
                    if let Some(doc) = self.workspace.get_mut(id) {
                        doc.mark_modified();
                    }
                    self.error = Some("recovered unsaved work from a previous session".to_owned());
                } else {
                    self.error = None;
                }
                false
            }
            Err(e) => {
                // The bar stays up. A wrong passphrase is the expected
                // failure, and it is indistinguishable from a damaged file --
                // see `bp_crypto`'s docs for why nothing can do better.
                self.passphrase_status = e.to_string();
                self.ask = Some(crate::passphrase::Ask::Unlock(path));
                true
            }
        }
    }

    /// Seal `id`'s text to `target` and let the tab adopt it.
    fn encrypt(&mut self, id: DocumentId, target: &Path, passphrase: &str) -> bool {
        let sealed = match bp_crypto::seal(
            self.text_of(id).as_bytes(),
            passphrase,
            bp_crypto::SealOptions::default(),
        ) {
            Ok(sealed) => sealed,
            Err(e) => {
                self.passphrase_status = e.to_string();
                return true;
            }
        };

        match atomic_write(target, &sealed, SaveOptions::default()) {
            Ok(_) => {
                // The tab adopts the encrypted file, so later saves stay
                // encrypted. Recording the passphrase is what makes the
                // document encrypted as far as the rest of the shell is
                // concerned -- there is no second flag.
                self.passphrases
                    .insert(id, zeroize::Zeroizing::new(passphrase.to_owned()));
                if let Some(doc) = self.workspace.get_mut(id) {
                    doc.set_path(target.to_path_buf());
                    doc.record_disk_save(now());
                }
                self.mark_in_step(id, target);
                self.passphrase_status.clear();
                self.error = Some(format!("encrypted to {}", target.display()));
                false
            }
            Err(e) => {
                self.passphrase_status = e.to_string();
                true
            }
        }
    }

    /// Forget an encrypted document's passphrase.
    ///
    /// Called when a document closes. Holding it for the session is a
    /// deliberate trade; holding it past the document's life is just a leak.
    pub(crate) fn forget_passphrase(&mut self, id: DocumentId) {
        self.passphrases.remove(&id);
    }

    /// Security ▸ Lock Document (ADR-0048): forget this document's
    /// passphrase now, so the next save or reload asks for it again.
    ///
    /// **The row exists because unlocking is sticky and nothing said so.**
    /// Opening a `.bpadx` holds its passphrase for the life of the tab, which
    /// is what makes saving bearable -- and it means a document unlocked an
    /// hour ago is still unlocked to anyone at the keyboard. Closing the tab
    /// was the only way to undo that, and closing a tab is not what somebody
    /// stepping away from the machine wants to do.
    ///
    /// Returns the sentence the status bar shows. **It does not re-encrypt
    /// anything**: the file on disk has been encrypted the whole time and the
    /// text in the buffer stays where it is. What changes is that this
    /// process no longer holds the key.
    pub(crate) fn lock_document(&mut self) -> String {
        let Some(id) = self.workspace.active_id() else {
            return "There is no document to lock.".to_owned();
        };
        if !self.passphrases.contains_key(&id) {
            // Two different situations, one sentence, because from the user's
            // side they are the same: this tab is not holding a key.
            return "This document is not unlocked: either it is not encrypted,                     or its passphrase is not being held."
                .to_owned();
        }

        self.forget_passphrase(id);
        "Locked. The passphrase will be asked for again on the next save or          reload; the text on screen is unchanged."
            .to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::SaveResult;

    #[test]
    fn locking_a_document_that_holds_no_passphrase_says_so() {
        let mut state = AppState::new();
        assert!(
            state.lock_document().contains("not unlocked"),
            "a plain document and a locked one are the same situation from              the user's side: this tab is not holding a key"
        );
    }

    #[test]
    fn locking_forgets_the_passphrase_without_touching_the_text() {
        let mut state = AppState::new();
        let id = state.workspace.active_id().expect("a tab");
        state
            .passphrases
            .insert(id, zeroize::Zeroizing::new("hunter2".to_owned()));
        let before = state.active_text();

        let said = state.lock_document();

        assert!(said.starts_with("Locked."), "{said}");
        assert!(
            !state.passphrases.contains_key(&id),
            "the whole point of the row is that the process stops holding it"
        );
        assert_eq!(
            state.active_text(),
            before,
            "locking must not re-encrypt or clear the buffer -- the file on              disk was encrypted the whole time"
        );
    }

    /// Drive the passphrase bar the way the shell does: submit, and be told
    /// whether it stays open.
    fn answer(state: &mut AppState, entered: &str) -> bool {
        state.answer_passphrase(entered)
    }

    #[test]
    fn encrypting_a_document_asks_twice_and_writes_only_on_a_match() {
        // A typo when *setting* a passphrase costs the document permanently,
        // which is why this is the one place the bar asks twice.
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("notes.bpadx");
        let mut state = AppState::new();
        state.edit("secret notes".to_owned());
        let id = state.workspace.active_id().unwrap();

        state.ask = Some(crate::passphrase::Ask::Set {
            id,
            target: target.clone(),
        });

        assert!(answer(&mut state, "hunter2"), "asks again to confirm");
        assert!(!target.exists(), "nothing is written after only one entry");

        assert!(!answer(&mut state, "hunter2"), "the bar closes on a match");
        assert!(target.exists(), "and the document is written");
        assert!(state.is_encrypted(id));
    }

    #[test]
    fn a_mismatched_confirmation_starts_again_rather_than_writing() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("notes.bpadx");
        let mut state = AppState::new();
        state.edit("secret".to_owned());
        let id = state.workspace.active_id().unwrap();
        state.ask = Some(crate::passphrase::Ask::Set {
            id,
            target: target.clone(),
        });

        answer(&mut state, "hunter2");
        assert!(answer(&mut state, "hunter3"), "the bar stays up");

        assert!(!target.exists(), "a mismatch must not write anything");
        assert!(!state.is_encrypted(id));
        assert!(
            matches!(state.ask, Some(crate::passphrase::Ask::Set { .. })),
            "back to the first question, not the second -- confirming again \
             would compare against the entry the user already thinks is wrong"
        );
    }

    #[test]
    fn an_empty_passphrase_is_refused_before_it_is_typed_twice() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = AppState::new();
        state.edit("secret".to_owned());
        let id = state.workspace.active_id().unwrap();
        state.ask = Some(crate::passphrase::Ask::Set {
            id,
            target: dir.path().join("notes.bpadx"),
        });

        assert!(answer(&mut state, ""), "the bar stays up");
        assert!(
            matches!(state.ask, Some(crate::passphrase::Ask::Set { .. })),
            "still on the first question"
        );
        assert!(!state.passphrase_status.is_empty(), "and says why");
    }

    #[test]
    fn an_encrypted_document_round_trips_through_the_shell() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("notes.bpadx");

        let mut writer = AppState::new();
        writer.edit("the quick brown fox".to_owned());
        let id = writer.workspace.active_id().unwrap();
        writer.ask = Some(crate::passphrase::Ask::Set {
            id,
            target: target.clone(),
        });
        answer(&mut writer, "hunter2");
        answer(&mut writer, "hunter2");

        // The file on disk must not be readable as the text that went in.
        let raw = std::fs::read(&target).unwrap();
        assert!(bp_crypto::is_bpadx(&raw));
        assert!(
            !raw.windows(3).any(|w| w == b"fox"),
            "the plaintext is in the file"
        );

        let mut reader = AppState::new();
        assert!(
            reader.open_maybe_encrypted(target.clone()),
            "an encrypted file asks for a passphrase"
        );
        assert!(!answer(&mut reader, "hunter2"), "and then opens");

        assert_eq!(reader.active_text(), "the quick brown fox");
    }

    #[test]
    fn an_encrypted_file_does_not_become_a_tab_until_it_is_unlocked() {
        // A tab nobody can read looks like an empty document, and saving it
        // would write emptiness over the real one.
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("notes.bpadx");
        let sealed =
            bp_crypto::seal(b"secret", "hunter2", bp_crypto::SealOptions::default()).unwrap();
        std::fs::write(&target, sealed).unwrap();

        let mut state = AppState::new();
        let before = state.workspace.len();
        assert!(state.open_maybe_encrypted(target));

        assert_eq!(state.workspace.len(), before, "no tab was opened");
    }

    #[test]
    fn a_wrong_passphrase_keeps_the_bar_up_and_opens_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("notes.bpadx");
        let sealed =
            bp_crypto::seal(b"secret", "hunter2", bp_crypto::SealOptions::default()).unwrap();
        std::fs::write(&target, sealed).unwrap();

        let mut state = AppState::new();
        state.open_maybe_encrypted(target);
        let before = state.workspace.len();

        assert!(answer(&mut state, "wrong"), "the bar stays up for a retry");
        assert_eq!(state.workspace.len(), before);
        assert!(
            !state.passphrase_status.is_empty(),
            "a wrong passphrase has to say something, or it looks like the \
             file simply refused to open"
        );
    }

    #[test]
    fn saving_an_encrypted_document_keeps_it_encrypted() {
        // The failure this prevents is the worst one available: a later
        // Ctrl+S quietly writing the document in clear.
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("notes.bpadx");
        let mut state = AppState::new();
        state.edit("first".to_owned());
        let id = state.workspace.active_id().unwrap();
        state.ask = Some(crate::passphrase::Ask::Set {
            id,
            target: target.clone(),
        });
        answer(&mut state, "hunter2");
        answer(&mut state, "hunter2");

        state.edit("first and second".to_owned());
        assert_eq!(state.save_document(id, None), SaveResult::Saved);

        let raw = std::fs::read(&target).unwrap();
        assert!(bp_crypto::is_bpadx(&raw), "the save wrote plaintext");
        let opened = bp_crypto::open(&raw, "hunter2").unwrap();
        assert_eq!(
            String::from_utf8_lossy(&opened),
            "first and second",
            "the edit did not reach the encrypted file"
        );
    }

    #[test]
    fn a_plain_document_is_opened_without_asking_for_anything() {
        let dir = tempfile::tempdir().unwrap();
        let plain = dir.path().join("notes.txt");
        std::fs::write(&plain, "ordinary text").unwrap();

        let mut state = AppState::new();
        assert!(
            !state.open_maybe_encrypted(plain),
            "a plain file must not prompt"
        );
        assert_eq!(state.active_text(), "ordinary text");
    }

    #[test]
    fn closing_an_encrypted_document_forgets_its_passphrase() {
        // Holding it for the session is a deliberate trade; holding it past
        // the document's life is just a leak.
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("notes.bpadx");
        let mut state = AppState::new();
        state.edit("secret".to_owned());
        let id = state.workspace.active_id().unwrap();
        state.ask = Some(crate::passphrase::Ask::Set {
            id,
            target: target.clone(),
        });
        answer(&mut state, "hunter2");
        answer(&mut state, "hunter2");
        assert!(state.is_encrypted(id));

        state.close(id);

        assert!(!state.is_encrypted(id), "the passphrase outlived the tab");
    }
}
