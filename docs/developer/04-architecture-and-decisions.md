# Architecture and decisions

**This page is a map to the documents that own these facts, not a copy of
them.** One home per kind of fact is the rule this repository runs on, and a
fact repeated in four files is a fact that will be updated in three.

## Where each fact lives

| Document | Owns |
| --- | --- |
| [`docs/architecture/ARCHITECTURE.md`](../architecture/ARCHITECTURE.md) | Crates, modules, sizes, seams |
| [`docs/product/MENU_MAP.md`](../product/MENU_MAP.md) | Every menu row and its state |
| [`DECISIONS.md`](../../DECISIONS.md) | The ADR index, **and every lesson learned** |
| [`docs/decisions/ADR-*.md`](../decisions/) | One decision each, with its reasoning |
| [`ROADMAP.md`](../../ROADMAP.md) | Phase status, and what is blocked on what |
| [`docs/governance/`](../governance/) | The gate, the definition of done, the work model |
| [`README.md`](../../README.md) | What works today, for a reader who has never seen the project |
| [`specs.md`](../../specs.md) | The original specification. **Historical** -- several sections describe things since removed |

## The shape, in five sentences

A Slint front end (`bp-ui`) over fifteen library crates. `bp-ui` connects and
displays; it never decides what a feature should do. `bp-platform` isolates
everything the two operating systems disagree about, and takes the platform as
a **parameter** rather than a `cfg`, so each leg of the gate executes the
other's rules. `bp-editor` holds a rope, so an edit costs what it changes and
scrolling costs what is on screen. Nothing anywhere reaches the network or
runs a program.

## The traps, named

`CLAUDE.md` names them and `DECISIONS.md` explains each. In short:

1. A function reading the real profile directory means `cargo test` writes
   there.
2. Code taking a `Platform` parameter must not let `std::path` answer for it.
3. **A claim in a comment is not a property of the code.** What would fail if
   this stopped being true? If nothing, it is a wish.
4. **Two claims about the same function coexist for as long as nothing asks.**
5. "Unique" must mean no *earlier* run either.
6. **A claim in the record is not a property of the repository.** A question
   that has waited several sessions should have its premise checked before it
   is asked again.
7. A round trip through one build says nothing about another build.

Trap 6 is the one this project has been caught by twice, most recently by
[ADR-0073](../decisions/ADR-0073.md): an accepted ADR rested on *GitHub
Actions is not available to this project*, and one API call disproved it.

**An ADR whose reasoning rests on an external fact should name the command
that would test it.**

## The decisions a newcomer meets first

| | |
| --- | --- |
| [ADR-0001](../decisions/ADR-0001.md) | Windows 10, Windows 11 and Linux are equal targets |
| [ADR-0006](../decisions/ADR-0006.md) | Core editing never depends on cloud or AI services |
| [ADR-0016](../decisions/ADR-0016.md), [ADR-0073](../decisions/ADR-0073.md) | The local gate is authoritative; hosted CI judges pull requests |
| [ADR-0018](../decisions/ADR-0018.md) | Two editor surfaces, and why the custom one is opt-in |
| [ADR-0055](../decisions/ADR-0055.md) | Unsigned releases, and self-signing refused outright |
| [ADR-0057](../decisions/ADR-0057.md) | This product executes nothing |
| [ADR-0059](../decisions/ADR-0059.md)--[ADR-0064](../decisions/ADR-0064.md) | The reduction: what was removed to make this Lite, and why |
| [ADR-0067](../decisions/ADR-0067.md) | No installer |
| [ADR-0071](../decisions/ADR-0071.md) | GPL-3.0-only, and why not `-or-later` |
| [ADR-0072](../decisions/ADR-0072.md) | macOS declined, with what it would cost |
| [ADR-0074](../decisions/ADR-0074.md) | The product is BachelorPad+ Lite |
| [ADR-0075](../decisions/ADR-0075.md) | One documentation source, four destinations |
| [ADR-0076](../decisions/ADR-0076.md) | The website: what it may collect, and why it does not follow af-site on analytics |
| [ADR-0077](../decisions/ADR-0077.md) | Reporting a problem composes a document, because this product opens no browser |

## Writing an ADR

One decision per file, numbered in sequence, with **Status**, **Date** and the
ADRs it depends on. Context, Decision, Consequences, and -- the part this
project actually reads -- **The lesson**: the general form of the mistake, so
the next person recognises its shape somewhere else.

Add a row to `DECISIONS.md`, which is the index and the one home for lessons.

**A decision that removes a planned feature is a real result** and needs no
permission. The ADR is the point: the reasoning is worth more than the row
would have been.
