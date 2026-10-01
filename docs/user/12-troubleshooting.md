# Troubleshooting

Start with **Help > Diagnostics**. It prints the version, the renderer in use,
and every directory as resolved on your machine -- which is what most of the
questions below turn out to be about.

## Windows warns me before it will run this

SmartScreen, because the archives are unsigned
([ADR-0055](../decisions/ADR-0055.md)). *More info* then *Run anyway*.

Verify the download first if you would rather:

```powershell
(Get-FileHash -Algorithm SHA256 .\bachelorpad-lite-0.9.5-windows-x86_64.zip).Hash
```

against the line in `SHA256SUMS.txt` on the releases page.

## It will not start at all

**A leftover recovery journal is the usual cause**, and it does not look like
one: the recovery prompt is its own top-level window, and the main window
never appears behind it, so the program looks hung rather than waiting.

Look in `%LOCALAPPDATA%\bachelorpad\state\recovery\` (Linux:
`~/.local/state/bachelorpad/recovery/`). The files are small JSON documents
that name the document they belong to.

**Read them before deleting any.** It is somebody's unsaved work, however
little of it. An *Untitled* buffer's journal names no path at all, which is
why you cannot filter these safely -- open them.

If it is not that, run from a terminal with logging on:

```sh
bpad --log debug
```

## The window has no icon, or a file type shows a blank page in Explorer

The icons ship **beside** the executable, not inside it
([ADR-0068](../decisions/ADR-0068.md)). If you moved `bachelorpad.exe` out of
the unpacked folder and left `bachelorpad.ico` and
`io.github.dboles99.bachelorpluslite.png` behind, both break.

Move the whole folder, or unpack it again.

## Text looks wrong, or the window renders oddly

Try the other renderer:

```sh
bpad --renderer platform
bpad --renderer software
```

Software is the default and is the one that behaves the same everywhere.

## `--help` prints nothing when I double-click it

Expected on Windows. A release build is a GUI-subsystem executable with no
console of its own, so stdout goes nowhere unless a terminal is attached. Run
it from PowerShell or a command prompt.

## Duplicate line and Alt+Up do nothing

They need the custom editor surface:

```sh
bpad --editor-view
```

The default surface is the toolkit's text widget, which will not say where the
caret is -- and a feature that has to know cannot be built on it
([ADR-0018](../decisions/ADR-0018.md)). See
[Editing](03-editing.md#the-two-editor-surfaces).

## Typing is slow in a large file

The default editing view lays out the whole document again on every key, so
typing slows as the file grows: measured at about 80 ms of processor per key
at 100 KB, and more above that. From 512 KiB the status bar says so when the
file opens ([ADR-0092](../decisions/ADR-0092.md)).

Reading, scrolling, Find and saving are not affected. To type in a large
file, start the program with `--editor-view`, which draws only the lines on
screen. It is not the default because it cannot take input-method text, so
Japanese or Chinese cannot be typed in it.

## Dropping a file on the window does nothing

Not implemented yet. It is buildable on Windows and on an X11 Linux session,
and will not work on a native Wayland one, whose window system does not hand
this toolkit a dropped file ([ADR-0081](../decisions/ADR-0081.md)). Until
then, use File > Open, Open Recent, or name the file on the command line.

## A file opens as gibberish

If it is a `.bpadx`, that is expected -- it is encrypted, and this build has no
decryption in it ([ADR-0064](../decisions/ADR-0064.md)). It opens as
ciphertext because that is literally what is in the file.

Otherwise the file is probably not UTF-8. This product reads and writes UTF-8
and UTF-8 with BOM, and nothing else; see
[Encoding](07-encoding-and-formats.md) for why.

## Related Notes is empty, or Research Report says there is nothing

Your privacy profile is probably set to record nothing, which is a valid
choice and not a fault. The rows say so in their own labels rather than
appearing broken. See [Privacy](11-privacy.md).

## Reporting a problem

[Open an issue](https://github.com/dboles99/bachelorpluslite/issues) with the
output of Help > Diagnostics, what you did, and what happened instead.

**For a security vulnerability, do not open a public issue** -- see
[SECURITY.md](../../SECURITY.md).
