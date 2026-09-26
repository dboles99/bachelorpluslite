# Print what a screen reader is handed: every accessible object under the
# application named on the command line, one per line, indented by depth.
#
#   [role] 'name' desc='description' {states}
#
# Used by check-accessibility-xvfb.sh, which greps it. It names only the states
# a reader announces, so a line changes when what is said changes and not
# otherwise.
#
# With `--do NAME`, it instead performs the first action of the first object
# called NAME -- what a reader's "activate" sends -- and prints nothing.
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


def find(node, name):
    if node.name == name:
        return node
    for i in range(node.childCount):
        child = node.getChildAtIndex(i)
        hit = find(child, name) if child is not None else None
        if hit is not None:
            return hit
    return None


want = sys.argv[1].lower()
act = sys.argv[3] if len(sys.argv) > 3 and sys.argv[2] == "--do" else None
desktop = pyatspi.Registry.getDesktop(0)
found = False
for i in range(desktop.childCount):
    app = desktop.getChildAtIndex(i)
    if app is None or want not in (app.name or "").lower():
        continue
    if act is None:
        walk(app, 0)
        found = True
    else:
        target = find(app, act)
        if target is not None:
            target.queryAction().doAction(0)
            found = True
sys.exit(0 if found else 1)
