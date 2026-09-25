# Editing

Typing, selecting, cut, copy, paste and select-all behave the way they do
everywhere else. This page covers the parts that are this product's own.

The full key list is in
[the shortcuts reference](Keyboard-Shortcuts), which is
generated from the same constant the Help > Keyboard Shortcuts dialog shows,
so the two cannot disagree.

## Line operations

| | |
| --- | --- |
| `Ctrl+D` | Duplicate the current line |
| `Alt+Up` / `Alt+Down` | Move the current line up or down |

**These need the custom editor surface**, which is off by default -- start
with `--editor-view` to get it. See
[The two editor surfaces](#the-two-editor-surfaces) below.

## Zoom

`Ctrl+=` and `Ctrl+-` change the editor font size; `Ctrl+0` puts it back.
This is the same setting as `font_size` in the config file, so a zoom you
like can be made permanent by writing it down -- see [Settings](Settings).

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
([ADR-0018](https://github.com/dboles99/bachelorpluslite/blob/main/docs/decisions/ADR-0018.md)). Everything that does *not* need the
caret -- Find, Find Next, Go to Line, the note layer, saving, encoding --
works identically in both.

The default is the toolkit's widget because it is the better-behaved one for
ordinary typing on every platform. `--editor-view` is opt-in, and the decision
to keep both has been re-examined and reaffirmed.

## What this product will not do while you type

**It does not run anything** ([ADR-0057](https://github.com/dboles99/bachelorpluslite/blob/main/docs/decisions/ADR-0057.md)). There is
no execution in this product at all -- no scripts, no cells, no interpreters,
nothing auto-triggered by content.

**It does not reach the network.** Core editing never depends on a cloud or AI
service ([ADR-0006](https://github.com/dboles99/bachelorpluslite/blob/main/docs/decisions/ADR-0006.md)). The product works identically
with the network cable out.

---

*This page is generated from [`docs/user/03-editing.md`](https://github.com/dboles99/bachelorpluslite/blob/main/docs/user/03-editing.md) and
edits made here will be overwritten. Change the source and open a pull request.*
