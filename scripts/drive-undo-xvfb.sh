#!/usr/bin/env bash
# Undo and redo under `TextInput`, driven at the window, with the saved file
# checked after each run (W1-05).
#
#   scripts/drive-undo-xvfb.sh
#
# Default surface only: `--editor-view` never had two histories, and its undo
# is `bp-editor`'s, which the unit tests cover.
#
# What it proves, in order:
#
#   1. Sort Lines is undone by Ctrl+Z, and by Edit > Undo, and so is Replace
#      All -- the widget cannot, because Slint clears its history when Rust
#      replaces the text;
#   2. Ctrl+Y redoes it;
#   3. typing is still undone and redone by the widget, Ctrl+Y included,
#      which on Linux the widget does not answer by itself;
#   4. an operation followed by typing undoes the typing, not the operation;
#   5. Ctrl+Z in a new tab does nothing -- Slint 1.17.1 panicked here, with
#      "end byte index 3 is out of bounds for string of length 0", replaying
#      the other tab's typing into an empty document;
#   6. the keyboard still works after answering Replace All's question.
#      It did not: the caret went nowhere until a click, Ctrl+S included.
#
# Needs what `drive-window-xvfb.sh` needs, and a built binary.
set -u

here=$(cd "$(dirname "$0")" && pwd)
failed=0
# Three lines, so a sorted document is a readable diff.
export FIXTURE_LINES=3
sort_desc="F10 Right Down Down Down Down Down Down Down Return"
undo_row="F10 Right Return"
# "abc" to "Z" across the document. Tab reaches the Replace box; Replace All
# is a label, not a control, so it is clicked, at its place on the find bar
# in the 1107-pixel window. Return answers the question it asks.
replace_all="ctrl+f a b c Tab Z click:902,618 Return Escape"

fixture() {
    for i in $(seq -w 1 "$FIXTURE_LINES"); do printf 'line %s abcdefghij\n' "$i"; done
}

# The diff `drive-window-xvfb.sh` prints for KEYS, compared with the diff the
# file WANT would give.
expect() {
    local what=$1 keys=$2 want=$3 got expected
    got=$("$here/drive-window-xvfb.sh" "$keys" 2>&1)
    expected=$(diff <(fixture) <(printf '%s' "$want") | cat -A | sed 's/\$$//')
    if [ "$got" = "$expected" ]; then
        echo "ok:   $what"
    else
        echo "FAIL: $what" >&2
        echo "      keys: $keys" >&2
        echo "$got" | head -8 | sed 's/^/      /' >&2
        failed=1
    fi
}

as_typed='line 1 abcdefghijQ
line 2 abcdefghij
line 3 abcdefghij
'
sorted='line 3 abcdefghijQ
line 2 abcdefghij
line 1 abcdefghij
'

expect "Ctrl+Z undoes Sort Lines" \
    "ctrl+Home $sort_desc ctrl+z ctrl+Home End Q" "$as_typed"
expect "Edit > Undo undoes Sort Lines" \
    "ctrl+Home $sort_desc $undo_row ctrl+Home End Q" "$as_typed"
expect "Ctrl+Z undoes Replace All" \
    "ctrl+Home $replace_all ctrl+z ctrl+Home End Q" "$as_typed"
expect "Ctrl+Y redoes Sort Lines" \
    "ctrl+Home $sort_desc ctrl+z ctrl+y ctrl+Home End Q" "$sorted"
expect "Ctrl+Z undoes typing" \
    "ctrl+Home a b c ctrl+z End Q" "$as_typed"
expect "Ctrl+Y redoes typing" \
    "ctrl+Home a b c ctrl+z ctrl+y End Q" "abc$as_typed"
# The X goes, the sort stays: typing after an operation is the newer step.
expect "after an operation, Ctrl+Z undoes the typing that followed it" \
    "ctrl+Home $sort_desc ctrl+Home X ctrl+z ctrl+Home End Q" "$sorted"
expect "Ctrl+Z in a new tab does nothing" \
    "ctrl+Home a b c ctrl+n ctrl+z ctrl+w ctrl+Home End Q" "abc$as_typed"
expect "the keyboard still works after answering Replace All" \
    "ctrl+Home $replace_all ctrl+Home End Q" 'line 1 ZdefghijQ
line 2 Zdefghij
line 3 Zdefghij
'

exit $failed
