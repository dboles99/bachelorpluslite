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

**Three whole menus have left, and this file no longer has a section for
any of them.** [ADR-0062](../decisions/ADR-0062.md) removed **Data** -- eight
rows over JSON, JSON Lines, TOML, YAML and delimited text, all of them live.
And [ADR-0057](../decisions/ADR-0057.md) removed Notebook and Run --
the cell outline, the `.ipynb` export, one row per runnable cell, Stop, Run
Document and Interpreters. Every one of them was live when it was deleted,
which is the point: these were decisions about what the product is, not a
sweep of rows that never worked. **The menu bar is eleven menus now.**

## File

| Row | State |
| --- | --- |
| New, Open, Open Recent, Save All, Close Tab | **live**. Open classifies the document by size before reading it (ADR-0027), and all three classes now open. Under 8 MiB it opens as ever. Between 8 and 192 MiB it opens and edits with `Large file (n MiB)` in the status bar, and every whole-document row stays live rather than being deferred or withdrawn — the label explains a pause instead of removing a capability. Over 192 MiB it is **read from disk as you scroll** (ADR-0030), in the custom surface whichever flag the app started with, because `TextInput` owns its own text and cannot be handed a window of a file it does not have |
| Save, Save As, Reload | **live**, and greyed with the reason for a document served from disk. Not politeness: such a document has no text in a rope, so a save that merely did nothing *special* would encode the empty string and atomically write it over two gigabytes, reporting success. `AppState` refuses all three by name and a test asserts the file's size is unchanged rather than trusting a return value |
| Save a Copy | **live** — writes the buffer elsewhere without moving the document's path, clearing its dirty flag, or touching Open Recent; all three are pinned by tests. Greyed for a document served from disk, with the others: an empty file presented as a copy of a 2 GB log is worse than a refusal, because it looks like it worked |
| Set as Default Editor... | **live** — `bp_platform::editor`, under ADR-0012. The row's hint names the preset it registers for (Notepad Replacement), because the preset is a real choice and one made out of sight is one nobody made. A click **reports before it offers**: the dialog opens with what opens each of those types today, from `association_report`. On Windows that reads "cannot tell what opens .txt files", never "nothing is set to" — `AssociationState::Unknown` is not `Unclaimed`, and rendering the second from the first invites the user to fix something that may not be broken. Then it shows the whole plan — the exact files, the root they go under, the commands that still have to run — and only an explicit OK becomes `Consent::Granted`. On Linux `install` writes the `.desktop` entry and the MIME package under the root `install_root` names, and the shell then **shows the `xdg-mime default` command rather than pretending the job is done**; nothing here runs it, because running it is deciding for the user, which is the thing ADR-0012 draws its line around. On Windows `install` refuses, in the capability register's own words, so a status bar line and a refusal cannot drift apart; what the user gets instead is the `.reg` script, saved where *they* chose, plus `ms-settings:defaultapps` to paste into Run. **No key belonging to another application is ever written, and Notepad is never named**: `bp-platform` refuses to build such a plan and the shell re-checks with `registry_objections` before a `.reg` reaches a Save dialog, because that path does not go through `install` at all |
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
| Split / Preview | **Not a row, and this answers D14** ([ADR-0048](../decisions/ADR-0048.md)). Slint 1.17.1 has no rich-text item — no styled runs, no spans — so bold inside a sentence is not representable, and a preview that silently dropped inline formatting would be worse than none. Handing HTML to the system browser means writing the document's text to a temporary file in plaintext, which is what `Policy::temporary_files` exists to forbid for a Confidential document. Most of what a reader wanted is elsewhere: Note ▸ Outline. Revisit if Slint ships styled text; *Split* was never the hard half |

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

## Security

| Row | State |
| --- | --- |
| Standard / Private / Confidential / Maximum | **live** — the active document's profile; exactly one ticks, and a Custom policy ticks none. Private and Confidential seal the recovery journal with the document's passphrase, so they need the document encrypted |
| What the profile permits (recovery, network) | **live** — a readout, greyed because it is not clickable. A profile is a promise about invisible behaviour, and a promise nobody can see is not one. It named the clipboard until [ADR-0061](../decisions/ADR-0061.md) removed that axis with the crate it governed |
| Encrypt Document... | **live** — asks for a passphrase twice, writes a `.bpadx` beside the original, and the tab adopts it so later saves stay encrypted. Reads "Encrypted (.bpadx)" and greys once the document is |
| Privacy Mode | **live** — a session-wide override that can only tighten |
| Scan for Secrets | **live** — `bp_secrets::scan` over the active document. The status bar gives the count, the kinds and the first three positions; the full listing is a dialog, one line per finding. **Neither ever prints the matched text**: a `Finding` deliberately carries a position and a classification and nothing else, and the shell must not undo that by reaching back into the document to quote it |
| Hash Document (SHA-256) | **live** — `bp_crypto::hash_document` over the bytes the document *would be written as*, not over the buffer, so the digest matches `sha256sum` on a document with a BOM or CRLF endings. Shown grouped in fours (to read down a telephone) and unbroken (to paste), and says so when unsaved edits mean it is not yet the digest of anything on disk |
| Sign Document... | **live**, since ADR-0031 answered where a signing key lives: sealed in a `.bpadx` envelope under a passphrase, rather than protected by file permissions Windows cannot narrow from safe Rust. **One ceremony, not one per signature** — the first signature creates the key, because what was asked for was a signature and a key created without one is a ritual nobody requested; the second finds it and asks only to unlock it. The row's hint says which the click will do, "creates a signing key" or "unlocks your signing key", so a first-time click is not followed by an unexplained ceremony. It still greys for two reasons, and neither is about key storage: a signature is over the bytes **on disk** (ADR-0026), so a document that has never been saved has nothing to sign, and one with unsaved changes would receive a valid signature over the *previous* version — worse than a refusal, because it verifies. The two say different things because the way out of each differs, Save As against Ctrl+S, and the row and the action ask the same predicate so the greying cannot promise what the click then refuses. The message carries the public key, because a signature nobody has the key for is one nobody can check and the moment somebody has just made one is the moment they need to send it |
| Verify Signature... | **live** — `bp_integrity::verify_file` over the `.sig` sidecar, per ADR-0026. The sidecar is **found, not asked for**: `document.ext` is signed by `document.ext.sig`, appended and never substituted, and that name is `bp-integrity`'s function rather than a rule the shell writes down a second time. Greyed for a document that has never been saved, with the reason on the row — there is no file for a sidecar to sit beside. The first pass asks the user nothing, because a key cannot change its answer; **a missing sidecar fails there, closed**, since a check that can be passed by deleting a file is not a check. Only a document that already holds together is worth asking for a key, and that key is what turns "intact" into "signed by who you expected". Each of the five verdicts gets its own sentence, worded by `Verification::explain` so that every surface says the same thing about the same answer: verified / no signature file / the file cannot be read / does not match, the named signer being a claim nothing confirmed / signed by a different key, the document intact. That last one is the verdict a bare 64-byte `.sig` could not produce at all, and the whole reason the format records a key. A pass with no key named carries the caveat `Expectation::AnySigner` earns — anyone who alters a document can re-sign it with a key of their own. The one sentence the shell adds is its own: unsaved edits mean these are not the bytes anybody signed, and the check was made against the file on disk. Public keys are still read in either spelling, 32 raw bytes or hexadecimal as pasted out of an email |
| Security History... | **live** — `bp-audit` under ADR-0024. The reading end of every row above it: a profile change, Privacy Mode, a secret scan, a redaction and a signature check each append one. What is recorded is governed by the document's own profile, so under Confidential and Maximum, and under Privacy Mode, **nothing is written at all** — `Destination::for_policy` resolves those to `SessionOnly`. A tightening is therefore silent and a loosening is recorded, which is the direction worth having: a move down to Standard re-enables everything the profile was switched on to stop. Under Private the history is sealed with the document's own passphrase, so an unencrypted document under that profile gets the notice telling it so rather than a plaintext line |
| Redact Found Secrets... | **live** — `bp_secrets::scan` produces the spans and `bp_redaction::redact` destroys them. `Placeholder`, not `Mask`: `MatchOriginal` publishes the length of what was removed, which for a PIN or a short token is most of the secret. The marker is `[REDACTED: kind]`, labelled with the name of the rule that matched and never with what it matched. A confirmation dialog lists line and kind first and states plainly that this changes the document and not the file — the file on disk, the recovery journal, the undo history and the clipboard all still hold the originals, and saying "redacted" without saying that is the same lie the black rectangle tells. Applied as an ordinary undoable edit for the same reason. **A private key block is deliberately not redacted**: `bp-secrets` marks only its `-----BEGIN` line, so redacting the span would take out the label and leave the key body — the row says so rather than half-doing it. `bp_redaction::verify` runs afterwards and a survivor is reported by line number, never by text. Greyed on an empty document, with the reason on the row |
| Inspect Metadata | **live** — `bp_redaction::metadata::inspect` over the text, reporting kind, exposure and line and never the value. For a container this build cannot open — `.docx`, `.pdf`, `.rtf`, an image — `Container::hidden()` and `requires()` are reported instead of silence: "no metadata found" about a `.docx` reads as an all-clear and would be a lie. For plain text it says the opposite thing it is easy to leave out — that the filesystem entry around the file, its timestamps, ownership and alternate data streams, is not part of the check |
| Lock Document | **live** — forgets this document's passphrase now, so the next save or reload asks again. The row exists because unlocking is sticky for the life of the tab, which is what makes saving an encrypted document bearable and also means one unlocked an hour ago is still unlocked to whoever is at the keyboard; closing the tab was the only way to undo that. It re-encrypts nothing: the file has been encrypted the whole time and the text on screen is unchanged. Greyed when there is no key to forget, because "locked" and "never encrypted" are the same thing to this row |
| Audit history | **live**, as Security History above |

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
| Semantic search | Not a row. It needs embeddings; `bp-security`'s policy has an `Embeddings` axis and nothing computes one, so building it reaches [ADR-0033](../decisions/ADR-0033.md). Search ▸ cross-file search is the honest thing that exists |
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

## Security (the rest)

**This section said "Lock, decrypt in place, secure clipboard, audit,
settings. Planned — phases 15 and 16" until 2026-08-22, and every part of that
was wrong**, 220 lines below a heading that says there are no planned rows
left. Kept as a correction rather than deleted, because the shape recurs:

- **Lock Document is a live row** (`action::LOCK_DOCUMENT`, ADR-0048), and a
  test says so by name — *"Lock Document is live now and must have a real
  action"*. It was still listed as planned here.
- **Audit is Security ▸ Security History** (id 206, ADR-0024), live since
  before this sentence was last touched.
- **Settings is Tools ▸ Configuration** (id 723, ADR-0048).
- **Decrypt in place** and **secure clipboard** were never menu rows and are
  not planned as any: opening a `.bpadx` asks for its passphrase, and what the
  clipboard could retain was a *policy* the profile decided
  ([ADR-0020](../decisions/ADR-0020.md)), read out three rows up. **Both sides
  of that sentence are now gone** — [ADR-0061](../decisions/ADR-0061.md)
  removed the history and its policy axis together, because an axis outlives
  its subject by nothing.
- **Phases 15 and 16 are both "Started"** in `ROADMAP.md`, which owns phase
  status, and their crates ship.

So this is a paragraph of roadmap prose that outlived four of its five nouns.
It is the trap `CLAUDE.md` names — *a name that has sat in a plan long enough
starts to read like a specification* — arriving in a file whose whole job is
to say what is real.

Redaction and the metadata inspector are rows above, and were already.

Hash, sign and verify are no longer among them: `bp-crypto`'s hashing half and
`bp-integrity`'s sidecar are all wired into the rows above, **signing
included** as of ADR-0031. It was the last row in this product greyed for a
missing *decision* rather than a missing prerequisite, and it stayed that way
for three sessions with the reason in its label — which is the shape such a
row should take. The decision, when it came, was to protect the key's contents
rather than its permissions, so nothing new had to be designed, reviewed or
fuzzed: it is the envelope encrypted documents and the recovery journal
already use.

The things that were waiting on profiles now read them: the recovery journal
refuses rather than writing plaintext under a profile that forbids it, and
`bp-storage`'s `record_document` honours `Metadata`. **There were three and
there are two**, since ADR-0061 took the clipboard — and two is now the whole
count of policy axes anything enforces (ADR-0059 §4).

## Tools

| Row | State |
| --- | --- |
| Document Inspector | **live** — what is *in* the document: words, lines, paragraphs, format, encoding, security profile, size on disk |
| Security Inspector | **live** — the policy in force on **all seven** axes. The Security menu shows three; embeddings, temporary files and zeroising were governed invisibly. Every axis Privacy Mode overrode shows the profile's own answer too, so the readout cannot look as though the document itself had changed |
| File Analysis | **live** — the *file*, which is a different object from the document: size, size class and what it implies, read-only, and whether it changed on disk since it was opened |
| Configuration | **live**, and **read-only, which it says.** Every setting is already editable in the menu it belongs to; what none of them answers is where a value came from when the user did not pick it this session |
| Benchmarks | Not a row. `benches/` holds a README and no benchmark, and a row named for a suite that does not exist is the promise "DOI Lookup" was ([ADR-0048](../decisions/ADR-0048.md)) |
| Conversions | Not a row, and no longer possible — the Data menu owned format conversion and left under [ADR-0062](../decisions/ADR-0062.md) |

## Help

| Row | State |
| --- | --- |
| Keyboard Shortcuts, About | **live** |
| Keyboard Shortcuts, About, Diagnostics | **live** — Diagnostics names the version, the renderer and every resolved directory |

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
