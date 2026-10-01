# Settings and customisation

Settings come from three places, and the later ones win:

1. built-in defaults
2. `config.toml`
3. command-line flags

**A setting you get wrong is reported, not swallowed.** An unknown key, an
unreadable file, a value that is not one of the accepted ones, or a flag this
product does not accept -- each produces a notice you can see. That is true of
the config file *and* of the command line, which it once was not: one end told
you and the other end quietly discarded it, and nobody had chosen that.

## The config file

| | |
| --- | --- |
| Windows | `%APPDATA%\bachelorpad\config.toml` |
| Linux | `$XDG_CONFIG_HOME/bachelorpad/config.toml`, else `~/.config/bachelorpad/config.toml` |

**Help > Diagnostics prints the path as resolved on your machine**, which
beats this table. Tools > Configuration shows what was actually loaded.

```toml
# Every key, with its default.

# Light, Dark, Organic, Green -- or omitted, to follow the desktop.
# theme = "Dark"

# "software" or "platform". Software is the default and is the one that
# behaves the same everywhere.
renderer = "software"

# A tracing filter. Quiet by default: an editor that chatters on stdout is
# an editor whose real warnings get ignored.
log = "warn"

# Editor font size, in points. Ctrl+= and Ctrl+- change this for the session.
font_size = 14

# Columns a Tab advances to.
tab_width = 4

# Insert spaces for Tab rather than a tab character.
indent_spaces = false
```

Settings roam with your Windows profile. Data and caches do not -- an index
has no business crossing a network at sign-out.

## Flags

Anything in the config file can be overridden for one run:

```sh
bpad --theme Dark --font-size 16 notes.md
bpad --editor-view
bpad --line 427 server.log
```

A value can follow its flag after a space or after `=`: `--line 427` and
`--line=427` are the same. Until 1.0 only the second worked, and the first
opened a file called `427`. Everything after `--` is a file name, however it
begins, which is how to open a file called `-notes.txt`:

```sh
bpad -- -notes.txt
```

`tab_width` and `indent_spaces` decide what Tab inserts from the first
document onwards. Before 1.0 they were read and then ignored, and every run
began with tabs four columns wide.

The full list is in [the CLI reference](Command-Line), which
is generated from `--help` -- so it is the binary's own answer rather than a
copy of it.

`--help` and `--version` are handled before configuration is loaded, on
purpose: `--help` is what you reach for when the product is misbehaving, and a
config file broken badly enough to be the reason is not a reason to withhold
it.

**On Windows, a release build has no console of its own.** `--help` and
`--version` print to stdout, which reaches a terminal when you run it from
one, and goes nowhere when you double-click the executable. That is a
deliberate concession -- the fix costs `unsafe` and a Windows dependency, to
serve somebody double-clicking a text editor in order to read its help.

## Themes

**View > Theme**: Light, Dark, Organic, Green, and System.

System follows the desktop's own light/dark preference and changes with it.

## What is not configurable

**Keyboard shortcuts are fixed.** Rebinding is a real feature with a real
design behind it -- a keymap file, a conflict story, a way to see what a key
does now -- and none of that has been designed, so it is absent rather than
half-present.

**The window does not remember its size or position**, and open tabs are not
restored on the next launch.

---

*This page is generated from [`docs/user/08-settings.md`](https://github.com/dboles99/bachelorpluslite/blob/main/docs/user/08-settings.md) and
edits made here will be overwritten. Change the source and open a pull request.*
