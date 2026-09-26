# Print what a screen reader is handed: every accessible object under the
# application named on the command line, one per line, indented by depth.
#
#   [role] 'name' desc='description' {states}
#
# Used by check-accessibility-xvfb.sh, which greps it. It names only the states
# a reader announces, so a line changes when what is said changes and not
# otherwise.
import sys

import pyatspi

STATES = [
    (pyatspi.STATE_EXPANDABLE, "expandable"),
    (pyatspi.STATE_EXPANDED, "expanded"),
    (pyatspi.STATE_SELECTED, "selected"),
    (pyatspi.STATE_CHECKED, "checked"),
    (pyatspi.STATE_SENSITIVE, "sensitive"),
    (pyatspi.STATE_FOCUSED, "focused"),
]


def describe(node):
    held = node.getState()
    states = ",".join(name for state, name in STATES if held.contains(state))
    desc = f" desc={node.description!r}" if node.description else ""
    return f"[{node.getRoleName()}] {node.name!r}{desc} {{{states}}}"


def walk(node, depth):
    print("  " * depth + describe(node))
    for i in range(node.childCount):
        child = node.getChildAtIndex(i)
        if child is not None:
            walk(child, depth + 1)


want = sys.argv[1].lower()
desktop = pyatspi.Registry.getDesktop(0)
found = False
for i in range(desktop.childCount):
    app = desktop.getChildAtIndex(i)
    if app is not None and want in (app.name or "").lower():
        walk(app, 0)
        found = True
sys.exit(0 if found else 1)
