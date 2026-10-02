#!/usr/bin/env bash
# Install the OpenTranslator macOS desktop client (Tauri) for the current user:
#   - builds the release client (the engine is embedded)
#   - creates ~/Applications/OpenTranslator.app
#   - installs a LaunchAgent so the client starts at login
#
# Usage: ./install-macos.sh [--uninstall]
#
# After the first launch, grant Accessibility permission to OpenTranslator in
# System Settings -> Privacy & Security -> Accessibility (needed to send Cmd+C).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
APP_NAME="OpenTranslator"
APP_DIR="$HOME/Applications/$APP_NAME.app"
BIN_NAME="translator-popup-tauri"
LABEL="io.github.opentranslator.popup"
PLIST="$HOME/Library/LaunchAgents/$LABEL.plist"

if [ "${1:-}" = "--uninstall" ]; then
    launchctl bootout "gui/$(id -u)" "$PLIST" 2>/dev/null || true
    rm -f "$PLIST"
    rm -rf "$APP_DIR"
    echo "removed $APP_DIR and $PLIST"
    exit 0
fi

echo "Building the release client..."
(cd "$SCRIPT_DIR/translator-popup-tauri" && cargo build --release)

BIN="$SCRIPT_DIR/translator-popup-tauri/target/release/$BIN_NAME"

if [ ! -x "$BIN" ]; then
    echo "release binary missing after build: $BIN" >&2
    exit 1
fi

MODEL_FILE="$HOME/Library/Application Support/open-translator/models/hy-mt1.5-1.8b-q4_k_m.gguf"
if [ ! -f "$MODEL_FILE" ]; then
    echo "warning: model not found at $MODEL_FILE"
    echo "         place a .gguf there, or set model_path in ~/Library/Application Support/open-translator/config"
fi

mkdir -p "$APP_DIR/Contents/MacOS"
cp "$BIN" "$APP_DIR/Contents/MacOS/$APP_NAME"
cp "$SCRIPT_DIR/../packaging/macos/Info.plist" "$APP_DIR/Contents/Info.plist"

mkdir -p "$HOME/Library/LaunchAgents"
cat > "$PLIST" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>$LABEL</string>
    <key>ProgramArguments</key>
    <array>
        <string>$APP_DIR/Contents/MacOS/$APP_NAME</string>
        <string>--autostart</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>
EOF

launchctl bootout "gui/$(id -u)" "$PLIST" 2>/dev/null || true
launchctl bootstrap "gui/$(id -u)" "$PLIST"

echo
echo "Installed:"
echo "  app:     $APP_DIR"
echo "  agent:   $PLIST"
echo
echo "Select text and press Ctrl+Alt+T (Ctrl+C capture). Esc hides the window."
echo "Tray menu: 显示窗口 / 立即翻译 / 历史… / 设置… / 退出."
echo "First run: allow OpenTranslator under System Settings -> Privacy & Security -> Accessibility."
echo "Logs: ~/Library/Logs/open-translator/"
