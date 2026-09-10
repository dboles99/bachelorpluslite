# Command line

*Generated from the binary's own `--help` by `scripts/Build-Docs.ps1`. Do not
edit.*

The flag list in `bp_config::cli::FLAGS` is the one home for this, and
`--help` is rendered from it rather than written out -- so a flag that exists
without a line here is not possible.

```text
BachelorPad+ Lite 0.9.5 -- Text editor for notes and logs

Usage:
  bachelorpad [options] [file...]

Every file named is opened in its own tab. How it opens is decided
from its size before a byte is read, so there is no flag for that.

General
  --help, -h                  Print this and exit.
  --version, -V               Print the version and licence, and exit.

Opening a file
  --line=N                    Put the caret on this line of the first file.

Settings
  --theme=NAME                Start in a named theme, e.g. Light, Dark, Green.
  --font-size=N               Editor font size in points.
  --tab-width=N               Columns a Tab advances to.
  --indent-spaces=true|false  Insert spaces for Tab, rather than a tab.
  --renderer=NAME             software or platform. Software is the default.
  --log=FILTER                Tracing filter, e.g. warn or bp_ui=debug.

Development
  --editor-view               Draw with the custom editor surface.
  --self-check                Load, print a marker and exit. For the gate.
  --measure-exit              Print the time to the first frame, then exit.
  --latency-probe             Run the input-latency probe, then exit.

Settings also come from BACHELORPAD_* in the environment and from
config.toml in this product's configuration directory. The command
line wins, then the environment, then the file.
```

## Notes

**On Windows, a release build prints this to a console only when one is
attached.** It is a GUI-subsystem executable, so running it from a terminal
works and double-clicking it shows nothing. Measured and conceded
deliberately: the alternative costs `unsafe` and a Windows dependency, to
serve somebody double-clicking a text editor in order to read its help
(ADR-0054).

**A flag this product does not accept is reported, not swallowed.** That was
once untrue in one direction only -- the config file told you about an
unknown key and the command line quietly discarded an unknown flag, and
nobody had chosen that.
