#!/usr/bin/env bash
# Find and Go to Line in a document that is not ASCII, then type one letter,
# and check that only what was selected changed (W1-04).
#
#   scripts/drive-offsets-xvfb.sh              # the default surface
#   scripts/drive-offsets-xvfb.sh --editor-view
#
# Everything in this workspace counts characters and `TextInput` counts UTF-8
# bytes. Before `AppState::widget_range` converted between them, going to
# line 3 of a Japanese file and typing a letter joined lines 1 and 2, and
# finding "003" replaced a character in line 2. In an ASCII file the two
# counts agree, which is why no earlier test saw it.
#
# Needs what `drive-window-xvfb.sh` needs, and a built binary.
set -u

flag=${1:-}
here=$(cd "$(dirname "$0")" && pwd)
failed=0
# Three-byte characters on both sides of the line number, so an offset off by
# bytes lands inside a character on the wrong line rather than near the right
# one.
export FIXTURE_LINE='日本語 %s テキスト'

# The whole diff for KEYS, compared with WANT.
expect() {
    local what=$1 keys=$2 want=$3 got
    # shellcheck disable=SC2086 -- an empty flag must vanish, not become "".
    got=$("$here/drive-window-xvfb.sh" "$keys" $flag 2>&1)
    if [ "$got" = "$want" ]; then
        echo "ok:   $what"
    else
        echo "FAIL: $what" >&2
        echo "      keys: $keys" >&2
        echo "$got" | head -6 | sed 's/^/      /' >&2
        failed=1
    fi
}

# `drive-window-xvfb.sh` prints the diff through `cat -A`, so the fixture's
# bytes appear escaped; build the expected lines the same way.
line() { printf "$FIXTURE_LINE" "$1" | cat -A | sed 's/\$$//'; }
with_z() { printf '日本語 Z テキスト' | cat -A | sed 's/\$$//'; }

expect "Go to Line 3, then a letter, replaces line 3" \
    "ctrl+g 3 Return Escape Z" \
    "$(printf '3c3\n< %s\n---\n> Z' "$(line 003)")"
expect "Find 003, then a letter, replaces 003 in line 3" \
    "ctrl+f 0 0 3 Escape Z" \
    "$(printf '3c3\n< %s\n---\n> %s' "$(line 003)" "$(with_z)")"
expect "Find 150, far below the fold, replaces it there" \
    "ctrl+f 1 5 0 Escape Z" \
    "$(printf '150c150\n< %s\n---\n> %s' "$(line 150)" "$(with_z)")"

exit $failed
