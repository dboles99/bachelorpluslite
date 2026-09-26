#!/usr/bin/env bash
# Read the window the way a screen reader does, and fail if a control has lost
# its name, its role or its focus (ADR-0088).
#
#   scripts/check-accessibility-xvfb.sh
#
# Runs the editor on a private session bus with AT-SPI switched on, drives it
# into each state below with the keyboard, and greps the tree
# `scripts/atspi-tree.py` prints. It asserts what Slint and AccessKit hand to
# AT-SPI -- names, roles, the focused object -- which is everything a reader
# builds its speech from, and nothing about how Orca or NVDA then speaks it.
# **That half needs a person with a screen reader**, and ADR-0088 says so.
#
# Needs what `drive-window-xvfb.sh` needs, plus `at-spi2-core`,
# `python3-pyatspi` and `dbus-daemon`. Builds nothing; run
# `cargo build -p bachelorpad` first.
#
# **A state where the tree is empty is the harness, not the product**: with no
# window manager the first input after the window maps is sometimes lost.
# Rerun it before believing it.
set -u

# Everything below runs on a bus of its own, so the check never reads -- or
# switches accessibility on for -- somebody's desktop session.
if [ -z "${A11Y_INNER:-}" ]; then
    A11Y_INNER=1 exec dbus-run-session -- "$0" "$@"
fi

root=$(cd "$(dirname "$0")/.." && pwd)
bin="$root/target/debug/bachelorpad"
[ -x "$bin" ] || { echo "no $bin -- run: cargo build -p bachelorpad" >&2; exit 2; }
launcher=/usr/libexec/at-spi-bus-launcher
[ -x "$launcher" ] || { echo "no $launcher -- apt-get install at-spi2-core" >&2; exit 2; }
# The first Python the installed bindings were built for. Ubuntu's
# `python3-pyatspi` is compiled for one interpreter, and `python3` on PATH is
# not always that one.
python=
for p in /usr/bin/python3 /usr/bin/python3.*[0-9]; do
    if "$p" -c 'import pyatspi' 2>/dev/null; then python=$p; break; fi
done
[ -n "$python" ] || { echo "no Python can import pyatspi -- apt-get install python3-pyatspi" >&2; exit 2; }

run=$(mktemp -d)
trap 'rm -rf "$run"' EXIT
export HOME=$run/home XDG_CONFIG_HOME=$run/config XDG_DATA_HOME=$run/data \
    XDG_STATE_HOME=$run/state XDG_CACHE_HOME=$run/cache
mkdir -p "$HOME" "$XDG_CONFIG_HOME" "$XDG_DATA_HOME" "$XDG_STATE_HOME" "$XDG_CACHE_HOME"
printf 'hello\n' >"$run/doc.txt"

export DISPLAY=${XVFB_DISPLAY:-:99}
unset WAYLAND_DISPLAY
export SLINT_BACKEND=${SLINT_BACKEND:-winit-software}
if ! xdotool getmouselocation >/dev/null 2>&1; then
    Xvfb "$DISPLAY" -screen 0 1280x900x24 >/dev/null 2>&1 &
    sleep 2
fi
for p in $(pgrep -x bachelorpad); do
    tr '\0' '\n' <"/proc/$p/environ" 2>/dev/null | grep -qxF "DISPLAY=$DISPLAY" && kill "$p"
done

"$launcher" --launch-immediately >/dev/null 2>&1 &
sleep 1
# AccessKit publishes nothing until something says a reader is listening.
busctl --user set-property org.a11y.Bus /org/a11y/bus org.a11y.Status IsEnabled b true
busctl --user set-property org.a11y.Bus /org/a11y/bus org.a11y.Status ScreenReaderEnabled b true

failed=0
# Launch, press KEYS, and leave the tree in $run/tree.
state() {
    "$bin" "$run/doc.txt" >"$run/app.log" 2>&1 &
    local app=$! window=
    for _ in $(seq 1 60); do
        window=$(xdotool search --pid "$app" 2>/dev/null | tail -1)
        [ -n "$window" ] && break
        sleep 0.5
    done
    sleep 5
    xdotool windowfocus "$window"
    xdotool mousemove 100 90 click 1
    sleep 0.5
    for key in $1; do
        xdotool key "$key"
        sleep 0.3
    done
    sleep 1
    "$python" "$root/scripts/atspi-tree.py" bachelorpad >"$run/tree" 2>&1
    kill "$app"
    wait "$app" 2>/dev/null
    echo "-- ${1:-(opened)}"
}
has() {
    if grep -qF -- "$1" "$run/tree"; then
        echo "ok:   $1"
    else
        echo "FAIL: missing $1" >&2
        failed=1
    fi
}
lacks() {
    if grep -qF -- "$1" "$run/tree"; then
        echo "FAIL: present $1" >&2
        failed=1
    else
        echo "ok:   no $1"
    fi
}

state ""
has "[push button] 'File'"
has "[push button] 'Help'"
has "[page tab list] 'Open documents'"
has "[page tab] 'doc.txt' {selected"
has "[push button] 'Close doc.txt'"
has "[push button] 'New document'"
has "[entry] 'Document'"
has "[push button] 'Theme: "
# Read aloud, the gutter was every line number before the first word, and each
# status readout began "vertical line".
lacks "[label] '1"
lacks "[label] '│"
# A named control's own text, exposed as well, is read twice -- "Close
# doc.txt, button, times".
lacks "[label] '×'"

state "F10 Right Down Down"
has "[list box] 'Edit'"
has "[list item] 'Cut' desc='Ctrl+X' {selected,sensitive,focused}"

state "F10 Right End Up Up Up"
has "[list item] 'Duplicate Line' desc='Ctrl+D, unavailable'"

state "ctrl+f"
has "[entry] 'Find' {sensitive,focused}"
has "[check box] 'Match case'"
has "[check box] 'Whole word'"
has "[check box] 'Regular expression'"
has "[push button] 'Previous match'"
has "[push button] 'Next match'"
has "[entry] 'Replace with'"
has "[push button] 'Close find bar'"

state "ctrl+g"
has "[entry] 'Go to line number' {sensitive,focused}"
has "[push button] 'Close go to line'"

state "x ctrl+w Right"
has "[panel] 'Unsaved changes'"
has "[push button] \"Don't Save\" {sensitive,focused}"

# A question whose default is not its first button, and which has no Decline
# -- the case where the focused child's index shifts.
state "F10 End Up Return"
has "[panel] 'Set as default editor'"
has "[push button] 'Cancel' {sensitive,focused}"

exit $failed
