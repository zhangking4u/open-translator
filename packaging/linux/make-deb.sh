#!/usr/bin/env bash
# Build a Debian package for the Linux/GNOME OpenTranslator client.
#
# Usage:
#   VERSION=0.1.0 ARCH=amd64 ./make-deb.sh
#
# Expects release binaries at the default cargo locations (override with
# POPUP_BIN / SERVICE_BIN):
#   desktop/translator-popup/target/release/translator-popup
#   core/translator-service/target/release/translator-service
#
# Output: packaging/linux/build/OpenTranslator-linux-<arch>.deb

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

if [ -z "${VERSION:-}" ]; then
    VERSION="$(git -C "$REPO_ROOT" describe --tags --always 2>/dev/null || true)"
    VERSION="${VERSION#v}"

    # Shallow clones have no tags; `git describe --always` then returns a bare
    # SHA, which is not a valid package version.
    if ! [[ "$VERSION" =~ ^[0-9] ]]; then
        VERSION="0.1.0"
    fi
fi

if ! [[ "$VERSION" =~ ^[0-9] ]]; then
    echo "invalid VERSION: $VERSION" >&2
    exit 1
fi

ARCH="${ARCH:-$(dpkg --print-architecture)}"
POPUP_BIN="${POPUP_BIN:-$REPO_ROOT/desktop/translator-popup/target/release/translator-popup}"
SERVICE_BIN="${SERVICE_BIN:-$REPO_ROOT/core/translator-service/target/release/translator-service}"
OUT_DIR="${OUT_DIR:-$SCRIPT_DIR/build}"

case "$ARCH" in
    amd64) RELEASE_ARCH="x64" ;;
    arm64) RELEASE_ARCH="arm64" ;;
    *) RELEASE_ARCH="$ARCH" ;;
esac

for bin in "$POPUP_BIN" "$SERVICE_BIN"; do
    if [ ! -x "$bin" ]; then
        echo "missing binary: $bin (build it with cargo build --release)" >&2
        exit 1
    fi
done

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
chmod 755 "$STAGE"

install -Dm755 "$POPUP_BIN" "$STAGE/usr/lib/open-translator/translator-popup"
install -Dm755 "$SERVICE_BIN" "$STAGE/usr/lib/open-translator/translator-service"

mkdir -p "$STAGE/usr/bin"
ln -s /usr/lib/open-translator/translator-popup "$STAGE/usr/bin/translator-popup"

install -Dm755 "$SCRIPT_DIR/open-translator-setup" "$STAGE/usr/bin/open-translator-setup"
install -Dm644 "$SCRIPT_DIR/open-translator.desktop" \
    "$STAGE/usr/share/applications/io.github.opentranslator.popup.desktop"
install -Dm644 "$SCRIPT_DIR/io.github.opentranslator.popup.svg" \
    "$STAGE/usr/share/icons/hicolor/scalable/apps/io.github.opentranslator.popup.svg"
install -Dm644 "$SCRIPT_DIR/README.txt" "$STAGE/usr/share/doc/open-translator/README.txt"

mkdir -p "$STAGE/DEBIAN"
install -m755 "$SCRIPT_DIR/postinst" "$STAGE/DEBIAN/postinst"

INSTALLED_SIZE="$(du -sk "$STAGE/usr" | cut -f1)"

cat > "$STAGE/DEBIAN/control" <<EOF
Package: open-translator
Version: $VERSION
Architecture: $ARCH
Maintainer: OpenTranslator Maintainers <noreply@github.com>
Section: utils
Priority: optional
Homepage: https://github.com/zhangking4u/open-translator
Installed-Size: $INSTALLED_SIZE
Depends: libc6 (>= 2.39), libgtk-4-1, wl-clipboard, libgomp1
Recommends: gnome-shell
Description: Local-first AI selection translation
 Select text anywhere, press a global shortcut, and get an AI translation
 from a model running fully on your machine (llama.cpp, HY-MT 1.8B).
 .
 The model (~1.1 GB) is downloaded from ModelScope on first use.
 Run "open-translator-setup" once to bind the global shortcut.
EOF

mkdir -p "$OUT_DIR"
OUT="$OUT_DIR/OpenTranslator-linux-$RELEASE_ARCH.deb"
dpkg-deb --build --root-owner-group "$STAGE" "$OUT" >/dev/null

echo "built $OUT"
