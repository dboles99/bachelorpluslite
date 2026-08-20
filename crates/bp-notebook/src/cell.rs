//! What a notebook is made of: an identity, a language, some text, and the
//! record of what the text last produced.
//!
//! The eight kinds in [`CellKind`] are specs.md section 12 verbatim, and the
//! split that matters is not "code versus prose" but
//! [`CellKind::is_executable`]: three of the eight can never be handed to a
//! runner, and the type is the only place that fact is recorded once instead
//! of being re-decided at every call site.

use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::NotebookError;
use crate::output::Output;

/// A cell's handle inside one notebook.
///
/// Opaque, and deliberately not constructible from outside: the notebook is
/// the only thing that may hand out an identity, because it is the only thing
/// that can tell whether an identity is still unique. A caller holding a
/// `CellId` across an edit that removed that cell gets
/// [`NotebookError::NoSuchCell`] rather than someone else's text -- with the
/// one caveat spelled out on [`Notebook`](crate::Notebook), whose identity
/// allocator is derived from the cells and so can reuse a number after a
/// merge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CellId(u64);

impl CellId {
    /// The number a `CellId` carries, for a UI that needs a stable key or a
    /// log line that needs to name a cell. Not an index: it says nothing
    /// about where the cell sits.
    pub fn value(self) -> u64 {
        self.0
    }

    pub(crate) fn new(value: u64) -> Self {
        Self(value)
    }
}

impl fmt::Display for CellId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cell {}", self.0)
    }
}

/// What a cell contains, and therefore what may be done with it.
///
/// specs.md section 12 names all eight. Jupyter has only three cell types, so
/// this enum is the thing `.ipynb` cannot carry natively -- see the
/// [`ipynb`](crate::ipynb) module for how it is smuggled through and what
/// happens when the smuggling fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum CellKind {
    /// Prose with no markup rules. The default for a note that is only a note.
    #[serde(rename = "plain_text")]
    PlainText,
    /// Prose a renderer will read as Markdown.
    #[serde(rename = "markdown")]
    Markdown,
    /// Python source.
    #[serde(rename = "python")]
    Python,
    /// Rust source.
    #[serde(rename = "rust")]
    Rust,
    /// PowerShell source.
    #[serde(rename = "powershell")]
    PowerShell,
    /// SQL source.
    #[serde(rename = "sql")]
    Sql,
    /// POSIX shell source.
    #[serde(rename = "shell")]
    Shell,
    /// Text that must survive untouched: no rendering, no execution, no
    /// reformatting. Also where anything we could not identify lands, because
    /// "keep the bytes and do nothing" is the only safe answer to text whose
    /// language we would otherwise be guessing at.
    #[serde(rename = "raw")]
    Raw,
}

impl CellKind {
    /// Every kind, in the order specs.md lists them, so that a menu and a test
    /// can both be exhaustive without either restating the list.
    pub const ALL: [CellKind; 8] = [
        CellKind::PlainText,
        CellKind::Markdown,
        CellKind::Python,
        CellKind::Rust,
        CellKind::PowerShell,
        CellKind::Sql,
        CellKind::Shell,
        CellKind::Raw,
    ];

    /// Whether a runner could ever be given this cell.
    ///
    /// The crate's one authority on the question. Everything that refuses to
    /// produce a [`RunRequest`](crate::RunRequest), refuses to hold outputs, or
    /// exports as a Jupyter `code` cell asks here, so the answer cannot drift
    /// apart between them.
    pub fn is_executable(self) -> bool {
        matches!(
            self,
            CellKind::Python
                | CellKind::Rust
                | CellKind::PowerShell
                | CellKind::Sql
                | CellKind::Shell
        )
    }

    /// The stable machine name, used in `.ipynb` metadata and anywhere else a
    /// kind is written down. Fixed forever once written to a file; the
    /// user-facing spelling is [`CellKind::label`] and may change freely.
    pub fn as_str(self) -> &'static str {
        match self {
            CellKind::PlainText => "plain_text",
            CellKind::Markdown => "markdown",
            CellKind::Python => "python",
            CellKind::Rust => "rust",
            CellKind::PowerShell => "powershell",
            CellKind::Sql => "sql",
            CellKind::Shell => "shell",
            CellKind::Raw => "raw",
        }
    }

    /// How the kind is spelled to a person -- menu entries, error messages,
    /// the status bar. Separate from [`CellKind::as_str`] so that renaming
    /// "plain text" in the UI cannot silently change what is written to disk.
    pub fn label(self) -> &'static str {
        match self {
            CellKind::PlainText => "plain text",
            CellKind::Markdown => "Markdown",
            CellKind::Python => "Python",
            CellKind::Rust => "Rust",
            CellKind::PowerShell => "PowerShell",
            CellKind::Sql => "SQL",
            CellKind::Shell => "Shell",
            CellKind::Raw => "Raw",
        }
    }

    /// The inverse of [`CellKind::as_str`], for reading our own metadata back.
    pub fn from_machine_name(name: &str) -> Option<CellKind> {
        CellKind::ALL.into_iter().find(|k| k.as_str() == name)
    }

    /// Best guess at a kind from the language name some other tool wrote.
    ///
    /// Needed because a foreign `.ipynb` states its language in a kernelspec,
    /// an editor extension key, or nothing at all, and each spells the same
    /// language differently. Deliberately conservative: an alias is listed only
    /// when it can mean nothing else, because guessing wrong here means
    /// offering to run someone's text through the wrong interpreter.
    pub fn from_language_name(name: &str) -> Option<CellKind> {
        let name = name.trim().to_ascii_lowercase();
        // A kernelspec name is usually the language plus a version ("python3").
        let base = name.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.');
        match base {
            "python" | "ipython" | "py" => Some(CellKind::Python),
            "rust" | "rs" | "evcxr" => Some(CellKind::Rust),
            "powershell" | "pwsh" | "ps" => Some(CellKind::PowerShell),
            "sql" => Some(CellKind::Sql),
            "shell" | "sh" | "bash" | "zsh" | "shellscript" => Some(CellKind::Shell),
            "markdown" | "md" => Some(CellKind::Markdown),
            "plaintext" | "plain_text" | "text" => Some(CellKind::PlainText),
            "raw" => Some(CellKind::Raw),
            _ => None,
        }
    }
}

impl fmt::Display for CellKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// One cell: its identity, its language, its text, and the record of its last
/// run.
///
/// Every field is private, and not out of habit. `id` is the notebook's to
/// assign; `kind` and `outputs` are tied together by a rule the compiler cannot
/// state on its own (outputs belong to the language that produced them), so
/// both go through methods that keep the pair honest. There is no `auto_run`
/// field, no `run_on_open`, and no execution state of any kind -- see the crate
/// docs for why that absence is the design and not an omission.
#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    id: CellId,
    kind: CellKind,
    source: String,
    collapsed: bool,
    outputs: Vec<Output>,
    metadata: Map<String, Value>,
}

impl Cell {
    pub(crate) fn new(id: CellId, kind: CellKind, source: String) -> Self {
        Self {
            id,
            kind,
            source,
            collapsed: false,
            outputs: Vec::new(),
            metadata: Map::new(),
        }
    }

    pub(crate) fn with_id(&self, id: CellId) -> Self {
        Self { id, ..self.clone() }
    }

    /// The handle to pass back to the notebook for any operation on this cell.
    pub fn id(&self) -> CellId {
        self.id
    }

    /// What language, if any, this cell is written in.
    pub fn kind(&self) -> CellKind {
        self.kind
    }

    /// Change the language, discarding any outputs.
    ///
    /// The outputs go because they were produced by the old language: leaving a
    /// Python traceback attached to a cell now labelled SQL would put a
    /// confident, wrong explanation in front of the user. Returns whether
    /// anything changed, so a caller can decide whether to mark the document
    /// dirty.
    pub fn set_kind(&mut self, kind: CellKind) -> bool {
        if self.kind == kind {
            return false;
        }
        self.kind = kind;
        self.outputs.clear();
        true
    }

    /// The cell's text, exactly as typed -- no trailing-newline convention, no
    /// normalisation. A round trip through `.ipynb` must not alter a single
    /// byte, which it cannot promise if we normalise anywhere.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Replace the text. Outputs are left alone: they are now stale, but
    /// silently deleting the result of a long run because the user fixed a typo
    /// loses more than it protects.
    pub fn set_source(&mut self, source: impl Into<String>) {
        self.source = source.into();
    }

    /// Direct access for an editor that edits in place rather than replacing
    /// the whole string. Same reasoning as [`Cell::set_source`]: no invariant
    /// binds the text, so none can be broken here.
    pub fn source_mut(&mut self) -> &mut String {
        &mut self.source
    }

    /// Whether the UI should show this cell folded. Presentational, but part of
    /// the document -- a user who collapsed forty cells expects them collapsed
    /// tomorrow -- so it is stored and exported.
    pub fn collapsed(&self) -> bool {
        self.collapsed
    }

    /// Set the fold state. Returns whether it changed.
    pub fn set_collapsed(&mut self, collapsed: bool) -> bool {
        let changed = self.collapsed != collapsed;
        self.collapsed = collapsed;
        changed
    }

    /// What the last run produced. Read-only; see [`Cell::set_outputs`].
    pub fn outputs(&self) -> &[Output] {
        &self.outputs
    }

    /// Record what a run produced.
    ///
    /// Refuses on a cell that cannot run, because an output on a Markdown cell
    /// is not a harmless oddity: `.ipynb` has nowhere to put it, so it would be
    /// dropped without a word on the next export and the user would lose data
    /// they were never told about. Failing here means the caller finds out at
    /// the moment of the mistake instead.
    pub fn set_outputs(&mut self, outputs: Vec<Output>) -> Result<(), NotebookError> {
        if !outputs.is_empty() && !self.kind.is_executable() {
            return Err(NotebookError::NotExecutable(self.kind));
        }
        self.outputs = outputs;
        Ok(())
    }

    /// Append one output, for a runner streaming results as they arrive.
    pub fn push_output(&mut self, output: Output) -> Result<(), NotebookError> {
        if !self.kind.is_executable() {
            return Err(NotebookError::NotExecutable(self.kind));
        }
        self.outputs.push(output);
        Ok(())
    }

    /// Throw away the recorded results. Always allowed -- clearing can break no
    /// rule.
    pub fn clear_outputs(&mut self) {
        self.outputs.clear();
    }

    /// Whatever else the file said about this cell.
    ///
    /// Kept verbatim so that a notebook edited here and reopened in Jupyter
    /// still has its slide settings, tags and extension keys. We interpret none
    /// of it. The one reserved name is `bachelorpad`, which export overwrites
    /// with our own kind and fold state.
    pub fn metadata(&self) -> &Map<String, Value> {
        &self.metadata
    }

    /// Mutable access to the passenger metadata. No invariant guards it; the
    /// `bachelorpad` key is the only one that will not survive a save.
    pub fn metadata_mut(&mut self) -> &mut Map<String, Value> {
        &mut self.metadata
    }

    pub(crate) fn set_metadata(&mut self, metadata: Map<String, Value>) {
        self.metadata = metadata;
    }

    pub(crate) fn take_outputs(&mut self) -> Vec<Output> {
        std::mem::take(&mut self.outputs)
    }

    pub(crate) fn extend_outputs(&mut self, outputs: Vec<Output>) {
        self.outputs.extend(outputs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::{Output, Stream};

    #[test]
    fn exactly_five_of_the_eight_kinds_can_run() {
        // specs.md section 12 lists eight kinds; the three that are prose or
        // verbatim text must never reach a runner. Pinning the count here
        // means adding a ninth kind forces a decision rather than defaulting.
        let runnable: Vec<_> = CellKind::ALL
            .into_iter()
            .filter(|k| k.is_executable())
            .collect();
        assert_eq!(runnable.len(), 5, "{runnable:?}");
        for prose in [CellKind::PlainText, CellKind::Markdown, CellKind::Raw] {
            assert!(!prose.is_executable(), "{prose} must never run");
        }
    }

    #[test]
    fn machine_names_are_unique_and_round_trip() {
        // These strings go into files. A collision would make two kinds
        // indistinguishable on the next open.
        let mut names: Vec<_> = CellKind::ALL.iter().map(|k| k.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), CellKind::ALL.len());

        for kind in CellKind::ALL {
            assert_eq!(CellKind::from_machine_name(kind.as_str()), Some(kind));
        }
        assert_eq!(CellKind::from_machine_name("perl"), None);
        assert_eq!(CellKind::from_machine_name(""), None);
    }

    #[test]
    fn language_names_are_recognised_across_the_spellings_other_tools_use() {
        assert_eq!(
            CellKind::from_language_name("python3"),
            Some(CellKind::Python)
        );
        assert_eq!(
            CellKind::from_language_name("  PWSH "),
            Some(CellKind::PowerShell)
        );
        assert_eq!(CellKind::from_language_name("bash"), Some(CellKind::Shell));
        assert_eq!(CellKind::from_language_name("evcxr"), Some(CellKind::Rust));
        // Not recognised is the correct answer for a language we cannot run;
        // a wrong guess offers to feed someone's text to the wrong process.
        assert_eq!(CellKind::from_language_name("julia"), None);
        assert_eq!(CellKind::from_language_name(""), None);
    }

    #[test]
    fn an_output_cannot_be_attached_to_a_cell_that_cannot_run() {
        // The type permits it; the domain does not, and the loss would only
        // surface on the next export.
        let mut cell = Cell::new(CellId::new(1), CellKind::Markdown, "# hi".to_owned());
        let err = cell
            .set_outputs(vec![Output::Text {
                stream: Stream::Stdout,
                text: "1".to_owned(),
            }])
            .unwrap_err();
        assert!(err.to_string().contains("Markdown"), "{err}");
        assert!(cell.outputs().is_empty());

        // Setting *no* outputs is always fine -- clearing breaks no rule.
        assert!(cell.set_outputs(Vec::new()).is_ok());
    }

    #[test]
    fn changing_the_language_drops_the_old_languages_outputs() {
        let mut cell = Cell::new(CellId::new(1), CellKind::Python, "print(1)".to_owned());
        cell.push_output(Output::Text {
            stream: Stream::Stdout,
            text: "1".to_owned(),
        })
        .unwrap();

        assert!(cell.set_kind(CellKind::Sql));
        assert!(
            cell.outputs().is_empty(),
            "a Python result must not be shown as a SQL result"
        );
        assert!(!cell.set_kind(CellKind::Sql), "no change, no work");
    }
}
