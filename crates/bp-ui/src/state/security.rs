//! The security operations, and everything only they need.
//!
//! Seven capabilities live here -- scan for secrets, redact, inspect
//! metadata, hash, sign, verify, and the security history they all write
//! into -- together with the profile and Privacy Mode switches that decide
//! what each of them is permitted to do.
//!
//! ## Why these are one module and not seven
//!
//! They share a rule rather than a subject: **nothing here may put a secret
//! somewhere the user did not ask for it to be.** `bp-secrets` refuses to
//! carry the matched text, `bp-redaction` reports positions and rule names,
//! the status-bar summaries and the fuller reports are written from those and
//! never from the document, and the security history records that an
//! operation happened rather than what it found. That rule is easy to hold
//! while the code that obeys it is in one place and easy to break one
//! function at a time when it is not, which is why `secret_scan_summary`,
//! `metadata_report` and `hash_report` are here beside the methods that call
//! them rather than among the shell's general helpers.
//!
//! The second thing they share is [`AppState::record_security_event`]. Every
//! capability in this module ends by calling it, and it is the one function
//! that reads [`AppState::policy`] -- so the profile, Privacy Mode and the
//! history are the same subject as the operations, not a separate one.
//!
//! `bp-audit` owns what a history *is*; `crate::audit` owns where the file
//! goes. This module owns only the decision to write a line and the words in
//! it.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use bp_core::{Document, DocumentId};

use super::{AppState, checkpoint_encoding_of, checkpoint_line_ending_of, doc_path, now};

/// Whether a signing key had to be made on the way to a signature.
///
/// A two-variant enum rather than a `bool`, because the call sites read
/// `Created::Yes` instead of `true` -- and the message it decides is the one
/// place a user learns that a key now exists on this machine, which is not a
/// thing to communicate by a positional boolean.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Created {
    Yes,
    No,
}

impl AppState {
    /// Scan the active document for credentials, reporting into the status
    /// bar and handing the findings back for a fuller listing.
    ///
    /// The findings are positions and classifications; `bp-secrets` refuses
    /// to carry the matched text and this returns nothing that would
    /// reintroduce it.
    pub(crate) fn scan_for_secrets(&mut self) -> Vec<bp_secrets::Finding> {
        let findings = bp_secrets::scan(&self.active_text());
        self.error = Some(secret_scan_summary(&findings));
        // The count only. Where the findings are is in the document the user
        // is looking at; putting the offsets in a file beside it would make
        // the history a map to the credentials -- which is the one thing
        // `bp-secrets` is built never to hold.
        self.record_security_event(bp_audit::Event::SecretScanFinished {
            findings: u32::try_from(findings.len()).unwrap_or(u32::MAX),
        });
        findings
    }

    /// Plan a redaction of whatever a scan of the active document finds.
    ///
    /// Scans, says what it found, and hands back a plan for the caller to get
    /// consent for -- redaction destroys text, so the shell asks before it
    /// happens rather than offering undo afterwards as the whole answer.
    ///
    /// `None` means there is nothing this can act on, and the status bar
    /// already says which of the reasons it is.
    pub(crate) fn plan_redaction(&mut self) -> Option<RedactionPlan> {
        let text = self.active_text();
        let findings = bp_secrets::scan(&text);
        let plan = RedactionPlan::from_scan(text, &findings);
        if plan.spans.is_empty() {
            self.error = Some(plan.refusal());
            return None;
        }
        self.error = Some(secret_scan_summary(&findings));
        Some(plan)
    }

    /// Say that the user declined a redaction they were shown.
    ///
    /// Not silence. Cancelling a file dialog leaves the status bar alone
    /// because nothing was claimed; here the bar is showing a scan summary
    /// and the user has just been asked a question, so the answer belongs on
    /// screen -- otherwise "I clicked Cancel" and "it did nothing" look the
    /// same.
    pub(crate) fn decline_redaction(&mut self, plan: &RedactionPlan) {
        self.error = Some(format!(
            "nothing was redacted — {} possible credential{} {} still in this document",
            plan.spans.len(),
            if plan.spans.len() == 1 { "" } else { "s" },
            if plan.spans.len() == 1 { "is" } else { "are" },
        ));
    }

    /// Carry out a redaction the user has agreed to, and check it.
    ///
    /// An ordinary edit, like a data operation: undoable, and nothing reaches
    /// disk until the user saves. That is the only honest shape for this.
    /// Redaction is irreversible *in the string it returns*, but the document
    /// is not the file, and quietly rewriting the file instead would destroy
    /// the user's only copy of text a scanner guessed about.
    ///
    /// Returns whether the document changed, so the caller knows whether to
    /// re-push the text into the widget.
    pub(crate) fn apply_redaction(&mut self, plan: &RedactionPlan) -> bool {
        // The offsets were measured when the dialog opened, and `rfd` pumps
        // events while it is up. ADR-0028 is built on the rule that stale
        // offsets destroy the wrong text and report success, and its own
        // `PastEnd` catches only the half of that which runs off the end --
        // an edit that left the length alone would slip straight through. So
        // the document is compared rather than trusted.
        if self.active_text() != plan.original {
            self.error = Some(
                concat!(
                    "nothing was redacted — the document changed while the ",
                    "dialog was open, so these positions no longer describe it; scan again"
                )
                .to_owned(),
            );
            return false;
        }

        let redacted = match bp_redaction::redact(&plan.original, &plan.spans, REDACTION_MODE) {
            Ok(redacted) => redacted,
            Err(e) => {
                self.error = Some(format!("nothing was redacted — {e}"));
                return false;
            }
        };

        // `verify` re-derives what was removed from the original rather than
        // being handed it, so checking costs nobody a variable holding a
        // secret. It answers in indices into `applied`; those become line
        // numbers here and never text, which is the whole reason the type
        // reports indices in the first place.
        let survivors = bp_redaction::verify(&plan.original, &plan.spans, &redacted.text)
            .ok()
            .map(|verification| {
                verification
                    .surviving
                    .iter()
                    .filter_map(|index| redacted.applied.get(*index))
                    .map(|applied| line_at(&plan.original, applied.start))
                    .collect::<Vec<usize>>()
            });

        let written = redacted.applied.len();
        let bytes_removed = plan.original.len().saturating_sub(redacted.text.len());
        self.edit(redacted.text);
        self.error = Some(plan.outcome(written, survivors.as_deref()));
        // How much went, never what. The removed text is the text somebody
        // chose to destroy, which makes it the most sensitive thing this
        // product ever handles -- `Event::RedactionApplied` carries a count
        // and a byte total for that reason and this passes it nothing else.
        self.record_security_event(bp_audit::Event::RedactionApplied {
            spans: u32::try_from(written).unwrap_or(u32::MAX),
            bytes_removed: bytes_removed as u64,
        });
        true
    }

    /// Report what identifying metadata the active document carries.
    ///
    /// Returns the body of the listing to show, or `None` when the status bar
    /// has already said everything there is to say.
    pub(crate) fn inspect_metadata(&mut self) -> Option<String> {
        let container = container_of(self.workspace.active().and_then(Document::path));
        let findings = bp_redaction::inspect(&self.active_text());
        self.error = Some(metadata_summary(&findings, container));
        (!findings.is_empty() || !container.hidden().is_empty())
            .then(|| metadata_report(&findings, container))
    }

    /// Take a digest of the active document.
    ///
    /// Returns the body of the report to show, or `None` when the document
    /// cannot be encoded -- in which case the status bar already says why.
    /// The digest is not put in the status bar: it elides, and half a
    /// checksum compares equal to nothing.
    pub(crate) fn hash_active_document(&mut self) -> Option<String> {
        let bytes = match self.active_bytes() {
            Ok(bytes) => bytes,
            Err(e) => {
                self.error = Some(format!("cannot hash this document — {e}"));
                return None;
            }
        };
        let digest = bp_crypto::hash_document(&bytes, bp_crypto::HashAlgorithm::Sha256);
        self.error = Some(format!(
            "{} taken over {} bytes",
            digest.algorithm().name(),
            bytes.len()
        ));
        Some(hash_report(
            &digest,
            bytes.len(),
            self.active_differs_from_disk(),
        ))
    }

    /// Check the active document against the `.sig` sidecar beside it.
    ///
    /// The sidecar is found by `bp_integrity::sidecar_path` -- `document.ext`
    /// is signed by `document.ext.sig` -- rather than asked for. Two callers
    /// that disagree about that name produce a document one half of the
    /// product believes is unsigned, which is why the convention is a
    /// function in one crate and not a rule left to each caller.
    ///
    /// `expect` chooses between the two questions ADR-0026 keeps apart.
    /// [`Expectation::AnySigner`](bp_integrity::Expectation::AnySigner)
    /// answers "has this changed since the key named in the sidecar signed
    /// it", which anybody who alters a document can pass by re-signing with a
    /// key of their own; naming a key answers "was it *this* signer", and is
    /// the only way `SignedByAnotherKey` can arise at all.
    ///
    /// Returns whether the check passed, so the caller knows whether naming a
    /// key could still change the answer. It can only turn a pass into
    /// `SignedByAnotherKey`; a failure is a failure whatever key is offered,
    /// because `verify_file` tests the signature against the sidecar's own
    /// key before it compares that key with anybody's expectation.
    /// Where this session keeps its signing key.
    pub(crate) fn signing_key_path(&self) -> Option<PathBuf> {
        self.signing_key.clone()
    }

    /// Whether a signing key already exists on this machine.
    ///
    /// Six bytes read, not a file. The answer decides which of two questions
    /// the passphrase bar asks, and they are genuinely different: "unlock
    /// your key" and "choose a passphrase for a new key" have different
    /// consequences for a typo.
    pub(crate) fn has_signing_key(&self) -> bool {
        self.signing_key_path()
            .is_some_and(|path| bp_integrity::is_sealed_key_file(&path))
    }

    /// Begin Security ▸ Sign Document.
    ///
    /// Asks the right question and stops. Everything after this happens in
    /// [`Self::answer_passphrase`], because the passphrase bar is the only
    /// place a passphrase is typed and routing it anywhere else would be a
    /// second place for one to live.
    ///
    /// The refusals come first and are checked in the order that puts the
    /// most useful sentence in front of the user: a document with no path
    /// cannot be signed at all, and a document with unsaved changes would be
    /// signed as it is *on disk*, which is not what the person clicking
    /// means.
    pub(crate) fn begin_signing(&mut self) -> bool {
        self.error = None;
        let Some(id) = self.workspace.active_id() else {
            return false;
        };
        if self.refuse_on_viewer(id, "Signing") {
            return false;
        }
        if self.workspace.active().and_then(Document::path).is_none() {
            self.error = Some(
                "cannot sign — this document has never been saved, and a signature is over \
                 the bytes on disk"
                    .to_owned(),
            );
            return false;
        }
        // `bp_integrity::sign_file` signs **what is on the disk**, because
        // that is what a recipient will check. Signing now would produce a
        // valid signature over the previous version, which is worse than a
        // refusal: it verifies.
        if self.active_differs_from_disk() {
            self.error = Some(
                "cannot sign — this document has unsaved changes, and a signature is over \
                 the bytes on disk; save it first"
                    .to_owned(),
            );
            return false;
        }

        self.passphrase_status.clear();
        self.ask = Some(if self.has_signing_key() {
            crate::passphrase::Ask::UnlockKey { id }
        } else {
            crate::passphrase::Ask::SetKey { id }
        });
        true
    }

    /// Make a signing key, seal it under `passphrase`, and sign with it.
    ///
    /// One step rather than two, because a key created and then not used is a
    /// ceremony the user did not ask for. What they asked for was a signature.
    pub(super) fn create_key_and_sign(&mut self, id: DocumentId, passphrase: &str) -> bool {
        let Some(path) = self.signing_key_path() else {
            self.error = Some(
                "cannot sign — this environment does not say where the user's profile is, \
                 so there is nowhere to keep a key"
                    .to_owned(),
            );
            return false;
        };
        let key = match bp_integrity::SigningKey::generate() {
            Ok(key) => key,
            Err(e) => {
                self.error = Some(format!("cannot sign — {e}"));
                return false;
            }
        };
        if let Some(parent) = path.parent()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            self.error = Some(format!("cannot sign — {e}"));
            return false;
        }
        if let Err(e) = bp_integrity::write_sealed_signing_key(&path, &key, passphrase) {
            self.error = Some(format!("cannot sign — {e}"));
            return false;
        }
        self.sign_with(id, &key, Created::Yes)
    }

    /// Unlock the stored key and sign with it.
    pub(super) fn sign_with_stored_key(&mut self, id: DocumentId, passphrase: &str) -> bool {
        let Some(path) = self.signing_key_path() else {
            return false;
        };
        match bp_integrity::read_sealed_signing_key(&path, passphrase) {
            Ok(key) => self.sign_with(id, &key, Created::No),
            Err(e) => {
                // Deliberately `bp-integrity`'s sentence and not a guess: an
                // authenticated envelope cannot tell a wrong passphrase from
                // a damaged file, so a message that picked one would be wrong
                // half the time.
                self.error = Some(format!("cannot sign — {e}"));
                false
            }
        }
    }

    /// Write the sidecar, record the event, and say what happened.
    fn sign_with(&mut self, id: DocumentId, key: &bp_integrity::SigningKey, made: Created) -> bool {
        let Some(path) = self.workspace.get(id).and_then(Document::path) else {
            return false;
        };
        let path = path.to_path_buf();

        match bp_integrity::sign_file(&path, key) {
            Ok(sidecar) => {
                self.record_security_event(bp_audit::Event::SignatureMade);
                // The public key is on the message because a signature nobody
                // has the key for is a signature nobody can check, and the
                // moment somebody has just made one is the moment they need
                // to send it. It is public by construction -- ADR-0026 puts
                // it in the sidecar in clear for the same reason.
                let fingerprint = key.verifying_key().to_hex();
                self.error = Some(format!(
                    "{}signed — {} · your public key is {}",
                    match made {
                        Created::Yes => "signing key created and ",
                        Created::No => "",
                    },
                    sidecar.display(),
                    fingerprint
                ));
                true
            }
            Err(e) => {
                self.error = Some(format!("cannot sign — {e}"));
                false
            }
        }
    }

    pub(crate) fn verify_signature(&mut self, expect: &bp_integrity::Expectation) -> bool {
        let Some(path) = self
            .workspace
            .active()
            .and_then(Document::path)
            .map(Path::to_path_buf)
        else {
            self.error = Some(
                "cannot verify — this document has never been saved, so there is no file \
                 for a signature to sit beside"
                    .to_owned(),
            );
            return false;
        };

        // A missing *document* is an error and a missing *signature* is a
        // verdict -- and a failing one. `bp-integrity` draws that line, and
        // the shell does not get to soften it: an unsigned document must not
        // come back as anything but a failure, or deleting a file would be a
        // way to pass the check.
        let verification = match bp_integrity::verify_file(&path, expect) {
            Ok(verification) => verification,
            Err(e) => {
                self.error = Some(format!("cannot verify — {e}"));
                return false;
            }
        };

        let verified = verification.is_verified();
        // The flag rather than only the fact of a check: `Event` makes the
        // outcome part of the event for a reason, because a recorded
        // verification that omitted its answer reads as reassurance.
        self.record_security_event(bp_audit::Event::SignatureVerified { valid: verified });
        // `explain` rather than a second set of sentences here. Every verdict
        // words itself in `bp-integrity`, beside the logic that produces it,
        // so the status bar and any other surface say the same thing about
        // the same answer -- and so the five outcomes ADR-0026 exists to keep
        // apart stay five, rather than collapsing into "does not verify" on
        // the way to a screen.
        let mut message = format!(
            "{} {}",
            if verified { "✓" } else { "✗" },
            verification.explain()
        );

        if verified && *expect == bp_integrity::Expectation::AnySigner {
            // The caveat `Expectation::AnySigner` carries in its own doc
            // comment. Without it a tick reads as "this is from who you
            // think", which is a claim nothing here checked.
            message.push_str(
                " — no key was named, so this says only that the document has not changed \
                 since the key above signed it; anyone who alters a document can re-sign it \
                 with a key of their own",
            );
        }
        if self.active_differs_from_disk() {
            // The shell's own contribution, and the only sentence here it is
            // in a position to write: the verdict is about the file, and the
            // buffer on screen is not that file.
            message.push_str(
                " — this document has unsaved changes, so these are not the bytes anybody \
                 signed; the check was made against the file on disk",
            );
        }
        self.error = Some(message);
        verified
    }

    /// Read the public key a signature is claimed to have been made with.
    ///
    /// Reports its own failure, because "could not read the key" and "the
    /// signature does not match" are answers with different fixes and the
    /// second must never be shown for the first.
    pub(crate) fn expected_signer(&mut self, key_path: &Path) -> Option<bp_integrity::Expectation> {
        match read_verifying_key(key_path) {
            Ok(key) => Some(bp_integrity::Expectation::Key(key)),
            Err(e) => {
                self.error = Some(format!("could not read the key — {e}"));
                None
            }
        }
    }

    /// Write `event` to the security history, if the profile permits it.
    ///
    /// Never fails the operation it describes. A security capability that
    /// stopped working because its *log* could not be written would be a
    /// worse product than one whose log has a gap -- so a refusal or an I/O
    /// error becomes a status-bar line and the capability carries on. The
    /// one refusal worth a notice is `NoSealer`, which tells the user how to
    /// fix it (encrypt the document); `ProfileForbidsIt` is the profile
    /// doing its job and says nothing.
    ///
    /// The policy passed is the one *in force* -- `self.policy()` already
    /// resolves Privacy Mode -- because a caller that passes the document's
    /// own profile is a caller Privacy Mode does not reach.
    pub(crate) fn record_security_event(&mut self, event: bp_audit::Event) {
        let policy = self.policy();
        // Opened per event rather than held. `AuditLog::open` reads and
        // checks the existing file, which is what makes its sequence
        // trustworthy; a handle kept across a session would be a sequence
        // that stopped agreeing with the file the moment anything else
        // appended to it. Security events are rare enough to afford it.
        let sealer = self
            .workspace
            .active_id()
            .and_then(|id| self.passphrases.get(&id))
            .and_then(|p| bp_audit::PassphraseSealer::new(p).ok());

        let mut log = match bp_audit::AuditLog::open(
            self.audit_path.clone(),
            sealer.as_ref().map(|s| s as &dyn bp_audit::Sealer),
        ) {
            Ok(log) => log,
            Err(e) => {
                self.note_quietly(format!("security history unavailable -- {e}"));
                return;
            }
        };

        let document = self
            .workspace
            .active_id()
            .map(|id| bp_audit::DocumentId::new(id.get()));

        match log.append(
            now(),
            document,
            event,
            policy,
            sealer.as_ref().map(|s| s as &dyn bp_audit::Sealer),
        ) {
            Ok(bp_audit::Appended::Persisted(_)) => {}
            Ok(bp_audit::Appended::NotWritten(reason)) => {
                if let Some(notice) = reason.notice() {
                    self.note_quietly(notice.to_owned());
                }
            }
            Err(e) => self.note_quietly(format!("security history not written -- {e}")),
        }
    }

    /// The security history, as a report to show.
    ///
    /// Returns `None` when it cannot be read, having put the reason in the
    /// status bar -- a sealed history with no passphrase to hand is the
    /// common case, and it is a real answer rather than a failure.
    pub(crate) fn security_history(&mut self) -> Option<String> {
        let sealer = self
            .workspace
            .active_id()
            .and_then(|id| self.passphrases.get(&id))
            .and_then(|p| bp_audit::PassphraseSealer::new(p).ok());

        match bp_audit::AuditLog::open(
            self.audit_path.clone(),
            sealer.as_ref().map(|s| s as &dyn bp_audit::Sealer),
        ) {
            Ok(log) => {
                self.error = Some(format!("{} security events", log.records().len()));
                Some(crate::audit::history_report(&log))
            }
            Err(e) => {
                self.error = Some(format!("cannot read the security history -- {e}"));
                None
            }
        }
    }

    /// The active document's security policy.
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
    /// obliges us to remove what the looser one already permitted. The
    /// journal is handled by the next checkpoint, which deletes a refused
    /// document's file; the clipboard is shared and has to be dealt with
    /// here.
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

        // Recorded against the *new* profile, which is the one that decides
        // whether it may be kept. Recording a tightening under the old, looser
        // profile would write the line the user just asked to stop writing;
        // recording a loosening under the old, stricter one would lose the
        // single most interesting event a history can hold.
        self.record_security_event(bp_audit::Event::SecurityProfileChanged {
            from: bp_audit::ProfileLabel::of(&previous),
            to: bp_audit::ProfileLabel::of(&security),
        });

        // The journal for *this* document only. A refused checkpoint deletes
        // its file, so asking for one now is what turns the profile change
        // into an actual deletion rather than waiting up to the autosave
        // interval with plaintext still on disk.
        let entry = bp_history::Checkpoint {
            path: doc_path(&self.workspace, id),
            name: self.display_name(id),
            text: self.text_of(id).to_owned(),
            written_at: bp_history::now_unix(),
            encoding: checkpoint_encoding_of(encoding),
            line_ending: Some(checkpoint_line_ending_of(line_ending)),
        };
        let passphrase = self.passphrases.get(&id).map(|p| p.to_string());
        if let Ok(bp_history::Written::Refused(refusal)) =
            self.journal
                .checkpoint(id.get(), &entry, policy.recovery, passphrase.as_deref())
            && let Some(message) = refusal.notice()
        {
            self.error = Some(message.to_owned());
        }
    }

    /// Turn Privacy Mode on or off, and make the world match.
    ///
    /// Switching it on has to *act*, not merely be recorded: a mode that only
    /// governed future writes would leave the clipboard history and the
    /// journals gathered a moment ago exactly where they were, which is the
    /// opposite of what somebody switching it on wants.
    pub(crate) fn set_privacy(&mut self, privacy: bp_security::Privacy) {
        if self.privacy == privacy {
            return;
        }
        self.privacy = privacy;
        if !privacy.is_on() {
            // Turning it off restores each document's own profile. Nothing to
            // clean up -- the clamp only ever removed permissions.
            //
            // Recorded after the field changes, so the policy deciding whether
            // this line may be kept is the one now in force. Leaving Privacy
            // Mode is the event that re-enables everything it was switched on
            // to stop, which is exactly why it is worth a line.
            self.record_security_event(bp_audit::Event::PrivacyModeLeft);
            return;
        }
        self.record_security_event(bp_audit::Event::PrivacyModeEntered);

        // Every document, not only the active one: the mode is session-wide,
        // and a journal left behind for a background tab is exactly what it
        // was switched on to prevent.
        self.checkpoint_all();
        self.error = Some("Privacy Mode on -- journals removed".to_owned());
    }
}

/// How redaction replaces what it destroys.
///
/// [`bp_redaction::Replacement::Placeholder`], and the two it was chosen over
/// matter more than the one it is.
///
/// `Mask` with `MaskWidth::MatchOriginal` is out on its own terms: it draws
/// one character per character removed, which publishes the *length* of what
/// was there. For a name that is nearly nothing; for a PIN, a short token, or
/// an answer from a fixed set of options it is most of the secret, and a
/// redaction that discloses the secret is the failure ADR-0028 exists to
/// prevent.
///
/// `Remove` closes the gap, so the document reads as though the credential
/// had never been typed. That is right when the withholding itself should be
/// invisible. It is wrong here: the user is about to be told to save, to
/// rotate the key and to deal with every other copy, and a change they cannot
/// see on screen is one they will not act on.
///
/// `Placeholder` leaves `[REDACTED: AWS access key ID]`. The label is
/// `SecretKind::label` -- the name of the rule that matched, never what it
/// matched -- so the marker says what kind of thing was taken out without
/// putting it back. The cost is real, and is why this is a decision rather
/// than a default: the marker tells whoever reads the document afterwards
/// that an AWS key was there. That is a disclosure an author cleaning up
/// their own note can live with, and it is what makes a redaction findable
/// again later, which `bp_redaction::PLACEHOLDER` is public for.
const REDACTION_MODE: bp_redaction::Replacement = bp_redaction::Replacement::Placeholder;

/// A redaction the user has been shown and has not yet agreed to.
///
/// Holds the text the spans were measured against, so that a buffer edited
/// while the confirmation dialog was up is caught rather than redacted
/// against offsets that no longer describe it.
///
/// Carries no secret: `spans` are positions, `listing` is line numbers and
/// rule names, and `original` is the document the user is already looking at.
pub(crate) struct RedactionPlan {
    original: String,
    spans: Vec<bp_redaction::Span<'static>>,
    /// Line and kind per span, for the confirmation dialog. Never the text.
    listing: Vec<(usize, &'static str)>,
    /// Private-key blocks deliberately left alone. See [`RedactionPlan::from_scan`].
    key_blocks: usize,
    /// Findings whose position did not resolve to a range of this document.
    unlocatable: usize,
    /// Everything the scan reported, including what is not being redacted.
    total: usize,
}

impl RedactionPlan {
    /// Turn a scan of `original` into byte spans `bp_redaction` can act on.
    ///
    /// The conversion is the whole job, and it is not a formality.
    /// `bp_secrets::Finding` reports a 1-based line, a **character** column
    /// and a length in **characters**, because those are what a caret is
    /// moved to. `bp_redaction::Span` is in bytes, because those are what a
    /// `&str` can be sliced at. On any line holding a multi-byte character
    /// the two disagree, and ADR-0028 is explicit that a span in the wrong
    /// unit destroys the wrong text and reports success. A finding that does
    /// not resolve is counted and dropped rather than clamped, for the same
    /// reason `bp_redaction` refuses rather than clamping.
    ///
    /// A `PrivateKeyBlock` is deliberately left out. `bp-secrets` documents
    /// that its finding covers the `-----BEGIN ... PRIVATE KEY-----` marker
    /// only -- one line, column and length cannot describe a block that runs
    /// on to a matching `END` -- so redacting that span would take out the
    /// label and leave the key material in the document under a `[REDACTED]`
    /// marker. That is exactly the black-rectangle failure ADR-0028 is
    /// written against, so it is refused and named rather than half done.
    fn from_scan(original: String, findings: &[bp_secrets::Finding]) -> Self {
        let lines = line_spans(&original);
        let mut spans = Vec::new();
        let mut listing = Vec::new();
        let mut key_blocks = 0usize;
        let mut unlocatable = 0usize;

        for finding in findings {
            if finding.kind == bp_secrets::SecretKind::PrivateKeyBlock {
                key_blocks += 1;
                continue;
            }
            let Some((offset, line)) = finding
                .line
                .checked_sub(1)
                .and_then(|index| lines.get(index))
                .copied()
            else {
                unlocatable += 1;
                continue;
            };
            let first = finding.column.saturating_sub(1);
            let start = byte_of_char(line, first);
            let end = byte_of_char(line, first.saturating_add(finding.length));
            if start >= end {
                unlocatable += 1;
                continue;
            }
            spans.push(bp_redaction::Span::labelled(
                offset + start,
                offset + end,
                finding.kind.label(),
            ));
            listing.push((finding.line, finding.kind.label()));
        }

        Self {
            original,
            spans,
            listing,
            key_blocks,
            unlocatable,
            total: findings.len(),
        }
    }

    /// Why there is nothing to redact, when there is nothing to redact.
    ///
    /// Three different sentences, because they ask three different things of
    /// the user. "No credentials found" is an all-clear; the other two are
    /// not, and reporting them as one would be the quiet lie.
    fn refusal(&self) -> String {
        if self.key_blocks > 0 {
            return format!(
                concat!(
                    "nothing was redacted — the {} private key block{} found {} marked only by ",
                    "the BEGIN line, and replacing that would leave the key body in the ",
                    "document; remove {} by hand",
                ),
                self.key_blocks,
                if self.key_blocks == 1 { "" } else { "s" },
                if self.key_blocks == 1 { "is" } else { "are" },
                if self.key_blocks == 1 { "it" } else { "them" },
            );
        }
        if self.unlocatable > 0 {
            return format!(
                concat!(
                    "nothing was redacted — {} of the {} finding{} could not be located in ",
                    "this text; scan again",
                ),
                self.unlocatable,
                self.total,
                if self.total == 1 { "" } else { "s" },
            );
        }
        "nothing to redact — no credentials found".to_owned()
    }

    /// What the user is agreeing to, in full, before anything is destroyed.
    ///
    /// Line numbers and rule names only. The value is never printed here for
    /// the same reason the scan's own listing never prints it: a dialog is a
    /// thing people screenshot into bug reports.
    pub(crate) fn consent_body(&self) -> String {
        let mut body = format!(
            "{} credential{} will be replaced with {}: kind] markers:\n\n",
            self.listing.len(),
            if self.listing.len() == 1 { "" } else { "s" },
            // Built from the constant rather than written out, so the shape
            // the dialog promises and the shape a later search for previous
            // redactions looks for cannot drift apart.
            bp_redaction::PLACEHOLDER.trim_end_matches(']'),
        );
        for (line, kind) in &self.listing {
            let _ = writeln!(body, "    line {line} — {kind}");
        }
        body.push_str(concat!(
            "\nPositions and kinds only. The marker names the rule that matched, ",
            "never what it matched.\n",
        ));
        if self.key_blocks > 0 {
            let _ = write!(
                body,
                concat!(
                    "\n{} private key block{} will be left alone: the scan marks only the ",
                    "-----BEGIN ... PRIVATE KEY----- line, and replacing that would take out ",
                    "the label and leave the key body in the document. Remove {} by hand.\n",
                ),
                self.key_blocks,
                if self.key_blocks == 1 { "" } else { "s" },
                if self.key_blocks == 1 { "it" } else { "them" },
            );
        }
        if self.unlocatable > 0 {
            let _ = write!(
                body,
                concat!(
                    "\n{} finding{} could not be located in this text and will be left alone. ",
                    "That should not happen; scan again before trusting this.\n",
                ),
                self.unlocatable,
                if self.unlocatable == 1 { "" } else { "s" },
            );
        }
        body.push_str(concat!(
            "\nThis changes the document in front of you, not the file on disk, and undo ",
            "restores what was removed. The file you saved earlier, its recovery journal, ",
            "the undo history, and anything already copied to the clipboard or sent still ",
            "hold the originals. Redacting here deals with one of the places this text ",
            "lives.\n",
        ));
        body
    }

    /// One line for the status bar once the redaction has been carried out.
    ///
    /// `survivors` are the lines of redactions whose text still occurs
    /// somewhere in the result, or `None` when the check itself could not be
    /// run. ADR-0028 is careful that a survivor is a reason to look and not a
    /// verdict -- the same value appearing somewhere the scan did not mark is
    /// the usual cause -- so the wording says look, rather than failed.
    fn outcome(&self, written: usize, survivors: Option<&[usize]>) -> String {
        let mut line = match survivors {
            Some([]) => format!(
                concat!(
                    "✓ {} redaction{} written — undo brings the originals back, and the copy ",
                    "on disk still holds them until you save",
                ),
                written,
                if written == 1 { "" } else { "s" },
            ),
            Some(lines) => format!(
                concat!(
                    "⚠ {} redaction{} written, but {} still occurs elsewhere in this ",
                    "document (line{} {}) — the scan did not mark that copy; look at it and ",
                    "redact it by hand",
                ),
                written,
                if written == 1 { "" } else { "s" },
                lines.len(),
                if lines.len() == 1 { "" } else { "s" },
                lines
                    .iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
            None => format!(
                concat!(
                    "⚠ {} redaction{} written, but the check that nothing survived could not ",
                    "be run",
                ),
                written,
                if written == 1 { "" } else { "s" },
            ),
        };
        if self.key_blocks > 0 {
            let _ = write!(
                line,
                "; {} private key block{} left alone, marked only by the BEGIN line",
                self.key_blocks,
                if self.key_blocks == 1 { "" } else { "s" },
            );
        }
        line
    }
}

/// Byte offset and text of every line, split exactly as `str::lines` splits.
///
/// `bp_secrets` numbers its findings by enumerating `text.lines()`, so this
/// has to agree with it line for line -- including that `lines` drops the
/// carriage return of a CRLF ending, which shifts every byte offset after it
/// if it is not put back.
fn line_spans(text: &str) -> Vec<(usize, &str)> {
    let mut spans = Vec::new();
    let mut offset = 0usize;
    for line in text.lines() {
        spans.push((offset, line));
        offset += line.len();
        if text[offset..].starts_with('\r') {
            offset += 1;
        }
        if text[offset..].starts_with('\n') {
            offset += 1;
        }
    }
    spans
}

/// The byte offset of character `index`, or the end of the line past it.
///
/// The end rather than `None`, so a column that runs off the line collapses
/// to an empty span, which the caller counts as unlocatable rather than
/// redacting something it guessed at.
fn byte_of_char(line: &str, index: usize) -> usize {
    line.char_indices()
        .nth(index)
        .map_or(line.len(), |(offset, _)| offset)
}

/// The 1-based line a byte offset falls on.
fn line_at(text: &str, offset: usize) -> usize {
    text[..offset.min(text.len())]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1
}

/// Which shape a document arrived in, from its extension.
///
/// By extension rather than through `bp_formats::detect`, because every
/// format `bp-formats` knows is one whose bytes *are* its content -- there is
/// no `Format::Docx` to ask about. The question here is the opposite one: is
/// this a container whose metadata reading the text cannot reach?
///
/// An unknown extension, and a document that has never been saved, are
/// [`bp_redaction::Container::PlainText`]. That claims nothing about hidden
/// metadata and still carries the sentence about the filesystem entry around
/// the file, which is the honest answer where there is no evidence either way.
fn container_of(path: Option<&Path>) -> bp_redaction::Container {
    use bp_redaction::Container;

    let extension = path
        .and_then(Path::extension)
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "docx" | "docm" | "xlsx" | "xlsm" | "pptx" | "pptm" => Container::OfficeOpenXml,
        "odt" | "ods" | "odp" | "odg" | "odf" => Container::OpenDocument,
        "pdf" => Container::Pdf,
        "rtf" => Container::RichText,
        "png" | "jpg" | "jpeg" | "gif" | "tif" | "tiff" | "webp" | "heic" | "heif" | "avif" => {
            Container::Image
        }
        _ => Container::PlainText,
    }
}

/// How many metadata findings the status bar names, for the same reason
/// [`SECRETS_IN_SUMMARY`] exists.
const METADATA_IN_SUMMARY: usize = 3;

/// One line for the status bar describing a metadata inspection.
///
/// Kinds and positions, never the value -- `MetadataFinding` deliberately
/// carries neither, and a status bar that reached back into the document to
/// quote an email address would undo that.
///
/// The container half is not garnish. For a format this build cannot see
/// inside, "nothing found" reads to a user as an all-clear and would be a
/// lie, so what was *not* looked at is said in the same sentence as what was.
pub(crate) fn metadata_summary(
    findings: &[bp_redaction::MetadataFinding],
    container: bp_redaction::Container,
) -> String {
    let hidden = container.hidden();
    let found = if findings.is_empty() {
        "nothing identifying in the text".to_owned()
    } else {
        let named: Vec<String> = findings
            .iter()
            .take(METADATA_IN_SUMMARY)
            .map(|f| format!("{} at line {}", f.kind.label(), f.line))
            .collect();
        let rest = findings.len() - named.len();
        format!(
            "{} identifying item{} — {}{}",
            findings.len(),
            if findings.len() == 1 { "" } else { "s" },
            named.join(", "),
            if rest == 0 {
                String::new()
            } else {
                format!(", and {rest} more")
            }
        )
    };

    if hidden.is_empty() {
        if findings.is_empty() {
            format!(
                concat!(
                    "✓ {} — the file's own timestamps, ownership and extended attributes are ",
                    "outside this check",
                ),
                found,
            )
        } else {
            format!("⚠ {found}")
        }
    } else {
        // "but" rather than "and" when the text came back clean, because
        // that clause is the one contradicting the clause before it: a clean
        // text scan is exactly what a user would otherwise read as an
        // all-clear for the whole file.
        format!(
            "⚠ {found}{} this {} file also carries {} where reading its text cannot look",
            if findings.is_empty() {
                ", but"
            } else {
                ", and"
            },
            container.label(),
            join_and(&hidden.iter().map(|kind| kind.label()).collect::<Vec<_>>()),
        )
    }
}

/// The full listing of a metadata inspection, one finding per line.
///
/// Ends with what was *not* looked at, in every case. For plain text that is
/// the filesystem entry around the file, which `bp-redaction` says explicitly
/// is the shell's and not its own; for anything else it is the container's
/// own metadata and what a build would need to read it.
pub(crate) fn metadata_report(
    findings: &[bp_redaction::MetadataFinding],
    container: bp_redaction::Container,
) -> String {
    let mut body = if findings.is_empty() {
        "Nothing identifying was found in the text.\n".to_owned()
    } else {
        let mut listing = format!(
            "{} item{} of identifying metadata in the text:\n\n",
            findings.len(),
            if findings.len() == 1 { "" } else { "s" },
        );
        for finding in findings {
            let _ = writeln!(
                listing,
                "    line {} — {} ({})",
                finding.line,
                finding.kind.label(),
                exposure_label(finding.exposure),
            );
        }
        listing.push_str("\nPositions and kinds only. The value itself is never printed here.\n");
        listing
    };

    match container.requires() {
        None => body.push_str(concat!(
            "\nThe bytes of a plain text file are the whole document, so this looked at all ",
            "of it. What it cannot see is the filesystem entry around the file: modification ",
            "and creation times, ownership, extended attributes and, on Windows, alternate ",
            "data streams. Those identify a document too, and they travel with a copy.\n",
        )),
        // No article in front of the label: a sentence that had to choose
        // between "a PDF" and "an Office Open XML" would need to know which,
        // and `Container` grows.
        Some(needs) => {
            let _ = write!(
                body,
                concat!(
                    "\nThis {} file also carries {} that reading its text cannot reach; ",
                    "seeing those would need {}. Nothing above is an all-clear for ",
                    "this file.\n",
                ),
                container.label(),
                join_and(
                    &container
                        .hidden()
                        .iter()
                        .map(|kind| kind.label())
                        .collect::<Vec<_>>()
                ),
                needs,
            );
        }
    }
    body
}

/// How far a finding goes towards identifying somebody, in the user's terms.
///
/// Presentation, so it lives here rather than in `bp-redaction`, on the same
/// argument as [`confidence_label`]: the crate is pure and has no opinion
/// about wording, and a bare "circumstantial" beside a real name reads as
/// "ignore me".
const fn exposure_label(exposure: bp_redaction::Exposure) -> &'static str {
    match exposure {
        bp_redaction::Exposure::Attributable => "names a person",
        bp_redaction::Exposure::Identifying => "names a machine, account or organisation",
        bp_redaction::Exposure::Circumstantial => "narrows the field",
    }
}

/// `"a, b and c"`. A sentence rather than a list, because these are read once
/// rather than scanned.
fn join_and(items: &[&str]) -> String {
    match items {
        [] => String::new(),
        [only] => (*only).to_owned(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// How many findings the status bar names before it stops counting them out.
///
/// The bar elides, so a list longer than this is a list whose tail nobody
/// reads. The full set goes in the report instead.
const SECRETS_IN_SUMMARY: usize = 3;

/// One line for the status bar describing a secret scan.
///
/// Names the kinds and where they are, and never the matched text. That is
/// not a nicety: `bp_secrets::Finding` deliberately holds no secret at all,
/// so that no `Debug` print or log line can leak one, and a status bar that
/// quoted the credential would put it back on screen -- and into the
/// screenshot of the bug it was attached to.
pub(crate) fn secret_scan_summary(findings: &[bp_secrets::Finding]) -> String {
    if findings.is_empty() {
        return "✓ no credentials found".to_owned();
    }
    let named: Vec<String> = findings
        .iter()
        .take(SECRETS_IN_SUMMARY)
        .map(|f| format!("{} at line {} col {}", f.kind.label(), f.line, f.column))
        .collect();
    let rest = findings.len() - named.len();
    format!(
        "⚠ {} possible credential{} — {}{}",
        findings.len(),
        if findings.len() == 1 { "" } else { "s" },
        named.join(", "),
        if rest == 0 {
            String::new()
        } else {
            format!(", and {rest} more")
        }
    )
}

/// The full listing of a scan, one finding per line.
///
/// Line and column first, because the reason to read this is to go and look
/// at each one, and `bp-secrets` reports 1-based positions for exactly that.
pub(crate) fn secret_scan_report(findings: &[bp_secrets::Finding]) -> String {
    findings
        .iter()
        .map(|f| {
            format!(
                "line {}, column {} — {} ({})",
                f.line,
                f.column,
                f.kind.label(),
                confidence_label(f.confidence)
            )
        })
        .collect::<Vec<_>>()
        .join(
            "
",
        )
}

/// How sure the scanner is, in the user's terms.
///
/// Presentation, so it lives here rather than in `bp-secrets`, which is pure
/// and has no opinion about wording. Low is spelled out rather than left as a
/// bare word: a row reading "low" beside a real password reads as "ignore
/// me", and the whole point of the tier is that the user decides.
const fn confidence_label(confidence: bp_secrets::Confidence) -> &'static str {
    match confidence {
        bp_secrets::Confidence::High => "high confidence",
        bp_secrets::Confidence::Medium => "medium confidence",
        bp_secrets::Confidence::Low => "low confidence, worth a look",
    }
}

/// The report shown for a document digest.
///
/// Both spellings, because they are read by different people in different
/// ways: the grouped one is for a human comparing two screens or reading it
/// down a telephone, the unbroken one is for pasting beside what `sha256sum`
/// or `certutil` printed. `bp_crypto` offers both and deliberately refuses to
/// choose between them.
fn hash_report(digest: &bp_crypto::DocumentHash, bytes: usize, unsaved: bool) -> String {
    let mut report = format!(
        "{}

{}

Taken over the {bytes} bytes this document would be written as, so it \
        matches the file once it is saved.",
        digest.to_display(),
        digest.to_hex(),
    );
    if unsaved {
        // The one way this digest can be honestly wrong about the file, and
        // it is invisible from the dialog otherwise.
        report.push_str(
            "

This document has unsaved changes, so it is not yet the digest of \
            anything on disk.",
        );
    }
    report
}

/// Read a verifying key from the file the user chose, in either spelling.
///
/// Thirty-two raw bytes is what `VerifyingKey::to_bytes` writes; hexadecimal
/// is what arrives when somebody pastes a key out of an email into a text
/// file, which is how a public key actually travels between people. The raw
/// form is tried first because a 32-byte file cannot also be 64 hex digits,
/// so the two can never be confused for one another.
fn read_verifying_key(path: &Path) -> Result<bp_crypto::VerifyingKey, String> {
    let raw = std::fs::read(path).map_err(|e| e.to_string())?;
    if let Ok(key) = bp_crypto::VerifyingKey::from_bytes(&raw) {
        return Ok(key);
    }
    let text = std::str::from_utf8(&raw)
        .map_err(|_| "this is neither 32 raw bytes nor a key in hexadecimal".to_owned())?;
    bp_crypto::VerifyingKey::from_hex(text).map_err(|e| e.to_string())
}

/// Where this machine's signing key goes.
///
/// **`DirKind::Data`, and not `State`** (ADR-0031, amended when it came to be
/// built). `bp-platform` calls `Data` "things the product made and would
/// rather not remake", which is exactly what a signing key is: everything
/// already signed with it verifies forever, and nothing new can ever join
/// those documents once it is gone. `State` is defined by the opposite —
/// window geometry, the recent list, logs, things whose loss is noticeable
/// and survivable.
///
/// The ADR said `State` in passing, beside the work that moved three other
/// files there, and that was the sentence carrying the habit rather than the
/// reasoning. The decision it records — that the key is protected by an
/// envelope rather than by file permissions — is what makes this a question
/// about *durability* rather than about secrecy, and durability answers
/// `Data`.
///
/// `None` when the environment does not say where the user's profile is.
/// `bp-platform` refuses to guess one, and the signing path says so rather
/// than putting a key beside whatever file the user happened to open.
#[cfg(not(test))]
pub(super) fn default_signing_key_path() -> Option<PathBuf> {
    Some(
        bp_platform::dirs::host_directory(bp_platform::DirKind::Data)?
            .join(format!("signing.{}", bp_integrity::SEALED_KEY_EXTENSION)),
    )
}

/// The same, redirected and made unique under test.
///
/// Third time this crate has needed it. Without it, any test that reaches the
/// signing flow writes **a real Ed25519 private key** into the developer's own
/// `%LOCALAPPDATA%` — worse than the live security history that prompted the
/// first of these, because a key is the one file in this product that is
/// supposed to be secret.
///
/// Unique per `AppState` rather than per call, which is why it is a field:
/// `has_signing_key` decides which question the passphrase bar asks and the
/// signing that follows has to open the file that answer was about. A
/// `cfg(test)` redirect on the *function* — the shape the audit path uses —
/// would have them disagree.
#[cfg(test)]
pub(super) fn default_signing_key_path() -> Option<PathBuf> {
    // Uniqueness belongs to `crate::testpaths`, which has the story: this
    // used to build its own pid-and-counter name, and a recycled pid meant
    // `begin_signing` could find a key sealed under a passphrase the test
    // did not know.
    Some(crate::testpaths::unique(
        "signing",
        bp_integrity::SEALED_KEY_EXTENSION,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::SaveResult;
    use bp_core::Encoding;

    // --- signing (ADR-0031) -----------------------------------------------

    /// A state with one saved, clean document, and the path it was saved to.
    ///
    /// The signing key lives in a unique temp file per state, so none of
    /// these touches the developer's own profile -- see
    /// `default_signing_key_path`.
    fn a_saved_document() -> (tempfile::TempDir, AppState, PathBuf) {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("report.txt");
        std::fs::write(&path, "a document worth signing\n").expect("write");

        let mut state = AppState::new();
        state.open(path.clone());
        assert!(
            !state.active_differs_from_disk(),
            "the fixture is not clean"
        );
        (dir, state, path)
    }

    #[test]
    fn a_test_signing_key_never_lands_in_the_users_own_profile() {
        // The protection itself, pinned, and the third time this crate has
        // needed one. A real Ed25519 private key written into `%LOCALAPPDATA%`
        // by `cargo test` is worse than the live security history that
        // prompted the first: a key is the one file here that is supposed to
        // be secret.
        let state = AppState::new();
        let path = state.signing_key_path().expect("a test path is always set");
        assert!(
            path.starts_with(std::env::temp_dir()),
            "the test key must live in the temp directory, not {}",
            path.display()
        );
        assert_ne!(
            path,
            AppState::new()
                .signing_key_path()
                .expect("a test path is always set"),
            "two states in one process must not share a key file"
        );
    }

    #[test]
    fn signing_creates_a_key_the_first_time_and_reuses_it_after() {
        // One ceremony, not one per signature. The first signature makes a
        // key because the user asked to sign, not because they asked for a
        // key; the second finds it and asks only to unlock it.
        let (dir, mut state, path) = a_saved_document();

        assert!(state.begin_signing());
        assert_eq!(
            state.ask,
            Some(crate::passphrase::Ask::SetKey {
                id: state.workspace.active_id().expect("a tab")
            }),
            "with no key yet, the bar must ask for a new one"
        );
        state.answer_passphrase("correct horse battery staple");
        assert!(state.answer_passphrase("correct horse battery staple"));

        let key_path = state.signing_key_path().expect("a key path");
        assert!(key_path.is_file(), "no key was written");
        assert!(state.has_signing_key());

        let sidecar = bp_integrity::sidecar_path(&path).expect("a sidecar path");
        assert!(sidecar.is_file(), "no signature was written");

        // And again. One entry this time, because the key exists.
        let _ = dir;
        assert!(state.begin_signing());
        assert_eq!(
            state.ask,
            Some(crate::passphrase::Ask::UnlockKey {
                id: state.workspace.active_id().expect("a tab")
            }),
            "with a key present, the bar must ask to unlock it"
        );
        assert!(state.answer_passphrase("correct horse battery staple"));
    }

    #[test]
    fn a_signature_this_product_makes_is_one_it_verifies() {
        // The seam that matters, and it crosses three crates: the key store
        // seals and unseals, `sign_file` writes the sidecar, and the row a
        // user clicks to check it reads that same sidecar back. Each half is
        // tested alone; this is the only place they meet.
        let (_dir, mut state, _path) = a_saved_document();

        assert!(state.begin_signing());
        state.answer_passphrase("hunter2");
        assert!(state.answer_passphrase("hunter2"));

        assert!(
            state.verify_signature(&bp_integrity::Expectation::AnySigner),
            "the product refused its own signature: {:?}",
            state.error
        );
    }

    #[test]
    fn the_public_key_is_on_the_message_because_nobody_can_check_without_it() {
        // A signature nobody has the key for is a signature nobody can check,
        // and the moment somebody has just made one is the moment they need
        // to send it. It is public by construction -- ADR-0026 puts it in the
        // sidecar in clear for the same reason.
        let (_dir, mut state, _path) = a_saved_document();

        assert!(state.begin_signing());
        state.answer_passphrase("hunter2");
        assert!(state.answer_passphrase("hunter2"));

        let message = state.error.clone().unwrap_or_default();
        assert!(
            message.contains("public key"),
            "the message must carry the key: {message}"
        );
        assert!(
            message.contains("signing key created"),
            "the user has to be told a key now exists on this machine: {message}"
        );
    }

    #[test]
    fn the_wrong_key_passphrase_refuses_without_saying_which_thing_was_wrong() {
        // `bp-crypto`'s decision, carried through unchanged: an authenticated
        // envelope cannot distinguish a wrong passphrase from a damaged file,
        // so a message that picked one would be wrong half the time.
        let (_dir, mut state, path) = a_saved_document();

        assert!(state.begin_signing());
        state.answer_passphrase("hunter2");
        assert!(state.answer_passphrase("hunter2"));
        let sidecar = bp_integrity::sidecar_path(&path).expect("a sidecar path");
        std::fs::remove_file(&sidecar).expect("clear the first signature");

        assert!(state.begin_signing());
        assert!(!state.answer_passphrase("hunter3"));
        assert!(
            !sidecar.exists(),
            "a refused unlock must not have signed anything"
        );
        let message = state.error.clone().unwrap_or_default();
        assert!(message.starts_with("cannot sign"), "got {message}");
    }

    #[test]
    fn two_entries_that_do_not_match_start_again_rather_than_sealing() {
        // A key sealed under a passphrase the user did not mean to type is a
        // key they cannot open, and nothing says so until the next time they
        // try to sign. The recovery is the same as a document's and the
        // stakes are larger.
        let (_dir, mut state, _path) = a_saved_document();
        let id = state.workspace.active_id().expect("a tab");

        assert!(state.begin_signing());
        state.answer_passphrase("first");
        assert!(state.answer_passphrase("second"));

        assert_eq!(
            state.ask,
            Some(crate::passphrase::Ask::SetKey { id }),
            "a mismatch must return to the first question, not the second"
        );
        assert!(state.passphrase_status.contains("did not match"));
        assert!(!state.has_signing_key(), "a key was written anyway");
    }

    #[test]
    fn a_document_with_unsaved_changes_is_refused_rather_than_signed_as_it_was() {
        // **The refusal that matters.** `sign_file` signs what is on the
        // disk, because that is what a recipient checks. Signing here would
        // produce a valid signature over the *previous* version -- worse than
        // a refusal, because it verifies.
        let (_dir, mut state, path) = a_saved_document();
        state.edit("edited but not saved\n".to_owned());

        assert!(!state.begin_signing());
        assert!(state.ask.is_none(), "it must not even ask");
        assert!(
            !bp_integrity::sidecar_path(&path)
                .expect("a sidecar path")
                .exists()
        );
        let message = state.error.clone().unwrap_or_default();
        assert!(message.contains("unsaved changes"), "got {message}");
        assert!(message.contains("save it first"), "got {message}");
    }

    #[test]
    fn a_document_that_has_never_been_saved_says_so_and_not_something_else() {
        // A different problem from the one above with a different way out --
        // Save As rather than Ctrl+S -- so it gets its own sentence.
        let mut state = AppState::new();
        state.edit("never saved\n".to_owned());

        assert!(!state.begin_signing());
        let message = state.error.clone().unwrap_or_default();
        assert!(message.contains("never been saved"), "got {message}");
    }

    #[test]
    fn signing_reaches_the_security_history() {
        // ADR-0024: the five capabilities that write into the history are
        // five because each one is a thing somebody may need to prove
        // happened. Making a signature is the clearest of them.
        let (_dir, mut state, _path) = a_saved_document();
        assert!(state.begin_signing());
        state.answer_passphrase("hunter2");
        assert!(state.answer_passphrase("hunter2"));

        let history = state
            .security_history()
            .expect("the history must be readable");
        assert!(
            history.contains(&bp_audit::Event::SignatureMade.describe().to_string()),
            "signing left no trace: {history}"
        );
    }

    // --- the security history ---------------------------------------------

    #[test]
    fn a_security_operation_reaches_the_history() {
        // The wiring, end to end and without a window: an operation happens,
        // and the report the menu row shows can see it. Everything about
        // *what* a history is belongs to `bp-audit`; what this asserts is
        // that the shell actually calls it. Standard is the default profile
        // and the one that keeps a plaintext history.
        let mut state = AppState::new();
        let before = state.security_history().unwrap_or_default();

        state.edit(
            "nothing secret here
"
            .to_owned(),
        );
        state.scan_for_secrets();

        let after = state
            .security_history()
            .expect("the history must be readable");
        // Not a length comparison: an empty history is a paragraph explaining
        // itself and a one-event history is a single line, so the report gets
        // *shorter* when the first thing is recorded.
        assert_ne!(after, before, "the scan recorded nothing");
        assert!(
            after.contains(&bp_audit::Event::SecretScanFinished { findings: 0 }.describe()),
            "the history does not mention the scan:
{after}"
        );
    }

    #[test]
    fn loosening_a_profile_is_recorded_and_names_both_ends() {
        // The direction is the interesting part, and this is the direction
        // that matters: a move down to Standard re-enables everything the
        // profile was switched on to stop, which is exactly the event a
        // history exists to hold. It is recorded because the *new* profile
        // permits a history -- see the sibling test for the other direction.
        let mut state = AppState::new();
        state.set_security(bp_security::Security::Named(
            bp_security::Profile::Confidential,
        ));
        state.set_security(bp_security::Security::Named(bp_security::Profile::Standard));

        let report = state.security_history().expect("readable");
        let expected = bp_audit::Event::SecurityProfileChanged {
            from: bp_audit::ProfileLabel::Confidential,
            to: bp_audit::ProfileLabel::Standard,
        };
        assert!(
            report.contains(&expected.describe()),
            "the history does not name both ends of the change:
{report}"
        );
    }

    #[test]
    fn tightening_into_a_profile_that_forbids_a_history_writes_nothing() {
        // The event is recorded against the profile being *moved into*, which
        // is the one that decides whether it may be kept. Recording a
        // tightening under the old, looser profile would write the very line
        // the user just asked to stop writing. `Destination::for_policy`
        // resolves Confidential to `SessionOnly`, so this is that decision
        // reaching the shell rather than a gap in the wiring.
        let mut state = AppState::new();
        let before = state.security_history().unwrap_or_default();
        state.set_security(bp_security::Security::Named(
            bp_security::Profile::Confidential,
        ));

        let after = state.security_history().expect("readable");
        assert_eq!(
            after, before,
            "a profile that forbids a history had one written for it"
        );
    }

    #[test]
    fn privacy_mode_records_nothing_because_it_clamps_the_policy_first() {
        // Switching Privacy Mode on is itself governed by the policy Privacy
        // Mode puts in force, which forbids writing anything down. That is
        // the mode doing what it was switched on for, and it is worth pinning
        // so nobody later "fixes" it into a line on disk.
        let mut state = AppState::new();
        let before = state.security_history().unwrap_or_default();
        state.set_privacy(bp_security::Privacy::On);

        let after = state.security_history().expect("readable");
        assert_eq!(
            after, before,
            "Privacy Mode wrote a line to disk about being switched on"
        );
    }

    #[test]
    fn a_history_notice_never_displaces_the_answer_the_user_asked_for() {
        // "3 possible credentials" is what somebody clicked Scan for. A line
        // about the *history* of that scan taking its place would be the log
        // making the product worse, which is what `note_quietly` exists for.
        let mut state = AppState::new();
        state.edit(
            "nothing secret here
"
            .to_owned(),
        );
        state.scan_for_secrets();
        let message = state.error.clone().expect("the scan says what it found");
        assert!(
            !message.contains("security history"),
            "the audit notice displaced the scan result: {message}"
        );
    }

    // --- security profiles ------------------------------------------------

    #[test]
    fn a_new_document_starts_on_the_default_profile() {
        let state = AppState::new();
        assert_eq!(state.security(), bp_security::Security::default());
        assert_eq!(state.policy().recovery, bp_security::Recovery::Plaintext);
    }

    #[test]
    fn the_profile_belongs_to_the_document_not_the_application() {
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
    fn a_plaintext_document_under_a_strict_profile_is_told_how_to_fix_it() {
        // ADR-0020: fail loudly rather than degrade. The journal is sealed
        // with the document's own passphrase, so a plaintext document has no
        // key -- and the message says what turns recovery back on rather than
        // only that it is off.
        let mut state = AppState::new();
        state.set_security(bp_security::Security::Named(bp_security::Profile::Private));

        let message = state.error.expect("a capability gap reaches the user");
        assert!(message.contains("encrypt"), "got {message}");
    }

    #[test]
    fn an_encrypted_document_gets_an_encrypted_journal_and_recovers_at_unlock() {
        // The whole point of keying the sealed journal by path: a crashed
        // session's work comes back when the document is unlocked, and never
        // needs a prompt of its own.
        let recovery = tempfile::tempdir().unwrap();
        let docs = tempfile::tempdir().unwrap();
        let target = docs.path().join("notes.bpadx");

        let mut before = AppState::new();
        before.journal = bp_history::Journal::new(recovery.path().to_path_buf());
        before.edit("saved text".to_owned());
        let id = before.workspace.active_id().unwrap();
        before.ask = Some(crate::passphrase::Ask::Set {
            id,
            target: target.clone(),
        });
        before.answer_passphrase("hunter2");
        before.answer_passphrase("hunter2");

        // Work done after the save, then the session "crashes".
        before.set_security(bp_security::Security::Named(bp_security::Profile::Private));
        before.edit("saved text plus unsaved".to_owned());
        before.checkpoint_all();

        // Nothing readable without the passphrase, but something is there.
        assert!(before.journal.pending().is_empty());
        assert_eq!(before.journal.sealed_count(), 1);

        let mut after = AppState::new();
        after.journal = bp_history::Journal::new(recovery.path().to_path_buf());
        after.open_maybe_encrypted(target);
        after.answer_passphrase("hunter2");

        assert_eq!(
            after.active_text(),
            "saved text plus unsaved",
            "the unsaved work did not come back at unlock"
        );
        assert!(
            after.workspace.active().unwrap().is_dirty(),
            "recovered work is not on disk, so the tab must be dirty or the \
             user can close it and lose it without being asked"
        );
    }

    #[test]
    fn saving_an_encrypted_document_discards_its_sealed_journal() {
        // The work is on disk now. A journal left behind would offer to
        // "recover" a stale copy at the next unlock.
        let recovery = tempfile::tempdir().unwrap();
        let docs = tempfile::tempdir().unwrap();
        let target = docs.path().join("notes.bpadx");

        let mut state = AppState::new();
        state.journal = bp_history::Journal::new(recovery.path().to_path_buf());
        state.edit("first".to_owned());
        let id = state.workspace.active_id().unwrap();
        state.ask = Some(crate::passphrase::Ask::Set {
            id,
            target: target.clone(),
        });
        state.answer_passphrase("hunter2");
        state.answer_passphrase("hunter2");
        state.set_security(bp_security::Security::Named(bp_security::Profile::Private));

        state.edit("first and second".to_owned());
        state.checkpoint_all();
        assert_eq!(state.journal.sealed_count(), 1);

        assert_eq!(state.save_document(id, None), SaveResult::Saved);

        assert_eq!(
            state.journal.sealed_count(),
            0,
            "a stale sealed journal would be offered at the next unlock"
        );
    }

    #[test]
    fn a_confidential_document_is_not_checkpointed_in_plaintext() {
        // The end-to-end version of the journal's own test, through the
        // autosave path the application actually uses.
        let dir = tempfile::tempdir().unwrap();
        let mut state = AppState::new();
        state.journal = bp_history::Journal::new(dir.path().to_path_buf());
        state.edit("sensitive text".to_owned());

        state.checkpoint_all();
        assert_eq!(state.journal.pending().len(), 1, "Standard journals it");

        state.set_security(bp_security::Security::Named(
            bp_security::Profile::Confidential,
        ));
        state.checkpoint_all();

        assert!(
            state.journal.pending().is_empty(),
            "the earlier plaintext must be gone, not merely not added to"
        );
    }

    // --- privacy mode -----------------------------------------------------

    #[test]
    fn privacy_mode_removes_journals_already_written() {
        // Switching it on has to *act*, not merely be recorded. A mode that
        // only governed future writes would leave everything gathered a
        // moment ago exactly where it was, which is the opposite of what
        // somebody switching it on wants.
        //
        // The clipboard history was the other half of this test until
        // ADR-0061 removed it. The journal is now the only thing Privacy Mode
        // has already-written state to clear -- which makes this test more
        // load-bearing than it was, not less.
        let dir = tempfile::tempdir().unwrap();
        let mut state = AppState::new();
        state.journal = bp_history::Journal::new(dir.path().to_path_buf());
        state.edit("unsaved work".to_owned());
        state.checkpoint_all();
        assert_eq!(state.journal.pending().len(), 1);

        state.set_privacy(bp_security::Privacy::On);

        assert!(state.journal.pending().is_empty(), "the journal survived");
    }

    #[test]
    fn privacy_mode_acts_on_every_tab_not_only_the_active_one() {
        // It is session-wide. A journal left behind for a background tab is
        // exactly what it was switched on to prevent.
        let dir = tempfile::tempdir().unwrap();
        let mut state = AppState::new();
        state.journal = bp_history::Journal::new(dir.path().to_path_buf());
        state.edit("first tab".to_owned());
        state.new_document();
        state.edit("second tab".to_owned());
        state.checkpoint_all();
        assert_eq!(state.journal.pending().len(), 2);

        state.set_privacy(bp_security::Privacy::On);

        assert!(state.journal.pending().is_empty());
    }

    #[test]
    fn privacy_mode_does_not_relax_a_document_that_was_already_stricter() {
        // The clamp takes the stricter of each axis rather than substituting
        // a policy. Substituting would drag a Maximum document *down* to
        // whatever Privacy Mode specified.
        let mut state = AppState::new();
        state.set_security(bp_security::Security::Named(bp_security::Profile::Maximum));
        let before = state.policy();

        state.set_privacy(bp_security::Privacy::On);

        assert_eq!(state.policy(), before);
    }

    #[test]
    fn turning_privacy_mode_off_restores_each_documents_own_profile() {
        let mut state = AppState::new();
        let standard = state.policy();

        state.set_privacy(bp_security::Privacy::On);
        assert_ne!(state.policy(), standard);

        state.set_privacy(bp_security::Privacy::Off);
        assert_eq!(state.policy(), standard);
    }

    // --- Scan for Secrets --------------------------------------------------

    /// A credential the scanner actually recognises, and a document with one
    /// in it, so a test can assert the shell never repeats it back.
    ///
    /// Not one of the vendors' published `EXAMPLE` keys: `bp-secrets`
    /// deliberately refuses those, on the grounds that documentation is not a
    /// leak, so a fixture built from one would test nothing.
    const LEAKED: &str = "AKIA3G7QVHBRN2WPKZ5F";
    const LEAKY: &str = "notes\naws_access_key_id = AKIA3G7QVHBRN2WPKZ5F\ndone\n";

    #[test]
    fn a_clean_document_is_told_it_is_clean_rather_than_told_nothing() {
        // Silence after a scan is indistinguishable from a scan that did not
        // run, which is the one thing a security check must never be.
        let mut state = AppState::new();
        state.edit("nothing to see here\n".to_owned());
        let findings = state.scan_for_secrets();

        assert!(findings.is_empty());
        assert_eq!(
            state.error.as_deref(),
            Some("\u{2713} no credentials found")
        );
    }

    #[test]
    fn a_scan_reports_the_count_the_kind_and_where_it_is() {
        let mut state = AppState::new();
        state.edit(LEAKY.to_owned());
        let findings = state.scan_for_secrets();

        assert_eq!(findings.len(), 1);
        let notice = state.error.clone().expect("a status message");
        assert!(
            notice.contains('1'),
            "the count belongs in it; got {notice}"
        );
        assert!(
            notice.contains("AWS access key ID"),
            "the kind belongs in it; got {notice}"
        );
        assert!(
            notice.contains("line 2"),
            "where it is is the actionable part; got {notice}"
        );
    }

    #[test]
    fn neither_the_summary_nor_the_report_ever_repeats_the_secret() {
        // The whole design of `bp-secrets`: a `Finding` carries a position and
        // a classification and deliberately not the matched text, so that no
        // print of one can leak a credential. The shell is the last place that
        // could undo that, by reaching back into the document to quote it.
        let findings = bp_secrets::scan(LEAKY);
        assert!(!findings.is_empty(), "the fixture must actually be found");

        for line in [
            secret_scan_summary(&findings),
            secret_scan_report(&findings),
        ] {
            assert!(
                !line.contains(LEAKED),
                "the credential is back on screen: {line}"
            );
        }
    }

    #[test]
    fn a_long_scan_names_the_first_few_and_says_how_many_it_did_not() {
        // The status bar elides, so a list longer than the bar is a list whose
        // tail is invisible. Saying "and 2 more" is what stops the invisible
        // part reading as "there were only three".
        let text: String = (0..5)
            .map(|i| format!("aws_access_key_id{i} = {LEAKED}\n"))
            .collect();
        let findings = bp_secrets::scan(&text);
        assert_eq!(findings.len(), 5, "the fixture should yield five");

        let summary = secret_scan_summary(&findings);
        assert!(summary.contains('5'), "got {summary}");
        assert!(summary.contains("and 2 more"), "got {summary}");
    }

    #[test]
    fn the_full_report_has_a_line_per_finding_with_its_position() {
        let findings = bp_secrets::scan(LEAKY);
        let report = secret_scan_report(&findings);
        assert_eq!(report.lines().count(), findings.len());
        assert!(report.contains("line 2, column"), "got {report}");
    }

    // --- Hash, sign, verify ------------------------------------------------

    #[test]
    fn the_digest_is_taken_over_the_bytes_the_document_would_be_written_as() {
        // Not over the buffer. The BOM is stripped on load and written back on
        // save, so a digest of the buffer is one `sha256sum` disagrees with --
        // and a user comparing the two would conclude their file had been
        // altered when nothing had touched it.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bom.txt");
        std::fs::write(&path, b"\xEF\xBB\xBFone\ntwo\n").unwrap();

        let mut state = AppState::new();
        state.open(path.clone());
        assert_eq!(
            state.active_text(),
            "one\ntwo\n",
            "the BOM belongs on disk, not in the buffer"
        );
        let report = state.hash_active_document().expect("a digest");

        let on_disk = std::fs::read(&path).unwrap();
        let expected = bp_crypto::hash_document(&on_disk, bp_crypto::HashAlgorithm::Sha256);
        assert!(
            report.contains(&expected.to_hex()),
            "the digest should match the file on disk; got {report}"
        );
        let of_the_buffer = bp_crypto::hash_document(
            state.active_text().as_bytes(),
            bp_crypto::HashAlgorithm::Sha256,
        );
        assert!(
            !report.contains(&of_the_buffer.to_hex()),
            "hashing the buffer would leave the BOM out and give the wrong answer"
        );
    }

    #[test]
    fn the_digest_is_offered_grouped_and_unbroken_and_names_its_algorithm() {
        // Two spellings for two readers: grouped for a person comparing two
        // screens or reading it aloud, unbroken for pasting beside what
        // `sha256sum` printed.
        let mut state = AppState::new();
        state.edit("hello".to_owned());
        let report = state.hash_active_document().expect("a digest");

        let digest = bp_crypto::hash_document(b"hello", bp_crypto::HashAlgorithm::Sha256);
        assert!(report.contains(&digest.to_display()), "got {report}");
        assert!(report.contains(&digest.to_hex()), "got {report}");
        assert!(
            state
                .error
                .as_deref()
                .is_some_and(|e| e.contains("SHA-256")),
            "the status bar should name the algorithm; got {:?}",
            state.error
        );
    }

    #[test]
    fn a_digest_of_an_unsaved_document_says_it_is_not_the_file_on_disk() {
        let mut state = AppState::new();
        state.edit("draft".to_owned());
        let report = state.hash_active_document().expect("a digest");
        assert!(
            report.contains("unsaved changes"),
            "an unsaved digest that claims to be the file's is a lie; got {report}"
        );
    }

    #[test]
    fn the_digest_report_wraps_in_the_source_without_wrapping_on_the_screen() {
        // Both sentences in `hash_report` are wrapped to fit this file, and a
        // string continuation is the only wrapping that leaves no trace. The
        // escaped newline these two started as put a line break and
        // eight spaces into the middle of a sentence in the dialog
        // instead. Both branches are checked, because the unsaved
        // sentence is its own literal and was separately wrong.
        let mut unsaved_state = AppState::new();
        unsaved_state.edit("draft".to_owned());
        let unsaved = unsaved_state.hash_active_document().expect("a digest");

        let dir = tempfile::tempdir().unwrap();
        let (mut saved_doc, _) = saved_state(dir.path(), "notes.txt", "saved content\n");
        let saved = saved_doc.hash_active_document().expect("a digest");

        for report in [&unsaved, &saved] {
            assert!(
                report.contains("would be written as, so it matches the file"),
                "the continuation has to close the sentence up; got {report}"
            );
            for line in report.lines() {
                assert!(
                    !line.starts_with(' '),
                    "a wrapped literal leaked its indentation: {line:?}"
                );
            }
        }
        assert!(
            unsaved.contains("not yet the digest of anything on disk"),
            "got {unsaved}"
        );
    }

    #[test]
    fn a_utf16_document_hashes_rather_than_refusing() {
        // This test used to assert the opposite, and it was right to: the
        // shell's own encoder refused UTF-16, and a digest over an empty
        // vector would have been a plausible-looking wrong answer. `bp-files`
        // encodes UTF-16 now, so the refusal is gone and what is worth
        // pinning instead is that the digest covers the two-byte form with
        // its byte-order mark -- the bytes the file would actually hold.
        let mut state = AppState::new();
        state.edit("hello".to_owned());
        state.set_encoding(Encoding::Utf16Le);

        let bytes = state.active_bytes().expect("a document is open");
        assert_eq!(
            bytes.len(),
            2 + "hello".len() * 2,
            "a byte-order mark plus two bytes per character"
        );
        assert!(
            state.hash_active_document().is_some(),
            "got {:?}",
            state.error
        );
    }

    /// A signing identity that is the same on every run.
    ///
    /// A fixed seed rather than `SigningKey::generate`, so a failing
    /// assertion names the same key twice and a test cannot fail once a
    /// fortnight because the OS random source was briefly unavailable.
    fn signer(seed: u8) -> bp_integrity::SigningKey {
        bp_integrity::SigningKey::from_bytes(&[seed; bp_integrity::SIGNING_KEY_LEN])
            .expect("32 bytes is a valid seed")
    }

    /// Write the public half where the Verify row's key picker would find it.
    fn publish(dir: &std::path::Path, key: &bp_integrity::SigningKey) -> PathBuf {
        let path = dir.join(format!("{}.pub", key.verifying_key().to_hex()));
        std::fs::write(&path, key.verifying_key().to_hex()).unwrap();
        path
    }

    /// A document open on a file that exists, with no unsaved edits.
    fn saved_state(dir: &std::path::Path, name: &str, text: &str) -> (AppState, PathBuf) {
        let path = dir.join(name);
        std::fs::write(&path, text).unwrap();
        let mut state = AppState::new();
        state.open(path.clone());
        (state, path)
    }

    #[test]
    fn the_sidecar_is_found_beside_the_document_rather_than_asked_for() {
        // `document.ext.sig`, appended and not substituted. Two callers that
        // disagree about that name produce a document one half of the product
        // believes is unsigned, which is why the convention is a function in
        // `bp-integrity` and not a rule the shell writes down again.
        let dir = tempfile::tempdir().unwrap();
        let (mut state, path) = saved_state(dir.path(), "notes.txt", "signed content\n");
        let key = signer(7);
        bp_integrity::sign_file(&path, &key).expect("the sidecar is written");

        assert!(
            dir.path().join("notes.txt.sig").is_file(),
            "the sidecar is named after the whole file name"
        );
        assert!(state.verify_signature(&bp_integrity::Expectation::AnySigner));
        let notice = state.error.clone().unwrap();
        assert!(notice.starts_with('\u{2713}'), "got {notice}");
        assert!(
            notice.contains(&key.verifying_key().to_hex()),
            "the signer has to be named; got {notice}"
        );
    }

    #[test]
    fn a_missing_sidecar_fails_closed_and_says_where_it_looked() {
        // The variant the whole design rests on. A signature that can be
        // deleted to produce a pass is not a signature, so an unsigned
        // document is a failure and not an absence of opinion.
        let dir = tempfile::tempdir().unwrap();
        let (mut state, _) = saved_state(dir.path(), "notes.txt", "unsigned\n");

        assert!(
            !state.verify_signature(&bp_integrity::Expectation::AnySigner),
            "an unsigned document must not pass"
        );
        let notice = state.error.clone().unwrap();
        assert!(notice.starts_with('\u{2717}'), "got {notice}");
        assert!(notice.contains("Not signed"), "got {notice}");
        assert!(
            notice.contains("notes.txt.sig"),
            "the user has to be told which file to go and find; got {notice}"
        );
    }

    #[test]
    fn a_malformed_sidecar_is_not_reported_as_a_tampered_document() {
        // A truncated file is a copying accident and a bad signature is an
        // accusation. Telling the user the second when the first happened
        // sends them looking for an attacker who does not exist.
        let dir = tempfile::tempdir().unwrap();
        let (mut state, path) = saved_state(dir.path(), "notes.txt", "signed content\n");
        bp_integrity::sign_file(&path, &signer(7)).unwrap();
        std::fs::write(dir.path().join("notes.txt.sig"), "not a sidecar at all\n").unwrap();

        assert!(!state.verify_signature(&bp_integrity::Expectation::AnySigner));
        let notice = state.error.clone().unwrap();
        assert!(notice.contains("cannot be read"), "got {notice}");
        assert!(
            !notice.contains("altered"),
            "nothing here says anything about the document; got {notice}"
        );
    }

    #[test]
    fn a_document_altered_after_signing_does_not_match_and_the_signer_is_a_claim() {
        let dir = tempfile::tempdir().unwrap();
        let (mut state, path) = saved_state(dir.path(), "notes.txt", "signed content\n");
        let key = signer(7);
        bp_integrity::sign_file(&path, &key).unwrap();
        // Changed on disk, not in the buffer: this is about the file.
        std::fs::write(&path, "signed content, plus a line somebody added\n").unwrap();

        assert!(!state.verify_signature(&bp_integrity::Expectation::AnySigner));
        let notice = state.error.clone().unwrap();
        assert!(notice.contains("does NOT match"), "got {notice}");
        assert!(
            notice.contains("could not be confirmed"),
            "the named signer is a claim, not an attribution; got {notice}"
        );
        assert!(
            notice.contains(&key.verifying_key().to_hex()),
            "got {notice}"
        );
    }

    #[test]
    fn an_intact_document_signed_by_somebody_else_is_its_own_answer() {
        // The verdict a bare 64-byte `.sig` cannot produce, and the entire
        // reason ADR-0026's sidecar records a key. Nothing is damaged and
        // nothing was tampered with: this is a filing question, and reporting
        // it as "does not match" would be an alarm about nothing.
        let dir = tempfile::tempdir().unwrap();
        let (mut state, path) = saved_state(dir.path(), "notes.txt", "signed content\n");
        let colleague = signer(7);
        let expected = signer(9);
        bp_integrity::sign_file(&path, &colleague).unwrap();

        assert!(!state.verify_signature(&bp_integrity::Expectation::Key(expected.verifying_key())));
        let notice = state.error.clone().unwrap();
        assert!(notice.contains("by a different key"), "got {notice}");
        assert!(notice.contains("intact"), "got {notice}");
        assert!(
            notice.contains(&colleague.verifying_key().to_hex()),
            "got {notice}"
        );
        assert!(
            notice.contains(&expected.verifying_key().to_hex()),
            "got {notice}"
        );
    }

    #[test]
    fn naming_the_expected_key_is_what_turns_a_pass_into_a_claim_about_a_person() {
        let dir = tempfile::tempdir().unwrap();
        let (mut state, path) = saved_state(dir.path(), "notes.txt", "signed content\n");
        let key = signer(7);
        bp_integrity::sign_file(&path, &key).unwrap();

        assert!(state.verify_signature(&bp_integrity::Expectation::Key(key.verifying_key())));
        let notice = state.error.clone().unwrap();
        assert!(notice.starts_with('\u{2713}'), "got {notice}");
        assert!(
            !notice.contains("no key was named"),
            "the caveat belongs only to the permissive check; got {notice}"
        );
    }

    #[test]
    fn a_pass_with_no_key_named_carries_the_caveat_that_makes_it_honest() {
        // `Expectation::AnySigner` establishes only that the document has not
        // changed since the key *in the sidecar* signed it. Anyone who alters
        // a document can re-sign it with a key of their own and pass. A tick
        // with no caveat reads as "this is from who you think", which is a
        // claim nothing checked.
        let dir = tempfile::tempdir().unwrap();
        let (mut state, path) = saved_state(dir.path(), "notes.txt", "signed content\n");
        bp_integrity::sign_file(&path, &signer(7)).unwrap();

        assert!(state.verify_signature(&bp_integrity::Expectation::AnySigner));
        let notice = state.error.clone().unwrap();
        assert!(notice.contains("no key was named"), "got {notice}");
        assert!(
            notice.contains("re-sign it with a key of their own"),
            "got {notice}"
        );
    }

    #[test]
    fn every_verdict_gets_its_own_sentence() {
        // Four of the five are "no", and they are four rather than one
        // because their fixes are four different things: find the file, get
        // an undamaged copy, get an untampered document, get the right key. A
        // shell that collapsed them would undo the point of the format.
        let dir = tempfile::tempdir().unwrap();
        let mut seen: Vec<String> = Vec::new();

        // Verified, and signed by another key.
        let (mut state, path) = saved_state(dir.path(), "a.txt", "content\n");
        bp_integrity::sign_file(&path, &signer(7)).unwrap();
        state.verify_signature(&bp_integrity::Expectation::Key(signer(7).verifying_key()));
        seen.push(state.error.clone().unwrap());
        state.verify_signature(&bp_integrity::Expectation::Key(signer(9).verifying_key()));
        seen.push(state.error.clone().unwrap());

        // Missing.
        let (mut state, _) = saved_state(dir.path(), "b.txt", "content\n");
        state.verify_signature(&bp_integrity::Expectation::AnySigner);
        seen.push(state.error.clone().unwrap());

        // Malformed.
        let (mut state, path) = saved_state(dir.path(), "c.txt", "content\n");
        bp_integrity::sign_file(&path, &signer(7)).unwrap();
        std::fs::write(dir.path().join("c.txt.sig"), "rubbish\n").unwrap();
        state.verify_signature(&bp_integrity::Expectation::AnySigner);
        seen.push(state.error.clone().unwrap());

        // Does not match.
        let (mut state, path) = saved_state(dir.path(), "d.txt", "content\n");
        bp_integrity::sign_file(&path, &signer(7)).unwrap();
        std::fs::write(&path, "content, altered\n").unwrap();
        state.verify_signature(&bp_integrity::Expectation::AnySigner);
        seen.push(state.error.clone().unwrap());

        let mut distinct = seen.clone();
        distinct.sort();
        distinct.dedup();
        assert_eq!(
            distinct.len(),
            seen.len(),
            "two verdicts share a message: {seen:#?}"
        );
    }

    #[test]
    fn unsaved_edits_are_reported_because_the_verdict_is_about_the_file() {
        // The shell's own contribution and the only sentence here it is in a
        // position to write: `verify_file` checks what is on the disk, and
        // the buffer on screen is not that.
        let dir = tempfile::tempdir().unwrap();
        let (mut state, path) = saved_state(dir.path(), "notes.txt", "signed content\n");
        bp_integrity::sign_file(&path, &signer(7)).unwrap();
        state.edit("signed content\nand a line typed since\n".to_owned());

        // The file still verifies -- it is untouched -- and saying so without
        // the caveat would be a tick beside text nobody signed.
        assert!(state.verify_signature(&bp_integrity::Expectation::AnySigner));
        let notice = state.error.clone().unwrap();
        assert!(notice.contains("unsaved changes"), "got {notice}");
        assert!(
            notice.contains("not the bytes anybody signed"),
            "got {notice}"
        );
    }

    #[test]
    fn a_document_that_was_never_saved_has_nothing_for_a_sidecar_to_sit_beside() {
        // The row is greyed for this, and the arm says it anyway: a silent
        // no-op would be a bug report nobody could describe.
        let mut state = AppState::new();
        state.edit("never saved\n".to_owned());

        assert!(!state.verify_signature(&bp_integrity::Expectation::AnySigner));
        let notice = state.error.clone().unwrap();
        assert!(notice.contains("never been saved"), "got {notice}");
        assert!(
            !notice.contains("  "),
            "a wrapped literal leaked its indentation into the status bar: {notice:?}"
        );
    }

    #[test]
    fn a_public_key_pasted_out_of_an_email_as_hexadecimal_is_accepted() {
        // The realistic way a public key travels. Refusing it would teach
        // users to retype keys by hand, which is how a wrong key gets used.
        let dir = tempfile::tempdir().unwrap();
        let mut state = AppState::new();
        let key = signer(7);

        let hex_key = dir.path().join("key.txt");
        let text = key.verifying_key().to_hex();
        // Wrapped across two lines, exactly as an email client would leave it.
        std::fs::write(&hex_key, format!("{}\n{}\n", &text[..32], &text[32..])).unwrap();

        assert_eq!(
            state.expected_signer(&hex_key),
            Some(bp_integrity::Expectation::Key(key.verifying_key()))
        );
    }

    #[test]
    fn the_raw_thirty_two_byte_spelling_of_a_public_key_is_accepted_too() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = AppState::new();
        let key = signer(7);
        let raw = publish(dir.path(), &key);
        std::fs::write(&raw, key.verifying_key().to_bytes()).unwrap();

        assert_eq!(
            state.expected_signer(&raw),
            Some(bp_integrity::Expectation::Key(key.verifying_key()))
        );
    }

    #[test]
    fn a_key_file_that_is_not_a_key_says_which_of_the_two_files_was_wrong() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = AppState::new();
        let not_a_key = dir.path().join("letter.txt");
        std::fs::write(&not_a_key, "dear bob, here is the file\n").unwrap();

        assert_eq!(state.expected_signer(&not_a_key), None);
        let notice = state.error.clone().unwrap();
        assert!(
            notice.contains("key"),
            "the user has to know which file to replace; got {notice}"
        );
    }

    // --- Security ▸ Redact -------------------------------------------------

    /// The same fake credential the scan tests use, on a line with multi-byte
    /// characters in front of it.
    ///
    /// The byte offset of the key and its character column differ by three
    /// here, which is the entire point: `bp_secrets::Finding` counts
    /// characters and `bp_redaction::Span` counts bytes, and a shell that
    /// treated them as the same unit would destroy the wrong text.
    const LEAKY_MULTIBYTE: &str = "«clé» aws_access_key_id = AKIA3G7QVHBRN2WPKZ5F\n";

    /// A PEM block, whose finding covers the BEGIN line and not the key.
    const PEM: &str =
        "-----BEGIN RSA PRIVATE KEY-----\nMIIBOgIBAAJBAKtQ\n-----END RSA PRIVATE KEY-----\n";

    #[test]
    fn redacting_takes_the_credential_out_and_leaves_a_marker_where_it_was() {
        let mut state = AppState::new();
        state.edit(LEAKY.to_owned());
        let plan = state.plan_redaction().expect("a plan");

        assert!(state.apply_redaction(&plan), "the document should change");
        let after = state.active_text();
        assert!(
            !after.contains(LEAKED),
            "the credential is still there: {after}"
        );
        assert!(
            after.contains(bp_redaction::PLACEHOLDER.trim_end_matches(']')),
            "a redaction nobody can see is one nobody will act on; got {after}"
        );
        assert!(
            after.contains("AWS access key ID"),
            "the marker names the rule that matched; got {after}"
        );
        assert!(
            after.starts_with("notes\n"),
            "only the span may go; got {after}"
        );
    }

    #[test]
    fn a_credential_after_multibyte_text_is_located_in_bytes_not_characters() {
        // The conversion this shell owns, and the one place an off-by-three
        // would destroy the wrong words and report success.
        let mut state = AppState::new();
        state.edit(LEAKY_MULTIBYTE.to_owned());
        let plan = state.plan_redaction().expect("a plan");
        assert!(state.apply_redaction(&plan));

        let after = state.active_text();
        assert!(!after.contains(LEAKED), "got {after}");
        assert!(
            after.starts_with("«clé» aws_access_key_id = "),
            "everything before the credential must survive intact; got {after}"
        );
        assert!(
            after.ends_with("]\n"),
            "and nothing after it may be eaten; got {after}"
        );
    }

    #[test]
    fn redaction_is_an_edit_to_the_document_and_not_to_the_file() {
        // Irreversible in the string it returns, and deliberately not
        // irreversible on disk. `bp-redaction` rewrites one string;
        // ADR-0028's first consequence is that a shell which saved over the
        // original would have dealt with exactly one of the places the text
        // lives and told the user it had dealt with all of them.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("leaky.txt");
        let mut state = AppState::new();
        state.edit(LEAKY.to_owned());
        let id = state.workspace.active_id().unwrap();
        assert_eq!(
            state.save_document(id, Some(path.clone())),
            SaveResult::Saved
        );

        let plan = state.plan_redaction().expect("a plan");
        assert!(state.apply_redaction(&plan));

        assert!(
            state.is_dirty(id),
            "the redaction is unsaved work like any edit"
        );
        assert!(
            std::fs::read_to_string(&path).unwrap().contains(LEAKED),
            "nothing may have been written to the file"
        );

        // And it is on the undo stack, which is the honest reading of "undo
        // brings the originals back" in the status bar.
        assert!(state.active_editor_mut().unwrap().undo());
        assert!(state.active_text().contains(LEAKED));
    }

    #[test]
    fn nothing_the_redaction_says_ever_repeats_the_credential() {
        // The guarantee the scan already makes, extended to the half of the
        // feature that has the document in its hand: the plan, the
        // confirmation dialog and every status line it produces.
        let mut state = AppState::new();
        state.edit(LEAKY.to_owned());
        let plan = state.plan_redaction().expect("a plan");
        let planned = state.error.clone().expect("a status message");
        let consent = plan.consent_body();
        assert!(state.apply_redaction(&plan));
        let outcome = state.error.clone().expect("a status message");

        for line in [planned, consent, outcome, plan.refusal()] {
            assert!(
                !line.contains(LEAKED),
                "the credential is back on screen: {line}"
            );
        }
    }

    #[test]
    fn a_private_key_block_is_left_alone_and_named_rather_than_half_redacted() {
        // `bp-secrets` marks the BEGIN line only -- one line, column and
        // length cannot describe a block that runs to a matching END -- so
        // redacting the span would take out the label and leave the key
        // material under a `[REDACTED]` marker. That is the black-rectangle
        // failure ADR-0028 exists to prevent.
        let mut state = AppState::new();
        state.edit(PEM.to_owned());

        assert!(
            state.plan_redaction().is_none(),
            "there is nothing here this can safely destroy"
        );
        let notice = state.error.clone().expect("a status message");
        assert!(notice.contains("nothing was redacted"), "got {notice}");
        assert!(notice.contains("private key"), "got {notice}");
        assert!(
            notice.contains("BEGIN"),
            "the user has to know why, or they will read it as a bug; got {notice}"
        );
        assert!(
            state.active_text().contains("MIIBOgIBAAJBAKtQ"),
            "the key body must be exactly where it was"
        );
    }

    #[test]
    fn a_clean_document_is_told_there_is_nothing_to_redact() {
        let mut state = AppState::new();
        state.edit("nothing to see here\n".to_owned());
        assert!(state.plan_redaction().is_none());
        assert_eq!(
            state.error.as_deref(),
            Some("nothing to redact — no credentials found")
        );
    }

    #[test]
    fn declining_a_redaction_says_so_rather_than_leaving_the_bar_alone() {
        // Cancelling a file dialog leaves the status bar alone because
        // nothing was claimed. Here the user was asked a question, and
        // "I pressed Cancel" and "the row does nothing" must not look alike.
        let mut state = AppState::new();
        state.edit(LEAKY.to_owned());
        let plan = state.plan_redaction().expect("a plan");
        state.decline_redaction(&plan);
        let notice = state.error.clone().expect("a status message");

        assert!(notice.contains("nothing was redacted"), "got {notice}");
        assert!(notice.contains("still in this document"), "got {notice}");
        assert_eq!(state.active_text(), LEAKY);
    }

    #[test]
    fn a_plan_measured_against_text_that_has_since_changed_is_refused() {
        // `rfd` pumps events while its dialog is up, so a callback can edit
        // the buffer underneath one. `RedactionError::PastEnd` only catches
        // the half of that which runs off the end -- an edit that left the
        // length alone would redact the wrong bytes and report success.
        let mut state = AppState::new();
        state.edit(LEAKY.to_owned());
        let plan = state.plan_redaction().expect("a plan");
        state.edit(format!("prefix\n{LEAKY}"));

        assert!(
            !state.apply_redaction(&plan),
            "stale offsets must not be applied"
        );
        let notice = state.error.clone().expect("a status message");
        assert!(notice.contains("nothing was redacted"), "got {notice}");
        assert!(
            notice.contains("changed while the dialog was open"),
            "got {notice}"
        );
        assert!(
            state.active_text().contains(LEAKED),
            "and nothing was destroyed"
        );
    }

    #[test]
    fn a_copy_the_scan_did_not_mark_is_reported_as_surviving() {
        // ADR-0028 is careful that this is a reason to look and not a verdict:
        // every named span was destroyed, and the same text simply also
        // appears where nothing marked it. Reporting it as a failure would
        // teach the user to ignore it; saying nothing would be worse.
        let mut state = AppState::new();
        state.edit(format!(
            "aws_access_key_id = {LEAKED}\nnote: xx{LEAKED}xx\n"
        ));
        let plan = state.plan_redaction().expect("a plan");
        assert_eq!(
            plan.spans.len(),
            1,
            "the fixture must yield exactly one finding"
        );

        assert!(state.apply_redaction(&plan));
        let notice = state.error.clone().expect("a status message");
        assert!(notice.starts_with('⚠'), "got {notice}");
        assert!(notice.contains("still occurs elsewhere"), "got {notice}");
        assert!(
            notice.contains("line 1"),
            "the survivor is reported by position, never by text; got {notice}"
        );
        assert!(!notice.contains(LEAKED), "got {notice}");
    }

    #[test]
    fn the_outcome_says_the_original_is_still_everywhere_else() {
        // The sentence ADR-0028 makes the shell's obligation. Redacting the
        // buffer deals with one of the places the text lives, and a status
        // bar that reads "redacted" full stop tells the same lie the black
        // rectangle does.
        let plan = RedactionPlan::from_scan(LEAKY.to_owned(), &bp_secrets::scan(LEAKY));
        let clean = plan.outcome(1, Some(&[]));

        assert!(clean.starts_with('✓'), "got {clean}");
        assert!(clean.contains("undo"), "got {clean}");
        assert!(clean.contains("disk"), "got {clean}");
    }

    #[test]
    fn a_redaction_whose_check_could_not_run_does_not_report_it_as_clean() {
        let plan = RedactionPlan::from_scan(LEAKY.to_owned(), &bp_secrets::scan(LEAKY));
        let unchecked = plan.outcome(1, None);

        assert!(unchecked.starts_with('⚠'), "got {unchecked}");
        assert!(unchecked.contains("could not"), "got {unchecked}");
    }

    #[test]
    fn the_consent_dialog_lists_what_will_go_and_what_it_does_not_reach() {
        let plan = RedactionPlan::from_scan(LEAKY.to_owned(), &bp_secrets::scan(LEAKY));
        let body = plan.consent_body();

        assert!(body.contains("line 2 — AWS access key ID"), "got {body}");
        assert!(body.contains("not the file on disk"), "got {body}");
        assert!(body.contains("recovery journal"), "got {body}");
        assert!(body.contains("clipboard"), "got {body}");
    }

    // --- Security ▸ Inspect Metadata ---------------------------------------

    #[test]
    fn a_container_this_build_cannot_open_never_reports_an_all_clear() {
        // The failure `Container::hidden` exists to prevent: an inspector
        // that quietly says "no metadata found" about a `.docx` is worse than
        // no inspector, because the user believes it and sends the file.
        let summary = metadata_summary(&[], bp_redaction::Container::OfficeOpenXml);

        assert!(!summary.starts_with('✓'), "got {summary}");
        assert!(summary.contains("author name"), "got {summary}");
        assert!(summary.contains("cannot look"), "got {summary}");
    }

    #[test]
    fn plain_text_says_the_filesystem_entry_is_outside_the_check() {
        // `bp-redaction` is explicit that modification times, ownership and
        // extended attributes are the shell's and not its own, so the shell
        // is the only place that sentence can be said.
        let summary = metadata_summary(&[], bp_redaction::Container::PlainText);
        assert!(summary.starts_with('✓'), "got {summary}");
        assert!(summary.contains("ownership"), "got {summary}");

        let report = metadata_report(&[], bp_redaction::Container::PlainText);
        assert!(report.contains("alternate data streams"), "got {report}");
    }

    #[test]
    fn the_report_for_a_container_names_what_a_build_would_need_to_read_it() {
        let report = metadata_report(&[], bp_redaction::Container::Pdf);
        assert!(report.contains("PDF parser"), "got {report}");
        assert!(
            report.contains("all-clear"),
            "the reader has to be told not to take the empty list as one; got {report}"
        );
    }

    #[test]
    fn a_metadata_report_names_positions_and_kinds_and_never_the_value() {
        // The same rule as the secret scan, and for the same reason: a dialog
        // is a thing people screenshot into bug reports.
        let text = "author: Daniel Boles <daniel@example.com>\nnotes\n";
        let findings = bp_redaction::inspect(text);
        assert!(!findings.is_empty(), "the fixture must actually be found");

        for line in [
            metadata_summary(&findings, bp_redaction::Container::PlainText),
            metadata_report(&findings, bp_redaction::Container::PlainText),
        ] {
            assert!(!line.contains("Daniel Boles"), "got {line}");
            assert!(!line.contains("daniel@example.com"), "got {line}");
            assert!(line.contains("line 1"), "got {line}");
        }
    }

    #[test]
    fn inspecting_a_clean_plain_document_says_so_without_opening_a_dialog() {
        let mut state = AppState::new();
        state.edit("nothing identifying here\n".to_owned());

        assert!(
            state.inspect_metadata().is_none(),
            "a dialog with nothing in it is worse than the status bar line"
        );
        assert!(state.error.as_deref().is_some_and(|e| e.starts_with('✓')));
    }

    #[test]
    fn inspecting_a_document_in_a_container_opens_the_dialog_even_when_the_text_is_clean() {
        let mut state = AppState::new();
        let id = state.workspace.active_id().unwrap();
        state
            .workspace
            .get_mut(id)
            .unwrap()
            .set_path(PathBuf::from("report.docx"));
        state.edit("nothing identifying here\n".to_owned());

        let report = state
            .inspect_metadata()
            .expect("the part that was not looked at is the answer");
        assert!(report.contains("ZIP reader"), "got {report}");
    }

    #[test]
    fn container_of_recognises_the_shapes_a_text_only_inspection_cannot_open() {
        use bp_redaction::Container;

        for (name, expected) in [
            ("notes.txt", Container::PlainText),
            ("notes", Container::PlainText),
            ("report.DOCX", Container::OfficeOpenXml),
            ("sheet.ods", Container::OpenDocument),
            ("paper.pdf", Container::Pdf),
            ("letter.rtf", Container::RichText),
            ("photo.JPEG", Container::Image),
        ] {
            assert_eq!(
                container_of(Some(&PathBuf::from(name))),
                expected,
                "{name} was classified wrongly"
            );
        }
        assert_eq!(
            container_of(None),
            Container::PlainText,
            "a document that has never been saved claims nothing about hidden metadata"
        );
    }
}
