#!/usr/bin/env bash
# release.sh -- release naivepost, or build it as a Flatpak or an AppImage.
#
#   ./release.sh              -> next tag (v0.1, v0.2, ...), pushed; GitHub
#                                Actions then builds both packages and attaches
#                                them to the release (.github/workflows/release.yml)
#   ./release.sh flatpak      -> dist/naivepost-<git describe>.flatpak, here
#   ./release.sh appimage     -> dist/naivepost-<git describe>-x86_64.AppImage, here
#   VERSION=1.0 ./release.sh appimage   -> dist/naivepost-1.0-x86_64.AppImage
#   DISTRO=debian:forky ./release.sh appimage   (the default)
#
# The Flatpak is the one to prefer where Flatpak exists: the GNOME runtime
# already carries GTK4, the GTK4 video sink, GStreamer and ffmpeg with h264,
# so nothing is bundled by us and the whole thing is the manifest,
# ch.bocek.naivepost.yml. It needs flatpak-builder on the host and the GNOME 50
# SDK plus its Go extension, which the first run installs from Flathub.
#
# The AppImage is for machines without Flatpak. It carries GTK4 and GStreamer
# inside, and runs on the glibc of the machine it is opened on, so it is built
# INSIDE A CONTAINER of a fixed distribution, never on the host: a binary built
# against Arch's glibc will not open on anything older. The container is
# Debian testing (forky: GLib 2.89, GTK 4.22, glibc 2.43), and the reason it
# cannot be older is the Go bindings: gotk4 0.4.1 calls GLib functions that
# arrived in 2.86 (g_get_monotonic_time_ns, g_source_dup_context, ...), and
# Debian 13 stops at 2.84, where the build fails at compile time -- tried, not
# guessed. So the AppImage runs on distributions from 2026 on. For older ones
# pin an older gotk4 in gui/go.mod and set DISTRO=debian:trixie, which does
# package the GTK4 video sink (gstreamer1.0-gtk4); Ubuntu 22.04 and 24.04 do
# not package it at all.
#
# Needs on the host: docker or podman, git. Nothing else; Go, GTK, GStreamer,
# linuxdeploy and its plugins are fetched into the container.
#
# Not bundled in the AppImage, on purpose: ffmpeg/ffprobe (the app finds them
# on PATH; bundling a full ffmpeg is a licensing question this script does not
# answer) and the four model servers, which are the user's to run. Also left to
# the host, by linuxdeploy's standard exclude list: glibc, libstdc++, GL/EGL,
# X11/Wayland client libraries, fontconfig, freetype, harfbuzz, fribidi -- and
# an icon theme (adwaita-icon-theme) for the few symbolic icons GTK does not
# carry itself. Checked by running the AppImage on a Debian testing container
# with exactly those and nothing else: the window comes up, all icons drawn.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DISTRO="${DISTRO:-debian:forky}"
GO_VERSION="${GO_VERSION:-$(sed -n 's/^go \([0-9.]*\).*/\1/p' "$ROOT/gui/go.mod")}"
VERSION="${VERSION:-$(git -C "$ROOT" describe --tags --always --dirty 2>/dev/null || echo dev)}"
ARCH="$(uname -m)"
OUT="$ROOT/dist"
APP=naivepost
APPID=ch.bocek.naivepost

# ---- flatpak -------------------------------------------------------------------
if [ "${1:-}" = flatpak ]; then
  command -v flatpak-builder >/dev/null || { echo "release.sh: needs flatpak-builder (pacman -S flatpak-builder)" >&2; exit 1; }
  flatpak remote-info --user flathub org.gnome.Sdk//50 >/dev/null 2>&1 || \
    flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
  # flatpak-builder builds with the network off, so the modules come along in
  # the tree. gui/vendor is build output, not source: it is not committed.
  echo ">>> vendoring go modules"
  (cd "$ROOT/gui" && go mod vendor)
  mkdir -p "$OUT"
  echo ">>> flatpak-builder"
  flatpak-builder --user --install-deps-from=flathub --force-clean \
    --repo="$OUT/repo" "$OUT/flatpak-build" "$ROOT/$APPID.yml"
  flatpak build-bundle "$OUT/repo" "$OUT/$APP-$VERSION.flatpak" "$APPID"
  rm -rf "$ROOT/gui/vendor"
  echo
  echo "built: $OUT/$APP-$VERSION.flatpak"
  echo "install: flatpak install --user $OUT/$APP-$VERSION.flatpak   (codecs-extra comes with it)"
  echo "it needs the four servers running; ffmpeg is inside."
  exit 0
fi

# ---- release: tag and push, the workflow does the building ---------------------
if [ "${1:-release}" = release ]; then
  [ -z "$(git -C "$ROOT" status --porcelain)" ] || { echo "release.sh: commit or stash first, the tag would not contain these changes:" >&2; git -C "$ROOT" status --short >&2; exit 1; }
  last="$(git -C "$ROOT" tag --list 'v*' --sort=-v:refname | head -1)"
  if [ -z "$last" ]; then next=v0.1
  else next="${last%.*}.$(( ${last##*.} + 1 ))"
  fi
  echo ">>> $last -> $next on $(git -C "$ROOT" rev-parse --short HEAD)"
  git -C "$ROOT" tag -a "$next" -m "naivepost $next"
  git -C "$ROOT" push origin "$next"
  echo "pushed $next; the release workflow builds the AppImage and the Flatpak and attaches them:"
  echo "  $(git -C "$ROOT" remote get-url origin | sed 's/\.git$//; s#^git@github.com:#https://github.com/#')/actions"
  exit 0
fi

# ---- appimage ------------------------------------------------------------------
[ "$1" = appimage ] || { echo "release.sh: usage: ./release.sh [release|flatpak|appimage]" >&2; exit 1; }
if command -v docker >/dev/null; then ENGINE=docker
elif command -v podman >/dev/null; then ENGINE=podman
else echo "release.sh: needs docker or podman" >&2; exit 1; fi

if [ "$ARCH" != x86_64 ]; then
  echo "release.sh: linuxdeploy's plugins ship for x86_64; on $ARCH build natively (see the inner script)" >&2
  exit 1
fi

mkdir -p "$OUT"
echo ">>> building $APP $VERSION in $DISTRO with go $GO_VERSION"

# Everything below runs inside the container. It is one script, passed in
# whole, so that the build is the same whether it is read here or run by hand.
INNER=$(cat <<'EOF'
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
APP=naivepost; APPID=ch.bocek.naivepost
cd /src

echo ">>> packages"
apt-get update -qq
apt-get install -y -qq --no-install-recommends \
  ca-certificates curl git build-essential pkg-config file desktop-file-utils patchelf librsvg2-bin \
  libgtk-4-dev libgraphene-1.0-dev libcairo2-dev libpango1.0-dev libgdk-pixbuf-2.0-dev \
  libjson-glib-dev libgirepository1.0-dev \
  libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev libgstreamer-plugins-bad1.0-dev \
  gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-plugins-bad \
  gstreamer1.0-plugins-ugly gstreamer1.0-libav gstreamer1.0-gl gstreamer1.0-tools \
  gstreamer1.0-gtk4 || true
# The video sink is a Rust plugin (gst-plugins-rs) and not every release
# packages it. Without it the preview is a black square, so this is a hard
# stop, not a warning.
if ! gst-inspect-1.0 gtk4paintablesink >/dev/null 2>&1; then
  echo "!!! gtk4paintablesink is not in $DISTRO's packages." >&2
  echo "    Build gst-plugin-gtk4 from gst-plugins-rs (cargo cinstall -p gst-plugin-gtk4)" >&2
  echo "    or use the default DISTRO, debian:forky, which ships it." >&2
  exit 1
fi

echo ">>> go $GO_VERSION"
curl -fsSL "https://go.dev/dl/go${GO_VERSION}.linux-amd64.tar.gz" | tar -C /usr/local -xz
export PATH=/usr/local/go/bin:$PATH GOFLAGS=-mod=mod GOCACHE=/tmp/gocache

echo ">>> build"
cd gui
# -buildvcs=false: the checkout is mounted from another user, so git inside
# the container refuses it, and the version is passed in by hand anyway
CGO_ENABLED=1 go build -trimpath -buildvcs=false -ldflags "-s -w" -o "/tmp/$APP" .
cd ..

echo ">>> AppDir"
D=/tmp/AppDir; rm -rf "$D"
install -Dm755 "/tmp/$APP" "$D/usr/bin/$APP"
# the icon tree twice: where freedesktop looks, and beside the binary, which is
# where the app itself looks first (gui/icon.go iconDirs). usr/share must exist
# first, or cp renames the tree to usr/share instead of putting it inside.
mkdir -p "$D/usr/share"
cp -r gui/icons "$D/usr/share/"
ln -s ../share/icons "$D/usr/bin/icons"
install -Dm644 "gui/icons/hicolor/scalable/apps/$APPID.svg" "$D/$APPID.svg"
# linuxdeploy looks the desktop file's icon up in the sized directories only,
# so the SVG is rasterised into them; GTK on the target machine gets a PNG it
# can always decode, too
for px in 16 32 48 64 128 256; do
  install -d "$D/usr/share/icons/hicolor/${px}x${px}/apps"
  rsvg-convert -w $px -h $px "gui/icons/hicolor/scalable/apps/$APPID.svg" \
    -o "$D/usr/share/icons/hicolor/${px}x${px}/apps/$APPID.png"
done
mkdir -p "$D/usr/share/applications" "$D/usr/share/metainfo"
cat > "$D/usr/share/applications/$APPID.desktop" <<DESK
[Desktop Entry]
Type=Application
Name=Naivepost
Comment=An almost linear video editor
Exec=$APP %f
Icon=$APPID
Terminal=false
Categories=AudioVideo;Video;AudioVideoEditing;
MimeType=video/x-matroska;video/mp4;video/webm;video/quicktime;
DESK
cat > "$D/usr/share/metainfo/$APPID.metainfo.xml" <<META
<?xml version="1.0" encoding="UTF-8"?>
<component type="desktop-application">
  <id>$APPID</id>
  <name>Naivepost</name>
  <summary>An almost linear video editor</summary>
  <metadata_license>CC0-1.0</metadata_license>
  <description><p>A desktop editor for sessions you have already recorded. Four workflows, run in order: Prepare, Cut, Narrate, Produce. Models on your own machine do the tedious half, and every answer they give is a proposal you can overrule.</p></description>
  <launchable type="desktop-id">$APPID.desktop</launchable>
  <url type="homepage">https://github.com/tbocek/naivepost</url>
  <provides><binary>$APP</binary></provides>
  <releases><release version="$VERSION" date="$(date +%F)"/></releases>
</component>
META

echo ">>> linuxdeploy"
cd /tmp
for f in linuxdeploy-x86_64.AppImage linuxdeploy-plugin-gtk.sh linuxdeploy-plugin-gstreamer.sh; do
  case $f in
    linuxdeploy-x86_64.AppImage) u=https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/$f ;;
    linuxdeploy-plugin-gtk.sh)   u=https://raw.githubusercontent.com/linuxdeploy/linuxdeploy-plugin-gtk/master/$f ;;
    linuxdeploy-plugin-gstreamer.sh) u=https://raw.githubusercontent.com/linuxdeploy/linuxdeploy-plugin-gstreamer/master/$f ;;
  esac
  [ -f "$f" ] || curl -fsSL -o "$f" "$u"; chmod +x "$f"
done
# The gtk plugin copies two module trees unconditionally that Debian's current
# GTK stack no longer has: $libdir/gtk-4.0 (the media-backend modules that
# lived there are gone from GTK 4.22, and the app does not use them: video
# goes through gtk4paintablesink) and the gdk-pixbuf loader directory
# (gdk-pixbuf 2.44 decodes png and jpeg itself; there are no loaders to ship).
# Empty directories keep the plugin going.
mkdir -p "$(pkg-config --variable=libdir gtk4)/gtk-4.0" \
         "$(pkg-config --variable=gdk_pixbuf_binarydir gdk-pixbuf-2.0)/loaders"
# no FUSE in a container: the tools unpack themselves instead
export APPIMAGE_EXTRACT_AND_RUN=1 DEPLOY_GTK_VERSION=4 GSTREAMER_INCLUDE_BAD_PLUGINS=1 \
  LINUXDEPLOY_OUTPUT_VERSION="$VERSION" \
  OUTPUT="/out/$APP-$VERSION-x86_64.AppImage"
./linuxdeploy-x86_64.AppImage --appdir "$D" \
  --desktop-file "$D/usr/share/applications/$APPID.desktop" \
  --icon-file "$D/usr/share/icons/hicolor/256x256/apps/$APPID.png" \
  --plugin gtk --plugin gstreamer \
  --output appimage
chmod +x "$OUTPUT"
echo ">>> $OUTPUT"
ls -la "$OUTPUT"
EOF
)

$ENGINE run --rm \
  -v "$ROOT:/src:ro" \
  -v "$OUT:/out" \
  -e VERSION="$VERSION" -e GO_VERSION="$GO_VERSION" -e DISTRO="$DISTRO" \
  "$DISTRO" bash -c "$INNER"

echo
echo "built: $OUT/$APP-$VERSION-x86_64.AppImage"
echo "it needs ffmpeg and ffprobe on PATH, an icon theme, and the four servers running (Settings names them)."
