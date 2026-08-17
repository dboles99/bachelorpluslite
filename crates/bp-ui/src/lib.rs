//! The BachelorPad+ application shell.
//!
//! This crate owns the window and nothing else. Document state lives in
//! `bp-core`, saving in `bp-files`, naming in `bp-naming`, colours in
//! `bp-theme`. The rule from `docs/architecture/ARCHITECTURE.md` runs one
//! way: the UI may call those, and none of them know this crate exists.
//!
//! Text buffers are a `HashMap<DocumentId, String>` here, which is a
//! placeholder. `bp-buffer` (phase 2) replaces it with a rope; until then the
//! gutter is rebuilt on every keystroke, which is fine for notes and wrong
//! for the large files specs.md section 5 promises.

// `deny` rather than `forbid`: Slint's build script generates the window's
// item tree into this crate, and that generated code needs `unsafe` with a
// local `allow`. `forbid` cannot be overridden even by generated code, so it
// fails the build. `deny` still stops hand-written `unsafe` here.
#![deny(unsafe_code)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use bp_core::{Document, DocumentId, Encoding, LineEnding, UNTITLED, Workspace};
use bp_files::{SaveOptions, atomic_write, load};
use bp_naming::SemanticName;
use bp_theme::{Palette as ThemePalette, ThemeId};
use time::OffsetDateTime;

slint::include_modules!();

/// Crate identity used by workspace smoke tests and diagnostics.
pub const CRATE_NAME: &str = "bp-ui";

#[derive(Debug, thiserror::Error)]
pub enum UiError {
    #[error("could not start the user interface: {0}")]
    Platform(#[from] slint::PlatformError),
}

/// Local wall-clock time, falling back to UTC.
///
/// `now_local` can fail on Unix in a multithreaded process. A timestamp in
/// the wrong zone is a small wrong; refusing to record the save at all would
/// be a large one.
fn now() -> OffsetDateTime {
    OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc())
}

/// `8:05 PM`, as specs.md section 3 shows it.
fn clock(t: OffsetDateTime) -> String {
    let (hour, meridiem) = match t.hour() {
        0 => (12, "AM"),
        h @ 1..=11 => (h, "AM"),
        12 => (12, "PM"),
        h => (h - 12, "PM"),
    };
    format!("{hour}:{:02} {meridiem}", t.minute())
}

/// Encode the buffer for disk, honouring the document's encoding and line
/// endings.
///
/// Round-tripping matters: a file opened as CRLF with a BOM must be written
/// back that way, or saving silently rewrites every line of someone's file.
fn encode(text: &str, encoding: Encoding, line_ending: LineEnding) -> Result<Vec<u8>, String> {
    if matches!(encoding, Encoding::Utf16Le | Encoding::Utf16Be) {
        return Err(format!("saving {} is not supported yet", encoding.label()));
    }
    // Normalise to LF first so mixed input converges on one convention.
    let lf = text.replace("\r\n", "\n");
    let body = match line_ending {
        LineEnding::Lf => lf,
        LineEnding::CrLf => lf.replace('\n', "\r\n"),
    };
    let mut out = encoding.bom().to_vec();
    out.extend_from_slice(body.as_bytes());
    Ok(out)
}

/// How a save attempt ended.
///
/// Three cases, not two: a close-on-quit flow must not treat "needs a path"
/// or "the disk refused" as success and then discard the buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SaveResult {
    Saved,
    /// No path yet. The caller should escalate to Save As.
    NeedsPath,
    /// Attempted and refused; `AppState::error` explains why.
    Failed,
}

struct AppState {
    workspace: Workspace,
    texts: HashMap<DocumentId, String>,
    theme: ThemeId,
    error: Option<String>,
    /// Cached gutter text, and the line count it was built for.
    ///
    /// Rebuilding this on every keystroke was the single largest avoidable
    /// cost in the typing path: it allocates proportionally to the document
    /// on each character typed, while the content only changes when a line is
    /// added or removed. `usize::MAX` is a sentinel meaning "never built",
    /// since 0 is a line count `max(1)` can never produce.
    gutter: String,
    gutter_lines: usize,
}

impl AppState {
    fn new() -> Self {
        let mut workspace = Workspace::new();
        let id = workspace.open_new(now());
        let mut texts = HashMap::new();
        texts.insert(id, String::new());
        Self {
            workspace,
            texts,
            theme: ThemeId::default(),
            error: None,
            gutter: String::new(),
            gutter_lines: usize::MAX,
        }
    }

    fn active_text(&self) -> &str {
        self.workspace
            .active_id()
            .and_then(|id| self.texts.get(&id))
            .map_or("", String::as_str)
    }

    fn text_of(&self, id: DocumentId) -> &str {
        self.texts.get(&id).map_or("", String::as_str)
    }

    /// Tab label for `id`, for use in prompts.
    fn display_name(&self, id: DocumentId) -> String {
        self.workspace
            .get(id)
            .map_or_else(|| UNTITLED.to_owned(), |d| d.display_name().to_owned())
    }

    fn is_dirty(&self, id: DocumentId) -> bool {
        self.workspace.get(id).is_some_and(Document::is_dirty)
    }

    /// Rebuild the gutter only when the line count actually changed.
    ///
    /// Returns `true` if the cache changed and the UI needs the new value.
    fn sync_gutter(&mut self) -> bool {
        let lines = self.active_text().lines().count().max(1);
        if self.gutter_lines == lines {
            return false;
        }
        self.gutter_lines = lines;
        self.gutter.clear();
        for n in 1..=lines {
            if n > 1 {
                self.gutter.push('\n');
            }
            let _ = write!(self.gutter, "{n}");
        }
        true
    }

    fn new_document(&mut self) {
        self.error = None;
        let id = self.workspace.open_new(now());
        self.texts.insert(id, String::new());
    }

    fn open(&mut self, path: PathBuf) {
        self.error = None;
        match load(&path) {
            Ok(file) => {
                let id = self.workspace.open_path(path, now());
                if let Some(doc) = self.workspace.get_mut(id) {
                    doc.set_encoding(file.encoding);
                    doc.set_line_ending(file.line_ending);
                }
                self.texts.insert(id, file.text);
            }
            // The error types already render a message naming the file and
            // what to do about it, which is exactly what the status bar wants.
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    /// Outcome of a save attempt, so callers can tell the three cases apart.
    ///
    /// A close-on-quit flow must not treat "needs a path" or "the disk
    /// refused" as success and then discard the buffer.
    fn save_document(&mut self, id: DocumentId, path: Option<PathBuf>) -> SaveResult {
        self.error = None;
        let Some(doc) = self.workspace.get(id) else {
            return SaveResult::Saved;
        };

        let Some(target) = path.or_else(|| doc.path().map(Path::to_path_buf)) else {
            return SaveResult::NeedsPath;
        };

        let bytes = match encode(self.text_of(id), doc.encoding(), doc.line_ending()) {
            Ok(b) => b,
            Err(message) => {
                self.error = Some(message);
                return SaveResult::Failed;
            }
        };

        match atomic_write(&target, &bytes, SaveOptions::default()) {
            Ok(_) => {
                if let Some(doc) = self.workspace.get_mut(id) {
                    doc.set_path(target);
                    // Only now, after a verified write, is the document clean.
                    doc.record_disk_save(now());
                }
                SaveResult::Saved
            }
            Err(e) => {
                self.error = Some(e.to_string());
                SaveResult::Failed
            }
        }
    }

    /// A default filename for Save As, in the grammar from ADR-0003.
    fn suggested_filename(&self, id: DocumentId) -> String {
        self.workspace.get(id).map_or_else(
            || "Untitled.txt".to_owned(),
            |doc| SemanticName::new(doc.title(), doc.created_at().date(), "txt").to_filename(),
        )
    }

    fn close(&mut self, id: DocumentId) {
        self.error = None;
        if self.workspace.close(id).is_some() {
            self.texts.remove(&id);
        }
        // Never leave the user staring at an empty frame with no way back.
        if self.workspace.is_empty() {
            self.new_document();
        }
    }

    fn edit(&mut self, text: String) {
        let Some(id) = self.workspace.active_id() else {
            return;
        };
        if self
            .texts
            .get(&id)
            .is_some_and(|existing| *existing == text)
        {
            return;
        }
        self.texts.insert(id, text);
        if let Some(doc) = self.workspace.get_mut(id) {
            doc.mark_modified();
        }
    }
}

/// Status-bar save state, per specs.md section 3.
fn save_state_label(doc: &Document) -> String {
    if doc.is_dirty() {
        doc.last_disk_save().map_or_else(
            || "● Unsaved".to_owned(),
            |t| format!("● Unsaved │ Last disk save {}", clock(t.get())),
        )
    } else {
        doc.last_disk_save().map_or_else(
            || "● Never saved".to_owned(),
            |t| format!("✓ Saved {}", clock(t.get())),
        )
    }
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

/// Whether the buffer text needs pushing back into the editor widget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PushText {
    /// The document changed underneath the widget: opened, switched, closed.
    Yes,
    /// The user typed. The widget already holds the text, and pushing it back
    /// would clone the whole document on every keystroke to no effect.
    No,
}

fn refresh(ui: &AppWindow, state: &mut AppState, push_text: PushText) {
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

    if push_text == PushText::Yes {
        ui.set_doc_text(state.active_text().into());
    }
    if state.sync_gutter() {
        ui.set_gutter(state.gutter.as_str().into());
    }
    let lines = state.gutter_lines;

    if let Some(doc) = state.workspace.active() {
        ui.set_save_state(save_state_label(doc).into());
        ui.set_is_dirty(doc.is_dirty());
        ui.set_location(doc.location_label().into());
        ui.set_encoding_label(doc.encoding().label().into());
        ui.set_line_ending_label(doc.line_ending().label().into());
        ui.set_format_label("TXT".into());
        ui.set_cursor_label(format!("{lines} lines").into());
    }

    ui.set_error_message(state.error.as_deref().unwrap_or_default().into());
}

/// Ask about unsaved work before discarding it.
///
/// Blocking and native. The three-way answer matters: "Cancel" has to be
/// distinguishable from "Discard", or the safe choice becomes the
/// destructive one.
fn ask_about_unsaved(name: &str) -> rfd::MessageDialogResult {
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Warning)
        .set_title("Unsaved changes")
        .set_description(format!(
            "{name} has unsaved changes.\n\nSave before closing?"
        ))
        .set_buttons(rfd::MessageButtons::YesNoCancel)
        .show()
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
    /// Print `BPSPIKE_READY_MS=<f64>` once the first frame has been rendered,
    /// then quit. Drives `scripts/Measure-UiSpike.ps1`.
    ///
    /// The instrumentation lives in the real application rather than in a
    /// separate benchmark binary on purpose: a fixture measures the fixture,
    /// and drifts from the product the moment either changes.
    pub measure_exit: bool,
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
    let mut initial = AppState::new();
    if let Some(theme) = options.theme {
        initial.theme = theme;
    }
    initial.error = options.startup_notice.clone();
    let state = Rc::new(RefCell::new(initial));

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
                    refresh(&ui, &mut cell.borrow_mut(), PushText::Yes);
                }
            });
        }};
    }

    wire!(on_new_document, |s| {
        s.new_document();
    });

    wire!(on_open_document, |s| {
        if let Some(path) = rfd::FileDialog::new().pick_file() {
            s.open(path);
        }
    });

    wire!(on_save_document, |s| {
        if let Some(id) = s.workspace.active_id() {
            save_with_prompt(&mut s, id);
        }
    });

    wire!(on_save_as_document, |s| {
        if let Some(id) = s.workspace.active_id() {
            let suggested = s.suggested_filename(id);
            if let Some(path) = rfd::FileDialog::new().set_file_name(suggested).save_file() {
                s.save_document(id, Some(path));
            }
        }
    });

    wire!(on_cycle_theme, |s| {
        s.theme = s.theme.next();
    });

    {
        let cell = Rc::clone(&state);
        let weak = ui.as_weak();
        ui.on_select_tab(move |raw| {
            {
                let mut s = cell.borrow_mut();
                if let Some(id) = find_id(&s.workspace, raw) {
                    s.workspace.set_active(id);
                }
            }
            if let Some(ui) = weak.upgrade() {
                refresh(&ui, &mut cell.borrow_mut(), PushText::Yes);
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
                refresh(&ui, &mut cell.borrow_mut(), PushText::Yes);
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
                refresh(&ui, &mut cell.borrow_mut(), PushText::Yes);
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
                refresh(&ui, &mut cell.borrow_mut(), PushText::No);
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
                match ask_about_unsaved(&name) {
                    rfd::MessageDialogResult::Yes => {
                        let mut s = cell.borrow_mut();
                        if save_with_prompt(&mut s, id) != SaveResult::Saved {
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

    refresh(&ui, &mut state.borrow_mut(), PushText::Yes);
    ui.run()?;
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
/// It is faithful in one uncomfortable way: Slint's `edited` callback hands us
/// the entire buffer as a fresh string, so a keystroke copies the document.
/// That is the real cost today and the reason `bp-buffer` exists on the
/// roadmap -- the numbers here should grow linearly with document size, and
/// if they do, that is the finding, not a flaw in the probe.
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

        let mut state = AppState::new();
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
    println!("Perceptible-latency threshold is roughly 16ms (16000µs) per frame.");
}

/// Save, escalating to Save As when the document has no path yet.
///
/// Save on a never-saved document must not silently do nothing.
fn save_with_prompt(state: &mut AppState, id: DocumentId) -> SaveResult {
    match state.save_document(id, None) {
        SaveResult::NeedsPath => {
            let suggested = state.suggested_filename(id);
            match rfd::FileDialog::new().set_file_name(suggested).save_file() {
                Some(path) => state.save_document(id, Some(path)),
                // The user dismissed the dialog. Nothing was written, and the
                // caller must not treat that as saved.
                None => SaveResult::NeedsPath,
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
fn close_with_prompt(cell: &Rc<RefCell<AppState>>, id: DocumentId) {
    let (dirty, name) = {
        let s = cell.borrow();
        (s.is_dirty(id), s.display_name(id))
    };

    if dirty {
        match ask_about_unsaved(&name) {
            rfd::MessageDialogResult::Yes => {
                let mut s = cell.borrow_mut();
                if save_with_prompt(&mut s, id) != SaveResult::Saved {
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

/// Map a tab id from the UI back to a `DocumentId`.
///
/// Returns `None` for ids that are no longer open, which is what makes a
/// stale click harmless rather than a panic.
fn find_id(workspace: &Workspace, raw: i32) -> Option<DocumentId> {
    workspace
        .iter()
        .map(Document::id)
        .find(|id| i32::try_from(id.get()).unwrap_or(i32::MAX) == raw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn clock_uses_twelve_hour_time() {
        assert_eq!(clock(datetime!(2026-08-16 20:05 UTC)), "8:05 PM");
        assert_eq!(clock(datetime!(2026-08-16 08:05 UTC)), "8:05 AM");
        assert_eq!(clock(datetime!(2026-08-16 00:30 UTC)), "12:30 AM");
        assert_eq!(clock(datetime!(2026-08-16 12:00 UTC)), "12:00 PM");
    }

    #[test]
    fn encode_round_trips_line_endings() {
        assert_eq!(
            encode("a\nb", Encoding::Utf8, LineEnding::CrLf).unwrap(),
            b"a\r\nb"
        );
        assert_eq!(
            encode("a\r\nb", Encoding::Utf8, LineEnding::Lf).unwrap(),
            b"a\nb"
        );
    }

    #[test]
    fn encode_does_not_double_convert_existing_crlf() {
        // Normalising to LF first is what stops "a\r\nb" becoming "a\r\r\nb".
        assert_eq!(
            encode("a\r\nb", Encoding::Utf8, LineEnding::CrLf).unwrap(),
            b"a\r\nb"
        );
    }

    #[test]
    fn encode_writes_the_bom_back() {
        assert_eq!(
            encode("hi", Encoding::Utf8Bom, LineEnding::Lf).unwrap(),
            b"\xEF\xBB\xBFhi"
        );
    }

    #[test]
    fn encode_refuses_utf16_rather_than_writing_mojibake() {
        assert!(encode("hi", Encoding::Utf16Le, LineEnding::Lf).is_err());
    }

    #[test]
    fn gutter_matches_line_count() {
        let mut state = AppState::new();
        assert!(state.sync_gutter());
        assert_eq!(state.gutter, "1", "an empty buffer still has line 1");

        state.edit("a\nb\nc".to_owned());
        assert!(state.sync_gutter());
        assert_eq!(state.gutter, "1\n2\n3");
    }

    #[test]
    fn gutter_is_not_rebuilt_when_the_line_count_is_unchanged() {
        // The whole point of the cache: typing within a line must not
        // reallocate a string proportional to the document.
        let mut state = AppState::new();
        state.edit("hello".to_owned());
        assert!(state.sync_gutter());

        state.edit("hello world".to_owned());
        assert!(
            !state.sync_gutter(),
            "same line count must not rebuild the gutter"
        );

        state.edit("hello\nworld".to_owned());
        assert!(state.sync_gutter(), "a new line must rebuild it");
        assert_eq!(state.gutter, "1\n2");
    }

    #[test]
    fn a_new_document_reports_never_saved() {
        let state = AppState::new();
        let doc = state.workspace.active().unwrap();
        assert_eq!(save_state_label(doc), "● Never saved");
    }

    #[test]
    fn an_edited_document_reports_unsaved() {
        let mut state = AppState::new();
        state.edit("typing".to_owned());
        let doc = state.workspace.active().unwrap();
        assert_eq!(save_state_label(doc), "● Unsaved");
        assert_eq!(state.active_text(), "typing");
    }

    #[test]
    fn a_saved_document_reports_the_save_time() {
        let mut state = AppState::new();
        let id = state.workspace.active_id().unwrap();
        state
            .workspace
            .get_mut(id)
            .unwrap()
            .record_disk_save(datetime!(2026-08-16 20:05 UTC));

        assert_eq!(
            save_state_label(state.workspace.active().unwrap()),
            "✓ Saved 8:05 PM"
        );
    }

    #[test]
    fn editing_after_a_save_still_shows_the_last_disk_save() {
        let mut state = AppState::new();
        let id = state.workspace.active_id().unwrap();
        state
            .workspace
            .get_mut(id)
            .unwrap()
            .record_disk_save(datetime!(2026-08-16 20:05 UTC));
        state.edit("more".to_owned());

        assert_eq!(
            save_state_label(state.workspace.active().unwrap()),
            "● Unsaved │ Last disk save 8:05 PM"
        );
    }

    #[test]
    fn save_without_a_path_reports_that_it_needs_one() {
        let mut state = AppState::new();
        let id = state.workspace.active_id().unwrap();
        assert_eq!(
            state.save_document(id, None),
            SaveResult::NeedsPath,
            "Save on an unsaved document must escalate to Save As, not no-op"
        );
    }

    #[test]
    fn a_failed_save_is_not_reported_as_saved() {
        // The distinction close-on-quit depends on: if this returned Saved,
        // the buffer would be discarded after a save that never happened.
        let mut state = AppState::new();
        let id = state.workspace.active_id().unwrap();
        state.edit("work".to_owned());

        let unwritable = std::path::PathBuf::from("no-such-dir-xyz").join("note.txt");
        assert_eq!(
            state.save_document(id, Some(unwritable)),
            SaveResult::Failed
        );
        assert!(state.error.is_some(), "a failure must be explained");
        assert!(state.is_dirty(id), "a failed save must not clear dirty");
    }

    #[test]
    fn a_successful_save_clears_dirty() {
        let dir = std::env::temp_dir().join(format!("bpad-ui-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("note.txt");

        let mut state = AppState::new();
        let id = state.workspace.active_id().unwrap();
        state.edit("work".to_owned());

        assert_eq!(
            state.save_document(id, Some(path.clone())),
            SaveResult::Saved
        );
        assert!(!state.is_dirty(id));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "work");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dirty_tracking_drives_the_close_prompt() {
        let mut state = AppState::new();
        let id = state.workspace.active_id().unwrap();

        assert!(!state.is_dirty(id), "a fresh buffer needs no prompt");
        state.edit("typed".to_owned());
        assert!(state.is_dirty(id), "an edited buffer must prompt");
        assert_eq!(state.display_name(id), UNTITLED);
    }

    #[test]
    fn closing_the_last_tab_opens_a_fresh_one() {
        let mut state = AppState::new();
        let id = state.workspace.active_id().unwrap();
        state.close(id);

        assert_eq!(state.workspace.len(), 1);
        assert!(state.workspace.active().is_some());
        assert!(
            !state.texts.contains_key(&id),
            "closed text must be dropped"
        );
    }

    #[test]
    fn suggested_filename_follows_the_grammar() {
        let state = AppState::new();
        let id = state.workspace.active_id().unwrap();
        let name = state.suggested_filename(id);
        assert!(name.starts_with("Untitled_"), "got {name}");
        assert!(name.ends_with(".txt"), "got {name}");
        assert!(SemanticName::parse(&name).is_some(), "got {name}");
    }
}
