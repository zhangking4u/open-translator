#!/usr/bin/env bash
# Install the OpenTranslator desktop integration for the current user:
#   - builds the release binaries (core service + popup)
#   - delegates shortcut registration to packaging/linux/open-translator-setup
#
# The popup starts the core service automatically on first use; the model is
# downloaded on first run (TRANSLATOR_ENGINE=ollama switches to a local Ollama).
#
# Usage:
#   ./install.sh [--binding "<Control><Alt>t"] [--name NAME] [--uninstall] [--help]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
POPUP_BIN="$SCRIPT_DIR/translator-popup/target/release/translator-popup"
SETUP="$REPO_ROOT/packaging/linux/open-translator-setup"

usage() {
    sed -n '2,10p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

for arg in "$@"; do
    case "$arg" in
        -h|--help)
            usage
            exit 0
            ;;
        --uninstall)
            exec "$SETUP" --bin "$POPUP_BIN" "$@"
            ;;
    esac
done

if [ ! -x "$SETUP" ]; then
    echo "setup script missing: $SETUP" >&2
    exit 1
fi

echo "Building release binaries..."
(cd "$REPO_ROOT/core/translator-service" && cargo build --release)
(cd "$SCRIPT_DIR/translator-popup" && cargo build --release)

if [ ! -x "$POPUP_BIN" ]; then
    echo "popup binary missing after build: $POPUP_BIN" >&2
    exit 1
fi

exec "$SETUP" --bin "$POPUP_BIN" "$@"
