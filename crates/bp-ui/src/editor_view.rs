//! Custom editor surface.
//!
//! Owns the custom `EditorSurface` handlers and the key translation the
//! surface needs.

use std::rc::Rc;

use crate::state::AppState;
use crate::{AppWindow, EditorRow, SelectionBox};

/// Tab stops the surface draws with, as the shell currently has them set.
///
/// Read from `AppState` rather than a constant, because the same number has
/// to serve three purposes that must never disagree: how wide a tab is drawn,
/// how a click resolves to a character, and how far a soft tab reaches. A
/// drawing width that differed from the click arithmetic would put the caret
/// somewhere other than where the pointer was, on tab-indented lines only.
fn tab_width(state: &AppState) -> usize {
    state.indent.width.max(1)
}

pub(crate) fn clamp_i32(value: usize) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

/// Slint's key text as the editor understands it.
///
/// Slint reports named keys as private-use characters. `slint::platform::Key`
/// gives them names, so this stays a readable list rather than a table of
/// code points that nobody can check.
pub(crate) fn translate_key(text: &str) -> Option<bp_editor::Key> {
    use bp_editor::Key as Editor;
    use slint::platform::Key as Slint;

    let ch = text.chars().next()?;
    let is = |key: Slint| char::from(key) == ch;

    Some(if is(Slint::LeftArrow) {
        Editor::Left
    } else if is(Slint::RightArrow) {
        Editor::Right
    } else if is(Slint::UpArrow) {
        Editor::Up
    } else if is(Slint::DownArrow) {
        Editor::Down
    } else if is(Slint::Home) {
        Editor::Home
    } else if is(Slint::End) {
        Editor::End
    } else if is(Slint::PageUp) {
        Editor::PageUp
    } else if is(Slint::PageDown) {
        Editor::PageDown
    } else if is(Slint::Backspace) {
        Editor::Backspace
    } else if is(Slint::Delete) {
        Editor::Delete
    } else if is(Slint::Return) {
        Editor::Enter
    } else if is(Slint::Tab) {
        Editor::Tab
    } else if is(Slint::Escape) {
        Editor::Escape
    } else {
        Editor::Char(ch)
    })
}

/// What a double- or triple-click selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClickSelection {
    Word,
    Line,
}

/// Select the word or the line under a clicked cell.
///
/// Shares `offset_at_cell` with [`place_caret`] rather than resolving the cell
/// its own way: a second resolution would be a second set of tab-stop and
/// clamping rules to keep in step, and the first click of a double click has
/// already placed the caret through the other one.
pub(crate) fn select_at_cell(state: &mut AppState, row: i32, column: i32, what: ClickSelection) {
    let first = state.first_line;
    let width = tab_width(state);
    let Some(editor) = state.active_editor_mut() else {
        return;
    };
    let offset = bp_editor::view::offset_at_cell(
        editor.buffer(),
        first,
        usize::try_from(row).unwrap_or(0),
        usize::try_from(column).unwrap_or(0),
        width,
    );
    match what {
        ClickSelection::Word => editor.select_word_at(offset),
        ClickSelection::Line => editor.select_line_at(offset),
    }
}

/// Move the caret to a clicked cell, extending the selection if asked.
pub(crate) fn place_caret(state: &mut AppState, row: i32, column: i32, extend: bool) {
    let first = state.first_line;
    let width = tab_width(state);
    let Some(editor) = state.active_editor_mut() else {
        return;
    };
    let offset = bp_editor::view::offset_at_cell(
        editor.buffer(),
        first,
        usize::try_from(row).unwrap_or(0),
        usize::try_from(column).unwrap_or(0),
        width,
    );

    if extend {
        // Extend from the far end of any existing selection, so shift-click
        // and drag both grow it rather than restarting it.
        let anchor = editor.selection().map_or(editor.cursor(), |range| {
            if editor.cursor() == range.start {
                range.end
            } else {
                range.start
            }
        });
        editor.select(anchor, offset);
    } else {
        editor.set_cursor(offset);
    }
}

/// Hand the surface the lines it should draw, the caret and the selection.
///
/// Only the visible rows cross the boundary, which is the whole point: this
/// costs the same on a 100,000-line document as on a ten-line one.
pub(crate) fn push_editor_view(ui: &AppWindow, state: &mut AppState) {
    state.reveal_caret();
    draw_editor_view(ui, state);
}

/// Draw the surface exactly where it is now.
///
/// Separate from [`push_editor_view`] because scrolling deliberately moves
/// the view away from the caret, and revealing it again would make the wheel
/// snap straight back.
pub(crate) fn draw_editor_view(ui: &AppWindow, state: &AppState) {
    let rows = state.visible_rows.max(1);
    let first = state.first_line;
    let width = tab_width(state);
    let Some(editor) = state.active_editor() else {
        return;
    };
    let buffer = editor.buffer();

    let visible: Vec<EditorRow> = bp_editor::view::visible_lines(buffer, first, rows)
        .into_iter()
        .map(|(line, text)| EditorRow {
            number: format!("{}", line + 1).into(),
            text: text.as_str().into(),
        })
        .collect();
    ui.set_editor_rows(Rc::new(slint::VecModel::from(visible)).into());

    let boxes: Vec<SelectionBox> = editor
        .selection()
        .map(|range| {
            bp_editor::view::selection_spans(
                buffer,
                &range,
                bp_editor::view::Metrics {
                    tab_width: width,
                    ..bp_editor::view::Metrics::default()
                },
                first,
                rows,
            )
        })
        .unwrap_or_default()
        .into_iter()
        .map(|span| SelectionBox {
            row: clamp_i32(span.row),
            start: clamp_i32(span.columns.start),
            end: clamp_i32(span.columns.end),
        })
        .collect();
    ui.set_editor_selection(Rc::new(slint::VecModel::from(boxes)).into());

    let (row, column) = bp_editor::view::caret_cell(buffer, first, editor.cursor(), width);
    // A row outside the viewport is reported as -1 rather than drawn off the
    // edge, which is what the surface checks before showing the caret.
    let visible_row = usize::try_from(row)
        .ok()
        .filter(|row| *row < rows)
        .map_or(-1, clamp_i32);
    ui.set_caret_row(visible_row);
    ui.set_caret_column(clamp_i32(column));
}

/// Carry out an editing command, reporting whether the editor claimed it.
///
/// The clipboard commands are handled here rather than in `bp-editor`: the
/// OS clipboard is the shell's business, and a crate tested without a window
/// has no way to reach one.
pub(crate) fn apply_editor_command(state: &mut AppState, command: &bp_editor::Command) -> bool {
    // Told here rather than when the editor was created, because the setting
    // can change while documents are open and every one of them has to follow
    // it. One assignment of a `Copy` struct on the typing path is cheaper
    // than a bookkeeping pass over the map each time the setting moves, and
    // it cannot fall out of step.
    let indent = state.indent;
    if let Some(editor) = state.active_editor_mut() {
        editor.set_indent(indent);
    }

    match command {
        bp_editor::Command::Ignore => false,

        bp_editor::Command::Copy => {
            let selected = state
                .active_editor()
                .and_then(|editor| Some(editor.buffer().slice(editor.selection()?)));
            // Copying nothing must not wipe what the user copied earlier.
            if let Some(text) = selected.filter(|text| !text.is_empty()) {
                crate::set_os_clipboard(&text);
            }
            true
        }

        bp_editor::Command::Cut => {
            let selected = state
                .active_editor()
                .and_then(|editor| Some(editor.buffer().slice(editor.selection()?)));
            if let Some(text) = selected.filter(|text| !text.is_empty()) {
                crate::set_os_clipboard(&text);
                if let Some(editor) = state.active_editor_mut() {
                    editor.delete_selection();
                }
                state.mark_edited();
            }
            true
        }

        bp_editor::Command::Paste => {
            if let Some(text) = crate::read_os_clipboard().filter(|text| !text.is_empty()) {
                if let Some(editor) = state.active_editor_mut() {
                    editor.insert(&text);
                }
                state.mark_edited();
            }
            true
        }

        other => {
            let changed = state
                .active_editor_mut()
                .is_some_and(|editor| editor.apply(other));
            if changed {
                state.mark_edited();
            }
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menus::action;

    #[test]
    fn slints_named_keys_are_recognised() {
        // The one place a Slint constant meets a bp-editor one. Slint reports
        // named keys as private-use characters, so a mismatch here is silent:
        // the arrow keys would type invisible glyphs into the document.
        use slint::platform::Key as Slint;

        let named = |key: Slint| translate_key(&char::from(key).to_string());
        assert_eq!(named(Slint::LeftArrow), Some(bp_editor::Key::Left));
        assert_eq!(named(Slint::RightArrow), Some(bp_editor::Key::Right));
        assert_eq!(named(Slint::UpArrow), Some(bp_editor::Key::Up));
        assert_eq!(named(Slint::DownArrow), Some(bp_editor::Key::Down));
        assert_eq!(named(Slint::Home), Some(bp_editor::Key::Home));
        assert_eq!(named(Slint::End), Some(bp_editor::Key::End));
        assert_eq!(named(Slint::PageUp), Some(bp_editor::Key::PageUp));
        assert_eq!(named(Slint::PageDown), Some(bp_editor::Key::PageDown));
        assert_eq!(named(Slint::Backspace), Some(bp_editor::Key::Backspace));
        assert_eq!(named(Slint::Delete), Some(bp_editor::Key::Delete));
        assert_eq!(named(Slint::Return), Some(bp_editor::Key::Enter));
        assert_eq!(named(Slint::Tab), Some(bp_editor::Key::Tab));
        assert_eq!(named(Slint::Escape), Some(bp_editor::Key::Escape));
    }

    #[test]
    fn ordinary_text_is_not_mistaken_for_a_named_key() {
        assert_eq!(translate_key("a"), Some(bp_editor::Key::Char('a')));
        assert_eq!(translate_key("日"), Some(bp_editor::Key::Char('日')));
        assert_eq!(translate_key(""), None, "a key with no text is not one");
    }

    #[test]
    fn typing_through_the_surface_edits_and_marks_the_document_unsaved() {
        let mut state = AppState::new();
        state.editor_view = true;

        for ch in "hi".chars() {
            let command = bp_editor::Command::Insert(ch.to_string());
            assert!(apply_editor_command(&mut state, &command));
        }

        assert_eq!(state.active_text(), "hi");
        assert!(state.workspace.active().unwrap().is_dirty());
    }

    #[test]
    fn a_chord_the_editor_does_not_claim_is_left_for_the_window() {
        // If this ever returns true, Ctrl+S stops saving.
        let mut state = AppState::new();
        assert!(!apply_editor_command(
            &mut state,
            &bp_editor::Command::Ignore
        ));
    }

    #[test]
    fn the_status_bar_shows_line_and_column_only_when_we_own_the_caret() {
        let mut state = AppState::new();
        state.edit("one\ntwo".to_owned());
        state.sync_gutter();

        assert_eq!(state.cursor_label(), "2 lines", "TextInput hides its caret");

        state.editor_view = true;
        state.active_editor_mut().unwrap().set_cursor(5);
        assert_eq!(state.cursor_label(), "Ln 2, Col 2");
    }

    #[test]
    fn a_double_click_selects_the_word_under_it_and_a_triple_click_the_line() {
        let mut state = AppState::new();
        state.editor_view = true;
        state.edit("hello world\nsecond line".to_owned());

        // Row 0, column 8: inside "world", which is 6..11.
        select_at_cell(&mut state, 0, 8, ClickSelection::Word);
        assert_eq!(state.active_editor().unwrap().selection(), Some(6..11));

        // The same cell, one more click: the whole line, newline included.
        select_at_cell(&mut state, 0, 8, ClickSelection::Line);
        assert_eq!(state.active_editor().unwrap().selection(), Some(0..12));
    }

    #[test]
    fn a_click_selection_resolves_the_cell_the_same_way_a_caret_click_does() {
        // Both go through `offset_at_cell`, and this is what says so. A second
        // resolution that disagreed would select a word beside the one the
        // first click had just put the caret in -- and only on tab-indented
        // lines, where the two rule sets differ.
        let mut state = AppState::new();
        state.editor_view = true;
        state.edit("\tindented word\nsecond".to_owned());

        // A tab is `tab_width` columns wide on screen but one character in
        // the document, so this cell is only reachable correctly through the
        // shared resolution.
        let column = i32::try_from(tab_width(&state)).unwrap() + 2;
        place_caret(&mut state, 0, column, false);
        let caret = state.active_editor().unwrap().cursor();

        select_at_cell(&mut state, 0, column, ClickSelection::Word);
        let selection = state.active_editor().unwrap().selection().unwrap();
        assert!(
            selection.contains(&caret) || selection.end == caret,
            "the word selected ({selection:?}) should be the one the caret \
             landed in ({caret})"
        );
        assert_eq!(
            &state.active_text()[selection.clone()],
            "indented",
            "got {:?}",
            &state.active_text()[selection]
        );
    }

    #[test]
    fn a_click_selection_past_the_end_of_the_document_selects_rather_than_panicking() {
        // Rows and columns arrive from a pointer, so they can name a cell no
        // line reaches -- clicking in the blank area below a short document.
        let mut state = AppState::new();
        state.editor_view = true;
        state.edit("one\ntwo".to_owned());

        select_at_cell(&mut state, 40, 200, ClickSelection::Word);
        select_at_cell(&mut state, 40, 200, ClickSelection::Line);
        assert!(state.active_editor().unwrap().selection().is_some());
    }

    #[test]
    fn clicking_puts_the_caret_where_the_click_was() {
        let mut state = AppState::new();
        state.editor_view = true;
        state.edit("hello\nworld".to_owned());

        place_caret(&mut state, 1, 3, false);
        assert_eq!(state.active_editor().unwrap().cursor(), 9);
        assert!(state.active_editor().unwrap().selection().is_none());

        // Shift-clicking elsewhere extends rather than restarting.
        place_caret(&mut state, 0, 1, true);
        assert_eq!(state.active_editor().unwrap().selection(), Some(1..9));
    }

    #[test]
    fn scrolling_far_past_the_end_still_shows_the_document() {
        let mut state = AppState::new();
        state.editor_view = true;
        state.edit("a\nb\nc".to_owned());
        state.visible_rows = 2;

        state.first_line = 99;
        state.reveal_caret();
        assert!(
            state.first_line < state.active_editor().unwrap().buffer().len_lines(),
            "revealing the caret must bring the view back to the document"
        );
    }

    #[test]
    fn a_line_operation_is_an_ordinary_undoable_edit() {
        let mut state = AppState::new();
        state.edit("b\na\n".to_owned());
        state.run_line_action(action::LINES_SORT_ASC);

        assert_eq!(state.active_text(), "a\nb\n");
        assert!(
            state.workspace.active().unwrap().is_dirty(),
            "nothing should reach disk on its own"
        );
    }

    #[test]
    fn an_action_that_is_not_a_line_operation_changes_nothing() {
        let mut state = AppState::new();
        state.edit("b\na".to_owned());
        state.run_line_action(action::SAVE);

        assert_eq!(state.active_text(), "b\na");
    }
}
