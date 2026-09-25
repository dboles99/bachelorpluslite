//! Every flag the executable accepts, in one place, and the two that answer
//! instead of running.
//!
//! This module owns the **inventory**: what may be typed, what each one
//! means, and how `--help` and `--version` read. It deliberately does *not*
//! act on any of it. Six of these flags are settings and [`crate::resolve`]
//! applies them; the rest belong to the shell (`apps/bachelorpad`) or to
//! `bp-ui`.
//!
//! **All of them are listed here anyway, and that is the point.** A list
//! holding only the flags this crate acts on could not answer the question
//! that makes it worth having -- *is what the user typed a flag, or a typo?*
//! Until this module existed, `--font_size=20` did nothing and said nothing,
//! which is the exact failure [`crate::Notice::UnknownKey`] had already been
//! written to prevent on the config-file side of the same precedence chain.
//! One layer reported a typo and the other swallowed it, for no reason
//! anybody had chosen.
//!
//! It owns no side effects: nothing here reads the environment, the
//! filesystem or the clock, so every answer is a pure function of the
//! arguments and can be asserted without a process.

use std::fmt::Write as _;

/// Which part of `--help` a flag appears under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    /// Answers and exits without opening a window.
    General,
    /// About this invocation rather than about the product: nothing here is
    /// remembered, and neither the config file nor the environment carries
    /// any of it. That is the whole distinction from [`Section::Setting`],
    /// and it is why `--line` is not a setting despite looking like one.
    Opening,
    /// Overrides a value the config file and the environment also carry.
    /// [`crate::resolve`] is what applies these.
    Setting,
    /// For the gate, the benchmarks, and whoever is trying the surface that
    /// has not reached parity yet. Shown rather than hidden: a flag a user
    /// can type is a flag they can mistype, and hiding it would leave the
    /// list unable to tell the two apart.
    Development,
}

impl Section {
    /// The heading this section is printed under.
    #[must_use]
    pub const fn heading(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Opening => "Opening a file",
            Self::Setting => "Settings",
            Self::Development => "Development",
        }
    }

    /// In the order `--help` prints them.
    pub const ALL: &'static [Section] = &[
        Section::General,
        Section::Opening,
        Section::Setting,
        Section::Development,
    ];
}

/// One flag, as the product accepts it and as `--help` shows it.
#[derive(Debug, Clone, Copy)]
pub struct Flag {
    /// The spellings that select this flag, dashes and any `=value`
    /// stripped. The first is the long form; any others are short ones.
    pub names: &'static [&'static str],
    /// The placeholder for the value it takes, or `None` for a switch.
    ///
    /// This field is the *only* statement of whether a flag takes a value:
    /// [`Flag::spelling`] renders from it rather than storing a second copy,
    /// so a flag cannot be documented as taking a value it will not accept.
    pub value: Option<&'static str>,
    /// One line, saying what it does.
    pub summary: &'static str,
    pub section: Section,
}

impl Flag {
    /// How the flag is written in `--help`: every spelling, with the value
    /// placeholder on the long one.
    #[must_use]
    pub fn spelling(&self) -> String {
        let mut out = String::new();
        for (i, name) in self.names.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            // A one-character name takes a single dash; anything longer takes
            // two. That is the only rule this product needs, because the long
            // form is always first and short forms are always one character.
            if name.chars().count() == 1 {
                out.push('-');
            } else {
                out.push_str("--");
            }
            out.push_str(name);
            if i == 0
                && let Some(placeholder) = self.value
            {
                out.push('=');
                out.push_str(placeholder);
            }
        }
        out
    }

    /// True if `name` (dashes and value already stripped) selects this flag.
    #[must_use]
    pub fn matches(&self, name: &str) -> bool {
        self.names.contains(&name)
    }
}

/// Everything the executable accepts.
///
/// The order is the order `--help` prints within each section, so it is
/// arranged for a reader rather than alphabetically.
pub const FLAGS: &[Flag] = &[
    Flag {
        names: &["help", "h"],
        value: None,
        summary: "Print this and exit.",
        section: Section::General,
    },
    Flag {
        names: &["version", "V"],
        value: None,
        summary: "Print the version and licence, and exit.",
        section: Section::General,
    },
    Flag {
        names: &["line"],
        value: Some("N"),
        summary: "Put the caret on this line of the first file.",
        section: Section::Opening,
    },
    Flag {
        names: &["theme"],
        value: Some("NAME"),
        summary: "Start in a named theme, e.g. Light, Dark, Green.",
        section: Section::Setting,
    },
    Flag {
        names: &["font-size"],
        value: Some("N"),
        summary: "Editor font size in points.",
        section: Section::Setting,
    },
    Flag {
        names: &["tab-width"],
        value: Some("N"),
        summary: "Columns a Tab advances to.",
        section: Section::Setting,
    },
    Flag {
        names: &["indent-spaces"],
        value: Some("true|false"),
        summary: "Insert spaces for Tab, rather than a tab.",
        section: Section::Setting,
    },
    Flag {
        names: &["renderer"],
        value: Some("NAME"),
        summary: "software or platform. Software is the default.",
        section: Section::Setting,
    },
    Flag {
        names: &["log"],
        value: Some("FILTER"),
        summary: "Tracing filter, e.g. warn or bp_ui=debug.",
        section: Section::Setting,
    },
    Flag {
        names: &["editor-view"],
        value: None,
        summary: "Draw with the custom editor surface.",
        section: Section::Development,
    },
    Flag {
        names: &["self-check"],
        value: None,
        summary: "Load, print a marker and exit. For the gate.",
        section: Section::Development,
    },
    Flag {
        names: &["measure-exit"],
        value: None,
        summary: "Print the time to the first frame, then exit.",
        section: Section::Development,
    },
    Flag {
        names: &["latency-probe"],
        value: None,
        summary: "Run the input-latency probe, then exit.",
        section: Section::Development,
    },
];

/// What the executable knows about itself, handed in by the shell.
///
/// Passed rather than read from this crate's own `CARGO_PKG_*`, so
/// `--version` reports the **binary's** identity and not this library's.
/// They are the same values today, because every crate in the workspace
/// inherits `[workspace.package]` -- and "the same today" is exactly the kind
/// of claim that stops being true without anybody noticing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Package {
    /// `env!("CARGO_PKG_VERSION")` at the shell.
    pub version: &'static str,
    /// `env!("CARGO_PKG_LICENSE")` at the shell. The workspace manifest is
    /// the one home for this string and `LICENSE` at the repository root is
    /// the text behind it (ADR-0071).
    ///
    /// **"The one home" was false for as long as nothing asked.** The About
    /// box in `bp-ui` carried the licence as a string literal, so the two
    /// disagreed the moment either changed -- trap 4, and it survived a
    /// relicence being planned rather than being found by one. About now
    /// reads `CARGO_PKG_LICENSE` too, which is what makes this sentence a
    /// property of the code rather than a wish about it.
    pub license: &'static str,
}

/// The flag name a typed argument selects, if it is a flag at all.
///
/// `--font-size=18` gives `Some("font-size")`, `-h` gives `Some("h")`, and
/// `notes.txt` gives `None`.
///
/// A bare `-` or `--` also gives `None`: they select nothing, and this
/// product gives neither a meaning. If `--` ever becomes an end-of-flags
/// marker -- the way to open a file whose name begins with a dash -- this is
/// where it starts.
#[must_use]
pub fn selector(arg: &str) -> Option<&str> {
    let rest = arg.strip_prefix("--").or_else(|| arg.strip_prefix('-'))?;
    let name = rest.split('=').next().unwrap_or(rest);
    if name.is_empty() { None } else { Some(name) }
}

/// The flag `name` selects, if the product accepts one by that spelling.
#[must_use]
pub fn find(name: &str) -> Option<&'static Flag> {
    FLAGS.iter().find(|flag| flag.matches(name))
}

/// Was `--help` asked for?
#[must_use]
pub fn asked_for_help(args: &[String]) -> bool {
    asked_for(args, "help")
}

/// Was `--version` asked for?
#[must_use]
pub fn asked_for_version(args: &[String]) -> bool {
    asked_for(args, "version")
}

fn asked_for(args: &[String], long_name: &str) -> bool {
    args.iter()
        .filter_map(|a| selector(a))
        .any(|name| find(name).is_some_and(|flag| flag.names.first() == Some(&long_name)))
}

/// The line `--line=N` asks the first file to open at.
///
/// `Ok(None)` when it was not typed. `Err` carries what was typed instead of
/// a number, because a `--line` that quietly does nothing is exactly the
/// failure this module exists to stop -- and the value is the only part of it
/// the user can act on.
///
/// **Zero is not a line.** This product numbers lines from one everywhere a
/// person sees one -- the status bar, Go to Line, a find result -- so
/// `--line=0` is a fencepost mistake rather than a request for the line
/// before the first.
///
/// The last one typed wins, matching how [`crate::resolve`] applies a setting
/// given twice.
// No `#[must_use]`: `Result` already carries one, and clippy's
// `double_must_use` is denied by the gate.
pub fn line(args: &[String]) -> Result<Option<usize>, String> {
    let mut typed = None;
    for arg in args {
        if selector(arg) != Some("line") {
            continue;
        }
        // A bare `--line` has already been reported by `resolve` as needing a
        // value; saying it twice in two voices would be worse than once.
        if let Some((_, value)) = arg.split_once('=') {
            typed = Some(value);
        }
    }
    let Some(value) = typed else {
        return Ok(None);
    };
    match value.trim().parse::<usize>() {
        Ok(n) if n >= 1 => Ok(Some(n)),
        _ => Err(value.to_owned()),
    }
}

/// One line for `--version`, plus the product's description and licence.
///
/// Three lines rather than one because this is what somebody pastes into a
/// bug report, and the licence is the part a packager needs.
#[must_use]
pub fn version(package: Package) -> String {
    format!(
        "{} {}\n{}\n{}\n",
        bp_platform::DISPLAY_NAME,
        package.version,
        bp_platform::DESCRIPTION,
        package.license,
    )
}

/// The whole of `--help`.
///
/// Rendered from [`FLAGS`] rather than written out, so a flag that is added
/// without a line here is not possible -- the list is the documentation.
#[must_use]
pub fn help(package: Package) -> String {
    // Measured rather than fixed: a constant column width goes stale the
    // first time a longer flag is added, and goes stale silently, by
    // producing a ragged line nobody is looking at.
    let column = FLAGS
        .iter()
        .map(|flag| flag.spelling().chars().count())
        .max()
        .unwrap_or(0);

    let mut out = format!(
        "{} {} -- {}\n\nUsage:\n  bachelorpad [options] [file...]\n\n\
         Every file named is opened in its own tab. How it opens is decided\n\
         from its size before a byte is read, so there is no flag for that.\n",
        bp_platform::DISPLAY_NAME,
        package.version,
        bp_platform::DESCRIPTION,
    );

    for section in Section::ALL {
        out.push('\n');
        out.push_str(section.heading());
        out.push('\n');
        for flag in FLAGS.iter().filter(|f| f.section == *section) {
            let spelling = flag.spelling();
            let pad = column.saturating_sub(spelling.chars().count());
            // `write!` to a String cannot fail, and the alternative -- format!
            // and push_str -- allocates a second string per line to say so.
            let _ = writeln!(out, "  {spelling}{:pad$}  {}", "", flag.summary);
        }
    }

    out.push_str(
        "\nSettings also come from BACHELORPAD_* in the environment and from\n\
         config.toml in this product's configuration directory. The command\n\
         line wins, then the environment, then the file.\n",
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const PACKAGE: Package = Package {
        version: "9.9.9",
        license: "GPL-3.0-only",
    };

    #[test]
    fn a_long_flag_with_a_value_is_selected_by_its_name() {
        assert_eq!(selector("--font-size=18"), Some("font-size"));
        assert_eq!(selector("--editor-view"), Some("editor-view"));
        assert_eq!(selector("-h"), Some("h"));
    }

    #[test]
    fn a_file_name_is_not_a_flag() {
        assert_eq!(selector("notes.txt"), None);
        assert_eq!(selector(""), None);
    }

    #[test]
    fn a_bare_dash_selects_nothing_rather_than_an_empty_flag() {
        // Reported as unknown would be worse than ignored: neither is a
        // misspelling of anything, so there is nothing to tell the user.
        assert_eq!(selector("-"), None);
        assert_eq!(selector("--"), None);
    }

    #[test]
    fn a_log_filter_containing_an_equals_sign_keeps_all_of_it() {
        // `bp_ui=debug` is an ordinary filter, and splitting on the last `=`
        // rather than the first would silently truncate it to `bp_ui`.
        assert_eq!(selector("--log=bp_ui=debug"), Some("log"));
    }

    #[test]
    fn every_flag_has_a_long_name_first_and_single_character_short_ones() {
        for flag in FLAGS {
            let Some(long) = flag.names.first() else {
                panic!("a flag with no spelling cannot be typed");
            };
            assert!(
                long.chars().count() > 1,
                "{long} is a short name in the long position, so `spelling` \
                 would print it with one dash"
            );
            for short in flag.names.iter().skip(1) {
                assert_eq!(
                    short.chars().count(),
                    1,
                    "{short} is a long name in a short position"
                );
            }
        }
    }

    #[test]
    fn no_two_flags_share_a_spelling() {
        // `find` returns the first match, so a duplicate would make one of
        // the two unreachable -- and it would still be accepted, which is the
        // hard version of the bug to notice.
        let mut seen: Vec<&str> = Vec::new();
        for flag in FLAGS {
            for name in flag.names {
                assert!(!seen.contains(name), "{name} is spelled by two flags");
                seen.push(name);
            }
        }
    }

    #[test]
    fn a_switch_is_spelled_without_a_value_and_a_setting_with_one() {
        let help = find("help").expect("--help exists");
        assert_eq!(help.spelling(), "--help, -h");
        let size = find("font-size").expect("--font-size exists");
        assert_eq!(size.spelling(), "--font-size=N");
    }

    #[test]
    fn help_and_version_are_recognised_by_either_spelling() {
        let args =
            |list: &[&str]| -> Vec<String> { list.iter().map(|s| (*s).to_owned()).collect() };
        assert!(asked_for_help(&args(&["--help"])));
        assert!(asked_for_help(&args(&["bachelorpad", "-h", "notes.txt"])));
        assert!(asked_for_version(&args(&["-V"])));
        assert!(asked_for_version(&args(&["--version"])));
        assert!(!asked_for_help(&args(&["--version"])));
        assert!(!asked_for_version(&args(&["--help"])));
        // `-v` is not a spelling of anything here, and guessing that it means
        // `--version` would make a typo do something.
        assert!(!asked_for_version(&args(&["-v"])));
    }

    #[test]
    fn a_line_number_is_read_and_the_last_one_typed_wins() {
        let args =
            |list: &[&str]| -> Vec<String> { list.iter().map(|s| (*s).to_owned()).collect() };
        assert_eq!(line(&args(&["--line=427", "log.txt"])), Ok(Some(427)));
        assert_eq!(line(&args(&["--line=1", "--line=9"])), Ok(Some(9)));
        assert_eq!(line(&args(&["log.txt"])), Ok(None));
        // A bare `--line` is `resolve`'s to report, not this function's.
        assert_eq!(line(&args(&["--line"])), Ok(None));
    }

    #[test]
    fn a_line_that_is_not_a_line_number_says_what_was_typed() {
        let args =
            |list: &[&str]| -> Vec<String> { list.iter().map(|s| (*s).to_owned()).collect() };
        assert_eq!(line(&args(&["--line=last"])), Err("last".to_owned()));
        assert_eq!(line(&args(&["--line=-3"])), Err("-3".to_owned()));
        // Lines are numbered from one everywhere a person sees one, so zero
        // is a fencepost mistake and not the line before the first.
        assert_eq!(line(&args(&["--line=0"])), Err("0".to_owned()));
    }

    #[test]
    fn the_first_version_line_is_the_name_then_a_space_then_the_version() {
        // Both release scripts and `release.yml` read this line, and they
        // read it as *last field = version, everything before = name*. They
        // used to read the second field, which broke the day the name gained
        // a space (ADR-0074): every archive was named "Lite" and no release
        // could be built. This is the contract they rely on, asked here
        // because no script test runs on every commit.
        //
        // Asked of the version that will actually ship, not a test constant:
        // a release candidate is cut by bumping it to `x.y.z-rc.n`, and that
        // is the string the scripts will be handed.
        let shipping = Package {
            version: env!("CARGO_PKG_VERSION"),
            license: PACKAGE.license,
        };
        let rendered = version(shipping);
        let first = rendered.lines().next().unwrap_or_default();
        let (name, number) = first.rsplit_once(' ').unwrap_or_default();
        assert_eq!(name, bp_platform::DISPLAY_NAME, "got {first:?}");
        assert_eq!(number, shipping.version, "got {first:?}");
        // The scripts match `^\d+\.\d+\.\d+`: a pre-release suffix may
        // follow it, and a name-shaped word may not.
        let core = number.split('-').next().unwrap_or_default();
        assert!(
            core.split('.').count() == 3 && core.split('.').all(|part| part.parse::<u32>().is_ok()),
            "the release scripts refuse a version that does not start x.y.z: {number:?}"
        );
    }

    #[test]
    fn version_names_the_product_the_desktop_registration_names() {
        // Trap 4: two claims about the same product coexist for as long as
        // nothing asks. This asks. `AppInfo::bachelorpad` is what writes the
        // name into every `mimeapps.list` on the machine.
        let registered = bp_platform::editor::AppInfo::bachelorpad("/x/bachelorpad");
        let rendered = version(PACKAGE);
        assert!(
            rendered.starts_with(&registered.display_name),
            "got {rendered}"
        );
        assert!(rendered.contains(&registered.description), "got {rendered}");
    }

    #[test]
    fn version_reports_the_version_and_licence_it_was_handed() {
        let rendered = version(PACKAGE);
        assert!(rendered.contains("9.9.9"), "got {rendered}");
        assert!(rendered.contains("GPL-3.0-only"), "got {rendered}");
    }

    #[test]
    fn help_documents_every_flag_that_can_be_typed() {
        // The mechanism behind "the list is the documentation": a flag added
        // to `FLAGS` without a `--help` line is not a state this can reach.
        let rendered = help(PACKAGE);
        for flag in FLAGS {
            assert!(
                rendered.contains(&flag.spelling()),
                "{} is missing from --help",
                flag.spelling()
            );
            assert!(
                rendered.contains(flag.summary),
                "{} has no summary in --help",
                flag.spelling()
            );
        }
        for section in Section::ALL {
            assert!(rendered.contains(section.heading()), "got {rendered}");
        }
    }

    #[test]
    fn help_says_how_to_name_a_file_and_where_settings_come_from() {
        let rendered = help(PACKAGE);
        assert!(rendered.contains("bachelorpad [options] [file...]"));
        assert!(rendered.contains("BACHELORPAD_"));
        assert!(rendered.contains("config.toml"));
    }

    #[test]
    fn no_summary_is_long_enough_to_wrap_an_eighty_column_terminal() {
        // 80 columns is the floor a terminal is allowed to be. A summary that
        // wraps is not wrong, but it is the kind of wrong nobody sees until a
        // stranger runs `--help` in a narrow window.
        let column = FLAGS
            .iter()
            .map(|flag| flag.spelling().chars().count())
            .max()
            .unwrap_or(0);
        for flag in FLAGS {
            let width = 2 + column + 2 + flag.summary.chars().count();
            assert!(
                width <= 80,
                "{} takes {width} columns: {}",
                flag.spelling(),
                flag.summary
            );
        }
    }
}
