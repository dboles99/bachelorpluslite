#!/bin/sh
# Build, stage and archive the Linux release. Called by scripts/New-Release.ps1
# through WSL; runnable on its own from a real Linux box, which is the point.
#
# **This is a file rather than a string inside the PowerShell script**, and
# the reason is worth keeping. It was a string first: PowerShell quoting,
# inside `bash -c`, around a bash `$(...)` containing an `awk '{print $2}'`
# is three levels of escaping over one line, and it produced a version check
# that compared an empty string and reported nothing useful about why. The
# rule that generalises: **when a quoted string needs a second quoted string
# inside a third, write a file.**
#
# Everything here is deliberately POSIX sh -- no bashisms -- because the
# distro on the other side of WSL is not a promise this repository makes.

set -eu

VERSION="${1:?version}"
COMMIT="${2:?commit}"
OUT="${3:?output directory}"

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$ROOT"

# The Linux toolchain, explicitly. WSL inherits the Windows PATH through
# interop, which already holds the Windows ~/.cargo/bin, while a rustup
# install into the distro lands in $HOME/.cargo/bin and does not reach a
# non-interactive shell. Harmless on a real Linux box.
PATH="$HOME/.cargo/bin:$PATH"
export PATH

# The stem follows the public name (ADR-0074).
NAME="bachelorpad-lite-$VERSION-linux-x86_64"

# Staged inside the distro's own filesystem, not in $OUT.
#
# $OUT is on a DrvFs mount when this runs under WSL, and **DrvFs reports 0777
# for everything and ignores `chmod`** -- so a tarball built there stores
# world-writable licence files, which is true of the mount and of nothing
# anybody intended. `chmod` was tried first and silently did nothing, which is
# the failure mode worth naming: a mode-setting call that succeeds and changes
# no mode. Only the finished archive crosses to $OUT.
STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT
DIR="$STAGE/$NAME"

cargo build --release -p bachelorpad

rm -rf "$DIR"
mkdir -p "$DIR"
cp target/release/bachelorpad "$DIR/bachelorpad"
chmod 755 "$DIR/bachelorpad"

# The short spelling specs.md section 19 promises. A symlink rather than a
# second copy, and made here rather than on the Windows side because creating
# a symlink from Windows needs a privilege this release should not want.
ln -s bachelorpad "$DIR/bpad"

cp README.md LICENSE THIRD-PARTY-NOTICES.md "$DIR/"

# The in-app help, generated from docs/ (ADR-0075). Help > User Guide opens
# it as a document rather than in a browser, so it has to travel in the
# archive -- the same argument the icon already makes.
test -f app-help/index.md || { echo "app-help/index.md is missing -- run Build-AppHelp.ps1; Help would open nothing" >&2; exit 1; }
cp -r app-help "$DIR/app-help"
find "$DIR/app-help" -type d -exec chmod 755 {} +
find "$DIR/app-help" -type f -exec chmod 644 {} +

# The icon, beside the binary rather than in an icon theme (ADR-0068). With no
# installer there is no step that could place it in one, so the `.desktop`
# file written by Set as Default Editor points at it here.
ICON=io.github.dboles99.BachelorPadPlus.png
test -f "assets/$ICON" || { echo "assets/$ICON is missing -- the .desktop file would name nothing" >&2; exit 1; }
cp "assets/$ICON" "$DIR/$ICON"

# Asked of the Linux binary rather than assumed from the Windows one. Two
# builds of one workspace reporting different versions is a thing that should
# stop a release, not be discovered by whoever downloads the wrong half.
#
# `sed` rather than `head | awk`: with a pipeline, the first process gets
# SIGPIPE when the second closes early, and under `set -o pipefail` that is a
# failure with no message. `sed 1q` reads one line and exits 0.
version_lines=$("$DIR/bachelorpad" --version)
reported=$(printf '%s\n' "$version_lines" | sed -n '1s/^[^ ]* *\([^ ]*\).*/\1/p')
if [ "$reported" != "$VERSION" ]; then
    echo "the Linux build reports '$reported'; the Windows build reports '$VERSION'" >&2
    exit 1
fi

# The name and the licence come from those same three lines, and that is a
# fix rather than a tidy-up. BUILD.txt said "BachelorPad+" here and
# "BachelorPlusLite" in New-Release.ps1 -- two literals for one fact, which is
# the naming defect ADR-0054 fixed in `--version` still alive in the half of
# the release script nobody re-read. Two literals cannot disagree if there are
# none.
DISPLAY_NAME=${version_lines%% *}
DECLARED_LICENCE=$(printf '%s\n' "$version_lines" | sed -n '3p')
test -n "$DISPLAY_NAME" || { echo "--version did not report a name" >&2; exit 1; }
test -n "$DECLARED_LICENCE" || { echo "--version did not report a licence" >&2; exit 1; }

cat > "$DIR/BUILD.txt" <<BUILDINFO
$DISPLAY_NAME $VERSION
commit:   $COMMIT
target:   linux-x86_64
signed:   no -- see ADR-0055
licence:  $DECLARED_LICENCE; the text is in LICENSE, beside this file
notices:  THIRD-PARTY-NOTICES.md lists every dependency and its licence
help:     app-help/index.md, which Help > User Guide opens
BUILDINFO

# Modes an extracting user should get, set here because here they stick.
chmod 755 "$DIR" "$DIR/bachelorpad"
chmod 644 "$DIR/README.md" "$DIR/LICENSE" "$DIR/THIRD-PARTY-NOTICES.md" "$DIR/BUILD.txt"

mkdir -p "$OUT"
rm -f "$OUT/$NAME.tar.gz"
tar czf "$OUT/$NAME.tar.gz" -C "$STAGE" "$NAME"
echo "staged $NAME.tar.gz"
