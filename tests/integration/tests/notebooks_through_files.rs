//! Seam: `bp-notebook` through `bp-files`.
//!
//! `bp-notebook` is pure -- "no clock, no filesystem, no configuration", its
//! own words -- and every one of its tests hands it a `&str` or a
//! `serde_json::Value` it built in memory. `bp-files` writes and reads text
//! and has never heard of a notebook. So the thing a user actually does --
//! save a notebook, close it, open it again -- has never been executed.
//!
//! Three properties, and they are not the same property:
//!
//! 1. **The round trip through a real file.** ADR-0025 promises that
//!    everything not on its "cannot carry" list survives export and import.
//!    Here the notebook goes out through `atomic_write` and comes back
//!    through `bp_files::load` -- the editor's own load path, which detects
//!    encoding and line endings and is where a notebook could quietly acquire
//!    a BOM or lose a `\r`.
//! 2. **What ADR-0025 says cannot survive genuinely does not** -- silently,
//!    with a warning, as documented. A format that loses something without
//!    saying so is the failure mode of every interchange format, and the
//!    warning is the whole mitigation.
//! 3. **The security rule holds across the file boundary.** specs.md
//!    section 15: never auto-run an opened or pasted notebook. `bp-notebook`
//!    enforces it in the type system, and a type-system guarantee is only
//!    worth what the call sites make of it -- so this asserts it against a
//!    file on disk that tries every trick a hostile `.ipynb` has: an
//!    `execution_count`, a kernelspec, and metadata keys invented to look
//!    like an autorun switch.
//!
//! Nothing here prints document content; the fixtures are fixed strings and
//! none is a secret.

use bp_files::{SaveOptions, atomic_write, load};
use bp_notebook::{
    Cell, CellKind, ImportWarning, Notebook, Output, OutputDropped, RunRequest, Stream,
    UserGesture, export_ipynb, import_ipynb, parse_raw_json_view, raw_json_view,
};
use serde_json::{Value, json};
use tempfile::{TempDir, tempdir};

/// Write `text` as a `.ipynb` and read it back the way the editor does.
///
/// `bp_files::load` rather than `fs::read_to_string`, deliberately: that is
/// the path a user's file actually takes, it sniffs encoding and line
/// endings, and a notebook that survived `fs::read` but not `load` would be a
/// notebook the product cannot open.
fn through_a_file(text: &str) -> (TempDir, String) {
    let dir = tempdir().expect("temp dir");
    let path = dir.path().join("analysis.ipynb");
    atomic_write(&path, text.as_bytes(), SaveOptions::default()).expect("atomic_write");

    let file = load(&path).expect("load");
    assert_eq!(
        file.bytes_on_disk as usize,
        text.len(),
        "the file on disk is not the notebook that was written"
    );
    (dir, file.text)
}

/// A notebook using every one of the eight kinds specs.md names, with an
/// output on a cell that is allowed to hold one.
fn every_kind() -> Notebook {
    let mut notebook = Notebook::new();
    for kind in CellKind::ALL {
        let id = notebook.push(kind, format!("source for {}\n", kind.as_str()));
        if kind.is_executable() {
            notebook
                .cell_mut(id)
                .expect("just pushed")
                .set_outputs(vec![
                    Output::Text {
                        stream: Stream::Stdout,
                        text: "ok\n".to_owned(),
                    },
                    Output::Table {
                        columns: vec!["region".to_owned()],
                        rows: vec![vec!["north".to_owned()]],
                    },
                ])
                .expect("outputs on an executable cell");
        }
    }
    // One collapsed cell, because `collapsed` is written to the file only
    // when true and is therefore the field most easily lost.
    let first = notebook.cells()[0].id();
    notebook.set_collapsed(first, true).expect("collapse");
    notebook
}

// --- the round trip ------------------------------------------------------

#[test]
fn a_notebook_survives_a_round_trip_through_a_real_file() {
    let notebook = every_kind();
    let (_dir, text) = through_a_file(&raw_json_view(&notebook));

    let import = parse_raw_json_view(&text).expect("import");
    assert!(
        import.is_lossless(),
        "a notebook of our own lost something on the way back: {:?}",
        import.warnings()
    );
    assert_eq!(
        import.notebook(),
        &notebook,
        "the notebook that came back off the disk is not the one that went on"
    );
}

#[test]
fn every_one_of_the_eight_kinds_survives_the_file() {
    // The point of ADR-0025's namespaced metadata. Jupyter has three cell
    // types and we have eight, so five of these kinds exist on disk *only* as
    // `metadata.bachelorpad.kind` -- and a round trip that dropped that key
    // would come back as a plausible-looking notebook of Markdown and Raw.
    let notebook = every_kind();
    let (_dir, text) = through_a_file(&raw_json_view(&notebook));
    let reopened = parse_raw_json_view(&text).expect("import").into_notebook();

    let before: Vec<CellKind> = notebook.cells().iter().map(Cell::kind).collect();
    let after: Vec<CellKind> = reopened.cells().iter().map(Cell::kind).collect();
    assert_eq!(after, before, "a cell kind did not survive the file");
    assert_eq!(after.len(), CellKind::ALL.len());

    // And the collapse flag, which is only written when true.
    assert!(
        reopened.cells()[0].collapsed(),
        "a collapsed cell came back expanded"
    );
}

#[test]
fn the_file_on_disk_is_a_notebook_jupyter_would_recognise() {
    // Interchange is the whole point of the format, so this asserts the shape
    // of the bytes rather than only that we can read them back. A file only
    // we can open is not interchange.
    let (_dir, text) = through_a_file(&raw_json_view(&every_kind()));
    let value: Value = serde_json::from_str(&text).expect("the file is JSON");

    assert_eq!(value["nbformat"], json!(4));
    let cells = value["cells"].as_array().expect("a cells array");
    assert_eq!(cells.len(), CellKind::ALL.len());
    for cell in cells {
        let cell_type = cell["cell_type"].as_str().expect("a cell_type");
        assert!(
            matches!(cell_type, "code" | "markdown" | "raw"),
            "{cell_type:?} is not one of Jupyter's three cell types"
        );
    }
}

// --- what the format cannot carry ----------------------------------------

/// A notebook as another tool would write it, with everything ADR-0025 says
/// it cannot carry.
fn a_foreign_notebook() -> Value {
    json!({
        "nbformat": 4,
        "nbformat_minor": 5,
        "metadata": {
            "kernelspec": { "name": "python3", "language": "python" },
            // A key of ours in a file we did not write. Import removes it.
            "bachelorpad": { "notebook_format": 99, "auto_run": true }
        },
        "cells": [
            {
                "cell_type": "code",
                "source": "print('hello')\n",
                // Execution state. Discarded, always, with a warning.
                "execution_count": 7,
                "metadata": { "run_on_open": true, "autorun": true },
                "outputs": [
                    { "output_type": "stream", "name": "stdout", "text": "hello\n" },
                    // An output type we do not model.
                    { "output_type": "unheard_of", "data": {} }
                ]
            },
            {
                "cell_type": "markdown",
                "source": "# Heading\n",
                // Inline attachments. We have no model for them.
                "attachments": { "image.png": { "image/png": "AAAA" } },
                "metadata": {}
            },
            {
                // Outputs on a cell that cannot run: illegal in nbformat and
                // unrepresentable here, so it can only arrive from a file.
                "cell_type": "raw",
                "source": "verbatim\n",
                "metadata": {},
                "outputs": [
                    { "output_type": "stream", "name": "stdout", "text": "impossible\n" }
                ]
            },
            {
                // Code in a language we cannot name.
                "cell_type": "code",
                "source": "SELECT 1 FROM DUAL CONNECT BY LEVEL < 2;\n",
                "metadata": { "vscode": { "languageId": "brainfuck" } },
                "outputs": []
            }
        ]
    })
}

#[test]
fn what_the_format_cannot_carry_is_lost_with_a_warning_and_not_silently() {
    let text = format!(
        "{}\n",
        serde_json::to_string_pretty(&a_foreign_notebook()).expect("json")
    );
    let (_dir, text) = through_a_file(&text);

    let import = parse_raw_json_view(&text).expect("import");
    let warnings = import.warnings();
    assert!(
        !import.is_lossless(),
        "a file full of unrepresentable things imported clean"
    );

    // Each loss ADR-0025 names, checked individually: a single "something was
    // lost" would let three of the four go unreported.
    assert!(
        warnings
            .iter()
            .any(|w| matches!(w, ImportWarning::DiscardedExecutionState { cell_index: 0 })),
        "the execution count was dropped without saying so: {warnings:?}"
    );
    assert!(
        warnings.iter().any(|w| matches!(
            w,
            ImportWarning::DroppedOutput {
                cell_index: 0,
                reason: OutputDropped::UnknownOutputType(_),
                ..
            }
        )),
        "an unmodelled output was dropped without saying so: {warnings:?}"
    );
    assert!(
        warnings
            .iter()
            .any(|w| matches!(w, ImportWarning::DroppedAttachments { cell_index: 1 })),
        "Markdown attachments were dropped without saying so: {warnings:?}"
    );
    assert!(
        warnings.iter().any(|w| matches!(
            w,
            ImportWarning::DroppedOutput {
                cell_index: 2,
                reason: OutputDropped::NotACodeCell,
                ..
            }
        )),
        "an output on a non-code cell was dropped without saying so: {warnings:?}"
    );
    // Cell 3 -- code explicitly tagged by another tool as a language we
    // cannot name -- is deliberately *not* asserted here. It produces no
    // warning at all, which is the defect pinned by
    // `an_unnameable_language_is_silently_replaced_by_the_notebooks_kernel`
    // at the bottom of this file.

    let notebook = import.into_notebook();

    // The losses are real losses, not warnings about things that survived.
    assert!(
        notebook.cells()[2].outputs().is_empty(),
        "a raw cell came back holding outputs, which nothing in this crate can build"
    );
    assert_eq!(
        notebook.cells()[0].outputs().len(),
        1,
        "the unmodelled output survived after being reported as dropped"
    );

    // Our own metadata key is ours: import removes it whatever it held.
    assert!(
        !notebook.metadata().contains_key("bachelorpad"),
        "a foreign file's `bachelorpad` key was trusted rather than removed"
    );
    // A kernelspec we did not write is preserved verbatim.
    assert_eq!(notebook.metadata()["kernelspec"]["name"], json!("python3"));
}

#[test]
fn a_stripped_metadata_key_costs_the_kinds_and_nothing_else() {
    // ADR-0025's most honest admission: a tool that discards unknown cell
    // metadata takes our eight kinds with it, and nothing can be done from
    // this side. Pinned so the loss stays the *documented* one -- the source
    // text must still come back intact, because losing that as well would
    // turn an interchange annoyance into data loss.
    let notebook = every_kind();
    let mut value = export_ipynb(&notebook);
    for cell in value["cells"].as_array_mut().expect("cells") {
        cell["metadata"]
            .as_object_mut()
            .expect("cell metadata")
            .remove("bachelorpad");
    }
    let (_dir, text) = through_a_file(&format!(
        "{}\n",
        serde_json::to_string_pretty(&value).expect("json")
    ));

    let reopened = parse_raw_json_view(&text).expect("import").into_notebook();

    let sources_before: Vec<&str> = notebook.cells().iter().map(Cell::source).collect();
    let sources_after: Vec<&str> = reopened.cells().iter().map(Cell::source).collect();
    assert_eq!(
        sources_after, sources_before,
        "stripping our metadata cost the user their text, not just the kinds"
    );

    let kinds: Vec<CellKind> = reopened.cells().iter().map(Cell::kind).collect();
    assert_ne!(
        kinds,
        CellKind::ALL.to_vec(),
        "this fixture no longer strips anything, so it is testing nothing"
    );
}

// --- the security rule, across the file boundary -------------------------

#[test]
fn importing_a_notebook_from_disk_produces_nothing_runnable_without_a_gesture() {
    // The rule specs.md section 15 states, checked where it actually has to
    // hold: on bytes that came off a disk.
    //
    // Two of `bp-notebook`'s three defences are compile-time and cannot be
    // asserted here -- `import_ipynb` returns an `Import` rather than a
    // `Notebook`, and every `request_run*` demands a `UserGesture` that is
    // not `Deserialize`, not `Default` and not `Clone`, so an import path
    // that started a run would not compile. What *can* be asserted, and is
    // asserted here, is the third: that no amount of contrivance in the file
    // produces work, and that when a gesture is finally supplied the work is
    // exactly the executable cells and nothing more.
    let text = format!(
        "{}\n",
        serde_json::to_string_pretty(&a_foreign_notebook()).expect("json")
    );
    let (_dir, text) = through_a_file(&text);

    let import = parse_raw_json_view(&text).expect("import");

    // The file said "this has been run": that state is gone.
    let notebook = import.into_notebook();
    for (index, cell) in notebook.cells().iter().enumerate() {
        assert!(
            !cell.metadata().contains_key("execution_count"),
            "cell {index} came back carrying execution state from the file"
        );
    }

    // The file also asked, in two invented spellings, to be run on open.
    // Those keys *do* survive, and correctly so: the nbformat schema requires
    // unknown cell metadata to round-trip, and `Cell` has no field that reads
    // them -- they are inert data. What matters is not their absence but
    // their inertness, which is what the comparison below establishes: the
    // same notebook with those keys stripped produces exactly the same work.
    let inert = {
        let mut value = a_foreign_notebook();
        for cell in value["cells"].as_array_mut().expect("cells") {
            if let Some(map) = cell["metadata"].as_object_mut() {
                map.remove("run_on_open");
                map.remove("autorun");
            }
        }
        import_ipynb(&value).expect("import").into_notebook()
    };
    let work_with = notebook.request_run_all(UserGesture::from_user_command());
    let work_without = inert.request_run_all(UserGesture::from_user_command());
    assert_eq!(
        work_with.len(),
        work_without.len(),
        "an invented metadata key changed how much work the notebook produced"
    );

    // A gesture is the only route to work, and the work it produces is
    // exactly the executable cells -- no more, and in particular not the
    // Markdown or Raw ones.
    assert!(
        !work_with.is_empty(),
        "the fixture produced no work at all, so this asserts nothing"
    );
    let executable: Vec<_> = notebook
        .cells()
        .iter()
        .filter(|c| c.kind().is_executable())
        .map(Cell::id)
        .collect();
    assert_eq!(
        work_with.iter().map(RunRequest::cell).collect::<Vec<_>>(),
        executable,
        "the runnable set is not the set of executable cells"
    );
    assert!(!CellKind::Raw.is_executable());
}

#[test]
fn an_unnameable_language_is_never_replaced_by_the_notebooks_kernel() {
    // The control first: with no kernelspec, the documented behaviour holds
    // exactly. This is what makes the failure below a disagreement about the
    // kernelspec fallback rather than a claim that the warning never fires.
    let no_kernel = json!({
        "nbformat": 4,
        "nbformat_minor": 5,
        "metadata": {},
        "cells": [{
            "cell_type": "code",
            "source": "++++++++[>++++++++<-]>.\n",
            "metadata": { "vscode": { "languageId": "brainfuck" } },
            "outputs": []
        }]
    });
    let import = import_ipynb(&no_kernel).expect("import");
    assert!(
        import
            .warnings()
            .iter()
            .any(|w| matches!(w, ImportWarning::UnknownCodeLanguage { cell_index: 0 })),
        "the control case no longer reproduces the documented behaviour"
    );
    let control = import.into_notebook();
    assert_eq!(control.cells()[0].kind(), CellKind::Raw);
    assert!(
        control
            .request_run_all(UserGesture::from_user_command())
            .is_empty()
    );

    // The same cell, in a notebook that names a kernel -- which is what every
    // notebook Jupyter ever wrote does.
    let with_kernel = json!({
        "nbformat": 4,
        "nbformat_minor": 5,
        "metadata": { "kernelspec": { "name": "python3", "language": "python" } },
        "cells": [{
            "cell_type": "code",
            "source": "++++++++[>++++++++<-]>.\n",
            "metadata": { "vscode": { "languageId": "brainfuck" } },
            "outputs": []
        }]
    });
    let import = import_ipynb(&with_kernel).expect("import");
    assert!(
        import
            .warnings()
            .iter()
            .any(|w| matches!(w, ImportWarning::UnknownCodeLanguage { cell_index: 0 })),
        "a cell whose stated language could not be named was reclassified with no \
         warning at all: {:?}",
        import.warnings()
    );

    let notebook = import.into_notebook();
    assert_eq!(
        notebook.cells()[0].kind(),
        CellKind::Raw,
        "code in a language we cannot name became {:?} and is therefore runnable",
        notebook.cells()[0].kind()
    );
    assert!(
        notebook
            .request_run_all(UserGesture::from_user_command())
            .is_empty(),
        "a cell another tool tagged as an unnameable language was offered to a runner"
    );
}

#[test]
fn a_notebook_claiming_a_kernel_does_not_make_its_cells_runnable() {
    // The subtler version: a file whose kernelspec says Python but whose
    // cells are markdown and raw. A reader that took the kernel's word for it
    // would offer to run prose.
    let value = json!({
        "nbformat": 4,
        "nbformat_minor": 5,
        "metadata": { "kernelspec": { "name": "python3", "language": "python" } },
        "cells": [
            { "cell_type": "markdown", "source": "# not code\n", "metadata": {} },
            { "cell_type": "raw", "source": "also not code\n", "metadata": {} }
        ]
    });
    let (_dir, text) = through_a_file(&format!(
        "{}\n",
        serde_json::to_string_pretty(&value).expect("json")
    ));

    let notebook = parse_raw_json_view(&text).expect("import").into_notebook();
    assert!(
        notebook
            .request_run_all(UserGesture::from_user_command())
            .is_empty(),
        "prose was offered to a runner because the file named a kernel"
    );
}

#[test]
fn a_file_that_is_not_a_notebook_is_refused_rather_than_guessed_at() {
    for (name, body) in [
        ("not-json.ipynb", "this is prose, not a notebook\n"),
        ("an-array.ipynb", "[1, 2, 3]\n"),
        ("no-version.ipynb", "{\"cells\": []}\n"),
        ("version-3.ipynb", "{\"nbformat\": 3, \"cells\": []}\n"),
        ("no-cells.ipynb", "{\"nbformat\": 4}\n"),
    ] {
        let dir = tempdir().expect("temp dir");
        let path = dir.path().join(name);
        atomic_write(&path, body.as_bytes(), SaveOptions::default()).expect("write");
        let text = load(&path).expect("load").text;

        assert!(
            parse_raw_json_view(&text).is_err(),
            "{name} was accepted as a notebook"
        );
    }
}

#[test]
fn a_notebook_that_arrived_with_crlf_endings_still_imports() {
    // A `.ipynb` that has been through a Windows tool, or a mail client, or
    // git with `core.autocrlf`. JSON does not care, `bp_files::load` reports
    // the ending it found, and the notebook has to survive both.
    let notebook = every_kind();
    let json_text = raw_json_view(&notebook).replace('\n', "\r\n");

    let dir = tempdir().expect("temp dir");
    let path = dir.path().join("analysis.ipynb");
    atomic_write(&path, json_text.as_bytes(), SaveOptions::default()).expect("write");

    let file = load(&path).expect("load");
    let import =
        import_ipynb(&serde_json::from_str::<Value>(&file.text).expect("json")).expect("import");
    assert!(
        import.is_lossless(),
        "a CRLF notebook lost something: {:?}",
        import.warnings()
    );
    assert_eq!(import.notebook(), &notebook);
}
