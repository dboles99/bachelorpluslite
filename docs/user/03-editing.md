# Editing

Typing, selecting, cut, copy, paste and select-all behave the way they do
everywhere else. This page covers the parts that are this product's own.

The full key list is in
[the shortcuts reference](../generated/reference/shortcuts.md), which is
generated from the same constant the Help > Keyboard Shortcuts dialog shows,
so the two cannot disagree.

## The menus from the keyboard

**F10 opens File.** From there:

| | |
| --- | --- |
| `Left` / `Right` | The menu beside this one. Left from File is Help, and Right from Help is File |
| `Up` / `Down`, `Home` / `End` | Move through the rows |
| `Enter` or `Space` | Run the row |
| `Esc` or `F10` | Close the menu and go back to the document |

A menu opened with the mouse answers the same keys. The keyboard can land on
a greyed row, because a greyed row's label is often the reason it is greyed;
`Enter` on it does nothing. While a menu is open, what you type goes to the
menu and never to the document.

**A bare `Alt` does not open the menus**, as it does in Notepad. On Linux a
window that loses the keyboard to `Alt+Tab` is sent the `Alt` release before
it is told it has lost focus, so it cannot tell `Alt+Tab` from `Alt` pressed
alone. Every `Alt+Tab` away would open File behind you.
[ADR-0088](../decisions/ADR-0088.md) has the detail.

### Screen readers

The menus and their rows, the tabs, the find and replace bar, Go to Line and
the questions the program asks all say what they are to a screen reader,
through UI Automation on Windows and AT-SPI on Linux. **It has not yet been
tried with NVDA or Orca**, so it is not called accessible; that pass is
planned for 1.0 ([ADR-0088](../decisions/ADR-0088.md)). What is known:

- **On Linux a menu does not say whether it is open.** The toolkit reports it
  on Windows only.
- **A greyed menu row says "unavailable"** in its description, because on
  Linux the disabled state itself does not reach the screen reader.
- **Under `--editor-view` the document is not a text box to a screen
  reader.** Its visible lines are there as separate labels, with no caret and
  nothing to say they can be edited. The default surface is a standard text
  box.

## Line operations

| | |
| --- | --- |
| `Ctrl+D` | Duplicate the current line |
| `Alt+Up` / `Alt+Down` | Move the current line up or down |

**These need the custom editor surface**, which is off by default -- start
with `--editor-view` to get it. See
[The two editor surfaces](#the-two-editor-surfaces) below.

## Overtype

**Insert** switches between inserting and overtyping, as it does in Notepad.
While overtype is on, the status bar says **Overtype**, the caret is drawn
heavier, and each character you type replaces the one after the caret.

It never replaces the end of a line. Typing past it adds to the line rather
than pulling the next one up, and Enter still splits the line. A selection is
replaced whole, in either mode.

`Ctrl+Insert` and `Shift+Insert` are copy and paste, and leave the mode alone.
The mode belongs to the window, not to a document: it stays as it is when you
switch tabs, and it is off whenever the program starts.

It works in both [editor surfaces](#the-two-editor-surfaces), with one
difference in undo. In the default surface each overtyped character takes two
presses of `Ctrl+Z` to undo, because the toolkit's text widget records a
replacement as a removal and an insertion. Nothing is lost -- it is only
slower. Under `--editor-view` a run of overtyping undoes in one step, the way
a run of typing does.

## Zoom

`Ctrl+=` and `Ctrl+-` change the editor font size; `Ctrl+0` puts it back.
This is the same setting as `font_size` in the config file, so a zoom you
like can be made permanent by writing it down -- see [Settings](08-settings.md).

## Indentation

**Format > Indentation** chooses between a literal tab and spaces, and sets
how many columns a tab advances to (2, 4 or 8).

The default is a literal tab. *Notepad when you want it* means Tab inserts a
tab; silently rewriting it into spaces is exactly the kind of surprise that
promise rules out, so spaces are something you ask for.

## Insert

**Insert** holds Markdown constructs -- bold, italic, a link, a code block, a
table. They wrap the selection if there is one and otherwise leave the caret
where you would want it.

These need `--editor-view` too, for the same reason the line operations do:
they have to know where the caret is.

## The two editor surfaces

The product draws text two ways, and which one you get is a flag.

| | Default | `--editor-view` |
| --- | --- | --- |
| Drawn by | The toolkit's own text widget | This product's editor surface |
| Typing, selection, clipboard | The toolkit | This product |
| Duplicate line, move line, Insert | **unavailable** | available |
| Status bar with `Ln`, `Col` | no | yes |

The reason is that the toolkit's text widget will not say where the caret is,
and a feature that has to know cannot be built on it
([ADR-0018](../decisions/ADR-0018.md)). Everything that does *not* need the
caret -- Find, Find Next, Go to Line, the note layer, saving, encoding --
works identically in both.

The default is the toolkit's widget because it is the better-behaved one for
ordinary typing on every platform. `--editor-view` is opt-in, and the decision
to keep both has been re-examined and reaffirmed.

## What this product will not do while you type

**It does not run anything** ([ADR-0057](../decisions/ADR-0057.md)). There is
no execution in this product at all -- no scripts, no cells, no interpreters,
nothing auto-triggered by content.

**It does not reach the network.** Core editing never depends on a cloud or AI
service ([ADR-0006](../decisions/ADR-0006.md)). The product works identically
with the network cable out.
