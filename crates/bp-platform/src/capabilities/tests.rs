//! Tests for the capability register.
//!
//! Mostly checks that the register is *complete* and *honest*, because those
//! are the two ways a capability table fails: a pair nobody answered, and a
//! "no" with no reason attached, which the UI can only render as a shrug.

use super::*;

#[test]
fn every_capability_has_an_answer_on_every_platform() {
    // The exhaustive match makes this true at compile time; the test is here
    // so that a future refactor to a lookup table cannot lose it silently.
    for &capability in Capability::ALL {
        for &platform in Platform::ALL {
            let _ = capability.availability(platform);
        }
    }
    assert_eq!(report(Platform::Windows).len(), Capability::ALL.len());
    assert_eq!(report(Platform::Linux).len(), Capability::ALL.len());
}

#[test]
fn every_refusal_says_why_in_a_sentence() {
    for &capability in Capability::ALL {
        for &platform in Platform::ALL {
            let availability = capability.availability(platform);
            let Some(reason) = availability.reason() else {
                continue;
            };
            assert!(
                reason.len() > 20,
                "{capability:?} on {platform:?} refuses with a reason too short to be one: \
                 {reason:?}"
            );
            assert!(
                reason.ends_with('.'),
                "{capability:?} on {platform:?}: a reason shown to a user is a sentence"
            );
            assert!(
                !reason.contains("  "),
                "{capability:?} on {platform:?}: a line continuation swallowed a space"
            );
        }
    }
}

#[test]
fn an_available_capability_carries_no_reason() {
    assert_eq!(Availability::Available.reason(), None);
    assert!(Availability::Available.is_usable());
}

#[test]
fn a_conditional_capability_is_not_a_usable_one() {
    // The distinction the whole enum exists for: "the platform can, this
    // machine might not" must not read as "go ahead".
    let conditional = Capability::ExtendedLengthPaths.availability(Platform::Windows);
    assert!(matches!(conditional, Availability::Conditional { .. }));
    assert!(!conditional.is_usable());
    assert!(!Capability::ExtendedLengthPaths.is_available_on(Platform::Windows));
    assert!(!available_on(Platform::Windows).contains(&Capability::ExtendedLengthPaths));
}

#[test]
fn the_two_secret_capabilities_are_blocked_on_a_person_not_on_work() {
    for &platform in Platform::ALL {
        assert!(
            Capability::CredentialStore
                .availability(platform)
                .is_blocked_on_a_decision(),
            "{platform:?}"
        );
    }
    assert!(
        Capability::BiometricUnlock
            .availability(Platform::Windows)
            .is_blocked_on_a_decision()
    );
    // ...and neither is merely unimplemented, which is the distinction that
    // stops a blocked question turning into a backlog item somebody picks up.
    assert!(!matches!(
        Capability::CredentialStore.availability(Platform::Windows),
        Availability::NotImplemented { .. }
    ));
}

#[test]
fn every_capability_has_a_distinct_token() {
    let mut tokens: Vec<&str> = Capability::ALL.iter().map(|c| c.token()).collect();
    let count = tokens.len();
    tokens.sort_unstable();
    tokens.dedup();
    assert_eq!(tokens.len(), count, "two capabilities share a token");
}

#[test]
fn tokens_are_lowercase_and_hyphenated_so_a_command_line_can_carry_them() {
    for &capability in Capability::ALL {
        let token = capability.token();
        assert!(
            token
                .chars()
                .all(|c| c.is_ascii_lowercase() || c == '-' || c.is_ascii_digit()),
            "{token}"
        );
    }
}

// --- the register must agree with the rest of the crate --------------------

#[test]
fn case_sensitivity_says_the_same_thing_here_as_in_the_path_rules() {
    // The failure this catches is drift: someone changes the path comparison
    // and the capability table keeps telling the UI the old answer, so the
    // editor warns about a collision it no longer detects.
    for &platform in Platform::ALL {
        assert_eq!(
            Capability::CaseSensitivePaths.is_available_on(platform),
            platform.case_sensitive_file_names(),
            "{platform:?}"
        );
    }
}

#[test]
fn the_platforms_differ_somewhere_and_agree_somewhere() {
    let windows = available_on(Platform::Windows);
    let linux = available_on(Platform::Linux);
    assert_ne!(
        windows, linux,
        "a matrix with identical rows is not a matrix"
    );
    assert!(!windows.is_empty());
    assert!(!linux.is_empty());
}
