//! Tests for the path rules.
//!
//! Every one of them runs on both legs of CI, because every rule under test
//! takes [`Platform`] as an argument. That is the whole reason this module can
//! assert what Windows does while standing on Linux.

use super::*;
use proptest::prelude::*;

/// Whether `problems` contains a variant of the same shape as `wanted`,
/// ignoring the payload -- the assertions below care which rule fired, not
/// which characters it echoed back.
fn has<F: Fn(&PathProblem) -> bool>(problems: &[PathProblem], wanted: F) -> bool {
    problems.iter().any(wanted)
}

fn is_reserved(p: &PathProblem) -> bool {
    matches!(p, PathProblem::ReservedName { .. })
}
fn is_forbidden(p: &PathProblem) -> bool {
    matches!(p, PathProblem::ForbiddenCharacter { .. })
}
fn is_trailing(p: &PathProblem) -> bool {
    matches!(p, PathProblem::TrailingDotOrSpace { .. })
}
fn is_too_long(p: &PathProblem) -> bool {
    matches!(p, PathProblem::ComponentTooLong { .. })
}
fn is_path_too_long(p: &PathProblem) -> bool {
    matches!(p, PathProblem::PathTooLong { .. })
}

/// Names built from the characters that actually break something, plus a
/// reserved device stem often enough that the de-reserving branch is reached.
/// `any::<String>()` would spend its whole budget on lowercase letters.
fn nasty_body() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop_oneof![
            prop::char::range('a', 'z'),
            Just('.'),
            Just(' '),
            Just(':'),
            Just('/'),
            Just('\\'),
            Just('*'),
            Just('?'),
            Just('<'),
            Just('>'),
            Just('|'),
            Just('"'),
            Just('\n'),
            Just('\t'),
            Just('\0'),
            Just('é'),
            Just('\u{1f600}'),
        ],
        0..300,
    )
    .prop_map(|chars| chars.into_iter().collect())
}

fn proposed_name() -> impl Strategy<Value = String> {
    prop_oneof![
        3 => nasty_body(),
        1 => (prop::sample::select(WINDOWS_RESERVED_NAMES), nasty_body())
            .prop_map(|(reserved, body)| format!("{reserved}{body}")),
    ]
}

// --- reserved device names -------------------------------------------------

#[test]
fn a_device_name_is_reserved_whatever_it_is_dressed_up_as() {
    for name in [
        "CON",
        "con",
        "Con.txt",
        "CON.tar.gz",
        "NUL",
        "aux",
        "COM1",
        "LPT9",
        "CONIN$",
    ] {
        assert!(
            reserved_device_name(name).is_some(),
            "{name} should name a device"
        );
        assert!(
            has(&file_name_problems(Platform::Windows, name), is_reserved),
            "{name} should be refused on Windows"
        );
        assert!(
            !has(&file_name_problems(Platform::Linux, name), is_reserved),
            "{name} is an ordinary file on Linux"
        );
    }
}

#[test]
fn trailing_spaces_do_not_hide_a_device_name() {
    // Win32 strips them before it looks the name up, so `CON   ` is `CON`.
    assert_eq!(reserved_device_name("CON   "), Some("CON"));
    assert_eq!(reserved_device_name("CON.txt   "), Some("CON"));
}

#[test]
fn a_name_that_merely_starts_like_a_device_is_a_file() {
    for name in [
        "CONSOLE", "CONS.txt", "COM0", "COM10", "LPT", "NULL", "con-fig",
    ] {
        assert_eq!(reserved_device_name(name), None, "{name}");
        assert!(is_portable_file_name(name), "{name}");
    }
}

// --- forbidden characters --------------------------------------------------

#[test]
fn the_characters_windows_forbids_are_ordinary_on_linux() {
    for name in [
        "report: q3.txt",
        "what?.md",
        "a<b>.txt",
        "quote\".txt",
        "pipe|d.log",
        "star*.csv",
        "line\nbreak.txt",
    ] {
        assert!(
            has(&file_name_problems(Platform::Windows, name), is_forbidden),
            "Windows should refuse {name:?}"
        );
        assert!(
            file_name_problems(Platform::Linux, name).is_empty(),
            "Linux should accept {name:?}"
        );
    }
}

#[test]
fn only_the_separator_and_nul_are_forbidden_on_linux() {
    assert!(has(
        &file_name_problems(Platform::Linux, "a/b.txt"),
        is_forbidden
    ));
    assert!(has(
        &file_name_problems(Platform::Linux, "a\0b.txt"),
        is_forbidden
    ));
    assert!(file_name_problems(Platform::Linux, "a\\b.txt").is_empty());
    // ...and that backslash is exactly why the same name will not travel.
    assert!(has(
        &file_name_problems(Platform::Windows, "a\\b.txt"),
        is_forbidden
    ));
}

#[test]
fn a_separator_is_a_name_problem_but_not_a_path_problem() {
    // The path checker splits first, so it must not report what splitting
    // consumed -- otherwise every path on Windows is invalid.
    assert!(path_problems(Platform::Windows, r"docs\notes.txt").is_empty());
    assert!(has(
        &file_name_problems(Platform::Windows, r"docs\notes.txt"),
        is_forbidden
    ));
}

// --- trailing dots and spaces ----------------------------------------------

#[test]
fn a_trailing_dot_or_space_is_a_windows_only_trap() {
    for name in ["notes.", "notes ", "folder."] {
        assert!(
            has(&file_name_problems(Platform::Windows, name), is_trailing),
            "{name:?}"
        );
        assert!(
            file_name_problems(Platform::Linux, name).is_empty(),
            "{name:?}"
        );
    }
    assert!(has(
        &path_problems(Platform::Windows, r"C:\notes \report.txt"),
        is_trailing
    ));
}

#[test]
fn dot_and_dotdot_are_path_components_but_not_file_names() {
    for name in [".", ".."] {
        assert!(path_problems(Platform::Linux, name).is_empty(), "{name}");
        assert!(path_problems(Platform::Windows, name).is_empty(), "{name}");
        assert!(
            has(&file_name_problems(Platform::Linux, name), |p| matches!(
                p,
                PathProblem::DotName { .. }
            )),
            "{name}"
        );
    }
    assert!(is_portable_file_name(".gitignore"));
}

// --- lengths ---------------------------------------------------------------

#[test]
fn a_component_may_be_two_hundred_and_fifty_five_units_and_no_more() {
    let ok = "a".repeat(MAX_COMPONENT_LEN);
    let over = "a".repeat(MAX_COMPONENT_LEN + 1);
    for &platform in Platform::ALL {
        assert!(!has(&file_name_problems(platform, &ok), is_too_long));
        assert!(has(&file_name_problems(platform, &over), is_too_long));
    }
}

#[test]
fn the_length_limit_is_counted_in_different_units_on_each_platform() {
    // 200 accented characters: 200 UTF-16 units (Windows is happy) and 400
    // UTF-8 bytes (Linux is not). The asymmetry runs the *other* way to the
    // usual one, which is precisely why a single "is this name ok" check with
    // one unit is wrong on one of the two platforms.
    let name = "é".repeat(200);
    assert_eq!(component_len(Platform::Windows, &name), 200);
    assert_eq!(component_len(Platform::Linux, &name), 400);
    assert!(file_name_problems(Platform::Windows, &name).is_empty());
    assert!(has(
        &file_name_problems(Platform::Linux, &name),
        is_too_long
    ));
}

#[test]
fn max_path_is_two_hundred_and_fifty_nine_and_the_prefix_lifts_it() {
    let long = format!(r"C:\{}", "a".repeat(300));
    assert!(has(&path_problems(Platform::Windows, &long), |p| matches!(
        p,
        PathProblem::PathTooLong { .. }
    )));
    assert!(needs_extended_length_prefix(&long));

    let prefixed = to_extended_length(&long).expect("a fully qualified drive path converts");
    assert!(!needs_extended_length_prefix(&prefixed));
    assert!(!has(&path_problems(Platform::Windows, &prefixed), |p| {
        matches!(p, PathProblem::PathTooLong { .. })
    }));

    // The same path is nowhere near Linux's limit -- Linux objects to it, but
    // for the unrelated reason that it has no separators, so on Linux the
    // whole thing is one 303-byte component.
    assert!(!has(
        &path_problems(Platform::Linux, &long),
        is_path_too_long
    ));
    assert!(has(&path_problems(Platform::Linux, &long), is_too_long));
}

#[test]
fn the_path_length_boundary_is_where_it_is_documented() {
    // The numbers themselves, spelled out, because a boundary test that takes
    // its input from `max_path_len` proves only that the function agrees with
    // itself -- it stays green if the constant is wrong, which is the one
    // failure worth catching here. 259 is MAX_PATH less its NUL; 4095 is
    // PATH_MAX less its NUL.
    assert_eq!(max_path_len(Platform::Windows), 259);
    assert_eq!(max_path_len(Platform::Linux), 4095);

    for &platform in Platform::ALL {
        let limit = max_path_len(platform);
        let at_limit = "a".repeat(limit);
        let over = "a".repeat(limit + 1);
        assert!(
            !has(&path_problems(platform, &at_limit), is_path_too_long),
            "{platform:?} refused a path of exactly {limit}"
        );
        assert!(
            has(&path_problems(platform, &over), is_path_too_long),
            "{platform:?} accepted a path of {}",
            limit + 1
        );
    }
}

#[test]
fn an_empty_path_is_reported_once_and_not_as_a_length() {
    assert_eq!(path_problems(Platform::Linux, ""), vec![PathProblem::Empty]);
    assert_eq!(
        file_name_problems(Platform::Windows, ""),
        vec![PathProblem::Empty]
    );
}

#[test]
fn repeated_separators_are_not_a_problem() {
    assert!(path_problems(Platform::Linux, "a//b///c").is_empty());
    assert!(path_problems(Platform::Windows, r"C:\\a\\b").is_empty());
}

// --- what the user is told -------------------------------------------------

#[test]
fn every_problem_can_be_explained_without_leaking_a_control_character() {
    let problems = [
        PathProblem::Empty,
        PathProblem::DotName {
            component: "..".into(),
        },
        PathProblem::ReservedName {
            component: "con.txt".into(),
            reserved: "CON",
        },
        PathProblem::ForbiddenCharacter {
            component: "a:b".into(),
            character: ':',
        },
        PathProblem::ForbiddenCharacter {
            component: "a\nb".into(),
            character: '\n',
        },
        PathProblem::TrailingDotOrSpace {
            component: "notes ".into(),
        },
        PathProblem::ComponentTooLong {
            component: "x".repeat(300),
            len: 300,
            limit: 255,
        },
        PathProblem::PathTooLong {
            len: 400,
            limit: 259,
        },
    ];
    for problem in &problems {
        let message = problem.describe();
        assert!(!message.is_empty(), "{problem:?}");
        assert!(
            !message.chars().any(char::is_control),
            "{problem:?} put a control character in a message"
        );
        // A message the length of the offending name is not a message.
        assert!(message.len() < 300, "{problem:?}");
    }
}

// --- case sensitivity ------------------------------------------------------

#[test]
fn case_decides_identity_on_one_platform_and_not_the_other() {
    assert!(!same_file_name(Platform::Linux, "Notes.txt", "notes.txt"));
    assert!(same_file_name(Platform::Windows, "Notes.txt", "notes.txt"));
    assert!(same_file_name(Platform::Windows, "Ä.txt", "ä.txt"));
    assert!(!same_file_name(Platform::Windows, "notes.txt", "note.txt"));
}

#[test]
fn a_set_that_is_fine_on_linux_can_lose_a_file_on_windows() {
    let names = ["Notes.txt", "budget.csv", "notes.txt"];
    assert_eq!(first_collision(Platform::Linux, &names), None);
    assert_eq!(
        first_collision(Platform::Windows, &names),
        Some(("Notes.txt", "notes.txt"))
    );
}

// --- extended-length paths -------------------------------------------------

#[test]
fn the_prefix_is_added_only_where_it_has_a_defined_meaning() {
    assert_eq!(
        to_extended_length(r"C:\Users\me\notes.txt").as_deref(),
        Some(r"\\?\C:\Users\me\notes.txt")
    );
    assert_eq!(
        to_extended_length(r"\\server\share\notes.txt").as_deref(),
        Some(r"\\?\UNC\server\share\notes.txt")
    );
    // Forward slashes are illegal after the prefix, so they are translated.
    assert_eq!(
        to_extended_length("C:/Users/me/notes.txt").as_deref(),
        Some(r"\\?\C:\Users\me\notes.txt")
    );
    // Already prefixed: unchanged, not double-prefixed.
    assert_eq!(
        to_extended_length(r"\\?\C:\a").as_deref(),
        Some(r"\\?\C:\a")
    );
}

#[test]
fn the_prefix_is_refused_where_it_would_change_where_the_path_points() {
    // Relative: the working directory is not applied to an extended path.
    assert_eq!(to_extended_length("notes.txt"), None);
    assert_eq!(to_extended_length(r"docs\notes.txt"), None);
    // Drive-relative and root-relative are not fully qualified either.
    assert_eq!(to_extended_length(r"C:notes.txt"), None);
    assert_eq!(to_extended_length(r"\notes.txt"), None);
    // `.` and `..` are not resolved after the prefix; they would become
    // directory names, so the conversion refuses rather than silently
    // relocating the file.
    assert_eq!(to_extended_length(r"C:\a\..\b"), None);
    assert_eq!(to_extended_length(r"C:\a\.\b"), None);
}

// --- components ------------------------------------------------------------

#[test]
fn windows_splits_on_both_separators_and_linux_on_one() {
    let path = r"C:\docs\notes.txt";
    assert_eq!(
        components(Platform::Windows, path).collect::<Vec<_>>(),
        vec!["docs", "notes.txt"]
    );
    assert_eq!(
        components(Platform::Linux, path).collect::<Vec<_>>(),
        vec![r"C:\docs\notes.txt"]
    );
    assert_eq!(
        components(Platform::Windows, r"\\server\share\a\b").collect::<Vec<_>>(),
        vec!["a", "b"]
    );
    assert_eq!(
        components(Platform::Windows, r"\\?\UNC\server\share\a").collect::<Vec<_>>(),
        vec!["a"]
    );
    assert_eq!(
        components(Platform::Linux, "/home/me/notes.txt").collect::<Vec<_>>(),
        vec!["home", "me", "notes.txt"]
    );
}

#[test]
fn a_drive_letters_colon_is_not_reported_as_a_forbidden_character() {
    assert!(path_problems(Platform::Windows, r"C:\docs\notes.txt").is_empty());
    assert!(path_problems(Platform::Windows, r"\\?\C:\docs\notes.txt").is_empty());
    assert!(path_problems(Platform::Windows, r"\\server\share\notes.txt").is_empty());
}

// --- absoluteness and joining ----------------------------------------------

#[test]
fn the_two_windows_shapes_that_look_absolute_and_are_not() {
    assert!(is_absolute(Platform::Windows, r"C:\Users\me"));
    assert!(is_absolute(Platform::Windows, "C:/Users/me"));
    assert!(is_absolute(Platform::Windows, r"\\server\share"));
    assert!(is_absolute(Platform::Windows, r"\\?\C:\a"));
    // Relative to the current directory *of drive C*.
    assert!(!is_absolute(Platform::Windows, r"C:notes"));
    // Relative to the current drive.
    assert!(!is_absolute(Platform::Windows, r"\notes"));
    assert!(!is_absolute(Platform::Windows, "notes"));
}

#[test]
fn absoluteness_is_answered_for_the_platform_named_not_the_one_compiled_for() {
    // The whole reason this function exists rather than
    // `std::path::Path::is_absolute`: each of these is absolute on exactly one
    // platform, and std would agree with only whichever leg of CI is running.
    assert!(is_absolute(Platform::Windows, r"C:\Users\me"));
    assert!(!is_absolute(Platform::Linux, r"C:\Users\me"));
    assert!(is_absolute(Platform::Linux, "/home/me"));
    assert!(!is_absolute(Platform::Windows, "/home/me"));
}

#[test]
fn joining_uses_the_named_platforms_separator_and_does_not_double_it() {
    assert_eq!(
        join(Platform::Windows, r"C:\Users\me", &["bachelorpad", "cache"]),
        r"C:\Users\me\bachelorpad\cache"
    );
    assert_eq!(
        join(Platform::Linux, "/home/me/.config", &["bachelorpad"]),
        "/home/me/.config/bachelorpad"
    );
    assert_eq!(
        join(Platform::Windows, r"C:\Users\me\", &["bachelorpad"]),
        r"C:\Users\me\bachelorpad"
    );
    assert_eq!(join(Platform::Linux, "/home/me/", &["", "a"]), "/home/me/a");
}

// --- properties ------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Whatever a caller proposes, the sanitised name is legal on both
    /// platforms. This is the promise the whole function exists to make, and
    /// the only one worth stating over arbitrary input.
    #[test]
    fn any_proposed_name_sanitises_to_one_that_travels(proposed in proposed_name()) {
        let name = to_portable_file_name(&proposed);
        prop_assert!(
            is_portable_file_name(&name),
            "{proposed:?} became {name:?}, which is still not portable: {:?}",
            file_name_problems(Platform::Windows, &name),
        );
    }

    /// Sanitising twice is sanitising once. Without this a name that survives
    /// a save, a sync and a reopen creeps an underscore longer each round.
    #[test]
    fn sanitising_is_idempotent(proposed in proposed_name()) {
        let once = to_portable_file_name(&proposed);
        prop_assert_eq!(to_portable_file_name(&once), once);
    }

    /// A name already legal everywhere is returned untouched. A sanitiser
    /// that rewrites good names is one users learn to route around.
    #[test]
    fn a_name_that_already_travels_is_left_alone(
        stem in "[a-zA-Z][a-zA-Z0-9 _-]{0,40}",
        ext in "[a-z]{1,5}",
    ) {
        let name = format!("{stem}.{ext}");
        prop_assume!(is_portable_file_name(&name));
        prop_assert_eq!(to_portable_file_name(&name), name);
    }

    /// For ASCII names, Windows is the stricter platform on every axis, so
    /// anything Windows accepts Linux accepts too. (It stops being true
    /// outside ASCII, where Linux's byte limit bites first -- see
    /// `the_length_limit_is_counted_in_different_units_on_each_platform`.)
    #[test]
    fn windows_is_the_stricter_platform_for_ascii_names(
        name in "[a-zA-Z0-9._ ()-]{0,300}",
    ) {
        prop_assume!(file_name_problems(Platform::Windows, &name).is_empty());
        prop_assert!(
            file_name_problems(Platform::Linux, &name).is_empty(),
            "{name:?} is legal on Windows and not on Linux",
        );
    }

    /// Windows' notion of "same file" is coarser than Linux's, so a set that
    /// collides on Linux always collides on Windows. The converse is the
    /// defect this pair of functions exists to find.
    #[test]
    fn a_linux_collision_is_always_a_windows_collision(
        names in prop::collection::vec("[a-zA-Z0-9]{1,6}", 0..8),
    ) {
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        if first_collision(Platform::Linux, &refs).is_some() {
            prop_assert!(first_collision(Platform::Windows, &refs).is_some());
        }
    }

    /// Whatever the prefix conversion accepts, it produces a path that is
    /// prefixed, contains no forward slash, and does not change when converted
    /// again -- the three things the object manager requires of it.
    #[test]
    fn a_converted_path_satisfies_what_the_prefix_demands(
        drive in "[A-Z]",
        rest in prop::collection::vec("[a-z0-9]{1,8}", 0..6),
    ) {
        let path = format!(r"{drive}:\{}", rest.join("\\"));
        let converted = to_extended_length(&path).expect("a drive path is fully qualified");
        prop_assert!(has_extended_length_prefix(&converted));
        prop_assert!(!converted.contains('/'));
        let again = to_extended_length(&converted);
        prop_assert_eq!(again.as_deref(), Some(converted.as_str()));
    }

    /// Splitting a path into components and joining them back with the
    /// platform's own separator yields the same components. A splitter that
    /// loses or invents one silently corrupts every path built on top of it.
    #[test]
    fn components_survive_a_join_and_a_second_split(
        parts in prop::collection::vec("[a-z0-9]{1,8}", 1..6),
        platform in prop::sample::select(Platform::ALL),
    ) {
        let joined = parts.join(&platform.separator().to_string());
        prop_assert_eq!(components(platform, &joined).collect::<Vec<_>>(), parts);
    }
}
