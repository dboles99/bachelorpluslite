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

    pub const THEME_LIGHT: i32 = 20;
    pub const THEME_DARK: i32 = 21;
    pub const THEME_ORGANIC: i32 = 22;
    pub const THEME_GREEN: i32 = 23;

    pub const TOGGLE_GUTTER: i32 = 30;
    pub const TOGGLE_WRAP: i32 = 31;

    pub const LINE_ENDING_LF: i32 = 40;
    pub const LINE_ENDING_CRLF: i32 = 41;
    pub const ENCODING_UTF8: i32 = 42;
    pub const ENCODING_UTF8_BOM: i32 = 43;

    pub const SHORTCUTS: i32 = 50;
    pub const ABOUT: i32 = 51;

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

pub fn file(any_dirty: bool, has_path: bool) -> Vec<MenuItem> {
    vec![
        row("New", "Ctrl+N", action::NEW),
        row_end("Open...", "Ctrl+O", action::OPEN),
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
        row("Close Tab", "Ctrl+W", action::CLOSE_TAB),
    ]
}

pub fn edit() -> Vec<MenuItem> {
    vec![
        row("Undo", "Ctrl+Z", action::UNDO),
        row_end("Redo", "Ctrl+Y", action::REDO),
        row("Cut", "Ctrl+X", action::CUT),
        row("Copy", "Ctrl+C", action::COPY),
        row_end("Paste", "Ctrl+V", action::PASTE),
        row_end("Select All", "Ctrl+A", action::SELECT_ALL),
        planned("Paste Special"),
        planned("Clipboard History"),
        planned("Line Operations"),
        arrives("phase 11"),
    ]
}

pub fn view(theme: ThemeId, gutter: bool, wrap: bool) -> Vec<MenuItem> {
    vec![
        toggle("Light", theme == ThemeId::Light, action::THEME_LIGHT),
        toggle("Dark", theme == ThemeId::Dark, action::THEME_DARK),
        toggle("Organic", theme == ThemeId::Organic, action::THEME_ORGANIC),
        MenuItem {
            separator_after: true,
            ..toggle("Green", theme == ThemeId::Green, action::THEME_GREEN)
        },
        toggle("Line Numbers", gutter, action::TOGGLE_GUTTER),
        MenuItem {
            separator_after: true,
            ..toggle("Word Wrap", wrap, action::TOGGLE_WRAP)
        },
        planned("Zoom"),
        planned("Split / Preview"),
        arrives("phase 8"),
    ]
}

pub fn format(encoding: Encoding, line_ending: LineEnding) -> Vec<MenuItem> {
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
        planned("Indentation"),
        arrives("phase 5"),
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

    #[test]
    fn save_all_is_disabled_when_nothing_is_unsaved() {
        let items = file(false, true);
        let save_all = items
            .iter()
            .find(|i| i.action == action::SAVE_ALL)
            .expect("Save All");
        assert!(!save_all.enabled);

        let items = file(true, true);
        let save_all = items.iter().find(|i| i.action == action::SAVE_ALL).unwrap();
        assert!(save_all.enabled);
    }

    #[test]
    fn reload_needs_a_file_to_reload_from() {
        let items = file(false, false);
        let reload = items
            .iter()
            .find(|i| i.action == action::RELOAD)
            .expect("Reload");
        assert!(!reload.enabled, "nothing on disk to reload from");
    }

    #[test]
    fn the_active_theme_is_ticked_and_only_it() {
        let items = view(ThemeId::Green, true, false);
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
    fn view_toggles_reflect_their_state() {
        let on = view(ThemeId::Dark, true, true);
        let gutter = on
            .iter()
            .find(|i| i.action == action::TOGGLE_GUTTER)
            .unwrap();
        assert!(gutter.label.starts_with('✓'));

        let off = view(ThemeId::Dark, false, false);
        let gutter = off
            .iter()
            .find(|i| i.action == action::TOGGLE_GUTTER)
            .unwrap();
        assert!(!gutter.label.starts_with('✓'));
    }

    #[test]
    fn format_ticks_the_documents_actual_settings() {
        let items = format(Encoding::Utf8Bom, LineEnding::CrLf);
        let ticked: Vec<&str> = items
            .iter()
            .filter(|i| i.label.starts_with('✓'))
            .map(|i| i.label.as_str())
            .collect();
        assert_eq!(ticked.len(), 2, "one line ending and one encoding");
        assert!(ticked.iter().any(|l| l.contains("CRLF")));
        assert!(ticked.iter().any(|l| l.contains("BOM")));
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
        all.extend(file(true, true));
        all.extend(edit());
        all.extend(view(ThemeId::Organic, true, false));
        all.extend(format(Encoding::Utf8, LineEnding::Lf));
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
    fn editor_actions_stay_above_the_rust_boundary() {
        // Slint routes >= 100 to TextInput and everything else to Rust. An id
        // on the wrong side of that line silently does nothing.
        for item in edit().iter().filter(|i| i.enabled) {
            assert!(
                item.action >= 100,
                "'{}' is an editor action but would be sent to Rust",
                item.label
            );
        }
        for item in file(true, true).iter().filter(|i| i.enabled) {
            assert!(item.action < 100, "'{}' would never reach Rust", item.label);
        }
    }
}
