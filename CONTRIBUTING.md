# Contributing

Thank you for looking. Issues and pull requests are both welcome.

**Read this section first.** It is the one that will surprise you.

---

## There is a second, proprietary product, and it changes what a PR means

BachelorPad+ Lite is the reduced edition of **BachelorPad+**, which is being
built as a commercial product. Daniel Boles holds copyright in all of this
code, so publishing it under GPL-3.0-only forecloses nothing about that.

**A contribution from anybody else is different.** Code you write and we merge
is yours, licensed to this project under GPL-3.0-only -- and moving it into a
proprietary build would need your permission, separately and specifically.

So every pull request needs one of two things, and **you choose which**:

### Option 1 -- sign off under the DCO (the default)

Add a `Signed-off-by` line to each commit:

```sh
git commit -s -m "fix: the find bar keeps the caret when the document scrolls"
```

That asserts the [Developer Certificate of Origin](https://developercertificate.org/):
you wrote it, or you have the right to submit it, and you are contributing it
under this project's licence.

**Your contribution stays GPL-3.0-only.** It will not appear in a proprietary
BachelorPad+ unless you are asked and agree.

### Option 2 -- assign copyright, if you would rather

If you are happy for your contribution to appear in the commercial product
too, say so in the pull request in as many words. You will be asked to confirm
it explicitly; a checkbox on a template is not consent for something like
this.

**Option 1 is the default and nobody will push you towards option 2.** This is
written down here rather than raised after a PR is open, because being told
about a licensing condition *after* doing the work is a bad experience and an
avoidable one.

---

## Before you write anything

**Open an issue first for anything larger than a bug fix.** This project
declines features on purpose and writes down why -- eleven menu rows were
deleted rather than built, and five whole feature areas were removed after
they worked. A pull request adding something the record has already declined
is a waste of your evening, and the record is long.

The three refusals most likely to catch a well-meant patch:

| Do not add | Because |
| --- | --- |
| Anything that **executes** | [ADR-0057](docs/decisions/ADR-0057.md). No scripting, no macros, no plugin host, no "just shell out to X" |
| Anything that **reaches the network** | [ADR-0006](docs/decisions/ADR-0006.md). Including an update check and a crash reporter |
| Anything that **launches a program** | Not even a browser for a help link ([ADR-0075](docs/decisions/ADR-0075.md)) |

**A new dependency needs saying out loud with the reason.** Dependency choices
are ADR material here, and it must pass `cargo deny check licenses` -- the
allow-list in `deny.toml` is the GPL-3.0-only compatibility audit written
down.

**File renames and moves need explicit approval.**

## Setting up

```sh
git clone https://github.com/dboles99/bachelorpluslite
cd bachelorpluslite
cargo build
```

```powershell
./scripts/Install-GitHooks.ps1
```

Linux needs `libfontconfig1-dev libxkbcommon-dev libxcb-shape0-dev
libxcb-xfixes0-dev libxcb1-dev libgl1-mesa-dev libwayland-dev`.
PowerShell 7 (`pwsh`) runs the gate and the build scripts on both platforms.

**Before touching `crates/bp-ui/`, read
[`prompts/rosettas/R011_WIRE_CAPABILITY_INTO_SHELL.md`](prompts/rosettas/R011_WIRE_CAPABILITY_INTO_SHELL.md).**
The traps, each of which has cost somebody a session -- action-id ranges, the
two editor views, Slint `if` scoping, and the one about a menu callback that
cannot take the keyboard focus, which shipped a defect that typed a passphrase
into the open document.

## The gate

```powershell
./scripts/Invoke-LocalCI.ps1 -Linux    # both legs, ~7 minutes
```

Green on both legs before you open a pull request. CI runs the same stages
under the same names ([ADR-0073](docs/decisions/ADR-0073.md)).

Three rules, each of which was learnt the hard way:

- **`cargo fmt --all` before you start it.** Always.
- **Background it and do not edit files while it runs.** The Linux leg re-runs
  `cargo fmt --check` from scratch.
- **Never pipe it through `tail` or `head`.** A failure's detail is *above* the
  summary. It happened once and a failing test could not be named afterwards.

## House style

This repository has a strong and consistent voice. Match it -- it is most of
what makes the record worth reading.

- **Comments explain *why*, not what.** A comment restating the code is worse
  than no comment.
- **Test names are full sentences:**
  `fn a_no_op_must_not_create_an_undo_entry()`.
- **British spelling in prose:** behaviour, recognised, initialise.
- `--` rather than an em dash inside code comments.
- **Every module has a `//!` header** saying what it owns and what it
  deliberately does not.
- **`bp-ui` connects; it does not decide.** An `if` about what a feature
  *should do* belongs in a library crate, where it can be tested without a
  window.

### The question this project asks about every claim

> **What would fail if this stopped being true?**

If the answer is *nothing*, the claim is a wish. That is trap 3, and most of
the defects in [`DECISIONS.md`](DECISIONS.md) are it. A comment saying a
function is the one home for a value, with six other places spelling that
value out, passed every test for months.

**So: when you add a claim, add the thing that would fail.**

## Documentation

**`docs/` is the source; `docs/generated/`, `app-help/` and `wiki/` are
generated from it** ([ADR-0075](docs/decisions/ADR-0075.md)). Do not edit the
generated trees -- the gate will fail and your edit will be reverted by the
next run.

```powershell
./scripts/Build-Docs.ps1
./scripts/Build-AppHelp.ps1
./scripts/Sync-GitHubWiki.ps1
```

Three reference pages come from **code**, not prose: the flag list from
`bp_config::cli::FLAGS`, the shortcuts from `bp_ui::dispatch::SHORTCUTS`, and
the menus from `docs/product/MENU_MAP.md`. Changing any of those three makes a
documentation page stale, which is why the docs workflow watches them.

## Commits and pull requests

Conventional commits: `feat:`, `fix:`, `docs:`, `test:`, `refactor:`,
`chore:`. The subject is a sentence about behaviour, not about files.

**Whoever finishes an item updates the record as part of it.** A fix that
lands without the record moving is how every stale sentence in this repository
got there. If your change makes a decision, write the ADR --
[`docs/developer/04-architecture-and-decisions.md`](docs/developer/04-architecture-and-decisions.md)
says how, and the **Lesson** section at the end is the part that gets read.

Please include, in the pull request:

1. what changed and why;
2. what you ran, and what it said;
3. anything you checked **by hand** -- five defects have lived in the seam
   between this product and the toolkit, and no test in this repository can
   see them.

## Reporting a bug

[Open an issue](https://github.com/dboles99/bachelorpluslite/issues) with the
output of **Help &rsaquo; Diagnostics**, what you did, and what happened
instead.

**For a security vulnerability, do not open a public issue.** See
[`SECURITY.md`](SECURITY.md).

## Code of conduct

Be decent. Disagree about the work rather than about the person. Anything that
would make somebody not want to open the repository again is out of place
here, and will be dealt with by removing it.
