//! Every package names the product, its id and its licence as the program
//! does (ADR-0093).
//!
//! The packaging files cannot read a Rust constant -- a `.desktop` entry, an
//! AppStream file, an Inno Setup script, a Flatpak manifest, a PKGBUILD -- so
//! each holds a literal copy of `DISPLAY_NAME`, `APP_ID`, `DESCRIPTION` or the
//! licence. ADR-0074 found what literal copies do across a rename: some move
//! and some do not. This is what asks.
//!
//! What it does not check is that the packages *build*; the release workflow
//! does that, and a dry run of it is the evidence.

use std::path::{Path, PathBuf};

fn packaging(relative: &str) -> String {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packaging")
        .join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .replace("\r\n", "\n")
}

fn has_line(text: &str, line: &str) -> bool {
    text.lines().any(|l| l.trim_end() == line)
}

const APP_ID: &str = bp_platform::APP_ID;
const NAME: &str = bp_platform::DISPLAY_NAME;
const DESCRIPTION: &str = bp_platform::DESCRIPTION;

#[test]
fn the_desktop_entry_names_the_product_and_matches_its_window() {
    let entry = packaging(&format!("linux/{APP_ID}.desktop"));
    for line in [
        format!("Name={NAME}"),
        format!("Comment={DESCRIPTION}"),
        format!("Icon={APP_ID}"),
        // What the window reports since ADR-0091; a dock matches the two.
        format!("StartupWMClass={APP_ID}"),
        "Exec=bachelorpad %F".to_owned(),
    ] {
        assert!(has_line(&entry, &line), "the .desktop entry lacks `{line}`");
    }
}

#[test]
fn the_appstream_file_names_the_product_its_licence_and_its_launcher() {
    let info = packaging(&format!("linux/{APP_ID}.metainfo.xml"));
    for fragment in [
        format!("<id>{APP_ID}</id>"),
        format!("<name>{NAME}</name>"),
        format!("<summary>{DESCRIPTION}</summary>"),
        format!("<launchable type=\"desktop-id\">{APP_ID}.desktop</launchable>"),
        format!(
            "<project_license>{}</project_license>",
            env!("CARGO_PKG_LICENSE")
        ),
    ] {
        assert!(
            info.contains(&fragment),
            "the AppStream file lacks `{fragment}`"
        );
    }
}

#[test]
fn the_flatpak_manifest_is_the_application_id() {
    let manifest = packaging(&format!("flatpak/{APP_ID}.yml"));
    assert!(has_line(&manifest, &format!("app-id: {APP_ID}")));
    // Asked of the YAML, not its comments: the comment explaining why the
    // permission is absent names it.
    assert!(
        !manifest
            .lines()
            .filter(|l| !l.trim_start().starts_with('#'))
            .any(|l| l.contains("--share=network")),
        "the product never touches the network (ADR-0006), and its sandbox must not let it"
    );
}

#[test]
fn the_windows_installer_names_the_product_and_never_asks_for_administrator() {
    let script = packaging("windows/bachelorpad-lite.iss");
    for line in [
        format!("AppName={NAME}"),
        "PrivilegesRequired=lowest".to_owned(),
        "#include \"registry.iss\"".to_owned(),
    ] {
        assert!(has_line(&script, &line), "the installer lacks `{line}`");
    }
    assert!(
        !script.contains("PrivilegesRequiredOverridesAllowed"),
        "nothing may turn the per-user install into an elevated one"
    );
    let open_with = script
        .lines()
        .find(|l| l.starts_with("Name: \"openwith\""))
        .expect("the Open with task");
    assert!(
        open_with.contains("Flags: unchecked"),
        "offering file types is opt-in (ADR-0012): {open_with}"
    );
}

#[test]
fn the_aur_package_describes_the_product_and_its_licence() {
    let pkgbuild = packaging("aur/PKGBUILD.in");
    assert!(has_line(&pkgbuild, &format!("pkgdesc='{DESCRIPTION}'")));
    assert!(has_line(
        &pkgbuild,
        &format!("license=('{}')", env!("CARGO_PKG_LICENSE"))
    ));
}
