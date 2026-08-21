//! Document and workspace state.
//!
//! Owns `AppState` and everything that answers questions about
//! document/workspace state: what the active document is, whether it is dirty,
//! how to save it, and how to label it in the status bar.
//!
//! ## The four things that are not here
//!
//! `AppState` is one struct with one set of fields, and it stays that way --
//! splitting the *state* would mean deciding which half of the product owns
//! the active document, and there is no such division. What is split is the
//! code, into submodules that hold an `impl AppState` block each:
//!
//! | Module | What it owns |
//! | --- | --- |
//! | [`security`] | Scan, redact, inspect, hash, sign, verify, the history they write into, and the profile and Privacy Mode switches that govern them |
//! | [`data`] | The Data menu: which `bp-data` operation a menu id means for the format in front of the user |
//! | [`encryption`] | The `.bpadx` passphrase flow: what the bar is asking, and what a wrong answer does |
//! | [`find`] | What the find bar is looking for, and which match the user is standing on |
//!
//! They are children of this module rather than siblings, which is the whole
//! reason the split is possible: a child can see its parent's private items,
//! so `editors`, `stamps`, `match_index` and the rest stay private to `state`
//! instead of becoming `pub(crate)` for the crate to reach into. A sibling
//! module would have widened every field it touched.
//!
//! What is left here is the part with no smaller subject: opening, saving,
//! closing, the editor and viewer maps, the gutter, and the labels the status
//! bar reads.

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

mod data;
mod encryption;
mod find;
mod security;

// The names the rest of the shell knows this module by. `dispatch` asks for a
// redaction plan and a scan report by way of `crate::state`, and moving the
// code that produces them is not a reason to move the path that names them.
pub(crate) use find::find_query;
pub(crate) use security::{RedactionPlan, secret_scan_report};
// Not `pub(crate)`: `AppState::new` is the only caller and the path is
// deliberately not reachable from outside `state`, because a second place
// deciding where the signing key lives is the defect this field exists to
// prevent.
use security::default_signing_key_path;

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
    /// Where the security history is appended and read back.
    ///
    /// A field rather than a call to `audit::audit_path()` at each use, so a
    /// test gets its own file. It is not configurable from outside: the
    /// production value is set once, here, and nothing changes it.
    pub(crate) audit_path: PathBuf,
    /// Size on disk of each open document, as `load` reported it.
    ///
    /// Kept so the status bar can say "Large file" without asking the
    /// filesystem on every refresh, and so the size class is the one the
    /// document was *opened* at rather than whatever the file is now.
    pub(crate) sizes: HashMap<DocumentId, u64>,
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
    /// Where this machine's signing key is, if the environment says where
    /// the user's profile is.
    ///
    /// **A field and not a function**, which is the third time this shape has
    /// been needed in this crate and the second time it was learned the hard
    /// way. A function reading the real profile directory means `cargo test`
    /// writes there -- the security history did it once, and
    /// `AppState::new()` built its recovery journal from the real path until
    /// this session. The difference here is that a `cfg(test)` redirect on
    /// the *function* would not have been enough: `has_signing_key` and the
    /// signing that follows it must agree on one path, and a function
    /// returning a fresh unique path per call would have them disagree.
    pub(crate) signing_key: Option<PathBuf>,
    /// How many rows the viewer actually drew last time.
    ///
    /// Not `visible_rows`: at the end of a file, and at any of
    /// `display_lines`' three caps, a screenful is short. The status bar says
    /// which lines are on screen, so it has to be told what was drawn rather
    /// than what was asked for.
    pub(crate) drawn_rows: usize,
    /// Documents served from disk in chunks rather than held in a rope.
    ///
    /// A document is in exactly one of `editors` and `viewers`, never both
    /// and never neither, and that is the whole of the distinction: anything
    /// reaching for `active_editor()` on one of these gets `None` and does
    /// nothing. See `crate::viewer` and ADR-0030.
    pub(crate) viewers: HashMap<DocumentId, crate::viewer::HugeView>,
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
            audit_path: crate::audit::audit_path(),
            sizes: HashMap::new(),
            passphrases: HashMap::new(),
            privacy: bp_security::Privacy::default(),
            ask: None,
            passphrase_status: String::new(),
            tab_context: None,
            file_hits: Vec::new(),
            clips: bp_clipboard::History::new(),
            signing_key: default_signing_key_path(),
            drawn_rows: 0,
            viewers: HashMap::new(),
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
                encoding: checkpoint_encoding_of(doc.encoding()),
                line_ending: Some(checkpoint_line_ending_of(doc.line_ending())),
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
    ///
    /// Encoding and line ending are restored alongside the text, or the next
    /// save would rewrite the file in whatever the platform default happens
    /// to be rather than the convention it was actually in (the bug this
    /// guards against: an LF file coming back reporting CRLF on Windows).
    pub(crate) fn restore(&mut self, entries: Vec<(u64, bp_history::Checkpoint)>) {
        for (_, entry) in entries {
            let id = match entry.path {
                Some(path) => self.workspace.open_path(path, now()),
                None => self.workspace.open_new(now()),
            };
            self.editors.insert(id, bp_editor::Editor::new(&entry.text));
            if let Some(doc) = self.workspace.get_mut(id) {
                doc.set_encoding(encoding_of_checkpoint(entry.encoding));
                // A pre-upgrade checkpoint has no recorded line ending; guess
                // from the text itself the way the rest of the codebase does
                // when certainty isn't available, rather than falling back to
                // the platform default.
                let line_ending = entry
                    .line_ending
                    .map(line_ending_of_checkpoint)
                    .or_else(|| LineEnding::detect(&entry.text))
                    .unwrap_or_default();
                doc.set_line_ending(line_ending);
                // Recovered work is by definition not on disk yet.
                doc.mark_modified();
            }
        }
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

    /// Discard edits and re-read the active document from disk.
    pub(crate) fn reload(&mut self) {
        self.error = None;
        let Some(id) = self.workspace.active_id() else {
            return;
        };
        // `load` slurps. Reloading a document that was deliberately never
        // loaded would undo the whole of ADR-0027 with one menu row.
        if self.refuse_on_viewer(id, "Reloading") {
            return;
        }
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

        // Classified from the *metadata*, before a byte of the document is
        // read. `load` slurps the whole file, so asking it how big the file
        // was is asking after the damage is done: ADR-0027 measured that a
        // document past `HUGE_FILE_BYTES` must not reach a rope at all, and
        // the only place that can be honoured is here, in front.
        if self.is_too_large_to_load(&path) {
            self.open_viewer(path);
            return;
        }

        let path2 = path.clone();
        match load(&path) {
            Ok(file) => {
                let id = self.workspace.open_path(path, now());
                if let Some(doc) = self.workspace.get_mut(id) {
                    doc.set_encoding(file.encoding);
                    doc.set_line_ending(file.line_ending);
                }
                self.editors.insert(id, bp_editor::Editor::new(&file.text));
                self.sizes.insert(id, file.bytes_on_disk);
                self.mark_in_step(id, &path2);
            }
            // The error types already render a message naming the file and
            // what to do about it, which is exactly what the status bar wants.
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    /// Open a document too large for a rope, served from disk in chunks.
    ///
    /// It gets a tab and a `HugeView` and **no `Editor`**, which is what makes
    /// every editing path a no-op rather than an edit to an empty document
    /// that looks real. ADR-0030 has the rest: it draws in the custom surface
    /// whether or not `--editor-view` was passed, because `TextInput` owns its
    /// own text and cannot be handed a window of a file it does not have.
    fn open_viewer(&mut self, path: PathBuf) {
        let view = match crate::viewer::HugeView::open(&path) {
            Ok(view) => view,
            // The error already names the file and what is wrong with it,
            // which is what the status bar wants. A file that cannot be
            // opened is not a file that is too large.
            Err(e) => {
                self.error = Some(e.to_string());
                return;
            }
        };
        let bytes = view.len_bytes();
        let message = view.access().message();

        let id = self.workspace.open_path(path.clone(), now());
        self.sizes.insert(id, bytes);
        self.viewers.insert(id, view);
        self.mark_in_step(id, &path);
        // Said once, on open, rather than left for the reader to infer from a
        // caret that never appears. It is `Access`'s own sentence: it names
        // the size and distinguishes itself from a file that is read-only on
        // disk, which is a different problem with a different way out.
        self.error = Some(message);
    }

    /// Whether `id` is served from disk rather than held in a rope.
    pub(crate) fn is_viewer(&self, id: DocumentId) -> bool {
        self.viewers.contains_key(&id)
    }

    /// The active document's viewer, if it has one.
    pub(crate) fn active_viewer_mut(&mut self) -> Option<&mut crate::viewer::HugeView> {
        self.workspace
            .active_id()
            .and_then(|id| self.viewers.get_mut(&id))
    }

    /// Whether the active document is served from disk.
    pub(crate) fn active_is_viewer(&self) -> bool {
        self.workspace
            .active_id()
            .is_some_and(|id| self.is_viewer(id))
    }

    /// **Which surface draws the active document.** The one function that
    /// decides, so the two views cannot come to disagree about which sizes
    /// they claim.
    ///
    /// ADR-0030: a document the rope does not hold is drawn by
    /// `EditorSurface` in every build, because it is the only surface that
    /// can be handed a window of a file. `--editor-view` decides the rest.
    pub(crate) fn uses_custom_surface(&self) -> bool {
        self.editor_view || self.active_is_viewer()
    }

    /// Refuse an operation a document served from disk cannot support.
    ///
    /// `true` means refused, and `error` says so. **Every caller of this is a
    /// path where doing nothing would not be harmless**: `text_of` a viewer is
    /// the empty string, so a save that merely no-oped would write an empty
    /// file over two gigabytes, and a reload would slurp the whole document
    /// into memory — which is the one thing ADR-0027 exists to prevent.
    ///
    /// Named for the reason rather than for the size, because the user asked
    /// to save and the answer is about what this document is.
    fn refuse_on_viewer(&mut self, id: DocumentId, what: &str) -> bool {
        if !self.is_viewer(id) {
            return false;
        }
        self.error = Some(format!(
            "{what} is not available for a document this large: it is read \
             from disk as you scroll and is never held whole."
        ));
        true
    }

    /// Outcome of a save attempt, so callers can tell the three cases apart.
    ///
    /// A close-on-quit flow must not treat "needs a path" or "the disk
    /// refused" as success and then discard the buffer.
    pub(crate) fn save_document(&mut self, id: DocumentId, path: Option<PathBuf>) -> SaveResult {
        self.error = None;
        // Before anything reads the document. `text_of` a viewer is the empty
        // string, so without this a Ctrl+S writes nothing over everything.
        if self.refuse_on_viewer(id, "Saving") {
            return SaveResult::Failed;
        }
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
        // Not destructive to the original, and refused anyway: an empty file
        // presented as a copy of a 2 GB document is a worse outcome than a
        // refusal, because it looks like it worked.
        if self.refuse_on_viewer(id, "Saving a copy") {
            return SaveResult::Failed;
        }
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

    /// One combined report for Tools ▸ Document Inspector: statistics,
    /// format, encoding, line ending, security profile and on-disk size,
    /// concatenated rather than sent to four separate menus for four
    /// separate numbers.
    ///
    /// Calls `bp_semantic::statistics` directly rather than through
    /// `report_statistics`, which already writes its own answer to
    /// `self.error` -- this wants the numbers folded into one report instead
    /// of a second, competing write to the same field.
    ///
    /// `None` in the sense `active_has_content` uses it: an empty document
    /// has nothing here worth a dialog.
    pub(crate) fn inspector_report(&self) -> Option<String> {
        if !self.active_has_content() {
            return None;
        }
        let doc = self.workspace.active()?;
        let stats = bp_semantic::statistics(&self.active_text());
        let size = self
            .workspace
            .active_id()
            .and_then(|id| self.sizes.get(&id))
            .map_or_else(|| "not available".to_owned(), |&bytes| human_bytes(bytes));

        Some(format!(
            "{} words, {} lines, {} paragraphs, {} characters ({} without spaces)\n\n\
             Format: {}\nEncoding: {}\nLine ending: {}\nSecurity profile: {}\nSize on disk: {size}",
            stats.words,
            stats.lines,
            stats.paragraphs,
            stats.characters,
            stats.characters_no_whitespace,
            self.format().label(),
            doc.encoding().label(),
            doc.line_ending().label(),
            self.security().name(),
        ))
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
        // Hashing, signing and verifying all come through here. For a
        // document served from disk the answer would be a digest of the empty
        // string presented as a digest of two gigabytes -- which is worse
        // than no answer, because a signature over it would verify.
        if self.active_is_viewer() {
            return Err(
                "this document is read from disk as you scroll and is never held whole, \
                 so its bytes cannot be hashed or signed here"
                    .to_owned(),
            );
        }
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
    pub(crate) fn active_differs_from_disk(&self) -> bool {
        self.workspace
            .active()
            .is_some_and(|doc| doc.is_dirty() || doc.path().is_none())
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

    /// Insert a markdown skeleton at the caret, replacing any selection.
    ///
    /// One method taking the literal text rather than five, or an enum of
    /// constructs: there is no behaviour here that differs by *which*
    /// construct it is, only the text and how far back from the end of it
    /// the caret should land -- `step_back` says that, `0` for "leave it at
    /// the end", which is every construct but the code block.
    ///
    /// Same caret-only constraint as `insert_stamp`: `TextInput` exposes no
    /// caret, so this is only reachable under `--editor-view`, and the menu
    /// disables the rows there.
    pub(crate) fn insert_markdown(&mut self, text: &str, step_back: usize) -> bool {
        let Some(editor) = self.active_editor_mut() else {
            return false;
        };
        editor.insert(text);
        // Measured back from where the caret landed after the insert, not
        // forward from where it started: a selection replaced by the insert
        // moves the start, but `insert` always leaves the caret at the end of
        // what it just wrote, which is the one position both cases agree on.
        if step_back > 0 {
            let end = editor.cursor();
            editor.set_cursor(end.saturating_sub(step_back));
        }
        self.mark_edited();
        true
    }

    /// What the status bar says about the active document's size.
    ///
    /// Empty for an ordinary document. `SizeClass::label` makes the argument:
    /// "a status bar that labels the ordinary case teaches people to ignore
    /// it", and it returns `""` for `Normal` for exactly that reason.
    ///
    /// The size is spelled out beside the class because "Large file" alone
    /// invites the question this readout exists to answer -- how large, and
    /// therefore how much of a pause to expect.
    pub(crate) fn size_label(&self) -> String {
        let Some(id) = self.workspace.active_id() else {
            return String::new();
        };
        let Some(&bytes) = self.sizes.get(&id) else {
            return String::new();
        };
        let class = bp_buffer::SizeClass::of(bytes);
        if !class.is_large() {
            return String::new();
        }
        format!("{} ({})", class.label(), human_bytes(bytes))
    }

    /// Whether this file must not reach a rope, judged from its metadata.
    ///
    /// **The size class comes from `bp-buffer`.** ADR-0027 measured the
    /// thresholds and `SizeClass::must_stream` is the question they answer;
    /// asking it here, before `load`, is the only place the answer can change
    /// what happens, because `load` slurps.
    ///
    /// A file whose metadata cannot be read is *not* diverted here. It is
    /// about to be opened, and `load` reports what is wrong with it far
    /// better than a guess from a failed `stat` would.
    fn is_too_large_to_load(&self, path: &Path) -> bool {
        std::fs::metadata(path)
            .map(|meta| bp_buffer::SizeClass::of(meta.len()).must_stream())
            .unwrap_or(false)
    }

    /// Say something only if nothing more important is already being said.
    ///
    /// Every caller of `record_security_event` has just finished an operation
    /// that put its own result in the status bar, and that result is what the
    /// user asked for. A notice about the *history* of the operation must not
    /// take the place of the operation's own answer -- "3 possible
    /// credentials" is the thing somebody clicked for, and losing it to a
    /// line about sealing would be the log making the product worse.
    fn note_quietly(&mut self, message: String) {
        if self.error.is_none() {
            self.error = Some(message);
        }
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
            // An open file handle and a line index, held for a tab that is
            // gone. Closing thirty huge documents in a session would
            // otherwise keep thirty handles open.
            self.viewers.remove(&id);
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
        // A viewer has no caret -- there is no rope for a position to be in --
        // so it reports where in the document the screen is instead. Asked
        // before the editor views, because it is neither of them.
        if let Some(id) = self.workspace.active_id()
            && let Some(view) = self.viewers.get(&id)
        {
            return crate::viewer::viewer_label(view.top(), self.drawn_rows);
        }
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

/// The directory this machine's own state goes in.
///
/// One function so that the recovery journal and the security history cannot
/// end up in two different places, and so that moving either moves both.
/// Neither is configuration: both hold machine-specific absolute paths, and
/// [`bp_platform::dirs::DirKind::roams`] says exactly what happens to those
/// in `%APPDATA%` — they are copied to every machine the user signs into,
/// where the paths in them point at nothing.
///
/// `None` when the environment does not say where the user's profile is.
/// `bp-platform` refuses to guess one, and [`recovery_dir_under`] decides
/// what to do about that rather than having a guess made for it here.
pub(crate) fn state_dir() -> Option<PathBuf> {
    bp_platform::dirs::host_directory(bp_platform::DirKind::State)
}

/// The folder inside the state directory that checkpoints go in.
pub(crate) const RECOVERY_DIR_NAME: &str = "recovery";

/// Where recovery checkpoints live, given a state directory.
///
/// The rule, separated from the edge that reads the environment, so that it
/// is assertable without a real profile directory — the same split
/// `bp_config::config_path` and `config_path_in` make, and for the same
/// reason.
///
/// `PathBuf::join` is the right join here and the wrong one almost everywhere
/// else in this workspace: there is no [`bp_platform::Platform`] parameter to
/// be inconsistent with, because `state` has already been resolved for the
/// host and this is running on it.
///
/// Without a state directory this is a *relative* path, which is a guess of
/// the kind `bp-platform` refuses to make and is kept only because the
/// alternative is dropping crash recovery entirely for an environment odd
/// enough to have no profile at all. It is recorded in
/// `project/WORK_QUEUE.md` rather than fixed here, because deciding what a
/// journal with nowhere to live should do is a product answer.
pub(crate) fn recovery_dir_under(state: Option<&Path>) -> PathBuf {
    state.map_or_else(
        || PathBuf::from(RECOVERY_DIR_NAME),
        |dir| dir.join(RECOVERY_DIR_NAME),
    )
}

/// Where recovery checkpoints live: in the state directory, in `recovery/`.
///
/// The `.gitignore` already excludes `/recovery`, and this is deliberately a
/// path the user can be told — it holds copies of their unsaved work.
#[cfg(not(test))]
pub(crate) fn recovery_dir() -> PathBuf {
    recovery_dir_under(state_dir().as_deref())
}

/// The same, redirected and made unique under test.
///
/// Without this, `AppState::new()` builds its journal from the real state
/// directory, so `cargo test` creates one in the developer's own
/// `%LOCALAPPDATA%` — the same shape as the defect the security history had,
/// and the same fix ([`crate::audit::audit_path`] carries the longer version
/// of this note). A counter and the process id rather than a random name, so
/// a failing run names a directory that can be looked at and two runs at once
/// cannot collide.
#[cfg(test)]
pub(crate) fn recovery_dir() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("bpad-test-recovery-{}-{n}", std::process::id()))
}

/// A byte count in the unit a person would use.
///
/// One decimal place and binary units, matching what `bp-buffer`'s own
/// thresholds are stated in -- 8 MiB and 192 MiB -- so a status bar reading
/// "9.4 MB" against a threshold documented as 8 MiB would invite arithmetic
/// that does not work out.
fn human_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    let bytes = bytes as f64;
    if bytes >= KIB * KIB * KIB {
        format!("{:.1} GiB", bytes / (KIB * KIB * KIB))
    } else if bytes >= KIB * KIB {
        format!("{:.1} MiB", bytes / (KIB * KIB))
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes / KIB)
    } else {
        format!("{bytes:.0} bytes")
    }
}

/// The first few bytes of `path`, for deciding whether it is a `.bpadx`.
///
/// **A header, not a file.** This existed as `std::fs::read(&path)`, which
/// answered a six-byte question by holding the whole document in memory --
/// and then `bp_files::load` read it a second time. On a 2 GB file that was
/// 4 GB of I/O and 2 GB resident before anything reached the screen, on the
/// path taken by *every* open.
///
/// A short file is not an error. Fewer than `MAGIC_LEN` bytes cannot be the
/// magic, and `is_bpadx` says so about whatever it is given.
fn read_header(path: &Path) -> std::io::Result<Vec<u8>> {
    use std::io::Read as _;

    let mut header = vec![0_u8; bp_crypto::MAGIC_LEN];
    let mut file = std::fs::File::open(path)?;
    // Not `read` -- one call may return fewer bytes than asked for without
    // being at the end, and a short read would report an encrypted document
    // as plaintext and show the user its ciphertext.
    let read = match file.read_exact(&mut header) {
        Ok(()) => bp_crypto::MAGIC_LEN,
        // A file shorter than the magic is a real file and a valid answer.
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => 0,
        Err(e) => return Err(e),
    };
    header.truncate(read);
    Ok(header)
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

/// `bp-history` mirrors `Encoding` and `LineEnding` rather than depending on
/// `bp-core` for them, so a `Checkpoint` is built and read through these
/// conversions rather than by sharing the type.
fn checkpoint_encoding_of(encoding: Encoding) -> bp_history::CheckpointEncoding {
    match encoding {
        Encoding::Utf8 => bp_history::CheckpointEncoding::Utf8,
        Encoding::Utf8Bom => bp_history::CheckpointEncoding::Utf8Bom,
        Encoding::Utf16Le => bp_history::CheckpointEncoding::Utf16Le,
        Encoding::Utf16Be => bp_history::CheckpointEncoding::Utf16Be,
    }
}

fn encoding_of_checkpoint(encoding: bp_history::CheckpointEncoding) -> Encoding {
    match encoding {
        bp_history::CheckpointEncoding::Utf8 => Encoding::Utf8,
        bp_history::CheckpointEncoding::Utf8Bom => Encoding::Utf8Bom,
        bp_history::CheckpointEncoding::Utf16Le => Encoding::Utf16Le,
        bp_history::CheckpointEncoding::Utf16Be => Encoding::Utf16Be,
    }
}

fn checkpoint_line_ending_of(line_ending: LineEnding) -> bp_history::CheckpointLineEnding {
    match line_ending {
        LineEnding::Lf => bp_history::CheckpointLineEnding::Lf,
        LineEnding::CrLf => bp_history::CheckpointLineEnding::CrLf,
    }
}

fn line_ending_of_checkpoint(line_ending: bp_history::CheckpointLineEnding) -> LineEnding {
    match line_ending {
        bp_history::CheckpointLineEnding::Lf => LineEnding::Lf,
        bp_history::CheckpointLineEnding::CrLf => LineEnding::CrLf,
    }
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

/// Build the command that opens a second, independent instance of this
/// process, with none of this process's own arguments carried over.
///
/// A function of its own rather than inlined at the dispatch call site, so
/// the executable path and the empty argument list can be pinned by a test
/// without a real process ever being spawned -- that part is what needs to
/// be right; whether a window then opens is not this crate's to prove.
pub(crate) fn new_window_command() -> std::io::Result<std::process::Command> {
    Ok(std::process::Command::new(std::env::current_exe()?))
}

/// What the application thinks its environment is: version, renderer, and
/// where it resolves its config, state and data directories to.
///
/// A free function rather than an `AppState` method: unlike the Document
/// Inspector, nothing here is about the *active document*, so there is no
/// state to reach into. Deliberately silent about what is inside those
/// directories -- a passphrase, a document, a file listing -- this reports
/// what the paths resolve to, not what is in them.
pub(crate) fn diagnostics_report() -> String {
    let named = |dir: Option<PathBuf>| {
        dir.map_or_else(|| "not available".to_owned(), |p| p.display().to_string())
    };
    format!(
        "BachelorPad+ {}\n\nRenderer: {}\n\nConfig file: {}\nState directory: {}\nData directory: {}",
        env!("CARGO_PKG_VERSION"),
        std::env::var("SLINT_BACKEND").unwrap_or_else(|_| "software".to_owned()),
        named(bp_config::config_path()),
        named(state_dir()),
        named(bp_platform::dirs::host_directory(
            bp_platform::DirKind::Data
        )),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    // --- where this machine's own state goes ---------------------------

    #[test]
    fn the_journal_and_the_history_share_the_state_directory_and_not_a_name() {
        // The rule, asserted against a fabricated state directory so it needs
        // no real profile and both legs of CI check the same thing. They must
        // be inside it -- neither is configuration, and the configuration
        // directory roams on Windows while every path they hold is
        // machine-specific -- and they must not be the same path.
        let state = PathBuf::from("/state/bachelorpad");
        let journal = recovery_dir_under(Some(&state));
        let history = crate::audit::audit_path_under(Some(&state));

        assert!(journal.starts_with(&state), "{}", journal.display());
        assert!(history.starts_with(&state), "{}", history.display());
        assert_ne!(journal, history);
        assert_eq!(
            journal.file_name().and_then(|n| n.to_str()),
            Some(RECOVERY_DIR_NAME)
        );
        assert_ne!(
            history.parent(),
            Some(journal.as_path()),
            "the history must not be inside the journal, where discarding \
             every checkpoint would take it with them"
        );
    }

    #[test]
    fn neither_is_derived_from_the_others_last_component() {
        // What this replaced: the history was `recovery_dir()` with its last
        // component swapped. Renaming the recovery folder would have moved
        // the security history with it, silently, and a history that moves is
        // a history that starts again at sequence one.
        let a = PathBuf::from("/state/one");
        let b = PathBuf::from("/state/two");
        assert_ne!(
            crate::audit::audit_path_under(Some(&a)),
            crate::audit::audit_path_under(Some(&b))
        );
        assert_eq!(
            crate::audit::audit_path_under(Some(&a)).parent(),
            Some(a.as_path()),
            "the history sits directly in the state directory"
        );
    }

    #[test]
    fn the_state_directory_is_the_platforms_state_directory() {
        // Guards the edge against being wired to the wrong `DirKind`. Reads
        // the environment and computes a path; creates nothing.
        assert_eq!(
            state_dir(),
            bp_platform::dirs::host_directory(bp_platform::DirKind::State)
        );
        if let Some(state) = state_dir() {
            let config = bp_platform::dirs::host_directory(bp_platform::DirKind::Config)
                .expect("a resolved state directory implies a resolved config one");
            assert_ne!(
                state, config,
                "state and settings must not be the same directory"
            );
            assert!(
                !bp_platform::DirKind::State.roams(bp_platform::Platform::HOST),
                "the directory the journal now lives in roams, which is the \
                 thing this move was for"
            );
        }
    }

    #[test]
    fn a_test_journal_never_lands_in_the_users_own_state_directory() {
        // The protection itself, pinned -- the same one `audit_path` carries.
        // `AppState::new()` builds a `Journal` from `recovery_dir()`, so
        // every test that constructs a state would otherwise create a
        // recovery directory in the developer's `%LOCALAPPDATA%`.
        let dir = recovery_dir();
        assert!(
            dir.starts_with(std::env::temp_dir()),
            "the test journal must live in the temp directory, not {}",
            dir.display()
        );
        assert_ne!(
            dir,
            recovery_dir(),
            "two states in one process must not share a journal; parallel \
             tests would each write checkpoints the others then read"
        );
    }

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
    fn restoring_a_checkpoint_recovers_its_encoding_and_line_ending_not_the_platform_default() {
        // The bug this guards against: recovery rebuilt the text correctly
        // but silently forgot the encoding and line ending, so a recovered
        // LF document came back reporting the platform default (CRLF on
        // Windows) instead of what it actually was.
        let mut state = AppState::new();
        let entry = bp_history::Checkpoint {
            path: Some(PathBuf::from("/notes/recovered.txt")),
            name: "recovered.txt".to_owned(),
            text: "line one\nline two\n".to_owned(),
            written_at: bp_history::now_unix(),
            encoding: bp_history::CheckpointEncoding::Utf16Le,
            line_ending: Some(bp_history::CheckpointLineEnding::Lf),
        };
        state.restore(vec![(1, entry)]);

        let doc = state
            .workspace
            .iter()
            .find(|d| d.display_name() == "recovered.txt")
            .expect("the recovered document was opened");
        assert_eq!(doc.encoding(), Encoding::Utf16Le);
        assert_eq!(doc.line_ending(), LineEnding::Lf);
    }

    #[test]
    fn restoring_a_pre_upgrade_checkpoint_guesses_the_line_ending_from_the_text() {
        // Checkpoints already on disk were written before `line_ending`
        // existed, so `#[serde(default)]` gives them `None` here. `restore`
        // must guess from the text itself -- the same guess the rest of the
        // codebase makes when certainty isn't available -- rather than fall
        // back to the platform default, which would rewrite the file's line
        // endings on the very next save.
        let mut state = AppState::new();
        let entry = bp_history::Checkpoint {
            path: Some(PathBuf::from("/notes/old.txt")),
            name: "old.txt".to_owned(),
            text: "line one\nline two\n".to_owned(),
            written_at: bp_history::now_unix(),
            encoding: bp_history::CheckpointEncoding::default(),
            line_ending: None,
        };
        state.restore(vec![(1, entry)]);

        let doc = state
            .workspace
            .iter()
            .find(|d| d.display_name() == "old.txt")
            .expect("the recovered document was opened");
        assert_eq!(
            doc.line_ending(),
            LineEnding::Lf,
            "guessed from the text, not the platform default"
        );
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
    fn the_inspector_report_folds_statistics_format_and_profile_into_one_string() {
        let mut state = AppState::new();
        state.edit("one two three\n\nfour\n".to_owned());

        let report = state
            .inspector_report()
            .expect("a document with content has something to report");

        assert!(report.contains("4 words"), "got {report}");
        assert!(report.contains("Encoding:"), "got {report}");
        assert!(report.contains("Line ending:"), "got {report}");
        assert!(
            report.contains(bp_security::Security::default().name()),
            "got {report}"
        );
    }

    #[test]
    fn the_inspector_report_is_absent_for_an_empty_document() {
        let state = AppState::new();
        assert_eq!(
            state.inspector_report(),
            None,
            "an empty document has nothing worth a dialog"
        );
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
    fn a_markdown_construct_is_inserted_at_the_caret_and_marks_the_document_unsaved() {
        let mut state = AppState::new();
        state.editor_view = true;
        state.edit("ab".to_owned());
        state.active_editor_mut().unwrap().set_cursor(1);

        assert!(state.insert_markdown("**text**", 0));

        assert_eq!(state.active_text(), "a**text**b");
        assert!(state.workspace.active().unwrap().is_dirty());
    }

    #[test]
    fn a_code_block_leaves_the_caret_between_the_fences() {
        // The one construct of the five with somewhere other than the end of
        // the inserted text to land: typing right after the insert should
        // start writing code, not follow the closing fence.
        let mut state = AppState::new();
        state.editor_view = true;

        assert!(state.insert_markdown("```\n```", 3));

        assert_eq!(state.active_text(), "```\n```");
        assert_eq!(
            state.active_editor_mut().unwrap().cursor(),
            4,
            "the caret should sit right after the opening fence's newline"
        );
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

    // --- size decides how a document is opened -----------------------------

    #[test]
    fn an_ordinary_document_carries_no_size_label() {
        // The argument `SizeClass::label` makes: a status bar that labels the
        // ordinary case teaches people to ignore it.
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("small.txt");
        std::fs::write(&path, "a few lines\nof text\n").expect("write");

        let mut state = AppState::new();
        state.open(path);
        assert!(state.error.is_none(), "{:?}", state.error);
        assert_eq!(state.size_label(), "");
    }

    #[test]
    fn a_large_document_says_so_and_says_how_large() {
        // Past LARGE_FILE_BYTES it still opens and still edits -- the label
        // exists to explain a pause, not to take anything away. "Large file"
        // alone would invite the question the readout is here to answer.
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("big.txt");
        let line = "x".repeat(79);
        let bytes = bp_buffer::LARGE_FILE_BYTES + 1024;
        let mut text = String::with_capacity(bytes as usize + 128);
        while (text.len() as u64) < bytes {
            text.push_str(&line);
            text.push('\n');
        }
        std::fs::write(&path, &text).expect("write");

        let mut state = AppState::new();
        state.open(path);
        assert!(state.error.is_none(), "{:?}", state.error);
        let label = state.size_label();
        assert!(label.starts_with("Large file ("), "got {label:?}");
        assert!(
            label.contains("MiB"),
            "the size belongs on the label: {label:?}"
        );
    }

    /// A file past `HUGE_FILE_BYTES` whose first lines are real text.
    ///
    /// Written short and then extended with `set_len`, which is what makes
    /// this affordable: both NTFS and ext4 record the length without writing
    /// the bytes, so the fixture costs a few hundred bytes of I/O rather than
    /// 192 MiB of it. The tail is NULs, which nothing here reads.
    fn a_huge_file(dir: &std::path::Path) -> PathBuf {
        let path = dir.join("enormous.log");
        let mut text = String::new();
        for n in 1..=200 {
            let _ = writeln!(text, "line {n}");
        }
        std::fs::write(&path, &text).expect("the fixture's real lines");

        let file = std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .expect("the fixture reopens");
        file.set_len(bp_buffer::HUGE_FILE_BYTES + 1)
            .expect("the fixture is extended");
        assert!(
            bp_buffer::SizeClass::of(bp_buffer::HUGE_FILE_BYTES + 1).must_stream(),
            "this fixture is not actually huge"
        );
        path
    }

    #[test]
    fn a_huge_document_opens_rather_than_being_refused() {
        // What ADR-0030 changed. It used to be turned away at the door with a
        // message saying the chunked reader was built and not yet connected
        // to a view; it is connected now, so the document opens, gets a tab,
        // and is drawn from disk.
        let dir = tempfile::tempdir().expect("temp dir");
        let path = a_huge_file(dir.path());

        let mut state = AppState::new();
        state.open(path);

        let id = state.workspace.active_id().expect("a tab was opened");
        assert!(state.is_viewer(id), "a huge document is served from disk");
        assert!(
            !state.editors.contains_key(&id),
            "a viewer must have no editor: that is what makes every editing \
             path a no-op rather than an edit to an empty document"
        );
        assert!(
            state.size_label().starts_with("Huge file ("),
            "got {:?}",
            state.size_label()
        );
    }

    #[test]
    fn the_notice_is_access_s_own_sentence_and_blames_the_size() {
        // Not this module's wording. `Access::ReadOnlyBySize` exists so a
        // refusal can say *how* large and distinguish itself from a file that
        // is read-only on disk -- a different problem with a different way
        // out, and telling someone "read-only" without saying which leaves
        // them clicking at permissions that were never at fault.
        let dir = tempfile::tempdir().expect("temp dir");
        let path = a_huge_file(dir.path());

        let mut state = AppState::new();
        state.open(path);

        let notice = state.error.clone().expect("opening one says what it is");
        assert_eq!(
            notice,
            bp_buffer::Access::ReadOnlyBySize {
                bytes: bp_buffer::HUGE_FILE_BYTES + 1
            }
            .message()
        );
        assert!(
            !notice.contains("not yet"),
            "the viewer exists now; saying otherwise is how a limitation \
             becomes folklore in the other direction: {notice}"
        );
        assert!(
            !notice.contains("permission"),
            "the size is the reason: {notice}"
        );
    }

    #[test]
    fn saving_a_document_served_from_disk_writes_nothing_at_all() {
        // **The guard that matters.** `text_of` a viewer is the empty string,
        // so a save that merely did nothing special would encode nothing and
        // atomically write it over the document -- a Ctrl+S that destroys two
        // gigabytes and reports success. The assertion is on the file, not on
        // the return value.
        let dir = tempfile::tempdir().expect("temp dir");
        let path = a_huge_file(dir.path());
        let before = std::fs::metadata(&path).expect("metadata").len();

        let mut state = AppState::new();
        state.open(path.clone());
        let id = state.workspace.active_id().expect("a tab was opened");

        assert_eq!(state.save_document(id, None), SaveResult::Failed);
        assert_eq!(
            std::fs::metadata(&path).expect("metadata").len(),
            before,
            "the document was written over"
        );
        assert_eq!(
            state.save_copy(id, &dir.path().join("copy.log")),
            SaveResult::Failed,
            "an empty file presented as a copy is worse than a refusal"
        );
        assert!(
            !dir.path().join("copy.log").exists(),
            "and it must not be created either"
        );

        let notice = state.error.clone().unwrap_or_default();
        assert!(
            notice.contains("read from disk"),
            "the refusal has to say why: {notice}"
        );
    }

    #[test]
    fn reloading_a_document_served_from_disk_refuses() {
        // `load` slurps. Reloading a document that was deliberately never
        // loaded would undo the whole of ADR-0027 with one menu row -- and it
        // would do it by allocating two gigabytes, which is the failure mode
        // hardest to recover from.
        let dir = tempfile::tempdir().expect("temp dir");
        let path = a_huge_file(dir.path());

        let mut state = AppState::new();
        state.open(path);
        let id = state.workspace.active_id().expect("a tab was opened");

        state.reload();
        assert!(state.is_viewer(id), "still served from disk");
        assert!(
            !state.editors.contains_key(&id),
            "reload put the document in a rope"
        );
    }

    #[test]
    fn the_bytes_of_a_document_served_from_disk_are_refused_rather_than_empty() {
        // Hashing, signing and verifying all come through `active_bytes`. A
        // digest of the empty string presented as a digest of two gigabytes
        // is worse than no answer, because a signature over it verifies.
        let dir = tempfile::tempdir().expect("temp dir");
        let path = a_huge_file(dir.path());

        let mut state = AppState::new();
        state.open(path);

        let refusal = state.active_bytes().expect_err("must not answer");
        assert!(refusal.contains("never held whole"), "got {refusal}");
    }

    #[test]
    fn a_huge_document_draws_in_the_custom_surface_whichever_flag_was_passed() {
        // ADR-0030, and the reason there is one function rather than an `if`
        // in two places: `TextInput` owns its own text and cannot be handed a
        // window of a file, so there is no build in which this document opens
        // for one user and is refused for another.
        let dir = tempfile::tempdir().expect("temp dir");
        let path = a_huge_file(dir.path());

        for flag in [false, true] {
            let mut state = AppState::new();
            state.editor_view = flag;
            state.open(path.clone());
            assert!(
                state.uses_custom_surface(),
                "--editor-view={flag} left a huge document in a TextInput"
            );
        }
    }

    #[test]
    fn an_ordinary_document_still_follows_the_flag() {
        // The other half of the same function. ADR-0030 changed what a *huge*
        // document does and nothing else; a flag that started claiming every
        // document would have retired `TextInput` by accident, and with it
        // input-method composition.
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("small.txt");
        std::fs::write(&path, "a few lines\nof text\n").expect("write");

        for flag in [false, true] {
            let mut state = AppState::new();
            state.editor_view = flag;
            state.open(path.clone());
            assert_eq!(state.uses_custom_surface(), flag);
        }
    }

    #[test]
    fn closing_a_viewer_lets_go_of_the_file() {
        // An open handle and a line index per tab. Thirty huge documents
        // opened and closed in a session would otherwise be thirty handles.
        let dir = tempfile::tempdir().expect("temp dir");
        let path = a_huge_file(dir.path());

        let mut state = AppState::new();
        state.open(path);
        let id = state.workspace.active_id().expect("a tab was opened");
        assert!(state.is_viewer(id));

        state.close(id);
        assert!(!state.viewers.contains_key(&id));
    }

    #[test]
    fn the_readout_says_which_lines_are_on_screen() {
        // A viewer has no caret, because a caret is a position in a rope and
        // the rope is a disk. What it has instead is a viewport, and saying
        // where that is answers the question a reader of a huge log has.
        let dir = tempfile::tempdir().expect("temp dir");
        let path = a_huge_file(dir.path());

        let mut state = AppState::new();
        state.open(path);
        state.drawn_rows = 30;

        assert_eq!(state.cursor_label(), "Ln 1-30");
        assert!(
            state
                .active_viewer_mut()
                .expect("the active document is a viewer")
                .scroll_by(100, 30)
        );
        assert_eq!(state.cursor_label(), "Ln 101-130");
    }

    #[test]
    fn a_byte_count_reads_the_way_a_person_would_say_it() {
        assert_eq!(human_bytes(512), "512 bytes");
        assert_eq!(human_bytes(2048), "2.0 KiB");
        assert_eq!(human_bytes(9 * 1024 * 1024), "9.0 MiB");
        assert_eq!(human_bytes(3 * 1024 * 1024 * 1024), "3.0 GiB");
    }

    // --- opening reads a header, not a file --------------------------------

    #[test]
    fn an_encrypted_document_is_recognised_from_its_header_alone() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("secret.bpadx");
        let sealed = bp_crypto::seal(
            b"a document\n",
            "correct horse",
            bp_crypto::SealOptions::default(),
        )
        .expect("seal");
        std::fs::write(&path, &sealed).expect("write");

        let header = read_header(&path).expect("header");
        assert_eq!(
            header.len(),
            bp_crypto::MAGIC_LEN,
            "a header is read, not a file"
        );
        assert!(
            bp_crypto::is_bpadx(&header),
            "the magic must be recognisable from the header alone, or the user \
             is shown their own ciphertext"
        );
    }

    #[test]
    fn a_file_shorter_than_the_magic_is_plaintext_and_not_an_error() {
        // The short-read trap. `read` may return fewer bytes than asked for
        // without being at the end, so this uses `read_exact` -- and a file
        // genuinely shorter than the magic must still open, as the ordinary
        // small text file it is.
        let dir = tempfile::tempdir().expect("temp dir");
        for (name, contents) in [("empty.txt", ""), ("tiny.txt", "hi")] {
            let path = dir.path().join(name);
            std::fs::write(&path, contents).expect("write");
            let header = read_header(&path).expect("a short file is not an error");
            assert!(
                !bp_crypto::is_bpadx(&header),
                "{name} is not encrypted and must not be treated as though it were"
            );
        }
    }

    #[test]
    fn a_plaintext_document_that_starts_like_text_is_not_taken_for_a_bpadx() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("notes.txt");
        std::fs::write(&path, "BPAD is not BPADX\0 and this is prose").expect("write");
        assert!(!bp_crypto::is_bpadx(&read_header(&path).expect("header")));
    }

    // --- new window, diagnostics -----------------------------------------

    #[test]
    fn a_new_window_launches_the_same_executable_with_no_arguments() {
        // Spawning a real process is heavier than this needs: what has to be
        // right is which binary would run and that none of this process's
        // own arguments follow it, not that a window then opens.
        let command = new_window_command().expect("this test binary can resolve its own path");
        assert_eq!(
            command.get_program(),
            std::env::current_exe().unwrap().as_os_str(),
            "a second instance of the same binary, not a different one"
        );
        assert_eq!(
            command.get_args().count(),
            0,
            "a fresh instance opens with no file, same as launching the app fresh"
        );
    }

    #[test]
    fn the_diagnostics_report_names_the_version_and_every_resolved_directory() {
        let report = diagnostics_report();
        assert!(report.contains(env!("CARGO_PKG_VERSION")), "got {report}");
        assert!(report.contains("Config file:"), "got {report}");
        assert!(report.contains("State directory:"), "got {report}");
        assert!(report.contains("Data directory:"), "got {report}");
        // Never a panic for an environment that resolves none of them --
        // `named` says "not available" rather than unwrapping.
        assert!(!report.contains("None"), "got {report}");
    }
}
