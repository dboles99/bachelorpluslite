# Installation and first launch

BachelorPad+ Lite runs on **Windows 10, Windows 11 and Linux** (x86-64).
macOS is not built, and [ADR-0072](https://bpad.prompt-forge.dev/docs) says why.

There is an **installer for Windows**, a **package for each family of
Linux**, and a plain archive for either, for anyone who would rather unpack a
folder and run it ([ADR-0093](https://bpad.prompt-forge.dev/docs)). Every one of them
holds the same program. Wherever it is installed, what the product keeps about
itself goes in your user profile, and
[Where it puts things](#where-it-puts-things) lists every directory.

## What it needs

**Windows 10 or 11, x86-64, and nothing else.** The C runtime is built into
the executable, so there is no Visual C++ redistributable to install first.

**Linux, x86-64, with glibc 2.35 or later**: Ubuntu 22.04 and later, Debian
12 and later, Linux Mint 21 and later, Fedora 36 and later
([ADR-0087](https://bpad.prompt-forge.dev/docs)). The release is built on the oldest of
those and refused if it would need anything newer, so the list is checked
rather than hoped. RHEL 8 and 9 and Debian 11 are older than that and are not
supported.

It also loads the libraries any desktop session already has -- fontconfig,
xkbcommon (with its X11 half), and Wayland or X11. The `.deb` and `.rpm`
declare them, so your package manager fetches anything missing. For the
AppImage or the archive on a minimal install, on Debian or Ubuntu:

```sh
sudo apt install libfontconfig1 libxkbcommon0 libxkbcommon-x11-0 libwayland-client0 libx11-xcb1 libxcursor1 libxi6
```

## Download

From [the releases page](https://github.com/dboles99/bachelorpluslite/releases):

| You have | Download |
| --- | --- |
| **Windows 10 or 11** | `bachelorpad-lite-<version>-windows-x86_64-setup.exe` |
| Windows, no installer | `bachelorpad-lite-<version>-windows-x86_64.zip` |
| **Ubuntu, Debian, Mint, Pop!_OS** | `bachelorpad-lite_<version>_amd64.deb` |
| **Fedora, openSUSE** | `bachelorpad-lite-<version>-1.x86_64.rpm` |
| Any other Linux | `bachelorpad-lite-<version>-x86_64.AppImage` |
| Linux, no package | `bachelorpad-lite-<version>-linux-x86_64.tar.gz` |
| Arch | `PKGBUILD`, for `makepkg -si` |

## The downloads are unsigned, and you should know that before you click

Windows SmartScreen will warn you when you download the installer and again
when you first run it: choose **More info**, then **Run anyway**. On Windows
11 with **Smart App Control** turned on, it may refuse to run at all -- the
zip is no different there. That is code signing not yet in place
([ADR-0094](https://bpad.prompt-forge.dev/docs)). Self-signing was refused outright,
because a self-signed certificate is only satisfied once you install a root
certificate you have no reason to trust, which is a worse thing to ask of
somebody than a warning is.

What is offered instead is a checksum, and it is not nothing: it answers
tampering in transit, which is the threat a certificate answers worst.

```sh
sha256sum -c SHA256SUMS.txt --ignore-missing
```

```powershell
(Get-FileHash -Algorithm SHA256 .\bachelorpad-lite-1.0.0-windows-x86_64-setup.exe).Hash
```

Compare it with the line in `SHA256SUMS.txt` beside the download.

## Windows

### With the installer

Run `bachelorpad-lite-<version>-windows-x86_64-setup.exe`. **It never asks
for administrator rights**: it installs for you alone, in
`%LOCALAPPDATA%\Programs\BachelorPad+ Lite`, adds a Start menu entry, and
appears in **Settings > Apps** with an uninstaller.

It offers two things, both unticked:

- **Open with**, which puts the program in Explorer's *Open with* list and in
  **Settings > Default apps** for text and Markdown files. It does **not**
  make itself the default for anything; that stays your choice, made in
  Windows' own settings ([ADR-0012](https://bpad.prompt-forge.dev/docs)).
- **A desktop shortcut.**

### From the zip

```powershell
Expand-Archive bachelorpad-lite-1.0.0-windows-x86_64.zip C:\Apps\BachelorPadLite
C:\Apps\BachelorPadLite\bachelorpad.exe
```

Shortcuts are yours to make, and File > Set as Default Editor registers file
types for wherever you unpacked it.

Both the installer and the zip contain:

| File | What it is |
| --- | --- |
| `bachelorpad.exe` | The program |
| `bpad.cmd` | A short spelling, for the command line |
| `bachelorpad.ico` | The icon Explorer draws for registered file types |
| `io.github.dboles99.bachelorpluslite.png` | The icon the window shows |
| `app-help/` | The in-app help, opened by Help > User Guide |
| `LICENSE`, `THIRD-PARTY-NOTICES.md` | GPL-3.0-only, and the crates it links |
| `BUILD.txt` | Version, commit, target, and that it is unsigned |
| `README.md` | The project's own front page |

Both icons ship because they are read by different things: Explorer needs a
real `.ico`, and the toolkit that draws the window can decode only PNG and
JPEG ([ADR-0068](https://bpad.prompt-forge.dev/docs)).

## Linux

### Ubuntu, Debian, Mint, Pop!_OS

```sh
sudo apt install ./bachelorpad-lite_1.0.0_amd64.deb
```

### Fedora, openSUSE

```sh
sudo dnf install ./bachelorpad-lite-1.0.0-1.x86_64.rpm      # Fedora
sudo zypper install ./bachelorpad-lite-1.0.0-1.x86_64.rpm   # openSUSE
```

Either package puts the program in your applications menu, with its icon,
and `bachelorpad` and `bpad` on your `PATH`. The program and its help live
together in `/usr/lib/bachelorpad-lite/`.

### Arch

Build and install from the `PKGBUILD` attached to the release:

```sh
makepkg -si
```

### Anything else: the AppImage

```sh
chmod +x bachelorpad-lite-1.0.0-x86_64.AppImage
./bachelorpad-lite-1.0.0-x86_64.AppImage
```

It is one file and installs nothing. To have it in your applications menu,
run File > Set as Default Editor from it.

### The archive

```sh
tar xzf bachelorpad-lite-1.0.0-linux-x86_64.tar.gz
cd bachelorpad-lite-1.0.0-linux-x86_64
./bachelorpad
```

`bpad` beside it is a symlink to the same binary. The executable bit and the
symlink are both set inside the archive, so neither needs restoring.

### Flatpak

A manifest for Flathub is written ([ADR-0093](https://bpad.prompt-forge.dev/docs)) and
not yet published there. Until it is, use one of the above.

## First launch

The window opens on an empty, unsaved document called *Untitled*. Nothing has
been written anywhere yet. Once you type, the recovery journal starts keeping
a copy of the unsaved work in your profile, so a crash does not cost it --
see [Undo and recovery](../troubleshooting/recovery.md).

Three things worth doing once:

1. **Pick a theme.** View > Theme offers Light, Dark, Organic and Green, plus
   System, which follows the desktop.
2. **Look at Privacy.** The Privacy menu governs what this product records
   about the documents you open -- see [Privacy](../troubleshooting/privacy.md). The default
   is deliberately modest, and it is worth knowing what it is rather than
   assuming.
3. **Decide about file associations**, if you want double-clicking a `.txt` to
   open this. File > Set as Default Editor never seizes anything; it shows you
   what it would do and hands you a script to run. See
   [Making it your default editor](../workflows/default-editor.md).

## Where it puts things

Wherever the program is installed, everything it writes about itself goes in
the usual per-user locations:

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

| Installed with | Remove with |
| --- | --- |
| The Windows installer | **Settings > Apps**, BachelorPad+ Lite, Uninstall. It removes the program, its shortcuts and the Open with entries it added |
| The zip or the archive | Delete the folder |
| The `.deb` | `sudo apt remove bachelorpad-lite` |
| The `.rpm` | `sudo dnf remove bachelorpad-lite` |
| The AppImage | Delete the file |

None of them removes your settings, recent files, recovery journal or notes
index -- the three directories above. Delete those as well to remove
everything.

If you registered file types with File > Set as Default Editor, undo that
first: run the `-remove.reg` file it wrote beside the registration on
Windows, or delete the two files on Linux, as
[Making it your default editor](../workflows/default-editor.md) says.
**This product will not delete anything out of your profile on your behalf**,
and [ADR-0070](https://bpad.prompt-forge.dev/docs) explains why that is deliberate
rather than an omission.

---

[Back to the help index](../index.md)
