//! What the passphrase bar is currently asking for, and what to do with the
//! answer.
//!
//! A small state machine rather than a flag, because the same one-field bar
//! serves three questions and answering the wrong one is expensive: unlocking
//! with a typo costs a retry, while *setting* one with a typo costs the
//! document, permanently and with no way to notice until it is too late.
//!
//! That is the whole reason [`Ask::Confirm`] exists. Encrypting asks twice and
//! compares, using one field and two submits rather than two fields, which
//! keeps the bar a bar.

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
        }
    }

    pub(crate) fn action(&self) -> &'static str {
        match self {
            Self::Unlock(_) => "Unlock",
            Self::Set { .. } => "Next",
            Self::Confirm { .. } => "Encrypt",
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
