#!/usr/bin/env bash
# Build loadable extension directories (and optionally zips) under browser/dist.
#
# Usage: ./build.sh [firefox|chrome|all] [--zip]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SRC="$SCRIPT_DIR/extension"
DIST="$SCRIPT_DIR/dist"
PACKAGING="$SCRIPT_DIR/../packaging/browser"

SHARED_FILES=(background.js content.js dropdown.js languages.js tokens.css tokens.js options.html options.js popup.html popup.js result.html result.js)

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

    if [ -f "$PACKAGING/README.txt" ]; then
        cp "$PACKAGING/README.txt" "$out/README.txt"
    fi

    echo "built: $out"
}

zip_target() {
    local name="$1"
    local version
    version="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["version"])' "$DIST/$name/manifest.json")"
    local archive="$DIST/open-translator-$name-$version.zip"

    rm -f "$archive"

    if command -v zip >/dev/null 2>&1; then
        (cd "$DIST" && zip -q -r "$archive" "$name")
    else
        (cd "$DIST" && python3 - "$archive" "$name" <<'PY'
import os
import sys
import zipfile

archive, folder = sys.argv[1], sys.argv[2]

with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as bundle:
    for root, _dirs, files in os.walk(folder):
        for name in files:
            path = os.path.join(root, name)
            bundle.write(path, path)
PY
        )
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
        if [ "$with_zip" -eq 1 ]; then
            zip_target firefox
        fi
        ;;
    chrome)
        build_target chrome manifest.chrome.json
        if [ "$with_zip" -eq 1 ]; then
            zip_target chrome
        fi
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
