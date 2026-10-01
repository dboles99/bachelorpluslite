//! Tests for default-editor registration.
//!
//! Two rules shape this file. **No test installs anything anywhere but a
//! `tempfile` directory** -- there is no code path that could reach the real
//! `~/.local/share`, because [`install`] has no default root, but a test that
//! wrote one would still be wrong. And **every test runs on both legs of CI**,
//! including the Windows ones: [`plan`] takes the platform as an argument, so
//! the `.reg` shape and the ADR-0012 guard are checked on machines with no
//! registry, which is where they would otherwise never be checked at all.

use super::*;
use proptest::prelude::*;
use std::collections::BTreeMap;
use tempfile::tempdir;

fn app() -> AppInfo {
    AppInfo::bachelorpad("/usr/local/bin/bachelorpad")
}

fn windows_app() -> AppInfo {
    AppInfo::bachelorpad(r"C:\Program Files\BachelorPad+\bachelorpad.exe")
}

fn everything() -> AssociationSelection {
    AssociationSelection::preset(AssociationPreset::EverythingSupported)
}

/// The `key=value` pairs of a `.desktop` file, in order.
fn desktop_keys(entry: &str) -> Vec<(&str, &str)> {
    entry
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('['))
        .map(|line| {
            line.split_once('=')
                .unwrap_or_else(|| panic!("a .desktop line without an `=`: {line:?}"))
        })
        .collect()
}

// --- the file-type table ---------------------------------------------------

/// Whether `bp-formats` must be able to identify this extension.
///
#[test]
fn every_registerable_extension_is_one_bp_formats_can_identify() {
    // The one agreement that must hold between the registration table and the
    // parser: claiming a file type the editor cannot open is the failure the
    // user experiences as a broken machine rather than a missing feature.
    // No exemption: ADR-0069 removed the one type that had one.
    for file_type in FILE_TYPES {
        assert!(
            bp_formats::Format::from_extension(file_type.extension).is_some(),
            ".{} is offered for registration and bp-formats does not know it",
            file_type.extension
        );
    }
}

#[test]
fn extensions_are_unique_lowercase_and_undotted() {
    let mut seen: Vec<&str> = FILE_TYPES.iter().map(|t| t.extension).collect();
    let count = seen.len();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen.len(), count, "an extension is listed twice");
    for file_type in FILE_TYPES {
        assert!(!file_type.extension.starts_with('.'), "{file_type:?}");
        assert_eq!(
            file_type.extension.to_ascii_lowercase(),
            file_type.extension,
            "{file_type:?}"
        );
        assert!(!file_type.description.is_empty(), "{file_type:?}");
        assert!(file_type.mime.contains('/'), "{file_type:?}");
    }
}

#[test]
fn a_file_type_is_found_however_the_extension_is_spelled() {
    for spelling in ["txt", ".txt", ".TXT", "  Txt "] {
        assert_eq!(
            file_type(spelling).map(|t| t.extension),
            Some("txt"),
            "{spelling}"
        );
    }
    assert_eq!(file_type("docx"), None);
    assert_eq!(file_type(""), None);
    assert_eq!(file_type("."), None);
}

// --- presets ---------------------------------------------------------------

#[test]
fn the_escalating_presets_are_nested_in_the_order_they_are_offered() {
    // The same property the security profiles earn: each step adds and never
    // takes away, so a user moving down the list never loses an association
    // they had. `Developer` is deliberately excluded -- see the next test.
    let chain = [
        AssociationPreset::NotepadReplacement,
        AssociationPreset::TextAndNotes,
        AssociationPreset::TextAndStructuredData,
        AssociationPreset::EverythingSupported,
    ];
    for pair in chain.windows(2) {
        let narrower = AssociationSelection::preset(pair[0]);
        let wider = AssociationSelection::preset(pair[1]);
        for extension in narrower.extensions() {
            assert!(
                wider.contains(extension),
                "{} loses .{extension} relative to {}",
                pair[1].label(),
                pair[0].label()
            );
        }
        assert!(
            wider.len() > narrower.len(),
            "{} adds nothing to {}",
            pair[1].label(),
            pair[0].label()
        );
    }
}

#[test]
fn developer_is_not_on_the_escalating_chain_and_says_so_by_dropping_the_spreadsheets() {
    let developer = AssociationSelection::preset(AssociationPreset::Developer);
    let structured = AssociationSelection::preset(AssociationPreset::TextAndStructuredData);
    assert!(developer.contains("rs") && !structured.contains("rs"));
    assert!(structured.contains("csv") && !developer.contains("csv"));
}

#[test]
fn everything_supported_is_the_whole_table_and_the_others_are_inside_it() {
    let everything = everything();
    assert_eq!(everything.len(), FILE_TYPES.len());
    for &preset in AssociationPreset::ALL {
        let selection = AssociationSelection::preset(preset);
        assert!(!selection.is_empty(), "{} is empty", preset.label());
        // Every preset opens plain text -- whatever else a user picked, a
        // text editor that could not be asked to open a .txt is not one.
        assert!(selection.contains("txt"), "{}", preset.label());
        for extension in selection.extensions() {
            assert!(everything.contains(extension), "{extension}");
        }
    }
}

#[test]
fn a_preset_round_trips_through_its_token_and_recognises_itself() {
    for &preset in AssociationPreset::ALL {
        assert_eq!(AssociationPreset::parse(preset.token()), Some(preset));
        assert_eq!(
            AssociationSelection::preset(preset).matching_preset(),
            Some(preset),
            "{}",
            preset.label()
        );
    }
    assert_eq!(AssociationPreset::parse("custom"), None);
}

#[test]
fn a_custom_selection_drops_what_the_editor_cannot_open() {
    let selection = AssociationSelection::custom(["txt", ".MD", "docx", "xlsx"]);
    assert_eq!(
        selection.extensions().collect::<Vec<_>>(),
        vec!["md", "txt"]
    );
    assert_eq!(
        unknown_extensions(&["txt", ".MD", "docx", "xlsx"]),
        vec!["docx", "xlsx"]
    );
    // A custom selection that happens to equal a preset is reported as that
    // preset rather than as "Custom".
    assert_eq!(selection.matching_preset(), None);
}

#[test]
fn the_mime_list_is_deduplicated() {
    // Six extensions map to text/plain; a MimeType line that repeats it six
    // times fails desktop-file-validate.
    let mimes = everything().mime_types();
    let mut sorted = mimes.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), mimes.len());
    assert!(mimes.contains(&"text/plain"));
}

// --- the .desktop entry ----------------------------------------------------

#[test]
fn the_desktop_entry_has_the_keys_a_desktop_environment_looks_for() {
    let entry = desktop::desktop_entry(&app(), &everything());
    assert!(entry.starts_with("[Desktop Entry]\n"));
    let keys: Vec<&str> = desktop_keys(&entry).into_iter().map(|(k, _)| k).collect();
    for required in ["Type", "Name", "Exec", "MimeType", "Icon", "Categories"] {
        assert!(keys.contains(&required), "no {required} key in\n{entry}");
    }
    assert!(entry.contains("Exec=/usr/local/bin/bachelorpad %F"));
    // The product owns no type of its own (ADR-0069), so the list is entirely
    // types the distribution already defines, and text/plain leads it.
    assert!(entry.contains("MimeType=text/plain;"), "{entry}");
    assert!(entry.contains("Categories=Utility;TextEditor;\n"));
}

#[test]
fn an_empty_selection_still_produces_an_entry_but_claims_nothing() {
    let entry = desktop::desktop_entry(&app(), &AssociationSelection::default());
    assert!(!entry.contains("MimeType"));
    assert!(entry.contains("Exec="));
}

#[test]
fn a_path_with_a_space_is_quoted_in_exec_and_not_in_tryexec() {
    let mut app = app();
    app.executable = "/opt/BachelorPad Plus/bachelorpad".to_owned();
    let entry = desktop::desktop_entry(&app, &everything());
    assert!(
        entry.contains("Exec=\"/opt/BachelorPad Plus/bachelorpad\" %F"),
        "{entry}"
    );
    // TryExec is a plain path, not an Exec value, so it is not quoted.
    assert!(entry.contains("TryExec=/opt/BachelorPad Plus/bachelorpad"));
}

#[test]
fn a_windows_style_path_survives_both_layers_of_escaping() {
    // The classic defect: the Exec value is quoted by the Exec rules and then
    // escaped again as a desktop string, so one backslash becomes four. A
    // single-escaped path launches the wrong program.
    let quoted = desktop::quote_exec_argument(r"C:\Program Files\bp.exe");
    assert_eq!(quoted, "\"C:\\\\Program Files\\\\bp.exe\"");
    assert_eq!(
        desktop::escape_value(&quoted),
        r#""C:\\\\Program Files\\\\bp.exe""#
    );
}

#[test]
fn the_mime_package_defines_only_the_products_own_types() {
    // PowerShell is the only type left that `shared-mime-info` does not define
    // (ADR-0069 took the other one), so the package is exactly that type.
    let package = desktop::mime_package(&everything()).expect("PowerShell is in Developer");
    // Not a redefinition of the distribution's own types.
    assert!(!package.contains("application/json"));
    assert!(!package.contains("text/plain"));
    // The three PowerShell extensions are one type with three globs, not
    // three definitions of the same type.
    assert_eq!(package.matches("text/x-powershell").count(), 1);
    assert_eq!(package.matches("<glob pattern=\"*.ps").count(), 3);
}

// --- the Windows registry shape --------------------------------------------

#[test]
fn registration_offers_the_extension_and_never_takes_it() {
    let values = windows::registry_values(&windows_app(), &everything());
    // The additive value: our ProgID appears as a *value name* under
    // OpenWithProgids.
    assert!(values.iter().any(|v| {
        v.key == r"HKCU\Software\Classes\.txt\OpenWithProgids"
            && v.name == ValueName::Named("BachelorPadPlusLite.txt".to_owned())
    }));
    // ...and nothing writes the extension key's own default value, which is
    // what taking the association looks like.
    assert!(
        !values
            .iter()
            .any(|v| v.key == r"HKCU\Software\Classes\.txt" && v.name == ValueName::Default),
        "the extension's default value is the association itself"
    );
    // ...nor UserChoice, which is the user's own record and is hash-protected.
    assert!(!values.iter().any(|v| v.key.contains("UserChoice")));
}

#[test]
fn no_generated_key_is_one_this_crate_refuses_to_write() {
    for &preset in AssociationPreset::ALL {
        let selection = AssociationSelection::preset(preset);
        let plan = plan(Platform::Windows, &windows_app(), &selection);
        assert_eq!(
            plan.registry_objections(),
            vec![],
            "{} generated a key the guard refuses",
            preset.label()
        );
    }
}

#[test]
fn the_guard_refuses_what_adr_0012_forbids() {
    use windows::{KeyObjection, key_objection};
    assert_eq!(
        key_objection(r"HKLM\Software\Classes\BachelorPadPlus.txt"),
        Some(KeyObjection::OutsideTheUserHive)
    );
    assert_eq!(
        key_objection(r"HKCU\Software\Classes\txtfile\shell\open\command"),
        Some(KeyObjection::AnotherApplication)
    );
    assert_eq!(
        key_objection(r"HKCU\Software\Classes\Applications\notepad.exe"),
        Some(KeyObjection::TouchesNotepad)
    );
    assert_eq!(
        key_objection(r"HKCU\Software\Classes\.txt\UserChoice"),
        Some(KeyObjection::SeizesTheAssociation)
    );
    assert_eq!(
        key_objection(r"HKCU\Software\Classes\.txt"),
        Some(KeyObjection::SeizesTheAssociation)
    );
    assert_eq!(
        key_objection(r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run"),
        Some(KeyObjection::AnotherApplication)
    );
    // ...and permits exactly what registration needs.
    assert_eq!(
        key_objection(r"HKCU\Software\Classes\.txt\OpenWithProgids"),
        None
    );
    assert_eq!(
        key_objection(r"HKCU\Software\Classes\BachelorPadPlusLite.md"),
        None
    );
    assert_eq!(
        key_objection(r"HKCU\Software\BachelorPad+ Lite\Capabilities"),
        None
    );
    assert_eq!(key_objection(r"HKCU\Software\RegisteredApplications"), None);
}

#[test]
fn the_full_products_names_are_another_applications() {
    use windows::{KeyObjection, key_objection};
    // ADR-0091. The guard used to admit anything under `Software\BachelorPad+`
    // and any ProgID beginning `BachelorPadPlus` -- which is the full
    // BachelorPad+'s territory, and writing there would repoint its file
    // types the day it shipped.
    assert_eq!(
        key_objection(r"HKCU\Software\Classes\BachelorPadPlus.md"),
        Some(KeyObjection::AnotherApplication)
    );
    assert_eq!(
        key_objection(r"HKCU\Software\BachelorPad+\Capabilities"),
        Some(KeyObjection::AnotherApplication)
    );
    assert_eq!(
        key_objection(r"HKCU\Software\BachelorPad+ Lite Extra"),
        Some(KeyObjection::AnotherApplication),
        "a name that merely begins with ours is not ours"
    );
}

#[test]
fn the_registration_names_follow_the_display_name() {
    // Trap 4's sharpest form: these are literals because a `const` cannot be
    // formatted, so this is what asks whether they still agree.
    assert_eq!(
        windows::SOFTWARE_KEY,
        format!(r"HKCU\Software\{}", crate::DISPLAY_NAME)
    );
    assert_eq!(
        windows::CAPABILITIES_KEY,
        format!(r"{}\Capabilities", windows::SOFTWARE_KEY)
    );
    assert_eq!(
        windows::PROG_ID_PREFIX,
        crate::DISPLAY_NAME.replace('+', "Plus").replace(' ', "")
    );
    assert!(crate::APP_ID.ends_with(".bachelorpluslite"));
}

#[test]
fn the_removal_script_takes_off_what_registration_put_on() {
    let script = windows::removal_script(crate::DISPLAY_NAME, &["txt"]);
    // Our ProgID key, and the earlier build's, deleted outright.
    assert!(script.contains(r"[-HKEY_CURRENT_USER\Software\Classes\BachelorPadPlusLite.txt]"));
    assert!(script.contains(r"[-HKEY_CURRENT_USER\Software\Classes\BachelorPadPlus.txt]"));
    // From the shared keys, only our *values* -- never the key, which every
    // other application registered under `.txt` shares.
    assert!(!script.contains(r"[-HKEY_CURRENT_USER\Software\Classes\.txt"));
    assert!(script.contains("\"BachelorPadPlusLite.txt\"=-"));
    assert!(!script.contains(r"[-HKEY_CURRENT_USER\Software\RegisteredApplications]"));
    assert!(script.contains(&format!("\"{}\"=-", crate::DISPLAY_NAME)));
    assert!(script.contains(r"[-HKEY_CURRENT_USER\Software\BachelorPad+ Lite]"));
    assert!(script.contains(r"[-HKEY_CURRENT_USER\Software\BachelorPad+]"));
    assert!(!script.to_ascii_lowercase().contains("notepad"));
}

#[test]
fn every_key_the_removal_script_names_is_ours_or_was_ours() {
    use windows::key_objection;
    // The removal is the one place the old names may appear, and only those.
    let every: Vec<&str> = FILE_TYPES.iter().map(|t| t.extension).collect();
    let script = windows::removal_script(crate::DISPLAY_NAME, &every);
    for line in script.lines().filter(|l| l.starts_with('[')) {
        let key = line
            .trim_start_matches("[-")
            .trim_start_matches('[')
            .trim_end_matches(']')
            .replace("HKEY_CURRENT_USER", "HKCU");
        let legacy = key.contains(&format!(r"\{}.", windows::LEGACY_PROG_ID_PREFIX))
            || key.eq_ignore_ascii_case(windows::LEGACY_SOFTWARE_KEY);
        assert!(
            legacy || key_objection(&key).is_none(),
            "the removal script names a key that is not ours: {key}"
        );
    }
}

#[test]
fn a_registry_script_is_written_as_utf16_with_a_byte_order_mark() {
    // regedit reads a UTF-8 .reg through the system code page, so an `é` in
    // the install path registered a command pointing nowhere (W2-05).
    let plan = plan(
        Platform::Windows,
        &AppInfo::bachelorpad(r"C:\Users\Zoë\Apps\bachelorpad.exe"),
        &everything(),
    );
    for kind in [ArtefactKind::RegistryScript, ArtefactKind::RegistryRemoval] {
        let bytes = plan
            .artefact(kind)
            .expect("both scripts are planned")
            .bytes();
        assert_eq!(&bytes[..2], &[0xFF, 0xFE], "{kind:?} lacks the BOM");
        let units: Vec<u16> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&pair| u16::from_le_bytes(pair))
            .collect();
        let text = String::from_utf16(&units).expect("valid UTF-16");
        assert!(text.starts_with("Windows Registry Editor Version 5.00\r\n"));
    }
    let script = plan
        .artefact(ArtefactKind::RegistryScript)
        .expect("planned");
    let text = String::from_utf16(
        &script.bytes()[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&pair| u16::from_le_bytes(pair))
            .collect::<Vec<_>>(),
    )
    .expect("valid UTF-16");
    assert!(text.contains("Zoë"), "the é must survive as itself");
}

#[test]
fn the_guard_is_case_insensitive_like_the_registry() {
    use windows::{KeyObjection, key_objection};
    assert_eq!(
        key_objection(r"hkcu\software\classes\.TXT\openwithprogids"),
        None
    );
    assert_eq!(
        key_objection(r"HKEY_CURRENT_USER\Software\Classes\NOTEPAD.exe"),
        Some(KeyObjection::TouchesNotepad)
    );
}

#[test]
fn the_registry_script_is_something_regedit_would_accept() {
    let plan = plan(Platform::Windows, &windows_app(), &everything());
    let script = plan
        .artefact(ArtefactKind::RegistryScript)
        .expect("Windows plans carry a .reg script");
    assert!(
        script
            .contents
            .starts_with("Windows Registry Editor Version 5.00\r\n")
    );
    assert!(
        script
            .contents
            .contains("[HKEY_CURRENT_USER\\Software\\Classes\\")
    );
    assert!(
        !script.contents.contains("[HKCU\\"),
        "the short hive name is not valid in a .reg file"
    );
    // Backslashes in the command doubled, quotes escaped.
    assert!(
        script
            .contents
            .contains(r#"@="\"C:\\Program Files\\BachelorPad+\\bachelorpad.exe\" \"%1\"""#),
        "{}",
        script.contents
    );
    // Every line ends CRLF.
    assert!(!script.contents.replace("\r\n", "").contains('\n'));
}

// --- the plan --------------------------------------------------------------

#[test]
fn each_platform_gets_the_artefacts_it_has_a_use_for() {
    let linux = plan(Platform::Linux, &app(), &everything());
    assert!(linux.artefact(ArtefactKind::DesktopEntry).is_some());
    assert!(linux.artefact(ArtefactKind::MimePackage).is_some());
    assert!(linux.artefact(ArtefactKind::RegistryScript).is_none());
    assert!(linux.registry_values.is_empty());
    assert!(matches!(linux.handoff, Handoff::LinuxSetDefault { .. }));

    let windows = plan(Platform::Windows, &windows_app(), &everything());
    assert!(windows.artefact(ArtefactKind::RegistryScript).is_some());
    assert!(windows.artefact(ArtefactKind::DesktopEntry).is_none());
    assert!(!windows.registry_values.is_empty());
    assert_eq!(
        windows.handoff,
        Handoff::WindowsDefaultApps {
            uri: "ms-settings:defaultapps"
        }
    );
}

#[test]
fn the_follow_up_commands_are_named_and_explained_never_run() {
    let plan = plan(Platform::Linux, &app(), &everything());
    let programs: Vec<&str> = plan.follow_up.iter().map(|f| f.program).collect();
    assert_eq!(
        programs,
        vec!["update-desktop-database", "update-mime-database"]
    );
    for follow_up in &plan.follow_up {
        assert!(follow_up.why.len() > 20, "{follow_up:?}");
        assert!(!follow_up.command_line().is_empty());
    }
}

#[test]
fn the_handoff_names_the_command_that_would_set_the_default_and_stops_there() {
    let plan = plan(
        Platform::Linux,
        &app(),
        &AssociationSelection::custom(["txt"]),
    );
    let Handoff::LinuxSetDefault { program, args } = &plan.handoff else {
        panic!("Linux hands off through xdg-mime");
    };
    assert_eq!(*program, "xdg-mime");
    assert_eq!(args[0], "default");
    assert_eq!(args[1], desktop::desktop_file_name(&app()));
    assert!(args.contains(&"text/plain".to_owned()));
    assert!(plan.handoff.describe().contains("xdg-mime"));
}

#[test]
fn an_app_info_says_what_is_wrong_with_it_before_a_plan_is_built() {
    let mut app = app();
    app.executable = "bachelorpad".to_owned();
    let problems = app.problems(Platform::Linux);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("relative"));
    assert!(
        AppInfo::bachelorpad("/usr/bin/bachelorpad")
            .problems(Platform::Linux)
            .is_empty()
    );
    // The same absolute-path question has a different answer per platform,
    // which is the point of taking the platform.
    assert!(
        !AppInfo::bachelorpad("/usr/bin/bachelorpad")
            .problems(Platform::Windows)
            .is_empty()
    );
}

// --- reading the current state ---------------------------------------------

#[test]
fn a_mimeapps_list_is_read_the_way_the_desktop_reads_it() {
    // The entry naming *us* is derived rather than spelt, so the
    // application id changing (ADR-0032) does not quietly turn this into a
    // test that we are not the default.
    let ours = desktop::desktop_file_name(&app());
    let text = format!(
        "\
# a comment
[Added Associations]
text/plain=someone-else.desktop;

[Default Applications]
text/plain = {ours};gedit.desktop;
text/markdown=marker.desktop
application/json=
"
    );
    let report = state::report_from_mimeapps(&text, &app(), &everything());
    assert_eq!(report.state_of("txt"), Some(&AssociationState::Ours));
    assert_eq!(
        report.state_of("md"),
        Some(&AssociationState::Other {
            handler: "marker.desktop".to_owned()
        })
    );
    // An empty value claims nothing, and `[Added Associations]` is not the
    // default -- it only means "can open", which is a different question.
    assert_eq!(report.state_of("json"), Some(&AssociationState::Unclaimed));
    assert!(report.is_ours(".TXT"));
    assert!(!report.is_ours("md"));
}

#[test]
fn a_missing_or_empty_mimeapps_list_means_unclaimed_not_broken() {
    let report = state::report_from_mimeapps("", &app(), &everything());
    assert!(
        report
            .entries()
            .iter()
            .all(|e| e.state == AssociationState::Unclaimed)
    );
    assert_eq!(report.ours(), 0);
}

#[test]
fn a_repeated_key_takes_the_first_like_the_desktop_entry_specification() {
    let text = "[Default Applications]\ntext/plain=first.desktop\ntext/plain=second.desktop\n";
    let defaults = state::default_applications(text);
    assert_eq!(defaults["text/plain"], vec!["first.desktop".to_owned()]);
}

#[test]
fn the_windows_state_is_read_from_prog_ids_supplied_by_the_caller() {
    let mut prog_ids = BTreeMap::new();
    prog_ids.insert(".txt".to_owned(), "BachelorPadPlusLite.txt".to_owned());
    prog_ids.insert("MD".to_owned(), "Notepad++_file".to_owned());
    let report = state::report_from_prog_ids(&prog_ids, &everything());
    assert_eq!(report.state_of("txt"), Some(&AssociationState::Ours));
    assert_eq!(
        report.state_of("md"),
        Some(&AssociationState::Other {
            handler: "Notepad++_file".to_owned()
        })
    );
    assert_eq!(report.state_of("json"), Some(&AssociationState::Unclaimed));
}

#[test]
fn windows_reports_cannot_tell_rather_than_not_registered() {
    // The distinction a user acts on: "unclaimed" invites them to fix
    // something, "cannot tell" tells them the truth.
    let report = association_report(Platform::Windows, None, &app(), &everything());
    for entry in report.entries() {
        let AssociationState::Unknown { reason } = &entry.state else {
            panic!("{entry:?} claimed to know something Windows cannot tell us");
        };
        assert!(reason.contains("registry"), "{reason}");
    }
    assert!(report.summary().starts_with("Cannot tell"));
}

#[test]
fn the_report_is_read_from_a_directory_the_caller_names() {
    let dir = tempdir().unwrap();
    std::fs::write(
        dir.path().join("mimeapps.list"),
        format!(
            "[Default Applications]\ntext/plain={};\n",
            desktop::desktop_file_name(&app())
        ),
    )
    .unwrap();
    let selection = AssociationSelection::custom(["txt", "md"]);
    let report = association_report(Platform::Linux, Some(dir.path()), &app(), &selection);
    assert!(report.is_ours("txt"));
    assert!(!report.is_ours("md"));
    assert_eq!(
        report.summary(),
        format!(
            "{} opens 1 of the 2 selected file types.",
            crate::DISPLAY_NAME
        )
    );
}

#[test]
fn a_directory_with_no_mimeapps_list_is_not_an_error() {
    let dir = tempdir().unwrap();
    let report = association_report(
        Platform::Linux,
        Some(dir.path()),
        &app(),
        &AssociationSelection::custom(["txt"]),
    );
    assert_eq!(report.state_of("txt"), Some(&AssociationState::Unclaimed));
}

// --- installing, the one thing that changes anything -----------------------

#[test]
fn nothing_is_written_without_consent() {
    let dir = tempdir().unwrap();
    let plan = plan(Platform::Linux, &app(), &everything());
    assert_eq!(
        install(&plan, dir.path(), Consent::Withheld),
        Err(InstallRefusal::ConsentWithheld)
    );
    assert_eq!(
        std::fs::read_dir(dir.path()).unwrap().count(),
        0,
        "a refused install wrote something"
    );
}

#[test]
fn consent_is_checked_before_anything_else_can_refuse_first() {
    // Otherwise a refused install reports "nothing selected" and the fact that
    // nobody agreed to it never reaches the log.
    let dir = tempdir().unwrap();
    let empty = plan(Platform::Linux, &app(), &AssociationSelection::default());
    assert_eq!(
        install(&empty, dir.path(), Consent::Withheld),
        Err(InstallRefusal::ConsentWithheld)
    );
    assert_eq!(
        install(&empty, dir.path(), Consent::Granted),
        Err(InstallRefusal::NothingSelected)
    );
}

#[test]
fn a_consented_install_writes_exactly_the_plans_artefacts_under_the_given_root() {
    let dir = tempdir().unwrap();
    let plan = plan(Platform::Linux, &app(), &everything());
    let installed = install(&plan, dir.path(), Consent::Granted).expect("Linux can install");

    assert_eq!(installed.written.len(), plan.artefacts.len());
    for path in &installed.written {
        assert!(path.starts_with(dir.path()), "{path:?} escaped the root");
        assert!(path.is_file());
    }
    let entry = std::fs::read_to_string(
        dir.path()
            .join("applications")
            .join(desktop::desktop_file_name(&app())),
    )
    .expect("the desktop entry landed where mimeapps.list will look for it");
    assert_eq!(entry, plan.artefacts[0].contents);
    assert!(
        dir.path()
            .join("mime/packages")
            .join(desktop::mime_package_file_name(&app()))
            .is_file()
    );
    assert_eq!(installed.follow_up, plan.follow_up);
}

#[test]
fn installing_sets_no_default_and_the_report_still_says_so() {
    // The line ADR-0012 draws: after a successful install the product is in
    // Open With, and nothing about what opens `.txt` has changed.
    let data = tempdir().unwrap();
    let config = tempdir().unwrap();
    let plan = plan(Platform::Linux, &app(), &everything());
    install(&plan, data.path(), Consent::Granted).unwrap();

    let report = association_report(Platform::Linux, Some(config.path()), &app(), &everything());
    assert_eq!(report.ours(), 0, "installing took an association");
}

#[test]
fn windows_refuses_to_install_and_says_the_same_thing_the_capability_says() {
    let dir = tempdir().unwrap();
    let plan = plan(Platform::Windows, &windows_app(), &everything());
    let refusal = install(&plan, dir.path(), Consent::Granted).expect_err("Windows cannot install");
    let InstallRefusal::NotSupported { platform, reason } = refusal else {
        panic!("Windows refused for the wrong reason: {refusal:?}");
    };
    assert_eq!(platform, Platform::Windows);
    assert_eq!(
        Some(reason),
        Capability::AssociationInstall
            .availability(Platform::Windows)
            .reason(),
        "the refusal and the capability register must not drift apart"
    );
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn an_artefact_path_that_could_escape_the_root_is_refused_before_anything_is_written() {
    let dir = tempdir().unwrap();
    for bad in [
        "../elsewhere/bachelorpad.desktop",
        "/etc/bachelorpad.desktop",
        r"C:\Windows\bachelorpad.desktop",
        r"..\up.desktop",
        "  ",
    ] {
        let mut plan = plan(Platform::Linux, &app(), &everything());
        plan.artefacts[0].relative_path = bad.to_owned();
        let refusal = install(&plan, dir.path(), Consent::Granted).expect_err(bad);
        assert!(
            matches!(refusal, InstallRefusal::UnsafeArtefactPath { .. }),
            "{bad} produced {refusal:?}"
        );
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            0,
            "{bad}: a bad path in one artefact wrote another"
        );
    }
}

#[test]
fn install_root_is_the_shared_data_hierarchy_on_linux_and_nothing_on_windows() {
    let env = EnvSnapshot {
        home: Some("/home/me".into()),
        ..EnvSnapshot::default()
    };
    assert_eq!(
        install_root(Platform::Linux, &env),
        Some(PathBuf::from("/home/me/.local/share"))
    );
    assert_eq!(
        install_root(Platform::Windows, &EnvSnapshot::default()),
        None
    );
}

#[test]
fn every_refusal_can_be_explained() {
    let refusals = [
        InstallRefusal::ConsentWithheld,
        InstallRefusal::NothingSelected,
        InstallRefusal::NotSupported {
            platform: Platform::Windows,
            reason: "There is no registry writer here.",
        },
        InstallRefusal::UnsafeArtefactPath {
            relative_path: "../x".to_owned(),
        },
        InstallRefusal::Failed {
            path: PathBuf::from("/x"),
            message: "permission denied".to_owned(),
        },
    ];
    for refusal in &refusals {
        assert!(refusal.describe().len() > 20, "{refusal:?}");
    }
}

// --- properties ------------------------------------------------------------

fn any_selection() -> impl Strategy<Value = AssociationSelection> {
    prop_oneof![
        prop::sample::select(AssociationPreset::ALL).prop_map(AssociationSelection::preset),
        prop::collection::vec(
            prop::sample::select(FILE_TYPES.iter().map(|t| t.extension).collect::<Vec<_>>()),
            0..8,
        )
        .prop_map(AssociationSelection::custom),
    ]
}

/// Strings a hostile or merely careless caller might put in an `AppInfo`.
fn awkward_text() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop_oneof![
            prop::char::range('a', 'z'),
            Just(' '),
            Just('='),
            Just(';'),
            Just('\n'),
            Just('\r'),
            Just('\t'),
            Just('\\'),
            Just('"'),
            Just('<'),
            Just('&'),
            Just('$'),
        ],
        0..40,
    )
    .prop_map(|chars| chars.into_iter().collect())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// Whatever the shell puts in an `AppInfo`, the generated `.desktop` file
    /// is still a sequence of `key=value` lines under one group header. A
    /// display name containing a newline must not become an `Exec` line, which
    /// is the injection this format invites: there is no quoting at the line
    /// level, so a bad value produces a *valid* file that does something else.
    #[test]
    fn no_app_info_can_add_a_key_to_the_desktop_entry(
        display_name in awkward_text(),
        description in awkward_text(),
        executable in awkward_text(),
    ) {
        let mut app = app();
        app.display_name = display_name;
        app.description = description;
        app.executable = executable;
        let entry = desktop::desktop_entry(&app, &everything());

        let expected: Vec<&str> = vec![
            "Type", "Version", "Name", "GenericName", "Comment", "Exec", "TryExec", "Icon",
            "Terminal", "StartupNotify", "Categories", "Keywords", "MimeType",
        ];
        let keys: Vec<&str> = desktop_keys(&entry).into_iter().map(|(k, _)| k).collect();
        prop_assert_eq!(keys, expected, "the entry grew or lost a key:\n{}", entry);
        prop_assert_eq!(entry.matches("[Desktop Entry]").count(), 1);
    }

    /// Whatever the selection, every registry key a plan generates is one the
    /// ADR-0012 guard permits. The guard is only worth having if the generator
    /// cannot get around it, and this is the statement of that.
    #[test]
    fn no_selection_produces_a_key_the_guard_refuses(selection in any_selection()) {
        let plan = plan(Platform::Windows, &windows_app(), &selection);
        prop_assert_eq!(plan.registry_objections(), vec![]);
        // ...and none of them so much as mentions Notepad.
        for value in &plan.registry_values {
            prop_assert!(!value.key.to_ascii_lowercase().contains("notepad"));
            prop_assert!(!value.data.to_ascii_lowercase().contains("notepad"));
        }
    }

    /// Whatever the selection, an install writes only inside the root it was
    /// given, and writes one file per artefact.
    #[test]
    fn an_install_never_leaves_the_root_it_was_given(
        selection in any_selection(),
        prefix in prop::sample::select(vec![
            "", "../", "..\\", "/", r"C:\", "./", "a/../../",
        ]),
    ) {
        prop_assume!(!selection.is_empty());
        let dir = tempdir().unwrap();
        let mut plan = plan(Platform::Linux, &app(), &selection);
        // A plan is a value, and values arrive from elsewhere. Whatever is
        // stuck on the front of an artefact's path, an install either refuses
        // outright or writes inside the root it was given -- never both, and
        // never outside.
        for artefact in &mut plan.artefacts {
            artefact.relative_path = format!("{prefix}{}", artefact.relative_path);
        }
        match install(&plan, dir.path(), Consent::Granted) {
            Err(refusal) => prop_assert!(
                matches!(refusal, InstallRefusal::UnsafeArtefactPath { .. }),
                "{:?}",
                refusal,
            ),
            Ok(installed) => {
                prop_assert_eq!(installed.written.len(), plan.artefacts.len());
                for path in &installed.written {
                    prop_assert!(path.starts_with(dir.path()), "{:?}", path);
                    prop_assert!(!path.to_string_lossy().contains(".."), "{:?}", path);
                }
            }
        }
    }

    /// A plan is deterministic. Two runs over the same inputs give
    /// byte-identical artefacts, so a registration file does not appear in
    /// every backup diff for having reordered itself.
    #[test]
    fn a_plan_is_the_same_plan_twice(
        selection in any_selection(),
        platform in prop::sample::select(Platform::ALL),
    ) {
        let first = plan(platform, &windows_app(), &selection);
        let second = plan(platform, &windows_app(), &selection);
        prop_assert_eq!(first, second);
    }

    /// Whatever is selected, the product claims no file type it cannot open.
    /// Stated over selections rather than over the table, because `custom`
    /// takes arbitrary strings and dropping the unknown ones is what keeps
    /// this true.
    #[test]
    fn a_plan_never_claims_a_type_the_editor_cannot_open(
        proposed in prop::collection::vec("[a-z]{1,6}", 0..10),
    ) {
        let refs: Vec<&str> = proposed.iter().map(String::as_str).collect();
        let selection = AssociationSelection::custom(refs);
        for file_type in selection.file_types() {
            prop_assert!(
                bp_formats::Format::from_extension(file_type.extension).is_some(),
                "{}",
                file_type.extension,
            );
        }
    }
}

#[test]
fn the_icon_a_plan_names_is_the_file_the_release_stages() {
    // **The seam ADR-0068 exists to hold.** The icon is not embedded in the
    // executable and is not installed into a theme; it travels in the
    // archive, and two independent things have to agree about its name: this
    // crate, which writes it into a `.desktop` file or a registry plan, and
    // the release scripts, which copy it. Those scripts are PowerShell and
    // shell and cannot be type-checked against this, so the agreement is
    // asserted by reading them.
    //
    // Before ADR-0068 there was no file at all: Windows registration wrote
    // `DefaultIcon` as `"<exe>",0` and the executable has never carried an
    // icon resource, so every type registered through it rendered blank.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("the workspace root is two levels above this crate");

    for platform in [Platform::Windows, Platform::Linux] {
        let name = icon_file_name(platform);
        assert!(
            root.join("assets").join(name).is_file(),
            "{platform:?}: assets/{name} is named by this crate and does not exist"
        );
    }

    for (script, name) in [
        ("scripts/New-Release.ps1", icon_file_name(Platform::Windows)),
        ("scripts/release-linux.sh", icon_file_name(Platform::Linux)),
    ] {
        let text = std::fs::read_to_string(root.join(script))
            .unwrap_or_else(|e| panic!("{script} is unreadable: {e}"));
        assert!(
            text.contains(name),
            "{script} never mentions {name}, so the archive would ship without \
             the icon its own registration points at"
        );
    }
}

#[test]
fn the_icon_path_follows_the_executable_and_the_platform() {
    // A user unpacks the archive wherever they like, so the icon's location
    // is only ever knowable relative to the binary.
    assert_eq!(
        icon_beside(Platform::Linux, "/opt/bp/bachelorpad").as_deref(),
        Some("/opt/bp/io.github.dboles99.bachelorpluslite.png")
    );
    assert_eq!(
        icon_beside(Platform::Windows, r"C:\Apps\bp\bachelorpad.exe").as_deref(),
        Some(r"C:\Apps\bp\bachelorpad.ico")
    );
    // A bare name has no parent, and guessing one would write a path into a
    // `.desktop` file that resolves to nothing.
    assert_eq!(icon_beside(Platform::Linux, "bachelorpad"), None);
}

#[test]
fn each_icon_asset_is_the_format_its_name_claims() {
    // `is_file()` is satisfied by an empty file, and an empty file is exactly
    // what a mangled copy or a failed generator leaves behind. The window
    // icon fails *silently* by design -- a missing icon must not stop the
    // editor starting -- so nothing at runtime would report this, and the
    // Explorer icon would simply be blank again.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("the workspace root is two levels above this crate");
    let read = |name: &str| std::fs::read(root.join("assets").join(name)).expect("asset");

    let png = read(icon_file_name(Platform::Linux));
    assert_eq!(
        &png[..8],
        b"\x89PNG\r\n\x1a\n",
        "the Linux icon is not a PNG"
    );
    // IHDR width and height, big-endian at bytes 16..24.
    let width = u32::from_be_bytes(png[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(png[20..24].try_into().unwrap());
    assert_eq!((width, height), (256, 256), "the Linux icon is not 256x256");

    let ico = read(icon_file_name(Platform::Windows));
    // ICONDIR: reserved 0, type 1 (icon), then the image count.
    assert_eq!(&ico[..4], &[0, 0, 1, 0], "the Windows icon is not an ICO");
    let count = u16::from_le_bytes(ico[4..6].try_into().unwrap());
    assert!(
        count >= 4,
        "an ICO with {count} sizes will be scaled badly somewhere; \
         Explorer alone asks for 16, 32, 48 and 256"
    );
    // Every directory entry must point inside the file, or the reader gets a
    // truncated image and shows nothing.
    for i in 0..usize::from(count) {
        let e = 6 + i * 16;
        let len = u32::from_le_bytes(ico[e + 8..e + 12].try_into().unwrap()) as usize;
        let off = u32::from_le_bytes(ico[e + 12..e + 16].try_into().unwrap()) as usize;
        assert!(
            off + len <= ico.len(),
            "ICO entry {i} runs past the end of the file"
        );
    }
}

#[test]
fn the_window_icon_is_a_format_the_toolkit_can_actually_decode() {
    // **Caught by extracting an archive and looking at it**, an hour after
    // the window icon was added. Slint 1.17 builds the `image` crate with
    // `png` and `jpeg` only; there is no ICO decoder. The window icon loads
    // silently-or-not by design, so on Windows it produced the toolkit's
    // default and nothing reported it.
    //
    // Asserted as a property of the *name* rather than of Slint, because
    // this crate cannot depend on the toolkit: the window icon must be a PNG
    // on every platform, whatever registration names.
    for platform in [Platform::Windows, Platform::Linux] {
        let named = window_icon_beside(
            platform,
            match platform {
                Platform::Windows => r"C:\Apps\bp\bachelorpad.exe",
                Platform::Linux => "/opt/bp/bachelorpad",
            },
        )
        .expect("an absolute executable has a parent");
        assert!(
            named.ends_with(".png"),
            "{platform:?}: the window icon is {named}, which Slint cannot decode"
        );
    }

    // And the Windows archive must carry it, not only the .ico.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("workspace root");
    let script = std::fs::read_to_string(root.join("scripts/New-Release.ps1")).expect("script");
    assert!(
        script.contains(window_icon_file_name()),
        "the Windows archive ships no {}, so the window icon would fall back \
         to the toolkit default on every machine",
        window_icon_file_name()
    );
}

// --- the Windows installer (ADR-0093) ---------------------------------------

#[test]
fn the_installers_registry_section_is_the_registrations_own() {
    // The installer's optional Open with task writes what File > Set as
    // Default Editor would for the Text + Notes preset, generated by the code
    // that decides what that is -- so the two cannot drift into registering
    // different things.
    let executable = r"{app}\bachelorpad.exe";
    let app = AppInfo {
        display_name: crate::DISPLAY_NAME.to_owned(),
        description: crate::DESCRIPTION.to_owned(),
        executable: executable.to_owned(),
        app_id: crate::APP_ID.to_owned(),
        icon: icon_beside(Platform::Windows, executable).expect("the executable has a folder"),
    };
    let selection = AssociationSelection::preset(AssociationPreset::TextAndNotes);
    let rendered =
        windows::inno_registry_section(&windows::registry_values(&app, &selection), "openwith");

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packaging/windows/registry.iss");
    if std::env::var_os("BLESS").is_some() {
        std::fs::write(&path, &rendered).expect("rewrite registry.iss");
    }
    let committed = std::fs::read_to_string(&path).expect("packaging/windows/registry.iss");
    assert_eq!(
        committed.replace("\r\n", "\n"),
        rendered,
        "packaging/windows/registry.iss is not what the registration generates -- \
         rerun this test with BLESS=1 and review the diff"
    );
}

#[test]
fn the_installer_deletes_our_keys_and_only_our_values_from_shared_ones() {
    let rendered = windows::inno_registry_section(
        &windows::registry_values(&windows_app(), &everything()),
        "openwith",
    );
    for line in rendered.lines().filter(|l| l.starts_with("Root:")) {
        let shared = line.contains(r"\OpenWithProgids") || line.contains("RegisteredApplications");
        if shared {
            assert!(line.contains("uninsdeletevalue"), "{line}");
        } else {
            assert!(!line.contains("uninsdeletevalue"), "{line}");
        }
        assert!(line.starts_with("Root: HKA;"), "per-user only: {line}");
        assert!(!line.to_ascii_lowercase().contains("notepad"), "{line}");
    }
}
