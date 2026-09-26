# Building from source

## What you need

| | |
| --- | --- |
| Rust | stable, per `rust-toolchain.toml` (rustfmt and clippy come with it) |
| Windows | nothing else |
| Linux | `libfontconfig1-dev libxkbcommon-dev libxcb-shape0-dev libxcb-xfixes0-dev libxcb1-dev libgl1-mesa-dev libwayland-dev` |
| PowerShell 7 | for the gate and the build scripts. Cross-platform; `pwsh` on Linux |
| Optional | `cargo-nextest` (about 2.5x faster tests), `cargo-deny` (the licence stage) |

The exact Linux package list is in `.github/workflows/ci.yml`, named
individually rather than through a meta-package so that a failure says which
one went missing.

## Build and run

```sh
git clone https://github.com/dboles99/bachelorpluslite
cd bachelorpluslite
cargo run --release
```

```sh
cargo run --release -- --help          # every flag it accepts
cargo run --release -- --editor-view   # the custom editor surface
cargo run --release -- --self-check    # load, print a marker, exit
```

## Install the hooks, once per clone

```powershell
./scripts/Install-GitHooks.ps1
```

This wires the gate into `pre-commit` and `pre-push`. It verifies the
executable bit, because a hook without it is skipped silently -- the hardest
failure mode to notice.

## The workspace

Fifteen crates plus the binary. `docs/architecture/ARCHITECTURE.md` owns the
map -- what each crate is for, its size, and the seams between them -- and
this page does not restate it.

The shape worth knowing before you change anything:

- **`bp-ui` connects; it does not decide.** If you are writing an `if` about
  *what a feature should do* inside `bp-ui`, that logic belongs in a library
  crate where it can be tested without a window.
- **`bp-platform` takes the platform as a parameter**, not as a `cfg`. That is
  what lets the Linux leg of the gate execute the Windows rules and vice
  versa. The compile target is consulted in exactly one place,
  `Platform::HOST`.
- **`fuzz/` is its own workspace**, so `--workspace` walks past it. It has its
  own gate stages.

**Before touching `crates/bp-ui/`, read
`prompts/rosettas/R011_WIRE_CAPABILITY_INTO_SHELL.md`.** It holds the traps
that have each cost somebody a session -- action-id ranges, the two editor
views, Slint `if` scoping, and the one about a menu callback that cannot take
the keyboard focus, which shipped a defect that typed a passphrase into the
open document.

## Panic lints

`unwrap_used`, `panic`, `todo` and `unimplemented` are warnings at the
workspace and errors under the gate's `-D warnings`. They were all at zero
before they were switched on.

`expect_used`, `unreachable` and `indexing_slicing` are deliberately not
enabled; each is a change to the code rather than to the policy and wants its
own pass.

## Adding a dependency

**Say so and give the reason.** Dependency choices are ADR material here, and
a new one has to pass `cargo deny check licenses` -- the allow-list in
`deny.toml` is the GPL-3.0-only compatibility audit written down
([ADR-0071](https://github.com/dboles99/bachelorpluslite/blob/main/docs/decisions/ADR-0071.md)).

If it changes the graph, regenerate the notices:

```powershell
./scripts/New-ThirdPartyNotices.ps1
```

The gate checks this, so a stale notices file fails rather than shipping.

---

*This page is generated from [`docs/developer/01-building.md`](https://github.com/dboles99/bachelorpluslite/blob/main/docs/developer/01-building.md) and
edits made here will be overwritten. Change the source and open a pull request.*
