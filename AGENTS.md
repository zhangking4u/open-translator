# AGENTS.md

OpenTranslator: local-first AI translation platform. Only the Rust core service exists so far; `desktop/`, `models/`, and `tests/` are empty placeholders. `README.md` is empty — `docs/` is the project context.

## Repo layout / build

- The only crate is `core/translator-service`. There is no root Cargo.toml or workspace: run all cargo commands from `core/translator-service/`:
  - `cargo run` — starts Axum server at `http://127.0.0.1:17890` (hardcoded)
  - `cargo test` — runs the `MockEngine` unit test plus `tests/api.rs` integration tests via tower `oneshot` (no port or network needed)
  - `cargo check`
- `Cargo.lock` is committed; keep it.
- A root `tests/` dir exists but is not a Cargo test dir — Rust tests belong under `core/translator-service/tests/` or as `#[cfg(test)]` modules.
- Root `.gitignore` ignores `package.json`, lockfiles, and `node_modules` (legacy template leftovers), so JS tooling files would be silently ignored.
- Active branch is `main`.
- No CI workflows and no rustfmt/clippy config; verify locally with `cargo test`. Existing sources are not rustfmt-formatted, so don't run repo-wide `cargo fmt` unprompted.

## Architecture (current state)

`src/lib.rs` exposes `api`, `domain`, `engine`; `src/main.rs` is bootstrap only (builds `api::router(...)` and binds the port).

Flow: `src/api/mod.rs` (router + handlers + DTOs) → `src/domain/translation.rs` (`TranslationRequest`/`TranslationResult`) → `src/engine/mod.rs` (`TranslationEngine` trait) → `src/engine/mock.rs` (`MockEngine`).

- `TranslationEngine::translate` is async and dyn-compatible: it returns `TranslationFuture` (`Pin<Box<dyn Future<Output = TranslationResult> + Send + '_>>`), and the trait has `Send + Sync` supertraits so API state is `Arc<dyn TranslationEngine>`. New engines must use this signature — plain `async fn` in the trait would break `dyn` support.
- API: `GET /health` → `{"status":"ok","service":"translator-core"}`; `POST /translate` body `{"text","source","target"}` → `{"translation":"[Mock Translation] <text>"}`.
- `source`/`target` are accepted but unused by `MockEngine`.

## Conventions

- Commits use conventional prefixes: `feat:`, `docs:` (see `git log`).
- `docs/PROJECT_STATUS.md` and `docs/DEVELOPMENT_LOG.md` are kept as living sprint logs; update them when sprint scope/progress changes, and fix stale statements when touching related code.
- `docs/ARCHITECTURE.md` is the design intent (modularity, model independence, local-first); keep the core crate model-agnostic and platform-independent.
