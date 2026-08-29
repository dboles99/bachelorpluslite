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
| launch | Runs `bachelorpad --self-check` and asserts it starts |
| log hygiene | Fails on any `tracing::` call that may carry document content or secrets |
| fuzz | fmt, clippy and test over the standalone workspace in `fuzz/` |
| spikes | fmt and clippy over each standalone workspace in `spikes/` |
| linux (wsl) | fmt, clippy and test inside WSL |
| linux fuzz (wsl) | The same for `fuzz/`, inside WSL |

Every stage runs even after one fails, so a single run reports everything
rather than making you fix problems one at a time. The exit code is non-zero
if any stage failed.

The `launch` stage exists because neither `cargo build` nor `cargo test` ever
loads the linked executable. Enabling one rfd feature — which embedded a
side-by-side manifest — produced a binary that failed with
`STATUS_ENTRYPOINT_NOT_FOUND` before `main`, with the whole gate green.
Anything that changes link-time features, manifests or linked libraries needs
this stage.

The `log hygiene` stage enforces ADR-0011's rule that document content,
clipboard data, passphrases and key material never reach logs. It greps for
`tracing::` macros mentioning content-bearing identifiers and fails the build
on a hit. It is crude and will need widening as more crates land, but it fails
closed and runs on every commit — which a review convention does not. Log
*about* a document (path, size), never what it contains.

The `clippy` stage carries more than clippy's defaults.
`[workspace.lints.clippy]` in the root `Cargo.toml` sets `unwrap_used`,
`panic`, `todo` and `unimplemented` to `warn`, which `-D warnings` turns into
build failures here while leaving them as warnings in an editor. All four were
already at zero in production library code when they were switched on -- the
discipline existed and was held by whoever was writing remembering it, which
is not a mechanism. `clippy.toml` exempts test code, because `unwrap()` in a
fixture is the assertion; where clippy's own "is this a test" heuristic cannot
tell (a bench target, a helper that is not a `#[test]` function), the file
carries a local allow with its reason.

The `fuzz` stage exists because `fuzz/` declares its own `[workspace]` table,
so every `--workspace` command in every other stage walks straight past it.
Five harnesses feeding hostile input to `bp-crypto`, `bp-data`, `bp-files`,
`bp-formats` and `bp-notebook` sat outside the gate that validates everything
else, formatted, linted and run by nobody, while ROADMAP called phase 19
*Started* on the strength of them. **Three run in it today**: ADR-0057 deleted
`bp-notebook` and ADR-0062 deleted `bp-data` along with the YAML target, and a
harness outlives its subject by nothing.

It is deliberately **not** behind `-IncludeSpikes`: a spike is a prototype the
product does not depend on, and these are tests of shipped crates against
input designed to break them.

It runs on both legs, for the reason the Linux leg exists at all. One-leg
testing hides defects and this repository has been caught by that twice --
`PathBuf::join` standing in for `bp_platform::paths::join`, and a device-name
rule whose judgement took a platform parameter while its split used
`std::path`. `fuzz/tests/files.rs` drives `bp-files`, where the second one
lived. The Linux run uses its own `CARGO_TARGET_DIR`, because two workspaces
sharing one target directory evict each other's artefacts and every run
becomes a cold build of whichever went second.

It costs about two and a half minutes per leg, which is most of why it is in
the full run and not in `-Quick`. Where one harness is too expensive to gate
on, the answer is the one `fuzz/tests/envelope.rs` already used: `#[ignore]`
that test with the reason on it and leave the rest running.

`-Quick` skips the locked dependency resolve and the fuzz workspace. The
first is the slowest part of a cold run and cannot regress from an edit that
does not touch a manifest; the second is simply slow, and `pre-push` runs the
full gate before anything leaves the machine.

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

**Working as of 2026-08-17** on Ubuntu-24.04 — the workspace builds, clippies
and tests clean there.

It needs a Rust toolchain **inside the distro**; the Windows toolchain is not
visible there. If you are setting up a fresh distro:

```powershell
# apt needs root. WSL grants that without a password, which is why -u root
# works where plain sudo would sit waiting for one.
wsl -d Ubuntu-24.04 -u root -- bash -c "apt-get update && apt-get install -y build-essential pkg-config libfontconfig1-dev libxkbcommon-dev libxcb1-dev libgl1-mesa-dev libwayland-dev libssl-dev"
wsl -d Ubuntu-24.04 -- bash -c "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal -c rustfmt,clippy"

# Only if you want to *run* the GUI under WSLg, not just build it:
wsl -d Ubuntu-24.04 -u root -- bash -c "apt-get install -y libxkbcommon-x11-0 libxcb-cursor0"
```

Two traps this leg had to work around, both worth knowing:

- **PATH.** WSL inherits the Windows PATH through interop, which already
  contains the *Windows* `~/.cargo/bin`. A rustup install into the distro
  lands in the distro's `$HOME/.cargo/bin` and does not reach a
  non-interactive shell at all. The script exports it explicitly; without
  that the leg either fails to find cargo or finds the wrong one.
- **Target directory.** The run sets `CARGO_TARGET_DIR` to a WSL-native path.
  Sharing `target/` with Windows would make each platform invalidate the
  other's artifacts on every switch, and building onto `/mnt/g` is slow
  enough already.

Timing measured under WSLg is *not* representative — the compositor adds
overhead a native desktop does not. Use it to verify correctness, not
performance.

## The dormant workflow

`.github/workflows/ci.yml` is kept and kept correct, set to
`workflow_dispatch` only so that pushes do not queue runs that cannot execute.
It exists so hosted CI can be switched back on without rebuilding it.

Actions is also **disabled at the repository level**, because editing the
trigger only protects branches carrying the edit — pushing an older branch
still queued a run. To re-enable hosted CI later you need both:

```bash
gh api -X PUT repos/:owner/:repo/actions/permissions -F enabled=true
# then restore `on: push` / `on: pull_request` in the workflow
```

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
