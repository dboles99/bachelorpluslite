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

/// A way of pasting something that is not simply verbatim.
///
/// specs.md section 14 calls for "format-aware paste transformations".
/// Detecting a [`ClipKind`] is only half of that; this type and [`apply`]
/// are the part that actually acts on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transform {
    /// Collapse to a single line.
    JoinLines,
    /// Strip Markdown decoration, leaving the prose.
    PlainText,
    /// Re-indent JSON. **Carried out by `bp-data`, not here** -- see
    /// [`apply`].
    PrettyJson,
    /// Strip all optional whitespace from JSON. Also `bp-data`'s.
    MinifyJson,
    /// Backslashes to forward slashes.
    ForwardSlashes,
    /// Wrap each line in a Markdown list item.
    BulletList,
    /// Indent every line by four spaces, making it a Markdown code block.
    CodeBlock,
}

/// Apply `transform` to `text`.
///
/// `None` covers three cases the caller should treat the same way -- offer
/// nothing rather than a menu row that visibly does nothing:
///
/// * the transform would not change anything, checked once here rather than
///   in every helper below;
/// * the transform cannot apply to this text;
/// * the transform is **JSON reformatting**, which this crate deliberately
///   does not do.
///
/// That last one is a layering decision rather than an omission. `bp-data`
/// already pretty-prints and minifies JSON through `serde_json`, and its
/// pretty-printer sorts object keys. A second implementation here would
/// disagree with it -- the same document coming out differently depending on
/// which menu the user reached for -- and this crate would acquire a JSON
/// parser it has no business owning. The shell holds both crates and routes
/// `PrettyJson` and `MinifyJson` to `bp_data::json_format` and
/// `bp_data::json_minify`; everything else is done here.
pub fn apply(transform: Transform, text: &str) -> Option<String> {
    let result = match transform {
        Transform::JoinLines => join_lines(text),
        Transform::PlainText => strip_markdown(text),
        Transform::PrettyJson | Transform::MinifyJson => None,
        Transform::ForwardSlashes => Some(text.replace('\\', "/")),
        Transform::BulletList => bullet_list(text),
        Transform::CodeBlock => code_block(text),
    }?;

    if result == text { None } else { Some(result) }
}

/// The transformations worth offering for a given kind, in menu order.
///
/// `JoinLines` is missing from `Url` and `Path` on purpose: both are
/// single-line by the very definition `detect_kind` uses to classify them
/// that way, so the transform could never fire for either. Whether some
/// *other* kind's specific entry happens to be single-line is a different
/// question that this function cannot answer -- it only ever sees the
/// `ClipKind`, never the entry's text -- so that check lives in `apply`
/// (inside `join_lines`, which does have the text) instead.
pub fn transforms_for(kind: ClipKind) -> &'static [Transform] {
    match kind {
        ClipKind::PlainText => &[
            Transform::JoinLines,
            Transform::BulletList,
            Transform::CodeBlock,
        ],
        ClipKind::Markdown => &[Transform::JoinLines, Transform::PlainText],
        ClipKind::Json => &[
            Transform::JoinLines,
            Transform::PrettyJson,
            Transform::MinifyJson,
        ],
        ClipKind::Code => &[Transform::JoinLines, Transform::CodeBlock],
        // A path's only meaningful transform is separator style.
        ClipKind::Path => &[Transform::ForwardSlashes],
        // Nothing here means anything for a bare URL.
        ClipKind::Url => &[],
    }
}

/// Collapse `text` to a single line, or refuse if it already is one.
fn join_lines(text: &str) -> Option<String> {
    if text.lines().count() <= 1 {
        return None;
    }
    let joined = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if joined.is_empty() {
        // Every line was blank -- there is no prose here to offer.
        return None;
    }
    Some(joined)
}

/// Strip common Markdown decoration, leaving the prose underneath.
///
/// This is deliberately not a Markdown parser: no nested emphasis, no
/// fenced code blocks, no tables, no reference-style links. A real parser
/// is a dependency and a project of its own, and a paste transform only
/// needs "reads as plain prose for the common cases" -- headings, bullets,
/// bold/italic, and link text -- which a handful of substring rules covers
/// without the risk of a parser reinterpreting content it was never built
/// to see.
fn strip_markdown(text: &str) -> Option<String> {
    if text.trim().is_empty() {
        return None;
    }
    let newline = newline_style(text);
    Some(
        text.lines()
            .map(strip_markdown_line)
            .collect::<Vec<_>>()
            .join(newline),
    )
}

fn strip_markdown_line(line: &str) -> String {
    let mut s = line.to_string();

    // Heading: one to six leading `#` then a space.
    let hashes = s.chars().take_while(|&c| c == '#').count();
    if (1..=6).contains(&hashes) && s.as_bytes().get(hashes) == Some(&b' ') {
        s = s[hashes + 1..].to_string();
    }

    // List bullet: `-`, `*` or `+` then a space. Indentation is kept.
    let trimmed_start = s.trim_start();
    let indent_len = s.len() - trimmed_start.len();
    if let Some(rest) = trimmed_start
        .strip_prefix("- ")
        .or_else(|| trimmed_start.strip_prefix("* "))
        .or_else(|| trimmed_start.strip_prefix("+ "))
    {
        s = format!("{}{rest}", &s[..indent_len]);
    }

    s = strip_link_text(&s);

    // Bold, either delimiter.
    s = s.replace("**", "").replace("__", "");
    // Italic via asterisk. A lone underscore is left alone -- it is common
    // inside plain identifiers (snake_case) even in the middle of prose,
    // and guessing wrong there corrupts a word to save one character of
    // decoration.
    s = s.replace('*', "");

    s
}

/// Replace `[text](url)` with `text`. Anything that is not a well-formed
/// link -- an unmatched `[`, or a `[...]` with no following `(url)` -- is
/// left exactly as written rather than guessed at.
fn strip_link_text(line: &str) -> String {
    let mut result = String::with_capacity(line.len());
    let mut remainder = line;

    while let Some(open) = remainder.find('[') {
        result.push_str(&remainder[..open]);
        let after_open = &remainder[open + 1..];

        let Some(close) = after_open.find(']') else {
            result.push_str(&remainder[open..]);
            remainder = "";
            break;
        };

        let link_text = &after_open[..close];
        let after_text = &after_open[close + 1..];

        if let Some(after_paren_open) = after_text.strip_prefix('(')
            && let Some(paren_close) = after_paren_open.find(')')
        {
            // `[text](url)` -- keep the text, drop the markup.
            result.push_str(link_text);
            remainder = &after_paren_open[paren_close + 1..];
            continue;
        }

        // `[...]` without a following `(url)` is not a link.
        result.push('[');
        remainder = after_open;
    }

    result.push_str(remainder);
    result
}

/// Wrap every line in a Markdown list item.
fn bullet_list(text: &str) -> Option<String> {
    if text.trim().is_empty() {
        return None;
    }
    let newline = newline_style(text);
    Some(
        text.lines()
            .map(|line| format!("- {line}"))
            .collect::<Vec<_>>()
            .join(newline),
    )
}

/// Indent every line by four spaces -- the Markdown convention for a code
/// block.
fn code_block(text: &str) -> Option<String> {
    if text.trim().is_empty() {
        return None;
    }
    let newline = newline_style(text);
    Some(
        text.lines()
            .map(|line| format!("    {line}"))
            .collect::<Vec<_>>()
            .join(newline),
    )
}

/// Which line ending `text` uses, so a transform that rebuilds line by line
/// can put the same one back rather than quietly turning CRLF into LF.
fn newline_style(text: &str) -> &'static str {
    if text.contains("\r\n") { "\r\n" } else { "\n" }
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

    #[test]
    fn joining_lines_collapses_multiple_lines_into_one() {
        assert_eq!(
            apply(Transform::JoinLines, "one\ntwo\nthree"),
            Some("one two three".to_string())
        );
    }

    #[test]
    fn joining_lines_also_collapses_crlf_input() {
        assert_eq!(
            apply(Transform::JoinLines, "one\r\ntwo\r\nthree"),
            Some("one two three".to_string()),
            "CRLF is still a line break"
        );
    }

    #[test]
    fn joining_lines_leaves_a_single_line_entry_alone() {
        assert_eq!(
            apply(Transform::JoinLines, "just one line"),
            None,
            "nothing to collapse"
        );
    }

    #[test]
    fn joining_blank_lines_yields_nothing_to_offer() {
        assert_eq!(
            apply(Transform::JoinLines, "\n\n\n"),
            None,
            "collapsing blank lines to an empty string is not a useful paste"
        );
    }

    #[test]
    fn stripping_markdown_removes_headings_bullets_bold_and_link_markup() {
        let input = "# Title\n- **bold** item\nsee [docs](http://example.com)";
        assert_eq!(
            apply(Transform::PlainText, input),
            Some("Title\nbold item\nsee docs".to_string()),
            "decoration gone, prose intact"
        );
    }

    #[test]
    fn stripping_markdown_leaves_a_lone_underscore_alone() {
        // Common inside identifiers (snake_case) even in prose; treating it
        // as emphasis would corrupt the word.
        assert_eq!(
            apply(Transform::PlainText, "see my_variable_name in the log"),
            None,
            "nothing here was actually decoration"
        );
    }

    #[test]
    fn stripping_markdown_returns_none_for_blank_input() {
        assert_eq!(apply(Transform::PlainText, "   "), None);
    }

    #[test]
    fn the_json_transforms_are_offered_here_and_carried_out_elsewhere() {
        // `bp-data` already does this through `serde_json`, and its
        // pretty-printer sorts object keys. Doing it again here would give
        // the same document two different answers depending on which menu
        // the user reached for.
        for json in [
            r#"{"a":1,"b":[1,2,3]}"#,
            "{
  \"a\": 1
}",
            "not json at all",
        ] {
            assert_eq!(
                apply(Transform::PrettyJson, json),
                None,
                "the shell routes this to bp-data"
            );
            assert_eq!(apply(Transform::MinifyJson, json), None);
        }
    }

    #[test]
    fn a_json_entry_still_offers_the_json_transforms() {
        // Deferring the work must not remove the menu rows -- the whole
        // point of `transforms_for` is that a JSON clip offers them.
        let offered = transforms_for(ClipKind::Json);
        assert!(offered.contains(&Transform::PrettyJson));
        assert!(offered.contains(&Transform::MinifyJson));
    }

    #[test]
    fn forward_slashes_converts_backslashes() {
        assert_eq!(
            apply(Transform::ForwardSlashes, r"C:\Users\danie\note.txt"),
            Some("C:/Users/danie/note.txt".to_string())
        );
    }

    #[test]
    fn forward_slashes_returns_none_when_there_are_none_to_convert() {
        assert_eq!(
            apply(Transform::ForwardSlashes, "/usr/local/bin"),
            None,
            "nothing would change"
        );
    }

    #[test]
    fn bullet_list_wraps_every_line() {
        assert_eq!(
            apply(Transform::BulletList, "one\ntwo"),
            Some("- one\n- two".to_string())
        );
    }

    #[test]
    fn bullet_list_returns_none_for_blank_input() {
        assert_eq!(apply(Transform::BulletList, ""), None);
    }

    #[test]
    fn code_block_indents_every_line_by_four_spaces() {
        assert_eq!(
            apply(Transform::CodeBlock, "fn main() {}\nlet x = 1;"),
            Some("    fn main() {}\n    let x = 1;".to_string())
        );
    }

    #[test]
    fn code_block_preserves_crlf_line_endings() {
        assert_eq!(
            apply(Transform::CodeBlock, "one\r\ntwo"),
            Some("    one\r\n    two".to_string()),
            "CRLF in, CRLF out"
        );
    }

    #[test]
    fn transforms_for_json_offers_the_json_specific_transforms() {
        let offered = transforms_for(ClipKind::Json);
        assert!(offered.contains(&Transform::PrettyJson));
        assert!(offered.contains(&Transform::MinifyJson));
        assert!(
            !offered.contains(&Transform::ForwardSlashes),
            "ForwardSlashes means nothing for JSON"
        );
    }

    #[test]
    fn transforms_for_path_offers_only_forward_slashes() {
        assert_eq!(transforms_for(ClipKind::Path), &[Transform::ForwardSlashes]);
    }

    #[test]
    fn transforms_for_url_offers_nothing() {
        // A URL is single-line by definition, so JoinLines can never fire,
        // and none of the other transforms mean anything for one either.
        assert_eq!(transforms_for(ClipKind::Url), &[]);
    }

    #[test]
    fn transforms_for_markdown_offers_plain_text() {
        assert!(transforms_for(ClipKind::Markdown).contains(&Transform::PlainText));
    }

    #[test]
    fn transforms_for_plain_text_offers_bullet_list_and_code_block() {
        let offered = transforms_for(ClipKind::PlainText);
        assert!(offered.contains(&Transform::BulletList));
        assert!(offered.contains(&Transform::CodeBlock));
    }

    #[test]
    fn every_transform_survives_empty_input_without_panicking() {
        for transform in [
            Transform::JoinLines,
            Transform::PlainText,
            Transform::PrettyJson,
            Transform::MinifyJson,
            Transform::ForwardSlashes,
            Transform::BulletList,
            Transform::CodeBlock,
        ] {
            assert_eq!(
                apply(transform, ""),
                None,
                "{transform:?} on empty input must not panic and has nothing to offer"
            );
        }
    }

    #[test]
    fn line_based_transforms_preserve_multibyte_characters() {
        let text = "日本語\nテスト行";

        assert_eq!(
            apply(Transform::BulletList, text),
            Some("- 日本語\n- テスト行".to_string())
        );
        assert_eq!(
            apply(Transform::CodeBlock, text),
            Some("    日本語\n    テスト行".to_string())
        );
        assert_eq!(
            apply(Transform::JoinLines, text),
            Some("日本語 テスト行".to_string())
        );
    }
}
