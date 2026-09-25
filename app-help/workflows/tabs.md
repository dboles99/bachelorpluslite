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

**Dropping a file on the window does not open it yet.** This page used to
call that a toolkit limitation, and it was not one: the drop reaches the
application through the windowing layer beneath the toolkit, on Windows and on
an X11 Linux session ([ADR-0081](https://bpad.prompt-forge.dev/docs)). It is queued. On a
native Wayland session it will not work even once it is built, because that
layer does not report a dropped file there.

Use File > Open, Open Recent, or the command line.

## What survives a restart

**Open Recent** does: the last ten documents, most recent first.

**Open tabs do not.** Starting the program gives you an empty *Untitled*
document rather than restoring what was open. Unsaved work is a separate
question and is handled by the recovery journal -- see
[Undo, history and recovery](../troubleshooting/recovery.md).

---

[Back to the help index](../index.md)
