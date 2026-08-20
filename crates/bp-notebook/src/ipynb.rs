//! Jupyter `.ipynb` import and export, and the raw notebook JSON view.
//!
//! # The mapping, and why it is not one-to-one
//!
//! Jupyter has three cell types -- `code`, `markdown`, `raw` -- and states the
//! language once for the whole file, in `metadata.kernelspec` and
//! `metadata.language_info`. specs.md section 12 requires eight kinds *and*
//! mixed-language notebooks, so the two models do not line up and no amount of
//! cleverness will make them.
//!
//! Three ways out were available. Encoding the language in a `%%` cell magic
//! would alter the user's source, which is the one thing an interchange format
//! must never do. Writing one file per language would stop being interchange.
//! What is done instead is the third: **the Jupyter `cell_type` carries as much
//! as it can, and a namespaced `metadata.bachelorpad` key carries the rest.**
//!
//! | our kind | `cell_type` | `metadata.bachelorpad.kind` |
//! |---|---|---|
//! | Markdown | `markdown` | `markdown` |
//! | plain text | `raw` | `plain_text` |
//! | Raw | `raw` | `raw` |
//! | Python, Rust, PowerShell, SQL, Shell | `code` | the language |
//!
//! Unknown cell metadata is required to survive a Jupyter round trip by the
//! nbformat schema, so this is the escape hatch the format itself provides
//! rather than a private extension bolted on. A notebook we wrote opens in
//! Jupyter as an ordinary notebook; a notebook Jupyter wrote opens here with
//! its languages read from, in order: our key, `metadata.vscode.languageId`,
//! then the notebook's own kernelspec.
//!
//! When even that fails -- a `code` cell in a language we cannot name -- the
//! cell becomes [`CellKind::Raw`] and an [`ImportWarning::UnknownCodeLanguage`]
//! says so. Raw, not plain text, because Raw is the kind that is carried
//! verbatim and never run, which is exactly the right treatment for code we
//! cannot identify.
//!
//! # What the format cannot carry
//!
//! Stated here because a silent loss is the failure mode of every interchange
//! format. On import, each of these also produces an [`ImportWarning`] where
//! there is a specific cell to blame.
//!
//! * **Our eight kinds, if the metadata is stripped.** A notebook edited by a
//!   tool that discards unknown cell metadata comes back as Markdown and Raw.
//!   Nothing can be done about that from this side, and it is why the kind is
//!   also cross-checked against `cell_type`: if the two disagree, `cell_type`
//!   wins, on the grounds that it is what the other tool last acted on.
//! * **Outputs on a cell that is not code.** Jupyter has nowhere to put them.
//!   [`Cell::set_outputs`] refuses to create that state in the first place, so
//!   it can only arrive from a foreign file, and it is dropped with a warning.
//! * **[`Output::Table`] and [`Output::File`].** No native representation
//!   exists, so both travel as custom MIME bundles ([`TABLE_MIME`],
//!   [`FILE_MIME`]) inside a `display_data` output. Jupyter preserves them and
//!   cannot render them; we read them back exactly.
//! * **Execution counts and any other execution state.** Discarded on import,
//!   always, with an [`ImportWarning::DiscardedExecutionState`] when there was
//!   any. This is not tidiness: see the [`run`](crate::run) module for the rule
//!   it belongs to.
//! * **`attachments`** on Markdown cells, output-level `metadata`, and outputs
//!   whose type or MIME bundle we do not model. Dropped, each with a warning.
//! * **The `bachelorpad` metadata key**, at notebook and cell level. It is
//!   ours; export overwrites it and import removes it.
//! * **A kernelspec we did not write.** Preserved verbatim in
//!   [`Notebook::metadata`] and written straight back out. We never synthesise
//!   one: a mixed-language notebook has no single kernel, and claiming one we
//!   cannot honour would send the user to a kernel that fails on two thirds of
//!   their cells.
//!
//! Everything else round-trips exactly, and the test for that is a property
//! over arbitrary notebooks rather than a handful of examples:
//! export then import returns an equal [`Notebook`] and an empty warning list.

use serde_json::{Map, Value};

use crate::cell::{Cell, CellId, CellKind};
use crate::notebook::Notebook;
use crate::output::{Output, Stream};

/// The nbformat major version we read and write. Version 4 has been current
/// since 2015 and is what every tool in use emits.
pub const NBFORMAT_MAJOR: i64 = 4;

/// The minor version we write. 4.5 is the one that gives cells a stable `id`,
/// which is what lets our own identities survive a round trip.
pub const NBFORMAT_MINOR: i64 = 5;

/// The metadata key everything of ours lives under, at notebook, cell and
/// output level. One namespaced key rather than several loose ones, so that
/// stripping it is a single, obvious operation.
pub const BACHELORPAD_KEY: &str = "bachelorpad";

/// MIME type for [`Output::Table`], which Jupyter has no equivalent of. A
/// vendor tree type, so no other tool will mistake it for something it can
/// render.
pub const TABLE_MIME: &str = "application/vnd.bachelorpad.table+json";

/// MIME type for [`Output::File`], for the same reason as [`TABLE_MIME`].
pub const FILE_MIME: &str = "application/vnd.bachelorpad.file+json";

/// A file that could not be read as a notebook at all.
///
/// Only the failures that stop us producing *any* notebook are errors.
/// Everything survivable is an [`ImportWarning`] on a notebook the user still
/// gets to see, because a notebook with one unreadable output is far more
/// useful than a refusal.
#[derive(Debug, thiserror::Error)]
pub enum IpynbError {
    /// The bytes are not JSON.
    #[error("this file is not valid JSON, so it cannot be a notebook: {0}")]
    NotJson(#[from] serde_json::Error),

    /// Valid JSON, but not a notebook -- an array, a number, a string.
    #[error("a notebook has to be a JSON object, and this file contains {found}")]
    NotAnObject {
        /// What the top level actually was.
        found: &'static str,
    },

    /// No `nbformat` field. Required by the schema; without it we would be
    /// guessing at the shape of everything else.
    #[error("this file has no \"nbformat\" version, so it is not a Jupyter notebook")]
    NoFormatVersion,

    /// An nbformat major version we do not read.
    #[error(
        "this is a version {major} notebook; BachelorPad+ reads version {NBFORMAT_MAJOR}. Open it in Jupyter once to convert it"
    )]
    UnsupportedFormat {
        /// The major version the file claims.
        major: i64,
    },

    /// No `cells` array.
    #[error("this notebook has no \"cells\" list")]
    NoCells,
}

/// Something an import had to leave behind.
///
/// Reported rather than logged, because the caller is the only thing that knows
/// whether the user should be shown a note, a dialog, or nothing at all.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ImportWarning {
    /// A `cell_type` that is not one of Jupyter's three. Kept as Raw so the
    /// text is not lost.
    #[error("cell {cell_index} has an unknown type \"{cell_type}\" and was kept as raw text")]
    UnknownCellType {
        /// Position in the file, from zero.
        cell_index: usize,
        /// What the file said.
        cell_type: String,
    },

    /// A code cell in a language we cannot name. Kept as Raw, and therefore not
    /// runnable.
    #[error("cell {cell_index} is code in an unrecognised language and was kept as raw text")]
    UnknownCodeLanguage {
        /// Position in the file, from zero.
        cell_index: usize,
    },

    /// Our own `kind` metadata contradicted the Jupyter `cell_type`. The
    /// `cell_type` was used.
    #[error("cell {cell_index} was changed by another tool; its BachelorPad+ kind was out of date")]
    StaleKindMetadata {
        /// Position in the file, from zero.
        cell_index: usize,
    },

    /// The cell had been run. The count, and everything else about that run
    /// except its recorded outputs, is gone.
    #[error("cell {cell_index} had been run before; its execution state was not imported")]
    DiscardedExecutionState {
        /// Position in the file, from zero.
        cell_index: usize,
    },

    /// One output could not be represented.
    #[error("an output of cell {cell_index} was not imported: {reason}")]
    DroppedOutput {
        /// Position in the file, from zero.
        cell_index: usize,
        /// Position within that cell's outputs, from zero.
        output_index: usize,
        /// Why.
        reason: OutputDropped,
    },

    /// A Markdown cell's inline attachments. We have no model for them.
    #[error("cell {cell_index} had embedded attachments, which were not imported")]
    DroppedAttachments {
        /// Position in the file, from zero.
        cell_index: usize,
    },

    /// A `source` field that was neither a string nor a list of strings. What
    /// could be read was kept.
    #[error("the text of cell {cell_index} was malformed; what could be read was kept")]
    MalformedSource {
        /// Position in the file, from zero.
        cell_index: usize,
    },

    /// Two cells claiming the same `id`. The later one was given a fresh one,
    /// so that a handle to a cell cannot resolve to two different cells.
    #[error("cell {cell_index} reused another cell's identifier and was given a new one")]
    DuplicateCellId {
        /// Position in the file, from zero.
        cell_index: usize,
    },
}

/// Why one output was not imported.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OutputDropped {
    /// Not a JSON object.
    #[error("it was not an output")]
    NotAnObject,
    /// An `output_type` we do not model.
    #[error("its type \"{0}\" is not one BachelorPad+ can show")]
    UnknownOutputType(String),
    /// A display bundle offering only formats we do not model.
    #[error("it was offered only in formats BachelorPad+ cannot show")]
    NoModelledMimeType,
    /// An output attached to a cell that cannot run -- illegal in nbformat and
    /// unrepresentable here.
    #[error("the cell it belonged to is not code")]
    NotACodeCell,
}

/// The result of reading a notebook file: a notebook, and everything that was
/// lost getting there.
///
/// A struct rather than a bare `Notebook` on purpose, and the reason is the
/// security rule in [`run`](crate::run) as much as the honesty about loss:
/// importing hands back a *report*, and getting at the notebook is a second,
/// explicit step. There is no path from bytes to a running cell, and there is
/// no path that does not go through a caller deciding to take one.
#[derive(Debug, Clone)]
pub struct Import {
    notebook: Notebook,
    warnings: Vec<ImportWarning>,
}

impl Import {
    /// The notebook, borrowed.
    pub fn notebook(&self) -> &Notebook {
        &self.notebook
    }

    /// The notebook, taken. Named for the fact that it is a decision: the
    /// caller has seen [`Import::warnings`] by now, or has chosen not to.
    pub fn into_notebook(self) -> Notebook {
        self.notebook
    }

    /// Everything the file contained that this notebook does not, in file
    /// order.
    pub fn warnings(&self) -> &[ImportWarning] {
        &self.warnings
    }

    /// Whether the file arrived intact. The common case, and worth being able
    /// to ask in one call so that the UI shows nothing when there is nothing to
    /// show.
    pub fn is_lossless(&self) -> bool {
        self.warnings.is_empty()
    }
}

/// Turn a notebook into `.ipynb` JSON.
///
/// Total: there is no notebook this can refuse, which is the point of
/// [`Cell::set_outputs`] refusing to build the one state that has nowhere to
/// go.
pub fn export_ipynb(notebook: &Notebook) -> Value {
    let mut metadata = notebook.metadata().clone();
    metadata.insert(
        BACHELORPAD_KEY.to_owned(),
        Value::Object(Map::from_iter([(
            "notebook_format".to_owned(),
            Value::from(1),
        )])),
    );

    let mut root = Map::new();
    root.insert(
        "cells".to_owned(),
        Value::Array(notebook.cells().iter().map(export_cell).collect()),
    );
    root.insert("metadata".to_owned(), Value::Object(metadata));
    root.insert("nbformat".to_owned(), Value::from(NBFORMAT_MAJOR));
    root.insert("nbformat_minor".to_owned(), Value::from(NBFORMAT_MINOR));
    Value::Object(root)
}

/// Read `.ipynb` JSON into a notebook and a list of what was lost.
///
/// Nothing here starts, schedules, or enables anything. It cannot: see
/// [`Import`] and the [`run`](crate::run) module.
pub fn import_ipynb(value: &Value) -> Result<Import, IpynbError> {
    let root = value.as_object().ok_or(IpynbError::NotAnObject {
        found: describe(value),
    })?;

    let major = root
        .get("nbformat")
        .and_then(Value::as_i64)
        .ok_or(IpynbError::NoFormatVersion)?;
    if major != NBFORMAT_MAJOR {
        return Err(IpynbError::UnsupportedFormat { major });
    }

    let cells = root
        .get("cells")
        .and_then(Value::as_array)
        .ok_or(IpynbError::NoCells)?;

    let mut metadata = root
        .get("metadata")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    metadata.remove(BACHELORPAD_KEY);

    let notebook_language = metadata
        .get("language_info")
        .and_then(|l| l.get("name"))
        .or_else(|| metadata.get("kernelspec").and_then(|k| k.get("language")))
        .or_else(|| metadata.get("kernelspec").and_then(|k| k.get("name")))
        .and_then(Value::as_str)
        .and_then(CellKind::from_language_name);

    let mut warnings = Vec::new();
    let mut parsed = Vec::with_capacity(cells.len());
    for (index, cell) in cells.iter().enumerate() {
        parsed.push(import_cell(index, cell, notebook_language, &mut warnings));
    }

    let ids = assign_ids(&parsed, &mut warnings);
    let cells = parsed
        .into_iter()
        .zip(ids)
        .map(|(partial, id)| partial.into_cell(id))
        .collect();

    Ok(Import {
        notebook: Notebook::from_parts(cells, metadata),
        warnings,
    })
}

/// The raw notebook JSON view specs.md section 12 asks for.
///
/// It shows the `.ipynb` form, not some internal shape of ours. A raw view
/// exists so that a user can see and hand-edit exactly what is on disk and what
/// another tool will read; a view of a private representation would answer a
/// question nobody asked.
pub fn raw_json_view(notebook: &Notebook) -> String {
    let mut text =
        serde_json::to_string_pretty(&export_ipynb(notebook)).expect("a notebook is always JSON");
    text.push('\n');
    text
}

/// Read back a raw JSON view the user has edited.
///
/// The same path as [`import_ipynb`], deliberately: text typed into the raw
/// view is exactly as untrusted as text read from a file, and giving it a
/// gentler route would be the pasted-notebook hole specs.md section 15 closes.
pub fn parse_raw_json_view(text: &str) -> Result<Import, IpynbError> {
    let value: Value = serde_json::from_str(text)?;
    import_ipynb(&value)
}

// --- export ------------------------------------------------------------

fn jupyter_cell_type(kind: CellKind) -> &'static str {
    match kind {
        CellKind::Markdown => "markdown",
        CellKind::PlainText | CellKind::Raw => "raw",
        _ => "code",
    }
}

fn export_cell(cell: &Cell) -> Value {
    let mut ours = Map::new();
    ours.insert(
        "kind".to_owned(),
        Value::String(cell.kind().as_str().to_owned()),
    );
    if cell.collapsed() {
        // Written only when true: a file full of `"collapsed": false` is
        // noise, and our key being present at all is what tells import that
        // its absence means false rather than "unknown".
        ours.insert("collapsed".to_owned(), Value::Bool(true));
    }
    let mut metadata = cell.metadata().clone();
    metadata.insert(BACHELORPAD_KEY.to_owned(), Value::Object(ours));

    let cell_type = jupyter_cell_type(cell.kind());
    let mut object = Map::new();
    object.insert("cell_type".to_owned(), Value::String(cell_type.to_owned()));
    object.insert(
        "id".to_owned(),
        Value::String(cell.id().value().to_string()),
    );
    object.insert("metadata".to_owned(), Value::Object(metadata));
    object.insert("source".to_owned(), split_lines(cell.source()));
    if cell_type == "code" {
        // Required by the schema. Always null: we export a document, never a
        // session, so there is no count to write.
        object.insert("execution_count".to_owned(), Value::Null);
        object.insert(
            "outputs".to_owned(),
            Value::Array(cell.outputs().iter().map(export_output).collect()),
        );
    }
    Value::Object(object)
}

/// Split text the way nbformat does: keep the newlines, so joining the list
/// back together is exact for every possible string, including the empty one.
fn split_lines(text: &str) -> Value {
    Value::Array(
        text.split_inclusive('\n')
            .map(|line| Value::String(line.to_owned()))
            .collect(),
    )
}

fn display_data(data: Map<String, Value>, marker: Map<String, Value>) -> Value {
    let mut object = Map::new();
    object.insert(
        "output_type".to_owned(),
        Value::String("display_data".into()),
    );
    object.insert("data".to_owned(), Value::Object(data));
    object.insert(
        "metadata".to_owned(),
        Value::Object(Map::from_iter([(
            BACHELORPAD_KEY.to_owned(),
            Value::Object(marker),
        )])),
    );
    Value::Object(object)
}

fn marker(output: &str) -> Map<String, Value> {
    Map::from_iter([("output".to_owned(), Value::String(output.to_owned()))])
}

fn export_output(output: &Output) -> Value {
    match output {
        Output::Text { stream, text } => {
            let name = match stream {
                Stream::Stdout => "stdout",
                Stream::Stderr => "stderr",
            };
            let mut object = Map::new();
            object.insert("output_type".to_owned(), Value::String("stream".into()));
            object.insert("name".to_owned(), Value::String(name.to_owned()));
            object.insert("text".to_owned(), split_lines(text));
            Value::Object(object)
        }
        Output::Error {
            name,
            message,
            traceback,
        } => {
            let mut object = Map::new();
            object.insert("output_type".to_owned(), Value::String("error".into()));
            object.insert("ename".to_owned(), Value::String(name.clone()));
            object.insert("evalue".to_owned(), Value::String(message.clone()));
            object.insert(
                "traceback".to_owned(),
                Value::Array(traceback.iter().cloned().map(Value::String).collect()),
            );
            Value::Object(object)
        }
        Output::Json { value } => display_data(
            Map::from_iter([("application/json".to_owned(), value.clone())]),
            marker("json"),
        ),
        Output::Html { html } => display_data(
            Map::from_iter([("text/html".to_owned(), split_lines(html))]),
            marker("html"),
        ),
        Output::Image {
            media_type,
            data_base64,
            alt,
        } => {
            // The media type is repeated into our marker rather than inferred
            // from the bundle key on the way back: a caller may hand us a
            // media type that does not begin with `image/`, and sniffing would
            // then read the output back as something else entirely.
            let mut mark = marker("image");
            mark.insert("media_type".to_owned(), Value::String(media_type.clone()));
            if let Some(alt) = alt {
                mark.insert("alt".to_owned(), Value::String(alt.clone()));
            }
            display_data(
                Map::from_iter([(media_type.clone(), Value::String(data_base64.clone()))]),
                mark,
            )
        }
        Output::Table { columns, rows } => {
            let payload = serde_json::json!({ "columns": columns, "rows": rows });
            display_data(
                Map::from_iter([(TABLE_MIME.to_owned(), payload)]),
                marker("table"),
            )
        }
        Output::File {
            name,
            media_type,
            data_base64,
        } => {
            let payload = serde_json::json!({
                "name": name,
                "media_type": media_type,
                "data_base64": data_base64,
            });
            display_data(
                Map::from_iter([(FILE_MIME.to_owned(), payload)]),
                marker("file"),
            )
        }
    }
}

// --- import ------------------------------------------------------------

fn describe(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a true/false value",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "a list",
        Value::Object(_) => "an object",
    }
}

/// A cell read from the file, before identities have been handed out.
struct PartialCell {
    claimed_id: Option<u64>,
    kind: CellKind,
    source: String,
    collapsed: bool,
    outputs: Vec<Output>,
    metadata: Map<String, Value>,
}

impl PartialCell {
    fn into_cell(self, id: CellId) -> Cell {
        let mut cell = Cell::new(id, self.kind, self.source);
        cell.set_collapsed(self.collapsed);
        cell.set_metadata(self.metadata);
        // Cannot fail: `import_cell` drops outputs, with a warning, for every
        // kind that is not executable.
        cell.extend_outputs(self.outputs);
        cell
    }
}

fn import_cell(
    index: usize,
    value: &Value,
    notebook_language: Option<CellKind>,
    warnings: &mut Vec<ImportWarning>,
) -> PartialCell {
    let empty = Map::new();
    let object = value.as_object().unwrap_or(&empty);
    let cell_type = object
        .get("cell_type")
        .and_then(Value::as_str)
        .unwrap_or("raw");

    let mut metadata = object
        .get("metadata")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let ours = metadata
        .remove(BACHELORPAD_KEY)
        .and_then(|v| v.as_object().cloned());

    let claimed = ours
        .as_ref()
        .and_then(|o| o.get("kind"))
        .and_then(Value::as_str)
        .and_then(CellKind::from_machine_name);
    let kind = match claimed {
        Some(kind) if jupyter_cell_type(kind) == cell_type => kind,
        Some(_) => {
            warnings.push(ImportWarning::StaleKindMetadata { cell_index: index });
            infer_kind(index, cell_type, &metadata, notebook_language, warnings)
        }
        None => infer_kind(index, cell_type, &metadata, notebook_language, warnings),
    };

    let collapsed = match &ours {
        // Our key is authoritative when present, including by its silence:
        // we always write it, so a missing `collapsed` inside it means false.
        Some(ours) => ours.get("collapsed").and_then(Value::as_bool) == Some(true),
        None => {
            metadata
                .get("jupyter")
                .and_then(|j| j.get("source_hidden"))
                .and_then(Value::as_bool)
                == Some(true)
                || metadata.get("collapsed").and_then(Value::as_bool) == Some(true)
        }
    };

    let (source, malformed) = match object.get("source") {
        Some(value) => read_text(value),
        None => (String::new(), false),
    };
    if malformed {
        warnings.push(ImportWarning::MalformedSource { cell_index: index });
    }

    if object
        .get("attachments")
        .and_then(Value::as_object)
        .is_some_and(|a| !a.is_empty())
    {
        warnings.push(ImportWarning::DroppedAttachments { cell_index: index });
    }

    if object.get("execution_count").is_some_and(|c| !c.is_null()) {
        warnings.push(ImportWarning::DiscardedExecutionState { cell_index: index });
    }

    let mut outputs = Vec::new();
    if let Some(raw) = object.get("outputs").and_then(Value::as_array) {
        for (output_index, raw) in raw.iter().enumerate() {
            if !kind.is_executable() {
                warnings.push(ImportWarning::DroppedOutput {
                    cell_index: index,
                    output_index,
                    reason: OutputDropped::NotACodeCell,
                });
                continue;
            }
            match import_output(raw) {
                Ok(output) => outputs.push(output),
                Err(reason) => warnings.push(ImportWarning::DroppedOutput {
                    cell_index: index,
                    output_index,
                    reason,
                }),
            }
        }
    }

    PartialCell {
        claimed_id: object
            .get("id")
            .and_then(Value::as_str)
            .and_then(|id| id.parse::<u64>().ok())
            .filter(|id| *id > 0),
        kind,
        source,
        collapsed,
        outputs,
        metadata,
    }
}

fn infer_kind(
    index: usize,
    cell_type: &str,
    metadata: &Map<String, Value>,
    notebook_language: Option<CellKind>,
    warnings: &mut Vec<ImportWarning>,
) -> CellKind {
    match cell_type {
        "markdown" => CellKind::Markdown,
        "raw" => CellKind::Raw,
        "code" => {
            let stated = metadata
                .get("vscode")
                .and_then(|v| v.get("languageId"))
                .or_else(|| {
                    metadata
                        .get("polyglot_notebook")
                        .and_then(|p| p.get("kernelName"))
                })
                .and_then(Value::as_str)
                .and_then(CellKind::from_language_name);
            match stated.or(notebook_language).filter(|k| k.is_executable()) {
                Some(kind) => kind,
                None => {
                    warnings.push(ImportWarning::UnknownCodeLanguage { cell_index: index });
                    CellKind::Raw
                }
            }
        }
        other => {
            warnings.push(ImportWarning::UnknownCellType {
                cell_index: index,
                cell_type: other.to_owned(),
            });
            CellKind::Raw
        }
    }
}

/// nbformat writes text as either a string or a list of strings. Returns what
/// could be read and whether anything had to be skipped.
fn read_text(value: &Value) -> (String, bool) {
    match value {
        Value::String(text) => (text.clone(), false),
        Value::Array(items) => {
            let mut text = String::new();
            let mut malformed = false;
            for item in items {
                match item.as_str() {
                    Some(line) => text.push_str(line),
                    None => malformed = true,
                }
            }
            (text, malformed)
        }
        _ => (String::new(), true),
    }
}

fn import_output(value: &Value) -> Result<Output, OutputDropped> {
    let object = value.as_object().ok_or(OutputDropped::NotAnObject)?;
    let output_type = object
        .get("output_type")
        .and_then(Value::as_str)
        .unwrap_or_default();

    match output_type {
        "stream" => {
            let stream = match object.get("name").and_then(Value::as_str) {
                Some("stderr") => Stream::Stderr,
                _ => Stream::Stdout,
            };
            let text = object.get("text").map(read_text).unwrap_or_default().0;
            Ok(Output::Text { stream, text })
        }
        "error" => Ok(Output::Error {
            name: string_at(object, "ename"),
            message: string_at(object, "evalue"),
            traceback: object
                .get("traceback")
                .and_then(Value::as_array)
                .map(|frames| {
                    frames
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default(),
        }),
        "display_data" | "execute_result" => {
            let data = object
                .get("data")
                .and_then(Value::as_object)
                .ok_or(OutputDropped::NoModelledMimeType)?;
            let mark = object
                .get("metadata")
                .and_then(|m| m.get(BACHELORPAD_KEY))
                .and_then(Value::as_object);
            from_marker(data, mark)
                .or_else(|| sniff_bundle(data))
                .ok_or(OutputDropped::NoModelledMimeType)
        }
        other => Err(OutputDropped::UnknownOutputType(other.to_owned())),
    }
}

fn string_at(object: &Map<String, Value>, key: &str) -> String {
    object
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// Read a bundle we wrote ourselves. Returns `None` if the marker is absent or
/// does not match the bundle, in which case sniffing takes over -- being
/// lenient here costs nothing and rescues a file someone hand-edited.
fn from_marker(data: &Map<String, Value>, mark: Option<&Map<String, Value>>) -> Option<Output> {
    let mark = mark?;
    match mark.get("output").and_then(Value::as_str)? {
        "json" => Some(Output::Json {
            value: data.get("application/json")?.clone(),
        }),
        "html" => Some(Output::Html {
            html: read_text(data.get("text/html")?).0,
        }),
        "image" => {
            let media_type = mark.get("media_type").and_then(Value::as_str)?.to_owned();
            Some(Output::Image {
                data_base64: read_text(data.get(&media_type)?).0,
                media_type,
                alt: mark.get("alt").and_then(Value::as_str).map(str::to_owned),
            })
        }
        "table" => read_table(data.get(TABLE_MIME)?),
        "file" => read_file(data.get(FILE_MIME)?),
        _ => None,
    }
}

/// Read a bundle some other tool wrote, in order of how much of it we can
/// show. Our own types come first so a hand-edited file still reads back, then
/// the richest standard formats, and `text/plain` last because it is the
/// fallback every bundle carries.
fn sniff_bundle(data: &Map<String, Value>) -> Option<Output> {
    if let Some(table) = data.get(TABLE_MIME).and_then(read_table) {
        return Some(table);
    }
    if let Some(file) = data.get(FILE_MIME).and_then(read_file) {
        return Some(file);
    }
    if let Some(value) = data.get("application/json") {
        return Some(Output::Json {
            value: value.clone(),
        });
    }
    if let Some(html) = data.get("text/html") {
        return Some(Output::Html {
            html: read_text(html).0,
        });
    }
    if let Some((media_type, payload)) = data.iter().find(|(k, _)| k.starts_with("image/")) {
        return Some(Output::Image {
            media_type: media_type.clone(),
            data_base64: read_text(payload).0,
            alt: None,
        });
    }
    data.get("text/plain").map(|text| Output::Text {
        stream: Stream::Stdout,
        text: read_text(text).0,
    })
}

fn read_table(value: &Value) -> Option<Output> {
    let object = value.as_object()?;
    let columns = object
        .get("columns")?
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    let rows = object
        .get("rows")?
        .as_array()?
        .iter()
        .filter_map(Value::as_array)
        .map(|row| {
            row.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .collect();
    Some(Output::Table { columns, rows })
}

fn read_file(value: &Value) -> Option<Output> {
    let object = value.as_object()?;
    Some(Output::File {
        name: object.get("name")?.as_str()?.to_owned(),
        media_type: object.get("media_type")?.as_str()?.to_owned(),
        data_base64: object.get("data_base64")?.as_str()?.to_owned(),
    })
}

/// Hand out identities, honouring the ones the file claimed where they are
/// usable and unique.
///
/// Honouring them is what makes our own export-then-import an exact identity;
/// refusing a duplicate is what stops one `CellId` naming two cells, which
/// would break every operation in [`Notebook`] at once.
fn assign_ids(cells: &[PartialCell], warnings: &mut Vec<ImportWarning>) -> Vec<CellId> {
    let mut taken = std::collections::BTreeSet::new();
    let mut accepted = Vec::with_capacity(cells.len());
    for (index, cell) in cells.iter().enumerate() {
        match cell.claimed_id {
            Some(id) if taken.insert(id) => accepted.push(Some(id)),
            Some(_) => {
                warnings.push(ImportWarning::DuplicateCellId { cell_index: index });
                accepted.push(None);
            }
            None => accepted.push(None),
        }
    }

    let mut next = taken.iter().next_back().map_or(1, |highest| highest + 1);
    accepted
        .into_iter()
        .map(|id| match id {
            Some(id) => CellId::new(id),
            None => {
                let fresh = next;
                next += 1;
                CellId::new(fresh)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::arb_notebook;
    use proptest::prelude::*;
    use serde_json::json;

    fn import_str(text: &str) -> Import {
        parse_raw_json_view(text).expect("should import")
    }

    #[test]
    fn a_file_that_is_not_a_notebook_says_which_way_it_is_not() {
        assert!(matches!(
            parse_raw_json_view("{ not json").unwrap_err(),
            IpynbError::NotJson(_)
        ));
        assert!(matches!(
            parse_raw_json_view("[]").unwrap_err(),
            IpynbError::NotAnObject { found: "a list" }
        ));
        assert!(matches!(
            parse_raw_json_view("{}").unwrap_err(),
            IpynbError::NoFormatVersion
        ));
        assert!(matches!(
            import_ipynb(&json!({ "nbformat": 4 })).unwrap_err(),
            IpynbError::NoCells
        ));

        // The boundary, and one past it in both directions.
        assert!(import_ipynb(&json!({ "nbformat": 4, "cells": [] })).is_ok());
        for major in [3, 5] {
            let err = import_ipynb(&json!({ "nbformat": major, "cells": [] })).unwrap_err();
            assert!(matches!(err, IpynbError::UnsupportedFormat { .. }));
            assert!(
                err.to_string().contains(&major.to_string()),
                "the message must name the version the file claims: {err}"
            );
        }
    }

    #[test]
    fn an_empty_notebook_survives_the_round_trip() {
        let notebook = Notebook::new();
        let back = import_ipynb(&export_ipynb(&notebook)).unwrap();
        assert!(back.is_lossless());
        assert_eq!(back.into_notebook(), notebook);
    }

    #[test]
    fn a_foreign_python_notebook_reads_as_python_from_its_kernelspec_alone() {
        let import = import_str(
            r##"{
              "nbformat": 4, "nbformat_minor": 5,
              "metadata": { "kernelspec": { "name": "python3", "language": "python" } },
              "cells": [
                { "cell_type": "markdown", "metadata": {}, "source": ["# Title\n"] },
                { "cell_type": "code", "metadata": {}, "execution_count": null,
                  "outputs": [], "source": ["print(1)"] },
                { "cell_type": "raw", "metadata": {}, "source": "verbatim" }
              ]
            }"##,
        );
        assert!(import.is_lossless(), "{:?}", import.warnings());
        let kinds: Vec<_> = import.notebook().cells().iter().map(|c| c.kind()).collect();
        assert_eq!(
            kinds,
            vec![CellKind::Markdown, CellKind::Python, CellKind::Raw]
        );

        // The kernelspec is not ours, so it is carried, not interpreted away.
        assert!(import.notebook().metadata().contains_key("kernelspec"));
        let exported = export_ipynb(import.notebook());
        assert!(exported["metadata"]["kernelspec"].is_object());
    }

    #[test]
    fn a_code_cell_in_a_language_we_cannot_name_is_kept_as_raw_and_not_runnable() {
        let import = import_str(
            r##"{ "nbformat": 4, "cells": [
                 { "cell_type": "code", "metadata": {}, "source": "println(1)" } ] }"##,
        );
        assert_eq!(
            import.warnings(),
            [ImportWarning::UnknownCodeLanguage { cell_index: 0 }]
        );
        let cell = &import.notebook().cells()[0];
        assert_eq!(cell.kind(), CellKind::Raw);
        assert!(!cell.kind().is_executable(), "unknown code must not run");
        assert_eq!(cell.source(), "println(1)", "the text is never lost");
    }

    #[test]
    fn per_cell_language_beats_the_kernelspec_so_a_mixed_notebook_stays_mixed() {
        let import = import_str(
            r##"{
              "nbformat": 4,
              "metadata": { "language_info": { "name": "python" } },
              "cells": [
                { "cell_type": "code", "metadata": {}, "source": "x=1" },
                { "cell_type": "code", "source": "select 1",
                  "metadata": { "vscode": { "languageId": "sql" } } },
                { "cell_type": "code", "source": "ls",
                  "metadata": { "bachelorpad": { "kind": "shell" } } }
              ]
            }"##,
        );
        let kinds: Vec<_> = import.notebook().cells().iter().map(|c| c.kind()).collect();
        assert_eq!(
            kinds,
            vec![CellKind::Python, CellKind::Sql, CellKind::Shell]
        );
    }

    #[test]
    fn plain_text_and_raw_both_travel_as_raw_and_come_back_apart() {
        let mut notebook = Notebook::new();
        notebook.push(CellKind::PlainText, "a note");
        notebook.push(CellKind::Raw, "a note");

        let exported = export_ipynb(&notebook);
        assert_eq!(exported["cells"][0]["cell_type"], "raw");
        assert_eq!(exported["cells"][1]["cell_type"], "raw");

        let back = import_ipynb(&exported).unwrap().into_notebook();
        assert_eq!(back.cells()[0].kind(), CellKind::PlainText);
        assert_eq!(back.cells()[1].kind(), CellKind::Raw);
        assert_eq!(back, notebook);
    }

    #[test]
    fn a_kind_that_disagrees_with_the_cell_type_loses_to_the_cell_type() {
        // What a Jupyter user converting a code cell to Markdown leaves
        // behind: our metadata is stale, and the other tool's view is newer.
        let import = import_str(
            r##"{ "nbformat": 4, "cells": [
                 { "cell_type": "markdown", "source": "# now prose",
                   "metadata": { "bachelorpad": { "kind": "python" } } } ] }"##,
        );
        assert_eq!(
            import.warnings(),
            [ImportWarning::StaleKindMetadata { cell_index: 0 }]
        );
        assert_eq!(import.notebook().cells()[0].kind(), CellKind::Markdown);
    }

    #[test]
    fn execution_state_is_discarded_and_the_user_is_told() {
        let import = import_str(
            r##"{ "nbformat": 4,
                 "metadata": { "kernelspec": { "language": "python" } },
                 "cells": [
                 { "cell_type": "code", "execution_count": 7, "source": "x=1",
                   "metadata": {},
                   "outputs": [ { "output_type": "stream", "name": "stdout",
                                  "text": ["done\n"] } ] } ] }"##,
        );
        assert_eq!(
            import.warnings(),
            [ImportWarning::DiscardedExecutionState { cell_index: 0 }]
        );

        // The outputs are kept -- they are a record of what happened. The
        // count, which is the only thing that says a session exists, is not.
        let cell = &import.notebook().cells()[0];
        assert_eq!(cell.outputs().len(), 1);
        let exported = export_ipynb(import.notebook());
        assert_eq!(exported["cells"][0]["execution_count"], Value::Null);
    }

    #[test]
    fn outputs_on_a_cell_that_is_not_code_are_dropped_with_a_reason() {
        let import = import_str(
            r##"{ "nbformat": 4, "cells": [
                 { "cell_type": "markdown", "source": "# hi", "metadata": {},
                   "outputs": [ { "output_type": "stream", "name": "stdout",
                                  "text": "x" } ] } ] }"##,
        );
        assert_eq!(
            import.warnings(),
            [ImportWarning::DroppedOutput {
                cell_index: 0,
                output_index: 0,
                reason: OutputDropped::NotACodeCell,
            }]
        );
        assert!(import.notebook().cells()[0].outputs().is_empty());
    }

    #[test]
    fn outputs_we_cannot_show_are_dropped_one_at_a_time_and_the_rest_survive() {
        let import = import_str(
            r##"{ "nbformat": 4,
                 "metadata": { "kernelspec": { "language": "python" } },
                 "cells": [ { "cell_type": "code", "source": "x", "metadata": {},
                   "outputs": [
                     { "output_type": "update_display_data", "data": {} },
                     { "output_type": "display_data",
                       "data": { "application/x-nonesuch": "?" }, "metadata": {} },
                     "not an output",
                     { "output_type": "stream", "name": "stderr", "text": "kept\n" }
                   ] } ] }"##,
        );
        let reasons: Vec<_> = import
            .warnings()
            .iter()
            .map(|w| match w {
                ImportWarning::DroppedOutput { reason, .. } => reason.clone(),
                other => panic!("unexpected warning: {other}"),
            })
            .collect();
        assert_eq!(
            reasons,
            vec![
                OutputDropped::UnknownOutputType("update_display_data".to_owned()),
                OutputDropped::NoModelledMimeType,
                OutputDropped::NotAnObject,
            ]
        );
        assert_eq!(
            import.notebook().cells()[0].outputs(),
            [Output::Text {
                stream: Stream::Stderr,
                text: "kept\n".to_owned()
            }]
        );
    }

    #[test]
    fn a_foreign_display_bundle_is_read_at_the_richest_format_it_offers() {
        let import = import_str(
            r##"{ "nbformat": 4,
                 "metadata": { "kernelspec": { "language": "python" } },
                 "cells": [ { "cell_type": "code", "source": "x", "metadata": {},
                   "outputs": [
                     { "output_type": "execute_result", "execution_count": 1,
                       "metadata": {},
                       "data": { "text/plain": "<table>", "text/html": ["<table/>"] } },
                     { "output_type": "display_data", "metadata": {},
                       "data": { "text/plain": "just text" } }
                   ] } ] }"##,
        );
        assert_eq!(
            import.notebook().cells()[0].outputs(),
            [
                Output::Html {
                    html: "<table/>".to_owned()
                },
                Output::Text {
                    stream: Stream::Stdout,
                    text: "just text".to_owned()
                },
            ]
        );
    }

    #[test]
    fn a_table_and_a_file_travel_in_bundles_jupyter_will_carry_but_not_render() {
        let mut notebook = Notebook::new();
        let id = notebook.push(CellKind::Sql, "select 1");
        notebook
            .cell_mut(id)
            .unwrap()
            .set_outputs(vec![
                Output::Table {
                    columns: vec!["n".to_owned()],
                    rows: vec![vec!["1".to_owned()]],
                },
                Output::File {
                    name: "out.csv".to_owned(),
                    media_type: "text/csv".to_owned(),
                    data_base64: "bg==".to_owned(),
                },
            ])
            .unwrap();

        let exported = export_ipynb(&notebook);
        let bundles = &exported["cells"][0]["outputs"];
        assert!(bundles[0]["data"][TABLE_MIME].is_object());
        assert!(bundles[1]["data"][FILE_MIME].is_object());

        let back = import_ipynb(&exported).unwrap();
        assert!(back.is_lossless());
        assert_eq!(back.into_notebook(), notebook);
    }

    #[test]
    fn duplicate_identifiers_are_broken_up_so_one_handle_names_one_cell() {
        let import = import_str(
            r##"{ "nbformat": 4, "cells": [
                 { "cell_type": "raw", "id": "7", "source": "a", "metadata": {} },
                 { "cell_type": "raw", "id": "7", "source": "b", "metadata": {} },
                 { "cell_type": "raw", "id": "not-a-number", "source": "c", "metadata": {} } ] }"##,
        );
        assert_eq!(
            import.warnings(),
            [ImportWarning::DuplicateCellId { cell_index: 1 }]
        );
        let ids: Vec<_> = import
            .notebook()
            .cells()
            .iter()
            .map(|c| c.id().value())
            .collect();
        assert_eq!(ids, vec![7, 8, 9], "fresh ids start above the claimed one");

        let mut unique = ids.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), ids.len());
    }

    #[test]
    fn attachments_and_malformed_text_are_reported_rather_than_swallowed() {
        let import = import_str(
            r##"{ "nbformat": 4, "cells": [
                 { "cell_type": "markdown", "metadata": {},
                   "attachments": { "a.png": { "image/png": "x" } },
                   "source": ["kept ", 7, "and this\n"] } ] }"##,
        );
        assert_eq!(
            import.warnings(),
            [
                ImportWarning::MalformedSource { cell_index: 0 },
                ImportWarning::DroppedAttachments { cell_index: 0 },
            ]
        );
        assert_eq!(
            import.notebook().cells()[0].source(),
            "kept and this\n",
            "what could be read is kept"
        );
    }

    #[test]
    fn the_reserved_metadata_key_is_ours_and_does_not_survive() {
        // Documented loss: a caller that stores something under our key gets
        // it back overwritten, at both levels.
        let mut notebook = Notebook::new();
        notebook
            .metadata_mut()
            .insert(BACHELORPAD_KEY.to_owned(), json!({ "mine": true }));
        let id = notebook.push(CellKind::Raw, "x");
        notebook
            .cell_mut(id)
            .unwrap()
            .metadata_mut()
            .insert(BACHELORPAD_KEY.to_owned(), json!({ "mine": true }));

        let back = import_ipynb(&export_ipynb(&notebook))
            .unwrap()
            .into_notebook();
        assert!(!back.metadata().contains_key(BACHELORPAD_KEY));
        assert!(!back.cells()[0].metadata().contains_key(BACHELORPAD_KEY));
        assert_ne!(back, notebook);
    }

    #[test]
    fn the_fold_state_of_a_foreign_cell_is_read_from_jupyters_own_keys() {
        let import = import_str(
            r##"{ "nbformat": 4, "cells": [
                 { "cell_type": "raw", "source": "a",
                   "metadata": { "jupyter": { "source_hidden": true } } },
                 { "cell_type": "raw", "source": "b", "metadata": { "collapsed": true } },
                 { "cell_type": "raw", "source": "c", "metadata": {} } ] }"##,
        );
        let folded: Vec<_> = import
            .notebook()
            .cells()
            .iter()
            .map(|c| c.collapsed())
            .collect();
        assert_eq!(folded, vec![true, true, false]);

        // Those foreign keys stay in the metadata, so Jupyter still sees them.
        assert!(
            import.notebook().cells()[0]
                .metadata()
                .contains_key("jupyter")
        );
    }

    #[test]
    fn source_as_one_string_and_as_a_list_of_lines_mean_the_same_thing() {
        let one = import_str(
            r##"{ "nbformat": 4, "cells": [
                 { "cell_type": "raw", "id": "1", "metadata": {}, "source": "a\nb" } ] }"##,
        );
        let many = import_str(
            r##"{ "nbformat": 4, "cells": [
                 { "cell_type": "raw", "id": "1", "metadata": {}, "source": ["a\n", "b"] } ] }"##,
        );
        assert_eq!(one.notebook(), many.notebook());
        assert_eq!(one.notebook().cells()[0].source(), "a\nb");
    }

    #[test]
    fn the_raw_view_is_the_file_and_nothing_else() {
        let mut notebook = Notebook::new();
        notebook.push(CellKind::Python, "print(1)\n");
        let text = raw_json_view(&notebook);

        assert!(text.ends_with('\n'), "a file ends with a newline");
        assert!(text.contains("\"nbformat\": 4"), "{text}");
        assert!(text.contains("\"cell_type\": \"code\""), "{text}");
        assert_eq!(
            parse_raw_json_view(&text).unwrap().into_notebook(),
            notebook
        );
    }

    proptest! {
        /// The property the whole mapping exists to satisfy. Everything the
        /// format can carry comes back byte for byte, and the warning list --
        /// which is the crate's own account of what it lost -- is empty.
        #[test]
        fn exporting_a_notebook_and_importing_it_returns_the_exact_notebook(
            notebook in arb_notebook(0..5),
        ) {
            let import = import_ipynb(&export_ipynb(&notebook)).unwrap();
            prop_assert!(
                import.is_lossless(),
                "a notebook we wrote ourselves lost something: {:?}",
                import.warnings()
            );
            prop_assert_eq!(import.into_notebook(), notebook);
        }

        /// The raw JSON view is the same thing through the text path, which is
        /// what makes hand-editing it safe to offer.
        #[test]
        fn the_raw_json_view_reads_back_as_the_exact_notebook(
            notebook in arb_notebook(0..5),
        ) {
            let text = raw_json_view(&notebook);
            let import = parse_raw_json_view(&text).unwrap();
            prop_assert!(import.is_lossless(), "{:?}", import.warnings());
            prop_assert_eq!(import.into_notebook(), notebook);
        }

        /// Whatever a notebook-shaped file contains, what comes out is a
        /// notebook a caller could have built by hand: identities that name
        /// one cell each, and outputs only where a cell could have produced
        /// them. The first is what every operation in [`Notebook`] depends on;
        /// the second is the state `.ipynb` cannot represent and so must never
        /// arrive from one.
        #[test]
        fn importing_a_hostile_notebook_produces_one_a_caller_could_have_built(
            value in crate::testing::arb_ipynb_json(),
        ) {
            let Ok(import) = import_ipynb(&value) else { return Ok(()) };
            let notebook = import.notebook();

            let mut ids: Vec<_> = notebook.cells().iter().map(|c| c.id()).collect();
            let count = ids.len();
            ids.sort_unstable();
            ids.dedup();
            prop_assert_eq!(ids.len(), count, "two cells share an identity");

            for cell in notebook.cells() {
                prop_assert!(
                    cell.kind().is_executable() || cell.outputs().is_empty(),
                    "a {} cell came back holding outputs",
                    cell.kind()
                );
                // Every id is reachable, which is what makes it a handle.
                prop_assert!(notebook.cell(cell.id()).is_some());
            }
        }
    }
}
