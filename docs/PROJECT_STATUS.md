# OpenTranslator Project Status


## 1. Project Overview

Project Name:

OpenTranslator


Vision:

Build an open-source, local AI translation infrastructure.

The goal is to provide universal translation capability across desktop applications, browsers, meetings and mobile devices.

The system should be:

- Local-first
- Privacy-preserving
- Free and open-source
- Extensible


---

## 2. Product Direction

MVP Goal:

Desktop selection translation.


Primary user scenario:

A user selects English text anywhere on the desktop and immediately receives AI-powered translation.


Example:

Input:

kernel panic


Output:

内核崩溃

Explanation:

Linux terminology describing a fatal kernel error state.


---

## 3. Development Environment


Operating System:

Ubuntu 26.04.1 LTS


Hardware:

CPU:
Intel Core i5-14400


Memory:

32GB RAM


GPU:

Intel UHD Graphics 730


Architecture:

x86_64


Model Runtime:

- In-process llama.cpp via `core/inference` (build needs cmake + clang/libclang; cmake at `~/.local/opt/cmake`)
- Ollama v0.35.0 (user-space install at `~/.local/opt/ollama`) for the service/power-user path


Current model:

hy-mt1.5-1.8b (recommended default; translategemma:4b quality option)


---

## 4. Technology Stack


Core Service:

Rust


Web Framework:

Axum


API:

REST API


Planned Database:

SQLite


Model Runtime:

- Consumer edition: in-process llama.cpp (`llama-cpp-2`), no external service
- Service/power users: the same in-process engine (`TRANSLATOR_ENGINE=llama-cpp`) or Ollama


Current model:

hy-mt1.5-1.8b (recommended default); translategemma:4b / qwen2.5:7b alternatives


Candidate models:

- HY-MT
- NLLB
- Qwen Translation Models


---

## 5. Repository Structure


open-translator/

├── core/
│   ├── translator-service/   (HTTP service + engines)
│   └── inference/            (llama.cpp engine, no HTTP)
├── desktop/
│   ├── translator-core/          (platform-agnostic desktop lib)
│   ├── translator-popup-tauri/   (Tauri v2 client; shipped on Windows/macOS/Linux)
│   └── install-windows.ps1 / install-macos.sh
├── browser/extension/        (Firefox MV2 + Chrome MV3)
├── packaging/                (installers used by the release workflow)
├── models/                   (HY-MT → Ollama import script)
├── docs/
└── tests/                    (placeholder; Rust tests live in the crates)


---

## 6. Completed Work


### Environment

Completed:

- Ubuntu development environment
- Git installation
- Rust toolchain installation


### Core Service

Completed:

- Rust project created
- Axum HTTP server
- Health endpoint
- Translation endpoint (mock, Ollama and in-process llama.cpp engines)
- Prompt styles/sampling, keep-alive, request length cap, `source=auto`


Current APIs:


GET:

/health


POST:

/translate


Example:

Input:

hello world


Current output:

[Mock Translation] hello world


---

## 7. Git Status


Current branch:

main


Latest commit:

v0.5.0 tagged at c948d99 (test: stabilize the dropdown placement e2e checks); release run 37138919115 published five assets


---

## 8. Current Development Phase


Phase:

Consumer edition packaging (Phase C)


Goal:

Ordinary users on Windows/macOS can download, install and use it without technical setup.


Status:

`v0.3.0` published 2026-10-02 with the Tauri client on Windows/macOS/Linux (https://github.com/zhangking4u/open-translator/releases/tag/v0.3.0); `v0.3.1` followed 2026-10-03; `v0.4.0` (2026-10-03) shipped the desktop redesign (card/settings/history) and the browser extension overhaul (streaming bubble, toolbar popup) plus the extension zips as release assets; `v0.4.1` (2026-10-03) closes the Linux gaps (copy/replace/read-aloud/one-click deb update) and fixes the card surfacing above fullscreen windows from the tray; `v0.5.0` (2026-10-04) adds the browser extension 边写边译 inline translation (caret bubble, `Tab` commit, target-language chip, `Alt+Shift+L` cycle-target) and unifies the extension/desktop UI on shared Apple-style tokens and `OTSelect` dropdowns single-sourced under `shared/ui`. The desktop 划词翻译 feature landed on Linux 2026-10-04 (floating-ball and direct modes, settings group, card gear button); Windows/macOS watchers are staged in `docs/SELECTION_TRANSLATION.md`.


---

## 9. Next Steps


1. Real-machine visual pass of the redesigned desktop UI (card/settings/history, v0.4.0–v0.5.0) on Windows/macOS

2. Code signing / notarization (budget decision); macOS real-machine verification deferred (no Mac hardware; dmg is arm64-only); AppImage deferred

3. Selection translation follow-ups: manual X11 session check (the dev machine is GNOME Wayland, where only the direct fallback runs), then the Windows watcher (mouse hook + UI Automation) and the macOS watcher (NSEvent + AX) phases from `docs/SELECTION_TRANSLATION.md`

4. Release history in the sections below: v0.2.x desktop fixes, v0.3.0 (Tauri client on three platforms), v0.3.1 (update UI moved to settings), v0.4.0 (desktop + extension redesign, extension zips attached to releases), v0.4.1 (Linux gap closure + fullscreen tray fix), v0.5.0 (inline translation + unified UI, shared UI sources)


---

## 10. Development Principles


Follow:

- Clean Architecture
- Domain Driven Design concepts
- API-first design
- Modular architecture
- Open-source engineering practices


Avoid:

- Premature complexity
- Tight coupling with a specific model
- Platform-specific implementation in core layer


---

## Sprint 1.2 Progress


Completed:

- Created translation domain model
- Added TranslationEngine trait (async, dyn-compatible)
- Added MockTranslationEngine
- Integrated API with engine layer
- Split API layer into `src/api`
- Added library target (`src/lib.rs`) with unit and integration tests


Current runtime flow:


HTTP API

↓

Translation Domain

↓

TranslationEngine

↓

MockEngine

↓

TranslationResult


Status:

Translation architecture abstraction completed; API layer split and test foundation in place.


## Sprint 2 Preparation Progress


Completed:

- TranslationEngine now returns `Result` with a shared `TranslationError`
- API maps errors to JSON `{"error":{"kind","message"}}` (400/500/502/504)
- Env-based config: `TRANSLATOR_BIND_ADDR`, `TRANSLATOR_ENGINE`, `TRANSLATOR_TIMEOUT_MS`; engine factory (`engine::build`)
- Timeout guard (`TimeoutEngine`) wrapping every engine; expiry maps to 504
- Language tag normalization (`domain::language`) and MT prompt builder (`domain::prompt`)
- Ollama engine adapter and model config (`TRANSLATOR_MODEL_URL`, `TRANSLATOR_MODEL`)
- Live end-to-end translation via local Ollama (qwen2.5:7b, CPU)
- Model quality evaluation (translategemma:4b + official prompt recommended; qwen2.5:7b fallback)
- HY-MT evaluation: hy-mt1.5-1.8b / hy-mt2-1.8b imported from ModelScope GGUFs (0.1–0.5s warm, >4× faster than 4B/7B models); hy-mt1.5-1.8b recommended default
- Per-model prompt styles and sampling (`TRANSLATOR_PROMPT_STYLE=generic|translategemma|hymt`); HY-MT import script in `models/`
- Service hardening: tracing logs (`RUST_LOG`), startup warmup (`TRANSLATOR_WARMUP`), `/health` reports engine and model
- Tests for invalid input, engine failure, timeout, config parsing, prompt building and Ollama adapter


Pending:

- Desktop client polish (hotkey setup, popup UX, service startup)


## Sprint 3 Progress (Phase 0)


Completed:

- `desktop/translator-popup`: reads the Wayland primary selection (or clipboard), calls `POST /translate` and shows a zenity popup
- `--stdin` / `--print` keep it scriptable; end-to-end verified against the Ollama engine (kernel panic → 内核崩溃)
- GNOME constraint documented: `wl-clipboard-rs` unusable (no data-control protocol); reading goes through `wl-paste`


Pending:

- Phase 1 complete; portal hotkey deferred (install script covers shortcut setup)


Status:

Desktop Phase 1 complete: install script, GTK4 popup, service self-start, close-and-reopen on repeated hotkeys.


## Sprint 4 Progress (Browser Extension)


Completed:

- Firefox MV2 extension in `browser/extension/`: context menu + `Alt+Shift+T`, in-page bubble (loading/error/copy/Esc), options page (service URL, language pair)
- Bubble target-language switch and optional auto-translate; extension version 0.1.1
- Background fetch to the local service through the host permission (no CORS change)
- Chrome MV3 variant (`manifest.chrome.json`) and `browser/build.sh` producing `browser/dist/{firefox,chrome}`
- Packaging: `build.sh --zip` plus `browser/sign.sh` (`web-ext`, AMO unlisted); Firefox manifest lint-clean

Pending:

- (none)


Status:

Firefox extension MVP implemented and verified manually (context menu and `Alt+Shift+T`); the invalid host permission (port in match pattern) was fixed during testing. Chrome MV3 verified end-to-end on Chrome 154 and Edge 154 (service worker translation + content-script bubble). Version 0.1.0 signed via AMO unlisted (auto-approved) and installed permanently in Firefox.


## Consolidation (2026-09-30)


- Root `README.md` added (quick start, configuration reference, FAQ)
- Core: `TRANSLATOR_KEEP_ALIVE` (default `30m`) keeps the model loaded between uses (verified: `UNTIL 29 minutes from now`)
- Core: `TRANSLATOR_MAX_CHARS` (default 1500) rejects over-limit text instantly; `source=auto` supported
- Desktop: `~/.config/open-translator/config` for `service_url`/`source`/`target`, with CLI > file > environment > defaults precedence
- Desktop: in-window target-language dropdown (re-translates and persists the choice)
- Desktop: single-window interaction — every trigger reuses the window and updates content in place (no close/reopen)


## Release Readiness (2026-09-30)


- MIT `LICENSE` added
- GitHub Actions CI (`.github/workflows/ci.yml`): core/desktop tests, browser static checks + `web-ext lint`, Chrome e2e
- Repeatable browser e2e harness (`browser/test.sh`), 7/7 checks pass locally
- CI e2e cleanup bug fixed (`rm -rf` racing the browser shutdown overrode the exit status); all jobs green on the second run
- `actions/checkout` / `actions/setup-node` bumped to v5
- Windows: core service tested on `windows-latest` in CI; desktop remains Linux/GNOME-only; browser extension is cross-platform
- Desktop Phase 1 for Windows/macOS: platform-agnostic `desktop/translator-core` extracted (args/config/translate/service auto-start/per-OS paths); CI matrix tests it on ubuntu, windows and macos
- Desktop Phase 2: Windows client MVP (`desktop/translator-popup-desktop`, eframe + `Ctrl+Alt+T` + Ctrl+C capture) with `desktop/install-windows.ps1`; CI builds it on windows-latest
- Desktop Phase 3: the same client supports macOS (Cmd+C capture with Accessibility hint, `desktop/install-macos.sh` bundle + LaunchAgent); CI builds it on macos-latest
- Desktop polish: tray/menu-bar icon (显示窗口/立即翻译/退出) and a configurable hotkey (`hotkey` in the config file or `TRANSLATOR_HOTKEY`)
- In-process llama.cpp spike passed (`llama-cpp-2` + HY-MT GGUF: correct translations, ~29 tok/s, ~1.8 GB RAM, no external service) — the chosen direction for the consumer edition
- Phase B1: `core/inference` crate (`translator-inference`) with `InferenceEngine` (actor worker, sampling/stop strings, env-gated real-model test); CI job on ubuntu
- Phase B2: `EngineKind::LlamaCpp` in the service (`TRANSLATOR_MODEL_PATH`, `TRANSLATOR_N_CTX`); live-verified (kernel panic → 内核崩溃 in 0.26s, no Ollama)
- Phase B3: desktop client embeds the engine (no Ollama, no service process) and serves the HTTP API in-process for the browser extension; verified with all services stopped
- Phase C1: first-run model download (ModelScope + resume + SHA-256 + progress) in the desktop client; live-verified end to end (1.13 GB, then `/translate` 0.28s); normal startup stays hidden, and Esc/X quit where no tray/hotkey exists
- Phase C2: release workflow produces a Windows zip installer and a macOS dmg (tag `v*` or manual dispatch); README has an ordinary-user download section; code signing/notarization still pending


## v0.1.0 Release (2026-10-01)


- Real-machine fixes included in the tag: Windows defaults `WGPU_BACKEND=dx12` (Intel Vulkan driver crash `igvk64.dll`) and installs a system CJK font fallback (tofu boxes); capture waits for modifier release and only translates when the clipboard actually changed; the desktop app is single-instance and the extension HTTP server starts once (`AddrInUse` shows an informational status)
- `v0.1.0` published with `OpenTranslator-windows-x64.zip` and `OpenTranslator-macos-arm64.dmg`; unsigned, so SmartScreen/Gatekeeper warnings are documented in the release notes
- Windows real-machine re-verification passed (selection capture without manual copy, CJK font rendering, single-instance box); macOS verification deferred (no hardware), code signing/notarization still pending


## Linux deb Packaging (2026-10-01)


- GTK client now defaults to the in-process llama.cpp service with first-run model download (ModelScope, resume, SHA-256) and progress shown in the window; `TRANSLATOR_ENGINE=ollama` keeps the legacy Ollama path
- `packaging/linux/make-deb.sh` builds `OpenTranslator-linux-x64.deb`: `translator-popup` + `translator-service` in `/usr/lib/open-translator` (sibling discovery), `/usr/bin/translator-popup` symlink, `.desktop` entry, hicolor icon, README; depends on `libgtk-4-1`, `wl-clipboard`, `libgomp1`
- `open-translator-setup` registers the GNOME shortcut at user level (no root-time gsettings); the popup auto-registers on first launch via `--if-missing` (never overwrites a custom binding), and `postinst` still points users to the manual command
- Deb requires `libc6 (>= 2.39)` and GTK4 ≥ 4.10 (built on Ubuntu 24.04; binary symbol versions GLIBC_2.38/2.39)
- Release workflow gained a Linux job that attaches the deb to the release; local verification: extracted deb auto-started the sibling service (`llama-cpp`) and translated "kernel panic" → 内核崩溃; missing-model and setup-script paths checked
- Verified: `workflow_dispatch` release run built all three platforms; the CI deb (version `0.0.0+985830c`) downloaded the model (1.13 GB, 47 s, SHA-256 match) on first run in an isolated HOME and translated "kernel panic" → 内核崩溃 (warm run 0.36 s)
- Shipped in the re-released v0.1.0 (2026-10-01); pending: `apt install` on a real user account (needs admin rights); AppImage deferred


## Startup Update Check (2026-10-01)


- `translator-core::update` checks GitHub `releases/latest` once at startup (`check_updates`, default on; `TRANSLATOR_CHECK_UPDATES=false` disables) and compares the embedded release version with the latest tag
- GTK popup appends a "有新版本 vX.Y.Z，点击查看" link; the Windows/macOS client shows a window banner with a download button and enables a tray item that opens the release page
- Release builds embed the tag via `OPEN_TRANSLATOR_VERSION`; shipped in the re-released v0.1.0 (2026-10-01), so existing v0.1.0 installs need one manual update before the hint can reach them
- Pending: real-machine check of the tray item


## v0.1.0 Re-Release (2026-10-01)


- The original v0.1.0 release and tags were deleted and re-tagged at `fd18fc2` so the release includes the Linux deb and the startup update check; release run 36813116437 passed all package jobs
- Assets: `OpenTranslator-windows-x64.zip`, `OpenTranslator-macos-arm64.dmg`, `OpenTranslator-linux-x64.deb`; notes cover Linux install, shortcut auto-registration, glibc/GTK baseline and the update check


## Desktop UI Polish (2026-10-01)


- `translator-core::languages` now owns the shared language list and `label()` lookup; both clients dropped their duplicated tables
- Windows/macOS client: `ModelState` + `TranslationState` state machines replace the string status/error pair; selections made while the model is downloading are queued and translated automatically once it is ready
- Windows/macOS client: frameless transparent rounded-card window (custom draggable header, system light/dark theme), dimmed source card + prominent translation card, model-download progress bar, error card with retry, 1.5 s copy feedback, dismissible update/notice banners, Ctrl+Enter re-translate and Ctrl+Shift+C copy
- Linux/GNOME popup: the same state machine drives a CSS card layout with spinner, download progress bar, error card + retry, 2 s copy feedback and the same shortcuts
- Both clients adapt the window height to the translation (cap: 70% of the monitor, then scroll), so long results no longer stay hidden behind a fixed scroll area and short results do not leave empty space
- Pending: visual pass on Windows/macOS real hardware (the egui UI is compile-verified only on the Linux dev box)


## Flexible Source Language (2026-10-01)


- Source language is no longer fixed to English: `translator-core::detect` (whatlang trigrams, confidence ≥ 0.5) resolves the default `auto` source to a supported tag when confident and falls back to the service's source-agnostic prompt otherwise; the default HY-MT prompt ignores the source language anyway
- Both desktop clients now have a source-language dropdown (自动检测 + zh/en/ja/ko/fr/de/es/ru) with a detection hint, persisted to the config file like `target`; the browser extension defaults to `source = auto`
- Pending: real-machine visual pass of the dropdown/hint on Windows/macOS


## GTK Client Quick Wins (2026-10-01)


- `translator-popup`: the status bar shows character count and elapsed time, the source text is selectable, and an empty primary selection falls back to the clipboard
- user-facing errors are Chinese with actionable hints (wl-clipboard install, service auto-start/log paths, model download/auto-start failures)
- Language pair swap (previous translation becomes the new source) and Ctrl+1/2/3 recent targets are now implemented in the GTK popup as well; `translator-core::languages::recent_target_list` is shared by both desktop clients
- Streaming output is implemented end to end: `POST /translate/stream` (SSE) on the service, `translate::translate_stream` in translator-core and incremental rendering in the GTK popup
- An in-window 剪贴板 toggle switches between the primary selection and the clipboard (persisted as `clipboard`)
- Next GTK candidate: optional model pre-warm autostart


## v0.2.0 Release (2026-10-01)


- Tag `v0.2.0` at `dcebea8`; release run 36859033510 passed all package jobs (Linux 5m36s, macOS 9m10s, Windows) and published three assets: `OpenTranslator-windows-x64.zip`, `OpenTranslator-macos-arm64.dmg` (bundle version now injected from the tag), `OpenTranslator-linux-x64.deb`
- Highlights since v0.1.0: source-language auto detection + selector, SSE streaming (`POST /translate/stream`), GTK ⇄ swap + Ctrl+1/2/3 recent targets + clipboard toggle + Chinese errors, Windows clipboard restore / near-cursor placement / 替换原文, Thai support, per-script font fallback
- Release notes cover install steps, first-run model download and the unsigned-build caveats


## Silent Autostart Fix (2026-10-01, after v0.2.0)


- The Windows/macOS client no longer shows its window at launch: first-run download and model errors used to force it visible, so the window sat on screen after login; progress/errors now live in the tray tooltip and the window appears only on hotkey/tray/`--stdin`
- Follow-up to v0.2.0 — installed users need the next release to pick up the fix
- Shipped in v0.2.1 (`26f292f`, release run 36862981211, three assets)
- Incomplete in v0.2.1: eframe 0.36 force-shows the window after the first painted frame (`EpiIntegration::post_rendering`), so `with_visible(false)` alone did not keep it hidden; post-v0.2.1 `PopupApp::new` queues `ViewportCommand::Visible(false)` for silent startup, live-verified on Windows (patched build stays `IsWindowVisible=false`) — shipped in v0.2.2


## v0.2.2 Release (2026-10-02)


- Tag `v0.2.2` at `8cb4640`; release run 36943753792 passed the Linux, macOS and Windows package jobs and published the three assets with the usual release notes (the first publish, run 36941897001, carried only auto-generated notes, so the tag was re-pushed)
- The Windows installer now upgrades in place while the old version runs: `packaging/windows/install.ps1` stops the running instance before replacing the exe and retries the copy, `desktop/install-windows.ps1` waits for the binary to unlock before relinking, and the packaging README documents re-running `install.ps1` as the upgrade path
- Installed v0.2.1 users get the real silent-autostart behavior with this release


## Window on Manual Launch, Silent Login (post-v0.2.2)


- `--autostart` (installers' login entry) starts silently in the tray; launching the exe manually shows the window so first-run download progress and model errors are visible
- Windows installers restart the app in the tray after an in-place upgrade unless `-NoStart` is given


## System Notifications (post-v0.2.2)


- While the window is hidden, model download completion, model load/download failures and hotkey registration failures raise a system notification (`src/notify.rs`: WinRT toast on Windows, `mac-notification-sys` on macOS; no-op elsewhere)
- Notifications are suppressed when the window is visible; verified live on Windows (toast on hidden failures, none on manual launch)


## One-Click Update (post-v0.2.2)


- Release assets are parsed (`ReleaseAsset`); on Windows 立即更新 downloads the release zip to `%TEMP%\open-translator-update`, extracts it and runs `install.ps1`, which replaces the binary and restarts the tray app
- The banner shows download progress and offers 重试 on failure; macOS/Linux keep the release-page link
- Verified with a stub release server and a UIAutomation click through the real banner (download, extraction, installer, app exit)


## Translation History and Pinned Window (post-v0.2.2)


- Completed translations are recorded in `%APPDATA%\open-translator\history.json` (`translator-core::history`, max 10, deduped); 历史/Ctrl+H lists them and clicking restores the translation
- 固定 prevents Esc/× from hiding the window (已固定 replaces the close button); Esc closes the history panel first
- Verified live via UI Automation and unit tests (translator-core 60, desktop 15)


## Tauri Client Migration (post-v0.2.2)


- `desktop/translator-popup-tauri` (Tauri v2 + HTML/CSS/JS) now mirrors the eframe client: tray/hotkey/single-instance, cursor placement, streaming translation, notifications, history/pin/settings pages, extension HTTP API, update banner and Windows one-click update, replace-in-place
- Linux packaging switched to it (`make-deb.sh` ships the Tauri binary; `open-translator-setup` writes the shortcut `--translate` plus the `--autostart` entry); the release jobs build the Tauri client on all three platforms and CI tests it on ubuntu/windows/macos
- Windows behavior verified live per slice; Linux verified (see the Linux Real-Machine Verification section); macOS is CI-built only


## Settings Page (post-v0.2.2)


- The tray menu's 设置… opens a full-page settings view (返回翻译/Esc backs out); 历史… opens the history list; the translation card footer keeps only 复制译文/替换原文/重新翻译/固定 and falls back to 历史/设置/退出 when no tray exists
- Edits the hotkey (applied immediately; invalid/taken bindings keep the old one), model_path and the auto_download / check_updates / serve_extension switches (next start, written with `settings::persist_value`), and opens the config folder; `--settings` starts on the page
- Verified live: config writes and hotkey re-registration through the real UI (desktop client now 17 tests)


## Linux Real-Machine Verification (2026-10-02, post-v0.2.2)


- Ubuntu 26.04 + GNOME Wayland: deb in-place upgrade (0.1.0 GTK → Tauri), shortcut/autostart registration, tray item, engine (`--print` and `POST /translate`), single-instance forwarding, `wl-paste` selection capture + history, hidden-failure notification and `/health` verified; user confirmed tray icon, transparency, cursor placement, bottom clamp and pin
- Tray clicks could not raise an already-visible card (GNOME refuses token-less focus/raise and sets `_NET_WM_STATE_DEMANDS_ATTENTION`); `show_main` now pulses always-on-top for 700 ms, and `固定` works because the client prefers the X11 backend on Wayland sessions (XWayland), which also sidesteps blank tray-menu labels under native Wayland (`GDK_BACKEND=wayland` opts out)
- After a login autostart the tray menu labels stayed blank (the AppIndicator extension cancels its property fetch when a concurrent layout update races it and never retries); the client now nudges the update menu item while the menu is closed (4 s/15 s after an autostart launch) so the next open re-reads every label
- Completed: the Wayland pass (upgrades, setup, tray, engine/API, forwarding, capture/history, notifications, visuals), the Xorg session pass (positioning and pin) and the login-autostart check (including the tray nudge)


## Docs/Tests/CI Cleanup (2026-10-02, post-v0.2.2)


- README/AGENTS/release skill and the deb's README.txt describe the Tauri client as shipped (no automatic shortcut registration, webkit2gtk-4.1 runtime); the legacy GTK/eframe clients are labeled as legacy and keep their own build commands
- Tauri tests: the cursor/work-area clamping is a pure `card_position` helper with five unit tests; the Windows update pipeline is testable (`run_update_install_with`) with a stub-server test that runs in CI
- CI: the Tauri matrix runs `cargo test --release` on ubuntu/windows/macos and a `tauri-frontend` job validates the config JSON plus `node --check ui/main.js`


## v0.3.0 Release (2026-10-02)


- Tag `v0.3.0` at `8341470`; release run 36973900935 built the Tauri client on Linux/macOS/Windows and published `OpenTranslator-windows-x64.zip`, `OpenTranslator-macos-arm64.dmg` and `OpenTranslator-linux-x64.deb`
- First release shipping the Tauri client on all three platforms; the notes document the highlights, the Linux `open-translator-setup` step and the v0.2.x upgrade path (the old eframe client cannot one-click update — download the zip and run `install.ps1` once)
- Windows/Linux real-machine verified; macOS CI-built only (flagged in the release notes)
- Remaining: macOS real-machine verification


## Legacy Client Cleanup (2026-10-02, after v0.3.0)


- Removed `desktop/translator-popup` (GTK), `desktop/translator-popup-desktop` (eframe) and `desktop/install.sh`; the crates stay in git history and pre-v0.3.0 release tags (v0.2.2 remains downloadable as a macOS fallback)
- CI dropped the legacy `desktop`/`desktop-popup` jobs; README/AGENTS/ARCHITECTURE describe the Tauri client as the only desktop client
- The Windows installers still stop the legacy `translator-popup-desktop` process and delete its exe during an upgrade from v0.2.x


## Update UI in Settings (2026-10-02, after v0.3.0)


- The update banner was removed from the translation card (review feedback: a full row with 查看/立即更新 does not belong in the translation popup); update handling now lives in the settings page's 版本与更新 section — current version, 检查更新, 更新说明/前往下载, the primary 更新/重试 action and inline download progress
- New commands: `check_update_now` runs a manual check and reports 正在检查/已是最新版本/检查失败 (`update-checking`/`update-none`/`update-check-failed` events; the startup check still stays quiet about those), `get_update_state` re-seeds the settings page when the startup check finished before the webview was listening; `SettingsPayload` carries `app_version` (`OPEN_TRANSLATOR_VERSION`)
- The tray 有新版本 v… item is unchanged and remains the notification channel while the window is hidden; the Windows download/install path is unchanged


## Desktop UI Redesign (2026-10-03, after v0.3.1)


- Card controls iconified (SVG buttons + shortcut tooltips, copy check flash); 朗读译文 (`speechSynthesis`), right-click 复制/全选/朗读 menu, status dot, single streaming paragraph and a titlebar 历史 (`Ctrl+H`) button (`Ctrl+,` opens settings)
- History redesigned: timestamps (`HistoryEntry.at`), relative time rows, 暂无翻译记录 empty state and a confirmed 清空历史; pin persists across restarts (`FileConfig.pinned` → `SettingsPayload.pinned`)
- Settings regrouped into 常规/模型/服务/更新 with info tooltips and a hotkey recorder (double-press confirm, resets to `Ctrl+Alt+T`); model path autosaves; 更新到 v<version> falls back to 前往下载 when not installable
- Tray/placement: tray and CLI opens center the window on the work area (`center_window`), tray separators and localized update texts, macOS template tray icon


## Browser Extension Overhaul (2026-10-03, after v0.3.1)


- Versions bumped to 0.2.0; shared `languages.js`; Chrome MV3 `alarms`/`action`, Firefox MV2 toolbar popup
- Streaming translations over a long-lived port (`/translate/stream` SSE, cold-start retries, `/translate` fallback); health alarm drives a `!` badge when the service is down
- Toolbar popup: service status, 最近翻译 history (click copies, 清空历史), target language, 划词自动翻译 and per-site opt-out
- Bubble rebuilt as a popover: state-driven controls (select/copy/⋯ menu: 复制双语/复制原文/朗读/替换原文/重新翻译/互换源/语言设置…), only 停止 while streaming and 重试 on error, icon buttons, shimmer + caret streaming indicator, hover-revealed close in the action row
- Replace-original in `<input>`/`<textarea>`/`contenteditable`, input selection support, auto-translate guards, `disabledSites`; `Alt+Shift+Y` clipboard translation; `result.html` for pages without a content script (PDF viewer); history capped at 20, deduped
- Chrome e2e extended (history, ⋯ menu, textarea replace, streaming classes, hover-aware close check)


## Extension Release Assets (2026-10-03, after v0.3.1)


- `release.yml` `browser-extension` job builds and attaches `OpenTranslator-browser-chrome.zip` / `OpenTranslator-browser-firefox.zip` to `v*` releases (artifact-only on `workflow_dispatch`)
- `browser/build.sh` zips with a top-level folder and bundles `packaging/browser/README.txt`; README documents the zip install flow (Firefox zip is unsigned, temporary loading only)


## v0.4.0 Release (2026-10-03)


- Tag `v0.4.0` at `17019b6`; release run 37112505593 passed the Linux/macOS/Windows package jobs and the new browser-extension job, publishing five assets (three desktop packages + two extension zips)
- First release attaching `OpenTranslator-browser-chrome.zip` / `OpenTranslator-browser-firefox.zip`; notes cover the desktop redesign, the extension overhaul and the usual install/upgrade steps
- Desktop real-machine visual pass on Windows/macOS pending; macOS remains CI-built only


## Linux Real-Machine Verification of the v0.4.0 Redesign (2026-10-03)


- Runner: Ubuntu 26.04 + GNOME Wayland, `OpenTranslator-linux-x64.deb` 0.4.0 from the GitHub release installed over the previous version; the client runs through XWayland (`prefer_x11_backend`) with the embedded `hy-mt1.5-1.8b-q4_k_m.gguf` model
- Verified: redesigned card/settings/history (history newest first with relative times), `wl-paste --primary` selection capture via CLI forwarding and a cold `--translate` start, global shortcut Ctrl+Alt+T, hidden `--autostart` start, tray item and populated DBusMenu labels (显示窗口/历史…/设置…/已是最新版本/退出, update check reached GitHub), single-instance forwarding, extension API (`/health` → llama-cpp, `POST /translate`) and a hidden model-error notification through `notify-send` (stub-captured)
- Confirmed Linux gaps（已在下面的 Gap Closure 一节修复）: 复制 is a no-op (no Linux clipboard backend: `arboard`/`enigo` are Windows/macOS-only deps), 替换原文 is hidden (Windows-only), 朗读 is hidden (no `speechSynthesis` in this WebKitGTK), updates open the release page only, and the copy tooltip advertises Ctrl+Shift+C with no handler
- `apt` upgrades do not restart a running tray client; the running pre-upgrade binary keeps serving until the next launch/login (the restart was required to load the redesign)
- Pointer input cannot be synthesized under this Wayland session (XTEST motion ignored), so click-driven checks (copy button, pin, context menu) stay for the Xorg-session pass; keyboard paths used `XSetInputFocus` + XTEST


## Linux Gap Closure (2026-10-03, after v0.4.0)


- Copy works on Linux now: `arboard` X11 backend (bridge to Wayland verified), a single long-lived clipboard instance, shared `copy_text`, and the advertised `Ctrl+Shift+C` / `Ctrl+Enter` shortcuts are implemented
- Capture falls back to the X11 PRIMARY selection for Xorg sessions; replace-in-place works for X11/XWayland source windows (managed top-level via WM_STATE, own-window PID guard, verified focus, `enigo` Ctrl+V, clipboard/Ctrl guard on every exit)
- 朗读 speaks through `spd-say --wait` (accurate `speech-ended`/active state, stopped with `--stop`/`--cancel`, target-language voice via `-l`, button hidden when no voice matches); Linux one-click update verifies the GitHub asset SHA-256, stages the deb in a private 0700 directory and installs it with `pkexec apt-get`, then restarts the client (release-page fallback without pkexec/apt or a digest); the empty state shows the configured hotkey
- Packaging Recommends `speech-dispatcher` + `pkexec`; postinst reminds manual upgraders to restart the client
- Verified with the local release build on the same machine (arboard bridge, X11 fallback capture, replace target detection, installable update button, `cargo test`); the physical Xorg-session input pass (pointer and XTEST key injection) is still outstanding because Mutter drops synthetic input under Wayland


## Browser Extension Inline Translation (2026-10-03, after v0.4.0)


- 边写边译 in the content script (`typeTranslate`, default off): after a typing pause (`typeTranslateDelay`, default 500 ms) the sentence at the caret is translated and streamed into a non-interactive bubble anchored at the caret; `Tab` replaces the sentence (input/textarea via `setRangeText` + `input`, contenteditable via `execCommand("insertText")`), `Esc`/blur/mousedown dismisses; IME composition is skipped until `compositionend`, password/readonly fields never trigger, and `disabledSites` applies
- Typing previews use a second translate port with `record: false`, so they do not fill history; a successful Tab commit records the pair through the new `record-history` message
- Settings: toolbar popup 边写边译 checkbox; options page 边写边译 section (enable, minimum characters, pause delay)
- Follow-up (same day, "change language while typing"): the bubble is interactive — a target-language chip with a menu (picking one retranslates the current sentence immediately) and a 设置… gear that opens the options page; `mousedown` on the card is default-prevented so the edited field never loses focus; the language menu flows below the chip (it used to open upward over the translation), and a rebindable `cycle-target` command (`Alt+Shift+L`) cycles the target language while the bubble is visible, backed by up to three `recentTargets`
- Manifests bumped to 0.3.0 (a re-sign must not reuse the released 0.2.0)


## Extension UI Pass (2026-10-03, after the inline translation follow-up)


- Shared Apple-style design tokens (`UI_TOKENS_CSS`, injected into both bubble shadow roots): system font stack, neutral label/systemFill palette, translucent material (`backdrop-filter: blur(20px) saturate(180%)`, solid fallback), 12px card / 8px control radii, hairline plus layered shadows, `color-scheme: light dark`
- Typing bubble: target chip with an SVG chevron and a proper gear icon, `Tab`/`Esc` keycaps, opacity-pulse waiting state, 2px rounded streaming caret, bottom fade for clipped translations, language list as a section list (separator, hover fill, accent checkmark, 6px scrollbar)
- Selection bubble unified on the same tokens (material card, system fills, accent focus rings, Apple red errors); popup and options pages re-skinned (filled rounded controls, blue accent checkboxes, thin scrollbars)
- Language dropdowns unified through a shared `dropdown.js` (`OTSelect`, loaded as a content script and by the pages): the native `<select>` stays in the DOM as the hidden value holder while a chip button + checkmarked menu renders in the selection bubble, toolbar popup and options page; the typing bubble uses the same component with an in-flow menu variant; the menu's outside-click check uses `event.composedPath()` (Shadow DOM retargeting closed it on its own items)
- Verified with the 33-check Chrome e2e plus light/dark screenshots of the typing bubble, selection bubble, popup and options
- Chrome e2e extended with textarea and contenteditable typing flows (bubble, Tab hint, commit, dismissal); local Edge + llama-cpp run passes all 26 checks
- Tray hit-box report (2026-10-03) disproved: with the menu open, every row spans the full menu width (206 px) and the blank area right of the label delivers DBusMenu `clicked` events; the real cause of the perceived dead menu was the raise pulse being skipped for a hidden card, so a tray 历史…/设置… click mapped the card below a fullscreen window; `show_main_with` now pulses on every show (pinned windows excluded), the pulse is generation-guarded so overlapping shows do not cut a newer pulse short, and the hidden-card + fullscreen repro passes with the local 0.4.1 build


## v0.4.1 Release (2026-10-03)


- Tag `v0.4.1` at `c138c1e`; release run 37123869327 passed the Linux (~7m49s), macOS (~9m22s), Windows (~12m26s) and browser-extension (~9s) jobs, publishing the same five assets: `OpenTranslator-windows-x64.zip`, `OpenTranslator-macos-arm64.dmg`, `OpenTranslator-linux-x64.deb`, `OpenTranslator-browser-chrome.zip` and `OpenTranslator-browser-firefox.zip`
- Release notes recap the Linux gap closure (X11 clipboard copy, X11/XWayland replace-in-place, PRIMARY fallback, one-click deb update), the speech-dispatcher read-aloud fixes, the simplified history rows and the fullscreen tray fix, plus the usual install/upgrade steps
- Windows/macOS real-machine visual pass still pending; macOS remains an unsigned arm64 dmg


## Desktop UI Design Language (2026-10-03, after the extension UI pass)


- The Tauri client moved onto the same Apple-style `--ot-*` tokens as the extension (neutral label ramp, system fills, `#007aff`/`#0a84ff` accent, 12/8px radii, hairline + layered shadow, system font stack, `color-scheme: light dark`); cards, groups, buttons, switches, keycaps, context menu, confirm dialog, scrollbars and the streaming caret were re-skinned, with Apple system green/orange/red for status colors
- `#source-select` / `#target-select` now render through the mirrored `ui/dropdown.js` (`OTSelect`) chip menu (checkmarks, hover/focused states, hidden native selects as value holders); `applyLanguageState` syncs the chips after programmatic updates so swap and `Ctrl+1/2/3` recent-target shortcuts stay accurate
- Fixed a pre-existing mojibake in `ui/main.js` (the detected label rendered as `妫€娴嬶細` instead of `检测：`)
- The design language is now a documented design principle: `docs/ARCHITECTURE.md` → UI Design Language (tokens, controls, the dropdown-copy sync rule, motion, enforcement), referenced from AGENTS and the Conventions
- Verified by previewing the frontend with a stubbed `window.__TAURI__` in headless Edge (translator/settings/history, light/dark; a real-click language pick updates the native select and calls `set_source`); frontend-only change, no Rust rebuild required
- Follow-up: a new translation (hotkey or `--translate`) now switches the card back to the translator view even when it sat on the history or settings page (`listen("source")` calls `showView("translator")` first)


## Dropdown Viewport Placement (2026-10-03, after the desktop UI pass)


- Long language lists were clipped when the control sat low on screen (the menu is an absolute layer, so the bubble placement cannot reserve room for it); `OTSelect.open()` now measures the free space above/below, flips the menu above the button when the space below is tighter, and always clamps `max-height` to the available space so the list scrolls instead of overflowing
- The inline typing-bubble variant follows the card flow and clamps its height to the space below; both variants recompute on resize/scroll while open
- Mirrored in the desktop copy (`ui/dropdown.js`); verified by a Chrome e2e bottom-anchored selection scenario asserting `ot-select-menu-up` and viewport bounds (35 checks), plus a desktop preview at a 230 px viewport showing a constrained menu


## Shared UI Sources (2026-10-03, after the dropdown placement fix)


- `shared/ui/tokens.css` and `shared/ui/dropdown.js` are now the single sources for the design tokens and the `OTSelect` component; `shared/sync-ui.sh` regenerates `browser/extension/tokens.{css,js}` (the JS variant rewrites `:root` to `:host` for shadow roots), `browser/extension/dropdown.js`, `desktop/.../ui/tokens.css` and `ui/dropdown.js`, with `--check` failing on drift (new `shared-ui` CI job)
- Extension pages link `tokens.css`; content scripts load `tokens.js` before `dropdown.js`/`content.js` (manifests and `browser/build.sh` updated); the desktop links `ui/tokens.css`; `content.js` dropped its inline token copy and uses `--ot-bg-material` for bubble material
- `OTSelect.place()` additionally clamps to the visible screen area (`screenY` / `screen.availHeight` / window chrome) so a webview or browser window extending past the screen bottom keeps menus reachable (best effort)
- Verified: fresh-profile Chrome e2e exit 0 (35 checks), token presence on popup/options/result/desktop (`--ot-accent` `#007aff`, radius 12), `shared/sync-ui.sh --check`, `web-ext lint` clean
- Accepted boundaries (recorded, not stylable): OS-native surfaces (tray menu, notifications, file dialogs, installers); the desktop card stays opaque because transparent-webview blur is unreliable across WebView2/WebKitGTK


## v0.5.0 Release (2026-10-04)


- Tag `v0.5.0` at `c948d99` (CI run 37138675325 green first); release run 37138919115 passed the Linux (~8m0s), macOS (~5m44s), Windows (~11m57s) and browser-extension (~9s) jobs, publishing the same five assets: `OpenTranslator-windows-x64.zip`, `OpenTranslator-macos-arm64.dmg`, `OpenTranslator-linux-x64.deb`, `OpenTranslator-browser-chrome.zip` and `OpenTranslator-browser-firefox.zip`
- Release notes cover the extension 边写边译 flow (caret bubble, `Tab` commit, target-language chip, `Alt+Shift+L` cycle-target), the shared Apple-style tokens and `OTSelect` dropdowns across extension and desktop, viewport-aware menu placement, the translator-view switch on new translations and the single-sourced `shared/ui` copies with the `shared-ui` CI check, plus the usual install/upgrade steps
- Verified before tagging on the same tree: local Linux deb + installed tray client manual pass, Firefox temporary-extension manual pass and Chrome e2e 36 checks; the CI flake that failed the previous run was fixed first (`test: stabilize the dropdown placement e2e checks`); Windows/macOS real-machine visual pass still pending


## Desktop Selection Translation — Linux Landing (2026-10-04)


- `docs/SELECTION_TRANSLATION.md` records the design and the staged plan: `selection_mode = off | ball | auto` (`selection_delay` ms, `selection_min_length` chars), a watcher thread with settle/dedupe/self-filter/suppression guards, and the floating-ball window
- Linux landed: `src/selection_watch.rs` polls the PRIMARY selection (direct X11 read or `wl-paste` on Wayland; no clipboard writes, no synthetic keys), `ui/ball.{html,css,js}` is a 44×44 transparent always-on-top window with a 120 ms hover dwell and click fallback, placed next to the selection and auto-hidden after 5 s
- Settings: new 划词翻译 group (mode `OTSelect`, minimum length, trigger delay) plus a gear button in the card titlebar; the empty-state hint follows the mode; on native Wayland the ball degrades to direct translation and the settings page says so
- Hotkey still wins: it hides the ball and suppresses the watcher for 1.2 s, so the same selection is never translated twice
- Verified: `cargo test` green in `translator-core` (61) and the client (15, incl. 4 new watcher tests), `node --check`, `json.tool`, `shared/sync-ui.sh --check`; Windows/macOS watchers staged (mouse hook + UIA / NSEvent + AX), manual X11 check pending
