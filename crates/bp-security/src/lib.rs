//! Security profiles and the policy they resolve to (ADR-0020).
//!
//! ADR-0011 says a document's security policy propagates to every artefact
//! derived from it -- the recovery journal, clipboard history, semantic
//! metadata, embeddings, temporary files and network eligibility. This crate
//! is where that policy is decided. It is decided here and performed
//! elsewhere: nothing in this crate writes a file, holds a secret or
//! encrypts anything.
//!
//! **Code reads a [`Policy`], never a profile's name.** A profile is a name
//! for a set of answers, and anything that matched on the name would have to
//! be revisited each time a profile was added or a `Custom` one configured.
//! One function, [`Security::policy`], stands between the two.
//!
//! Two things shape everything here.
//!
//! * **[`Profile::Standard`] is exactly what the product does today.**
//!   Adopting this model changes no behaviour until somebody chooses
//!   otherwise. The alternative was a flag day that silently disabled
//!   recovery for every existing user.
//! * **The named profiles are monotonic.** Standard, Private, Confidential,
//!   Maximum -- each at least as restrictive as the one before, on every
//!   axis. It is checked rather than asserted, because it is the property
//!   that decays: an eighth axis gets added, the interesting rows get filled
//!   in, and one profile is quietly left more permissive than the stricter
//!   one below it.
//!
//! What this crate deliberately does not decide is in ADR-0020: which
//! network destinations are acceptable when the network is permitted, how a
//! document acquires a profile, and encryption itself.

use serde::{Deserialize, Serialize};

/// Whether unsaved work may be written to the recovery journal, and how.
///
/// `bp-history` reads this. Today it writes plaintext into the user's config
/// directory, which is [`Recovery::Plaintext`] -- named rather than hidden,
/// because a recovery journal is a copy of unsaved work sitting outside the
/// file being edited.
///
/// Ordered permissive-first, and every axis in this crate is, so that
/// "at least as strict as" is a comparison rather than a table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Recovery {
    Plaintext,
    Encrypted,
    Disabled,
}

/// Whether copied text may outlive the moment it was copied.
///
/// `bp-clipboard` reads this. `InMemory` is today's behaviour: history is
/// kept for the session and never written down. `Persistent` is what
/// specs.md section 14 makes opt-in, and it is not reachable from any
/// profile below Standard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Clipboard {
    Persistent,
    InMemory,
    Disabled,
}

/// What the local metadata store may record about a document.
///
/// `bp-storage` reads this, and ADR-0019 is the reason it currently records
/// nothing: a document's extracted title is a summary of what the user wrote,
/// so a store full of them is a plaintext index of what they write about.
/// `PathOnly` is the middle ground -- enough to notice a file has been seen
/// before, not enough to say what is in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Metadata {
    Summary,
    PathOnly,
    Disabled,
}

/// Whether embeddings may be computed, and where.
///
/// Phase 10 reads this. `Cloud` means document content may be sent to a
/// service to be embedded, which is the strongest claim any axis here makes
/// and the reason it is the most permissive value of the most sensitive axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Embeddings {
    Cloud,
    Local,
    None,
}

/// Whether anything derived from the document may leave the machine.
///
/// `Allowed` means *this profile does not forbid it*. It does not mean any
/// particular destination is approved -- that is a separate decision and
/// deliberately not encoded here (ADR-0020).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Network {
    Allowed,
    Denied,
}

/// Whether document content may pass through a temporary file.
///
/// Atomic save writes one by design, so `Denied` is a real constraint on the
/// product rather than a formality: a profile that denies temporary files
/// needs a save path that does not use one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TemporaryFiles {
    Allowed,
    Denied,
}

/// Whether buffers holding document content should be wiped after use.
///
/// specs.md section 15, "sensitive-memory wrappers/zeroization where
/// practical". `Off` first, because off is the permissive answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Zeroise {
    Off,
    On,
}

/// What a profile permits, one field per axis.
///
/// This is what code reads. Seven axes, and every one has a named reader --
/// an axis nothing consults reads as a guarantee while being decoration,
/// which is worse than its absence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    pub recovery: Recovery,
    pub clipboard: Clipboard,
    pub metadata: Metadata,
    pub embeddings: Embeddings,
    pub network: Network,
    pub temporary_files: TemporaryFiles,
    pub zeroise: Zeroise,
}

impl Policy {
    /// Whether every axis of `self` is at least as restrictive as `other`'s.
    ///
    /// The comparison the monotonicity of the named profiles rests on. Not
    /// `PartialOrd` on `Policy`, deliberately: two policies can each be
    /// stricter than the other on different axes, and an `Ord` that claimed
    /// to order them would be lying about a genuine partial order.
    #[must_use]
    pub fn is_at_least_as_strict_as(&self, other: &Self) -> bool {
        self.recovery >= other.recovery
            && self.clipboard >= other.clipboard
            && self.metadata >= other.metadata
            && self.embeddings >= other.embeddings
            && self.network >= other.network
            && self.temporary_files >= other.temporary_files
            && self.zeroise >= other.zeroise
    }

    /// Whether this policy needs cryptography that does not exist yet.
    ///
    /// `bp-crypto` arrives in phase 15. Until it does, a profile asking for
    /// an encrypted journal cannot be honoured -- and ADR-0020 requires that
    /// to fail loudly rather than degrade to plaintext. A security control
    /// that quietly weakens itself is worse than an absent one, because the
    /// user has been told it is on.
    #[must_use]
    pub const fn needs_encryption(&self) -> bool {
        matches!(self.recovery, Recovery::Encrypted)
    }
}

/// A named security profile (specs.md section 15).
///
/// Ordered by strictness, which is what makes the monotonicity check a walk
/// over consecutive pairs rather than a hand-written table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
pub enum Profile {
    /// What the product does today. The default, and load-bearing
    /// documentation: behaviour that drifts from this row makes the model
    /// lie.
    #[default]
    Standard,
    /// Nothing derived from the document leaves the machine, and what is
    /// written down about it is reduced to a path.
    Private,
    /// Nothing about the document is recorded or kept beyond the session.
    Confidential,
    /// As little touches the disk as the product can manage.
    Maximum,
}

impl Profile {
    /// Every named profile, strictest last.
    #[must_use]
    pub const fn all() -> &'static [Self] {
        &[
            Self::Standard,
            Self::Private,
            Self::Confidential,
            Self::Maximum,
        ]
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Standard => "Standard",
            Self::Private => "Private",
            Self::Confidential => "Confidential",
            Self::Maximum => "Maximum",
        }
    }

    /// What this profile permits.
    ///
    /// The whole table, in one place, readable top to bottom. It is meant to
    /// be read as a table: the rows below are the specification, and a
    /// reviewer should be able to check them against ADR-0020 without
    /// following any code.
    #[must_use]
    pub const fn policy(self) -> Policy {
        match self {
            // Today's product, described. Recovery writes plaintext,
            // clipboard history is in memory, the metadata store is unwired
            // (ADR-0019) so nothing is recorded yet -- but Standard is what
            // permits it to be, which is what makes ADR-0019 revisitable
            // rather than reversible by hand.
            Self::Standard => Policy {
                recovery: Recovery::Plaintext,
                clipboard: Clipboard::InMemory,
                metadata: Metadata::Summary,
                embeddings: Embeddings::Local,
                network: Network::Allowed,
                temporary_files: TemporaryFiles::Allowed,
                zeroise: Zeroise::Off,
            },
            // The step that stops content leaving the machine. Recovery is
            // still kept, because losing unsaved work is its own harm -- but
            // it has to be encrypted, and until bp-crypto exists that means
            // the journal is refused rather than silently written in clear.
            Self::Private => Policy {
                recovery: Recovery::Encrypted,
                clipboard: Clipboard::InMemory,
                metadata: Metadata::PathOnly,
                embeddings: Embeddings::Local,
                network: Network::Denied,
                temporary_files: TemporaryFiles::Allowed,
                zeroise: Zeroise::On,
            },
            // Nothing is recorded about the document and nothing derived from
            // it is computed. Clipboard history is off: a document at this
            // level should not leave fragments in a list the user can page
            // through from any other tab.
            Self::Confidential => Policy {
                recovery: Recovery::Encrypted,
                clipboard: Clipboard::Disabled,
                metadata: Metadata::Disabled,
                embeddings: Embeddings::None,
                network: Network::Denied,
                temporary_files: TemporaryFiles::Denied,
                zeroise: Zeroise::On,
            },
            // The disk holds the file and nothing else. Recovery is disabled
            // rather than encrypted, which is a real trade the user is
            // making: a crash loses unsaved work, and that is the point.
            Self::Maximum => Policy {
                recovery: Recovery::Disabled,
                clipboard: Clipboard::Disabled,
                metadata: Metadata::Disabled,
                embeddings: Embeddings::None,
                network: Network::Denied,
                temporary_files: TemporaryFiles::Denied,
                zeroise: Zeroise::On,
            },
        }
    }
}

/// A session-wide override that can only ever tighten (specs.md §15).
///
/// Privacy Mode is not a fifth profile and not a `Policy`. It is a switch that
/// takes whatever each document's profile permits and clamps it, so a user who
/// is about to share their screen, or is on someone else's machine, can turn
/// one thing on instead of auditing every open tab.
///
/// **It can only make things stricter.** That is the whole guarantee, and it
/// is checked: applying it to any policy yields one at least as strict as the
/// original, on every axis. A privacy mode that could relax something would be
/// a switch that silently weakened a document the user had deliberately
/// protected -- and it would be found out at exactly the wrong moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Privacy {
    /// Each document's own profile applies, unmodified.
    #[default]
    Off,
    /// Nothing derived from any document is written down or leaves the
    /// machine, whatever the individual profiles say.
    On,
}

impl Privacy {
    /// Apply the override to a policy.
    ///
    /// Takes the stricter of each axis rather than substituting a fixed
    /// policy. Substituting would *relax* a Maximum document down to whatever
    /// Privacy Mode happened to specify, which is the exact failure this type
    /// exists to make impossible.
    #[must_use]
    pub fn clamp(self, policy: Policy) -> Policy {
        match self {
            Self::Off => policy,
            Self::On => Policy {
                recovery: policy.recovery.max(Recovery::Disabled),
                clipboard: policy.clipboard.max(Clipboard::Disabled),
                metadata: policy.metadata.max(Metadata::Disabled),
                embeddings: policy.embeddings.max(Embeddings::None),
                network: policy.network.max(Network::Denied),
                temporary_files: policy.temporary_files.max(TemporaryFiles::Denied),
                zeroise: policy.zeroise.max(Zeroise::On),
            },
        }
    }

    #[must_use]
    pub const fn is_on(self) -> bool {
        matches!(self, Self::On)
    }
}

/// A document's security setting: a named profile, or one spelled out.
///
/// `Custom` carries a `Policy` rather than being a fifth `Profile` variant,
/// because it is not a name for a set of answers -- it *is* the answers, and
/// it sits outside the monotonic chain by definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Security {
    Named(Profile),
    Custom(Policy),
}

impl Default for Security {
    fn default() -> Self {
        Self::Named(Profile::default())
    }
}

impl Security {
    /// The policy this document's own setting describes, before any
    /// session-wide override.
    ///
    /// Most callers want [`policy_under`](Self::policy_under) instead: a
    /// caller that reads this one directly is a caller that Privacy Mode does
    /// not reach.
    #[must_use]
    pub const fn policy(&self) -> Policy {
        match self {
            Self::Named(profile) => profile.policy(),
            Self::Custom(policy) => *policy,
        }
    }

    /// The policy actually in force, given the session's Privacy Mode.
    ///
    /// The one function every caller should go through.
    #[must_use]
    pub fn policy_under(&self, privacy: Privacy) -> Policy {
        privacy.clamp(self.policy())
    }

    /// What to show the user.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Named(profile) => profile.name(),
            Self::Custom(_) => "Custom",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_named_profiles_are_monotonic_on_every_axis() {
        // The property that decays, and the reason this test is a loop over
        // pairs rather than a list of assertions: an eighth axis added later
        // is covered by it automatically, which is exactly when the mistake
        // gets made -- the interesting rows get filled in and one profile is
        // left quietly more permissive than the stricter one below it.
        for pair in Profile::all().windows(2) {
            let (looser, stricter) = (pair[0], pair[1]);
            assert!(
                stricter.policy().is_at_least_as_strict_as(&looser.policy()),
                "{} must be at least as strict as {} on every axis;\n  {} = {:?}\n  {} = {:?}",
                stricter.name(),
                looser.name(),
                looser.name(),
                looser.policy(),
                stricter.name(),
                stricter.policy(),
            );
        }
    }

    #[test]
    fn a_stricter_profile_never_permits_something_a_looser_one_forbids() {
        // The user-facing statement of the same property, checked the other
        // way round: moving up a profile must never hand back a permission.
        for (index, stricter) in Profile::all().iter().enumerate() {
            for looser in &Profile::all()[..index] {
                let s = stricter.policy();
                let l = looser.policy();
                assert!(
                    s.recovery >= l.recovery,
                    "recovery: {stricter:?} vs {looser:?}"
                );
                assert!(s.clipboard >= l.clipboard, "clipboard");
                assert!(s.metadata >= l.metadata, "metadata");
                assert!(s.embeddings >= l.embeddings, "embeddings");
                assert!(s.network >= l.network, "network");
                assert!(s.temporary_files >= l.temporary_files, "temporary files");
                assert!(s.zeroise >= l.zeroise, "zeroise");
            }
        }
    }

    #[test]
    fn standard_is_the_default_and_describes_todays_product() {
        // If this row and the product ever disagree, one of them is a bug.
        // The behaviours pinned here are the ones that exist right now:
        // bp-history writes plaintext, bp-clipboard keeps history in memory
        // only, and nothing is forbidden that currently happens.
        let policy = Security::default().policy();

        assert_eq!(Profile::default(), Profile::Standard);
        assert_eq!(
            policy.recovery,
            Recovery::Plaintext,
            "bp-history writes plaintext today; the model must say so rather \
             than quietly promising otherwise"
        );
        assert_eq!(policy.clipboard, Clipboard::InMemory);
        assert_eq!(policy.temporary_files, TemporaryFiles::Allowed);
        assert!(
            !policy.needs_encryption(),
            "the default must not require cryptography that does not exist yet"
        );
    }

    #[test]
    fn every_profile_above_standard_denies_the_network() {
        // The line the profiles exist to draw. Whichever destinations turn
        // out to be acceptable, they are acceptable only under Standard.
        for profile in &Profile::all()[1..] {
            assert_eq!(
                profile.policy().network,
                Network::Denied,
                "{} must not permit content to leave the machine",
                profile.name()
            );
        }
    }

    #[test]
    fn the_profiles_that_need_cryptography_are_the_ones_that_ask_for_it() {
        // `needs_encryption` is what the journal checks before deciding
        // whether it can honour a profile at all. Getting it wrong in one
        // direction disables recovery for no reason; in the other it writes
        // plaintext for a user who was told it was encrypted.
        assert!(!Profile::Standard.policy().needs_encryption());
        assert!(Profile::Private.policy().needs_encryption());
        assert!(Profile::Confidential.policy().needs_encryption());
        assert!(
            !Profile::Maximum.policy().needs_encryption(),
            "Maximum disables recovery outright, so there is nothing to encrypt"
        );
    }

    #[test]
    fn a_custom_policy_is_returned_exactly_as_given() {
        // Custom is the escape hatch, and an escape hatch that normalises
        // what it is handed is not one.
        let policy = Policy {
            recovery: Recovery::Disabled,
            clipboard: Clipboard::Persistent,
            metadata: Metadata::Disabled,
            embeddings: Embeddings::Cloud,
            network: Network::Allowed,
            temporary_files: TemporaryFiles::Denied,
            zeroise: Zeroise::On,
        };
        let security = Security::Custom(policy);

        assert_eq!(security.policy(), policy);
        assert_eq!(security.name(), "Custom");
    }

    #[test]
    fn a_custom_policy_is_not_forced_into_the_monotonic_chain() {
        // Deliberate: constraining Custom would make it not custom. This
        // pins the decision so nobody "fixes" it later by clamping.
        let mixed = Policy {
            // Stricter than Standard on recovery...
            recovery: Recovery::Disabled,
            // ...and more permissive on the clipboard.
            clipboard: Clipboard::Persistent,
            ..Profile::Standard.policy()
        };
        let standard = Profile::Standard.policy();

        assert!(!mixed.is_at_least_as_strict_as(&standard));
        assert!(!standard.is_at_least_as_strict_as(&mixed));
    }

    #[test]
    fn privacy_mode_can_only_ever_tighten() {
        // The whole guarantee. A privacy mode that could relax something
        // would silently weaken a document the user had deliberately
        // protected, and would be found out at the worst possible moment.
        let mut policies: Vec<Policy> = Profile::all().iter().map(|p| p.policy()).collect();
        // Plus a deliberately odd custom one, since Custom sits outside the
        // monotonic chain and is where an unclamped axis would hide.
        policies.push(Policy {
            recovery: Recovery::Disabled,
            clipboard: Clipboard::Persistent,
            metadata: Metadata::Disabled,
            embeddings: Embeddings::Cloud,
            network: Network::Allowed,
            temporary_files: TemporaryFiles::Denied,
            zeroise: Zeroise::On,
        });

        for policy in policies {
            let clamped = Privacy::On.clamp(policy);
            assert!(
                clamped.is_at_least_as_strict_as(&policy),
                "Privacy Mode relaxed {policy:?} into {clamped:?}"
            );
        }
    }

    #[test]
    fn privacy_mode_off_changes_nothing() {
        for profile in Profile::all() {
            let policy = profile.policy();
            assert_eq!(Privacy::Off.clamp(policy), policy);
        }
    }

    #[test]
    fn privacy_mode_does_not_relax_a_stricter_document() {
        // Substituting a fixed policy -- rather than taking the stricter of
        // each axis -- would drag a Maximum document *down* to whatever
        // Privacy Mode specified. This is that bug, pinned.
        let maximum = Profile::Maximum.policy();
        assert_eq!(
            Privacy::On.clamp(maximum),
            maximum,
            "Privacy Mode must be a floor, not a replacement"
        );
    }

    #[test]
    fn privacy_mode_denies_everything_worth_denying() {
        // From the most permissive starting point, so every axis is actually
        // exercised rather than already being at its strictest.
        let clamped = Privacy::On.clamp(Profile::Standard.policy());

        assert_eq!(clamped.recovery, Recovery::Disabled);
        assert_eq!(clamped.clipboard, Clipboard::Disabled);
        assert_eq!(clamped.metadata, Metadata::Disabled);
        assert_eq!(clamped.embeddings, Embeddings::None);
        assert_eq!(clamped.network, Network::Denied);
        assert_eq!(clamped.temporary_files, TemporaryFiles::Denied);
        assert_eq!(clamped.zeroise, Zeroise::On);
    }

    #[test]
    fn the_policy_in_force_is_the_document_and_the_session_together() {
        let security = Security::Named(Profile::Standard);

        assert_eq!(security.policy_under(Privacy::Off), security.policy());
        assert!(
            security
                .policy_under(Privacy::On)
                .is_at_least_as_strict_as(&security.policy())
        );
    }

    #[test]
    fn a_security_setting_round_trips_through_json() {
        // A profile travels with its document, so it has to survive being
        // written down and read back -- including a Custom one, which is the
        // case a name-only encoding would silently lose.
        for security in [
            Security::Named(Profile::Confidential),
            Security::Custom(Profile::Maximum.policy()),
        ] {
            let json = serde_json::to_string(&security).unwrap();
            assert_eq!(
                serde_json::from_str::<Security>(&json).unwrap(),
                security,
                "round trip failed for {json}"
            );
        }
    }

    #[test]
    fn every_axis_orders_its_values_permissive_first() {
        // The ordering is what `is_at_least_as_strict_as` means. A variant
        // added in the wrong position would invert the comparison for that
        // axis and silently break every check above.
        assert!(Recovery::Plaintext < Recovery::Encrypted);
        assert!(Recovery::Encrypted < Recovery::Disabled);
        assert!(Clipboard::Persistent < Clipboard::InMemory);
        assert!(Clipboard::InMemory < Clipboard::Disabled);
        assert!(Metadata::Summary < Metadata::PathOnly);
        assert!(Metadata::PathOnly < Metadata::Disabled);
        assert!(Embeddings::Cloud < Embeddings::Local);
        assert!(Embeddings::Local < Embeddings::None);
        assert!(Network::Allowed < Network::Denied);
        assert!(TemporaryFiles::Allowed < TemporaryFiles::Denied);
        assert!(Zeroise::Off < Zeroise::On);
    }
}
