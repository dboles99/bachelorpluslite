# Encoding, line endings and file formats

## Encoding

**Format > Encoding** offers **UTF-8** and **UTF-8 with BOM**.

A document can also be opened in **UTF-16**, with a byte-order mark, and is
saved back that way.

**A file in a legacy code page is refused today** -- Windows-1252, Shift-JIS,
GBK, and UTF-16 with no byte-order mark all stop at *not valid UTF-8*. Notepad
opens them. Reading them, and saving them back in the encoding they came in,
is decided and not yet built ([ADR-0085](../decisions/ADR-0085.md)). Until it
is, open such a file in Notepad and save it as UTF-8 there.

The BOM variant exists because some Windows tools still want it.

Choosing the encoding a document already has does nothing, and records no undo
entry.

## Line endings

**Format > Line Ending** offers **LF** and **CRLF**.

A line is `\n` or `\r\n`, and nothing else ([ADR-0029](../decisions/ADR-0029.md)).
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
([ADR-0062](../decisions/ADR-0062.md)) as part of scoping it down to Notepad
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
([ADR-0081](../decisions/ADR-0081.md)). Whether this product should have a
preview is now an open question rather than a technical one. Writing it to
HTML and opening a browser is still ruled out -- this product does not launch
programs.

**No printing.** It is platform work with no cross-platform story yet, and
half of it is not a thing worth shipping.
