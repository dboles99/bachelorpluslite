//! Turning an arbitrary semantic title into something a filesystem accepts.
//!
//! The rules are the union of Windows and Linux restrictions, applied on both
//! platforms. A note created on Linux and synced to Windows must not become
//! unopenable, so we do not relax the rules per platform.

/// Characters Windows forbids in a path component. Linux only forbids `/`,
/// but a name containing `:` or `?` is still a portability trap.
const FORBIDDEN: [char; 9] = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// Windows reserved device names. Reserved with *any* extension, so
/// `CON.txt` is as unusable as `CON`.
const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
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
    let mut result = trimmed.to_owned();

    if is_reserved(&result) {
        // Suffixing keeps the user's word visible, which deleting would not.
        result.push_str(" File");
    }

    if result.is_empty() {
        return FALLBACK_TITLE.to_owned();
    }
    result
}

/// True if `name` is a Windows reserved device name, ignoring case.
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
        // Only exact matches are reserved.
        assert_eq!(sanitize_title("CONTENTS"), "CONTENTS");
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
