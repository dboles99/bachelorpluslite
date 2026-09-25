# Menu Map

The intended contents of each menu, and how much of each is real.

**There are no planned rows left.** Every row in every menu either does
something or is a *readout* — a greyed line that answers a question rather than
inviting a click ([ADR-0048](../decisions/ADR-0048.md)). This file used to
double as a roadmap, listing what each menu would one day hold; it records what
each menu *does* now.

That change was not mostly building. An inventory taken before any of it found
that of 38 rows marked planned, most had already shipped under another name and
nobody had pruned the list — the Security menu's seven were six already-live
features, and the Data menu managed to say "nothing for TXT documents" and then
list six things underneath. Ten rows were built; eleven were deleted, each for
a reason ADR-0048 records.

**Live** means reachable and doing something. **Caret** means live only under
`--editor-view`, because it needs to know *where the caret is*, and Slint's
`TextInput` will not say.

**Needing the caret *moved* is a different thing, and this file used to
conflate them.** `set-selection-offsets` moves `TextInput`'s caret perfectly
well — Find Next has always jumped that way — so a feature that only has to
put the caret somewhere is not a Caret feature at all. Go to Line was marked
as one until 2026-08-22 and was not. **Built** means the library work exists and is tested but has no row
yet — those are the cheapest items in `project/WORK_QUEUE.md`.

Two kinds of greyed row remain, and telling them apart matters:

- a row that **exists and cannot act right now**, with the reason in its own
  label — Save for a document served from disk, Sign for one with unsaved
  changes, Lock Document for one holding no passphrase. "You can fix this";
- a **readout**, which is not a row to click at all — the Security menu's
  policy lines, "nothing to convert in a plain-text document".

Nothing means "does not exist yet" any more, and `menus::planned_menu` and
`arrives()` were deleted rather than emptied so it cannot start to again.

**Four whole menus have left, and this file no longer has a section for any
of them; a fifth was renamed.** [ADR-0064](../decisions/ADR-0064.md) removed
nine of the Security menu's thirteen rows and renamed what was left
**Privacy**, because that is what four profiles and a toggle are. [ADR-0062](../decisions/ADR-0062.md) removed **Data** -- eight
rows over JSON, JSON Lines, TOML, YAML and delimited text, all of them live.
And [ADR-0057](../decisions/ADR-0057.md) removed Notebook and Run --
the cell outline, the `.ipynb` export, one row per runnable cell, Stop, Run
Document and Interpreters. Every one of them was live when it was deleted,
which is the point: these were decisions about what the product is, not a
sweep of rows that never worked. **The menu bar is eleven menus now** -- the
eleven this file has a section for, which is a sentence that had said *ten*
while its own headings, `menus::every_menu` and the window all said otherwise.
`the_menu_bar_and_the_list_of_menus_name_the_same_menus` reads `app.slint` and
asks, so that a menu cannot be added or removed without this line failing.

## File

| Row | State |
| --- | --- |
| New, Open, Open Recent, Save All, Close Tab | **live**. Every document is loaded whole into the rope. It was classified by size first and served from disk past 192 MiB (ADR-0027, ADR-0030) until [ADR-0063](../decisions/ADR-0063.md); a large file now loads or fails trying, which is what Notepad does |
| Save, Save As, Reload | **live**, and greyed with the reason for a document served from disk. Not politeness: such a document has no text in a rope, so a save that merely did nothing *special* would encode the empty string and atomically write it over two gigabytes, reporting success. `AppState` refuses all three by name and a test asserts the file's size is unchanged rather than trusting a return value |
| Save a Copy | **live** — writes the buffer elsewhere without moving the document's path, clearing its dirty flag, or touching Open Recent; all three are pinned by tests |
| Set as Default Editor... | **live** — `bp_platform::editor`, under ADR-0012. The row's hint names the preset it registers for (Notepad Replacement), because the preset is a real choice and one made out of sight is one nobody made. **Every extension any preset offers is one this build can open** ([ADR-0069](../decisions/ADR-0069.md) removed `.bpadx`, which every preset had claimed, and removed the test exemption that let it). A click **reports before it offers**: the dialog opens with what opens each of those types today, from `association_report`. On Windows that reads "cannot tell what opens .txt files", never "nothing is set to" — `AssociationState::Unknown` is not `Unclaimed`, and rendering the second from the first invites the user to fix something that may not be broken. Then it shows the whole plan — the exact files, the root they go under, the commands that still have to run — and only an explicit OK becomes `Consent::Granted`. On Linux `install` writes the `.desktop` entry and the MIME package under the root `install_root` names, and the shell then **shows the `xdg-mime default` command rather than pretending the job is done**; nothing here runs it, because running it is deciding for the user, which is the thing ADR-0012 draws its line around. On Windows `install` refuses, in the capability register's own words, so a status bar line and a refusal cannot drift apart; what the user gets instead is the `.reg` script, saved where *they* chose, plus `ms-settings:defaultapps` to paste into Run. **No key belonging to another application is ever written, and Notepad is never named**: `bp-platform` refuses to build such a plan and the shell re-checks with `registry_objections` before a `.reg` reaches a Save dialog, because that path does not go through `install` at all |
| New Window | **live** |
| Open Folder, Revert, Print | Not rows, and not planned. Revert is Reload; Open Folder needs a project concept this product does not have (ADR-0048); Print is platform work with no cross-platform story yet |

## Edit

| Row | State |
| --- | --- |
| Undo, Redo, Cut, Copy, Paste, Select All | **live** (both editor views) |
| Double-click a word, triple-click a line | **live** — native under `TextInput`, `bp_editor::{select_word_at, select_line_at}` under `--editor-view` |
| Clipboard History, Paste transformations | **Removed** ([ADR-0061](../decisions/ADR-0061.md)). Both were live. Cut, Copy and Paste above are the OS clipboard and are untouched — the history shared a word with them and nothing else |
| Sort / Deduplicate / Reverse / Trim lines | **live** |
| Duplicate Line, Move Line Up / Down | **caret** |
| Go to Line | **live** in both surfaces — it needs the caret *moved*, never read |
| Multi-cursor, column selection | **Not a row** ([ADR-0048](../decisions/ADR-0048.md)). `TextInput` has one caret and cannot draw a second, so it could only work under `--editor-view` — and `bp-editor` would have to carry a set of carets through every command, selection and undo entry. A real feature, and it comes back as a queue item with a design behind it rather than as a row that has sat in a menu since the scaffold commit |

## View

| Row | State |
| --- | --- |
| Light / Dark / Organic / Green | **live** |
| Line Numbers, Word Wrap | **live** |
| Zoom In / Zoom Out / Reset Zoom | **live** — Ctrl+= / Ctrl+- / Ctrl+0, bounded by `bp_config::{MIN,MAX}_FONT_SIZE`; the rows grey at the bound and the reset row names the size in force |
| Follow System | **live** — `ThemeId::for_system` resolves the desktop's preference to Light or Dark; a desktop that will not say leaves the theme alone and says so |
| Split / Preview | **Not a row, and D14 is open again** ([ADR-0081](../decisions/ADR-0081.md)). [ADR-0048](../decisions/ADR-0048.md) answered it on the premise that Slint 1.17.1 had no rich-text item; it had one, `StyledText`, public since Slint 1.15, which renders emphasis, links, lists and inline code but not headings, tables or images. Handing HTML to the system browser is still ruled out -- this product launches no programs (ADR-0075). Whether a product reduced to Notepad wants a preview at all is Daniel's question. *Split* -- two panes over one document -- is a separate feature and was never the hard half |

## Insert

| Row | State |
| --- | --- |
| Date, Time, Date and Time, ISO 8601, Filename date | **caret** — each row's hint is the stamp rendered from the clock, so the row shows what it will insert |
| Bold, Italic, Link, Code Block, Table | **caret** — the Markdown constructs, all live |
| Citation | Not a row, and now not possible either. `bp-research` modelled citations and left under [ADR-0060](../decisions/ADR-0060.md); there is nothing to insert *from* |

## Format

| Row | State |
| --- | --- |
| LF / CRLF, UTF-8 / UTF-8 with BOM | **live** |
| Indent with Tabs / Spaces, Tab Width 2 / 4 / 8 | **live** — one width serves both the Tab key and how wide a tab is drawn; a soft tab goes to the next stop, not a fixed count |

## Note

| Row | State |
| --- | --- |
| Suggest Title, Semantic Rename, Summary, Keywords, Outline | **live** |
| Document Statistics | **live** — `bp_semantic::statistics` into the status bar, one pass on a menu click and never on the typing path |
| Tags | **live** — what the store recorded for this document, and the sentence that says tags are extracted from its keywords at save rather than set by hand. The sibling of Keywords: that reads the text in front of you, this reads what was recorded, and they disagree exactly when there are unsaved edits |
| Recovery Checkpoints | **live** — what the crash-recovery journal holds for this document, and *before the count*, whether this profile writes one at all. **Not "Revision History"**, which this file named until 2026-08-22: `bp-history` holds the pending checkpoint and discards it the moment a save succeeds, so the name promised successive versions to go back to ([ADR-0048](../decisions/ADR-0048.md)) |
| Related Notes | **live, in Organize.** Two rows in two menus running the same query is how one of them goes stale |
| Classification, properties | Not rows. Both need a schema for what a note *is*, which nothing in this product has decided |

## Privacy

**It was the Security menu, and it had thirteen rows**
([ADR-0064](../decisions/ADR-0064.md)). What is left is what *governs* rather
than what performs: the profile a document carries, the session override, and
a readout of what the two of them decide. Renamed because "Security" over four
profiles and a toggle is a label that reads as a promise.

| Row | State |
| --- | --- |
| Standard / Private / Confidential / Maximum | **live** — the active document's profile; exactly one ticks, and a Custom policy ticks none |
| Recovery journal: … | **live** — a readout, greyed because it is not clickable. Standard and Private journal unsaved work in plaintext and this row says *"on, unencrypted"*; Confidential and Maximum journal nothing. **Not "encrypted"** — ADR-0064 deleted that variant rather than pointing it at plaintext. **This row is load-bearing** ([ADR-0065](../decisions/ADR-0065.md)): Private keeps a journal *because* the row names its form, and a test says that if the row stops doing so, Private goes back to no journal |
| Recorded: … | **live** — a readout: what the metadata store may keep about this document. Path, title and tags under Standard; the path only under Private; nothing under Confidential and Maximum |
| Privacy Mode | **live** — a session-wide override that can only tighten, and it *acts*: journals already written are removed |
| Encrypt Document, Scan for Secrets, Redact, Inspect Metadata, Hash, Sign, Verify, Security History, Lock Document | **Removed** ([ADR-0064](../decisions/ADR-0064.md)). All nine were live. `.bpadx` documents already on a disk cannot be opened by this build, and there is no migration — the ADR says why |

**The policy has two axes, and those are the two with a row here.** There were
six. `embeddings`, `network`, `temporary_files` and `zeroise` were reported by
Tools ▸ Security Inspector as the policy in force and consulted by nothing --
so a Confidential document was shown *"Temporary files: never written"* while
every save wrote one. They left under [ADR-0082](../decisions/ADR-0082.md),
which carried out what [ADR-0059](../decisions/ADR-0059.md) had decided.

## Organize

`bp-storage` is the foundation ([ADR-0019](../decisions/ADR-0019.md)) and is
wired ([ADR-0037](../decisions/ADR-0037.md)).

| Row | State |
| --- | --- |
| Related Notes | **live** — a collapsible panel of documents sharing tags with the active one ([ADR-0037](../decisions/ADR-0037.md)) |
| Duplicate Detection | **live** — automatic at save and on demand |
| Suggested Folder | **live** — where documents sharing this one's tags already live, counted from what the user has filed rather than a scheme imposed on them. **It never moves a file**; File ▸ Save a Copy is where that already lives |
| Project | Not a row. Nothing in this product has a concept of a project, and inventing one to fill a menu row is how a feature nobody asked for gets built ([ADR-0048](../decisions/ADR-0048.md)) |
| Topics | Not a row — already in two places: Note ▸ Tags for the document, Research ▸ Research Report's dominant themes for the store |
| Semantic search | **Declined** with phase 10 ([ADR-0082](../decisions/ADR-0082.md)). Find ▸ In Folder searches the text of every note and Related Notes answers *what else is about this*; ranking by meaning is the part that would need a model, and a model is what this product declined |
| Entities, smart collections | Not rows. Both need a data model nothing has decided |

## Research

| Row | State |
| --- | --- |
| Research Report | **live** — dominant themes, stale clusters, under-connected documents, consolidation candidates, each naming the documents behind it, and a closing section stating every threshold it applied ([ADR-0041](../decisions/ADR-0041.md), [ADR-0046](../decisions/ADR-0046.md)) |
| Citation Metadata, Find Identifiers, Check Bibliography | **Removed** ([ADR-0060](../decisions/ADR-0060.md)). All three were live and all three read the document through `bp-research`, which has left. The three rows below read the *store*, which is why they stay |
| Open Questions | **live** — every question the document asks, at the line it begins on; code inside a fence is skipped ([ADR-0046](../decisions/ADR-0046.md)) |
| What the Store Holds | **live** — the store's own contents, and the statement that it never holds the text of a document ([ADR-0046](../decisions/ADR-0046.md)) |

**Nothing in this menu is planned any more**, and it was the first to get
there — ADR-0046 emptied it a few hours before ADR-0048 emptied the rest.

**Three features share this menu and none is the other.** Research Report
reads the *store* and says what you have been writing about over time. Four
rows read the *document in front of you*: three say what it cites (ADR-0044)
and one says what it asks. What the Store Holds, below the second separator,
is the store talking about itself.

**Two rows this file named for years are not here, and their absence is the
decision.**

- **"DOI Lookup"**, until 2026-08-22. Finding an identifier and resolving one
  are different acts, and only the first was available under ADR-0006 —
  `bp-research` had no HTTP client and a test whose job was to notice if that
  changed. A row called Lookup would have been a promise the product could not
  keep. **Finding one is gone too now**
  ([ADR-0060](../decisions/ADR-0060.md)), so the distinction this row was
  refused over no longer has a live side.
- **"Research question, evidence, findings, methods, datasets"**, until
  2026-08-22. Those five are the structure of a research *paper* (IMRaD),
  written into this file by the scaffold commit and never elaborated;
  [ADR-0039](../decisions/ADR-0039.md) defines the mode as synthesis over your
  own notes, which is a different thing. ADR-0046 scoped all five: evidence
  and methods became part of Research Report, datasets became What the Store
  Holds, findings was dropped as a second name for the report itself, and
  research question became **Open Questions** — because a question has a
  grammar and can be found, while which one you are actually asking is not
  something the document says.

## Tools

| Row | State |
| --- | --- |
| Document Inspector | **live** — what is *in* the document: words, lines, paragraphs, format, encoding, security profile, size on disk |
| Security Inspector | **live** — the policy in force on **all six** axes. The Privacy menu shows the two that anything enforces; embeddings, network, temporary files and zeroising are reported here and consulted by nothing ([ADR-0059](../decisions/ADR-0059.md) §4), which the code says out loud rather than leaving the readout to imply otherwise. Every axis Privacy Mode overrode shows the profile's own answer too |
| File Analysis | **live** — the *file*, which is a different object from the document: size, whether it is marked read-only, and whether it changed on disk since it was opened. The size class left with huge-file mode ([ADR-0063](../decisions/ADR-0063.md)) |
| Configuration | **live**, and **read-only, which it says.** Every setting is already editable in the menu it belongs to; what none of them answers is where a value came from when the user did not pick it this session |
| Benchmarks | Not a row. `benches/` holds a README and no benchmark, and a row named for a suite that does not exist is the promise "DOI Lookup" was ([ADR-0048](../decisions/ADR-0048.md)) |
| Conversions | Not a row, and no longer possible — the Data menu owned format conversion and left under [ADR-0062](../decisions/ADR-0062.md) |

## Help

**This section had two rows for one menu**, one of them a subset of the other,
and they had coexisted for as long as nothing asked -- trap 4, in the file
whose whole job is to be the one home for what each menu holds. One row now.

| Row | State |
| --- | --- |
| User Guide | **live** ([ADR-0075](../decisions/ADR-0075.md)). Opens `app-help/index.md`, shipped beside the executable, **as a document in a new tab** -- never in a browser, because this product launches no programs ([ADR-0057](../decisions/ADR-0057.md)). Reports where it looked if the executable has been moved out of the unpacked folder, which is the same way the icon breaks ([ADR-0068](../decisions/ADR-0068.md)) |
| Report a Problem | **live** ([ADR-0077](../decisions/ADR-0077.md)). Composes a pre-filled bug report **as a document in a new tab**, with Help > Diagnostics already in it, and puts the issues URL on the clipboard. It opens no browser and sends nothing: the product launches no programs ([ADR-0057](../decisions/ADR-0057.md)) and makes no network connection ([ADR-0006](../decisions/ADR-0006.md)). The third row of this shape, after Set as Default Editor and User Guide: prepare the artefact, name the step, let the person take it |
| Keyboard Shortcuts | **live**. The same string `docs/generated/reference/shortcuts.md` is generated from, so the dialog and the documentation cannot disagree |
| About | **live**. Names the product, version, renderer and licence -- every one of them read from a constant or the manifest rather than written out ([ADR-0071](../decisions/ADR-0071.md), [ADR-0074](../decisions/ADR-0074.md)) |
| Diagnostics | **live**. The version, the renderer and every resolved directory |

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

**Not built, and not blocked** ([ADR-0081](../decisions/ADR-0081.md)). specs §4
wants files dropped onto the window to open. This section said for a month
that Slint had no channel for it. Slint's winit backend really does not turn
an OS drop into anything `DropArea` sees -- but the application does not need
it to: `slint::winit_030::WinitWindowAccessor::on_winit_window_event` hands it
every winit window event, and `WindowEvent::DroppedFile` is one. It needs the
`unstable-winit-030` feature, and winit 0.30 reports drops on Windows and X11
but not on native Wayland.
