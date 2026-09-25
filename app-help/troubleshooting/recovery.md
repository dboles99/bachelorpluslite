# Undo, history and recovery

## Undo and redo

`Ctrl+Z` and `Ctrl+Y`, per document. Each tab has its own history and they do
not interfere.

Undo entries are grouped the way you would expect: a run of typing is one
entry rather than one per character, and Replace All is one entry rather than
one per replacement.

**A no-op does not create an undo entry.** Choosing the encoding a document
already has, or replacing text with itself, leaves the history alone -- so
`Ctrl+Z` after one of those undoes the last thing you actually did, rather
than appearing to do nothing.

## Checkpoints

**Note > Recovery** lists checkpoints of the current document. Restoring one
brings back its text *and* its encoding and line ending, which is the part
worth knowing: a checkpoint is a whole document rather than a string, so
restoring one cannot silently convert CRLF to LF.

## The recovery journal

While a document has unsaved changes, the product keeps a journal so the work
survives the program stopping without being able to ask you anything -- a
crash, a power cut, a `kill`.

On the next launch you are asked what to do with it, and there are three
answers:

| | What happens |
| --- | --- |
| **Restore** | The work opens as unsaved changes, exactly as you left it, and you decide where it goes. Nothing is written over anything |
| **Discard** | The work is deleted. This is the only answer that deletes it |
| **Not Now** | The work stays where it is and you are asked again next time. Escape, and closing the window with the question still up, mean the same |

**A question you did not answer never costs you the work**
([ADR-0084](https://bpad.prompt-forge.dev/docs)). That was not always true: on a Linux
desktop without `zenity`, the question could not be drawn at all and was
answered *discard* on your behalf.

Closing a document with **Don't Save** deletes its journal too. Otherwise the
next launch would offer to recover work you had just chosen to throw away.

| | Windows | Linux |
| --- | --- | --- |
| Journals | `%LOCALAPPDATA%\bachelorpad\state\recovery\` | `$XDG_STATE_HOME/bachelorpad/recovery/`, else `~/.local/state/bachelorpad/recovery/` |

Each journal is a small JSON file naming the document it belongs to. An
*Untitled* buffer has no path, so its journal names none -- which matters if
you ever go looking, because such a file matches no filter you might write.
The file names begin with a tag for the run that wrote them, so two runs of
the program -- one after a crash, or two open at once -- never write over, or
tidy away, each other's work.

**Whether a journal is written at all is a privacy setting.** It is one of the
two axes the profile model governs; see [Privacy](../troubleshooting/privacy.md). A profile
that writes no journal means unsaved work does not survive a crash, and that
is the trade being made rather than a defect.

## What is not kept

**No clipboard history.** It existed and was removed
([ADR-0061](https://bpad.prompt-forge.dev/docs)).

**No open-tab session.** Starting the program gives you an empty document, not
what you had open last time. Open Recent is what remembers.

---

[Back to the help index](../index.md)
