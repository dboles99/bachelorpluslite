# Keyboard shortcuts

*Generated from `bp_ui::dispatch::SHORTCUTS` by `scripts/Build-Docs.ps1`. Do
not edit.*

This is the same string Help > Keyboard Shortcuts shows, so the dialog and
this page cannot disagree.

| Keys | Does |
| --- | --- |
| `Ctrl+N` | New |
| `Ctrl+O` | Open |
| `Ctrl+S` | Save |
| `Ctrl+Shift+S` | Save As |
| `Ctrl+W` | Close tab |
| `Ctrl+F` | Find and replace |
| `Ctrl+Z / Ctrl+Y` | Undo / Redo |
| `Ctrl+X/C/V` | Cut / Copy / Paste |
| `Ctrl+A` | Select all |
| `Insert` | Overtype on / off |
| `Ctrl+Insert` | Copy |
| `Shift+Insert` | Paste |
| `Ctrl+= / Ctrl+-` | Zoom in / out |
| `Ctrl+0` | Reset zoom |
| `Ctrl+D` | Duplicate line |
| `Alt+Up / Down` | Move line up / down |

## Two of these need the custom editor surface

`Ctrl+D` and `Alt+Up` / `Alt+Down` have to know where the caret is, and the
toolkit's text widget will not say. Start with `--editor-view` to get them
(ADR-0018).

Everything else works in both surfaces.

## They cannot be rebound

Deliberately absent rather than half-present: rebinding needs a keymap file, a
conflict story and a way to see what a key does now, and none of that has been
designed.
