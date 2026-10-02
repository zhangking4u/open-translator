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
│   ├── translator-popup/         (Linux/GNOME GTK popup)
│   ├── translator-popup-desktop/ (Windows/macOS eframe client)
│   └── install.sh / install-*.ps1
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

fd18fc2 docs: update release skill for the Linux deb (tag: v0.1.0); main at 4d813bc (desktop UI polish and Windows fixes, not yet released)


---

## 8. Current Development Phase


Phase:

Consumer edition packaging (Phase C)


Goal:

Ordinary users on Windows/macOS can download, install and use it without technical setup.


Status:

`v0.1.0` published 2026-10-01 (Windows zip + macOS arm64 dmg): https://github.com/zhangking4u/open-translator/releases/tag/v0.1.0


---

## 9. Next Steps


1. Code signing / notarization (budget decision)

2. macOS real-machine verification deferred (no Mac hardware; dmg is arm64-only); AppImage deferred

3. (Done 2026-10-01) v0.1.0 re-released with the Linux deb and the startup update check (three assets); Windows real-machine re-verification passed (selection capture, CJK fonts, single instance). v0.2.0 released the same day (release run 36859033510, three assets) with source-language detection, SSE streaming, GTK swap/recent targets and the clipboard toggle

4. Real-machine visual pass of the redesigned desktop UIs (Windows/macOS), including the new source-language dropdown and detection hint; window geometry/position memory and an in-app settings panel are candidates for the next polish round


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
