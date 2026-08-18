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
| Save a Copy | **live** — writes the buffer elsewhere without moving the document's path, clearing its dirty flag, or touching Open Recent; all three are pinned by tests |
| New Window, Open Folder, Revert, Print | planned |

## Edit

| Row | State |
| --- | --- |
| Undo, Redo, Cut, Copy, Paste, Select All | **live** (both editor views) |
| Double-click a word, triple-click a line | **live** — native under `TextInput`, `bp_editor::{select_word_at, select_line_at}` under `--editor-view` |
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
| Zoom In / Zoom Out / Reset Zoom | **live** — Ctrl+= / Ctrl+- / Ctrl+0, bounded by `bp_config::{MIN,MAX}_FONT_SIZE`; the rows grey at the bound and the reset row names the size in force |
| Follow System | **live** — `ThemeId::for_system` resolves the desktop's preference to Light or Dark; a desktop that will not say leaves the theme alone and says so |
| Split / Preview, panels, inspectors | planned |

## Insert

| Row | State |
| --- | --- |
| Date, Time, Date and Time, ISO 8601, Filename date | **caret** — each row's hint is the stamp rendered from the clock, so the row shows what it will insert |
| Markdown constructs, citation, code block, table, notebook cell | planned |

## Format

| Row | State |
| --- | --- |
| LF / CRLF, UTF-8 / UTF-8 with BOM | **live** |
| Indent with Tabs / Spaces, Tab Width 2 / 4 / 8 | **live** — one width serves both the Tab key and how wide a tab is drawn; a soft tab goes to the next stop, not a fixed count |

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
| Document Statistics | **live** — `bp_semantic::statistics` into the status bar, one pass on a menu click and never on the typing path |
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

## Find bar

Not a menu. The bar itself is **live**, and so are its three options.

| Control | State |
| --- | --- |
| Find, Replace, Replace All, In Folder | **live** |
| Aa / Word / .* toggles | **live** — `bp_search::Query::{case_sensitive, whole_word, regex}`; flipping one re-runs the search |

## Go to Line

Not a menu of its own. Edit ▸ Go to Line and Ctrl+G open a bar beside the find
bar, built the same way and for the same reason: a modal dialog is a heavy
answer to "which line?". `Editor::go_to_line` reports whether the line existed,
so an out-of-range number moves the caret to the end *and* says "there are only
42 lines". **caret** — `TextInput`'s caret cannot be moved from Rust.

## Tab strip context menu

Right-click a tab. The rows act on the tab that was clicked, not the active
one — the target is recorded when the menu opens, because clicking a row is a
separate event, by which time the pointer has moved.

| Row | State |
| --- | --- |
| Close Tab, Close Other Tabs, Close All Tabs | **live** — each goes through the same unsaved-changes prompt as closing one |
| Copy Full Path | **live** — greyed for a document that has never been saved |

## Drag and drop to open

**Blocked on Slint, not on effort.** specs §4 wants files dropped onto the
window to open. `DropArea` exists and compiles, but on Slint 1.17.1 the winit
backend has no file-drop plumbing and `DataTransfer` carries only plain text
or an image — there is no channel a file path could arrive through. It works
for drags that start inside a Slint window and nowhere else.
