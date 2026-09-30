#!/usr/bin/env bash
# Import a HY-MT GGUF into Ollama with the official chat template and
# recommended sampling parameters. Without the template the model degrades
# (e.g. outputs "内核 panic" instead of "内核崩溃").
#
# Get the GGUF from ModelScope (HuggingFace is unreachable on the dev machine):
#   https://modelscope.cn/models/Tencent-Hunyuan/HY-MT1.5-1.8B-GGUF
#   https://modelscope.cn/models/Tencent-Hunyuan/Hy-MT2-1.8B-GGUF
#
# Usage: ./import-hymt-ollama.sh <path-to.gguf> [ollama-model-name]
set -euo pipefail

GGUF="${1:?usage: $0 <path-to.gguf> [model-name]}"
NAME="${2:-hy-mt1.5-1.8b}"

OLLAMA_BIN="${OLLAMA_BIN:-ollama}"
if ! command -v "$OLLAMA_BIN" >/dev/null 2>&1; then
    OLLAMA_BIN="$HOME/.local/opt/ollama/bin/ollama"
fi

MODELFILE="$(mktemp)"
trap 'rm -f "$MODELFILE"' EXIT

{
    printf 'FROM %s\n' "$GGUF"
    cat <<'EOF'
TEMPLATE <｜hy_begin▁of▁sentence｜><｜hy_User｜>{{ .Prompt }}<｜hy_Assistant｜>
PARAMETER temperature 0.7
PARAMETER top_p 0.6
PARAMETER top_k 20
PARAMETER repeat_penalty 1.05
PARAMETER stop <｜hy_place▁holder▁no▁2｜>
EOF
} > "$MODELFILE"

"$OLLAMA_BIN" create "$NAME" -f "$MODELFILE"
"$OLLAMA_BIN" list
