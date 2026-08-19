//! The BachelorPad+ application shell.
//!
//! This crate owns the window and nothing else. Document state lives in
//! `bp-core`, saving in `bp-files`, naming in `bp-naming`, colours in
//! `bp-theme`. The rule from `docs/architecture/ARCHITECTURE.md` runs one
//! way: the UI may call those, and none of them know this crate exists.
//!
//! Text is held in `bp-editor`'s rope -- `HashMap<DocumentId, Editor>` --
//! whichever view is drawing (ADR-0018). It was a `HashMap<DocumentId,
//! String>` once, and why that changed is worth keeping: Slint's `TextInput`
//! owns its own text and caret, hands the whole buffer back on every edit,
//! keeps its own undo stack, and exposes the caret only through a property
//! marked "internal, undocumented, only exposed for tests". The rope became
//! the storage independently of which view draws, which is what let the two
//! be separated at all.
//!
//! So there are two views over one model. Under `TextInput`, still the
//! default, text arriving from the widget lands in the rope as a single
//! undoable replacement rather than as keystrokes, undo is Slint's, and the
//! status bar can offer only a line count. Under `--editor-view` this crate
//! owns the caret, `bp-editor`'s transactions are the undo, and the status
//! bar shows Ln/Col; `editor_view.rs` owns that path.
//!
//! Both costs are measured. The `TextInput` path copies the document on
//! every keystroke, so it stays inside a frame budget only to roughly 40 MB;
//! the rope path does not care how large the document is. See
//! `--latency-probe`, ADR-0018 and `docs/architecture/ARCHITECTURE.md`.

// `deny` rather than `forbid`: Slint's build script generates the window's
// item tree into this crate, and that generated code needs `unsafe` with a
// local `allow`. `forbid` cannot be overridden even by generated code, so it
// fails the build. `deny` still stops hand-written `unsafe` here.
#![deny(unsafe_code)]

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use bp_core::{Document, DocumentId};
use bp_theme::{Palette as ThemePalette, ThemeId};

slint::include_modules!();

mod menus;

mod dispatch;
mod editor_view;
mod passphrase;
mod state;

/// Crate identity used by workspace smoke tests and diagnostics.
pub const CRATE_NAME: &str = "bp-ui";

#[derive(Debug, thiserror::Error)]
pub enum UiError {
    #[error("could not start the user interface: {0}")]
    Platform(#[from] slint::PlatformError),
}

fn apply_theme(ui: &AppWindow, theme: ThemeId) {
    let p: ThemePalette = theme.palette();
    let c = |v: bp_theme::Rgb| slint::Color::from_rgb_u8(v.r, v.g, v.b);
    let palette = ui.global::<Palette>();
    palette.set_shell(c(p.shell));
    palette.set_panel(c(p.panel));
    palette.set_edge(c(p.edge));
    palette.set_ink(c(p.ink));
    palette.set_ink_dim(c(p.ink_dim));
    palette.set_accent(c(p.accent));
}

fn refresh(ui: &AppWindow, state: &mut state::AppState, push_text: state::PushText) {
    apply_theme(ui, state.theme);
    ui.set_theme_name(state.theme.name().into());

    let active = state.workspace.active_id();
    let tabs: Vec<TabInfo> = state
        .workspace
        .iter()
        .map(|doc| TabInfo {
            id: i32::try_from(doc.id().get()).unwrap_or(i32::MAX),
            label: doc.display_name().into(),
            dirty: doc.is_dirty(),
            active: Some(doc.id()) == active,
        })
        .collect();
    ui.set_tabs(Rc::new(slint::VecModel::from(tabs)).into());

    if push_text == state::PushText::Yes {
        ui.set_doc_text(state.active_text().as_str().into());
    }
    if state.editor_view {
        editor_view::push_editor_view(ui, state);
    }
    if state.sync_gutter() {
        ui.set_gutter(state.gutter.as_str().into());
    }
    let cursor = state.cursor_label();
    let format = state.format();

    if let Some(doc) = state.workspace.active() {
        ui.set_save_state(state::save_state_label(doc).into());
        ui.set_is_dirty(doc.is_dirty());
        ui.set_location(doc.location_label().into());
        ui.set_encoding_label(doc.encoding().label().into());
        ui.set_line_ending_label(doc.line_ending().label().into());
        ui.set_format_label(format.label().into());
        ui.set_cursor_label(cursor.as_str().into());
    }

    // Something that just failed outranks a standing warning about the file.
    let notice = state
        .error
        .as_deref()
        .or(state.disk_warning.as_deref())
        .unwrap_or_default();
    ui.set_error_message(notice.into());

    // Read on every refresh rather than once, because the desktop's setting
    // can change while the window is open -- which is the whole point of
    // following it.
    state.system_dark = ui.get_system_known().then(|| ui.get_system_dark());
    if state.follow_system_theme
        && let Some(theme) = ThemeId::for_system(state.system_dark)
    {
        state.theme = theme;
        apply_theme(ui, state.theme);
        ui.set_theme_name(state.theme.name().into());
    }

    // Empty under Standard: a permanent "Standard" would be noise on every
    // document, and the readout earns its place only when something unusual
    // is in force.
    let security = state.security();
    // Privacy Mode outranks the profile name here: it is the session-wide
    // fact, and it is the thing somebody switches on precisely because they
    // want to be able to see that it is on.
    ui.set_security_profile(
        if state.privacy.is_on() {
            "Privacy Mode".to_owned()
        } else if security == bp_security::Security::default() {
            String::new()
        } else {
            security.name().to_owned()
        }
        .into(),
    );

    // What the passphrase bar is asking, if anything.
    if let Some(ask) = &state.ask {
        ui.set_passphrase_prompt(ask.prompt().into());
        ui.set_passphrase_action(ask.action().into());
    }
    ui.set_passphrase_status(state.passphrase_status.as_str().into());

    ui.set_show_gutter(state.show_gutter);
    ui.set_wrap_text(state.wrap_text);
    // Points to Slint's `length`. Both editor views read this one property,
    // so a zoom cannot apply to the widget and not to the surface.
    ui.set_font_size(f32::from(state.font_size));

    // Only the menus whose contents depend on state are rebuilt here; the
    // rest are set once at startup.
    let any_dirty = state.workspace.dirty().next().is_some();
    let has_path = state.workspace.active().and_then(Document::path).is_some();
    let model = |items: Vec<MenuItem>| slint::ModelRc::new(slint::VecModel::from(items));

    ui.set_file_items(model(menus::file(
        any_dirty,
        has_path,
        state.recent.paths(),
    )));
    ui.set_view_items(model(menus::view(
        state.theme,
        state.follow_system_theme,
        state.show_gutter,
        state.wrap_text,
        state.font_size,
    )));
    if let Some(doc) = state.workspace.active() {
        ui.set_format_items(model(menus::format(
            doc.encoding(),
            doc.line_ending(),
            state.indent,
        )));
    }
    // Rebuilt rather than set once, because the previews are rendered from
    // the clock: a menu built at startup would still be offering this
    // morning's time this afternoon.
    ui.set_insert_items(model(menus::insert(state::now(), state.editor_view)));
    ui.set_data_items(model(menus::data(format)));
    ui.set_note_items(model(menus::note(state.active_has_content())));
    ui.set_edit_items(model(menus::edit(state.clips.entries(), state.editor_view)));
    // Rebuilt rather than set once: it shows the *active* document's profile
    // and what that profile permits, both of which change with the tab.
    let encrypted = state
        .workspace
        .active_id()
        .is_some_and(|id| state.is_encrypted(id));
    ui.set_security_items(model(menus::security(
        state.security(),
        encrypted,
        state.privacy,
        // Redaction needs something to redact. A prefix check rather than a
        // scan: this runs on every refresh, and scanning the whole document
        // there is the trap R011 spends a paragraph on.
        state.active_has_content(),
    )));
}

/// Menus whose contents never change. Set once, not on every refresh.
fn set_static_menus(ui: &AppWindow) {
    let model = |items: Vec<MenuItem>| slint::ModelRc::new(slint::VecModel::from(items));
    ui.set_help_items(model(menus::help()));
    ui.set_notebook_items(model(menus::planned_menu("Notebook")));
    ui.set_organize_items(model(menus::planned_menu("Organize")));
    ui.set_research_items(model(menus::planned_menu("Research")));
    ui.set_run_items(model(menus::planned_menu("Run")));
    ui.set_tools_items(model(menus::planned_menu("Tools")));
}

/// The query the find bar currently describes.
///
/// Every search path reads its options through here, so that a toggle cannot
/// apply to Find and quietly not to Replace All -- which would be a silent
/// wrong answer rather than a visible bug. The decision it wraps lives in
/// `state::find_query`, where it can be tested without a window.
fn ui_query(ui: &AppWindow) -> bp_search::Query {
    state::find_query(
        &ui.get_find_query(),
        ui.get_find_case_sensitive(),
        ui.get_find_whole_word(),
        ui.get_find_regex(),
    )
}

/// Open a file, starting in a sensible directory.
pub(crate) fn pick_file(state: &state::AppState) -> Option<PathBuf> {
    dispatch::pick_file(state)
}

/// Choose a save location, starting in a sensible directory with the
/// ADR-0003 filename suggested.
pub(crate) fn pick_save_path(state: &state::AppState, id: DocumentId) -> Option<PathBuf> {
    dispatch::pick_save_path(state, id)
}

/// Whatever the OS clipboard holds, if it holds text.
/// Read the OS clipboard, if it holds text.
///
/// Failures are silent: a clipboard held open by another process is normal
/// and momentary, and there is nothing useful to say about it.
pub(crate) fn read_os_clipboard() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}

/// Put text on the OS clipboard. Returns whether it worked.
pub(crate) fn set_os_clipboard(text: &str) -> bool {
    arboard::Clipboard::new().is_ok_and(|mut c| c.set_text(text.to_owned()).is_ok())
}

/// Which Slint renderer to ask for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Renderer {
    /// CPU rasterisation. The default, per ADR-0017: on this workload it is
    /// the only configuration that meets the specs.md section 22 budgets --
    /// 36 ms to window and 19 MB idle, against 129 ms and 63 MB for the GPU
    /// path, which also spends ~410 ms reaching its first frame.
    #[default]
    Software,
    /// Whatever Slint would pick on its own, normally GPU-accelerated.
    Platform,
}

/// How to run the shell.
#[derive(Debug, Clone, Default)]
pub struct RunOptions {
    pub renderer: Renderer,
    /// Starting theme. `None` uses the built-in default.
    pub theme: Option<ThemeId>,
    /// Shown in the status bar at startup, where configuration problems go.
    /// A warning the user cannot see is a warning they will hit again.
    pub startup_notice: Option<String>,
    /// Files named on the command line, opened at startup (specs.md §19).
    pub files: Vec<PathBuf>,
    /// Print `BPSPIKE_READY_MS=<f64>` once the first frame has been rendered,
    /// then quit. Drives `scripts/Measure-UiSpike.ps1`.
    ///
    /// The instrumentation lives in the real application rather than in a
    /// separate benchmark binary on purpose: a fixture measures the fixture,
    /// and drifts from the product the moment either changes.
    pub measure_exit: bool,
    /// Draw with the custom editor surface instead of Slint's `TextInput`.
    ///
    /// Opt-in while it reaches parity: word wrap and input-method
    /// composition still belong to the widget it replaces. The rope is the
    /// storage either way -- only the drawing and the caret differ -- so this
    /// is a choice of view, not of document model.
    pub editor_view: bool,
    /// Editor font size in points, already through `bp-config`'s bounds.
    ///
    /// `None` keeps `bp_config::DEFAULT_FONT_SIZE`, so a caller that does not
    /// care -- `run()`, and every test -- gets the size the editor has always
    /// drawn at.
    pub font_size: Option<u8>,
}

/// Run the BachelorPad+ shell.
pub fn run() -> Result<(), UiError> {
    run_with(RunOptions::default())
}

/// Run the shell with explicit options.
pub fn run_with(options: RunOptions) -> Result<(), UiError> {
    let started = std::time::Instant::now();

    // An explicit SLINT_BACKEND wins: someone debugging a rendering problem
    // has set it deliberately, and silently overriding them would waste an
    // afternoon. Selection must happen before the first window is created.
    if options.renderer == Renderer::Software && std::env::var_os("SLINT_BACKEND").is_none() {
        // A failure here is not fatal -- the platform default still draws a
        // usable window, just a heavier one.
        if let Err(e) = slint::BackendSelector::new()
            .renderer_name("software".to_owned())
            .select()
        {
            tracing::warn!("software renderer unavailable, using platform default: {e}");
        }
    }

    let ui = AppWindow::new()?;
    let mut initial = state::AppState::new();
    if let Some(theme) = options.theme {
        initial.theme = theme;
    }
    if let Some(size) = options.font_size {
        // Clamped again rather than trusted. `bp-config` bounds what it
        // parses, but `RunOptions` is a public API and a caller reaching it
        // by another route must not be able to hand the editor a 0 pt font.
        initial.font_size = bp_config::zoom(size, 0);
    }
    initial.error = options.startup_notice.clone();

    // Files from the command line. The first one replaces the empty document
    // opened at startup, so `bachelorpad note.txt` shows one tab rather than
    // an untouched Untitled beside it.
    if !options.files.is_empty() {
        let blank = initial.workspace.active_id();
        for path in &options.files {
            initial.open(path.clone());
        }
        if let Some(blank) = blank
            && initial.workspace.len() > 1
        {
            initial.close(blank);
        }
    }

    // Offer to recover anything a previous session left unsaved, before the
    // window appears — so nobody starts typing over work they have not been
    // told about.
    initial.journal.clean_temporaries();
    let pending = initial.journal.pending();
    if !pending.is_empty() {
        let names: Vec<&str> = pending
            .iter()
            .map(|(_, c)| c.name.as_str())
            .take(5)
            .collect();
        let answer = rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Warning)
            .set_title("Unsaved work recovered")
            .set_description(format!(
                "BachelorPad+ closed with {} unsaved document(s):\n\n{}\n\nRestore them?",
                pending.len(),
                names.join("\n")
            ))
            .set_buttons(rfd::MessageButtons::YesNo)
            .show();

        if answer == rfd::MessageDialogResult::Yes {
            initial.restore(pending);
        } else {
            // Declining is a decision; honour it rather than asking again
            // every launch.
            let _ = initial.journal.discard_all();
        }
    }

    initial.editor_view = options.editor_view;
    let state = Rc::new(RefCell::new(initial));
    ui.set_use_editor_view(options.editor_view);

    let mut reported = false;
    let measure_exit = options.measure_exit;
    let hooked = ui
        .window()
        .set_rendering_notifier(move |render_state, _api| {
            if reported || !matches!(render_state, slint::RenderingState::AfterRendering) {
                return;
            }
            reported = true;
            println!(
                "BPSPIKE_READY_MS={:.3}",
                started.elapsed().as_secs_f64() * 1000.0
            );
            if measure_exit {
                // Quitting inside a render callback would re-enter the
                // renderer; defer it to the next event-loop turn.
                let _ = slint::invoke_from_event_loop(|| {
                    let _ = slint::quit_event_loop();
                });
            }
        })
        .is_ok();

    // Slint's software renderer has no rendering notifier. Say so rather than
    // reporting a fast-looking number that is really a time-to-exit.
    if !hooked && measure_exit {
        println!("BPSPIKE_READY_UNAVAILABLE=this renderer has no rendering notifier");
        return Ok(());
    }

    macro_rules! wire {
        ($setter:ident, |$s:ident| $body:block) => {{
            let cell = Rc::clone(&state);
            let weak = ui.as_weak();
            ui.$setter(move || {
                {
                    let mut $s = cell.borrow_mut();
                    $body
                }
                if let Some(ui) = weak.upgrade() {
                    refresh(&ui, &mut cell.borrow_mut(), state::PushText::Yes);
                }
            });
        }};
    }

    wire!(on_new_document, |s| {
        s.new_document();
    });

    wire!(on_open_document, |s| {
        if let Some(path) = pick_file(&s) {
            s.open(path);
        }
    });

    wire!(on_save_document, |s| {
        if let Some(id) = s.workspace.active_id() {
            save_with_prompt(&mut s, id);
        }
    });

    wire!(on_save_as_document, |s| {
        if let Some(id) = s.workspace.active_id()
            && let Some(path) = pick_save_path(&s, id)
        {
            s.save_document(id, Some(path));
        }
    });

    wire!(on_cycle_theme, |s| {
        s.theme = s.theme.next();
    });

    {
        let cell = Rc::clone(&state);
        let weak = ui.as_weak();
        let target = Rc::clone(&state);
        let target_weak = ui.as_weak();
        ui.on_tab_context_target(move |raw| {
            let Some(ui) = target_weak.upgrade() else {
                return;
            };
            let mut s = target.borrow_mut();
            s.tab_context = state::find_id(&s.workspace, raw);
            // Built now rather than on every refresh: it is the only menu
            // whose contents depend on which tab the pointer is over, and
            // that is knowable only at this moment.
            let (others, has_path) = s.tab_context_shape();
            ui.set_tab_items(slint::ModelRc::new(slint::VecModel::from(
                menus::tab_context(others, has_path),
            )));
        });

        ui.on_select_tab(move |raw| {
            {
                let mut s = cell.borrow_mut();
                if let Some(id) = state::find_id(&s.workspace, raw) {
                    s.workspace.set_active(id);
                }
            }
            if let Some(ui) = weak.upgrade() {
                refresh(&ui, &mut cell.borrow_mut(), state::PushText::Yes);
            }
        });
    }

    {
        let cell = Rc::clone(&state);
        let weak = ui.as_weak();
        ui.on_close_tab(move |raw| {
            let id = cell
                .borrow()
                .workspace
                .iter()
                .map(Document::id)
                .find(|id| i32::try_from(id.get()).unwrap_or(i32::MAX) == raw);
            if let Some(id) = id {
                close_with_prompt(&cell, id);
            }
            if let Some(ui) = weak.upgrade() {
                refresh(&ui, &mut cell.borrow_mut(), state::PushText::Yes);
            }
        });
    }

    {
        let cell = Rc::clone(&state);
        let weak = ui.as_weak();
        ui.on_close_active_tab(move || {
            let id = cell.borrow().workspace.active_id();
            if let Some(id) = id {
                close_with_prompt(&cell, id);
            }
            if let Some(ui) = weak.upgrade() {
                refresh(&ui, &mut cell.borrow_mut(), state::PushText::Yes);
            }
        });
    }

    {
        let cell = Rc::clone(&state);
        let weak = ui.as_weak();
        ui.on_text_edited(move |text| {
            {
                cell.borrow_mut().edit(text.to_string());
            }
            if let Some(ui) = weak.upgrade() {
                // PushText::No -- the widget already holds this text. Pushing
                // it back would clone the document on every keystroke.
                refresh(&ui, &mut cell.borrow_mut(), state::PushText::No);
            }
        });
    }

    // Closing the window must not be a quieter way to lose the same work
    // that closing a tab prompts about.
    {
        let cell = Rc::clone(&state);
        ui.window().on_close_requested(move || {
            let dirty: Vec<DocumentId> =
                cell.borrow().workspace.dirty().map(Document::id).collect();

            for id in dirty {
                let name = cell.borrow().display_name(id);
                match dispatch::ask_about_unsaved(&name) {
                    rfd::MessageDialogResult::Yes => {
                        let mut s = cell.borrow_mut();
                        if save_with_prompt(&mut s, id) != state::SaveResult::Saved {
                            // Save failed or was abandoned. Quitting now would
                            // discard exactly what the user asked to keep.
                            return slint::CloseRequestResponse::KeepWindowShown;
                        }
                    }
                    rfd::MessageDialogResult::No => {}
                    _ => return slint::CloseRequestResponse::KeepWindowShown,
                }
            }
            slint::CloseRequestResponse::HideWindow
        });
    }

    // Menu dispatch. Ids of 100 and above never arrive here -- Slint handles
    // those on the TextInput itself.
    {
        let cell = Rc::clone(&state);
        let weak = ui.as_weak();
        ui.on_menu_action(move |id| {
            let Some(ui) = weak.upgrade() else { return };
            let _ = dispatch::handle_menu_action(&ui, &cell, id);
        });
    }

    // --- the custom editor surface -------------------------------------
    {
        let cell = Rc::clone(&state);
        let weak = ui.as_weak();
        ui.on_editor_key(move |text, control, shift, alt| {
            let Some(ui) = weak.upgrade() else {
                return false;
            };
            let modifiers = bp_editor::Modifiers {
                control,
                shift,
                alt,
            };

            let handled = {
                let mut s = cell.borrow_mut();
                let rows = s.visible_rows;

                // A backend can deliver more than one character at once --
                // an input method committing, most often. Insert the lot
                // rather than all but the first.
                let command = if text.chars().count() > 1 && !modifiers.control {
                    bp_editor::Command::Insert(text.to_string())
                } else {
                    match editor_view::translate_key(&text) {
                        Some(key) => bp_editor::keys::command_for(key, modifiers, rows),
                        None => bp_editor::Command::Ignore,
                    }
                };
                editor_view::apply_editor_command(&mut s, &command)
            };

            if handled {
                refresh(&ui, &mut cell.borrow_mut(), state::PushText::No);
            }
            handled
        });
    }

    {
        let cell = Rc::clone(&state);
        let weak = ui.as_weak();
        ui.on_editor_pressed(move |row, column, extend| {
            let Some(ui) = weak.upgrade() else { return };
            editor_view::place_caret(&mut cell.borrow_mut(), row, column, extend);
            refresh(&ui, &mut cell.borrow_mut(), state::PushText::No);
        });
    }

    // Double- and triple-click. `EditorSurface` only; `TextInput` does both
    // natively and never reports a press to us, so wiring this for that path
    // would be wiring it twice and getting two behaviours.
    for what in [
        editor_view::ClickSelection::Word,
        editor_view::ClickSelection::Line,
    ] {
        let cell = Rc::clone(&state);
        let weak = ui.as_weak();
        let select = move |row, column| {
            let Some(ui) = weak.upgrade() else { return };
            editor_view::select_at_cell(&mut cell.borrow_mut(), row, column, what);
            refresh(&ui, &mut cell.borrow_mut(), state::PushText::No);
        };
        match what {
            editor_view::ClickSelection::Word => ui.on_editor_selected_word(select),
            editor_view::ClickSelection::Line => ui.on_editor_selected_line(select),
        }
    }

    {
        let cell = Rc::clone(&state);
        let weak = ui.as_weak();
        ui.on_editor_dragged(move |row, column| {
            let Some(ui) = weak.upgrade() else { return };
            // A drag always extends -- that is what dragging means.
            editor_view::place_caret(&mut cell.borrow_mut(), row, column, true);
            refresh(&ui, &mut cell.borrow_mut(), state::PushText::No);
        });
    }

    {
        let cell = Rc::clone(&state);
        let weak = ui.as_weak();
        ui.on_editor_scrolled(move |lines| {
            let Some(ui) = weak.upgrade() else { return };
            let mut s = cell.borrow_mut();

            // By visual rows, not document lines. With wrapping on the two
            // differ, and a wheel that moved whole lines would skip past
            // everything the reader can see inside a long one.
            //
            // `step_row` is bounded at both ends of the document, so the view
            // cannot scroll into empty space.
            s.sync_wrap();
            let anchor = s.anchor;
            let layout = s.layout();
            if let Some(editor) = s.active_editor() {
                s.anchor = bp_editor::view::step_row(
                    editor.buffer(),
                    anchor,
                    isize::try_from(lines).unwrap_or(0),
                    layout,
                );
            }

            // Drawn where it now is rather than through `refresh`: scrolling
            // away from the caret is exactly what the user asked for, and
            // revealing it again would snap the wheel straight back.
            editor_view::draw_editor_view(&ui, &s);
        });
    }

    {
        let cell = Rc::clone(&state);
        let weak = ui.as_weak();
        ui.on_editor_resized(move |rows, columns| {
            let Some(ui) = weak.upgrade() else { return };
            {
                let mut s = cell.borrow_mut();
                s.visible_rows = usize::try_from(rows).unwrap_or(1).max(1);
                // The column count comes from Slint because Slint owns the
                // font. A width derived from anything but the advance it
                // actually draws with would wrap in the wrong place, and
                // visibly so on every line.
                s.wrap_columns = usize::try_from(columns).unwrap_or(80).max(1);
            }
            refresh(&ui, &mut cell.borrow_mut(), state::PushText::No);
        });
    }

    // --- find and replace ---------------------------------------------
    {
        let cell = Rc::clone(&state);
        let weak = ui.as_weak();
        ui.on_find_changed(move || {
            let Some(ui) = weak.upgrade() else { return };
            let query = ui_query(&ui);
            let selection = cell.borrow_mut().find(&query);
            ui.set_find_status(cell.borrow().find_status.as_str().into());
            if let Some(range) = selection {
                dispatch::select(&ui, &mut cell.borrow_mut(), &range);
            }
        });
    }

    for forward in [true, false] {
        let cell = Rc::clone(&state);
        let weak = ui.as_weak();
        let step = move || {
            let Some(ui) = weak.upgrade() else { return };
            let selection = cell.borrow_mut().step_match(forward);
            ui.set_find_status(cell.borrow().find_status.as_str().into());
            if let Some(range) = selection {
                dispatch::select(&ui, &mut cell.borrow_mut(), &range);
            }
        };
        if forward {
            ui.on_find_next(step);
        } else {
            ui.on_find_previous(step);
        }
    }

    {
        let cell = Rc::clone(&state);
        let weak = ui.as_weak();
        ui.on_replace_all(move || {
            let Some(ui) = weak.upgrade() else { return };
            let query = ui_query(&ui);
            let replacement = ui.get_replace_query().to_string();

            // Planned first, and the plan shown, before anything changes:
            // specs.md section 6 wants the changes visible before they are
            // applied. The borrow ends before the dialog blocks -- a native
            // dialog can pump events, and holding a `RefCell` across one is
            // how a re-entrant callback panics.
            let planned = {
                let s = cell.borrow();
                bp_search::plan_replace_all(&s.active_text(), &query, &replacement)
            };

            match planned {
                Err(e) => cell.borrow_mut().find_status = e.to_string(),
                Ok(plan) if plan.is_empty() => {
                    cell.borrow_mut().find_status = "no matches".to_owned();
                }
                Ok(plan) if !dispatch::confirm_replace(&plan) => {
                    cell.borrow_mut().find_status = "cancelled".to_owned();
                }
                Ok(plan) => {
                    let n = plan.count();
                    let mut s = cell.borrow_mut();
                    // An ordinary edit, so it is undoable and nothing reaches
                    // disk until the user saves.
                    s.edit(plan.apply());
                    s.matches.clear();
                    s.find_status = format!("replaced {n}");
                }
            }
            ui.set_find_status(cell.borrow().find_status.as_str().into());
            refresh(&ui, &mut cell.borrow_mut(), state::PushText::Yes);
        });
    }

    {
        let cell = Rc::clone(&state);
        let weak = ui.as_weak();
        ui.on_goto_submitted(move || {
            let Some(ui) = weak.upgrade() else { return };
            let moved = cell.borrow_mut().go_to_line(&ui.get_goto_line());
            ui.set_goto_status(cell.borrow().goto_status.as_str().into());
            if let Some(range) = moved {
                // The bar stays open: going to a line is often the first of
                // several, and closing it would make the second one two
                // keystrokes further away.
                dispatch::select(&ui, &mut cell.borrow_mut(), &range);
            }
        });
    }

    {
        let cell = Rc::clone(&state);
        let weak = ui.as_weak();
        ui.on_passphrase_submitted(move |entered| {
            let Some(ui) = weak.upgrade() else { return };
            let again = cell.borrow_mut().answer_passphrase(&entered);
            // Wiped whatever happened. A passphrase left in the widget
            // outlives the question it answered, and the next prompt would
            // start pre-filled with the last one.
            ui.invoke_clear_passphrase();
            ui.set_passphrase_open(again);
            if again {
                ui.invoke_focus_passphrase();
            } else {
                ui.invoke_focus_editor();
            }
            refresh(&ui, &mut cell.borrow_mut(), state::PushText::Yes);
        });
    }

    {
        let cell = Rc::clone(&state);
        let weak = ui.as_weak();
        ui.on_passphrase_cancelled(move || {
            let Some(ui) = weak.upgrade() else { return };
            {
                let mut s = cell.borrow_mut();
                // Dropping the ask drops any half-entered passphrase with it
                // -- the first of two entries travels inside `Ask::Confirm`
                // precisely so it cannot outlive the question.
                s.ask = None;
                s.passphrase_status.clear();
            }
            ui.invoke_clear_passphrase();
            ui.set_passphrase_open(false);
            ui.invoke_focus_editor();
            refresh(&ui, &mut cell.borrow_mut(), state::PushText::No);
        });
    }

    // --- cross-file search ---------------------------------------------
    {
        let cell = Rc::clone(&state);
        let weak = ui.as_weak();
        ui.on_search_folder(move || {
            let Some(ui) = weak.upgrade() else { return };
            let query = ui_query(&ui);
            if query.is_empty() {
                ui.set_results_summary("type something to search for".into());
                ui.set_results_open(true);
                return;
            }

            let start = cell.borrow().dialog_directory();
            let Some(folder) = rfd::FileDialog::new().set_directory(start).pick_folder() else {
                return;
            };

            match bp_search::search_dir(&folder, &query) {
                Ok(report) => {
                    let rows: Vec<SearchHit> = report
                        .hits
                        .iter()
                        .enumerate()
                        .map(|(index, hit)| SearchHit {
                            // Relative to the searched folder: the shared
                            // prefix is the same on every row and identifies
                            // nothing.
                            label: format!(
                                "{}:{}",
                                hit.path
                                    .strip_prefix(&folder)
                                    .unwrap_or(&hit.path)
                                    .display(),
                                hit.line
                            )
                            .into(),
                            detail: hit.preview.as_str().into(),
                            index: i32::try_from(index).unwrap_or(i32::MAX),
                        })
                        .collect();

                    ui.set_search_hits(Rc::new(slint::VecModel::from(rows)).into());
                    ui.set_results_summary(report.summary().as_str().into());
                    cell.borrow_mut().file_hits = report.hits;
                }
                Err(e) => {
                    ui.set_search_hits(Rc::new(slint::VecModel::from(Vec::new())).into());
                    ui.set_results_summary(e.to_string().as_str().into());
                    cell.borrow_mut().file_hits.clear();
                }
            }
            ui.set_results_open(true);
        });
    }

    {
        let cell = Rc::clone(&state);
        let weak = ui.as_weak();
        ui.on_open_hit(move |index| {
            let Some(ui) = weak.upgrade() else { return };
            let hit = usize::try_from(index)
                .ok()
                .and_then(|i| cell.borrow().file_hits.get(i).cloned());
            let Some(hit) = hit else { return };

            cell.borrow_mut().open(hit.path);
            refresh(&ui, &mut cell.borrow_mut(), state::PushText::Yes);
            // Select the match so the editor scrolls to it, rather than
            // opening the file at the top and leaving the user to hunt.
            dispatch::select(&ui, &mut cell.borrow_mut(), &(hit.offset..hit.offset));
        });
    }

    // Autosave to the recovery journal. Separate from the disk watcher
    // because they answer different questions on different clocks: "did
    // someone else change this file" and "is my work safe if the power goes".
    let journal_timer = slint::Timer::default();
    {
        let cell = Rc::clone(&state);
        let weak = ui.as_weak();
        journal_timer.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_secs(5),
            move || {
                cell.borrow_mut().checkpoint_all();
                // The status bar shows the checkpoint time, so it has to
                // repaint -- but a checkpoint is not a save and the save
                // state must not move.
                if let Some(ui) = weak.upgrade() {
                    refresh(&ui, &mut cell.borrow_mut(), state::PushText::No);
                }
            },
        );
    }

    // Capture clipboard changes. Polling, because neither platform offers a
    // portable change notification and a missed clip is a minor loss.
    //
    // History is in memory only: the clipboard carries passwords and tokens
    // constantly, and specs.md section 14 makes persistence opt-in.
    let clipboard_timer = slint::Timer::default();
    {
        let cell = Rc::clone(&state);
        let weak = ui.as_weak();
        clipboard_timer.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_millis(1200),
            move || {
                let Some(text) = read_os_clipboard() else {
                    return;
                };
                // The active document's policy, because you copy out of the
                // document you are looking at and an OS clipboard read says
                // nothing about where the text came from.
                let changed = {
                    let mut s = cell.borrow_mut();
                    let policy = s.policy().clipboard;
                    // Enforce first: a document whose profile forbids a
                    // history must not keep one gathered a moment ago under a
                    // looser profile, and the poll is the soonest reliable
                    // point at which that is noticed.
                    let cleared = s.clips.enforce(policy);
                    let added = s.clips.push(&text, policy);
                    cleared || added
                };
                // Only rebuild the menus when the history actually changed.
                if changed && let Some(ui) = weak.upgrade() {
                    refresh(&ui, &mut cell.borrow_mut(), state::PushText::No);
                }
            },
        );
    }

    // Watch the file on disk. Polling two numbers every couple of seconds is
    // cheap, needs no dependency and no background thread pushing events into
    // this loop, and behaves identically on both platforms. Must outlive the
    // event loop, hence the binding.
    let disk_timer = slint::Timer::default();
    {
        let cell = Rc::clone(&state);
        let weak = ui.as_weak();
        disk_timer.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_secs(2),
            move || {
                let changed = {
                    let mut s = cell.borrow_mut();
                    let before = s.disk_warning.clone();
                    s.poll_disk();
                    before != s.disk_warning
                };
                // Only touch the UI when the answer actually changed; a
                // two-second repaint of an unchanged status bar is waste.
                if changed && let Some(ui) = weak.upgrade() {
                    refresh(&ui, &mut cell.borrow_mut(), state::PushText::No);
                }
            },
        );
    }

    set_static_menus(&ui);
    refresh(&ui, &mut state.borrow_mut(), state::PushText::Yes);
    ui.run()?;
    drop(disk_timer);
    Ok(())
}

/// Measure the cost of one keystroke through the application's state path,
/// at a range of document sizes.
///
/// This is the half of typing latency we control: taking the edited text from
/// the widget, storing it, marking the document modified, and refreshing the
/// gutter. It deliberately does **not** include rasterisation or presentation,
/// which need a capture rig to measure and are the renderer's contribution.
///
/// The two tables are the two views. The first is the `TextInput` path, and
/// it is faithful in one uncomfortable way: Slint's `edited` callback hands
/// us the entire buffer as a fresh string, so a keystroke copies the
/// document. Those numbers should grow linearly with document size, and if
/// they do, that is the finding, not a flaw in the probe. The second is the
/// `--editor-view` path, where the rope is written at the caret and size
/// stops mattering -- which is the case ADR-0018 was decided on.
pub fn latency_probe() {
    const SAMPLES: usize = 300;
    const SIZES: [usize; 4] = [1_000, 10_000, 100_000, 1_000_000];

    println!("typing-path latency (state update only, excludes rendering)");
    println!(
        "{:>12}  {:>10}  {:>10}  {:>10}",
        "doc chars", "p50", "p95", "max"
    );

    for size in SIZES {
        // A realistic-ish document: 80-column lines.
        let mut text: String = (0..size)
            .map(|i| if i % 80 == 79 { '\n' } else { 'x' })
            .collect();

        let mut state = state::AppState::new();
        state.edit(text.clone());
        state.sync_gutter();

        let mut timings = Vec::with_capacity(SAMPLES);
        for _ in 0..SAMPLES {
            text.push('y');
            let next = text.clone();
            let start = std::time::Instant::now();
            state.edit(next);
            state.sync_gutter();
            timings.push(start.elapsed());
        }

        timings.sort_unstable();
        let at = |q: f64| timings[((timings.len() as f64 * q) as usize).min(timings.len() - 1)];
        println!(
            "{size:>12}  {:>8.1}µs  {:>8.1}µs  {:>8.1}µs",
            at(0.50).as_secs_f64() * 1e6,
            at(0.95).as_secs_f64() * 1e6,
            timings[timings.len() - 1].as_secs_f64() * 1e6,
        );
    }

    println!();
    println!("rope insert at the caret (what --editor-view costs)");
    println!(
        "{:>12}  {:>10}  {:>10}  {:>10}",
        "doc chars", "p50", "p95", "max"
    );

    for size in SIZES {
        let text: String = (0..size)
            .map(|i| if i % 80 == 79 { '\n' } else { 'x' })
            .collect();
        let mut buffer = bp_buffer::Buffer::from_text(&text);
        let mut timings = Vec::with_capacity(SAMPLES);

        for i in 0..SAMPLES {
            // Insert mid-document, the worst realistic case for a flat string
            // and the case a rope exists to make cheap.
            let at = buffer.len_chars() / 2 + i;
            let start = std::time::Instant::now();
            buffer.insert(at, "y");
            timings.push(start.elapsed());
        }

        timings.sort_unstable();
        let at = |q: f64| timings[((timings.len() as f64 * q) as usize).min(timings.len() - 1)];
        println!(
            "{size:>12}  {:>8.1}µs  {:>8.1}µs  {:>8.1}µs",
            at(0.50).as_secs_f64() * 1e6,
            at(0.95).as_secs_f64() * 1e6,
            timings[timings.len() - 1].as_secs_f64() * 1e6,
        );
    }

    println!();
    println!("Perceptible-latency threshold is roughly 16ms (16000µs) per frame.");
    println!("The first table grows with document size because Slint hands back the");
    println!("whole buffer on every edit -- that is the default view. The second is");
    println!("the same keystroke under --editor-view, which writes the rope directly.");
}

/// Save, escalating to Save As when the document has no path yet, and asking
/// first if the file changed underneath us.
///
/// Save on a never-saved document must not silently do nothing, and Save on a
/// file somebody else edited must not silently discard their work.
fn save_with_prompt(state: &mut state::AppState, id: DocumentId) -> state::SaveResult {
    if state.would_overwrite_external_change(id) {
        let name = state.display_name(id);
        let answer = rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Warning)
            .set_title("Changed on disk")
            .set_description(format!(
                "{name} has changed on disk since you opened it.\n\n\
                 Saving will overwrite those changes. Save anyway?"
            ))
            .set_buttons(rfd::MessageButtons::YesNo)
            .show();
        if answer != rfd::MessageDialogResult::Yes {
            // Not a failure -- a refusal. Nothing was written either way, so
            // the caller must not treat it as saved.
            return state::SaveResult::NeedsPath;
        }
    }

    match state.save_document(id, None) {
        state::SaveResult::NeedsPath => {
            match dispatch::pick_save_path(state, id) {
                Some(path) => state.save_document(id, Some(path)),
                // The user dismissed the dialog. Nothing was written, and the
                // caller must not treat that as saved.
                None => state::SaveResult::NeedsPath,
            }
        }
        other => other,
    }
}

/// Close a tab, asking first if it holds unsaved work.
///
/// Borrows are scoped tightly around each step: the dialogs block, and
/// holding a `RefCell` borrow across one would panic the moment any other
/// callback ran.
fn close_with_prompt(cell: &Rc<RefCell<state::AppState>>, id: DocumentId) {
    let (dirty, name) = {
        let s = cell.borrow();
        (s.is_dirty(id), s.display_name(id))
    };

    if dirty {
        match dispatch::ask_about_unsaved(&name) {
            rfd::MessageDialogResult::Yes => {
                let mut s = cell.borrow_mut();
                if save_with_prompt(&mut s, id) != state::SaveResult::Saved {
                    // Keep the tab open rather than discard unsaved work.
                    return;
                }
            }
            rfd::MessageDialogResult::No => {}
            // Cancel, or the dialog was dismissed. Do nothing.
            _ => return,
        }
    }
    cell.borrow_mut().close(id);
}
