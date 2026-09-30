#!/usr/bin/env bash
# Sign the Firefox build with web-ext (AMO, unlisted channel) and drop the
# signed .xpi into browser/dist/signed/.
#
# Requires AMO API credentials (one-time setup):
#   https://addons.mozilla.org/developers/addon/api/key/
#   export WEB_EXT_API_KEY=... WEB_EXT_API_SECRET=...
#
# Usage: ./sign.sh [--source-dir DIR] [--artifacts-dir DIR]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SOURCE_DIR="$SCRIPT_DIR/dist/firefox"
ARTIFACTS_DIR="$SCRIPT_DIR/dist/signed"

while [ $# -gt 0 ]; do
    case "$1" in
        --source-dir)
            SOURCE_DIR="${2:?--source-dir requires a value}"
            shift 2
            ;;
        --artifacts-dir)
            ARTIFACTS_DIR="${2:?--artifacts-dir requires a value}"
            shift 2
            ;;
        -h|--help)
            sed -n '2,10p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            echo "unknown argument: $1" >&2
            exit 2
            ;;
    esac
done

if [ ! -f "$SOURCE_DIR/manifest.json" ]; then
    echo "no build found at $SOURCE_DIR; run ./build.sh firefox first" >&2
    exit 1
fi

if [ -z "${WEB_EXT_API_KEY:-}" ] || [ -z "${WEB_EXT_API_SECRET:-}" ]; then
    echo "WEB_EXT_API_KEY / WEB_EXT_API_SECRET are not set." >&2
    echo "Create AMO credentials at https://addons.mozilla.org/developers/addon/api/key/ and export them." >&2
    exit 1
fi

mkdir -p "$ARTIFACTS_DIR"

if command -v web-ext >/dev/null 2>&1; then
    WEB_EXT=(web-ext)
else
    WEB_EXT=(npx --yes web-ext)
fi

"${WEB_EXT[@]}" sign \
    --source-dir "$SOURCE_DIR" \
    --artifacts-dir "$ARTIFACTS_DIR" \
    --channel unlisted \
    --no-input

echo
echo "signed artifact(s):"
ls -l "$ARTIFACTS_DIR"
