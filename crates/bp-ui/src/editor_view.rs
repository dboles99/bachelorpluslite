//! Custom editor surface.
//!
//! Owns the custom `EditorSurface` handlers and the key translation the
//! surface needs.

use std::rc::Rc;

use crate::state::AppState;
use crate::{AppWindow, EditorRow, SelectionBox};

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
/// Shares `offset_at_row_cell` with [`place_caret`] rather than resolving the
/// cell its own way: a second resolution would be a second set of tab-stop,
/// wrapping and clamping rules to keep in step, and the first click of a
/// double click has already placed the caret through the other one.
pub(crate) fn select_at_cell(state: &mut AppState, row: i32, column: i32, what: ClickSelection) {
    state.sync_wrap();
    let anchor = state.anchor;
    let layout = state.layout();
    let Some(editor) = state.active_editor_mut() else {
        return;
    };
    let offset = bp_editor::view::offset_at_row_cell(
        editor.buffer(),
        anchor,
        usize::try_from(row).unwrap_or(0),
        usize::try_from(column).unwrap_or(0),
        layout,
    );
    match what {
        ClickSelection::Word => editor.select_word_at(offset),
        ClickSelection::Line => editor.select_line_at(offset),
    }
}

/// Move the caret to a clicked cell, extending the selection if asked.
pub(crate) fn place_caret(state: &mut AppState, row: i32, column: i32, extend: bool) {
    state.sync_wrap();
    let anchor = state.anchor;
    let layout = state.layout();
    let Some(editor) = state.active_editor_mut() else {
        return;
    };
    let offset = bp_editor::view::offset_at_row_cell(
        editor.buffer(),
        anchor,
        usize::try_from(row).unwrap_or(0),
        usize::try_from(column).unwrap_or(0),
        layout,
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
    // A document served from disk has no caret to reveal -- a caret is a
    // position in a rope, and the rope is a disk -- so revealing one would be
    // asking the wrong question and would snap the viewport back to line 0 on
    // every refresh.
    if !state.active_is_viewer() {
        // Before revealing, because where the caret is on screen depends on
        // how the text wraps, and the wrap width follows the window.
        state.sync_wrap();
        state.reveal_caret();
    }
    draw_editor_view(ui, state);
}

/// Draw the surface exactly where it is now.
///
/// Separate from [`push_editor_view`] because scrolling deliberately moves
/// the view away from the caret, and revealing it again would make the wheel
/// snap straight back.
/// Takes `&mut AppState` because a viewer reads the disk to answer, and a
/// method that pretends to be a getter while doing I/O is how a UI thread
/// ends up blocked without anyone being able to see why.
pub(crate) fn draw_editor_view(ui: &AppWindow, state: &mut AppState) {
    if state.active_is_viewer() {
        draw_viewer(ui, state);
        return;
    }
    let rows = state.visible_rows.max(1);
    let anchor = state.anchor;
    let layout = state.layout();
    let Some(editor) = state.active_editor() else {
        return;
    };
    let buffer = editor.buffer();

    let visible: Vec<EditorRow> = bp_editor::view::visible_rows(buffer, anchor, rows, layout)
        .into_iter()
        .map(|row| EditorRow {
            // A continuation carries no number. Numbering it again would say
            // the document has more lines than it has, and the gutter is the
            // one place a reader trusts to count.
            number: if row.is_continuation() {
                slint::SharedString::new()
            } else {
                format!("{}", row.line + 1).into()
            },
            // Expanded here rather than drawn raw: the surface positions
            // the caret at `column x advance`, and a tab painted as one glyph
            // puts every caret on the line somewhere the character is not.
            text: row.display_text(layout.tab_width).into(),
        })
        .collect();
    ui.set_editor_rows(Rc::new(slint::VecModel::from(visible)).into());

    let boxes: Vec<SelectionBox> = editor
        .selection()
        .map(|range| bp_editor::view::selection_row_spans(buffer, &range, anchor, rows, layout))
        .unwrap_or_default()
        .into_iter()
        .map(|span| SelectionBox {
            row: clamp_i32(span.row),
            start: clamp_i32(span.columns.start),
            end: clamp_i32(span.columns.end),
        })
        .collect();
    ui.set_editor_selection(Rc::new(slint::VecModel::from(boxes)).into());

    let (row, column) = bp_editor::view::caret_row_cell(buffer, anchor, editor.cursor(), layout);
    // A row outside the viewport is reported as -1 rather than drawn off the
    // edge, which is what the surface checks before showing the caret.
    let visible_row = usize::try_from(row)
        .ok()
        .filter(|row| *row < rows)
        .map_or(-1, clamp_i32);
    ui.set_caret_row(visible_row);
    ui.set_caret_column(clamp_i32(column));
}

/// Draw a document that is read from disk as the reader scrolls.
///
/// The same `EditorSurface`, handed different rows. **Nothing else about it
/// changes**, which is the argument for reusing it rather than writing a
/// second surface: the gutter, the fonts, the click-to-row arithmetic and the
/// wheel are one implementation, so a defect fixed in one view is fixed in
/// both. What the viewer supplies instead is a window of a file, no selection,
/// and no caret.
fn draw_viewer(ui: &AppWindow, state: &mut AppState) {
    let count = state.visible_rows.max(1);
    let tab_width = state.indent.width;
    let Some(view) = state.active_viewer_mut() else {
        return;
    };
    let visible = viewer_rows(view, count, tab_width);
    // The one thing a viewer can highlight: the hit a scan is standing on
    // (ADR-0042). Asked for after the rows, because it reads the same cached
    // screen they came from.
    let hit = view.highlight(count, tab_width);

    // Recorded before the borrow ends, so the status bar says which lines are
    // *drawn* rather than how many were asked for. At the end of the file, and
    // at any of `display_lines`' three caps, a screenful is short.
    state.drawn_rows = visible.len();

    ui.set_editor_rows(Rc::new(slint::VecModel::from(visible)).into());
    // No caret: it is a position in a buffer and there is no buffer, and -1
    // is what the surface already checks before drawing one. A *selection*
    // box is different -- it is a rectangle over drawn rows, which this view
    // does have, so a search hit reuses the editor's own highlight rather
    // than needing anything new on the Slint side.
    let boxes: Vec<SelectionBox> = hit
        .into_iter()
        .map(|(row, columns)| SelectionBox {
            row: clamp_i32(row),
            start: clamp_i32(columns.start),
            end: clamp_i32(columns.end),
        })
        .collect();
    ui.set_editor_selection(Rc::new(slint::VecModel::from(boxes)).into());
    ui.set_caret_row(-1);
    ui.set_caret_column(0);
}

/// The rows a window of a file becomes.
///
/// Separate from [`draw_viewer`] because everything decided here can be
/// asserted without a window, and everything left there needs one. The
/// gutter numbering and the truncation marker are decisions.
fn viewer_rows(
    view: &mut crate::viewer::HugeView,
    count: usize,
    tab_width: usize,
) -> Vec<EditorRow> {
    view.rows(count)
        .iter()
        .map(|line| EditorRow {
            // The document's own line number, not the row's. The viewport
            // starts wherever the reader scrolled to, and a gutter that
            // restarted at 1 on every screen would be the one part of a log
            // viewer nobody could use.
            number: format!("{}", line.line + 1).into(),
            // Tabs expanded for the same reason as in the editor path: Slint
            // draws `\t` as a single glyph, so a tab-indented log would render
            // with its columns collapsed. There is no caret here to be put in
            // the wrong place, which makes this a legibility fix rather than a
            // correctness one -- but it is the same fix.
            //
            // `truncated` is surfaced rather than swallowed: `display_lines`
            // caps a line at `MAX_DISPLAY_LINE_BYTES`, and a viewer that
            // silently shows the first 64 KiB of a line is lying about the
            // file.
            text: {
                let text = bp_editor::view::expand_tabs(&line.text, tab_width);
                if line.truncated {
                    format!("{text} {TRUNCATION_MARK}").into()
                } else {
                    text.into()
                }
            },
        })
        .collect()
}

/// What a line cut at `MAX_DISPLAY_LINE_BYTES` ends with.
///
/// A constant so the test asserts the product's mark rather than a copy of
/// it, and so it is one thing to change when somebody decides an ellipsis is
/// not enough.
const TRUNCATION_MARK: &str = "[…]";

/// What a key means to a document that can only be scrolled.
///
/// The mapping is deliberately narrow. Left, Right, Home and End move a caret
/// along a line, and there is no caret; Up, Down and the page keys move the
/// *view*, which is the only thing here that can move. A motion this does not
/// recognise is swallowed rather than passed on, because the alternative is an
/// arrow key doing something at the window level that the reader did not ask
/// for.
///
/// **`Ctrl+End` is refused rather than obeyed, and says what it would cost.**
/// The end of the file is not known until the whole document has been
/// indexed, which for the documents that reach this viewer means reading two
/// gigabytes to answer one question. Doing it would block the window for
/// seconds with nothing on screen to explain why; a reader who genuinely
/// wants the tail of a log is better served by being told the price than by a
/// frozen editor.
fn scroll_for_command(state: &mut AppState, command: &bp_editor::Command) -> bool {
    use bp_editor::{Command, Motion};

    // **A key the editor did not map belongs to the window, and saying it was
    // handled here is how a huge document ends up with no shortcuts at all.**
    // `command_for` returns `Ignore` for exactly that class, in its own words:
    // "Ctrl+S, Ctrl+F and the rest belong to the window." `EditorSurface`'s
    // `FocusScope` accepts whatever this reports as handled, and an accepted
    // key never bubbles to the `KeyBinding` that wanted it -- so every
    // shortcut in the product was dead in a document served from disk, Ctrl+F
    // among them, which is the only way to reach Find here.
    //
    // The swallowing below it is still right and is a different statement: an
    // *editing* command has no meaning in a document with no caret, and a
    // Delete that bubbled to the window would be worse than one that did
    // nothing.
    if matches!(command, Command::Ignore) {
        return false;
    }

    let page = state.visible_rows.max(1);
    let delta = match command {
        Command::Move { motion, .. } => match motion {
            Motion::Up => -1,
            Motion::Down => 1,
            Motion::PageUp(rows) => -isize::try_from(*rows).unwrap_or(1),
            Motion::PageDown(rows) => isize::try_from(*rows).unwrap_or(1),
            Motion::DocumentStart => {
                if let Some(view) = state.active_viewer_mut() {
                    view.scroll_to(0, page);
                }
                return true;
            }
            Motion::DocumentEnd => {
                state.error = Some(
                    "the end of this document is not known until the whole file has been \
                     read; scroll or search instead"
                        .to_owned(),
                );
                return true;
            }
            _ => return true,
        },
        _ => return true,
    };

    if let Some(view) = state.active_viewer_mut() {
        view.scroll_by(delta, page);
    }
    true
}

/// Scroll whichever kind of document is showing, by visual rows.
///
/// One function because the wheel is one gesture. The two halves are genuinely
/// different -- `step_row` walks a rope and is bounded by its length, while a
/// viewer finds the bottom of the file by asking for a line and being told
/// there is none -- but the caller has no business knowing which.
pub(crate) fn scroll_by_rows(state: &mut AppState, lines: i32) {
    if state.active_is_viewer() {
        let count = state.visible_rows.max(1);
        if let Some(view) = state.active_viewer_mut() {
            view.scroll_by(isize::try_from(lines).unwrap_or(0), count);
        }
        return;
    }

    // By visual rows, not document lines. With wrapping on the two differ, and
    // a wheel that moved whole lines would skip past everything the reader can
    // see inside a long one.
    //
    // `step_row` is bounded at both ends of the document, so the view cannot
    // scroll into empty space.
    state.sync_wrap();
    let anchor = state.anchor;
    let layout = state.layout();
    if let Some(editor) = state.active_editor() {
        state.anchor = bp_editor::view::step_row(
            editor.buffer(),
            anchor,
            isize::try_from(lines).unwrap_or(0),
            layout,
        );
    }
}

/// Carry out an editing command, reporting whether the editor claimed it.
///
/// The clipboard commands are handled here rather than in `bp-editor`: the
/// OS clipboard is the shell's business, and a crate tested without a window
/// has no way to reach one.
pub(crate) fn apply_editor_command(state: &mut AppState, command: &bp_editor::Command) -> bool {
    // A document served from disk has no caret, so every motion is a scroll
    // instead. Handled before `sync_wrap`, which asks the editor for a layout
    // that does not exist.
    //
    // Everything that is not a motion falls through and is swallowed: there
    // is no editor to apply it to, and a Delete that bubbled up to the window
    // would be worse than one that did nothing.
    if state.active_is_viewer() {
        return scroll_for_command(state, command);
    }

    // Told here rather than when the editor was created, because both
    // settings can change while documents are open and every one of them has
    // to follow. Two `Copy` assignments on the typing path are cheaper than a
    // bookkeeping pass over the map each time either moves, and they cannot
    // fall out of step.
    //
    // The wrap matters to more than drawing: Up and Down move by visual row
    // when it is on, and an editor that had not been told would move by
    // document line while the screen showed something else.
    state.sync_wrap();

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

    // --- the viewer's rows, which need no window ------------------------

    /// A file whose lines are known, written to a temporary directory that
    /// the returned handle keeps alive.
    fn a_file(text: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("log.txt");
        std::fs::write(&path, text).expect("the fixture is written");
        (dir, path)
    }

    #[test]
    fn the_gutter_numbers_the_document_and_not_the_screen() {
        // The one part of a log viewer nobody could use if it were wrong. The
        // viewport starts wherever the reader scrolled to; a gutter that
        // restarted at 1 on every screen would make "the error is at line
        // 4,812,006" unfindable.
        let (_dir, path) = a_file("alpha\nbeta\ngamma\ndelta\nepsilon\n");
        let mut view = crate::viewer::HugeView::open(&path).expect("the fixture opens");
        view.scroll_to(2, 3);

        let rows = viewer_rows(&mut view, 3, 4);
        let numbers: Vec<String> = rows.iter().map(|r| r.number.to_string()).collect();
        assert_eq!(numbers, ["3", "4", "5"]);
        assert_eq!(rows[0].text.to_string(), "gamma");
    }

    #[test]
    fn every_row_in_the_viewer_carries_a_number() {
        // Unlike the editor path, where a wrapped continuation row carries a
        // blank number because it is not a new line. The viewer does not wrap,
        // so a blank here would mean a line went missing.
        let (_dir, path) = a_file("one\ntwo\nthree\n");
        let mut view = crate::viewer::HugeView::open(&path).expect("the fixture opens");

        for row in viewer_rows(&mut view, 3, 4) {
            assert!(
                !row.number.is_empty(),
                "a viewer row with no line number: {}",
                row.text
            );
        }
    }

    #[test]
    fn a_tab_is_expanded_rather_than_handed_to_slint_as_one_glyph() {
        // The same fix the editor path needed, for a smaller reason: there is
        // no caret here to put in the wrong place, so this is legibility
        // rather than correctness -- but a tab-indented log rendered with its
        // columns collapsed is unreadable, which is the whole job.
        let (_dir, path) = a_file("a\tb\n");
        let mut view = crate::viewer::HugeView::open(&path).expect("the fixture opens");

        let text = viewer_rows(&mut view, 1, 4)[0].text.to_string();
        assert!(
            !text.contains('\t'),
            "a raw tab reached the toolkit: {text:?}"
        );
        assert_eq!(text, "a   b", "expanded to the next stop at width 4");
    }

    #[test]
    fn a_line_cut_at_the_display_cap_says_so() {
        // `display_lines` caps a line at MAX_DISPLAY_LINE_BYTES and reports
        // it. Swallowing that would make the viewer lie about the file: the
        // first 64 KiB of a line drawn as though it were the line.
        let long = "x".repeat(bp_buffer::MAX_DISPLAY_LINE_BYTES + 1024);
        let (_dir, path) = a_file(&format!("{long}\nshort\n"));
        let mut view = crate::viewer::HugeView::open(&path).expect("the fixture opens");

        let rows = viewer_rows(&mut view, 2, 4);
        assert!(
            rows[0].text.ends_with(TRUNCATION_MARK),
            "a cut line must say it was cut"
        );
        assert!(
            !rows
                .iter()
                .skip(1)
                .any(|r| r.text.ends_with(TRUNCATION_MARK)),
            "a line that fits must not be marked"
        );
    }

    #[test]
    fn an_empty_window_produces_no_rows_rather_than_a_blank_one() {
        // Scrolled past the end. The scroll itself refuses, so this is the
        // belt to that brace: if it ever did not, the surface would show a
        // gutter numbering lines that are not there.
        let (_dir, path) = a_file("one\ntwo\n");
        let mut view = crate::viewer::HugeView::open(&path).expect("the fixture opens");
        view.scroll_to(900, 10);

        // The scroll was refused, so the view is still at the top -- which is
        // the assertion. A viewer that could reach line 900 of a two-line file
        // would draw an empty screen and call it the document.
        assert_eq!(view.top(), 0);
        // Three rows for two lines of text, and that is ADR-0029 showing
        // through rather than an off-by-one: a trailing newline ends a line,
        // so the empty line after it exists and the gutter numbers it. The
        // editor's rope counts the same way, which is the point -- one
        // document must not report two line counts depending on which surface
        // is drawing it.
        assert_eq!(viewer_rows(&mut view, 10, 4).len(), 3);
    }

    // --- keys, when there is no caret for them to move -------------------

    /// A state whose active document is a huge one, served from disk.
    ///
    /// Extended with `set_len` rather than written, so the fixture costs a few
    /// hundred bytes of I/O rather than 192 MiB of it.
    fn a_state_viewing_a_huge_file() -> (tempfile::TempDir, AppState) {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("enormous.log");
        let mut text = String::new();
        for n in 1..=500 {
            text.push_str(&format!("line {n}\n"));
        }
        std::fs::write(&path, &text).expect("the fixture's real lines");
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .expect("the fixture reopens")
            .set_len(bp_buffer::HUGE_FILE_BYTES + 1)
            .expect("the fixture is extended");

        let mut state = AppState::new();
        state.visible_rows = 20;
        state.open(path);
        assert!(state.active_is_viewer(), "the fixture is not being viewed");
        (dir, state)
    }

    fn top(state: &mut AppState) -> usize {
        state.active_viewer_mut().expect("a viewer").top()
    }

    #[test]
    fn the_arrow_and_page_keys_scroll_a_document_that_has_no_caret() {
        // Without this the only way to move through a two-gigabyte log is the
        // wheel, which is not how anybody reads one.
        let (_dir, mut state) = a_state_viewing_a_huge_file();
        let modifiers = bp_editor::Modifiers::default();
        let rows = state.visible_rows;
        let press = |state: &mut AppState, key| {
            apply_editor_command(state, &bp_editor::keys::command_for(key, modifiers, rows))
        };

        assert!(press(&mut state, bp_editor::Key::Down));
        assert_eq!(top(&mut state), 1);

        assert!(press(&mut state, bp_editor::Key::PageDown));
        assert_eq!(top(&mut state), 1 + rows);

        assert!(press(&mut state, bp_editor::Key::PageUp));
        assert_eq!(top(&mut state), 1);

        assert!(press(&mut state, bp_editor::Key::Up));
        assert_eq!(top(&mut state), 0);
    }

    #[test]
    fn a_key_that_would_edit_is_swallowed_rather_than_passed_on() {
        // There is no editor to apply it to, and a Delete that bubbled up to
        // the window would be worse than one that did nothing.
        let (_dir, mut state) = a_state_viewing_a_huge_file();

        for command in [
            bp_editor::Command::Insert("x".to_owned()),
            bp_editor::Command::DeleteForward,
            bp_editor::Command::Undo,
        ] {
            assert!(apply_editor_command(&mut state, &command));
            assert_eq!(top(&mut state), 0, "an edit moved the view");
        }
        let id = state.workspace.active_id().expect("a tab");
        assert!(!state.is_dirty(id), "a viewer became unsaved");
    }

    #[test]
    fn the_end_of_the_document_is_refused_with_what_it_would_cost() {
        // The end is not known until the whole file has been indexed, which
        // for a document that reaches this viewer means reading two gigabytes
        // to answer one question -- and blocking the window while it happens,
        // with nothing on screen to say why.
        let (_dir, mut state) = a_state_viewing_a_huge_file();
        let modifiers = bp_editor::Modifiers {
            control: true,
            shift: false,
            alt: false,
        };
        let rows = state.visible_rows;

        assert!(apply_editor_command(
            &mut state,
            &bp_editor::keys::command_for(bp_editor::Key::End, modifiers, rows)
        ));
        assert_eq!(top(&mut state), 0, "it must not have moved");

        let notice = state.error.clone().unwrap_or_default();
        assert!(
            notice.contains("whole file"),
            "a refusal has to say what it is refusing and why: {notice}"
        );
    }

    #[test]
    fn ctrl_home_goes_to_the_top_because_that_costs_nothing() {
        // The asymmetry with Ctrl+End is the point: line 0 is at byte 0 and
        // needs no index at all.
        let (_dir, mut state) = a_state_viewing_a_huge_file();
        let rows = state.visible_rows;
        let chord = bp_editor::Modifiers {
            control: true,
            shift: false,
            alt: false,
        };

        state
            .active_viewer_mut()
            .expect("a viewer")
            .scroll_by(200, rows);
        assert_eq!(top(&mut state), 200);

        assert!(apply_editor_command(
            &mut state,
            &bp_editor::keys::command_for(bp_editor::Key::Home, chord, rows)
        ));
        assert_eq!(top(&mut state), 0);
    }

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
        let column = i32::try_from(state.indent.width).unwrap() + 2;
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

    /// A state with the custom view on and wrapping at `columns`.
    fn wrapped_state(text: &str, columns: usize) -> AppState {
        let mut state = AppState::new();
        state.editor_view = true;
        state.wrap_text = true;
        state.wrap_columns = columns;
        state.edit(text.to_owned());
        state.sync_wrap();
        state
    }

    #[test]
    fn the_shell_only_wraps_when_word_wrap_is_on_and_the_surface_is_drawing() {
        // Three conditions, and all of them are the shell's to check --
        // `bp-editor` is told the answer rather than working it out.
        let mut state = wrapped_state("hello world again", 8);
        assert!(state.layout().wrap.is_on());

        state.wrap_text = false;
        state.sync_wrap();
        assert!(!state.layout().wrap.is_on(), "Word Wrap off means off");

        state.wrap_text = true;
        state.editor_view = false;
        state.sync_wrap();
        assert!(
            !state.layout().wrap.is_on(),
            "under TextInput, Slint does the wrapping and we must not also"
        );
    }

    #[test]
    fn a_click_on_a_wrapped_row_lands_on_the_text_that_is_drawn_there() {
        // The whole-stack version of the mapping test in `bp-editor`: what is
        // drawn on row 1 and what a click on row 1 resolves to have to be the
        // same characters, or the caret lands somewhere the user did not
        // point.
        let mut state = wrapped_state("hello world again", 8);
        place_caret(&mut state, 1, 0, false);

        assert_eq!(
            state.active_editor().unwrap().cursor(),
            6,
            "row 1 begins at 'world'"
        );
    }

    #[test]
    fn moving_down_a_wrapped_line_stays_inside_it() {
        // Under wrapping, Down means the next row on screen. Moving by
        // document line would skip everything the reader can see.
        let mut state = wrapped_state("hello world again\nsecond", 8);
        state.active_editor_mut().unwrap().set_cursor(0);

        apply_editor_command(
            &mut state,
            &bp_editor::Command::Move {
                motion: bp_editor::Motion::Down,
                select: false,
            },
        );
        assert_eq!(state.active_editor().unwrap().cursor(), 6);
    }

    #[test]
    fn the_wrap_width_follows_the_window_rather_than_being_fixed() {
        // Resizing is the one thing that changes where a line breaks without
        // the document changing at all.
        let mut state = wrapped_state("hello world again", 8);
        assert_eq!(state.layout().wrap.columns, 8);

        state.wrap_columns = 40;
        state.sync_wrap();
        assert_eq!(state.layout().wrap.columns, 40);
    }

    #[test]
    fn a_zero_width_window_does_not_wrap_to_nothing() {
        // Slint reports a column count from a measured width, and that can be
        // zero before the first layout pass. A wrap width of zero would mean
        // wrapping off, which is a safe answer, but a width of zero columns
        // reaching the layout would not be.
        let state = wrapped_state("hello world", 0);
        assert!(state.layout().wrap.columns >= 1);
    }

    #[test]
    fn scrolling_far_past_the_end_still_shows_the_document() {
        let mut state = AppState::new();
        state.editor_view = true;
        state.edit("a\nb\nc".to_owned());
        state.visible_rows = 2;

        state.anchor = bp_editor::view::Anchor::at(99, 0);
        state.reveal_caret();
        assert!(
            state.anchor.line < state.active_editor().unwrap().buffer().len_lines(),
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
