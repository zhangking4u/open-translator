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


## Milestone: Selection Capture Fix (Stale Clipboard)


Completed:

- Real-machine report: selecting Chinese text and pressing the default hotkey translated the previously copied text; pressing `Ctrl+C` manually before the hotkey was the only workaround
- Root cause: the global hotkey fires on key-down and `app.rs` triggers the capture immediately, so the injected copy usually ran while the user was still holding Ctrl/Alt — the focused app received `Ctrl+Alt+C` and did not copy; `capture.rs` then read the clipboard after a fixed 150 ms without checking that it had changed, returning the previous content
- `desktop/translator-popup-desktop/src/capture.rs` (Windows): wait for all left/right Ctrl/Alt/Shift/Win keys to be released (`GetAsyncKeyState`, 750 ms cap) before injecting, add a 15 ms gap between the modifier and `C`, and poll `GetClipboardSequenceNumber` for up to 1.5 s — translation starts only when the clipboard actually changed, otherwise the app reports "复制未生效" instead of translating stale text
- macOS: the same wait before injecting `Cmd+C`, reading physical key state via `CGEventSourceKeyState` (left/right Cmd/Shift/Option/Ctrl, 750 ms cap); the pre-copy text comparison still reports the Accessibility-permission hint when the copy does not happen
- Windows-only dependency `windows-sys 0.59` (`Win32_System_DataExchange`, `Win32_UI_Input_KeyboardAndMouse`)
- Left as follow-ups: retry if the clipboard is still busy when the sequence changes, moving capture off the UI thread


Verification:

- `capture.rs` compile-checked in a standalone crate against arboard 3.6.1 / enigo 0.6.1 / windows-sys 0.59, including `cargo check --target aarch64-apple-darwin` for the macOS path (raw `CGEventSourceKeyState` FFI); CMake 4.4 + LLVM 23 + VS 2022 Build Tools are installed on the verification laptop for future full builds
- Runtime verification pending: a rebuilt Windows artifact is required (select text, press the hotkey without manual copy, repeat with different selections, check an elevated window); macOS needs the same check on a real machine


---


## Milestone: Extension Server Bind Error and Single Instance


Completed:

- Real-machine report: "扩展服务启动失败：cannot bind 127.0.0.1:17890 ... (os error 10048)" kept appearing even with a single running instance; the earlier "duplicate instance" explanation was wrong
- Root cause: `main.rs` started the extension HTTP server when the model was already loaded, and `PopupApp::new` started it a second time for the same `Startup::Loaded(Ok)`; the second bind always failed with `AddrInUse` and that error was shown in the window, while the first server kept serving (`/health` answered normally)
- Fix: removed the duplicate start in `main.rs`; the server now starts only from the app (`maybe_start_server`, which also covers the post-download `Ready` event)
- `server.rs` returns `ServerError::{AddrInUse, Other}`; `AddrInUse` shows an informational status ("扩展服务未启动：… 已被其他服务占用") instead of a red error — the expected case when a headless `translator-service` already owns the port
- New `single_instance` module (`File::try_lock` on a per-user lock file in the temp dir, no new crates): a second desktop launch exits early, showing a Windows message box ("OpenTranslator 已在运行…"); macOS exits silently (LaunchServices normally prevents duplicates there anyway)


Verification:

- `single_instance.rs` and `capture.rs` compile-checked in a standalone crate for the Windows target and `--target aarch64-apple-darwin`
- Runtime check pending: start the app and confirm the window shows "等待划词…" instead of the bind error (with `/health` working); double-click a second copy and confirm the "已在运行" box appears and only one process remains


---


## Milestone: Windows Real-Machine Re-Verification (v0.1.0)


Completed:

- Ran the released v0.1.0 Windows build on a real machine; all three target checks passed:
  - Selection capture without a manual Ctrl+C: the hotkey fires, the modifier wait and clipboard-change detection work, and the fresh selection is translated
  - Chinese UI/translation text renders correctly (system CJK font fallback, no tofu boxes)
  - Launching a second copy shows the "OpenTranslator 已在运行…" box and only one process remains
- The DX12 default needs no user setting on the Intel Vulkan-driver machine


Deferred:

- macOS real-machine verification: no Mac hardware available; the dmg is arm64-only and the Accessibility flow is untested on device


Next:

- Linux packaging: deb/AppImage for the GNOME client


---


## Milestone: v0.1.0 Release


Completed:

- Tag `v0.1.0` pushed at `a6f1321`; the release workflow built and published the GitHub release (https://github.com/zhangking4u/open-translator/releases/tag/v0.1.0) with `OpenTranslator-windows-x64.zip` and `OpenTranslator-macos-arm64.dmg`
- The tagged build includes the latest real-machine fixes: Windows DX12 backend default + system CJK font fallback, capture waiting for modifier release with clipboard-change detection, and the desktop single-instance lock / extension-server bind fix
- Builds remain unsigned: SmartScreen/Gatekeeper warnings and the macOS Accessibility requirement are documented in the release notes and README


Verification:

- `gh release view v0.1.0`: published 2026-10-01 02:40 UTC, both assets attached
- Still pending: real-machine re-verification with the rebuilt artifacts


---


## Milestone: Linux deb Package (GNOME Client)


Completed:

- `translator-core::services` resolves the auto-start engine: no `TRANSLATOR_ENGINE` (or `llama-cpp`) uses in-process llama.cpp with the per-user model path; `TRANSLATOR_ENGINE=ollama` keeps the legacy Ollama path. `core_env` passes `TRANSLATOR_MODEL_PATH` + `TRANSLATOR_PROMPT_STYLE`, and `ensure` skips the Ollama readiness gate for llama-cpp
- First-run model download for the GTK popup: `services::ensure_with_download` reuses `translator_core::models::download` (ModelScope, resume, SHA-256) when the default model is missing and `auto_download = true`; the window shows percentage/MB progress and `--print` reports it on stderr
- `packaging/linux/make-deb.sh`: hand-rolled `dpkg-deb` package with `translator-popup` + `translator-service` in `/usr/lib/open-translator` (sibling layout used by `default_core_bin`, `/usr/bin/translator-popup` symlink), `.desktop` entry, hicolor icon, `/usr/share/doc` README; `Depends: libgtk-4-1, wl-clipboard, libgomp1`
- `packaging/linux/open-translator-setup`: user-level GNOME shortcut registration (`--binding`, `--name`, `--bin`, `--uninstall`) so the deb needs no root-time gsettings; `postinst` points users to it
- `release.yml` gained a Linux job that builds both crates and attaches `OpenTranslator-linux-x64.deb` to the release; `.gitignore` ignores `packaging/linux/build/`
- Popup help/config docs updated for `model_path` / `prompt_style` / `auto_download` and the new default engine; README documents the deb install


Review fixes (same day):

- Process-wide download lock (`static DOWNLOAD_LOCK`) so concurrent workers can no longer corrupt the shared `.part`; `ensure_with_download` now checks an existing service, `--no-start` and remote URLs before downloading; the missing-model hint distinguishes explicit paths from the default path
- `format_download_status` moved to `translator_core::models` and shared by the GTK popup, its headless path and the Windows/macOS client; `desktop/install.sh` now delegates shortcut handling to `open-translator-setup` (one gsettings implementation), which gained `--if-missing`
- The popup silently auto-registers the shortcut on first launch (`--if-missing`), so the deb is usable out of the box and custom bindings are never overwritten
- Deb metadata declares `libc6 (>= 2.39)` and the docs state the Ubuntu 24.04+ / GTK4 4.10+ baseline (binary symbol versions: GLIBC_2.38/2.39); Release dispatch runs export a numeric `0.0.0+<sha>` version


Verification:

- `cargo test`: translator-core 27 tests, popup tests move with the shared formatter; `cargo test --locked` now passes in both after regenerating the stale `Cargo.lock` files (the popup lock predated translator-core's sha2 dependency; both were missing platform deps that current Cargo requires under `--locked`); `cargo check --locked` passes for the Windows/macOS client with the shared formatter
- Extracted the built deb to a temp dir and ran the packaged `translator-popup --stdin --print`: auto-started the sibling `translator-service` (engine `llama-cpp`, default model) → "kernel panic" → 内核崩溃; `/health` reported `engine: llama-cpp`; the service process was stopped afterwards
- Missing-model path fails fast: `TRANSLATOR_MODEL_PATH=/tmp/nonexistent.gguf` → "model file not found ... (set model_path or enable auto_download)" (explicit paths never trigger a download)
- `open-translator-setup --name ot-selftest --bin /bin/true` registered and uninstalled cleanly without touching the existing shortcut; shell syntax checks pass on all packaging scripts
- After the review fixes: the packaged popup still translates "kernel panic" → 内核崩溃 (llama-cpp); `--no-start` with a missing default model fails in ~26 ms without creating the models dir; `--if-missing` is a silent no-op when the shortcut exists and registers only when missing
- CI `workflow_dispatch` run 36811560500 (commit `985830c`): all three jobs succeeded; the downloaded `OpenTranslator-linux-x64.deb` (version `0.0.0+985830c`) was extracted and run with an isolated HOME: the first run downloaded 1.13 GB in 47 s with progress, the SHA-256 matched (`4383ac0c…`), and "kernel panic" → 内核崩溃; the warm second run took 0.36 s
- Pending: an actual `apt install` on a user account (needs admin rights) and shipping the deb with the next release


Next:

- Ship the deb with the next release (release job and packaged first-run download verified); AppImage deferred


---


## Milestone: Startup Update Check


Completed:

- `translator-core::update`: GitHub `releases/latest` check returning `ReleaseInfo { version, url }`, numeric version comparison (`is_newer`; tolerates a `v` prefix and `-`/`+` suffixes), 5 s timeout, silent on any failure; `current_version()` embeds the release tag via `OPEN_TRANSLATOR_VERSION` (set by the release workflow) and falls back to the crate version
- Both clients check once at startup (`check_updates`, default true; `TRANSLATOR_CHECK_UPDATES=false` disables): the GTK popup appends a "有新版本 vX.Y.Z，点击查看" link to its window; the Windows/macOS client shows a banner with a download button and enables a tray/menu-bar item
- Release workflow build steps inject `OPEN_TRANSLATOR_VERSION` from the tag (dispatch runs pass an empty value, falling back to the crate version)
- README, the GTK/desktop help texts and the deb README document the new key and env vars


Verification:

- `cargo test`: translator-core 31 tests (version comparison, newer/current release, HTTP and parse errors against an axum stub); GTK popup and Windows/macOS client compile; `gh api releases/latest` confirmed reachable from the dev machine
- Note: the check ships with the next release; existing v0.1.0 installs cannot be notified until they update once manually


Next:

- Tag the next release so the update hint reaches users; the tray item needs a real Windows/macOS check


---


## Milestone: v0.1.0 Re-Release (Linux deb + Update Check)


Completed:

- Deleted the original v0.1.0 GitHub release and both tags, re-tagged `v0.1.0` at `fd18fc2` (main with the Linux deb and update-check work) and re-ran the Release workflow
- Release run 36813116437: all three package jobs succeeded; the release now carries `OpenTranslator-windows-x64.zip`, `OpenTranslator-macos-arm64.dmg` and `OpenTranslator-linux-x64.deb`
- Release notes rewritten: Linux `apt install`, first-launch shortcut registration, glibc/GTK baseline, startup update check, and the note that existing v0.1.0 installs need one manual update before hints can reach them


Verification:

- `gh release view v0.1.0`: three assets attached; `gh run view 36813116437`: Windows/macOS/Linux jobs all success


---


## Milestone: Desktop UI Polish (State Machine + Modern Visuals)


Completed:

- `translator-core::languages` holds the shared language list + `label()` lookup; both clients dropped their duplicated tables
- Windows/macOS client (`translator-popup-desktop`): `ModelState` (Downloading/Ready/Failed) and `TranslationState` (Idle/Empty/Waiting/Running/Done/Failed) replace the `status: String` + `error: bool` pair
- Selections captured before the model is ready are queued (`Waiting`) and translated automatically on `StartupEvent::Ready` instead of being dropped
- Frameless transparent window (520×420, not resizable) with a rounded card, drop shadow, custom draggable header and a light/dark theme following the system; no OS title bar
- Body split into a dimmed source card and a prominent translation card; model download shows a spinner + progress bar; failures render an error card with a 重试 button; copy button shows 已复制 for 1.5 s
- Update and notice banners (hotkey/tray/extension-server warnings) are dismissible; Ctrl+Enter re-translates, Ctrl+Shift+C copies the translation; × hides when a tray/hotkey can restore the window, otherwise quits
- Linux/GNOME popup (`translator-popup`): same state machine with a CSS-styled card layout, spinner, download progress bar (pulse when the total is unknown), error card + 重新翻译 button, and 2 s copy feedback; Ctrl+Enter / Ctrl+Shift+C shortcuts; dismissible update banner; default window 560×380
- Hotkey/extension-server failures no longer masquerade as model errors in the Windows/macOS client
- Both clients size the window to the translation: it grows with the content up to 70% of the monitor height and only then scrolls, so short results shrink the window instead of leaving empty space (egui sends `ViewportCommand::InnerSize`; GTK uses `propagate_natural_height` + `max_content_height` and `set_default_size`)


Verification:

- `cargo test --locked` in `desktop/translator-core` (33 passed), `desktop/translator-popup` and `desktop/translator-popup-desktop` (hotkey parser tests) all green; `cargo check` clean for both UI crates
- Adaptive height measured on the live GNOME/Wayland session (1920x1080, 70% cap = 756px): GTK window 333px for a short translation, 397px for the error card, 756px for a 1300-character translation; egui window 240px short and 752px long, with `ViewportCommand::InnerSize` confirmed via `content_rect`
- egui visual rendering still needs a real Windows/macOS pass; the Linux run exercises the layout/state logic only (capture/hotkey/tray are stubs)


Windows real-machine follow-up (2026-10-01):

- Verified the new UI on a Windows 11 machine (installed client): state machine, adaptive height, hotkey capture, tray and the embedded HTTP API all worked; a short paragraph translated in ~3.4 s with the hy-mt1.5-1.8b q4 model
- Fixed the black frame around the card: eframe 0.36 defaults to wgpu and its DX12 backend only offers an opaque swapchain for Win32 HWNDs (`wgpu-hal` lists `CompositeAlphaMode::Opaque` only), so the transparent window margin and shadow were composited black by DWM. Windows now uses an opaque window (`with_transparent(false)`), lets DWM round the corners (`DWMWA_WINDOW_CORNER_PREFERENCE`), fills the window with the card (no outer margin or shadow) and clears with the card fill; macOS keeps the transparent floating card. A glow/OpenGL attempt was tried first and still showed the black margins on the Intel UHD driver
- Rebuild note: the repo path plus cargo's `target/release/build/llama-cpp-sys-2-*` directory exceeds MSBuild FileTracker's path limit (`MSB6003`), so `desktop/install-windows.ps1` now builds into the short `%LOCALAPPDATA%\OpenTranslator\build` target dir (it also points the Startup shortcut at that binary); manual builds can set `CARGO_TARGET_DIR` to any short path


---

## Milestone: Flexible Source Language (Auto Detection)


Completed:

- `translator-core::detect` (new): language detection via `whatlang` trigrams; `detect(text)` maps the result to a shared tag (zh/en/ja/ko/fr/de/es/ru) and returns `None` when confidence is below 0.5 or the language is unsupported; `resolve_source(configured, text)` passes explicit tags through, replaces `auto` with the detected tag and keeps `auto` when unsure
- `translator-core::languages`: `AUTO_CODE` / `AUTO_LABEL`, `source_label()` and `source_options()` (auto + the shared list) so both clients offer the same 自动检测 entry; `is_supported()`
- `translator-core::args`: default source is now `auto` (was `en`); precedence unchanged (CLI > file > env > defaults)
- `translator-core::settings`: `persist_source()` plus a generic `persist_value()` / key-aware `apply_value()` now back `persist_target()`
- GTK popup: source dropdown (自动检测 + languages) with a detection hint next to it (e.g. （英语）), target dropdown stays; both write back to the config file and re-translate the current text; the title shows display names; the `--print` path resolves `auto` too
- Windows/macOS client: same source dropdown + detection hint in the header, persistence and re-translate behavior; the title and headless `--print` path resolve `auto`
- Browser extension: default source is `auto` in the worker/content/options page
- Detection only selects the prompt language, and the default HY-MT prompt is source-agnostic, so the change is safe for the default engine


Verification:

- `cargo test` in `desktop/translator-core` (41 passed, including detection tests); `desktop/translator-popup` builds and links; `desktop/translator-popup-desktop` tests green; `node --check` clean for the extension scripts; `browser/test.sh` Chrome e2e passed (11/11 checks, worker + content script translations)
- Headless GTK smoke test against the running llama.cpp service: `--stdin --print` with `source = auto` translated "kernel panic", a French sentence and an English sentence to Chinese ("内核崩溃" etc.)
- Pending: real-machine visual pass of the dropdown/hint on Windows/macOS


---


## Milestone: Selection UX Follow-ups (Clipboard + Popup Placement)


Completed:

- `translator-popup-desktop` capture restores the previous clipboard text after a successful selection copy (best-effort; non-text clipboard data such as images/files cannot be restored and is replaced by the captured text)
- The popup opens near the cursor: on Windows it reads `GetCursorPos` plus the cursor monitor's work area (`MonitorFromPoint`/`GetMonitorInfoW`, new `Win32_Graphics_Gdi` feature), places the window 12 pt below-right of the cursor and clamps it into the work area (multi-monitor and negative origins included), converting physical pixels with `pixels_per_point`; the hotkey, tray and first-show paths all go through `show_window`; macOS keeps its previous placement
- `clamp_to_work_area` is a pure helper covered by four unit tests


Verification:

- `cargo test --release` in `desktop/translator-popup-desktop`: 6 passed (4 placement + 2 hotkey parser tests)
- Windows real-machine pass: the clipboard keeps its pre-capture text after translating a selection, and the popup appears on the cursor's monitor near the cursor without crossing the work-area edges


---


## Milestone: Replace the Selection with the Translation (Windows)


Completed:

- `translator-popup-desktop` remembers the source window at capture time (`capture::foreground_window`) and adds a 替换原文 button (Windows only, shown when a source window is known): it writes the translation to the clipboard, calls `SetForegroundWindow`, sends Ctrl+V (real VK_V scancode), then restores the previous clipboard text
- Failures (e.g. the source window has closed) surface through the notice banner; success shows 已替换 for 1.5 s
- macOS and the GTK client keep copy-only behavior for now


Verification:

- `cargo test --release` in `desktop/translator-popup-desktop`: 6 passed
- Windows real-machine pass: 替换原文 replaces the selection in the source app, focus returns to that app and the clipboard keeps its previous content


---


## Milestone: Streaming Output + Modern Busy Indicator


Completed:

- `translator-inference`: `generate_streaming(prompt, stop_strings, options, on_delta)` reports every decoded token piece to a callback (the existing `generate` is unchanged); each `Request` carries an optional `on_delta`
- `translator-service`: `LlamaCppEngine::translate_blocking_streaming` shares the new `inference_request` helper; the `TranslationEngine` trait and HTTP API stay one-shot
- `translator-popup-desktop`: the worker forwards `Progress::Delta` pieces and the translation renders while it arrives — skeleton bars breathing before the first token, then the growing text with a pulsing caret instead of a spinner; while running the window repaints every 33 ms
- `translator-popup-desktop`: `fit_vertically` keeps the popup inside the monitor work area when the adaptive height changes, so a window near the bottom edge grows upward instead of off-screen; `WINDOW_MARGIN` unifies the placement margin


Verification:

- `core/inference`: the `TRANSLATOR_TEST_MODEL` test covers both `generate` and `generate_streaming` (deltas were emitted and the streamed result contains 内核)
- `core/translator-service`: 43 tests passed; `desktop/translator-popup-desktop`: 9 tests passed (7 placement + 2 hotkey parser)
- Windows real-machine pass: skeleton → streaming text with caret, the window grows upward near the bottom edge, and 替换原文 still replaces the selection


---


## Milestone: Language Switching + Thai Support


Completed:

- desktop client: a painted ⇄ button between the selectors swaps the language pair and shows the previous translation as the new source (it is translated back); the `⇄` (U+21C4) glyph was dropped because the loaded fonts had no such glyph and painted arrows render on every platform
- desktop client: target history (up to 3 entries, de-duplicated, supported tags only) switched with Ctrl+1/2/3; the previous target is pushed on every change and persisted as `recent_targets`
- `translator-core`: `FileConfig`/`Args` carry `recent_targets` and `persist_recent_targets` writes them back
- `translator-core`: Thai (`th`, 泰语) joins the shared language list and the whatlang detection mapping
- desktop client: per-script system-font fallback chains — Windows msyh → malgun → Yu Gothic → Leelawadee UI, macOS PingFang → Apple SD Gothic Neo → Hiragino → Thonburi, Linux Noto CJK → Noto Thai — fixing the tofu boxes for Korean and Thai
- browser extension: the bubble target list gains 泰语


Verification:

- `desktop/translator-core`: 42 tests passed (including Thai detection); `desktop/translator-popup-desktop`: 11 tests passed
- Windows real-machine pass: the swap button exchanges texts and languages, Ctrl+1/2/3 switches recent targets, and Korean/Thai translations render without tofu

---


## Milestone: GTK Quick Wins (Status, Selection, Errors)


Completed:

- `desktop/translator-popup`: the Done state shows `N 字符 · N ms` in the status bar (matching the Windows/macOS client); the source card text is selectable
- `desktop/translator-popup`: when the primary selection is empty, reading falls back to the clipboard, so a manual Ctrl+C before the hotkey still translates
- user-facing errors are Chinese with actionable hints: `read_selection` (wl-clipboard install), `translator-core::translate` (service connection/response, plus Chinese labels for the API error kinds) and `translator-core::services` (model missing, download, auto-start, Ollama, log paths)


Verification:

- `cargo test --locked` in `desktop/translator-core` (42 passed); `desktop/translator-popup` builds and `cargo check` is clean; headless smoke test: `kernel panic` → 内核崩溃, and the unreachable-service path prints 翻译服务 http://127.0.0.1:1 未运行（已通过 --no-start 禁用自动启动）
- Status bar/selectable source/primary fallback are compile-verified only; visual pass on GNOME still pending


---


## Milestone: GTK Language Swap + Recent Targets


Completed:

- `translator-core::languages::recent_target_list` and `swapped_pair` are shared by both desktop clients (the Windows/macOS client dropped its private copy and its two tests moved to translator-core); `swapped_pair` returns the swapped tag pair, using the detected tag for `auto` and refusing identical/unknown pairs
- `desktop/translator-popup`: a flat swap-icon button (`object-flip-horizontal-symbolic`) between the selectors exchanges the language pair, replaces the source text with the previous translation and re-translates it back (parity with the Windows/macOS client); it is disabled when both sides match or auto detection is unknown
- `desktop/translator-popup`: the previous target is remembered on every change (up to 3, persisted as `recent_targets`) and Ctrl+1/2/3 switch to it, re-translating in place


Verification:

- `translator-core`: 47 tests passed (2 for `recent_target_list`, 3 for `swapped_pair`); `desktop/translator-popup-desktop`: 9 tests passed; `desktop/translator-popup` builds with `cargo build --locked`
- The swap button and Ctrl+1/2/3 are compile-verified only; visual pass on GNOME still pending


---


## Milestone: Streaming Translation (SSE)


Completed:

- `TranslationEngine::translate_streaming` + `DeltaCallback` (`Box<dyn FnMut(&str) + Send + 'static>`): engines report decoded pieces; the default implementation emits the complete translation as one delta, `LlamaCppEngine` forwards the inference callback through `spawn_blocking`
- `TimeoutEngine`: streaming uses an idle timeout that restarts on every delta (unit-tested with a silent engine); non-streaming keeps the whole-request timeout
- API: `POST /translate/stream` returns SSE `{"type":"delta","delta":...}` events and terminates with `{"type":"done","translation","elapsed_ms"}` or `{"type":"error","kind","message"}`; the stream closes right after the terminal event; `TranslationError::kind()` is now shared with the JSON error mapping
- `desktop/translator-core`: `translate::translate_stream` parses the SSE stream (chunked framing, comments/keep-alives ignored) and reports deltas; error kinds mapped to Chinese
- `desktop/translator-popup`: the translation renders while streaming and the status line shows 翻译中… N 字符, then the Done state with character count and elapsed time; `--print` keeps the non-streaming path
- The Windows/macOS embedded server serves the new endpoint automatically (same router)


Verification:

- `core/translator-service`: 32 unit + 9 integration tests (3 new SSE tests: deltas+done, validation, error event); `desktop/translator-core`: 53 tests (parse unit tests plus stub-service streaming tests); `desktop/translator-popup` builds with `cargo build --locked`
- Live smoke against the release llama-cpp service: `curl -N` received 内核 at 51.348, 崩溃 at 51.386 and done at 51.425 (elapsed 267 ms); non-streaming `/translate` and the 400 validation path unchanged
- GUI rendering is compile-verified; visual pass on GNOME still pending


---


## Milestone: In-Window Selection / Clipboard Toggle


Completed:

- `translator-core`: `FileConfig`/`Args` accept `clipboard` (CLI `--clipboard` still wins) and `persist_clipboard` writes the flag back
- `desktop/translator-popup`: a header 剪贴板 toggle switches the read source between the Wayland primary selection and the clipboard; toggling persists the choice and re-reads the selection immediately; the toggle is hidden in `--stdin` mode


Verification:

- `desktop/translator-core`: 55 tests passed (file parsing, precedence and clipboard-mode tests); `desktop/translator-popup` builds with `cargo build --locked`
- The toggle interaction is compile-verified; visual pass on GNOME still pending


---


## Milestone: v0.2.0 Release


Released:

- Tag `v0.2.0` at `dcebea8` (CI run 36858635066 green first); release run 36859033510 passed the Windows, macOS and Linux package jobs and published `OpenTranslator-windows-x64.zip`, `OpenTranslator-macos-arm64.dmg` and `OpenTranslator-linux-x64.deb`
- Packaging fix before tagging: `make-dmg.sh` now injects the release version into `Info.plist` with PlistBuddy (explicit `VERSION` from the workflow, then the latest tag, then a placeholder), so the macOS bundle no longer ships the hardcoded 0.1.0; deb usage examples and the shallow-clone fallback were refreshed
- Release notes (edited after publishing) cover the highlights since v0.1.0: source-language auto detection + selector, SSE streaming, GTK swap/recent targets/clipboard toggle, Windows clipboard restore/near-cursor placement/替换原文, Thai support, plus install steps and the unsigned-build caveats


---


## Milestone: Silent Autostart (No Popup at Login)


Fixed:

- `desktop/translator-popup-desktop` no longer shows the window at launch: first-run model downloads and model-load errors used to force the window visible, which left it sitting on screen after login (the Startup shortcut / LaunchAgent starts the app at boot, and nobody needs a translation immediately)
- Download progress and errors now surface in the tray tooltip (`Tray::set_tooltip`, throttled to changes); the hotkey/tray actions still open the window, where the existing download-progress and error cards appear, and queued selections translate when the model is ready
- `--stdin` with initial text still opens the window immediately (explicit user intent); a unit-tested `tray_tooltip` builds the status text


Verification:

- `desktop/translator-popup-desktop`: 12 tests passed (3 new tray-tooltip tests); the Linux build compiles with the tray stubs
- Windows/macOS visual pass of the silent autostart still pending (needs a release to reach installed users)


---


## Milestone: v0.2.1 Release


Released:

- Tag `v0.2.1` at `26f292f` (CI run 36862376875 green first); release run 36862981211 passed the Linux (5m15s), macOS (6m12s) and Windows (10m33s) jobs and published the three assets; the dmg reports version 0.2.1
- Patch release carrying the silent-autostart fix; release notes document the fix plus update steps (Windows re-run `install.ps1`, macOS replace the app, Linux `apt install` upgrade in place)


---


## Milestone: Silent Autostart, Actually Hidden (post-v0.2.1)


Fixed:

- The v0.2.1 silent-start fix was incomplete: `ViewportBuilder::with_visible(false)` is a no-op in eframe 0.36 (`eframe::EpiIntegration::post_rendering` unconditionally calls `window.set_visible(true)` after the first painted frame for its white-flash fix), so the window still appeared at login even though the app no longer asked for it
- `PopupApp::new` now queues `ViewportCommand::Visible(false)` when startup should stay hidden (no `--stdin` text); eframe applies it right after that first frame, so the window ends the first frame hidden and loaded models / downloads never surface a window; if neither the hotkey nor the tray registered, the window is shown instead so the app cannot become unreachable
- Queued commands keep the intended order: a hotkey or tray action in that first frame still wins and shows the window


Verification:

- Live-tested on Windows against the installed v0.2.1 binary and the patched build (separate `TEMP` dirs, the user's running instance untouched): unpatched v0.2.1 shows `OpenTranslator (自动检测 → 中文)` 520x263 with `IsWindowVisible=true` ~14 s after launch; the patched build reports `IsWindowVisible=false`
- `desktop/translator-popup-desktop`: 12 unit tests pass; `--stdin` with initial text still opens the window immediately
- Installed users need the next release to pick this up (v0.2.1 still shows the window at login)


---


## Milestone: Direct Windows Reinstall (post-v0.2.1)


Fixed:

- `packaging/windows/install.ps1` and `desktop/install-windows.ps1` stop a running `translator-popup-desktop` instance before copying or building; Windows locks a running exe against overwrite, so upgrading while the old version ran used to fail with a sharing violation (or an LNK1104 from the linker in the source build)
- The packaging `README.txt` documents the upgrade flow: re-run `install.ps1`, the running old version is closed automatically, and the new one starts on demand or at the next login


Verification:

- Overwriting a running exe was reproduced failing with "正由另一进程使用"; the real-machine test then showed `Stop-Process` + `Wait-Process` still racing the lock while the old process finished exiting, so `packaging/windows/install.ps1` now retries the copy for up to 10 s and `desktop/install-windows.ps1` waits for the binary to unlock before building (both parse clean)
- End-to-end reinstall verified with the old version running: `install.ps1` stopped the instance, completed in 1.7 s with exit code 0, the installed exe hash matches the package, the Startup shortcut stays valid, and the newly installed build starts with `IsWindowVisible=false`


---


## Milestone: v0.2.2 Release


Released:

- Tag `v0.2.2` at `8cb4640` (CI run 36940722141 green first); release run 36943753792 passed the Linux (1m07s), macOS (1m05s) and Windows (1m41s) jobs and published the three assets
- Patch release carrying the "actually hidden at login" and "upgrade the Windows install in place" fixes; release notes document the fixes plus update steps (Windows re-run `install.ps1`, macOS replace the app, Linux `apt install` upgrade in place)
- The first publish (run 36941897001) shipped only auto-generated notes; the tag was deleted and re-pushed so the release matches the usual title/notes format


---


## Milestone: Manual Launch Shows the Window (post-v0.2.2)


Added:

- `--autostart` (translator-core `Args::autostart` + `parse_args`, `PopupApp::new`): only the login autostart starts silent in the tray; a manual launch (no flag) shows the window, so first-run download progress and model errors are visible
- `packaging/windows/install.ps1`, `desktop/install-windows.ps1` and `desktop/install-macos.sh` pass `--autostart` in the Startup shortcut / LaunchAgent entry
- The Windows installers start the app in the tray after install (unless `-NoStart`), so an in-place upgrade no longer leaves it stopped
- README.txt and AGENTS.md describe the new launch behavior


Verification:

- translator-core: 55 tests pass (parse_args covers `--autostart`); desktop client: 12 tests pass
- Live on Windows with the release build: no arguments shows `OpenTranslator (自动检测 → 中文)` with `IsWindowVisible=true`; `--autostart` reports `IsWindowVisible=false`


---


## Milestone: System Notifications (post-v0.2.2)


Added:

- `desktop/translator-popup-desktop/src/notify.rs` reports model download completion, model load/download failures and hotkey registration failures through the system notification center while the window is hidden: WinRT toast on Windows (`tauri-winrt-notification`), `mac-notification-sys` on macOS, no-op elsewhere
- Notifications are gated on the window being hidden (`PopupApp::window_visible`, maintained by `show_window`/`hide`) so a visible window with its error cards is never duplicated


Verification:

- Live on Windows: `--autostart` + a corrupt `TRANSLATOR_MODEL_PATH` → 1 toast, window stays hidden; `--autostart` with the hotkey held by another instance → 1 toast, window stays hidden; manual launch + the same model error → window visible with the error card and 0 toasts
- Desktop client: 12 unit tests pass; macOS compilation of the new dependency is covered by CI


---


## Milestone: One-Click Update (post-v0.2.2)


Added:

- `translator_core::update::ReleaseInfo` now carries the release `assets` (`ReleaseAsset { name, url }`) parsed from the GitHub response
- On Windows the update banner and the tray 有新版本 item offer 立即更新: the client downloads `OpenTranslator-windows-x64.zip` to `%TEMP%\open-translator-update`, extracts it with `Expand-Archive`, verifies `install.ps1` + the exe are present and starts the installer; the installer stops the app, replaces the binary and restarts the new version in the tray, so the old process quits after `Installed`
- The banner shows download progress and 更新失败 + 重试; macOS/Linux keep opening the release page


Verification:

- translator-core: 55 tests pass (`reports_a_newer_release` now covers asset parsing)
- Desktop client: 14 tests pass, including a Windows pipeline test (a local stub server serves a zip built with `Compress-Archive`; the extracted installer script runs and the worker reports progress + `Installed`)
- Live on Windows: a stub release server + crafted package, UIAutomation clicked 立即更新 in the real banner → download, extraction, stub installer and app exit all verified


---


## Milestone: Translation History and Pinned Window (post-v0.2.2)


Added:

- translator-core `history` module: `HistoryEntry` (source/target/text/translation), newest-first `push` (deduped by text + target, capped at `MAX_ENTRIES` = 10), `load`/`save` via `paths::history_path()` (`history.json` next to the config)
- Desktop client: every completed translation is recorded; the footer 历史 button (or Ctrl+H) opens a scrollable panel (清空, click an entry to restore its translation and language pair); 固定 keeps Esc/× from hiding the window, shows 已固定 in place of ×, and Esc closes the history panel first


Verification:

- translator-core: 60 tests (5 new history tests); desktop client: 15 tests
- Live on Windows via UIAutomation: 历史 shows a seeded entry and clicking it restores the translation; 固定 → Esc keeps the window visible, 取消固定 → Esc hides it; a real `--stdin` translation wrote a valid UTF-8 `history.json` entry (backed up and restored around the test)


---


## Milestone: Settings Panel (post-v0.2.2)


Added:

- The footer 设置 button opens a panel that edits the hotkey (应用 re-registers immediately; when the new binding is invalid or taken the old one is restored and the error shown), the model path (保存; empty means the per-user default) and the `auto_download` / `check_updates` / `serve_extension` switches (persisted through `settings::persist_value`, next start), plus 打开配置目录
- Esc closes the settings panel before hiding the window; opening 设置 closes the history list and vice versa


Verification:

- Desktop client: 17 tests pass (new `bool_value` / `bool_str` tests)
- Live on Windows via UIAutomation with a backed-up config: the panel renders; toggling 自动下载模型 writes `auto_download = false`; typing Ctrl+Alt+Y and 应用 updates the window hint and writes `hotkey = Ctrl+Alt+Y`; applying the taken Ctrl+Alt+T shows the error and keeps the old binding


Follow-up (layout):

- Review feedback: settings do not belong inside the translation card, so the tray menu now carries 历史… and 设置… (TrayCommand::History/Settings): both show the window, 设置 renders as a full-page view (返回翻译/Esc backs out) instead of a banner panel, and the footer only carries translation actions (复制译文 / 替换原文 / 重新翻译 / 固定), with 历史/设置/退出 kept only when no tray is available; `--settings` opens the page directly
- Live-verified with `--settings`: the page replaces the translation content, 返回翻译 restores it, and a tray-active footer has no 历史/设置 buttons
- The first live pass clipped the page's last row (打开配置目录): `body`/`settings_page` now include the header/banners above the page and measure the frame's outer rect; re-measured with 61 px clearance and verified by the user


---


## Milestone: Tauri Client Migration (post-v0.2.2)


Added:

- New `desktop/translator-popup-tauri` crate (Tauri v2 + plain HTML/CSS/JS): shell (tray/hotkey/single-instance/`--autostart`) → engine/capture/streaming → history, pin and settings pages → extension server, update check/one-click update and replace-in-place; every slice verified live on Windows (UIA against the real WebView via `--force-renderer-accessibility`, `--print`, and a stub release server)
- Linux switched to the Tauri client: `packaging/linux/make-deb.sh` packages it as `/usr/lib/open-translator/translator-popup` (runtime deps webkit2gtk-4.1 / gtk3 / ayatana-appindicator), `open-translator-setup` registers the GNOME shortcut as `<bin> --translate` and writes a `~/.config/autostart` entry, and the Tauri single-instance plugin forwards `--translate`/`--settings`/`--history` to the resident app (Wayland-friendly because the shortcut only needs to launch the second instance)
- `translator-core::args` gained `--translate`; CI has a Tauri job on ubuntu and the release Linux job builds the Tauri crate


Verification:

- Windows: engine `--print`, GUI streaming, cursor placement, hidden notifications, history/settings UIA flows, `/health`, stub-release one-click update (marker + exit) — all green as recorded in the individual commits
- Linux: no local toolchain, so the new ubuntu `tauri` CI job is the build gate; the deb ships the Tauri client and the GTK popup stays in the tree


---


## Next Steps: Tauri Client Completion (after v0.2.2)


Remaining work, in the agreed order:

1. (Done 2026-10-02 — see the Linux Real-Machine Verification milestone) Linux real-machine verification (compilation was CI-gated only before)
   - Install the deb on Ubuntu 24.04 on both Wayland and X11: `open-translator-setup` (GNOME shortcut `<bin> --translate` plus the `--autostart` entry), single-instance forwarding, tray icon (GNOME may need the AppIndicator extension), `wl-paste` selection capture, model download, cursor placement / pin / bottom clamp, the transparent floating card, `notify-send` notifications and the desktop entry/icon
   - Check `apt install` in-place upgrades over the previous deb
2. macOS verification (postponed — no machine available)
   - Build the dmg from the Tauri client; confirm the app starts in the tray/menu bar (LSUIElement), the Accessibility prompt for Cmd+C capture, the transparent floating card, the LaunchAgent (`--autostart`) and the browser fallback for updates
3. Release migration notes
   - Installed v0.2.x eframe clients cannot one-click update to the Tauri package (their updater requires `translator-popup-desktop.exe` in the zip); release notes must tell users to download the new zip and run `install.ps1` once. Tauri→Tauri one-click updates are already verified. (README and the release skill now carry the upgrade note; the v0.3.0 release notes remain)
4. (Done 2026-10-02 — see the Legacy Client Cleanup milestone) Legacy client cleanup
   - `desktop/translator-popup-desktop` (eframe), `desktop/translator-popup` (GTK) and `desktop/install.sh` were removed; their CI jobs (`desktop-popup`, `desktop`) were dropped and the docs describe the Tauri client as the only desktop client
5. (Done 2026-10-02 — see the Docs/Tests/CI Cleanup milestone) Docs and tests
   - README/AGENTS/release skill describe the Tauri client as shipped; `packaging/linux/README.txt` no longer claims automatic shortcut registration or GTK4
   - The Windows update-pipeline test runs in the Tauri crate and the cursor/work-area clamping is a unit-tested pure helper
   - CI tests the Tauri client on ubuntu/windows/macos and a `tauri-frontend` job checks the config JSON and `ui/main.js`
6. Known issues
   - Local incremental builds can mis-embed `OPEN_TRANSLATOR_VERSION` (the update banner shows even for newer builds); clean CI/release builds are correct
   - Windows keeps the opaque full-window card for now (tao does not use `WS_EX_LAYERED` there, so the transparent path was not enabled on Windows)
7. (Done 2026-10-02 — see the v0.3.0 Release milestone) Release v0.3.0
   - Tagged with the release skill, three assets published, release notes document the v0.2.x upgrade path; macOS is CI-built only and flagged as pending real-machine verification


---


## Milestone: Linux Real-Machine Verification (2026-10-02, post-v0.2.2)


Verified (Ubuntu 26.04 + GNOME Wayland, locally built deb):

- `apt install` upgraded the installed 0.1.0 GTK package in place: the old `translator-service` was removed, the Tauri binary, `/usr/bin/translator-popup` symlink, desktop entry and icon installed, and `open-translator-setup` updated the existing GNOME shortcut to `/usr/bin/translator-popup --translate` plus the `--autostart` entry
- The resident `--autostart` instance stays hidden and registers the AppIndicator item; `--stdin --print` and `POST /translate` both return translations (existing model, no download); `/health` reports the llama.cpp engine
- Single-instance forwarding: `--translate`/`--settings`/`--history` second instances exit in ~0.07 s and the resident handles them; `--translate` captured the `wl-copy --primary` selection, translated it and recorded `history.json`
- A hidden model failure raises the `notify-send` notification (captured on `org.freedesktop.Notifications` with dbus-monitor) while the window stays hidden
- User-verified: tray icon, card transparency/rounded corners, cursor placement, bottom clamp and pin (固定) behavior


Fixed:

- Tray menu clicks could not raise an already-visible card: GNOME refuses focus/raise to a token-less background app and sets `_NET_WM_STATE_DEMANDS_ATTENTION`; `show_main` now pulses `set_always_on_top(true)` for 700 ms (restoring the pin state afterwards) when the window was already visible
- `固定` (always-on-top) cannot work on native Wayland (GTK keep-above is X11-only); the client now prefers the X11 backend when a Wayland session offers XWayland (`GDK_BACKEND=x11` when `GDK_BACKEND` is unset), and `GDK_BACKEND=wayland` opts back out
- The native-Wayland tray menu rendered blank labels; the XWayland default also sidesteps that Shell rendering anomaly
- The tray menu labels came back blank after an autostart login: the AppIndicator extension drops the label property fetch when a concurrent layout update cancels it and never retries, so manual restarts only masked it. The client now nudges the update menu item once while the menu is closed (4 s and 15 s after an autostart launch), making the next open re-read every label


Remaining:

- None — the Xorg session pass and the login-autostart check both passed on 2026-10-02 (the logout/login that verified the tray-menu nudge also covered them)


---


## Milestone: Docs/Tests/CI Cleanup (2026-10-02, post-v0.2.2)


- README/AGENTS/release skill now describe the Tauri client as the shipped desktop client (install/upgrade steps, platform table, repo layout, dev commands, CI/release descriptions); `packaging/linux/README.txt` no longer claims the shortcut is registered automatically and lists the real runtime dependencies
- Tauri client tests: `card_position` is a pure helper with unit tests (normal placement, bottom-edge slide, corner clamps, negative monitor origins, oversized card); `run_update_install_with` is testable and a Windows test downloads a stub zip from an axum server, extracts it, runs the stub `install.ps1` and checks the reported progress
- CI: the Tauri job runs `cargo test --release` on ubuntu/windows/macos and a new `tauri-frontend` job validates `tauri.conf.json`/`capabilities` JSON plus `node --check ui/main.js`
- Migration notes for v0.2.x eframe users are in the README, the release skill and the v0.3.0 release notes


---


## Milestone: v0.3.0 Release (2026-10-02)


Released:

- Tag `v0.3.0` at `8341470` (CI run 36973212111 green first); release run 36973900935 passed the Linux (~8m), macOS (~7m23s) and Windows (~11m45s) package jobs and published `OpenTranslator-windows-x64.zip`, `OpenTranslator-macos-arm64.dmg` and `OpenTranslator-linux-x64.deb`
- First release shipping the Tauri client on all three platforms; the release notes list the highlights (tray/hotkey/history/pin/settings, in-app Windows updates, XWayland behavior), the Linux `open-translator-setup` step and the v0.2.x upgrade path (the old eframe client cannot one-click update: download the zip and run `install.ps1` once)
- Windows and Linux are real-machine verified; the macOS dmg is CI-built only and flagged as pending real-machine verification in the release notes


---


## Milestone: Legacy Client Cleanup (2026-10-02, after v0.3.0)


- Removed `desktop/translator-popup` (GTK), `desktop/translator-popup-desktop` (eframe) and `desktop/install.sh`; both clients stay available in git history and the pre-v0.3.0 release tags (v0.2.2 remains downloadable as a macOS fallback)
- CI dropped the `desktop` and `desktop-popup` jobs; the Tauri job covers all three platforms
- README/AGENTS/ARCHITECTURE/PROJECT_STATUS now describe the Tauri client as the only desktop client, and `desktop/translator-core`'s core-binary test fixture points at the Tauri binary; the Windows installers still stop/delete the legacy eframe exe so v0.2.x users upgrade cleanly


---


## Milestone: Update UI in Settings (2026-10-02, after v0.3.0)


Review feedback: the update banner crowded the translation card (one row with the message, ×, 查看 and 立即更新), so update handling moved into the settings page.

- The translation view no longer shows any update UI; the settings page gained a 版本与更新 section with the current version (`OPEN_TRANSLATOR_VERSION`), 检查更新, 更新说明/前往下载 (macOS/Linux) and the primary 更新/重试 action, plus the download progress bar inline
- `UpdateInfo` now stores the version and `can_install`; the new `check_update_now` command runs a manual check and reports `update-checking`/`update-none`/`update-check-failed` (the startup check stays silent about those), and `get_update_state` re-seeds the page when the startup check finished before the webview was listening; `SettingsPayload` carries `app_version`
- The tray 有新版本 v… item is unchanged and remains the notification channel while the window is hidden; the Windows download/install path is untouched


Verification:

- `node --check ui/main.js` passes; `cargo check --locked` passes; `cargo test --locked` on Windows: 6 tests pass, including the update-pipeline stub-server test


---


## Milestone: Desktop UI Redesign (2026-10-03, after v0.3.1)


Review feedback drove a visual and interaction pass over the Tauri client's card, history and settings pages.

- Card controls are iconified (inline SVGs with `aria-label`s replace the text/emoji buttons: pin/close/swap/replace/retranslate/copy); tooltips carry the shortcuts (`复制译文（Ctrl+Shift+C）`, `重新翻译（Ctrl+Enter）`, `交换语言（译文变为新原文）`), and copy flashes a check icon for 1200 ms
- The translator card gained 朗读译文 (`speechSynthesis`, per-language voices), a right-click 复制/全选/朗读 menu, a status dot (ready/loading/error) and a single streaming paragraph; the titlebar has a 历史 (`Ctrl+H`) button and `Ctrl+,` opens settings
- History page redesigned: entries carry a timestamp (`HistoryEntry.at`, stored as `SystemTime` ms on push in `main.rs`), rows show relative time (`刚刚`/`N 分钟前`/`昨天`/`M月D日`) + translation + chevron, the empty state is `暂无翻译记录 / 选中文字按 Ctrl+Alt+T 开始`, and 清空历史 asks for confirmation before clearing
- Settings page regrouped into 常规/模型/服务/更新 with `.setting-row`s, `role="switch"` toggles and `i` info tooltips; the hotkey field is a recorder (double-press confirms, Esc cancels, a modifier is required, `恢复默认快捷键 Ctrl+Alt+T` shows while recording, 已应用 feedback); the back label is 完成, the model path autosaves on blur/Enter, and 更新到 v<version> falls back to 前往下载 (`open_release_page`) when the asset cannot be installed
- Pin state persists (`FileConfig.pinned` → `SettingsPayload.pinned`; startup restores always-on-top) and `resize_window` accounts for `scale_factor()`
- Tray/placement polish: tray and CLI opens center the window on the work area (new `center_window` command + `center_window_position`), settings/history resize then center, tray separators and localized update texts (`正在检查更新…`/`已是最新版本`/`更新检查失败`/`更新检查已关闭`/`有新版本 v<ver>…`), macOS template tray icon


Verification:

- CI run 37111545988 (`d2e259f`) green on all 12 jobs; the preceding run 37110921070 was red only in the Chrome e2e close-button check, fixed by making the expectation hover-aware (`matchMedia("(hover: hover)")`)
- Real-machine visual pass of the redesigned card/settings/history on Windows/macOS pending


---


## Milestone: Browser Extension Overhaul (2026-10-03, after v0.3.1)


- Manifests bumped to 0.2.0; shared `languages.js` (`OT_LANGUAGES`/`OT_SOURCE_LANGUAGES`); Chrome MV3 gained `alarms` and `action`, Firefox MV2 the toolbar popup
- Translations stream: the background holds a long-lived port and parses `POST /translate/stream` SSE, with cold-start retries (`STREAM_RETRIES=3`, 1500 ms) and a fallback to `/translate` when streaming is unavailable; a 1-minute `open-translator-health` alarm drives the badge (`!` when the service is down)
- Toolbar popup (`popup.html`/`popup.js`): service status, 最近翻译 history (click copies, 清空历史), target language, 划词自动翻译, per-site 本网站不自动翻译 and a 设置… link
- Bubble rebuilt: visible controls are state-driven (target-language select, copy and a ⋯ menu with 复制双语/复制原文/朗读/替换原文/重新翻译/互换源/语言设置…), only 停止 while streaming and only 重试 on error; icon buttons with labels, a 已复制 flash, a shimmer + blinking caret replacing the spinner, and the close button moved into the action row (hover-revealed); popover styling (10 px radius, menu animation, custom select chevron, focus rings, dark palette)
- Replace-original works in `<input>`/`<textarea>`/`contenteditable`; selections in inputs/textareas translate; auto-translate guards (`autoTranslateMinLength` default 2, `autoTranslateDelay` 200–800 ms) and a `disabledSites` opt-out; history capped at 20 entries, deduped by text + target
- `translate-clipboard` command (`Alt+Shift+Y`, `clipboardRead` permission); context-menu clicks on pages without a content script (PDF viewer) open `result.html` in a small window; the content script runs in all frames
- Browser e2e (`browser/test-chrome.mjs`) extended: history-recorded, ⋯ menu, textarea replace, waiting/streaming classes, icon buttons and the hover-revealed close button


Verification:

- CI run 37111545988 green (browser static checks, `web-ext lint` and the Chrome e2e job included)


---


## Milestone: Extension Release Assets (2026-10-03, after v0.3.1)


- `release.yml` gained a `browser-extension` job: `browser/build.sh all --zip`, renaming to `OpenTranslator-browser-chrome.zip`/`OpenTranslator-browser-firefox.zip`, uploaded as workflow artifacts and attached to `v*` releases
- `browser/build.sh` zips with a top-level folder, copies `packaging/browser/README.txt` into each build and reads the version as UTF-8
- README gained a 浏览器扩展 install section (zip download → 加载已解压的扩展程序 / 临时载入附加组件); the Firefox zip stays unsigned, so it only supports temporary loading


Verification:

- Tagged builds attach the two zips alongside the desktop packages; `workflow_dispatch` keeps them as artifacts only


---


## Milestone: v0.4.0 Release (2026-10-03)


Released:

- Tag `v0.4.0` at `17019b6` (CI run 37112355538 green first); release run 37112505593 passed the Linux (~6m), macOS (~7m47s), Windows (~10m10s) and browser-extension (~7s) jobs and published five assets: `OpenTranslator-windows-x64.zip`, `OpenTranslator-macos-arm64.dmg`, `OpenTranslator-linux-x64.deb`, `OpenTranslator-browser-chrome.zip` and `OpenTranslator-browser-firefox.zip` (first release attaching the extension zips)
- Release notes summarize the desktop redesign (iconified card controls, speech/context menu/status dot, history and settings rework, tray/placement polish) and the extension overhaul (streaming bubble, toolbar popup, history, per-site opt-out, clipboard/PDF paths), plus the usual install/upgrade steps
- Desktop real-machine visual pass on Windows/macOS still pending; macOS remains CI-built only


---


## Linux Real-Machine Verification of the v0.4.0 Redesign (2026-10-03)


- Verified the shipped `OpenTranslator-linux-x64.deb` (0.4.0, GitHub release asset) on Ubuntu 26.04 + GNOME Wayland, client running through XWayland (`prefer_x11_backend`): redesigned translator card, settings (常规/模型/服务/更新, hotkey recorder, switches, config path, 当前版本 v0.4.0) and history (newest first, relative times, language tag, 清空历史) render correctly
- Selection capture (`wl-paste --primary`) works through both single-instance forwarding (`--settings`/`--history`/`--translate`) and a cold `--translate` start; translations, history writes and the `/health` + `POST /translate` extension API were exercised with the embedded llama.cpp engine
- Global shortcut Ctrl+Alt+T triggers translation; `--autostart` starts hidden; the StatusNotifierItem is registered with GNOME and its DBusMenu labels are populated after login (显示窗口/历史…/设置…/已是最新版本/退出, update check reached GitHub); a hidden model-load failure invoked `notify-send` with the expected message (captured with a stub)
- Confirmed Linux gaps (fixed in the next section): 复制 is a no-op (no Linux clipboard backend: `arboard`/`enigo` are Windows/macOS-only dependencies), 替换原文 stays hidden (Windows-only capture/replace), 朗读 is hidden (this WebKitGTK exposes no `speechSynthesis`), and updates only open the release page; the copy tooltip also advertises a Ctrl+Shift+C shortcut that has no handler in the Tauri UI
- `apt install` does not restart a running tray client: the pre-upgrade instance kept the old binary until it was restarted (the restart is what loaded the redesign), unlike the Windows one-click updater that restarts itself
- Pointer input cannot be synthesized on the Wayland session (XTEST motion is ignored by the compositor), so click-driven checks (copy button, pin, context menu) remain for an Xorg-session pass; keyboard paths were driven with `XSetInputFocus` + XTEST and worked


---


## Linux Gap Closure (2026-10-03, after v0.4.0)


Implemented:

- Copy: `arboard` is now a Linux dependency too (X11 backend) and `copy_text` is shared across platforms; the advertised `Ctrl+Shift+C` and `Ctrl+Enter` shortcuts now have handlers
- Capture/replace: `wl-paste --primary` keeps working on Wayland and falls back to an ICCCM X11 PRIMARY read for Xorg sessions; `foreground_window` walks from the focused toolkit child to the managed top-level ancestor (WM_STATE) and rejects our own windows via `_NET_WM_PID`, so 替换原文 is offered for X11/XWayland source windows and pastes with `enigo` Ctrl+V after verifying the `x11rb` focus move (native Wayland sources stay unsupported)
- Speech: WebKitGTK exposes no `speechSynthesis`, so the client speaks through `spd-say` (speech-dispatcher; `--priority important --pipe-mode`, text over stdin) and cancels with `--cancel`, exposes a `tts_backend` command and hides 朗读 when unavailable; the empty state now shows the configured hotkey label
- Update: Linux one-click update — `update_asset` picks `OpenTranslator-linux-x64.deb`, `can_install` requires `pkexec` + `apt-get` and a well-formed SHA-256 (`sha256:` digest from the GitHub asset, now parsed by `translator_core::update`), the deb is downloaded with the shared downloader and verified, staged in a private 0700 directory (`$XDG_RUNTIME_DIR` or temp, unique name), and installed via `pkexec env DEBIAN_FRONTEND=noninteractive apt-get install -y --allow-downgrades`, then the client restarts itself; without pkexec/apt or a digest the release-page link remains
- Packaging: the deb Recommends `speech-dispatcher` and `pkexec`, and postinst tells manual upgraders to restart the client

Review hardening (same day, after an adversarial review of the changes):

- One long-lived `arboard::Clipboard` in `AppState` fixes X11 clipboard ownership (copy content and replace-restore no longer die with a dropped instance); the Linux replace path runs behind a guard that restores the clipboard and releases Ctrl on every exit; `replace_text` is now `#[tauri::command(async)]` so its ~385 ms paste sequence leaves the main thread alone
- The Linux update requires the GitHub asset SHA-256 and stages the deb in a private 0700 directory, so another local user can no longer swap the package that pkexec installs
- TTS emits `speech-ended` (with a generation token) from the reaper thread so 朗读 leaves its active state, and the model-download progress path no longer spawns `spd-say --cancel` per event; the "no selected text found" marker is a shared constant, and saving a hotkey refreshes the empty-state label


Verified on the same Ubuntu 26.04/GNOME Wayland machine with a local release build:

- `arboard` writes the X11 CLIPBOARD and a Wayland `wl-paste` reads it back (XWayland bridge); the copy button was also exercised by hand
- Forced X11 with a broken `WAYLAND_DISPLAY` (so `wl-paste` fails) still captured the primary selection through the new X11 fallback and translated it
- With an X11 `zenity` entry focused, `--translate` recorded the window and the card showed the 替换 icon; `XSetInputFocus` to that window sticks, so the focus step is allowed (the Ctrl+V injection itself cannot be exercised on this Wayland session — Mutter drops XTEST events — so the physical Xorg pass remains)
- The settings page shows 更新到 v0.4.0 (installable) on Linux with pkexec/apt present; `cargo test --release --locked` passes (including the pkexec argument builder test)
- 朗读 appears because `spd-say` is installed; the exact `--priority important --pipe-mode` invocation was validated against a dummy output module (no sound played), and `spd-say --cancel` stops playback


---


## Linux TTS Fixes (2026-10-03, after the gap closure)


- `speak_text` now runs `spd-say --wait`: without it spd-say exited right after queueing, so `speech-ended` and the 停止朗读 state fired immediately and a second click restarted the utterance instead of stopping it
- `stop_speaking` uses `spd-say --stop` (plus `--cancel` for anything queued) — the previous per-connection `--cancel` could not interrupt the message owned by another spd-say process
- the target language is passed to speech-dispatcher (`zh` → `cmn`, region suffix stripped) and the new `tts_voice_available` command hides 朗读 when `spd-say -L` has no voice for it
- verified on the dev machine: a `--wait --pipe-mode` child stays alive during playback and `spd-say -S` terminates it; unit tests cover the language mapping, the command args and the voice-listing parser


## Tray Click Behind a Fullscreen Window (2026-10-03, after the TTS fixes)


- User report: tray menu items seemed to work only when clicking the text; instrumented the Shell side (Ubuntu AppIndicator extension) and the app's DBusMenu: menu rows span the full width (206px when open, label ~44px), the blank area right of the text delivers `clicked` events, and hover covers the whole row — the hit box was never the problem
- Real repro: with the card hidden and a fullscreen VS Code window, a tray 历史…/设置… click switched the view but the newly mapped card landed below the fullscreen window, so it looked like nothing happened until VS Code was hidden
- Fix: `show_main_with` now always runs the 700 ms always-on-top raise pulse (previously only when the card was already visible), so a freshly shown card also surfaces above fullscreen windows on GNOME; pinned windows keep their permanent always-on-top
- Verified on the dev machine (local 0.4.1 build): hidden card + fullscreen VS Code + tray 历史… now brings the history card to the front; `cargo test --release` 11 passed
- Review follow-up: the pulse is generation-guarded (`AppState::pulse_generation`) so two shows within 700 ms no longer let the older timer clear always-on-top while the newer pulse is still active; verified by firing two menu events 0.5 s apart and watching `_NET_WM_STATE_ABOVE` persist until the second pulse's deadline


## Milestone: v0.4.1 Release (2026-10-03)


Released:

- Tag `v0.4.1` at `c138c1e` (CI run 37123631360 green first); release run 37123869327 passed the browser-extension (~9s), Linux (~7m49s), macOS (~9m22s) and Windows (~12m26s) jobs and published five assets: `OpenTranslator-windows-x64.zip`, `OpenTranslator-macos-arm64.dmg`, `OpenTranslator-linux-x64.deb`, `OpenTranslator-browser-chrome.zip` and `OpenTranslator-browser-firefox.zip`
- Release notes summarize the Linux gap closure (X11 clipboard copy, X11/XWayland replace-in-place with PRIMARY fallback, speech-dispatcher read-aloud, verified one-click deb update), the simplified history rows, the version-prefix fix and the fullscreen tray fix, plus the usual install/upgrade steps
- Windows/macOS desktop real-machine visual pass still pending; macOS remains CI-built only


---


## Milestone: Browser Extension Inline Translation (2026-10-03, after v0.4.0)


Next feature after the selection flow: borrow 微信输入法's 边写边译 (type, see the translation live, commit with a key). The client is not an IME, so the MVP lives in the browser extension content script, where the field text and caret are directly readable.

- `content.js` gained a self-contained typing module: a 500 ms debounce after `input` (skipped while composing), sentence extraction around the caret (delimiters `。！？!?；;` + newline, trailing delimiter included), an element session for `<input>`/`<textarea>` and a block/Range session for contenteditable (block text + TreeWalker offsets)
- A separate non-interactive shadow-DOM bubble (`data-opentranslator="type"`, `pointer-events: none`) anchors at the caret (hidden-mirror measurement for fields, range rects for rich text, viewport clamping) and streams through a second `translate` port; a new request cancels the in-flight one and identical sentences are deduped
- `Tab` (capture phase) commits: `setRangeText` + synthetic `input` for fields, restore range + `execCommand("insertText")` for contenteditable; `Esc`/blur/outside mousedown hides; `typeApplying` suppresses the echo input event after a commit
- `background.js` threads a `record` flag through `translate`/`streamTranslation`/`sendOneShot` so typing previews skip history, and a `record-history` message records only committed pairs
- Settings: toolbar popup checkbox; options page 边写边译 section (`typeTranslate`, `typeTranslateMinLength`, `typeTranslateDelay`); the feature defaults off and respects `disabledSites`; password/readonly fields never trigger
- Manifests bumped to 0.3.0 (AMO rejects reused versions and 0.2.0 was already released)


Follow-up (same day, "change the language while typing"):

- The bubble became interactive: a target-language chip with a menu (picking a language retranslates the current sentence with the new target and updates the chip), and a 设置… gear that opens the options page; `mousedown` on the card is default-prevented so using the controls never blurs the edited field, and typing/Esc closes the menu instead of the bubble
- New `cycle-target` command (`Alt+Shift+L` suggested, rebindable at the browser shortcuts page): the background forwards it to the active tab and the visible bubble cycles through the current target plus up to three `recentTargets` (maintained on every target change in the top frame, falling back to zh/en/ja/ko when empty)
- E2E now covers the chip menu (ja→fr switch, chip label, settings button presence, retranslation) and the cycle path through the real `tabs.sendMessage` wiring (fr→ja); 32 checks pass
- Review feedback: the language menu was absolutely positioned above the chip and covered the translation being read; it now flows below the chip inside the card (the bubble repositions when it opens/closes so it stays in the viewport), with an e2e rect assertion (`menuTop >= statusBottom`)


Follow-up (same day, Apple-style UI pass):

- Shared design tokens (`UI_TOKENS_CSS`) now feed both bubble shadow roots: system font stack, neutral label/systemFill palette, translucent material with a solid fallback, concentric 12/8px radii, hairline + layered shadow, `color-scheme: light dark`
- Typing bubble rebuilt on the tokens: chip with SVG chevron and a real gear (the old icon was sliders), `Tab`/`Esc` keycaps, opacity-pulse waiting instead of the gradient shimmer, 2px rounded streaming caret, fade mask when the translation is clipped, and the language list as a section list (hairline separator, hover fill, accent checkmark, 6px scrollbar)
- Selection bubble unified on the same tokens; popup and options pages re-skinned (filled rounded controls, accent-colored checkboxes, thin scrollbars)
- Verified: `node --check` + `web-ext lint` clean, the 33-check Chrome e2e passes, and light/dark screenshots of the typing bubble (with/without menu), selection bubble, popup and options were reviewed


Follow-up (same day, unify the language dropdowns):

- The selection bubble, popup and options page used styled native `<select>`s whose OS-rendered popup could not match the typing bubble chip; new `dropdown.js` exposes `OTSelect` (chip button + checkmarked popup menu with hover/focused/selected states and arrow-key navigation)
- The native `<select>` stays hidden in the DOM as the value holder, so the existing `value`/`change` wiring is unchanged; content scripts share the isolated-world global and embed `OTSelect.cssText` in their shadow styles, while popup/options call `OTSelect.inject()`
- The typing bubble now uses the same component with `menuContainer: card` (in-flow section-list variant, preserving the "below the translation" behavior and `onToggle` repositioning); e2e selectors updated to `.ot-select-*`
- Screenshot review caught a missing wrapper insertion for page contexts (`select.parentNode.insertBefore`), fixed and re-verified: 33 checks pass on a fresh profile
- Bug report (real mouse): picking a language closed the menu without switching. Shadow DOM retargets `event.target` to the host, so `OTSelect`'s outside-mousedown check (`wrap.contains(event.target)`) fired for the menu's own items and closed it before click; it now uses `event.composedPath()`. The e2e language switches were changed from `element.click()` to trusted `Input.dispatchMouseEvent` so the mousedown/click path is actually exercised (both selection bubble and typing bubble), 33 checks pass

Verification:

- `node --check` on every extension script and `python -m json.tool` on both manifests; `browser/build.sh chrome` builds the dist
- Chrome e2e (`browser/test-chrome.mjs`, headless Edge 154 + the running llama-cpp service) gained a typing scenario: textarea and contenteditable bubbles, Tab hint, Tab commit and dismissal; all 26 checks pass
- Harness hardening found while testing: attach to the service worker by its `/background.js` URL (an options-page target could win otherwise) and cache-bust the test page query (a reused profile served the stale page)


---

## Milestone: Desktop UI Design Language (2026-10-03, after the extension dropdown unification)


Brought the Tauri client onto the same Apple-flavoured design language as the extension and documented it as a design principle.

- `ui/style.css` now defines the shared `--ot-*` tokens (neutral label ramp, system fills, `#007aff`/`#0a84ff` accent, 12/8px radii, hairline + layered shadow, system font stack, `color-scheme: light dark`); cards, groups, buttons, switches, keycaps, context menu, confirm dialog, scrollbars and the streaming caret were re-skinned on top of them, with Apple system green/orange/red for status colors
- The `#source-select` / `#target-select` language dropdowns render through the mirrored `ui/dropdown.js` (`OTSelect`): chip button + checkmarked menu, hidden native selects as value holders; `applyLanguageState` calls `OTSelect.sync` after programmatic updates so swap/recent-target shortcuts keep the chips accurate
- Fixed a pre-existing mojibake in `ui/main.js` ("妫€娴嬶細" → "检测：") spotted while editing
- Documented the UI design language in `docs/ARCHITECTURE.md` (tokens, controls, dropdown sync rule, motion, enforcement) and referenced it from AGENTS and the conventions

Verification:

- `node --check ui/main.js ui/dropdown.js`; previewed `ui/index.html` in headless Edge with a stubbed `window.__TAURI__` (translator/settings/history, light/dark) and confirmed a real-click language pick updates the native select and calls `set_source`
- Frontend-only change (CSS/JS): no Rust rebuild needed; the `tauri-frontend` CI job's `node --check ui/*.js` covers the new script


---

## Milestone: Dropdown Viewport Placement (2026-10-03, after the desktop UI pass)


- Long language lists were clipped when the chip sat low on screen (the menu is an absolute layer, so the bubble/window placement cannot reserve room for it); `OTSelect.open()` now measures the free space below/above the button, flips the menu above when below is tighter than the preferred height, and always clamps `max-height` to the available space so the list scrolls instead of overflowing
- The inline (typing bubble) variant keeps following the card flow and only clamps its height to the space below it; both variants recompute on resize and scroll while open, and the placement is applied before `onToggle` so card repositioning measures the constrained menu
- Mirrored into `desktop/translator-popup-tauri/ui/dropdown.js` (the two files differ only in the header comment)
- Verified: Chrome e2e gained a bottom-anchored selection scenario asserting `ot-select-menu-up` plus menu bounds (35 checks pass); a desktop preview at a 230 px viewport showed the constrained menu (`bottom 222 <= innerHeight 230`) with no script errors


---

## Milestone: Shared UI Sources (2026-10-03, after the dropdown placement fix)


- `shared/ui/tokens.css` and `shared/ui/dropdown.js` became the single sources; `shared/sync-ui.sh` copies/regenerates the extension copies (`tokens.css`, `tokens.js` with `:root` → `:host`, `dropdown.js`) and the desktop copies (`ui/tokens.css`, `ui/dropdown.js`), with a `--check` mode wired into a new `shared-ui` CI job
- Extension pages now `<link>` `tokens.css` instead of carrying their own `:root` blocks; content scripts load `tokens.js` first (manifests + `browser/build.sh` updated) and `content.js` dropped its inline `UI_TOKENS_CSS`, using `--ot-bg-material` for the translucent bubble background
- `OTSelect.place()` was extended with a best-effort visible-area clamp (`screenY` / `screen.availHeight` / window chrome) so menus stay reachable when the window extends past the bottom of the screen
- Accepted boundaries recorded in PROJECT_STATUS: OS-native surfaces (tray, notifications, dialogs, installers) stay native; the desktop card remains opaque (transparent-webview blur is unreliable)

Verification:

- `bash shared/sync-ui.sh --check`; `node --check` on all touched scripts; `browser/build.sh all`; `web-ext lint` clean
- Fresh-profile Chrome e2e: exit 0, 35 checks
- Token presence checked live on popup/options/result/desktop (`--ot-accent` `#007aff`, `--ot-radius` 12, expected chip counts)


---

## Milestone: View Switch on New Translations (2026-10-03, after the shared UI sources)


- Hotkey/`--translate` translations started while the card sat on the history or settings page stayed on that page even though the history list updated; `listen("source")` now switches back to the translator view before rendering the new source (history-entry clicks already did this)
- Verified with the desktop frontend stub in headless Edge: `source` from the history view and from the settings view both reveal `#translator-view` with the new text, no script errors; the local Windows instance was rebuilt and swapped for the manual check


---


## Milestone: v0.5.0 Release (2026-10-04)


Released:

- Tag `v0.5.0` at `c948d99` (CI run 37138675325 green first); release run 37138919115 passed the browser-extension (~9s), Linux (~8m0s), macOS (~5m44s) and Windows (~11m57s) jobs and published five assets: `OpenTranslator-windows-x64.zip`, `OpenTranslator-macos-arm64.dmg`, `OpenTranslator-linux-x64.deb`, `OpenTranslator-browser-chrome.zip` and `OpenTranslator-browser-firefox.zip`
- Release notes summarize the extension 边写边译 flow (caret bubble, `Tab` commit, target-language chip, `Alt+Shift+L` cycle-target), the shared Apple-style tokens and `OTSelect` dropdowns across extension and desktop, viewport-aware menu placement, the translator-view switch on new translations, and the single-sourced `shared/ui` copies behind the new `shared-ui` CI check, plus the usual install/upgrade steps
- Verified on the same tree before tagging: local Linux deb install + tray client manual pass, Firefox temporary-extension manual pass and Chrome e2e 36 checks; the CI flake that failed the previous run was fixed first (`test: stabilize the dropdown placement e2e checks`)
- Windows/macOS desktop real-machine visual pass still pending; macOS remains CI-built only


## Selection Translation (划词翻译): Plan + Linux Landing (2026-10-04)


- New design/plan document `docs/SELECTION_TRANSLATION.md`: three modes (`off`/`ball`/`auto`), config keys (`selection_mode`, `selection_delay`, `selection_min_length`), runtime state, event flow, platform matrix (Linux first; Windows mouse-hook + UI Automation and macOS NSEvent + AX staged) and the privacy rule that no clipboard writes or synthetic keys happen on Linux
- `translator-core` settings parse the three keys with clamping (`100..3000` ms, `1..50` chars, unknown mode → `off`) plus tests; run: 61 core tests green
- New `selection_watch.rs` on the client: 400 ms PRIMARY polling (direct X11 read on X11 sessions, `wl-paste` on Wayland), settle delay, minimum length, watcher-local dedupe, guards for pinned cards, our own focused windows and recent hotkey triggers; `ball` degrades to `auto` on native Wayland because there is no global anchor (settings page shows a note)
- Floating ball: hidden 44×44 transparent always-on-top window (`ui/ball.html`/`ball.css`/`ball.js`), 120 ms hover-intent dwell plus click fallback, positioned with the shared `card_position` clamp from an X11 `XQueryPointer` anchor, auto-hidden after 5 s behind a generation counter; hovering commits the pending selection, records the replace window and opens the card near the cursor
- Settings/card: new 划词翻译 group (mode through `OTSelect`, minimum length, trigger delay) and a gear button in the card titlebar; the empty-state hint follows the active mode; hotkey now hides the ball and suppresses the watcher for 1.2 s
- Verified: `cargo test --locked` green in both crates (client 15 tests incl. 4 new watcher/placement tests), `node --check` on the UI scripts, `json.tool` on the capabilities/config, `shared/sync-ui.sh --check`; manual X11 check still pending because the dev machine runs GNOME Wayland (only the direct fallback can be exercised there); committed as `5eb520c`


## Selection Translation: Windows Watcher (2026-10-04)


- Windows watcher added to `selection_watch.rs`: a `WH_MOUSE_LL` hook on its own thread with a message pump records down/up positions (drag threshold 4 px) and forwards mouse-up events through a channel; the worker settles until the mouse has been quiet for `selection_delay`, applies the shared guards and anchors the ball at the mouse-up point
- Selection text comes from UI Automation (`TextPattern.GetSelection()` on the focused element, password fields skipped via `CurrentIsPassword`); only a detected drag outside a known terminal class falls back to the synthetic Ctrl+C capture, so a plain click never touches the clipboard and a terminal can never receive SIGINT by accident
- Added the `windows` 0.61 dependency for UIA (plus `Win32_System_LibraryLoader` for `windows-sys`); `Cargo.lock` only gained the direct edge
- Verification without a Windows machine: the module was prototyped and `cargo check --target x86_64-pc-windows-msvc` in a scratch crate until clean, then integrated; `cargo test --locked` on Linux still green (15 tests). Real-machine check on Windows is still pending


## Selection Translation: macOS Watcher (2026-10-04)


- macOS watcher added to `selection_watch.rs`: a listen-only session `CGEventTap` (core-graphics) runs on its own thread's run loop, records left-button down/up positions (drag threshold 4 px) and forwards mouse-up events; the worker settles until the mouse has been quiet for `selection_delay`, applies the shared guards and anchors the ball with `LogicalPosition` (CGEvent coordinates are global points)
- Selection text comes from the AX API (`AXUIElementCopyAttributeValue` on the system-wide focused element, gated by `AXIsProcessTrusted`, using the accessibility permission the capture path already needs); drags without AX text fall back to `capture_selection` (Cmd+C is a plain copy, safe without a selection)
- Added direct macOS deps `core-foundation` 0.10 / `core-graphics` 0.25 (already transitive); `cargo check --target aarch64-apple-darwin` in a scratch crate was clean before integration, and `cargo test --locked` on Linux stays green (15 tests)
- `selection_supported()` now covers Linux/Windows/macOS; macOS behavior is compile-checked but not real-machine-verified (no Mac hardware), so it ships as an experimental watcher. Manual X11/Windows/macOS checks remain on the follow-up list


## Selection Translation: Real-Machine Feedback Fix (2026-10-04, after 9b4b066)


- Local Linux verification on GNOME Wayland found direct mode unusable: the 400 ms settle fired during brief pauses while dragging, and every trigger re-showed the card with `set_focus()` plus the 700 ms always-on-top pulse and a fresh cursor placement, so the card flickered and stole focus mid-selection ("没法好好选词")
- First fixes: Linux default settle raised to 900 ms (Windows/macOS keep 400 ms because they see the mouse-up); the Linux watcher rate-limits triggers to one per 1.5 s and keeps the pending text, so an evolving selection is translated once with the latest text instead of on every pause; the watcher/ball commit path now uses `show_main_popup` — no focus, no always-on-top pulse, and no repositioning while the card is already visible
- The second manual pass still felt unusable, which exposed two deeper defects: every watcher trigger started a new `translate_text` whose streaming events kept rewriting the card (multiple streams interleaving and engine work stacking behind the serialized actor), and the card mapped at cursor + (12, 18), i.e. exactly where the next drag in the same paragraph starts
- Second fixes: `translate_text` is now single-flight — while a translation streams, newer text only replaces a one-slot `queued` follow-up and the running request chains into it when it finishes; the popup placement prefers above the cursor (below only when there is no room, with a wider gap); switching the mode off clears the queued follow-up
- Third pass still could not select with the mouse because the card kept mapping where the user was working (native Wayland has no trustworthy global pointer position, and the 520x344 card then swallowed the next press): on native Wayland the popup now parks in the bottom-right corner of the work area, and every watcher popup is shown `set_focusable(false)` (X11/Windows/macOS keep the above-cursor placement); clicking the card re-enables focus via the new `card_engaged` command, and disabling the mode hides a watcher popup
- Rebuilt locally and restarted the tray client for the next manual pass; X11/Windows/macOS verification still pending


## Selection Translation: Docked Confirm Ball Redesign (2026-10-04)


- First-principles decision after the Wayland failures: an app cannot observe global input, pointer coordinates or place a window next to a selection there, so confirmation moves to a target that does not need any of that. Confirmation is now an explicit, mouse-continuous hover on a fixed docked ball, which is the only interaction Wayland allows a regular application to detect reliably
- Linux `ball` mode is now the docked confirm ball: right-edge-center default, draggable (JS drag + `move_ball_by`/`save_ball_position`, persisted as `ball_x`/`ball_y`, clamped), `ball_visibility` = `always` (dim idle, default) or `selection`, and `idle`/`armed`/`busy` states pushed to `ball.js` via the `ball-state` event. The watcher only arms the pending text; it never translates
- Hover-only commit with a 150 ms dwell (no click confirm, dragging never commits); the non-focusable card opens beside the ball (`show_card_next_to_ball`) and clicking it restores focus (`card_engaged`). Hovering the same selection again re-shows the card without retranslating; a new selection replaces the pending text
- `auto` is Windows/macOS-only now: Linux clamps it to `ball` in `from_config`, the settings UI removes the option, and the settings group gains 悬浮球显示 + 恢复默认 rows (the Wayland note is gone). Windows/macOS keep the selection-following ball and the shared settle/guard pipeline; translations stay single-flight
- Verified: `cargo test --locked` green in both crates (core 63, client 16), `node --check`, `cargo check`; local client rebuilt and restarted for the next manual pass, X11/Windows/macOS checks still pending


## Selection Translation: Real-Machine Hardening (2026-10-04)


- Manual testing on GNOME Wayland/VS Code found the real drag interrupter: the 400 ms `wl-paste` PRIMARY polling. An A/B run with reads disabled selected normally; an XFixes probe (with `wl-copy` driving the primary) proved GNOME mirrors every Wayland selection update to X11 as a selection-owner event (owner constant, timestamp changes). The watcher is now event-driven: it never reads while the selection changes and reads PRIMARY once after the settle delay; only a session without an X display falls back to slow `wl-paste` polling
- The docked ball was actually 200x200: GTK locked it at its default size because `resizable(false)` was applied before the size request; the ball now removes that flag and enforces 44x44 before its first show (verified on X: `44x44+1868+534`). Idle watcher ticks no longer call `set_position`/`show` on the ball
- UX fixes from the same pass: the ball returns to idle after a translation (a newer armed selection keeps it lit); a hover commit reads the current PRIMARY on the spot instead of waiting for the settle/arm; the card raises itself with the 700 ms always-on-top pulse after every commit so it cannot stay hidden behind the active window
- AT-SPI typing filter (`atspi` 0.30, Linux): tracks the focused text object and queries `GetNSelections`, so caret/text updates without a nearby selection change keep the ball dim (applies to GTK/Firefox and other apps that expose text events). VS Code/Electron with accessibility off emits only focus events, so there is no signal to distinguish typing from selection there; accepted as a known limitation (documented, with the `editor.accessibilitySupport` workaround). Diagnostics: every event kind is logged under `TRANSLATOR_SELECTION_DEBUG=1`
- Verified: `cargo test --locked` green in both crates (core 63, client 16), release build clean, manual pass on GNOME Wayland for docked ball + hover + settings + history; Windows/macOS/X11 checks still pending


## Settings Page: First-Principles Simplification (2026-10-04)


- Rethought the settings page from its purpose: a row exists only when users materially differ and the product cannot infer the answer; everything else is a default (or a config-file key), direct manipulation, or read-only status. The earlier "advanced disclosure" was removed as a patch for wrong defaults
- 划词翻译 is now two rows: 选中文字后 (不自动翻译/悬浮球翻译/立即翻译, segmented) and 按快捷键翻译 (hotkey). The 悬浮球显示, 最小字数, 触发延迟, 自动下载模型 and ball position/reset rows are gone from the UI — the first three stay config-file keys (`ball_visibility`, `selection_min_length`, `selection_delay`) and the position reset became direct manipulation: dropping the ball within 48 px of its default dock removes `ball_x`/`ball_y`
- 常规 keeps 检查更新 + 浏览器扩展服务; version/update controls, model path and config directory moved into a collapsed 关于 block (status, not decisions). Removed the now-unused `save_ball_visibility`, `save_selection_options` and `reset_ball_position` commands and the dead settings-payload fields
- New desktop `ui/segmented.js` renders the short mode select as a segmented control (a hidden native select stays the source of truth)
- Polish from the same review: all value controls right-align (the hotkey recorder previously stretched with `flex: 1`, so the keys sat next to the label); the model row gained an open-location button (`open_model_location` opens the configured model's directory, falling back to the default models directory or the config directory)
- Model-path semantics fixed while polishing: the "留空使用默认位置" placeholder was a lie — `resolve_model_path` used a saved empty value as a real (unloadable) path. Blank config values now fall back to the default model, `get_settings` exposes `default_model_path`, and the input always shows the effective path (with a 使用默认 button when a custom path is set); clearing the field or typing the default persists an empty value, so the config keeps falling back
- Wording pass on the 翻译 group: the mode options are now parallel descriptions of what happens after a selection (不自动翻译/悬浮球翻译/立即翻译) and the hotkey row reads 按快捷键翻译 (tooltip: press it after selecting to translate), aligned with the extension's 划词自动翻译 wording; each segment carries its own hover hint (off: "选中文字后不自动翻译；仍可按快捷键翻译", ball: "选中文字后，把鼠标移到悬浮球上悬停即可翻译", auto: "选中文字后立即翻译"), copied from the option `title` by `ui/segmented.js`
- Verified with a headless-Chrome harness against the real settings page plus a stubbed Tauri bridge (Linux 2-segment and Windows/macOS-style 3-segment layouts, About expand/collapse, right-aligned values, the model button invoking `open_model_location`, no removed elements, no JS errors); release build and tests green


## Settings Page: Apple-PM Restructure (2026-10-04)


- Landed the prioritised review: (1) 关于 is now its own view (`about-view`) reached from a 版本 nav row, so the settings page keeps a stable height; (2) 划词翻译 became a master switch with a 翻译方式 segmented control (悬浮球翻译/立即翻译) shown only where auto is supported, backed by the new `selection_method` key so the last method survives a switch-off; (3) a 登录时启动 row manages the autostart entry from the client itself (`autostart_enabled`/`set_autostart`: Linux desktop file, macOS LaunchAgent + launchctl, Windows Startup shortcut via PowerShell); (4) copy/status pass: 自动检查更新 in 关于, 浏览器扩展 with a 运行中 · address status, 本地模型 with a 正在使用/未找到 status (`model_exists` in the payload); (5) the tray gained a 划词翻译 submenu with checkmarks that applies the mode and emits `selection-mode`, so an open settings page stays in sync
- Backend: `apply_selection_mode` is shared by the settings command and the tray; new `save_selection_method`, `autostart_enabled`, `set_autostart` commands and payload fields; `libc` added for the macOS autostart uid
- Verified: 17 client tests (including a new Linux autostart toggle test), headless-Chrome checks of the settings/关于 flow for both Linux and Windows-style stubs (master switch, conditional method row, About status texts, back navigation, autostart invokes, no JS errors), release build and restart
- Review fixes on the same batch: autostart artifacts now escape paths per format (Desktop Entry quoting with a line-break guard, XML escaping, PowerShell single-quote doubling; covered by a unit test), `apply_selection_mode` persists `selection_method` and refreshes the tray checkmarks so every entry point stays in sync, the `selection-mode` listener updates from the event payload in every view (About and the waiting hint included), the extension status reports 将在下次启动时运行/已关闭（重启后停止）instead of claiming a live state, `set_autostart` runs on a worker thread, and the dead `.group-title-toggle`/`.selection-number` CSS was removed


## Windows Real-Machine Verification (2026-10-04)


- Closed the pending Windows real-machine checks: the redesigned desktop UI (card/settings/history) and the selection-translation flow (selection-following ball / 立即翻译) are verified on hardware
- macOS cannot be real-machine-verified (no Mac hardware) and stays CI-built plus cross-target compile-checked; its watcher ships as experimental. The X11-session manual check is still pending (the dev machine is GNOME Wayland)


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


Released: `v0.4.1` (2026-10-03, Linux copy/replace/read-aloud/one-click update + fullscreen tray fix); `v0.5.0` (2026-10-04) adds the extension 边写边译 flow, the shared Apple-style UI tokens and `OTSelect` dropdowns across extension and desktop, viewport-aware menu placement, and the single-sourced `shared/ui` copies.

Planned:

1. X11-session manual check of the selection-translation flow (the dev machine is GNOME Wayland)

2. macOS real-machine verification is not possible (no Mac hardware); code signing / notarization (budget decision)


Completed sprints:

1. Sprint 1.2: core refactor, engine trait, API layer, tests

2. Sprint 2: Ollama runtime, model evaluation (HY-MT default), prompt styles, service hardening

3. Sprint 3: desktop selection popup (Phase 0/1), service self-start, GTK UI, install script

4. Sprint 4: Firefox browser extension MVP

5. Consumer edition: desktop core extraction, Windows/macOS clients, in-process llama.cpp engine, first-run model download, release packaging


---

# Future Milestones


## Distribution

v0.5.0 released (2026-10-04) with the extension 边写边译 inline translation and the unified extension/desktop UI on shared sources. Windows real-machine verification passed 2026-10-04; remaining: X11-session check, code signing/notarization, and macOS verification (not possible without Mac hardware).


## Sprint 5

Meeting translation. Product work is parked, but the M0 measurement spike ran
on 2026-10-04 (Linux dev machine) and passed its latency gate; the design
reference is `docs/LIVE_SUBTITLE_TRANSLATION.md`.

- Capture: `pw-record --target @DEFAULT_AUDIO_SINK@` links to the default
  sink's monitor ports and captures playback (PipeWire 1.6.2; no `parec`/
  `pactl` installed). `pw-record -n <samples>` exits 1 after writing the
  requested samples — rc=1 with a complete file is success.
- ASR: SenseVoice int8 (`model.int8.onnx` + `tokens.txt` via `hf-mirror.com`
  with `HF_HUB_DISABLE_XET=1`; ModelScope API 404s for this mirror) decodes
  real speech at RTF ~0.022 on CPU (7.15 s clip in 161 ms; ~0.09 CPU-seconds
  per audio-second). Real-speech en/zh test clips are accurate (one word off
  on the en clip); espeak-ng synthetic speech is recognized poorly, so WER
  must be judged on real meeting audio.
- MT: the running desktop client's in-process llama.cpp engine
  (`hy-mt1.5-1.8b-q4_k_m.gguf`) gives 240–255 ms to first token and 490–680 ms
  per short request. CPU is the hot spot: ~3.3 CPU-seconds per ~100-character
  request.
- End to end (monitor capture → energy VAD → SenseVoice → local MT), real
  speech: 384 ms endpointing, 415–446 ms segment close → first translated
  token, 665–721 ms → done; ~0.83 s from end of speech to first translated
  token and ~1.1 s to a finished segment.
- 31-minute run (English lecture video → zh, 1860 s window): 494 segments,
  0 dropped, 0 MT failures, 1501 s of speech; end of speech → first translated
  token p50 823 ms / p95 990 ms / max 1192 ms; → full translation p50 1199 ms /
  p95 1658 ms / max 2172 ms; no drift over time (Q1 848 ms → Q4 804 ms). Spot
  checks showed good translation quality; observed failure modes are
  word-level MT glitches and proper-noun ASR errors, supporting the
  glossary-first plan. Caveat: clean video speech, not an interactive meeting.
- M0 acceptance is complete. The `asr_ms` field of that run's JSONL is invalid
  (time-base bug, fixed for future runs); ASR timing stands on the separate
  RTF measurement. No product code was written; spike scripts live in
  `/tmp/kilo` (`m0_run.py`, `analyze_run.py`).
- M1 (live caption overlay, 2026-10-05) landed: `core/asr` (`translator-asr`,
  official sherpa-onnx Rust crate, SenseVoice + Silero VAD, synchronous
  `SpeechEngine`), `src/caption.rs` in the Tauri client (`pw-record` →
  segments → caption events) and `ui/caption.*` (click-through, non-focusable,
  always-on-top, fades after 6 s of silence). Tray 实时字幕 toggles it; config
  keys are `caption_enabled`/`caption_language`/`caption_x`/`caption_y`/
  `asr_model_dir`.
- Build findings: the prebuilt **static** sherpa/onnxruntime archive aborts
  with `free(): invalid pointer` in `onnxruntime::GetPciBusId` once linked
  into the Tauri binary; switched to the **shared** libraries with an
  `-Wl,-rpath,$ORIGIN` runpath, and the deb now ships
  `libsherpa-onnx-c-api.so` / `libsherpa-onnx-cxx-api.so` / `libonnxruntime.so`
  next to the binary (`pipewire-bin` added to Depends). Static mode remains
  broken for this binary.
- Runtime findings: `set_ignore_cursor_events(true)` panics in tao when the GTK
  widget is not realized (apply it after `show()`); the new `caption` window
  label must be listed in `capabilities/default.json` or its JS `listen()`
  calls are rejected — this kept the overlay invisible until fixed.
  `TRANSLATOR_CAPTION_DEBUG=1` prints `CAPDBG` status/segment lines.
- M1 real-machine checks passed 2026-10-05 (caption renders, click-through, no
  focus steal, tray toggle); the ASR models live in
  `~/.local/share/open-translator/models/sense-voice/` and are not downloaded
  by the app yet (next task).
- M2 (translation line, 2026-10-05) landed: finished segments run through the
  embedded llama.cpp engine with a single-flight newest-wins worker and stream
  `caption-translation` deltas into the overlay's second line; the tray
  实时字幕 submenu switches 开启/双语/仅译文/仅原文 (`caption_layout`), and a
  simple glossary (`source=target` per line, max 50, `caption_glossary` or
  `glossary.txt`) is appended to every prompt style. `TranslationRequest`
  gained an optional `glossary` field (also accepted by `/translate` and
  `/translate/stream`).
- M2 measured (from ASR completion, English → Korean, auto source, one
  glossary term): first translated token 411–606 ms, full segment 1.16–2.49 s.
  Real-machine checks passed (translation line, layout switching,
  click-through and focus unchanged). Follow-ups: ASR model download (release
  blocker) and overlay placement UI.
- ASR model download (2026-10-05): `translator-core::models` gained
  `ModelFile` / `ASR_MODEL_FILES` / `download_model_file` (tries URLs in
  order, verifies SHA-256, skips valid files, resumes partials) with the
  SenseVoice files on `hf-mirror.com` and Silero VAD on the sherpa-onnx GitHub
  release plus a gh-proxy fallback. Caption start downloads missing files and
  reports `caption-status` state `downloading` with percentage; failures notify
  and switch the tray back off. Real-machine check: deleted
  `models/sense-voice`, enabled captions — 240 MB downloaded at ~7.7 MB/s,
  checksums verified, then captions and translations ran (first token
  442–453 ms). `auto_download = false` keeps manual placement.
- M3 (trust, 2026-10-05) landed: the caption overlay marks provisional vs
  final text (`···` while waiting, dimmed translation while streaming, solid
  after `done`); the tray 实时字幕 submenu gained 编辑术语表… (creates a
  template `glossary.txt` on first use and opens it; edits apply to the next
  segment) and 打开字幕记录 (opens `captions/`). Every finalized pair is
  appended to `~/.local/share/open-translator/captions/captions-<epoch>.txt`
  with relative `HH:MM:SS` timestamps (`format_timestamp` / `transcript_entry`,
  unit-tested).
- M3 real-machine checks passed (pending dots, provisional → final, glossary
  editor, transcript folder); the 30-minute experiential acceptance stays a
  daily-use check.
- Overlay position UI (2026-10-05): the tray 实时字幕 submenu gained 移动字幕条…
  (click-through off for 30 s or until the drag ends; dashed border + hint;
  pointer-capture drag clamped to the work area; persists `caption_x`/`caption_y`)
  and 重置字幕位置 (clears both, back to bottom-center). Two bugs found on real
  hardware: the overlay's `pointer-events: none` blocked every mouse event, and
  plain mouse events stopped at the window edge — fixed with an editing-mode
  `pointer-events: auto` plus `setPointerCapture`. Real-machine checks passed
  (drag, click-through restoration, position restored after restart, reset).
- 30-minute soak (2026-10-05, real video, ran 37 min): 432 segments, 432/432
  translated, 0 gaps > 60 s, flat density across 5-minute buckets, no crash;
  spot-check quality good with the expected local-model misses. The maintainer
  reported no subjective issues, so the soak is fully accepted.
- Release packaging verified 2026-10-05 with a `workflow_dispatch` run
  (`Release`, run 37263196039): Windows/macOS/browser/Linux all built; the deb
  carries the binary + `libsherpa-onnx-{c-api,cxx-api}.so` + `libonnxruntime.so`
  and depends on `pipewire-bin`. Tagging `v0.7.0` was deferred at the time; the
  tag was pushed later the same day (release run 37266548836, all four packages
  green, five assets attached, notes rewritten) and v0.7.0 is the current
  `releases/latest`.
- Windows live captions (W0–W3, 2026-10-05): see the dedicated section below.


## Windows Live Captions (2026-10-05)

- W0 spike (scratch crate in `%TEMP%`): proved on the Windows 11 dev machine
  that `AUDCLNT_STREAMFLAGS_LOOPBACK` + `AUTOCONVERTPCM | SRC_DEFAULT_QUALITY`
  on the default render endpoint delivers exactly 16 kHz mono f32 (spike
  captured SAPI speech, peak 0.31 / RMS 0.03). Findings that shaped the
  implementation: `wasapi` client types are `!Send`, so setup/capture/teardown
  must share one thread; idle endpoints deliver no packets (VAD gating is
  free); sherpa's Silero VAD hard-exits on any sample rate other than 16 kHz,
  so the capture layer always requests that format.
- W1 landed: `translator-asr` is now a dependency on Linux and Windows
  (`Cargo.toml` target-gated), plus `wasapi` 0.25 (MIT) on Windows. New
  `caption.rs` `mod windows` runs the whole WASAPI lifecycle on one blocking
  thread, requests 16 kHz mono f32 via autoconvert, falls back to the device
  mix format plus an in-repo box-filter/linear resampler (`FormatConverter`,
  unit-tested), rebuilds the session after a device switch (up to 5 consecutive
  failures) and feeds the same Silero VAD → SenseVoice → single-flight MT
  pipeline. Shared helpers (model check/download/load, transcription worker,
  failure handling) were factored out of the Linux module, which keeps its
  `pw-record` path behaviour unchanged.
- Tray 实时字幕 items are enabled on Windows (`cfg!(any(target_os = "linux",
  target_os = "windows"))`); the caption overlay reuses the existing
  transparent/click-through window and needed no changes.
- W3 packaging: the Windows release zip now ships
  `sherpa-onnx-c-api.dll`/`sherpa-onnx-cxx-api.dll`/`onnxruntime.dll`/
  `onnxruntime_providers_shared.dll` and `install.ps1` copies them next to the
  exe (one-click updates run the new installer, so they carry the DLLs too).
  CI gained an `asr-windows` job (`core/asr` tests on windows-latest) and the
  Windows Tauri job got the same sherpa cache workaround as Linux.
- Real-machine verification (Windows 11): captions on system audio, the
  translation line and layouts, overlay placement/drag all passed. One bug
  found: 编辑术语表… and 打开字幕记录 used `open_in_default_app`, a Linux-only
  helper that merely showed a notification elsewhere; they now reuse the
  working `open_path` (cmd start / open / xdg-open) and open correctly.
  `cargo test --release` on Windows: 17/17.
- Model download mirrors (2026-10-05): a proxy-off re-run showed the only
  unreachable source was the Silero VAD GitHub release (connect timeout, then
  ~50 s through gh-proxy); an hf-mirror copy of the exact k2-fsa export
  (byte-identical, SHA-256 verified) is now tried first in `ASR_MODEL_FILES`,
  then GitHub, then gh-proxy. Empty-cache re-run: 240 MB downloaded and
  captions `listening` in 74 s (the VAD mirror took under a second).
- v0.8.0 released 2026-10-05 (tag `cb4d18d`, release run 37273967262): first
  real run of the DLL packaging — the Windows package job took 5m13s, the zip
  (15.4 MB) carries the four runtime DLLs, and five assets landed; v0.7.0
  users can one-click update because the updater runs the new installer.


## Multimodal Roadmap (2026-10-06)

Two decisions were locked with the maintainer after the multimodal
architecture discussion:

- **D1 — first image consumer: browser-extension screenshot translation.**
  `tabs.captureVisibleTab` from a user gesture → the existing extension port →
  a new local `POST /translate/image` endpoint → OCR (text + boxes) + the
  existing MT → a DOM overlay in the page. OCR first, VLM later; the desktop
  region-screenshot → card flow is the second image consumer, and system-level
  UI translation is deferred (Windows/macOS first; Wayland cannot position the
  overlay). Design reference: `docs/IMAGE_TRANSLATION.md`; the OCR selection
  spike is the next step.
- **D2 — second pipeline consumer: microphone mode.** `caption_source =
  sink | mic`, one capture session at a time, same VAD/ASR/MT/overlay stack,
  Linux first. It drives the shared audio-pipeline extraction once both
  capture sources work. Reference: `docs/LIVE_SUBTITLE_TRANSLATION.md` §6.8.

Phase 1a landed the same day: the duplicated single-flight latest-wins
schedulers — the card/selection chain in `main.rs` (`translating` + `queued`,
release/re-acquire) and the caption translation line in `caption.rs`
(take/store race loop) — are unified in
`translator-core::latest_wins::LatestWins` (one mutex;
`submit`/`finish`/`clear_queued`; unit tests including concurrent
submit/finish). Behaviour is unchanged: translator-core 75 tests / client 20
tests green.

OCR spike (2026-10-06, M0): selected `rapidocr-core` + `ort` `load-dynamic`
reusing the sherpa-shipped `libonnxruntime.so` 1.28.2 — the build needs no
OpenSSL and downloads no ONNX Runtime, so on Linux the deb's existing runtime
is the only one. `ocrs` was eliminated (Latin-only recognition alphabet, no
zh model); `ort` defaults were rejected (second runtime plus an OpenSSL build
dependency). On a Chrome-rendered 1280×800 zh/en fixture (41 detected lines)
PP-OCRv6 tiny measured 515 ms at 1 thread, **240 ms at 4 threads** (8t 274 ms;
det 108 / rec 132 ms, models ~6.2 MB), versus 1.39 s for PP-OCRv5 ch mobile
and 2.07 s for PP-OCRv6 small with no quality gain. Ground-truth scoring
(41 blocks, NFKC/whitespace-normalized): 41/41 matched, mean similarity
1.0000, recall@0.95 = 1.000 at both DPR 1 and DPR 2 (DPR 2 runs 596 ms at
4 threads). Windows/macOS ship no ORT (ASR is Linux-only), so their OCR
runtime bundle is the remaining packaging item, alongside a real-webpage
fixture. Spike lives in `/tmp/kilo/ocr-spike` (with a vendored
`rapidocr-core` feature patch and `score.py`).

M1 plumbing (2026-10-06): `core/ocr` (`translator-ocr`) added with the
engine-agnostic `OcrEngine` surface (`OcrBlock { text, score, quad }`, no
adapter yet); `translator-service` gained `POST /translate/image` (base64 or
data-URL PNG/JPEG → injected OCR provider → one joined text-engine request
with a per-block fallback → `{image:{width,height}, blocks}`), an explicit
`ocr_unavailable` 501 without a provider, image budgets (32 MiB base64 /
16 MP / `max_chars`) and no image logging. 10 integration tests cover the
endpoint (translation, data URLs, line-mismatch fallback, 400/501 paths); a
`core/ocr` CI job was added. The RapidOCR adapter, `translator-core::models`
download entries and the desktop wiring wait on the Windows/macOS ORT bundle
decision.

OCR adapter (2026-10-07, A2+B2): `vendor/rapidocr-core` (0.2.2, Apache-2.0)
vendored with one manifest change — `ort` builds with `load-dynamic`
(std/ndarray/api-28), so no build-time download, no linking and no OpenSSL;
`RapidOcrEngine` in `core/ocr` wraps it (`load` downloads missing ModelScope
assets, `load_offline` fails instead; PP-OCRv6-tiny default; 4 intra-op
threads) and loads the host-provided runtime (Linux: the sherpa-shipped
`libonnxruntime.so`). Re-verified end-to-end on the M0 fixture through the
adapter: 41 blocks, 273 ms at 1280×800. Windows/macOS installers will carry
a pinned ORT dylib (B2); model download and desktop wiring are next.

Desktop OCR wiring (2026-10-07): the client's extension server
(`serve_extension`) now loads a `RapidOcrEngine` when it starts, from
`ocr_model_dir` (new setting; default `models/ocr/` via
`translator-core::paths::default_ocr_model_dir`), downloading PP-OCRv6-tiny
from ModelScope on first use and injecting it into `AppState::with_ocr`; a
load failure logs and leaves `/translate/image` at 501 without blocking the
text path. `core/ocr` auto-detects a bundled runtime next to the executable
(`libonnxruntime.so`/`onnxruntime.dll`/`libonnxruntime.dylib`) through
`ort::init_from`, which is also the B2 installer hook on Windows/macOS.
Tests: translator-core 76, translator-ocr 7, client 20 green.

B2 runtime bundle (2026-10-07): the Windows zip and macOS dmg now carry a
pinned ONNX Runtime 1.28.2 (`onnxruntime.dll` / `libonnxruntime.dylib`,
SHA-256 verified in `release.yml` against the official GitHub asset digests;
1.28.2 ships an arm64 macOS build only, matching the arm64 runner artifact).
`packaging/windows/install.ps1` copies the dll next to the installed exe, the
dmg script takes `ORT_DYLIB`, and both dev install scripts fetch the runtime
on first run (cached). The deb still reuses the sherpa-shipped
`libonnxruntime.so`.

Extension screenshot UX (2026-10-07, M2 — superseded the same day, see below): the 截图翻译页面 context menu item and
the `screenshot-translate` command (`Alt+Shift+S`) capture the visible tab
(`activeTab` + `tabs.captureVisibleTab`), POST `/translate/image` and render
the blocks through a click-through DOM overlay in the content script
(`data-opentranslator="shot"`; toolbar 原文/译文 · 复制译文 · 关闭; scroll /
resize / Esc dismiss; errors show a toast; no content script → `result.html`
fallback). Blocks are positioned from their quads divided by
`imageWidth / innerWidth`, covering DPR and page zoom. `web-ext lint` is clean
and the Chrome e2e grew five checks against a mock `/translate/image`
(fetch → deliver → overlay render/toggle/close, error toast); the
`captureVisibleTab` gesture itself stays a manual check.

Screenshot UX revision (2026-10-07): the first manual check in Chrome killed
the whole-page screenshot overlay — translations stacked on the page's own
selectable text, which is exactly where a DOM translation belongs; 截图翻译页面
and the `Alt+Shift+S` command were removed the same day. The extension now
ships 翻译此图片 (image context): the content script supplies the image's
visible rect, crops the tab capture on a canvas (DPR/zoom-aware, data URLs
stay untainted) and the background POSTs `/translate/image`; the result is a
panel next to the image (译文在前/原文小字/复制译文/关闭), errors toast, and
pages without a content script fall back to `result.html`. Chrome e2e covers
the crop math, the panel and the error toast; `web-ext lint` stays clean. The
pivot is recorded in `docs/IMAGE_TRANSLATION.md` §5.6; the web reading
translation evaluation (C) lives in `docs/READING_TRANSLATION.md`; B (desktop
region screenshot → card) is next.

Image viewer revision (2026-10-07): the flat panel beside the image was
replaced the same day by a modal viewer that draws each translation over its
original position (camera-translation / Lens metaphor) — the panel lost the
spatial mapping that makes image translation intuitive. Boxes are
scale-mapped from the OCR quads; hovering shows the original in the footer,
clicking copies a block, the toolbar toggles 原文/译文 and 列表 (bilingual
pairs), copies all translations, backdrop/Esc close; errors toast and a
missing crop falls back to list-only. The page's own image is untouched and
the viewer works on the cropped capture (data URL), so no extra permissions
are needed. Chrome e2e covers the box overlay, the 原文 toggle, list mode and
close; `web-ext lint` stays 0/0/0.

Desktop region screenshot (2026-10-07, B): the Linux tray 截图翻译 item and a
new `screenshot_hotkey` (default Ctrl+Alt+S, config-only) capture a region
through the XDG portal (`ashpd`, native picker; Wayland and X11) and run the
in-process pipeline: `translator-service` gained `src/image.rs`
(`translate_image_bytes`, now shared with `POST /translate/image`), the
client caches a lazily loaded RapidOCR provider in `AppState` and reuses the
card (recognized lines as 原文, joined translations, history recorded). The
portal file is removed after reading; the tray item is disabled on
Windows/macOS. Non-interactive portal plumbing verified on the dev machine
(DBus → file URI → read); the interactive flow needs a human region pick.

Screenshot presentation revision 2 (2026-10-07): the card-based first cut was
replaced after a first-principles review — selecting a region is deixis, so
the answer stays at the region. `screenshot.rs` now captures through the
portal without a dialog, shows a full-screen selector (`ui/select.*`,
`shot-select`) whose drag is mapped to capture pixels (geometry unit-tested:
single/HiDPI/multi-monitor slices, CSS→pixel mapping, clamping), and opens a
Lens-style viewer window (`ui/shot.*`, `shot`) anchored at the region with
hover-original, click-copy, 原文/译文, bilingual 列表, 复制, 刷新 and close;
blur/Esc dismiss. The region is stored, so the hotkey (while the viewer is
visible) or 刷新 replays it without selecting again; with the viewer hidden a
new selection starts. The card no longer carries screenshot results; history
still records them. Client 26 tests green; 0.8.3 built, installed and
running on the dev machine.

Screenshot selection revision 3 (2026-10-07): the frozen, dimmed selector was
rejected in the first manual check ("screen froze and darkened — cannot pick
a region"). The selector is now a transparent click layer over the **live**
screen: only the dragged rectangle highlights (accent border + light fill),
no dim and no frozen capture; it hides, the compositor settles 200 ms, and
then the portal capture follows. This also removed the capture→selector image
channel entirely (no data-URL event, no black-screen failure mode). 0.8.4
built, installed and running; manual drag pass pending.

Screenshot viewer fixes (2026-10-07): the first working viewer exposed two
issues. The window kept its default 800×600 because tao pins min/max size
hints with `.resizable(false)`, so the later `set_size` was ignored and the
image stretched ~3.4× with most translation boxes below the fold — fixed by
dropping `resizable(false)`, a 64×64 initial size, and no-stretch image CSS
(`width:auto; max-width:100%`), plus `SHOTDBG` sequence logs. The viewer is
now movable (toolbar/footer as a `data-tauri-drag-region` handle; `core:window:
allow-start-dragging` was already granted) and a moved viewer keeps its
position on refresh. 0.8.8 built, installed and running.

Screenshot mapping fix (2026-10-07): the work-area constraint was the real
cause of the "long selection loses text" reports. GNOME kept the selector
inside the work area (67 px dock + 32 px top bar), so the window actually sat
at (67,32) with 1853×1048 while the mapping assumed it covered the whole
1920×1080 monitor: crops were ~52×27 px off, growing toward the right — wide
selections diverged the most. `map_selection` now takes the selector window's
actual physical position and scale factor (`outer_position`), and coordinates
map 1:1 through the device scale without the viewport-stretch factor. Unit
tests cover the work-area offset and HiDPI. 0.8.11 built, installed and
running; a debug build (`TRANSLATOR_SHOT_DEBUG=1`) dumps the failing crop and
full capture to `/tmp/open-translator-shot-{crop,full}.png` for offline
analysis (private content; off by default).

Windows desktop screenshot translation (2026-10-07): the region-screenshot
flow now builds and runs on Windows. `screenshot.rs` is compiled for
Linux and Windows; `capture_screen(target)` is platform-split — Linux keeps
the XDG portal, Windows uses `xcap` 0.9 with the `wgc` feature (Windows
Graphics Capture) and captures only the monitor holding the selector in
physical pixels, so the slice math takes the single-monitor branch and a
mixed-DPI desktop needs no stitching. The selector is placed and sized with
`PhysicalPosition`/`PhysicalSize` (Tauri reports monitor rects in physical
pixels), avoiding a wrong logical conversion on mixed-DPI setups, and the
capture thread initializes a COM apartment (`CoInitializeEx`, best-effort)
for WinRT. Debug dumps moved from `/tmp` to `std::env::temp_dir()`. The tray
截图翻译 item, the `screenshot_hotkey` registration and the `shot_*` commands
are enabled for Windows; macOS stays disabled. Client
tests 23/23 green on Windows (incl. the geometry suite); real-machine manual
check pending.

Windows viewer drag fix (2026-10-07): the first real-machine pass found the
viewer could not be moved. Reproduced with synthetic input: `start_dragging`
(from the injected `data-tauri-drag-region` handler and from an explicit
`getCurrentWindow().startDragging()`) resolves but the OS move loop never
engages for this always-on-top tool window, and posting `WM_NCLBUTTONDOWN`
from Rust does not help either. The viewer now drags itself from the page
like the docked ball: mousedown records the pointer, mousemove invokes
`move_viewer_by` with the screen delta (Tauri `set_position`, scaled), and
Windows removes the drag-region attributes on load; Linux keeps the
compositor move. `setPosition` straight from JS was rejected by the ACL
(`core:default` has no `allow-set-position`), which the app command avoids.
Blur-dismiss stays Linux-only — Windows reports focus changes during normal
use. Verified by the user on the Windows real machine: the viewer moves.

Windows screenshot review follow-ups (2026-10-07): an unpushed-commit review
found four issues, all fixed. `move_viewer_by` now carries the fractional
physical-pixel remainder per axis, so slow drags at 125%/150%/175% scaling no
longer creep ahead of the pointer; the viewer is positioned and sized
physically on Windows like the selector, fixing mixed-DPI placement; the
Windows capture keeps the raw RGBA frame (`Capture::Rgba`) and crops before
encoding, dropping the full-monitor PNG encode/decode round trip (the portal
path keeps `Capture::Encoded`); and the debug dumps are written via
remove-then-`create_new`, so a pre-planted symlink or foreign file is not
followed (debug-only, `TRANSLATOR_SHOT_DEBUG=1`). Client tests 23/23 green;
synthetic end-to-end run on the dev machine: selection, OCR, translation,
viewer and the toolbar drag all pass.
- v0.9.0 released 2026-10-07 (tag `bc40b95`, release run 37593934905): the
  first release carrying the image-translation/OCR batch — the browser
  extension Lens viewer, the shared image pipeline, desktop region screenshot
  translation on Linux and Windows, and the pinned ONNX Runtime 1.28.2 in the
  Windows/macOS packages. Five assets landed (Windows zip 16.8 MB, Linux deb
  17.5 MB, macOS dmg 21.8 MB, extension zips); the Windows package job
  re-verified the DLL copy order from the v0.8.0 fix.


## Sprint 6

Mobile client.
