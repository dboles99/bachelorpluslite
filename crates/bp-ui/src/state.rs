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
use bp_files::{DiskState, FileStamp, LineEndingPolicy, SaveOptions, atomic_write, encode, load};
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

    /// Apply a YAML rewrite, and say what it cost or why it refused.
    ///
    /// Two things ADR-0023 requires the caller to say, and neither is
    /// `bp-data`'s to say for it.
    ///
    /// On success: `saphyr` parses YAML into data, so a comment -- which is
    /// not data -- has nothing to be put back from, and an alias is resolved
    /// into a copy of what it pointed at. Both preserve what the document
    /// *means* while changing what it says. The menu row warns before the
    /// click; this says it again after, because the status bar is what is on
    /// screen when the user looks at the result.
    ///
    /// On failure: that **nothing changed**. The library's own sentence
    /// follows verbatim rather than being re-worded here -- the three
    /// refusals ADR-0023 designs for each name their limit and where it was
    /// hit, and a shell that paraphrased them would be a second place for
    /// that wording to drift.
    fn apply_yaml(&mut self, result: Result<String, bp_data::DataError>, did: &str) {
        match result {
            Ok(text) => {
                self.edit(text);
                self.error = Some(format!(
                    "✓ {did} — comments were not kept and aliases were expanded; Ctrl+Z undoes it"
                ));
            }
            Err(e) => self.error = Some(format!("this YAML was left unchanged — {e}")),
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
            // The document count is part of the answer rather than a nicety.
            // A YAML file is a *stream*: Format and Minify write every
            // document back and Convert to JSON refuses more than one, so the
            // first place a user learns there are three must not be the
            // refusal (ADR-0023).
            (action::DATA_VALIDATE, Format::Yaml) => {
                let count = bp_data::yaml_validate(&text)
                    .and_then(|()| bp_data::yaml_document_count(&text));
                self.error = Some(match count {
                    Ok(0) => "✓ valid YAML — no documents in this file".to_owned(),
                    Ok(1) => "✓ valid YAML".to_owned(),
                    Ok(n) => format!("✓ valid YAML — {n} documents"),
                    Err(e) => format!("invalid YAML — {e}"),
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
            (action::DATA_FORMAT, Format::Yaml) => {
                self.apply_yaml(bp_data::yaml_format(&text), "formatted");
            }
            (action::DATA_MINIFY, Format::Yaml) => {
                self.apply_yaml(bp_data::yaml_minify(&text), "rewritten in flow style");
            }
            (action::DATA_YAML_TO_JSON, _) => {
                self.apply_yaml(bp_data::yaml_to_json(&text), "converted to JSON");
            }
            // The one direction that loses nothing, so it is the one that
            // does not go through `apply_yaml`: every JSON value has a YAML
            // spelling, and there are no comments in JSON to drop.
            (action::DATA_JSON_TO_YAML, _) => {
                let converted = bp_data::json_to_yaml(&text);
                let ok = converted.is_ok();
                self.apply_to_active(converted);
                if ok {
                    self.error = Some("✓ converted to YAML".to_owned());
                }
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

        // `Preserve`, not the document's declared line ending: a save must
        // return the bytes the user was given. Converting here is what made
        // every save quietly rewrite the minority convention in a mixed
        // document, with nothing on screen to say it had happened. Format ▸
        // LF / CRLF is where a conversion is asked for, and it asks first.
        let bytes = encode(
            &self.text_of(id),
            doc.encoding(),
            LineEndingPolicy::Preserve,
        );

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

        // The same policy as a real save, for the same reason the encoding is
        // shared: bytes written differently from the original would make the
        // copy a different file.
        let bytes = encode(
            &self.text_of(id),
            doc.encoding(),
            LineEndingPolicy::Preserve,
        );

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
    /// The encoded form rather than the buffer, because a digest is a claim
    /// about a *file*: hashing the buffer would print a digest that
    /// `sha256sum` disagrees with for every document carrying a byte-order
    /// mark, and a user comparing the two would conclude their file had been
    /// tampered with.
    ///
    /// [`LineEndingPolicy::Preserve`], the same policy `save_document` uses,
    /// and the two have to agree: a digest taken under one policy and a file
    /// written under the other would differ for exactly the mixed-ending
    /// documents somebody takes a digest to settle.
    ///
    /// A one-off on a menu click, like `report_statistics` -- it copies the
    /// whole document twice over and must never move onto the typing path.
    pub(crate) fn active_bytes(&self) -> Result<Vec<u8>, String> {
        let Some(doc) = self.workspace.active() else {
            return Err("there is no document to read".to_owned());
        };
        Ok(encode(
            &self.active_text(),
            doc.encoding(),
            LineEndingPolicy::Preserve,
        ))
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

    /// Plan a redaction of whatever a scan of the active document finds.
    ///
    /// Scans, says what it found, and hands back a plan for the caller to get
    /// consent for -- redaction destroys text, so the shell asks before it
    /// happens rather than offering undo afterwards as the whole answer.
    ///
    /// `None` means there is nothing this can act on, and the status bar
    /// already says which of the reasons it is.
    pub(crate) fn plan_redaction(&mut self) -> Option<RedactionPlan> {
        let text = self.active_text();
        let findings = bp_secrets::scan(&text);
        let plan = RedactionPlan::from_scan(text, &findings);
        if plan.spans.is_empty() {
            self.error = Some(plan.refusal());
            return None;
        }
        self.error = Some(secret_scan_summary(&findings));
        Some(plan)
    }

    /// Say that the user declined a redaction they were shown.
    ///
    /// Not silence. Cancelling a file dialog leaves the status bar alone
    /// because nothing was claimed; here the bar is showing a scan summary
    /// and the user has just been asked a question, so the answer belongs on
    /// screen -- otherwise "I clicked Cancel" and "it did nothing" look the
    /// same.
    pub(crate) fn decline_redaction(&mut self, plan: &RedactionPlan) {
        self.error = Some(format!(
            "nothing was redacted — {} possible credential{} {} still in this document",
            plan.spans.len(),
            if plan.spans.len() == 1 { "" } else { "s" },
            if plan.spans.len() == 1 { "is" } else { "are" },
        ));
    }

    /// Carry out a redaction the user has agreed to, and check it.
    ///
    /// An ordinary edit, like a data operation: undoable, and nothing reaches
    /// disk until the user saves. That is the only honest shape for this.
    /// Redaction is irreversible *in the string it returns*, but the document
    /// is not the file, and quietly rewriting the file instead would destroy
    /// the user's only copy of text a scanner guessed about.
    ///
    /// Returns whether the document changed, so the caller knows whether to
    /// re-push the text into the widget.
    pub(crate) fn apply_redaction(&mut self, plan: &RedactionPlan) -> bool {
        // The offsets were measured when the dialog opened, and `rfd` pumps
        // events while it is up. ADR-0028 is built on the rule that stale
        // offsets destroy the wrong text and report success, and its own
        // `PastEnd` catches only the half of that which runs off the end --
        // an edit that left the length alone would slip straight through. So
        // the document is compared rather than trusted.
        if self.active_text() != plan.original {
            self.error = Some(
                concat!(
                    "nothing was redacted — the document changed while the ",
                    "dialog was open, so these positions no longer describe it; scan again"
                )
                .to_owned(),
            );
            return false;
        }

        let redacted = match bp_redaction::redact(&plan.original, &plan.spans, REDACTION_MODE) {
            Ok(redacted) => redacted,
            Err(e) => {
                self.error = Some(format!("nothing was redacted — {e}"));
                return false;
            }
        };

        // `verify` re-derives what was removed from the original rather than
        // being handed it, so checking costs nobody a variable holding a
        // secret. It answers in indices into `applied`; those become line
        // numbers here and never text, which is the whole reason the type
        // reports indices in the first place.
        let survivors = bp_redaction::verify(&plan.original, &plan.spans, &redacted.text)
            .ok()
            .map(|verification| {
                verification
                    .surviving
                    .iter()
                    .filter_map(|index| redacted.applied.get(*index))
                    .map(|applied| line_at(&plan.original, applied.start))
                    .collect::<Vec<usize>>()
            });

        let written = redacted.applied.len();
        self.edit(redacted.text);
        self.error = Some(plan.outcome(written, survivors.as_deref()));
        true
    }

    /// Report what identifying metadata the active document carries.
    ///
    /// Returns the body of the listing to show, or `None` when the status bar
    /// has already said everything there is to say.
    pub(crate) fn inspect_metadata(&mut self) -> Option<String> {
        let container = container_of(self.workspace.active().and_then(Document::path));
        let findings = bp_redaction::inspect(&self.active_text());
        self.error = Some(metadata_summary(&findings, container));
        (!findings.is_empty() || !container.hidden().is_empty())
            .then(|| metadata_report(&findings, container))
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

    /// Check the active document against the `.sig` sidecar beside it.
    ///
    /// The sidecar is found by `bp_integrity::sidecar_path` -- `document.ext`
    /// is signed by `document.ext.sig` -- rather than asked for. Two callers
    /// that disagree about that name produce a document one half of the
    /// product believes is unsigned, which is why the convention is a
    /// function in one crate and not a rule left to each caller.
    ///
    /// `expect` chooses between the two questions ADR-0026 keeps apart.
    /// [`Expectation::AnySigner`](bp_integrity::Expectation::AnySigner)
    /// answers "has this changed since the key named in the sidecar signed
    /// it", which anybody who alters a document can pass by re-signing with a
    /// key of their own; naming a key answers "was it *this* signer", and is
    /// the only way `SignedByAnotherKey` can arise at all.
    ///
    /// Returns whether the check passed, so the caller knows whether naming a
    /// key could still change the answer. It can only turn a pass into
    /// `SignedByAnotherKey`; a failure is a failure whatever key is offered,
    /// because `verify_file` tests the signature against the sidecar's own
    /// key before it compares that key with anybody's expectation.
    pub(crate) fn verify_signature(&mut self, expect: &bp_integrity::Expectation) -> bool {
        let Some(path) = self
            .workspace
            .active()
            .and_then(Document::path)
            .map(Path::to_path_buf)
        else {
            self.error = Some(
                "cannot verify — this document has never been saved, so there is no file \
                 for a signature to sit beside"
                    .to_owned(),
            );
            return false;
        };

        // A missing *document* is an error and a missing *signature* is a
        // verdict -- and a failing one. `bp-integrity` draws that line, and
        // the shell does not get to soften it: an unsigned document must not
        // come back as anything but a failure, or deleting a file would be a
        // way to pass the check.
        let verification = match bp_integrity::verify_file(&path, expect) {
            Ok(verification) => verification,
            Err(e) => {
                self.error = Some(format!("cannot verify — {e}"));
                return false;
            }
        };

        let verified = verification.is_verified();
        // `explain` rather than a second set of sentences here. Every verdict
        // words itself in `bp-integrity`, beside the logic that produces it,
        // so the status bar and any other surface say the same thing about
        // the same answer -- and so the five outcomes ADR-0026 exists to keep
        // apart stay five, rather than collapsing into "does not verify" on
        // the way to a screen.
        let mut message = format!(
            "{} {}",
            if verified { "✓" } else { "✗" },
            verification.explain()
        );

        if verified && *expect == bp_integrity::Expectation::AnySigner {
            // The caveat `Expectation::AnySigner` carries in its own doc
            // comment. Without it a tick reads as "this is from who you
            // think", which is a claim nothing here checked.
            message.push_str(
                " — no key was named, so this says only that the document has not changed \
                 since the key above signed it; anyone who alters a document can re-sign it \
                 with a key of their own",
            );
        }
        if self.active_differs_from_disk() {
            // The shell's own contribution, and the only sentence here it is
            // in a position to write: the verdict is about the file, and the
            // buffer on screen is not that file.
            message.push_str(
                " — this document has unsaved changes, so these are not the bytes anybody \
                 signed; the check was made against the file on disk",
            );
        }
        self.error = Some(message);
        verified
    }

    /// Read the public key a signature is claimed to have been made with.
    ///
    /// Reports its own failure, because "could not read the key" and "the
    /// signature does not match" are answers with different fixes and the
    /// second must never be shown for the first.
    pub(crate) fn expected_signer(&mut self, key_path: &Path) -> Option<bp_integrity::Expectation> {
        match read_verifying_key(key_path) {
            Ok(key) => Some(bp_integrity::Expectation::Key(key)),
            Err(e) => {
                self.error = Some(format!("could not read the key — {e}"));
                None
            }
        }
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

/// How redaction replaces what it destroys.
///
/// [`bp_redaction::Replacement::Placeholder`], and the two it was chosen over
/// matter more than the one it is.
///
/// `Mask` with `MaskWidth::MatchOriginal` is out on its own terms: it draws
/// one character per character removed, which publishes the *length* of what
/// was there. For a name that is nearly nothing; for a PIN, a short token, or
/// an answer from a fixed set of options it is most of the secret, and a
/// redaction that discloses the secret is the failure ADR-0028 exists to
/// prevent.
///
/// `Remove` closes the gap, so the document reads as though the credential
/// had never been typed. That is right when the withholding itself should be
/// invisible. It is wrong here: the user is about to be told to save, to
/// rotate the key and to deal with every other copy, and a change they cannot
/// see on screen is one they will not act on.
///
/// `Placeholder` leaves `[REDACTED: AWS access key ID]`. The label is
/// `SecretKind::label` -- the name of the rule that matched, never what it
/// matched -- so the marker says what kind of thing was taken out without
/// putting it back. The cost is real, and is why this is a decision rather
/// than a default: the marker tells whoever reads the document afterwards
/// that an AWS key was there. That is a disclosure an author cleaning up
/// their own note can live with, and it is what makes a redaction findable
/// again later, which `bp_redaction::PLACEHOLDER` is public for.
const REDACTION_MODE: bp_redaction::Replacement = bp_redaction::Replacement::Placeholder;

/// A redaction the user has been shown and has not yet agreed to.
///
/// Holds the text the spans were measured against, so that a buffer edited
/// while the confirmation dialog was up is caught rather than redacted
/// against offsets that no longer describe it.
///
/// Carries no secret: `spans` are positions, `listing` is line numbers and
/// rule names, and `original` is the document the user is already looking at.
pub(crate) struct RedactionPlan {
    original: String,
    spans: Vec<bp_redaction::Span<'static>>,
    /// Line and kind per span, for the confirmation dialog. Never the text.
    listing: Vec<(usize, &'static str)>,
    /// Private-key blocks deliberately left alone. See [`RedactionPlan::from_scan`].
    key_blocks: usize,
    /// Findings whose position did not resolve to a range of this document.
    unlocatable: usize,
    /// Everything the scan reported, including what is not being redacted.
    total: usize,
}

impl RedactionPlan {
    /// Turn a scan of `original` into byte spans `bp_redaction` can act on.
    ///
    /// The conversion is the whole job, and it is not a formality.
    /// `bp_secrets::Finding` reports a 1-based line, a **character** column
    /// and a length in **characters**, because those are what a caret is
    /// moved to. `bp_redaction::Span` is in bytes, because those are what a
    /// `&str` can be sliced at. On any line holding a multi-byte character
    /// the two disagree, and ADR-0028 is explicit that a span in the wrong
    /// unit destroys the wrong text and reports success. A finding that does
    /// not resolve is counted and dropped rather than clamped, for the same
    /// reason `bp_redaction` refuses rather than clamping.
    ///
    /// A `PrivateKeyBlock` is deliberately left out. `bp-secrets` documents
    /// that its finding covers the `-----BEGIN ... PRIVATE KEY-----` marker
    /// only -- one line, column and length cannot describe a block that runs
    /// on to a matching `END` -- so redacting that span would take out the
    /// label and leave the key material in the document under a `[REDACTED]`
    /// marker. That is exactly the black-rectangle failure ADR-0028 is
    /// written against, so it is refused and named rather than half done.
    fn from_scan(original: String, findings: &[bp_secrets::Finding]) -> Self {
        let lines = line_spans(&original);
        let mut spans = Vec::new();
        let mut listing = Vec::new();
        let mut key_blocks = 0usize;
        let mut unlocatable = 0usize;

        for finding in findings {
            if finding.kind == bp_secrets::SecretKind::PrivateKeyBlock {
                key_blocks += 1;
                continue;
            }
            let Some((offset, line)) = finding
                .line
                .checked_sub(1)
                .and_then(|index| lines.get(index))
                .copied()
            else {
                unlocatable += 1;
                continue;
            };
            let first = finding.column.saturating_sub(1);
            let start = byte_of_char(line, first);
            let end = byte_of_char(line, first.saturating_add(finding.length));
            if start >= end {
                unlocatable += 1;
                continue;
            }
            spans.push(bp_redaction::Span::labelled(
                offset + start,
                offset + end,
                finding.kind.label(),
            ));
            listing.push((finding.line, finding.kind.label()));
        }

        Self {
            original,
            spans,
            listing,
            key_blocks,
            unlocatable,
            total: findings.len(),
        }
    }

    /// Why there is nothing to redact, when there is nothing to redact.
    ///
    /// Three different sentences, because they ask three different things of
    /// the user. "No credentials found" is an all-clear; the other two are
    /// not, and reporting them as one would be the quiet lie.
    fn refusal(&self) -> String {
        if self.key_blocks > 0 {
            return format!(
                concat!(
                    "nothing was redacted — the {} private key block{} found {} marked only by ",
                    "the BEGIN line, and replacing that would leave the key body in the ",
                    "document; remove {} by hand",
                ),
                self.key_blocks,
                if self.key_blocks == 1 { "" } else { "s" },
                if self.key_blocks == 1 { "is" } else { "are" },
                if self.key_blocks == 1 { "it" } else { "them" },
            );
        }
        if self.unlocatable > 0 {
            return format!(
                concat!(
                    "nothing was redacted — {} of the {} finding{} could not be located in ",
                    "this text; scan again",
                ),
                self.unlocatable,
                self.total,
                if self.total == 1 { "" } else { "s" },
            );
        }
        "nothing to redact — no credentials found".to_owned()
    }

    /// What the user is agreeing to, in full, before anything is destroyed.
    ///
    /// Line numbers and rule names only. The value is never printed here for
    /// the same reason the scan's own listing never prints it: a dialog is a
    /// thing people screenshot into bug reports.
    pub(crate) fn consent_body(&self) -> String {
        let mut body = format!(
            "{} credential{} will be replaced with {}: kind] markers:\n\n",
            self.listing.len(),
            if self.listing.len() == 1 { "" } else { "s" },
            // Built from the constant rather than written out, so the shape
            // the dialog promises and the shape a later search for previous
            // redactions looks for cannot drift apart.
            bp_redaction::PLACEHOLDER.trim_end_matches(']'),
        );
        for (line, kind) in &self.listing {
            let _ = writeln!(body, "    line {line} — {kind}");
        }
        body.push_str(concat!(
            "\nPositions and kinds only. The marker names the rule that matched, ",
            "never what it matched.\n",
        ));
        if self.key_blocks > 0 {
            let _ = write!(
                body,
                concat!(
                    "\n{} private key block{} will be left alone: the scan marks only the ",
                    "-----BEGIN ... PRIVATE KEY----- line, and replacing that would take out ",
                    "the label and leave the key body in the document. Remove {} by hand.\n",
                ),
                self.key_blocks,
                if self.key_blocks == 1 { "" } else { "s" },
                if self.key_blocks == 1 { "it" } else { "them" },
            );
        }
        if self.unlocatable > 0 {
            let _ = write!(
                body,
                concat!(
                    "\n{} finding{} could not be located in this text and will be left alone. ",
                    "That should not happen; scan again before trusting this.\n",
                ),
                self.unlocatable,
                if self.unlocatable == 1 { "" } else { "s" },
            );
        }
        body.push_str(concat!(
            "\nThis changes the document in front of you, not the file on disk, and undo ",
            "restores what was removed. The file you saved earlier, its recovery journal, ",
            "the undo history, and anything already copied to the clipboard or sent still ",
            "hold the originals. Redacting here deals with one of the places this text ",
            "lives.\n",
        ));
        body
    }

    /// One line for the status bar once the redaction has been carried out.
    ///
    /// `survivors` are the lines of redactions whose text still occurs
    /// somewhere in the result, or `None` when the check itself could not be
    /// run. ADR-0028 is careful that a survivor is a reason to look and not a
    /// verdict -- the same value appearing somewhere the scan did not mark is
    /// the usual cause -- so the wording says look, rather than failed.
    fn outcome(&self, written: usize, survivors: Option<&[usize]>) -> String {
        let mut line = match survivors {
            Some([]) => format!(
                concat!(
                    "✓ {} redaction{} written — undo brings the originals back, and the copy ",
                    "on disk still holds them until you save",
                ),
                written,
                if written == 1 { "" } else { "s" },
            ),
            Some(lines) => format!(
                concat!(
                    "⚠ {} redaction{} written, but {} still occurs elsewhere in this ",
                    "document (line{} {}) — the scan did not mark that copy; look at it and ",
                    "redact it by hand",
                ),
                written,
                if written == 1 { "" } else { "s" },
                lines.len(),
                if lines.len() == 1 { "" } else { "s" },
                lines
                    .iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
            None => format!(
                concat!(
                    "⚠ {} redaction{} written, but the check that nothing survived could not ",
                    "be run",
                ),
                written,
                if written == 1 { "" } else { "s" },
            ),
        };
        if self.key_blocks > 0 {
            let _ = write!(
                line,
                "; {} private key block{} left alone, marked only by the BEGIN line",
                self.key_blocks,
                if self.key_blocks == 1 { "" } else { "s" },
            );
        }
        line
    }
}

/// Byte offset and text of every line, split exactly as `str::lines` splits.
///
/// `bp_secrets` numbers its findings by enumerating `text.lines()`, so this
/// has to agree with it line for line -- including that `lines` drops the
/// carriage return of a CRLF ending, which shifts every byte offset after it
/// if it is not put back.
fn line_spans(text: &str) -> Vec<(usize, &str)> {
    let mut spans = Vec::new();
    let mut offset = 0usize;
    for line in text.lines() {
        spans.push((offset, line));
        offset += line.len();
        if text[offset..].starts_with('\r') {
            offset += 1;
        }
        if text[offset..].starts_with('\n') {
            offset += 1;
        }
    }
    spans
}

/// The byte offset of character `index`, or the end of the line past it.
///
/// The end rather than `None`, so a column that runs off the line collapses
/// to an empty span, which the caller counts as unlocatable rather than
/// redacting something it guessed at.
fn byte_of_char(line: &str, index: usize) -> usize {
    line.char_indices()
        .nth(index)
        .map_or(line.len(), |(offset, _)| offset)
}

/// The 1-based line a byte offset falls on.
fn line_at(text: &str, offset: usize) -> usize {
    text[..offset.min(text.len())]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1
}

/// Which shape a document arrived in, from its extension.
///
/// By extension rather than through `bp_formats::detect`, because every
/// format `bp-formats` knows is one whose bytes *are* its content -- there is
/// no `Format::Docx` to ask about. The question here is the opposite one: is
/// this a container whose metadata reading the text cannot reach?
///
/// An unknown extension, and a document that has never been saved, are
/// [`bp_redaction::Container::PlainText`]. That claims nothing about hidden
/// metadata and still carries the sentence about the filesystem entry around
/// the file, which is the honest answer where there is no evidence either way.
fn container_of(path: Option<&Path>) -> bp_redaction::Container {
    use bp_redaction::Container;

    let extension = path
        .and_then(Path::extension)
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "docx" | "docm" | "xlsx" | "xlsm" | "pptx" | "pptm" => Container::OfficeOpenXml,
        "odt" | "ods" | "odp" | "odg" | "odf" => Container::OpenDocument,
        "pdf" => Container::Pdf,
        "rtf" => Container::RichText,
        "png" | "jpg" | "jpeg" | "gif" | "tif" | "tiff" | "webp" | "heic" | "heif" | "avif" => {
            Container::Image
        }
        _ => Container::PlainText,
    }
}

/// How many metadata findings the status bar names, for the same reason
/// [`SECRETS_IN_SUMMARY`] exists.
const METADATA_IN_SUMMARY: usize = 3;

/// One line for the status bar describing a metadata inspection.
///
/// Kinds and positions, never the value -- `MetadataFinding` deliberately
/// carries neither, and a status bar that reached back into the document to
/// quote an email address would undo that.
///
/// The container half is not garnish. For a format this build cannot see
/// inside, "nothing found" reads to a user as an all-clear and would be a
/// lie, so what was *not* looked at is said in the same sentence as what was.
pub(crate) fn metadata_summary(
    findings: &[bp_redaction::MetadataFinding],
    container: bp_redaction::Container,
) -> String {
    let hidden = container.hidden();
    let found = if findings.is_empty() {
        "nothing identifying in the text".to_owned()
    } else {
        let named: Vec<String> = findings
            .iter()
            .take(METADATA_IN_SUMMARY)
            .map(|f| format!("{} at line {}", f.kind.label(), f.line))
            .collect();
        let rest = findings.len() - named.len();
        format!(
            "{} identifying item{} — {}{}",
            findings.len(),
            if findings.len() == 1 { "" } else { "s" },
            named.join(", "),
            if rest == 0 {
                String::new()
            } else {
                format!(", and {rest} more")
            }
        )
    };

    if hidden.is_empty() {
        if findings.is_empty() {
            format!(
                concat!(
                    "✓ {} — the file's own timestamps, ownership and extended attributes are ",
                    "outside this check",
                ),
                found,
            )
        } else {
            format!("⚠ {found}")
        }
    } else {
        // "but" rather than "and" when the text came back clean, because
        // that clause is the one contradicting the clause before it: a clean
        // text scan is exactly what a user would otherwise read as an
        // all-clear for the whole file.
        format!(
            "⚠ {found}{} this {} file also carries {} where reading its text cannot look",
            if findings.is_empty() {
                ", but"
            } else {
                ", and"
            },
            container.label(),
            join_and(&hidden.iter().map(|kind| kind.label()).collect::<Vec<_>>()),
        )
    }
}

/// The full listing of a metadata inspection, one finding per line.
///
/// Ends with what was *not* looked at, in every case. For plain text that is
/// the filesystem entry around the file, which `bp-redaction` says explicitly
/// is the shell's and not its own; for anything else it is the container's
/// own metadata and what a build would need to read it.
pub(crate) fn metadata_report(
    findings: &[bp_redaction::MetadataFinding],
    container: bp_redaction::Container,
) -> String {
    let mut body = if findings.is_empty() {
        "Nothing identifying was found in the text.\n".to_owned()
    } else {
        let mut listing = format!(
            "{} item{} of identifying metadata in the text:\n\n",
            findings.len(),
            if findings.len() == 1 { "" } else { "s" },
        );
        for finding in findings {
            let _ = writeln!(
                listing,
                "    line {} — {} ({})",
                finding.line,
                finding.kind.label(),
                exposure_label(finding.exposure),
            );
        }
        listing.push_str("\nPositions and kinds only. The value itself is never printed here.\n");
        listing
    };

    match container.requires() {
        None => body.push_str(concat!(
            "\nThe bytes of a plain text file are the whole document, so this looked at all ",
            "of it. What it cannot see is the filesystem entry around the file: modification ",
            "and creation times, ownership, extended attributes and, on Windows, alternate ",
            "data streams. Those identify a document too, and they travel with a copy.\n",
        )),
        // No article in front of the label: a sentence that had to choose
        // between "a PDF" and "an Office Open XML" would need to know which,
        // and `Container` grows.
        Some(needs) => {
            let _ = write!(
                body,
                concat!(
                    "\nThis {} file also carries {} that reading its text cannot reach; ",
                    "seeing those would need {}. Nothing above is an all-clear for ",
                    "this file.\n",
                ),
                container.label(),
                join_and(
                    &container
                        .hidden()
                        .iter()
                        .map(|kind| kind.label())
                        .collect::<Vec<_>>()
                ),
                needs,
            );
        }
    }
    body
}

/// How far a finding goes towards identifying somebody, in the user's terms.
///
/// Presentation, so it lives here rather than in `bp-redaction`, on the same
/// argument as [`confidence_label`]: the crate is pure and has no opinion
/// about wording, and a bare "circumstantial" beside a real name reads as
/// "ignore me".
const fn exposure_label(exposure: bp_redaction::Exposure) -> &'static str {
    match exposure {
        bp_redaction::Exposure::Attributable => "names a person",
        bp_redaction::Exposure::Identifying => "names a machine, account or organisation",
        bp_redaction::Exposure::Circumstantial => "narrows the field",
    }
}

/// `"a, b and c"`. A sentence rather than a list, because these are read once
/// rather than scanned.
fn join_and(items: &[&str]) -> String {
    match items {
        [] => String::new(),
        [only] => (*only).to_owned(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
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

Taken over the {bytes} bytes this document would be written as, so it \
        matches the file once it is saved.",
        digest.to_display(),
        digest.to_hex(),
    );
    if unsaved {
        // The one way this digest can be honestly wrong about the file, and
        // it is invisible from the dialog otherwise.
        report.push_str(
            "

This document has unsaved changes, so it is not yet the digest of \
            anything on disk.",
        );
    }
    report
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
    fn saving_a_mixed_document_does_not_rewrite_the_minority_line_break() {
        // The defect this replaced. The shell used to normalise to LF and
        // re-expand to the document's declared ending, so every save quietly
        // converted whichever convention was in the minority -- and nothing
        // on screen said so. `bp-files` owns the encoder now and the shell
        // asks it for `Preserve`; this is the test that goes red if somebody
        // "tidies" that back to the declared ending.
        let dir = std::env::temp_dir().join(format!("bpad-ui-mixed-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mixed.txt");

        let mut state = AppState::new();
        let id = state.workspace.active_id().unwrap();
        state.edit("crlf\r\nlf\nend".to_owned());

        assert_eq!(
            state.save_document(id, Some(path.clone())),
            SaveResult::Saved
        );
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"crlf\r\nlf\nend",
            "both conventions survive a save exactly as the user left them"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_digest_is_taken_over_the_same_bytes_a_save_would_write() {
        // `active_bytes` and `save_document` have to agree on the policy, or
        // a digest taken to settle a question about a file describes bytes
        // that file does not contain -- and a mixed-ending document is
        // exactly the kind somebody takes a digest to settle.
        let dir = std::env::temp_dir().join(format!("bpad-ui-digest-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mixed.txt");

        let mut state = AppState::new();
        let id = state.workspace.active_id().unwrap();
        state.edit("crlf\r\nlf\nend".to_owned());

        let hashed = state.active_bytes().expect("a document is open");
        assert_eq!(
            state.save_document(id, Some(path.clone())),
            SaveResult::Saved
        );
        assert_eq!(
            hashed,
            std::fs::read(&path).unwrap(),
            "the digest describes the file that was actually written"
        );

        let _ = std::fs::remove_dir_all(&dir);
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
            // In the File menu, and therefore in the same *menu* as the
            // recent rows -- which is where an id landing in that window
            // would be least visible and most confusing.
            action::SET_DEFAULT_EDITOR,
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
    fn the_digest_report_wraps_in_the_source_without_wrapping_on_the_screen() {
        // Both sentences in `hash_report` are wrapped to fit this file, and a
        // string continuation is the only wrapping that leaves no trace. The
        // escaped newline these two started as put a line break and
        // eight spaces into the middle of a sentence in the dialog
        // instead. Both branches are checked, because the unsaved
        // sentence is its own literal and was separately wrong.
        let mut unsaved_state = AppState::new();
        unsaved_state.edit("draft".to_owned());
        let unsaved = unsaved_state.hash_active_document().expect("a digest");

        let dir = tempfile::tempdir().unwrap();
        let (mut saved_doc, _) = saved_state(dir.path(), "notes.txt", "saved content\n");
        let saved = saved_doc.hash_active_document().expect("a digest");

        for report in [&unsaved, &saved] {
            assert!(
                report.contains("would be written as, so it matches the file"),
                "the continuation has to close the sentence up; got {report}"
            );
            for line in report.lines() {
                assert!(
                    !line.starts_with(' '),
                    "a wrapped literal leaked its indentation: {line:?}"
                );
            }
        }
        assert!(
            unsaved.contains("not yet the digest of anything on disk"),
            "got {unsaved}"
        );
    }

    #[test]
    fn a_utf16_document_hashes_rather_than_refusing() {
        // This test used to assert the opposite, and it was right to: the
        // shell's own encoder refused UTF-16, and a digest over an empty
        // vector would have been a plausible-looking wrong answer. `bp-files`
        // encodes UTF-16 now, so the refusal is gone and what is worth
        // pinning instead is that the digest covers the two-byte form with
        // its byte-order mark -- the bytes the file would actually hold.
        let mut state = AppState::new();
        state.edit("hello".to_owned());
        state.set_encoding(Encoding::Utf16Le);

        let bytes = state.active_bytes().expect("a document is open");
        assert_eq!(
            bytes.len(),
            2 + "hello".len() * 2,
            "a byte-order mark plus two bytes per character"
        );
        assert!(
            state.hash_active_document().is_some(),
            "got {:?}",
            state.error
        );
    }

    /// A signing identity that is the same on every run.
    ///
    /// A fixed seed rather than `SigningKey::generate`, so a failing
    /// assertion names the same key twice and a test cannot fail once a
    /// fortnight because the OS random source was briefly unavailable.
    fn signer(seed: u8) -> bp_integrity::SigningKey {
        bp_integrity::SigningKey::from_bytes(&[seed; bp_integrity::SIGNING_KEY_LEN])
            .expect("32 bytes is a valid seed")
    }

    /// Write the public half where the Verify row's key picker would find it.
    fn publish(dir: &std::path::Path, key: &bp_integrity::SigningKey) -> PathBuf {
        let path = dir.join(format!("{}.pub", key.verifying_key().to_hex()));
        std::fs::write(&path, key.verifying_key().to_hex()).unwrap();
        path
    }

    /// A document open on a file that exists, with no unsaved edits.
    fn saved_state(dir: &std::path::Path, name: &str, text: &str) -> (AppState, PathBuf) {
        let path = dir.join(name);
        std::fs::write(&path, text).unwrap();
        let mut state = AppState::new();
        state.open(path.clone());
        (state, path)
    }

    #[test]
    fn the_sidecar_is_found_beside_the_document_rather_than_asked_for() {
        // `document.ext.sig`, appended and not substituted. Two callers that
        // disagree about that name produce a document one half of the product
        // believes is unsigned, which is why the convention is a function in
        // `bp-integrity` and not a rule the shell writes down again.
        let dir = tempfile::tempdir().unwrap();
        let (mut state, path) = saved_state(dir.path(), "notes.txt", "signed content\n");
        let key = signer(7);
        bp_integrity::sign_file(&path, &key).expect("the sidecar is written");

        assert!(
            dir.path().join("notes.txt.sig").is_file(),
            "the sidecar is named after the whole file name"
        );
        assert!(state.verify_signature(&bp_integrity::Expectation::AnySigner));
        let notice = state.error.clone().unwrap();
        assert!(notice.starts_with('\u{2713}'), "got {notice}");
        assert!(
            notice.contains(&key.verifying_key().to_hex()),
            "the signer has to be named; got {notice}"
        );
    }

    #[test]
    fn a_missing_sidecar_fails_closed_and_says_where_it_looked() {
        // The variant the whole design rests on. A signature that can be
        // deleted to produce a pass is not a signature, so an unsigned
        // document is a failure and not an absence of opinion.
        let dir = tempfile::tempdir().unwrap();
        let (mut state, _) = saved_state(dir.path(), "notes.txt", "unsigned\n");

        assert!(
            !state.verify_signature(&bp_integrity::Expectation::AnySigner),
            "an unsigned document must not pass"
        );
        let notice = state.error.clone().unwrap();
        assert!(notice.starts_with('\u{2717}'), "got {notice}");
        assert!(notice.contains("Not signed"), "got {notice}");
        assert!(
            notice.contains("notes.txt.sig"),
            "the user has to be told which file to go and find; got {notice}"
        );
    }

    #[test]
    fn a_malformed_sidecar_is_not_reported_as_a_tampered_document() {
        // A truncated file is a copying accident and a bad signature is an
        // accusation. Telling the user the second when the first happened
        // sends them looking for an attacker who does not exist.
        let dir = tempfile::tempdir().unwrap();
        let (mut state, path) = saved_state(dir.path(), "notes.txt", "signed content\n");
        bp_integrity::sign_file(&path, &signer(7)).unwrap();
        std::fs::write(dir.path().join("notes.txt.sig"), "not a sidecar at all\n").unwrap();

        assert!(!state.verify_signature(&bp_integrity::Expectation::AnySigner));
        let notice = state.error.clone().unwrap();
        assert!(notice.contains("cannot be read"), "got {notice}");
        assert!(
            !notice.contains("altered"),
            "nothing here says anything about the document; got {notice}"
        );
    }

    #[test]
    fn a_document_altered_after_signing_does_not_match_and_the_signer_is_a_claim() {
        let dir = tempfile::tempdir().unwrap();
        let (mut state, path) = saved_state(dir.path(), "notes.txt", "signed content\n");
        let key = signer(7);
        bp_integrity::sign_file(&path, &key).unwrap();
        // Changed on disk, not in the buffer: this is about the file.
        std::fs::write(&path, "signed content, plus a line somebody added\n").unwrap();

        assert!(!state.verify_signature(&bp_integrity::Expectation::AnySigner));
        let notice = state.error.clone().unwrap();
        assert!(notice.contains("does NOT match"), "got {notice}");
        assert!(
            notice.contains("could not be confirmed"),
            "the named signer is a claim, not an attribution; got {notice}"
        );
        assert!(
            notice.contains(&key.verifying_key().to_hex()),
            "got {notice}"
        );
    }

    #[test]
    fn an_intact_document_signed_by_somebody_else_is_its_own_answer() {
        // The verdict a bare 64-byte `.sig` cannot produce, and the entire
        // reason ADR-0026's sidecar records a key. Nothing is damaged and
        // nothing was tampered with: this is a filing question, and reporting
        // it as "does not match" would be an alarm about nothing.
        let dir = tempfile::tempdir().unwrap();
        let (mut state, path) = saved_state(dir.path(), "notes.txt", "signed content\n");
        let colleague = signer(7);
        let expected = signer(9);
        bp_integrity::sign_file(&path, &colleague).unwrap();

        assert!(!state.verify_signature(&bp_integrity::Expectation::Key(expected.verifying_key())));
        let notice = state.error.clone().unwrap();
        assert!(notice.contains("by a different key"), "got {notice}");
        assert!(notice.contains("intact"), "got {notice}");
        assert!(
            notice.contains(&colleague.verifying_key().to_hex()),
            "got {notice}"
        );
        assert!(
            notice.contains(&expected.verifying_key().to_hex()),
            "got {notice}"
        );
    }

    #[test]
    fn naming_the_expected_key_is_what_turns_a_pass_into_a_claim_about_a_person() {
        let dir = tempfile::tempdir().unwrap();
        let (mut state, path) = saved_state(dir.path(), "notes.txt", "signed content\n");
        let key = signer(7);
        bp_integrity::sign_file(&path, &key).unwrap();

        assert!(state.verify_signature(&bp_integrity::Expectation::Key(key.verifying_key())));
        let notice = state.error.clone().unwrap();
        assert!(notice.starts_with('\u{2713}'), "got {notice}");
        assert!(
            !notice.contains("no key was named"),
            "the caveat belongs only to the permissive check; got {notice}"
        );
    }

    #[test]
    fn a_pass_with_no_key_named_carries_the_caveat_that_makes_it_honest() {
        // `Expectation::AnySigner` establishes only that the document has not
        // changed since the key *in the sidecar* signed it. Anyone who alters
        // a document can re-sign it with a key of their own and pass. A tick
        // with no caveat reads as "this is from who you think", which is a
        // claim nothing checked.
        let dir = tempfile::tempdir().unwrap();
        let (mut state, path) = saved_state(dir.path(), "notes.txt", "signed content\n");
        bp_integrity::sign_file(&path, &signer(7)).unwrap();

        assert!(state.verify_signature(&bp_integrity::Expectation::AnySigner));
        let notice = state.error.clone().unwrap();
        assert!(notice.contains("no key was named"), "got {notice}");
        assert!(
            notice.contains("re-sign it with a key of their own"),
            "got {notice}"
        );
    }

    #[test]
    fn every_verdict_gets_its_own_sentence() {
        // Four of the five are "no", and they are four rather than one
        // because their fixes are four different things: find the file, get
        // an undamaged copy, get an untampered document, get the right key. A
        // shell that collapsed them would undo the point of the format.
        let dir = tempfile::tempdir().unwrap();
        let mut seen: Vec<String> = Vec::new();

        // Verified, and signed by another key.
        let (mut state, path) = saved_state(dir.path(), "a.txt", "content\n");
        bp_integrity::sign_file(&path, &signer(7)).unwrap();
        state.verify_signature(&bp_integrity::Expectation::Key(signer(7).verifying_key()));
        seen.push(state.error.clone().unwrap());
        state.verify_signature(&bp_integrity::Expectation::Key(signer(9).verifying_key()));
        seen.push(state.error.clone().unwrap());

        // Missing.
        let (mut state, _) = saved_state(dir.path(), "b.txt", "content\n");
        state.verify_signature(&bp_integrity::Expectation::AnySigner);
        seen.push(state.error.clone().unwrap());

        // Malformed.
        let (mut state, path) = saved_state(dir.path(), "c.txt", "content\n");
        bp_integrity::sign_file(&path, &signer(7)).unwrap();
        std::fs::write(dir.path().join("c.txt.sig"), "rubbish\n").unwrap();
        state.verify_signature(&bp_integrity::Expectation::AnySigner);
        seen.push(state.error.clone().unwrap());

        // Does not match.
        let (mut state, path) = saved_state(dir.path(), "d.txt", "content\n");
        bp_integrity::sign_file(&path, &signer(7)).unwrap();
        std::fs::write(&path, "content, altered\n").unwrap();
        state.verify_signature(&bp_integrity::Expectation::AnySigner);
        seen.push(state.error.clone().unwrap());

        let mut distinct = seen.clone();
        distinct.sort();
        distinct.dedup();
        assert_eq!(
            distinct.len(),
            seen.len(),
            "two verdicts share a message: {seen:#?}"
        );
    }

    #[test]
    fn unsaved_edits_are_reported_because_the_verdict_is_about_the_file() {
        // The shell's own contribution and the only sentence here it is in a
        // position to write: `verify_file` checks what is on the disk, and
        // the buffer on screen is not that.
        let dir = tempfile::tempdir().unwrap();
        let (mut state, path) = saved_state(dir.path(), "notes.txt", "signed content\n");
        bp_integrity::sign_file(&path, &signer(7)).unwrap();
        state.edit("signed content\nand a line typed since\n".to_owned());

        // The file still verifies -- it is untouched -- and saying so without
        // the caveat would be a tick beside text nobody signed.
        assert!(state.verify_signature(&bp_integrity::Expectation::AnySigner));
        let notice = state.error.clone().unwrap();
        assert!(notice.contains("unsaved changes"), "got {notice}");
        assert!(
            notice.contains("not the bytes anybody signed"),
            "got {notice}"
        );
    }

    #[test]
    fn a_document_that_was_never_saved_has_nothing_for_a_sidecar_to_sit_beside() {
        // The row is greyed for this, and the arm says it anyway: a silent
        // no-op would be a bug report nobody could describe.
        let mut state = AppState::new();
        state.edit("never saved\n".to_owned());

        assert!(!state.verify_signature(&bp_integrity::Expectation::AnySigner));
        let notice = state.error.clone().unwrap();
        assert!(notice.contains("never been saved"), "got {notice}");
        assert!(
            !notice.contains("  "),
            "a wrapped literal leaked its indentation into the status bar: {notice:?}"
        );
    }

    #[test]
    fn a_public_key_pasted_out_of_an_email_as_hexadecimal_is_accepted() {
        // The realistic way a public key travels. Refusing it would teach
        // users to retype keys by hand, which is how a wrong key gets used.
        let dir = tempfile::tempdir().unwrap();
        let mut state = AppState::new();
        let key = signer(7);

        let hex_key = dir.path().join("key.txt");
        let text = key.verifying_key().to_hex();
        // Wrapped across two lines, exactly as an email client would leave it.
        std::fs::write(&hex_key, format!("{}\n{}\n", &text[..32], &text[32..])).unwrap();

        assert_eq!(
            state.expected_signer(&hex_key),
            Some(bp_integrity::Expectation::Key(key.verifying_key()))
        );
    }

    #[test]
    fn the_raw_thirty_two_byte_spelling_of_a_public_key_is_accepted_too() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = AppState::new();
        let key = signer(7);
        let raw = publish(dir.path(), &key);
        std::fs::write(&raw, key.verifying_key().to_bytes()).unwrap();

        assert_eq!(
            state.expected_signer(&raw),
            Some(bp_integrity::Expectation::Key(key.verifying_key()))
        );
    }

    #[test]
    fn a_key_file_that_is_not_a_key_says_which_of_the_two_files_was_wrong() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = AppState::new();
        let not_a_key = dir.path().join("letter.txt");
        std::fs::write(&not_a_key, "dear bob, here is the file\n").unwrap();

        assert_eq!(state.expected_signer(&not_a_key), None);
        let notice = state.error.clone().unwrap();
        assert!(
            notice.contains("key"),
            "the user has to know which file to replace; got {notice}"
        );
    }

    // --- Data ▸ YAML -------------------------------------------------------

    /// A document `run_data_action` will see as `Format::Yaml`.
    ///
    /// Detection is extension-first, so the path is what decides it and no
    /// file has to exist -- the same trick `csv_state` uses.
    fn yaml_state(text: &str) -> AppState {
        let mut state = AppState::new();
        let id = state.workspace.active_id().unwrap();
        state
            .workspace
            .get_mut(id)
            .unwrap()
            .set_path(PathBuf::from("config.yaml"));
        state.edit(text.to_owned());
        state
    }

    /// The "billion laughs" document from ADR-0023: ten anchors, each a
    /// sequence of ten references to the one before. Around 200 bytes, and it
    /// names more nodes than the machine has memory for.
    fn billion_laughs() -> String {
        let mut text = String::from("a: &a [x,x,x,x,x,x,x,x,x,x]\n");
        for (name, previous) in [
            ('b', 'a'),
            ('c', 'b'),
            ('d', 'c'),
            ('e', 'd'),
            ('f', 'e'),
            ('g', 'f'),
        ] {
            let refs = std::iter::repeat_n(format!("*{previous}"), 10)
                .collect::<Vec<_>>()
                .join(",");
            text.push_str(&format!("{name}: &{name} [{refs}]\n"));
        }
        text
    }

    #[test]
    fn validating_yaml_says_how_many_documents_the_file_holds() {
        // A YAML file is a stream. Format writes every document back and
        // Convert to JSON refuses more than one, so the refusal must not be
        // where the user first learns there are three.
        let mut state = yaml_state("one: 1\n---\ntwo: 2\n---\nthree: 3\n");
        state.run_data_action(action::DATA_VALIDATE);
        let notice = state.error.clone().expect("a status message");

        assert!(notice.starts_with('✓'), "got {notice}");
        assert!(
            notice.contains('3'),
            "the count belongs in it; got {notice}"
        );
    }

    #[test]
    fn a_single_yaml_document_is_not_counted_out_at_the_user() {
        let mut state = yaml_state("one: 1\n");
        state.run_data_action(action::DATA_VALIDATE);
        assert_eq!(state.error.as_deref(), Some("✓ valid YAML"));
    }

    #[test]
    fn formatting_yaml_says_that_the_comments_did_not_survive() {
        // ADR-0023's one place where "never silently change the user's data"
        // needs a warning rather than an error: `saphyr` parses YAML into
        // data, and a comment is not data.
        let mut state = yaml_state("# why this value matters\nkey:   value\n");
        state.run_data_action(action::DATA_FORMAT);
        let notice = state.error.clone().expect("a status message");

        assert!(
            !state.active_text().contains("why this value matters"),
            "the fixture must actually lose its comment, or this proves nothing"
        );
        assert!(
            notice.contains("comments"),
            "the user has to be told what went; got {notice}"
        );
        assert!(
            notice.contains("Ctrl+Z"),
            "and how to get it back; got {notice}"
        );
    }

    #[test]
    fn minifying_yaml_rewrites_it_in_flow_style_and_says_the_same_thing() {
        let mut state = yaml_state("key:\n  - one\n  - two\n");
        state.run_data_action(action::DATA_MINIFY);
        let notice = state.error.clone().expect("a status message");

        assert!(
            state.active_text().contains('['),
            "got {}",
            state.active_text()
        );
        assert!(notice.contains("comments"), "got {notice}");
    }

    #[test]
    fn yaml_nested_past_the_limit_is_refused_with_the_limit_named() {
        // Not a taste judgement. `saphyr`'s loader recurses one stack frame
        // per level and a stack overflow aborts the process, so this refusal
        // is the only form the answer can take -- and the number has to be in
        // it, because "too deep" is not something a person can act on.
        let deep = format!(
            "{}{}",
            "[".repeat(bp_data::MAX_YAML_NESTING + 1),
            "]".repeat(bp_data::MAX_YAML_NESTING + 1)
        );
        let mut state = yaml_state(&deep);
        state.run_data_action(action::DATA_FORMAT);
        let notice = state.error.clone().expect("a status message");

        assert!(
            notice.contains("left unchanged"),
            "the user has to know the document was not touched; got {notice}"
        );
        assert!(
            notice.contains(&bp_data::MAX_YAML_NESTING.to_string()),
            "the limit belongs in it; got {notice}"
        );
        assert!(
            notice.contains("line 1, column"),
            "and where it was hit; got {notice}"
        );
        assert_eq!(state.active_text(), deep, "nothing may have been rewritten");
    }

    #[test]
    fn yaml_that_expands_past_the_node_budget_is_refused_before_it_is_built() {
        // Around 200 bytes, so neither the file size nor the nesting cap sees
        // it coming -- the document is only ever seven levels deep.
        let laughs = billion_laughs();
        assert!(
            laughs.len() < 400,
            "the fixture must stay small to mean anything"
        );

        let mut state = yaml_state(&laughs);
        state.run_data_action(action::DATA_FORMAT);
        let notice = state.error.clone().expect("a status message");

        assert!(notice.contains("left unchanged"), "got {notice}");
        assert!(
            notice.contains(&bp_data::MAX_YAML_NODES.to_string()),
            "the budget belongs in it; got {notice}"
        );
        assert_eq!(state.active_text(), laughs);
    }

    #[test]
    fn a_duplicate_yaml_key_is_refused_in_words_that_say_why_it_matters() {
        // The whole reason this is an error rather than a merge: `saphyr`
        // drops the earlier value on the way into the map, so formatting the
        // file would delete a line and report success. A message that read
        // like an ordinary parse error would leave the user editing their
        // YAML looking for a missing colon.
        let mut state = yaml_state("name: first\nname: second\n");
        state.run_data_action(action::DATA_FORMAT);
        let notice = state.error.clone().expect("a status message");

        assert!(notice.contains("duplicate key"), "got {notice}");
        assert!(
            notice.contains("replace"),
            "it has to say what would have been lost, not just that it refused; got {notice}"
        );
        assert!(
            notice.contains("line 2"),
            "and which of the two keys to go and look at; got {notice}"
        );
        assert_eq!(state.active_text(), "name: first\nname: second\n");
    }

    #[test]
    fn a_duplicate_key_is_reported_by_validate_as_well_as_by_format() {
        // Validate is where somebody checks a file they are about to hand
        // over, and it is the row that must not answer "fine".
        let mut state = yaml_state("name: first\nname: second\n");
        state.run_data_action(action::DATA_VALIDATE);
        let notice = state.error.clone().expect("a status message");

        assert!(!notice.starts_with('✓'), "got {notice}");
        assert!(notice.contains("duplicate key"), "got {notice}");
    }

    #[test]
    fn converting_a_yaml_stream_to_json_refuses_and_names_the_count() {
        // JSON has one root value. Wrapping three documents in an array would
        // hand back a different shape from the one on screen.
        let mut state = yaml_state("one: 1\n---\ntwo: 2\n");
        state.run_data_action(action::DATA_YAML_TO_JSON);
        let notice = state.error.clone().expect("a status message");

        assert!(notice.contains("left unchanged"), "got {notice}");
        assert!(
            notice.contains('2'),
            "the count belongs in it; got {notice}"
        );
        assert_eq!(state.active_text(), "one: 1\n---\ntwo: 2\n");
    }

    #[test]
    fn converting_one_yaml_document_to_json_replaces_the_document() {
        let mut state = yaml_state("key: value\n");
        state.run_data_action(action::DATA_YAML_TO_JSON);

        assert!(
            state.active_text().contains("\"key\""),
            "got {}",
            state.active_text()
        );
        assert!(state.error.as_deref().is_some_and(|e| e.starts_with('✓')));
    }

    #[test]
    fn converting_json_to_yaml_promises_nothing_was_lost() {
        // The asymmetry ADR-0023 draws: every JSON value has a YAML spelling,
        // so this direction has no warning to carry -- and a warning attached
        // to it anyway would teach the user to ignore the ones that matter.
        let mut state = AppState::new();
        let id = state.workspace.active_id().unwrap();
        state
            .workspace
            .get_mut(id)
            .unwrap()
            .set_path(PathBuf::from("data.json"));
        state.edit("{\"key\": \"value\"}".to_owned());
        state.run_data_action(action::DATA_JSON_TO_YAML);

        assert!(
            state.active_text().contains("key: value"),
            "got {}",
            state.active_text()
        );
        assert_eq!(state.error.as_deref(), Some("✓ converted to YAML"));
        assert!(
            !state.error.as_deref().unwrap().contains("comments"),
            "there are no comments in JSON to lose"
        );
    }

    // --- Security ▸ Redact -------------------------------------------------

    /// The same fake credential the scan tests use, on a line with multi-byte
    /// characters in front of it.
    ///
    /// The byte offset of the key and its character column differ by three
    /// here, which is the entire point: `bp_secrets::Finding` counts
    /// characters and `bp_redaction::Span` counts bytes, and a shell that
    /// treated them as the same unit would destroy the wrong text.
    const LEAKY_MULTIBYTE: &str = "«clé» aws_access_key_id = AKIA3G7QVHBRN2WPKZ5F\n";

    /// A PEM block, whose finding covers the BEGIN line and not the key.
    const PEM: &str =
        "-----BEGIN RSA PRIVATE KEY-----\nMIIBOgIBAAJBAKtQ\n-----END RSA PRIVATE KEY-----\n";

    #[test]
    fn redacting_takes_the_credential_out_and_leaves_a_marker_where_it_was() {
        let mut state = AppState::new();
        state.edit(LEAKY.to_owned());
        let plan = state.plan_redaction().expect("a plan");

        assert!(state.apply_redaction(&plan), "the document should change");
        let after = state.active_text();
        assert!(
            !after.contains(LEAKED),
            "the credential is still there: {after}"
        );
        assert!(
            after.contains(bp_redaction::PLACEHOLDER.trim_end_matches(']')),
            "a redaction nobody can see is one nobody will act on; got {after}"
        );
        assert!(
            after.contains("AWS access key ID"),
            "the marker names the rule that matched; got {after}"
        );
        assert!(
            after.starts_with("notes\n"),
            "only the span may go; got {after}"
        );
    }

    #[test]
    fn a_credential_after_multibyte_text_is_located_in_bytes_not_characters() {
        // The conversion this shell owns, and the one place an off-by-three
        // would destroy the wrong words and report success.
        let mut state = AppState::new();
        state.edit(LEAKY_MULTIBYTE.to_owned());
        let plan = state.plan_redaction().expect("a plan");
        assert!(state.apply_redaction(&plan));

        let after = state.active_text();
        assert!(!after.contains(LEAKED), "got {after}");
        assert!(
            after.starts_with("«clé» aws_access_key_id = "),
            "everything before the credential must survive intact; got {after}"
        );
        assert!(
            after.ends_with("]\n"),
            "and nothing after it may be eaten; got {after}"
        );
    }

    #[test]
    fn redaction_is_an_edit_to_the_document_and_not_to_the_file() {
        // Irreversible in the string it returns, and deliberately not
        // irreversible on disk. `bp-redaction` rewrites one string;
        // ADR-0028's first consequence is that a shell which saved over the
        // original would have dealt with exactly one of the places the text
        // lives and told the user it had dealt with all of them.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("leaky.txt");
        let mut state = AppState::new();
        state.edit(LEAKY.to_owned());
        let id = state.workspace.active_id().unwrap();
        assert_eq!(
            state.save_document(id, Some(path.clone())),
            SaveResult::Saved
        );

        let plan = state.plan_redaction().expect("a plan");
        assert!(state.apply_redaction(&plan));

        assert!(
            state.is_dirty(id),
            "the redaction is unsaved work like any edit"
        );
        assert!(
            std::fs::read_to_string(&path).unwrap().contains(LEAKED),
            "nothing may have been written to the file"
        );

        // And it is on the undo stack, which is the honest reading of "undo
        // brings the originals back" in the status bar.
        assert!(state.active_editor_mut().unwrap().undo());
        assert!(state.active_text().contains(LEAKED));
    }

    #[test]
    fn nothing_the_redaction_says_ever_repeats_the_credential() {
        // The guarantee the scan already makes, extended to the half of the
        // feature that has the document in its hand: the plan, the
        // confirmation dialog and every status line it produces.
        let mut state = AppState::new();
        state.edit(LEAKY.to_owned());
        let plan = state.plan_redaction().expect("a plan");
        let planned = state.error.clone().expect("a status message");
        let consent = plan.consent_body();
        assert!(state.apply_redaction(&plan));
        let outcome = state.error.clone().expect("a status message");

        for line in [planned, consent, outcome, plan.refusal()] {
            assert!(
                !line.contains(LEAKED),
                "the credential is back on screen: {line}"
            );
        }
    }

    #[test]
    fn a_private_key_block_is_left_alone_and_named_rather_than_half_redacted() {
        // `bp-secrets` marks the BEGIN line only -- one line, column and
        // length cannot describe a block that runs to a matching END -- so
        // redacting the span would take out the label and leave the key
        // material under a `[REDACTED]` marker. That is the black-rectangle
        // failure ADR-0028 exists to prevent.
        let mut state = AppState::new();
        state.edit(PEM.to_owned());

        assert!(
            state.plan_redaction().is_none(),
            "there is nothing here this can safely destroy"
        );
        let notice = state.error.clone().expect("a status message");
        assert!(notice.contains("nothing was redacted"), "got {notice}");
        assert!(notice.contains("private key"), "got {notice}");
        assert!(
            notice.contains("BEGIN"),
            "the user has to know why, or they will read it as a bug; got {notice}"
        );
        assert!(
            state.active_text().contains("MIIBOgIBAAJBAKtQ"),
            "the key body must be exactly where it was"
        );
    }

    #[test]
    fn a_clean_document_is_told_there_is_nothing_to_redact() {
        let mut state = AppState::new();
        state.edit("nothing to see here\n".to_owned());
        assert!(state.plan_redaction().is_none());
        assert_eq!(
            state.error.as_deref(),
            Some("nothing to redact — no credentials found")
        );
    }

    #[test]
    fn declining_a_redaction_says_so_rather_than_leaving_the_bar_alone() {
        // Cancelling a file dialog leaves the status bar alone because
        // nothing was claimed. Here the user was asked a question, and
        // "I pressed Cancel" and "the row does nothing" must not look alike.
        let mut state = AppState::new();
        state.edit(LEAKY.to_owned());
        let plan = state.plan_redaction().expect("a plan");
        state.decline_redaction(&plan);
        let notice = state.error.clone().expect("a status message");

        assert!(notice.contains("nothing was redacted"), "got {notice}");
        assert!(notice.contains("still in this document"), "got {notice}");
        assert_eq!(state.active_text(), LEAKY);
    }

    #[test]
    fn a_plan_measured_against_text_that_has_since_changed_is_refused() {
        // `rfd` pumps events while its dialog is up, so a callback can edit
        // the buffer underneath one. `RedactionError::PastEnd` only catches
        // the half of that which runs off the end -- an edit that left the
        // length alone would redact the wrong bytes and report success.
        let mut state = AppState::new();
        state.edit(LEAKY.to_owned());
        let plan = state.plan_redaction().expect("a plan");
        state.edit(format!("prefix\n{LEAKY}"));

        assert!(
            !state.apply_redaction(&plan),
            "stale offsets must not be applied"
        );
        let notice = state.error.clone().expect("a status message");
        assert!(notice.contains("nothing was redacted"), "got {notice}");
        assert!(
            notice.contains("changed while the dialog was open"),
            "got {notice}"
        );
        assert!(
            state.active_text().contains(LEAKED),
            "and nothing was destroyed"
        );
    }

    #[test]
    fn a_copy_the_scan_did_not_mark_is_reported_as_surviving() {
        // ADR-0028 is careful that this is a reason to look and not a verdict:
        // every named span was destroyed, and the same text simply also
        // appears where nothing marked it. Reporting it as a failure would
        // teach the user to ignore it; saying nothing would be worse.
        let mut state = AppState::new();
        state.edit(format!(
            "aws_access_key_id = {LEAKED}\nnote: xx{LEAKED}xx\n"
        ));
        let plan = state.plan_redaction().expect("a plan");
        assert_eq!(
            plan.spans.len(),
            1,
            "the fixture must yield exactly one finding"
        );

        assert!(state.apply_redaction(&plan));
        let notice = state.error.clone().expect("a status message");
        assert!(notice.starts_with('⚠'), "got {notice}");
        assert!(notice.contains("still occurs elsewhere"), "got {notice}");
        assert!(
            notice.contains("line 1"),
            "the survivor is reported by position, never by text; got {notice}"
        );
        assert!(!notice.contains(LEAKED), "got {notice}");
    }

    #[test]
    fn the_outcome_says_the_original_is_still_everywhere_else() {
        // The sentence ADR-0028 makes the shell's obligation. Redacting the
        // buffer deals with one of the places the text lives, and a status
        // bar that reads "redacted" full stop tells the same lie the black
        // rectangle does.
        let plan = RedactionPlan::from_scan(LEAKY.to_owned(), &bp_secrets::scan(LEAKY));
        let clean = plan.outcome(1, Some(&[]));

        assert!(clean.starts_with('✓'), "got {clean}");
        assert!(clean.contains("undo"), "got {clean}");
        assert!(clean.contains("disk"), "got {clean}");
    }

    #[test]
    fn a_redaction_whose_check_could_not_run_does_not_report_it_as_clean() {
        let plan = RedactionPlan::from_scan(LEAKY.to_owned(), &bp_secrets::scan(LEAKY));
        let unchecked = plan.outcome(1, None);

        assert!(unchecked.starts_with('⚠'), "got {unchecked}");
        assert!(unchecked.contains("could not"), "got {unchecked}");
    }

    #[test]
    fn the_consent_dialog_lists_what_will_go_and_what_it_does_not_reach() {
        let plan = RedactionPlan::from_scan(LEAKY.to_owned(), &bp_secrets::scan(LEAKY));
        let body = plan.consent_body();

        assert!(body.contains("line 2 — AWS access key ID"), "got {body}");
        assert!(body.contains("not the file on disk"), "got {body}");
        assert!(body.contains("recovery journal"), "got {body}");
        assert!(body.contains("clipboard"), "got {body}");
    }

    // --- Security ▸ Inspect Metadata ---------------------------------------

    #[test]
    fn a_container_this_build_cannot_open_never_reports_an_all_clear() {
        // The failure `Container::hidden` exists to prevent: an inspector
        // that quietly says "no metadata found" about a `.docx` is worse than
        // no inspector, because the user believes it and sends the file.
        let summary = metadata_summary(&[], bp_redaction::Container::OfficeOpenXml);

        assert!(!summary.starts_with('✓'), "got {summary}");
        assert!(summary.contains("author name"), "got {summary}");
        assert!(summary.contains("cannot look"), "got {summary}");
    }

    #[test]
    fn plain_text_says_the_filesystem_entry_is_outside_the_check() {
        // `bp-redaction` is explicit that modification times, ownership and
        // extended attributes are the shell's and not its own, so the shell
        // is the only place that sentence can be said.
        let summary = metadata_summary(&[], bp_redaction::Container::PlainText);
        assert!(summary.starts_with('✓'), "got {summary}");
        assert!(summary.contains("ownership"), "got {summary}");

        let report = metadata_report(&[], bp_redaction::Container::PlainText);
        assert!(report.contains("alternate data streams"), "got {report}");
    }

    #[test]
    fn the_report_for_a_container_names_what_a_build_would_need_to_read_it() {
        let report = metadata_report(&[], bp_redaction::Container::Pdf);
        assert!(report.contains("PDF parser"), "got {report}");
        assert!(
            report.contains("all-clear"),
            "the reader has to be told not to take the empty list as one; got {report}"
        );
    }

    #[test]
    fn a_metadata_report_names_positions_and_kinds_and_never_the_value() {
        // The same rule as the secret scan, and for the same reason: a dialog
        // is a thing people screenshot into bug reports.
        let text = "author: Daniel Boles <daniel@example.com>\nnotes\n";
        let findings = bp_redaction::inspect(text);
        assert!(!findings.is_empty(), "the fixture must actually be found");

        for line in [
            metadata_summary(&findings, bp_redaction::Container::PlainText),
            metadata_report(&findings, bp_redaction::Container::PlainText),
        ] {
            assert!(!line.contains("Daniel Boles"), "got {line}");
            assert!(!line.contains("daniel@example.com"), "got {line}");
            assert!(line.contains("line 1"), "got {line}");
        }
    }

    #[test]
    fn inspecting_a_clean_plain_document_says_so_without_opening_a_dialog() {
        let mut state = AppState::new();
        state.edit("nothing identifying here\n".to_owned());

        assert!(
            state.inspect_metadata().is_none(),
            "a dialog with nothing in it is worse than the status bar line"
        );
        assert!(state.error.as_deref().is_some_and(|e| e.starts_with('✓')));
    }

    #[test]
    fn inspecting_a_document_in_a_container_opens_the_dialog_even_when_the_text_is_clean() {
        let mut state = AppState::new();
        let id = state.workspace.active_id().unwrap();
        state
            .workspace
            .get_mut(id)
            .unwrap()
            .set_path(PathBuf::from("report.docx"));
        state.edit("nothing identifying here\n".to_owned());

        let report = state
            .inspect_metadata()
            .expect("the part that was not looked at is the answer");
        assert!(report.contains("ZIP reader"), "got {report}");
    }

    #[test]
    fn container_of_recognises_the_shapes_a_text_only_inspection_cannot_open() {
        use bp_redaction::Container;

        for (name, expected) in [
            ("notes.txt", Container::PlainText),
            ("notes", Container::PlainText),
            ("report.DOCX", Container::OfficeOpenXml),
            ("sheet.ods", Container::OpenDocument),
            ("paper.pdf", Container::Pdf),
            ("letter.rtf", Container::RichText),
            ("photo.JPEG", Container::Image),
        ] {
            assert_eq!(
                container_of(Some(&PathBuf::from(name))),
                expected,
                "{name} was classified wrongly"
            );
        }
        assert_eq!(
            container_of(None),
            Container::PlainText,
            "a document that has never been saved claims nothing about hidden metadata"
        );
    }
}
