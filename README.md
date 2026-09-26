# BachelorPad+ Lite

**A text editor that stops where it should.**
Windows 10, Windows 11 and Linux. Free software under GPL-3.0-only.

> **Notepad when you want it. More when you need it.**

[**Download**](https://github.com/dboles99/bachelorpluslite/releases/latest) &middot;
[**bpad.prompt-forge.dev**](https://bpad.prompt-forge.dev) &middot;
[**Manual**](docs/generated/manual.md) &middot;
[**Wiki**](https://github.com/dboles99/bachelorpluslite/wiki)

---

BachelorPad+ Lite is Windows 11 Notepad's feature set, plus one thing Notepad
does not have: a note layer that knows which of your documents are about the
same subject.

It opens instantly, it never touches the network, and **it cannot run
anything**.

This started as a much larger product and was deliberately cut down. Ten
crates were removed on purpose, each with a written decision behind it
([ADR-0057](docs/decisions/ADR-0057.md),
[ADR-0059](docs/decisions/ADR-0059.md)--[ADR-0064](docs/decisions/ADR-0064.md)),
until what was left was Notepad plus the one idea worth keeping. The larger
one is still being built; there is [a waitlist](https://bpad.prompt-forge.dev/waitlist.html).

## What works today

**Editing.** Tabs, find and replace, search across a folder, go to line, undo
and redo, overtype on the Insert key, zoom, UTF-8 and UTF-8 with BOM, LF and CRLF, Light/Dark/Organic/Green
themes plus System. A rope underneath, so an edit costs what it changes rather
than what the file weighs.

**The note layer**, which is the part that is not Notepad:

- **semantic filenames** -- `Title_DDMMMYYYY.ext`, proposed from the document
  itself and never applied without you seeing it first
- **related notes** -- what else have I written that is about this
- **duplicate detection** -- automatically on save, and on demand
- **a research report** over your whole collection, which tells you what it
  did not look at
- **per-document privacy profiles** governing what is written down at all

**Recovery.** A journal keeps unsaved work through a crash, a kill or a power
cut, and asks on the next launch -- and keeps it until you say Discard, not
merely until you close the question
([ADR-0084](docs/decisions/ADR-0084.md)).

**Default-editor registration** on Windows and Linux, which never seizes an
association -- it shows you what it would do and hands you the means
([ADR-0012](docs/decisions/ADR-0012.md)).

Format *recognition* for Markdown, YAML, JSON, JSONL/NDJSON, TOML, CSV/TSV,
XML and source code. The operations over them left under
[ADR-0062](docs/decisions/ADR-0062.md).

### What it refuses to do

| | |
| --- | --- |
| **Execute anything** | No scripting, no macros, no cells, no interpreters ([ADR-0057](docs/decisions/ADR-0057.md)) |
| **Reach the network** | No telemetry, no update check, no crash reporting, no AI ([ADR-0006](docs/decisions/ADR-0006.md)) |
| **Launch another program** | Not even a browser for its own help ([ADR-0075](docs/decisions/ADR-0075.md)) |
| **Seize a file association** | It writes a script you read and run ([ADR-0012](docs/decisions/ADR-0012.md)) |
| **Delete out of your profile** | Even files left behind by removed features ([ADR-0070](docs/decisions/ADR-0070.md)) |
| **Rename a file silently** | Every rename is proposed and approved first |

### What it does not have

No macOS build ([ADR-0072](docs/decisions/ADR-0072.md), which says what it
would cost). No printing. No Markdown preview, which is undecided rather than
impossible ([ADR-0081](docs/decisions/ADR-0081.md)). No drag and drop to open
yet -- it is buildable, and does not work on a native Wayland session when it
is (the same ADR). No encryption; that left with
[ADR-0064](docs/decisions/ADR-0064.md). No claim to be accessible yet: F10
reaches the menus and the controls are named for a screen reader, but nobody
has run NVDA or Orca on it, and until somebody has it does not say so
([ADR-0088](docs/decisions/ADR-0088.md)).

Duplicate line, move line and the Insert menu need `--editor-view`, because
they have to know where the caret is and the toolkit's text widget will not
say ([ADR-0018](docs/decisions/ADR-0018.md)).

## Install

**There is no installer, and that is a decision**
([ADR-0067](docs/decisions/ADR-0067.md)). Unpack the archive where you want it
and run it.

```powershell
Expand-Archive bachelorpad-lite-0.9.5-windows-x86_64.zip C:\Apps\BachelorPadLite
C:\Apps\BachelorPadLite\bachelorpad.exe
```

```sh
tar xzf bachelorpad-lite-0.9.5-linux-x86_64.tar.gz
cd bachelorpad-lite-0.9.5-linux-x86_64 && ./bachelorpad
```

> **The archives are unsigned.** Windows SmartScreen will warn. A certificate
> has not been bought and self-signing was **refused outright**, because a
> self-signed certificate is only satisfied once the user installs a root
> certificate they have no reason to trust
> ([ADR-0055](docs/decisions/ADR-0055.md)). `SHA256SUMS.txt` is attached to
> every release and answers tampering in transit, which is the threat a
> certificate answers worst.

Full instructions: [Installation and first launch](docs/user/01-installation.md).

## Build it yourself

```sh
git clone https://github.com/dboles99/bachelorpluslite
cd bachelorpluslite
cargo run --release
```

Linux needs `libfontconfig1-dev libxkbcommon-dev libxcb-shape0-dev
libxcb-xfixes0-dev libxcb1-dev libgl1-mesa-dev libwayland-dev`.

```powershell
./scripts/Install-GitHooks.ps1      # once per clone
./scripts/Invoke-LocalCI.ps1 -Linux # the gate: both legs, ~7 minutes
./scripts/New-Release.ps1 -Linux    # archives for both targets, with checksums
```

**`scripts/Invoke-LocalCI.ps1` is the authoritative gate** and runs from
`pre-commit` and `pre-push`. Every commit in this repository has passed it.
GitHub Actions runs the same stages under the same names for pull requests and
releases ([ADR-0073](docs/decisions/ADR-0073.md)); when the two disagree, the
local one is right.

More: [Building from source](docs/developer/01-building.md),
[Testing and the gate](docs/developer/02-testing.md).

## Documentation

**`docs/` is the one source. Everything else is generated from it**
([ADR-0075](docs/decisions/ADR-0075.md)) -- the manual, the in-app help the
archives carry, the GitHub Wiki, and the website. Three reference pages are
generated from the *code* rather than from prose, because the flag list, the
shortcut list and the menu map already have a home there and a second copy
would be wrong within a session.

| | |
| --- | --- |
| [The manual](docs/generated/manual.md) | Everything, one file |
| [`docs/user/`](docs/user/) | One topic per file |
| [`docs/tutorials/`](docs/tutorials/) | Start-to-finish walkthroughs |
| [`docs/developer/`](docs/developer/) | Building, testing, releasing, architecture |
| Help &rsaquo; User Guide | The same pages, inside the editor, as a document |

## Licence

**GPL-3.0-only.** Not `-or-later`, and the difference is the decision:
[Slint](https://slint.dev)'s grant is to version 3 and no other, so a project
offering "or later" would be offering terms it has not been granted
([ADR-0071](docs/decisions/ADR-0071.md)).

[`LICENSE`](LICENSE) is the text. [`THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md)
lists all 626 crates linked into the binary, generated from the resolve graph
and checked by the gate.

## Contributing

Issues and pull requests are welcome. Read [`CONTRIBUTING.md`](CONTRIBUTING.md)
first -- particularly the part about a **separate BachelorPad+**, which is why
contributions need a sign-off.

Before touching `crates/bp-ui/`, read
[`prompts/rosettas/R011_WIRE_CAPABILITY_INTO_SHELL.md`](prompts/rosettas/R011_WIRE_CAPABILITY_INTO_SHELL.md).
It holds ten traps that have each cost somebody a session.

**For a security problem, do not open a public issue** --
[`SECURITY.md`](SECURITY.md).

## How this project works, if you are curious

[`DECISIONS.md`](DECISIONS.md) is the index of 76 architecture decision
records **and the one home for every lesson learned**. The lessons are the
interesting part: this repository writes down the *shape* of each mistake, not
only the fix.

A sample, all of them real:

- A comment claiming something is not a property of the code. Ask what would
  fail if it stopped being true; if nothing would, the comment is a wish.
- Two claims about the same fact coexist for exactly as long as nothing asks.
- A test that proves two things agree is evidence about two things. Before
  trusting a constant as the one home for a value, **grep for the value**.
- A claim in the record is not a property of the repository. An ADR whose
  reasoning rests on an external fact should name the command that would test
  it.
- Deleting the code that writes a file does not delete the file.
- The dangerous unsupported platform is not the one that fails to compile. It
  is the one that compiles and answers as something else.
