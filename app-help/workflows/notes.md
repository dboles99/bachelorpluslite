# The note layer

This is the part that is not Notepad, and the only part. Everything on this
page is optional, local, and off the network.

The product keeps a small index of documents you have opened -- their paths,
their titles, and keywords drawn from their text. Nothing leaves your machine,
and **what is recorded at all is governed by your privacy profile**; see
[Privacy](../troubleshooting/privacy.md).

| | Windows | Linux |
| --- | --- | --- |
| The index | `%LOCALAPPDATA%\bachelorpad\data\organize.sqlite` | `$XDG_DATA_HOME/bachelorpad/organize.sqlite`, else `~/.local/share/bachelorpad/organize.sqlite` |

## Note

About the document in front of you.

| Row | What it tells you |
| --- | --- |
| **Suggest Title** | The title found in the document, and where it was found |
| **Semantic Rename...** | A proposed `Title_DDMMMYYYY.ext` filename, for you to approve |
| **Summary** | A short summary drawn from the text |
| **Keywords** | The terms this document is indexed under, read from the text in front of you |
| **Outline** | Its headings, as a structure |
| **Document Statistics** | Words, lines and characters, computed on the click and never on the typing path |
| **Tags** | What the store *recorded* for this document. The sibling of Keywords -- that reads the text, this reads the record, and they disagree exactly when there are unsaved edits |
| **Recovery Checkpoints** | What the journal holds, and **before the count**, whether this profile writes one at all |

## Organize

About this document's relationship to the others.

**Related Notes** opens a collapsible panel of documents sharing tags with this
one, most related first. Clicking a row opens it and takes you to the relevant place.

**Duplicate Detection** works two ways: a notice when you save something that
closely resembles a document already indexed, and a menu row that asks
on demand.

**Suggested Folder** answers a narrower question -- where do documents sharing
this one's tags already live? It counts what you have actually filed rather
than imposing a scheme on you, and **it never moves a file**. File > Save a
Copy is where that already lives.

## Research

**Research Report** reads the index and writes a report about your notes as a
whole: dominant themes, under-connected documents, what the store holds.

**It says what it did not look at.** The report is built from the 500 most
recently seen documents, and if you have more than that the report says so, in
the report, where you will read it. Each section names at most five items and
each item at most a few documents, and it says that too.

That is deliberate and it is worth knowing why. An earlier version scanned 500
documents and mentioned it nowhere a reader could see, so a report about "your
notes" was a report about some of them, presented as if it were about all of
them. Being inspectable in the source is not the same as being honest to the
person reading the output.

**Open Questions** and **What the Store Holds** are the other two rows.
The first finds every question the document asks, at the line it begins on,
skipping anything inside a code fence. The second reports the store's own
contents -- and states, in the report, that it never holds the text of a
document.

## Turning it off

Set a privacy profile that records nothing. The menu rows stay and report that
there is nothing to report, rather than disappearing -- a greyed row with its
reason in its own label is more use than a row that is not there.

Deleting `organize.sqlite` empties the index. The product will make a new one
if your profile permits it.

---

[Back to the help index](../index.md)
