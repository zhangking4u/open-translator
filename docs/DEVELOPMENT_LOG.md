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


## Milestone: Desktop 1b — GTK Popup


Completed:

- Replaced the zenity dialog with a GTK4 window (`gtk4` crate 0.11): source text, status line, selectable translation, 复制/关闭 buttons, Esc closes; errors shown in-window
- Popup mode shows the window immediately and runs service self-start + translation on a worker thread, updating the UI through a channel ("正在准备翻译服务…" → "翻译中…" → result)
- Repeated hotkey presses: GtkApplication's single-instance behavior forwards new launches to the running instance; the same window is reused — `activate` re-reads the selection and updates the content in place (mainstream single-window interaction). An earlier close-and-reopen revision felt like "a new window opens", especially after changing the target language; the in-place update also removes the application hold-guard workaround
- `--print` path unchanged; HTTP client and translation moved to `src/translate.rs`, UI in `src/ui.rs`
- Build requirement added: `libgtk-4-dev` + `pkg-config`


Gotcha:

- GTK's `Application::run()` feeds process arguments to GApplication, which rejects custom flags; use `run_with_args(&[])`


Verification:

- cargo test (9 tests)
- Popup smoke tests via `--stdin`: success and error windows both open and stay until closed; `--print` regression passes
- Refresh check: the same instance survived two quick activations and translated a 3-char, then 10-char, then 16-char selection after each launch (0.03s forwarding)
- Interaction review (2026-09-30): close-and-reopen revised to in-place updates — one window for all triggers, language changes included


Note:

- GNOME/Wayland does not let applications position windows, so the popup is compositor-placed; cursor-following would require a GNOME Shell extension


---


## Milestone: Desktop Install Script


Completed:

- `desktop/install.sh`: builds both release binaries and registers/updates the GNOME custom shortcut (default `Ctrl+Alt+T`), with `--binding`, `--name`, `--uninstall` options
- Idempotent: finds an existing shortcut with the same name and updates it in place, otherwise picks the first free `customN` slot
- Warns about duplicate bindings and a missing `wl-paste`


Verification:

- Install updated the existing shortcut; `--uninstall` emptied the keybinding array and reset the schema; reinstall restored it
- `bash -n` syntax check and `--help` output


---


## Milestone: Browser Extension MVP (Firefox)


Completed:

- `browser/extension/` (plain JS, no build step): context menu "翻译选中文本（OpenTranslator）" and `Alt+Shift+T` command → content-script bubble with loading/error/copy states and Esc close
- Background page posts to `{serviceUrl}/translate` using the `http://127.0.0.1:17890/*` host permission (no service or CORS changes)
- Options page: service URL and language pair stored in `storage.local`, plus a `/health` connection test
- Firefox MV2 chosen (host permission granted at install); Chrome MV3 variant deferred


Verification:

- `python3 -m json.tool` manifest check and `node --check` for all JS files
- Manual test in Firefox passed: context menu and `Alt+Shift+T` translate the selection in-page


Gotcha:

- WebExtension match patterns do not allow ports; `http://127.0.0.1:17890/*` is invalid and grants nothing. Use `http://127.0.0.1/*` (all ports on that host)


Note:

- The extension cannot start local services; the core service must be running (one desktop hotkey press auto-starts it)
- Temporary add-ons are removed when Firefox restarts; permanent installation needs an AMO-signed package or a Firefox edition that allows unsigned extensions


---


## Milestone: Consolidation — README, Keep-Alive, Desktop Config


Completed:

- Root `README.md`: quick start (Ollama, core service, desktop, Firefox extension), environment/flag reference tables, FAQ (network, proxy, Wayland limits, logs)
- Core: `TRANSLATOR_KEEP_ALIVE` (default `30m`) is sent with every Ollama generate request, so the model stays loaded between uses instead of unloading after 5 minutes
- Desktop: `~/.config/open-translator/config` (`service_url`, `source`, `target`) with CLI > config file > environment > defaults precedence; a commented default config was created on the dev machine
- `DEVELOPMENT_LOG` "Current Sprint" refreshed (was stale at Sprint 3 preparation)


Verification:

- cargo test: core 33 tests, desktop 13 tests
- `ollama ps` after a translation shows `UNTIL 29 minutes from now` (default was 5 minutes)
- Config file check: `target = ja` translated "hello" → こんにちは; `--target zh` overrode it; restoring `target = zh` gave 内核崩溃


---


## Milestone: Chrome MV3 Variant


Completed:

- `browser/extension/manifest.chrome.json`: MV3 manifest (service-worker background, `host_permissions`, no Firefox-only keys)
- Shared `background.js`/`content.js`/`options.*` unchanged: the `globalThis.browser ?? globalThis.chrome` shim, promise-based calls and `return true` message handling work in both browsers
- `browser/build.sh` builds `browser/dist/firefox` (MV2) and `browser/dist/chrome` (MV3) from the shared sources; `browser/dist/` is gitignored


Verification:

- Both manifests pass `python3 -m json.tool`; all JS passes `node --check`; `bash -n` on the build script
- `./browser/build.sh` produces both dist directories with the expected files (chrome manifest_version 3, firefox 2)
- Real-browser test on Chrome 154 and Edge 154 (headless, CDP): extension loaded, service worker `translate("kernel panic")` → `{"ok":true,"translation":"内核崩溃"}`, and the content script showed the bubble "快速的棕色狐狸跳过了那只懒惰的狗。" for a selected page sentence
- Note: Chrome 137+ ignores `--load-extension`; loading via CDP `Extensions.loadUnpacked` works (a local page server is needed for the content-script leg)


---


## Milestone: Install Script Fix — Empty Keybinding Array


Problem:

- After `--uninstall`, `gsettings get ... custom-keybindings` returns `@as []`; the script's parser turned the type tag `@as` into a bogus path on reinstall, producing `['@as', '/org/.../custom0/']`
- `gsd-media-keys` crashed on the invalid path (SEGV loop until systemd's "Start request repeated too quickly"), which silently disabled all GNOME custom shortcuts


Fix:

- `list_paths()` filters tokens starting with `@`; install normalizes the array when updating an existing shortcut
- Verified: uninstall → `@as []`; reinstall → clean `['/org/.../custom0/']`; `gsd-media-keys` stays alive
- Recovery without logout: `systemctl --user reset-failed org.gnome.SettingsDaemon.MediaKeys.service`, then pull the unit in through a transient dependency (`RefuseManualStart` blocks `systemctl --user start`)


---


## Milestone: Extension Packaging and Signing


Completed:

- `browser/build.sh` gains `--zip`: packages `dist/open-translator-{firefox,chrome}-<version>.zip` (uses `zip`, falls back to `python3 -m zipfile`)
- `browser/sign.sh`: signs the Firefox build through `npx web-ext sign --channel unlisted` into `dist/signed/`; requires `WEB_EXT_API_KEY`/`WEB_EXT_API_SECRET` and exits with setup instructions otherwise
- Firefox manifest: added `browser_specific_settings.gecko.data_collection_permissions.required = ["none"]` and bumped `strict_min_version` to 142.0 (minimum for that key)


Verification:

- `./browser/build.sh all --zip` produced both zips; the Firefox zip contains the five expected files at the archive root
- `npx web-ext lint --source-dir browser/dist/firefox` → 0 errors / 0 notices / 0 warnings
- `./browser/sign.sh` without credentials exits 1 with instructions (actual signing needs the user's AMO API key)


Signing result (2026-09-30):

- AMO unlisted signing auto-approved version 0.1.0 (add-on 3082435); the .xpi was installed permanently in Firefox
- Gotchas: the `web-ext sign` artifact download is the last step (do not interrupt the terminal), and a version number cannot be reused — bump the manifest version before re-signing; alternatively download the signed file from the AMO developer hub


---


## Milestone: Release Readiness — License, CI, Browser E2E


Completed:

- `LICENSE`: MIT
- `.github/workflows/ci.yml`: core and desktop `cargo test` (desktop installs `libgtk-4-dev`), browser static checks + `web-ext lint`, and a Chrome end-to-end job
- `browser/test.sh` + `browser/test-chrome.mjs` + `browser/test-page.html`: repeatable Chrome/Edge CDP smoke test (loads the MV3 build, exercises the service worker and the content-script bubble; starts a mock service when none is running)
- Fixed `browser/build.sh` exiting 1 when `--zip` was not passed (a `[ ... ] && ...` as the final statement); the new harness surfaced it


Verification:

- `./browser/test.sh` → 7/7 PASS locally against the real model: worker `translate("kernel panic")` → 内核崩溃; bubble → "快速的棕色狐狸跳过了那只懒惰的狗。"
- `browser/build.sh chrome` → exit 0
- First CI run: core/desktop/browser jobs green; the e2e job passed all 7 checks but failed on cleanup (`rm -rf` racing the browser shutdown, overriding the exit status) — fixed by waiting for the browser process and preserving the exit code
- Second CI run green: all four jobs pass (core 14s, browser 21s, Chrome e2e 56s, desktop 34s); `actions/checkout` and `actions/setup-node` bumped to v5 to clear the Node 20 deprecation annotation


---


## Milestone: Long-Text Guard, Auto Source, Language Switcher


Completed:

- Core: `TRANSLATOR_MAX_CHARS` (default 1500) — over-limit requests return 400 `invalid_request` immediately instead of waiting for the 30s timeout (measured: 1799 chars → 400 in 6ms; previously 2249 chars → 504 after 30s)
- Core: `source=auto` — generic and TranslateGemma prompts get source-less templates; HY-MT's template was already source-agnostic
- Desktop popup: target-language dropdown (zh/en/ja/ko/fr/de/es/ru, unknown values appended dynamically); changing it re-translates the **source text already shown in the window** (not the current selection), updates the window title and writes `target` back to `~/.config/open-translator/config`
- Desktop popup: single-window interaction — activation reuses the window and updates in place; language changes and repeated hotkeys never open a second window


Verification:

- cargo test: core 36 tests, desktop 15 tests
- Live: 899 chars translated (200); 1799 chars rejected in 6ms; `source=auto` translated Spanish and French input into Chinese
- Popup smoke-tested with the new dropdown row (window opens; selection changed by user for the real check)


Gotcha:

- `GtkComboBoxText` changes its selection on mouse-wheel events; the popup attaches an `EventControllerScroll` returning `Propagation::Stop` so scrolling over the window never switches the target language


---


## Milestone: Browser Bubble Enhancements — Language Switch and Auto-Translate


Completed:

- Bubble footer gains a target-language selector (zh/en/ja/ko/fr/de/es/ru plus dynamic values); switching it persists `target` to `storage.local` and re-translates the current text in place
- Optional auto-translate (`autoTranslate`, default off): a 400ms-debounced `mouseup` after a selection triggers translation; input/textarea/contenteditable and bubble interactions are ignored; configurable in the options page
- Extension version bumped to 0.1.1 (0.1.0 is signed on AMO and version numbers cannot be reused)


Verification:

- `browser/test.sh` 10/10 PASS locally against the real model: after switching the bubble to Japanese the sentence re-translated in place, the target persisted, and a simulated `mouseup` auto-translated the selection
- Static checks (`node --check`, manifest JSON) pass


---


## Milestone: Windows Support Verification


Completed:

- CI: new `core-windows` job (`windows-latest`) runs the core service's `cargo test`, keeping the service portable
- README: platform support table (core service Linux/Windows, extension all platforms, desktop Linux/GNOME only with the adaptation requirements documented)


Verification:

- Code review: `core/translator-service` has no Unix-specific code or path assumptions; dependencies (axum/tokio/reqwest/tracing) are cross-platform
- CI run 36716297123: all five jobs green, Windows core tests pass in 1m57s


---


## Milestone: Desktop Core Extraction (Phase 1 for Windows/macOS)


Completed:

- New crate `desktop/translator-core` (platform-agnostic, no GTK): CLI args + config-file precedence, settings persistence, HTTP translate client, service health checks and auto-start
- Per-OS paths in `paths.rs`: Linux XDG, Windows `%APPDATA%`/`%LOCALAPPDATA%` (plus `translator-service.exe`/`ollama.exe` discovery), macOS `~/Library`
- `desktop/translator-popup` now depends on `translator-core` by path; the `wl-paste` selection reader and GTK UI stay Linux-specific in the popup
- CI: `desktop-core` matrix job runs the lib's tests on ubuntu, windows and macos


Verification:

- `cargo test` in `translator-core` → 15 tests; popup builds and its `--print` path still works ("hello" → 嗨)
- `browser/test.sh` regression → 10/10 PASS


---


## Milestone: Windows Desktop Client (Phase 2 MVP)


Completed:

- New crate `desktop/translator-popup-windows` (eframe/egui): resident app with a hidden window, `Ctrl+Alt+T` global hotkey (`global-hotkey`), selection capture via simulated Ctrl+C (`enigo`) + clipboard (`arboard`), in-place translation updates, target-language selector, 复制/隐藏/退出, Esc hides
- Windows-only dependencies gated under `[target.'cfg(windows)'.dependencies]`; on Linux the crate still builds (capture falls back to `wl-paste`, hotkey is a no-op), enabling local checks and smoke tests
- `desktop/install-windows.ps1`: builds the core service + popup in release and adds a Startup shortcut (`-Uninstall` removes it)
- CI: `desktop-popup-windows` job on windows-latest (`cargo test` + `cargo build --release`)


Verification:

- `cargo check` (Linux) and `cargo check --target x86_64-pc-windows-msvc` both clean; unit test passes
- Local render smoke on Linux (`--stdin`): the eframe window starts and the service log shows the translation request (`text_chars=12`)
- Windows runtime behaviour (hotkey registration, Ctrl+C capture) still needs testing on a real Windows machine


---


## Milestone: macOS Desktop Client (Phase 3)


Completed:

- The eframe client is now cross-platform: crate renamed `desktop/translator-popup-desktop`; macOS capture uses Cmd+C (`enigo`) + clipboard (`arboard`) with a clipboard-unchanged heuristic that returns an Accessibility-permission hint; `global-hotkey` covers macOS too
- `desktop/install-macos.sh`: builds both release binaries, creates `~/Applications/OpenTranslator.app` (LSUIElement, core service bundled next to the popup) and installs a LaunchAgent (`--uninstall` removes both)
- `translator-core::paths::default_core_bin` now prefers a `translator-service` binary next to the popup, falling back to the repo layout
- CI: the desktop client job is a matrix over windows-latest and macos-latest


Verification:

- `cargo check` clean for Linux, `x86_64-pc-windows-msvc` and `aarch64-apple-darwin` targets (enigo/arboard/global-hotkey compile on both desktop platforms)
- Unit tests pass (client 1, core lib 15); macOS runtime behaviour (Accessibility prompt, hotkey) still needs a real Mac


---


## Milestone: Desktop Tray Icon and Configurable Hotkey


Completed:

- Tray/menu-bar icon on Windows and macOS (`tray-icon`): menu 显示窗口 / 立即翻译选中文本 / 退出, so the resident app is visible and quit-able (macOS keeps `LSUIElement`); a small generated icon (blue circle with two white bars) avoids an image dependency
- Configurable hotkey: `hotkey` key in the config file or `TRANSLATOR_HOTKEY`; parser (`src/hotkey.rs`) accepts Ctrl/Alt/Shift/Meta + letters/digits/space/enter/tab/F1–F12 and is unit-tested on every platform


Verification:

- `cargo test` on Linux (3 tests, including the hotkey parser), `cargo check` clean for `x86_64-pc-windows-msvc` and `aarch64-apple-darwin`, 0 warnings on all targets
- Runtime tray/hotkey behaviour still needs a real Windows/macOS machine


---


## Milestone: In-Process llama.cpp Spike (Phase B groundwork)


Context:

- Direction decided: target ordinary users with an in-process runtime — one application, no Ollama, no separate service process (the desktop app can expose the HTTP endpoint for the browser extension itself)


Spike setup:

- `llama-cpp-2` 0.1.157 (llama.cpp bindings; needs cmake + clang/libclang to build), loading `HY-MT1.5-1.8B-Q4_K_M.gguf` with the official chat template (`<｜hy_begin▁of▁sentence｜><｜hy_User｜>…<｜hy_Assistant｜>`) and sampling temperature 0.7 / top_k 20 / top_p 0.6 / repeat 1.05
- Dev-machine build deps: cmake at `~/.local/opt/cmake` (user-space), `clang` + `libclang-dev` from apt; build with `PATH="$HOME/.local/opt/cmake/bin:$PATH" LIBCLANG_PATH=/usr/lib/llvm-21/lib`


Results (release, CPU i5-14400):

- "kernel panic" → 内核崩溃 (0.07s); graceful-shutdown sentence → correct Chinese (0.48s); ~29 tok/s; model ~1.8 GB resident (1069 MB mapped + 711 MB repack)
- llama.cpp logs go through tracing and are noisy by default; production code should configure a subscriber/filter


Next:

- Phase B: extract this into a `core/inference` crate (`InferenceEngine`: load/generate/stop tokens/sampling/errors, no HTTP, no platform deps), then wire it as `EngineKind::LlamaCpp` for the service and embed it in the desktop client; first-run model download and installers follow


---


## Milestone: In-Process Inference Crate (Phase B1)


Completed:

- New crate `core/inference` (`translator-inference`): `InferenceEngine::load` + `generate(prompt, stop_strings, GenerateOptions)` around `llama-cpp-2`
- Actor design: one worker thread owns the backend/model/context; requests are serialized over a channel and the per-request KV cache is cleared between calls (safe Rust, no unsafe impls, no thread-affine misuse)
- Robustness: prompt/context fit check, greedy sampling when `temperature <= 0`, penalties/top_k/top_p/temp/dist otherwise, stop-string truncation, llama.cpp stderr logs voided by default (`void_llama_logs`)
- Testing: unit tests run everywhere (missing model, defaults); the real translation test is gated by `TRANSLATOR_TEST_MODEL`
- CI: new `inference` job on ubuntu-latest


Verification:

- `cargo test` → 3 passed (no model required)
- `TRANSLATOR_TEST_MODEL=/tmp/kilo/hymt/HY-MT1.5-1.8B-Q4_K_M.gguf cargo test translates_with_env_model` → passed in 1.05s


Next:

- B2: `EngineKind::LlamaCpp` in the service (`TRANSLATOR_MODEL_PATH`), `spawn_blocking` wrapper; B3: embed in the desktop client


---


## Milestone: LlamaCpp Engine in the Service (Phase B2)


Completed:

- `EngineKind::LlamaCpp` (`TRANSLATOR_ENGINE=llama-cpp`, `TRANSLATOR_MODEL_PATH`, `TRANSLATOR_N_CTX` default 4096): the service translates fully in-process through `core/inference`, no Ollama
- `LlamaCppEngine` builds prompts with the existing `PromptStyle` and runs generation in `spawn_blocking`; `PromptStyle` now owns `sampling()` / `stop_strings()` (shared by the Ollama and llama.cpp engines) and `raw_prompt()` adds the HY-MT chat-template tokens for runtimes that tokenize raw text (llama.cpp) — without this the model hallucinated badly
- `translator-inference`: the llama backend is now process-wide (`OnceLock`; llama.cpp allows init once), the sender is behind a `Mutex` so the engine is `Sync`, and missing model files return a clean error instead of a debug assertion
- `engine::build` returns `Result` (model loading can fail); `/health` reports `llama-cpp` and the model file name
- Tests: `tests/llama_cpp.rs` (missing model always; real translation gated by `TRANSLATOR_TEST_MODEL`)


Verification:

- Full suite: service 43 tests, inference 3 tests, 0 warnings
- Live service with the llama-cpp engine: `/health` → `engine: llama-cpp`; "kernel panic" → 内核崩溃 (0.26s), long sentence → correct Chinese (0.73s), "Break a leg!" → 祝你好运！ (0.35s), no Ollama involved


Next:

- B3: embed the engine in the desktop client (drop the external service for end users) and expose the local HTTP endpoint from the app for the browser extension


---


## Milestone: Embedded Desktop Client (Phase B3)


Completed:

- `desktop/translator-popup-desktop` now embeds `translator-service::engine::llama_cpp`: no Ollama and no external service process; the app loads the GGUF at startup (`TRANSLATOR_MODEL_PATH` → config `model_path` → per-user default models dir from `translator-core::paths::default_model_path`)
- While running, the app serves the core HTTP API on `service_url` for the browser extension (`src/server.rs`: axum on a background thread with the shared timeout wrapper; `serve_extension = false` disables); port conflicts are non-fatal
- `LlamaCppEngine` gained `translate_blocking` (the async trait impl now wraps it in `spawn_blocking`), which the popup worker calls directly — no tokio runtime needed for translations
- New config keys in the shared settings file: `model_path`, `prompt_style` (default `hymt`), `serve_extension` (default `true`); install scripts warn when the model file is missing
- The GTK Linux popup (`desktop/translator-popup`) keeps the external-service path (Ollama/llama-cpp service or HTTP)

Verification:

- With the service and Ollama stopped: `--print --stdin` translates in-process (`kernel panic` → 内核崩溃 in 1.1s including model load; `Break a leg!` → 祝你好运！)
- Embedded HTTP: `/health` → `engine: llama-cpp, model: HY-MT1.5-1.8B-Q4_K_M.gguf`; `/translate` → 分段错误 in 0.30s
- `cargo build --all-targets` and tests (3) clean, 0 warnings. Local cross-target checks of this crate are no longer possible (it compiles llama.cpp); CI covers windows/macos


Next:

- Phase C: first-run model download (ModelScope + progress/checksum), installers (NSIS/dmg/deb), code signing


---


## Milestone: First-Run Model Download (Phase C1)


Completed:

- `translator-core::models::download`: ModelScope HTTP download with resume (`Range`; ModelScope returns 206/`accept-ranges: bytes`), streamed SHA-256 verification, progress callback, `.part` staging renamed on success, stale-partial cleanup on checksum mismatch, restart when the server ignores the range request
- `download_client()` sets a `User-Agent` — ModelScope's CDN answers 403 without one (reqwest sends none by default); `translator-core`'s reqwest gained the `rustls-tls` feature for the HTTPS source
- Desktop client first run: when the model file is missing and `auto_download = true` (default), the app downloads with a live progress line (percentage/MB) in the window, verifies the checksum, loads the engine and then starts the extension HTTP endpoint; `TRANSLATOR_MODEL_PATH` and config `model_path` still take precedence
- Window behaviour fix (found while testing): normal startup stays hidden (only first-run download or errors surface the window; startup no longer grabs the selection), and Esc/X quit the app when no tray/hotkey can restore it (Linux dev builds) instead of hiding it forever


Verification:

- Unit tests (translator-core 20): full download + progress + checksum, checksum mismatch removes the partial, resume from a half-written `.part`, restart when the server ignores `Range`
- Live first-run: the desktop client on an empty models dir downloaded 1.13 GB, verified the SHA-256 (`4383ac0c…`), loaded the engine and served `/health` + `/translate` (kernel panic → 内核崩溃 in 0.28s); the model now lives at the default path for future runs


Next:

- Phase C2: installers (NSIS/dmg/deb), code signing decision; real-machine verification of the Windows/macOS clients


---


## Milestone: Release Packaging (Phase C2)


Completed:

- `.github/workflows/release.yml`: triggered by tags (`v*`) or manual `workflow_dispatch`; builds the desktop client on Windows and macOS and produces:
  - `OpenTranslator-windows-x64.zip` (exe + `packaging/windows/install.ps1` per-user installer + README)
  - `OpenTranslator-macos-<arch>.dmg` (`packaging/macos/make-dmg.sh` builds the `.app` bundle from `packaging/macos/Info.plist` and calls `hdiutil`)
- Tag runs also create/update the GitHub release and upload the artifacts; dispatch runs only upload workflow artifacts (no tag needed to test)
- `packaging/windows/install.ps1`: copies the exe into `%LOCALAPPDATA%\Programs\OpenTranslator`, adds a Startup shortcut, `-Uninstall` removes both (no admin required)
- `desktop/macos/Info.plist` moved to `packaging/macos/Info.plist` (single source, used by `install-macos.sh` and the dmg script)


Verification:

- YAML and shell syntax checks locally; packaging logic verified by running the workflow via `workflow_dispatch` and inspecting the downloaded artifacts (see CI logs)
- Signing/notarization still pending (budget decision); macOS Gatekeeper and Windows SmartScreen warnings are documented in the README


---


# 2026-10-01


## Milestone: Windows Real-Machine Verification (Intel Vulkan Crash)


Completed:

- Installed the `OpenTranslator-windows-x64` release zip on a real Windows machine: `install.ps1` copied the exe to `%LOCALAPPDATA%\Programs\OpenTranslator` and created the Startup shortcut correctly, but the app crashed immediately on every launch
- Event log diagnosis: `Application Error` 1000, faulting module `igvk64.dll` (Intel Vulkan driver, 30.0.101.1692), exception `0xc0000005`; eframe 0.36 defaults to the wgpu renderer, which enumerates the Vulkan backend first and the driver bug kills the process before a window appears (silent because release builds have no console)
- Fix: `desktop/translator-popup-desktop/src/main.rs` defaults `WGPU_BACKEND=dx12` on Windows when the variable is not already set; a user-set value still wins
- Workaround for the existing installed binary: user-level `WGPU_BACKEND=dx12` (`setx`), verified against the installed exe
- Second real-machine issue: eframe's built-in fonts have no CJK glyphs, so every Chinese label (download status, buttons, language names, translation output) rendered as tofu boxes; `desktop/translator-popup-desktop/src/app.rs` now installs a system CJK font as a fallback (Windows Microsoft YaHei/SimHei/SimSun, macOS PingFang/STHeiti, Linux Noto CJK/WQY) for both the proportional and monospace families
- Both fixes need a rebuilt Windows artifact to reach users; the downloaded release zip predates them


Verification:

- Default launch: crash event 1000 (`igvk64.dll`, `0xc0000005`); launch with `WGPU_BACKEND=dx12`: window opens and the process stays resident
- Follow-up: a rebuilt Windows artifact is required to ship the in-code fix; the current release zip still needs the environment variable


---


## Milestone: Windows Selection Capture Fix (Stale Clipboard)


Completed:

- Real-machine report: selecting Chinese text and pressing the default hotkey translated the previously copied text; pressing `Ctrl+C` manually before the hotkey was the only workaround
- Root cause: the global hotkey fires on key-down and `app.rs` triggers the capture immediately, so the injected copy usually ran while the user was still holding Ctrl/Alt — the focused app received `Ctrl+Alt+C` and did not copy; `capture.rs` then read the clipboard after a fixed 150 ms without checking that it had changed, returning the previous content
- `desktop/translator-popup-desktop/src/capture.rs` (Windows): wait for all left/right Ctrl/Alt/Shift/Win keys to be released (`GetAsyncKeyState`, 750 ms cap) before injecting, add a 15 ms gap between the modifier and `C`, and poll `GetClipboardSequenceNumber` for up to 1.5 s — translation starts only when the clipboard actually changed, otherwise the app reports "复制未生效" instead of translating stale text
- Windows-only dependency `windows-sys 0.59` (`Win32_System_DataExchange`, `Win32_UI_Input_KeyboardAndMouse`); the macOS path is unchanged and still has the same race (it errors out instead, thanks to the pre-copy comparison)
- Left as follow-ups: macOS modifier wait, retry if the clipboard is still busy when the sequence changes, moving capture off the UI thread


Verification:

- `capture.rs` compile-checked in a standalone crate against arboard 3.6.1 / enigo 0.6.1 / windows-sys 0.59; CMake 4.4 + LLVM 23 + VS 2022 Build Tools are installed on the verification laptop for future full builds
- Runtime verification pending: a rebuilt Windows artifact is required (select text, press the hotkey without manual copy, repeat with different selections, check an elevated window)


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


Consumer edition (ordinary users); Sprint 5 meeting translation parked.


Planned:

1. Tag `v0.1.0` and publish the GitHub release

2. Real-machine verification on Windows/macOS

3. Code signing / notarization (budget decision)


Completed sprints:

1. Sprint 1.2: core refactor, engine trait, API layer, tests

2. Sprint 2: Ollama runtime, model evaluation (HY-MT default), prompt styles, service hardening

3. Sprint 3: desktop selection popup (Phase 0/1), service self-start, GTK UI, install script

4. Sprint 4: Firefox browser extension MVP

5. Consumer edition: desktop core extraction, Windows/macOS clients, in-process llama.cpp engine, first-run model download, release packaging


---

# Future Milestones


## Distribution

Tag `v0.1.0`, real-machine verification on Windows/macOS, code signing/notarization.


## Sprint 5

Meeting translation (parked).


## Sprint 6

Mobile client.
