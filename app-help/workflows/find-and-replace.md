# Find and replace

`Ctrl+F` opens the find bar. It takes the caret; typing goes into the search
box, not into your document.

## In this document

Type, and the first match is found, scrolled to and counted -- the bar tells
you how many there are. **Find Next** steps between them without changing the
document.

That last clause is worth stating because it was once false. Pressing Enter
twice in the find box used to replace the match with a line break, and the
cause was a comment asserting that Find Next, Go to Line and a cross-file
result were "each a single deliberate jump the user makes once" -- three
claims, one of them true. It is fixed, and
[ADR-0052](https://bpad.prompt-forge.dev/docs) is the record, because the shape of the
mistake is more useful than the fix.

**Replace** and **Replace All** work on the current document. Replace All is a
single undo entry, so `Ctrl+Z` puts every one of them back at once.

## Across a folder

The find bar has **In Folder...**, which opens a folder picker and searches
every document beneath it.

Results appear in a panel. Clicking a row opens that document, scrolls to the
match and highlights it -- including when the row is below the fold, which is
the case that used to be broken.

Cross-file search reads files. It does not modify them, and there is no
replace-across-files. Changing many files at once with no way to see what
happened is the kind of operation that needs a preview and an undo bigger than
this product has, so it is not offered rather than offered badly.

## Go to line

`Ctrl+G`, a number, Enter. The line is scrolled to and highlighted.

It works in **both** editor surfaces. It was marked as needing the custom
surface for a long time and did not: moving the caret is something the
toolkit's widget will do perfectly well, and only *reporting where the caret
is* is the thing it refuses.

From the command line:

```sh
bpad --line 427 server.log
```

which does exactly what `Ctrl+G` does, using the same code path.

---

[Back to the help index](../index.md)
