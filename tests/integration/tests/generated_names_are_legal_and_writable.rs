//! Seam: `bp-naming` generates, `bp-platform` judges, `bp-files` writes.
//!
//! Three crates, one chain, and nothing joins them. `bp-naming` turns a
//! semantic title into a filename and applies "the union of Windows and Linux
//! restrictions ... on both platforms" -- its own words. `bp-platform` holds
//! the authoritative rules for each platform as *data*, so the Windows rules
//! are executed by the Linux leg of CI and the other way round. `bp-files` is
//! what actually has to write the name, and it depends on `bp-naming` but not
//! on `bp-platform` at all.
//!
//! The property worth having is one sentence:
//!
//! > **A name `bp-naming` generates is a name `bp-platform` calls legal on
//! > both platforms, and one `bp-files` can actually write.**
//!
//! If it does not hold, a note created on Linux and synced to Windows becomes
//! unopenable, or -- for a reserved device name -- a save appears to succeed
//! and the document is gone. Two ways it does not hold are pinned as ignored
//! tests at the bottom of this file; both are real defects and neither is
//! fixed here.
//!
//! Nothing here writes outside a `tempfile` directory. The only strings
//! involved are titles, drawn from a fixed alphabet, and none is a secret.

mod common;

use common::no_files;

use bp_files::{SaveOptions, atomic_write};
use bp_naming::SemanticName;
use bp_platform::{
    Platform,
    paths::{MAX_COMPONENT_LEN, PathProblem, component_len, file_name_problems},
};
use proptest::prelude::*;
use tempfile::tempdir;
use time::{Date, Month};

/// A fixed date, so a failure names a title rather than a calendar.
fn a_date() -> Date {
    Date::from_calendar_date(2026, Month::August, 19).expect("a real date")
}

/// Everything either platform objects to about `name`.
fn problems_on_both(name: &str) -> Vec<(Platform, PathProblem)> {
    Platform::ALL
        .iter()
        .flat_map(|&platform| {
            file_name_problems(platform, name)
                .into_iter()
                .map(move |problem| (platform, problem))
        })
        .collect()
}

/// Assert that a generated name is legal on both platforms.
fn name_is_legal(name: &str) {
    let problems = problems_on_both(name);
    assert!(
        problems.is_empty(),
        "bp-naming produced {name:?}, which bp-platform rejects: {}",
        problems
            .iter()
            .map(|(platform, problem)| format!("[{}] {}", platform.token(), problem.describe()))
            .collect::<Vec<_>>()
            .join("; ")
    );

    for &platform in Platform::ALL {
        assert!(
            component_len(platform, name) <= MAX_COMPONENT_LEN,
            "{name:?} is {} units long on {}, over the {MAX_COMPONENT_LEN} limit",
            component_len(platform, name),
            platform.token(),
        );
    }
}

/// The whole chain for one title: legal on both platforms, and writable.
///
/// The write is the half a rules table cannot fake. A name can satisfy every
/// listed rule and still be refused by the filesystem -- and on Windows some
/// of the interesting failures do not return an error at all.
fn name_is_legal_and_writable(title: &str, extension: &str) {
    let name = SemanticName::new(title, a_date(), extension).to_filename();
    name_is_legal(&name);

    let dir = tempdir().expect("temp dir");
    let path = dir.path().join(&name);
    atomic_write(&path, b"body\n", SaveOptions::default())
        .unwrap_or_else(|e| panic!("bp-files could not write {name:?}: {e}"));

    // Written where we asked, under the name we asked for. `exists()` alone
    // would pass for a path Windows silently normalised into a different one.
    let written: Vec<_> = std::fs::read_dir(dir.path())
        .expect("read_dir")
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        written,
        vec![name.clone()],
        "the directory does not hold exactly the name that was asked for"
    );
}

// --- the classes that do hold --------------------------------------------

#[test]
fn characters_windows_forbids_never_reach_the_filename() {
    // A colon is the one users type without thinking -- "Notes: part 2" -- and
    // on Windows it opens an alternate data stream rather than failing.
    for title in [
        "Notes: part 2",
        "what? yes*",
        "a/b\\c",
        "<angle> \"quoted\" |piped|",
        "control\u{0}chars\u{7}here",
        "tab\tand\nnewline",
    ] {
        name_is_legal_and_writable(title, "txt");
    }
}

#[test]
fn trailing_dots_and_spaces_never_reach_the_filename() {
    // Windows strips these silently, so a name we thought we wrote is not the
    // name on the disk -- and a caller that thought it had two files has one.
    for title in [
        "trailing dots...",
        "trailing space   ",
        "  both  .  ",
        "trailing sep_",
        "trailing dash-",
    ] {
        name_is_legal_and_writable(title, "txt");
    }
}

#[test]
fn short_multibyte_and_astral_titles_are_legal_and_writable() {
    for title in [
        "F\u{fc}hrungsnotizen",
        "\u{7814}\u{7a76}\u{30ce}\u{30fc}\u{30c8}",
        "\u{1f3bc}\u{1f3bb} Takt 4 \u{1f3ba}",
    ] {
        name_is_legal_and_writable(title, "txt");
    }
}

#[test]
fn an_overlong_title_is_cut_to_a_name_both_platforms_call_legal() {
    // The unit trap `bp-platform` names: Windows counts UTF-16 code units and
    // Linux counts bytes, and `bp-naming` budgets in bytes. Bytes are the
    // stricter of the two for every character -- one byte per unit for ASCII
    // and more for everything else -- so the byte budget also satisfies
    // Windows. That is arithmetic nobody had checked across the two crates.
    //
    // The write is deliberately not attempted here. It fails on Windows for a
    // reason that has nothing to do with the *name* and everything to do with
    // the *path*, which is
    // `a_name_bp_naming_will_generate_can_be_unwritable_on_windows` below.
    for title in [
        &"\u{e9}".repeat(400),
        &"\u{7814}".repeat(400),
        &"\u{1f3bc}".repeat(400),
        &"a".repeat(400),
    ] {
        name_is_legal(&SemanticName::new(title, a_date(), "txt").to_filename());
    }
}

#[test]
fn a_title_that_sanitises_away_to_nothing_still_produces_a_writable_name() {
    for title in ["", "   ", "///", "...", "\u{0}\u{1}\u{2}"] {
        name_is_legal_and_writable(title, "txt");
    }
}

#[test]
fn a_hostile_extension_still_produces_a_writable_name() {
    for extension in ["", "///", "..tar.gz", "TXT", &"x".repeat(80), "c:on"] {
        name_is_legal_and_writable("Ordinary title", extension);
    }
}

#[test]
fn a_bare_reserved_device_name_is_defused() {
    // The case `bp-naming` does handle, worth pinning separately from the one
    // it does not: the sanitiser's exact-match rule catches a title that *is*
    // a device name, and the ignored test below shows how narrowly it misses.
    for title in ["CON", "con", "NUL", "aux", "LPT1", "prn"] {
        name_is_legal_and_writable(title, "txt");
    }
}

proptest! {
    #![proptest_config(no_files(256))]

    /// The property, over generated titles.
    ///
    /// The alphabet is every class that has ever caused this to break --
    /// forbidden characters, separators, dots, spaces, controls, multibyte and
    /// astral characters -- **except** a device name followed by a dot, which
    /// is a defect rather than a gap and is pinned by
    /// `a_reserved_device_name_before_a_dot_survives_into_the_filename` below.
    /// Generating it here would turn one loud, precise failure into a shrunk
    /// one that says less.
    #[test]
    fn every_generated_name_is_legal_on_both_platforms(
        title in "[a-zA-Z0-9 ._<>:\"/\\\\|?*\t\u{0}\u{7}\u{e9}\u{fc}\u{df}\u{7814}\u{1f3bc}-]{0,40}",
        extension in "[a-zA-Z0-9.]{0,8}",
    ) {
        let name = SemanticName::new(&title, a_date(), &extension).to_filename();
        let problems = problems_on_both(&name);
        prop_assert!(
            problems.is_empty(),
            "bp-naming produced a name bp-platform rejects: {:?}",
            problems,
        );
        for &platform in Platform::ALL {
            prop_assert!(
                component_len(platform, &name) <= MAX_COMPONENT_LEN,
                "the name is too long on {}",
                platform.token(),
            );
        }
    }
}

// --- defect one: reserved device names ------------------------------------

#[test]
#[ignore = "DEFECT: bp-naming's reserved-name check is an exact match on the whole \
            sanitised title, while bp-platform (and Win32) test only the stem before the \
            first dot. A title with a dot after a device name -- \"con.txt\", \"CON.notes\", \
            \"aux.log\" -- passes bp-naming's check unchanged and yields a filename \
            bp-platform calls ReservedName on Windows. Opening a reserved name succeeds \
            and reads or writes the DEVICE, so the save appears to work and the document \
            is gone. Fix belongs in bp-naming::sanitize::is_reserved, which should test \
            the stem rather than the whole string, and whose list should also gain \
            CONIN$ and CONOUT$. Left red rather than weakened, awaiting a human decision."]
fn a_reserved_device_name_before_a_dot_survives_into_the_filename() {
    // `bp_naming::sanitize::is_reserved` upper-cases the sanitised title and
    // asks whether the *whole string* is in its reserved list.
    // `bp_platform::paths::reserved_device_name` -- which is what Win32
    // actually does -- takes the stem before the first dot, trims trailing
    // spaces, and asks about that. A dot is not a forbidden character, so it
    // survives sanitising, and the two checks then disagree about every title
    // of the form `<device>.<anything>`.
    //
    // This asserts the verdict and deliberately does **not** attempt the
    // write: writing to `CON` in a test would put the fixture on the console
    // and would prove nothing the verdict does not already prove.
    //
    // A second, smaller disagreement rides along and is asserted here so a fix
    // is measured against both: `bp-naming`'s list omits `CONIN$` and
    // `CONOUT$`, which `bp-platform` includes because modern Windows reserves
    // them. A bare `CONIN$` title is saved by the date suffix --
    // `CONIN$_19AUG2026.txt` has stem `CONIN$_19AUG2026` -- but
    // `CONIN$.notes` is not.
    let mut offenders = Vec::new();

    for title in [
        "con.txt",
        "CON.notes",
        "aux.log",
        "NUL.dat",
        "lpt1.bak",
        "CONIN$.notes",
        "CONOUT$.notes",
        "prn.2026",
    ] {
        let name = SemanticName::new(title, a_date(), "txt").to_filename();
        let problems = file_name_problems(Platform::Windows, &name);
        if !problems.is_empty() {
            offenders.push(format!(
                "{title:?} -> {name:?}: {}",
                problems
                    .iter()
                    .map(PathProblem::describe)
                    .collect::<Vec<_>>()
                    .join(" / ")
            ));
        }
    }

    assert!(
        offenders.is_empty(),
        "bp-naming produced {} filename(s) Windows refuses:\n  {}",
        offenders.len(),
        offenders.join("\n  ")
    );
}

// --- defect two: MAX_PATH -------------------------------------------------

#[test]
#[ignore = "DEFECT: bp-files never applies the \\\\?\\ extended-length prefix, and does \
            not depend on bp-platform at all, so a filename at bp-naming's own documented \
            maximum (255 bytes) is unwritable on Windows inside any directory whose path \
            takes the total past MAX_PATH -- which the system temp directory alone \
            already does. The name is legal by every rule bp-platform states; it is the \
            PATH that is too long, and bp-platform already provides both the diagnosis \
            (paths::needs_extended_length_prefix) and the fix (paths::to_extended_length) \
            that bp-files does not call. Worse than the refusal is the message: \
            SaveError says the file 'may be read-only, or open in another program', which \
            sends the user to look at the wrong thing. Left red rather than weakened."]
fn a_name_bp_naming_will_generate_can_be_unwritable_on_windows() {
    // 255 bytes is not an unreasonable name a fuzzer dreamt up: it is exactly
    // what `SemanticName::to_filename` promises to produce for a long title,
    // and long titles are what a semantic filename scheme is *for*.
    let name = SemanticName::new(&"a".repeat(400), a_date(), "txt").to_filename();
    name_is_legal(&name);

    let dir = tempdir().expect("temp dir");
    let path = dir.path().join(&name);
    let full = path.to_string_lossy().into_owned();

    // bp-platform saw this coming, on the path rather than the name.
    let path_problems = bp_platform::paths::path_problems(Platform::Windows, &full);
    assert!(
        path_problems
            .iter()
            .any(|p| matches!(p, PathProblem::PathTooLong { .. })),
        "this test no longer reproduces: the temp path is short enough that \
         MAX_PATH is not reached ({} characters)",
        full.chars().count(),
    );
    assert!(
        bp_platform::paths::needs_extended_length_prefix(&full),
        "bp-platform does not think this path needs the escape hatch"
    );
    assert!(
        bp_platform::paths::to_extended_length(&full).is_some(),
        "bp-platform cannot even express the fix for this path"
    );

    // And bp-files, which has never heard of any of that, refuses.
    atomic_write(&path, b"body\n", SaveOptions::default()).unwrap_or_else(|e| {
        panic!(
            "bp-files could not write a name bp-naming generated and bp-platform \
             calls legal; bp-platform's own to_extended_length would have made it \
             writable. The error blames the wrong thing: {e}"
        )
    });
}
