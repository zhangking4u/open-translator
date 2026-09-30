#!/usr/bin/env bash
# Build a macOS .dmg containing OpenTranslator.app from a release binary.
#
# Usage: make-dmg.sh <popup-binary> <output.dmg>

set -euo pipefail

BIN="${1:?usage: $0 <popup-binary> <output.dmg>}"
OUT="${2:?usage: $0 <popup-binary> <output.dmg>}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

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

hdiutil create -volname "OpenTranslator" -srcfolder "$STAGE" -ov -format UDZO "$OUT"

echo "created $OUT"
