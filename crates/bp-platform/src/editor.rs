//! Default-editor registration, under ADR-0012.
//!
//! ADR-0012 accepts one way of becoming the user's text editor and refuses
//! another. Accepted: register as a supported handler for the types the
//! product understands, and guide the user to the operating system's own
//! default-app selection. Refused: replacing, patching, shimming or
//! redirecting `notepad.exe`; and, by extension, any mechanism that decides on
//! the user's behalf what opens their files.
//!
//! ## The line between asking and doing
//!
//! Three things happen in this module and only one of them can change the
//! user's machine.
//!
//! | | needs consent | available |
//! |---|---|---|
//! | [`state`] -- what opens `.txt` today | no | Linux; Windows reports "cannot tell" |
//! | [`plan`] -- the artefacts and registry values registration would use | no | both |
//! | [`install`] -- putting the artefacts where the desktop reads them | **yes** | Linux only |
//!
//! Reporting and generating are pure functions over their arguments. They
//! read no environment, write no file, and are safe to call from a settings
//! screen that is merely being drawn. [`install`] is the one function that
//! writes, and it takes two things nothing else does: an explicit
//! [`Consent`] value, and an explicit destination root. A caller cannot reach
//! it by accident and a test cannot reach the user's real directories at all.
//!
//! Even [`install`] stops short of setting the default. It puts the `.desktop`
//! entry and the MIME package where the desktop environment can see them --
//! which is what "appear in Open With" means -- and hands back a [`Handoff`]
//! naming the command that would make the product the default, for the user or
//! the shell to run. That is the ADR's "guide the user through the OS's own
//! default-app selection", one step short of doing it for them.
//!
//! ## Windows: generated, never applied
//!
//! [`plan`] on Windows produces the complete `HKCU\Software\Classes` shape and
//! renders it as a `.reg` script, and [`install`] refuses. Not out of caution
//! -- writing a registry key needs either the `winreg` dependency or a Win32
//! call, and this crate has neither a third-party dependency nor `unsafe`. The
//! artefact is in the plan, so a shell can offer it through a Save dialog: a
//! file the user chose where to put and can read before running is a
//! defensible answer to a registration this crate cannot perform, and arguably
//! a better one than an installer that writes the same keys invisibly.
//!
//! Every generated key is checked against [`windows::key_objection`], which is
//! ADR-0012 written as a predicate rather than as a paragraph.

pub mod desktop;
pub mod file_type;
pub mod state;
pub mod windows;

use std::fs;
use std::path::{Path, PathBuf};

use crate::capabilities::{Availability, Capability};
use crate::paths::{self, is_absolute};
use crate::{DirKind, EnvSnapshot, Platform};

pub use file_type::{
    AssociationPreset, AssociationSelection, FILE_TYPES, FileType, TypeGroup, file_type,
    unknown_extensions,
};
pub use state::{AssociationEntry, AssociationReport, AssociationState};
pub use windows::{KeyObjection, RegistryValue, ValueName};

/// What the product is, from the point of view of a registration.
///
/// A value the caller fills in rather than a set of constants, for one reason
/// that is not negotiable: `executable` is the path to the running binary, and
/// only the running process knows it. A hard-coded `/usr/bin/bachelorpad` is a
/// registration that launches the wrong program for anyone who installed to
/// `~/.local/bin`, ran a portable build, or is a developer with a `target/`
/// directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppInfo {
    /// Shown in Open With and in Default Apps.
    pub display_name: String,
    /// One line, shown beside the name.
    pub description: String,
    /// Absolute path to the executable to launch.
    pub executable: String,
    /// The identifier that names the `.desktop` file, and therefore the
    /// product's identity in every `mimeapps.list` on the machine. Changing it
    /// orphans every association the user has already made, so it is a value
    /// with a compatibility promise attached rather than a label.
    pub app_id: String,
    /// The icon name (Linux) -- on Windows the icon comes from the executable.
    pub icon: String,
}

impl AppInfo {
    /// This product, launched from `executable`.
    ///
    /// Everything but the path is fixed, because everything but the path is a
    /// promise: the `app_id` in particular must not drift between releases.
    ///
    /// It reads [`crate::APP_ID`] and not [`crate::APP_DIR`], which is the
    /// whole of ADR-0032. The icon takes the same value because freedesktop
    /// names an application's icon after its id.
    #[must_use]
    pub fn bachelorpad(executable: impl Into<String>) -> Self {
        Self {
            display_name: crate::DISPLAY_NAME.to_owned(),
            description: crate::DESCRIPTION.to_owned(),
            executable: executable.into(),
            app_id: crate::APP_ID.to_owned(),
            icon: crate::APP_ID.to_owned(),
        }
    }

    /// What is wrong with this description of the application, if anything.
    ///
    /// Worth calling before a plan is built rather than after it is installed:
    /// a relative `executable` produces a `.desktop` entry that launches
    /// nothing, and the failure surfaces as "double-clicking a text file does
    /// nothing", which is about as far from its cause as a defect gets.
    #[must_use]
    pub fn problems(&self, platform: Platform) -> Vec<String> {
        let mut problems = Vec::new();
        if self.executable.trim().is_empty() {
            problems.push("The path to the executable is empty.".to_owned());
        } else if !is_absolute(platform, &self.executable) {
            problems.push(format!(
                "The executable path \"{}\" is relative; a registration must name an absolute path.",
                self.executable
            ));
        }
        if self.app_id.trim().is_empty() {
            problems.push("The application id is empty.".to_owned());
        } else if !paths::is_portable_file_name(&self.app_id) {
            problems.push(format!(
                "The application id \"{}\" cannot be used as a file name.",
                self.app_id
            ));
        }
        if self.display_name.trim().is_empty() {
            problems.push("The display name is empty.".to_owned());
        }
        problems
    }
}

/// What kind of artefact a generated file is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtefactKind {
    /// A freedesktop `.desktop` entry.
    DesktopEntry,
    /// A `shared-mime-info` package defining the product's own types.
    MimePackage,
    /// A `.reg` script. Generated, never applied -- see the module docs.
    RegistryScript,
}

/// A file a registration would consist of.
///
/// The path is *relative*, always, and that is a safety property rather than
/// a convenience: an artefact carrying an absolute path is one that can be
/// written anywhere, and a plan is a value that travels. [`install`] re-checks
/// it before writing regardless.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artefact {
    /// Where it goes, relative to the install root.
    pub relative_path: String,
    /// The whole file.
    pub contents: String,
    /// What it is.
    pub kind: ArtefactKind,
}

/// A command that must run before the registration takes effect, named rather
/// than run.
///
/// Nothing in this crate spawns a process. A library that runs commands on the
/// user's behalf is a library that can be talked into running a different one,
/// and the value of these is mostly that a user can see them: "the association
/// will not appear until `update-desktop-database` has run" is the single most
/// common reason a correct `.desktop` file appears to do nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FollowUp {
    /// The program, by name -- resolved through `PATH` by whoever runs it.
    pub program: &'static str,
    /// Its arguments.
    pub args: Vec<String>,
    /// What goes wrong if it is skipped.
    pub why: &'static str,
}

impl FollowUp {
    /// The command as a user would type it, for display and for a log.
    ///
    /// Not for execution: arguments are joined with spaces and a path
    /// containing one would be ambiguous. Anything running these should use
    /// [`Self::program`] and [`Self::args`] as the separate values they are.
    #[must_use]
    pub fn command_line(&self) -> String {
        std::iter::once(self.program.to_owned())
            .chain(self.args.iter().cloned())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// The step that ends with the user, rather than with us.
///
/// ADR-0012's "guide the user through the OS's own default-app selection", as
/// a value the shell can act on. Neither variant is executed here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Handoff {
    /// Open the Windows Settings page. `ms-settings:` is the supported way to
    /// reach it on Windows 10 and 11; the older `control.exe /name
    /// Microsoft.DefaultPrograms` still works but lands on a page Windows 11
    /// redirects away from.
    WindowsDefaultApps { uri: &'static str },
    /// Run `xdg-mime default`, the standards-based way to set an association
    /// on Linux. It is offered rather than run because it *is* the choice --
    /// running it is deciding for the user, which is the thing ADR-0012 draws
    /// its line around.
    LinuxSetDefault {
        program: &'static str,
        args: Vec<String>,
    },
}

impl Handoff {
    /// A sentence telling the user what happens next.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::WindowsDefaultApps { .. } => {
                "BachelorPad+ now appears in Windows' list of applications. Choose it in \
                 Settings ▸ Apps ▸ Default apps to make it open these file types."
                    .to_owned()
            }
            Self::LinuxSetDefault { program, args } => format!(
                "BachelorPad+ now appears in Open With. To make it the default, run: {program} {}",
                args.join(" ")
            ),
        }
    }
}

/// Everything a registration would do, as a value, before anything is done.
///
/// Building one is free of consequence, which is the point: a settings screen
/// can show the user the exact `.desktop` file and the exact registry values
/// before asking whether to proceed. Nothing else in this crate needs
/// permission, and this is the value the one thing that does operates on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistrationPlan {
    /// Which platform it was built for.
    pub platform: Platform,
    /// The file types it claims.
    pub selection: AssociationSelection,
    /// The files it would write, with relative paths.
    pub artefacts: Vec<Artefact>,
    /// The registry values it would set. Always empty on Linux.
    pub registry_values: Vec<RegistryValue>,
    /// Commands that must run afterwards for it to take effect.
    pub follow_up: Vec<FollowUp>,
    /// Where the user finishes the job.
    pub handoff: Handoff,
}

impl RegistrationPlan {
    /// Any generated registry key this crate refuses to write.
    ///
    /// Always empty for a plan this crate built -- a test asserts it over
    /// every preset on both platforms. It is public so that whatever
    /// eventually *applies* a plan checks the plan it was handed rather than
    /// trusting where it came from.
    #[must_use]
    pub fn registry_objections(&self) -> Vec<(String, KeyObjection)> {
        windows::objections(&self.registry_values)
    }

    /// The artefact of one kind, if the plan has one.
    #[must_use]
    pub fn artefact(&self, kind: ArtefactKind) -> Option<&Artefact> {
        self.artefacts.iter().find(|a| a.kind == kind)
    }
}

/// Everything a registration for `selection` would consist of, on `platform`.
///
/// Pure: no environment, no filesystem, no clock. `platform` is a parameter
/// rather than a `cfg`, so the Windows plan is built and asserted by the Linux
/// leg of CI and the Linux plan by the Windows leg -- which is the only way
/// the `.reg` shape and the ADR-0012 guard get tested at all on a machine that
/// has no registry.
#[must_use]
pub fn plan(
    platform: Platform,
    app: &AppInfo,
    selection: &AssociationSelection,
) -> RegistrationPlan {
    let mut artefacts = Vec::new();
    let mut registry_values = Vec::new();
    let mut follow_up = Vec::new();

    let handoff = match platform {
        Platform::Linux => {
            artefacts.push(Artefact {
                relative_path: format!("applications/{}", desktop::desktop_file_name(app)),
                contents: desktop::desktop_entry(app, selection),
                kind: ArtefactKind::DesktopEntry,
            });
            follow_up.push(FollowUp {
                program: "update-desktop-database",
                args: vec!["applications".to_owned()],
                why: "Until the desktop database is rebuilt, the new entry does not appear in \
                      Open With.",
            });
            if let Some(package) = desktop::mime_package(selection) {
                artefacts.push(Artefact {
                    relative_path: format!(
                        "mime/packages/{}",
                        desktop::mime_package_file_name(app)
                    ),
                    contents: package,
                    kind: ArtefactKind::MimePackage,
                });
                follow_up.push(FollowUp {
                    program: "update-mime-database",
                    args: vec!["mime".to_owned()],
                    why: "Until the MIME database is rebuilt, the product's own file types have \
                          no MIME type and no association can match them.",
                });
            }
            Handoff::LinuxSetDefault {
                program: "xdg-mime",
                args: std::iter::once("default".to_owned())
                    .chain(std::iter::once(desktop::desktop_file_name(app)))
                    .chain(selection.mime_types().into_iter().map(str::to_owned))
                    .collect(),
            }
        }
        Platform::Windows => {
            registry_values = windows::registry_values(app, selection);
            artefacts.push(Artefact {
                relative_path: format!("{}-file-types.reg", app.app_id),
                contents: windows::registry_script(&registry_values),
                kind: ArtefactKind::RegistryScript,
            });
            Handoff::WindowsDefaultApps {
                uri: "ms-settings:defaultapps",
            }
        }
    };

    RegistrationPlan {
        platform,
        selection: selection.clone(),
        artefacts,
        registry_values,
        follow_up,
        handoff,
    }
}

/// Whether the caller has been told what will happen and said yes.
///
/// A two-variant enum rather than a `bool`, because `install(&plan, root,
/// true)` at a call site says nothing about what the `true` means, and the one
/// function in this crate that changes a machine is the one where a reader
/// should not have to check the signature. There is deliberately no `Default`:
/// consent that arrives by default is not consent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Consent {
    /// The user has seen what would be written and agreed to it.
    Granted,
    /// They have not. [`install`] refuses.
    Withheld,
}

/// What an install actually did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    /// The files written, in the order they were written, as absolute paths
    /// under the root that was given.
    pub written: Vec<PathBuf>,
    /// What still has to run. Carried forward from the plan so the caller does
    /// not have to hold both.
    pub follow_up: Vec<FollowUp>,
}

/// Why an install did not happen.
///
/// Refusals, not errors, for everything except [`Self::Failed`]: they are
/// answers the caller can act on rather than things that went wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallRefusal {
    /// [`Consent::Withheld`]. The first thing checked, before the plan is even
    /// examined, so that a refused install cannot fail for some other reason
    /// and hide the fact that nobody agreed to it.
    ConsentWithheld,
    /// The plan claims no file types. Writing it would be a confusing no-op.
    NothingSelected,
    /// This platform cannot install through this crate. The reason comes from
    /// the capability register, so the refusal and the greyed-out menu item say
    /// the same thing.
    NotSupported {
        platform: Platform,
        reason: &'static str,
    },
    /// An artefact's relative path is not one this crate will write: absolute,
    /// empty, or containing `..`. Impossible for a plan built by [`plan`] and
    /// checked anyway, because a plan is a value and values arrive from
    /// elsewhere.
    UnsafeArtefactPath { relative_path: String },
    /// The filesystem said no.
    Failed { path: PathBuf, message: String },
}

impl InstallRefusal {
    /// A sentence for the user.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::ConsentWithheld => {
                "Nothing was changed: registration was not confirmed.".to_owned()
            }
            Self::NothingSelected => "Nothing was changed: no file types were selected.".to_owned(),
            Self::NotSupported { platform, reason } => {
                format!(
                    "Registration cannot be applied on {}. {reason}",
                    platform.token()
                )
            }
            Self::UnsafeArtefactPath { relative_path } => format!(
                "Nothing was changed: \"{relative_path}\" is not a path this application will \
                 write to."
            ),
            Self::Failed { path, message } => {
                format!("Could not write {}: {message}", path.display())
            }
        }
    }
}

/// Write a plan's artefacts under `root`.
///
/// **The only function in this crate that changes anything.** It writes the
/// `.desktop` entry and the MIME package into the places the desktop
/// environment reads, which is what makes the product appear in Open With.
/// It does not set any default, does not run any command, and does not touch
/// the registry.
///
/// `root` is a parameter with no default, and that is the mechanism by which
/// no test can reach a real user directory: a test passes a `tempfile`
/// directory, the shell passes [`install_root`]. There is no code path that
/// picks one for you.
///
/// # Errors
///
/// Returns [`InstallRefusal`] rather than writing when consent is withheld,
/// the selection is empty, the platform has no supported install (Windows --
/// see the module docs), an artefact path is not one this crate will write, or
/// the filesystem refuses. Nothing partial is cleaned up: a failure part-way
/// through leaves the files already written, because deleting files during
/// error handling in a directory the user named is a worse risk than an
/// incomplete registration that the next attempt overwrites.
pub fn install(
    plan: &RegistrationPlan,
    root: &Path,
    consent: Consent,
) -> Result<Installed, InstallRefusal> {
    if consent != Consent::Granted {
        return Err(InstallRefusal::ConsentWithheld);
    }
    if plan.selection.is_empty() {
        return Err(InstallRefusal::NothingSelected);
    }
    if let Availability::NotImplemented { reason } | Availability::Unsupported { reason } =
        Capability::AssociationInstall.availability(plan.platform)
    {
        return Err(InstallRefusal::NotSupported {
            platform: plan.platform,
            reason,
        });
    }

    // Every path is checked before anything is written, so a plan with one bad
    // artefact writes none of them rather than half of them.
    for artefact in &plan.artefacts {
        if !is_safe_relative_path(&artefact.relative_path) {
            return Err(InstallRefusal::UnsafeArtefactPath {
                relative_path: artefact.relative_path.clone(),
            });
        }
    }

    let mut written = Vec::new();
    for artefact in &plan.artefacts {
        let target = artefact
            .relative_path
            .split('/')
            .fold(root.to_path_buf(), |acc, part| acc.join(part));
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|error| InstallRefusal::Failed {
                path: parent.to_path_buf(),
                message: error.to_string(),
            })?;
        }
        fs::write(&target, &artefact.contents).map_err(|error| InstallRefusal::Failed {
            path: target.clone(),
            message: error.to_string(),
        })?;
        written.push(target);
    }

    Ok(Installed {
        written,
        follow_up: plan.follow_up.clone(),
    })
}

/// The directory [`install`] should be given on this platform.
///
/// `$XDG_DATA_HOME` on Linux -- the per-user half of the shared data
/// hierarchy, which is where a `.desktop` entry belongs and is why nothing
/// here ever needs to write outside the user's own home. `None` on Windows,
/// which has no supported install through this crate at all.
///
/// Takes the environment as a value like everything else in this crate, so a
/// test can ask what the answer *would* be for an environment it makes up.
#[must_use]
pub fn install_root(platform: Platform, env: &EnvSnapshot) -> Option<PathBuf> {
    match platform {
        Platform::Linux => crate::dirs::root(platform, DirKind::Data, env),
        Platform::Windows => None,
    }
}

/// What currently opens the selected file types, read from `config_dir`.
///
/// The read half of registration, and the one that needs no permission. On
/// Linux `config_dir` is `$XDG_CONFIG_HOME` -- the directory holding
/// `mimeapps.list` -- and a missing file is not an error: a user who has never
/// chosen a handler has none recorded, which is
/// [`AssociationState::Unclaimed`], not a failure.
///
/// On Windows it returns [`AssociationState::Unknown`] for everything, with
/// the capability register's own reason attached. This crate cannot read
/// `HKCU\...\UserChoice` without a dependency, and reporting "unclaimed"
/// instead would tell the user their machine is unconfigured when it is
/// merely unreadable from here.
#[must_use]
pub fn association_report(
    platform: Platform,
    config_dir: Option<&Path>,
    app: &AppInfo,
    selection: &AssociationSelection,
) -> AssociationReport {
    match platform {
        Platform::Linux => {
            let text = config_dir
                .map(|dir| dir.join("mimeapps.list"))
                .and_then(|path| fs::read_to_string(path).ok())
                .unwrap_or_default();
            state::report_from_mimeapps(&text, app, selection)
        }
        Platform::Windows => state::unknown_report(
            Capability::AssociationQuery
                .availability(platform)
                .reason()
                .unwrap_or("This platform cannot be queried."),
            selection,
        ),
    }
}

/// Whether this crate will write to `relative_path` under a given root.
///
/// The rules are the boring ones and they are all load-bearing: not empty, not
/// absolute on *either* platform (a plan built for one may be inspected on the
/// other), no `.` or `..` component, and every component a legal file name on
/// both. Together they mean the written path is under the root the caller
/// named and nowhere else.
fn is_safe_relative_path(relative_path: &str) -> bool {
    if relative_path.trim().is_empty() {
        return false;
    }
    if Platform::ALL
        .iter()
        .any(|&platform| is_absolute(platform, relative_path))
    {
        return false;
    }
    // Split on both separators whatever the platform, so a backslash cannot
    // hide a `..` from a check done with forward slashes.
    let components: Vec<&str> = relative_path
        .split(['/', '\\'])
        .filter(|part| !part.is_empty())
        .collect();
    !components.is_empty()
        && components
            .iter()
            .all(|part| paths::is_portable_file_name(part))
}

#[cfg(test)]
mod tests;
