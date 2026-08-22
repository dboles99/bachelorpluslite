//! Running one cell of a literate document (ADR-0043, ADR-0045).
//!
//! **This module is the only place `bp-notebook`'s cell kinds and
//! `bp-execution`'s languages are both visible**, and that is deliberate on
//! both sides: neither crate depends on the other, so the mapping between
//! eight kinds and three interpreters has to live in the shell. It is the
//! same shape as `bp-secrets` and `bp-redaction` meeting here rather than
//! knowing about each other.
//!
//! ## Consent
//!
//! specs.md section 15: never auto-run an opened or pasted notebook.
//! [`AppState::run_cell`] takes both gestures **by value** from its caller,
//! and does not manufacture either. `UserGesture::from_user_command()` is
//! called in exactly one place in this crate -- the dispatch arm for a Run
//! menu click -- which is what both crates ask for and what makes the
//! assertion greppable.
//!
//! Listing the cells is not gated, and that is a decision rather than an
//! oversight: reading a document to say what is in it is what every other
//! menu here does, and demanding a gesture to *look* would make the gesture
//! mean something weaker than it does now.
//!
//! ## Why the run is polled
//!
//! `bp_execution::run` blocks until the cell ends or the deadline kills it.
//! A window cannot afford that -- a cell that sleeps would freeze it for as
//! long as it sleeps, and a hung one would freeze it for the whole timeout,
//! turning the protection against a hang into a scheduled one. So the shell
//! drives `Run::poll` from a timer, exactly as ADR-0042's scan is driven.

use bp_notebook::{CellKind, Notebook};

use super::AppState;

/// A document that has runnable pieces in it, and which kind it is.
///
/// **Both arms produce a `bp_notebook::Notebook`**, which is the whole reason
/// a `.md` file costs almost nothing here: consent, the refusal of a prose
/// cell, and everything else built on `Notebook` work on a README without
/// knowing it was ever one. What differs is only how a row names a piece's
/// position -- a notebook has cells, a Markdown file has lines.
#[derive(Debug)]
pub(crate) enum Source {
    /// A `.ipynb`. Carries the whole import, because its warnings are the
    /// only place "we could not name this language" survives.
    Ipynb(Box<bp_notebook::Import>),
    /// A `.md`, read as prose and fenced blocks (ADR-0045).
    Markdown(Box<bp_notebook::MarkdownDocument>),
}

impl Source {
    pub(crate) fn notebook(&self) -> &Notebook {
        match self {
            Self::Ipynb(import) => import.notebook(),
            Self::Markdown(document) => document.notebook(),
        }
    }

    /// How a row names the `index`-th cell's position.
    ///
    /// The reader is looking at the file, so the name has to be findable in
    /// it: a cell number for a notebook, whose JSON has cells in order, and a
    /// **line** for Markdown, whose cells are this product's idea rather than
    /// anything written in the document.
    fn position(&self, index: usize) -> String {
        match self {
            Self::Ipynb(_) => format!("Cell {}", index + 1),
            Self::Markdown(document) => match document.line_of(index) {
                Some(line) => format!("Line {line}"),
                None => format!("Block {}", index + 1),
            },
        }
    }

    /// The 1-based line the `index`-th cell begins on.
    ///
    /// Markdown knows exactly, because its cells *are* spans of the file.
    /// A `.ipynb` does not: its cells are JSON objects, and the line a
    /// reader would scroll to is the line of the `"source"` array, which
    /// `bp-notebook` has no reason to record. `None` says so rather than
    /// guessing, and the caller leaves the reader where they are.
    pub(crate) fn line_of(&self, index: usize) -> Option<usize> {
        match self {
            Self::Ipynb(_) => None,
            Self::Markdown(document) => document.line_of(index),
        }
    }

    /// How many pieces name a language this build cannot run.
    ///
    /// Counted differently on each side, and neither way works for the other:
    /// a notebook's unknown languages survive only as import *warnings*,
    /// because `Raw` is equally what a deliberately-raw cell is; a Markdown
    /// file has no warnings, but every `Raw` cell in one *is* an unlabelled
    /// or unrecognised fence, because its prose becomes `Markdown` cells.
    fn unnamed(&self) -> usize {
        match self {
            Self::Ipynb(import) => import
                .warnings()
                .iter()
                .filter(|warning| {
                    matches!(
                        warning,
                        bp_notebook::ImportWarning::UnknownCodeLanguage { .. }
                    )
                })
                .count(),
            Self::Markdown(document) => document
                .notebook()
                .cells()
                .iter()
                .filter(|cell| cell.kind() == CellKind::Raw)
                .count(),
        }
    }
}

/// Why a document that read correctly still offers nothing to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unnamed {
    /// A `.ipynb` with no `metadata.kernelspec`.
    NotebookWithoutKernelspec,
    /// Fenced blocks with no language after the backticks.
    FencesWithoutLanguage,
}

/// A cell run in flight, and what it is about.
#[derive(Debug)]
pub(crate) struct CellRun {
    run: bp_execution::Run,
    /// What the panel calls it while it is going and after it ends.
    label: String,
}

/// One row the Run menu can offer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RunRow {
    /// What the row says.
    pub(crate) label: String,
    /// False for a cell whose language has no runner yet. **Greyed rather
    /// than omitted**: a reader looking at a Rust cell should be told it
    /// cannot run, not left to wonder where it went.
    pub(crate) enabled: bool,
}

/// One row of the cell outline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OutlineRow {
    /// "Line 42" or "Cell 3", whichever the document can be searched by.
    pub(crate) position: String,
    pub(crate) kind: String,
    /// The first thing the cell says, for telling one from another.
    pub(crate) summary: String,
    /// Where a click goes, when there is anywhere to go.
    pub(crate) line: Option<usize>,
    /// Whether the Run menu would offer it. Shown so the outline and the menu
    /// agree in front of the reader rather than only in the code.
    pub(crate) runnable: bool,
}

/// What the Run menu should show for the active document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RunMenu {
    /// The document is not a notebook. Says what would make it one.
    NotANotebook,
    /// It is a notebook and could not be read. Carries the reason.
    ///
    /// Distinct from an empty list on purpose: an empty Run menu reads as
    /// "this notebook has nothing to run", which is a different and wrong
    /// statement about a file that failed to parse.
    Unreadable(String),
    /// The cells, in document order.
    Cells(Vec<RunRow>),
    /// It read, and every code cell in it is of an unknown language.
    ///
    /// **Its own case, because "no cells that can run" would be true and
    /// baffling.** A Jupyter file names its language once for the whole
    /// notebook, in `metadata.kernelspec`, so a file written without one has
    /// code cells this product cannot identify -- `bp-notebook` turns each
    /// into a `Raw` cell and says so in an `ImportWarning`. Carrying that
    /// through is what makes `import_ipynb` returning a report rather than a
    /// notebook worth the extra call.
    NoKnownLanguage {
        /// How many pieces were affected.
        count: usize,
        /// Which shape of document it is, because the fix differs: a
        /// notebook needs a kernelspec, a fence needs a word after its
        /// backticks.
        why: Unnamed,
    },
}

/// The interpreter a cell kind runs under, if one exists.
///
/// `None` for the two executable kinds ADR-0040 deferred by name -- Rust is a
/// compile step rather than an interpreter, and SQL has no engine to be
/// against -- and for every prose kind, which has nothing to run.
pub(crate) fn language_of(kind: CellKind) -> Option<bp_execution::Language> {
    match kind {
        CellKind::Python => Some(bp_execution::Language::Python),
        CellKind::PowerShell => Some(bp_execution::Language::PowerShell),
        CellKind::Shell => Some(bp_execution::Language::Shell),
        CellKind::Rust | CellKind::Sql => None,
        CellKind::PlainText | CellKind::Markdown | CellKind::Raw => None,
    }
}

/// What to call a cell kind in a menu row.
fn kind_label(kind: CellKind) -> &'static str {
    match kind {
        CellKind::Python => "Python",
        CellKind::PowerShell => "PowerShell",
        CellKind::Shell => "Shell",
        CellKind::Rust => "Rust",
        CellKind::Sql => "SQL",
        CellKind::PlainText => "Text",
        CellKind::Markdown => "Markdown",
        CellKind::Raw => "Raw",
    }
}

/// The first line of a cell, short enough for a menu row.
///
/// A menu row is a handle, not a preview: enough to tell one cell from
/// another in a document the reader is looking at anyway. Whitespace-only
/// leading lines are skipped so a cell that starts with a blank line is not
/// labelled with nothing.
fn first_line(source: &str) -> String {
    const MAX: usize = 48;
    let line = source
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("");
    if line.chars().count() <= MAX {
        return line.to_owned();
    }
    let cut: String = line.chars().take(MAX).collect();
    format!("{cut}…")
}

/// The rows a notebook offers, in document order.
///
/// Pure, and separate from [`AppState::run_menu`] for that reason: everything
/// decided here -- which cells appear, what they are called, which of them
/// grey -- can be asserted against a notebook built from a string, with no
/// window, no file and no interpreter anywhere near it.
pub(crate) fn rows_of(source: &Source) -> Vec<RunRow> {
    source
        .notebook()
        .cells()
        .iter()
        .enumerate()
        .filter(|(_, cell)| cell.kind().is_executable())
        .map(|(index, cell)| RunRow {
            // The cell's position in the *file*, one-based, because the file
            // is what the reader is looking at. Numbering the runnable cells
            // 1, 2, 3 would name something that appears nowhere in the
            // document they can see.
            label: format!(
                "{} · {} · {}",
                source.position(index),
                kind_label(cell.kind()),
                first_line(cell.source())
            ),
            enabled: language_of(cell.kind()).is_some(),
        })
        .collect()
}

impl AppState {
    /// Parse the active document as a notebook, if it is one.
    ///
    /// **Not cached.** The document's text *is* the notebook, and a parse
    /// kept across edits would list cells the file no longer has. The cost is
    /// paid when the Run menu opens rather than on every refresh, which is
    /// what keeps it off the typing path.
    pub(crate) fn notebook(&self) -> Option<Result<Source, String>> {
        self.workspace.active()?;
        let text = self.active_text();
        match self.format() {
            bp_formats::Format::Notebook => {
                let value: serde_json::Value = match serde_json::from_str(&text) {
                    Ok(value) => value,
                    Err(e) => return Some(Err(e.to_string())),
                };
                Some(
                    bp_notebook::import_ipynb(&value)
                        .map(|import| Source::Ipynb(Box::new(import)))
                        .map_err(|e| e.to_string()),
                )
            }
            // Markdown never fails to read -- an unclosed fence is a fence
            // that reaches the end, not an error -- so this arm has no
            // failure to report (ADR-0045).
            bp_formats::Format::Markdown => Some(Ok(Source::Markdown(Box::new(
                bp_notebook::markdown::parse(&text),
            )))),
            _ => None,
        }
    }

    /// The rows the Run menu should offer for the active document.
    pub(crate) fn run_menu(&self) -> RunMenu {
        let Some(parsed) = self.notebook() else {
            return RunMenu::NotANotebook;
        };
        let source = match parsed {
            Ok(source) => source,
            Err(reason) => return RunMenu::Unreadable(reason),
        };
        let rows = rows_of(&source);
        if rows.is_empty() {
            let count = source.unnamed();
            if count > 0 {
                return RunMenu::NoKnownLanguage {
                    count,
                    why: match source {
                        Source::Ipynb(_) => Unnamed::NotebookWithoutKernelspec,
                        Source::Markdown(_) => Unnamed::FencesWithoutLanguage,
                    },
                };
            }
        }
        RunMenu::Cells(rows)
    }

    /// Notebook ▸ Cell Outline (ADR-0045): every cell, prose included.
    ///
    /// **Not the same list as the Run menu's**, and the difference is the
    /// point: the Run menu offers what can run, and an outline is a map of
    /// the whole document. A reader of a runbook wants to see the prose
    /// headings between the examples, which is most of what makes it a
    /// runbook rather than a script.
    pub(crate) fn outline(&self) -> Vec<OutlineRow> {
        let Some(Ok(source)) = self.notebook() else {
            return Vec::new();
        };
        source
            .notebook()
            .cells()
            .iter()
            .enumerate()
            .map(|(index, cell)| OutlineRow {
                position: source.line_of(index).map_or_else(
                    || format!("Cell {}", index + 1),
                    |line| format!("Line {line}"),
                ),
                kind: kind_label(cell.kind()).to_owned(),
                summary: first_line(cell.source()),
                line: source.line_of(index),
                runnable: language_of(cell.kind()).is_some(),
            })
            .collect()
    }

    /// Put the caret at the `index`-th cell, for a click on an outline row.
    ///
    /// `None` when there is no line to go to -- a `.ipynb`'s cells have none
    /// -- and the caller then leaves the reader where they are rather than
    /// scrolling somewhere arbitrary.
    pub(crate) fn go_to_cell(&mut self, index: usize) -> Option<std::ops::Range<usize>> {
        let line = {
            let Some(Ok(source)) = self.notebook() else {
                return None;
            };
            source.line_of(index)?
        };
        // Through the editor rather than through `AppState::go_to_line`,
        // which refuses without `--editor-view` because it moves a caret the
        // `TextInput` surface does not expose. A *selection* works in both,
        // which is how Find jumps in either one.
        let editor = self.active_editor_mut()?;
        editor.go_to_line(line);
        let start = editor.cursor();
        // **The whole line, not a caret at its start.** A zero-width
        // selection gives `TextInput` nothing to scroll to, so the jump
        // silently does nothing in the default surface -- which is how this
        // was found. Selecting the line also shows the reader which cell
        // they landed on, which a bare caret would not.
        let length = editor.buffer().line_len_chars(line.saturating_sub(1));
        Some(start..start + length)
    }

    /// Start running the `index`-th *executable* cell of the active notebook.
    ///
    /// Both gestures come from the caller by value, and this function does
    /// not make either -- see the module docs. Returns whether a run started;
    /// when it did not, [`AppState::error`] says why.
    /// Run ▸ Run Document (ADR-0048): the whole active file as a script.
    ///
    /// **The sibling of `run_cell`, for a file that is a script rather than
    /// a notebook.** A `.py` is not a notebook and has no cells, so the Run
    /// menu had nothing to offer it even though `bp-execution` could run it
    /// perfectly well.
    ///
    /// Takes a `UserGesture` by value for the same reason `run_cell` does,
    /// and it is the *whole* reason this is not a convenience wrapper: ADR-0011
    /// and specs.md section 15 say notebook content is never auto-run, and a
    /// gesture that only a click can produce is how that is enforced by the
    /// type system rather than by everyone remembering.
    ///
    /// Unlike `run_cell` there is no `bp-notebook` request to go through --
    /// there is no cell, no prose to refuse, and the source is the document.
    pub(crate) fn run_document(&mut self, gesture: bp_execution::UserGesture) -> bool {
        let Some(language) = self.document_language() else {
            self.error = Some(
                "there is no runner for this kind of file: Run Document works                  on .py, .ps1 and .sh"
                    .to_owned(),
            );
            return false;
        };

        // The whole document, which `active_text` copies. A one-off on a menu
        // click, never the typing path (R011 rule 5).
        let source = self.active_text();
        if source.trim().is_empty() {
            self.error = Some("there is nothing in this document to run".to_owned());
            return false;
        }

        let label = self
            .workspace
            .active()
            .and_then(bp_core::Document::path)
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .map_or_else(|| format!("{language} document"), ToOwned::to_owned);

        match bp_execution::Run::start(language, &source, gesture) {
            Ok(run) => {
                self.run_summary = format!("Running {label}…");
                self.run_output.clear();
                self.run_errors.clear();
                self.run = Some(CellRun { run, label });
                self.run_open = true;
                true
            }
            Err(e) => {
                self.error = Some(e.to_string());
                self.run_summary = format!("{label} — {e}");
                self.run_output.clear();
                self.run_errors.clear();
                self.run_open = true;
                false
            }
        }
    }

    /// Run ▸ Interpreters (ADR-0048): which interpreter each language
    /// resolves to on this machine.
    ///
    /// **Not "Choose Interpreter", which `MENU_MAP.md` named until
    /// 2026-08-22.** An interpreter here is *resolved*, not chosen -- each
    /// language has a documented fallback chain (`python3` then `python`,
    /// `pwsh` then `powershell`, `sh`) and the first that answers wins.
    /// Letting a user point the runner at an arbitrary binary is a security
    /// decision, not a menu row, and ADR-0011's "notebook content never
    /// auto-runs" is the neighbourhood it would sit in.
    ///
    /// What the row *can* honestly do is answer the question the chain makes
    /// unanswerable from outside: **why did my cell not run?** A machine with
    /// neither `python3` nor `python` produced a failure naming neither.
    ///
    /// Spawns each candidate until one answers, so this is a menu-click
    /// operation and never a refresh.
    pub(crate) fn interpreters_report(&self) -> String {
        let mut lines = vec![
            "What each runnable language resolves to on this machine:".to_owned(),
            String::new(),
        ];
        for language in bp_execution::Language::all() {
            let candidates = language.candidates().join(", ");
            match language.interpreter() {
                Some(found) => lines.push(format!("- {language}: {found}")),
                // Names what it looked for. "Not found" without the list is
                // the same dead end the chain already was.
                None => lines.push(format!("- {language}: not found — tried {candidates}")),
            }
        }
        lines.push(String::new());
        lines.push(
            "The first candidate that answers wins, and the order is fixed. \
             There is no way to point the runner at a different binary: \
             choosing one is a security decision rather than a setting."
                .to_owned(),
        );
        lines.join("\n")
    }

    /// Which language the *whole* active document is, by its extension.
    ///
    /// Deliberately the extension and not the format registry: `bp-formats`
    /// answers "what shape is this text" for the Data menu, and a `.py` file
    /// is plain text to it. Running a file is a question about what the file
    /// claims to be.
    pub(crate) fn document_language(&self) -> Option<bp_execution::Language> {
        let extension = self
            .workspace
            .active()
            .and_then(bp_core::Document::path)
            .and_then(|path| path.extension())
            .and_then(|extension| extension.to_str())?
            .to_ascii_lowercase();

        match extension.as_str() {
            "py" => Some(bp_execution::Language::Python),
            "ps1" => Some(bp_execution::Language::PowerShell),
            "sh" => Some(bp_execution::Language::Shell),
            _ => None,
        }
    }

    /// The filename the export dialog suggests: the document's own stem with
    /// an `.ipynb` extension.
    ///
    /// The stem rather than the whole name, so `runbook.md` suggests
    /// `runbook.ipynb` and not `runbook.md.ipynb` -- `with_bpadx_extension`
    /// appends deliberately, because an encrypted `notes.txt` is still a
    /// `notes.txt`; an exported notebook is a different document and takes
    /// the name outright.
    pub(crate) fn suggested_ipynb_name(&self) -> String {
        self.workspace
            .active()
            .and_then(bp_core::Document::path)
            .and_then(|path| path.file_stem())
            .and_then(|stem| stem.to_str())
            .map_or_else(
                || "notebook.ipynb".to_owned(),
                |stem| format!("{stem}.ipynb"),
            )
    }

    /// Notebook ▸ Export as .ipynb (ADR-0048): the active document written
    /// out as a Jupyter notebook.
    ///
    /// **The useful direction is Markdown to `.ipynb`**, and it is what makes
    /// this a row rather than a round trip. ADR-0045 made a `.md` with fenced
    /// blocks readable as a `Notebook`; this is the other end of that, so a
    /// runbook written as ordinary prose can leave as something Jupyter
    /// opens. Exporting an `.ipynb` that was imported as one is allowed and
    /// nearly a no-op -- refusing it would mean explaining a distinction the
    /// user has no reason to care about.
    ///
    /// Returns the JSON to write, or the reason there is none. The shell owns
    /// the file dialog; this owns what goes in the file.
    pub(crate) fn export_ipynb(&self) -> Result<String, String> {
        let Some(source) = self.notebook() else {
            return Err("There is no document to export.".to_owned());
        };
        let source = source?;
        let notebook = source.notebook();
        if notebook.cells().is_empty() {
            return Err(
                "This document has no cells to export. A `.md` needs fenced                  code blocks; an `.ipynb` needs at least one cell."
                    .to_owned(),
            );
        }

        // Pretty-printed rather than compact: an `.ipynb` is JSON somebody
        // may well open in this very editor, and `bp-notebook`'s own raw view
        // is formatted for the same reason.
        serde_json::to_string_pretty(&bp_notebook::ipynb::export_ipynb(notebook))
            .map_err(|error| format!("The notebook could not be written: {error}"))
    }

    pub(crate) fn run_cell(
        &mut self,
        index: usize,
        notebook_gesture: bp_notebook::UserGesture,
        execution_gesture: bp_execution::UserGesture,
    ) -> bool {
        let Some(Ok(source)) = self.notebook() else {
            self.error = Some("this document has nothing this product knows how to run".to_owned());
            return false;
        };
        let notebook = source.notebook();
        let executable: Vec<&bp_notebook::Cell> = notebook
            .cells()
            .iter()
            .filter(|cell| cell.kind().is_executable())
            .collect();
        let Some(cell) = executable.get(index) else {
            self.error = Some("that cell is no longer there".to_owned());
            return false;
        };
        let Some(language) = language_of(cell.kind()) else {
            self.error = Some(format!(
                "{} cells cannot be run yet: there is no runner for them",
                kind_label(cell.kind())
            ));
            return false;
        };

        // Through `request_run` rather than reading the source directly, so
        // the one description of a run this product makes is the one
        // `bp-notebook` built -- including its refusal of a prose cell.
        let request = match notebook.request_run(cell.id(), notebook_gesture) {
            Ok(request) => request,
            Err(e) => {
                self.error = Some(e.to_string());
                return false;
            }
        };

        let label = format!(
            "{} · {}",
            kind_label(cell.kind()),
            first_line(cell.source())
        );
        match bp_execution::Run::start(language, request.source(), execution_gesture) {
            Ok(run) => {
                self.run_summary = format!("Running {label}…");
                self.run_output.clear();
                self.run_errors.clear();
                self.run = Some(CellRun { run, label });
                self.run_open = true;
                true
            }
            Err(e) => {
                // An interpreter that is not installed is the common case
                // here, and its message names the language and what was
                // tried -- which is the whole of what a reader needs.
                self.error = Some(e.to_string());
                self.run_summary = format!("{label} — {e}");
                self.run_output.clear();
                self.run_errors.clear();
                self.run_open = true;
                false
            }
        }
    }

    /// Ask the run in flight whether it is done. Returns whether it is still
    /// going, which is all the shell's timer needs to act on.
    pub(crate) fn poll_run(&mut self) -> bool {
        let Some(CellRun { run, label }) = self.run.take() else {
            return false;
        };
        match run.poll() {
            bp_execution::Progress::Running(run) => {
                self.run_summary = format!("Running {label}… {:.1}s", run.elapsed().as_secs_f64());
                self.run = Some(CellRun { run, label });
                true
            }
            bp_execution::Progress::Finished(outcome) => {
                self.finish_run(&label, &outcome, false);
                false
            }
        }
    }

    /// End the run now, keeping what it produced.
    pub(crate) fn stop_run(&mut self) {
        let Some(CellRun { run, label }) = self.run.take() else {
            return;
        };
        let outcome = run.stop();
        self.finish_run(&label, &outcome, true);
    }

    /// Whether a cell is running, which is what greys the Stop row.
    pub(crate) fn is_running(&self) -> bool {
        self.run.is_some()
    }

    /// Put an outcome into the panel.
    fn finish_run(&mut self, label: &str, outcome: &bp_execution::RunOutcome, stopped: bool) {
        self.run_output = outcome.stdout().to_owned();
        self.run_errors = outcome.stderr().to_owned();
        let ending = if stopped {
            Ending::Stopped
        } else if outcome.timed_out() {
            Ending::TimedOut
        } else {
            Ending::Finished(outcome.exit_code())
        };
        self.run_summary = run_summary(
            label,
            ending,
            outcome.duration(),
            outcome.stdout_truncated() || outcome.stderr_truncated(),
        );
    }
}

/// How a run ended, in the terms the summary line needs.
///
/// Named rather than passed as three booleans, because "stopped" and "timed
/// out" are mutually exclusive and a pair of flags would allow a state that
/// says both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ending {
    /// Ran to its own end, with whatever exit code it had.
    Finished(Option<i32>),
    /// Killed at the deadline.
    TimedOut,
    /// Killed because the reader asked.
    Stopped,
}

/// The one line above a cell's output.
///
/// A free function so its test asserts the product's sentence rather than a
/// copy of it, the same reason `viewer_label` is one. Every clause here is a
/// thing the reader cannot see from the output itself: whether it was cut
/// short, and by what.
pub(crate) fn run_summary(
    label: &str,
    ending: Ending,
    duration: std::time::Duration,
    capped: bool,
) -> String {
    let mut summary = format!("{label} — ");
    match ending {
        Ending::Stopped => summary.push_str("stopped"),
        Ending::TimedOut => summary.push_str("timed out"),
        Ending::Finished(Some(0)) => summary.push_str("finished"),
        Ending::Finished(Some(code)) => summary.push_str(&format!("exit {code}")),
        Ending::Finished(None) => summary.push_str("ended without an exit code"),
    }
    summary.push_str(&format!(" in {:.1}s", duration.as_secs_f64()));
    if capped {
        summary.push_str(" · output capped");
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Build a notebook from `.ipynb` JSON, with no file and no window.
    ///
    /// **The kernelspec is not decoration.** Jupyter names a notebook's
    /// language once for the whole file, so a fixture without one has code
    /// cells of no known language -- which is a real case, tested separately
    /// below, and not the one most of these tests are about.
    fn import_of(cells: &str) -> bp_notebook::Import {
        let json = format!(
            r#"{{"nbformat":4,"nbformat_minor":5,"metadata":{{"kernelspec":{{"language":"python","name":"python3"}}}},"cells":[{cells}]}}"#
        );
        let value: serde_json::Value =
            serde_json::from_str(&json).expect("the fixture is valid JSON");
        bp_notebook::import_ipynb(&value).expect("the fixture is a valid notebook")
    }

    fn ipynb_of(cells: &str) -> Source {
        Source::Ipynb(Box::new(import_of(cells)))
    }

    fn markdown_of(text: &str) -> Source {
        Source::Markdown(Box::new(bp_notebook::markdown::parse(text)))
    }

    fn code(source: &str) -> String {
        format!(
            r#"{{"cell_type":"code","metadata":{{}},"source":["{source}"],"outputs":[],"execution_count":null}}"#
        )
    }

    fn prose(source: &str) -> String {
        format!(r#"{{"cell_type":"markdown","metadata":{{}},"source":["{source}"]}}"#)
    }

    #[test]
    fn every_cell_kind_is_answered_about_its_runner_one_way_or_the_other() {
        // A match that grew an arm for a new kind by accident would be a Rust
        // cell quietly acquiring Python's interpreter, which is exactly what
        // ADR-0040 named as the thing not to do.
        assert_eq!(
            language_of(CellKind::Python),
            Some(bp_execution::Language::Python)
        );
        assert_eq!(
            language_of(CellKind::PowerShell),
            Some(bp_execution::Language::PowerShell)
        );
        assert_eq!(
            language_of(CellKind::Shell),
            Some(bp_execution::Language::Shell)
        );
        assert_eq!(language_of(CellKind::Rust), None, "deferred by ADR-0040");
        assert_eq!(language_of(CellKind::Sql), None, "deferred by ADR-0040");
        for prose in [CellKind::PlainText, CellKind::Markdown, CellKind::Raw] {
            assert_eq!(language_of(prose), None, "{prose:?} has nothing to run");
        }
    }

    #[test]
    fn an_executable_kind_with_no_runner_is_offered_and_greyed_rather_than_hidden() {
        // The distinction `row_enabled` exists for, and the reason `rows_of`
        // filters on `is_executable` rather than on `language_of`: a Rust
        // cell is executable in `bp-notebook`'s sense and has no runner here,
        // and a reader must be told that rather than left looking for a row
        // that is not there.
        assert!(CellKind::Rust.is_executable());
        assert!(language_of(CellKind::Rust).is_none());
    }

    #[test]
    fn a_row_is_numbered_by_its_position_in_the_file() {
        // Not by its position among the runnable cells. The reader is looking
        // at the JSON, and a number that appears nowhere in it names nothing.
        let source = ipynb_of(&format!(
            "{},{},{},{}",
            prose("intro"),
            code("print(1)"),
            prose("more"),
            code("print(2)")
        ));

        let rows = rows_of(&source);

        assert_eq!(rows.len(), 2, "two code cells among four");
        assert!(
            rows[0].label.starts_with("Cell 2 \u{b7} "),
            "{:?}",
            rows[0].label
        );
        assert!(
            rows[1].label.starts_with("Cell 4 \u{b7} "),
            "{:?}",
            rows[1].label
        );
    }

    #[test]
    fn a_row_names_the_language_and_the_first_thing_the_cell_does() {
        let rows = rows_of(&ipynb_of(&code("print('hello')")));

        assert_eq!(rows.len(), 1);
        assert!(rows[0].label.contains("Python"), "{:?}", rows[0].label);
        assert!(
            rows[0].label.contains("print('hello')"),
            "{:?}",
            rows[0].label
        );
        assert!(rows[0].enabled);
    }

    #[test]
    fn a_notebook_with_no_kernelspec_has_code_cells_of_no_known_language() {
        // The case that would otherwise show "no cells that can run", which
        // is true and tells the reader nothing they can act on. Jupyter names
        // the language once for the file; without it, `bp-notebook` keeps
        // each code cell verbatim as `Raw` and warns.
        let json = r#"{"nbformat":4,"nbformat_minor":5,"metadata":{},"cells":[{"cell_type":"code","metadata":{},"source":["print(1)"],"outputs":[],"execution_count":null}]}"#;
        let value: serde_json::Value = serde_json::from_str(json).expect("valid JSON");
        let import = bp_notebook::import_ipynb(&value).expect("a valid notebook");
        let source = Source::Ipynb(Box::new(import));

        assert!(
            rows_of(&source).is_empty(),
            "a cell of unknown language must not be offered a runner"
        );
        assert_eq!(
            source.unnamed(),
            1,
            "and the reason must survive the import"
        );
    }

    #[test]
    fn a_genuinely_raw_cell_is_not_counted_as_an_unknown_language() {
        // `Raw` is both "carried verbatim on purpose" and "we could not tell
        // what this was", which is why the count comes from the warnings
        // rather than from the cells.
        let source = ipynb_of(r#"{"cell_type":"raw","metadata":{},"source":["verbatim"]}"#);
        assert_eq!(source.unnamed(), 0);
    }

    #[test]
    fn a_notebook_of_nothing_but_prose_offers_no_rows() {
        let source = ipynb_of(&format!("{},{}", prose("one"), prose("two")));
        assert!(rows_of(&source).is_empty());
    }

    #[test]
    fn a_label_skips_leading_blank_lines_rather_than_naming_a_cell_nothing() {
        assert_eq!(first_line("\n\n  print(1)\n"), "print(1)");
        assert_eq!(first_line(""), "");
        assert_eq!(first_line("   \n\t\n"), "");
    }

    #[test]
    fn a_long_first_line_is_cut_and_says_it_was() {
        let long = "x".repeat(200);
        let label = first_line(&long);
        assert!(label.chars().count() < 60, "{}", label.chars().count());
        assert!(label.ends_with('\u{2026}'), "{label}");
    }

    #[test]
    fn a_label_is_cut_by_characters_and_not_by_bytes() {
        // Slicing a multi-byte character in half panics, and a cell that
        // starts with a comment in Japanese is not exotic.
        let long = "\u{65e5}".repeat(200);
        let label = first_line(&long);
        assert!(label.ends_with('\u{2026}'));
        assert!(label.chars().count() <= 49);
    }

    #[test]
    fn a_markdown_row_is_named_by_its_line_and_not_by_a_cell_number() {
        // A `.md` has no cells written in it -- they are this product's idea
        // -- so a cell number would name something the reader cannot find.
        // A line number they can go to.
        let source = markdown_of("intro\n\n```python\nprint(1)\n```\n");

        let rows = rows_of(&source);
        assert_eq!(rows.len(), 1);
        assert!(
            rows[0].label.starts_with("Line 3 · Python · "),
            "{:?}",
            rows[0].label
        );
    }

    #[test]
    fn prose_around_a_fence_is_not_offered_as_something_to_run() {
        let source = markdown_of("words\n\n```sh\necho hi\n```\n\nmore words\n");
        let rows = rows_of(&source);

        assert_eq!(rows.len(), 1, "one fence among three cells");
        assert!(rows[0].label.contains("Shell"), "{:?}", rows[0].label);
    }

    #[test]
    fn an_unlabelled_fence_is_counted_as_unnamed_and_the_fix_differs_from_a_notebooks() {
        // Both shapes can read correctly and still offer nothing, and the
        // thing the reader has to do about it is different: a notebook needs
        // a kernelspec, a fence needs a word after its backticks.
        let source = markdown_of("```\nsomething\n```\n");

        assert!(rows_of(&source).is_empty());
        assert_eq!(source.unnamed(), 1);
    }

    #[test]
    fn a_markdown_file_with_no_fences_offers_nothing_and_is_not_an_unnamed_block() {
        // Plain prose is not a broken document, so it must not produce the
        // "name your fences" hint -- there are no fences to name.
        let source = markdown_of("just words, no code at all\n");

        assert!(rows_of(&source).is_empty());
        assert_eq!(source.unnamed(), 0);
    }

    #[test]
    fn a_fence_and_a_notebook_cell_of_the_same_language_produce_the_same_row_but_its_position() {
        // The property that makes ADR-0045 cheap: past the position, a
        // Markdown fence and a notebook cell are the same thing to
        // everything downstream.
        let from_markdown = rows_of(&markdown_of("```python\nprint('x')\n```\n"));
        let from_notebook = rows_of(&ipynb_of(&code("print('x')")));

        assert_eq!(from_markdown.len(), 1);
        assert_eq!(from_notebook.len(), 1);
        assert!(from_markdown[0].enabled && from_notebook[0].enabled);

        let tail = |label: &str| {
            label
                .split_once(" · ")
                .map(|(_, rest)| rest.to_owned())
                .unwrap_or_default()
        };
        assert_eq!(tail(&from_markdown[0].label), tail(&from_notebook[0].label));
    }

    #[test]
    fn the_outline_lists_prose_as_well_as_code() {
        // The difference between an outline and the Run menu, and the reason
        // both exist: a runbook is mostly prose, and a map of it that showed
        // only the code would leave out what the code is for.
        let source = markdown_of("# Heading\n\n```python\nprint(1)\n```\n");
        let rows: Vec<OutlineRow> = source
            .notebook()
            .cells()
            .iter()
            .enumerate()
            .map(|(index, cell)| OutlineRow {
                position: source.line_of(index).map_or_else(
                    || format!("Cell {}", index + 1),
                    |line| format!("Line {line}"),
                ),
                kind: kind_label(cell.kind()).to_owned(),
                summary: first_line(cell.source()),
                line: source.line_of(index),
                runnable: language_of(cell.kind()).is_some(),
            })
            .collect();

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].kind, "Markdown");
        assert!(!rows[0].runnable);
        assert_eq!(rows[1].kind, "Python");
        assert!(rows[1].runnable);
    }

    #[test]
    fn a_markdown_cell_knows_its_line_and_a_notebook_cell_admits_it_does_not() {
        // A `.md` cell *is* a span of the file. A `.ipynb` cell is a JSON
        // object, and the line a reader would scroll to is not something
        // `bp-notebook` records -- so the answer is `None`, not a guess.
        assert_eq!(markdown_of("a\n\n```sh\nx\n```\n").line_of(1), Some(3));
        assert_eq!(ipynb_of(&code("print(1)")).line_of(0), None);
    }

    #[test]
    fn the_summary_says_what_the_output_itself_cannot() {
        // Every clause here is a fact the two streams do not carry: how it
        // ended, how long it took, and whether it was cut off.
        let second = Duration::from_secs(1);
        assert!(
            run_summary(
                "Python \u{b7} print",
                Ending::Finished(Some(0)),
                second,
                false
            )
            .contains("finished")
        );
        assert!(
            run_summary(
                "Python \u{b7} boom",
                Ending::Finished(Some(1)),
                second,
                false
            )
            .contains("exit 1")
        );
        assert!(
            run_summary("Shell \u{b7} sleep", Ending::TimedOut, second, false)
                .contains("timed out")
        );
        assert!(
            run_summary(
                "Python \u{b7} loud",
                Ending::Finished(Some(0)),
                second,
                true
            )
            .contains("capped")
        );
    }

    #[test]
    fn a_stopped_run_is_not_reported_as_a_timeout() {
        // Two different things: one is the cell hanging, the other is the
        // reader changing their mind, and calling the second the first blames
        // the cell for a decision that was not its.
        let summary = run_summary(
            "Shell \u{b7} sleep",
            Ending::Stopped,
            Duration::from_secs(3),
            false,
        );
        assert!(summary.contains("stopped"), "{summary}");
        assert!(!summary.contains("timed out"), "{summary}");
    }
}
