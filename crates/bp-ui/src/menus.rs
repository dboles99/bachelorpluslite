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

    /// Security profiles, one id per `bp_security::Profile::all()` in that
    /// order. A base plus an offset for the same reason as `STAMP_BASE`: the
    /// menu and the dispatch cannot disagree about which row is which, and
    /// adding a profile cannot leave the dispatch matching the old count.
    /// Bounded by [`super::profile_end`]. 56-59 are free.
    pub const PROFILE_BASE: i32 = 52;

    /// Encrypt the active document to a `.bpadx` file (ADR-0021). 57-59 free.
    pub const ENCRYPT_DOCUMENT: i32 = 56;
    /// Privacy Mode, a session-wide override (specs §15). 58-59 free.
    pub const PRIVACY_MODE: i32 = 57;

    /// Security operations that act on the *document* rather than on its
    /// profile (specs §15): scanning, hashing, signing, verifying. 200-209,
    /// a block of their own rather than the two spare ids at 58-59 -- four
    /// rows do not fit in two, and splitting one family across two blocks is
    /// how the fifth row later lands inside the recent-files window at 60.
    ///
    /// Above 100 for the same reason `CLIP_BASE` is: Slint's `dispatch`
    /// routes exactly `UNDO..=SELECT_ALL` to the widget and everything else
    /// to Rust, so a block only has to avoid that window rather than sit
    /// below it. 207-209 are free.
    pub const SCAN_SECRETS: i32 = 200;
    pub const HASH_DOCUMENT: i32 = 201;
    pub const SIGN_DOCUMENT: i32 = 202;
    pub const VERIFY_SIGNATURE: i32 = 203;
    /// Redact what the scan found (ADR-0028). Beside `SCAN_SECRETS` because
    /// it is the same document operation continued -- the scan produces the
    /// spans and this destroys them -- and a family split across two blocks
    /// is how the next row lands somewhere it is dispatched as something
    /// else.
    pub const REDACT_SECRETS: i32 = 204;
    /// Report what identifying metadata the document carries (ADR-0028).
    pub const INSPECT_METADATA: i32 = 205;
    /// The security history (ADR-0024). In this block rather than one of its
    /// own, because it is the same family: every other row here *produces* a
    /// line in it, and a history filed away from the operations it records is
    /// the split this block's comment argues against, seen from the reading
    /// end. 207-209 are free.
    pub const SECURITY_HISTORY: i32 = 206;

    /// The YAML conversions (ADR-0023), in a block of their own rather than
    /// in the Data block at 70-79.
    ///
    /// 79 was the only id left there and these are two, and splitting a pair
    /// across two blocks is precisely the mistake `SCAN_SECRETS`' comment
    /// describes. They get their own ids rather than sharing `DATA_TO_JSON`
    /// for the same reason `DATA_CSV_TO_JSON` does: a different library
    /// function behind an identically worded row, and sharing an id would
    /// make `run_data_action`'s match depend on the format to know which one
    /// a click meant.
    ///
    /// Above 100, so outside Slint's window, on the same argument as the
    /// block above. 212-219 are free.
    pub const DATA_YAML_TO_JSON: i32 = 210;
    pub const DATA_JSON_TO_YAML: i32 = 211;

    /// File ▸ Set as Default Editor (ADR-0012), opening a block of its own at
    /// 220-229 for platform integration. 222-229 are free.
    ///
    /// Not one of the three ids left at 207-209, and not one of the eight at
    /// 212-219: those are the Security operations' block and the Data
    /// conversions', and a File action sitting inside either is how a block
    /// stops meaning anything -- which is the whole argument `SCAN_SECRETS`'
    /// comment makes about splitting a family across two blocks, seen from
    /// the other direction.
    ///
    /// Above 100, so outside Slint's `UNDO..=SELECT_ALL` window, on the same
    /// argument as the two blocks above.
    pub const SET_DEFAULT_EDITOR: i32 = 220;

    /// File ▸ New Window: a second, independent instance of this process.
    /// 222-229 are still free.
    pub const NEW_WINDOW: i32 = 221;

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

    /// Organize, ADR-0037. A block of its own -- 700-709, 702-709 free.
    pub const ORGANIZE_RELATED_NOTES: i32 = 700;
    pub const ORGANIZE_DUPLICATE_DETECTION: i32 = 701;

    /// Insert ▸ Markdown constructs. A block of their own -- 710-719, 715-719
    /// free -- rather than borrowed from an unrelated family's spare ids, for
    /// the reason `SET_DEFAULT_EDITOR`'s own comment gives.
    ///
    /// Caret-only, same constraint as the date/time stamps above: `TextInput`
    /// exposes no caret to insert at, so these need --editor-view.
    pub const INSERT_BOLD: i32 = 710;
    pub const INSERT_ITALIC: i32 = 711;
    pub const INSERT_LINK: i32 = 712;
    pub const INSERT_CODE_BLOCK: i32 = 713;
    pub const INSERT_TABLE: i32 = 714;

    /// Tools ▸ Document Inspector. A block of its own -- 720-729, 721-729
    /// free.
    pub const TOOLS_INSPECTOR: i32 = 720;

    /// Help ▸ Diagnostics. A block of its own -- 730-739, 731-739 free.
    pub const DIAGNOSTICS: i32 = 730;

    /// Research ▸ Research Report (ADR-0041). A block of its own -- 740-749,
    /// 741-749 free.
    pub const RESEARCH_REPORT: i32 = 740;

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

/// One past the last id `PROFILE_BASE` can produce.
///
/// Sized from `Profile::all()` rather than written down, so a fifth named
/// profile extends the window with the menu instead of landing outside it.
pub(crate) fn profile_end() -> i32 {
    action::PROFILE_BASE + i32::try_from(bp_security::Profile::all().len()).unwrap_or(0)
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

pub fn file(
    any_dirty: bool,
    has_path: bool,
    served_from_disk: bool,
    recent: &[std::path::PathBuf],
) -> Vec<MenuItem> {
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

    // A document served from disk in chunks (ADR-0030) has no text to write
    // and must not be re-read whole. `AppState` refuses all four by name, and
    // this greys them so the refusal is not the first the user hears of it --
    // `row_enabled(.., false)` and deliberately not `planned`, because these
    // rows exist and cannot act right now, which is a different statement
    // from "does not exist yet".
    //
    // Save All is not among them: it acts on whichever *other* tabs are
    // dirty, and a viewer is never dirty, so it is already correct.
    let writable = !served_from_disk;
    items.extend([
        row_enabled("Save", "Ctrl+S", action::SAVE, writable),
        row_enabled("Save As...", "Ctrl+Shift+S", action::SAVE_AS, writable),
        MenuItem {
            enabled: any_dirty,
            ..row_end("Save All", "", action::SAVE_ALL)
        },
        MenuItem {
            // Reload means "discard my edits and re-read the file", which
            // needs a file to re-read -- and, for a document that was
            // deliberately never loaded, would load it.
            enabled: has_path && writable,
            ..row_end("Reload from Disk", "", action::RELOAD)
        },
        MenuItem {
            separator_after: true,
            ..row_enabled("Save a Copy...", "", action::SAVE_COPY, writable)
        },
        row_end("Close Tab", "Ctrl+W", action::CLOSE_TAB),
        // Live on both platforms, and it is not the same amount of work on
        // each: on Linux it writes the artefacts, on Windows it saves a `.reg`
        // the user applies. Neither sets a default -- ADR-0012 leaves that to
        // the operating system's own UI -- so the row asks first and says so.
        //
        // The preset is named in the hint rather than chosen out of sight.
        // specs.md section 19 wants all five offered, which wants a settings
        // screen; five more rows in the File menu would be a worse answer
        // than one row that says which preset it means.
        row(
            "Set as Default Editor...",
            "Notepad Replacement",
            action::SET_DEFAULT_EDITOR,
        ),
        row("New Window", "", action::NEW_WINDOW),
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

/// The Security menu: the active document's profile, and what it permits.
///
/// The rows below the profiles are not decoration. A profile is a promise
/// about what happens to derived data, and a user cannot check a promise they
/// cannot see -- so the menu states the three that are observable today
/// rather than making them infer it from behaviour that is, by design,
/// invisible.
pub fn security(
    current: bp_security::Security,
    encrypted: bool,
    privacy: bp_security::Privacy,
    has_content: bool,
    has_path: bool,
    can_sign: bool,
    has_key: bool,
) -> Vec<MenuItem> {
    // What is actually in force, which is the document's profile *and* the
    // session override. Showing the unclamped policy would tell the user their
    // clipboard is kept while Privacy Mode is discarding it.
    let policy = current.policy_under(privacy);
    let mut items: Vec<MenuItem> = bp_security::Profile::all()
        .iter()
        .enumerate()
        .map(|(index, profile)| {
            toggle(
                profile.name(),
                matches!(current, bp_security::Security::Named(p) if p == *profile),
                action::PROFILE_BASE + i32::try_from(index).unwrap_or(0),
            )
        })
        .collect();
    if let Some(last) = items.last_mut() {
        last.separator_after = true;
    }

    // What the profile in force actually does, in the user's terms. Greyed,
    // because they are a readout rather than something to click.
    items.extend([
        planned(&format!(
            "Recovery journal: {}",
            describe_recovery(policy.recovery)
        )),
        planned(&format!(
            "Clipboard history: {}",
            describe_clipboard(policy.clipboard)
        )),
        MenuItem {
            separator_after: true,
            ..planned(&format!(
                "Leaves this machine: {}",
                match policy.network {
                    bp_security::Network::Allowed => "permitted",
                    bp_security::Network::Denied => "never",
                }
            ))
        },
        row_enabled(
            if encrypted {
                "Encrypted (.bpadx)"
            } else {
                "Encrypt Document..."
            },
            "",
            action::ENCRYPT_DOCUMENT,
            !encrypted,
        ),
        MenuItem {
            separator_after: true,
            ..toggle("Privacy Mode", privacy.is_on(), action::PRIVACY_MODE)
        },
        // Not gated on there being content: "no credentials found" is a real
        // answer, and an empty document is exactly when somebody checks they
        // are looking at the tab they think they are.
        row("Scan for Secrets", "", action::SCAN_SECRETS),
        // Greyed rather than `planned`, and the label carries the reason:
        // `bp-redaction` exists and is tested, and what is missing is a
        // document to act on. An empty document has nothing to redact, and a
        // row that looked live and then reported "nothing found" would be a
        // worse answer than one that says so before the click.
        //
        // The ellipsis is the house convention for a row that asks first, and
        // this one asks: redaction destroys text, so it takes consent rather
        // than assuming it.
        row_enabled(
            if has_content {
                "Redact Found Secrets..."
            } else {
                "Redact Found Secrets — this document is empty"
            },
            "",
            action::REDACT_SECRETS,
            has_content,
        ),
        // Not gated on content, and not gated on the format either. For a
        // container this build cannot see inside -- a .docx, a PDF -- the
        // answer worth having is exactly "there is metadata here that was not
        // looked at", and an empty buffer does not change that.
        MenuItem {
            separator_after: true,
            ..row("Inspect Metadata", "", action::INSPECT_METADATA)
        },
        // The algorithm is on the row rather than in the result, because a
        // digest you are about to read down a telephone is useless unless you
        // already know which of the two the other end took.
        row("Hash Document (SHA-256)", "", action::HASH_DOCUMENT),
        // Live since ADR-0031 answered where a signing key lives: sealed in
        // a `.bpadx` envelope under a passphrase, which is the same envelope
        // encrypted documents use and needs nothing new designed, reviewed or
        // fuzzed. It was greyed with the reason on the row for three sessions
        // before that, which is the shape a blocked-on-a-decision row should
        // take -- `row_enabled(.., false)` and not `planned`.
        //
        // Greyed now for the two reasons a signature cannot be made rather
        // than for the absence of a key store, and each says which: signing
        // is over the bytes **on disk** (ADR-0026), so a document that has
        // never been saved has nothing to sign, and one with unsaved changes
        // would get a valid signature over the previous version -- which is
        // worse than a refusal, because it verifies.
        //
        // The hint names the key, not the document. Whether one exists yet is
        // what decides which question the passphrase bar asks, and saying so
        // before the click is what stops "Sign" being followed by an
        // unexplained ceremony.
        MenuItem {
            separator_after: false,
            ..row_enabled(
                match (has_path, can_sign) {
                    (false, _) => "Sign Document — this document has never been saved",
                    (true, false) => "Sign Document — save it first",
                    (true, true) => "Sign Document...",
                },
                if has_path && can_sign {
                    if has_key {
                        "unlocks your signing key"
                    } else {
                        "creates a signing key"
                    }
                } else {
                    ""
                },
                action::SIGN_DOCUMENT,
                has_path && can_sign,
            )
        },
        // Live even though signing is not: verifying needs the other party's
        // signature and, to say more than "intact", their public key. It is
        // the half of the feature that needs nothing stored.
        //
        // Greyed rather than `planned` for a document that has never been
        // saved, and the reason is on the row: `bp_integrity::verify_file`
        // checks the *file*, and a sidecar is named after a file name, so
        // there is nothing to look beside. The hint names the convention, so
        // the user knows which file to go and find before they click.
        MenuItem {
            separator_after: true,
            ..row_enabled(
                if has_path {
                    "Verify Signature..."
                } else {
                    "Verify Signature — this document has never been saved"
                },
                if has_path { "document.ext.sig" } else { "" },
                action::VERIFY_SIGNATURE,
                has_path,
            )
        },
        // The reading end of every row above it (ADR-0024). Not gated on
        // anything: an empty history is a real answer, and the one time
        // somebody most wants to look is when they think something should be
        // there and are not sure it is.
        row("Security History...", "", action::SECURITY_HISTORY),
        planned("Lock Document"),
        arrives("phase 16"),
    ]);
    items
}

fn describe_recovery(recovery: bp_security::Recovery) -> &'static str {
    match recovery {
        // Named plainly. A user who has not thought about it should be able
        // to read this row and understand that unsaved work is on disk.
        bp_security::Recovery::Plaintext => "on, unencrypted",
        // The honest answer while bp-crypto does not exist (ADR-0020): the
        // profile asks for encryption, so the journal is off rather than
        // silently plaintext.
        bp_security::Recovery::Encrypted => "off until encryption ships (phase 15)",
        bp_security::Recovery::Disabled => "off",
    }
}

fn describe_clipboard(clipboard: bp_security::Clipboard) -> &'static str {
    match clipboard {
        bp_security::Clipboard::Persistent => "kept, including on disk",
        bp_security::Clipboard::InMemory => "kept in memory only",
        bp_security::Clipboard::Disabled => "not kept",
    }
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
    // Caret-only, same as the stamps above and for the same reason: each of
    // these inserts at a position `TextInput` cannot report.
    items.extend([
        MenuItem {
            enabled: editor_view,
            ..row("Bold", "", action::INSERT_BOLD)
        },
        MenuItem {
            enabled: editor_view,
            ..row("Italic", "", action::INSERT_ITALIC)
        },
        MenuItem {
            enabled: editor_view,
            ..row("Link", "", action::INSERT_LINK)
        },
        // The only one of the five that leaves the caret somewhere other
        // than after the inserted text -- between the fences, so typing
        // starts inside the code block rather than after it.
        MenuItem {
            enabled: editor_view,
            ..row("Code Block", "", action::INSERT_CODE_BLOCK)
        },
        MenuItem {
            enabled: editor_view,
            ..row("Table", "", action::INSERT_TABLE)
        },
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
            // No warning on this one, and the asymmetry is the point:
            // every JSON value has a YAML spelling, so this is the
            // direction that cannot lose anything (ADR-0023).
            row("Convert to YAML", "", action::DATA_JSON_TO_YAML),
        ],
        // The same shape as JSON above, minus Sort Keys: sorting a YAML
        // mapping needs an ordering over YAML nodes that ADR-0023 says does
        // not exist yet, and an unimplemented row is not offered.
        //
        // The hints are not decoration. `saphyr` parses YAML into data, and a
        // comment is not data, so a round trip has nothing to put a comment
        // back from; an alias is resolved on the way in, so the output
        // repeats a value rather than referring to it. ADR-0023 names a
        // Format row that does not say so as a trap, and the row is the last
        // place to say it before the document changes.
        Format::Yaml => vec![
            row_end("Validate", "", action::DATA_VALIDATE),
            row("Format", "comments not kept", action::DATA_FORMAT),
            row_end("Minify", "comments not kept", action::DATA_MINIFY),
            // Not greyed for a file holding several documents, even though
            // the conversion refuses one: knowing how many there are means
            // parsing the whole document, and the Data menu is rebuilt on
            // every refresh. The refusal names the count instead.
            row(
                "Convert to JSON",
                "comments not kept",
                action::DATA_YAML_TO_JSON,
            ),
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

/// The Tools menu: utilities over the active document that belong to neither
/// Note (deterministic extraction about it) nor Security (protecting it).
pub fn tools(has_content: bool) -> Vec<MenuItem> {
    vec![
        // Not gated on the format, unlike Data: `bp_semantic::statistics`
        // reads any text, and the readout also carries the format, encoding
        // and profile, which apply to every document regardless of shape.
        MenuItem {
            enabled: has_content,
            ..row_end("Document Inspector", "", action::TOOLS_INSPECTOR)
        },
        planned("Security Inspector"),
        planned("File Analysis"),
        planned("Benchmarks"),
        planned("Settings"),
        arrives("phase 19"),
    ]
}

pub fn help() -> Vec<MenuItem> {
    vec![
        row("Keyboard Shortcuts", "", action::SHORTCUTS),
        row("About BachelorPad+", "", action::ABOUT),
        row_end("Diagnostics", "", action::DIAGNOSTICS),
    ]
}

/// The Organize menu (ADR-0037): what `bp-storage`'s two new queries give a
/// user, plus what is still ahead for the rest of `docs/product/MENU_MAP.md`'s
/// Organize section.
///
/// Gated on `has_content` the same way `note`'s rows are: both read the
/// active document's text (a title to relate or check duplicates by), and an
/// empty document has none to give either.
pub fn organize(has_content: bool) -> Vec<MenuItem> {
    let mut items = vec![
        MenuItem {
            enabled: has_content,
            ..row("Related Notes", "", action::ORGANIZE_RELATED_NOTES)
        },
        MenuItem {
            enabled: has_content,
            ..row_end(
                "Duplicate Detection",
                "",
                action::ORGANIZE_DUPLICATE_DETECTION,
            )
        },
    ];
    // What is left of `docs/product/MENU_MAP.md`'s Organize section --
    // `planned_menu`'s own list, so the two real rows above are the only
    // place that list had to change.
    items.extend(planned_menu("Organize"));
    items
}

/// The Research menu (ADR-0041): the one real row `AppState::research_report`
/// gives, plus what is still ahead for the rest of
/// `docs/product/MENU_MAP.md`'s Research section.
///
/// Unlike `organize`'s rows, not gated on `has_content`: the report reads
/// the whole store through `bp-storage`, not the active document, so an
/// empty active tab is not a reason to grey it out -- the same reasoning
/// `DOCUMENT_STATS` already applies to itself above.
pub fn research() -> Vec<MenuItem> {
    let mut items = vec![row_end("Research Report", "", action::RESEARCH_REPORT)];
    // What is left of `docs/product/MENU_MAP.md`'s Research section --
    // `planned_menu`'s own list, so the one real row above is the only place
    // that list had to change.
    items.extend(planned_menu("Research"));
    items
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
        // Related Notes and Duplicate Detection are real rows now
        // (ADR-0037) -- see `organize`. What is left here is the rest of
        // what `docs/product/MENU_MAP.md`'s Organize section names.
        "Organize" => (
            &["Project", "Suggested Folder", "Topics", "Semantic Search"],
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
        // Scoped to the stamp window rather than "any real action": the
        // markdown rows beside them now carry real ids too, and a filter
        // that swept those in would count the wrong rows and still pass.
        let rows: Vec<&MenuItem> = items
            .iter()
            .filter(|i| (action::STAMP_BASE..stamp_end()).contains(&i.action))
            .collect();

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
    fn every_profile_row_is_inside_the_range_dispatch_matches() {
        // `profile_end` is sized from `Profile::all()`, so a fifth profile
        // extends the window with the menu rather than landing outside it and
        // silently doing nothing.
        let items = security(
            bp_security::Security::default(),
            false,
            bp_security::Privacy::Off,
            true,
            true,
            true,
            false,
        );
        // The profile toggles are the leading rows, by construction. Taking
        // them positionally rather than by id range is what lets the range
        // itself be the thing under test.
        let count = bp_security::Profile::all().len();
        for (index, row) in items.iter().take(count).enumerate() {
            let expected = action::PROFILE_BASE + i32::try_from(index).unwrap();
            assert_eq!(row.action, expected, "row {index} has the wrong id");
            assert!((action::PROFILE_BASE..profile_end()).contains(&row.action));
        }

        // And nothing else in the menu may sit inside that window. Encrypt
        // Document is the row that would collide first, being the next id
        // allocated in this block.
        for row in items.iter().skip(count) {
            assert!(
                !(action::PROFILE_BASE..profile_end()).contains(&row.action),
                "'{}' ({}) is inside the profile window and would be dispatched \
                 as a profile",
                row.label,
                row.action
            );
        }
    }

    #[test]
    fn exactly_one_profile_is_ticked() {
        // Two ticks would say the document is governed by two policies; none
        // would leave the user unable to tell which is in force.
        for profile in bp_security::Profile::all() {
            let items = security(
                bp_security::Security::Named(*profile),
                false,
                bp_security::Privacy::Off,
                true,
                true,
                true,
                false,
            );
            let ticked: Vec<&str> = items
                .iter()
                .filter(|i| i.label.starts_with('✓'))
                .map(|i| i.label.as_str())
                .collect();

            assert_eq!(ticked.len(), 1, "for {:?}, got {ticked:?}", profile);
            assert!(ticked[0].contains(profile.name()));
        }
    }

    #[test]
    fn a_custom_policy_ticks_no_named_profile() {
        // Custom is not one of the four, and ticking the nearest would tell
        // the user their document is governed by a profile it is not.
        let items = security(
            bp_security::Security::Custom(bp_security::Profile::Maximum.policy()),
            false,
            bp_security::Privacy::Off,
            true,
            true,
            true,
            false,
        );
        assert!(
            !items.iter().any(|i| i.label.starts_with('✓')),
            "a custom policy must not claim to be a named profile"
        );
    }

    #[test]
    fn the_menu_states_what_the_profile_actually_does() {
        // A profile is a promise about invisible behaviour. A user cannot
        // check a promise they cannot see, so the menu says it.
        let standard = security(
            bp_security::Security::Named(bp_security::Profile::Standard),
            false,
            bp_security::Privacy::Off,
            true,
            true,
            true,
            false,
        );
        assert!(
            standard
                .iter()
                .any(|i| i.label.contains("Recovery journal") && i.label.contains("unencrypted")),
            "Standard writes plaintext and the menu must say so; got {:?}",
            standard
                .iter()
                .map(|i| i.label.as_str())
                .collect::<Vec<_>>()
        );

        let maximum = security(
            bp_security::Security::Named(bp_security::Profile::Maximum),
            false,
            bp_security::Privacy::Off,
            true,
            true,
            true,
            false,
        );
        assert!(
            maximum
                .iter()
                .any(|i| i.label.contains("Leaves this machine") && i.label.contains("never"))
        );
        assert!(
            maximum
                .iter()
                .any(|i| i.label.contains("Clipboard history") && i.label.contains("not kept"))
        );
    }

    #[test]
    fn a_profile_wanting_encryption_says_recovery_is_off_not_encrypted() {
        // The honest readout while `bp-crypto` does not exist. Saying
        // "encrypted" here would be the exact lie ADR-0020 forbids.
        let items = security(
            bp_security::Security::Named(bp_security::Profile::Private),
            false,
            bp_security::Privacy::Off,
            true,
            true,
            true,
            false,
        );
        let row = items
            .iter()
            .find(|i| i.label.contains("Recovery journal"))
            .expect("a recovery row");

        assert!(
            row.label.contains("off"),
            "the journal is not being written; got {:?}",
            row.label
        );
    }

    /// The Security menu as the shell builds it for an ordinary document.
    fn security_menu() -> Vec<MenuItem> {
        security(
            bp_security::Security::default(),
            false,
            bp_security::Privacy::Off,
            true,
            true,
            true,
            false,
        )
    }

    /// Every range `handle_menu_action` matches with `contains`, in one place.
    ///
    /// Written out rather than derived, because the point is to be a second
    /// copy of what the dispatch says: a list generated from the same
    /// constants the dispatch uses would agree with it by construction and
    /// catch nothing. Adding a range arm there means adding a line here, and
    /// forgetting to is what the tests below are for.
    fn range_dispatch_windows() -> Vec<(&'static str, std::ops::Range<i32>)> {
        vec![
            (
                "recent files",
                action::RECENT_BASE
                    ..action::RECENT_BASE + i32::try_from(bp_config::MAX_RECENT).unwrap(),
            ),
            ("profiles", action::PROFILE_BASE..profile_end()),
            ("stamps", action::STAMP_BASE..stamp_end()),
            ("clipboard history", action::CLIP_BASE..clip_end()),
            (
                "paste transformations",
                action::CLIP_TRANSFORM_BASE..clip_transform_end(),
            ),
            // Inclusive in dispatch; written as a half-open range one past
            // the last so the two spellings cannot disagree.
            ("editor commands", action::UNDO..action::SELECT_ALL + 1),
            ("note actions", action::NOTE_TITLE..action::NOTE_OUTLINE + 1),
            (
                "data operations",
                action::DATA_VALIDATE..action::DATA_COLUMN_TYPES + 1,
            ),
            (
                "YAML conversions",
                action::DATA_YAML_TO_JSON..action::DATA_JSON_TO_YAML + 1,
            ),
            (
                "line operations",
                action::LINES_SORT_ASC..action::LINES_TRIM + 1,
            ),
            (
                "caret line edits",
                action::DUPLICATE_LINE..action::MOVE_LINE_DOWN + 1,
            ),
        ]
    }

    #[test]
    fn the_security_operations_sit_outside_every_range_dispatch_matches() {
        // The load-bearing one. Every id below is matched by an *exact* arm
        // in `handle_menu_action`, but several arms above it match ranges --
        // and a range arm comes first, so an id that strays into one silently
        // opens a recent file or pastes a clipboard entry instead. That is
        // the failure the recent-files window has already caused once.
        for id in [
            action::SCAN_SECRETS,
            action::HASH_DOCUMENT,
            action::SIGN_DOCUMENT,
            action::VERIFY_SIGNATURE,
            action::REDACT_SECRETS,
            action::INSPECT_METADATA,
            action::SECURITY_HISTORY,
        ] {
            for (name, window) in range_dispatch_windows() {
                assert!(
                    !window.contains(&id),
                    "id {id} falls inside the {name} window and would be dispatched as one"
                );
            }
        }
    }

    #[test]
    fn set_as_default_editor_sits_outside_every_range_dispatch_matches() {
        // A File action at 220 is matched by an exact arm, and several arms
        // above it match ranges -- a range arm comes first, so an id that
        // strayed into one would silently paste a clipboard entry instead of
        // registering a file type.
        for (name, window) in range_dispatch_windows() {
            assert!(
                !window.contains(&action::SET_DEFAULT_EDITOR),
                "id {} falls inside the {name} window and would be dispatched as one",
                action::SET_DEFAULT_EDITOR
            );
        }
        assert!(
            (220..230).contains(&action::SET_DEFAULT_EDITOR),
            "the comment on this id promises a block at 220-229"
        );
    }

    #[test]
    fn the_ids_this_change_adds_sit_outside_every_range_dispatch_matches() {
        // Each is matched by an exact arm in `handle_menu_action`, and a
        // range arm coming first would silently dispatch it as something
        // else -- the same failure `SET_DEFAULT_EDITOR`'s own test above
        // guards against.
        for id in [
            action::NEW_WINDOW,
            action::ORGANIZE_RELATED_NOTES,
            action::ORGANIZE_DUPLICATE_DETECTION,
            action::INSERT_BOLD,
            action::INSERT_ITALIC,
            action::INSERT_LINK,
            action::INSERT_CODE_BLOCK,
            action::INSERT_TABLE,
            action::TOOLS_INSPECTOR,
            action::DIAGNOSTICS,
            action::RESEARCH_REPORT,
        ] {
            for (name, window) in range_dispatch_windows() {
                assert!(
                    !window.contains(&id),
                    "id {id} falls inside the {name} window and would be dispatched as one"
                );
            }
        }
    }

    #[test]
    fn the_file_menu_offers_a_new_window() {
        let row = file(false, true, false, &[])
            .into_iter()
            .find(|i| i.action == action::NEW_WINDOW)
            .expect("a New Window row");
        assert!(row.enabled, "a second instance is always available");
    }

    #[test]
    fn the_markdown_insert_rows_are_caret_only_and_in_order() {
        // Same constraint as the date/time stamps beside them: `TextInput`
        // exposes no caret to insert at.
        let disabled = insert(STAMP_CLOCK, false);
        let markdown_ids = [
            action::INSERT_BOLD,
            action::INSERT_ITALIC,
            action::INSERT_LINK,
            action::INSERT_CODE_BLOCK,
            action::INSERT_TABLE,
        ];
        for id in markdown_ids {
            let row = disabled
                .iter()
                .find(|i| i.action == id)
                .unwrap_or_else(|| panic!("a row for {id}"));
            assert!(!row.enabled, "'{}' should be greyed", row.label);
        }

        let enabled = insert(STAMP_CLOCK, true);
        for id in markdown_ids {
            let row = enabled
                .iter()
                .find(|i| i.action == id)
                .unwrap_or_else(|| panic!("a row for {id}"));
            assert!(
                row.enabled,
                "'{}' should be live under --editor-view",
                row.label
            );
        }

        let labels: Vec<&str> = markdown_ids
            .iter()
            .map(|id| {
                enabled
                    .iter()
                    .find(|i| i.action == *id)
                    .unwrap()
                    .label
                    .as_str()
            })
            .collect();
        assert_eq!(labels, ["Bold", "Italic", "Link", "Code Block", "Table"]);
    }

    #[test]
    fn the_tools_menu_greys_the_inspector_on_an_empty_document() {
        let empty = tools(false);
        let row = empty
            .iter()
            .find(|i| i.action == action::TOOLS_INSPECTOR)
            .expect("a Document Inspector row");
        assert!(!row.enabled, "nothing to inspect on an empty document");

        let with_content = tools(true);
        let row = with_content
            .iter()
            .find(|i| i.action == action::TOOLS_INSPECTOR)
            .expect("a Document Inspector row");
        assert!(row.enabled);
    }

    #[test]
    fn help_offers_diagnostics_as_a_real_row() {
        let items = help();
        let row = items
            .iter()
            .find(|i| i.action == action::DIAGNOSTICS)
            .expect("a Diagnostics row");
        assert!(row.enabled, "diagnostics has nothing to grey on");
        assert_ne!(
            row.action,
            action::NONE,
            "this was a planned row and now has to do something"
        );
    }

    #[test]
    fn the_default_editor_row_names_the_preset_it_would_register_for() {
        // The preset is a real choice and this row makes one. Naming it on
        // the row is what keeps it from being made out of sight -- and the
        // ellipsis is the house convention for a row that asks first, which
        // this one does, because registration writes files.
        let row = file(false, true, false, &[])
            .into_iter()
            .find(|i| i.action == action::SET_DEFAULT_EDITOR)
            .expect("a default-editor row");

        assert!(row.enabled, "the row is live on both platforms");
        assert!(row.label.ends_with("..."), "got '{}'", row.label);
        assert!(
            row.shortcut.contains("Notepad Replacement"),
            "the hint has to name the preset; got '{}'",
            row.shortcut
        );
    }

    #[test]
    fn verifying_greys_out_for_a_document_that_has_never_been_saved() {
        // `bp_integrity::verify_file` checks the *file*, and a sidecar is
        // named after a file name -- so there is nothing to look beside.
        // `row_enabled` rather than `planned`: the capability exists and what
        // is missing is a file, which is a different sentence.
        let items = security(
            bp_security::Security::default(),
            false,
            bp_security::Privacy::Off,
            true,
            false,
            true,
            false,
        );
        let row = items
            .iter()
            .find(|i| i.action == action::VERIFY_SIGNATURE)
            .expect("a verify row");

        assert!(
            !row.enabled,
            "there is no file for a signature to sit beside"
        );
        assert_ne!(
            row.action,
            action::NONE,
            "a real capability with nothing to act on is not a planned row"
        );
        assert!(
            row.label.contains("never been saved"),
            "the row must say why it is greyed; got '{}'",
            row.label
        );
    }

    #[test]
    fn the_verify_row_names_where_it_will_look_for_the_signature() {
        // `document.ext.sig`, appended and not substituted. The row is the
        // last place to say so before a user goes hunting for the file, and
        // saying it here is cheaper than a refusal that names a path.
        let row = security_menu()
            .into_iter()
            .find(|i| i.action == action::VERIFY_SIGNATURE)
            .expect("a verify row");

        assert!(row.enabled);
        assert!(
            row.shortcut.contains(".sig"),
            "the sidecar convention belongs on the row; got '{}'",
            row.shortcut
        );
    }

    #[test]
    fn the_yaml_conversions_sit_outside_every_other_range_dispatch_matches() {
        // Same failure, from the other side. These two *are* a dispatch
        // window, so they are checked against all the others -- an id that
        // strayed into the recent-files window would open a file instead of
        // converting a document, and neither the menu nor the compiler would
        // notice.
        for id in [action::DATA_YAML_TO_JSON, action::DATA_JSON_TO_YAML] {
            for (name, window) in range_dispatch_windows() {
                if name == "YAML conversions" {
                    continue;
                }
                assert!(
                    !window.contains(&id),
                    "id {id} falls inside the {name} window and would be dispatched as one"
                );
            }
        }
    }

    #[test]
    fn yaml_offers_the_shape_json_offers_next_door() {
        // Deliberately the same idiom rather than a second one: the two rows
        // a user reaches for on a data file are Validate and Format, and a
        // menu where YAML spells them differently from JSON is a menu the
        // user has to read twice.
        let items = data(Format::Yaml);
        for id in [
            action::DATA_VALIDATE,
            action::DATA_FORMAT,
            action::DATA_MINIFY,
            action::DATA_YAML_TO_JSON,
        ] {
            let row = items
                .iter()
                .find(|i| i.action == id)
                .unwrap_or_else(|| panic!("YAML is missing action {id}"));
            assert!(
                row.enabled,
                "'{}' should not be offered disabled",
                row.label
            );
        }
    }

    #[test]
    fn the_yaml_rewriting_rows_say_what_they_will_lose_before_they_are_clicked() {
        // ADR-0023 names this exactly: a Format row that does not say
        // comments will go is a trap. The row is the last place to say it
        // while the document is still intact.
        let items = data(Format::Yaml);
        for id in [
            action::DATA_FORMAT,
            action::DATA_MINIFY,
            action::DATA_YAML_TO_JSON,
        ] {
            let row = items.iter().find(|i| i.action == id).unwrap();
            assert!(
                row.shortcut.contains("comments"),
                "'{}' rewrites through a tree and must say so; got hint '{}'",
                row.label,
                row.shortcut
            );
        }
    }

    #[test]
    fn yaml_is_not_offered_sort_keys() {
        // `bp-data` has no ordering over YAML nodes (ADR-0023), so there is
        // nothing behind the row. A row that did nothing would read as broken,
        // and a greyed one would claim the feature exists.
        let items = data(Format::Yaml);
        assert!(
            items.iter().all(|i| !i.label.contains("Sort")),
            "sorting a YAML mapping is not implemented"
        );
    }

    #[test]
    fn converting_json_to_yaml_carries_no_warning_and_yaml_to_json_does() {
        // The asymmetry is the feature. Every JSON value has a YAML spelling,
        // so that direction loses nothing; the reverse goes through a tree
        // that has nowhere to keep a comment.
        let to_yaml = data(Format::Json)
            .into_iter()
            .find(|i| i.action == action::DATA_JSON_TO_YAML)
            .expect("JSON offers a conversion to YAML");
        assert!(
            to_yaml.shortcut.is_empty(),
            "nothing is lost converting JSON to YAML; got hint '{}'",
            to_yaml.shortcut
        );

        let to_json = data(Format::Yaml)
            .into_iter()
            .find(|i| i.action == action::DATA_YAML_TO_JSON)
            .expect("YAML offers a conversion to JSON");
        assert!(!to_json.shortcut.is_empty());
    }

    #[test]
    fn a_document_with_nothing_in_it_greys_redaction_and_says_why() {
        // `row_enabled` rather than `planned`: `bp-redaction` exists and is
        // tested, and what is missing is a document to act on. The reason has
        // to be on the row, because the greying is all the user gets.
        let items = security(
            bp_security::Security::default(),
            false,
            bp_security::Privacy::Off,
            false,
            true,
            true,
            false,
        );
        let row = items
            .iter()
            .find(|i| i.action == action::REDACT_SECRETS)
            .expect("a redaction row");

        assert!(!row.enabled, "there is nothing to redact");
        assert_ne!(
            row.action,
            action::NONE,
            "a real capability with nothing to act on is not a planned row"
        );
        assert!(
            row.label.contains("empty"),
            "the row must say why it is greyed; got '{}'",
            row.label
        );
        assert!(
            security_menu()
                .iter()
                .find(|i| i.action == action::REDACT_SECRETS)
                .is_some_and(|i| i.enabled),
            "a document with content can be redacted"
        );
    }

    #[test]
    fn the_metadata_inspector_is_offered_on_every_document() {
        // Not gated on content, and not on the format. For a container this
        // build cannot see inside, "there is metadata here that was not looked
        // at" is the answer worth having, and an empty buffer does not change
        // it.
        for has_content in [false, true] {
            let items = security(
                bp_security::Security::default(),
                false,
                bp_security::Privacy::Off,
                has_content,
                true,
                true,
                false,
            );
            let row = items
                .iter()
                .find(|i| i.action == action::INSPECT_METADATA)
                .expect("a metadata row");
            assert!(row.enabled, "'{}' should be usable", row.label);
        }
    }

    #[test]
    fn the_security_menu_no_longer_calls_redaction_planned() {
        // The planned row and the live rows would otherwise both be on screen,
        // which reads as the feature being in two states at once.
        let items = security_menu();
        assert!(
            !items
                .iter()
                .any(|i| i.action == action::NONE && i.label.contains("Redaction")),
            "redaction is live; it must not also be listed as planned"
        );
    }

    #[test]
    fn the_security_operations_are_distinct_ids_in_their_documented_block() {
        // The comment on `SCAN_SECRETS` promises 200-209. A row taking 210
        // would compile, and would sit inside the YAML conversions instead,
        // which is exactly how a block stops being a block.
        let ids = [
            action::SCAN_SECRETS,
            action::HASH_DOCUMENT,
            action::SIGN_DOCUMENT,
            action::VERIFY_SIGNATURE,
            action::REDACT_SECRETS,
            action::INSPECT_METADATA,
            action::SECURITY_HISTORY,
        ];
        for id in ids {
            assert!((200..210).contains(&id), "id {id} is outside the block");
        }
        let mut sorted = ids;
        sorted.sort_unstable();
        sorted.windows(2).for_each(|pair| {
            assert_ne!(pair[0], pair[1], "two security rows share an id");
        });
    }

    #[test]
    fn the_security_menu_no_longer_calls_the_history_planned() {
        // The same trap as the redaction row above: a planned label and a
        // live row for the same thing reads as the feature being in two
        // states at once. `Lock Document` is still planned and must stay.
        let items = security_menu();
        assert!(
            !items
                .iter()
                .any(|i| i.action == action::NONE && i.label.contains("audit history")),
            "the security history is live; it must not also be listed as planned"
        );
        assert!(
            items
                .iter()
                .any(|i| i.action == action::NONE && i.label.contains("Lock Document")),
            "Lock Document is still planned and must still say so"
        );
    }

    #[test]
    fn the_history_is_offered_whatever_the_document_is() {
        // An empty history is an answer, and the moment somebody most wants
        // to look is when they expected an event and are not sure it is
        // there. Gating this row would hide exactly that case.
        let row = security_menu()
            .into_iter()
            .find(|i| i.action == action::SECURITY_HISTORY)
            .expect("no row for the security history");
        assert!(row.enabled, "'{}' should be usable", row.label);
    }

    #[test]
    fn scanning_and_hashing_are_offered_on_every_document() {
        // Neither needs a path, a profile or content: an empty document has a
        // digest and has no credentials in it, and both are answers.
        let items = security_menu();
        for id in [action::SCAN_SECRETS, action::HASH_DOCUMENT] {
            let row = items
                .iter()
                .find(|i| i.action == id)
                .unwrap_or_else(|| panic!("no row for action {id}"));
            assert!(row.enabled, "'{}' should be usable", row.label);
        }
    }

    #[test]
    fn the_hash_row_names_the_algorithm_before_it_is_clicked() {
        // A digest read down a telephone is only comparable if both ends know
        // which one was taken.
        let items = security_menu();
        let row = items
            .iter()
            .find(|i| i.action == action::HASH_DOCUMENT)
            .expect("a hash row");
        assert!(
            row.label.contains("SHA-256"),
            "the row should say which digest; got '{}'",
            row.label
        );
    }

    /// The Security menu for a document in a given state, for the signing
    /// rows.
    fn signing_row(has_path: bool, can_sign: bool, has_key: bool) -> MenuItem {
        security(
            bp_security::Security::default(),
            false,
            bp_security::Privacy::Off,
            true,
            has_path,
            can_sign,
            has_key,
        )
        .into_iter()
        .find(|i| i.action == action::SIGN_DOCUMENT)
        .expect("a signing row")
    }

    #[test]
    fn signing_is_live_now_that_a_key_has_somewhere_to_live() {
        // This row was greyed for three sessions with "no signing key yet" on
        // it, because `bp_crypto::sign_document` existed and there was
        // nowhere to keep a key. ADR-0031 answered that -- sealed in the
        // `.bpadx` envelope encrypted documents already use -- so the row
        // acts.
        let row = signing_row(true, true, false);
        assert!(row.enabled, "got '{}'", row.label);
        assert!(
            !row.label.contains("no signing key"),
            "the reason it was greyed for is gone; got '{}'",
            row.label
        );
    }

    #[test]
    fn signing_greys_for_the_two_reasons_a_signature_cannot_be_made() {
        // Neither is about a key store any more. A signature is over the
        // bytes **on disk** (ADR-0026), so a document that has never been
        // saved has nothing to sign -- and one with unsaved changes would get
        // a valid signature over the *previous* version, which is worse than
        // a refusal because it verifies.
        //
        // The two say different things, because the way out of each is
        // different: one is Save As, the other is Ctrl+S.
        let never_saved = signing_row(false, true, true);
        let unsaved_changes = signing_row(true, false, true);

        for row in [&never_saved, &unsaved_changes] {
            assert!(!row.enabled, "got '{}'", row.label);
            assert_ne!(
                row.action,
                action::NONE,
                "a real capability with a missing prerequisite is not a planned row"
            );
        }
        assert_ne!(
            never_saved.label, unsaved_changes.label,
            "two different problems must not read as one"
        );
        assert!(never_saved.label.contains("never been saved"));
        assert!(unsaved_changes.label.contains("save it first"));
    }

    #[test]
    fn the_signing_row_says_which_question_the_click_will_ask() {
        // A click is followed by a passphrase bar, and *which* one depends on
        // whether a key exists. Saying so before the click is what stops
        // "Sign" being followed by an unexplained ceremony -- and creating a
        // signing key is a thing somebody may want to know is about to happen.
        let first_time = signing_row(true, true, false);
        let afterwards = signing_row(true, true, true);

        assert!(
            first_time.shortcut.contains("creates"),
            "got '{}'",
            first_time.shortcut
        );
        assert!(
            afterwards.shortcut.contains("unlocks"),
            "got '{}'",
            afterwards.shortcut
        );
        assert_ne!(first_time.shortcut, afterwards.shortcut);
    }

    #[test]
    fn verifying_is_live_even_though_signing_is_not() {
        // Verification needs the other party's public key and their `.sig`,
        // both supplied by the user. Greying it alongside signing would hide
        // the half of the feature that needs nothing stored.
        let items = security_menu();
        assert!(
            items
                .iter()
                .find(|i| i.action == action::VERIFY_SIGNATURE)
                .is_some_and(|i| i.enabled)
        );
    }

    #[test]
    fn the_security_menu_no_longer_calls_secret_scanning_planned() {
        // The planned row and the live row would otherwise both be on screen,
        // which reads as the feature being in two states at once.
        let items = security_menu();
        assert!(
            !items
                .iter()
                .any(|i| i.action == action::NONE && i.label.contains("Secret scanning")),
            "secret scanning is live; it must not also be listed as planned"
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
        let items = file(false, true, false, &[]);
        let save_all = items
            .iter()
            .find(|i| i.action == action::SAVE_ALL)
            .expect("Save All");
        assert!(!save_all.enabled);

        let items = file(true, true, false, &[]);
        let save_all = items.iter().find(|i| i.action == action::SAVE_ALL).unwrap();
        assert!(save_all.enabled);
    }

    #[test]
    fn reload_needs_a_file_to_reload_from() {
        let items = file(false, false, false, &[]);
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
        all.extend(file(true, true, false, &[]));
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
        all.extend(data(Format::Yaml));
        all.extend(insert(STAMP_CLOCK, true));
        all.extend(tools(true));
        all.extend(help());
        all.extend(security_menu());

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
        let items = file(false, true, false, &recent);

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
        let items = file(false, true, false, &[]);
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
        let items = file(false, true, false, &recent);

        // The recent rows, picked out by their labels rather than by the id
        // range under test -- a filter written from `RECENT_BASE` would agree
        // with the code by construction and catch nothing. The File menu has
        // fixed rows above 100 of its own now (Set as Default Editor), and
        // those are not what this is about.
        let max = items
            .iter()
            .filter(|i| i.label.ends_with(".txt"))
            .map(|i| i.action)
            .max()
            .expect("a full list produces recent rows");
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
        rust_side.extend(file(
            true,
            true,
            false,
            &[std::path::PathBuf::from("/a.txt")],
        ));
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
        rust_side.extend(data(Format::Yaml));
        rust_side.extend(help());
        rust_side.extend(insert(STAMP_CLOCK, true));
        rust_side.extend(tools(true));
        rust_side.extend(tab_context(2, true));
        rust_side.extend(security(
            bp_security::Security::default(),
            false,
            bp_security::Privacy::Off,
            true,
            true,
            true,
            false,
        ));

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
    fn a_document_served_from_disk_greys_every_row_that_would_write_or_reload_it() {
        // ADR-0030. `AppState` refuses all four by name -- a save would write
        // an empty file over two gigabytes, and a reload would load a
        // document that was deliberately never loaded -- and these grey so
        // the refusal is not the first the user hears of it.
        //
        // `row_enabled(.., false)` rather than `planned`: the rows exist and
        // cannot act right now, which is a different statement from "does not
        // exist yet", and the greying is the only thing on screen that
        // explains why Ctrl+S stopped responding.
        let served = file(false, true, true, &[]);
        for id in [
            action::SAVE,
            action::SAVE_AS,
            action::SAVE_COPY,
            action::RELOAD,
        ] {
            let row = served
                .iter()
                .find(|i| i.action == id)
                .unwrap_or_else(|| panic!("the File menu lost action {id}"));
            assert!(
                !row.enabled,
                "'{}' would act on a document that is never held whole",
                row.label
            );
        }

        // And the same menu for an ordinary document, so this is a test of
        // the flag rather than of the rows always being off.
        let ordinary = file(false, true, false, &[]);
        for id in [action::SAVE, action::SAVE_AS, action::SAVE_COPY] {
            assert!(
                ordinary
                    .iter()
                    .find(|i| i.action == id)
                    .is_some_and(|row| row.enabled),
                "action {id} is greyed for an ordinary document"
            );
        }
    }

    #[test]
    fn save_all_is_not_greyed_by_the_active_document_being_a_viewer() {
        // It acts on whichever *other* tabs are dirty. A viewer is never
        // dirty, so it is already excluded -- and greying the row would stop
        // somebody saving the note in the next tab because a log is in front.
        let items = file(true, true, true, &[]);
        assert!(
            items
                .iter()
                .find(|i| i.action == action::SAVE_ALL)
                .is_some_and(|row| row.enabled),
            "Save All must still reach the other tabs"
        );
    }

    #[test]
    fn the_data_menu_and_bp_formats_agree_on_which_documents_have_one() {
        // Two answers to one question, kept in step here because there is
        // nowhere else they meet. `Format::has_data_operations` is what a
        // caller outside this crate asks -- a keyboard shortcut, a toolbar, a
        // future command palette -- and this `match` is what the menu itself
        // does. A format added to one and not the other is a row that exists
        // and a shortcut that says it does not, or the reverse.
        //
        // The trap this replaced was the same question asked of the
        // *profile*: `Profile::StructuredData` covers INI and XML, neither of
        // which `bp-data` can parse, so a menu gated on the profile failed on
        // every row it offered.
        for &format in Format::ALL {
            let items = data(format);
            let acts = items.iter().any(|i| i.action != action::NONE);
            assert_eq!(
                acts,
                format.has_data_operations(),
                "{}: the Data menu {} rows that do something, and \
                 has_data_operations says {}",
                format.label(),
                if acts { "has" } else { "has no" },
                format.has_data_operations()
            );
            assert!(
                !items.is_empty(),
                "{}: a menu with no rows at all reads as broken; a format \
                 with nothing to offer says so instead",
                format.label()
            );
        }
    }

    #[test]
    fn a_format_without_data_operations_names_itself_in_the_refusal() {
        // Offering an empty menu, or one that says only "not implemented",
        // leaves the user guessing which of the two it is. INI is the case
        // worth pinning: it is structured data, so the answer is genuinely
        // surprising.
        for format in [Format::Ini, Format::Xml, Format::PlainText] {
            let items = data(format);
            assert!(
                items.iter().any(|i| i.label.contains(format.label())),
                "{} documents get a Data menu that does not say so",
                format.label()
            );
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
