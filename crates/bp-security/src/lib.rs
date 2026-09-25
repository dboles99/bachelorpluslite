//! Security profiles and the policy they resolve to (ADR-0020).
//!
//! ADR-0011 says a document's security policy propagates to every artefact
//! derived from it. What is left of that here is two artefacts -- the
//! recovery journal and the metadata store -- because those are the two
//! things this product derives from a document and writes down. This crate
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
//!   that decays: another axis gets added, the interesting rows get filled
//!   in, and one profile is quietly left more permissive than the stricter
//!   one below it.
//!
//! What this crate deliberately does not decide is in ADR-0020: which
//! network destinations are acceptable when the network is permitted, how a
//! document acquires a profile, and encryption itself.

use serde::{Deserialize, Serialize};

/// Whether unsaved work may be written to the recovery journal.
///
/// **There was an `Encrypted` between the two until ADR-0064**, and it was
/// deleted rather than quietly re-pointed at `Plaintext`. A profile that
/// asked for an encrypted journal now asks for `Disabled`: *no journal*
/// rather than *a journal somebody was told was encrypted*, which is the
/// failure ADR-0020 exists to prevent.
///
/// The recovery journal reads this, in `bp-ui`'s `state/privacy.rs`. It
/// writes plaintext into the user's state directory, which is
/// [`Recovery::Plaintext`] -- named rather than hidden,
/// because a recovery journal is a copy of unsaved work sitting outside the
/// file being edited.
///
/// Ordered permissive-first, and every axis in this crate is, so that
/// "at least as strict as" is a comparison rather than a table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Recovery {
    Plaintext,
    Disabled,
}

/// What the local metadata store may record about a document.
///
/// `bp-storage` reads this, through `bp-ui`'s `state/organize.rs`, whenever a
/// document is recorded (ADR-0037). ADR-0019 is why it exists at all: a
/// document's extracted title is a summary of what the user wrote, so a store
/// full of them is a plaintext index of what they write about.
/// `PathOnly` is the middle ground -- enough to notice a file has been seen
/// before, not enough to say what is in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Metadata {
    Summary,
    PathOnly,
    Disabled,
}

/// What a profile permits, one field per axis.
///
/// This is what code reads. **Two axes, and each has a reader that acts on
/// it**: `recovery` governs the journal and `metadata` governs the store.
///
/// There were seven. An axis nothing consults reads as a guarantee while
/// being decoration, which is worse than its absence -- and four of these
/// were worse than decoration, because the Security Inspector showed them to
/// users as the policy in force. `clipboard` left with its crate (ADR-0061);
/// `embeddings`, `network`, `temporary_files` and `zeroise` left together
/// under ADR-0082, which carried out what ADR-0059 had decided and ADR-0064
/// deferred. A new axis arrives with the code that reads it, or not at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    pub recovery: Recovery,
    pub metadata: Metadata,
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
        self.recovery >= other.recovery && self.metadata >= other.metadata
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
            // Today's product, described: the journal writes plaintext, and
            // the store records a document's title as well as its path
            // (ADR-0037).
            Self::Standard => Policy {
                recovery: Recovery::Plaintext,
                metadata: Metadata::Summary,
            },
            // The step that stops the store knowing what a document is about:
            // it records the path and never the title. **Recovery is
            // plaintext, and the menu says so** (ADR-0065).
            //
            // This row used to be described as the one that stops content
            // leaving the machine. Nothing enforced that -- the `network`
            // axis had no reader -- and nothing needs to: this product has no
            // network code in any profile (ADR-0006, ADR-0082).
            //
            // It asked for an encrypted journal until ADR-0064 removed the
            // sealed form, and that ADR set it to `Disabled` -- no journal --
            // on ADR-0020's rule that a control which quietly weakens itself
            // is worse than an absent one. The word doing the work there is
            // **quietly**: this menu has always printed what the journal
            // actually is, and it now prints "on, unencrypted".
            //
            // A journal on the local disk does not contradict what Private is
            // for, which is keeping the store from indexing subjects. Losing a
            // session's unsaved work to protect against an attacker who
            // already has the disk is the wrong trade, and it is the one
            // Confidential exists to make instead.
            Self::Private => Policy {
                recovery: Recovery::Plaintext,
                metadata: Metadata::PathOnly,
            },
            // Nothing is recorded about the document and nothing derived
            // from it is computed -- and that includes the recovery journal,
            // which is a copy of unsaved work sitting outside the file. This
            // is where "no journal" belongs, because it is the profile whose
            // whole subject is what touches the disk.
            Self::Confidential => Policy {
                recovery: Recovery::Disabled,
                metadata: Metadata::Disabled,
            },
            // The disk holds the file and nothing else. A crash loses
            // unsaved work, and that is the point rather than a gap.
            //
            // **The same policy as Confidential**, and it was before ADR-0082
            // too: the four axes that told them apart had no reader. Two
            // names for one policy is a question for Daniel, not something
            // to settle by inventing a difference.
            Self::Maximum => Policy {
                recovery: Recovery::Disabled,
                metadata: Metadata::Disabled,
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
                metadata: policy.metadata.max(Metadata::Disabled),
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
        // pairs rather than a list of assertions: another axis added later
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
                assert!(s.metadata >= l.metadata, "metadata");
            }
        }
    }

    #[test]
    fn standard_is_the_default_and_describes_todays_product() {
        // If this row and the product ever disagree, one of them is a bug.
        // The behaviours pinned here are the ones that exist right now:
        // bp-history writes plaintext, and nothing is forbidden that
        // currently happens.
        let policy = Security::default().policy();

        assert_eq!(Profile::default(), Profile::Standard);
        assert_eq!(
            policy.recovery,
            Recovery::Plaintext,
            "bp-history writes plaintext today; the model must say so rather \
             than quietly promising otherwise"
        );
    }

    #[test]
    fn the_journal_stops_at_confidential_and_not_before() {
        // This asked which profiles *needed cryptography* until ADR-0064, and
        // then whether any but Standard journalled at all. Both answers moved;
        // the reason for pinning it has not. Getting it wrong in one direction
        // disables recovery for no reason, and in the other writes unsaved
        // work to disk for somebody who asked for it not to be.
        //
        // **The line is between Private and Confidential**, and that is the
        // decision ADR-0065 took: a local journal does not contradict "stop
        // content leaving the machine", and it does contradict "nothing is
        // recorded about this document".
        for profile in [Profile::Standard, Profile::Private] {
            assert_eq!(
                profile.policy().recovery,
                Recovery::Plaintext,
                "{} keeps a journal, and the menu says it is unencrypted",
                profile.name()
            );
        }
        for profile in [Profile::Confidential, Profile::Maximum] {
            assert_eq!(
                profile.policy().recovery,
                Recovery::Disabled,
                "{} journals work its user asked not to have written down",
                profile.name()
            );
        }
    }

    #[test]
    fn a_custom_policy_is_returned_exactly_as_given() {
        // Custom is the escape hatch, and an escape hatch that normalises
        // what it is handed is not one.
        let policy = Policy {
            recovery: Recovery::Disabled,
            metadata: Metadata::Disabled,
        };
        let security = Security::Custom(policy);

        assert_eq!(security.policy(), policy);
        assert_eq!(security.name(), "Custom");
    }

    #[test]
    fn a_custom_policy_is_not_forced_into_the_monotonic_chain() {
        // Deliberate: constraining Custom would make it not custom. This
        // pins the decision so nobody "fixes" it later by clamping.
        //
        // Measured against Private, not Standard. Standard is the most
        // permissive value on every axis left, so nothing can be looser than
        // it anywhere, and a policy compared with it could only ever be
        // stricter -- the test would pass while asserting nothing. The loose
        // axis used to be clipboard, then embeddings, and each was *replaced*
        // when its axis left (ADR-0061, ADR-0082) for exactly that reason.
        let private = Profile::Private.policy();
        let mixed = Policy {
            // Stricter than Private on recovery...
            recovery: Recovery::Disabled,
            // ...and more permissive on metadata, which is what makes the two
            // genuinely incomparable rather than merely different.
            metadata: Metadata::Summary,
        };

        assert!(!mixed.is_at_least_as_strict_as(&private));
        assert!(!private.is_at_least_as_strict_as(&mixed));
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
            metadata: Metadata::Disabled,
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
        assert_eq!(clamped.metadata, Metadata::Disabled);
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
        assert!(Recovery::Plaintext < Recovery::Disabled);
        assert!(Metadata::Summary < Metadata::PathOnly);
        assert!(Metadata::PathOnly < Metadata::Disabled);
    }
}
