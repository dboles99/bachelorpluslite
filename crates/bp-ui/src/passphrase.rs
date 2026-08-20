//! What the passphrase bar is currently asking for, and what to do with the
//! answer.
//!
//! A small state machine rather than a flag, because the same one-field bar
//! serves six questions and answering the wrong one is expensive: unlocking
//! with a typo costs a retry, while *setting* one with a typo costs the
//! document, permanently and with no way to notice until it is too late.
//!
//! That is the whole reason [`Ask::Confirm`] exists. Encrypting asks twice and
//! compares, using one field and two submits rather than two fields, which
//! keeps the bar a bar. [`Ask::ConfirmKey`] is the same device for a signing
//! key, where the stakes are the same shape and larger: a mistyped key
//! passphrase does not cost one document, it costs the identity every
//! document signed with that key was signed under (ADR-0031).

use std::path::PathBuf;

use bp_core::DocumentId;
use zeroize::Zeroizing;

/// What the bar is asking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Ask {
    /// A `.bpadx` file was opened and needs its passphrase before it can
    /// become a tab. The path is held here rather than opened first, because
    /// a document nobody can read should not appear as an empty tab.
    Unlock(PathBuf),
    /// Encrypting the active document: the first of two entries.
    Set { id: DocumentId, target: PathBuf },
    /// The second entry, compared against the first.
    ///
    /// The first passphrase travels in this variant rather than in a separate
    /// field, so it cannot outlive the question it belongs to -- when the ask
    /// is dropped or replaced, so is the passphrase.
    Confirm {
        id: DocumentId,
        target: PathBuf,
        first: Zeroizing<String>,
    },
    /// Signing, and there is no key yet: the first of two entries for a new
    /// one.
    ///
    /// The document to sign travels with the question, because creating a key
    /// is something the user is doing *in order to* sign this document -- and
    /// finishing the ceremony only to be asked which document would be the
    /// product forgetting what it was in the middle of.
    SetKey { id: DocumentId },
    /// The second entry for a new signing key, compared against the first.
    ///
    /// The same shape as [`Self::Confirm`] and for a sharper reason. A
    /// mistyped document passphrase costs that document; a mistyped key
    /// passphrase costs the key, and with it the ability to ever sign
    /// anything else as the same signer. Everything already signed still
    /// verifies, which is what makes the loss quiet.
    ConfirmKey {
        id: DocumentId,
        first: Zeroizing<String>,
    },
    /// Signing, and the key exists: unlock it.
    UnlockKey { id: DocumentId },
}

impl Ask {
    /// The words on the bar.
    pub(crate) fn prompt(&self) -> &'static str {
        match self {
            Self::Unlock(_) => "Passphrase",
            Self::Set { .. } => "New passphrase",
            // Named differently on purpose. "Passphrase" twice gives no sign
            // that the first entry was accepted, and a user who thinks the
            // first did not register types something else the second time.
            Self::Confirm { .. } => "Confirm passphrase",
            // Every key prompt says *key*. The bar is one field in one place,
            // so the only thing separating "the passphrase that opens this
            // document" from "the passphrase that unlocks your signing
            // identity" is these words -- and a user who types the document's
            // one here has told this product a document passphrase it had no
            // reason to be told.
            Self::SetKey { .. } => "New signing key passphrase",
            Self::ConfirmKey { .. } => "Confirm key passphrase",
            Self::UnlockKey { .. } => "Signing key passphrase",
        }
    }

    pub(crate) fn action(&self) -> &'static str {
        match self {
            Self::Unlock(_) => "Unlock",
            Self::Set { .. } => "Next",
            Self::Confirm { .. } => "Encrypt",
            Self::SetKey { .. } => "Next",
            // Both end in a signature, and the button says so rather than
            // saying "Unlock" or "Create": what the user asked for was to
            // sign a document, and the key is machinery on the way there.
            Self::ConfirmKey { .. } | Self::UnlockKey { .. } => "Sign",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `DocumentId` is `bp-core`'s to mint, so a real workspace supplies one
    /// rather than this module inventing a value the type does not permit.
    fn an_id() -> DocumentId {
        let mut workspace = bp_core::Workspace::new();
        workspace.open_new(time::OffsetDateTime::UNIX_EPOCH)
    }

    fn set() -> Ask {
        Ask::Set {
            id: an_id(),
            target: PathBuf::from("/notes/a.bpadx"),
        }
    }

    #[test]
    fn the_two_entries_of_a_new_passphrase_are_worded_differently() {
        // A user who sees "Passphrase" twice has no sign the first entry
        // registered, and types something else the second time -- which turns
        // a working encryption into a mismatch they cannot explain.
        let confirm = Ask::Confirm {
            id: an_id(),
            target: PathBuf::from("/notes/a.bpadx"),
            first: Zeroizing::new("first".to_owned()),
        };
        assert_ne!(set().prompt(), confirm.prompt());
        assert_ne!(set().action(), confirm.action());
    }

    #[test]
    fn unlocking_and_setting_do_not_look_the_same() {
        // The two have opposite consequences for a typo, so they must not be
        // mistakable for each other.
        assert_ne!(
            Ask::Unlock(PathBuf::from("/a.bpadx")).prompt(),
            set().prompt()
        );
        assert_eq!(Ask::Unlock(PathBuf::from("/a.bpadx")).action(), "Unlock");
    }
}
