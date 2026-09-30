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


Sprint 2 (preparation)


Goal:

Integrate a local translation model.


Planned:

1. Choose model runtime (Ollama / llama.cpp server / in-process — TBD)

2. Add model adapter behind engine config

3. Extend config with model address, model name and timeout


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
