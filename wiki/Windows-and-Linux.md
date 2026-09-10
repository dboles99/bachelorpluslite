# Windows and Linux differences

**Windows 10, Windows 11 and Linux are equal targets**
([ADR-0001](https://github.com/dboles99/bachelorpluslite/blob/main/docs/decisions/ADR-0001.md)), and both legs are tested on every
change. The differences below are the operating systems' rather than this
product's.

macOS is not a target. [ADR-0072](https://github.com/dboles99/bachelorpluslite/blob/main/docs/decisions/ADR-0072.md) says why, and what
it would cost.

## Where things are kept

| | Windows | Linux |
| --- | --- | --- |
| Settings | `%APPDATA%\bachelorpad\` (roams) | `$XDG_CONFIG_HOME/bachelorpad/`, else `~/.config/bachelorpad/` |
| Index, recent files | `%LOCALAPPDATA%\bachelorpad\data\` | `$XDG_DATA_HOME/bachelorpad/`, else `~/.local/share/bachelorpad/` |
| Cache | `%LOCALAPPDATA%\bachelorpad\cache\` | `$XDG_CACHE_HOME/bachelorpad/`, else `~/.cache/bachelorpad/` |
| Recovery journals | `%LOCALAPPDATA%\bachelorpad\state\` | `$XDG_STATE_HOME/bachelorpad/`, else `~/.local/state/bachelorpad/` |

A relative `XDG_*` variable is ignored, as the specification requires -- an
editor that honoured one would put your settings under whatever directory it
happened to be launched from.

## Filenames

Windows forbids `< > : " / \ | ? *`, forbids the reserved device names `CON`,
`PRN`, `AUX`, `NUL`, `COM1`-`COM9` and `LPT1`-`LPT9` **whatever extension you
give them**, and quietly strips trailing dots and spaces. Linux forbids only
`/` and the null byte.

A name that is fine on Linux and illegal on Windows is caught before the save
rather than after it fails.

Windows path length is handled with the `\\?\` prefix where it is needed.
Linux is case-sensitive and Windows is not, so two filenames differing only in
case are two files on one and one file on the other -- which the product knows
about when it asks whether you are about to overwrite something.

## Default-editor registration

Different mechanisms, the same rule: **this product never seizes a file
association** ([ADR-0012](https://github.com/dboles99/bachelorpluslite/blob/main/docs/decisions/ADR-0012.md)). It shows you what it
would do and hands you the means to do it.

| | Windows | Linux |
| --- | --- | --- |
| What is written | `HKCU\Software\Classes` entries | a `.desktop` entry and a MIME package |
| How | a `.reg` file you read and run | files written under your home directory |
| Finishing the job | Settings > Default apps | `update-desktop-database`, `update-mime-database` |

Both are named for you and neither is run for you. A library that spawns
processes on your behalf is a library that can be talked into spawning a
different one.

See [Making it your default editor](Default-Editor).

## The console, on Windows

A Windows release build is a GUI-subsystem executable and has no console of
its own, so `--help` and `--version` print to a terminal when you run it from
one and go nowhere when you double-click it. Measured and conceded
deliberately -- the alternative costs `unsafe` and a Windows dependency.

On Linux the binary is an ordinary console program and always prints.

## File dialogs

Linux uses the XDG desktop portal rather than a specific toolkit, so the
program does not need a particular desktop environment installed just to open
a file.

## Capabilities that differ

Help > Diagnostics and the Tools menu report what this platform can and cannot
do, with the reason attached, rather than a feature quietly being absent. Two
of them -- a credential store and biometric unlock -- report *awaiting
decision* on both platforms, because what would have gone in them was a
signing key and signing was removed.

---

*This page is generated from [`docs/user/10-platform-differences.md`](https://github.com/dboles99/bachelorpluslite/blob/main/docs/user/10-platform-differences.md) and
edits made here will be overwritten. Change the source and open a pull request.*
