//! Where the product's own files live on each platform.
//!
//! Four directories, not one, because the four have different backup, sync
//! and lifetime requirements and putting them together is what turns a search
//! index into a two-gigabyte roaming profile:
//!
//! | | Windows | Linux |
//! |---|---|---|
//! | [`DirKind::Config`] | `%APPDATA%\bachelorpad` | `$XDG_CONFIG_HOME/bachelorpad` |
//! | [`DirKind::Data`] | `%LOCALAPPDATA%\bachelorpad\data` | `$XDG_DATA_HOME/bachelorpad` |
//! | [`DirKind::Cache`] | `%LOCALAPPDATA%\bachelorpad\cache` | `$XDG_CACHE_HOME/bachelorpad` |
//! | [`DirKind::State`] | `%LOCALAPPDATA%\bachelorpad\state` | `$XDG_STATE_HOME/bachelorpad` |
//!
//! ## Why this is not one `cfg!(windows)` in the caller
//!
//! Because the rules are not "pick one of two variables". They are: an
//! `XDG_*` value that is relative must be *ignored* rather than used, each
//! one has its own fallback under `$HOME` and they are not all under
//! `.config`, `%APPDATA%` and `%LOCALAPPDATA%` mean different things (one
//! roams to every machine the user signs into, the other does not) and can be
//! reconstructed from `%USERPROFILE%` when absent. That is a dozen rules, and
//! a dozen rules written inline is a dozen rules only one leg of CI compiles.
//!
//! Here they are one function of a [`Platform`] and an [`EnvSnapshot`], so
//! every one of them is checked by both legs.
//!
//! ## Never a guess
//!
//! When the environment does not say where the user's profile is, every
//! function here returns `None`. It does not fall back to the working
//! directory, to `/tmp`, or to the directory the executable is in. An editor
//! that writes its config next to whatever file the user happened to open is
//! worse than an editor that runs on defaults and says so, and
//! [`missing_variables`] exists so it can say which variable was missing.

use std::path::PathBuf;

use crate::paths::{is_absolute, join};
use crate::{APP_DIR, Platform};

/// One of the four directories the product keeps files in.
///
/// Separate variants rather than one "app directory" because the difference
/// is not cosmetic: [`Self::Config`] roams to every machine the user signs
/// into on Windows and is the thing they would want restored from a backup,
/// while [`Self::Cache`] is disposable by definition and must never do
/// either.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DirKind {
    /// Settings the user chose. Small, precious, and worth roaming.
    Config,
    /// Things the product made and would rather not remake: the semantic
    /// index, clipboard history, the recovery journal.
    Data,
    /// Recomputable. Deleting it while the editor is closed must cost the
    /// user nothing but time.
    Cache,
    /// Things that describe this machine's session -- window geometry, the
    /// recent-files list, logs. Not settings, because they are per-machine;
    /// not cache, because losing them is noticeable.
    State,
}

impl DirKind {
    /// Every kind, so that a test or a diagnostic covers all four without a
    /// list that goes stale.
    pub const ALL: &'static [DirKind] = &[
        DirKind::Config,
        DirKind::Data,
        DirKind::Cache,
        DirKind::State,
    ];

    /// A stable token for logs and for a `--print-dirs` style diagnostic.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Config => "config",
            Self::Data => "data",
            Self::Cache => "cache",
            Self::State => "state",
        }
    }

    /// Whether this directory follows the user to another machine.
    ///
    /// True only for [`Self::Config`] on Windows. Reported rather than
    /// assumed because it changes what may be written there: a roaming
    /// directory is copied at sign-out over a network the user is paying for,
    /// so a search index in it is a support ticket, and a machine-specific
    /// absolute path in it is a broken install on the second machine.
    /// Linux has no roaming profile at all, so the answer there is always
    /// false and code that relies on roaming is code that works on one
    /// platform.
    #[must_use]
    pub const fn roams(self, platform: Platform) -> bool {
        matches!((platform, self), (Platform::Windows, Self::Config))
    }
}

/// The environment variables that decide where anything goes, as a value.
///
/// Passed in rather than read, which is the single decision that makes this
/// module testable: `%APPDATA%` behaviour can be asserted from Linux and XDG
/// behaviour from Windows, and a test never has to set a process-wide
/// variable that another test running in the same process would see.
///
/// Fields are `String` rather than `OsString` because every rule here needs to
/// look at the text -- "is this absolute" is a question about characters. A
/// value that is not valid Unicode is therefore treated as absent, which is a
/// real if vanishingly rare limitation on Windows and is reported by
/// [`missing_variables`] like any other absence.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EnvSnapshot {
    /// `%APPDATA%`: the roaming half of the Windows profile.
    pub appdata: Option<String>,
    /// `%LOCALAPPDATA%`: the machine-local half.
    pub local_appdata: Option<String>,
    /// `%USERPROFILE%`: used only to rebuild the two above when a stripped or
    /// service environment has lost them.
    pub user_profile: Option<String>,
    /// `$HOME`: the root of every XDG fallback.
    pub home: Option<String>,
    /// `$XDG_CONFIG_HOME`, and the three below it. Honoured only when
    /// absolute -- see [`directory`].
    pub xdg_config_home: Option<String>,
    pub xdg_data_home: Option<String>,
    pub xdg_cache_home: Option<String>,
    pub xdg_state_home: Option<String>,
}

impl EnvSnapshot {
    /// Read the real process environment.
    ///
    /// The only function in this module that touches anything outside its
    /// arguments, and deliberately not behind `cfg`: asking Linux for
    /// `%APPDATA%` costs one failed lookup and yields `None`, whereas a
    /// `cfg`-split here would mean the Windows branch is never compiled by the
    /// Linux leg -- the exact failure this crate is shaped to avoid.
    #[must_use]
    pub fn from_environment() -> Self {
        let get = |key: &str| {
            std::env::var_os(key)
                .and_then(|value| value.into_string().ok())
                .filter(|value| !value.trim().is_empty())
        };
        Self {
            appdata: get("APPDATA"),
            local_appdata: get("LOCALAPPDATA"),
            user_profile: get("USERPROFILE"),
            home: get("HOME"),
            xdg_config_home: get("XDG_CONFIG_HOME"),
            xdg_data_home: get("XDG_DATA_HOME"),
            xdg_cache_home: get("XDG_CACHE_HOME"),
            xdg_state_home: get("XDG_STATE_HOME"),
        }
    }
}

/// The directory `kind` belongs in on `platform`, or `None` if the
/// environment does not say where the user's profile is.
///
/// The XDG rule worth spelling out, because it is the one most
/// implementations get wrong: a `XDG_*` variable holding a *relative* path is
/// invalid and must be ignored, falling back to the `$HOME` default. The
/// specification says so, and the reason is concrete -- honouring a relative
/// value puts the user's configuration under whatever directory the editor
/// happened to be launched from, and a second launch from elsewhere loses it.
///
/// Paths are assembled with the named platform's separator rather than
/// [`std::path::PathBuf::join`]'s, so the Windows answer is the same string
/// whichever leg of CI computed it.
#[must_use]
pub fn directory(platform: Platform, kind: DirKind, env: &EnvSnapshot) -> Option<PathBuf> {
    let (root, sub) = root_and_subdirectory(platform, kind, env)?;
    Some(PathBuf::from(join(platform, &root, &[APP_DIR, sub])))
}

/// The platform's own root for `kind`, *before* the product's folder.
///
/// `%LOCALAPPDATA%`, `$XDG_DATA_HOME` and so on. Needed because not everything
/// the product writes goes in the product's folder: a `.desktop` entry belongs
/// in `$XDG_DATA_HOME/applications`, beside every other application's, because
/// that is the directory the desktop environment reads. Anything writing there
/// wants the root and not [`directory`].
#[must_use]
pub fn root(platform: Platform, kind: DirKind, env: &EnvSnapshot) -> Option<PathBuf> {
    let (root, _sub) = root_and_subdirectory(platform, kind, env)?;
    Some(PathBuf::from(root))
}

/// The platform root, and the sub-path under the product folder that Windows
/// needs and Linux does not.
///
/// Windows has one local root for three kinds, so data, cache and state are
/// told apart by a folder inside `bachelorpad\`. Linux has a separate root per
/// kind, so nothing extra is needed and the sub-path is empty.
fn root_and_subdirectory(
    platform: Platform,
    kind: DirKind,
    env: &EnvSnapshot,
) -> Option<(String, &'static str)> {
    match platform {
        Platform::Windows => Some(match kind {
            // Roaming: the user's settings should follow them.
            DirKind::Config => (windows_roaming(env)?, ""),
            // Local: an index, a cache and a log have no business crossing a
            // network at sign-out.
            DirKind::Data => (windows_local(env)?, "data"),
            DirKind::Cache => (windows_local(env)?, "cache"),
            DirKind::State => (windows_local(env)?, "state"),
        }),
        Platform::Linux => {
            let (explicit, fallback) = match kind {
                DirKind::Config => (env.xdg_config_home.as_deref(), ".config"),
                DirKind::Data => (env.xdg_data_home.as_deref(), ".local/share"),
                DirKind::Cache => (env.xdg_cache_home.as_deref(), ".cache"),
                DirKind::State => (env.xdg_state_home.as_deref(), ".local/state"),
            };
            let root = match absolute(platform, explicit) {
                Some(value) => value,
                // `HOME` gets the same treatment: an unset, blank or relative
                // one is not a home directory, and joining onto it would give
                // `.config/bachelorpad` beside the working directory.
                None => join(
                    platform,
                    &absolute(platform, env.home.as_deref())?,
                    &[fallback],
                ),
            };
            Some((root, ""))
        }
    }
}

/// The directory `kind` belongs in on the platform this binary runs on.
///
/// The convenience wrapper the shell actually calls, and the only place in
/// this module where the compile target has any influence -- through
/// [`Platform::HOST`], not through a `cfg` of its own.
#[must_use]
pub fn host_directory(kind: DirKind) -> Option<PathBuf> {
    directory(Platform::HOST, kind, &EnvSnapshot::from_environment())
}

/// The environment variables this platform expects and does not usably have.
///
/// The answer to "why is the editor running on defaults", which is otherwise
/// invisible. Note the asymmetry with [`directory`]: a non-empty result does
/// *not* imply resolution failed, because a missing `%APPDATA%` can be covered
/// by `%USERPROFILE%`. It reports what is absent, and the caller pairs it with
/// a `None` from [`directory`] to decide whether that mattered. Reporting only
/// unrecoverable absences would hide the case worth knowing about -- an
/// environment odd enough to be worth mentioning in a bug report even though
/// the fallback saved it.
///
/// "Usably" carries weight: a variable set to a blank or relative value is
/// reported as missing, because it is.
#[must_use]
pub fn missing_variables(platform: Platform, env: &EnvSnapshot) -> Vec<&'static str> {
    let mut missing = Vec::new();
    match platform {
        Platform::Windows => {
            if absolute(platform, env.appdata.as_deref()).is_none() {
                missing.push("APPDATA");
            }
            if absolute(platform, env.local_appdata.as_deref()).is_none() {
                missing.push("LOCALAPPDATA");
            }
            if !missing.is_empty() && absolute(platform, env.user_profile.as_deref()).is_none() {
                missing.push("USERPROFILE");
            }
        }
        Platform::Linux => {
            // Asked of the resolver rather than of the field, so a `HOME` that
            // is set to something unusable -- blank, or relative -- is
            // reported as missing rather than as present and broken.
            if DirKind::ALL
                .iter()
                .any(|&kind| directory(platform, kind, env).is_none())
            {
                missing.push("HOME");
            }
        }
    }
    missing
}

/// `%APPDATA%`, or the standard place it would point if it were set.
///
/// The reconstruction from `%USERPROFILE%` is not a guess: `AppData\Roaming`
/// is the fixed layout of a Windows profile, and a stripped environment (a
/// service, a `runas`, a shell that inherited almost nothing) is common enough
/// that refusing outright would strand a real user. It is still a fallback,
/// which is why [`missing_variables`] reports `APPDATA` as absent even when
/// this succeeds through it.
fn windows_roaming(env: &EnvSnapshot) -> Option<String> {
    absolute(Platform::Windows, env.appdata.as_deref()).or_else(|| {
        absolute(Platform::Windows, env.user_profile.as_deref())
            .map(|profile| join(Platform::Windows, &profile, &["AppData", "Roaming"]))
    })
}

/// `%LOCALAPPDATA%`, or the standard place it would point. See
/// [`windows_roaming`].
fn windows_local(env: &EnvSnapshot) -> Option<String> {
    absolute(Platform::Windows, env.local_appdata.as_deref()).or_else(|| {
        absolute(Platform::Windows, env.user_profile.as_deref())
            .map(|profile| join(Platform::Windows, &profile, &["AppData", "Local"]))
    })
}

/// `value`, if it is set and absolute on `platform`.
///
/// The same rule XDG states explicitly, applied to the Windows variables too:
/// a relative `%APPDATA%` is not a profile root, it is a directory beside
/// whatever the working directory happens to be.
fn absolute(platform: Platform, value: Option<&str>) -> Option<String> {
    value
        .filter(|v| is_absolute(platform, v))
        .map(str::to_owned)
}

#[cfg(test)]
mod tests;
