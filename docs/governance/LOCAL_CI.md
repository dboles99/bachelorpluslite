# Local CI

BachelorPad+ has no hosted CI. GitHub Actions is unavailable to this project,
so the gate is local and it is the only gate. See ADR-0016.

## Setup, once per clone

```powershell
./scripts/Install-GitHooks.ps1
```

That copies `scripts/hooks/*` into `.git/hooks/`. Hooks cannot be tracked by
git, so the tracked copies are the source of truth and the installer puts them
in place.

Re-run it after pulling a change to `scripts/hooks/`.

## Running it by hand

```powershell
./scripts/Invoke-LocalCI.ps1                       # full gate, Windows
./scripts/Invoke-LocalCI.ps1 -Quick                # what pre-commit runs
./scripts/Invoke-LocalCI.ps1 -Linux -IncludeSpikes # everything, both platforms
```

| Stage | What it runs |
| --- | --- |
| fmt | `cargo fmt --all -- --check` |
| check (locked) | `cargo check --workspace --all-targets --locked` |
| clippy | `cargo clippy --workspace --all-targets -- -D warnings` |
| test | `cargo test --workspace` |
| spikes | fmt and clippy over each standalone workspace in `spikes/` |
| linux (wsl) | fmt, clippy and test inside WSL |

Every stage runs even after one fails, so a single run reports everything
rather than making you fix problems one at a time. The exit code is non-zero
if any stage failed.

`-Quick` skips the locked dependency resolve, which is the slowest part of a
cold run and cannot regress from an edit that does not touch a manifest.
`pre-push` runs the full gate.

## What runs when

| Hook | Gate |
| --- | --- |
| `pre-commit` | `Invoke-LocalCI.ps1 -Quick` |
| `pre-push` | `Invoke-LocalCI.ps1` |

Both can be bypassed for a deliberate work-in-progress commit:

```sh
BPAD_SKIP_CI=1 git commit -m "wip: ..."
```

The escape hatch is deliberate. A gate with no bypass gets disabled wholesale
the first time it is wrong, and then nothing is checked at all.

## The Linux leg

BP-ADR-0001 makes Linux a first-class target, and nothing else in this setup
would catch a Windows-only regression. `-Linux` runs the same gate inside WSL.

It needs a Rust toolchain **inside the distro** — the Windows toolchain is not
visible there. As of 2026-08-17 the distro does not have one, so the stage
reports as `skipped` rather than silently passing. To enable it:

```powershell
wsl -d Ubuntu-24.04 -- bash -lc "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y"
wsl -d Ubuntu-24.04 -- bash -lc "sudo apt-get update && sudo apt-get install -y build-essential pkg-config"
```

The run sets `CARGO_TARGET_DIR` to a WSL-native path. Sharing `target/` with
Windows would make each platform invalidate the other's artifacts on every
switch, and building onto `/mnt/g` is slow enough already.

## The dormant workflow

`.github/workflows/ci.yml` is kept and kept correct, set to
`workflow_dispatch` only so that pushes do not queue runs that cannot execute.
It exists so hosted CI can be switched back on without rebuilding it.

**Keep it in step with `Invoke-LocalCI.ps1`.** Two gates that disagree are
worse than one.

## A note on this machine

`core.hooksPath` is set globally here, which normally makes git ignore every
repo-local `.git/hooks` script. The global hooks directory handles that with
pass-through stubs that exec the repo's own hook if it is executable — so
repo hooks work, and the global commit-msg hook keeps working too.

Two consequences:

- The installer must not set a repo-local `core.hooksPath`. That would
  override the global path and disable the commit-msg hook.
- The executable bit matters. The stubs test `-x` before exec'ing, so a hook
  without it is skipped **silently**. `Install-GitHooks.ps1` verifies this and
  warns; do not ignore that warning.
