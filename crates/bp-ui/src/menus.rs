//! Menu contents.
//!
//! Built in Rust rather than in the `.slint` markup so that enabled state can
//! depend on application state -- Save All only when something is unsaved,
//! the current encoding ticked, and so on.
//!
//! Menus whose subsystems do not exist yet list their intended contents as
//! disabled rows. A label that swallows a click reads as broken; a menu of
//! greyed rows reads as "not yet", which is what is true. Each such menu
//! names the phase it arrives in, so the UI doubles as the roadmap.

use bp_core::{Encoding, LineEnding};
use bp_editor::Indent;
use bp_formats::Format;
use bp_theme::ThemeId;

use crate::MenuItem;

/// Action ids. Below 100 are handled in Rust; 100 and above are editor
/// functions Slint owns (`TextInput` has its own text, selection and undo
/// stack, so those cannot be handled here).
pub mod action {
    pub const NEW: i32 = 1;
    pub const OPEN: i32 = 2;
    pub const SAVE: i32 = 3;
    pub const SAVE_AS: i32 = 4;
    pub const SAVE_ALL: i32 = 5;
    pub const RELOAD: i32 = 6;
    pub const CLOSE_TAB: i32 = 7;
    /// Write a copy elsewhere without adopting it. In the File block because
    /// that is where the user looks for it, not because it shares any code
    /// with Save As -- it deliberately shares none.
    pub const SAVE_COPY: i32 = 8;
    /// Tab context menu. `CLOSE_TAB` closes the active tab; these act on the
    /// tab that was right-clicked, which the shell puts in `AppState` before
    /// dispatching, so the ids stay position-free.
    pub const CLOSE_OTHER_TABS: i32 = 9;
    pub const CLOSE_ALL_TABS: i32 = 10;
    pub const COPY_TAB_PATH: i32 = 11;

    /// Insert ▸ Date / Time, one per `bp_naming::Stamp::all()`, in that
    /// order. A base plus an offset rather than five constants, so the menu
    /// and the dispatch cannot disagree about which row is which -- the same
    /// shape as `RECENT_BASE`, and bounded by [`super::stamp_end`] for the
    /// same reason.
    pub const STAMP_BASE: i32 = 12;

    pub const THEME_LIGHT: i32 = 20;
    pub const THEME_DARK: i32 = 21;
    pub const THEME_ORGANIC: i32 = 22;
    pub const THEME_GREEN: i32 = 23;
    /// Follow the desktop's light/dark preference (specs §16). Resolves to
    /// one of the four palettes above rather than adding a fifth -- ADR-0009
    /// makes themes data, and a theme nobody can override would not be.
    pub const THEME_SYSTEM: i32 = 24;

    pub const TOGGLE_GUTTER: i32 = 30;
    pub const TOGGLE_WRAP: i32 = 31;

    /// Zoom, in the View menu's block beside the other view toggles rather
    /// than in one of its own. 35-39 remain free.
    ///
    /// Below 100, so these always reach Rust: the font size is `bp-config`'s
    /// and applies to both editor views, unlike the caret operations Slint
    /// conditionally keeps to itself.
    pub const ZOOM_IN: i32 = 32;
    pub const ZOOM_OUT: i32 = 33;
    pub const ZOOM_RESET: i32 = 34;

    pub const LINE_ENDING_LF: i32 = 40;
    pub const LINE_ENDING_CRLF: i32 = 41;
    pub const ENCODING_UTF8: i32 = 42;
    pub const ENCODING_UTF8_BOM: i32 = 43;

    /// Indentation, in the Format block: it is a property of the text, like
    /// the line ending and the encoding beside it. 48 and 49 are free.
    pub const INDENT_TABS: i32 = 44;
    pub const INDENT_SPACES: i32 = 45;
    pub const TAB_WIDTH_2: i32 = 46;
    pub const TAB_WIDTH_4: i32 = 47;
    pub const TAB_WIDTH_8: i32 = 48;

    pub const SHORTCUTS: i32 = 50;
    pub const ABOUT: i32 = 51;

    /// Clipboard history occupies `CLIP_BASE ..` (bounded by [`super::clip_end`]).
    ///
    /// Slint's `dispatch` routes exactly `UNDO..=SELECT_ALL` to the widget and
    /// everything else to Rust, so this range only has to avoid that window —
    /// not sit below it.
    pub const CLIP_BASE: i32 = 300;

    /// How many transform rows are reserved per clipboard entry.
    ///
    /// `transforms_for` currently offers at most three for any one
    /// `ClipKind` (`PlainText` and `Json`); one spare slot is headroom.
    /// `no_clip_kind_offers_more_transforms_than_the_reserved_slots` fails
    /// loudly if that ever stops being true, rather than letting a fourth
    /// transform silently share an id with the next entry's first one.
    pub const CLIP_TRANSFORM_SLOTS: i32 = 4;
    /// Paste-transformation rows: entry `i`'s transforms occupy
    /// `CLIP_TRANSFORM_BASE + i * CLIP_TRANSFORM_SLOTS ..` (bounded by
    /// [`super::clip_transform_end`]). A block per entry, in a range of its
    /// own above `CLIP_BASE`'s, rather than interleaved with it -- so the
    /// plain-paste arm's `id - CLIP_BASE` arithmetic never has to know
    /// transform rows exist.
    pub const CLIP_TRANSFORM_BASE: i32 = 500;

    pub const NOTE_TITLE: i32 = 80;
    pub const NOTE_RENAME: i32 = 81;
    pub const NOTE_SUMMARY: i32 = 82;
    pub const NOTE_KEYWORDS: i32 = 83;
    pub const NOTE_OUTLINE: i32 = 84;
    /// Document statistics -- `bp_semantic::statistics`, so it sits with the
    /// rest of that crate's rows. 86-89 are free.
    pub const DOCUMENT_STATS: i32 = 85;
    /// Go to Line. Opens the bar; the line number arrives as text, not as an
    /// id, so one action is enough for the whole feature.
    pub const GO_TO_LINE: i32 = 86;

    /// Line operations that need no caret: a whole-document replace, like a
    /// data operation.
    pub const LINES_SORT_ASC: i32 = 90;
    pub const LINES_SORT_DESC: i32 = 91;
    pub const LINES_DEDUPE: i32 = 92;
    pub const LINES_REVERSE: i32 = 93;
    pub const LINES_TRIM: i32 = 94;

    /// The rest of the line-operation family: it needs the caret `bp-editor`
    /// owns, which only the custom editor view exposes -- `TextInput`'s
    /// caret is unreadable from here. 98 and 99 are free.
    pub const DUPLICATE_LINE: i32 = 95;
    pub const MOVE_LINE_UP: i32 = 96;
    pub const MOVE_LINE_DOWN: i32 = 97;

    pub const DATA_VALIDATE: i32 = 70;
    pub const DATA_FORMAT: i32 = 71;
    pub const DATA_MINIFY: i32 = 72;
    pub const DATA_TO_JSONL: i32 = 73;
    pub const DATA_TO_JSON: i32 = 74;
    pub const DATA_REPORT: i32 = 75;
    /// CSV/TSV only. Kept apart from `DATA_TO_JSON`/`DATA_TO_JSONL`, which
    /// convert between JSON and JSON Lines -- a delimited table becoming
    /// JSON is a different operation behind a different library function,
    /// and sharing an id would make `run_data_action`'s match depend on the
    /// format to know which one a click meant.
    pub const DATA_CSV_TO_JSON: i32 = 76;
    pub const DATA_CSV_TO_JSONL: i32 = 77;
    pub const DATA_COLUMN_TYPES: i32 = 78;
    // 79 is free.

    /// Recently-opened files occupy `RECENT_BASE .. RECENT_BASE + MAX_RECENT`.
    /// The range is sized to the list so a longer list cannot silently run
    /// into the editor ids at 100.
    pub const RECENT_BASE: i32 = 60;

    // Slint-handled; these never reach `menu-action`.
    pub const UNDO: i32 = 100;
    pub const REDO: i32 = 101;
    pub const CUT: i32 = 102;
    pub const COPY: i32 = 103;
    pub const PASTE: i32 = 104;
    pub const SELECT_ALL: i32 = 105;

    /// Rows that do nothing yet.
    pub const NONE: i32 = 0;
}

fn row(label: &str, shortcut: &str, action: i32) -> MenuItem {
    MenuItem {
        label: label.into(),
        shortcut: shortcut.into(),
        action,
        enabled: true,
        separator_after: false,
    }
}

/// A row that is real but not always available.
///
/// Distinct from [`planned`], which means "this does not exist yet". This one
/// exists and cannot act right now -- at a zoom bound, say -- and greys for
/// that reason. Keeping the two apart matters because the greying is the only
/// thing on screen that explains why a key stopped responding.
fn row_enabled(label: &str, shortcut: &str, action: i32, enabled: bool) -> MenuItem {
    MenuItem {
        enabled,
        ..row(label, shortcut, action)
    }
}

/// An enabled row followed by a divider.
fn row_end(label: &str, shortcut: &str, action: i32) -> MenuItem {
    MenuItem {
        separator_after: true,
        ..row(label, shortcut, action)
    }
}

/// A row that exists to say what is coming, and does nothing.
fn planned(label: &str) -> MenuItem {
    MenuItem {
        label: label.into(),
        shortcut: String::new().into(),
        action: action::NONE,
        enabled: false,
        separator_after: false,
    }
}

/// A ticked row, for a setting that is currently in force.
fn toggle(label: &str, on: bool, action: i32) -> MenuItem {
    // Two leading spaces when off, so labels do not shift as they toggle.
    row(
        &format!("{} {label}", if on { "✓" } else { " " }),
        "",
        action,
    )
}

/// The trailing note naming when a menu's contents arrive.
fn arrives(phase: &str) -> MenuItem {
    planned(&format!("— not implemented yet ({phase})"))
}

/// One past the last plain-paste clipboard id `CLIP_BASE` can produce.
///
/// Bounded the same way `RECENT_BASE`'s window is bounded by `MAX_RECENT`:
/// dispatch used to match `id >= CLIP_BASE` with no upper edge at all, which
/// is the same shape of mistake that once let the recent-files arm claim
/// everything up to 100. A function rather than a `const` because
/// `i32::try_from` is not usable in a const initialiser.
pub(crate) fn clip_end() -> i32 {
    action::CLIP_BASE + i32::try_from(bp_clipboard::MAX_ENTRIES).unwrap_or(0)
}

/// One past the last id `STAMP_BASE` can produce.
///
/// Sized from `Stamp::all()` rather than written down, so adding a sixth
/// stamp cannot leave the dispatch matching five and silently ignoring it.
pub(crate) fn stamp_end() -> i32 {
    action::STAMP_BASE + i32::try_from(bp_naming::Stamp::all().len()).unwrap_or(0)
}

/// One past the last paste-transformation id.
pub(crate) fn clip_transform_end() -> i32 {
    action::CLIP_TRANSFORM_BASE
        + action::CLIP_TRANSFORM_SLOTS * i32::try_from(bp_clipboard::MAX_ENTRIES).unwrap_or(0)
}

/// A menu label for a paste transformation.
///
/// Presentation only -- what each `Transform` actually does lives in
/// `bp_clipboard` and, for the two JSON ones, `bp_data`; this just names the
/// row for a menu built in Rust.
fn transform_label(transform: bp_clipboard::Transform) -> &'static str {
    use bp_clipboard::Transform;
    match transform {
        Transform::JoinLines => "Join Lines",
        Transform::PlainText => "Strip Markdown",
        Transform::PrettyJson => "Pretty-Print JSON",
        Transform::MinifyJson => "Minify JSON",
        Transform::ForwardSlashes => "Forward Slashes",
        Transform::BulletList => "As Bullet List",
        Transform::CodeBlock => "As Code Block",
    }
}

/// Apply a paste transformation, routing JSON reformatting to `bp-data`.
///
/// `bp_clipboard::apply` deliberately returns `None` for `PrettyJson` and
/// `MinifyJson` -- its doc comment explains why: `bp-data` already owns JSON
/// formatting through `serde_json`, and a second implementation here would
/// give the same document two different answers depending on which menu
/// reached for it. `None` still means what it means for every other
/// transform -- nothing would change, so no row should offer it -- which is
/// why the JSON branches are also checked against the input rather than
/// trusted to always differ.
pub(crate) fn apply_transform(transform: bp_clipboard::Transform, text: &str) -> Option<String> {
    use bp_clipboard::Transform;
    let result = match transform {
        Transform::PrettyJson => bp_data::json_format(text).ok(),
        Transform::MinifyJson => bp_data::json_minify(text).ok(),
        other => bp_clipboard::apply(other, text),
    }?;
    (result != text).then_some(result)
}

/// Decode a `CLIP_TRANSFORM_BASE`-range id back to which entry and
/// transform it means.
///
/// `None` covers a stale id the same way as an unknown one: the range is
/// wrong, the entry is no longer there (the history moved between the menu
/// being built and the click landing), or the slot is past however many
/// transforms that entry's kind actually offers. All three are "do nothing"
/// as far as the caller is concerned.
pub(crate) fn decode_transform(
    id: i32,
    clips: &[bp_clipboard::Entry],
) -> Option<(usize, bp_clipboard::Transform)> {
    if !(action::CLIP_TRANSFORM_BASE..clip_transform_end()).contains(&id) {
        return None;
    }
    let offset = id - action::CLIP_TRANSFORM_BASE;
    let entry_index = usize::try_from(offset / action::CLIP_TRANSFORM_SLOTS).ok()?;
    let slot = usize::try_from(offset % action::CLIP_TRANSFORM_SLOTS).ok()?;
    let entry = clips.get(entry_index)?;
    let transform = *bp_clipboard::transforms_for(entry.kind).get(slot)?;
    Some((entry_index, transform))
}

pub fn file(any_dirty: bool, has_path: bool, recent: &[std::path::PathBuf]) -> Vec<MenuItem> {
    let mut items = vec![
        row("New", "Ctrl+N", action::NEW),
        row_end("Open...", "Ctrl+O", action::OPEN),
    ];

    // Recently opened, most recent first. Shown by file name with the parent
    // folder as the right-hand hint, because a column of identical
    // "Untitled_17AUG2026.txt" tells you nothing about which is which.
    for (index, path) in recent.iter().take(bp_config::MAX_RECENT).enumerate() {
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |n| n.to_string_lossy().into(),
        );
        let parent = path
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        items.push(row(
            &name,
            &shorten(&parent, 34),
            action::RECENT_BASE + i32::try_from(index).unwrap_or(0),
        ));
    }
    if !recent.is_empty()
        && let Some(last) = items.last_mut()
    {
        last.separator_after = true;
    }

    items.extend([
        row("Save", "Ctrl+S", action::SAVE),
        row("Save As...", "Ctrl+Shift+S", action::SAVE_AS),
        MenuItem {
            enabled: any_dirty,
            ..row_end("Save All", "", action::SAVE_ALL)
        },
        MenuItem {
            // Reload means "discard my edits and re-read the file", which
            // needs a file to re-read.
            enabled: has_path,
            ..row_end("Reload from Disk", "", action::RELOAD)
        },
        MenuItem {
            separator_after: true,
            ..row("Save a Copy...", "", action::SAVE_COPY)
        },
        row("Close Tab", "Ctrl+W", action::CLOSE_TAB),
    ]);
    items
}

/// Trim a path for the menu's right-hand hint, keeping the tail.
///
/// The end of a path identifies it; the start is usually `C:\Users\...`,
/// which every entry shares.
fn shorten(text: &str, max: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max {
        return text.to_owned();
    }
    let tail: String = chars[chars.len() - max.saturating_sub(1)..]
        .iter()
        .collect();
    format!("…{tail}")
}

/// `editor_view` gates the caret-dependent line operations: `TextInput`
/// never exposes where the caret is, so those rows stay present but greyed
/// rather than vanishing, per this module's convention for a feature that
/// exists but is not usable right now.
pub fn edit(clips: &[bp_clipboard::Entry], editor_view: bool) -> Vec<MenuItem> {
    let mut items = vec![
        row("Undo", "Ctrl+Z", action::UNDO),
        row_end("Redo", "Ctrl+Y", action::REDO),
        row("Cut", "Ctrl+X", action::CUT),
        row("Copy", "Ctrl+C", action::COPY),
        row_end("Paste", "Ctrl+V", action::PASTE),
        row_end("Select All", "Ctrl+A", action::SELECT_ALL),
    ];

    // Clipboard history. Each row shows a preview and what the entry looks
    // like, so a column of similar-looking clips is still distinguishable.
    if clips.is_empty() {
        items.push(planned("Clipboard History — nothing copied yet"));
    } else {
        for (index, entry) in clips.iter().take(bp_clipboard::MAX_ENTRIES).enumerate() {
            let pin = if entry.pinned { "📌 " } else { "" };
            items.push(row(
                &format!("{pin}{}", entry.preview(44)),
                entry.kind.label(),
                action::CLIP_BASE + i32::try_from(index).unwrap_or(0),
            ));

            // Format-aware paste transformations (specs.md section 14) --
            // only the ones that would actually change this entry's text.
            // A row that does nothing when clicked is worse than no row.
            for (slot, &transform) in bp_clipboard::transforms_for(entry.kind).iter().enumerate() {
                if apply_transform(transform, &entry.text).is_none() {
                    continue;
                }
                let id = action::CLIP_TRANSFORM_BASE
                    + i32::try_from(index).unwrap_or(0) * action::CLIP_TRANSFORM_SLOTS
                    + i32::try_from(slot).unwrap_or(0);
                items.push(row(
                    &format!("    ↳ {}", transform_label(transform)),
                    "",
                    id,
                ));
            }
        }
    }
    if let Some(last) = items.last_mut() {
        last.separator_after = true;
    }

    items.extend([
        row("Sort Lines A → Z", "", action::LINES_SORT_ASC),
        row("Sort Lines Z → A", "", action::LINES_SORT_DESC),
        row("Remove Duplicate Lines", "", action::LINES_DEDUPE),
        row("Reverse Lines", "", action::LINES_REVERSE),
        row_end("Trim Trailing Whitespace", "", action::LINES_TRIM),
        // Both need the caret bp-editor owns, which only the custom surface
        // exposes -- TextInput's caret is unreadable from here.
        MenuItem {
            enabled: editor_view,
            ..row("Duplicate Line", "Ctrl+D", action::DUPLICATE_LINE)
        },
        MenuItem {
            enabled: editor_view,
            ..row("Move Line Up", "Alt+Up", action::MOVE_LINE_UP)
        },
        MenuItem {
            enabled: editor_view,
            ..row_end("Move Line Down", "Alt+Down", action::MOVE_LINE_DOWN)
        },
        // Caret work as well -- `Editor::go_to_line` moves the caret we own,
        // and `TextInput`'s cannot be moved from here.
        MenuItem {
            enabled: editor_view,
            ..row_end("Go to Line...", "Ctrl+G", action::GO_TO_LINE)
        },
        planned("Multi-cursor"),
        arrives("phase 2"),
    ]);
    items
}

pub fn view(
    theme: ThemeId,
    follow_system: bool,
    gutter: bool,
    wrap: bool,
    font_size: u8,
) -> Vec<MenuItem> {
    // The four names tick only when they were chosen by name. Following the
    // system resolves to one of them, so ticking both would say the user
    // picked Dark when what they picked was "whatever the desktop is".
    let chosen = |id: ThemeId| !follow_system && theme == id;
    vec![
        toggle("Light", chosen(ThemeId::Light), action::THEME_LIGHT),
        toggle("Dark", chosen(ThemeId::Dark), action::THEME_DARK),
        toggle("Organic", chosen(ThemeId::Organic), action::THEME_ORGANIC),
        toggle("Green", chosen(ThemeId::Green), action::THEME_GREEN),
        MenuItem {
            separator_after: true,
            ..toggle("Follow System", follow_system, action::THEME_SYSTEM)
        },
        toggle("Line Numbers", gutter, action::TOGGLE_GUTTER),
        MenuItem {
            separator_after: true,
            ..toggle("Word Wrap", wrap, action::TOGGLE_WRAP)
        },
        // The rows carry the current size rather than a separate readout,
        // because the one question a zoom menu is opened to answer is what
        // the size is now -- and at the bounds, why a key stopped doing
        // anything. Disabled at the bound says that; a row that still looks
        // live and does nothing does not.
        row_enabled(
            "Zoom In",
            "Ctrl+=",
            action::ZOOM_IN,
            font_size < bp_config::MAX_FONT_SIZE,
        ),
        row_enabled(
            "Zoom Out",
            "Ctrl+-",
            action::ZOOM_OUT,
            font_size > bp_config::MIN_FONT_SIZE,
        ),
        MenuItem {
            separator_after: true,
            ..row_enabled(
                &format!("Reset Zoom ({font_size} pt)"),
                "Ctrl+0",
                action::ZOOM_RESET,
                font_size != bp_config::DEFAULT_FONT_SIZE,
            )
        },
        planned("Split / Preview"),
        arrives("phase 8"),
    ]
}

pub fn format(encoding: Encoding, line_ending: LineEnding, indent: Indent) -> Vec<MenuItem> {
    vec![
        toggle("LF", line_ending == LineEnding::Lf, action::LINE_ENDING_LF),
        MenuItem {
            separator_after: true,
            ..toggle(
                "CRLF",
                line_ending == LineEnding::CrLf,
                action::LINE_ENDING_CRLF,
            )
        },
        toggle("UTF-8", encoding == Encoding::Utf8, action::ENCODING_UTF8),
        MenuItem {
            separator_after: true,
            ..toggle(
                "UTF-8 with BOM",
                encoding == Encoding::Utf8Bom,
                action::ENCODING_UTF8_BOM,
            )
        },
        toggle("Indent with Tabs", !indent.spaces, action::INDENT_TABS),
        MenuItem {
            separator_after: true,
            ..toggle("Indent with Spaces", indent.spaces, action::INDENT_SPACES)
        },
        toggle("Tab Width 2", indent.width == 2, action::TAB_WIDTH_2),
        toggle("Tab Width 4", indent.width == 4, action::TAB_WIDTH_4),
        MenuItem {
            separator_after: true,
            ..toggle("Tab Width 8", indent.width == 8, action::TAB_WIDTH_8)
        },
        arrives("phase 5"),
    ]
}

/// The tab strip's context menu.
///
/// `others` is how many other tabs are open and `has_path` whether the
/// right-clicked document has ever been saved -- both are why a row is live
/// or grey, and neither is knowable from the id alone.
pub fn tab_context(others: usize, has_path: bool) -> Vec<MenuItem> {
    vec![
        row("Close Tab", "Ctrl+W", action::CLOSE_TAB),
        MenuItem {
            enabled: others > 0,
            ..row("Close Other Tabs", "", action::CLOSE_OTHER_TABS)
        },
        MenuItem {
            separator_after: true,
            ..row("Close All Tabs", "", action::CLOSE_ALL_TABS)
        },
        MenuItem {
            // An unsaved document has no path to copy, and copying the
            // display name instead would put "Untitled" on the clipboard,
            // which is worse than the row being visibly unavailable.
            enabled: has_path,
            ..row("Copy Full Path", "", action::COPY_TAB_PATH)
        },
    ]
}

/// Insert ▸ Date / Time.
///
/// The rows and their order come from `Stamp::all()`, and the preview text
/// from `render` at the instant the menu was built -- so what the row says is
/// exactly what clicking it inserts, rather than a description of it.
///
/// Caret rows, like Duplicate Line: inserting at the caret needs a caret, and
/// `TextInput` does not expose one (ADR-0018).
pub fn insert(at: time::OffsetDateTime, editor_view: bool) -> Vec<MenuItem> {
    let mut items: Vec<MenuItem> = bp_naming::Stamp::all()
        .iter()
        .enumerate()
        .map(|(index, stamp)| MenuItem {
            enabled: editor_view,
            ..row(
                stamp.label(),
                &shorten(&bp_naming::render(*stamp, at), 24),
                action::STAMP_BASE + i32::try_from(index).unwrap_or(0),
            )
        })
        .collect();
    if let Some(last) = items.last_mut() {
        last.separator_after = true;
    }
    items.extend([
        planned("Markdown constructs, citation, code block, table"),
        arrives("phase 6"),
    ]);
    items
}

/// The Data menu, which depends entirely on what the document is.
///
/// Offering "Minify" on a note would be noise, and offering it greyed on
/// every note would be worse. A format that has no data operations gets the
/// planned list instead.
pub fn data(format: Format) -> Vec<MenuItem> {
    match format {
        Format::Json => vec![
            row_end("Validate", "", action::DATA_VALIDATE),
            row("Format", "", action::DATA_FORMAT),
            row("Minify", "", action::DATA_MINIFY),
            row_end("Sort Keys", "", action::DATA_FORMAT),
            row("Convert to JSON Lines", "", action::DATA_TO_JSONL),
        ],
        Format::JsonLines => vec![
            row_end("Validate", "", action::DATA_VALIDATE),
            row("Convert to JSON", "", action::DATA_TO_JSON),
        ],
        Format::Toml => vec![
            row_end("Validate", "", action::DATA_VALIDATE),
            row("Format", "", action::DATA_FORMAT),
        ],
        Format::Csv | Format::Tsv => vec![
            row("Report Shape", "", action::DATA_REPORT),
            row_end("Column Types", "", action::DATA_COLUMN_TYPES),
            row("Convert to JSON", "", action::DATA_CSV_TO_JSON),
            row_end("Convert to JSON Lines", "", action::DATA_CSV_TO_JSONL),
        ],
        _ => {
            let mut items = planned_menu("Data");
            items.insert(
                0,
                planned(&format!("— nothing for {} documents", format.label())),
            );
            items
        }
    }
}

/// The Note menu: what the document says about itself.
///
/// Everything here is deterministic extraction (specs.md section 10, layer
/// one). Nothing needs a model or a network, per ADR-0006.
pub fn note(has_content: bool) -> Vec<MenuItem> {
    vec![
        MenuItem {
            enabled: has_content,
            ..row("Suggest Title", "", action::NOTE_TITLE)
        },
        MenuItem {
            enabled: has_content,
            // "Rename" is really Save As with a suggested name: a physical
            // rename needs explicit approval (PROJECT_MEMORY), and a file
            // dialog *is* that approval.
            ..row_end("Semantic Rename...", "", action::NOTE_RENAME)
        },
        MenuItem {
            enabled: has_content,
            ..row("Summary", "", action::NOTE_SUMMARY)
        },
        MenuItem {
            enabled: has_content,
            ..row("Keywords", "", action::NOTE_KEYWORDS)
        },
        MenuItem {
            enabled: has_content,
            ..row_end("Outline", "", action::NOTE_OUTLINE)
        },
        // Not gated on `has_content`: "0 words" is a legitimate answer to
        // "how long is this", and an empty document is exactly when someone
        // might check they are looking at the right tab.
        row_end("Document Statistics", "", action::DOCUMENT_STATS),
        planned("Tags"),
        planned("Related Notes"),
        planned("Revision History"),
        arrives("phase 9"),
    ]
}

pub fn help() -> Vec<MenuItem> {
    vec![
        row("Keyboard Shortcuts", "", action::SHORTCUTS),
        row_end("About BachelorPad+", "", action::ABOUT),
        planned("Diagnostics"),
        arrives("phase 19"),
    ]
}

/// The menus whose subsystems do not exist yet.
///
/// Their contents come from `docs/product/MENU_MAP.md`, so the UI shows the
/// intended shape of the product rather than an empty box.
pub fn planned_menu(name: &str) -> Vec<MenuItem> {
    let (entries, phase): (&[&str], &str) = match name {
        "Insert" => (
            &["Date and Time", "Code Block", "Table", "Citation", "Cell"],
            "phase 5",
        ),
        "Data" => (
            &[
                "Validate",
                "Format / Minify",
                "Sort Keys",
                "Filter / Query",
                "Statistics",
                "Convert",
            ],
            "phase 6",
        ),
        "Note" => (
            &[
                "Semantic Rename",
                "Title and Filename",
                "Tags",
                "Summary",
                "Related Notes",
                "History",
            ],
            "phase 8",
        ),
        "Notebook" => (
            &[
                "Enable Notebook Mode",
                "New Cell",
                "Run Cell",
                "Run All",
                "Export .ipynb",
            ],
            "phase 12",
        ),
        "Organize" => (
            &[
                "Project",
                "Suggested Folder",
                "Topics",
                "Duplicate Detection",
                "Semantic Search",
            ],
            "phase 9",
        ),
        "Research" => (
            &["Citation Metadata", "DOI Lookup", "Evidence", "Datasets"],
            "phase 13",
        ),
        "Run" => (
            &[
                "Run Selection",
                "Run Document",
                "Choose Interpreter",
                "Stop",
                "Rust Scratchpad",
            ],
            "phase 12",
        ),
        "Security" => (
            &[
                "Lock Document",
                "Encrypt as .bpadx",
                "Privacy Mode",
                "Scan for Secrets",
                "Hash and Sign",
                "Redact",
                "Audit History",
            ],
            "phase 14",
        ),
        "Tools" => (
            &[
                "Document Inspector",
                "Security Inspector",
                "File Analysis",
                "Benchmarks",
                "Settings",
            ],
            "phase 19",
        ),
        _ => (&[], "later"),
    };

    let mut items: Vec<MenuItem> = entries.iter().map(|e| planned(e)).collect();
    if let Some(last) = items.last_mut() {
        last.separator_after = true;
    }
    items.push(arrives(phase));
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fixed instant, so a stamp preview is the same on every run and in
    /// every timezone. `bp-naming` never reads a clock, which is what makes
    /// pinning one here enough.
    const STAMP_CLOCK: time::OffsetDateTime = time::macros::datetime!(2026-08-19 14:05:09 UTC);

    #[test]
    fn every_stamp_row_is_inside_the_range_dispatch_matches() {
        // `stamp_end` is sized from `Stamp::all()`, so a sixth stamp extends
        // the window with the menu. This fails if the two ever part company.
        let items = insert(STAMP_CLOCK, true);
        let rows: Vec<&MenuItem> = items.iter().filter(|i| i.action != action::NONE).collect();

        assert_eq!(rows.len(), bp_naming::Stamp::all().len());
        for (index, row) in rows.iter().enumerate() {
            let expected = action::STAMP_BASE + i32::try_from(index).unwrap();
            assert_eq!(row.action, expected, "row {index} has the wrong id");
            assert!(
                (action::STAMP_BASE..stamp_end()).contains(&row.action),
                "id {} is outside the window dispatch matches",
                row.action
            );
        }
    }

    #[test]
    fn a_stamp_row_shows_what_it_will_actually_insert() {
        // The label names the format; the hint is the format applied to the
        // clock. A row reading "ISO 8601" tells you nothing about whether it
        // is the one you want.
        let items = insert(STAMP_CLOCK, true);
        let iso = items
            .iter()
            .find(|i| i.label.contains("ISO"))
            .expect("an ISO 8601 row");
        assert!(
            iso.shortcut.contains("2026-08-19"),
            "the hint should be the rendered stamp; got {:?}",
            iso.shortcut
        );
    }

    #[test]
    fn the_stamp_rows_are_disabled_without_the_custom_editor_view() {
        // Inserting at the caret needs a caret. `TextInput` does not expose
        // one, so the rows grey rather than silently doing nothing.
        for row in insert(STAMP_CLOCK, false)
            .iter()
            .filter(|i| i.action != action::NONE)
        {
            assert!(!row.enabled, "'{}' should be greyed", row.label);
        }
        assert!(
            insert(STAMP_CLOCK, true)
                .iter()
                .filter(|i| i.action != action::NONE)
                .all(|i| i.enabled)
        );
    }

    #[test]
    fn close_other_tabs_is_unavailable_when_there_are_no_others() {
        let alone = tab_context(0, true);
        assert!(
            alone
                .iter()
                .find(|i| i.action == action::CLOSE_OTHER_TABS)
                .is_some_and(|i| !i.enabled)
        );
        assert!(
            tab_context(3, true)
                .iter()
                .find(|i| i.action == action::CLOSE_OTHER_TABS)
                .is_some_and(|i| i.enabled)
        );
    }

    #[test]
    fn copy_full_path_greys_out_when_there_is_no_path() {
        assert!(
            tab_context(1, false)
                .iter()
                .find(|i| i.action == action::COPY_TAB_PATH)
                .is_some_and(|i| !i.enabled),
            "copying 'Untitled' is worse than an unavailable row"
        );
    }

    #[test]
    fn save_all_is_disabled_when_nothing_is_unsaved() {
        let items = file(false, true, &[]);
        let save_all = items
            .iter()
            .find(|i| i.action == action::SAVE_ALL)
            .expect("Save All");
        assert!(!save_all.enabled);

        let items = file(true, true, &[]);
        let save_all = items.iter().find(|i| i.action == action::SAVE_ALL).unwrap();
        assert!(save_all.enabled);
    }

    #[test]
    fn reload_needs_a_file_to_reload_from() {
        let items = file(false, false, &[]);
        let reload = items
            .iter()
            .find(|i| i.action == action::RELOAD)
            .expect("Reload");
        assert!(!reload.enabled, "nothing on disk to reload from");
    }

    #[test]
    fn the_active_theme_is_ticked_and_only_it() {
        let items = view(
            ThemeId::Green,
            false,
            true,
            false,
            bp_config::DEFAULT_FONT_SIZE,
        );
        let ticked: Vec<_> = items
            .iter()
            .filter(|i| {
                (action::THEME_LIGHT..=action::THEME_GREEN).contains(&i.action)
                    && i.label.starts_with('✓')
            })
            .collect();
        assert_eq!(ticked.len(), 1);
        assert!(ticked[0].label.contains("Green"));
    }

    #[test]
    fn the_zoom_rows_grey_out_at_the_bound_they_cannot_pass() {
        let at_max = view(ThemeId::Dark, false, true, false, bp_config::MAX_FONT_SIZE);
        let zoom_in = at_max
            .iter()
            .find(|i| i.action == action::ZOOM_IN)
            .expect("Zoom In row");
        assert!(
            !zoom_in.enabled,
            "at the largest size, Zoom In has nowhere to go -- a row that \
             still looks live is the only explanation the user gets for a key \
             that stopped working"
        );
        assert!(
            at_max
                .iter()
                .find(|i| i.action == action::ZOOM_OUT)
                .is_some_and(|i| i.enabled),
            "Zoom Out is still available at the largest size"
        );

        let at_min = view(ThemeId::Dark, false, true, false, bp_config::MIN_FONT_SIZE);
        assert!(
            at_min
                .iter()
                .find(|i| i.action == action::ZOOM_OUT)
                .is_some_and(|i| !i.enabled)
        );
        assert!(
            at_min
                .iter()
                .find(|i| i.action == action::ZOOM_IN)
                .is_some_and(|i| i.enabled)
        );
    }

    #[test]
    fn reset_zoom_is_inert_at_the_size_it_would_reset_to() {
        let items = view(
            ThemeId::Dark,
            false,
            true,
            false,
            bp_config::DEFAULT_FONT_SIZE,
        );
        let reset = items
            .iter()
            .find(|i| i.action == action::ZOOM_RESET)
            .expect("Reset Zoom row");
        assert!(!reset.enabled, "already at the default");

        let zoomed = view(
            ThemeId::Dark,
            false,
            true,
            false,
            bp_config::DEFAULT_FONT_SIZE + 2,
        );
        assert!(
            zoomed
                .iter()
                .find(|i| i.action == action::ZOOM_RESET)
                .is_some_and(|i| i.enabled)
        );
    }

    #[test]
    fn the_reset_row_names_the_size_currently_in_force() {
        // The question a zoom menu is opened to answer. A row reading "Reset
        // Zoom" alone leaves the user no way to tell 13 pt from 15 pt.
        let items = view(ThemeId::Dark, false, true, false, 22);
        let reset = items
            .iter()
            .find(|i| i.action == action::ZOOM_RESET)
            .expect("Reset Zoom row");
        assert!(
            reset.label.contains("22"),
            "the current size should be on the row; got '{}'",
            reset.label
        );
    }

    #[test]
    fn view_toggles_reflect_their_state() {
        let on = view(
            ThemeId::Dark,
            false,
            true,
            true,
            bp_config::DEFAULT_FONT_SIZE,
        );
        let gutter = on
            .iter()
            .find(|i| i.action == action::TOGGLE_GUTTER)
            .unwrap();
        assert!(gutter.label.starts_with('✓'));

        let off = view(
            ThemeId::Dark,
            false,
            false,
            false,
            bp_config::DEFAULT_FONT_SIZE,
        );
        let gutter = off
            .iter()
            .find(|i| i.action == action::TOGGLE_GUTTER)
            .unwrap();
        assert!(!gutter.label.starts_with('✓'));
    }

    #[test]
    fn format_ticks_the_documents_actual_settings() {
        let items = format(Encoding::Utf8Bom, LineEnding::CrLf, Indent::default());
        let ticked: Vec<&str> = items
            .iter()
            .filter(|i| i.label.starts_with('✓'))
            .map(|i| i.label.as_str())
            .collect();
        assert_eq!(
            ticked.len(),
            4,
            "one line ending, one encoding, one indent mode and one tab width \
             -- exactly one from each group, or the menu is claiming the \
             document is two things at once; got {ticked:?}"
        );
        assert!(ticked.iter().any(|l| l.contains("CRLF")));
        assert!(ticked.iter().any(|l| l.contains("BOM")));
        assert!(ticked.iter().any(|l| l.contains("Tabs")));
        assert!(ticked.iter().any(|l| l.contains("Width 4")));
    }

    #[test]
    fn the_indent_rows_tick_exactly_one_mode_and_one_width() {
        // The two groups are independent -- a width applies to tabs as well
        // as to spaces -- so switching mode must not disturb the width.
        let soft = format(
            Encoding::Utf8,
            LineEnding::Lf,
            Indent {
                spaces: true,
                width: 8,
            },
        );
        let ticked: Vec<&str> = soft
            .iter()
            .filter(|i| i.label.starts_with('✓'))
            .map(|i| i.label.as_str())
            .collect();
        assert!(ticked.iter().any(|l| l.contains("Spaces")));
        assert!(!ticked.iter().any(|l| l.contains("Tabs")));
        assert!(ticked.iter().any(|l| l.contains("Width 8")));
        assert!(!ticked.iter().any(|l| l.contains("Width 4")));
    }

    #[test]
    fn an_indent_width_that_is_not_offered_ticks_no_width_row() {
        // `--tab-width=3` is legitimate config; the menu offers 2, 4 and 8.
        // Ticking the nearest would tell the user their setting is something
        // it is not.
        let items = format(
            Encoding::Utf8,
            LineEnding::Lf,
            Indent {
                spaces: false,
                width: 3,
            },
        );
        assert!(
            !items
                .iter()
                .any(|i| i.label.starts_with('✓') && i.label.contains("Width")),
            "no width row should claim to be a width of 3"
        );
    }

    #[test]
    fn planned_menus_are_entirely_inert() {
        for name in [
            "Insert", "Data", "Note", "Notebook", "Organize", "Research", "Run", "Security",
            "Tools",
        ] {
            let items = planned_menu(name);
            assert!(!items.is_empty(), "{name} has no contents");
            assert!(
                items.iter().all(|i| !i.enabled),
                "{name} has an enabled row that does nothing"
            );
            assert!(
                items.iter().all(|i| i.action == action::NONE),
                "{name} has a row with a real action"
            );
            assert!(
                items.last().unwrap().label.contains("not implemented"),
                "{name} does not say when it arrives"
            );
        }
    }

    #[test]
    fn every_working_row_has_an_action() {
        let mut all = Vec::new();
        all.extend(file(true, true, &[]));
        // `true` so the caret-dependent rows are enabled here too -- the
        // stronger check, since a disabled row is exempt below regardless.
        all.extend(edit(&[], true));
        all.extend(view(
            ThemeId::Organic,
            false,
            true,
            false,
            bp_config::DEFAULT_FONT_SIZE,
        ));
        all.extend(format(Encoding::Utf8, LineEnding::Lf, Indent::default()));
        all.extend(data(Format::Csv));
        all.extend(help());

        for item in all.iter().filter(|i| i.enabled) {
            assert_ne!(
                item.action,
                action::NONE,
                "enabled row '{}' does nothing when clicked",
                item.label
            );
        }
    }

    #[test]
    fn recent_files_appear_newest_first_with_distinguishing_folders() {
        let recent = [
            std::path::PathBuf::from("/notes/a/Report_17AUG2026.txt"),
            std::path::PathBuf::from("/notes/b/Report_17AUG2026.txt"),
        ];
        let items = file(false, true, &recent);

        let rows: Vec<&MenuItem> = items
            .iter()
            .filter(|i| i.action >= action::RECENT_BASE && i.action < 100)
            .collect();

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].action, action::RECENT_BASE);
        assert_eq!(rows[1].action, action::RECENT_BASE + 1);
        assert!(rows.iter().all(|r| r.label == "Report_17AUG2026.txt"));
        assert_ne!(
            rows[0].shortcut, rows[1].shortcut,
            "identical names must be told apart by their folder"
        );
    }

    #[test]
    fn no_recent_files_means_no_recent_rows() {
        let items = file(false, true, &[]);
        assert!(
            items
                .iter()
                .all(|i| i.action < action::RECENT_BASE || i.action >= 100)
        );
    }

    #[test]
    fn a_full_recent_list_cannot_collide_with_editor_actions() {
        // The failure this prevents is silent: a recent id reaching 100 would
        // be routed to TextInput and open nothing.
        let recent: Vec<std::path::PathBuf> = (0..bp_config::MAX_RECENT + 5)
            .map(|i| std::path::PathBuf::from(format!("/f{i}.txt")))
            .collect();
        let items = file(false, true, &recent);

        let max = items.iter().map(|i| i.action).max().unwrap();
        assert!(
            max < 100,
            "recent ids reached {max}, which Slint would swallow"
        );
    }

    #[test]
    fn shorten_keeps_the_identifying_tail() {
        assert_eq!(shorten("/short", 34), "/short");
        let long = "/very/long/path/that/goes/on/and/on/and/on/finally/here";
        let short = shorten(long, 20);
        assert!(short.starts_with('…'));
        assert!(short.ends_with("finally/here"), "got {short}");
        assert!(short.chars().count() <= 20);
    }

    #[test]
    fn only_editor_actions_fall_in_slints_window() {
        // Slint's `dispatch` handles EDITOR_FIRST..=EDITOR_LAST itself and
        // sends everything else to Rust. An id on the wrong side of that
        // window silently does nothing when clicked, which no other test
        // would catch.
        let editor_window = action::UNDO..=action::SELECT_ALL;

        let clips = [bp_clipboard::Entry::new("copied")];
        let mut rust_side = Vec::new();
        rust_side.extend(file(true, true, &[std::path::PathBuf::from("/a.txt")]));
        rust_side.extend(view(
            ThemeId::Green,
            false,
            true,
            false,
            bp_config::DEFAULT_FONT_SIZE,
        ));
        rust_side.extend(format(Encoding::Utf8, LineEnding::Lf, Indent::default()));
        rust_side.extend(note(true));
        rust_side.extend(data(Format::Json));
        rust_side.extend(data(Format::Csv));
        rust_side.extend(help());
        rust_side.extend(insert(STAMP_CLOCK, true));
        rust_side.extend(tab_context(2, true));

        for item in rust_side.iter().filter(|i| i.enabled) {
            assert!(
                !editor_window.contains(&item.action),
                "'{}' ({}) would be swallowed by the editor",
                item.label,
                item.action
            );
        }

        // Edit is the one menu that legitimately contains widget operations,
        // so it is checked against the six by name rather than by range.
        // Every other enabled row there -- clipboard history, line operations
        // and whatever comes next -- has to sit outside the window, or the
        // focused TextInput swallows it.
        let widget_rows = [
            action::UNDO,
            action::REDO,
            action::CUT,
            action::COPY,
            action::PASTE,
            action::SELECT_ALL,
        ];
        // `true` so Duplicate Line and Move Line Up/Down are enabled here
        // too -- they are outside the editor window either way, but a
        // disabled row would skip the check below and prove nothing.
        for item in edit(&clips, true).iter().filter(|i| i.enabled) {
            if widget_rows.contains(&item.action) {
                continue;
            }
            assert!(
                !editor_window.contains(&item.action),
                "'{}' ({}) would be swallowed by the editor",
                item.label,
                item.action
            );
        }
    }

    #[test]
    fn clipboard_rows_are_indexed_from_the_base() {
        let clips = [
            bp_clipboard::Entry::new("first"),
            bp_clipboard::Entry::new("second"),
        ];
        let items = edit(&clips, false);
        // Bounded above by `clip_end()`: both entries are plain text and
        // single words, so each also offers transform rows (As Bullet
        // List, As Code Block) whose ids live past `clip_end()` -- an
        // unbounded `>= CLIP_BASE` filter here would count those too.
        let rows: Vec<&MenuItem> = items
            .iter()
            .filter(|i| i.action >= action::CLIP_BASE && i.action < clip_end())
            .collect();

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].action, action::CLIP_BASE);
        assert!(rows[0].label.contains("first"));
        assert_eq!(rows[1].action, action::CLIP_BASE + 1);
    }

    #[test]
    fn a_full_clipboard_history_makes_the_edit_menu_taller_than_any_window() {
        // The Edit menu's height is user data, not a constant: one row per
        // clipboard entry plus up to three for the transformations it offers,
        // and the line operations sit *after* that block. At capacity the menu
        // wants more rows than a screen has.
        //
        // This is the reason `MenuPopup` caps its height and scrolls instead
        // of binding to `content.preferred-height`. Without the cap the rows
        // that fall off the bottom are simply unreachable, and observed on a
        // 719px window, five copied clips were already enough to hide every
        // row below them.
        let clips: Vec<bp_clipboard::Entry> = (0..bp_clipboard::MAX_ENTRIES)
            .map(|i| bp_clipboard::Entry::new(&format!("first line {i}\nsecond line {i}")))
            .collect();
        let items = edit(&clips, true);

        // A row is 28px. Against the 680px client area the window starts with,
        // and menus opening 28px down, about 23 rows is all that fits.
        assert!(
            items.len() > 60,
            "a full history should ask for far more rows than a window can \
             show, so that the cap is doing something; got {}",
            items.len()
        );
        // The rows most worth reaching are the ones furthest down, and five of
        // these have no keyboard shortcut at all.
        for action in [
            action::LINES_SORT_ASC,
            action::LINES_DEDUPE,
            action::LINES_TRIM,
            action::MOVE_LINE_DOWN,
        ] {
            assert!(
                items.iter().any(|i| i.action == action),
                "action {action} must still be in the menu -- scrolling is \
                 what puts it back in reach"
            );
        }
    }

    #[test]
    fn an_entry_with_only_rejected_transforms_offers_no_transform_row() {
        // A path with no backslashes: `transforms_for(Path)` offers only
        // `ForwardSlashes`, and `apply` refuses because nothing would
        // change. The plain-paste row must still be there; nothing beyond
        // it should be.
        let clips = [bp_clipboard::Entry::new("/usr/local/bin")];
        let items = edit(&clips, false);

        assert!(
            items.iter().any(|i| i.action == action::CLIP_BASE),
            "the plain paste row must still be offered"
        );
        assert!(
            items.iter().all(|i| i.action < action::CLIP_TRANSFORM_BASE),
            "a transform that changes nothing must not get a row"
        );
    }

    #[test]
    fn a_json_entry_offers_pretty_print_and_minify_routed_through_bp_data() {
        // `bp_clipboard::apply` always returns `None` for these two -- the
        // shell is supposed to route them to `bp-data` instead, and the
        // menu row is the proof that routing actually happens.
        let clips = [bp_clipboard::Entry::new(r#"{"b":1,"a":2}"#)];
        let items = edit(&clips, false);

        assert!(
            items.iter().any(|i| i.label.contains("Pretty-Print JSON")),
            "unsorted, compact JSON should offer to be pretty-printed"
        );
        assert!(
            items.iter().any(|i| i.label.contains("Minify JSON")),
            "unsorted JSON minifies to a different (sorted) string"
        );
    }

    #[test]
    fn plain_clipboard_ids_and_transform_ids_never_overlap() {
        assert!(
            clip_end() <= action::CLIP_TRANSFORM_BASE,
            "a full clipboard history would run into the transform-row ids"
        );
    }

    #[test]
    fn no_clip_kind_offers_more_transforms_than_the_reserved_slots() {
        // The id scheme assumes a fixed number of slots per entry. A kind
        // that grows past it would make two transforms share an id instead
        // of failing to compile or panicking -- exactly the silent failure
        // this codebase cares most about catching.
        for kind in [
            bp_clipboard::ClipKind::PlainText,
            bp_clipboard::ClipKind::Json,
            bp_clipboard::ClipKind::Url,
            bp_clipboard::ClipKind::Path,
            bp_clipboard::ClipKind::Code,
            bp_clipboard::ClipKind::Markdown,
        ] {
            let offered = bp_clipboard::transforms_for(kind).len();
            assert!(
                i32::try_from(offered).unwrap_or(i32::MAX) <= action::CLIP_TRANSFORM_SLOTS,
                "{kind:?} offers {offered} transforms, more than CLIP_TRANSFORM_SLOTS reserves"
            );
        }
    }

    #[test]
    fn duplicate_and_move_line_rows_are_present_but_disabled_without_the_custom_editor_view() {
        let items = edit(&[], false);
        for id in [
            action::DUPLICATE_LINE,
            action::MOVE_LINE_UP,
            action::MOVE_LINE_DOWN,
        ] {
            let row = items
                .iter()
                .find(|i| i.action == id)
                .unwrap_or_else(|| panic!("row for action {id} is missing"));
            assert!(
                !row.enabled,
                "'{}' needs the caret, which TextInput does not expose",
                row.label
            );
        }
    }

    #[test]
    fn duplicate_and_move_line_rows_are_enabled_under_the_custom_editor_view() {
        let items = edit(&[], true);
        for id in [
            action::DUPLICATE_LINE,
            action::MOVE_LINE_UP,
            action::MOVE_LINE_DOWN,
        ] {
            let row = items
                .iter()
                .find(|i| i.action == id)
                .unwrap_or_else(|| panic!("row for action {id} is missing"));
            assert!(row.enabled, "'{}' should be usable here", row.label);
        }
    }

    #[test]
    fn csv_and_tsv_offer_conversions_and_column_types() {
        for fmt in [Format::Csv, Format::Tsv] {
            let items = data(fmt);
            for id in [
                action::DATA_REPORT,
                action::DATA_COLUMN_TYPES,
                action::DATA_CSV_TO_JSON,
                action::DATA_CSV_TO_JSONL,
            ] {
                let row = items
                    .iter()
                    .find(|i| i.action == id)
                    .unwrap_or_else(|| panic!("{fmt:?} is missing action {id}"));
                assert!(
                    row.enabled,
                    "'{}' should not be offered disabled",
                    row.label
                );
            }
        }
    }

    #[test]
    fn only_csv_and_tsv_offer_the_csv_specific_data_rows() {
        let items = data(Format::Json);
        for id in [
            action::DATA_COLUMN_TYPES,
            action::DATA_CSV_TO_JSON,
            action::DATA_CSV_TO_JSONL,
        ] {
            assert!(
                items.iter().all(|i| i.action != id),
                "JSON has no delimited table to convert or type-check"
            );
        }
    }

    #[test]
    fn an_empty_clipboard_history_says_so_rather_than_showing_nothing() {
        let items = edit(&[], false);
        assert!(
            items.iter().any(|i| i.label.contains("nothing copied yet")),
            "an empty section with no explanation reads as broken"
        );
    }
}
