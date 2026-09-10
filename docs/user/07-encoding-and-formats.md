# Encoding, line endings and file formats

## Encoding

**Format > Encoding** offers **UTF-8** and **UTF-8 with BOM**.

That is the whole list, and it is a decision rather than an unfinished one.
Legacy code pages are a way to lose text: a document saved as one and opened
as another is silently wrong, and nothing in the file says which it was. UTF-8
is what everything else on both supported platforms now writes.

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

**No Markdown preview.** The toolkit has no rich-text item -- no styled runs,
no spans -- so inline formatting could not be rendered at all, and a preview
that dropped bold and links would be worse than none
([ADR-0048](../decisions/ADR-0048.md)). Writing it to HTML and opening a
browser was the other option, and this product does not launch programs.

It is re-checked on every toolkit upgrade.

**No printing.** It is platform work with no cross-platform story yet, and
half of it is not a thing worth shipping.
