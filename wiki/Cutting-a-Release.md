# Cutting a release

Two ways. The scripts are the definition; the workflow runs them on a clean
machine ([ADR-0073](https://github.com/dboles99/bachelorpluslite/blob/main/docs/decisions/ADR-0073.md)) and is the only way that
builds every package.

## By hand

```powershell
./scripts/Invoke-LocalCI.ps1 -Linux    # green on both legs first
./scripts/New-Release.ps1 -Linux
```

Into `artifacts/releases/`: a `.zip`, a `.tar.gz` and `SHA256SUMS.txt` --
and the Windows installer too, if Inno Setup 6 is installed
(`winget install JRSoftware.InnoSetup`); without it the script says so and
skips it. The Linux packages are built by the workflow only: they need
`dpkg-deb`, `rpmbuild` and a network to fetch the AppImage tool.

`-Linux` builds the Linux target through WSL. It is a switch rather than the
default only because it needs a Rust toolchain inside the distro. **A release
without it is half a release** -- ADR-0001 makes Linux an equal target.

The script refuses a dirty tree. An archive built from a tree nobody can check
out again is not a release, it is a copy, and the difference only shows up when
somebody asks what is in it.

## By tag

```sh
git tag v0.9.5
git push origin v0.9.5
```

`.github/workflows/release.yml` builds both targets on clean runners, checks
the tag against what the binary reports, and **creates a draft** holding every
download ([ADR-0093](https://github.com/dboles99/bachelorpluslite/blob/main/docs/decisions/ADR-0093.md)):

| | Built by |
| --- | --- |
| `-windows-x86_64-setup.exe` | `New-Release.ps1 -RequireInstaller`, Inno Setup on `packaging/windows/bachelorpad-lite.iss` |
| `-windows-x86_64.zip` | `New-Release.ps1` |
| `-linux-x86_64.tar.gz` | `scripts/release-linux.sh`, on `ubuntu-22.04` for the glibc floor |
| `_amd64.deb`, `-1.x86_64.rpm`, `-x86_64.AppImage`, `PKGBUILD` | `packaging/linux/build-packages.sh`, from that `.tar.gz` |
| `SHA256SUMS.txt` | the publish job, over all of them |

Run it with **`dry_run`** from the Actions tab first, against the branch: it
builds everything and publishes nothing.

**Publishing is a person's job.** It is the act that changes what a stranger
downloads, which is the same gesture ADR-0053 keeps for clicking merge.

## What the version is, and where it comes from

`[workspace.package] version` in `Cargo.toml`, and **nowhere else**.

Both release scripts ask the binary rather than reading a manifest:

```
BachelorPad+ Lite 0.9.5
Text editor for notes and logs
GPL-3.0-only
```

Three lines, and the scripts take the name from the first, the version from
the first, and the licence from the third. A manifest can be edited without a
rebuild; the binary cannot. So the name on the archive is what the executable
inside it will tell a user ([ADR-0054](https://github.com/dboles99/bachelorpluslite/blob/main/docs/decisions/ADR-0054.md)), and
`BUILD.txt` cannot disagree with the program it sits beside.

That last property is a fix rather than a flourish. `BUILD.txt` carried the
product name as a literal in *both* scripts, and the two literals said
different things ([ADR-0074](https://github.com/dboles99/bachelorpluslite/blob/main/docs/decisions/ADR-0074.md)).

## What goes in an archive

`bachelorpad` (`.exe`), the short spelling (`bpad.cmd` or a `bpad` symlink),
both icons on Windows and the PNG on Linux, `app-help/`, `README.md`,
`LICENSE`, `THIRD-PARTY-NOTICES.md`, and `BUILD.txt`.

`LICENSE` is GPL-3.0-only and `THIRD-PARTY-NOTICES.md` lists the 626 crates
linked into the binary. Both are obligations rather than courtesies
([ADR-0071](https://github.com/dboles99/bachelorpluslite/blob/main/docs/decisions/ADR-0071.md)).

## Two things a release does not do

**The Flatpak** is a manifest in `packaging/flatpak/`; Flathub builds it,
from a tag, after a pull request to `flathub/flathub`, which wants
`cargo-sources.json` generated beside it and a screenshot. **The AUR** takes
the rendered `PKGBUILD` and a `.SRCINFO`, pushed from an AUR account. Both are
a person's.

## They are unsigned, and every run says so

[ADR-0055](https://github.com/dboles99/bachelorpluslite/blob/main/docs/decisions/ADR-0055.md) deferred code signing and **refused
self-signing outright** -- a self-signed Authenticode certificate is only
satisfied once the user installs a root certificate they have no reason to
trust. 1.0.0 ships unsigned ([ADR-0094](https://github.com/dboles99/bachelorpluslite/blob/main/docs/decisions/ADR-0094.md)); SignPath
or the Store is a 1.x release's.

`New-Release.ps1` prints it on every run, in the same words, and the release
notes repeat it. A release that stays quiet about it is asking the user to
discover SmartScreen themselves.

`Add-Signature` at the bottom of `New-Release.ps1` exists to be filled in
rather than written from scratch. It throws today.

## Before tagging

1. `Invoke-LocalCI.ps1 -Linux`, green on both legs.
2. `New-ThirdPartyNotices.ps1` if the dependency graph moved.
3. The manual checklist in `project/NEXT_SESSION.md` section 3. **Four
   user-visible defects have shipped past a green gate**, and every one fell
   out of building an archive, extracting it, installing it and clicking
   things.
4. Update `README.md`, `ROADMAP.md` and `DECISIONS.md`. Whoever finishes the
   work updates the record as part of it.

---

*This page is generated from [`docs/developer/03-releasing.md`](https://github.com/dboles99/bachelorpluslite/blob/main/docs/developer/03-releasing.md) and
edits made here will be overwritten. Change the source and open a pull request.*
