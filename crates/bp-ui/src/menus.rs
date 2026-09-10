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

    /// Privacy Mode, a session-wide override (specs §15). 56 and 58-59 free.
    pub const PRIVACY_MODE: i32 = 57;

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

    /// Tools ▸ the four readouts. A block of its own -- 720-729, 724-729
    /// free.
    pub const TOOLS_INSPECTOR: i32 = 720;
    /// The three ADR-0048 added: the policy in force, the file on disk, and
    /// where each setting came from.
    pub const TOOLS_SECURITY_INSPECTOR: i32 = 721;
    pub const TOOLS_FILE_ANALYSIS: i32 = 722;
    pub const TOOLS_CONFIGURATION: i32 = 723;

    /// Help ▸ Diagnostics. A block of its own -- 730-739, 733-739 free.
    pub const DIAGNOSTICS: i32 = 730;
    /// Help ▸ User Guide (ADR-0075). Opens the shipped `app-help/index.md`
    /// **as a document**, in a new tab -- never in a browser, because this
    /// product launches no programs (ADR-0057).
    pub const USER_GUIDE: i32 = 731;
    /// Help ▸ Report a Problem (ADR-0077). Composes a pre-filled bug report
    /// **as a document**, with Help ▸ Diagnostics already in it, and puts the
    /// issues URL on the clipboard. It opens no browser and sends nothing:
    /// this product launches no programs (ADR-0057) and makes no network
    /// connection (ADR-0006).
    pub const REPORT_PROBLEM: i32 = 732;

    /// Research ▸ Research Report (ADR-0041). A block of its own -- 740-749,
    /// 747-749 free.
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

    /// Note ▸ Tags and Recovery Checkpoints (ADR-0048). 89 is free.
    pub const NOTE_TAGS: i32 = 87;
    pub const NOTE_RECOVERY: i32 = 88;

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

    /// Research ▸ what the active document asks (ADR-0046). In the Research
    /// block with the rest of the menu rather than beside `DOCUMENT_STATS`,
    /// whose crate it shares: an id block follows the menu a row is in,
    /// because `range_dispatch_windows` is what the blocks exist to keep
    /// clear of, and that is a dispatch concern rather than a crate one.
    pub const OPEN_QUESTIONS: i32 = 745;

    /// Research ▸ What the Store Holds (ADR-0046).
    pub const STORE_CONTENTS: i32 = 746;

    /// Organize ▸ Suggested Folder (ADR-0048). 703-709 free.
    pub const ORGANIZE_SUGGESTED_FOLDER: i32 = 702;

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

/// An inert row: greyed, with no action behind it.
///
/// **It used to mean "not built yet" and it does not any more** (ADR-0048).
/// `arrives()` and `planned_menu()` went with the last unbuilt row, so every
/// remaining caller is a *readout* -- the Privacy menu's two policy lines,
/// the Data menu's "nothing to convert here", the Tools menu's account of
/// where each setting came from. Those are answers, not promises.
///
/// The name is kept because the rendering is the same and renaming it would
/// touch every call site to say nothing new. What changed is what it is
/// allowed to mean, and `no_menu_offers_a_row_that_does_nothing` is what
/// holds the line.
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

    // A document served from disk in chunks (ADR-0030) has no text to write
    // Four of these were greyed for a document served from disk, which could
    // not be written and must not be re-read whole. ADR-0063 removed that
    // document, so `writable` is gone rather than pinned to `true` -- a gate
    // that is always open is a gate a reader has to check.
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
pub fn edit(editor_view: bool) -> Vec<MenuItem> {
    let mut items = vec![
        row("Undo", "Ctrl+Z", action::UNDO),
        row_end("Redo", "Ctrl+Y", action::REDO),
        row("Cut", "Ctrl+X", action::CUT),
        row("Copy", "Ctrl+C", action::COPY),
        row_end("Paste", "Ctrl+V", action::PASTE),
        row_end("Select All", "Ctrl+A", action::SELECT_ALL),
    ];

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
        // **Not `editor_view`-gated any more, and the note that gated it was
        // wrong.** `TextInput`'s caret cannot be *read* from here, which is
        // what stops Duplicate Line and the date stamps above; it can be
        // *moved*, via `set-selection-offsets`, which is how Find Next has
        // always jumped. Going to a line only ever needed the second.
        row_end("Go to Line...", "Ctrl+G", action::GO_TO_LINE),
        // **"Multi-cursor" is not here, and its absence is the decision**
        // (ADR-0048). `TextInput` has one caret and no way to draw a second,
        // so it could only ever work under `--editor-view` -- the same
        // one-view half-feature Run Selection was dropped for, except this
        // one also needs `bp-editor` to carry a set of carets through every
        // command, every selection and every undo entry.
        //
        // That is a real feature and a large one. It comes back as a queue
        // item with a design behind it, not as a row that has been sitting in
        // a menu since the repository was scaffolded.
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
        // **"Split / Preview" is not here, and its absence is the decision**
        // (ADR-0048, answering D14). Slint 1.17.1 has no rich-text item --
        // no styled runs, no spans -- so bold inside a sentence is not
        // representable at all, and a Markdown preview that silently dropped
        // inline formatting would be worse than none. The alternative, an
        // HTML file handed to the system browser, means writing the
        // document's text to a temporary file in plaintext, which is exactly
        // what `Policy::temporary_files` exists to forbid for a Confidential
        // document.
        //
        // What a reader actually wanted from it partly exists: Note ▸ Outline
        // gives the structure. Two of the three answers this comment used to
        // give were Notebook ▸ Cell Outline and Run, and ADR-0057 removed
        // both -- so the case for a preview is *stronger* than it was, not
        // weaker, and it is still blocked on the same toolkit gap.
        //
        // Revisit if Slint ships styled text. *Split* -- two panes over one
        // document -- is a separate feature and was never the hard half.
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
        // No `arrives(..)` line: there was one naming phase 5 with **no
        // planned rows above it** (ADR-0048), so the menu said more was
        // coming and listed nothing. Line endings, encoding and indentation
        // are the whole of what `MENU_MAP.md` asks of this menu, and all of
        // it works.
    ]
}

/// The Privacy menu: what this program may write down about a document.
///
/// **It was the Security menu and had thirteen rows.** ADR-0064 removed
/// encryption, signing, secret scanning, redaction, metadata inspection and
/// the security history; what is left is the profile a document carries, the
/// session override, and a readout of what the two of them decide. That is
/// privacy rather than security, so it is named for what it does -- keeping
/// "Security" over four profiles and a toggle would be the kind of label that
/// reads as a promise.
pub fn privacy_menu(
    current: bp_security::Security,
    privacy: bp_security::Privacy,
) -> Vec<MenuItem> {
    // What is actually in force, which is the document's profile *and* the
    // session override. Showing the unclamped policy would tell the user
    // their work is journalled while Privacy Mode is discarding it.
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
    // because they are a readout rather than something to click -- and both
    // of them name an axis something actually enforces, which is why these
    // two survived and the other four readouts did not (ADR-0059 section 4).
    items.extend([
        planned(&format!(
            "Recovery journal: {}",
            describe_recovery(policy.recovery)
        )),
        MenuItem {
            separator_after: true,
            ..planned(&format!("Recorded: {}", describe_metadata(policy.metadata)))
        },
        toggle("Privacy Mode", privacy.is_on(), action::PRIVACY_MODE),
    ]);

    items
}

/// What the metadata store is permitted to keep, in one phrase.
fn describe_metadata(metadata: bp_security::Metadata) -> &'static str {
    match metadata {
        bp_security::Metadata::Summary => "path, title and tags",
        bp_security::Metadata::PathOnly => "the path only",
        bp_security::Metadata::Disabled => "nothing",
    }
}

fn describe_recovery(recovery: bp_security::Recovery) -> &'static str {
    match recovery {
        // Named plainly. A user who has not thought about it should be able
        // to read this row and understand that unsaved work is on disk.
        bp_security::Recovery::Plaintext => "on, unencrypted",
        // A profile that asked for an encrypted journal now asks for none:
        // ADR-0064 deleted the variant rather than pointing it at plaintext,
        // so there is no third answer to give here.
        bp_security::Recovery::Disabled => "off",
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
        MenuItem {
            separator_after: true,
            ..row("Document Statistics", "", action::DOCUMENT_STATS)
        },
        // The store's view of this document rather than the text's. Gated on
        // content for the same reason Keywords is: there is nothing to have
        // tagged.
        MenuItem {
            enabled: has_content,
            ..row("Tags", "", action::NOTE_TAGS)
        },
        // **Not "Revision History"** (ADR-0048). `bp-history` is a
        // crash-recovery journal, not a version store -- it holds the pending
        // checkpoint and discards it the moment a save succeeds. A row named
        // Revision History would promise successive versions to go back to.
        //
        // Ungated: whether a journal would be written at all is a property of
        // the profile, and that is worth being able to ask with an empty tab.
        row_end("Recovery Checkpoints", "", action::NOTE_RECOVERY),
        // **"Related Notes" is not here, and its absence is the decision.**
        // It is a live row in the Organize menu (ADR-0037), and two rows in
        // two menus running the same query is how one of them goes stale.
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
        // Each reads something that exists whether or not a document is
        // open: the policy belongs to the session as much as the tab, and
        // Configuration is about the process. Only File Analysis needs a
        // document, and it needs a *saved* one -- which it says itself
        // rather than being greyed, because "this has never been saved" is
        // an answer and a greyed row is not.
        row("Security Inspector", "", action::TOOLS_SECURITY_INSPECTOR),
        MenuItem {
            enabled: has_content,
            ..row("File Analysis", "", action::TOOLS_FILE_ANALYSIS)
        },
        // **"Benchmarks" is not here, and its absence is the decision**
        // (ADR-0048). `benches/` holds a README and no benchmark; a row
        // named for a suite that does not exist is the same promise "DOI
        // Lookup" was. It comes back when there is something to run.
        //
        // **"Settings" is named Configuration**, because it reads and does
        // not write. Every setting is already editable in the menu it
        // belongs to; what none of them says is where a value came from.
        row_end("Configuration", "", action::TOOLS_CONFIGURATION),
    ]
}

pub fn help() -> Vec<MenuItem> {
    vec![
        // First, because it is the row somebody who has just installed
        // this is looking for. It opens a document rather than a browser
        // (ADR-0075).
        row("User Guide", "", action::USER_GUIDE),
        row("Keyboard Shortcuts", "", action::SHORTCUTS),
        row("Report a Problem", "", action::REPORT_PROBLEM),
        row(
            &format!("About {}", bp_platform::DISPLAY_NAME),
            "",
            action::ABOUT,
        ),
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
    // Where documents sharing this one's tags already live. Counted from
    // what the user has filed, not a scheme imposed on them.
    items.push(MenuItem {
        enabled: has_content,
        ..row_end("Suggested Folder", "", action::ORGANIZE_SUGGESTED_FOLDER)
    });

    // **Three rows left this menu on 2026-08-22 and none was built**
    // (ADR-0048):
    //
    // - *Project* -- nothing in this product has a concept of a project, and
    //   inventing one to fill a menu row is how a feature nobody asked for
    //   gets built. It needs a decision first, not a row.
    // - *Topics* -- what it would show is already in two places: Note ▸ Tags
    //   for this document, and Research ▸ Research Report's dominant themes
    //   for the store.
    // - *Semantic Search* -- needs embeddings. `bp-security`'s policy has an
    //   `Embeddings` axis and nothing computes one; doing so reaches ADR-0033
    //   and is a decision rather than a row. Search ▸ cross-file search is the
    //   honest thing that exists.
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
pub fn research(has_content: bool) -> Vec<MenuItem> {
    // Two features share this menu, and the separator is where one ends and
    // the other begins (ADR-0044). Above it: what the *store* says the user
    // has been writing about (ADR-0041). Below it: what the *document in
    // front of them* cites.
    let mut items = vec![row_end("Research Report", "", action::RESEARCH_REPORT)];

    // **Not "Research Question", which `MENU_MAP.md` named until 2026-08-22**
    // -- ADR-0046, and the same correction "DOI Lookup" got one row up.
    // A question has a grammar and can be found; which one you are actually
    // asking is not something the document says, so the row is named for the
    // act it can perform.
    items.push(MenuItem {
        separator_after: true,
        ..row_enabled("Open Questions", "", action::OPEN_QUESTIONS, has_content)
    });

    // Below the second separator: the store talking about itself rather than
    // about the user's subject matter. Not gated on `has_content`, for the
    // reason Research Report is not -- it reads the store, not the tab.
    items.push(row_end("What the Store Holds", "", action::STORE_CONTENTS));

    // Nothing is left of `MENU_MAP.md`'s Research section: ADR-0046 scoped
    // its last five names, and none of them survived as a row of its own.
    // `planned_menu("Research")` is therefore gone rather than empty -- a
    // menu that ends in a separator with nothing after it is a promise the
    // product has already kept.
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every menu, built with a document open and everything available.
    ///
    /// One list so a menu added later cannot quietly escape the check below
    /// -- the failure mode of a hand-written sweep is the menu nobody added
    /// to it.
    ///
    /// **That is exactly what happened, and the comment above predicted it
    /// without preventing it.** The Run menu was absent from this list from
    /// the day it was written, so neither the readout check nor
    /// `LABEL_BUDGET` ever saw it -- and it was carrying a 52-character
    /// readout and two around 70. It is the one menu built from the document
    /// rather than written down, which is both why it was easy to forget and
    /// why it most needed the sweep.
    ///
    /// It takes arguments, so it appears once per state worth checking rather
    /// than once. A menu whose rows depend on a value is not covered by
    /// building it one way.
    fn every_menu() -> Vec<(&'static str, Vec<MenuItem>)> {
        vec![
            ("File", file(true, true, &[])),
            ("Edit", edit(true)),
            ("View", view(ThemeId::Dark, false, true, false, 14)),
            (
                "Format",
                format(Encoding::Utf8, LineEnding::Lf, Indent::default()),
            ),
            ("Insert", insert(time::OffsetDateTime::UNIX_EPOCH, true)),
            ("Note", note(true)),
            ("Organize", organize(true)),
            ("Research", research(true)),
            ("Tools", tools(true)),
            ("Help", help()),
            (
                "Privacy",
                privacy_menu(bp_security::Security::default(), bp_security::Privacy::Off),
            ),
        ]
    }

    /// The menu bar's names, read out of the Slint source that draws it.
    ///
    /// Scoped to the *named* `MenuLabel`s. The tab strip's "+" is a
    /// `MenuLabel` too and is not a menu, which is the whole reason this
    /// reads the binding rather than counting the component.
    fn menu_bar_names_in_slint() -> Vec<&'static str> {
        const SOURCE: &str = include_str!("../ui/app.slint");
        let mut names = Vec::new();
        let mut lines = SOURCE.lines();
        while let Some(line) = lines.next() {
            if !line.contains(":= MenuLabel {") {
                continue;
            }
            let label = lines.next().expect("a MenuLabel declares a label next");
            let open = label.find('"').expect("the label is a quoted string");
            let close = label.rfind('"').expect("the label is a quoted string");
            names.push(&label[open + 1..close]);
        }
        names
    }

    #[test]
    fn the_menu_bar_and_the_list_of_menus_name_the_same_menus() {
        // `every_menu` is a Rust list and `app.slint` is what a user clicks,
        // and nothing made them agree. They did agree; what had drifted was
        // `docs/product/MENU_MAP.md`, whose prose said ten while its own
        // section headings, this list and the window all said eleven.
        //
        // A count on its own would not have caught a rename, so this asserts
        // the names. **The number is deliberately spelled out as well**, so
        // that adding or removing a menu fails here and sends whoever did it
        // to MENU_MAP -- which is the step that was skipped.
        let mut bar = menu_bar_names_in_slint();
        let mut listed: Vec<&str> = every_menu().into_iter().map(|(name, _)| name).collect();
        assert_eq!(
            bar.len(),
            11,
            "the menu bar is eleven menus; MENU_MAP says so too"
        );
        bar.sort_unstable();
        listed.sort_unstable();
        assert_eq!(
            bar, listed,
            "app.slint and every_menu disagree about the menu bar"
        );
    }

    #[test]
    fn no_menu_offers_a_row_that_does_nothing() {
        // **The goal of 2026-08-22, made checkable** (ADR-0048). Every row a
        // user can see either does something or is a readout -- a line that
        // answers a question rather than inviting a click.
        //
        // A readout is recognised by carrying a colon or being a whole
        // sentence, which is deliberately loose: the point is to catch a row
        // that *looks* like a command and is not, and "Multi-cursor" or
        // "Rust Scratchpad" would fail it while "Recovery journal: on" and
        // "Nothing to convert in a plain-text document." pass.
        for (menu, items) in every_menu() {
            for item in items {
                if item.action != action::NONE {
                    continue;
                }
                assert!(
                    item.label.contains(':') || item.label.split_whitespace().count() >= 5,
                    "{menu} ▸ {:?} does nothing and does not read as a readout",
                    item.label
                );
            }
        }
    }

    /// The widest label that fits `MenuPopup`'s 280px at 13px, measured by
    /// driving the window rather than computed: "Sign Document — it has never
    /// bee…" elided at 34 characters, and "Redact Found Secrets — it is
    /// empty" at 34 did not.
    ///
    /// Deliberately generous, because this is a smoke alarm rather than a
    /// ruler: proportional text has no character count, and a test that
    /// pretended to know the exact one would fail on a font change for a
    /// reason nobody could act on.
    const LABEL_BUDGET: usize = 42;

    #[test]
    fn no_row_label_is_too_long_for_the_popup_to_show() {
        // **Found by driving the window** (ADR-0048). The popup is a fixed
        // width and clips; before `overflow: elide` a label was cut mid-word
        // with nothing to say it had been, and every label it happened to was
        // a greyed row's *reason* -- the one text on screen whose whole job
        // is to explain.
        //
        // Eliding makes truncation visible. This keeps the reasons short
        // enough not to need it, which is the half a stylesheet cannot do.
        for (menu, items) in every_menu() {
            for item in items {
                assert!(
                    item.label.chars().count() <= LABEL_BUDGET,
                    "{menu} ▸ {:?} is {} characters and will be elided by the popup",
                    item.label,
                    item.label.chars().count()
                );
            }
        }
    }

    #[test]
    fn no_menu_says_a_feature_is_coming_later() {
        // `arrives()` is gone, and this is what stops it coming back one row
        // at a time. A menu that announces a backlog is a menu somebody has
        // to keep true.
        for (menu, items) in every_menu() {
            for item in items {
                assert!(
                    !item.label.contains("not implemented"),
                    "{menu} ▸ {:?} still promises a later phase",
                    item.label
                );
            }
        }
    }

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
        let items = privacy_menu(bp_security::Security::default(), bp_security::Privacy::Off);
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
            let items = privacy_menu(
                bp_security::Security::Named(*profile),
                bp_security::Privacy::Off,
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
        let items = privacy_menu(
            bp_security::Security::Custom(bp_security::Profile::Maximum.policy()),
            bp_security::Privacy::Off,
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
        let standard = privacy_menu(
            bp_security::Security::Named(bp_security::Profile::Standard),
            bp_security::Privacy::Off,
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

        let maximum = privacy_menu(
            bp_security::Security::Named(bp_security::Profile::Maximum),
            bp_security::Privacy::Off,
        );
        assert!(
            maximum
                .iter()
                .any(|i| i.label.contains("Recorded") && i.label.contains("nothing")),
            "Maximum records nothing and the menu must say so"
        );
        assert!(
            maximum
                .iter()
                .any(|i| i.label.contains("Recovery journal") && i.label.contains("off"))
        );
        // The clipboard readout was the third assertion here until ADR-0061.
        // It is replaced rather than dropped: this test's subject is that the
        // menu *states* what a profile does, and two readouts checked is what
        // makes that a claim about the menu rather than about one row.
    }

    #[test]
    fn the_recovery_row_says_unencrypted_where_the_journal_is_unencrypted() {
        // **This is the whole licence for Private keeping a journal**
        // (ADR-0065). ADR-0064 set Private to `Disabled` on ADR-0020's rule
        // that a control which quietly weakens itself is worse than an absent
        // one -- and the word carrying that rule is *quietly*. A plaintext
        // journal under a profile that says "on, unencrypted" is not quiet.
        //
        // So this test is not decoration: if the readout ever stops naming
        // the journal's actual form, Private must go back to `Disabled`.
        for profile in [
            bp_security::Profile::Standard,
            bp_security::Profile::Private,
        ] {
            let items = privacy_menu(
                bp_security::Security::Named(profile),
                bp_security::Privacy::Off,
            );
            let row = items
                .iter()
                .find(|i| i.label.contains("Recovery journal"))
                .expect("a recovery row");

            assert!(
                row.label.contains("unencrypted"),
                "{} keeps a plaintext journal and the row must say so; got {:?}",
                profile.name(),
                row.label
            );
        }

        // And where there is no journal, it says so rather than nothing.
        let items = privacy_menu(
            bp_security::Security::Named(bp_security::Profile::Confidential),
            bp_security::Privacy::Off,
        );
        let row = items
            .iter()
            .find(|i| i.label.contains("Recovery journal"))
            .expect("a recovery row");
        assert!(row.label.contains("off"), "got {:?}", row.label);
    }

    /// The Privacy menu as the shell builds it for an ordinary document.
    fn security_menu() -> Vec<MenuItem> {
        privacy_menu(bp_security::Security::default(), bp_security::Privacy::Off)
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
            // Inclusive in dispatch; written as a half-open range one past
            // the last so the two spellings cannot disagree.
            ("editor commands", action::UNDO..action::SELECT_ALL + 1),
            ("note actions", action::NOTE_TITLE..action::NOTE_OUTLINE + 1),
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
    fn no_two_range_dispatch_windows_overlap() {
        // A range arm swallowing another range's ids is the same failure as
        // one swallowing an exact id, and nothing else checks for it. The
        // recent-files window has caused that once already, from 60 to 100.
        let windows = range_dispatch_windows();
        for (i, (left_name, left)) in windows.iter().enumerate() {
            for (right_name, right) in windows.iter().skip(i + 1) {
                assert!(
                    left.end <= right.start || right.end <= left.start,
                    "the {left_name} window {left:?} overlaps the {right_name} window {right:?}"
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
            // ADR-0046.
            action::OPEN_QUESTIONS,
            action::STORE_CONTENTS,
            // ADR-0048.
            action::ORGANIZE_SUGGESTED_FOLDER,
            action::NOTE_TAGS,
            action::NOTE_RECOVERY,
            action::TOOLS_SECURITY_INSPECTOR,
            action::TOOLS_FILE_ANALYSIS,
            action::TOOLS_CONFIGURATION,
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
        let row = file(false, true, &[])
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
        let row = file(false, true, &[])
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
    fn the_research_menu_has_nothing_planned_left_in_it() {
        let items = research(true);
        assert!(
            items.iter().all(|i| i.action != action::NONE),
            "ADR-0046 resolved the last five planned names; a row that does \
             nothing should not have outlived them"
        );
    }

    #[test]
    fn open_questions_greys_out_with_nothing_to_read_and_the_store_row_does_not() {
        let empty = research(false);
        let asks = |items: &[MenuItem], id: i32| {
            items
                .iter()
                .find(|i| i.action == id)
                .expect("the row exists")
                .enabled
        };
        assert!(
            !asks(&empty, action::OPEN_QUESTIONS),
            "a document reading has nothing to read"
        );
        assert!(
            asks(&empty, action::STORE_CONTENTS),
            "the store is there whether or not a tab is -- the same reasoning \
             Research Report already applies to itself"
        );
        assert!(asks(&research(true), action::OPEN_QUESTIONS));
    }

    #[test]
    fn the_research_menu_offers_no_row_named_for_a_paper_section() {
        // ADR-0046. The five IMRaD names were written down before anybody had
        // decided what the mode was for, and a row that carried one now would
        // be a promise about a structure this product does not have.
        for absent in [
            "Research Question",
            "Evidence",
            "Findings",
            "Methods",
            "Datasets",
        ] {
            assert!(
                !research(true).iter().any(|i| i.label.contains(absent)),
                "{absent} is back in the Research menu"
            );
        }
    }

    #[test]
    fn the_format_menu_does_not_promise_more_than_it_has() {
        // It carried an "arrives in phase 5" line with no planned rows above
        // it, so it announced a backlog and listed nothing (ADR-0048).
        assert!(
            !format(Encoding::Utf8, LineEnding::Lf, Indent::default())
                .iter()
                .any(|i| i.label.contains("not implemented")),
            "the Format menu is complete and must not say otherwise"
        );
    }

    #[test]
    fn the_note_menu_has_nothing_planned_left_in_it() {
        assert!(
            note(true).iter().all(|i| i.action != action::NONE),
            "ADR-0048 resolved all three"
        );
    }

    #[test]
    fn related_notes_lives_in_exactly_one_menu() {
        // Two rows running the same query is how one of them goes stale.
        assert!(
            !note(true).iter().any(|i| i.label.contains("Related Notes")),
            "Related Notes is back in the Note menu; it belongs to Organize"
        );
        assert!(
            organize(true)
                .iter()
                .any(|i| i.label.contains("Related Notes")),
            "and it must still be in Organize"
        );
    }

    #[test]
    fn the_note_menu_offers_no_revision_history() {
        // ADR-0048: bp-history is crash recovery, not a version store.
        assert!(
            !note(true).iter().any(|i| i.label.contains("Revision")),
            "Revision History promises versions this product does not keep"
        );
    }

    #[test]
    fn the_tools_menu_has_nothing_planned_left_in_it() {
        assert!(
            tools(true).iter().all(|i| i.action != action::NONE),
            "ADR-0048 resolved all four; a row that does nothing should not \
             have outlived them"
        );
    }

    #[test]
    fn the_tools_menu_offers_no_benchmarks_row() {
        // ADR-0048: `benches/` is a README and no benchmark. A row named for
        // a suite that does not exist is the promise "DOI Lookup" was.
        assert!(
            !tools(true).iter().any(|i| i.label.contains("Benchmark")),
            "Benchmarks is back in the Tools menu"
        );
    }

    #[test]
    fn only_file_analysis_needs_a_document() {
        let empty = tools(false);
        let asks = |id: i32| {
            empty
                .iter()
                .find(|i| i.action == id)
                .expect("the row exists")
                .enabled
        };
        assert!(
            !asks(action::TOOLS_FILE_ANALYSIS),
            "there is no file to analyse without a document"
        );
        assert!(
            asks(action::TOOLS_SECURITY_INSPECTOR),
            "the policy belongs to the session as much as to the tab"
        );
        assert!(
            asks(action::TOOLS_CONFIGURATION),
            "configuration is about the process, not the document"
        );
    }

    #[test]
    fn every_working_row_has_an_action() {
        let mut all = Vec::new();
        all.extend(file(true, true, &[]));
        // `true` so the caret-dependent rows are enabled here too -- the
        // stronger check, since a disabled row is exempt below regardless.
        all.extend(edit(true));
        all.extend(view(
            ThemeId::Organic,
            false,
            true,
            false,
            bp_config::DEFAULT_FONT_SIZE,
        ));
        all.extend(format(Encoding::Utf8, LineEnding::Lf, Indent::default()));
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
        rust_side.extend(help());
        rust_side.extend(insert(STAMP_CLOCK, true));
        rust_side.extend(tools(true));
        rust_side.extend(tab_context(2, true));
        rust_side.extend(privacy_menu(
            bp_security::Security::default(),
            bp_security::Privacy::Off,
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
        for item in edit(true).iter().filter(|i| i.enabled) {
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
    fn duplicate_and_move_line_rows_are_present_but_disabled_without_the_custom_editor_view() {
        let items = edit(false);
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
        let items = edit(true);
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
    fn the_edit_menu_offers_no_clipboard_history() {
        // ADR-0061 removed `bp-clipboard`. This replaces the test that
        // asserted the empty history explained itself -- kept as an assertion
        // of *absence* rather than deleted, because the readout row and the
        // history rows shared a code path and a partial removal would leave
        // the row saying "nothing copied yet" forever.
        let items = edit(false);
        assert!(
            !items.iter().any(|i| i.label.contains("copied")),
            "the Edit menu still mentions a clipboard history"
        );
        assert!(
            items.iter().any(|i| i.label == "Paste"),
            "Paste is the OS clipboard and stays"
        );
    }
    /// The About row, the second of the eight sites ADR-0074 found.
    ///
    /// Asserted here rather than trusted, because this row is built by a
    /// `format!` a rename could quietly replace with a literal again -- and
    /// the previous rename did exactly that in `app.slint`.
    #[test]
    fn the_about_row_must_read_the_product_name_rather_than_spell_it() {
        let about = every_menu()
            .into_iter()
            .flat_map(|(_, rows)| rows)
            .find(|r| r.action == action::ABOUT)
            .expect("no menu holds the About row");
        assert!(
            about.label.contains(bp_platform::DISPLAY_NAME),
            "About row reads {:?}, which does not name {}",
            about.label,
            bp_platform::DISPLAY_NAME
        );
    }
}
