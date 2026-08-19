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
//! and the document is gone. The device-name half was one of the two ways it
//! did not hold, and is now fixed: `bp-naming` tests the stem, as Win32 does.
//! The other way is still pinned as an ignored test at the bottom of this
//! file, and is a real defect.
//!
//! Nothing here writes outside a `tempfile` directory. The only strings
//! involved are titles, drawn from a fixed alphabet, and none is a secret.

mod common;

use common::no_files;

use bp_files::{SaveOptions, atomic_write};
use bp_naming::{SemanticName, sanitize_title};
use bp_platform::{
    Platform,
    paths::{
        MAX_COMPONENT_LEN, PathProblem, WINDOWS_RESERVED_NAMES, component_len, file_name_problems,
        reserved_device_name,
    },
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
    // The easy half, kept separate from the dotted one below so a regression
    // says which of the two rules broke.
    for title in [
        "CON", "con", "NUL", "aux", "LPT1", "prn", "CONIN$", "conout$",
    ] {
        name_is_legal_and_writable(title, "txt");
    }
}

#[test]
fn every_device_name_bp_platform_knows_is_one_bp_naming_defuses() {
    // The two crates keep separate lists, on purpose: `bp-naming` depends on
    // nothing else in the workspace, and that is worth more than sharing a
    // constant. What it costs is the risk of the lists drifting apart, and
    // this is the payment -- `bp-platform` names the devices, `bp-naming` has
    // to defuse every one of them, bare and with anything at all after a dot.
    for &device in WINDOWS_RESERVED_NAMES {
        for title in [device.to_owned(), format!("{device}.txt")] {
            let name = SemanticName::new(&title, a_date(), "txt").to_filename();
            assert!(
                reserved_device_name(&name).is_none(),
                "bp-naming let {title:?} through as {name:?}; bp-platform calls that the                  {device} device, and on Windows saving to it destroys the document",
            );
            name_is_legal(&name);
        }
    }
}

#[test]
fn a_device_name_before_a_dot_is_defused() {
    // The defect this file was written to find. A dot is not a forbidden
    // character, so it survives sanitising -- and Win32 asks about the stem
    // before the first dot, not about the whole name. Opening a reserved name
    // succeeds and reads or writes the DEVICE: the save reports success and
    // the document is gone.
    //
    // The write is attempted for each of these, because the whole point is
    // that the generated name is an ordinary file. It is a name like
    // `con File.txt_19AUG2026.txt`, whose stem is `con File` and names
    // nothing.
    for title in [
        "con.txt",
        "CON.notes",
        "aux.log",
        "NUL.dat",
        "lpt1.bak",
        "CONIN$.notes",
        "CONOUT$.notes",
        "prn.2026",
        // The first dot, not the last.
        "con.tar.gz",
        // Win32 ignores trailing spaces in the stem, so this is the console.
        "con .txt",
    ] {
        name_is_legal_and_writable(title, "txt");
    }
}

#[test]
fn a_word_that_merely_starts_with_a_device_name_is_left_alone() {
    // The other direction, and the one an over-eager fix breaks: defusing is
    // a stem match, so `CONTENTS` and `console.log` are ordinary titles and
    // must reach the filename with their spelling intact.
    for title in ["CONTENTS", "console.log", "AUXILIARY", "printer.cfg"] {
        assert_eq!(
            sanitize_title(title),
            title,
            "{title:?} was defused wrongly"
        );
        name_is_legal_and_writable(title, "txt");
    }
}

proptest! {
    #![proptest_config(no_files(256))]

    /// The property, over generated titles.
    ///
    /// The alphabet is every class that has ever caused this to break --
    /// forbidden characters, separators, dots, spaces, controls, multibyte and
    /// astral characters. It *can* spell a device name followed by a dot, but
    /// only by accident and not often enough to be evidence, so that class has
    /// its own strategy in
    /// `every_device_name_before_any_suffix_is_defused` below.
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

// --- what was defect one --------------------------------------------------

proptest! {
    #![proptest_config(no_files(256))]

    /// Every device name, followed by anything at all, is defused.
    ///
    /// This was a defect and is the reason the sanitiser tests the stem: the
    /// exact-match rule it replaced caught `CON` and missed `con.txt`, and the
    /// second of those destroys the document it claims to have saved. The
    /// suffix alphabet includes dots and spaces because those are what decide
    /// where the stem ends.
    #[test]
    fn every_device_name_before_any_suffix_is_defused(
        device in prop::sample::select(WINDOWS_RESERVED_NAMES),
        upper in prop::bool::ANY,
        suffix in "[a-zA-Z0-9 .]{0,12}",
    ) {
        let device = if upper { device.to_uppercase() } else { device.to_lowercase() };
        let title = format!("{device}{suffix}");
        let name = SemanticName::new(&title, a_date(), "txt").to_filename();

        prop_assert!(
            reserved_device_name(&name).is_none(),
            "{title:?} became {name:?}, which names a device",
        );
        prop_assert!(problems_on_both(&name).is_empty(), "{name:?}");
    }

    /// The two reserved lists agree, in both directions.
    ///
    /// The alphabet is exactly the characters sanitising leaves untouched, so
    /// the only reason `sanitize_title` can change one of these strings is
    /// that it decided the string names a device. That makes the equality
    /// below a direct comparison of the two crates' verdicts: neither a name
    /// `bp-platform` reserves may pass, nor one it does not may be mangled.
    #[test]
    fn the_two_reserved_lists_decide_the_same_stems(stem in "[A-Za-z0-9$]{1,8}") {
        let defused = sanitize_title(&stem) != stem;
        prop_assert_eq!(
            defused,
            reserved_device_name(&stem).is_some(),
            "the crates disagree about {:?}: bp-naming says {}, bp-platform says {}",
            stem,
            if defused { "device" } else { "file" },
            if reserved_device_name(&stem).is_some() { "device" } else { "file" },
        );
    }
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
