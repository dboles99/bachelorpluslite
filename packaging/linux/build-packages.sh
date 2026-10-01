#!/bin/sh
# Build the Linux packages from the release archive (ADR-0093):
#
#   packaging/linux/build-packages.sh VERSION ARCHIVE OUT_DIR
#
# ARCHIVE is the release's .tar.gz, from `scripts/release-linux.sh`, and is
# unpacked here rather than staged again: every format then ships exactly the
# files the archive does -- the binary, `bpad`, `app-help/`, the icon and the
# licence files -- because they are the archive's files.
#
# Produces, in OUT_DIR:
#
#   bachelorpad-lite_VERSION_amd64.deb        Debian, Ubuntu, Mint, Pop!_OS
#   bachelorpad-lite-VERSION-1.x86_64.rpm     Fedora, openSUSE, RHEL-alikes
#   bachelorpad-lite-VERSION-x86_64.AppImage  anything else
#   PKGBUILD                                   for the AUR, checksum filled in
#
# **Built with the distributions' own tools** -- `dpkg-deb` and `rpmbuild` --
# rather than `cargo-deb` and `cargo-generate-rpm`. Both of those work out a
# package's dependencies from the binary's link table, and this binary loads
# almost everything it needs at run time with dlopen -- xkbcommon, X11, xcb,
# Wayland, EGL -- so the tools would declare fontconfig and libc and leave
# the rest to be discovered as a window that never opens. The list below was
# read from the binary instead (`strings`, which is where dlopen's names
# live), and both package formats say it by hand.
#
# POSIX sh, like release-linux.sh beside it, because the distribution this
# runs on is not a promise this repository makes.

set -eu

VERSION="${1:?version}"
ARCHIVE="${2:?release .tar.gz}"
OUT="${3:?output directory}"

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
APP_ID=io.github.dboles99.bachelorpluslite
NAME=bachelorpad-lite

test -f "$ARCHIVE" || { echo "$ARCHIVE is missing -- build the release archive first" >&2; exit 1; }
mkdir -p "$OUT"
OUT=$(CDPATH= cd -- "$OUT" && pwd)
ARCHIVE=$(CDPATH= cd -- "$(dirname -- "$ARCHIVE")" && pwd)/$(basename -- "$ARCHIVE")

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
mkdir "$WORK/unpacked"
tar xzf "$ARCHIVE" -C "$WORK/unpacked"
STAGED="$WORK/unpacked/$NAME-$VERSION-linux-x86_64"
test -x "$STAGED/bachelorpad" || { echo "$ARCHIVE does not hold $NAME-$VERSION-linux-x86_64/bachelorpad" >&2; exit 1; }

# --- the tree every package installs ---------------------------------------
#
# The program and everything it reads *beside itself* -- app-help/ and the
# window icon -- go together under /usr/lib, because that is where the program
# looks: beside its own executable, which `std::env::current_exe` resolves
# through the /usr/bin symlink to the real file. Splitting them the
# conventional way, help in /usr/share and binary in /usr/bin, would give
# Help > User Guide nothing to open.
tree() {
    dest=$1
    mkdir -p "$dest/usr/lib/$NAME" "$dest/usr/bin" \
        "$dest/usr/share/applications" \
        "$dest/usr/share/icons/hicolor/256x256/apps" \
        "$dest/usr/share/metainfo"
    cp -a "$STAGED/." "$dest/usr/lib/$NAME/"
    ln -s "../lib/$NAME/bachelorpad" "$dest/usr/bin/bachelorpad"
    ln -s "../lib/$NAME/bachelorpad" "$dest/usr/bin/bpad"
    install -m 644 "$STAGED/$APP_ID.desktop" "$dest/usr/share/applications/$APP_ID.desktop"
    install -m 644 "$STAGED/$APP_ID.png" "$dest/usr/share/icons/hicolor/256x256/apps/$APP_ID.png"
    install -m 644 "$STAGED/$APP_ID.metainfo.xml" "$dest/usr/share/metainfo/$APP_ID.metainfo.xml"
}

# --- .deb ------------------------------------------------------------------

DEB="$WORK/deb"
tree "$DEB"
mkdir -p "$DEB/DEBIAN" "$DEB/usr/share/doc/$NAME"
cat > "$DEB/usr/share/doc/$NAME/copyright" <<COPYRIGHT
Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Upstream-Name: BachelorPad+ Lite
Source: https://github.com/dboles99/bachelorpluslite

Files: *
Copyright: Daniel Boles
License: GPL-3.0-only
 On Debian systems the full text is in /usr/share/common-licenses/GPL-3.
 The third-party crates linked into the program, and their licences, are
 listed in /usr/lib/$NAME/THIRD-PARTY-NOTICES.md.
COPYRIGHT
# Installed-Size is in KiB, and dpkg reads it to say how much space is needed.
SIZE=$(du -sk "$DEB/usr" | cut -f1)
# Depends: what the binary links or must dlopen to open a window at all.
# Recommends: EGL and GL for the GPU renderer the default does not use, and
# D-Bus and the portal for native file dialogs, which fall back without them.
cat > "$DEB/DEBIAN/control" <<CONTROL
Package: $NAME
Version: $VERSION
Architecture: amd64
Maintainer: Daniel Boles <daniel.boles@gmail.com>
Installed-Size: $SIZE
Depends: libc6 (>= 2.35), libgcc-s1, libfontconfig1, libxkbcommon0, libxkbcommon-x11-0, libx11-6, libx11-xcb1, libxcb1, libxcursor1, libxi6, libxrender1, libwayland-client0
Recommends: libegl1, libgl1, libdbus-1-3, xdg-desktop-portal
Section: editors
Priority: optional
Homepage: https://bpad.prompt-forge.dev
Description: Text editor for notes and logs
 Notepad when you want it, more when you need it. A plain text editor that
 opens instantly, never touches the network and cannot run anything, with a
 note layer that knows which of your documents are about the same subject.
CONTROL
dpkg-deb --root-owner-group --build "$DEB" "$OUT/${NAME}_${VERSION}_amd64.deb" >/dev/null
echo "built ${NAME}_${VERSION}_amd64.deb"

# --- .rpm ------------------------------------------------------------------
#
# Requires by soname rather than by package name: Fedora calls it
# libxkbcommon-x11, openSUSE libxkbcommon-x11-0, and both packages provide
# libxkbcommon-x11.so.0()(64bit). Automatic requirements are off, because
# they would be the link table again.
RPMTOP="$WORK/rpm"
mkdir -p "$RPMTOP/BUILD" "$RPMTOP/RPMS" "$RPMTOP/SOURCES" "$RPMTOP/SPECS" "$RPMTOP/SRPMS"
tree "$RPMTOP/tree"
cat > "$RPMTOP/SPECS/$NAME.spec" <<SPEC
Name:           $NAME
Version:        $VERSION
Release:        1
Summary:        Text editor for notes and logs
License:        GPL-3.0-only
URL:            https://bpad.prompt-forge.dev
BuildArch:      x86_64
AutoReqProv:    no
Requires:       libc.so.6(GLIBC_2.35)(64bit)
Requires:       libfontconfig.so.1()(64bit)
Requires:       libxkbcommon.so.0()(64bit)
Requires:       libxkbcommon-x11.so.0()(64bit)
Requires:       libX11.so.6()(64bit)
Requires:       libX11-xcb.so.1()(64bit)
Requires:       libxcb.so.1()(64bit)
Requires:       libXcursor.so.1()(64bit)
Requires:       libXi.so.6()(64bit)
Requires:       libXrender.so.1()(64bit)
Requires:       libwayland-client.so.0()(64bit)
Recommends:     libEGL.so.1()(64bit)
Recommends:     libdbus-1.so.3()(64bit)
Provides:       bachelorpad = %{version}

%description
Notepad when you want it, more when you need it. A plain text editor that
opens instantly, never touches the network and cannot run anything, with a
note layer that knows which of your documents are about the same subject.

%install
cp -a $RPMTOP/tree/. %{buildroot}/

%files
/usr/lib/$NAME
/usr/bin/bachelorpad
/usr/bin/bpad
/usr/share/applications/$APP_ID.desktop
/usr/share/icons/hicolor/256x256/apps/$APP_ID.png
/usr/share/metainfo/$APP_ID.metainfo.xml
SPEC
# The binary is already stripped by the release build's own settings, and a
# debuginfo package of a stripped binary is empty; both are turned off rather
# than left to fail on a runner with no debug tooling.
rpmbuild --quiet --define "_topdir $RPMTOP" \
    --define "debug_package %{nil}" --define "__strip /bin/true" \
    -bb "$RPMTOP/SPECS/$NAME.spec"
cp "$RPMTOP/RPMS/x86_64/$NAME-$VERSION-1.x86_64.rpm" "$OUT/"
echo "built $NAME-$VERSION-1.x86_64.rpm"

# --- AppImage --------------------------------------------------------------
#
# The staged directory *is* the AppDir: the program finds its help and icon
# beside itself, and AppRun is that program. appimagetool is fetched at a
# pinned version and refused unless its checksum matches the one GitHub
# recorded for that release -- a tool downloaded at build time and run
# unchecked is a stranger's code inside every AppImage.
APPIMAGETOOL_URL=https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage
APPIMAGETOOL_SHA256=ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0
TOOL="$WORK/appimagetool"
curl --proto '=https' --tlsv1.2 -fsSL -o "$TOOL" "$APPIMAGETOOL_URL"
echo "$APPIMAGETOOL_SHA256  $TOOL" | sha256sum -c - >/dev/null || {
    echo "appimagetool's checksum does not match the one pinned here -- refusing to run it" >&2
    exit 1
}
chmod 755 "$TOOL"

APPDIR="$WORK/AppDir"
cp -a "$STAGED/." "$APPDIR/"
ln -s bachelorpad "$APPDIR/AppRun"
# The archive already holds the .desktop file at its root, which is where
# an AppDir wants it.
cp "$STAGED/$APP_ID.png" "$APPDIR/.DirIcon"
mkdir -p "$APPDIR/usr/share/metainfo"
cp "$STAGED/$APP_ID.metainfo.xml" "$APPDIR/usr/share/metainfo/$APP_ID.appdata.xml"
# No FUSE on a CI runner: the tool unpacks itself instead of mounting.
# Its output is kept: a failure's detail is above the summary, and a release
# log that discarded it would hold nothing worth reading (CLAUDE.md, the gate).
APPIMAGE_EXTRACT_AND_RUN=1 ARCH=x86_64 "$TOOL" --no-appstream "$APPDIR" \
    "$OUT/$NAME-$VERSION-x86_64.AppImage"
echo "built $NAME-$VERSION-x86_64.AppImage"

# --- the AUR's PKGBUILD ----------------------------------------------------
#
# Rendered with the release archive's checksum, which only exists once the
# archive does. Publishing it is a push to aur.archlinux.org, which needs an
# AUR account and is not this script's to do.
SUM=$(sha256sum "$ARCHIVE" | cut -d' ' -f1)
sed -e "s/@VERSION@/$VERSION/g" -e "s/@SHA256@/$SUM/g" \
    "$ROOT/packaging/aur/PKGBUILD.in" > "$OUT/PKGBUILD"
echo "rendered PKGBUILD"
