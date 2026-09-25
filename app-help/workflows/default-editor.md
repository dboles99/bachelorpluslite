# Making it your default editor

**This product never seizes a file association**
([ADR-0012](https://bpad.prompt-forge.dev/docs)). It tells you what is registered now,
plans a change, shows you the plan, and hands you the means to apply it. The
last step is always yours.

## Look before you change anything

**File > Set as Default Editor.**

The screen has three parts:

1. **What opens each file type today** -- read from your machine, not assumed.
2. **A preset**, choosing which types to register for. *Notepad Replacement*
   is the modest one: plain text and logs.
3. **The plan** -- exactly what would be written, and where.

Nothing has happened yet.

> **`.bpadx` is not in any preset**, including *Notepad Replacement*. It used
> to be in all of them, and this build opens such a file as ciphertext because
> it has no decryption in it. Registering a file type you cannot open is worse
> than not registering it ([ADR-0069](https://bpad.prompt-forge.dev/docs)).

## Windows

Choose a preset and confirm. You get a **`.reg` file**, written where you can
find it.

**Read it.** It is a few lines, and it is your machine.

```
HKEY_CURRENT_USER\Software\Classes\...
```

Everything is under `HKEY_CURRENT_USER`. Nothing machine-wide is touched, and
nothing needs administrator rights.

Double-click it, accept the prompt, then finish in **Settings > Default apps**
-- Windows requires the final choice to be made by you, there, and no
application can make it for you.

Explorer will draw the icon from `bachelorpad.ico` **beside the executable**.
If you move the executable and leave the icon behind, every registered type
draws blank ([ADR-0068](https://bpad.prompt-forge.dev/docs)) -- move the whole folder.

## Linux

Confirm, and two things are written under your home directory:

- a `.desktop` entry, in `~/.local/share/applications/`
- a MIME package, in `~/.local/share/mime/packages/`

Then run the two commands the screen names:

```sh
update-desktop-database ~/.local/share/applications
update-mime-database ~/.local/share/mime
```

**They are named for you and not run for you.** A program that spawns
processes on your behalf is a program that can be talked into spawning a
different one, and that is a line this project holds everywhere rather than
only where it is convenient.

The `.desktop` entry points at the PNG beside the binary, for the same reason
as on Windows: with no installer there is no step that could put an icon in a
system theme.

## Undoing it

Windows: **Settings > Default apps**, and set the types back. The registry
entries can be removed by hand if you want them gone entirely.

Linux: delete the two files and run the two commands again.

This product will not remove them for you, which is the same rule as
everywhere else ([ADR-0070](https://bpad.prompt-forge.dev/docs)).

---

[Back to the help index](../index.md)
