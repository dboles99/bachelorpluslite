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
    /// The visual row the surface is scrolled to.
    ///
    /// A line *and* a row within it, not a line alone: once a line can wrap,
    /// one taller than the window has to be scrollable through, and a
    /// line-only anchor could only jump over it.
    pub(crate) anchor: bp_editor::view::Anchor,
    /// How many rows fit in it. Slint measures and tells us.
    pub(crate) visible_rows: usize,
    /// How many characters fit across it, for wrapping. Slint measures this
    /// too -- it owns the font, and a column count derived from anything but
    /// the drawn advance would wrap in the wrong place.
    pub(crate) wrap_columns: usize,
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
    /// Passphrases for the encrypted documents open right now.
    ///
    /// Held for the session so that saving a `.bpadx` does not ask again on
    /// every Ctrl+S -- which would train the user to type it reflexively,
    /// which is worse than holding it. `Zeroizing` because these are wiped
    /// when a document closes rather than left in freed memory.
    ///
    /// A document with an entry here is encrypted; one without is not. That
    /// is the whole test, so there is no second flag to fall out of step.
    pub(crate) passphrases: HashMap<DocumentId, zeroize::Zeroizing<String>>,
    /// Privacy Mode: a session-wide override that can only tighten.
    ///
    /// On `AppState` rather than on a document, because that is what it is
    /// for -- one switch when you are about to share a screen, instead of
    /// auditing every open tab.
    pub(crate) privacy: bp_security::Privacy,
    /// What the passphrase bar is currently asking, if anything.
    pub(crate) ask: Option<crate::passphrase::Ask>,
    /// What the passphrase bar is reporting.
    pub(crate) passphrase_status: String,
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
            anchor: bp_editor::view::Anchor::default(),
            // Replaced by Slint's own measurement as soon as the surface has
            // a height; only Page Up before the first frame would see this.
            visible_rows: 30,
            // Replaced by Slint's measurement on the first layout pass, like
            // `visible_rows`. Only wrapping before the first frame sees this.
            wrap_columns: 80,
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
            passphrases: HashMap::new(),
            privacy: bp_security::Privacy::default(),
            ask: None,
            passphrase_status: String::new(),
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
            // The document's own profile, not a global setting: two tabs
            // open side by side can be governed differently, and the
            // stricter one must not be relaxed by the other being open.
            let recovery = doc.security().policy_under(self.privacy).recovery;
            // Its own passphrase, if it has one. A sealed journal is
            // encrypted with the document's key so that it can be recovered
            // at unlock time and never needs a prompt of its own (ADR-0022).
            let passphrase = self.passphrases.get(&id).map(|p| p.to_string());
            match self
                .journal
                .checkpoint(id.get(), &entry, recovery, passphrase.as_deref())
            {
                Ok(bp_history::Written::Yes) => {
                    if let Some(doc) = self.workspace.get_mut(id) {
                        doc.record_checkpoint(at);
                    }
                }
                // A profile doing what it was set to do is not news; one that
                // cannot be honoured is. `notice` decides which this was.
                Ok(bp_history::Written::Refused(refusal)) => {
                    if let Some(message) = refusal.notice() {
                        self.error = Some(message.to_owned());
                    }
                }
                Err(_) => {}
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

        // An encrypted document stays encrypted. The passphrase is held for
        // the session precisely so this does not ask again on every save --
        // a prompt on every Ctrl+S trains the user to type it without
        // reading, which is worse than holding it in memory.
        let bytes = match self.passphrases.get(&id) {
            None => bytes,
            Some(passphrase) => {
                match bp_crypto::seal(&bytes, passphrase, bp_crypto::SealOptions::default()) {
                    Ok(sealed) => sealed,
                    Err(e) => {
                        // Refused rather than falling back to plaintext. A
                        // save that silently wrote the document in clear
                        // would be the worst failure this program has.
                        self.error = Some(format!("not saved -- {e}"));
                        return SaveResult::Failed;
                    }
                }
            }
        };

        match atomic_write(&target, &bytes, SaveOptions::default()) {
            Ok(_) => {
                if let Some(doc) = self.workspace.get_mut(id) {
                    doc.set_path(target.clone());
                    // Only now, after a verified write, is the document clean.
                    doc.record_disk_save(now());
                }
                // Both forms of journal go once the work is on disk. The
                // sealed one is keyed by path rather than by session id, so
                // it has to be discarded by path.
                let _ = self.journal.discard(id.get());
                let _ = self.journal.discard_sealed_for(&target);
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

    /// The active document as the bytes it would be written to disk as.
    ///
    /// The encoded form rather than the buffer, because a digest and a
    /// signature are claims about a *file*: hashing the buffer would print a
    /// digest that `sha256sum` disagrees with for every CRLF document and
    /// every one with a BOM, and a user comparing the two would conclude
    /// their file had been tampered with.
    ///
    /// A one-off on a menu click, like `report_statistics` -- it copies the
    /// whole document twice over and must never move onto the typing path.
    pub(crate) fn active_bytes(&self) -> Result<Vec<u8>, String> {
        let Some(doc) = self.workspace.active() else {
            return Err("there is no document to read".to_owned());
        };
        let (encoding, line_ending) = (doc.encoding(), doc.line_ending());
        encode(&self.active_text(), encoding, line_ending)
    }

    /// Whether the active document differs from whatever is on disk.
    ///
    /// A never-saved document counts, which is why this is not simply
    /// `is_dirty` at the call site: "there is no file on disk" and "the file
    /// on disk is older than this" are the same fact as far as a digest or a
    /// signature over these bytes is concerned.
    fn active_differs_from_disk(&self) -> bool {
        self.workspace
            .active()
            .is_some_and(|doc| doc.is_dirty() || doc.path().is_none())
    }

    /// Scan the active document for credentials, reporting into the status
    /// bar and handing the findings back for a fuller listing.
    ///
    /// The findings are positions and classifications; `bp-secrets` refuses
    /// to carry the matched text and this returns nothing that would
    /// reintroduce it.
    pub(crate) fn scan_for_secrets(&mut self) -> Vec<bp_secrets::Finding> {
        let findings = bp_secrets::scan(&self.active_text());
        self.error = Some(secret_scan_summary(&findings));
        findings
    }

    /// Take a digest of the active document.
    ///
    /// Returns the body of the report to show, or `None` when the document
    /// cannot be encoded -- in which case the status bar already says why.
    /// The digest is not put in the status bar: it elides, and half a
    /// checksum compares equal to nothing.
    pub(crate) fn hash_active_document(&mut self) -> Option<String> {
        let bytes = match self.active_bytes() {
            Ok(bytes) => bytes,
            Err(e) => {
                self.error = Some(format!("cannot hash this document — {e}"));
                return None;
            }
        };
        let digest = bp_crypto::hash_document(&bytes, bp_crypto::HashAlgorithm::Sha256);
        self.error = Some(format!(
            "{} taken over {} bytes",
            digest.algorithm().name(),
            bytes.len()
        ));
        Some(hash_report(
            &digest,
            bytes.len(),
            self.active_differs_from_disk(),
        ))
    }

    /// Check a detached signature against the active document.
    ///
    /// The shell reads two files the user chose and calls `bp_crypto`; it
    /// owns no sidecar convention of its own. Where a `.sig` lives, how it is
    /// found and what else travels beside it are `bp-integrity`'s to decide,
    /// and a second answer here would be one the two could disagree about.
    pub(crate) fn verify_signature(&mut self, signature_path: &Path, key_path: &Path) {
        let bytes = match self.active_bytes() {
            Ok(bytes) => bytes,
            Err(e) => {
                self.error = Some(format!("cannot verify this document — {e}"));
                return;
            }
        };

        let signature = match read_signature(signature_path) {
            Ok(signature) => signature,
            Err(e) => {
                self.error = Some(format!("could not read the signature — {e}"));
                return;
            }
        };
        let key = match read_verifying_key(key_path) {
            Ok(key) => key,
            Err(e) => {
                self.error = Some(format!("could not read the key — {e}"));
                return;
            }
        };

        // A failure is reported as `SignError::DoesNotVerify` says it: the
        // bytes do not distinguish an altered document from an altered
        // signature from the wrong key. The one extra sentence the shell can
        // honestly add is about its own state -- unsaved edits mean these are
        // not the bytes anybody signed, and that is the likeliest explanation
        // by far.
        self.error = Some(match bp_crypto::verify_document(&key, &bytes, &signature) {
            Ok(()) => format!(
                "✓ signature verified — the holder of key {} approved these exact bytes",
                key.to_hex()
            ),
            Err(_) if self.active_differs_from_disk() => "✗ signature does not match — this                  document has unsaved changes, so these are not the bytes that were signed;                  save it and verify again"
                .to_owned(),
            Err(e) => format!("✗ {e}"),
        });
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

    /// The active document's security policy.
    ///
    /// Falls back to the default when there is no active document, so callers
    /// never have to choose a policy for themselves -- picking one at a call
    /// site is how a permissive default gets applied to a document that asked
    /// for something stricter.
    pub(crate) fn policy(&self) -> bp_security::Policy {
        // Through `policy_under`, always. Reading a document's own policy
        // directly is how a caller ends up outside Privacy Mode's reach.
        self.security().policy_under(self.privacy)
    }

    pub(crate) fn security(&self) -> bp_security::Security {
        self.workspace
            .active()
            .map_or_else(bp_security::Security::default, Document::security)
    }

    /// Give the active document a profile, and make the world match it.
    ///
    /// Setting a profile is not only a note on the document: tightening one
    /// obliges us to remove what the looser one already permitted. The
    /// journal is handled by the next checkpoint, which deletes a refused
    /// document's file; the clipboard is shared and has to be dealt with
    /// here.
    pub(crate) fn set_security(&mut self, security: bp_security::Security) {
        let Some(id) = self.workspace.active_id() else {
            return;
        };
        let Some(doc) = self.workspace.get_mut(id) else {
            return;
        };
        let previous = doc.set_security(security);
        if previous == security {
            return;
        }

        let policy = security.policy_under(self.privacy);
        if self.clips.enforce(policy.clipboard) {
            self.error = Some("clipboard history cleared to match this profile".to_owned());
        }

        // The journal for *this* document only. A refused checkpoint deletes
        // its file, so asking for one now is what turns the profile change
        // into an actual deletion rather than waiting up to the autosave
        // interval with plaintext still on disk.
        let entry = bp_history::Checkpoint {
            path: doc_path(&self.workspace, id),
            name: self.display_name(id),
            text: self.text_of(id).to_owned(),
            written_at: bp_history::now_unix(),
        };
        let passphrase = self.passphrases.get(&id).map(|p| p.to_string());
        if let Ok(bp_history::Written::Refused(refusal)) =
            self.journal
                .checkpoint(id.get(), &entry, policy.recovery, passphrase.as_deref())
            && let Some(message) = refusal.notice()
        {
            self.error = Some(message.to_owned());
        }
    }

    /// Whether `id` is an encrypted document.
    pub(crate) fn is_encrypted(&self, id: DocumentId) -> bool {
        self.passphrases.contains_key(&id)
    }

    /// Begin opening `path`, asking for a passphrase if it is encrypted.
    ///
    /// Read as bytes rather than through `bp_files::load`, which decodes text
    /// and would mangle ciphertext on the way in.
    ///
    /// An encrypted file does **not** become a tab until it is unlocked. A tab
    /// nobody can read is worse than no tab: it looks like an empty document,
    /// and saving it would write emptiness over the real one.
    pub(crate) fn open_maybe_encrypted(&mut self, path: PathBuf) -> bool {
        match std::fs::read(&path) {
            Ok(bytes) if bp_crypto::is_bpadx(&bytes) => {
                self.ask = Some(crate::passphrase::Ask::Unlock(path));
                self.passphrase_status.clear();
                true
            }
            // Not encrypted, or unreadable -- either way `open` handles it and
            // reports properly.
            _ => {
                self.open(path);
                false
            }
        }
    }

    /// Answer whatever the passphrase bar was asking.
    ///
    /// Returns whether the bar should stay open: a wrong passphrase keeps it
    /// up with a message, because closing it would make a typo look like a
    /// refusal to open the file at all.
    pub(crate) fn answer_passphrase(&mut self, entered: &str) -> bool {
        use crate::passphrase::Ask;
        let Some(ask) = self.ask.take() else {
            return false;
        };

        match ask {
            Ask::Unlock(path) => self.unlock(path, entered),
            Ask::Set { id, target } => {
                if entered.is_empty() {
                    // `bp_crypto::seal` refuses this too, but saying so here
                    // means the user is told before typing it twice.
                    self.passphrase_status = "a passphrase is required".to_owned();
                    self.ask = Some(Ask::Set { id, target });
                    return true;
                }
                self.passphrase_status.clear();
                self.ask = Some(Ask::Confirm {
                    id,
                    target,
                    first: zeroize::Zeroizing::new(entered.to_owned()),
                });
                true
            }
            Ask::Confirm { id, target, first } => {
                if first.as_str() != entered {
                    // Back to the first question, not the second. Asking to
                    // confirm again would compare against a passphrase the
                    // user may have already decided was the mistake.
                    self.passphrase_status = "those did not match -- start again".to_owned();
                    self.ask = Some(Ask::Set { id, target });
                    return true;
                }
                self.encrypt(id, &target, &first)
            }
        }
    }

    /// Decrypt `path` and open it as a tab.
    fn unlock(&mut self, path: PathBuf, passphrase: &str) -> bool {
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) => {
                self.error = Some(e.to_string());
                return false;
            }
        };

        match bp_crypto::open(&bytes, passphrase) {
            Ok(plain) => {
                // Lossy rather than refusing: a document that decrypted and
                // authenticated is the user's own text, and refusing to show
                // it because one byte is not UTF-8 would strand it inside a
                // file only this program can open.
                let text = String::from_utf8_lossy(&plain).into_owned();
                let id = self.workspace.open_path(path.clone(), now());

                // Unsaved work from a crashed session, if there is any. This
                // is the only moment it can be read: the journal is sealed
                // with this passphrase, which nothing had until now. Recovery
                // therefore never needs a prompt of its own -- a program
                // asking for a passphrase unprompted is the habit that makes
                // phishing work.
                let recovered = self.journal.sealed_pending(&path, passphrase);
                let text = match &recovered {
                    Some(checkpoint) => checkpoint.text.clone(),
                    None => text,
                };

                self.editors.insert(id, bp_editor::Editor::new(&text));
                self.passphrases
                    .insert(id, zeroize::Zeroizing::new(passphrase.to_owned()));
                self.mark_in_step(id, &path);
                self.passphrase_status.clear();

                if recovered.is_some() {
                    // Marked modified, because what is on screen is *not*
                    // what is in the file. Leaving it clean would let the
                    // user close the tab and lose the recovered work without
                    // being asked.
                    if let Some(doc) = self.workspace.get_mut(id) {
                        doc.mark_modified();
                    }
                    self.error = Some("recovered unsaved work from a previous session".to_owned());
                } else {
                    self.error = None;
                }
                false
            }
            Err(e) => {
                // The bar stays up. A wrong passphrase is the expected
                // failure, and it is indistinguishable from a damaged file --
                // see `bp_crypto`'s docs for why nothing can do better.
                self.passphrase_status = e.to_string();
                self.ask = Some(crate::passphrase::Ask::Unlock(path));
                true
            }
        }
    }

    /// Seal `id`'s text to `target` and let the tab adopt it.
    fn encrypt(&mut self, id: DocumentId, target: &Path, passphrase: &str) -> bool {
        let sealed = match bp_crypto::seal(
            self.text_of(id).as_bytes(),
            passphrase,
            bp_crypto::SealOptions::default(),
        ) {
            Ok(sealed) => sealed,
            Err(e) => {
                self.passphrase_status = e.to_string();
                return true;
            }
        };

        match atomic_write(target, &sealed, SaveOptions::default()) {
            Ok(_) => {
                // The tab adopts the encrypted file, so later saves stay
                // encrypted. Recording the passphrase is what makes the
                // document encrypted as far as the rest of the shell is
                // concerned -- there is no second flag.
                self.passphrases
                    .insert(id, zeroize::Zeroizing::new(passphrase.to_owned()));
                if let Some(doc) = self.workspace.get_mut(id) {
                    doc.set_path(target.to_path_buf());
                    doc.record_disk_save(now());
                }
                self.mark_in_step(id, target);
                self.passphrase_status.clear();
                self.error = Some(format!("encrypted to {}", target.display()));
                false
            }
            Err(e) => {
                self.passphrase_status = e.to_string();
                true
            }
        }
    }

    /// Turn Privacy Mode on or off, and make the world match.
    ///
    /// Switching it on has to *act*, not merely be recorded: a mode that only
    /// governed future writes would leave the clipboard history and the
    /// journals gathered a moment ago exactly where they were, which is the
    /// opposite of what somebody switching it on wants.
    pub(crate) fn set_privacy(&mut self, privacy: bp_security::Privacy) {
        if self.privacy == privacy {
            return;
        }
        self.privacy = privacy;
        if !privacy.is_on() {
            // Turning it off restores each document's own profile. Nothing to
            // clean up -- the clamp only ever removed permissions.
            return;
        }

        let cleared = self.clips.enforce(self.policy().clipboard);
        // Every document, not only the active one: the mode is session-wide,
        // and a journal left behind for a background tab is exactly what it
        // was switched on to prevent.
        self.checkpoint_all();
        self.error = Some(if cleared {
            "Privacy Mode on -- clipboard history cleared and journals removed".to_owned()
        } else {
            "Privacy Mode on".to_owned()
        });
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

    /// Forget an encrypted document's passphrase.
    ///
    /// Called when a document closes. Holding it for the session is a
    /// deliberate trade; holding it past the document's life is just a leak.
    pub(crate) fn forget_passphrase(&mut self, id: DocumentId) {
        self.passphrases.remove(&id);
    }

    pub(crate) fn close(&mut self, id: DocumentId) {
        self.error = None;
        if self.workspace.close(id).is_some() {
            self.editors.remove(&id);
            // Holding a passphrase for the session is a deliberate trade;
            // holding it past the document's life is just a leak.
            self.forget_passphrase(id);
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
        let anchor = self.anchor;
        let Some(editor) = self.active_editor() else {
            return;
        };
        self.anchor = bp_editor::view::reveal_row(
            editor.buffer(),
            anchor,
            rows,
            editor.cursor(),
            editor.layout(),
        );
    }

    /// The layout the surface is drawing with.
    ///
    /// Taken from the active editor rather than assembled here, so the
    /// numbers that decide where the caret goes and the numbers that decide
    /// what is drawn are the same ones.
    pub(crate) fn layout(&self) -> bp_editor::view::Layout {
        self.active_editor().map_or_else(
            || bp_editor::view::Layout {
                wrap: bp_editor::wrap::Wrap::OFF,
                tab_width: self.indent.width,
            },
            bp_editor::Editor::layout,
        )
    }

    /// Tell the active editor how the surface is currently wrapping.
    ///
    /// Called before anything that depends on the layout, because the wrap
    /// width follows the window and the editor cannot see the window.
    pub(crate) fn sync_wrap(&mut self) {
        let wrap = if self.wrap_text && self.editor_view {
            bp_editor::wrap::Wrap::at(self.wrap_columns.max(1))
        } else {
            bp_editor::wrap::Wrap::OFF
        };
        let indent = self.indent;
        if let Some(editor) = self.active_editor_mut() {
            editor.set_indent(indent);
            editor.set_wrap(wrap);
        }
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

/// How many findings the status bar names before it stops counting them out.
///
/// The bar elides, so a list longer than this is a list whose tail nobody
/// reads. The full set goes in the report instead.
const SECRETS_IN_SUMMARY: usize = 3;

/// One line for the status bar describing a secret scan.
///
/// Names the kinds and where they are, and never the matched text. That is
/// not a nicety: `bp_secrets::Finding` deliberately holds no secret at all,
/// so that no `Debug` print or log line can leak one, and a status bar that
/// quoted the credential would put it back on screen -- and into the
/// screenshot of the bug it was attached to.
pub(crate) fn secret_scan_summary(findings: &[bp_secrets::Finding]) -> String {
    if findings.is_empty() {
        return "✓ no credentials found".to_owned();
    }
    let named: Vec<String> = findings
        .iter()
        .take(SECRETS_IN_SUMMARY)
        .map(|f| format!("{} at line {} col {}", f.kind.label(), f.line, f.column))
        .collect();
    let rest = findings.len() - named.len();
    format!(
        "⚠ {} possible credential{} — {}{}",
        findings.len(),
        if findings.len() == 1 { "" } else { "s" },
        named.join(", "),
        if rest == 0 {
            String::new()
        } else {
            format!(", and {rest} more")
        }
    )
}

/// The full listing of a scan, one finding per line.
///
/// Line and column first, because the reason to read this is to go and look
/// at each one, and `bp-secrets` reports 1-based positions for exactly that.
pub(crate) fn secret_scan_report(findings: &[bp_secrets::Finding]) -> String {
    findings
        .iter()
        .map(|f| {
            format!(
                "line {}, column {} — {} ({})",
                f.line,
                f.column,
                f.kind.label(),
                confidence_label(f.confidence)
            )
        })
        .collect::<Vec<_>>()
        .join(
            "
",
        )
}

/// How sure the scanner is, in the user's terms.
///
/// Presentation, so it lives here rather than in `bp-secrets`, which is pure
/// and has no opinion about wording. Low is spelled out rather than left as a
/// bare word: a row reading "low" beside a real password reads as "ignore
/// me", and the whole point of the tier is that the user decides.
const fn confidence_label(confidence: bp_secrets::Confidence) -> &'static str {
    match confidence {
        bp_secrets::Confidence::High => "high confidence",
        bp_secrets::Confidence::Medium => "medium confidence",
        bp_secrets::Confidence::Low => "low confidence, worth a look",
    }
}

/// The report shown for a document digest.
///
/// Both spellings, because they are read by different people in different
/// ways: the grouped one is for a human comparing two screens or reading it
/// down a telephone, the unbroken one is for pasting beside what `sha256sum`
/// or `certutil` printed. `bp_crypto` offers both and deliberately refuses to
/// choose between them.
fn hash_report(digest: &bp_crypto::DocumentHash, bytes: usize, unsaved: bool) -> String {
    let mut report = format!(
        "{}

{}

Taken over the {bytes} bytes this document would be written as,          so it matches the file once it is saved.",
        digest.to_display(),
        digest.to_hex(),
    );
    if unsaved {
        // The one way this digest can be honestly wrong about the file, and
        // it is invisible from the dialog otherwise.
        report.push_str(
            "

This document has unsaved changes, so it is not yet the digest of              anything on disk.",
        );
    }
    report
}

/// Read a detached signature from the file the user chose.
fn read_signature(path: &Path) -> Result<bp_crypto::Signature, String> {
    let raw = std::fs::read(path).map_err(|e| e.to_string())?;
    bp_crypto::Signature::from_bytes(&raw).map_err(|e| e.to_string())
}

/// Read a verifying key from the file the user chose, in either spelling.
///
/// Thirty-two raw bytes is what `VerifyingKey::to_bytes` writes; hexadecimal
/// is what arrives when somebody pastes a key out of an email into a text
/// file, which is how a public key actually travels between people. The raw
/// form is tried first because a 32-byte file cannot also be 64 hex digits,
/// so the two can never be confused for one another.
fn read_verifying_key(path: &Path) -> Result<bp_crypto::VerifyingKey, String> {
    let raw = std::fs::read(path).map_err(|e| e.to_string())?;
    if let Ok(key) = bp_crypto::VerifyingKey::from_bytes(&raw) {
        return Ok(key);
    }
    let text = std::str::from_utf8(&raw)
        .map_err(|_| "this is neither 32 raw bytes nor a key in hexadecimal".to_owned())?;
    bp_crypto::VerifyingKey::from_hex(text).map_err(|e| e.to_string())
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

/// A document's path, if it has one.
///
/// A free function so `set_security` can read it without holding a borrow of
/// the workspace across the mutable one it already has.
fn doc_path(workspace: &Workspace, id: DocumentId) -> Option<PathBuf> {
    workspace
        .get(id)
        .and_then(Document::path)
        .map(Path::to_path_buf)
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
            action::SCAN_SECRETS,
            action::HASH_DOCUMENT,
            action::SIGN_DOCUMENT,
            action::VERIFY_SIGNATURE,
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

    // --- security profiles ------------------------------------------------

    #[test]
    fn a_new_document_starts_on_the_default_profile() {
        let state = AppState::new();
        assert_eq!(state.security(), bp_security::Security::default());
        assert_eq!(state.policy().recovery, bp_security::Recovery::Plaintext);
    }

    #[test]
    fn the_profile_belongs_to_the_document_not_the_application() {
        // Two tabs open side by side can be governed differently, and the
        // stricter one must not be relaxed by the other being open.
        let mut state = AppState::new();
        state.set_security(bp_security::Security::Named(
            bp_security::Profile::Confidential,
        ));
        let strict = state.workspace.active_id().unwrap();

        state.new_document();
        assert_eq!(
            state.security(),
            bp_security::Security::default(),
            "a new document is not governed by the last one"
        );

        state.workspace.set_active(strict);
        assert_eq!(
            state.policy().clipboard,
            bp_security::Clipboard::Disabled,
            "switching back restores the stricter document's policy"
        );
    }

    #[test]
    fn tightening_a_profile_clears_the_clipboard_history() {
        // The history is one list for the whole application, so it is the
        // only dependant the profile change has to deal with directly -- the
        // journal is handled by the checkpoint the change triggers.
        let mut state = AppState::new();
        state
            .clips
            .push("copied earlier", bp_security::Clipboard::InMemory);
        assert!(!state.clips.is_empty());

        state.set_security(bp_security::Security::Named(
            bp_security::Profile::Confidential,
        ));

        assert!(
            state.clips.is_empty(),
            "a profile that forbids a clipboard history must not leave one \
             gathered under a looser profile sitting in the menu"
        );
        assert!(
            state.error.is_some(),
            "and the user is told why it vanished"
        );
    }

    #[test]
    fn setting_the_same_profile_again_changes_nothing() {
        // Refresh rebuilds menus constantly; a no-op that cleared the
        // clipboard would empty it whenever the menu was opened.
        let mut state = AppState::new();
        state.clips.push("keep", bp_security::Clipboard::InMemory);

        state.set_security(bp_security::Security::default());

        assert_eq!(state.clips.entries().len(), 1);
    }

    #[test]
    fn a_plaintext_document_under_a_strict_profile_is_told_how_to_fix_it() {
        // ADR-0020: fail loudly rather than degrade. The journal is sealed
        // with the document's own passphrase, so a plaintext document has no
        // key -- and the message says what turns recovery back on rather than
        // only that it is off.
        let mut state = AppState::new();
        state.set_security(bp_security::Security::Named(bp_security::Profile::Private));

        let message = state.error.expect("a capability gap reaches the user");
        assert!(
            message.contains("encrypt") || message.contains("clipboard"),
            "got {message}"
        );
    }

    #[test]
    fn an_encrypted_document_gets_an_encrypted_journal_and_recovers_at_unlock() {
        // The whole point of keying the sealed journal by path: a crashed
        // session's work comes back when the document is unlocked, and never
        // needs a prompt of its own.
        let recovery = tempfile::tempdir().unwrap();
        let docs = tempfile::tempdir().unwrap();
        let target = docs.path().join("notes.bpadx");

        let mut before = AppState::new();
        before.journal = bp_history::Journal::new(recovery.path().to_path_buf());
        before.edit("saved text".to_owned());
        let id = before.workspace.active_id().unwrap();
        before.ask = Some(crate::passphrase::Ask::Set {
            id,
            target: target.clone(),
        });
        before.answer_passphrase("hunter2");
        before.answer_passphrase("hunter2");

        // Work done after the save, then the session "crashes".
        before.set_security(bp_security::Security::Named(bp_security::Profile::Private));
        before.edit("saved text plus unsaved".to_owned());
        before.checkpoint_all();

        // Nothing readable without the passphrase, but something is there.
        assert!(before.journal.pending().is_empty());
        assert_eq!(before.journal.sealed_count(), 1);

        let mut after = AppState::new();
        after.journal = bp_history::Journal::new(recovery.path().to_path_buf());
        after.open_maybe_encrypted(target);
        after.answer_passphrase("hunter2");

        assert_eq!(
            after.active_text(),
            "saved text plus unsaved",
            "the unsaved work did not come back at unlock"
        );
        assert!(
            after.workspace.active().unwrap().is_dirty(),
            "recovered work is not on disk, so the tab must be dirty or the \
             user can close it and lose it without being asked"
        );
    }

    #[test]
    fn saving_an_encrypted_document_discards_its_sealed_journal() {
        // The work is on disk now. A journal left behind would offer to
        // "recover" a stale copy at the next unlock.
        let recovery = tempfile::tempdir().unwrap();
        let docs = tempfile::tempdir().unwrap();
        let target = docs.path().join("notes.bpadx");

        let mut state = AppState::new();
        state.journal = bp_history::Journal::new(recovery.path().to_path_buf());
        state.edit("first".to_owned());
        let id = state.workspace.active_id().unwrap();
        state.ask = Some(crate::passphrase::Ask::Set {
            id,
            target: target.clone(),
        });
        state.answer_passphrase("hunter2");
        state.answer_passphrase("hunter2");
        state.set_security(bp_security::Security::Named(bp_security::Profile::Private));

        state.edit("first and second".to_owned());
        state.checkpoint_all();
        assert_eq!(state.journal.sealed_count(), 1);

        assert_eq!(state.save_document(id, None), SaveResult::Saved);

        assert_eq!(
            state.journal.sealed_count(),
            0,
            "a stale sealed journal would be offered at the next unlock"
        );
    }

    #[test]
    fn a_confidential_document_is_not_checkpointed_in_plaintext() {
        // The end-to-end version of the journal's own test, through the
        // autosave path the application actually uses.
        let dir = tempfile::tempdir().unwrap();
        let mut state = AppState::new();
        state.journal = bp_history::Journal::new(dir.path().to_path_buf());
        state.edit("sensitive text".to_owned());

        state.checkpoint_all();
        assert_eq!(state.journal.pending().len(), 1, "Standard journals it");

        state.set_security(bp_security::Security::Named(
            bp_security::Profile::Confidential,
        ));
        state.checkpoint_all();

        assert!(
            state.journal.pending().is_empty(),
            "the earlier plaintext must be gone, not merely not added to"
        );
    }

    // --- privacy mode -----------------------------------------------------

    #[test]
    fn privacy_mode_clears_the_clipboard_and_removes_journals() {
        // Switching it on has to *act*, not merely be recorded. A mode that
        // only governed future writes would leave everything gathered a
        // moment ago exactly where it was, which is the opposite of what
        // somebody switching it on wants.
        let dir = tempfile::tempdir().unwrap();
        let mut state = AppState::new();
        state.journal = bp_history::Journal::new(dir.path().to_path_buf());
        state.edit("unsaved work".to_owned());
        state
            .clips
            .push("copied earlier", bp_security::Clipboard::InMemory);
        state.checkpoint_all();
        assert_eq!(state.journal.pending().len(), 1);
        assert!(!state.clips.is_empty());

        state.set_privacy(bp_security::Privacy::On);

        assert!(state.clips.is_empty(), "clipboard history survived");
        assert!(state.journal.pending().is_empty(), "the journal survived");
    }

    #[test]
    fn privacy_mode_acts_on_every_tab_not_only_the_active_one() {
        // It is session-wide. A journal left behind for a background tab is
        // exactly what it was switched on to prevent.
        let dir = tempfile::tempdir().unwrap();
        let mut state = AppState::new();
        state.journal = bp_history::Journal::new(dir.path().to_path_buf());
        state.edit("first tab".to_owned());
        state.new_document();
        state.edit("second tab".to_owned());
        state.checkpoint_all();
        assert_eq!(state.journal.pending().len(), 2);

        state.set_privacy(bp_security::Privacy::On);

        assert!(state.journal.pending().is_empty());
    }

    #[test]
    fn privacy_mode_does_not_relax_a_document_that_was_already_stricter() {
        // The clamp takes the stricter of each axis rather than substituting
        // a policy. Substituting would drag a Maximum document *down* to
        // whatever Privacy Mode specified.
        let mut state = AppState::new();
        state.set_security(bp_security::Security::Named(bp_security::Profile::Maximum));
        let before = state.policy();

        state.set_privacy(bp_security::Privacy::On);

        assert_eq!(state.policy(), before);
    }

    #[test]
    fn turning_privacy_mode_off_restores_each_documents_own_profile() {
        let mut state = AppState::new();
        let standard = state.policy();

        state.set_privacy(bp_security::Privacy::On);
        assert_ne!(state.policy(), standard);

        state.set_privacy(bp_security::Privacy::Off);
        assert_eq!(state.policy(), standard);
    }

    // --- encryption -------------------------------------------------------

    /// Drive the passphrase bar the way the shell does: submit, and be told
    /// whether it stays open.
    fn answer(state: &mut AppState, entered: &str) -> bool {
        state.answer_passphrase(entered)
    }

    #[test]
    fn encrypting_a_document_asks_twice_and_writes_only_on_a_match() {
        // A typo when *setting* a passphrase costs the document permanently,
        // which is why this is the one place the bar asks twice.
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("notes.bpadx");
        let mut state = AppState::new();
        state.edit("secret notes".to_owned());
        let id = state.workspace.active_id().unwrap();

        state.ask = Some(crate::passphrase::Ask::Set {
            id,
            target: target.clone(),
        });

        assert!(answer(&mut state, "hunter2"), "asks again to confirm");
        assert!(!target.exists(), "nothing is written after only one entry");

        assert!(!answer(&mut state, "hunter2"), "the bar closes on a match");
        assert!(target.exists(), "and the document is written");
        assert!(state.is_encrypted(id));
    }

    #[test]
    fn a_mismatched_confirmation_starts_again_rather_than_writing() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("notes.bpadx");
        let mut state = AppState::new();
        state.edit("secret".to_owned());
        let id = state.workspace.active_id().unwrap();
        state.ask = Some(crate::passphrase::Ask::Set {
            id,
            target: target.clone(),
        });

        answer(&mut state, "hunter2");
        assert!(answer(&mut state, "hunter3"), "the bar stays up");

        assert!(!target.exists(), "a mismatch must not write anything");
        assert!(!state.is_encrypted(id));
        assert!(
            matches!(state.ask, Some(crate::passphrase::Ask::Set { .. })),
            "back to the first question, not the second -- confirming again \
             would compare against the entry the user already thinks is wrong"
        );
    }

    #[test]
    fn an_empty_passphrase_is_refused_before_it_is_typed_twice() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = AppState::new();
        state.edit("secret".to_owned());
        let id = state.workspace.active_id().unwrap();
        state.ask = Some(crate::passphrase::Ask::Set {
            id,
            target: dir.path().join("notes.bpadx"),
        });

        assert!(answer(&mut state, ""), "the bar stays up");
        assert!(
            matches!(state.ask, Some(crate::passphrase::Ask::Set { .. })),
            "still on the first question"
        );
        assert!(!state.passphrase_status.is_empty(), "and says why");
    }

    #[test]
    fn an_encrypted_document_round_trips_through_the_shell() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("notes.bpadx");

        let mut writer = AppState::new();
        writer.edit("the quick brown fox".to_owned());
        let id = writer.workspace.active_id().unwrap();
        writer.ask = Some(crate::passphrase::Ask::Set {
            id,
            target: target.clone(),
        });
        answer(&mut writer, "hunter2");
        answer(&mut writer, "hunter2");

        // The file on disk must not be readable as the text that went in.
        let raw = std::fs::read(&target).unwrap();
        assert!(bp_crypto::is_bpadx(&raw));
        assert!(
            !raw.windows(3).any(|w| w == b"fox"),
            "the plaintext is in the file"
        );

        let mut reader = AppState::new();
        assert!(
            reader.open_maybe_encrypted(target.clone()),
            "an encrypted file asks for a passphrase"
        );
        assert!(!answer(&mut reader, "hunter2"), "and then opens");

        assert_eq!(reader.active_text(), "the quick brown fox");
    }

    #[test]
    fn an_encrypted_file_does_not_become_a_tab_until_it_is_unlocked() {
        // A tab nobody can read looks like an empty document, and saving it
        // would write emptiness over the real one.
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("notes.bpadx");
        let sealed =
            bp_crypto::seal(b"secret", "hunter2", bp_crypto::SealOptions::default()).unwrap();
        std::fs::write(&target, sealed).unwrap();

        let mut state = AppState::new();
        let before = state.workspace.len();
        assert!(state.open_maybe_encrypted(target));

        assert_eq!(state.workspace.len(), before, "no tab was opened");
    }

    #[test]
    fn a_wrong_passphrase_keeps_the_bar_up_and_opens_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("notes.bpadx");
        let sealed =
            bp_crypto::seal(b"secret", "hunter2", bp_crypto::SealOptions::default()).unwrap();
        std::fs::write(&target, sealed).unwrap();

        let mut state = AppState::new();
        state.open_maybe_encrypted(target);
        let before = state.workspace.len();

        assert!(answer(&mut state, "wrong"), "the bar stays up for a retry");
        assert_eq!(state.workspace.len(), before);
        assert!(
            !state.passphrase_status.is_empty(),
            "a wrong passphrase has to say something, or it looks like the \
             file simply refused to open"
        );
    }

    #[test]
    fn saving_an_encrypted_document_keeps_it_encrypted() {
        // The failure this prevents is the worst one available: a later
        // Ctrl+S quietly writing the document in clear.
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("notes.bpadx");
        let mut state = AppState::new();
        state.edit("first".to_owned());
        let id = state.workspace.active_id().unwrap();
        state.ask = Some(crate::passphrase::Ask::Set {
            id,
            target: target.clone(),
        });
        answer(&mut state, "hunter2");
        answer(&mut state, "hunter2");

        state.edit("first and second".to_owned());
        assert_eq!(state.save_document(id, None), SaveResult::Saved);

        let raw = std::fs::read(&target).unwrap();
        assert!(bp_crypto::is_bpadx(&raw), "the save wrote plaintext");
        let opened = bp_crypto::open(&raw, "hunter2").unwrap();
        assert_eq!(
            String::from_utf8_lossy(&opened),
            "first and second",
            "the edit did not reach the encrypted file"
        );
    }

    #[test]
    fn a_plain_document_is_opened_without_asking_for_anything() {
        let dir = tempfile::tempdir().unwrap();
        let plain = dir.path().join("notes.txt");
        std::fs::write(&plain, "ordinary text").unwrap();

        let mut state = AppState::new();
        assert!(
            !state.open_maybe_encrypted(plain),
            "a plain file must not prompt"
        );
        assert_eq!(state.active_text(), "ordinary text");
    }

    #[test]
    fn closing_an_encrypted_document_forgets_its_passphrase() {
        // Holding it for the session is a deliberate trade; holding it past
        // the document's life is just a leak.
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("notes.bpadx");
        let mut state = AppState::new();
        state.edit("secret".to_owned());
        let id = state.workspace.active_id().unwrap();
        state.ask = Some(crate::passphrase::Ask::Set {
            id,
            target: target.clone(),
        });
        answer(&mut state, "hunter2");
        answer(&mut state, "hunter2");
        assert!(state.is_encrypted(id));

        state.close(id);

        assert!(!state.is_encrypted(id), "the passphrase outlived the tab");
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

    // --- Scan for Secrets --------------------------------------------------

    /// A credential the scanner actually recognises, and a document with one
    /// in it, so a test can assert the shell never repeats it back.
    ///
    /// Not one of the vendors' published `EXAMPLE` keys: `bp-secrets`
    /// deliberately refuses those, on the grounds that documentation is not a
    /// leak, so a fixture built from one would test nothing.
    const LEAKED: &str = "AKIA3G7QVHBRN2WPKZ5F";
    const LEAKY: &str = "notes\naws_access_key_id = AKIA3G7QVHBRN2WPKZ5F\ndone\n";

    #[test]
    fn a_clean_document_is_told_it_is_clean_rather_than_told_nothing() {
        // Silence after a scan is indistinguishable from a scan that did not
        // run, which is the one thing a security check must never be.
        let mut state = AppState::new();
        state.edit("nothing to see here\n".to_owned());
        let findings = state.scan_for_secrets();

        assert!(findings.is_empty());
        assert_eq!(
            state.error.as_deref(),
            Some("\u{2713} no credentials found")
        );
    }

    #[test]
    fn a_scan_reports_the_count_the_kind_and_where_it_is() {
        let mut state = AppState::new();
        state.edit(LEAKY.to_owned());
        let findings = state.scan_for_secrets();

        assert_eq!(findings.len(), 1);
        let notice = state.error.clone().expect("a status message");
        assert!(
            notice.contains('1'),
            "the count belongs in it; got {notice}"
        );
        assert!(
            notice.contains("AWS access key ID"),
            "the kind belongs in it; got {notice}"
        );
        assert!(
            notice.contains("line 2"),
            "where it is is the actionable part; got {notice}"
        );
    }

    #[test]
    fn neither_the_summary_nor_the_report_ever_repeats_the_secret() {
        // The whole design of `bp-secrets`: a `Finding` carries a position and
        // a classification and deliberately not the matched text, so that no
        // print of one can leak a credential. The shell is the last place that
        // could undo that, by reaching back into the document to quote it.
        let findings = bp_secrets::scan(LEAKY);
        assert!(!findings.is_empty(), "the fixture must actually be found");

        for line in [
            secret_scan_summary(&findings),
            secret_scan_report(&findings),
        ] {
            assert!(
                !line.contains(LEAKED),
                "the credential is back on screen: {line}"
            );
        }
    }

    #[test]
    fn a_long_scan_names_the_first_few_and_says_how_many_it_did_not() {
        // The status bar elides, so a list longer than the bar is a list whose
        // tail is invisible. Saying "and 2 more" is what stops the invisible
        // part reading as "there were only three".
        let text: String = (0..5)
            .map(|i| format!("aws_access_key_id{i} = {LEAKED}\n"))
            .collect();
        let findings = bp_secrets::scan(&text);
        assert_eq!(findings.len(), 5, "the fixture should yield five");

        let summary = secret_scan_summary(&findings);
        assert!(summary.contains('5'), "got {summary}");
        assert!(summary.contains("and 2 more"), "got {summary}");
    }

    #[test]
    fn the_full_report_has_a_line_per_finding_with_its_position() {
        let findings = bp_secrets::scan(LEAKY);
        let report = secret_scan_report(&findings);
        assert_eq!(report.lines().count(), findings.len());
        assert!(report.contains("line 2, column"), "got {report}");
    }

    // --- Hash, sign, verify ------------------------------------------------

    #[test]
    fn the_digest_is_taken_over_the_bytes_the_document_would_be_written_as() {
        // Not over the buffer. The BOM is stripped on load and written back on
        // save, so a digest of the buffer is one `sha256sum` disagrees with --
        // and a user comparing the two would conclude their file had been
        // altered when nothing had touched it.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bom.txt");
        std::fs::write(&path, b"\xEF\xBB\xBFone\ntwo\n").unwrap();

        let mut state = AppState::new();
        state.open(path.clone());
        assert_eq!(
            state.active_text(),
            "one\ntwo\n",
            "the BOM belongs on disk, not in the buffer"
        );
        let report = state.hash_active_document().expect("a digest");

        let on_disk = std::fs::read(&path).unwrap();
        let expected = bp_crypto::hash_document(&on_disk, bp_crypto::HashAlgorithm::Sha256);
        assert!(
            report.contains(&expected.to_hex()),
            "the digest should match the file on disk; got {report}"
        );
        let of_the_buffer = bp_crypto::hash_document(
            state.active_text().as_bytes(),
            bp_crypto::HashAlgorithm::Sha256,
        );
        assert!(
            !report.contains(&of_the_buffer.to_hex()),
            "hashing the buffer would leave the BOM out and give the wrong answer"
        );
    }

    #[test]
    fn the_digest_is_offered_grouped_and_unbroken_and_names_its_algorithm() {
        // Two spellings for two readers: grouped for a person comparing two
        // screens or reading it aloud, unbroken for pasting beside what
        // `sha256sum` printed.
        let mut state = AppState::new();
        state.edit("hello".to_owned());
        let report = state.hash_active_document().expect("a digest");

        let digest = bp_crypto::hash_document(b"hello", bp_crypto::HashAlgorithm::Sha256);
        assert!(report.contains(&digest.to_display()), "got {report}");
        assert!(report.contains(&digest.to_hex()), "got {report}");
        assert!(
            state
                .error
                .as_deref()
                .is_some_and(|e| e.contains("SHA-256")),
            "the status bar should name the algorithm; got {:?}",
            state.error
        );
    }

    #[test]
    fn a_digest_of_an_unsaved_document_says_it_is_not_the_file_on_disk() {
        let mut state = AppState::new();
        state.edit("draft".to_owned());
        let report = state.hash_active_document().expect("a digest");
        assert!(
            report.contains("unsaved changes"),
            "an unsaved digest that claims to be the file's is a lie; got {report}"
        );
    }

    #[test]
    fn a_document_that_cannot_be_encoded_says_so_instead_of_hashing_nothing() {
        // `encode` refuses UTF-16 rather than writing mojibake, and a digest
        // of an empty vector would be a perfectly plausible-looking wrong
        // answer.
        let mut state = AppState::new();
        state.edit("hello".to_owned());
        state.set_encoding(Encoding::Utf16Le);

        assert!(state.hash_active_document().is_none());
        assert!(
            state
                .error
                .as_deref()
                .is_some_and(|e| e.contains("cannot hash")),
            "got {:?}",
            state.error
        );
    }

    /// Write a signing identity's signature over `bytes`, plus its public
    /// key, and return the two paths -- the shape the Verify row expects.
    fn sign_to_files(dir: &std::path::Path, bytes: &[u8]) -> (PathBuf, PathBuf) {
        let key = bp_crypto::SigningKey::generate().expect("the OS random source");
        let signature = bp_crypto::sign_document(&key, bytes);

        let sig_path = dir.join("document.sig");
        let key_path = dir.join("document.pub");
        std::fs::write(&sig_path, signature.to_bytes()).unwrap();
        std::fs::write(&key_path, key.verifying_key().to_bytes()).unwrap();
        (sig_path, key_path)
    }

    #[test]
    fn a_signature_over_the_documents_own_bytes_verifies() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = AppState::new();
        state.edit("signed content\n".to_owned());
        let bytes = state.active_bytes().unwrap();
        let (sig, key) = sign_to_files(dir.path(), &bytes);

        state.verify_signature(&sig, &key);
        assert!(
            state
                .error
                .as_deref()
                .is_some_and(|e| e.starts_with('\u{2713}')),
            "got {:?}",
            state.error
        );
    }

    #[test]
    fn a_verified_signature_names_the_key_and_claims_nothing_about_whose_it_is() {
        // `bp-crypto` is explicit that it cannot say whose key it is, and the
        // status bar must not quietly promise what the library refuses to.
        let dir = tempfile::tempdir().unwrap();
        let mut state = AppState::new();
        state.edit("signed content\n".to_owned());
        let bytes = state.active_bytes().unwrap();
        let (sig, key_path) = sign_to_files(dir.path(), &bytes);
        let key = bp_crypto::VerifyingKey::from_bytes(&std::fs::read(&key_path).unwrap()).unwrap();

        state.verify_signature(&sig, &key_path);
        let notice = state.error.clone().unwrap();
        assert!(notice.contains(&key.to_hex()), "got {notice}");
        assert!(
            !notice.to_lowercase().contains("signed by the author"),
            "got {notice}"
        );
    }

    #[test]
    fn a_public_key_pasted_out_of_an_email_as_hexadecimal_is_accepted() {
        // The realistic way a public key travels. Refusing it would teach
        // users to retype keys by hand, which is how a wrong key gets used.
        let dir = tempfile::tempdir().unwrap();
        let mut state = AppState::new();
        state.edit("signed content\n".to_owned());
        let bytes = state.active_bytes().unwrap();
        let (sig, raw_key) = sign_to_files(dir.path(), &bytes);

        let hex_key = dir.path().join("key.txt");
        let key = bp_crypto::VerifyingKey::from_bytes(&std::fs::read(&raw_key).unwrap()).unwrap();
        // Wrapped across two lines, exactly as an email client would leave it.
        let text = key.to_hex();
        std::fs::write(&hex_key, format!("{}\n{}\n", &text[..32], &text[32..])).unwrap();

        state.verify_signature(&sig, &hex_key);
        assert!(
            state
                .error
                .as_deref()
                .is_some_and(|e| e.starts_with('\u{2713}')),
            "got {:?}",
            state.error
        );
    }

    #[test]
    fn a_signature_over_different_bytes_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let (sig, key) = sign_to_files(dir.path(), b"something else entirely");

        let mut state = AppState::new();
        state.edit("signed content\n".to_owned());
        state.verify_signature(&sig, &key);
        assert!(
            state
                .error
                .as_deref()
                .is_some_and(|e| e.starts_with('\u{2717}')),
            "got {:?}",
            state.error
        );
    }

    #[test]
    fn a_refusal_on_an_edited_document_says_the_edits_are_the_likely_reason() {
        // The only sentence the shell can honestly add to `DoesNotVerify`:
        // the library cannot tell an altered document from a wrong key, but
        // we do know our own buffer is not what is on disk.
        let dir = tempfile::tempdir().unwrap();
        let (mut state, _, _) = dirty_saved_state(dir.path());
        let (sig, key) = sign_to_files(dir.path(), b"first\n");

        state.verify_signature(&sig, &key);
        let notice = state.error.clone().unwrap();
        assert!(notice.contains("unsaved changes"), "got {notice}");
    }

    #[test]
    fn a_truncated_signature_file_is_reported_as_incomplete_not_as_a_mismatch() {
        // A filing accident and a tampered document are different problems,
        // and only one of them sends the user hunting for an attacker.
        let dir = tempfile::tempdir().unwrap();
        let mut state = AppState::new();
        state.edit("signed content\n".to_owned());
        let bytes = state.active_bytes().unwrap();
        let (sig, key) = sign_to_files(dir.path(), &bytes);
        std::fs::write(&sig, b"too short").unwrap();

        state.verify_signature(&sig, &key);
        let notice = state.error.clone().unwrap();
        assert!(notice.contains("signature"), "got {notice}");
        assert!(
            notice.contains("64 bytes"),
            "the length is the actionable part; got {notice}"
        );
    }

    #[test]
    fn a_key_file_that_is_not_a_key_says_which_of_the_two_files_was_wrong() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = AppState::new();
        state.edit("signed content\n".to_owned());
        let bytes = state.active_bytes().unwrap();
        let (sig, _) = sign_to_files(dir.path(), &bytes);

        let not_a_key = dir.path().join("notes.txt");
        std::fs::write(&not_a_key, "dear bob, here is the file\n").unwrap();

        state.verify_signature(&sig, &not_a_key);
        let notice = state.error.clone().unwrap();
        assert!(
            notice.contains("key"),
            "the user has to know which file to replace; got {notice}"
        );
    }
}
