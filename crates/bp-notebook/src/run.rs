//! Asking for a run, which is not the same thing as running.
//!
//! specs.md section 15 gives this crate one hard rule: **never auto-run an
//! opened or pasted notebook or cell.** This crate cannot execute anything, so
//! it would be easy to declare the rule satisfied by construction and write a
//! paragraph about it. That is not enough, because the crate that *can*
//! execute will take its instructions from here, and a rule that lives only in
//! prose is a rule the next caller re-decides.
//!
//! So the rule is put where the compiler keeps it:
//!
//! * [`RunRequest`] is the only description of a run this crate produces, and
//!   its fields are private, so it cannot be assembled from parts.
//! * The only way to obtain one is a `request_run*` method, and every one of
//!   them takes a [`UserGesture`] **by value**.
//! * `UserGesture` implements neither `Deserialize` nor `Default` nor `Clone`.
//!   It has exactly one constructor,
//!   [`UserGesture::from_user_command`]. Deserialising a file therefore cannot
//!   produce one at any depth, no `..Default::default()` can conjure one, and
//!   one gesture cannot be cloned into a second run. An import path that tried
//!   to start a run would not compile.
//!
//! What that does *not* claim: it cannot stop a caller from calling
//! `from_user_command` in the wrong place. Nothing in a type system can. What
//! it does is make that call the single, greppable, obviously-named line where
//! consent is asserted, instead of a `run: bool` somewhere in a struct that a
//! `serde` derive will happily fill in from a downloaded file.

use std::ops::Range;

use crate::cell::{CellId, CellKind};
use crate::notebook::{Notebook, NotebookError};

/// Evidence that a person asked for this, right now.
///
/// A token with no data in it: its whole value is that it can only come into
/// existence at a call site someone wrote deliberately. See the module docs for
/// what it does and does not guarantee.
///
/// It is not `Clone`, so one gesture buys one call:
///
/// ```compile_fail
/// fn only_clonable<T: Clone>(_: T) {}
/// only_clonable(bp_notebook::UserGesture::from_user_command());
/// ```
///
/// And it is not deserialisable, so no file can produce one. The first of
/// these two compiles, which is what proves the second is failing for the
/// reason claimed rather than because `serde` is out of scope:
///
/// ```
/// fn needs_deserialize<T: serde::de::DeserializeOwned>() {}
/// needs_deserialize::<bp_notebook::CellKind>();
/// ```
///
/// ```compile_fail
/// fn needs_deserialize<T: serde::de::DeserializeOwned>() {}
/// needs_deserialize::<bp_notebook::UserGesture>();
/// ```
#[derive(Debug)]
pub struct UserGesture(());

impl UserGesture {
    /// Call this from the handler for a menu item, a key press or a toolbar
    /// button -- somewhere a human's action is on the stack. Calling it from a
    /// file-open path, a paste handler or a deserialiser is the one mistake
    /// this design is built to make visible in review.
    pub fn from_user_command() -> Self {
        Self(())
    }
}

/// One cell's worth of work, described but not started.
///
/// Everything a runner needs and nothing more: which cell the results belong
/// to, which language to use, and the exact text. No handle, no callback, no
/// environment -- a `RunRequest` is inert data that a caller may also discard,
/// which is what makes "prepare a run" a separable step from "run".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunRequest {
    cell: CellId,
    language: CellKind,
    source: String,
}

impl RunRequest {
    /// Which cell this belongs to, so the runner knows where to put the
    /// outputs. Re-resolved by the notebook when they arrive: the cell may have
    /// been moved, or removed, while the run was in flight.
    pub fn cell(&self) -> CellId {
        self.cell
    }

    /// The language to run it as. Always an executable kind -- the
    /// `request_run*` methods refuse to build one otherwise.
    pub fn language(&self) -> CellKind {
        self.language
    }

    /// The exact text to run. For a whole-cell request this is the cell's
    /// source; for a selection it is only the selected part.
    pub fn source(&self) -> &str {
        &self.source
    }
}

impl Notebook {
    /// Prepare to run one cell.
    ///
    /// Refuses a cell that is prose rather than silently doing nothing: a "run"
    /// that quietly succeeds without running is worse than one that says the
    /// cell has nothing to run.
    pub fn request_run(
        &self,
        id: CellId,
        gesture: UserGesture,
    ) -> Result<RunRequest, NotebookError> {
        let _ = gesture;
        let cell = self.cell(id).ok_or(NotebookError::NoSuchCell(id))?;
        if !cell.kind().is_executable() {
            return Err(NotebookError::NotExecutable(cell.kind()));
        }
        Ok(RunRequest {
            cell: id,
            language: cell.kind(),
            source: cell.source().to_owned(),
        })
    }

    /// Prepare to run part of one cell -- specs.md's "run selection".
    ///
    /// The range is in bytes, because that is what an editor's selection
    /// already is. A backwards range, one past the end, or one that cuts a
    /// character in half is [`NotebookError::InvalidSelection`]: all three are
    /// the same mistake from the user's point of view (that is not a piece of
    /// this cell), and splitting them into three messages would explain our
    /// data structures rather than their problem.
    ///
    /// An empty selection is allowed and yields an empty request. The runner,
    /// not us, decides what running nothing means in its language.
    pub fn request_run_selection(
        &self,
        id: CellId,
        selection: Range<usize>,
        gesture: UserGesture,
    ) -> Result<RunRequest, NotebookError> {
        let _ = gesture;
        let cell = self.cell(id).ok_or(NotebookError::NoSuchCell(id))?;
        if !cell.kind().is_executable() {
            return Err(NotebookError::NotExecutable(cell.kind()));
        }
        let source = cell.source();
        let valid = selection.start <= selection.end
            && selection.end <= source.len()
            && source.is_char_boundary(selection.start)
            && source.is_char_boundary(selection.end);
        if !valid {
            return Err(NotebookError::InvalidSelection {
                start: selection.start,
                end: selection.end,
                len: source.len(),
            });
        }
        Ok(RunRequest {
            cell: id,
            language: cell.kind(),
            source: source[selection].to_owned(),
        })
    }

    /// Prepare to run every runnable cell, top to bottom.
    ///
    /// Prose cells are skipped rather than refused. Unlike
    /// [`Notebook::request_run`], the user did not point at a Markdown cell and
    /// ask for it; they asked for the notebook, and a notebook with a heading in
    /// it is the normal case, not an error.
    pub fn request_run_all(&self, gesture: UserGesture) -> Vec<RunRequest> {
        let _ = gesture;
        self.cells()
            .iter()
            .filter(|c| c.kind().is_executable())
            .map(|c| RunRequest {
                cell: c.id(),
                language: c.kind(),
                source: c.source().to_owned(),
            })
            .collect()
    }

    /// Prepare to run everything above a cell, not including it.
    ///
    /// Exclusive because the reason to run "above" is to rebuild the state a
    /// cell depends on before editing it; including the cell would run the
    /// thing the user was about to change.
    pub fn request_run_above(
        &self,
        id: CellId,
        gesture: UserGesture,
    ) -> Result<Vec<RunRequest>, NotebookError> {
        let index = self.index_of(id).ok_or(NotebookError::NoSuchCell(id))?;
        Ok(self.requests_in(0..index, gesture))
    }

    /// Prepare to run a cell and everything below it.
    ///
    /// Inclusive, for the mirrored reason: "run below" is what you ask for
    /// after fixing this cell, and leaving it out would run the rest against
    /// the old version of it.
    pub fn request_run_below(
        &self,
        id: CellId,
        gesture: UserGesture,
    ) -> Result<Vec<RunRequest>, NotebookError> {
        let index = self.index_of(id).ok_or(NotebookError::NoSuchCell(id))?;
        Ok(self.requests_in(index..self.len(), gesture))
    }

    fn requests_in(&self, range: Range<usize>, gesture: UserGesture) -> Vec<RunRequest> {
        let _ = gesture;
        self.cells()[range]
            .iter()
            .filter(|c| c.kind().is_executable())
            .map(|c| RunRequest {
                cell: c.id(),
                language: c.kind(),
                source: c.source().to_owned(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::arb_notebook;
    use proptest::prelude::*;

    fn gesture() -> UserGesture {
        UserGesture::from_user_command()
    }

    fn mixed() -> Notebook {
        let mut notebook = Notebook::new();
        notebook.push(CellKind::Markdown, "# Report\n");
        notebook.push(CellKind::Python, "x = 1");
        notebook.push(CellKind::PlainText, "a note");
        notebook.push(CellKind::Sql, "select 1");
        notebook.push(CellKind::Raw, "verbatim");
        notebook
    }

    #[test]
    fn pointing_at_a_prose_cell_and_asking_to_run_it_is_refused_by_name() {
        let notebook = mixed();
        let markdown = notebook.cells()[0].id();
        assert_eq!(
            notebook.request_run(markdown, gesture()),
            Err(NotebookError::NotExecutable(CellKind::Markdown))
        );
        assert_eq!(
            notebook
                .request_run(markdown, gesture())
                .unwrap_err()
                .to_string(),
            "a Markdown cell has nothing to run"
        );
    }

    #[test]
    fn running_the_whole_notebook_skips_prose_instead_of_refusing() {
        let notebook = mixed();
        let requests = notebook.request_run_all(gesture());
        assert_eq!(
            requests.iter().map(|r| r.language()).collect::<Vec<_>>(),
            vec![CellKind::Python, CellKind::Sql],
            "in document order, prose passed over"
        );
        assert_eq!(requests[0].source(), "x = 1");
    }

    #[test]
    fn above_excludes_the_cell_and_below_includes_it() {
        let notebook = mixed();
        let sql = notebook.cells()[3].id();
        let python = notebook.cells()[1].id();

        let above = notebook.request_run_above(sql, gesture()).unwrap();
        assert_eq!(
            above.iter().map(|r| r.cell()).collect::<Vec<_>>(),
            vec![python]
        );

        let below = notebook.request_run_below(sql, gesture()).unwrap();
        assert_eq!(
            below.iter().map(|r| r.cell()).collect::<Vec<_>>(),
            vec![sql]
        );
    }

    #[test]
    fn the_ends_of_the_notebook_give_an_empty_list_not_an_error() {
        let notebook = mixed();
        let first = notebook.cells()[0].id();
        let last = notebook.cells()[4].id();

        assert!(
            notebook
                .request_run_above(first, gesture())
                .unwrap()
                .is_empty()
        );
        assert!(
            notebook
                .request_run_below(last, gesture())
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn every_way_of_asking_refuses_an_id_the_notebook_does_not_hold() {
        let notebook = mixed();
        let mut other = Notebook::new();
        for _ in 0..9 {
            other.push(CellKind::Python, "x");
        }
        let stranger = other.cells()[8].id();

        assert_eq!(
            notebook.request_run(stranger, gesture()),
            Err(NotebookError::NoSuchCell(stranger))
        );
        assert_eq!(
            notebook.request_run_above(stranger, gesture()),
            Err(NotebookError::NoSuchCell(stranger))
        );
        assert_eq!(
            notebook.request_run_below(stranger, gesture()),
            Err(NotebookError::NoSuchCell(stranger))
        );
        assert_eq!(
            notebook.request_run_selection(stranger, 0..0, gesture()),
            Err(NotebookError::NoSuchCell(stranger))
        );
    }

    #[test]
    fn a_selection_that_is_not_a_piece_of_the_cell_is_refused() {
        let mut notebook = Notebook::new();
        let id = notebook.push(CellKind::Python, "日本語");

        // Backwards.
        assert_eq!(
            // Constructed field-by-field: `3..0` is a literal clippy refuses,
            // and a backwards selection is exactly what a caller can hand us.
            notebook.request_run_selection(id, Range { start: 3, end: 0 }, gesture()),
            Err(NotebookError::InvalidSelection {
                start: 3,
                end: 0,
                len: 9
            })
        );
        // Past the end.
        assert!(
            notebook
                .request_run_selection(id, 0..10, gesture())
                .is_err()
        );
        // Inside a character.
        assert!(notebook.request_run_selection(id, 1..3, gesture()).is_err());
        // The exact boundaries are fine, including the empty one at the end.
        assert_eq!(
            notebook
                .request_run_selection(id, 3..9, gesture())
                .unwrap()
                .source(),
            "本語"
        );
        assert_eq!(
            notebook
                .request_run_selection(id, 9..9, gesture())
                .unwrap()
                .source(),
            ""
        );
    }

    proptest! {
        /// Whatever a notebook contains and however it got there, the only
        /// cells that can produce work are the executable ones, and each
        /// request carries that cell's own text.
        #[test]
        fn a_request_is_only_ever_produced_for_a_cell_that_can_run(
            notebook in arb_notebook(0..6),
        ) {
            let requests = notebook.request_run_all(UserGesture::from_user_command());
            prop_assert_eq!(
                requests.len(),
                notebook.cells().iter().filter(|c| c.kind().is_executable()).count()
            );
            for request in &requests {
                let cell = notebook.cell(request.cell()).unwrap();
                prop_assert!(request.language().is_executable());
                prop_assert_eq!(request.language(), cell.kind());
                prop_assert_eq!(request.source(), cell.source());
            }

            // Above plus the cell's own run plus below-minus-the-cell is the
            // whole notebook, once, in order: the four selections partition
            // the same work rather than overlapping or dropping any of it.
            if let Some(pivot) = notebook.cells().first().map(|c| c.id()) {
                let mut split = notebook.request_run_above(pivot, UserGesture::from_user_command()).unwrap();
                split.extend(notebook.request_run_below(pivot, UserGesture::from_user_command()).unwrap());
                prop_assert_eq!(split, requests);
            }
        }
    }
}
