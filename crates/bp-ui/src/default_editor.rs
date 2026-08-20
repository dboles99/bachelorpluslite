//! File ▸ Set as Default Editor: the shell's half of ADR-0012.
//!
//! Every decision here belongs to `bp-platform`. Which file types may be
//! claimed, what a `.desktop` entry and a `.reg` script contain, which
//! registry keys this product refuses to write, whether the platform can
//! apply a registration at all, and where the artefacts go on the one
//! platform that can -- all of it is answered there, by functions that take
//! the platform as a parameter and are therefore tested by both legs of CI.
//!
//! What is left for the shell is the part that is genuinely the shell's:
//! reading what opens these file types **today** and showing it before
//! offering to change anything, putting the whole plan in front of the user,
//! and turning their answer into the one [`Consent`] value
//! `bp_platform::editor::install` will accept.
//!
//! Three things this module deliberately does not do.
//!
//! * **It picks no destination root of its own.** `install` takes an explicit
//!   root because there is no code path in `bp-platform` that chooses one for
//!   you; the answer comes from
//!   [`install_root`](bp_platform::editor::install_root) and is named in the
//!   consent dialog, because a root chosen out of sight is a file written
//!   somewhere nobody agreed to.
//! * **It runs no command.** `update-desktop-database`, `xdg-mime default`
//!   and the Windows Settings page are shown as text for the user to run.
//!   That is ADR-0012's "guide the user through the OS's own default-app
//!   selection" rather than deciding for them, and it is the same reason
//!   `bp-platform` names those steps and executes none of them.
//! * **It constructs no registration of its own.** The plan comes from
//!   [`plan`](bp_platform::editor::plan) and is passed through
//!   [`registry_objection`] before it is offered, so a key belonging to
//!   another application -- Notepad's above all -- cannot reach a Save
//!   dialog even if a plan arrived from somewhere unexpected.

use std::path::{Path, PathBuf};

use bp_platform::editor::{
    AppInfo, ArtefactKind, AssociationPreset, AssociationReport, AssociationSelection,
    AssociationState, Handoff, InstallRefusal, Installed, RegistrationPlan, association_report,
    install_root,
};
use bp_platform::{Capability, DirKind, EnvSnapshot, Platform};

/// The preset one click on the row registers for.
///
/// The smallest useful one -- `bp_platform` calls it "the answer that
/// surprises nobody" -- and it is named on the menu row rather than chosen
/// out of sight. specs.md section 19 wants all five offered; that wants a
/// settings screen, because five more rows in the File menu is a worse answer
/// than one row that says which preset it means.
const PRESET: AssociationPreset = AssociationPreset::NotepadReplacement;

/// Everything the user has to see before a single file is written.
#[derive(Debug)]
pub(crate) struct Offer {
    /// What `install` would be handed. Built by `bp_platform`, never here.
    pub(crate) plan: RegistrationPlan,
    /// Where `install` would write. `None` on Windows, which has no supported
    /// install at all -- `install_root` says so, and the shell does not
    /// second-guess it with a root of its own.
    pub(crate) root: Option<PathBuf>,
    /// The consent dialog: what opens these types today, then exactly what
    /// would change, then what would still be left for the user to do.
    pub(crate) body: String,
}

/// Build the offer for this machine.
///
/// The executable path is read from the running process because only the
/// running process knows it: a registration naming a hard-coded install
/// directory launches the wrong program for anybody who ran a portable build.
pub(crate) fn offer() -> Result<Offer, String> {
    let executable = std::env::current_exe().map_err(|e| {
        format!(
            "cannot register file types — this application cannot find its own executable ({e})"
        )
    })?;
    let app = AppInfo::bachelorpad(executable.display().to_string());
    offer_for(Platform::HOST, &app, &EnvSnapshot::from_environment())
}

/// The offer for a named platform and a named environment.
///
/// Everything [`offer`] does once the two facts about *this* process have
/// been read. Split out so a test can build the Windows offer while standing
/// on Linux and vice versa, and -- the part that matters more -- so no test
/// ever has to point this at a real user directory: the environment is a
/// value, so `tempfile` roots reach every path this function follows.
pub(crate) fn offer_for(
    platform: Platform,
    app: &AppInfo,
    env: &EnvSnapshot,
) -> Result<Offer, String> {
    let problems = app.problems(platform);
    if !problems.is_empty() {
        return Err(format!(
            "cannot register file types — {}",
            problems.join(" ")
        ));
    }

    let selection = AssociationSelection::preset(PRESET);
    let registration = bp_platform::editor::plan(platform, app, &selection);

    // ADR-0012 as a check rather than as a paragraph. `bp-platform` will not
    // build such a plan and re-checks before it writes; the shell checks too,
    // because the shell is the thing that would otherwise hand a `.reg` to a
    // Save dialog, and that path never goes through `install` at all.
    if let Some(objection) = registry_objection(&registration) {
        return Err(objection);
    }

    let root = install_root(platform, env);
    if platform == Platform::Linux && root.is_none() {
        return Err(
            "cannot register file types — this environment does not say where the user's data \
             directory is, so there is nowhere to put a .desktop entry"
                .to_owned(),
        );
    }

    // Read before offering, and read rather than assumed. On Windows the
    // answer is "cannot tell", which is not the same fact as "nothing is
    // registered": rendering the second from the first invites the user to
    // fix something that may not be broken.
    let report = association_report(
        platform,
        bp_platform::dirs::root(platform, DirKind::Config, env).as_deref(),
        app,
        &selection,
    );

    let body = body(&report, &registration, root.as_deref());
    Ok(Offer {
        plan: registration,
        root,
        body,
    })
}

/// The consent dialog: the state of play, then the change, then what is left.
///
/// Pure over its arguments, so the wording is testable for both platforms
/// from either one.
fn body(report: &AssociationReport, plan: &RegistrationPlan, root: Option<&Path>) -> String {
    let mut out = report.summary();
    out.push_str("\n\n");

    // The one case `unknown_report` exists to let a UI special-case: when
    // nothing could be read, every row carries the identical reason, and
    // seven copies of a four-line explanation is a dialog nobody finishes.
    // The summary above already says "cannot tell which application opens
    // the 7 selected file types"; what is left to add is *why*, once.
    let unreadable = report
        .entries()
        .iter()
        .filter_map(|entry| match &entry.state {
            AssociationState::Unknown { reason } => Some(*reason),
            _ => None,
        })
        .next()
        .filter(|_| {
            report
                .entries()
                .iter()
                .all(|entry| matches!(entry.state, AssociationState::Unknown { .. }))
        });
    match unreadable {
        Some(reason) => {
            out.push_str(reason);
            out.push('\n');
        }
        None => {
            for entry in report.entries() {
                out.push_str("  ");
                out.push_str(&entry.state.describe(entry.extension));
                out.push('\n');
            }
        }
    }

    let claimed: Vec<String> = plan
        .selection
        .extensions()
        .map(|extension| format!(".{extension}"))
        .collect();
    out.push_str(&format!(
        "\nRegistering claims {} file types: {}\n",
        claimed.len(),
        claimed.join(", ")
    ));

    match plan.platform {
        Platform::Linux => {
            out.push_str(&format!(
                "\nIt writes {} files under {}:\n",
                plan.artefacts.len(),
                root.map_or_else(|| "(nowhere)".to_owned(), |r| r.display().to_string()),
            ));
            for artefact in &plan.artefacts {
                out.push_str(&format!("  {}\n", artefact.relative_path));
            }
            out.push_str(
                "\nIt sets no default and takes no file type away from anything else. These \
                 still have to run:\n",
            );
            for step in &plan.follow_up {
                out.push_str(&format!("  {}\n      {}\n", step.command_line(), step.why));
            }
        }
        Platform::Windows => {
            // The register's own sentence, not a second one written here: a
            // greyed row, a refusal and this dialog all have to say the same
            // thing about the same obstacle or one of them is wrong.
            out.push_str(&format!(
                "\n{}\n",
                cannot_apply(Platform::Windows).describe()
            ));
            out.push_str(&format!(
                "\nA registry script can be saved instead, for you to read before you run it: \
                 {} values, every one under HKEY_CURRENT_USER. It adds BachelorPad+ to the Open \
                 With list and takes no file type away from any other application (ADR-0012).\n",
                plan.registry_values.len()
            ));
        }
    }

    out.push('\n');
    out.push_str(&handoff_note(&plan.handoff));
    out.push('\n');
    out
}

/// The sentence and the address the user needs once the artefacts exist.
///
/// `Handoff::describe` names the Settings page on Windows but not how to
/// reach it, and this product does not open it for them -- launching the
/// user's settings on their behalf is the same shape of decision ADR-0012
/// declines to make. The URI is `bp-platform`'s constant, printed so it can
/// be pasted into Run.
pub(crate) fn handoff_note(handoff: &Handoff) -> String {
    let mut note = handoff.describe();
    if let Handoff::WindowsDefaultApps { uri } = handoff {
        note.push_str(&format!("\n\nSettings page: {uri}"));
    }
    note
}

/// Why this registration plan must not be written, if it must not.
///
/// `bp_platform::editor::RegistrationPlan::registry_objections` is public
/// precisely so that whatever *applies* a plan checks the plan rather than
/// trusting where it came from. The Windows path here never goes through
/// `install`, so this is the only guard between a plan and a `.reg` file on
/// the user's disk.
pub(crate) fn registry_objection(plan: &RegistrationPlan) -> Option<String> {
    plan.registry_objections().first().map(|(key, why)| {
        format!(
            "nothing was written — this registration would write {key}: {}",
            why.describe()
        )
    })
}

/// The refusal this platform's capability register produces for an install.
///
/// Built from [`Capability::AssociationInstall`] rather than written out, so
/// the dialog, the status bar and whatever `install` itself returns are one
/// string with one place to change it.
pub(crate) fn cannot_apply(platform: Platform) -> InstallRefusal {
    InstallRefusal::NotSupported {
        platform,
        reason: Capability::AssociationInstall
            .availability(platform)
            .reason()
            .unwrap_or("This platform has no supported install."),
    }
}

/// What to say once `install` has been asked to run.
///
/// Two values, because the status bar elides and a command the user still has
/// to type is the last thing to put somewhere that elides. `None` for the
/// second means there is nothing more to show than the line itself.
pub(crate) fn install_outcome(
    result: Result<Installed, InstallRefusal>,
    handoff: &Handoff,
    root: &Path,
) -> (String, Option<String>) {
    match result {
        Ok(installed) => {
            let mut body = format!(
                "Wrote {} files under {}:\n",
                installed.written.len(),
                root.display()
            );
            for path in &installed.written {
                body.push_str(&format!("  {}\n", path.display()));
            }
            // Said again after the fact, not only before it: this is the
            // moment a user concludes the job is done, and on Linux it is not
            // -- a correct `.desktop` file that appears to do nothing is the
            // single most common outcome of skipping these.
            body.push_str("\nNothing is the default yet. These still have to run:\n");
            for step in &installed.follow_up {
                body.push_str(&format!("  {}\n      {}\n", step.command_line(), step.why));
            }
            body.push('\n');
            body.push_str(&handoff_note(handoff));
            (
                format!(
                    "wrote {} registration files under {} — nothing is the default until the \
                     commands in the dialog have run",
                    installed.written.len(),
                    root.display()
                ),
                Some(body),
            )
        }
        // Every refusal words itself, including the one about this platform
        // having no install: `InstallRefusal::describe` is where that
        // sentence lives.
        Err(refusal) => (refusal.describe(), None),
    }
}

/// Write the plan's `.reg` script where the user chose to put it.
///
/// The Windows answer to a registration this product will not apply: a file
/// the user picked the location of and can read every line of before running
/// it, which ADR-0012 argues is a better answer than an installer writing the
/// same keys invisibly.
pub(crate) fn save_registry_script(
    plan: &RegistrationPlan,
    target: &Path,
) -> Result<String, String> {
    if let Some(objection) = registry_objection(plan) {
        return Err(objection);
    }
    let Some(artefact) = plan.artefact(ArtefactKind::RegistryScript) else {
        return Err(
            "nothing was written — this registration has no registry script in it".to_owned(),
        );
    };
    std::fs::write(target, &artefact.contents)
        .map_err(|e| format!("could not write {} — {e}", target.display()))?;
    Ok(format!(
        "wrote {} — {} values, every one under HKEY_CURRENT_USER; read it before you run it",
        target.display(),
        plan.registry_values.len()
    ))
}

/// The file name to suggest for the `.reg` script.
///
/// `bp-platform`'s own, so the file the user saves is the file the plan
/// names.
pub(crate) fn registry_script_name(plan: &RegistrationPlan) -> String {
    plan.artefact(ArtefactKind::RegistryScript)
        .map_or_else(|| "file-types.reg".to_owned(), |a| a.relative_path.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bp_platform::editor::Consent;

    /// The application, described for a named platform.
    ///
    /// The executable has to be absolute in *that* platform's spelling, which
    /// is the one thing about a registration that cannot be shared between
    /// them: `AppInfo::problems` refuses a relative path, and a relative path
    /// is what a Windows path looks like to the Linux rules.
    fn app_for(platform: Platform) -> AppInfo {
        AppInfo::bachelorpad(match platform {
            Platform::Windows => r"C:\Program Files\BachelorPad\bachelorpad.exe",
            Platform::Linux => "/usr/bin/bachelorpad",
        })
    }

    /// An environment whose every root is inside `dir`.
    ///
    /// Used only with [`Platform::HOST`], because a temporary directory is
    /// absolute in the host's spelling and in no other: `C:\Users\...` is a
    /// relative path to the XDG rules, and `/tmp/...` is a path on whatever
    /// drive happens to be current on Windows. Everything that has to be
    /// asserted for *both* platforms goes through [`body_for`] instead, which
    /// touches no filesystem at all.
    fn env_under(dir: &Path) -> EnvSnapshot {
        let at = |name: &str| Some(dir.join(name).display().to_string());
        EnvSnapshot {
            appdata: at("roaming"),
            local_appdata: at("local"),
            user_profile: at("profile"),
            home: at("home"),
            xdg_config_home: at("config"),
            xdg_data_home: at("data"),
            xdg_cache_home: at("cache"),
            xdg_state_home: at("state"),
        }
    }

    /// The consent dialog either platform would show, built from values.
    ///
    /// No environment, no filesystem and no clock, so the Windows wording is
    /// asserted by the Linux leg of CI and the Linux wording by the Windows
    /// leg -- which is the same reason `bp-platform` takes the platform as a
    /// parameter, applied one layer up.
    fn body_for(platform: Platform, mimeapps: &str) -> String {
        let app = app_for(platform);
        let selection = AssociationSelection::preset(PRESET);
        let plan = bp_platform::editor::plan(platform, &app, &selection);
        let (report, root) = match platform {
            Platform::Linux => (
                bp_platform::editor::state::report_from_mimeapps(mimeapps, &app, &selection),
                Some(PathBuf::from("/home/tester/.local/share")),
            ),
            // `association_report` on Windows reads nothing at all -- it is
            // the "cannot tell" report, and that is the whole point of it.
            Platform::Windows => (association_report(platform, None, &app, &selection), None),
        };
        body(&report, &plan, root.as_deref())
    }

    #[test]
    fn the_dialog_says_what_opens_these_file_types_before_it_offers_to_change_anything() {
        // ADR-0012 is about the user keeping control. A user cannot keep
        // control of a change they were not shown the starting point of.
        let body = body_for(Platform::Linux, "");
        assert!(
            body.contains("Nothing is set to open .txt files."),
            "an empty mimeapps.list means nothing claims .txt; got:\n{body}"
        );
        assert!(
            body.contains("Registering claims"),
            "and the dialog must say what would be claimed; got:\n{body}"
        );
    }

    #[test]
    fn an_existing_association_is_reported_as_the_application_that_holds_it() {
        let body = body_for(
            Platform::Linux,
            "[Default Applications]\ntext/plain=org.gnome.gedit.desktop;\n",
        );
        assert!(
            body.contains("org.gnome.gedit.desktop opens .txt files."),
            "the current handler has to be named; got:\n{body}"
        );
    }

    #[test]
    fn windows_reports_that_it_cannot_tell_rather_than_that_nothing_is_registered() {
        // The distinction `bp-platform` refuses to collapse: `Unknown` is not
        // `Unclaimed`, and a dialog rendering "not the default" out of "could
        // not look" invites the user to fix something that is not broken.
        let body = body_for(Platform::Windows, "");
        assert!(
            body.contains("Cannot tell which application opens the 7 selected file types."),
            "Windows cannot read the association; got:\n{body}"
        );
        assert!(
            !body.contains("Nothing is set to open"),
            "'cannot tell' must never be rendered as 'unclaimed'; got:\n{body}"
        );
        // The reason, once. Every row carries the identical four-line
        // explanation, and seven copies of it is a dialog nobody finishes --
        // which is what `unknown_report` exists to let a UI special-case.
        assert_eq!(
            body.matches("UserChoice").count(),
            1,
            "the obstacle should be stated once; got:\n{body}"
        );
    }

    #[test]
    fn the_linux_dialog_names_the_root_it_would_write_under() {
        // The one thing that must not be out of sight: `install` takes an
        // explicit root because nothing in `bp-platform` picks one, and a
        // root the dialog does not name is one picked out of sight anyway.
        let body = body_for(Platform::Linux, "");
        assert!(
            body.contains("/home/tester/.local/share"),
            "the destination has to be on screen; got:\n{body}"
        );
        // Derived rather than spelt, so that ADR-0032's application id can
        // change without this quietly becoming a test that the dialog names
        // *a* file.
        let entry = format!(
            "applications/{}",
            bp_platform::editor::desktop::desktop_file_name(&app_for(Platform::Linux))
        );
        assert!(body.contains(&entry), "and so do the files; got:\n{body}");
    }

    #[test]
    fn the_windows_refusal_is_the_library_s_own_words_and_not_ours() {
        // The load-bearing one. The refusal string is deliberately identical
        // to the capability register's, so a greyed row, a status-bar line
        // and `install`'s own answer cannot drift apart. This asserts the
        // shell's copy *is* `install`'s copy, by asking `install` for it.
        let dir = tempfile::tempdir().expect("a temporary directory");
        let selection = AssociationSelection::preset(PRESET);
        let plan =
            bp_platform::editor::plan(Platform::Windows, &app_for(Platform::Windows), &selection);

        let refused = bp_platform::editor::install(&plan, dir.path(), Consent::Granted)
            .expect_err("Windows has no supported install");

        assert_eq!(
            cannot_apply(Platform::Windows).describe(),
            refused.describe(),
            "the shell must not word this refusal itself"
        );
        assert!(
            body_for(Platform::Windows, "").contains(&refused.describe()),
            "and the dialog has to carry that same sentence"
        );
        assert_eq!(
            std::fs::read_dir(dir.path())
                .expect("the root is readable")
                .count(),
            0,
            "a refused install must write nothing"
        );
    }

    #[test]
    fn a_plan_that_would_touch_another_application_is_refused_before_it_is_offered() {
        // `bp-platform` never builds one, which is exactly why the guard is
        // worth having: the plan the shell hands to a Save dialog does not go
        // through `install`, so this is the only thing standing between
        // ADR-0012 and a `.reg` file on the user's disk.
        use bp_platform::editor::{RegistryValue, ValueName};

        let selection = AssociationSelection::preset(PRESET);
        let mut plan =
            bp_platform::editor::plan(Platform::Windows, &app_for(Platform::Windows), &selection);
        assert_eq!(
            registry_objection(&plan),
            None,
            "a plan bp-platform built has nothing objectionable in it"
        );

        plan.registry_values.push(RegistryValue {
            key: r"HKCU\Software\Classes\txtfile\shell\open\command".to_owned(),
            name: ValueName::Default,
            data: "notepad.exe".to_owned(),
        });
        let objection = registry_objection(&plan).expect("another application's ProgID");
        assert!(
            objection.contains("txtfile"),
            "the refusal must name the key; got '{objection}'"
        );

        let dir = tempfile::tempdir().expect("a temporary directory");
        let target = dir.path().join("out.reg");
        assert!(
            save_registry_script(&plan, &target).is_err(),
            "an objectionable plan must not reach a file"
        );
        assert!(
            !target.exists(),
            "and must not have written one on the way to refusing"
        );
    }

    #[test]
    fn the_registry_script_is_written_where_the_user_chose_and_nowhere_else() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let selection = AssociationSelection::preset(PRESET);
        let plan =
            bp_platform::editor::plan(Platform::Windows, &app_for(Platform::Windows), &selection);
        let target = dir.path().join(registry_script_name(&plan));

        let status = save_registry_script(&plan, &target).expect("the script is written");
        let written = std::fs::read_to_string(&target).expect("the script is readable");

        assert!(
            written.starts_with("Windows Registry Editor Version 5.00"),
            "regedit refuses a file without that header"
        );
        assert!(
            !written.to_ascii_lowercase().contains("notepad"),
            "ADR-0012: this product never names Notepad in a registry script"
        );
        assert!(
            written
                .lines()
                .filter(|line| line.starts_with('['))
                .all(|line| line.starts_with("[HKEY_CURRENT_USER\\")),
            "every key must be under the user hive"
        );
        assert!(
            status.contains("read it before you run it"),
            "got '{status}'"
        );
    }

    #[test]
    fn installing_writes_only_under_the_root_it_was_given() {
        // `install` consults no environment: the root is the argument, which
        // is what lets this run on either host and touch nothing but a
        // temporary directory.
        let dir = tempfile::tempdir().expect("a temporary directory");
        let selection = AssociationSelection::preset(PRESET);
        let plan =
            bp_platform::editor::plan(Platform::Linux, &app_for(Platform::Linux), &selection);

        let (status, body) = install_outcome(
            bp_platform::editor::install(&plan, dir.path(), Consent::Granted),
            &plan.handoff,
            dir.path(),
        );
        let body = body.expect("a successful install has something to show");

        assert!(
            dir.path()
                .join("applications")
                .join(bp_platform::editor::desktop::desktop_file_name(&app_for(
                    Platform::Linux
                )))
                .is_file(),
            "the desktop entry belongs under the given root"
        );
        assert!(status.contains("nothing is the default"), "got '{status}'");
        assert!(
            body.contains("update-desktop-database applications"),
            "the command that finishes the job must be named; got:\n{body}"
        );
        assert!(
            body.contains("xdg-mime default"),
            "so must the one that actually sets the default; got:\n{body}"
        );
    }

    #[test]
    fn withheld_consent_writes_nothing_and_says_so_in_the_library_s_words() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let selection = AssociationSelection::preset(PRESET);
        let plan =
            bp_platform::editor::plan(Platform::Linux, &app_for(Platform::Linux), &selection);

        let (status, body) = install_outcome(
            bp_platform::editor::install(&plan, dir.path(), Consent::Withheld),
            &plan.handoff,
            dir.path(),
        );

        assert_eq!(status, InstallRefusal::ConsentWithheld.describe());
        assert!(body.is_none(), "there is nothing to list");
        assert_eq!(
            std::fs::read_dir(dir.path())
                .expect("the root is readable")
                .count(),
            0,
            "consent withheld must leave the disk untouched"
        );
    }

    #[test]
    fn the_linux_handoff_shows_the_command_rather_than_claiming_the_job_is_done() {
        let selection = AssociationSelection::preset(PRESET);
        let plan =
            bp_platform::editor::plan(Platform::Linux, &app_for(Platform::Linux), &selection);
        let note = handoff_note(&plan.handoff);

        assert!(note.contains("xdg-mime default"), "got '{note}'");
        assert!(
            note.contains("To make it the default"),
            "appearing in Open With is not being the default; got '{note}'"
        );
    }

    #[test]
    fn the_windows_handoff_carries_the_settings_address_because_nothing_opens_it_for_the_user() {
        let selection = AssociationSelection::preset(PRESET);
        let plan =
            bp_platform::editor::plan(Platform::Windows, &app_for(Platform::Windows), &selection);
        let note = handoff_note(&plan.handoff);

        assert!(note.contains("ms-settings:defaultapps"), "got '{note}'");
    }

    #[test]
    fn the_offer_takes_its_root_from_the_platform_rather_than_choosing_one() {
        // The host leg: everything above is built from values, and this is
        // the one test that walks the real `offer_for` path -- against an
        // environment made of temporary directories, so nothing the user owns
        // is read or written.
        let dir = tempfile::tempdir().expect("a temporary directory");
        let env = env_under(dir.path());
        let offer = offer_for(Platform::HOST, &app_for(Platform::HOST), &env)
            .expect("the host can always describe its own registration");

        assert_eq!(
            offer.root,
            install_root(Platform::HOST, &env),
            "the root is bp-platform's answer, never one invented here"
        );
        assert!(
            offer.body.contains("Registering claims 7 file types"),
            "got:\n{}",
            offer.body
        );
    }

    #[test]
    fn a_registration_naming_a_relative_executable_is_refused_before_anything_is_planned() {
        // The failure this prevents surfaces as "double-clicking a text file
        // does nothing", which is about as far from its cause as a defect
        // gets. `AppInfo::problems` names it; the shell just has to ask.
        let dir = tempfile::tempdir().expect("a temporary directory");
        let app = AppInfo::bachelorpad("bachelorpad");
        let error = offer_for(Platform::HOST, &app, &env_under(dir.path()))
            .expect_err("a relative executable cannot be registered");

        assert!(error.contains("relative"), "got '{error}'");
    }

    #[test]
    fn linux_without_a_home_directory_says_so_rather_than_writing_somewhere_arbitrary() {
        let error = offer_for(
            Platform::Linux,
            &app_for(Platform::Linux),
            &EnvSnapshot::default(),
        )
        .expect_err("there is nowhere to put a .desktop entry");

        assert!(error.contains("nowhere to put"), "got '{error}'");
    }

    #[test]
    fn no_message_this_module_writes_carries_a_wrapped_line_s_indentation() {
        // A `\` continuation strips the newline and the leading whitespace;
        // forgetting it leaves a run of spaces in the middle of a sentence
        // the user reads. It has happened twice in this crate already.
        let messages = [
            offer_for(
                Platform::Linux,
                &app_for(Platform::Linux),
                &EnvSnapshot::default(),
            )
            .expect_err("no data directory"),
            cannot_apply(Platform::Windows).describe(),
        ];
        for message in messages {
            assert!(
                !message.contains("  "),
                "a wrapped literal leaked its indentation: {message:?}"
            );
        }
    }
}
