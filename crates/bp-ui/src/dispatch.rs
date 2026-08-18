//! Menu action dispatch and shared UI helpers.
//!
//! Owns the menu-action match and the small UI helpers (dialogs, selection)
//! that the action arms and find/replace share.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use bp_core::{Document, DocumentId, Encoding, LineEnding};
use bp_theme::ThemeId;

use crate::menus::{self, action};
use crate::state::{AppState, NoteOutcome, PushText, SaveResult};
use crate::{AppWindow, set_os_clipboard};

const SHORTCUTS: &str = "\
Ctrl+N          New
Ctrl+O          Open
Ctrl+S          Save
Ctrl+Shift+S    Save As
Ctrl+W          Close tab
Ctrl+F          Find and replace
Ctrl+Z / Ctrl+Y Undo / Redo
Ctrl+X/C/V      Cut / Copy / Paste
Ctrl+A          Select all
Ctrl+= / Ctrl+- Zoom in / out
Ctrl+0          Reset zoom
Ctrl+D          Duplicate line
Alt+Up / Down   Move line up / down";

/// Static information, shown in a native dialog rather than built as a
/// bespoke window.
fn show_info(title: &str, body: &str) {
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Info)
        .set_title(title)
        .set_description(body)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

/// Ask about unsaved work before discarding it.
///
/// Blocking and native. The three-way answer matters: "Cancel" has to be
/// distinguishable from "Discard", or the safe choice becomes the
/// destructive one.
pub(crate) fn ask_about_unsaved(name: &str) -> rfd::MessageDialogResult {
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Warning)
        .set_title("Unsaved changes")
        .set_description(format!(
            "{name} has unsaved changes.\n\nSave before closing?"
        ))
        .set_buttons(rfd::MessageButtons::YesNoCancel)
        .show()
}

/// Open a file, starting in a sensible directory.
pub(crate) fn pick_file(state: &crate::state::AppState) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_directory(state.dialog_directory())
        .pick_file()
}

/// Choose a save location, starting in a sensible directory with the
/// ADR-0003 filename suggested.
pub(crate) fn pick_save_path(
    state: &crate::state::AppState,
    id: bp_core::DocumentId,
) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_directory(state.dialog_directory())
        .set_file_name(state.suggested_filename(id))
        .save_file()
}

/// Show what Replace All would do, and ask before doing it.
///
/// specs.md section 6 wants the changes visible before they are applied.
/// Replace All is the one search operation that rewrites the document in
/// places the user cannot see, so it is also the one worth confirming --
/// undo covers a mistake, but only if you notice you made one.
///
/// The listing is bounded: a preview of forty thousand replacements is not a
/// preview, and a dialog taller than the screen has no buttons on it.
pub(crate) fn confirm_replace(plan: &bp_search::ReplacePlan) -> bool {
    const SHOWN: usize = 12;

    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Warning)
        .set_title("Replace All")
        .set_description(format!("{}\n\n{}", plan.summary(), plan.preview(SHOWN)))
        .set_buttons(rfd::MessageButtons::OkCancel)
        .show()
        == rfd::MessageDialogResult::Ok
}

/// Select a character range in the editor.
///
/// Under the custom surface we own the selection, so this sets it and asks
/// only for the focus back. Under `TextInput` the widget owns it, and the
/// offsets are clamped into `i32` because that is what its API takes -- a
/// document long enough to overflow one would have other problems first.
pub(crate) fn select(ui: &AppWindow, state: &mut AppState, range: &std::ops::Range<usize>) {
    use crate::editor_view::draw_editor_view;

    if state.editor_view {
        if let Some(editor) = state.active_editor_mut() {
            editor.select(range.start, range.end);
        }
        state.reveal_caret();
        draw_editor_view(ui, state);
        ui.invoke_focus_editor();
        return;
    }
    let start = i32::try_from(range.start).unwrap_or(i32::MAX);
    let end = i32::try_from(range.end).unwrap_or(i32::MAX);
    ui.invoke_select_range(start, end);
}

/// Handle a menu action id from the UI.
///
/// Returns `Some(push)` when the action was handled and the caller should
/// refresh with the given `PushText` value. Returns `None` when the action
/// already performed its own refresh, or returned early without changing
/// state, so no further refresh is needed.
pub fn handle_menu_action(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    id: i32,
) -> Option<PushText> {
    use crate::editor_view::apply_editor_command;
    use crate::{close_with_prompt, refresh, save_with_prompt};

    let mut push = PushText::Yes;
    match id {
        action::NEW => state.borrow_mut().new_document(),
        action::OPEN => {
            let chosen = pick_file(&state.borrow());
            if let Some(path) = chosen {
                state.borrow_mut().open(path);
            }
        }
        action::SAVE => {
            let id = state.borrow().workspace.active_id();
            if let Some(id) = id {
                save_with_prompt(&mut state.borrow_mut(), id);
            }
        }
        action::SAVE_AS => {
            let target = state.borrow().workspace.active_id();
            if let Some(id) = target {
                let chosen = pick_save_path(&state.borrow(), id);
                if let Some(path) = chosen {
                    state.borrow_mut().save_document(id, Some(path));
                }
            }
        }
        action::SAVE_ALL => {
            let dirty: Vec<DocumentId> =
                state.borrow().workspace.dirty().map(Document::id).collect();
            for id in dirty {
                // Stop at the first refusal rather than firing a
                // dialog per document at someone who just cancelled.
                if save_with_prompt(&mut state.borrow_mut(), id) != SaveResult::Saved {
                    break;
                }
            }
        }
        action::RELOAD => {
            let (dirty, name) = {
                let s = state.borrow();
                let id = s.workspace.active_id();
                (
                    id.is_some_and(|i| s.is_dirty(i)),
                    id.map(|i| s.display_name(i)).unwrap_or_default(),
                )
            };
            // Reloading discards edits, so it asks like closing does.
            if dirty && ask_about_unsaved(&name) != rfd::MessageDialogResult::No {
                return None;
            }
            state.borrow_mut().reload();
        }
        action::CLOSE_TAB => {
            let id = state.borrow().workspace.active_id();
            if let Some(id) = id {
                close_with_prompt(state, id);
            }
        }

        action::THEME_LIGHT => state.borrow_mut().theme = ThemeId::Light,
        action::THEME_DARK => state.borrow_mut().theme = ThemeId::Dark,
        action::THEME_ORGANIC => state.borrow_mut().theme = ThemeId::Organic,
        action::THEME_GREEN => state.borrow_mut().theme = ThemeId::Green,

        action::TOGGLE_GUTTER => {
            let mut s = state.borrow_mut();
            s.show_gutter = !s.show_gutter;
            push = PushText::No;
        }
        action::TOGGLE_WRAP => {
            let mut s = state.borrow_mut();
            s.wrap_text = !s.wrap_text;
            push = PushText::No;
        }

        // The document does not change, so the text is not re-pushed: under
        // `TextInput` that would discard the widget's caret and selection for
        // a change of size, which is the one thing a zoom must not do.
        action::ZOOM_IN | action::ZOOM_OUT => {
            let mut s = state.borrow_mut();
            let steps = if id == action::ZOOM_IN { 1 } else { -1 };
            s.font_size = bp_config::zoom(s.font_size, steps);
            push = PushText::No;
        }
        action::ZOOM_RESET => {
            state.borrow_mut().font_size = bp_config::DEFAULT_FONT_SIZE;
            push = PushText::No;
        }

        action::LINE_ENDING_LF => state.borrow_mut().set_line_ending(LineEnding::Lf),
        action::LINE_ENDING_CRLF => state.borrow_mut().set_line_ending(LineEnding::CrLf),
        action::ENCODING_UTF8 => state.borrow_mut().set_encoding(Encoding::Utf8),
        action::ENCODING_UTF8_BOM => state.borrow_mut().set_encoding(Encoding::Utf8Bom),

        action::SHORTCUTS => show_info("Keyboard shortcuts", SHORTCUTS),
        action::ABOUT => show_info(
            "About BachelorPad+",
            &format!(
                "BachelorPad+ {}\n\nNotepad when you want it. More when you need it.\n\n\
                 Renderer: {}\nLicence: MIT OR Apache-2.0",
                env!("CARGO_PKG_VERSION"),
                std::env::var("SLINT_BACKEND").unwrap_or_else(|_| "software".to_owned()),
            ),
        ),

        // Only reachable with the custom surface. Under `TextInput`
        // Slint handles these on the widget itself and they never get
        // this far.
        id if (action::UNDO..=action::SELECT_ALL).contains(&id) => {
            let command = match id {
                action::UNDO => bp_editor::Command::Undo,
                action::REDO => bp_editor::Command::Redo,
                action::CUT => bp_editor::Command::Cut,
                action::COPY => bp_editor::Command::Copy,
                action::PASTE => bp_editor::Command::Paste,
                _ => bp_editor::Command::SelectAll,
            };
            apply_editor_command(&mut state.borrow_mut(), &command);
            push = PushText::No;
        }

        // Bounded above by `clip_end()` -- an unbounded `>=` here is exactly
        // what let the recent-files arm swallow everything up to 100 before
        // it had a real range, and the paste-transformation ids just below
        // would have fallen into this arm the same way.
        id if (action::CLIP_BASE..menus::clip_end()).contains(&id) => {
            let index = usize::try_from(id - action::CLIP_BASE).unwrap_or(0);
            let text = state.borrow().clips.get(index).map(|e| e.text.clone())?;
            set_os_clipboard(&text);

            let owns_caret = state.borrow().editor_view;
            if owns_caret {
                {
                    let mut s = state.borrow_mut();
                    if let Some(editor) = s.active_editor_mut() {
                        editor.insert(&text);
                    }
                    s.mark_edited();
                }
                refresh(ui, &mut state.borrow_mut(), PushText::No);
                ui.invoke_focus_editor();
            } else {
                // The OS clipboard now holds the entry, so the
                // widget's own paste puts it at the caret -- the one
                // way to insert there without caret access.
                ui.invoke_paste_from_clipboard();
            }
            return None;
        }

        // A paste transformation. Same two-path shape as the plain
        // clipboard rows just above -- the caret is reached differently
        // under each editor view, and missing one path is how a feature
        // works for whoever tested it and does nothing for whoever did not.
        id if (action::CLIP_TRANSFORM_BASE..menus::clip_transform_end()).contains(&id) => {
            let (entry_index, transform) = {
                let s = state.borrow();
                menus::decode_transform(id, s.clips.entries())?
            };
            let source = state
                .borrow()
                .clips
                .get(entry_index)
                .map(|e| e.text.clone())?;
            let text = menus::apply_transform(transform, &source)?;
            set_os_clipboard(&text);

            let owns_caret = state.borrow().editor_view;
            if owns_caret {
                {
                    let mut s = state.borrow_mut();
                    if let Some(editor) = s.active_editor_mut() {
                        editor.insert(&text);
                    }
                    s.mark_edited();
                }
                refresh(ui, &mut state.borrow_mut(), PushText::No);
                ui.invoke_focus_editor();
            } else {
                ui.invoke_paste_from_clipboard();
            }
            return None;
        }

        id if (action::NOTE_TITLE..=action::NOTE_OUTLINE).contains(&id) => {
            let outcome = state.borrow().note_action(id);
            match outcome {
                NoteOutcome::Show { title, body } => show_info(&title, &body),
                NoteOutcome::SaveAs => {
                    let target = state.borrow().workspace.active_id();
                    if let Some(doc_id) = target {
                        let chosen = pick_save_path(&state.borrow(), doc_id);
                        if let Some(path) = chosen {
                            state.borrow_mut().save_document(doc_id, Some(path));
                        }
                    }
                }
                NoteOutcome::Nothing => {}
            }
        }

        id if (action::DATA_VALIDATE..=action::DATA_COLUMN_TYPES).contains(&id) => {
            state.borrow_mut().run_data_action(id);
        }

        id if (action::LINES_SORT_ASC..=action::LINES_TRIM).contains(&id) => {
            state.borrow_mut().run_line_action(id);
        }

        // Duplicate Line, Move Line Up/Down. A separate range from the line
        // operations above: those replace the whole document, these need
        // the caret, and `run_line_edit` is where that distinction lives.
        id if (action::DUPLICATE_LINE..=action::MOVE_LINE_DOWN).contains(&id) => {
            state.borrow_mut().run_line_edit(id);
        }

        // Recently opened files. Bounded by the length of the list
        // rather than by the next block of ids: this arm used to
        // claim everything up to 100, which was safe only because
        // every other arm in that window happened to come first.
        id if (action::RECENT_BASE
            ..action::RECENT_BASE + i32::try_from(bp_config::MAX_RECENT).unwrap_or(0))
            .contains(&id) =>
        {
            let index = usize::try_from(id - action::RECENT_BASE).unwrap_or(0);
            let path = state.borrow().recent_path(index);
            if let Some(path) = path {
                state.borrow_mut().open(path);
            }
        }

        // action::NONE and anything unrecognised: a row that exists
        // to describe what is coming.
        _ => return None,
    }

    refresh(ui, &mut state.borrow_mut(), push);
    None
}
