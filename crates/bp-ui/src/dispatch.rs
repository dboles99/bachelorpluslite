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
use slint::ComponentHandle;

use crate::AppWindow;
use crate::dialog::{Dialogs, Question};
use crate::menus::{self, action};
use crate::state::{AppState, NoteOutcome, PushText};

const SHORTCUTS: &str = "\
Ctrl+N          New
Ctrl+O          Open
Ctrl+S          Save
Ctrl+Shift+S    Save As
Ctrl+W          Close tab
Ctrl+F          Find and replace
Ctrl+G          Go to line
Ctrl+Z / Ctrl+Y Undo / Redo
Ctrl+X/C/V      Cut / Copy / Paste
Ctrl+A          Select all
Insert          Overtype on / off
Ctrl+Insert     Copy
Shift+Insert    Paste
Ctrl+= / Ctrl+- Zoom in / out
Ctrl+0          Reset zoom
Ctrl+D          Duplicate line
Alt+Up / Down   Move line up / down
F10             Menus: arrows to move, Enter to run, Esc to close";

/// The About box, as text, so the claim in it has a test behind it.
///
/// **The licence is read from `CARGO_PKG_LICENSE`, never written out.** It
/// was written out until ADR-0071, while `bp_config::cli::Package`'s
/// `license` field claimed three crates away that the workspace manifest was
/// "the one home for this string". Both sentences were in the repository at
/// once and neither could fail -- trap 4 -- so what actually caught it was a
/// relicence needing to know where the string lived.
///
/// A function rather than a `const` because the renderer is read from the
/// environment at click time, and separate from the match arm because a body
/// built inside `handle_menu_action` is one no test can reach.
fn about_text() -> String {
    format!(
        "{} {}\n\nNotepad when you want it. More when you need it.\n\n\
         Renderer: {}\nLicence: {}\n\n\
         Free software, and you may redistribute and modify it under the\n\
         GNU General Public License version 3. It comes with NO WARRANTY.\n\
         The full text is in LICENSE, beside this program.",
        bp_platform::DISPLAY_NAME,
        env!("CARGO_PKG_VERSION"),
        std::env::var("SLINT_BACKEND").unwrap_or_else(|_| "software".to_owned()),
        env!("CARGO_PKG_LICENSE"),
    )
}

/// Where an issue is filed, derived from the manifest rather than written
/// out.
///
/// `CARGO_PKG_REPOSITORY` is the workspace manifest's `repository` field, so
/// a fork that changes it gets its own issues URL for free and this cannot
/// point at somebody else's tracker. The same reasoning as the licence and
/// the product name (ADR-0071, ADR-0074): the value has one home and this
/// reads it.
fn issues_url() -> String {
    format!("{}/issues/new/choose", env!("CARGO_PKG_REPOSITORY"))
}

/// A bug report, pre-filled, ready for the user to add what they did.
///
/// **The diagnostics are already in it, and that is the whole point**
/// (ADR-0077). `.github/ISSUE_TEMPLATE/bug_report.yml` asks for the version,
/// the renderer, every resolved directory and which editor surface was in
/// use, because reports without them cannot be acted on -- and somebody
/// filing in a browser has to come back here, find Help > Diagnostics and
/// copy it. Most will not. So the constraint that this product cannot open
/// a browser produces the *better* report rather than a worse one.
///
/// Pure, and takes the diagnostics as a parameter, so a test can read it
/// without a window or a real profile directory.
fn problem_report(diagnostics: &str) -> String {
    format!(
        "# Problem report\n\n\
         Fill in the three sections below, then paste the whole thing at:\n\
         {}\n\n\
         Nothing has been sent. This is a document; it goes nowhere until you\n\
         put it there yourself.\n\n\
         ## What I did\n\n\n\
         ## What I expected\n\n\n\
         ## What happened instead\n\n\n\
         ## Diagnostics\n\n\
         ```\n{}\n```\n\n\
         ## Before you send it\n\n\
         - An issue is public. Check the paths above are ones you are happy to\n\
         share, and edit them if not.\n\
         - If this is a security problem, do not open a public issue. See\n\
         SECURITY.md in the repository.\n",
        issues_url(),
        diagnostics.trim_end(),
    )
}

/// Where the shipped help is, or `None` if this build cannot work out where
/// it is running from.
///
/// The path rule is `bp_platform::help`'s and the *question* -- where is this
/// executable -- is the shell's, which is the split R011 asks for: `bp-ui`
/// connects, and does not decide.
///
/// `current_exe` can fail, and the caller says so plainly rather than opening
/// nothing. It is also the case that matters most in practice: somebody who
/// moved `bachelorpad.exe` out of the unpacked folder has left the help
/// behind, exactly as they would have left the icon behind (ADR-0068).
fn user_guide_path() -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;
    let path = bp_platform::help::index_beside(bp_platform::Platform::HOST, executable.to_str()?)?;
    let path = PathBuf::from(path);
    path.is_file().then_some(path)
}

/// A document's filename, for a row whose detail column already shows the
/// full path -- the same reasoning `state::organize`'s own `filename_of`
/// applies to a status-bar notice, applied here to a panel row's label.
fn filename(path: &str) -> &str {
    std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
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

/// Carry out a registration the user has just answered, and say what
/// happened.
///
/// Runs from the answer to the consent question, which is drawn in the window
/// and so arrives after `handle_menu_action` has returned (ADR-0084).
fn register(
    state: &Rc<RefCell<AppState>>,
    dialogs: &Dialogs,
    offer: &crate::default_editor::Offer,
    consent: Consent,
) -> String {
    match (consent, offer.root.as_deref()) {
        (Consent::Withheld, _) => InstallRefusal::ConsentWithheld.describe(),
        // The only call in this application that writes into a directory the
        // desktop environment reads. It takes the root `bp-platform` named and
        // the consent the user just gave, and neither has a default.
        (Consent::Granted, Some(root)) => {
            let (status, body) = crate::default_editor::install_outcome(
                bp_platform::editor::install(&offer.plan, root, consent),
                &offer.plan.handoff,
                root,
            );
            if let Some(body) = body {
                dialogs.inform("Set as default editor", &body);
            }
            status
        }
        // No install root means this platform has no supported install --
        // Windows. The `.reg` script is what ADR-0012 leaves in its place: a
        // file the user chose the location of and can read before running.
        (Consent::Granted, None) => {
            let chosen = pick_registry_script(&state.borrow(), &offer.plan);
            match chosen {
                Some(path) => {
                    match crate::default_editor::save_registry_script(&offer.plan, &path) {
                        Ok(status) => {
                            let note = crate::default_editor::handoff_note(&offer.plan.handoff);
                            dialogs.inform("Set as default editor", &format!("{status}\n\n{note}"));
                            status
                        }
                        Err(refusal) => refusal,
                    }
                }
                None => "nothing was written — no location was chosen for the registry script"
                    .to_owned(),
            }
        }
    }
}

/// Select a character range in the editor **and take the caret with it**.
///
/// The range is in characters, like everything this workspace hands around;
/// it is converted to the bytes `TextInput` counts here and nowhere else.
///
/// [`reveal`] does the selecting; this adds the focus, and the focus is the
/// whole difference between them. Use it when the action came from somewhere
/// that is not a text box -- a results panel, a list of related notes -- so
/// there is no box the caret can be stolen from.
///
/// **Do not use it for anything submitted from a bar.** Find Next and Go to
/// Line both leave their bar open on purpose, and moving the caret into the
/// document while a box is still on screen means the user's next keystroke
/// edits the document. That destroyed text for as long as this function was
/// used for all five callers; see [`reveal`].
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
    // Bytes, not characters: see `AppState::widget_range`.
    let (start, end) = state.widget_range(range);
    ui.invoke_select_range(start, end);
}

/// Select a character range and scroll it into view, **leaving the caret
/// wherever it already is**.
///
/// This is the one to reach for by default, and [`select`] is the exception.
///
/// It used to be called `preview_match` and to exist only for the find box's
/// keystroke-by-keystroke preview, on the premise -- written into `select` --
/// that Find Next, Go to Line and a cross-file result are each "a single
/// deliberate jump the user makes once" and may therefore take the caret.
/// **Two of the three are repeated, from a bar that stays open on purpose.**
/// Go to Line's call site says so in as many words, three lines from a call
/// whose comment said the opposite, and neither had been asked which was
/// right.
///
/// What that cost: pressing Enter twice in the find box replaced the match
/// with a line break and went on inserting one per press, with nothing to
/// announce it but the dirty dot and a line count going up. Found by driving
/// the window over a 601-line fixture -- a document that fits on one screen
/// cannot show it, because the damage is off-screen by the time it happens.
///
/// Focus is not lost, only deferred: closing either bar calls
/// `focus-editor()`, so the caret returns when the user is finished with the
/// box rather than while they are still typing into it.
pub(crate) fn reveal(ui: &AppWindow, state: &mut AppState, range: &std::ops::Range<usize>) {
    use crate::editor_view::draw_editor_view;

    if state.editor_view {
        if let Some(editor) = state.active_editor_mut() {
            editor.select(range.start, range.end);
        }
        state.reveal_caret();
        draw_editor_view(ui, state);
        return;
    }
    // Bytes, not characters: see `AppState::widget_range`.
    let (start, end) = state.widget_range(range);
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
    dialogs: &Rc<Dialogs>,
    id: i32,
) -> Option<PushText> {
    use crate::editor_view::apply_editor_command;
    use crate::{close_with_prompt, refresh, save_all, save_with_prompt};

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
                save_with_prompt(&ui.as_weak(), state, dialogs, id, |_| {});
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
            let dirty = state.borrow().workspace.dirty().map(Document::id).collect();
            save_all(ui.as_weak(), Rc::clone(state), Rc::clone(dialogs), dirty);
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
            // Reloading discards edits, so it asks, and only an explicit
            // yes reloads (ADR-0084).
            if dirty {
                let (weak, cell) = (ui.as_weak(), Rc::clone(state));
                dialogs.ask(Question::reload(&name), move |answer| {
                    if crate::dialog::confirmed(answer) {
                        cell.borrow_mut().reload();
                        if let Some(ui) = weak.upgrade() {
                            refresh(&ui, &mut cell.borrow_mut(), PushText::Yes);
                        }
                    }
                });
                return None;
            }
            state.borrow_mut().reload();
        }
        action::CLOSE_TAB => {
            let id = state.borrow().workspace.active_id();
            if let Some(id) = id {
                close_with_prompt(&ui.as_weak(), state, dialogs, id);
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
            let report = state.borrow().inspector_report();
            if let Some(report) = report {
                dialogs.inform("Document Inspector", &report);
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
                close_with_prompt(&ui.as_weak(), state, dialogs, id);
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

        action::PRIVACY_MODE => {
            let next = if state.borrow().privacy.is_on() {
                bp_security::Privacy::Off
            } else {
                bp_security::Privacy::On
            };
            state.borrow_mut().set_privacy(next);
            push = PushText::No;
        }

        action::SET_DEFAULT_EDITOR => {
            push = PushText::No;
            match crate::default_editor::offer() {
                Err(reason) => state.borrow_mut().error = Some(reason),
                Ok(offer) => {
                    let (weak, cell, d) = (ui.as_weak(), Rc::clone(state), Rc::clone(dialogs));
                    // The answer becomes the `Consent` value
                    // `bp_platform::editor::install` refuses to act without.
                    // Only the Continue button grants it.
                    dialogs.ask(Question::registration(&offer.body), move |answer| {
                        let consent = if crate::dialog::confirmed(answer) {
                            Consent::Granted
                        } else {
                            Consent::Withheld
                        };
                        let message = register(&cell, &d, &offer, consent);
                        cell.borrow_mut().error = Some(message);
                        if let Some(ui) = weak.upgrade() {
                            refresh(&ui, &mut cell.borrow_mut(), PushText::No);
                        }
                    });
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
        action::ENCODING_UTF16_LE => state.borrow_mut().set_encoding(Encoding::Utf16Le),
        id if (action::REOPEN_BASE..menus::reopen_end()).contains(&id) => {
            let encoding = usize::try_from(id - action::REOPEN_BASE)
                .ok()
                .and_then(|index| menus::REOPEN_AS.get(index))
                .and_then(|(_, label)| bp_files::encoding_named(label))?;
            let (dirty, name) = {
                let s = state.borrow();
                let id = s.workspace.active_id();
                (
                    id.is_some_and(|i| s.is_dirty(i)),
                    id.map(|i| s.display_name(i)).unwrap_or_default(),
                )
            };
            // Reopening reads the file again and discards edits, exactly as
            // Reload does, so it asks the same question (ADR-0084).
            if dirty {
                let (weak, cell) = (ui.as_weak(), Rc::clone(state));
                dialogs.ask(Question::reload(&name), move |answer| {
                    if crate::dialog::confirmed(answer) {
                        cell.borrow_mut().reload_as(Some(encoding));
                        if let Some(ui) = weak.upgrade() {
                            refresh(&ui, &mut cell.borrow_mut(), PushText::Yes);
                        }
                    }
                });
                return None;
            }
            state.borrow_mut().reload_as(Some(encoding));
        }

        // Help ▸ User Guide. Opened as a document, which is what makes it
        // possible at all: this product launches nothing, so a browser was
        // never an option, and the editor already knows how to show text
        // (ADR-0075). `bp_platform::help` decides *where* the file is,
        // because joining a path is a platform question -- this arm only
        // asks where the executable is, which is a shell question.
        action::USER_GUIDE => match user_guide_path() {
            Some(path) => state.borrow_mut().open(path),
            None => dialogs.inform(
                "User Guide",
                "The help that ships beside this program could not be found.

\n                 It is `app-help/index.md`, in the folder the executable is in.
\n                 Moving the executable out of the unpacked archive leaves it
\n                 behind -- move the whole folder instead.

\n                 The same pages are at https://bpad.prompt-forge.dev/docs",
            ),
        },
        action::SHORTCUTS => dialogs.inform("Keyboard shortcuts", SHORTCUTS),
        // Help ▸ Report a Problem. Composes a document and copies a URL; it
        // opens nothing and sends nothing (ADR-0077).
        action::REPORT_PROBLEM => {
            // A free function in `state`, not a method: it reads the process
            // environment and the resolved directories rather than the open
            // document, so it needs no `AppState` and takes no borrow. Which
            // means the report is composed *before* any borrow is taken.
            let report = problem_report(&crate::state::diagnostics_report());
            {
                let mut s = state.borrow_mut();
                s.new_document();
                s.edit(report);
            }
            let copied = crate::set_os_clipboard(&issues_url());
            let where_to = if copied {
                "The address is on your clipboard."
            } else {
                "The address is in the document; the clipboard was unavailable."
            };
            dialogs.inform(
                "Report a Problem",
                &format!(
                    "A report has been opened in a new tab, with your diagnostics\n\
                     already filled in.\n\n\
                     Add what you did and what happened, then paste the whole\n\
                     document into a new issue. {}\n\n\
                     Nothing has been sent anywhere.",
                    where_to
                ),
            );
        }
        action::ABOUT => dialogs.inform(
            &format!("About {}", bp_platform::DISPLAY_NAME),
            &about_text(),
        ),
        // Organize ▸ where documents sharing this one's tags already live.
        action::ORGANIZE_SUGGESTED_FOLDER => {
            let report = state.borrow().suggested_folder_report();
            push = PushText::No;
            dialogs.inform("Suggested Folder", &report);
        }

        // Note ▸ the store's view of this document, and the journal's
        // (ADR-0048).
        action::NOTE_TAGS => {
            let report = state.borrow().tags_report();
            push = PushText::No;
            dialogs.inform("Tags", &report);
        }
        action::NOTE_RECOVERY => {
            let report = state.borrow().recovery_report();
            push = PushText::No;
            dialogs.inform("Recovery Checkpoints", &report);
        }

        // Tools ▸ the three readouts ADR-0048 added.
        action::TOOLS_SECURITY_INSPECTOR => {
            let report = state.borrow().security_inspector_report();
            push = PushText::No;
            dialogs.inform("Security Inspector", &report);
        }
        action::TOOLS_FILE_ANALYSIS => {
            let report = state.borrow().file_analysis_report();
            push = PushText::No;
            dialogs.inform("File Analysis", &report);
        }
        action::TOOLS_CONFIGURATION => {
            let report = state.borrow().configuration_report();
            push = PushText::No;
            dialogs.inform("Configuration", &report);
        }

        // What the application thinks its environment is, not a file
        // browser: no document content, no passphrase, no listing of what is
        // in the directories it names -- only where it resolved them to.
        action::DIAGNOSTICS => {
            dialogs.inform("Diagnostics", &crate::state::diagnostics_report());
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

        id if (action::NOTE_TITLE..=action::NOTE_OUTLINE).contains(&id) => {
            let outcome = state.borrow().note_action(id);
            match outcome {
                NoteOutcome::Show { title, body } => dialogs.inform(&title, &body),
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
            dialogs.inform("Duplicate Detection", &report);
        }

        // Research ▸ Research Report (ADR-0041).
        action::RESEARCH_REPORT => {
            let report = state.borrow().research_report();
            push = PushText::No;
            dialogs.inform("Research Report", &report);
        }

        // Research ▸ what the active document asks, and what the store holds
        // (ADR-0046). Same borrow discipline as every arm above.
        action::OPEN_QUESTIONS => {
            let report = state.borrow().open_questions_report();
            push = PushText::No;
            dialogs.inform("Open Questions", &report);
        }
        action::STORE_CONTENTS => {
            let report = state.borrow().store_contents_report();
            push = PushText::No;
            dialogs.inform("What the Store Holds", &report);
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

#[cfg(test)]
mod tests {
    use super::{about_text, issues_url, problem_report};
    use bp_platform::DISPLAY_NAME;

    #[test]
    fn the_about_box_must_report_the_licence_the_manifest_declares() {
        assert!(
            about_text().contains(env!("CARGO_PKG_LICENSE")),
            "About said: {}",
            about_text()
        );
    }

    /// The specific way this went wrong, kept as its own test.
    ///
    /// The previous literal outlived the licence it named, and a test
    /// asserting only that About mentions *some* licence would have passed
    /// over it unchanged -- `CARGO_PKG_LICENSE` was never in the string at
    /// all. So this asserts the shape of the mistake rather than its value.
    #[test]
    fn no_licence_name_may_be_written_into_the_about_box_by_hand() {
        let declared = env!("CARGO_PKG_LICENSE");
        for spelling in ["MIT", "Apache-2.0", "BSD", "MPL"] {
            assert!(
                declared.contains(spelling) || !about_text().contains(spelling),
                "About names {spelling}, which the manifest does not: {}",
                about_text()
            );
        }
    }

    /// One of eight sites that spelled the product name out by hand.
    ///
    /// `DISPLAY_NAME` has said since it was written that the name "must not
    /// be spelled out at the three places that show it", and there were
    /// eight. This is the About body; `menus` asserts the About row and
    /// `state` the diagnostics report, because a claim about eight sites
    /// needs a reader at each of them rather than one that reads the
    /// constant and proves only that the constant exists (ADR-0074).
    #[test]
    fn the_about_box_must_read_the_product_name_rather_than_spell_it() {
        assert!(
            about_text().starts_with(DISPLAY_NAME),
            "About said: {}",
            about_text()
        );
    }

    /// The version has the same one-home rule and had never been asserted.
    #[test]
    fn the_about_box_must_report_the_version_the_manifest_declares() {
        assert!(about_text().contains(env!("CARGO_PKG_VERSION")));
    }

    /// The diagnostics are the reason this row exists, so their presence is
    /// asserted rather than assumed (ADR-0077).
    #[test]
    fn a_problem_report_carries_the_diagnostics_it_was_given() {
        let report = problem_report("BachelorPad+ Lite 0.9.5\n\nRenderer: software");
        assert!(
            report.contains("Renderer: software"),
            "report was: {report}"
        );
        assert!(report.contains("0.9.5"), "report was: {report}");
    }

    /// A report that did not say where to send it would be a document the
    /// user has to go and look something up for, which is the friction the
    /// whole row exists to remove.
    #[test]
    fn a_problem_report_says_where_to_paste_it() {
        let report = problem_report("diagnostics");
        assert!(report.contains(&issues_url()), "report was: {report}");
    }

    /// The issues URL is derived from the manifest, so a fork gets its own
    /// tracker rather than sending its users here.
    #[test]
    fn the_issues_url_is_built_from_the_repository_the_manifest_declares() {
        assert!(issues_url().starts_with(env!("CARGO_PKG_REPOSITORY")));
    }

    /// The one thing this row must never imply.
    #[test]
    fn a_problem_report_must_say_that_nothing_has_been_sent() {
        let report = problem_report("diagnostics");
        assert!(
            report.to_lowercase().contains("nothing has been sent"),
            "report was: {report}"
        );
    }
}
