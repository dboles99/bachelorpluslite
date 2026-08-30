//! The platform seam ADR-0001 requires (specs.md section 20).
//!
//! ADR-0001 says portable behaviour lives in shared crates and OS behaviour is
//! isolated behind an adapter. The failure mode that decision guards against
//! is not "we called a Win32 function"; it is `cfg!(windows)` sprinkled
//! through twenty files, where each site is a rule nobody wrote down and half
//! of them are only ever compiled by half of CI.
//!
//! So this crate is built the other way round. **Almost everything here is a
//! plain function that takes [`Platform`] as a parameter**, which means the
//! Windows rules are executed by the Linux leg of CI and the Linux rules by
//! the Windows leg. Only a handful of functions -- the ones that genuinely
//! read the process environment or touch a disk -- are behind `cfg`, and each
//! of them is a thin wrapper whose only job is to choose the parameter.
//! [`Platform::HOST`] is the one place the compile target is consulted.
//!
//! ## What is here
//!
//! * [`paths`] -- the rules that actually differ. Reserved device names
//!   (`CON`, `NUL`, `AUX`), the characters each platform forbids, trailing
//!   dots and spaces, `MAX_PATH` and the `\\?\` escape hatch, and whether two
//!   names are the same file. A path that is fine on Linux and illegal on
//!   Windows is a defect this crate exists to catch before a save fails.
//! * [`dirs`] -- where configuration, data, cache and state live, resolved
//!   from an [`EnvSnapshot`](dirs::EnvSnapshot) that is passed in rather than
//!   read, so `%APPDATA%` behaviour is testable on Linux and XDG behaviour on
//!   Windows.
//! * [`capabilities`] -- what this platform can and cannot do, so callers ask
//!   instead of assuming. This is the register that keeps
//!   `docs/architecture/PLATFORM_MATRIX.md` from being the only place the
//!   answer exists.
//! * [`editor`] -- default-editor registration under ADR-0012: which file
//!   types we would register for, what the current association actually is,
//!   and the artefacts registration needs (a `.desktop` entry and a MIME
//!   package on Linux, an `HKCU\Software\Classes` shape on Windows).
//!
//! ## The line this crate draws
//!
//! **Reading state and generating artefacts is decision-free. Changing the
//! user's machine is not.** Every function that answers a question or builds
//! a file is pure or nearly so, always available, and needs no permission.
//! The single function that writes anything into a location the desktop
//! environment reads -- [`editor::install`] -- takes an explicit
//! [`Consent`](editor::Consent) value and an explicit destination root, and
//! refuses without both. It is the only item in this crate that can change
//! what happens when the user double-clicks a file.
//!
//! ## What is deliberately not here
//!
//! **Credential storage.** specs.md asks for DPAPI on Windows and Secret
//! Service on Linux, and `docs/architecture/PLATFORM_MATRIX.md` lists both.
//! They are not implemented, because what would go in them is entangled with
//! an open question a human has to answer: where the signing key of
//! `bp-integrity` lives, and whether a platform credential store is the
//! custodian or merely a wrapper around a key file. Implementing a keyring
//! first would decide that by accident. They appear here as
//! [`Capability::CredentialStore`](capabilities::Capability::CredentialStore)
//! and [`Capability::BiometricUnlock`](capabilities::Capability::BiometricUnlock),
//! reported as
//! [`Availability::AwaitingDecision`](capabilities::Availability::AwaitingDecision)
//! with the reason attached, so a caller gets an honest "not yet, and here is
//! why" rather than a silent absence.
//!
//! **Anything that runs a program.** Registration on Linux is only complete
//! once `update-desktop-database` and `update-mime-database` have run, and on
//! Windows the user has to visit Default Apps. This crate names those steps
//! ([`editor::FollowUp`], [`editor::Handoff`]) and executes none of them: a
//! library that spawns processes on the user's behalf is a library that can
//! be talked into spawning a different one.

#![forbid(unsafe_code)]

pub mod capabilities;
pub mod dirs;
pub mod editor;
pub mod paths;

pub use capabilities::{Availability, Capability};
pub use dirs::{DirKind, EnvSnapshot};
pub use paths::{PathProblem, path_problems};

/// Crate identity used by workspace smoke tests and diagnostics.
pub const CRATE_NAME: &str = "bp-platform";

/// The directory name this product owns under every platform root it uses.
///
/// One constant rather than a literal per call site, because the string
/// appears in a config path, a data path, a cache path and a registry key,
/// and four spellings of it is four half-migrated installs.
///
/// **Not [`APP_ID`], and the two must not be re-fused** (ADR-0032). This one
/// is a folder a person reads in a file manager and types in a terminal;
/// that one is a machine identifier that has to be globally unique and must
/// never change. They were the same string once, by accident, which meant a
/// future decision about either would silently have moved the other -- and
/// moving this one moves the user's settings, their recent list and their
/// recovery journal.
pub const APP_DIR: &str = "bachelorpad";

/// This product's identity to the operating system.
///
/// Reverse-DNS, per ADR-0032, because it names the `.desktop` file, every
/// association in every `mimeapps.list` on the machine, the AppStream
/// metadata, and the `HKCU\Software\Classes` ProgId prefix on Windows. A bare
/// name lives in a flat namespace shared with every other application
/// installed, which is what the convention exists to avoid.
///
/// `io.github.dboles99` rather than a `com.` domain because it is a namespace
/// this project demonstrably controls; it is the standard form for a project
/// without its own domain. The last segment is the application's name in its
/// own capitalisation, minus the `+`, which is not a character a `.desktop`
/// file name or a ProgId carries portably.
///
/// **This value has a compatibility promise attached.** Changing it after
/// anyone installs orphans every association they have made -- silently, by
/// the old id simply ceasing to be anything. See [`APP_DIR`] for what it is
/// deliberately not.
pub const APP_ID: &str = "io.github.dboles99.BachelorPadPlus";

/// The product's name as a person reads it.
///
/// Neither [`APP_DIR`] nor [`APP_ID`], and it carries no compatibility
/// promise at all -- this one may be changed freely, which is precisely why
/// it must not be spelled out at the three places that show it: the desktop
/// registration, `--version`, and Help ▸ Diagnostics. A rename that reaches
/// two of the three is worse than no rename.
pub const DISPLAY_NAME: &str = "BachelorPad+";

/// One line, wherever the product introduces itself.
///
/// Short enough for a `.desktop` `Comment=`, which is the tightest of the
/// places it appears.
pub const DESCRIPTION: &str = "Text editor for notes, data and logs";

/// One of the two operating systems ADR-0001 names, as a *value*.
///
/// The whole design of this crate rests on this being an argument rather than
/// a compile-time fact. `cfg!(windows)` at a call site produces code that one
/// leg of CI never runs; `Platform::Windows` passed to a function produces
/// code both legs run, and a test that can assert the Windows answer while
/// standing on Linux. The compile target is consulted exactly once, in
/// [`Platform::HOST`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Platform {
    /// Windows 10 and Windows 11. The two are the same platform for
    /// everything in this crate; where they differ it is a *capability*
    /// difference (Windows Hello) rather than a rule difference, and
    /// [`capabilities`] carries that distinction instead of the enum.
    Windows,
    /// Linux. Desktop environment unknown -- which is itself a rule, since it
    /// is why so many Linux capabilities here are conditional rather than
    /// simply present.
    Linux,
}

impl Platform {
    /// Every platform, for exhaustive tests and for UI that offers a choice.
    ///
    /// A slice rather than a derived iterator so that a test can loop over it
    /// in a `const` context and a new variant makes the loop cover it without
    /// anyone remembering to add a case.
    pub const ALL: &'static [Platform] = &[Platform::Windows, Platform::Linux];

    /// The platform this binary was compiled for.
    ///
    /// The single `cfg` on which the rest of the crate's platform knowledge
    /// depends. A target that is neither Windows nor Linux is reported as
    /// Linux rather than refusing to compile: ADR-0001 names exactly two
    /// targets, so anything else is unsupported, and of the two the
    /// Unix-family answer is the one that will not silently produce a
    /// backslash-separated path on a system that treats `\` as an ordinary
    /// filename character.
    #[cfg(windows)]
    pub const HOST: Platform = Platform::Windows;

    /// The platform this binary was compiled for. See the Windows arm for why
    /// non-Windows collapses to Linux.
    #[cfg(not(windows))]
    pub const HOST: Platform = Platform::Linux;

    /// A stable lowercase token, for config files, logs and diagnostics.
    ///
    /// Not `Display`, and not the `Debug` spelling: a value that ends up in a
    /// file the user might edit needs a spelling that is promised not to
    /// change, and `Debug` output is explicitly not promised.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::Linux => "linux",
        }
    }

    /// The inverse of [`Self::token`], for reading back what was written.
    ///
    /// Case-insensitive and whitespace-tolerant because the value's usual
    /// journey is through a hand-edited text file. Returns `None` rather than
    /// falling back to [`Self::HOST`]: a caller that meant "the host" can say
    /// so, and one that read `"macos"` out of a file needs to know it was not
    /// understood.
    #[must_use]
    pub fn parse(token: &str) -> Option<Self> {
        match token.trim().to_ascii_lowercase().as_str() {
            "windows" | "win32" | "win" => Some(Self::Windows),
            "linux" | "unix" => Some(Self::Linux),
            _ => None,
        }
    }

    /// The separator this platform's own APIs produce.
    ///
    /// Windows accepts `/` almost everywhere and Linux accepts only `/`, so
    /// this is for *writing* a path a human will read, never for parsing one.
    /// Parsing must accept both on Windows -- see [`paths::components`].
    #[must_use]
    pub const fn separator(self) -> char {
        match self {
            Self::Windows => '\\',
            Self::Linux => '/',
        }
    }

    /// Whether two file names differing only in case name different files.
    ///
    /// The single most expensive difference between the two targets for an
    /// editor: on Linux `Notes.txt` and `notes.txt` are two documents, on
    /// Windows they are one, and code that assumes either one corrupts data on
    /// the other. See [`paths::same_file_name`] for the comparison itself, and
    /// note that this is the *default* -- NTFS can be made case-sensitive per
    /// directory and Linux can host a case-insensitive filesystem, so this
    /// answers "what should I assume", not "what is true of this directory".
    #[must_use]
    pub const fn case_sensitive_file_names(self) -> bool {
        match self {
            Self::Windows => false,
            Self::Linux => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_is_the_package_name() {
        assert_eq!(CRATE_NAME, env!("CARGO_PKG_NAME"));
    }

    #[test]
    fn every_platform_round_trips_through_its_own_token() {
        for &platform in Platform::ALL {
            assert_eq!(
                Platform::parse(platform.token()),
                Some(platform),
                "{platform:?} does not survive token -> parse"
            );
        }
    }

    #[test]
    fn parse_tolerates_the_journey_through_a_text_file() {
        assert_eq!(Platform::parse("  WINDOWS\n"), Some(Platform::Windows));
        assert_eq!(Platform::parse("Linux"), Some(Platform::Linux));
    }

    #[test]
    fn an_unsupported_target_is_not_guessed_at() {
        assert_eq!(Platform::parse("macos"), None);
        assert_eq!(Platform::parse(""), None);
    }

    #[test]
    fn host_is_one_of_the_two_supported_platforms() {
        assert!(Platform::ALL.contains(&Platform::HOST));
    }

    // --- the two identities, which are deliberately not one -------------

    #[test]
    fn the_application_id_is_reverse_dns_and_is_not_the_directory_name() {
        // The *form* rather than the string, per ADR-0032, so registering a
        // domain later is a one-line edit and re-fusing the two constants is
        // not. What must not happen is the two silently becoming one again:
        // moving APP_DIR moves the user's settings, their recent list and
        // their recovery journal, and moving APP_ID orphans every file
        // association they have made.
        assert_ne!(
            APP_ID, APP_DIR,
            "the machine identifier and the folder name are different \
             questions with opposite constraints"
        );
        let segments: Vec<&str> = APP_ID.split('.').collect();
        assert!(
            segments.len() >= 3,
            "{APP_ID} is not reverse-DNS; it needs at least a domain and a name"
        );
        assert!(
            segments.iter().all(|s| !s.is_empty()),
            "{APP_ID} has an empty segment, so it has a leading, trailing or \
             doubled dot"
        );
    }

    #[test]
    fn the_application_id_survives_being_a_file_name_on_both_platforms() {
        // It becomes `<id>.desktop`, `<id>.xml` and `<id>-file-types.reg`, so
        // this is not a stylistic check. `AppInfo::problems` enforces it at
        // runtime; this fails at the constant instead, which is where anyone
        // changing it will be looking.
        assert!(
            paths::is_portable_file_name(APP_ID),
            "{APP_ID} cannot be used as a file name on both platforms"
        );
        for &platform in Platform::ALL {
            for suffix in [".desktop", ".xml", "-file-types.reg"] {
                let name = format!("{APP_ID}{suffix}");
                assert!(
                    paths::file_name_problems(platform, &name).is_empty(),
                    "{name} is not a legal file name on {platform:?}"
                );
            }
        }
    }

    #[test]
    fn the_directory_name_stays_something_a_person_can_type() {
        // The other half of ADR-0032, and the reason APP_DIR did not simply
        // follow APP_ID: this string is a folder a user reads in a file
        // manager and types in a terminal. A leading dot would additionally
        // make it a hidden directory on Linux.
        assert!(
            !APP_DIR.contains('.'),
            "{APP_DIR} reads as a hidden directory"
        );
        assert!(
            APP_DIR.chars().all(|c| c.is_ascii_lowercase()),
            "{APP_DIR} is not a plain lowercase folder name"
        );
        assert!(paths::is_portable_file_name(APP_DIR));
    }
}
