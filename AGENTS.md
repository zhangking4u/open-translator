# AGENTS.md

OpenTranslator: local-first AI translation platform. Two crates exist: `core/translator-service` (HTTP service + engines) and `desktop/translator-popup` (Wayland selection popup, Phase 0). `tests/` is an empty placeholder; `README.md` is empty — `docs/` is the project context.

## Repo layout / build

- The core crate is `core/translator-service`. There is no root Cargo.toml or workspace: run all cargo commands from the crate directory:
  - `cargo run` — Axum server at `TRANSLATOR_BIND_ADDR` (default `http://127.0.0.1:17890`)
  - `cargo test` — unit tests plus `tests/api.rs` (tower `oneshot`) and `tests/ollama.rs` (local axum stub server) integration tests; no external services needed
  - `cargo check`
- `desktop/translator-popup` is a separate crate: run cargo from `desktop/translator-popup/`. It calls the core service over HTTP, reads the Wayland selection via `wl-paste` (from the `wl-clipboard` package, installed on the dev machine) and shows a GTK4 window (building it needs `libgtk-4-dev` + `pkg-config`); `--stdin` and `--print` keep it scriptable and testable. `~/.config/open-translator/config` (`service_url`/`source`/`target`) is read with CLI > file > env precedence; the in-window target-language dropdown re-translates and writes `target` back to that file. On startup it checks `/health` and auto-starts ollama + the core release binary if needed (detached, logs in `~/.local/state/open-translator/`); paths default to the repo layout and `~/.local/opt/ollama/bin/ollama`, overridable via `TRANSLATOR_CORE_BIN` / `TRANSLATOR_OLLAMA_BIN`; `--no-start` disables this. `desktop/install.sh` builds both crates in release and registers/updates the GNOME shortcut (`--uninstall` removes it). GNOME/Mutter does not implement the data-control protocol, so `wl-clipboard-rs` cannot be used.
- `browser/extension` holds shared extension sources (plain JS, no bundler): Firefox uses `manifest.json` (MV2, `data_collection_permissions.required = ["none"]`, `strict_min_version` 142), Chrome uses `manifest.chrome.json` (MV3, service worker, `host_permissions`); `browser/build.sh [firefox|chrome|all] [--zip]` copies them into loadable dirs/zips under `browser/dist/` (gitignored) and `browser/sign.sh` signs the Firefox build via `npx web-ext sign --channel unlisted` (needs `WEB_EXT_API_KEY`/`WEB_EXT_API_SECRET`; the xpi download is the last step — do not interrupt; AMO rejects reused version numbers, so bump the manifest version before re-signing). Validate with `python3 -m json.tool *.json`, `node --check *.js` and `npx web-ext lint --source-dir browser/dist/firefox`; load via `about:debugging` (Firefox) or `chrome://extensions` (Chrome). Note: match patterns cannot contain ports, so the host permission is `http://127.0.0.1/*`. Headless smoke test without a GUI: launch Chrome/Edge with `--headless=new --remote-debugging-port`, then CDP `Extensions.loadUnpacked` + `Runtime.evaluate` in the service worker and the content-script isolated world (`--load-extension` is ignored by Chrome 137+).
- `Cargo.lock` is committed in both crates; keep them.
- CI lives in `.github/workflows/ci.yml`: `cargo test` for both crates (the desktop job installs `libgtk-4-dev` + `pkg-config`), browser static checks + `web-ext lint`, and a Chrome e2e job running `browser/test.sh` (starts a mock-engine service if none is running). License: MIT (`LICENSE`).
- A root `tests/` dir exists but is not a Cargo test dir — Rust tests belong under `core/translator-service/tests/` or as `#[cfg(test)]` modules. `models/` holds the HY-MT → Ollama import script (`import-hymt-ollama.sh`).
- Root `.gitignore` ignores Rust `target/` and `browser/dist/`; `.kilo/.gitignore` covers JS tooling files inside `.kilo/` only.
- Active branch is `main`.
- Dev-machine runtime (not repo state): Ollama v0.35.0 at `~/.local/opt/ollama` (models in `~/.ollama/models`; installed: `hy-mt1.5-1.8b`, `hy-mt2-1.8b` — imported from ModelScope GGUFs with official chat template, `translategemma:4b`, `qwen2.5:7b`, `qwen2.5:3b`; start with `~/.local/opt/ollama/bin/ollama serve`). Network quirk: `ollama.com` and HuggingFace are unreachable; ModelScope works and GitHub release assets need a proxy such as `https://gh-proxy.com/`.
- No CI workflows and no rustfmt/clippy config; verify locally with `cargo test`. Existing sources are not rustfmt-formatted, so don't run repo-wide `cargo fmt` unprompted.

## Architecture (current state)

`src/lib.rs` exposes `api`, `config`, `domain`, `engine`; `src/main.rs` is bootstrap only (`Config::from_env()` → `engine::build(&config)` → `api::router(...)` → bind).

Flow: `src/api/mod.rs` (router + handlers + DTOs + error mapping) → `src/domain/` (`translation.rs` types/errors, `language.rs` tag normalization, `prompt.rs` prompt styles) → `src/engine/mod.rs` (`TranslationEngine` trait, `EngineKind`, `build` factory) → `src/engine/mock.rs` / `src/engine/ollama.rs` / `src/engine/timeout.rs` (`MockEngine`, `OllamaEngine`, `TimeoutEngine` wrapper).

- `TranslationEngine::translate` is async and dyn-compatible: it returns `TranslationFuture` (`Pin<Box<dyn Future<Output = Result<TranslationResult, TranslationError>> + Send + '_>>`), and the trait has `Send + Sync` supertraits so API state is `EngineRef` (`Arc<dyn TranslationEngine>`). New engines must use this signature — plain `async fn` in the trait would break `dyn` support.
- `engine::build` wraps every engine in `TimeoutEngine`; an expired translation becomes `TranslationError::Timeout` → HTTP 504.
- Errors: engines return `TranslationError::{InvalidRequest, EngineUnavailable, Timeout, Internal}`; `api` implements `IntoResponse` mapping them to 400/502/504/500 with body `{"error":{"kind","message"}}`. Empty/whitespace `text` is rejected in the handler with 400.
- API: `GET /health` → `{"status":"ok","service":"translator-core","engine":"<kind>","model":"<model>"}`; `POST /translate` body `{"text","source","target"}` → `{"translation":"[Mock Translation] <text>"}`.
- Logging uses `tracing` (`RUST_LOG`, default `info`); request logs carry source/target/text_chars/elapsed_ms and error kind but never the raw text.
- Startup warmup is on by default (`TRANSLATOR_WARMUP=false` to disable); warmup failure warns and does not block startup.
- `source`/`target` are accepted but unused by `MockEngine`; `OllamaEngine` builds prompts via `domain::prompt::translation_prompt` (styles: `generic`, `translategemma`, `hymt`) and posts to `{TRANSLATOR_MODEL_URL}/api/generate`, mapping HTTP/connection failures to `EngineUnavailable`. Each style sends its recommended sampling options (`hymt`: 0.7/0.6/20/1.05; others: temperature 0).
- Startup config is env-only: `TRANSLATOR_BIND_ADDR`, `TRANSLATOR_ENGINE` (default `mock`; `ollama` requires `TRANSLATOR_MODEL`), `TRANSLATOR_TIMEOUT_MS` (default `30000`, must be > 0), `TRANSLATOR_MODEL_URL` (default `http://127.0.0.1:11434`), `TRANSLATOR_MODEL`, `TRANSLATOR_PROMPT_STYLE` (default `generic`), `TRANSLATOR_WARMUP` (`true`/`false`, default `true`), `TRANSLATOR_KEEP_ALIVE` (Ollama keep-alive, default `30m`), `TRANSLATOR_MAX_CHARS` (per-request character cap, default `1500`, over-limit → 400); invalid values abort startup with exit 1. `source` may be `auto` (prompts omit the source language; HY-MT's template is already source-agnostic).

## Conventions

- Commits use conventional prefixes: `feat:`, `docs:` (see `git log`).
- `docs/PROJECT_STATUS.md` and `docs/DEVELOPMENT_LOG.md` are kept as living sprint logs; update them when sprint scope/progress changes, and fix stale statements when touching related code.
- `docs/ARCHITECTURE.md` is the design intent (modularity, model independence, local-first); keep the core crate model-agnostic and platform-independent.
