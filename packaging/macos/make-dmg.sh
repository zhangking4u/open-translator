#!/usr/bin/env bash
# Build a macOS .dmg containing OpenTranslator.app from a release binary.
#
# Usage: make-dmg.sh <popup-binary> <output.dmg>

set -euo pipefail

BIN="${1:?usage: $0 <popup-binary> <output.dmg>}"
OUT="${2:?usage: $0 <popup-binary> <output.dmg>}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Version baked into the app bundle: explicit VERSION (CI passes the tag),
# then the latest tag, then a placeholder.
if [ -z "${VERSION:-}" ]; then
    VERSION="$(git -C "$REPO_ROOT" describe --tags --always 2>/dev/null || true)"
    VERSION="${VERSION#v}"

    if ! [[ "$VERSION" =~ ^[0-9] ]]; then
        VERSION="0.0.0"
    fi
fi

if [ ! -x "$BIN" ]; then
    echo "popup binary not found: $BIN" >&2
    exit 1
fi

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

APP="$STAGE/OpenTranslator.app"
mkdir -p "$APP/Contents/MacOS"
cp "$BIN" "$APP/Contents/MacOS/OpenTranslator"
cp "$REPO_ROOT/packaging/macos/Info.plist" "$APP/Contents/Info.plist"

# Optional: the pinned ONNX Runtime used by screenshot OCR, loaded from the
# directory next to the binary at runtime.
if [ -n "${ORT_DYLIB:-}" ]; then
    if [ ! -f "$ORT_DYLIB" ]; then
        echo "ONNX Runtime dylib not found: $ORT_DYLIB" >&2
        exit 1
    fi
    cp "$ORT_DYLIB" "$APP/Contents/MacOS/libonnxruntime.dylib"
fi

/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $VERSION" "$APP/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $VERSION" "$APP/Contents/Info.plist"

hdiutil create -volname "OpenTranslator" -srcfolder "$STAGE" -ov -format UDZO "$OUT"

echo "created $OUT (version $VERSION)"
