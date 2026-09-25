# Creating, opening and saving documents

## New and Open

| | |
| --- | --- |
| **File > New** (`Ctrl+N`) | An empty *Untitled* document, in a new tab |
| **File > Open** (`Ctrl+O`) | A file picker |
| **File > Open Recent** | The last ten documents, most recent first |
| **File > Save** (`Ctrl+S`) | Write to the document's path, or ask for one |
| **File > Save As** (`Ctrl+Shift+S`) | Write to a new path, and keep editing that one |
| **File > Save a Copy** | Write to a new path, and keep editing the **original** |
| **File > Save All** | Every modified tab |
| **File > Reload** | Discard unsaved changes and re-read from disk |

**Save a Copy is the one whose behaviour is easy to assume wrongly.** After
Save As, you are editing the new file. After Save a Copy, you are still
editing the old one and a snapshot has been written elsewhere.

You can also name files on the command line, and each one opens in its own
tab:

```sh
bpad notes.md server.log
bpad --line 427 server.log
```

## Opening a large file

A document is loaded whole. There is no separate mode for a big one and no
flag to choose one -- a large file loads or it fails trying, which is what
Notepad does ([ADR-0063](https://github.com/dboles99/bachelorpluslite/blob/main/docs/decisions/ADR-0063.md)).

Editing itself does not care about size. The text is held in a rope, so an
insert in the middle of a very large document costs the same as an insert
anywhere else, and scrolling costs what is on screen rather than what is in
the file.

## Renaming a document

**Note > Semantic Rename...** proposes a semantic filename and shows it to you before
anything happens. The shape is:

```
Title_DDMMMYYYY.ext        Quarterly_Review_10SEP2026.md
```

The title comes from the document -- its first heading, or its first
meaningful line -- and the date is the day it is being named. You get the
proposal, you approve it, and only then is anything renamed.

Nothing is renamed silently, ever. That is a project rule rather than a
feature of this dialog.

## Deleting a document

**There is no Delete row, and that is deliberate.** This product does not
delete your files. Use your file manager, where deletion goes to a recycle
bin or trash you can get things back out of.

The same reasoning covers everything else this product will not do on your
behalf: it will not seize a file association
([ADR-0012](https://github.com/dboles99/bachelorpluslite/blob/main/docs/decisions/ADR-0012.md)), it will not clean up files left behind
by features that have been removed ([ADR-0070](https://github.com/dboles99/bachelorpluslite/blob/main/docs/decisions/ADR-0070.md)), and
it will not run anything ([ADR-0057](https://github.com/dboles99/bachelorpluslite/blob/main/docs/decisions/ADR-0057.md)).

## When the file changes underneath you

If a document changes on disk while it is open, you are told and asked what to
do. Nothing is overwritten and nothing is discarded until you say which one
you want.

## Closing

| | |
| --- | --- |
| **Close Tab** (`Ctrl+W`) | One document |
| **Close Other Tabs** | Everything except the one you are in |
| **Close All Tabs** | All of them |

Anything unsaved prompts. If the program stops without getting the chance to
ask -- a crash, a power cut, a `kill` -- see
[Undo, history and recovery](Undo-and-Recovery), because the work is
probably still there.

---

*This page is generated from [`docs/user/02-documents.md`](https://github.com/dboles99/bachelorpluslite/blob/main/docs/user/02-documents.md) and
edits made here will be overwritten. Change the source and open a pull request.*
