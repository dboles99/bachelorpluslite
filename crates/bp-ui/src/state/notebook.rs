//! Running one cell of a literate document (ADR-0043).
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
        /// How many code cells were affected.
        cells: usize,
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

/// How many code cells the import could not name a language for.
///
/// Counted from the report rather than by re-inspecting the cells: `Raw` is
/// also what a genuinely raw cell is, so the cells alone cannot tell "carried
/// verbatim on purpose" from "we could not tell what this was".
pub(crate) fn unknown_languages(import: &bp_notebook::Import) -> usize {
    import
        .warnings()
        .iter()
        .filter(|warning| {
            matches!(
                warning,
                bp_notebook::ImportWarning::UnknownCodeLanguage { .. }
            )
        })
        .count()
}

/// The rows a notebook offers, in document order.
///
/// Pure, and separate from [`AppState::run_menu`] for that reason: everything
/// decided here -- which cells appear, what they are called, which of them
/// grey -- can be asserted against a notebook built from a string, with no
/// window, no file and no interpreter anywhere near it.
pub(crate) fn rows_of(notebook: &Notebook) -> Vec<RunRow> {
    notebook
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
                "Cell {} · {} · {}",
                index + 1,
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
    pub(crate) fn notebook(&self) -> Option<Result<bp_notebook::Import, String>> {
        self.workspace.active()?;
        if self.format() != bp_formats::Format::Notebook {
            return None;
        }
        let text = self.active_text();
        let value: serde_json::Value = match serde_json::from_str(&text) {
            Ok(value) => value,
            Err(e) => return Some(Err(e.to_string())),
        };
        Some(bp_notebook::import_ipynb(&value).map_err(|e| e.to_string()))
    }

    /// The rows the Run menu should offer for the active document.
    pub(crate) fn run_menu(&self) -> RunMenu {
        let Some(parsed) = self.notebook() else {
            return RunMenu::NotANotebook;
        };
        let import = match parsed {
            Ok(import) => import,
            Err(reason) => return RunMenu::Unreadable(reason),
        };
        let rows = rows_of(import.notebook());
        if rows.is_empty() {
            let unknown = unknown_languages(&import);
            if unknown > 0 {
                return RunMenu::NoKnownLanguage { cells: unknown };
            }
        }
        RunMenu::Cells(rows)
    }

    /// Start running the `index`-th *executable* cell of the active notebook.
    ///
    /// Both gestures come from the caller by value, and this function does
    /// not make either -- see the module docs. Returns whether a run started;
    /// when it did not, [`AppState::error`] says why.
    pub(crate) fn run_cell(
        &mut self,
        index: usize,
        notebook_gesture: bp_notebook::UserGesture,
        execution_gesture: bp_execution::UserGesture,
    ) -> bool {
        let Some(Ok(import)) = self.notebook() else {
            self.error = Some("this document is not a notebook this product can read".to_owned());
            return false;
        };
        let notebook = import.notebook();
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

    fn notebook_of(cells: &str) -> Notebook {
        import_of(cells).into_notebook()
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
        let notebook = notebook_of(&format!(
            "{},{},{},{}",
            prose("intro"),
            code("print(1)"),
            prose("more"),
            code("print(2)")
        ));

        let rows = rows_of(&notebook);

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
        let notebook = notebook_of(&code("print('hello')"));
        let rows = rows_of(&notebook);

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

        assert!(
            rows_of(import.notebook()).is_empty(),
            "a cell of unknown language must not be offered a runner"
        );
        assert_eq!(
            unknown_languages(&import),
            1,
            "and the reason must survive the import"
        );
    }

    #[test]
    fn a_genuinely_raw_cell_is_not_counted_as_an_unknown_language() {
        // `Raw` is both "carried verbatim on purpose" and "we could not tell
        // what this was", which is why the count comes from the warnings
        // rather than from the cells.
        let import = import_of(r#"{"cell_type":"raw","metadata":{},"source":["verbatim"]}"#);
        assert_eq!(unknown_languages(&import), 0);
    }

    #[test]
    fn a_notebook_of_nothing_but_prose_offers_no_rows() {
        let notebook = notebook_of(&format!("{},{}", prose("one"), prose("two")));
        assert!(rows_of(&notebook).is_empty());
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
