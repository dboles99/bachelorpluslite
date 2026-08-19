//! The notebook and the five operations specs.md names on it: split, merge,
//! move, duplicate, collapse.
//!
//! Two decisions here shape everything else.
//!
//! **The identity allocator is derived, not stored.** The next `CellId` is
//! `max(existing) + 1`, computed when a cell is inserted. A stored counter
//! would be a second piece of state that a saved file has to carry and a round
//! trip has to preserve, and it would make two notebooks with identical
//! contents unequal. Deriving it means a notebook is completely described by
//! its cells, which is what lets export-then-import be an equality property
//! rather than a vaguer "equivalent" one. The price, stated plainly: after a
//! merge removes the highest-numbered cell, that number is handed out again, so
//! a `CellId` held across an edit is stale rather than merely dead. Every
//! operation takes the id by value and re-resolves it, so a stale id names a
//! different cell rather than a freed one -- which is why nothing here caches
//! an index.
//!
//! **Split and merge are exact inverses.** That is a stronger promise than
//! either operation needs on its own, and it is deliberate: it is the property
//! that catches the errors a pair of hand-written examples never would. It
//! forces merge to join text with no separator (Jupyter inserts a newline;
//! doing so would make the pair lossy), forces split to keep the outputs on the
//! upper half rather than discarding both, and forces merge to refuse two cells
//! of different kinds instead of silently adopting the first one's language.

use std::fmt;

use serde_json::{Map, Value};

use crate::cell::{Cell, CellId, CellKind};

/// What went wrong with an operation on a notebook.
///
/// Every message is written to be shown to the user as it stands. A caller
/// that has to rewrite our wording is a caller that will get it wrong
/// somewhere.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotebookError {
    /// The id names nothing. Usually a handle held across an edit that removed
    /// the cell.
    #[error("that cell is no longer in the notebook")]
    NoSuchCell(CellId),

    /// A position outside the notebook. `len` is the number of cells, so
    /// `len` itself is a legal destination for a move (the end).
    #[error("position {index} is outside a notebook of {len} cells")]
    IndexOutOfRange {
        /// The position asked for.
        index: usize,
        /// How many cells there are.
        len: usize,
    },

    /// A split point past the end of the cell's text.
    #[error("cannot split at byte {offset}: the cell is only {len} bytes long")]
    OffsetPastEnd {
        /// The offset asked for.
        offset: usize,
        /// The length of the cell's text in bytes.
        len: usize,
    },

    /// A split point in the middle of a character. Reachable whenever the cell
    /// contains anything outside ASCII and the caller measured in bytes.
    #[error("cannot split at byte {offset}: that is inside a character, not between two")]
    OffsetNotACharBoundary {
        /// The offset asked for.
        offset: usize,
    },

    /// Two cells that are not next to each other, or the same cell twice.
    #[error("only two cells next to each other can be merged")]
    NotAdjacent,

    /// Two cells in different languages. Refused rather than resolved: see
    /// [`Notebook::merge`].
    #[error(
        "a {first} cell and a {second} cell cannot be merged: the result would claim one language and contain the other's text"
    )]
    KindMismatch {
        /// The kind of the upper cell.
        first: CellKind,
        /// The kind of the lower cell.
        second: CellKind,
    },

    /// Asked to run, or to hold the results of running, a cell that is prose.
    #[error("a {0} cell has nothing to run")]
    NotExecutable(CellKind),

    /// A run-selection range that is backwards, past the end, or inside a
    /// character.
    #[error("that selection is not a range of this cell: {start}..{end} of {len} bytes")]
    InvalidSelection {
        /// Start of the range.
        start: usize,
        /// End of the range.
        end: usize,
        /// The length of the cell's text in bytes.
        len: usize,
    },
}

/// An ordered list of cells, plus whatever the file said about itself.
///
/// Deliberately not `Deserialize`. The way into a `Notebook` from bytes is
/// [`import_ipynb`](crate::import_ipynb), which returns a report rather than a
/// notebook and cannot construct anything runnable on the way -- see the crate
/// docs. A derived `Deserialize` would be a second door into the same room with
/// none of that on it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Notebook {
    cells: Vec<Cell>,
    metadata: Map<String, Value>,
}

impl Notebook {
    /// An empty notebook. The empty case is a real state, not an error: a
    /// notebook the user has just created has no cells and must still save,
    /// export and open.
    pub fn new() -> Self {
        Self::default()
    }

    /// The cells, in document order.
    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    /// How many cells there are.
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    /// Whether there are no cells at all.
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// Whatever the notebook file said about itself -- kernelspec, language
    /// info, extension keys -- kept verbatim so a notebook edited here and
    /// reopened elsewhere is not stripped of its settings. We interpret none of
    /// it; `bachelorpad` is the one reserved key.
    pub fn metadata(&self) -> &Map<String, Value> {
        &self.metadata
    }

    /// Mutable access to that passenger metadata.
    pub fn metadata_mut(&mut self) -> &mut Map<String, Value> {
        &mut self.metadata
    }

    /// Where a cell currently sits, or `None` if the id names nothing.
    ///
    /// Exposed because a UI needs it for scrolling and selection, and because
    /// re-resolving on every operation -- rather than caching -- is what keeps
    /// a stale id from reaching into a moved cell.
    pub fn index_of(&self, id: CellId) -> Option<usize> {
        self.cells.iter().position(|c| c.id() == id)
    }

    /// One cell by id.
    pub fn cell(&self, id: CellId) -> Option<&Cell> {
        self.cells.iter().find(|c| c.id() == id)
    }

    /// One cell by id, for editing its text or its fold state.
    pub fn cell_mut(&mut self, id: CellId) -> Option<&mut Cell> {
        self.cells.iter_mut().find(|c| c.id() == id)
    }

    /// The identity the next inserted cell will get.
    ///
    /// `max + 1`, derived rather than stored -- see the module docs for why,
    /// and for what that costs.
    fn next_id(&self) -> CellId {
        let highest = self.cells.iter().map(|c| c.id().value()).max().unwrap_or(0);
        CellId::new(highest + 1)
    }

    /// Add a cell at the end.
    pub fn push(&mut self, kind: CellKind, source: impl Into<String>) -> CellId {
        let id = self.next_id();
        self.cells.push(Cell::new(id, kind, source.into()));
        id
    }

    /// Add a cell at a position. `index == len` appends, which is why the
    /// bound is inclusive; anything beyond is
    /// [`NotebookError::IndexOutOfRange`].
    pub fn insert(
        &mut self,
        index: usize,
        kind: CellKind,
        source: impl Into<String>,
    ) -> Result<CellId, NotebookError> {
        if index > self.cells.len() {
            return Err(NotebookError::IndexOutOfRange {
                index,
                len: self.cells.len(),
            });
        }
        let id = self.next_id();
        self.cells.insert(index, Cell::new(id, kind, source.into()));
        Ok(id)
    }

    /// Take a cell out, returning it so the caller can put it in an undo
    /// record. Returning it rather than dropping it is the difference between
    /// a delete the user can take back and one they cannot.
    pub fn remove(&mut self, id: CellId) -> Result<Cell, NotebookError> {
        let index = self.index_of(id).ok_or(NotebookError::NoSuchCell(id))?;
        Ok(self.cells.remove(index))
    }

    /// Cut a cell in two at a byte offset in its text.
    ///
    /// The upper half keeps the id, the kind, the fold state, the metadata
    /// **and the outputs**; the lower half is a new cell with the same kind and
    /// fold state and nothing else. Putting the outputs on the upper half
    /// rather than discarding them is what makes [`Notebook::merge`] able to
    /// undo this exactly; it also means a user who splits a cell after a long
    /// run does not lose the result of it.
    ///
    /// `at == 0` and `at == source.len()` are allowed and produce an empty
    /// half. That is a strange cell to have, but it is what the user asked for,
    /// and refusing would mean a caret at the top of a cell has no split.
    pub fn split(&mut self, id: CellId, at: usize) -> Result<CellId, NotebookError> {
        let index = self.index_of(id).ok_or(NotebookError::NoSuchCell(id))?;
        let len = self.cells[index].source().len();
        if at > len {
            return Err(NotebookError::OffsetPastEnd { offset: at, len });
        }
        if !self.cells[index].source().is_char_boundary(at) {
            return Err(NotebookError::OffsetNotACharBoundary { offset: at });
        }

        let new_id = self.next_id();
        let (kind, collapsed, tail) = {
            let cell = &mut self.cells[index];
            let tail = cell.source()[at..].to_owned();
            cell.source_mut().truncate(at);
            (cell.kind(), cell.collapsed(), tail)
        };

        let mut lower = Cell::new(new_id, kind, tail);
        lower.set_collapsed(collapsed);
        self.cells.insert(index + 1, lower);
        Ok(new_id)
    }

    /// Join two adjacent cells into the first.
    ///
    /// The text is concatenated with **no separator**. Jupyter inserts a
    /// newline here; we do not, because then split and merge would not be
    /// inverses and every split-merge pair would grow the document by a byte.
    /// A caller that wants a blank line between two merged cells adds it before
    /// merging, where the user can see it.
    ///
    /// Refuses cells of different kinds. Adopting the first cell's language
    /// would take a Shell cell's text and file it as Python source -- and the
    /// next "run cell" would hand exactly that to a Python interpreter. There
    /// is no safe default, so there is no default.
    ///
    /// Outputs are concatenated in order, and the second cell's metadata keys
    /// are added where the first has none, so the first's settings win a
    /// conflict.
    pub fn merge(&mut self, first: CellId, second: CellId) -> Result<(), NotebookError> {
        let upper = self
            .index_of(first)
            .ok_or(NotebookError::NoSuchCell(first))?;
        let lower = self
            .index_of(second)
            .ok_or(NotebookError::NoSuchCell(second))?;
        if lower != upper + 1 {
            return Err(NotebookError::NotAdjacent);
        }
        let (first_kind, second_kind) = (self.cells[upper].kind(), self.cells[lower].kind());
        if first_kind != second_kind {
            return Err(NotebookError::KindMismatch {
                first: first_kind,
                second: second_kind,
            });
        }

        let mut removed = self.cells.remove(lower);
        let tail = removed.source().to_owned();
        let outputs = removed.take_outputs();
        let metadata = removed.metadata().clone();

        let cell = &mut self.cells[upper];
        cell.source_mut().push_str(&tail);
        cell.extend_outputs(outputs);
        for (key, value) in metadata {
            cell.metadata_mut().entry(key).or_insert(value);
        }
        Ok(())
    }

    /// Move a cell to an exact position, the other cells closing up behind it.
    ///
    /// `to` is the index the cell will occupy afterwards, so the legal range is
    /// `0..len` -- unlike [`Notebook::insert`], there is no "one past the end"
    /// here, because the cell is already in the list and cannot end up outside
    /// it.
    pub fn move_cell(&mut self, id: CellId, to: usize) -> Result<(), NotebookError> {
        let from = self.index_of(id).ok_or(NotebookError::NoSuchCell(id))?;
        if to >= self.cells.len() {
            return Err(NotebookError::IndexOutOfRange {
                index: to,
                len: self.cells.len(),
            });
        }
        let cell = self.cells.remove(from);
        self.cells.insert(to, cell);
        Ok(())
    }

    /// Move a cell one place towards the top.
    ///
    /// Returns whether it moved. A cell already at the top is not an error to
    /// move up -- it is already where "up" would put it, and a user holding a
    /// keyboard shortcut down should not be shown a failure for reaching the
    /// end of the list.
    pub fn move_up(&mut self, id: CellId) -> Result<bool, NotebookError> {
        let index = self.index_of(id).ok_or(NotebookError::NoSuchCell(id))?;
        if index == 0 {
            return Ok(false);
        }
        self.cells.swap(index - 1, index);
        Ok(true)
    }

    /// Move a cell one place towards the bottom. Same reasoning as
    /// [`Notebook::move_up`] at the far end.
    pub fn move_down(&mut self, id: CellId) -> Result<bool, NotebookError> {
        let index = self.index_of(id).ok_or(NotebookError::NoSuchCell(id))?;
        if index + 1 >= self.cells.len() {
            return Ok(false);
        }
        self.cells.swap(index, index + 1);
        Ok(true)
    }

    /// Copy a cell, placing the copy directly below the original.
    ///
    /// The outputs are copied too. They were produced by exactly this text, so
    /// they describe the copy as truthfully as they describe the original;
    /// clearing them would make the duplicate look unrun when it is not.
    pub fn duplicate(&mut self, id: CellId) -> Result<CellId, NotebookError> {
        let index = self.index_of(id).ok_or(NotebookError::NoSuchCell(id))?;
        let new_id = self.next_id();
        let copy = self.cells[index].with_id(new_id);
        self.cells.insert(index + 1, copy);
        Ok(new_id)
    }

    /// Fold or unfold a cell. Returns whether it changed, so a caller need not
    /// read the state first to decide whether the document is now dirty.
    pub fn set_collapsed(&mut self, id: CellId, collapsed: bool) -> Result<bool, NotebookError> {
        let cell = self.cell_mut(id).ok_or(NotebookError::NoSuchCell(id))?;
        Ok(cell.set_collapsed(collapsed))
    }

    /// Flip a cell's fold state, returning the new one. Two toggles are the
    /// identity, which is the property a keyboard shortcut needs to hold.
    pub fn toggle_collapsed(&mut self, id: CellId) -> Result<bool, NotebookError> {
        let cell = self.cell_mut(id).ok_or(NotebookError::NoSuchCell(id))?;
        let next = !cell.collapsed();
        cell.set_collapsed(next);
        Ok(next)
    }

    pub(crate) fn from_parts(cells: Vec<Cell>, metadata: Map<String, Value>) -> Self {
        Self { cells, metadata }
    }
}

impl fmt::Display for Notebook {
    /// A one-line description for a status bar: how many cells, and how many
    /// of them could be run.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let runnable = self
            .cells
            .iter()
            .filter(|c| c.kind().is_executable())
            .count();
        let cells = if self.cells.len() == 1 {
            "cell"
        } else {
            "cells"
        };
        write!(f, "{} {cells}, {runnable} runnable", self.cells.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::{Output, Stream};
    use crate::testing::{arb_kind, arb_notebook, arb_source};
    use proptest::prelude::*;

    fn sample() -> Notebook {
        let mut notebook = Notebook::new();
        notebook.push(CellKind::Markdown, "# Title\n");
        notebook.push(CellKind::Python, "print(1)\n");
        notebook.push(CellKind::PlainText, "a note");
        notebook
    }

    #[test]
    fn a_new_notebook_is_empty_and_every_operation_on_it_says_so() {
        let mut notebook = Notebook::new();
        assert!(notebook.is_empty());
        assert_eq!(notebook.len(), 0);
        assert_eq!(notebook.to_string(), "0 cells, 0 runnable");

        // An id cannot be fabricated from outside, but one from another
        // notebook resolves to nothing here -- the same failure a stale
        // handle produces.
        let stranger = sample().cells()[0].id();
        assert_eq!(
            notebook.move_up(stranger),
            Err(NotebookError::NoSuchCell(stranger))
        );
        assert_eq!(
            notebook.insert(1, CellKind::Raw, ""),
            Err(NotebookError::IndexOutOfRange { index: 1, len: 0 })
        );
        // Inserting at the end of nothing is the one legal position.
        assert!(notebook.insert(0, CellKind::Raw, "").is_ok());
    }

    #[test]
    fn ids_are_handed_out_above_every_id_in_use() {
        let mut notebook = sample();
        let ids: Vec<_> = notebook.cells().iter().map(|c| c.id().value()).collect();
        assert_eq!(ids, vec![1, 2, 3]);

        notebook.remove(CellId::new(2)).unwrap();
        // The gap is not reused while a higher id is still in the notebook,
        // so a handle to cell 3 keeps meaning cell 3.
        assert_eq!(notebook.push(CellKind::Raw, "x").value(), 4);
    }

    #[test]
    fn splitting_at_the_two_boundaries_gives_an_empty_half_rather_than_an_error() {
        let mut notebook = sample();
        let id = notebook.cells()[0].id();

        let lower = notebook.split(id, 0).unwrap();
        assert_eq!(notebook.cell(id).unwrap().source(), "");
        assert_eq!(notebook.cell(lower).unwrap().source(), "# Title\n");

        let mut notebook = sample();
        let id = notebook.cells()[0].id();
        let len = notebook.cell(id).unwrap().source().len();
        let lower = notebook.split(id, len).unwrap();
        assert_eq!(notebook.cell(lower).unwrap().source(), "");
    }

    #[test]
    fn a_split_point_past_the_end_or_inside_a_character_is_refused_by_name() {
        let mut notebook = Notebook::new();
        let id = notebook.push(CellKind::PlainText, "日本語");

        assert_eq!(
            notebook.split(id, 10),
            Err(NotebookError::OffsetPastEnd { offset: 10, len: 9 })
        );
        // Byte 1 is inside the first three-byte character.
        assert_eq!(
            notebook.split(id, 1),
            Err(NotebookError::OffsetNotACharBoundary { offset: 1 })
        );
        assert!(
            notebook
                .split(id, 10)
                .unwrap_err()
                .to_string()
                .contains('9'),
            "the message has to say how long the cell actually is"
        );
        assert!(notebook.split(id, 3).is_ok(), "3 is a boundary");
    }

    #[test]
    fn splitting_keeps_the_outputs_on_the_upper_half_only() {
        let mut notebook = Notebook::new();
        let id = notebook.push(CellKind::Python, "print(1)\nprint(2)\n");
        notebook
            .cell_mut(id)
            .unwrap()
            .push_output(Output::Text {
                stream: Stream::Stdout,
                text: "1\n2\n".to_owned(),
            })
            .unwrap();

        let lower = notebook.split(id, 9).unwrap();
        assert_eq!(notebook.cell(id).unwrap().outputs().len(), 1);
        assert!(
            notebook.cell(lower).unwrap().outputs().is_empty(),
            "the lower half never produced anything"
        );
    }

    #[test]
    fn merging_refuses_cells_that_are_not_neighbours_or_not_the_same_language() {
        let mut notebook = sample();
        let (first, second, third) = (
            notebook.cells()[0].id(),
            notebook.cells()[1].id(),
            notebook.cells()[2].id(),
        );

        assert_eq!(
            notebook.merge(first, third),
            Err(NotebookError::NotAdjacent)
        );
        assert_eq!(
            notebook.merge(second, first),
            Err(NotebookError::NotAdjacent)
        );
        assert_eq!(
            notebook.merge(first, first),
            Err(NotebookError::NotAdjacent)
        );
        assert_eq!(
            notebook.merge(first, second),
            Err(NotebookError::KindMismatch {
                first: CellKind::Markdown,
                second: CellKind::Python,
            })
        );
        let message = notebook.merge(first, second).unwrap_err().to_string();
        assert!(
            message.contains("Markdown") && message.contains("Python"),
            "{message}"
        );
        assert_eq!(notebook.len(), 3, "a refused merge changes nothing");
    }

    #[test]
    fn merging_joins_the_text_with_nothing_between_it() {
        let mut notebook = Notebook::new();
        let first = notebook.push(CellKind::Sql, "select 1");
        let second = notebook.push(CellKind::Sql, " from t");
        notebook.merge(first, second).unwrap();

        assert_eq!(notebook.len(), 1);
        assert_eq!(
            notebook.cell(first).unwrap().source(),
            "select 1 from t",
            "no newline invented"
        );
    }

    #[test]
    fn a_merged_away_id_names_nothing() {
        let mut notebook = Notebook::new();
        let first = notebook.push(CellKind::Sql, "a");
        let second = notebook.push(CellKind::Sql, "b");
        notebook.merge(first, second).unwrap();
        assert_eq!(
            notebook.toggle_collapsed(second),
            Err(NotebookError::NoSuchCell(second))
        );
    }

    #[test]
    fn moving_off_either_end_is_answered_not_refused() {
        let mut notebook = sample();
        let (top, bottom) = (notebook.cells()[0].id(), notebook.cells()[2].id());
        let before = notebook.clone();

        assert_eq!(notebook.move_up(top), Ok(false));
        assert_eq!(notebook.move_down(bottom), Ok(false));
        assert_eq!(notebook, before, "a refused move must change nothing");
    }

    #[test]
    fn moving_to_an_exact_position_stops_one_short_of_insert() {
        // insert() takes 0..=len because the cell is not in the list yet;
        // move_cell takes 0..len because it already is.
        let mut notebook = sample();
        let id = notebook.cells()[0].id();
        assert_eq!(
            notebook.move_cell(id, 3),
            Err(NotebookError::IndexOutOfRange { index: 3, len: 3 })
        );
        assert!(notebook.move_cell(id, 2).is_ok());
        assert_eq!(notebook.index_of(id), Some(2));
    }

    #[test]
    fn duplicating_puts_the_copy_underneath_with_the_results_it_earned() {
        let mut notebook = Notebook::new();
        let id = notebook.push(CellKind::Python, "print(1)");
        notebook
            .cell_mut(id)
            .unwrap()
            .push_output(Output::Text {
                stream: Stream::Stdout,
                text: "1".to_owned(),
            })
            .unwrap();

        let copy = notebook.duplicate(id).unwrap();
        assert_eq!(notebook.index_of(copy), Some(1));
        assert_ne!(copy, id, "a duplicate is a different cell");
        assert_eq!(
            notebook.cell(copy).unwrap().outputs(),
            notebook.cell(id).unwrap().outputs(),
            "same text, same result"
        );
    }

    #[test]
    fn the_status_line_counts_only_what_could_run() {
        assert_eq!(sample().to_string(), "3 cells, 1 runnable");
        let mut one = Notebook::new();
        one.push(CellKind::Raw, "x");
        assert_eq!(one.to_string(), "1 cell, 0 runnable");
    }

    proptest! {
        /// The property the whole split/merge design is built around.
        #[test]
        fn splitting_a_cell_and_merging_it_back_returns_the_exact_notebook(
            notebook in arb_notebook(1..5),
            which in 0usize..5,
            at in 0usize..40,
        ) {
            let index = which % notebook.len();
            let id = notebook.cells()[index].id();
            let source_len = notebook.cells()[index].source().len();
            // Land on a character boundary at or below the end; the refusals
            // for everything else are pinned by the examples above.
            let at = (at % (source_len + 1)..=source_len)
                .find(|a| notebook.cells()[index].source().is_char_boundary(*a))
                .unwrap_or(source_len);

            let mut edited = notebook.clone();
            let lower = edited.split(id, at).unwrap();
            prop_assert_eq!(edited.len(), notebook.len() + 1);
            edited.merge(id, lower).unwrap();

            prop_assert_eq!(edited, notebook);
        }

        /// Down then up, at every position -- including the two where "down"
        /// has nowhere to go and must leave the notebook alone.
        #[test]
        fn moving_a_cell_down_and_back_up_returns_the_exact_notebook(
            notebook in arb_notebook(0..6),
            which in 0usize..6,
        ) {
            prop_assume!(!notebook.is_empty());
            let index = which % notebook.len();
            let id = notebook.cells()[index].id();

            let mut edited = notebook.clone();
            let moved = edited.move_down(id).unwrap();
            if !moved {
                prop_assert_eq!(&edited, &notebook, "a move that did not happen changed something");
            } else {
                prop_assert_ne!(&edited, &notebook, "a move that happened changed nothing");
                prop_assert_eq!(edited.move_up(id), Ok(true));
                prop_assert_eq!(edited, notebook);
            }
        }

        /// Moving to an arbitrary legal position keeps every cell, in some
        /// order, with the moved one where it was asked to be.
        #[test]
        fn moving_to_a_position_puts_the_cell_there_and_loses_nobody(
            notebook in arb_notebook(1..6),
            which in 0usize..6,
            to in 0usize..6,
        ) {
            let index = which % notebook.len();
            let to = to % notebook.len();
            let id = notebook.cells()[index].id();

            let mut edited = notebook.clone();
            edited.move_cell(id, to).unwrap();
            prop_assert_eq!(edited.index_of(id), Some(to));

            let mut before: Vec<_> = notebook.cells().iter().map(|c| c.id()).collect();
            let mut after: Vec<_> = edited.cells().iter().map(|c| c.id()).collect();
            before.sort_unstable();
            after.sort_unstable();
            prop_assert_eq!(before, after);
        }

        /// Duplicate then remove the copy is the identity, which pins that the
        /// copy took a fresh id and left the original untouched.
        #[test]
        fn duplicating_and_removing_the_copy_returns_the_exact_notebook(
            notebook in arb_notebook(1..5),
            which in 0usize..5,
        ) {
            let index = which % notebook.len();
            let id = notebook.cells()[index].id();

            let mut edited = notebook.clone();
            let copy = edited.duplicate(id).unwrap();
            prop_assert_eq!(edited.len(), notebook.len() + 1);
            edited.remove(copy).unwrap();
            prop_assert_eq!(edited, notebook);
        }

        /// Collapse is a two-state toggle, so two of them is the identity and
        /// setting the state it already has is a no-op that says so.
        #[test]
        fn two_toggles_of_the_fold_state_return_the_exact_notebook(
            notebook in arb_notebook(1..5),
            which in 0usize..5,
        ) {
            let index = which % notebook.len();
            let id = notebook.cells()[index].id();
            let was = notebook.cells()[index].collapsed();

            let mut edited = notebook.clone();
            prop_assert_eq!(edited.toggle_collapsed(id), Ok(!was));
            prop_assert_eq!(edited.toggle_collapsed(id), Ok(was));
            prop_assert_eq!(&edited, &notebook);

            prop_assert_eq!(edited.set_collapsed(id, was), Ok(false), "no change reported");
            prop_assert_eq!(edited, notebook);
        }

        /// Every operation refuses an id the notebook does not hold, rather
        /// than reaching into whatever now sits at that position.
        #[test]
        fn a_stale_id_is_refused_by_every_operation(
            notebook in arb_notebook(0..4),
            kind in arb_kind(),
            source in arb_source(),
        ) {
            // Add a cell, then take it away: the handle is now stale in the
            // way a UI's handle goes stale after an undo.
            let mut notebook = notebook;
            let id = notebook.push(kind, source);
            notebook.remove(id).unwrap();

            prop_assert_eq!(notebook.cell(id), None);
            prop_assert_eq!(notebook.index_of(id), None);
            prop_assert_eq!(notebook.split(id, 0), Err(NotebookError::NoSuchCell(id)));
            prop_assert_eq!(notebook.move_up(id), Err(NotebookError::NoSuchCell(id)));
            prop_assert_eq!(notebook.move_down(id), Err(NotebookError::NoSuchCell(id)));
            prop_assert_eq!(notebook.move_cell(id, 0), Err(NotebookError::NoSuchCell(id)));
            prop_assert_eq!(notebook.duplicate(id), Err(NotebookError::NoSuchCell(id)));
            prop_assert_eq!(notebook.toggle_collapsed(id), Err(NotebookError::NoSuchCell(id)));
            prop_assert!(notebook.remove(id).is_err());
        }
    }
}
