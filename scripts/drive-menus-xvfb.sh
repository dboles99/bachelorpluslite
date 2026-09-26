#!/usr/bin/env bash
# Drive the menu bar with the keyboard alone and check the saved file after
# each run (ADR-0088).
#
#   scripts/drive-menus-xvfb.sh              # the default surface
#   scripts/drive-menus-xvfb.sh --editor-view
#
# Every case ends in a row whose effect lands in the file -- Edit > Select All
# followed by a letter replaces the whole document with that letter -- so the
# evidence is bytes, as in `drive-window-xvfb.sh`, which this runs once per
# case. What it proves, in order:
#
#   1. F10 opens File, Right walks to Edit, Down and Return run a row;
#   2. letters typed while a menu is open do not reach the document, and
#      Escape gives the caret back;
#   3. a menu opened a second time takes the keyboard again;
#   4. Left walks back, and wraps from File to Help and back;
#   5. Return on a greyed row does nothing;
#   6. a menu opened with the mouse answers the arrows too.
#
# Needs what `drive-window-xvfb.sh` needs, and a built binary.
set -u

flag=${1:-}
here=$(cd "$(dirname "$0")" && pwd)
failed=0
down5="Down Down Down Down Down"

# The last three lines of the diff `drive-window-xvfb.sh` prints for KEYS --
# enough to tell every case here apart -- compared with WANT.
expect() {
    local what=$1 keys=$2 want=$3 got
    # shellcheck disable=SC2086 -- an empty flag must vanish, not become "".
    got=$("$here/drive-window-xvfb.sh" "$keys" $flag 2>&1 | tail -n 3)
    if [ "$got" = "$want" ]; then
        echo "ok:   $what"
    else
        echo "FAIL: $what" >&2
        echo "      keys: $keys" >&2
        echo "      got:  $(echo "$got" | head -3 | tr '\n' '|')" >&2
        failed=1
    fi
}

# Select All then X leaves one line, "X", with no newline after it.
replaced='---
> X
\ No newline at end of file'
first_q='< line 001 abcdefghij
---
> Qline 001 abcdefghij'

expect "F10, Right, Down and Return run Edit > Select All" \
    "ctrl+Home F10 Right $down5 Return X" "$replaced"
expect "letters typed in an open menu stay out of the document" \
    "ctrl+Home F10 a b Escape Q" "$first_q"
expect "a menu opened a second time takes the keyboard again" \
    "ctrl+Home F10 Escape F10 Right $down5 Return X" "$replaced"
expect "Left walks back from View to Edit" \
    "ctrl+Home F10 Right Right Left $down5 Return X" "$replaced"
expect "Left from File wraps to Help, and Right comes back" \
    "ctrl+Home F10 Left Right Right $down5 Return X" "$replaced"
if [ -z "$flag" ]; then
    # Duplicate Line needs the caret only the custom surface exposes, so it is
    # greyed here -- and Return on it must do nothing.
    expect "Return on a greyed row does nothing" \
        "ctrl+Home F10 Right End Up Up Up Return Escape Q" "$first_q"
else
    expect "Return on Duplicate Line duplicates the line" \
        "ctrl+Home F10 Right End Up Up Up Return Q" \
        "$(printf '1a2\n> Qline 001 abcdefghij')"
fi
# Edit's label is at about x=70 on the menu bar. A mouse-opened menu starts
# with nothing highlighted, so the sixth Down reaches Select All.
expect "a menu opened with the mouse answers the arrows" \
    "ctrl+Home click:70,14 $down5 Down Return X" "$replaced"

exit $failed
