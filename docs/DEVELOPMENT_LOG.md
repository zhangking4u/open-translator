# OpenTranslator Development Log


# 2026-09-29


## Milestone: Project Bootstrap


Completed:

- Initialized Git repository
- Configured Rust development environment
- Created Rust translator-service project
- Added Axum HTTP server


Implemented:


## Core API


GET:

/health


Response:


{
    "status":"ok",
    "service":"translator-core"
}


POST:

/translate


Current behavior:

Returns mock translation result.


Example:


Input:

hello world


Output:

[Mock Translation] hello world



---

# 2026-09-30


## Milestone: API Layer Refactor and Test Foundation


Completed:

- Moved routes, handlers and DTOs from `main.rs` into `src/api/mod.rs`
- Added library target (`src/lib.rs`); `main.rs` is now bootstrap only
- Made `TranslationEngine` async and dyn-compatible; API state is `Arc<dyn TranslationEngine>`
- Added `MockEngine` unit test and `/health`, `/translate` integration tests (tower `oneshot`)
- Added dev-dependencies `tower` and `http-body-util`


Verification:

cargo test


## Milestone: Engine Error Path and Configuration


Completed:

- `TranslationEngine::translate` now returns `Result<TranslationResult, TranslationError>`
- API error contract: JSON `{"error":{"kind","message"}}` with 400 invalid request, 500 internal, 502 engine unavailable, 504 timeout
- Added `config` module and engine factory: `TRANSLATOR_BIND_ADDR` (default `127.0.0.1:17890`), `TRANSLATOR_ENGINE` (default `mock`)
- Added tests for empty input rejection, engine failure mapping and engine kind parsing


Verification:

cargo test


---


## Milestone: Timeout Guard and Prompt Foundation


Completed:

- Added `TimeoutEngine`; `engine::build` wraps every engine and returns `TranslationError::Timeout` on expiry (HTTP 504)
- Added `TRANSLATOR_TIMEOUT_MS` config (default 30000, must be > 0)
- Added `domain::language` (tag normalization + common language names) and `domain::prompt::translation_prompt`
- Added tests for timeout mapping, timeout parsing, tag normalization and prompt building


Verification:

cargo test


Runtime environment check:

- ollama / llama.cpp / docker not installed on the dev machine; model runtime still to be chosen


---


## Milestone: Local Model Runtime (Ollama) and Benchmark


Completed:

- Installed Ollama v0.35.0 user-space at `~/.local/opt/ollama` (release asset fetched via gh-proxy; ollama.com and GitHub release-assets are unreachable from this network)
- Started `ollama serve` and pulled `qwen2.5:7b` (4.7 GB, from registry.ollama.ai)
- Benchmarked on i5-14400 CPU only: cold start 6.1s (4.5s model load); warm short en→zh 0.45s total; 113-char sentence 2.1s; generation ~9.5–12 tok/s


Notes:

- Loaded model uses ~5.1 GB RAM, 100% CPU, 4096 context
- Qwen2.5-7B quality is acceptable but can be literal ("kernel panic" → 内核恐慌, expected 内核崩溃); a dedicated MT model (e.g. Hunyuan-MT) can be swapped in later since the model name will be configurable


Verification:

- `curl http://127.0.0.1:11434/api/generate` with prompts in `domain::prompt` format
- `cargo test` unaffected (service still Mock-only)


---


## Milestone: Ollama Engine Adapter


Completed:

- Added `OllamaEngine` (`src/engine/ollama.rs`): posts `domain::prompt::translation_prompt` output to `{TRANSLATOR_MODEL_URL}/api/generate` and parses the `response` field
- Added `EngineKind::Ollama`; `engine::build` now builds from `Config`
- New config: `TRANSLATOR_MODEL_URL` (default `http://127.0.0.1:11434`), `TRANSLATOR_MODEL` (required when engine is `ollama`)
- Added `reqwest` (no default TLS features; local HTTP only)
- Tests: config resolution, engine kind parsing, stub-server integration (`tests/ollama.rs`) covering success, HTTP errors, invalid JSON and connection failure


Verification:

cargo test (24 tests)


Live end-to-end (qwen2.5:7b, CPU):

- cold 6.4s (model load), warm en→zh 1.7s, warm zh→en 0.9s
- Missing `TRANSLATOR_MODEL` aborts startup with exit 1


---


## Milestone: Model Quality Evaluation


Setup: 12 cases (10 en→zh, 2 zh→en), temperature 0, four configs — qwen2.5:7b / qwen2.5:3b with the generic prompt, translategemma:4b with its official template and with the generic prompt.


Findings:

- translategemma:4b + official prompt wins on quality: correct technical terms ("kernel panic" → 内核崩溃, "segmentation fault" → 段错误) and idiomatic phrasing ("Break a leg!" → 祝您演出成功！, "free lunch" → “天上不会掉馅饼。”)
- qwen2.5:7b is usable but literal ("kernel panic" → 内核恐慌, "Let me know if you have any questions." → 让我知道如果你有任何问题。)
- qwen2.5:3b mixes English into terms ("内核 Panic", "段页 fault") — not suitable
- Warm latency (short text, CPU): 3b ~0.3–0.8s, 4b ~0.8–2.1s, 7b ~0.5–1.8s — all within the target
- TranslateGemma's official prompt matters for idioms; the generic prompt keeps technical terms but flattens idioms


Decision:

- Recommended default: `translategemma:4b` with the model-specific (official) prompt; `qwen2.5:7b` as fallback model
- Follow-up: the adapter needs a prompt-style option so TranslateGemma can use its official template
- License note: TranslateGemma is distributed under the Gemma Terms of Use


Verification:

- Raw results: `/tmp/kilo/eval_results.json` (ephemeral)


---


## Milestone: HY-MT Evaluation


Setup: HuggingFace is unreachable, so HY-MT GGUFs came from ModelScope (`Tencent-Hunyuan/HY-MT1.5-1.8B-GGUF`, `Tencent-Hunyuan/Hy-MT2-1.8B-GGUF`, ~24 MB/s) and were imported via `ollama create`. Serving HY-MT correctly requires its official chat template (`<｜hy_begin▁of▁sentence｜><｜hy_User｜>…<｜hy_Assistant｜>`, absent from the GGUF) and its recommended sampling (temperature 0.7, top_p 0.6, top_k 20, repetition_penalty 1.05). Prompts use the official Chinese form: 将以下文本翻译为<目标语言>，注意只需要输出翻译后的结果，不要额外解释：


Results (12 cases, warm):

- hy-mt1.5-1.8b: 0.11–0.55s, 34–47 tok/s; "kernel panic" → 内核崩溃, "segmentation fault" → 分段错误, natural phrasing on UI/technical strings
- hy-mt2-1.8b: 0.11–0.51s, 35–48 tok/s; "segmentation fault" → 段错误, "kernel panic" → 内核恐慌; marginally less natural on some sentences
- Both beat qwen2.5:7b on latency (>4× faster) and quality; translategemma:4b still wins on idioms (free lunch → 天上不会掉馅饼) but is 4–5× slower


Decision:

- Recommended default for desktop/low latency: `hy-mt1.5-1.8b` (Q4_K_M)
- Quality option: `translategemma:4b`; general fallback: `qwen2.5:7b`
- Follow-up: the adapter needs per-model prompt style and sampling config (currently generic prompt + temperature 0)


Verification:

- Raw results: `/tmp/kilo/eval_hymt_results.json` (ephemeral)


---


## Milestone: Per-Model Prompt Styles and Sampling


Completed:

- `domain::prompt` now exposes `PromptStyle` (`generic`, `translategemma`, `hymt`) and `translation_prompt(style, request)`; `domain::language` gained Chinese display names (`display_name_zh`)
- `OllamaEngine` sends the style's prompt and recommended sampling options (`hymt`: temperature 0.7, top_p 0.6, top_k 20, repeat_penalty 1.05; others: temperature 0)
- New config `TRANSLATOR_PROMPT_STYLE` (default `generic`); unknown styles abort startup with exit 1
- Added `models/import-hymt-ollama.sh` to import HY-MT GGUFs with the official chat template (missing template in the GGUF was the root cause of degraded HY-MT output)
- Tests: prompt styles, Chinese language names, stub assertions for per-style sampling options


Verification:

cargo test (29 tests)


Live end-to-end (`TRANSLATOR_ENGINE=ollama TRANSLATOR_MODEL=hy-mt1.5-1.8b TRANSLATOR_PROMPT_STYLE=hymt`):

- "kernel panic" → 内核崩溃 (1.8s cold, 0.12s warm), "segmentation fault" → 分段错误, "Break a leg!" → 祝你好运！, "请稍后再试。" → Please try again later.


---


## Milestone: Service Hardening


Completed:

- Switched to `tracing` + `tracing-subscriber` (RUST_LOG, default info); request logs carry source/target/text_chars/elapsed_ms and error kind, never the raw text
- Added startup warmup (`TRANSLATOR_WARMUP`, default true): preloads the model so the first request is warm (live: first request 0.13s vs 1.8s cold); warmup failure logs a warning and does not block startup
- `/health` now reports `engine` and `model` from config (API state is `AppState { engine, engine_name, model }`)
- Tests: health fields, warmup failure tolerance, warmup flag parsing


Verification:

cargo test (32 tests)


Live checks:

- `{"status":"ok","service":"translator-core","engine":"ollama","model":"hy-mt1.5-1.8b"}`
- Warmup log "engine warmup completed"; with an unreachable model URL the service logs a warning and still starts


---


## Milestone: Desktop Phase 0 — Selection Popup


Completed:

- New crate `desktop/translator-popup`: reads the Wayland primary selection (or clipboard), calls `POST /translate` and shows the result in a zenity popup
- Flags: `--source/-s`, `--target/-t`, `--clipboard`, `--stdin`, `--print`, `--service` (default `$TRANSLATOR_SERVICE_URL`)


Findings (GNOME Wayland):

- `wl-clipboard-rs` cannot be used: it requires the data-control protocol, which Mutter does not implement (both set and read fail)
- GUI processes cannot set the selection without keyboard focus, so scripted selection injection is not possible; test setup needs a real focused app
- Reading via `wl-paste` (wl-clipboard package) is the GNOME-compatible path


Verification:

- cargo test (4 tests)
- `echo "kernel panic" | translator-popup --stdin --print` → 内核崩溃 against local Ollama (hy-mt1.5-1.8b)
- Unreachable service exits 1 with a clear message


Real selection verification (2026-09-30, wl-clipboard installed):

- `wl-copy --primary "kernel panic"` + `translator-popup --print` → 内核崩溃
- `wl-copy "Break a leg!"` + `translator-popup --clipboard --print` → 祝你好运！
- GNOME custom shortcut "translator-popup" (`Ctrl+Alt+T`, no conflict with the empty default terminal binding) points at `desktop/translator-popup/target/release/translator-popup`
- Release binary re-verified with a real selection: graceful-shutdown sentence translated in 0.64s


---


## Milestone: Desktop 1a — Service Self-Start


Completed:

- Popup now checks the core `/health` before translating; if down it starts ollama (when the model URL is local) and the core release binary, polling until ready
- Detached spawn with logs under `~/.local/state/open-translator/` (`ollama.log`, `translator-service.log`); the child processes survive the popup exiting
- Paths derive from the repo layout (`core/translator-service/target/release/translator-service`, `~/.local/opt/ollama/bin/ollama`) and can be overridden with `TRANSLATOR_CORE_BIN` / `TRANSLATOR_OLLAMA_BIN`; `--no-start` disables auto-start
- Started core inherits `TRANSLATOR_BIND_ADDR` derived from `--service` when the URL is local


Verification:

cargo test (9 tests)


Live cold start (both services stopped):

- `echo "kernel panic" | translator-popup --stdin --print` → 内核崩溃 in 2.7s (starts ollama + core, waits for warmup)
- Warm path 0.24s; `--no-start` and remote-URL errors exit 1 with actionable messages


---


# Git History


Commit:

331d1da

Message:

feat: initialize translator core service



Commit:

510a56e

Message:

docs: add project status document



---

# Current Sprint


Sprint 3 (preparation)


Goal:

Desktop selection translation (Ubuntu / GNOME Wayland).


Planned:

1. Choose the desktop integration approach (GNOME Shell extension recommended; Wayland blocks global hotkeys/clipboard injection)

2. Minimal loop: hotkey → selection capture → core `/translate` → popup


Sprint 2 (completed 2026-09-30):

1. Ollama runtime installed and benchmarked

2. Ollama engine adapter with configurable model

3. Model evaluation (HY-MT / TranslateGemma) and per-model prompt styles

4. Service hardening (tracing, warmup, health)


Sprint 1.2 (completed 2026-09-30):

1. Create domain model

2. Define TranslationEngine trait (async, fallible)

3. Implement MockTranslationEngine

4. Refactor API layer

5. Add unit and integration tests


---

# Future Milestones


## Sprint 2

Integrate local translation model.


## Sprint 3

Desktop selection translation.


## Sprint 4

Browser integration.


## Sprint 5

Meeting translation.


## Sprint 6

Mobile client.
