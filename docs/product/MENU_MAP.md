# Menu Map

The intended contents of each menu, and how much of each is real.

The application shows planned rows greyed rather than hiding them, so the menu
bar already doubles as a roadmap — a greyed row reads as "not yet", while a
label that swallows a click reads as broken. This file is the same information
in one place, for deciding what to build next.

**Live** means reachable and doing something. **Caret** means live only under
`--editor-view`, because it needs the caret Slint's `TextInput` does not
expose. **Built** means the library work exists and is tested but has no row
yet — those are the cheapest items in `project/WORK_QUEUE.md`.

## File

| Row | State |
| --- | --- |
| New, Open, Open Recent, Save, Save As, Save All, Reload, Close Tab | **live** |
| Save Copy | planned — `bp_files::atomic_write` is enough; the risk is leaving the document's path and dirty flag alone |
| New Window, Open Folder, Revert, Print | planned |

## Edit

| Row | State |
| --- | --- |
| Undo, Redo, Cut, Copy, Paste, Select All | **live** (both editor views) |
| Clipboard History | **live** |
| Paste transformations | **live** — offered per entry kind, and only where the result would differ |
| Sort / Deduplicate / Reverse / Trim lines | **live** |
| Duplicate Line, Move Line Up / Down | **caret** |
| Multi-cursor, column selection | planned — phase 2, needs the caret |

## View

| Row | State |
| --- | --- |
| Light / Dark / Organic / Green | **live** |
| Line Numbers, Word Wrap | **live** |
| Zoom | **built** — `Config::font_size`, no row yet |
| Follow System theme | planned — specs §16 |
| Split / Preview, panels, inspectors | planned |

## Insert

| Row | State |
| --- | --- |
| Date, Time, Date and Time, ISO 8601, Filename date | **built** — `bp_naming::Stamp::all()` gives the rows and their labels |
| Markdown constructs, citation, code block, table, notebook cell | planned |

## Format

| Row | State |
| --- | --- |
| LF / CRLF, UTF-8 / UTF-8 with BOM | **live** |
| Indentation (tab width, spaces or tabs) | **built** — `Config::{tab_width, indent_spaces}`, no row yet |

## Data

Context-sensitive: the menu is built from the detected format, so a note gets
no data rows rather than a column of greyed ones.

| Row | State |
| --- | --- |
| JSON: validate, format, minify, sort keys, to JSON Lines | **live** |
| JSON Lines: validate, to JSON | **live** |
| TOML: validate, format | **live** |
| CSV/TSV: shape report, to JSON, to JSON Lines, column types | **live** |
| YAML | blocked — `serde_yaml` is deprecated and the replacement wants a decision |
| Filter, query, schema, export | planned |

## Note

| Row | State |
| --- | --- |
| Suggest Title, Semantic Rename, Summary, Keywords, Outline | **live** |
| Document statistics | **built** — `bp_semantic::statistics`, no row yet |
| Tags, classification, related notes, properties, history | planned — phase 9 |

## Notebook

Enable mode, new cell, run, run selection, run all, convert selection,
split/merge cells, export. All planned — phase 12. Notebook content never
auto-runs (ADR-0011).

## Organize

Project, suggested folder, tags, topics, entities, related notes, smart
collections, duplicate detection, semantic search. All planned — phase 9.
`bp-storage` is the foundation and is deliberately unwired; see ADR-0019
before adding anything here.

## Research

Citation metadata, DOI and scholarly metadata, research question, evidence,
findings, methods, datasets. All planned — phase 13.

## Run

Run selection/cell/document, choose interpreter, stop, history, Rust
scratchpad. All planned — phase 12.

## Security

Lock, encrypt/decrypt, passphrase protection, secure clipboard, integrity,
hash, sign, verify, redact, scan secrets, privacy mode, metadata inspector,
audit, settings. All planned — phases 14 to 16.

Three shipped things are waiting on the profiles this menu configures: the
recovery journal, clipboard history, and `bp-storage`.

## Tools

| Row | State |
| --- | --- |
| Document inspector | planned — `bp_semantic::statistics` is most of it |
| Security inspector, file analysis, conversions, benchmarks, settings | planned |

## Help

| Row | State |
| --- | --- |
| Keyboard Shortcuts, About | **live** |
| Help, diagnostics | planned |

## Go to Line

Not a menu of its own — it belongs in Edit or a find-bar-style input.
`Editor::go_to_line` is **built** and reports whether the line existed, so the
caller can move somewhere sensible *and* say "there are only 42 lines".
