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
| YAML: validate, format, minify, to JSON | **live** — `saphyr`, per ADR-0023. Validate names how many documents the file holds, because Format writes all of them back and Convert to JSON refuses more than one. The Format and Minify rows carry "comments not kept" as their hint and say it again in the status bar: a tree round trip has nothing to put a comment back from and resolves aliases into copies, and ADR-0023 names a row that offers this silently as a trap. Three refusals are the feature and not a failure mode — nesting past 128 levels (a 200-byte paste would otherwise overflow the stack and abort the process), alias expansion past 1,000,000 nodes, and a duplicate mapping key, which is refused rather than merged because formatting such a file would delete a line and report success. Each names its limit and where it was hit, and says the document was left unchanged. Sort Keys is not offered: there is no ordering over YAML nodes yet |
| JSON: to YAML | **live** — the direction that cannot lose anything, so it is the one with no warning on the row |
| Filter, query, schema, export | planned |

## Note

| Row | State |
| --- | --- |
| Suggest Title, Semantic Rename, Summary, Keywords, Outline | **live** |
| Document Statistics | **live** — `bp_semantic::statistics` into the status bar, one pass on a menu click and never on the typing path |
| Tags, classification, related notes, properties, history | planned — phase 9 |

## Security

| Row | State |
| --- | --- |
| Standard / Private / Confidential / Maximum | **live** — the active document's profile; exactly one ticks, and a Custom policy ticks none. Private and Confidential seal the recovery journal with the document's passphrase, so they need the document encrypted |
| What the profile permits (recovery, clipboard, network) | **live** — a readout, greyed because it is not clickable. A profile is a promise about invisible behaviour, and a promise nobody can see is not one |
| Encrypt Document... | **live** — asks for a passphrase twice, writes a `.bpadx` beside the original, and the tab adopts it so later saves stay encrypted. Reads "Encrypted (.bpadx)" and greys once the document is |
| Privacy Mode | **live** — a session-wide override that can only tighten |
| Scan for Secrets | **live** — `bp_secrets::scan` over the active document. The status bar gives the count, the kinds and the first three positions; the full listing is a dialog, one line per finding. **Neither ever prints the matched text**: a `Finding` deliberately carries a position and a classification and nothing else, and the shell must not undo that by reaching back into the document to quote it |
| Hash Document (SHA-256) | **live** — `bp_crypto::hash_document` over the bytes the document *would be written as*, not over the buffer, so the digest matches `sha256sum` on a document with a BOM or CRLF endings. Shown grouped in fours (to read down a telephone) and unbroken (to paste), and says so when unsaved edits mean it is not yet the digest of anything on disk |
| Sign Document | **greyed, with the reason on the row** — `bp_crypto::sign_document` exists and is tested; there is nowhere to keep a signing key yet. Deliberately not a planned row: `planned` means "does not exist", and this exists and is missing a prerequisite |
| Verify Signature... | **live** — the user chooses a `.sig` and a public key (32 raw bytes, or hexadecimal as pasted out of an email), and `bp_crypto::verify_document` answers. A refusal is reported as the library words it, since the bytes cannot tell an altered document from the wrong key; the one sentence the shell adds is that unsaved edits mean these are not the bytes anybody signed. The shell owns no sidecar convention — naming and finding `.sig` files is `bp-integrity`'s |
| Redact Found Secrets... | **live** — `bp_secrets::scan` produces the spans and `bp_redaction::redact` destroys them. `Placeholder`, not `Mask`: `MatchOriginal` publishes the length of what was removed, which for a PIN or a short token is most of the secret. The marker is `[REDACTED: kind]`, labelled with the name of the rule that matched and never with what it matched. A confirmation dialog lists line and kind first and states plainly that this changes the document and not the file — the file on disk, the recovery journal, the undo history and the clipboard all still hold the originals, and saying "redacted" without saying that is the same lie the black rectangle tells. Applied as an ordinary undoable edit for the same reason. **A private key block is deliberately not redacted**: `bp-secrets` marks only its `-----BEGIN` line, so redacting the span would take out the label and leave the key body — the row says so rather than half-doing it. `bp_redaction::verify` runs afterwards and a survivor is reported by line number, never by text. Greyed on an empty document, with the reason on the row |
| Inspect Metadata | **live** — `bp_redaction::metadata::inspect` over the text, reporting kind, exposure and line and never the value. For a container this build cannot open — `.docx`, `.pdf`, `.rtf`, an image — `Container::hidden()` and `requires()` are reported instead of silence: "no metadata found" about a `.docx` reads as an all-clear and would be a lie. For plain text it says the opposite thing it is easy to leave out — that the filesystem entry around the file, its timestamps, ownership and alternate data streams, is not part of the check |
| Lock Document, audit history | planned — phase 16 |

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

## Security (the rest)

Lock, decrypt in place, secure clipboard, audit, settings. Planned — phases 15
and 16. Redaction and the metadata inspector are no longer among them; both are
rows above.

Hash, sign and verify are no longer among them: `bp-crypto`'s signing half is
wired into the rows above. **Signing is the one that is not**, and the reason
is worth writing down rather than rediscovering — an Ed25519 signing key has
to live somewhere, and this product has no key store, no platform-keyring
integration and no decision on file permissions for a key on disk. That is a
decision with consequences past one file, so the row greys and says why
instead of a key appearing in the config directory because a menu row needed
one.

The three things that were waiting on profiles now read them: the recovery
journal refuses rather than writing plaintext under a profile that forbids it,
clipboard history is cleared and stops recording, and `bp-storage`'s
`record_document` honours `Metadata`. `bp-storage` is still not called by the
application — that is ADR-0019's product decision, not a security one any
more.

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
