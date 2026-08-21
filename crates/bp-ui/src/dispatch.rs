//! Menu action dispatch and shared UI helpers.
//!
//! Owns the menu-action match and the small UI helpers (dialogs, selection)
//! that the action arms and find/replace share.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use bp_core::{Document, DocumentId, Encoding, LineEnding};
use bp_platform::editor::{Consent, InstallRefusal, RegistrationPlan};
use bp_theme::ThemeId;

use crate::menus::{self, action};
use crate::state::{AppState, NoteOutcome, PushText, SaveResult, secret_scan_report};
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

/// A document's filename, for a row whose detail column already shows the
/// full path -- the same reasoning `state::organize`'s own `filename_of`
/// applies to a status-bar notice, applied here to a panel row's label.
fn filename(path: &str) -> &str {
    std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
}

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

/// Give a chosen path the `.bpadx` extension.
///
/// The file dialog suggests the document's current name, and a user who
/// accepts it would otherwise get an encrypted file called `notes.txt` --
/// which the shell would happily reopen, but which every other program on the
/// machine would treat as text and show as binary noise.
fn with_bpadx_extension(path: PathBuf) -> PathBuf {
    if path.extension().is_some_and(|e| e == "bpadx") {
        path
    } else {
        let mut name = path.file_name().unwrap_or_default().to_os_string();
        name.push(".bpadx");
        path.with_file_name(name)
    }
}

/// Choose the public key a signature is claimed to have been made with.
fn pick_verifying_key(state: &AppState) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Choose the public key to verify against")
        .set_directory(state.dialog_directory())
        .add_filter("Public key", &["pub", "key", "txt"])
        .add_filter("Any file", &["*"])
        .pick_file()
}

/// Choose where to put the registry script Windows registration needs.
///
/// The file name comes from `bp-platform`'s own artefact, so what the user
/// saves is what the plan names. Where it goes is entirely their choice --
/// ADR-0012's whole position is that the user reads this file and applies it
/// themselves, and a location chosen for them is one step back towards an
/// installer writing the same keys invisibly.
fn pick_registry_script(state: &AppState, plan: &RegistrationPlan) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Save the registry script")
        .set_directory(state.dialog_directory())
        .set_file_name(crate::default_editor::registry_script_name(plan))
        .add_filter("Registry script", &["reg"])
        .save_file()
}

/// Show what registration would do, and ask before doing any of it.
///
/// The body is built in `default_editor`, which puts the current
/// associations above the plan: ADR-0012 is about the user keeping control,
/// and a user cannot keep control of a change they were not shown the
/// starting point of. The answer becomes the `Consent` value
/// `bp_platform::editor::install` refuses to act without.
fn confirm_registration(body: &str) -> Consent {
    if rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Warning)
        .set_title("Set as default editor")
        .set_description(body)
        .set_buttons(rfd::MessageButtons::OkCancel)
        .show()
        == rfd::MessageDialogResult::Ok
    {
        Consent::Granted
    } else {
        Consent::Withheld
    }
}

/// Show what a redaction is about to destroy, and ask before destroying it.
///
/// The listing is not bounded the way `confirm_replace`'s is. A scan that
/// found four hundred credentials is a document nobody should redact without
/// reading the list, and a truncated list is one nobody read.
fn confirm_redaction(plan: &crate::state::RedactionPlan) -> bool {
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Warning)
        .set_title("Redact")
        .set_description(plan.consent_body())
        .set_buttons(rfd::MessageButtons::OkCancel)
        .show()
        == rfd::MessageDialogResult::Ok
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

/// Preview a match in the editor without moving focus.
///
/// `select` is for a completed "jump to this now" action -- Find Next/
/// Previous, Go to Line, and clicking a cross-file search result all move
/// focus into the editor on purpose, because each of those is a single
/// deliberate jump the user makes once. This function is for the opposite
/// case: the find box fires its `find-changed` callback on every keystroke
/// as the user is still typing a query, and `select`'s focus-stealing would
/// send the very next character typed into the document instead of the
/// box. So this sets the same selection `select` would, but leaves focus
/// wherever it already is.
pub(crate) fn preview_match(ui: &AppWindow, state: &mut AppState, range: &std::ops::Range<usize>) {
    use crate::editor_view::draw_editor_view;

    if state.editor_view {
        if let Some(editor) = state.active_editor_mut() {
            editor.select(range.start, range.end);
        }
        state.reveal_caret();
        draw_editor_view(ui, state);
        return;
    }
    let start = i32::try_from(range.start).unwrap_or(i32::MAX);
    let end = i32::try_from(range.end).unwrap_or(i32::MAX);
    ui.invoke_preview_range(start, end);
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
                // An encrypted file does not become a tab until it is
                // unlocked: a tab nobody can read looks like an empty
                // document, and saving it would write emptiness over the
                // real one.
                if state.borrow_mut().open_maybe_encrypted(path) {
                    ui.invoke_focus_passphrase();
                    return None;
                }
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

        // Picking a theme by name stops following the system. Leaving the
        // flag set would let the next preference change silently undo the
        // choice the user just made.
        action::THEME_LIGHT | action::THEME_DARK | action::THEME_ORGANIC | action::THEME_GREEN => {
            let mut s = state.borrow_mut();
            s.theme = match id {
                action::THEME_LIGHT => ThemeId::Light,
                action::THEME_DARK => ThemeId::Dark,
                action::THEME_ORGANIC => ThemeId::Organic,
                _ => ThemeId::Green,
            };
            s.follow_system_theme = false;
        }
        action::THEME_SYSTEM => {
            let mut s = state.borrow_mut();
            s.follow_system_theme = true;
            match ThemeId::for_system(s.system_dark) {
                Some(theme) => s.theme = theme,
                // Not an error, and not a guess: a desktop that will not say
                // leaves the current theme alone. The row still ticks, so the
                // preference is remembered for when an answer arrives.
                None => {
                    s.error = Some(
                        "this desktop does not report a light or dark preference; \
                                    keeping the current theme"
                            .to_owned(),
                    );
                }
            }
        }

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

        // A copy, deliberately not a save: `save_copy` leaves the path, the
        // dirty flag and the recent-files list exactly where they were.
        action::SAVE_COPY => {
            // Same shape as Save As -- the borrow ends with the statement,
            // before anything takes a mutable one.
            let target = state.borrow().workspace.active_id();
            if let Some(id) = target {
                let chosen = pick_save_path(&state.borrow(), id);
                if let Some(path) = chosen {
                    state.borrow_mut().save_copy(id, &path);
                }
            }
            // The document did not change, so its text must not be re-pushed:
            // under `TextInput` that would throw away the caret for an
            // operation that did not touch the document at all.
            push = PushText::No;
        }

        action::DOCUMENT_STATS => {
            state.borrow_mut().report_statistics();
            push = PushText::No;
        }

        action::TOOLS_INSPECTOR => {
            // The borrow ends with the statement, before the dialog opens:
            // `rfd` pumps events, and a re-entrant callback on a live
            // `borrow_mut()` panics.
            let report = state.borrow().inspector_report();
            if let Some(report) = report {
                show_info("Document Inspector", &report);
            }
            push = PushText::No;
        }

        // Tab context menu. All three act on `tab_context` -- the tab that was
        // right-clicked -- falling back to the active one, so a row can never
        // act on a tab the user was not pointing at.
        action::CLOSE_OTHER_TABS | action::CLOSE_ALL_TABS => {
            let keep = (id == action::CLOSE_OTHER_TABS)
                .then(|| state.borrow().tab_context_id())
                .flatten();
            let doomed: Vec<DocumentId> = state
                .borrow()
                .workspace
                .iter()
                .map(Document::id)
                .filter(|id| Some(*id) != keep)
                .collect();
            // `close_with_prompt` for each, so unsaved work still asks --
            // closing several tabs is exactly when losing one would hurt.
            for id in doomed {
                close_with_prompt(state, id);
            }
            refresh(ui, &mut state.borrow_mut(), PushText::Yes);
            return None;
        }
        action::COPY_TAB_PATH => {
            let path = {
                let s = state.borrow();
                s.tab_context_id()
                    .and_then(|id| s.workspace.get(id))
                    .and_then(Document::path)
                    .map(|p| p.display().to_string())
            };
            let mut s = state.borrow_mut();
            s.error = match path {
                Some(path) if crate::set_os_clipboard(&path) => Some(format!("copied {path}")),
                Some(_) => Some("could not reach the clipboard".to_owned()),
                None => Some("this document has never been saved".to_owned()),
            };
            push = PushText::No;
        }

        id if (action::PROFILE_BASE..menus::profile_end()).contains(&id) => {
            let index = usize::try_from(id - action::PROFILE_BASE).unwrap_or(0);
            if let Some(profile) = bp_security::Profile::all().get(index).copied() {
                state
                    .borrow_mut()
                    .set_security(bp_security::Security::Named(profile));
            }
            push = PushText::No;
        }

        action::RUN_STOP => {
            state.borrow_mut().stop_run();
            push = PushText::No;
        }

        id if (action::RUN_CELL_BASE..crate::menus::run_cell_end()).contains(&id) => {
            // **The one place in this product where a run is consented to.**
            // specs.md section 15 says never auto-run an opened or pasted
            // notebook, and both crates demand a gesture by value for that
            // reason. This arm is reached only from a click on a Run menu
            // row, which is what `from_user_command` is documented to mean --
            // "somewhere a human's action is on the stack".
            let index = usize::try_from(id - action::RUN_CELL_BASE).unwrap_or(0);
            state.borrow_mut().run_cell(
                index,
                bp_notebook::UserGesture::from_user_command(),
                bp_execution::UserGesture::from_user_command(),
            );
            push = PushText::No;
        }

        action::PRIVACY_MODE => {
            let next = if state.borrow().privacy.is_on() {
                bp_security::Privacy::Off
            } else {
                bp_security::Privacy::On
            };
            state.borrow_mut().set_privacy(next);
            push = PushText::No;
        }

        action::ENCRYPT_DOCUMENT => {
            // Save As, not encrypt-in-place: the plaintext original is left
            // where it was rather than silently destroyed by an operation
            // that is irreversible without the passphrase. The tab then
            // adopts the encrypted file, so later saves stay encrypted.
            let target = state.borrow().workspace.active_id();
            if let Some(id) = target {
                let suggested = pick_save_path(&state.borrow(), id);
                if let Some(path) = suggested {
                    let path = with_bpadx_extension(path);
                    state.borrow_mut().ask = Some(crate::passphrase::Ask::Set { id, target: path });
                    ui.invoke_focus_passphrase();
                }
            }
            return None;
        }

        action::SCAN_SECRETS => {
            let findings = state.borrow_mut().scan_for_secrets();
            // The listing is a dialog rather than more status bar: the bar
            // elides, and the positions are the part worth reading. Nothing
            // here is the matched text -- a `Finding` does not carry it, and
            // reaching back into the document to quote it would undo the one
            // decision the crate is built around.
            if !findings.is_empty() {
                show_info("Possible credentials", &secret_scan_report(&findings));
            }
            push = PushText::No;
        }

        action::HASH_DOCUMENT => {
            // The borrow ends with the statement, before the dialog opens:
            // `rfd` pumps events, and a re-entrant callback on a live borrow
            // panics.
            let report = state.borrow_mut().hash_active_document();
            if let Some(report) = report {
                show_info("Document hash", &report);
            }
            push = PushText::No;
        }

        action::SIGN_DOCUMENT => {
            push = PushText::No;
            // Asks the right question and stops. Whether the passphrase bar
            // says "new signing key passphrase" or "signing key passphrase"
            // depends on whether a key exists, and `begin_signing` is what
            // decides -- the whole flow lives there and in
            // `answer_passphrase`, so a passphrase is typed in exactly one
            // place in this product.
            if state.borrow_mut().begin_signing() {
                ui.invoke_focus_passphrase();
            }
        }

        action::VERIFY_SIGNATURE => {
            push = PushText::No;
            // The first pass asks the user nothing, because a key cannot
            // change its answer: whether a sidecar is there at all, whether
            // it reads as one, and whether it holds against the key it itself
            // names are all settled before anybody is asked for a file. A
            // missing one fails here, closed, and stops.
            let holds = state
                .borrow_mut()
                .verify_signature(&bp_integrity::Expectation::AnySigner);

            // Only now is a key worth asking for. It is the one thing that
            // tells "signed by who you expected" from "signed by somebody
            // else" -- the verdict a bare 64-byte `.sig` cannot produce, and
            // the reason ADR-0026's sidecar records a key at all. Cancelling
            // leaves the first pass's answer standing, caveat and all, which
            // is why that caveat is written.
            if holds && let Some(path) = pick_verifying_key(&state.borrow()) {
                let expect = state.borrow_mut().expected_signer(&path);
                if let Some(expect) = expect {
                    state.borrow_mut().verify_signature(&expect);
                }
            }
        }

        action::SET_DEFAULT_EDITOR => {
            push = PushText::No;
            // Nothing is borrowed while a dialog is up: `rfd` pumps events,
            // and a re-entrant callback on a live `borrow_mut()` panics.
            match crate::default_editor::offer() {
                Err(reason) => state.borrow_mut().error = Some(reason),
                Ok(offer) => {
                    let consent = confirm_registration(&offer.body);
                    let message = match (consent, offer.root.as_deref()) {
                        (Consent::Withheld, _) => InstallRefusal::ConsentWithheld.describe(),
                        // The only call in this application that writes into
                        // a directory the desktop environment reads. It takes
                        // the root `bp-platform` named and the consent the
                        // user just gave, and neither has a default.
                        (Consent::Granted, Some(root)) => {
                            let (status, body) = crate::default_editor::install_outcome(
                                bp_platform::editor::install(&offer.plan, root, consent),
                                &offer.plan.handoff,
                                root,
                            );
                            if let Some(body) = body {
                                show_info("Set as default editor", &body);
                            }
                            status
                        }
                        // No install root means this platform has no
                        // supported install -- Windows. The `.reg` script is
                        // what ADR-0012 leaves in its place: a file the user
                        // chose the location of and can read before running.
                        (Consent::Granted, None) => {
                            let chosen = pick_registry_script(&state.borrow(), &offer.plan);
                            match chosen {
                                Some(path) => {
                                    match crate::default_editor::save_registry_script(
                                        &offer.plan,
                                        &path,
                                    ) {
                                        Ok(status) => {
                                            let note = crate::default_editor::handoff_note(
                                                &offer.plan.handoff,
                                            );
                                            show_info(
                                                "Set as default editor",
                                                &format!("{status}\n\n{note}"),
                                            );
                                            status
                                        }
                                        Err(refusal) => refusal,
                                    }
                                }
                                None => "nothing was written — no location was chosen for the \
                                         registry script"
                                    .to_owned(),
                            }
                        }
                    };
                    state.borrow_mut().error = Some(message);
                }
            }
        }

        // A second, wholly independent instance -- its own `AppState`, its
        // own window, none of this process's arguments carried over, so it
        // opens exactly as launching the app fresh would. Nothing about
        // *this* window's state changes, so there is nothing to push back
        // into the widget; the fall-through refresh below still runs, which
        // is what shows the error message if the spawn failed.
        action::NEW_WINDOW => {
            push = PushText::No;
            let spawned = crate::state::new_window_command()
                .and_then(|mut command| command.spawn().map(|_child| ()));
            if let Err(e) = spawned {
                state.borrow_mut().error = Some(format!("could not open a new window -- {e}"));
            }
        }

        action::REDACT_SECRETS => {
            // Three statements, each ending its borrow before the next, and
            // the dialog in between opened while nothing is borrowed at all:
            // `rfd` pumps events, and a re-entrant callback on a live
            // `borrow_mut()` panics.
            //
            // The consent is not ceremony. Redaction destroys text the user
            // wrote, in places on the screen they cannot all see at once, and
            // it is offered because a scanner *guessed* the text was a
            // credential. Replace All is confirmed for the weaker version of
            // the same reason.
            let plan = state.borrow_mut().plan_redaction();
            push = PushText::No;
            if let Some(plan) = plan {
                if confirm_redaction(&plan) {
                    // The document changed, so its text has to be re-pushed
                    // -- and only then, because re-pushing throws away
                    // `TextInput`'s caret.
                    if state.borrow_mut().apply_redaction(&plan) {
                        push = PushText::Yes;
                    }
                } else {
                    state.borrow_mut().decline_redaction(&plan);
                }
            }
        }

        action::SECURITY_HISTORY => {
            // Same shape as Inspect Metadata: the borrow ends with the
            // statement, before the dialog opens. `rfd` pumps events, and a
            // re-entrant callback on a live `borrow_mut()` panics.
            let report = state.borrow_mut().security_history();
            if let Some(report) = report {
                show_info("Security History", &report);
            }
            push = PushText::No;
        }

        action::INSPECT_METADATA => {
            // Same shape as Hash Document: the borrow ends with the
            // statement, before the dialog opens.
            let report = state.borrow_mut().inspect_metadata();
            if let Some(report) = report {
                show_info("Metadata", &report);
            }
            push = PushText::No;
        }

        action::GO_TO_LINE => {
            // The bar owns the interaction from here; opening it is all the
            // menu row does, which is why one action id covers the feature.
            ui.invoke_focus_goto();
            return None;
        }

        // Indentation. The width applies to both modes -- it is how far a
        // literal tab reaches as well as how far a soft one goes.
        action::INDENT_TABS | action::INDENT_SPACES => {
            let mut s = state.borrow_mut();
            s.indent.spaces = id == action::INDENT_SPACES;
            push = PushText::No;
        }
        action::TAB_WIDTH_2 | action::TAB_WIDTH_4 | action::TAB_WIDTH_8 => {
            let mut s = state.borrow_mut();
            s.indent.width = match id {
                action::TAB_WIDTH_2 => 2,
                action::TAB_WIDTH_8 => 8,
                _ => 4,
            };
            push = PushText::No;
        }

        id if (action::STAMP_BASE..menus::stamp_end()).contains(&id) => {
            let index = usize::try_from(id - action::STAMP_BASE).unwrap_or(0);
            let mut s = state.borrow_mut();
            if !s.insert_stamp(index) {
                // The rows are disabled without the custom editor view, so
                // this is only reachable by a keyboard route that does not
                // exist yet -- but a silent no-op would be a bug report
                // nobody could describe.
                s.error = Some("date and time insertion needs --editor-view".to_owned());
            }
            // `PushText::Yes`, the default: the document changed, same as
            // Duplicate Line beside it.
        }

        id if (action::INSERT_BOLD..=action::INSERT_TABLE).contains(&id) => {
            // The text each construct inserts, and how far back from the end
            // of it the caret should land -- 0 for every construct but the
            // code block, which leaves it between the fences rather than
            // after the closing one.
            let (text, step_back): (&str, usize) = match id {
                action::INSERT_BOLD => ("**text**", 0),
                action::INSERT_ITALIC => ("*text*", 0),
                action::INSERT_LINK => ("[text](url)", 0),
                action::INSERT_CODE_BLOCK => ("```\n```", 3),
                _ => ("| Header | Header |\n| --- | --- |\n| Cell | Cell |", 0),
            };
            let mut s = state.borrow_mut();
            if !s.insert_markdown(text, step_back) {
                // Same shape as the date/time stamps above: the rows are
                // disabled without the custom editor view, so this is only
                // reachable by a route that does not exist yet -- but a
                // silent no-op would still be a bug report nobody could
                // describe.
                s.error = Some("markdown insertion needs --editor-view".to_owned());
            }
            // `PushText::Yes`, the default: the document changed.
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
        // What the application thinks its environment is, not a file
        // browser: no document content, no passphrase, no listing of what is
        // in the directories it names -- only where it resolved them to.
        action::DIAGNOSTICS => {
            show_info("Diagnostics", &crate::state::diagnostics_report());
            push = PushText::No;
        }

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

        // Organize ▸ Related Notes (ADR-0037). Same shape as cross-file
        // search's `on_search_folder`: the query runs once, here, rather
        // than being re-run on every `refresh` -- `refresh` only converts
        // whatever `related_notes` already holds into rows if it ever needs
        // to, the way `note_items` and the rest of the state-dependent menus
        // do, but nothing here forces it to re-query the store on every
        // keystroke the way a field read in `refresh` would.
        action::ORGANIZE_RELATED_NOTES => {
            push = PushText::No;
            let related = state.borrow().related_notes_for_active();
            let rows: Vec<crate::SearchHit> = related
                .iter()
                .enumerate()
                .map(|(index, record)| crate::SearchHit {
                    label: record
                        .title
                        .clone()
                        .unwrap_or_else(|| filename(&record.path).to_owned())
                        .into(),
                    detail: record.path.as_str().into(),
                    index: i32::try_from(index).unwrap_or(i32::MAX),
                })
                .collect();
            let summary = if rows.is_empty() {
                "no related notes found".to_owned()
            } else {
                format!(
                    "{} related note{}",
                    rows.len(),
                    if rows.len() == 1 { "" } else { "s" }
                )
            };
            state.borrow_mut().related_notes = related;
            ui.set_organize_hits(Rc::new(slint::VecModel::from(rows)).into());
            ui.set_organize_summary(summary.into());
            ui.set_organize_open(true);
        }

        // The on-demand half of Duplicate Detection. `duplicate_detection_report`
        // is the same check the automatic save-time notice runs -- see
        // `AppState::record_for_organize` -- so a click here and a save can
        // never disagree about what counts as a duplicate.
        action::ORGANIZE_DUPLICATE_DETECTION => {
            let report = state.borrow().duplicate_detection_report();
            push = PushText::No;
            show_info("Duplicate Detection", &report);
        }

        // Research ▸ Research Report (ADR-0041). The borrow ends with the
        // statement, before the dialog opens -- the same reason
        // `TOOLS_INSPECTOR`'s own arm gives: `rfd` pumps events, and a
        // re-entrant callback on a live `borrow_mut()` panics.
        action::RESEARCH_REPORT => {
            let report = state.borrow().research_report();
            push = PushText::No;
            show_info("Research Report", &report);
        }

        // Notebook ▸ Cell Outline (ADR-0045). Toggles, like every other
        // bottom panel: the row that opened it closes it again.
        action::CELL_OUTLINE => {
            let mut s = state.borrow_mut();
            s.outline_open = !s.outline_open;
            push = PushText::No;
        }

        // Research ▸ what the active document cites (ADR-0044). Each borrow
        // ends with its statement, before the dialog opens, for the reason
        // `RESEARCH_REPORT` gives just above.
        action::CITATION_METADATA => {
            let report = state.borrow().citation_metadata_report();
            push = PushText::No;
            show_info("Citation Metadata", &report);
        }
        action::FIND_IDENTIFIERS => {
            let report = state.borrow().identifiers_report();
            push = PushText::No;
            show_info("Identifiers", &report);
        }
        action::CHECK_BIBLIOGRAPHY => {
            let report = state.borrow().bibliography_report();
            push = PushText::No;
            show_info("Bibliography", &report);
        }

        // Two ranges rather than one, because the Data block at 70-79 had a
        // single id left when YAML needed two. Both reach the same place; the
        // arm that decides which library function a click meant is
        // `run_data_action`, where the format is in scope.
        id if (action::DATA_VALIDATE..=action::DATA_COLUMN_TYPES).contains(&id)
            || (action::DATA_YAML_TO_JSON..=action::DATA_JSON_TO_YAML).contains(&id) =>
        {
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
