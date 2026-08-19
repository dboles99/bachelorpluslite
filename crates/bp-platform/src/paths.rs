//! The path rules that actually differ between Windows and Linux.
//!
//! An editor that saves files is an editor that invents file names -- from a
//! heading, from a date, from a search phrase, from whatever the user typed
//! into Save As. Most of those names are fine everywhere. The ones that are
//! not fail asymmetrically: `report: q3.txt` saves happily on Linux and is
//! rejected by Windows, `CON.txt` is an ordinary file on Linux and is the
//! console device on Windows, and `Notes.txt` beside `notes.txt` is two
//! documents on Linux and one on Windows. Every one of those is a data-loss
//! bug rather than an error message, which is why the knowledge lives in one
//! module with tests instead of being rediscovered per call site.
//!
//! **Every rule here takes [`Platform`] as a parameter.** Nothing in this
//! module is behind `cfg`, so the Windows rules are exercised by the Linux
//! leg of CI and the Linux rules by the Windows leg. Ask about the host by
//! passing [`Platform::HOST`].
//!
//! ## What this module does not do
//!
//! It does not touch the filesystem. "Is this name legal" is a question about
//! a string and a platform; "does this file exist" is a question about a disk,
//! and mixing the two produces a checker that cannot be tested exhaustively.
//! Consequently every answer here is about what the *platform* permits, not
//! what the *volume* permits: an exFAT stick, a network share and a
//! case-sensitive NTFS directory each narrow or widen these rules, and only a
//! real call against a real path can tell you which.

use crate::Platform;

/// Longest single path component both platforms accept.
///
/// 255 on NTFS and on every mainstream Linux filesystem, so one constant
/// covers both -- but the *unit* differs, which is the part that bites. See
/// [`component_len`].
pub const MAX_COMPONENT_LEN: usize = 255;

/// Characters Windows forbids anywhere in a file name.
///
/// `/` and `\` are in the list because a *name* may not contain them; a
/// *path* is split on them first, so [`path_problems`] never sees one inside
/// a component while [`file_name_problems`] does. Control characters
/// `U+0000`--`U+001F` are also forbidden and are not listed here because a
/// range is not a set of literals.
pub const WINDOWS_FORBIDDEN_CHARS: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// Characters Linux forbids in a file name: the separator, and NUL.
///
/// That really is the whole list. Newlines, backslashes, leading dashes and
/// emoji are all legal, which is why a name that came from Linux has to be
/// checked before it is written on Windows and not the other way round.
pub const LINUX_FORBIDDEN_CHARS: &[char] = &['/', '\0'];

/// The MS-DOS device names Windows still reserves, in every directory.
///
/// They are reserved *with any extension* and in any case, so `con.txt`,
/// `Aux.log` and `NUL` all name a device rather than a file. Opening one
/// succeeds and reads or writes the device, which is worse than an error: a
/// save appears to work and the document is gone.
///
/// `CONIN$` and `CONOUT$` are included because they are reserved on modern
/// Windows even though the classic lists omit them. `COM0`/`LPT0` are not:
/// they are not device names. The superscript-digit forms (`COM¹`, `LPT²`)
/// that Windows also maps to devices are deliberately absent -- they cannot be
/// typed by accident and listing them would imply this set is exhaustive
/// against the kernel, which no user-space list is.
pub const WINDOWS_RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$", "COM1", "COM2", "COM3", "COM4", "COM5",
    "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8",
    "LPT9",
];

/// The name given to a file whose proposed name sanitised away to nothing.
///
/// A constant rather than a literal so that a caller can recognise "we had to
/// invent this" and, for instance, put the cursor in the name field.
pub const FALLBACK_FILE_NAME: &str = "untitled";

/// Something about a path or name that the platform will not accept, or will
/// accept and quietly change.
///
/// A list rather than a `bool`, because the caller's next move differs per
/// problem: a forbidden character can be substituted, a reserved name needs a
/// suffix, and a path that is merely too long may just need the `\\?\` prefix.
/// Each variant carries the offending component so a message can point at it
/// instead of at the whole path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathProblem {
    /// The path or name was empty, or was nothing but separators.
    Empty,

    /// A component is `.` or `..`. Legal *inside* a path, never as a file
    /// name, on either platform -- both are directory entries that already
    /// exist.
    DotName { component: String },

    /// The component names an MS-DOS device rather than a file. `reserved` is
    /// the canonical spelling from [`WINDOWS_RESERVED_NAMES`], so a message
    /// can say which device without echoing the user's casing back at them.
    ReservedName {
        component: String,
        reserved: &'static str,
    },

    /// The component contains a character the platform forbids. Reported once
    /// per component with the first offender, because a name with six illegal
    /// characters is one mistake and six messages is noise.
    ForbiddenCharacter { component: String, character: char },

    /// The component ends in a dot or a space. Windows-only, and the reason
    /// it is a problem rather than a nuisance: Win32 path normalisation
    /// *silently strips* them, so `"notes ."` and `"notes"` are the same file
    /// and a caller that thought it had two has lost one.
    TrailingDotOrSpace { component: String },

    /// The component is longer than the filesystem's per-name limit. `len` is
    /// in the unit that platform counts in -- see [`component_len`] -- which
    /// is why the number can differ between platforms for the same string.
    ComponentTooLong {
        component: String,
        len: usize,
        limit: usize,
    },

    /// The whole path is longer than the platform's limit. On Windows this is
    /// the one problem here with a workaround rather than a fix: see
    /// [`to_extended_length`].
    PathTooLong { len: usize, limit: usize },
}

impl PathProblem {
    /// A sentence for the user, naming the fix rather than the rule.
    ///
    /// Public because the shell must not paraphrase these: the difference
    /// between "invalid character" and "Windows does not allow `:` in a file
    /// name" is the difference between a user retrying blindly and a user
    /// fixing it.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Empty => "A file name is required.".to_owned(),
            Self::DotName { component } => {
                format!("\"{component}\" cannot be used as a file name; it names a directory.")
            }
            Self::ReservedName {
                component,
                reserved,
            } => format!(
                "\"{component}\" is reserved on Windows: {reserved} names a device, not a file. \
                 Add a character to the name, for example \"{reserved}_\"."
            ),
            Self::ForbiddenCharacter {
                component,
                character,
            } => {
                if character.is_control() {
                    format!(
                        "\"{}\" contains a control character (U+{:04X}), which cannot appear in a \
                         file name.",
                        component.escape_debug(),
                        *character as u32
                    )
                } else {
                    format!(
                        "\"{component}\" contains {character}, which is not allowed in a file name."
                    )
                }
            }
            Self::TrailingDotOrSpace { component } => format!(
                "\"{component}\" ends in a dot or a space. Windows removes those silently, so the \
                 file would be saved under a different name."
            ),
            Self::ComponentTooLong {
                component,
                len,
                limit,
            } => {
                let shown: String = component.chars().take(24).collect();
                format!("\"{shown}...\" is {len} characters long; the limit is {limit}.")
            }
            Self::PathTooLong { len, limit } => format!(
                "This path is {len} characters long; the limit is {limit}. Save it somewhere with \
                 a shorter folder path."
            ),
        }
    }
}

/// The length of `name` in the unit this platform counts names in.
///
/// Windows limits are in UTF-16 code units, Linux limits are in bytes, and
/// the two disagree for anything outside ASCII: `"é"` is one unit on Windows
/// and two bytes on Linux, while an emoji is two units on Windows and four
/// bytes on Linux. A length check that picks one unit is wrong on one
/// platform, and it is wrong in the direction that lets a name through and
/// fails at save time.
#[must_use]
pub fn component_len(platform: Platform, name: &str) -> usize {
    match platform {
        Platform::Windows => name.encode_utf16().count(),
        Platform::Linux => name.len(),
    }
}

/// The longest path this platform accepts without an escape hatch.
///
/// Windows: 259, being `MAX_PATH` (260) less the terminating NUL, and it
/// applies to the *fully qualified* path. Linux: 4095, being `PATH_MAX` less
/// its NUL.
///
/// Windows 10 1607 and later can lift the 259 for most APIs when the machine
/// sets `LongPathsEnabled` *and* the application manifest opts in; neither is
/// something this crate can observe from a string, so the conservative number
/// is the one reported and [`to_extended_length`] is the portable answer.
#[must_use]
pub const fn max_path_len(platform: Platform) -> usize {
    match platform {
        Platform::Windows => 259,
        Platform::Linux => 4095,
    }
}

/// Whether `c` may appear inside a single path component on `platform`.
#[must_use]
fn is_forbidden_in_component(platform: Platform, c: char) -> bool {
    match platform {
        // The separators are excluded here and re-added by
        // `file_name_problems`: a component obtained by splitting a path can
        // never contain one, so reporting them at this level would be dead.
        Platform::Windows => {
            matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*') || (c as u32) < 0x20
        }
        Platform::Linux => c == '\0',
    }
}

/// The device this name refers to on Windows, if it refers to one.
///
/// The test is on the stem -- everything before the first dot -- with trailing
/// spaces removed and case ignored, because that is what the Win32 path parser
/// does. `CON.txt`, `con`, `CON  ` and `CON.tar.gz` all name the console.
///
/// Returns the canonical spelling rather than `true` so a message can be
/// specific, and takes no [`Platform`]: the question "is this a DOS device
/// name" has one answer, and whether it *matters* is the caller's platform
/// question.
#[must_use]
pub fn reserved_device_name(name: &str) -> Option<&'static str> {
    let stem = name.split('.').next().unwrap_or(name).trim_end_matches(' ');
    WINDOWS_RESERVED_NAMES
        .iter()
        .copied()
        .find(|reserved| reserved.eq_ignore_ascii_case(stem))
}

/// Everything wrong with `name` treated as a single file or directory name.
///
/// Separators are forbidden here, unlike in [`path_problems`], because a name
/// containing one is a name that would silently become a path. This is the
/// function Save As wants; [`path_problems`] is the one a whole path wants.
#[must_use]
pub fn file_name_problems(platform: Platform, name: &str) -> Vec<PathProblem> {
    if name.is_empty() {
        return vec![PathProblem::Empty];
    }
    let mut problems = Vec::new();
    if name == "." || name == ".." {
        problems.push(PathProblem::DotName {
            component: name.to_owned(),
        });
    }
    let separators: &[char] = match platform {
        Platform::Windows => &['/', '\\'],
        Platform::Linux => &['/'],
    };
    if let Some(character) = name.chars().find(|c| separators.contains(c)) {
        problems.push(PathProblem::ForbiddenCharacter {
            component: name.to_owned(),
            character,
        });
    }
    check_component(platform, name, &mut problems);
    problems
}

/// Everything wrong with `path` on `platform`.
///
/// The path is split on the platform's separators (Windows accepts `/` as
/// well as `\`, so both split it) and each component is checked. Empty
/// components are skipped: `a//b` is `a/b` on both platforms, and reporting
/// it would flag every path built by naive string joining.
///
/// The length check is applied to the string as given. For a relative path
/// that is a *lower bound* -- the current directory is prepended before the
/// limit is applied for real -- so an absent [`PathProblem::PathTooLong`] on a
/// relative Windows path is not a promise.
#[must_use]
pub fn path_problems(platform: Platform, path: &str) -> Vec<PathProblem> {
    let mut problems = Vec::new();
    let (_prefix, rest) = split_prefix(platform, path);
    let mut any = false;

    for component in components_of(platform, rest) {
        any = true;
        if component == "." || component == ".." {
            // Legal inside a path on both platforms. Only a *name* may not be
            // one, which is `file_name_problems`' business.
            continue;
        }
        check_component(platform, component, &mut problems);
    }

    if !any && path.is_empty() {
        problems.push(PathProblem::Empty);
    }

    let limit = max_path_len(platform);
    // An extended-length path is exempt from MAX_PATH -- that is the entire
    // point of the prefix -- so checking it against 259 would report a problem
    // the caller has already solved.
    if !(platform == Platform::Windows && has_extended_length_prefix(path)) {
        let len = component_len(platform, path);
        if len > limit {
            problems.push(PathProblem::PathTooLong { len, limit });
        }
    }
    problems
}

/// The checks that apply to a component however it was obtained.
fn check_component(platform: Platform, component: &str, problems: &mut Vec<PathProblem>) {
    if let Some(character) = component
        .chars()
        .find(|&c| is_forbidden_in_component(platform, c))
    {
        problems.push(PathProblem::ForbiddenCharacter {
            component: component.to_owned(),
            character,
        });
    }

    if platform == Platform::Windows {
        if let Some(reserved) = reserved_device_name(component) {
            problems.push(PathProblem::ReservedName {
                component: component.to_owned(),
                reserved,
            });
        }
        if component.ends_with('.') || component.ends_with(' ') {
            problems.push(PathProblem::TrailingDotOrSpace {
                component: component.to_owned(),
            });
        }
    }

    let len = component_len(platform, component);
    if len > MAX_COMPONENT_LEN {
        problems.push(PathProblem::ComponentTooLong {
            component: component.to_owned(),
            len,
            limit: MAX_COMPONENT_LEN,
        });
    }
}

/// Whether `path` is legal on `platform`.
#[must_use]
pub fn is_valid_on(platform: Platform, path: &str) -> bool {
    path_problems(platform, path).is_empty()
}

/// Whether `path` is legal on both platforms.
///
/// "Legal on both" is not "means the same on both": `C:\notes` is a valid
/// relative name on Linux and a drive-rooted path on Windows, and this returns
/// true for it. Use it to reject names that *cannot* travel, not to prove that
/// one will.
#[must_use]
pub fn is_portable(path: &str) -> bool {
    Platform::ALL
        .iter()
        .all(|&platform| is_valid_on(platform, path))
}

/// Whether `name` is legal as a single file name on both platforms.
///
/// The stricter sibling of [`is_portable`], and the one worth checking before
/// a Save As: it forbids separators, `.` and `..`, which a path is allowed to
/// contain and a name is not.
#[must_use]
pub fn is_portable_file_name(name: &str) -> bool {
    Platform::ALL
        .iter()
        .all(|&platform| file_name_problems(platform, name).is_empty())
}

/// A file name close to `name` that is legal on both platforms.
///
/// Substituting rather than refusing, because most of the names that reach
/// here were generated -- from a heading, a search phrase, a date -- and the
/// user never typed them. Refusing a generated name puts a dialog in front of
/// someone who did nothing wrong.
///
/// In order: forbidden characters become `_`, surrounding whitespace goes,
/// the name is truncated to fit while keeping its extension, trailing dots and
/// spaces go, a reserved device stem gains a `_`, and a name that survived
/// none of that becomes [`FALLBACK_FILE_NAME`]. The result satisfies
/// [`is_portable_file_name`] for every input, which is the property this
/// function is really promising, and it is idempotent, which is what keeps a
/// name from creeping a character longer every time it is round-tripped.
#[must_use]
pub fn to_portable_file_name(name: &str) -> String {
    let mapped: String = name
        .chars()
        .map(|c| {
            if WINDOWS_FORBIDDEN_CHARS.contains(&c)
                || LINUX_FORBIDDEN_CHARS.contains(&c)
                || (c as u32) < 0x20
            {
                '_'
            } else {
                c
            }
        })
        .collect();

    let truncated = truncate_keeping_extension(mapped.trim(), MAX_COMPONENT_LEN);
    let trimmed = truncated.trim_end_matches(['.', ' ']);

    if trimmed.is_empty() {
        return FALLBACK_FILE_NAME.to_owned();
    }

    let Some(_) = reserved_device_name(trimmed) else {
        return trimmed.to_owned();
    };
    // Suffixing the *stem* rather than the whole name keeps `CON.txt` openable
    // as a text file; appending to the end would give `CON.txt_`, which no
    // longer has an extension.
    let suffixed = match trimmed.split_once('.') {
        Some((stem, rest)) => format!("{stem}_.{rest}"),
        None => format!("{trimmed}_"),
    };
    // That one byte can push a name that was already at the limit past it.
    // Capping here rather than reserving a byte up front is what makes this
    // function idempotent: reserving a byte means a second pass truncates
    // again, and the name loses a character every time it is round-tripped.
    // Nothing is lost by capping instead -- a device stem is at most seven
    // characters, so the byte comes out of a 240-character extension.
    truncate_on_boundary(&suffixed, MAX_COMPONENT_LEN)
        .trim_end_matches(['.', ' '])
        .to_owned()
}

/// Shorten `name` to at most `budget` bytes, keeping its extension.
///
/// The limit is 255 UTF-16 units on Windows and 255 bytes on Linux, so the
/// stricter of the two has to hold; UTF-8 bytes are never fewer than UTF-16
/// units for the same text, so satisfying the byte count satisfies both, and
/// counting bytes is the check that needs no platform argument.
fn truncate_keeping_extension(name: &str, budget: usize) -> String {
    if name.len() <= budget {
        return name.to_owned();
    }
    let (stem, ext) = match name.rfind('.') {
        // A leading dot is the whole name being an extension (`.gitignore`),
        // not a stem with a suffix, so there is nothing to preserve.
        Some(0) | None => (name, ""),
        Some(at) => (&name[..at], &name[at..]),
    };
    // An extension long enough to fill the budget on its own is not an
    // extension; fall back to a plain truncation rather than emitting a name
    // that is nothing but a suffix.
    if ext.len() >= budget {
        return truncate_on_boundary(name, budget).to_owned();
    }
    let room = budget - ext.len();
    format!("{}{ext}", truncate_on_boundary(stem, room))
}

/// The longest prefix of `s` that is at most `max` bytes and ends on a
/// character boundary.
fn truncate_on_boundary(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Whether `a` and `b` name the same file on `platform`.
///
/// On Linux this is byte equality. On Windows it is a case-insensitive
/// comparison, and that is where the honesty is owed: Windows folds case with
/// a fixed table baked into the volume when it was formatted, this uses
/// Rust's current-Unicode lowercase mapping, and the two disagree on a handful
/// of code points (`ı` U+0131 among them). ASCII -- which is every generated
/// name and almost every typed one -- is exact. Treat a `true` as reliable and
/// a `false` on a non-ASCII pair as very likely but not proven.
#[must_use]
pub fn same_file_name(platform: Platform, a: &str, b: &str) -> bool {
    if platform.case_sensitive_file_names() {
        a == b
    } else if a.is_ascii() && b.is_ascii() {
        a.eq_ignore_ascii_case(b)
    } else {
        a.to_lowercase() == b.to_lowercase()
    }
}

/// The first pair of names in `names` that would collide on `platform`.
///
/// The case an editor actually hits: a set of files listed on Linux, exported,
/// and unpacked on Windows, where `Notes.txt` and `notes.txt` become one file
/// and the second silently overwrites the first. Returns the pair rather than
/// a bool so the message can name both, and stops at the first because a
/// user fixing collisions fixes them one at a time.
#[must_use]
pub fn first_collision<'a>(platform: Platform, names: &[&'a str]) -> Option<(&'a str, &'a str)> {
    for (i, a) in names.iter().enumerate() {
        for b in &names[i + 1..] {
            if same_file_name(platform, a, b) {
                return Some((a, b));
            }
        }
    }
    None
}

/// Whether `path` names a location without reference to anything else.
///
/// This exists because [`std::path::Path::is_absolute`] answers for the
/// *compile target* and not for a platform you name, so on Linux it calls
/// `C:\Users\me` relative and on Windows it calls `/home/me` relative. Both
/// answers are correct and both are useless to a function reasoning about the
/// other platform -- which is exactly what a cross-platform editor's directory
/// and manifest logic spends its time doing.
///
/// Windows has two shapes that look absolute and are not, and both are quietly
/// destructive if treated as roots: `C:notes` is relative to the *current
/// directory of drive C*, and `\notes` is relative to the current drive.
/// Neither is accepted here.
#[must_use]
pub fn is_absolute(platform: Platform, path: &str) -> bool {
    match platform {
        Platform::Linux => path.starts_with('/'),
        Platform::Windows => {
            if path.starts_with(r"\\") || path.starts_with("//") {
                return true;
            }
            let mut chars = path.chars();
            matches!(
                (chars.next(), chars.next(), chars.next()),
                (Some(drive), Some(':'), Some('\\' | '/')) if drive.is_ascii_alphabetic()
            )
        }
    }
}

/// `base` with `parts` appended, using the separator `platform` writes with.
///
/// [`std::path::PathBuf::join`] would use the *host* separator, so a Windows
/// path assembled on Linux comes out as `C:\Users\me/bachelorpad` -- which
/// mostly works on Windows and is wrong in every log line, every comparison
/// and every test assertion. Building the string explicitly is the only way a
/// directory rule can be asserted identically on both legs of CI.
///
/// An empty part is skipped and an existing trailing separator is not
/// doubled, so callers may join a root that came from the environment with or
/// without its slash.
#[must_use]
pub fn join(platform: Platform, base: &str, parts: &[&str]) -> String {
    let separator = platform.separator();
    let mut out = base.to_owned();
    for part in parts.iter().filter(|p| !p.is_empty()) {
        if !out.is_empty() && !out.ends_with(['\\', '/']) {
            out.push(separator);
        }
        out.push_str(part);
    }
    out
}

// --- Windows path prefixes -------------------------------------------------

/// Whether `path` already carries the `\\?\` extended-length prefix.
///
/// Only backslashes count. `//?/` is not the prefix: the prefix is recognised
/// by the path parser *before* any separator translation happens, which is the
/// same reason a `\\?\` path may not contain forward slashes at all.
#[must_use]
pub fn has_extended_length_prefix(path: &str) -> bool {
    path.starts_with(r"\\?\")
}

/// Whether this path is too long for Windows without the `\\?\` prefix.
///
/// Answers only the length question. A path can also *need* the prefix to
/// exist at all -- a component ending in a space can only be created through
/// it -- and this deliberately does not say so, because such a path is a
/// problem to fix rather than a limit to work around.
#[must_use]
pub fn needs_extended_length_prefix(path: &str) -> bool {
    !has_extended_length_prefix(path)
        && component_len(Platform::Windows, path) > max_path_len(Platform::Windows)
}

/// `path` rewritten with the `\\?\` prefix, or `None` if it cannot be.
///
/// The prefix means "pass this to the object manager verbatim", which buys
/// the ~32 767 character limit and costs every convenience Win32 normally
/// applies. So the conversion refuses in three cases rather than producing a
/// path that resolves somewhere else:
///
/// * **a relative path**, because the current directory is not applied to an
///   extended-length path -- `\\?\notes.txt` is not a file under the working
///   directory, it is a parse error;
/// * **a path containing a `.` or `..` component**, because they are not
///   resolved and become literal directory names;
/// * **anything that is not a drive path or a UNC path**, since those are the
///   two shapes with a defined rewriting.
///
/// Forward slashes are translated to backslashes as part of the rewrite, since
/// an extended-length path may not contain them. A path that already has the
/// prefix is returned unchanged.
#[must_use]
pub fn to_extended_length(path: &str) -> Option<String> {
    if has_extended_length_prefix(path) {
        return Some(path.to_owned());
    }
    let normalised = path.replace('/', "\\");
    if normalised
        .split('\\')
        .any(|component| component == "." || component == "..")
    {
        return None;
    }

    if let Some(rest) = normalised.strip_prefix(r"\\") {
        // A UNC path becomes `\\?\UNC\server\share\...`; the leading pair of
        // backslashes is replaced, not kept, or the server name would be
        // parsed as an empty component.
        let mut parts = rest.splitn(3, '\\');
        let server = parts.next().filter(|s| !s.is_empty())?;
        let share = parts.next().filter(|s| !s.is_empty())?;
        let tail = parts.next().unwrap_or("");
        return Some(if tail.is_empty() {
            format!(r"\\?\UNC\{server}\{share}")
        } else {
            format!(r"\\?\UNC\{server}\{share}\{tail}")
        });
    }

    let mut chars = normalised.chars();
    let drive = chars.next()?;
    if !drive.is_ascii_alphabetic() || chars.next() != Some(':') || chars.next() != Some('\\') {
        // Drive-relative (`C:notes`) and root-relative (`\notes`) paths are
        // not fully qualified, so there is nothing to prefix.
        return None;
    }
    Some(format!(r"\\?\{normalised}"))
}

/// The rooting prefix of `path` and the rest of it.
///
/// Windows has four shapes that are not ordinary components and must not be
/// checked as if they were -- a drive letter contains a colon, a UNC server
/// name is not a directory -- so they are peeled off first. Linux has one
/// shape, the leading `/`, which peels off to nothing.
fn split_prefix(platform: Platform, path: &str) -> (&str, &str) {
    if platform == Platform::Linux {
        return ("", path);
    }
    if let Some(rest) = path.strip_prefix(r"\\?\") {
        return match rest.strip_prefix("UNC\\") {
            // `\\?\UNC\server\share\...`: the server and share are part of the
            // root, not directories, so two components go with the prefix.
            Some(unc) => {
                let remainder = skip_components(unc, 2).unwrap_or("");
                (&path[..path.len() - remainder.len()], remainder)
            }
            // `\\?\C:\...` leaves `C:\...`, whose drive letter is peeled off
            // below; `\\?\Volume{...}\...` leaves a component this treats as
            // an ordinary directory name, which is wrong only in that it is
            // then length-checked, and a volume GUID always fits.
            None => split_drive(path, rest),
        };
    }
    if let Some(rest) = path.strip_prefix(r"\\").or_else(|| path.strip_prefix("//")) {
        let remainder = skip_components(rest, 2).unwrap_or("");
        return (&path[..path.len() - remainder.len()], remainder);
    }
    split_drive(path, path)
}

/// Split `C:` (or `C:\`) off the front of `rest`, reporting the prefix as a
/// slice of the original `path` for the caller's convenience.
fn split_drive<'a>(path: &'a str, rest: &'a str) -> (&'a str, &'a str) {
    let mut chars = rest.chars();
    match (chars.next(), chars.next()) {
        (Some(drive), Some(':')) if drive.is_ascii_alphabetic() => {
            let remainder = &rest[2..];
            (&path[..path.len() - remainder.len()], remainder)
        }
        _ => (&path[..path.len() - rest.len()], rest),
    }
}

/// Skip `n` non-empty components of `s`, returning what is left.
fn skip_components(s: &str, n: usize) -> Option<&str> {
    let mut rest = s;
    for _ in 0..n {
        rest = rest.trim_start_matches(['\\', '/']);
        let end = rest.find(['\\', '/'])?;
        rest = &rest[end..];
    }
    Some(rest)
}

/// The non-empty components of `path`, split on this platform's separators.
///
/// Windows treats `/` and `\` alike when parsing, Linux only `/`, so a path
/// written with backslashes is one component on Linux and several on Windows.
/// That asymmetry is the point: it is why a manifest written on Windows with
/// backslash separators cannot be read on Linux, and why `bp-integrity`
/// normalises to `/` before writing one.
pub fn components(platform: Platform, path: &str) -> impl Iterator<Item = &str> {
    let (_prefix, rest) = split_prefix(platform, path);
    components_of(platform, rest)
}

fn components_of(platform: Platform, rest: &str) -> impl Iterator<Item = &str> {
    let separators: &'static [char] = match platform {
        Platform::Windows => &['\\', '/'],
        Platform::Linux => &['/'],
    };
    rest.split(separators).filter(|c| !c.is_empty())
}

#[cfg(test)]
mod tests;
