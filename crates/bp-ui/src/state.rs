//! Document and workspace state.
//!
//! Owns `AppState` and everything that answers questions about
//! document/workspace state: what the active document is, whether it is dirty,
//! how to save it, and how to label it in the status bar.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use bp_config::Recent;
use bp_core::{Document, DocumentId, Encoding, LineEnding, UNTITLED, Workspace};
use bp_files::{DiskState, FileStamp, SaveOptions, atomic_write, load};
use bp_formats::Format;
use bp_naming::SemanticName;
use bp_theme::ThemeId;
use time::OffsetDateTime;

use crate::menus::action;

/// How much of a document is enough to answer a question about its start.
///
/// Format detection sniffs the beginning, and `refresh` asks on every
/// keystroke. Copying a whole document to look at its first line would put
/// the document's size back into the typing path, which is the cost the rope
/// exists to remove.
pub(crate) const PREFIX_CHARS: usize = 4096;

/// How a save attempt ended.
///
/// Three cases, not two: a close-on-quit flow must not treat "needs a path"
/// or "the disk refused" as success and then discard the buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SaveResult {
    Saved,
    /// No path yet. The caller should escalate to Save As.
    NeedsPath,
    /// Attempted and refused; `AppState::error` explains why.
    Failed,
}

/// What a Note-menu action decided to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NoteOutcome {
    /// Report something to the user.
    Show {
        title: String,
        body: String,
    },
    /// Offer to save under a suggested name.
    SaveAs,
    Nothing,
}

/// Whether the buffer text needs pushing back into the editor widget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushText {
    /// The document changed underneath the widget: opened, switched, closed.
    Yes,
    /// The user typed. The widget already holds the text, and pushing it back
    /// would clone the whole document on every keystroke to no effect.
    No,
}

pub struct AppState {
    pub(crate) workspace: Workspace,
    /// One editor per document: the rope is the storage, and the caret,
    /// selection and undo stack belong to us.
    ///
    /// True whichever view is drawing. `TextInput` still owns *its* caret and
    /// undo when it is the one on screen, but the text it hands back lands
    /// here as a single undoable replacement, so there is one document.
    editors: HashMap<DocumentId, bp_editor::Editor>,
    /// Draw with the custom surface rather than Slint's `TextInput`.
    pub(crate) editor_view: bool,
    /// First document line the surface is showing.
    pub(crate) first_line: usize,
    /// How many lines fit in it. Slint measures and tells us.
    pub(crate) visible_rows: usize,
    pub(crate) theme: ThemeId,
    /// Track the desktop's light/dark preference rather than staying put.
    ///
    /// A flag beside `theme` rather than a fifth `ThemeId`, because the theme
    /// in force is always one of the four either way -- this only says where
    /// it came from, and therefore whether a later preference change should
    /// move it. ADR-0009 keeps themes as data; this is not one.
    pub(crate) follow_system_theme: bool,
    /// What the desktop last said, or `None` if it will not say. Slint
    /// reports it; nothing here asks the platform directly.
    pub(crate) system_dark: Option<bool>,
    pub(crate) error: Option<String>,
    /// Cached gutter text, and the line count it was built for.
    ///
    /// Rebuilding this on every keystroke was the single largest avoidable
    /// cost in the typing path: it allocates proportionally to the document
    /// on each character typed, while the content only changes when a line is
    /// added or removed. `usize::MAX` is a sentinel meaning "never built",
    /// since 0 is a line count `max(1)` can never produce.
    pub(crate) gutter: String,
    gutter_lines: usize,
    pub(crate) show_gutter: bool,
    pub(crate) wrap_text: bool,
    /// How the Tab key indents, and how wide a tab is drawn.
    ///
    /// One value serving both, because they are the same number seen from two
    /// sides -- a document indented to eight columns and drawn at four does
    /// not look wrong, it looks like a different document.
    pub(crate) indent: bp_editor::Indent,
    /// Editor font size in points, and the only place it is decided.
    ///
    /// Held here rather than read back off the widget because both editor
    /// views draw at it and neither is authoritative -- `EditorSurface`
    /// measures its character advance from whatever it is given, so a size
    /// that lived in the view would put the caret arithmetic somewhere Rust
    /// cannot see. Config supplies the starting value; zoom moves it.
    pub(crate) font_size: u8,
    /// What each document's file looked like when we last read or wrote it,
    /// so an edit made by another program can be noticed.
    stamps: HashMap<DocumentId, FileStamp>,
    pub(crate) recent: Recent,
    /// Standing warning about the file on disk. Distinct from `error`, which
    /// reports something that just failed; this persists until resolved.
    pub(crate) disk_warning: Option<String>,
    /// Matches for the current find query, and which one is selected.
    pub(crate) matches: Vec<bp_search::Match>,
    match_index: usize,
    pub(crate) find_status: String,
    /// What the go-to-line bar is reporting. Separate from `error`, which is
    /// the status bar's: a bar with its own input owns its own message, or
    /// "there are only 42 lines" would appear at the far end of the window
    /// from the box it is about.
    pub(crate) goto_status: String,
    pub(crate) journal: bp_history::Journal,
    /// The tab the context menu was opened on.
    ///
    /// Held rather than passed with the click, because opening the menu and
    /// choosing a row are two separate events -- by the time a row is
    /// clicked the pointer has moved and the tab under it may be a different
    /// one. `None` means the menu is not open, in which case the rows fall
    /// back to the active tab.
    pub(crate) tab_context: Option<DocumentId>,
    /// Cross-file search results, indexed by the row the user clicks.
    pub(crate) file_hits: Vec<bp_search::FileHit>,
    pub(crate) clips: bp_clipboard::History,
}

impl AppState {
    pub(crate) fn new() -> Self {
        let mut workspace = Workspace::new();
        let id = workspace.open_new(now());
        let mut editors = HashMap::new();
        editors.insert(id, bp_editor::Editor::default());
        Self {
            workspace,
            editors,
            editor_view: false,
            first_line: 0,
            // Replaced by Slint's own measurement as soon as the surface has
            // a height; only Page Up before the first frame would see this.
            visible_rows: 30,
            theme: ThemeId::default(),
            follow_system_theme: false,
            system_dark: None,
            error: None,
            gutter: String::new(),
            gutter_lines: usize::MAX,
            show_gutter: true,
            wrap_text: false,
            indent: bp_editor::Indent::default(),
            font_size: bp_config::DEFAULT_FONT_SIZE,
            stamps: HashMap::new(),
            recent: bp_config::load_recent(),
            disk_warning: None,
            matches: Vec::new(),
            match_index: 0,
            find_status: String::new(),
            goto_status: String::new(),
            journal: bp_history::Journal::new(recovery_dir()),
            tab_context: None,
            file_hits: Vec::new(),
            clips: bp_clipboard::History::new(),
        }
    }

    /// Write a recovery checkpoint for every unsaved document, and clear the
    /// journal for those that are now clean.
    ///
    /// specs.md section 3 is emphatic that a checkpoint is not a save, and
    /// `bp-core` enforces that: `record_checkpoint` deliberately leaves the
    /// document dirty.
    pub(crate) fn checkpoint_all(&mut self) {
        let ids: Vec<DocumentId> = self.workspace.iter().map(Document::id).collect();
        let at = now();

        for id in ids {
            let Some(doc) = self.workspace.get(id) else {
                continue;
            };
            if !doc.is_dirty() {
                // Clean means the file holds the work; the journal must not
                // linger and offer to "recover" a stale copy.
                let _ = self.journal.discard(id.get());
                continue;
            }

            let entry = bp_history::Checkpoint {
                path: doc.path().map(Path::to_path_buf),
                name: doc.display_name().to_owned(),
                text: self.text_of(id).to_owned(),
                written_at: bp_history::now_unix(),
            };
            if self.journal.checkpoint(id.get(), &entry).is_ok()
                && let Some(doc) = self.workspace.get_mut(id)
            {
                doc.record_checkpoint(at);
            }
        }
    }

    /// Open recovered documents as unsaved tabs.
    pub(crate) fn restore(&mut self, entries: Vec<(u64, bp_history::Checkpoint)>) {
        for (_, entry) in entries {
            let id = match entry.path {
                Some(path) => self.workspace.open_path(path, now()),
                None => self.workspace.open_new(now()),
            };
            self.editors.insert(id, bp_editor::Editor::new(&entry.text));
            // Recovered work is by definition not on disk yet.
            if let Some(doc) = self.workspace.get_mut(id) {
                doc.mark_modified();
            }
        }
    }

    /// Recompute matches for `query` against the active document.
    ///
    /// Returns the range to select, if there is one.
    pub(crate) fn find(&mut self, query: &bp_search::Query) -> Option<std::ops::Range<usize>> {
        self.match_index = 0;
        if query.is_empty() {
            self.matches.clear();
            self.find_status.clear();
            return None;
        }
        let text = self.active_text();
        match bp_search::find_all(&text, query) {
            Ok(found) => {
                self.matches = found;
                self.find_status = if self.matches.is_empty() {
                    "no matches".to_owned()
                } else {
                    format!("1 of {}", self.matches.len())
                };
                self.matches.first().map(|m| m.range.clone())
            }
            Err(e) => {
                // A half-typed regex is the normal case while typing, so this
                // reports rather than alarms.
                self.matches.clear();
                self.find_status = e.to_string();
                None
            }
        }
    }

    /// Step to the next or previous match, wrapping.
    pub(crate) fn step_match(&mut self, forward: bool) -> Option<std::ops::Range<usize>> {
        if self.matches.is_empty() {
            return None;
        }
        let len = self.matches.len();
        self.match_index = if forward {
            (self.match_index + 1) % len
        } else {
            (self.match_index + len - 1) % len
        };
        self.find_status = format!("{} of {len}", self.match_index + 1);
        Some(self.matches[self.match_index].range.clone())
    }

    /// Note that this document and its file are in step, and remember the
    /// file as recently used.
    fn mark_in_step(&mut self, id: DocumentId, path: &Path) {
        if let Some(stamp) = FileStamp::of(path) {
            self.stamps.insert(id, stamp);
        }
        self.disk_warning = None;
        self.recent.push(path);
        bp_config::save_recent(&self.recent);
    }

    /// How the active document's file compares with what we last saw.
    fn disk_state(&self) -> Option<DiskState> {
        let id = self.workspace.active_id()?;
        let path = self.workspace.get(id)?.path()?;
        Some(bp_files::check(path, *self.stamps.get(&id)?))
    }

    /// Refresh the standing warning about the file on disk.
    pub(crate) fn poll_disk(&mut self) {
        self.disk_warning = match self.disk_state() {
            Some(DiskState::Modified) => {
                Some("⚠ changed on disk by another program — File ▸ Reload from Disk".to_owned())
            }
            Some(DiskState::Missing) => {
                Some("⚠ no longer on disk — Save will write it again".to_owned())
            }
            _ => None,
        };
    }

    pub(crate) fn recent_path(&self, index: usize) -> Option<PathBuf> {
        self.recent.paths().get(index).cloned()
    }

    /// Work out what a Note-menu action should do.
    ///
    /// Returns a decision rather than acting, so the extraction stays
    /// testable without a window and the dialogs stay in one place.
    pub(crate) fn note_action(&self, id: i32) -> NoteOutcome {
        let text = self.active_text();
        match id {
            action::NOTE_TITLE => bp_semantic::suggest_title(&text).map_or(
                NoteOutcome::Show {
                    title: "Suggest title".to_owned(),
                    body: "There is nothing in this document to take a title from.".to_owned(),
                },
                |title| NoteOutcome::Show {
                    title: "Suggested title".to_owned(),
                    body: format!("{title}\n\nUse Note ▸ Semantic Rename to save under this name."),
                },
            ),

            // A physical rename needs explicit approval (PROJECT_MEMORY), so
            // this offers a filename and lets the save dialog be the consent.
            action::NOTE_RENAME => NoteOutcome::SaveAs,

            action::NOTE_SUMMARY => {
                let summary = bp_semantic::summary(&text, 400);
                NoteOutcome::Show {
                    title: "Summary".to_owned(),
                    body: if summary.is_empty() {
                        "No prose to summarise.".to_owned()
                    } else {
                        summary
                    },
                }
            }

            action::NOTE_KEYWORDS => {
                let keywords = bp_semantic::keywords(&text, 12);
                NoteOutcome::Show {
                    title: "Keywords".to_owned(),
                    body: if keywords.is_empty() {
                        "No distinctive words found.".to_owned()
                    } else {
                        keywords.join(", ")
                    },
                }
            }

            action::NOTE_OUTLINE => {
                let outline = bp_semantic::outline(&text);
                NoteOutcome::Show {
                    title: "Outline".to_owned(),
                    body: if outline.is_empty() {
                        "No headings in this document.".to_owned()
                    } else {
                        outline
                            .iter()
                            .map(|h| {
                                format!(
                                    "{}{}  (line {})",
                                    "    ".repeat(h.level.saturating_sub(1)),
                                    h.text,
                                    h.line
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("\n")
                    },
                }
            }

            _ => NoteOutcome::Nothing,
        }
    }

    /// A filename suggested from the document's own content.
    ///
    /// Falls back to the document's title when there is nothing to extract,
    /// so Save As always has something to offer.
    fn semantic_filename(&self, id: DocumentId) -> Option<String> {
        let doc = self.workspace.get(id)?;
        let title = bp_semantic::suggest_title(&self.text_of(id))?;
        let extension = doc
            .path()
            .and_then(Path::extension)
            .and_then(|e| e.to_str())
            .unwrap_or("txt");
        Some(SemanticName::new(&title, doc.created_at().date(), extension).to_filename())
    }

    /// What the active document is, by extension then by content.
    pub(crate) fn format(&self) -> Format {
        let path = self.workspace.active().and_then(Document::path);
        // A prefix, not the document: this is asked on every refresh, and
        // detection only ever sniffs the beginning.
        bp_formats::detect(path, &self.active_prefix())
    }

    /// Replace the active document's text with the result of a data
    /// operation, leaving it unsaved.
    ///
    /// Applied as an ordinary edit: the user can undo it, and nothing reaches
    /// disk until they save.
    fn apply_to_active(&mut self, result: Result<String, bp_data::DataError>) {
        match result {
            Ok(text) => {
                self.error = None;
                self.edit(text);
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    /// Apply a whole-document line operation.
    ///
    /// An ordinary edit, like the data operations: undoable, and nothing
    /// reaches disk until the user saves. The caret-dependent members of the
    /// family -- Duplicate Line, Move Line Up/Down -- are not here, because
    /// the caret is not readable until the custom editor view exists.
    pub(crate) fn run_line_action(&mut self, id: i32) {
        use bp_editor::lines::{self, Order};

        let text = self.active_text();
        let changed = match id {
            action::LINES_SORT_ASC => lines::sort(&text, Order::Ascending),
            action::LINES_SORT_DESC => lines::sort(&text, Order::Descending),
            action::LINES_DEDUPE => lines::remove_duplicates(&text),
            action::LINES_REVERSE => lines::reverse(&text),
            action::LINES_TRIM => lines::trim_trailing_whitespace(&text),
            _ => return,
        };
        self.error = None;
        self.edit(changed);
    }

    /// Apply a caret-dependent line operation: Duplicate Line, Move Line
    /// Up/Down.
    ///
    /// Unlike `run_line_action`, these move and duplicate the line the
    /// caret is on rather than rewriting the whole document, so they call
    /// `Editor` directly. That caret is `bp-editor`'s and `TextInput` never
    /// exposes it, so this is a no-op outside the custom editor view -- the
    /// menu already disables these rows there, and a shortcut key that
    /// reaches here anyway is safer treated as doing nothing than acted on
    /// against a caret position this crate cannot trust.
    pub(crate) fn run_line_edit(&mut self, id: i32) {
        if !self.editor_view {
            return;
        }
        // Compared rather than assumed: Duplicate Line always changes the
        // document, but Move Line Up/Down is a deliberate no-op at the
        // first or last line, and `bp-editor` reports that only through its
        // undo stack, not a return value. A no-op must not mark a clean
        // document dirty.
        let before = self.active_text();
        let Some(editor) = self.active_editor_mut() else {
            return;
        };
        match id {
            action::DUPLICATE_LINE => editor.duplicate_line(),
            action::MOVE_LINE_UP => editor.move_line_up(),
            action::MOVE_LINE_DOWN => editor.move_line_down(),
            _ => return,
        }
        if self.active_text() != before {
            self.mark_edited();
        }
    }

    /// Run a data operation, reporting the outcome in the status bar.
    pub(crate) fn run_data_action(&mut self, id: i32) {
        let text = self.active_text().to_owned();
        match (id, self.format()) {
            (action::DATA_VALIDATE, Format::Json) => {
                self.error = Some(match bp_data::json_validate(&text) {
                    Ok(()) => "✓ valid JSON".to_owned(),
                    Err(e) => format!("invalid JSON — {e}"),
                });
            }
            (action::DATA_VALIDATE, Format::JsonLines) => {
                self.error = Some(bp_data::jsonl_validate(&text).summary());
            }
            (action::DATA_VALIDATE, Format::Toml) => {
                self.error = Some(match bp_data::toml_validate(&text) {
                    Ok(()) => "✓ valid TOML".to_owned(),
                    Err(e) => format!("invalid TOML — {e}"),
                });
            }
            (action::DATA_FORMAT, Format::Json) => {
                self.apply_to_active(bp_data::json_format(&text));
            }
            (action::DATA_FORMAT, Format::Toml) => {
                self.apply_to_active(bp_data::toml_format(&text));
            }
            (action::DATA_MINIFY, Format::Json) => {
                self.apply_to_active(bp_data::json_minify(&text));
            }
            (action::DATA_TO_JSONL, _) => self.apply_to_active(bp_data::json_to_jsonl(&text)),
            (action::DATA_TO_JSON, _) => self.apply_to_active(bp_data::jsonl_to_json(&text)),
            (action::DATA_REPORT, _) => {
                self.error = Some(match bp_data::delimited_report(&text) {
                    Ok(report) => report.summary(),
                    Err(e) => format!("could not read as a table — {e}"),
                });
            }
            (action::DATA_CSV_TO_JSON, _) => {
                self.apply_to_active(bp_data::delimited_to_json(&text));
            }
            (action::DATA_CSV_TO_JSONL, _) => {
                self.apply_to_active(bp_data::delimited_to_jsonl(&text));
            }
            (action::DATA_COLUMN_TYPES, _) => {
                self.error = Some(match bp_data::column_types(&text) {
                    Ok(columns) if columns.is_empty() => "no columns to report".to_owned(),
                    Ok(columns) => columns
                        .iter()
                        .map(bp_data::ColumnReport::summary)
                        .collect::<Vec<_>>()
                        .join(" │ "),
                    Err(e) => format!("could not read as a table — {e}"),
                });
            }
            // A row that does not apply to this format. The menu should not
            // have offered it; doing nothing is better than guessing.
            _ => {}
        }
    }

    /// Discard edits and re-read the active document from disk.
    pub(crate) fn reload(&mut self) {
        self.error = None;
        let Some(id) = self.workspace.active_id() else {
            return;
        };
        let Some(path) = self
            .workspace
            .get(id)
            .and_then(|d| d.path())
            .map(Path::to_path_buf)
        else {
            return;
        };
        match load(&path) {
            Ok(file) => {
                if let Some(doc) = self.workspace.get_mut(id) {
                    doc.set_encoding(file.encoding);
                    doc.set_line_ending(file.line_ending);
                    // Back in step with disk, so the document is clean again.
                    doc.record_disk_save(now());
                }
                self.editors.insert(id, bp_editor::Editor::new(&file.text));
                self.mark_in_step(id, &path);
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    /// Change how the document will be written, and mark it unsaved.
    ///
    /// The bytes on disk no longer match the intent, which is exactly what
    /// "unsaved" means -- leaving it clean would hide a pending change.
    pub(crate) fn set_line_ending(&mut self, line_ending: LineEnding) {
        if let Some(doc) = self.workspace.active_mut()
            && doc.line_ending() != line_ending
        {
            doc.set_line_ending(line_ending);
            doc.mark_modified();
        }
    }

    pub(crate) fn set_encoding(&mut self, encoding: Encoding) {
        if let Some(doc) = self.workspace.active_mut()
            && doc.encoding() != encoding
        {
            doc.set_encoding(encoding);
            doc.mark_modified();
        }
    }

    pub(crate) fn active_editor(&self) -> Option<&bp_editor::Editor> {
        self.workspace
            .active_id()
            .and_then(|id| self.editors.get(&id))
    }

    pub(crate) fn active_editor_mut(&mut self) -> Option<&mut bp_editor::Editor> {
        self.workspace
            .active_id()
            .and_then(|id| self.editors.get_mut(&id))
    }

    /// The active document as one string.
    ///
    /// Allocates: a rope is not contiguous, so "the whole document as text"
    /// costs a copy. That is the right trade -- saving, searching and format
    /// conversion each want the whole thing once, while editing wants none of
    /// it -- but it is why nothing on the typing path calls this.
    pub(crate) fn active_text(&self) -> String {
        self.active_editor()
            .map(bp_editor::Editor::text)
            .unwrap_or_default()
    }

    fn text_of(&self, id: DocumentId) -> String {
        self.editors
            .get(&id)
            .map(bp_editor::Editor::text)
            .unwrap_or_default()
    }

    /// The first [`PREFIX_CHARS`] characters of the active document.
    fn active_prefix(&self) -> String {
        self.active_editor().map_or_else(String::new, |editor| {
            editor
                .buffer()
                .slice(0..PREFIX_CHARS.min(editor.buffer().len_chars()))
        })
    }

    /// Whether the document has anything worth extracting a title from.
    ///
    /// Checks a prefix rather than the whole document, because this is asked
    /// on every refresh. A document longer than the prefix and made entirely
    /// of whitespace would answer wrongly, and is not worth a full scan.
    pub(crate) fn active_has_content(&self) -> bool {
        self.active_editor().is_some_and(|editor| {
            editor.buffer().len_chars() > PREFIX_CHARS || !self.active_prefix().trim().is_empty()
        })
    }

    /// Tab label for `id`, for use in prompts.
    pub(crate) fn display_name(&self, id: DocumentId) -> String {
        self.workspace
            .get(id)
            .map_or_else(|| UNTITLED.to_owned(), |d| d.display_name().to_owned())
    }

    pub(crate) fn is_dirty(&self, id: DocumentId) -> bool {
        self.workspace.get(id).is_some_and(Document::is_dirty)
    }

    /// Rebuild the gutter only when the line count actually changed.
    ///
    /// Returns `true` if the cache changed and the UI needs the new value.
    pub(crate) fn sync_gutter(&mut self) -> bool {
        // From the rope, which already counts the line after a trailing
        // newline -- unlike `str::lines()`, which ignores it and left every
        // file ending in one with a gutter shorter than the text beside it.
        let lines = self
            .active_editor()
            .map_or(1, |editor| editor.buffer().len_lines());
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

    pub(crate) fn new_document(&mut self) {
        self.error = None;
        let id = self.workspace.open_new(now());
        self.editors.insert(id, bp_editor::Editor::default());
    }

    pub(crate) fn open(&mut self, path: PathBuf) {
        self.error = None;
        let path2 = path.clone();
        match load(&path) {
            Ok(file) => {
                let id = self.workspace.open_path(path, now());
                if let Some(doc) = self.workspace.get_mut(id) {
                    doc.set_encoding(file.encoding);
                    doc.set_line_ending(file.line_ending);
                }
                self.editors.insert(id, bp_editor::Editor::new(&file.text));
                self.mark_in_step(id, &path2);
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
    pub(crate) fn save_document(&mut self, id: DocumentId, path: Option<PathBuf>) -> SaveResult {
        self.error = None;
        let Some(doc) = self.workspace.get(id) else {
            return SaveResult::Saved;
        };

        let Some(target) = path.or_else(|| doc.path().map(Path::to_path_buf)) else {
            return SaveResult::NeedsPath;
        };

        let bytes = match encode(&self.text_of(id), doc.encoding(), doc.line_ending()) {
            Ok(b) => b,
            Err(message) => {
                self.error = Some(message);
                return SaveResult::Failed;
            }
        };

        match atomic_write(&target, &bytes, SaveOptions::default()) {
            Ok(_) => {
                if let Some(doc) = self.workspace.get_mut(id) {
                    doc.set_path(target.clone());
                    // Only now, after a verified write, is the document clean.
                    doc.record_disk_save(now());
                }
                // Re-stamp from what we just wrote, or our own save would
                // look like somebody else's change on the next poll.
                self.mark_in_step(id, &target);
                SaveResult::Saved
            }
            Err(e) => {
                self.error = Some(e.to_string());
                SaveResult::Failed
            }
        }
    }

    /// Write `id`'s current text to `target` without adopting it.
    ///
    /// Deliberately not a call into [`save_document`](Self::save_document)
    /// with a path. A copy is a different operation that happens to write the
    /// same bytes, and the three things it must *not* do are exactly the
    /// three that one does: it must not move the document's path, must not
    /// mark it clean, and must not touch the recent-files list or the disk
    /// stamp. Sharing the code would mean sharing all four behaviours and
    /// then subtracting three, which is how Save Copy quietly becomes
    /// Save As.
    ///
    /// Only the encoding is shared, because bytes written differently from a
    /// real save would make the copy a different file from the original.
    pub(crate) fn save_copy(&mut self, id: DocumentId, target: &Path) -> SaveResult {
        self.error = None;
        let Some(doc) = self.workspace.get(id) else {
            return SaveResult::Saved;
        };

        let bytes = match encode(&self.text_of(id), doc.encoding(), doc.line_ending()) {
            Ok(b) => b,
            Err(message) => {
                self.error = Some(message);
                return SaveResult::Failed;
            }
        };

        match atomic_write(target, &bytes, SaveOptions::default()) {
            Ok(_) => {
                // Reported, because a write that leaves no trace anywhere in
                // the window is indistinguishable from one that did not
                // happen -- the tab does not change, and neither does the
                // save state.
                self.error = Some(format!("copy written to {}", target.display()));
                SaveResult::Saved
            }
            Err(e) => {
                self.error = Some(e.to_string());
                SaveResult::Failed
            }
        }
    }

    /// Report the active document's statistics into the status bar.
    ///
    /// A one-off on a menu click, which is what makes reading the whole
    /// document acceptable here: `active_text()` copies it, so this must
    /// never move onto the typing path or into `refresh` (see the trap in
    /// R011).
    pub(crate) fn report_statistics(&mut self) {
        let stats = bp_semantic::statistics(&self.active_text());
        self.error = Some(format!(
            "{} words, {} lines, {} paragraphs, {} characters ({} without spaces)",
            stats.words,
            stats.lines,
            stats.paragraphs,
            stats.characters,
            stats.characters_no_whitespace,
        ));
    }

    /// Insert a date or time stamp at the caret.
    ///
    /// The clock is read here rather than in `bp-naming`, which is pure and
    /// never reads one -- so the shell decides *when*, and the crate only
    /// decides how that instant is written.
    ///
    /// Caret work, so it needs the editor we own. Under `TextInput` there is
    /// no readable caret (ADR-0018) and the rows are disabled instead.
    pub(crate) fn insert_stamp(&mut self, index: usize) -> bool {
        let Some(stamp) = bp_naming::Stamp::all().get(index).copied() else {
            return false;
        };
        let text = bp_naming::render(stamp, now());
        let Some(editor) = self.active_editor_mut() else {
            return false;
        };
        editor.insert(&text);
        self.mark_edited();
        true
    }

    /// Which document the tab context menu's rows should act on.
    ///
    /// The right-clicked tab, or the active one when the menu was not opened
    /// from a tab -- so the same ids serve a keyboard route later without the
    /// rows changing meaning.
    pub(crate) fn tab_context_id(&self) -> Option<DocumentId> {
        self.tab_context.or_else(|| self.workspace.active_id())
    }

    /// What the tab context menu needs to know: how many other tabs are open,
    /// and whether the one under the pointer has a path to copy.
    pub(crate) fn tab_context_shape(&self) -> (usize, bool) {
        let target = self.tab_context_id();
        let others = self.workspace.len().saturating_sub(1);
        let has_path = target
            .and_then(|id| self.workspace.get(id))
            .and_then(Document::path)
            .is_some();
        (others, has_path)
    }

    /// Move the caret to the line the user typed, and say what happened.
    ///
    /// `Editor::go_to_line` moves somewhere sensible *and* reports whether
    /// the line existed, so a number past the end lands at the last line and
    /// says why rather than doing nothing and looking broken.
    ///
    /// Caret work, so `--editor-view` only: `TextInput`'s caret cannot be
    /// moved from here (ADR-0018).
    pub(crate) fn go_to_line(&mut self, text: &str) -> Option<std::ops::Range<usize>> {
        if !self.editor_view {
            self.goto_status = "Go to Line needs --editor-view".to_owned();
            return None;
        }
        let trimmed = text.trim();
        if trimmed.is_empty() {
            self.goto_status.clear();
            return None;
        }
        // Parsed rather than trusted: `input-type: number` is a hint to the
        // keyboard, not a guarantee from the backend.
        let Ok(line) = trimmed.parse::<usize>() else {
            self.goto_status = "not a line number".to_owned();
            return None;
        };
        let editor = self.active_editor_mut()?;
        let existed = editor.go_to_line(line);
        let cursor = editor.cursor();
        let total = editor.buffer().len_lines();
        self.goto_status = if existed {
            String::new()
        } else {
            format!("there are only {total} lines")
        };
        Some(cursor..cursor)
    }

    /// True if `id`'s file changed underneath us since we last read or wrote
    /// it — meaning a save would overwrite somebody else's work.
    pub(crate) fn would_overwrite_external_change(&self, id: DocumentId) -> bool {
        let Some(path) = self.workspace.get(id).and_then(Document::path) else {
            return false;
        };
        let Some(stamp) = self.stamps.get(&id) else {
            return false;
        };
        bp_files::check(path, *stamp) == DiskState::Modified
    }

    /// Where a file dialog should open.
    ///
    /// The active document's own folder, else the user's Documents folder.
    /// Explicitly *not* the process working directory, which is wherever the
    /// binary happened to be launched from -- during testing that was a git
    /// checkout, and notes were saved straight into it.
    pub(crate) fn dialog_directory(&self) -> PathBuf {
        self.workspace
            .active()
            .and_then(Document::path)
            .and_then(Path::parent)
            .filter(|p| !p.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .or_else(documents_dir)
            .unwrap_or_else(|| PathBuf::from("."))
    }

    /// A default filename for Save As, in the grammar from ADR-0003.
    ///
    /// Prefers a title extracted from the document's own content — that is
    /// the "Semantic Filing Apparatus" doing its job — and falls back to the
    /// document's title when there is nothing to extract.
    pub(crate) fn suggested_filename(&self, id: DocumentId) -> String {
        self.semantic_filename(id).unwrap_or_else(|| {
            self.workspace.get(id).map_or_else(
                || "Untitled.txt".to_owned(),
                |doc| SemanticName::new(doc.title(), doc.created_at().date(), "txt").to_filename(),
            )
        })
    }

    pub(crate) fn close(&mut self, id: DocumentId) {
        self.error = None;
        if self.workspace.close(id).is_some() {
            self.editors.remove(&id);
        }
        // Never leave the user staring at an empty frame with no way back.
        if self.workspace.is_empty() {
            self.new_document();
        }
    }

    /// Replace the active document's text as a single undoable edit.
    ///
    /// What a data operation, Replace All or a line operation does, and what
    /// `TextInput` reports after every keystroke. Unchanged text is not an
    /// edit: a Format that found nothing to reformat must not mark a saved
    /// document unsaved.
    pub(crate) fn edit(&mut self, text: String) {
        let Some(id) = self.workspace.active_id() else {
            return;
        };
        let changed = self
            .editors
            .get_mut(&id)
            .is_some_and(|editor| editor.replace_all_text(&text));

        if changed && let Some(doc) = self.workspace.get_mut(id) {
            doc.mark_modified();
        }
    }

    /// `Ln 42, Col 18` when we own the caret; the line count when we do not.
    ///
    /// specs.md section 3 asks for line and column. Under `TextInput` the
    /// caret is reachable only through a property marked internal and
    /// undocumented, so the honest answer there is the thing we do know.
    pub(crate) fn cursor_label(&self) -> String {
        let lines = self.gutter_lines;
        if !self.editor_view {
            return format!("{lines} lines");
        }
        self.active_editor().map_or_else(
            || format!("{lines} lines"),
            |editor| {
                let position = editor.position();
                format!("Ln {}, Col {}", position.line, position.column)
            },
        )
    }

    /// Scroll so the caret is on screen, moving as little as possible.
    pub(crate) fn reveal_caret(&mut self) {
        let rows = self.visible_rows.max(1);
        let caret_line = self
            .active_editor()
            .map_or(0, |editor| editor.position().line - 1);
        self.first_line = bp_editor::view::reveal(self.first_line, rows, caret_line);
    }

    /// Note that the active document was edited in place, through the caret.
    pub(crate) fn mark_edited(&mut self) {
        if let Some(id) = self.workspace.active_id()
            && let Some(doc) = self.workspace.get_mut(id)
        {
            doc.mark_modified();
        }
    }
}

/// Status-bar save state, per specs.md section 3.
pub(crate) fn save_state_label(doc: &Document) -> String {
    if !doc.is_dirty() {
        return doc.last_disk_save().map_or_else(
            || "● Never saved".to_owned(),
            |t| format!("✓ Saved {}", clock(t.get())),
        );
    }

    // specs.md section 3's unsaved form: the recovery checkpoint and the last
    // real disk save, side by side and never confused for one another. The
    // whole reason `CheckpointTime` and `DiskSaveTime` are different types is
    // so this line cannot accidentally claim the work is safe.
    let mut parts = vec!["● Unsaved".to_owned()];
    if let Some(checkpoint) = doc.last_checkpoint() {
        parts.push(format!("Recovery {}", clock(checkpoint.get())));
    }
    if let Some(saved) = doc.last_disk_save() {
        parts.push(format!("Last disk save {}", clock(saved.get())));
    }
    parts.join(" │ ")
}

/// Local wall-clock time, falling back to UTC.
///
/// `now_local` can fail on Unix in a multithreaded process. A timestamp in
/// the wrong zone is a small wrong; refusing to record the save at all would
/// be a large one.
pub(crate) fn now() -> OffsetDateTime {
    OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc())
}

/// `8:05 PM`, as specs.md section 3 shows it.
///
/// Delegated to `bp-naming` rather than written here. The Insert menu puts
/// the same rendering into documents, and two copies of a twelve-hour clock
/// is two places for midnight to come out as `0:30 AM` -- in one of which
/// nobody would notice.
pub(crate) fn clock(t: OffsetDateTime) -> String {
    bp_naming::render(bp_naming::Stamp::Time, t)
}

/// Encode the buffer for disk, honouring the document's encoding and line
/// endings.
///
/// Round-tripping matters: a file opened as CRLF with a BOM must be written
/// back that way, or saving silently rewrites every line of someone's file.
pub(crate) fn encode(
    text: &str,
    encoding: Encoding,
    line_ending: LineEnding,
) -> Result<Vec<u8>, String> {
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

/// The user's Documents folder, if the platform names one.
pub(crate) fn documents_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        let profile = std::env::var_os("USERPROFILE")?;
        let docs = PathBuf::from(profile).join("Documents");
        docs.is_dir().then_some(docs)
    } else {
        // XDG_DOCUMENTS_DIR is set by user-dirs; fall back to the convention,
        // then to the home directory itself.
        if let Some(dir) = std::env::var_os("XDG_DOCUMENTS_DIR") {
            let dir = PathBuf::from(dir);
            if dir.is_dir() {
                return Some(dir);
            }
        }
        let home = PathBuf::from(std::env::var_os("HOME")?);
        let docs = home.join("Documents");
        Some(if docs.is_dir() { docs } else { home })
    }
}

/// Where recovery checkpoints live: beside the config file, in `recovery/`.
///
/// The `.gitignore` already excludes `/recovery`, and this is deliberately a
/// path the user can be told — it holds copies of their unsaved work.
pub(crate) fn recovery_dir() -> PathBuf {
    bp_config::config_path().map_or_else(
        || PathBuf::from("recovery"),
        |p| p.with_file_name("recovery"),
    )
}

/// Map a tab id from the UI back to a `DocumentId`.
///
/// Returns `None` for ids that are no longer open, which is what makes a
/// stale click harmless rather than a panic.
pub(crate) fn find_id(workspace: &Workspace, raw: i32) -> Option<DocumentId> {
    workspace
        .iter()
        .map(Document::id)
        .find(|id| i32::try_from(id.get()).unwrap_or(i32::MAX) == raw)
}

/// Build the query the find bar currently describes.
///
/// One function rather than three `Query` literals, because every place that
/// searches has to agree about the options -- a find that honours the regex
/// toggle and a Replace All that quietly does not is worse than neither
/// honouring it.
///
/// What `bp-search` cannot catch is a toggle landing in the wrong field:
/// `case_sensitive` and `whole_word` are both `bool`, so swapping them
/// compiles, passes every test in that crate, and gives the user a whole-word
/// switch that changes the case rules. That is what the test below is for.
pub(crate) fn find_query(
    pattern: &str,
    case_sensitive: bool,
    whole_word: bool,
    regex: bool,
) -> bp_search::Query {
    bp_search::Query {
        pattern: pattern.to_owned(),
        case_sensitive,
        whole_word,
        regex,
    }
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
    fn the_gutter_numbers_the_line_after_a_trailing_newline() {
        // Regression: `str::lines()` reports "a\n" as one line, so the gutter
        // was a line short for essentially every file on disk.
        let mut state = AppState::new();
        state.edit("a\n".to_owned());
        state.sync_gutter();
        assert_eq!(state.gutter, "1\n2");
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
    fn a_recovery_checkpoint_shows_beside_the_disk_save_not_instead_of_it() {
        // specs.md section 3. The failure this guards against is a status bar
        // that says "saved" because a checkpoint happened.
        let mut state = AppState::new();
        let id = state.workspace.active_id().unwrap();
        let doc = state.workspace.get_mut(id).unwrap();
        doc.record_disk_save(datetime!(2026-08-16 20:01 UTC));
        doc.mark_modified();
        doc.record_checkpoint(datetime!(2026-08-16 20:05 UTC));

        let label = save_state_label(state.workspace.active().unwrap());
        assert_eq!(
            label,
            "● Unsaved │ Recovery 8:05 PM │ Last disk save 8:01 PM"
        );
        assert!(!label.contains("Saved 8:05"), "a checkpoint is not a save");
    }

    #[test]
    fn a_checkpoint_alone_never_reads_as_saved() {
        let mut state = AppState::new();
        let id = state.workspace.active_id().unwrap();
        state.edit("work".to_owned());
        state
            .workspace
            .get_mut(id)
            .unwrap()
            .record_checkpoint(datetime!(2026-08-16 20:05 UTC));

        let label = save_state_label(state.workspace.active().unwrap());
        assert_eq!(label, "● Unsaved │ Recovery 8:05 PM");
        assert!(!label.contains('✓'));
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
            !state.editors.contains_key(&id),
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

    #[test]
    fn no_menu_action_id_falls_inside_the_recent_files_window() {
        // The dispatch arm for recent files is a *range*, so an id landing
        // inside it opens a file instead of doing what its row says. That
        // was previously prevented only by the order the arms happened to be
        // written in.
        let end = action::RECENT_BASE + i32::try_from(bp_config::MAX_RECENT).unwrap();
        let window = action::RECENT_BASE..end;

        for id in [
            action::LINES_SORT_ASC,
            action::LINES_SORT_DESC,
            action::LINES_DEDUPE,
            action::LINES_REVERSE,
            action::LINES_TRIM,
            action::DUPLICATE_LINE,
            action::MOVE_LINE_UP,
            action::MOVE_LINE_DOWN,
            action::DATA_VALIDATE,
            action::DATA_REPORT,
            action::DATA_CSV_TO_JSON,
            action::DATA_CSV_TO_JSONL,
            action::DATA_COLUMN_TYPES,
            action::NOTE_TITLE,
            action::NOTE_OUTLINE,
        ] {
            assert!(!window.contains(&id), "id {id} collides with recent files");
        }
    }

    // --- Duplicate Line / Move Line Up / Move Line Down --------------------

    #[test]
    fn duplicating_a_line_through_the_menu_marks_the_document_modified() {
        let mut state = AppState::new();
        state.editor_view = true;
        state.edit("one\ntwo\nthree".to_owned());
        let id = state.workspace.active_id().unwrap();
        state.workspace.get_mut(id).unwrap().record_disk_save(now());
        state.active_editor_mut().unwrap().set_cursor(5); // on "two"

        state.run_line_edit(action::DUPLICATE_LINE);

        assert_eq!(state.active_text(), "one\ntwo\ntwo\nthree");
        assert!(state.is_dirty(id), "duplicating a line is an edit");
    }

    #[test]
    fn moving_a_line_down_through_the_menu_swaps_it_and_marks_modified() {
        let mut state = AppState::new();
        state.editor_view = true;
        state.edit("one\ntwo\nthree".to_owned());
        let id = state.workspace.active_id().unwrap();
        state.workspace.get_mut(id).unwrap().record_disk_save(now());
        state.active_editor_mut().unwrap().set_cursor(1); // on "one"

        state.run_line_edit(action::MOVE_LINE_DOWN);

        assert_eq!(state.active_text(), "two\none\nthree");
        assert!(state.is_dirty(id));
    }

    #[test]
    fn moving_the_top_line_up_is_a_no_op_and_must_not_mark_the_document_dirty() {
        // bp-editor's own no-op for Move Line Up at the first line; the point
        // of this test is that bp-ui does not turn that no-op into a false
        // "modified" flag.
        let mut state = AppState::new();
        state.editor_view = true;
        state.edit("one\ntwo".to_owned());
        let id = state.workspace.active_id().unwrap();
        state.workspace.get_mut(id).unwrap().record_disk_save(now());
        state.active_editor_mut().unwrap().set_cursor(1); // on "one", the top line

        state.run_line_edit(action::MOVE_LINE_UP);

        assert_eq!(state.active_text(), "one\ntwo");
        assert!(
            !state.is_dirty(id),
            "a no-op must not mark a clean document dirty"
        );
    }

    #[test]
    fn duplicate_and_move_line_do_nothing_outside_the_custom_editor_view() {
        // The row is disabled in that view, but a shortcut key reaches
        // `run_line_edit` directly and must not act on a caret this crate
        // cannot trust `TextInput` to have reported correctly.
        let mut state = AppState::new();
        state.edit("one\ntwo\nthree".to_owned());
        let id = state.workspace.active_id().unwrap();
        state.workspace.get_mut(id).unwrap().record_disk_save(now());

        state.run_line_edit(action::DUPLICATE_LINE);

        assert_eq!(state.active_text(), "one\ntwo\nthree");
        assert!(!state.is_dirty(id));
    }

    // --- CSV / TSV data operations ------------------------------------------

    fn csv_state(text: &str) -> AppState {
        let mut state = AppState::new();
        let id = state.workspace.active_id().unwrap();
        // Format detection is extension-first (`bp_formats::detect`), so a
        // `.csv` path is what makes `run_data_action` see `Format::Csv`
        // without needing a real file on disk.
        state
            .workspace
            .get_mut(id)
            .unwrap()
            .set_path(PathBuf::from("table.csv"));
        state.edit(text.to_owned());
        state
    }

    #[test]
    fn converting_csv_to_json_replaces_the_document() {
        let mut state = csv_state("name,age\nAlice,30\nBob,25\n");
        let id = state.workspace.active_id().unwrap();
        // Cleared explicitly, rather than relying on the dirty flag the
        // initial `edit` already set, so the assertion below proves the
        // conversion itself is what marks the document unsaved.
        state.workspace.get_mut(id).unwrap().record_disk_save(now());

        state.run_data_action(action::DATA_CSV_TO_JSON);

        assert!(
            state.error.is_none(),
            "a successful conversion is not an error"
        );
        assert!(state.active_text().contains("Alice"));
        assert!(state.active_text().trim_start().starts_with('['));
        assert!(
            state.is_dirty(id),
            "nothing reaches disk on its own -- the conversion is an ordinary edit"
        );
    }

    #[test]
    fn converting_csv_to_json_lines_replaces_the_document() {
        let mut state = csv_state("name,age\nAlice,30\nBob,25\n");
        state.run_data_action(action::DATA_CSV_TO_JSONL);

        let text = state.active_text();
        assert_eq!(text.lines().count(), 2, "one compact object per data row");
        assert!(text.contains("Alice") && text.contains("Bob"));
    }

    #[test]
    fn column_types_reports_into_the_status_bar_without_changing_the_document() {
        let mut state = csv_state("name,age\nAlice,30\nBob,25\n");
        let before = state.active_text();

        state.run_data_action(action::DATA_COLUMN_TYPES);

        assert_eq!(
            state.active_text(),
            before,
            "a report must not edit the document"
        );
        let message = state.error.expect("column report in the status bar");
        assert!(message.contains("name"), "got {message}");
        assert!(
            message.contains("integer"),
            "age should read as integer; got {message}"
        );
    }

    #[test]
    fn an_action_that_is_not_a_csv_operation_on_a_csv_document_changes_nothing() {
        let mut state = csv_state("name,age\nAlice,30\n");
        let before = state.active_text();
        state.run_data_action(action::NOTE_TITLE);
        assert_eq!(state.active_text(), before);
    }

    // --- Save Copy ------------------------------------------------------

    /// A document with a path and unsaved edits -- the state in which every
    /// difference between Save Copy and Save As is visible.
    fn dirty_saved_state(dir: &std::path::Path) -> (AppState, DocumentId, PathBuf) {
        let original = dir.join("original.txt");
        std::fs::write(&original, "first\n").unwrap();

        let mut state = AppState::new();
        state.open(original.clone());
        let id = state.workspace.active_id().unwrap();
        state.edit("first\nsecond\n".to_owned());
        assert!(state.workspace.get(id).unwrap().is_dirty());
        (state, id, original)
    }

    #[test]
    fn a_copy_does_not_become_the_documents_path() {
        // The difference that turns Save Copy into Save As. If the path
        // moves, the next Ctrl+S writes the copy and leaves the original
        // behind, unsaved and unmentioned.
        let dir = tempfile::tempdir().unwrap();
        let (mut state, id, original) = dirty_saved_state(dir.path());
        let copy = dir.path().join("copy.txt");

        assert_eq!(state.save_copy(id, &copy), SaveResult::Saved);

        assert_eq!(
            state.workspace.get(id).unwrap().path(),
            Some(original.as_path()),
            "the document still belongs to the file it was opened from"
        );
    }

    #[test]
    fn a_copy_leaves_the_document_unsaved() {
        // The dangerous one. A cleared dirty flag means the close prompt
        // never appears and the real edits are discarded in silence.
        let dir = tempfile::tempdir().unwrap();
        let (mut state, id, _) = dirty_saved_state(dir.path());

        state.save_copy(id, &dir.path().join("copy.txt"));

        assert!(
            state.workspace.get(id).unwrap().is_dirty(),
            "the original still has edits that are not on disk"
        );
    }

    #[test]
    fn a_copy_does_not_join_the_recent_files_list() {
        // Recent files is "documents you worked on". A copy is an export;
        // reopening it would give a stale fork of the document.
        let dir = tempfile::tempdir().unwrap();
        let (mut state, id, _) = dirty_saved_state(dir.path());
        let copy = dir.path().join("copy.txt");
        let before = state.recent.paths().to_vec();

        state.save_copy(id, &copy);

        assert_eq!(state.recent.paths(), before.as_slice());
        assert!(
            !state.recent.paths().contains(&copy),
            "the copy must not appear in Open Recent"
        );
    }

    #[test]
    fn a_copy_contains_the_edits_that_are_not_yet_on_disk() {
        // The point of the feature: it writes what is in the buffer, not
        // what is in the file it was opened from.
        let dir = tempfile::tempdir().unwrap();
        let (mut state, id, original) = dirty_saved_state(dir.path());
        let copy = dir.path().join("copy.txt");

        state.save_copy(id, &copy);

        assert_eq!(std::fs::read_to_string(&copy).unwrap(), "first\nsecond\n");
        assert_eq!(
            std::fs::read_to_string(&original).unwrap(),
            "first\n",
            "and the original on disk is untouched"
        );
    }

    #[test]
    fn a_copy_that_cannot_be_written_says_so_rather_than_reporting_success() {
        let dir = tempfile::tempdir().unwrap();
        let (mut state, id, _) = dirty_saved_state(dir.path());
        // A directory that does not exist: the write fails, and the user has
        // to be told, or they will believe a copy exists.
        let unwritable = dir.path().join("no-such-folder").join("copy.txt");

        assert_eq!(state.save_copy(id, &unwritable), SaveResult::Failed);
        assert!(
            state.error.is_some(),
            "a failed copy must reach the status bar"
        );
    }

    // --- statistics, stamps and go-to-line -------------------------------

    #[test]
    fn document_statistics_report_without_changing_the_document() {
        let mut state = AppState::new();
        state.edit("one two three\n\nfour\n".to_owned());
        let before = state.active_text();

        state.report_statistics();

        assert_eq!(state.active_text(), before, "a report must not edit");
        let message = state.error.expect("statistics in the status bar");
        assert!(message.contains("4 words"), "got {message}");
        assert!(message.contains("2 paragraphs"), "got {message}");
    }

    #[test]
    fn a_stamp_is_inserted_at_the_caret_and_marks_the_document_unsaved() {
        let mut state = AppState::new();
        state.editor_view = true;
        state.edit("ab".to_owned());
        state.active_editor_mut().unwrap().set_cursor(1);

        assert!(state.insert_stamp(0), "Date is the first stamp");

        let text = state.active_text();
        assert!(
            text.starts_with('a') && text.ends_with('b') && text.len() > 2,
            "the stamp landed at the caret; got {text:?}"
        );
        assert!(state.workspace.active().unwrap().is_dirty());
    }

    #[test]
    fn a_stamp_index_past_the_end_of_the_list_changes_nothing() {
        // The ids are a base plus an offset, so an id from a stale menu can
        // name a row that no longer exists.
        let mut state = AppState::new();
        state.editor_view = true;
        state.edit("text".to_owned());

        assert!(!state.insert_stamp(99));
        assert_eq!(state.active_text(), "text");
    }

    #[test]
    fn going_to_a_line_past_the_end_says_how_many_there_are() {
        // `Editor::go_to_line` moves somewhere sensible *and* reports that
        // the line did not exist, so the bar can do both.
        let mut state = AppState::new();
        state.editor_view = true;
        state.edit("one\ntwo\n".to_owned());

        assert!(state.go_to_line("99").is_some(), "the caret still moves");
        assert!(
            state.goto_status.contains("only"),
            "got {:?}",
            state.goto_status
        );
    }

    #[test]
    fn going_to_a_line_that_exists_reports_nothing() {
        let mut state = AppState::new();
        state.editor_view = true;
        state.edit("one\ntwo\nthree".to_owned());

        let moved = state.go_to_line("2").expect("moved");
        assert_eq!(moved.start, 4, "the start of line two");
        assert!(state.goto_status.is_empty(), "success is silent");
    }

    #[test]
    fn a_line_number_that_is_not_a_number_is_reported_rather_than_ignored() {
        // `input-type: number` is a hint to the keyboard, not a guarantee
        // from the backend, so the text still has to be parsed.
        let mut state = AppState::new();
        state.editor_view = true;
        state.edit("one\ntwo".to_owned());

        assert!(state.go_to_line("seven").is_none());
        assert_eq!(state.goto_status, "not a line number");
    }

    #[test]
    fn go_to_line_without_the_custom_view_says_why_instead_of_doing_nothing() {
        let mut state = AppState::new();
        state.edit("one\ntwo".to_owned());

        assert!(state.go_to_line("2").is_none());
        assert!(
            state.goto_status.contains("--editor-view"),
            "got {:?}",
            state.goto_status
        );
    }

    // --- tab context menu -------------------------------------------------

    #[test]
    fn the_tab_menu_acts_on_the_right_clicked_tab_not_the_active_one() {
        // The whole reason the target is stored: opening the menu and
        // choosing a row are two events, and between them the active tab is
        // still whatever it was.
        let mut state = AppState::new();
        let first = state.workspace.active_id().unwrap();
        state.new_document();
        let second = state.workspace.active_id().unwrap();
        assert_ne!(first, second);

        state.tab_context = Some(first);
        assert_eq!(state.tab_context_id(), Some(first));

        // With no menu open the rows fall back to the active tab.
        state.tab_context = None;
        assert_eq!(state.tab_context_id(), Some(second));
    }

    #[test]
    fn copy_full_path_is_unavailable_for_a_document_that_has_never_been_saved() {
        let mut state = AppState::new();
        state.new_document();

        let (others, has_path) = state.tab_context_shape();
        assert_eq!(others, 1, "one other tab is open");
        assert!(!has_path, "an untitled document has no path to copy");
    }

    #[test]
    fn each_find_option_reaches_its_own_field_and_no_other() {
        // Set one at a time, so a pair swapped in the constructor fails here
        // rather than compiling into a switch that changes the wrong thing.
        let case = find_query("x", true, false, false);
        assert!(case.case_sensitive);
        assert!(!case.whole_word, "case sensitivity must not imply words");
        assert!(!case.regex, "case sensitivity must not imply a pattern");

        let word = find_query("x", false, true, false);
        assert!(word.whole_word);
        assert!(!word.case_sensitive);
        assert!(!word.regex);

        let re = find_query("x", false, false, true);
        assert!(re.regex);
        assert!(!re.case_sensitive);
        assert!(!re.whole_word);
    }

    #[test]
    fn all_options_off_is_the_literal_query_the_find_box_used_to_build() {
        // The default the bar opens with. Everything searched before the
        // toggles existed went through `Query::literal`, so this is the
        // guarantee that adding them changed nothing for anyone who does not
        // touch them.
        assert_eq!(
            find_query("a.b", false, false, false),
            bp_search::Query::literal("a.b")
        );
    }

    #[test]
    fn the_regex_toggle_is_what_stops_a_pattern_being_escaped() {
        // The behaviour is `bp-search`'s and tested there; what this pins is
        // that the toggle reaches it at all. `a.b` matching `axb` is only
        // possible when `regex` arrived as true, so this fails if the flag is
        // dropped on the way through.
        let literal = find_query("a.b", false, false, false);
        let pattern = find_query("a.b", false, false, true);

        assert!(bp_search::find_all("axb", &literal).unwrap().is_empty());
        assert_eq!(bp_search::find_all("axb", &pattern).unwrap().len(), 1);
    }
}
