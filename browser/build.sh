#!/usr/bin/env bash
# Build loadable extension directories under browser/dist.
#
# Usage: ./build.sh [firefox|chrome|all]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SRC="$SCRIPT_DIR/extension"
DIST="$SCRIPT_DIR/dist"

SHARED_FILES=(background.js content.js options.html options.js)

usage() {
    sed -n '2,4p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

build_target() {
    local name="$1"
    local manifest="$2"
    local out="$DIST/$name"

    rm -rf "$out"
    mkdir -p "$out"

    local file
    for file in "${SHARED_FILES[@]}"; do
        cp "$SRC/$file" "$out/$file"
    done
    cp "$SRC/$manifest" "$out/manifest.json"

    echo "built: $out"
}

target="${1:-all}"

case "$target" in
    firefox) build_target firefox manifest.json ;;
    chrome) build_target chrome manifest.chrome.json ;;
    all)
        build_target firefox manifest.json
        build_target chrome manifest.chrome.json
        ;;
    -h|--help)
        usage
        exit 0
        ;;
    *)
        echo "unknown target: $target" >&2
        usage >&2
        exit 2
        ;;
esac
