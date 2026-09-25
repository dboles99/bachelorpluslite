# Tabs and windows

Every open document is a tab. Files named on the command line each open in
their own tab, so `bpad a.txt b.txt c.txt` gives you three.

## The tab strip

A tab shows the document's filename. An unsaved change is marked, and the
status bar below shows the **full path** and when the document was last
written to disk -- not when you last typed, which is a different and less
useful fact.

Right-clicking a tab gives you Close Tab, Close Other Tabs, Close All Tabs and
**Copy Full Path**, which is the fastest way to get a path into another
program.

## Windows

**File > New Window** opens a second window with its own tabs. The two share
your settings and the notes index; they do not share tabs, and closing one
does not close the other.

There is no split view inside a window. Two windows side by side is the
answer, and it is the one that works with the window manager you already use.

## Drag and drop

**Dropping a file on the window does not open it**, and that is a limitation
rather than a choice. The toolkit's backend carries no file-drop plumbing --
what it can hand over is plain text or an image, not a path. It is checked
against each toolkit upgrade and will be built the moment it becomes
possible.

Use File > Open, Open Recent, or the command line.

## What survives a restart

**Open Recent** does: the last ten documents, most recent first.

**Open tabs do not.** Starting the program gives you an empty *Untitled*
document rather than restoring what was open. Unsaved work is a separate
question and is handled by the recovery journal -- see
[Undo, history and recovery](Undo-and-Recovery).

---

*This page is generated from [`docs/user/04-tabs.md`](https://github.com/dboles99/bachelorpluslite/blob/main/docs/user/04-tabs.md) and
edits made here will be overwritten. Change the source and open a pull request.*
