#!/usr/bin/env bash
# Build loadable extension directories (and optionally zips) under browser/dist.
#
# Usage: ./build.sh [firefox|chrome|all] [--zip]

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

zip_target() {
    local name="$1"
    local version
    version="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["version"])' "$DIST/$name/manifest.json")"
    local archive="$DIST/open-translator-$name-$version.zip"

    rm -f "$archive"

    if command -v zip >/dev/null 2>&1; then
        (cd "$DIST/$name" && zip -q -r "$archive" .)
    else
        (cd "$DIST/$name" && python3 -m zipfile -c "$archive" "${SHARED_FILES[@]}" manifest.json)
    fi

    echo "packaged: $archive"
}

target=""
with_zip=0

while [ $# -gt 0 ]; do
    case "$1" in
        firefox|chrome|all) target="$1" ;;
        --zip) with_zip=1 ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "unknown argument: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
    shift
done

target="${target:-all}"

case "$target" in
    firefox)
        build_target firefox manifest.json
        [ "$with_zip" -eq 1 ] && zip_target firefox
        ;;
    chrome)
        build_target chrome manifest.chrome.json
        [ "$with_zip" -eq 1 ] && zip_target chrome
        ;;
    all)
        build_target firefox manifest.json
        build_target chrome manifest.chrome.json
        if [ "$with_zip" -eq 1 ]; then
            zip_target firefox
            zip_target chrome
        fi
        ;;
esac
