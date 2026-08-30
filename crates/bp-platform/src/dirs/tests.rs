//! Tests for directory resolution.
//!
//! None of them read or write the real environment, and none of them touch a
//! real user directory: every input is an [`EnvSnapshot`] built here. That is
//! what lets the Windows rules be asserted on the Linux leg of CI, and it is
//! also what keeps a test from leaving something in the developer's
//! `%APPDATA%`.

use super::*;
use proptest::prelude::*;

fn windows_env() -> EnvSnapshot {
    EnvSnapshot {
        appdata: Some(r"C:\Users\me\AppData\Roaming".into()),
        local_appdata: Some(r"C:\Users\me\AppData\Local".into()),
        user_profile: Some(r"C:\Users\me".into()),
        ..EnvSnapshot::default()
    }
}

fn linux_env() -> EnvSnapshot {
    EnvSnapshot {
        home: Some("/home/me".into()),
        ..EnvSnapshot::default()
    }
}

fn at(platform: Platform, kind: DirKind, env: &EnvSnapshot) -> String {
    directory(platform, kind, env)
        .expect("this environment resolves")
        .to_string_lossy()
        .into_owned()
}

// --- the documented layout -------------------------------------------------

#[test]
fn windows_puts_settings_in_roaming_and_everything_else_in_local() {
    let env = windows_env();
    assert_eq!(
        at(Platform::Windows, DirKind::Config, &env),
        r"C:\Users\me\AppData\Roaming\bachelorpad"
    );
    assert_eq!(
        at(Platform::Windows, DirKind::Data, &env),
        r"C:\Users\me\AppData\Local\bachelorpad\data"
    );
    assert_eq!(
        at(Platform::Windows, DirKind::Cache, &env),
        r"C:\Users\me\AppData\Local\bachelorpad\cache"
    );
    assert_eq!(
        at(Platform::Windows, DirKind::State, &env),
        r"C:\Users\me\AppData\Local\bachelorpad\state"
    );
}

#[test]
fn linux_follows_xdg_defaults_when_nothing_is_set() {
    let env = linux_env();
    assert_eq!(
        at(Platform::Linux, DirKind::Config, &env),
        "/home/me/.config/bachelorpad"
    );
    assert_eq!(
        at(Platform::Linux, DirKind::Data, &env),
        "/home/me/.local/share/bachelorpad"
    );
    assert_eq!(
        at(Platform::Linux, DirKind::Cache, &env),
        "/home/me/.cache/bachelorpad"
    );
    assert_eq!(
        at(Platform::Linux, DirKind::State, &env),
        "/home/me/.local/state/bachelorpad"
    );
}

#[test]
fn an_explicit_xdg_variable_wins_over_the_default() {
    let env = EnvSnapshot {
        xdg_config_home: Some("/etc/bpad-config".into()),
        ..linux_env()
    };
    assert_eq!(
        at(Platform::Linux, DirKind::Config, &env),
        "/etc/bpad-config/bachelorpad"
    );
    // ...and only that one; the others keep their defaults.
    assert_eq!(
        at(Platform::Linux, DirKind::Cache, &env),
        "/home/me/.cache/bachelorpad"
    );
}

// --- the rules that are easy to get wrong ----------------------------------

#[test]
fn a_relative_xdg_variable_is_ignored_rather_than_honoured() {
    // The XDG specification says an implementation must treat a relative path
    // as invalid. Honouring it would put the user's settings under whatever
    // directory the editor was launched from, and lose them on the next
    // launch from somewhere else.
    let env = EnvSnapshot {
        xdg_config_home: Some("relative/config".into()),
        xdg_data_home: Some("../data".into()),
        ..linux_env()
    };
    assert_eq!(
        at(Platform::Linux, DirKind::Config, &env),
        "/home/me/.config/bachelorpad"
    );
    assert_eq!(
        at(Platform::Linux, DirKind::Data, &env),
        "/home/me/.local/share/bachelorpad"
    );
}

#[test]
fn a_relative_appdata_is_ignored_too() {
    let env = EnvSnapshot {
        appdata: Some(r"AppData\Roaming".into()),
        ..windows_env()
    };
    assert_eq!(
        at(Platform::Windows, DirKind::Config, &env),
        r"C:\Users\me\AppData\Roaming\bachelorpad"
    );
}

#[test]
fn a_stripped_windows_environment_is_rebuilt_from_the_profile() {
    let env = EnvSnapshot {
        user_profile: Some(r"C:\Users\me".into()),
        ..EnvSnapshot::default()
    };
    assert_eq!(
        at(Platform::Windows, DirKind::Config, &env),
        r"C:\Users\me\AppData\Roaming\bachelorpad"
    );
    assert_eq!(
        at(Platform::Windows, DirKind::Cache, &env),
        r"C:\Users\me\AppData\Local\bachelorpad\cache"
    );
    // It still says the variables were missing, because they were.
    assert_eq!(
        missing_variables(Platform::Windows, &env),
        vec!["APPDATA", "LOCALAPPDATA"]
    );
}

#[test]
fn an_empty_environment_is_never_guessed_at() {
    let env = EnvSnapshot::default();
    for &platform in Platform::ALL {
        for &kind in DirKind::ALL {
            assert_eq!(
                directory(platform, kind, &env),
                None,
                "{platform:?} {kind:?} invented a directory out of nothing"
            );
        }
        assert!(
            !missing_variables(platform, &env).is_empty(),
            "{platform:?} refused without saying why"
        );
    }
}

#[test]
fn a_working_environment_has_nothing_missing() {
    assert!(missing_variables(Platform::Windows, &windows_env()).is_empty());
    assert!(missing_variables(Platform::Linux, &linux_env()).is_empty());
}

#[test]
fn a_blank_variable_counts_as_unset() {
    // `HOME=` in a stripped shell is common, and treating it as a root gives
    // `/.config/bachelorpad` -- a path in the filesystem root that the user
    // cannot write to and would not want to.
    let env = EnvSnapshot {
        home: Some(String::new()),
        ..EnvSnapshot::default()
    };
    assert_eq!(directory(Platform::Linux, DirKind::Config, &env), None);
}

// --- roaming ---------------------------------------------------------------

#[test]
fn only_windows_settings_roam() {
    assert!(DirKind::Config.roams(Platform::Windows));
    assert!(!DirKind::Data.roams(Platform::Windows));
    assert!(!DirKind::Cache.roams(Platform::Windows));
    assert!(!DirKind::State.roams(Platform::Windows));
    for &kind in DirKind::ALL {
        assert!(!kind.roams(Platform::Linux), "{kind:?}");
    }
}

#[test]
fn every_kind_has_a_distinct_token() {
    let mut tokens: Vec<&str> = DirKind::ALL.iter().map(|k| k.token()).collect();
    tokens.sort_unstable();
    let count = tokens.len();
    tokens.dedup();
    assert_eq!(tokens.len(), count);
}

// --- reading the real environment ------------------------------------------

#[test]
fn the_host_wrapper_agrees_with_the_parameterised_function() {
    // The only test that reads the real environment, and it reads it once and
    // compares two computations over the same values rather than asserting
    // anything about this machine -- CI runners differ, and a test that
    // demands a particular `$HOME` is a test that fails on somebody's laptop.
    let env = EnvSnapshot::from_environment();
    for &kind in DirKind::ALL {
        assert_eq!(
            host_directory(kind),
            directory(Platform::HOST, kind, &env),
            "{kind:?}"
        );
    }
}

// --- properties ------------------------------------------------------------

/// An absolute root for each platform, so the properties below feed
/// `directory` inputs it is meant to accept.
fn root(platform: Platform) -> impl Strategy<Value = String> {
    match platform {
        Platform::Windows => prop::string::string_regex(r"[A-Z]:\\[a-z]{1,8}(\\[a-z]{1,8}){0,3}"),
        Platform::Linux => prop::string::string_regex("(/[a-z]{1,8}){1,4}"),
    }
    .expect("the regex is a literal and compiles")
}

fn env_for(platform: Platform) -> impl Strategy<Value = (Platform, EnvSnapshot)> {
    root(platform).prop_map(move |base| {
        let env = match platform {
            Platform::Windows => EnvSnapshot {
                appdata: Some(format!(r"{base}\Roaming")),
                local_appdata: Some(format!(r"{base}\Local")),
                ..EnvSnapshot::default()
            },
            Platform::Linux => EnvSnapshot {
                home: Some(base),
                ..EnvSnapshot::default()
            },
        };
        (platform, env)
    })
}

/// Both platforms, each with an environment it can actually resolve. One
/// strategy rather than a `select` over platforms plus a lookup, so a property
/// never has to build a runner of its own.
fn platform_and_env() -> impl Strategy<Value = (Platform, EnvSnapshot)> {
    prop_oneof![env_for(Platform::Windows), env_for(Platform::Linux)]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// The four directories are always four different directories. Sharing
    /// one would mean deleting the cache deletes the settings, and the bug
    /// would only appear on whichever platform the paths happened to collide
    /// on.
    #[test]
    fn the_four_kinds_never_share_a_directory((platform, env) in platform_and_env()) {
        let mut paths: Vec<_> = DirKind::ALL
            .iter()
            .map(|&kind| directory(platform, kind, &env).expect("this environment resolves"))
            .collect();
        let count = paths.len();
        paths.sort();
        paths.dedup();
        prop_assert_eq!(paths.len(), count, "{:?} gave {:?}", platform, paths);
    }

    /// Whatever root the environment names, the resolved directory is
    /// absolute and sits inside the product's own folder. A rule that escaped
    /// its root would write into somebody else's application data.
    #[test]
    fn every_directory_is_absolute_and_inside_the_product_folder(
        (platform, env) in platform_and_env(),
    ) {
        for &kind in DirKind::ALL {
            let resolved = directory(platform, kind, &env).expect("this environment resolves");
            let text = resolved.to_string_lossy().into_owned();
            prop_assert!(
                text.contains(crate::APP_DIR),
                "{kind:?} on {platform:?} resolved to {text} with no product folder",
            );
            prop_assert!(
                crate::paths::is_absolute(platform, &text),
                "{kind:?} on {platform:?} resolved to the relative path {text}",
            );
        }
    }

    /// Whatever the environment says, what this module *appends* to it is a
    /// legal path on the platform it was resolved for. The root itself is
    /// the environment's business, not checked here: `%APPDATA%` is read,
    /// not constructed, and an environment strange enough to name a device
    /// in its own root is a fact about that environment, not a defect this
    /// module could fix by refusing to resolve. `APP_DIR` and the per-kind
    /// sub-path are the only strings this module chooses, so they are the
    /// only strings it is answerable for.
    #[test]
    fn a_resolved_directory_is_a_legal_path((platform, env) in platform_and_env()) {
        for &kind in DirKind::ALL {
            let (_root, sub) =
                root_and_subdirectory(platform, kind, &env).expect("this environment resolves");
            let appended = crate::paths::join(platform, "", &[APP_DIR, sub]);
            let problems = crate::paths::path_problems(platform, &appended);
            prop_assert!(problems.is_empty(), "{kind:?} on {platform:?}: {problems:?}");
        }
    }
}
