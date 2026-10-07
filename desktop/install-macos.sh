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

# Pinned ONNX Runtime for screenshot OCR, loaded from the directory next to
# the binary at runtime. (ONNX Runtime 1.28.2 has no x86_64 macOS build.)
ORT_VERSION="1.28.2"
ORT_SHA256="c4fceacfc53765d0869dc9180c31ec91054d149017a99d1e80ffe28dc79596de"
ORT_DYLIB="$APP_DIR/Contents/MacOS/libonnxruntime.dylib"
ORT_ARCH="$(uname -m)"

if [ ! -f "$ORT_DYLIB" ]; then
    if [ "$ORT_ARCH" = "arm64" ]; then
        echo "Downloading ONNX Runtime $ORT_VERSION for screenshot OCR..."
        ORT_TMP="$(mktemp -d)"
        ORT_ARCHIVE="$ORT_TMP/onnxruntime-osx-arm64-$ORT_VERSION.tgz"
        curl -L -o "$ORT_ARCHIVE" "https://github.com/microsoft/onnxruntime/releases/download/v$ORT_VERSION/onnxruntime-osx-arm64-$ORT_VERSION.tgz"
        echo "$ORT_SHA256  $ORT_ARCHIVE" | shasum -a 256 -c -
        tar -xzf "$ORT_ARCHIVE" -C "$ORT_TMP"
        for dylib in "$ORT_TMP/onnxruntime-osx-arm64-$ORT_VERSION/lib/libonnxruntime."*.dylib; do
            cp "$dylib" "$ORT_DYLIB"
            break
        done
        rm -rf "$ORT_TMP"
    else
        echo "warning: no ONNX Runtime build for macOS $ORT_ARCH at $ORT_VERSION; screenshot OCR stays unavailable"
    fi
fi

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
echo "Tray menu: 显示窗口 / 历史… / 设置… / 退出."
echo "First run: allow OpenTranslator under System Settings -> Privacy & Security -> Accessibility."
echo "Logs: ~/Library/Logs/open-translator/"
