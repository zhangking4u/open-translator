# AGENTS.md

OpenTranslator: local-first AI translation platform. Only the Rust core service exists so far; `desktop/`, `models/`, and `tests/` are empty placeholders. `README.md` is empty — `docs/` is the project context.

## Repo layout / build

- The only crate is `core/translator-service`. There is no root Cargo.toml or workspace: run all cargo commands from `core/translator-service/`:
  - `cargo run` — Axum server at `TRANSLATOR_BIND_ADDR` (default `http://127.0.0.1:17890`)
  - `cargo test` — unit tests plus `tests/api.rs` (tower `oneshot`) and `tests/ollama.rs` (local axum stub server) integration tests; no external services needed
  - `cargo check`
- `Cargo.lock` is committed; keep it.
- A root `tests/` dir exists but is not a Cargo test dir — Rust tests belong under `core/translator-service/tests/` or as `#[cfg(test)]` modules.
- Root `.gitignore` ignores `package.json`, lockfiles, and `node_modules` (legacy template leftovers), so JS tooling files would be silently ignored.
- Active branch is `main`.
- Dev-machine runtime (not repo state): Ollama v0.35.0 at `~/.local/opt/ollama` (models in `~/.ollama/models`; start with `~/.local/opt/ollama/bin/ollama serve`). Network quirk: `ollama.com` and HuggingFace are unreachable; GitHub release assets need a proxy such as `https://gh-proxy.com/`.
- No CI workflows and no rustfmt/clippy config; verify locally with `cargo test`. Existing sources are not rustfmt-formatted, so don't run repo-wide `cargo fmt` unprompted.

## Architecture (current state)

`src/lib.rs` exposes `api`, `config`, `domain`, `engine`; `src/main.rs` is bootstrap only (`Config::from_env()` → `engine::build(&config)` → `api::router(...)` → bind).

Flow: `src/api/mod.rs` (router + handlers + DTOs + error mapping) → `src/domain/` (`translation.rs` types/errors, `language.rs` tag normalization, `prompt.rs` MT prompt) → `src/engine/mod.rs` (`TranslationEngine` trait, `EngineKind`, `build` factory) → `src/engine/mock.rs` / `src/engine/ollama.rs` / `src/engine/timeout.rs` (`MockEngine`, `OllamaEngine`, `TimeoutEngine` wrapper).

- `TranslationEngine::translate` is async and dyn-compatible: it returns `TranslationFuture` (`Pin<Box<dyn Future<Output = Result<TranslationResult, TranslationError>> + Send + '_>>`), and the trait has `Send + Sync` supertraits so API state is `EngineRef` (`Arc<dyn TranslationEngine>`). New engines must use this signature — plain `async fn` in the trait would break `dyn` support.
- `engine::build` wraps every engine in `TimeoutEngine`; an expired translation becomes `TranslationError::Timeout` → HTTP 504.
- Errors: engines return `TranslationError::{InvalidRequest, EngineUnavailable, Timeout, Internal}`; `api` implements `IntoResponse` mapping them to 400/502/504/500 with body `{"error":{"kind","message"}}`. Empty/whitespace `text` is rejected in the handler with 400.
- API: `GET /health` → `{"status":"ok","service":"translator-core"}`; `POST /translate` body `{"text","source","target"}` → `{"translation":"[Mock Translation] <text>"}`.
- `source`/`target` are accepted but unused by `MockEngine`; `OllamaEngine` builds prompts via `domain::prompt::translation_prompt` and posts to `{TRANSLATOR_MODEL_URL}/api/generate`, mapping HTTP/connection failures to `EngineUnavailable`.
- Startup config is env-only: `TRANSLATOR_BIND_ADDR`, `TRANSLATOR_ENGINE` (default `mock`; `ollama` requires `TRANSLATOR_MODEL`), `TRANSLATOR_TIMEOUT_MS` (default `30000`, must be > 0), `TRANSLATOR_MODEL_URL` (default `http://127.0.0.1:11434`), `TRANSLATOR_MODEL`; invalid values abort startup with exit 1.

## Conventions

- Commits use conventional prefixes: `feat:`, `docs:` (see `git log`).
- `docs/PROJECT_STATUS.md` and `docs/DEVELOPMENT_LOG.md` are kept as living sprint logs; update them when sprint scope/progress changes, and fix stale statements when touching related code.
- `docs/ARCHITECTURE.md` is the design intent (modularity, model independence, local-first); keep the core crate model-agnostic and platform-independent.
