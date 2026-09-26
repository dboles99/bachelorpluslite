#!/usr/bin/env bash
# Drive the Linux build's window with no display and no person, and report
# where the caret went as a diff rather than a picture (ADR-0080).
#
# Opens a 200-line fixture, clicks into it, presses each key in KEYS, saves
# with Ctrl+S, and prints what changed. A `click:X,Y` in KEYS clicks there, in
# window coordinates, instead of pressing a key. Put a marker letter after the key
# under test -- "ctrl+Home A Page_Down F" -- and where F lands in the saved
# file is where Page Down put the caret. Evidence in bytes: nothing to
# interpret.
#
#   scripts/drive-window-xvfb.sh "ctrl+Home A Page_Down F"
#   scripts/drive-window-xvfb.sh "ctrl+Home Insert X Y" --editor-view
#   FIXTURE_LINES=5000 SLINT_SCALE_FACTOR=2 scripts/drive-window-xvfb.sh "ctrl+End Z"
#   scripts/drive-window-xvfb.sh "ctrl+Home click:70,14 Down Down Return X"
#   WINDOW_SIZE=800x260 scripts/drive-window-xvfb.sh "ctrl+Home F10 Right End"
#
# The last is the long-document crash (ADR-0083): Slint 1.17.1's software
# renderer panicked past about 2,000 lines, or 1,100 at a scale factor of 2.
# A panic in the log fails the run whatever the diff says, because a Slint
# panic inside the event loop can leave the window up and the save working.
#
# Needs Xvfb, xdotool, imagemagick (for the screenshot) and libxkbcommon-x11
# -- `apt-get install xvfb xdotool imagemagick libxkbcommon-x11-0`. Builds
# nothing; run `cargo build -p bachelorpad` first.
#
# **A run where nothing at all changed is the harness, not the product.** With
# no window manager the first input after the window maps is sometimes lost,
# and then every key after it is too. Rerun it before believing it.
#
# **And suspect the probe before the product.** Slint re-selects the text an
# undo restores, so a marker typed straight after Ctrl+Z replaces what came
# back and reads as data loss. Send the marker somewhere neutral -- End, then
# the letter -- whenever the key under test can leave a selection behind.
set -u

keys=${1:?usage: drive-window-xvfb.sh "<xdotool keys>" [flag]}
flag=${2:-}
root=$(cd "$(dirname "$0")/.." && pwd)
bin="$root/target/debug/bachelorpad"
[ -x "$bin" ] || { echo "no $bin -- run: cargo build -p bachelorpad" >&2; exit 2; }

# A throwaway profile, so a run never writes to the real one (trap 1) and
# never meets a recovery journal an earlier run left behind.
run=$(mktemp -d)
trap 'rm -rf "$run"' EXIT
export HOME=$run/home XDG_CONFIG_HOME=$run/config XDG_DATA_HOME=$run/data \
    XDG_STATE_HOME=$run/state XDG_CACHE_HOME=$run/cache
mkdir -p "$HOME" "$XDG_CONFIG_HOME" "$XDG_DATA_HOME" "$XDG_STATE_HOME" "$XDG_CACHE_HOME"

fixture() { for i in $(seq -w 1 "${FIXTURE_LINES:-200}"); do echo "line $i abcdefghij"; done; }
fixture > "$run/doc.txt"

export DISPLAY=${XVFB_DISPLAY:-:99}
unset WAYLAND_DISPLAY
export SLINT_BACKEND=${SLINT_BACKEND:-winit-software}
if ! xdotool getmouselocation >/dev/null 2>&1; then
    Xvfb "$DISPLAY" -screen 0 1280x900x24 >/dev/null 2>&1 &
    sleep 2
fi
# Only instances on this display. Two there take each other's clicks; one on
# the real desktop is somebody's editor, and killing it could cost them work.
for p in $(pgrep -x bachelorpad); do
    tr '\0' '\n' <"/proc/$p/environ" 2>/dev/null | grep -qxF "DISPLAY=$DISPLAY" && kill "$p"
done

# shellcheck disable=SC2086 -- an empty flag must vanish, not become "".
"$bin" $flag "$run/doc.txt" >"$run/app.log" 2>&1 &
app=$!
window=
for _ in $(seq 1 60); do
    window=$(xdotool search --pid "$app" 2>/dev/null | tail -1)
    [ -n "$window" ] && break
    sleep 0.5
done
[ -n "$window" ] || { echo "no window appeared" >&2; cat "$run/app.log" >&2; kill "$app"; exit 1; }
# A short window makes a long menu scroll, which is the only way to see that
# the keyboard's row is kept on screen.
if [ -n "${WINDOW_SIZE:-}" ]; then
    xdotool windowsize "$window" "${WINDOW_SIZE%x*}" "${WINDOW_SIZE#*x}"
fi
sleep 6

xdotool windowfocus "$window"
sleep 0.3
xdotool mousemove 100 70 click 1
sleep 1
xdotool windowfocus "$window"
xdotool click 1
sleep 1
for key in $keys; do
    case $key in
        # A click at window coordinates, for a key sequence that starts from
        # something the mouse opened -- a menu, say, then the arrows.
        click:*,*)
            xy=${key#click:}
            xdotool mousemove --window "$window" "${xy%,*}" "${xy#*,}" click 1
            ;;
        *) xdotool key --delay 80 "$key" ;;
    esac
    sleep 0.25
done
xdotool key ctrl+s
sleep 1.5
import -window root "${SCREENSHOT:-/dev/null}" 2>/dev/null || true
kill "$app" 2>/dev/null
wait "$app" 2>/dev/null

diff <(fixture) "$run/doc.txt" | cat -A | sed 's/\$$//'
if grep -q 'panicked at' "$run/app.log"; then
    echo "the application panicked:" >&2
    grep -A2 'panicked at' "$run/app.log" >&2
    exit 1
fi
exit 0
