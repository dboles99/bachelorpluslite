# Encoding, line endings and file formats

## Encoding

**Format > Encoding** offers **UTF-8** and **UTF-8 with BOM**.

A document can also be opened in **UTF-16**, with a byte-order mark, and is
saved back that way.

**A file in a legacy code page is refused today.** Windows-1252, Shift-JIS
and GBK text with any accented or non-Latin character stops at *not valid
UTF-8*. Notepad opens them.

**UTF-16 with no byte-order mark is worse: it is taken for UTF-8.** Mostly
Latin text in it is valid UTF-8 byte for byte, so it opens with an invisible
NUL between every character. Saving writes back exactly what was read, so
nothing is lost, but it is not readable here.

Reading both properly, and saving them back in the encoding they came in, is
decided and not yet built ([ADR-0085](https://bpad.prompt-forge.dev/docs)). Until it is,
open such a file in Notepad and save it as UTF-8 there.

The BOM variant exists because some Windows tools still want it.

Choosing the encoding a document already has does nothing, and records no undo
entry.

## Line endings

**Format > Line Ending** offers **LF** and **CRLF**. Choosing one converts the
whole document when you next save it.

**A document is saved in its own line ending**, the one the status bar shows.
Press Enter in a file Notepad wrote and the new lines are saved as CRLF, like
the old ones; paste text with Unix line endings into it and they are saved as
CRLF too ([ADR-0090](https://bpad.prompt-forge.dev/docs)).

The exception is **a file that already mixed both** when you opened it. Which
of its line endings is the stray one is not for this program to guess, so it
is saved exactly as it came, until you choose LF or CRLF from the menu.

A line is `\n` or `\r\n`, and nothing else ([ADR-0029](https://bpad.prompt-forge.dev/docs)).
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
([ADR-0062](https://bpad.prompt-forge.dev/docs)) as part of scoping it down to Notepad
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
([ADR-0081](https://bpad.prompt-forge.dev/docs)). Whether this product should have a
preview is now an open question rather than a technical one. Writing it to
HTML and opening a browser is still ruled out -- this product does not launch
programs.

**No printing.** It is platform work with no cross-platform story yet, and
half of it is not a thing worth shipping.

---

[Back to the help index](../index.md)
