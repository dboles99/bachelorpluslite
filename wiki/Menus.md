# Menus

*Generated from `docs/product/MENU_MAP.md` by `scripts/Build-Docs.ps1`. Do not
edit.*

Every row in every menu either does something or is a **readout** -- a greyed
line that answers a question rather than inviting a click. Nothing here means
"not built yet"; there are no planned rows left (ADR-0048).

A greyed row is one of two things, and telling them apart matters:

- **it exists and cannot act right now**, with the reason in its own label --
  Save for a document with nowhere to go. *You can fix this*;
- **a readout**, which is not a row to click at all.

**Caret** means the row needs the custom editor surface (`--editor-view`),
because it has to know where the caret is (ADR-0018).

## File

| Row | State |
| --- | --- |
| New, Open, Open Recent, Save All, Close Tab | **live**. Every document is loaded whole into the rope. It was classified by size first and served from disk past 192 MiB (ADR-0027, ADR-0030) until [ADR-0063](https://github.com/dboles99/bachelorpluslite/blob/main/docs/generated/decisions/ADR-0063.md); a large file now loads or fails trying, which is what Notepad does |
| Save, Save As, Reload | **live**, and greyed with the reason for a document served from disk. Not politeness: such a document has no text in a rope, so a save that merely did nothing *special* would encode the empty string and atomically write it over two gigabytes, reporting success. `AppState` refuses all three by name and a test asserts the file's size is unchanged rather than trusting a return value |
| Save a Copy | **live** — writes the buffer elsewhere without moving the document's path, clearing its dirty flag, or touching Open Recent; all three are pinned by tests |
| Set as Default Editor... | **live** — `bp_platform::editor`, under ADR-0012. The row's hint names the preset it registers for (Notepad Replacement), because the preset is a real choice and one made out of sight is one nobody made. **Every extension any preset offers is one this build can open** ([ADR-0069](https://github.com/dboles99/bachelorpluslite/blob/main/docs/generated/decisions/ADR-0069.md) removed `.bpadx`, which every preset had claimed, and removed the test exemption that let it). A click **reports before it offers**: the dialog opens with what opens each of those types today, from `association_report`. On Windows that reads "cannot tell what opens .txt files", never "nothing is set to" — `AssociationState::Unknown` is not `Unclaimed`, and rendering the second from the first invites the user to fix something that may not be broken. Then it shows the whole plan — the exact files, the root they go under, the commands that still have to run — and only an explicit OK becomes `Consent::Granted`. On Linux `install` writes the `.desktop` entry and the MIME package under the root `install_root` names, and the shell then **shows the `xdg-mime default` command rather than pretending the job is done**; nothing here runs it, because running it is deciding for the user, which is the thing ADR-0012 draws its line around. On Windows `install` refuses, in the capability register's own words, so a status bar line and a refusal cannot drift apart; what the user gets instead is the `.reg` script, saved where *they* chose, plus `ms-settings:defaultapps` to paste into Run. **No key belonging to another application is ever written, and Notepad is never named**: `bp-platform` refuses to build such a plan and the shell re-checks with `registry_objections` before a `.reg` reaches a Save dialog, because that path does not go through `install` at all |
| New Window | **live** |
| Open Folder, Revert, Print | Not rows, and not planned. Revert is Reload; Open Folder needs a project concept this product does not have (ADR-0048); Print is platform work with no cross-platform story yet |

## Edit

| Row | State |
| --- | --- |
| Undo, Redo, Cut, Copy, Paste, Select All | **live** (both editor views) |
| Double-click a word, triple-click a line | **live** — native under `TextInput`, `bp_editor::{select_word_at, select_line_at}` under `--editor-view` |
| Clipboard History, Paste transformations | **Removed** ([ADR-0061](https://github.com/dboles99/bachelorpluslite/blob/main/docs/generated/decisions/ADR-0061.md)). Both were live. Cut, Copy and Paste above are the OS clipboard and are untouched — the history shared a word with them and nothing else |
| Sort / Deduplicate / Reverse / Trim lines | **live** |
| Duplicate Line, Move Line Up / Down | **caret** |
| Go to Line | **live** in both surfaces — it needs the caret *moved*, never read |
| Multi-cursor, column selection | **Not a row** ([ADR-0048](https://github.com/dboles99/bachelorpluslite/blob/main/docs/generated/decisions/ADR-0048.md)). `TextInput` has one caret and cannot draw a second, so it could only work under `--editor-view` — and `bp-editor` would have to carry a set of carets through every command, selection and undo entry. A real feature, and it comes back as a queue item with a design behind it rather than as a row that has sat in a menu since the scaffold commit |

## View

| Row | State |
| --- | --- |
| Light / Dark / Organic / Green | **live** |
| Line Numbers, Word Wrap | **live** |
| Zoom In / Zoom Out / Reset Zoom | **live** — Ctrl+= / Ctrl+- / Ctrl+0, bounded by `bp_config::{MIN,MAX}_FONT_SIZE`; the rows grey at the bound and the reset row names the size in force |
| Follow System | **live** — `ThemeId::for_system` resolves the desktop's preference to Light or Dark; a desktop that will not say leaves the theme alone and says so |
| Split / Preview | **Not a row, and this answers D14** ([ADR-0048](https://github.com/dboles99/bachelorpluslite/blob/main/docs/generated/decisions/ADR-0048.md)). Slint 1.17.1 has no rich-text item — no styled runs, no spans — so bold inside a sentence is not representable, and a preview that silently dropped inline formatting would be worse than none. Handing HTML to the system browser means writing the document's text to a temporary file in plaintext, which is what `Policy::temporary_files` exists to forbid for a Confidential document. Most of what a reader wanted is elsewhere: Note ▸ Outline. Revisit if Slint ships styled text; *Split* was never the hard half |

## Insert

| Row | State |
| --- | --- |
| Date, Time, Date and Time, ISO 8601, Filename date | **caret** — each row's hint is the stamp rendered from the clock, so the row shows what it will insert |
| Bold, Italic, Link, Code Block, Table | **caret** — the Markdown constructs, all live |
| Citation | Not a row, and now not possible either. `bp-research` modelled citations and left under [ADR-0060](https://github.com/dboles99/bachelorpluslite/blob/main/docs/generated/decisions/ADR-0060.md); there is nothing to insert *from* |

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
| Recovery Checkpoints | **live** — what the crash-recovery journal holds for this document, and *before the count*, whether this profile writes one at all. **Not "Revision History"**, which this file named until 2026-08-22: `bp-history` holds the pending checkpoint and discards it the moment a save succeeds, so the name promised successive versions to go back to ([ADR-0048](https://github.com/dboles99/bachelorpluslite/blob/main/docs/generated/decisions/ADR-0048.md)) |
| Related Notes | **live, in Organize.** Two rows in two menus running the same query is how one of them goes stale |
| Classification, properties | Not rows. Both need a schema for what a note *is*, which nothing in this product has decided |

## Privacy

| Row | State |
| --- | --- |
| Standard / Private / Confidential / Maximum | **live** — the active document's profile; exactly one ticks, and a Custom policy ticks none |
| Recovery journal: … | **live** — a readout, greyed because it is not clickable. Standard and Private journal unsaved work in plaintext and this row says *"on, unencrypted"*; Confidential and Maximum journal nothing. **Not "encrypted"** — ADR-0064 deleted that variant rather than pointing it at plaintext. **This row is load-bearing** ([ADR-0065](https://github.com/dboles99/bachelorpluslite/blob/main/docs/generated/decisions/ADR-0065.md)): Private keeps a journal *because* the row names its form, and a test says that if the row stops doing so, Private goes back to no journal |
| Recorded: … | **live** — a readout: what the metadata store may keep about this document. Path, title and tags under Standard; the path only under Private; nothing under Confidential and Maximum |
| Privacy Mode | **live** — a session-wide override that can only tighten, and it *acts*: journals already written are removed |
| Encrypt Document, Scan for Secrets, Redact, Inspect Metadata, Hash, Sign, Verify, Security History, Lock Document | **Removed** ([ADR-0064](https://github.com/dboles99/bachelorpluslite/blob/main/docs/generated/decisions/ADR-0064.md)). All nine were live. `.bpadx` documents already on a disk cannot be opened by this build, and there is no migration — the ADR says why |

## Organize

| Row | State |
| --- | --- |
| Related Notes | **live** — a collapsible panel of documents sharing tags with the active one ([ADR-0037](https://github.com/dboles99/bachelorpluslite/blob/main/docs/generated/decisions/ADR-0037.md)) |
| Duplicate Detection | **live** — automatic at save and on demand |
| Suggested Folder | **live** — where documents sharing this one's tags already live, counted from what the user has filed rather than a scheme imposed on them. **It never moves a file**; File ▸ Save a Copy is where that already lives |
| Project | Not a row. Nothing in this product has a concept of a project, and inventing one to fill a menu row is how a feature nobody asked for gets built ([ADR-0048](https://github.com/dboles99/bachelorpluslite/blob/main/docs/generated/decisions/ADR-0048.md)) |
| Topics | Not a row — already in two places: Note ▸ Tags for the document, Research ▸ Research Report's dominant themes for the store |
| Semantic search | Not a row. It needs embeddings; `bp-security`'s policy has an `Embeddings` axis and nothing computes one, so building it reaches [ADR-0033](https://github.com/dboles99/bachelorpluslite/blob/main/docs/generated/decisions/ADR-0033.md). Search ▸ cross-file search is the honest thing that exists |
| Entities, smart collections | Not rows. Both need a data model nothing has decided |

## Research

| Row | State |
| --- | --- |
| Research Report | **live** — dominant themes, stale clusters, under-connected documents, consolidation candidates, each naming the documents behind it, and a closing section stating every threshold it applied ([ADR-0041](https://github.com/dboles99/bachelorpluslite/blob/main/docs/generated/decisions/ADR-0041.md), [ADR-0046](https://github.com/dboles99/bachelorpluslite/blob/main/docs/generated/decisions/ADR-0046.md)) |
| Citation Metadata, Find Identifiers, Check Bibliography | **Removed** ([ADR-0060](https://github.com/dboles99/bachelorpluslite/blob/main/docs/generated/decisions/ADR-0060.md)). All three were live and all three read the document through `bp-research`, which has left. The three rows below read the *store*, which is why they stay |
| Open Questions | **live** — every question the document asks, at the line it begins on; code inside a fence is skipped ([ADR-0046](https://github.com/dboles99/bachelorpluslite/blob/main/docs/generated/decisions/ADR-0046.md)) |
| What the Store Holds | **live** — the store's own contents, and the statement that it never holds the text of a document ([ADR-0046](https://github.com/dboles99/bachelorpluslite/blob/main/docs/generated/decisions/ADR-0046.md)) |

## Tools

| Row | State |
| --- | --- |
| Document Inspector | **live** — what is *in* the document: words, lines, paragraphs, format, encoding, security profile, size on disk |
| Security Inspector | **live** — the policy in force on **all six** axes. The Privacy menu shows the two that anything enforces; embeddings, network, temporary files and zeroising are reported here and consulted by nothing ([ADR-0059](https://github.com/dboles99/bachelorpluslite/blob/main/docs/generated/decisions/ADR-0059.md) §4), which the code says out loud rather than leaving the readout to imply otherwise. Every axis Privacy Mode overrode shows the profile's own answer too |
| File Analysis | **live** — the *file*, which is a different object from the document: size, whether it is marked read-only, and whether it changed on disk since it was opened. The size class left with huge-file mode ([ADR-0063](https://github.com/dboles99/bachelorpluslite/blob/main/docs/generated/decisions/ADR-0063.md)) |
| Configuration | **live**, and **read-only, which it says.** Every setting is already editable in the menu it belongs to; what none of them answers is where a value came from when the user did not pick it this session |
| Benchmarks | Not a row. `benches/` holds a README and no benchmark, and a row named for a suite that does not exist is the promise "DOI Lookup" was ([ADR-0048](https://github.com/dboles99/bachelorpluslite/blob/main/docs/generated/decisions/ADR-0048.md)) |
| Conversions | Not a row, and no longer possible — the Data menu owned format conversion and left under [ADR-0062](https://github.com/dboles99/bachelorpluslite/blob/main/docs/generated/decisions/ADR-0062.md) |

## Help

| Row | State |
| --- | --- |
| User Guide | **live** ([ADR-0075](https://github.com/dboles99/bachelorpluslite/blob/main/docs/generated/decisions/ADR-0075.md)). Opens `app-help/index.md`, shipped beside the executable, **as a document in a new tab** -- never in a browser, because this product launches no programs ([ADR-0057](https://github.com/dboles99/bachelorpluslite/blob/main/docs/generated/decisions/ADR-0057.md)). Reports where it looked if the executable has been moved out of the unpacked folder, which is the same way the icon breaks ([ADR-0068](https://github.com/dboles99/bachelorpluslite/blob/main/docs/generated/decisions/ADR-0068.md)) |
| Report a Problem | **live** ([ADR-0077](https://github.com/dboles99/bachelorpluslite/blob/main/docs/generated/decisions/ADR-0077.md)). Composes a pre-filled bug report **as a document in a new tab**, with Help > Diagnostics already in it, and puts the issues URL on the clipboard. It opens no browser and sends nothing: the product launches no programs ([ADR-0057](https://github.com/dboles99/bachelorpluslite/blob/main/docs/generated/decisions/ADR-0057.md)) and makes no network connection ([ADR-0006](https://github.com/dboles99/bachelorpluslite/blob/main/docs/generated/decisions/ADR-0006.md)). The third row of this shape, after Set as Default Editor and User Guide: prepare the artefact, name the step, let the person take it |
| Keyboard Shortcuts | **live**. The same string `docs/generated/reference/shortcuts.md` is generated from, so the dialog and the documentation cannot disagree |
| About | **live**. Names the product, version, renderer and licence -- every one of them read from a constant or the manifest rather than written out ([ADR-0071](https://github.com/dboles99/bachelorpluslite/blob/main/docs/generated/decisions/ADR-0071.md), [ADR-0074](https://github.com/dboles99/bachelorpluslite/blob/main/docs/generated/decisions/ADR-0074.md)) |
| Diagnostics | **live**. The version, the renderer and every resolved directory |

## Find bar

| Control | State |
| --- | --- |
| Find, Replace, Replace All, In Folder | **live** |
| Aa / Word / .* toggles | **live** — `bp_search::Query::{case_sensitive, whole_word, regex}`; flipping one re-runs the search |

## Tab strip context menu

| Row | State |
| --- | --- |
| Close Tab, Close Other Tabs, Close All Tabs | **live** — each goes through the same unsaved-changes prompt as closing one |
| Copy Full Path | **live** — greyed for a document that has never been saved |

---

*This page is generated from [`docs/generated/reference/menus.md`](https://github.com/dboles99/bachelorpluslite/blob/main/docs/generated/reference/menus.md) and
edits made here will be overwritten. Change the source and open a pull request.*
