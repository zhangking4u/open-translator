#!/usr/bin/env bash
# End-to-end smoke test of the Chrome MV3 build against a local service.
#
# Requires Chrome/Edge (override with CHROME_BIN), Node 22+, python3 and curl.
# A translator service is started with the mock engine if none is running.
#
# Usage: ./test.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

PAGE_PORT="${PAGE_PORT:-8099}"
DEBUG_PORT="${DEBUG_PORT:-9222}"
SERVICE_URL="${TRANSLATOR_SERVICE_URL:-http://127.0.0.1:17890}"

BROWSER="${CHROME_BIN:-}"
if [ -z "$BROWSER" ]; then
    for candidate in google-chrome google-chrome-stable chromium microsoft-edge; do
        if command -v "$candidate" >/dev/null 2>&1; then
            BROWSER="$candidate"
            break
        fi
    done
fi
if [ -z "$BROWSER" ]; then
    echo "no Chrome/Edge found; set CHROME_BIN" >&2
    exit 1
fi

"$SCRIPT_DIR/build.sh" chrome

SERVICE_PID=""
PAGE_PID=""
BROWSER_PID=""
PROFILE_DIR="$(mktemp -d)"

cleanup() {
    local status=$?

    [ -n "$BROWSER_PID" ] && kill "$BROWSER_PID" 2>/dev/null || true
    [ -n "$PAGE_PID" ] && kill "$PAGE_PID" 2>/dev/null || true
    [ -n "$SERVICE_PID" ] && kill "$SERVICE_PID" 2>/dev/null || true

    [ -n "$BROWSER_PID" ] && wait "$BROWSER_PID" 2>/dev/null || true
    rm -rf "$PROFILE_DIR" 2>/dev/null || true

    exit "$status"
}
trap cleanup EXIT

if ! curl -fsS -m 2 "$SERVICE_URL/health" >/dev/null 2>&1; then
    echo "starting translator service (mock engine)..."
    (cd "$REPO_ROOT/core/translator-service" && cargo build --quiet)
    TRANSLATOR_ENGINE=mock "$REPO_ROOT/core/translator-service/target/debug/translator-service" \
        >/dev/null 2>&1 &
    SERVICE_PID=$!

    for _ in $(seq 1 120); do
        if curl -fsS -m 2 "$SERVICE_URL/health" >/dev/null 2>&1; then
            break
        fi
        sleep 0.5
    done

    if ! curl -fsS -m 2 "$SERVICE_URL/health" >/dev/null 2>&1; then
        echo "translator service did not become ready" >&2
        exit 1
    fi
fi

python3 -m http.server "$PAGE_PORT" --directory "$SCRIPT_DIR" >/dev/null 2>&1 &
PAGE_PID=$!

"$BROWSER" --headless=new --no-first-run --no-default-browser-check \
    --user-data-dir="$PROFILE_DIR" --remote-debugging-port="$DEBUG_PORT" \
    about:blank >/dev/null 2>&1 &
BROWSER_PID=$!

for _ in $(seq 1 100); do
    if curl -fsS -m 2 "http://127.0.0.1:$DEBUG_PORT/json/version" >/dev/null 2>&1; then
        break
    fi
    sleep 0.2
done

node "$SCRIPT_DIR/test-chrome.mjs" \
    --browser "http://127.0.0.1:$DEBUG_PORT" \
    --extension "$SCRIPT_DIR/dist/chrome" \
    --page "http://127.0.0.1:$PAGE_PORT/test-page.html"
