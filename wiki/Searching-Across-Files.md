# Searching across a folder

Finding something you know you wrote down, without remembering which file it
was in.

## In the document you are in

`Ctrl+F`.

The bar takes the caret, so typing goes into the search box rather than into
your document. As you type, the first match is found, scrolled to and
counted -- the bar tells you how many there are.

**Enter, or Find Next, steps between matches without changing the document.**

## Across a folder

In the find bar, **In Folder...**. Pick a folder, and every document beneath it
is searched.

Results open in a panel below. Each row names a file and shows the matching
line. Clicking one:

1. opens that document in a new tab,
2. scrolls to the match,
3. highlights it.

This works when the row is below the fold and when the document is much taller
than the window.

## Replace

**Replace and Replace All work on the current document only.**

Replace All is a single undo entry -- one `Ctrl+Z` puts all of them back.

**There is no replace-across-files, deliberately.** Changing many files at once
with no way to preview what would happen and no undo that spans them is an
operation that needs machinery this product does not have. It is absent rather
than present and dangerous.

To change many files, open them and do it one at a time, or use a tool built
for it.

## Finding by meaning rather than by string

**Organize > Related Notes** answers a different question: not *where does this
exact text appear* but *what else have I written that is about this*. It works
off the notes index -- tags and keywords -- rather than a literal search.

It is the better tool when you cannot remember the wording, which is most of
the time. It needs the index, so it needs a privacy profile that permits one;
see [Privacy](Privacy-and-Security).

## What it does not do

**It does not index the contents of files you have not opened.** The index
knows about documents you have worked on. A folder search reads the files
right then, and a folder full of documents this product has never opened is
searchable by string and not by relatedness.

---

*This page is generated from [`docs/tutorials/03-search-across-files.md`](https://github.com/dboles99/bachelorpluslite/blob/main/docs/tutorials/03-search-across-files.md) and
edits made here will be overwritten. Change the source and open a pull request.*
