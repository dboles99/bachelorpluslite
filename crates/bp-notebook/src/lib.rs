//! Notebooks: the cell model, the operations on it, and Jupyter interchange.
//!
//! specs.md section 12 asks for two separable things — a notebook document and
//! a way to run it — and this crate is only the first. There is no interpreter
//! here, no process, no channel, no thread. Execution is `bp-execution`'s, and
//! the split is not tidiness: it is what lets a notebook be opened, edited,
//! exported and tested on a machine with no Python, no kernel and no network.
//!
//! ## The security rule, and where it is kept
//!
//! specs.md section 15: **never auto-run an opened or pasted notebook or
//! cell.** A crate that cannot run anything can satisfy that rule by accident,
//! and an accident is not a guarantee — the crate that *can* run things will
//! take its instructions from here, so the rule has to survive being read by
//! someone building that one. It is therefore written into the API in three
//! places rather than into a comment:
//!
//! 1. [`import_ipynb`] returns an [`Import`] — a report — not a [`Notebook`].
//!    Getting the notebook out is a second call the caller has to write.
//! 2. [`RunRequest`] is the only description of work this crate produces, its
//!    fields are private, and the only way to obtain one is a `request_run*`
//!    method on a notebook.
//! 3. Every one of those methods demands a [`UserGesture`] by value, and
//!    `UserGesture` is not `Deserialize`, not `Default` and not `Clone`. No
//!    parsed file can produce one at any depth, and one gesture cannot be
//!    duplicated into a second run. **An import path that tried to start a run
//!    would not compile** — which is the difference between a rule and a note
//!    about a rule.
//!
//! Alongside that, [`Cell`] has no `auto_run`, no `run_on_open` and no
//! execution state at all; a Jupyter `execution_count` is discarded on import
//! with a warning saying so. The absence of those fields is the design. See
//! the [`run`] module for what this does and does not claim.
//!
//! ## What is here
//!
//! * [`Cell`] and [`CellKind`] — the eight kinds specs.md names, and the one
//!   authority on which of them can run.
//! * [`Notebook`] — an ordered list of cells and the five operations specs.md
//!   asks for: split, merge, move, duplicate, collapse. Split and merge are
//!   exact inverses, and that promise is what shapes both of them.
//! * [`Output`] — the seven result types, modelled and never rendered.
//! * [`ipynb`] — `.ipynb` import and export, the mapping from eight kinds onto
//!   Jupyter's three, and an explicit list of what the format cannot carry.
//! * [`raw_json_view`] — the raw notebook JSON view, which shows the `.ipynb`
//!   form because that is the thing the user might want to hand-edit.
//!
//! ## Pure
//!
//! No clock, no filesystem, no configuration. A notebook's bytes come in as a
//! `&str` or a `serde_json::Value` and go out the same way, so every behaviour
//! here is a function of its arguments and can be tested as one.

#![forbid(unsafe_code)]

pub mod cell;
pub mod ipynb;
pub mod notebook;
pub mod output;
pub mod run;

#[cfg(test)]
mod testing;

pub use cell::{Cell, CellId, CellKind};
pub use ipynb::{
    BACHELORPAD_KEY, FILE_MIME, Import, ImportWarning, IpynbError, NBFORMAT_MAJOR, NBFORMAT_MINOR,
    OutputDropped, TABLE_MIME, export_ipynb, import_ipynb, parse_raw_json_view, raw_json_view,
};
pub use notebook::{Notebook, NotebookError};
pub use output::{Output, Stream};
pub use run::{RunRequest, UserGesture};

/// Crate identity used by workspace smoke tests and diagnostics.
pub const CRATE_NAME: &str = "bp-notebook";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_notebook_can_be_built_edited_saved_and_reopened_without_running_anything() {
        // The whole crate, end to end, at the size a user actually works at.
        let mut notebook = Notebook::new();
        notebook.push(CellKind::Markdown, "# Sales\n");
        let query = notebook.push(CellKind::Sql, "select region, total\nfrom sales;");
        notebook.push(CellKind::PlainText, "check with finance");

        // A run happened somewhere else and the results came back here.
        notebook
            .cell_mut(query)
            .unwrap()
            .set_outputs(vec![Output::Table {
                columns: vec!["region".to_owned(), "total".to_owned()],
                rows: vec![vec!["north".to_owned(), "12".to_owned()]],
            }])
            .unwrap();

        // Edit: split the query in two, fold the second half, move it up.
        let second = notebook.split(query, 28).unwrap();
        notebook.set_collapsed(second, true).unwrap();
        assert_eq!(notebook.move_up(second), Ok(true));
        assert_eq!(notebook.to_string(), "4 cells, 2 runnable");

        // Save and reopen.
        let text = raw_json_view(&notebook);
        let reopened = parse_raw_json_view(&text).unwrap();
        assert!(reopened.is_lossless(), "{:?}", reopened.warnings());
        assert_eq!(reopened.notebook(), &notebook);

        // Reopening produced no work. Work needs a gesture, and the gesture
        // has to be written down at a call site someone chose.
        let reopened = reopened.into_notebook();
        let requests = reopened.request_run_all(UserGesture::from_user_command());
        assert_eq!(requests.len(), 2, "both halves of the query, nothing else");
        assert!(requests.iter().all(|r| r.language() == CellKind::Sql));
    }

    #[test]
    fn the_crate_names_itself() {
        assert_eq!(CRATE_NAME, "bp-notebook");
    }
}
