#!/usr/bin/env bash
# Crash with unsaved work, then relaunch four times and answer the recovery
# question four ways, checking the journal on disk after each (ADR-0084).
#
#   scripts/drive-recovery-xvfb.sh              # the default surface
#   scripts/drive-recovery-xvfb.sh --editor-view
#
# What it proves, in order:
#
#   1. unsaved work in a killed run is on disk;
#   2. a question nobody answers for two checkpoint passes keeps it -- the
#      case that lost it, twice over: `rfd` answered for the user when it could
#      not run `zenity`, and the next run's clean Untitled discarded the last
#      run's `1.json` because both were document 1;
#   3. Escape (Not Now) keeps it;
#   4. Restore brings it back and re-files it under the new run;
#   5. Discard, and only Discard, removes it.
#
# Run it on a machine *without* `zenity` to reproduce the defect's conditions.
# Needs what `drive-window-xvfb.sh` needs, and a built binary.
#
# **A run where the first launch leaves no checkpoint is the harness, not the
# product** -- the first input after a window maps is sometimes lost under a
# bare Xvfb. Rerun it before believing it.
set -u

flag=${1:-}
root=$(cd "$(dirname "$0")/.." && pwd)
bin="$root/target/debug/bachelorpad"
[ -x "$bin" ] || { echo "no $bin -- run: cargo build -p bachelorpad" >&2; exit 2; }

# One profile across every launch, because a relaunch is the thing under
# test; and a throwaway one, so a run never touches the real one (trap 1).
run=$(mktemp -d)
trap 'rm -rf "$run"' EXIT
export HOME=$run/home XDG_CONFIG_HOME=$run/config XDG_DATA_HOME=$run/data \
    XDG_STATE_HOME=$run/state XDG_CACHE_HOME=$run/cache
mkdir -p "$HOME" "$XDG_CONFIG_HOME" "$XDG_DATA_HOME" "$XDG_STATE_HOME" "$XDG_CACHE_HOME"
journal=$XDG_STATE_HOME/bachelorpad/recovery

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

failed=0
launch() {
    # shellcheck disable=SC2086 -- an empty flag must vanish, not become "".
    "$bin" $flag >"$run/app.log" 2>&1 &
    app=$!
    window=
    for _ in $(seq 1 60); do
        window=$(xdotool search --pid "$app" 2>/dev/null | tail -1)
        [ -n "$window" ] && break
        sleep 0.5
    done
    sleep 6
    xdotool windowfocus "$window" 2>/dev/null
    sleep 0.3
}
crash() {
    kill -9 "$app"
    wait "$app" 2>/dev/null
    if grep -q 'panicked at' "$run/app.log"; then
        echo "FAIL: the application panicked" >&2
        grep -A2 'panicked at' "$run/app.log" >&2
        failed=1
    fi
}
# The texts of every checkpoint on disk, one per line, sorted.
texts() {
    for f in "$journal"/*.json; do
        [ -f "$f" ] && python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["text"])' "$f"
    done | sort
}
expect() {
    local what=$1 want=$2 got
    got=$(texts | tr '\n' '|')
    if [ "$got" = "$want" ]; then
        echo "ok:   $what"
    else
        echo "FAIL: $what -- journal holds '$got', expected '$want'" >&2
        failed=1
    fi
}

launch
xdotool mousemove 300 300 click 1
sleep 0.5
xdotool windowfocus "$window"
xdotool click 1
sleep 0.5
xdotool type --delay 60 "survives"
sleep 7
crash
expect "a killed run leaves its unsaved work on disk" "survives|"

launch
sleep 12
crash
expect "a question nobody answers keeps the work" "survives|"

launch
xdotool key Escape
sleep 7
crash
expect "Not Now keeps the work" "survives|"

before=$(ls "$journal")
launch
xdotool key Return
sleep 7
crash
expect "Restore brings it back and checkpoints it again" "survives|"
if [ "$(ls "$journal")" = "$before" ]; then
    echo "FAIL: Restore left the old run's entry rather than re-filing it" >&2
    failed=1
fi

launch
xdotool key Right
sleep 0.3
xdotool key Return
sleep 7
crash
expect "Discard, and only Discard, removes it" ""

exit $failed
