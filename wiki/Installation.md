# Installation and first launch

BachelorPad+ Lite runs on **Windows 10, Windows 11 and Linux** (x86-64).
macOS is not built, and [ADR-0072](https://github.com/dboles99/bachelorpluslite/blob/main/docs/decisions/ADR-0072.md) says why.

**There is no installer, and that is a decision**
([ADR-0067](https://github.com/dboles99/bachelorpluslite/blob/main/docs/decisions/ADR-0067.md)). You unpack an archive wherever you
want the program to live and run it. Nothing is written outside that folder
until you ask for it.

## Download

From [the releases page](https://github.com/dboles99/bachelorpluslite/releases):

| Platform | File |
| --- | --- |
| Windows 10 / 11 | `bachelorpad-lite-<version>-windows-x86_64.zip` |
| Linux | `bachelorpad-lite-<version>-linux-x86_64.tar.gz` |

## The archives are unsigned, and you should know that before you click

Windows SmartScreen will warn you when you download and again when you first
run it. That is deferred code signing
([ADR-0055](https://github.com/dboles99/bachelorpluslite/blob/main/docs/decisions/ADR-0055.md)) -- a certificate has not been bought.
Self-signing was refused outright, because a self-signed certificate is only
satisfied once you install a root certificate you have no reason to trust,
which is a worse thing to ask of somebody than a warning is.

What is offered instead is a checksum, and it is not nothing: it answers
tampering in transit, which is the threat a certificate answers worst.

```sh
sha256sum -c SHA256SUMS.txt
```

```powershell
(Get-FileHash -Algorithm SHA256 .\bachelorpad-lite-0.9.5-windows-x86_64.zip).Hash
```

Compare it with the line in `SHA256SUMS.txt` beside the download.

## Windows

```powershell
Expand-Archive bachelorpad-lite-0.9.5-windows-x86_64.zip C:\Apps\BachelorPadLite
C:\Apps\BachelorPadLite\bachelorpad.exe
```

The archive contains:

| File | What it is |
| --- | --- |
| `bachelorpad.exe` | The program |
| `bpad.cmd` | A short spelling, for the command line |
| `bachelorpad.ico` | The icon Explorer draws for registered file types |
| `io.github.dboles99.BachelorPadPlus.png` | The icon the window shows |
| `app-help/` | The in-app help, opened by Help > User Guide |
| `LICENSE`, `THIRD-PARTY-NOTICES.md` | GPL-3.0-only, and the crates it links |
| `BUILD.txt` | Version, commit, target, and that it is unsigned |
| `README.md` | The project's own front page |

Both icons ship because they are read by different things: Explorer needs a
real `.ico`, and the toolkit that draws the window can decode only PNG and
JPEG ([ADR-0068](https://github.com/dboles99/bachelorpluslite/blob/main/docs/decisions/ADR-0068.md)).

**Desktop and Start Menu shortcuts are made by hand.** With no installer there
is no step that could make them, which is exactly the cost ADR-0067 accepted.

## Linux

```sh
tar xzf bachelorpad-lite-0.9.5-linux-x86_64.tar.gz
cd bachelorpad-lite-0.9.5-linux-x86_64
./bachelorpad
```

`bpad` beside it is a symlink to the same binary. The executable bit and the
symlink are both set inside the archive, so neither needs restoring.

## First launch

The window opens on an empty, unsaved document called *Untitled*. Nothing has
been written anywhere yet, and nothing will be until you save.

Three things worth doing once:

1. **Pick a theme.** View > Theme offers Light, Dark, Organic and Green, plus
   System, which follows the desktop.
2. **Look at Privacy.** The Privacy menu governs what this product records
   about the documents you open -- see [Privacy](Privacy-and-Security). The default
   is deliberately modest, and it is worth knowing what it is rather than
   assuming.
3. **Decide about file associations**, if you want double-clicking a `.txt` to
   open this. File > Set as Default Editor never seizes anything; it shows you
   what it would do and hands you a script to run. See
   [Making it your default editor](Default-Editor).

## Where it puts things

Nothing goes in the folder you unpacked. Everything the product writes about
itself goes in the usual per-user locations:

| | Windows | Linux |
| --- | --- | --- |
| Settings | `%APPDATA%\bachelorpad\config.toml` | `$XDG_CONFIG_HOME/bachelorpad/`, else `~/.config/bachelorpad/` |
| Notes index, recent files | `%LOCALAPPDATA%\bachelorpad\data\` | `$XDG_DATA_HOME/bachelorpad/`, else `~/.local/share/bachelorpad/` |
| Recovery journals | `%LOCALAPPDATA%\bachelorpad\state\` | `$XDG_STATE_HOME/bachelorpad/`, else `~/.local/state/bachelorpad/` |

Settings roam and the rest does not, on purpose: an index, a cache and a log
have no business crossing a network at sign-out.

Help > Diagnostics prints every one of these paths as resolved on your
machine, which is more reliable than this table.

## Removing it

Delete the folder you unpacked. To remove everything, delete the three
directories above as well.

If you registered file types, undo that first through the same File > Set as
Default Editor screen, or through Windows Settings > Default apps. **This
product will not delete anything out of your profile on your behalf**, and
[ADR-0070](https://github.com/dboles99/bachelorpluslite/blob/main/docs/decisions/ADR-0070.md) explains why that is deliberate rather
than an omission.

---

*This page is generated from [`docs/user/01-installation.md`](https://github.com/dboles99/bachelorpluslite/blob/main/docs/user/01-installation.md) and
edits made here will be overwritten. Change the source and open a pull request.*
