#!/usr/bin/env bash
# Type a thousand characters into a 100 KB document and fail if the editor's
# resident memory grows by more than a limit (W1-05).
#
#   scripts/probe-typing-memory-xvfb.sh              # the default surface
#   scripts/probe-typing-memory-xvfb.sh --editor-view
#   DRIVE_BIN=target/release/bachelorpad scripts/probe-typing-memory-xvfb.sh
#   KEYSTROKES=200 scripts/probe-typing-memory-xvfb.sh
#
# **Slow on a debug build**: `TextInput` lays out the whole document on every
# key, which at 100 KB is about a second a key in debug and 80 ms in release.
# A thousand keys is 20 minutes in debug and under two in release, so run the
# release build, or fewer keys. The defect was 200 KB a key, so 200 keys
# still show it forty megabytes wide.
#
# Under the default surface every keystroke reaches Rust as the widget's whole
# text. Before W1-05, `AppState::edit` handed that to `Editor::replace_all_text`,
# which recorded it as an undo step holding the old document and the new one
# -- about 200 KB kept per key at 100 KB, and never read, because Ctrl+Z there
# is the widget's. A thousand keys kept about 200 MB.
#
# Resident memory is a coarse instrument: the allocator keeps what it freed,
# and the first keys fault in pages nothing had touched. So the baseline is
# taken after a warm-up of typing, and the limit is generous against noise and
# small against the defect -- 5 MB for 1,000 keys, where the defect was 40
# times that.
#
# Needs what `drive-window-xvfb.sh` needs, and a built binary.
set -u

flag=${1:-}
root=$(cd "$(dirname "$0")/.." && pwd)
# DRIVE_BIN runs another build, as in `drive-window-xvfb.sh`.
bin=${DRIVE_BIN:-$root/target/debug/bachelorpad}
[ -x "$bin" ] || { echo "no $bin -- run: cargo build -p bachelorpad" >&2; exit 2; }
keystrokes=${KEYSTROKES:-1000}
limit_kb=${LIMIT_KB:-5120}

run=$(mktemp -d)
trap 'rm -rf "$run"' EXIT
export HOME=$run/home XDG_CONFIG_HOME=$run/config XDG_DATA_HOME=$run/data \
    XDG_STATE_HOME=$run/state XDG_CACHE_HOME=$run/cache
mkdir -p "$HOME" "$XDG_CONFIG_HOME" "$XDG_DATA_HOME" "$XDG_STATE_HOME" "$XDG_CACHE_HOME"
# 5,000 lines of 21 bytes: 105 KB.
for i in $(seq -w 1 5000); do printf 'line %s abcdefghij\n' "$i"; done >"$run/doc.txt"

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
sleep 6
xdotool windowfocus "$window"
sleep 0.3
xdotool mousemove 100 70 click 1
sleep 1
xdotool windowfocus "$window"
xdotool click 1
sleep 1
# The first key after the window maps is often lost, and with it would go the
# warm-up; spend it on one that changes nothing.
xdotool key ctrl+Home
sleep 0.5

rss() { awk '/^VmRSS:/ { print $2 }' "/proc/$app/status"; }
# Letters and no newline: the typing path, and nothing that ends an undo run.
# `xdotool key`, not `xdotool type`, which never reaches this window: winit
# ignores the keymap `type` rewrites to send a character.
type_n() {
    # shellcheck disable=SC2046 -- one argument per key is the point.
    xdotool key --delay 15 $(yes x | head -n "$1")
}
# Keys arrive faster than a debug build handles them at this size, so the
# window can be a long way behind the last one sent. Save, and wait for the
# file to hold them all: a reading taken earlier measures a backlog, and a
# probe whose keys never arrived would pass.
settled() {
    local want=$1
    # A save every ten seconds rather than every second: each one queues
    # behind the backlog, and a queue of saves would be a backlog of its own.
    for i in $(seq 0 599); do
        [ $((i % 10)) -eq 0 ] && xdotool key ctrl+s
        sleep 1
        [ "$(tr -cd x <"$run/doc.txt" | wc -c)" -ge "$want" ] && return 0
    done
    echo "the window never caught up: $(tr -cd x <"$run/doc.txt" | wc -c) of $want keys saved" >&2
    kill "$app" 2>/dev/null
    exit 1
}

type_n 100
settled 100
before=$(rss)
type_n "$keystrokes"
settled $((100 + keystrokes))
after=$(rss)
kill "$app" 2>/dev/null
wait "$app" 2>/dev/null

grown=$((after - before))
echo "resident: $before KB before, $after KB after $keystrokes keys -- grew $grown KB (limit $limit_kb KB)"
if grep -q 'panicked at' "$run/app.log"; then
    echo "the application panicked:" >&2
    grep -A2 'panicked at' "$run/app.log" >&2
    exit 1
fi
[ "$grown" -le "$limit_kb" ]
