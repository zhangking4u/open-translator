#!/usr/bin/env bash
# Regenerates the copies of the shared UI sources. Run with --check in CI.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

check=0
if [[ "${1:-}" == "--check" ]]; then
  check=1
fi

copies=(
  "shared/ui/dropdown.js|browser/extension/dropdown.js"
  "shared/ui/dropdown.js|desktop/translator-popup-tauri/ui/dropdown.js"
  "shared/ui/tokens.css|browser/extension/tokens.css"
  "shared/ui/tokens.css|desktop/translator-popup-tauri/ui/tokens.css"
)

fail=0
for pair in "${copies[@]}"; do
  src="${pair%%|*}"
  dst="${pair##*|}"

  if [[ $check -eq 1 ]]; then
    if ! cmp -s "$src" "$dst"; then
      echo "out of sync: $dst (run shared/sync-ui.sh)"
      fail=1
    fi
  else
    cp "$src" "$dst"
    echo "synced: $dst"
  fi
done

# tokens.js embeds the CSS into the content-script global; :root -> :host so it
# also applies inside shadow roots.
tmp="$(mktemp)"
node -e '
const fs = require("fs");
const css = fs.readFileSync("shared/ui/tokens.css", "utf8").replace(/:root/g, ":host");
const body =
  "\"use strict\";\n\n" +
  "// Generated from shared/ui/tokens.css by shared/sync-ui.sh; do not edit.\n" +
  "globalThis.OT_TOKENS_CSS = " + JSON.stringify(css) + ";\n";
fs.writeFileSync(process.argv[1], body);
' "$tmp"

if [[ $check -eq 1 ]]; then
  if ! cmp -s "$tmp" browser/extension/tokens.js; then
    echo "out of sync: browser/extension/tokens.js (run shared/sync-ui.sh)"
    fail=1
  fi
  rm -f "$tmp"
else
  mv "$tmp" browser/extension/tokens.js
  echo "synced: browser/extension/tokens.js"
fi

if [[ $check -eq 1 && $fail -ne 0 ]]; then
  exit 1
fi
