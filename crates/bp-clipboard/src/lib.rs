//! Clipboard history.
//!
//! specs.md section 14: a history of what has been copied, with the type of
//! each entry detected so paste can be format-aware.
//!
//! **Nothing here is written to disk.** The spec is explicit that persistent
//! clipboard history is opt-in, and a clipboard is the single most
//! consistently sensitive buffer on a machine — passwords, tokens and
//! one-time codes pass through it constantly. In-memory only is the correct
//! default, and when persistence arrives it belongs behind a setting and
//! under ADR-0011's security profile, not here.
//!
//! The history itself is a pure data structure so its behaviour — dedup,
//! ordering, capacity, pinning — is testable without touching an OS
//! clipboard.

#![forbid(unsafe_code)]

/// Crate identity used by workspace smoke tests and diagnostics.
pub const CRATE_NAME: &str = "bp-clipboard";

/// How many entries to keep.
pub const MAX_ENTRIES: usize = 25;

/// Longest entry retained in full. Anything larger is kept truncated, because
/// a history that can hold a 200 MB paste is a memory leak with a menu.
pub const MAX_ENTRY_CHARS: usize = 100_000;

/// What an entry looks like, so paste can be format-aware.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipKind {
    PlainText,
    Json,
    Url,
    Path,
    Code,
    Markdown,
}

impl ClipKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::PlainText => "text",
            Self::Json => "JSON",
            Self::Url => "URL",
            Self::Path => "path",
            Self::Code => "code",
            Self::Markdown => "Markdown",
        }
    }
}

/// Classify clipboard text.
///
/// Cheap structural rules only — no parsing. A wrong guess here costs nothing
/// but a mislabelled row, and parsing every clipboard change would not be
/// worth it.
pub fn detect_kind(text: &str) -> ClipKind {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return ClipKind::PlainText;
    }

    // A URL or path is a single line by definition; checking that first stops
    // a document that merely mentions "https://" being called a URL.
    let single_line = !trimmed.contains('\n');

    if single_line
        && (trimmed.starts_with("http://")
            || trimmed.starts_with("https://")
            || trimmed.starts_with("file://"))
    {
        return ClipKind::Url;
    }

    if single_line && looks_like_path(trimmed) {
        return ClipKind::Path;
    }

    if (trimmed.starts_with('{') && trimmed.ends_with('}'))
        || (trimmed.starts_with('[') && trimmed.ends_with(']'))
    {
        return ClipKind::Json;
    }

    if trimmed.starts_with('#') || trimmed.contains("\n#") || trimmed.contains("](") {
        return ClipKind::Markdown;
    }

    if looks_like_code(trimmed) {
        return ClipKind::Code;
    }

    ClipKind::PlainText
}

fn looks_like_path(text: &str) -> bool {
    // A Windows drive letter, a UNC prefix, or a rooted POSIX path. Requires
    // a separator, so a bare word is not a path.
    let windows = text.len() > 2
        && text.as_bytes()[0].is_ascii_alphabetic()
        && text.as_bytes()[1] == b':'
        && matches!(text.as_bytes()[2], b'\\' | b'/');
    let unc = text.starts_with("\\\\");
    let posix = text.starts_with('/') && text.len() > 1 && !text.contains(' ');
    windows || unc || posix
}

fn looks_like_code(text: &str) -> bool {
    // Deliberately conservative: prose with a stray semicolon must not be
    // called code.
    const MARKERS: &[&str] = &[
        "fn ",
        "let ",
        "const ",
        "class ",
        "def ",
        "import ",
        "function ",
        "public ",
        "return ",
        "#include",
        "SELECT ",
        "</",
        "=>",
        "();",
    ];
    MARKERS.iter().filter(|m| text.contains(*m)).count() >= 2
}

/// One remembered clipboard entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub text: String,
    pub kind: ClipKind,
    /// Kept across trims and deduplication.
    pub pinned: bool,
}

impl Entry {
    pub fn new(text: &str) -> Self {
        let text = if text.chars().count() > MAX_ENTRY_CHARS {
            text.chars().take(MAX_ENTRY_CHARS).collect()
        } else {
            text.to_owned()
        };
        Self {
            kind: detect_kind(&text),
            text,
            pinned: false,
        }
    }

    /// A single-line label for a menu row.
    pub fn preview(&self, width: usize) -> String {
        let collapsed = self.text.split_whitespace().collect::<Vec<_>>().join(" ");
        if collapsed.chars().count() <= width {
            return collapsed;
        }
        format!("{}…", collapsed.chars().take(width - 1).collect::<String>())
    }
}

/// Recent clipboard entries, newest first.
#[derive(Debug, Clone, Default)]
pub struct History {
    entries: Vec<Entry>,
}

impl History {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<&Entry> {
        self.entries.get(index)
    }

    /// Record clipboard text.
    ///
    /// Returns whether anything was added. Blank text and a repeat of what is
    /// already at the front are ignored: the clipboard is polled, so the same
    /// content arrives over and over and a history full of duplicates is
    /// useless.
    pub fn push(&mut self, text: &str) -> bool {
        if text.trim().is_empty() {
            return false;
        }
        if self.entries.first().is_some_and(|e| e.text == text) {
            return false;
        }

        // Copying something again moves it to the front rather than
        // duplicating it.
        self.entries.retain(|e| e.text != text);
        self.entries.insert(0, Entry::new(text));
        self.trim();
        true
    }

    /// Drop the oldest unpinned entries down to capacity.
    ///
    /// Pinned entries survive: pinning is a promise, and a history that
    /// discards what the user pinned is worse than one with no pinning.
    fn trim(&mut self) {
        if self.entries.len() <= MAX_ENTRIES {
            return;
        }
        let mut kept = 0;
        self.entries.retain(|entry| {
            if entry.pinned {
                return true;
            }
            kept += 1;
            kept <= MAX_ENTRIES
        });
    }

    pub fn toggle_pin(&mut self, index: usize) {
        if let Some(entry) = self.entries.get_mut(index) {
            entry.pinned = !entry.pinned;
        }
    }

    /// Forget everything unpinned.
    pub fn clear(&mut self) {
        self.entries.retain(|e| e.pinned);
    }

    /// Forget everything, including pinned entries.
    pub fn clear_all(&mut self) {
        self.entries.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newest_first() {
        let mut h = History::new();
        h.push("one");
        h.push("two");
        assert_eq!(h.entries()[0].text, "two");
        assert_eq!(h.entries()[1].text, "one");
    }

    #[test]
    fn a_repeat_of_the_front_entry_is_ignored() {
        // The clipboard is polled, so the same content arrives repeatedly.
        let mut h = History::new();
        assert!(h.push("same"));
        assert!(!h.push("same"));
        assert_eq!(h.entries().len(), 1);
    }

    #[test]
    fn copying_something_again_moves_it_to_the_front() {
        let mut h = History::new();
        h.push("a");
        h.push("b");
        h.push("a");

        assert_eq!(h.entries().len(), 2, "moved, not duplicated");
        assert_eq!(h.entries()[0].text, "a");
    }

    #[test]
    fn blank_text_is_not_recorded() {
        let mut h = History::new();
        assert!(!h.push(""));
        assert!(!h.push("   \n\t "));
        assert!(h.is_empty());
    }

    #[test]
    fn capacity_is_enforced() {
        let mut h = History::new();
        for i in 0..(MAX_ENTRIES + 10) {
            h.push(&format!("entry {i}"));
        }
        assert_eq!(h.entries().len(), MAX_ENTRIES);
        assert_eq!(h.entries()[0].text, format!("entry {}", MAX_ENTRIES + 9));
    }

    #[test]
    fn pinned_entries_survive_the_trim() {
        // Pinning is a promise. A history that discards what was pinned is
        // worse than one with no pinning at all.
        let mut h = History::new();
        h.push("precious");
        h.toggle_pin(0);

        for i in 0..(MAX_ENTRIES + 10) {
            h.push(&format!("filler {i}"));
        }

        assert!(
            h.entries().iter().any(|e| e.text == "precious"),
            "pinned entry was evicted"
        );
    }

    #[test]
    fn clear_keeps_pinned_and_clear_all_does_not() {
        let mut h = History::new();
        h.push("keep");
        h.toggle_pin(0);
        h.push("drop");

        h.clear();
        assert_eq!(h.entries().len(), 1);
        assert_eq!(h.entries()[0].text, "keep");

        h.clear_all();
        assert!(h.is_empty());
    }

    #[test]
    fn urls_and_paths_are_recognised_only_on_one_line() {
        assert_eq!(detect_kind("https://example.com/x"), ClipKind::Url);
        assert_eq!(detect_kind(r"C:\Users\danie\note.txt"), ClipKind::Path);
        assert_eq!(detect_kind("/usr/local/bin"), ClipKind::Path);

        // A document that mentions a URL is not a URL.
        assert_eq!(
            detect_kind("See https://example.com/x\nfor details"),
            ClipKind::PlainText
        );
    }

    #[test]
    fn json_needs_matching_delimiters() {
        assert_eq!(detect_kind("{\"a\": 1}"), ClipKind::Json);
        assert_eq!(detect_kind("[1, 2, 3]"), ClipKind::Json);
        assert_eq!(
            detect_kind("{ this never closes"),
            ClipKind::PlainText,
            "an opening brace alone is not JSON"
        );
    }

    #[test]
    fn markdown_and_code_are_told_apart_from_prose() {
        assert_eq!(detect_kind("# A heading\n\ntext"), ClipKind::Markdown);
        assert_eq!(detect_kind("see [docs](http://x)"), ClipKind::Markdown);
        assert_eq!(
            detect_kind("fn main() {\n    let x = 1;\n}"),
            ClipKind::Code
        );
        assert_eq!(
            detect_kind("A sentence; with a semicolon."),
            ClipKind::PlainText,
            "prose must not be called code"
        );
    }

    #[test]
    fn very_large_entries_are_truncated() {
        // Otherwise the history is a memory leak with a menu attached.
        let huge = "x".repeat(MAX_ENTRY_CHARS * 2);
        let entry = Entry::new(&huge);
        assert_eq!(entry.text.chars().count(), MAX_ENTRY_CHARS);
    }

    #[test]
    fn previews_are_single_line_and_bounded() {
        let entry = Entry::new("a line\nand another line with more words after it");
        let preview = entry.preview(20);

        assert!(!preview.contains('\n'));
        assert!(preview.chars().count() <= 20);
        assert!(preview.ends_with('…'));
    }

    #[test]
    fn a_short_preview_is_not_elided() {
        assert_eq!(Entry::new("short").preview(20), "short");
    }

    #[test]
    fn detection_never_panics() {
        for text in ["", "\u{0}", "日本語", "{", "//", &"x".repeat(5000)] {
            let _ = detect_kind(text);
            let _ = Entry::new(text).preview(10);
        }
    }
}
