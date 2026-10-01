# Encoding, line endings and file formats

## Encoding

**A file opens in the encoding it was written in, and is saved back in that
encoding** ([ADR-0085](https://github.com/dboles99/bachelorpluslite/blob/main/docs/decisions/ADR-0085.md)). The status bar always
shows which.

| A file in | Opens as |
| --- | --- |
| UTF-8, with or without a byte-order mark | UTF-8 |
| UTF-16 with a byte-order mark | UTF-16 LE or BE |
| UTF-16 without one | UTF-16, found by its pattern -- it has a zero in every other byte |
| Anything else -- the "ANSI" files Notepad writes, Shift-JIS, GBK | the legacy code page it most likely is, by name: `windows-1252`, `Shift_JIS`, `GBK` |

The last row is **a guess**, made by looking at the bytes, and on a short file
it can be wrong -- four bytes of French can look like Czech. When it is, use
**Format > Reopen As** and choose the right one: Western, Central European,
Cyrillic, Japanese, Simplified or Traditional Chinese, Korean, or back to
UTF-8 or UTF-16. Reopening reads the file again, so it asks first if you have
unsaved changes.

**Format > Encoding** offers **UTF-8**, **UTF-8 with BOM** and **UTF-16 LE**,
to change how the document is saved. Choosing one converts the file when you
next save it.

**Saving never changes a character behind your back.** If you type something
the file's encoding has no form for -- a Greek `α` into a Windows-1252 file --
Save refuses, names the character and its line, and suggests UTF-8. Nothing is
written, and your work stays unsaved rather than lost.

**And a file that would not save back unchanged is not opened.** A few code
pages have two codes for one character -- Shift-JIS has an NEC and an IBM code
for some kanji -- and only one can be written. Opening such a file and saving
it would change bytes you never edited, so it is refused with that reason, and
nothing in it is touched.

The BOM variant exists because some Windows tools still want it.

Choosing the encoding a document already has does nothing, and records no undo
entry.

## Line endings

**Format > Line Ending** offers **LF** and **CRLF**. Choosing one converts the
whole document when you next save it.

**A document is saved in its own line ending**, the one the status bar shows.
Press Enter in a file Notepad wrote and the new lines are saved as CRLF, like
the old ones; paste text with Unix line endings into it and they are saved as
CRLF too ([ADR-0090](https://github.com/dboles99/bachelorpluslite/blob/main/docs/decisions/ADR-0090.md)).

The exception is **a file that already mixed both** when you opened it. Which
of its line endings is the stray one is not for this program to guess, so it
is saved exactly as it came, until you choose LF or CRLF from the menu.

A line is `\n` or `\r\n`, and nothing else ([ADR-0029](https://github.com/dboles99/bachelorpluslite/blob/main/docs/decisions/ADR-0029.md)).
That is narrower than Unicode's definition of a line break, which also counts
things like `\u{2028}` -- treating those as line breaks makes line numbers
disagree with every other tool you would compare them against, so this product
does not.

The current encoding and line ending are shown in the status bar, always, so
you never have to open a menu to find out what you are about to save.

## Formats it recognises

Markdown, YAML, JSON, JSONL/NDJSON, TOML, CSV/TSV, XML and common source
languages are **recognised** -- the format is detected and shown in the status
bar, and used by the note layer for things like finding a title.

**Recognition, not transformation.** There is no format, validate, sort-keys
or convert. Those operations existed in this product and were removed
([ADR-0062](https://github.com/dboles99/bachelorpluslite/blob/main/docs/decisions/ADR-0062.md)) as part of scoping it down to Notepad
plus a note layer. The full BachelorPad+ is where they live.

What recognition is used for:

- the format label in the status bar;
- Markdown constructs in the Insert menu;
- finding a document's title for Note > Rename;
- Tools > Inspector and Tools > File Analysis, which report on a document
  rather than changing it.

## What you cannot do here

**No Markdown preview.** This page used to say the toolkit could not render
one. It can, and could when that was written: its styled-text element renders
emphasis, links, lists and inline code, though not headings or tables
([ADR-0081](https://github.com/dboles99/bachelorpluslite/blob/main/docs/decisions/ADR-0081.md)). Whether this product should have a
preview is now an open question rather than a technical one. Writing it to
HTML and opening a browser is still ruled out -- this product does not launch
programs.

**No printing.** It is platform work with no cross-platform story yet, and
half of it is not a thing worth shipping.

---

*This page is generated from [`docs/user/07-encoding-and-formats.md`](https://github.com/dboles99/bachelorpluslite/blob/main/docs/user/07-encoding-and-formats.md) and
edits made here will be overwritten. Change the source and open a pull request.*
