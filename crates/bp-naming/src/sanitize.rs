//! Turning an arbitrary semantic title into something a filesystem accepts.
//!
//! The rules are the union of Windows and Linux restrictions, applied on both
//! platforms. A note created on Linux and synced to Windows must not become
//! unopenable, so we do not relax the rules per platform.

/// Characters Windows forbids in a path component. Linux only forbids `/`,
/// but a name containing `:` or `?` is still a portability trap.
const FORBIDDEN: [char; 9] = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// Windows reserved device names. Reserved with *any* extension, so `CON.txt`
/// is as unusable as `CON` -- which is why the check below looks at the stem
/// and not at the whole name.
///
/// `CONIN$` and `CONOUT$` are here because modern Windows reserves them even
/// though the classic lists omit them. This list deliberately mirrors
/// `bp_platform::paths::WINDOWS_RESERVED_NAMES` rather than importing it: this
/// crate depends on nothing else in the workspace, which is what keeps the
/// filename grammar cheap to test. The two lists are pinned to agree by a
/// cross-crate test in `tests/integration`, so a divergence is a failure and
/// not a discovery.
const RESERVED: [&str; 24] = [
    "CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$", "COM1", "COM2", "COM3", "COM4", "COM5",
    "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8",
    "LPT9",
];

/// Used when sanitising leaves nothing usable.
pub const FALLBACK_TITLE: &str = "Untitled";

/// Maximum bytes in a single path component on the filesystems we target.
pub const MAX_COMPONENT_BYTES: usize = 255;

/// Sanitise a semantic title for use in a filename.
///
/// Forbidden and control characters become spaces rather than being deleted,
/// so `Notes:Part 2` reads as `Notes Part 2` instead of `NotesPart 2`.
/// Whitespace runs then collapse, and leading/trailing separators go, because
/// Windows silently strips trailing dots and spaces -- a name we thought we
/// wrote would not be the name on disk.
pub fn sanitize_title(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut pending_space = false;

    for ch in title.chars() {
        let replace = FORBIDDEN.contains(&ch) || ch.is_control() || ch.is_whitespace();
        if replace {
            // Collapse runs, and never emit a leading space.
            pending_space = !out.is_empty();
            continue;
        }
        if pending_space {
            out.push(' ');
            pending_space = false;
        }
        out.push(ch);
    }

    // Trailing dots and spaces are dropped by Windows; trailing separators
    // would collide with the `_` we join components with.
    let trimmed = out.trim_end_matches([' ', '.', '_', '-']).trim_start();
    let result = defuse_reserved(trimmed.to_owned());

    if result.is_empty() {
        return FALLBACK_TITLE.to_owned();
    }
    result
}

/// Make `title` safe to use as the leading part of a filename, whatever
/// follows it.
///
/// Win32 does not ask whether a whole name is a device: it takes the stem --
/// everything before the *first* dot, with trailing spaces ignored -- and asks
/// about that. So `con.txt` is the console and not a file, and a check against
/// the whole string misses it, because a dot is not a forbidden character and
/// survives sanitising untouched. Opening a reserved name succeeds and reads
/// or writes the *device*: the save reports success and the document is gone.
///
/// The word stays visible, as it did before -- ` File` is inserted after the
/// stem rather than appended to the end, because appending would leave the
/// stem, and therefore the device, exactly where it was.
fn defuse_reserved(title: String) -> String {
    let stem_end = title.find('.').unwrap_or(title.len());
    let stem = title[..stem_end].trim_end_matches(' ');
    if !is_reserved(stem) {
        return title;
    }
    format!("{stem} File{}", &title[stem_end..])
}

/// True if `name` is a Windows reserved device name, ignoring case.
///
/// Takes a stem, not a name: see [`defuse_reserved`].
fn is_reserved(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    RESERVED.contains(&upper.as_str())
}

/// Longest extension we will emit. Caps the fixed part of a filename so the
/// title always has room left inside [`MAX_COMPONENT_BYTES`].
pub const MAX_EXTENSION_CHARS: usize = 32;

/// Sanitise an extension: no dots, no separators, no forbidden characters.
///
/// An empty or fully-invalid extension yields `txt`; a file the editor wrote
/// should always be openable by double-clicking it.
pub fn sanitize_extension(ext: &str) -> String {
    let cleaned: String = ext
        .trim_start_matches('.')
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(MAX_EXTENSION_CHARS)
        .collect();

    if cleaned.is_empty() {
        "txt".to_owned()
    } else {
        cleaned
    }
}

/// Truncate `s` to at most `max` bytes without splitting a character.
pub fn truncate_bytes(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_ordinary_titles_intact() {
        assert_eq!(
            sanitize_title("Rust Migration Notes - UE AI Editor Suite"),
            "Rust Migration Notes - UE AI Editor Suite"
        );
    }

    #[test]
    fn replaces_forbidden_characters_with_spaces() {
        assert_eq!(sanitize_title("Notes:Part 2"), "Notes Part 2");
        assert_eq!(sanitize_title("a/b\\c"), "a b c");
        assert_eq!(sanitize_title("what? yes*"), "what yes");
    }

    #[test]
    fn collapses_whitespace_runs() {
        assert_eq!(sanitize_title("too    many\t\tgaps"), "too many gaps");
    }

    #[test]
    fn strips_control_characters() {
        assert_eq!(sanitize_title("line\u{0}break\u{7}"), "line break");
    }

    #[test]
    fn trims_leading_and_trailing_noise() {
        assert_eq!(sanitize_title("  padded  "), "padded");
        assert_eq!(sanitize_title("trailing dots..."), "trailing dots");
        assert_eq!(sanitize_title("trailing sep_"), "trailing sep");
    }

    #[test]
    fn never_returns_empty() {
        assert_eq!(sanitize_title(""), FALLBACK_TITLE);
        assert_eq!(sanitize_title("///"), FALLBACK_TITLE);
        assert_eq!(sanitize_title("   "), FALLBACK_TITLE);
    }

    #[test]
    fn defuses_windows_device_names() {
        assert_eq!(sanitize_title("CON"), "CON File");
        assert_eq!(sanitize_title("con"), "con File");
        assert_eq!(sanitize_title("LPT1"), "LPT1 File");
        // Reserved on modern Windows, absent from the classic lists.
        assert_eq!(sanitize_title("CONIN$"), "CONIN$ File");
        assert_eq!(sanitize_title("conout$"), "conout$ File");
        // Only the stem is a device; a longer word that starts with one is a
        // perfectly ordinary name.
        assert_eq!(sanitize_title("CONTENTS"), "CONTENTS");
        assert_eq!(sanitize_title("console.log notes"), "console.log notes");
    }

    #[test]
    fn defuses_a_device_name_that_a_dot_follows() {
        // The whole point: Win32 asks about the stem, so `con.txt` is the
        // console. A dot is not forbidden and survives sanitising, so a check
        // on the whole string used to let all of these through -- and saving
        // to one of them writes the device and loses the document.
        assert_eq!(sanitize_title("con.txt"), "con File.txt");
        assert_eq!(sanitize_title("CON.notes"), "CON File.notes");
        assert_eq!(sanitize_title("aux.log"), "aux File.log");
        assert_eq!(sanitize_title("NUL.dat"), "NUL File.dat");
        assert_eq!(sanitize_title("lpt1.bak"), "lpt1 File.bak");
        assert_eq!(sanitize_title("prn.2026"), "prn File.2026");
        assert_eq!(sanitize_title("CONIN$.notes"), "CONIN$ File.notes");
        // The *first* dot, not the last: `con.tar.gz` is still the console.
        assert_eq!(sanitize_title("con.tar.gz"), "con File.tar.gz");
    }

    #[test]
    fn a_device_name_padded_before_the_dot_is_still_a_device() {
        // Win32 ignores trailing spaces in the stem, so `con .txt` is the
        // console too. The padding goes rather than being preserved into a
        // double space.
        assert_eq!(sanitize_title("con .txt"), "con File.txt");
    }

    #[test]
    fn a_dot_before_the_device_name_leaves_an_ordinary_stem() {
        // The stem here is empty, which names no device, so nothing is done
        // to a name that is merely unusual.
        assert_eq!(sanitize_title(".con"), ".con");
    }

    #[test]
    fn keeps_unicode_titles() {
        assert_eq!(sanitize_title("Führungsnotizen"), "Führungsnotizen");
        assert_eq!(sanitize_title("研究ノート"), "研究ノート");
    }

    #[test]
    fn sanitizes_extensions() {
        assert_eq!(sanitize_extension("txt"), "txt");
        assert_eq!(sanitize_extension(".md"), "md");
        assert_eq!(sanitize_extension("..tar.gz"), "targz");
        assert_eq!(sanitize_extension(""), "txt");
        assert_eq!(sanitize_extension("///"), "txt");
    }

    #[test]
    fn truncation_respects_char_boundaries() {
        // "é" is two bytes: cutting at 3 must back off to 2, not split it.
        assert_eq!(truncate_bytes("aé", 3), "aé");
        assert_eq!(truncate_bytes("aéb", 3), "aé");
        assert_eq!(truncate_bytes("ééé", 3), "é");
        assert_eq!(truncate_bytes("abc", 10), "abc");
    }
}
