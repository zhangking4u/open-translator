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

a6f1321 fix: start the extension server once and add a single-instance lock (tag: v0.1.0)


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


1. Ship the deb with the next release (Linux release job and packaged first-run download verified 2026-10-01)

2. Code signing / notarization (budget decision)

3. (Done 2026-10-01) Linux deb package for the GNOME client (llama.cpp + first-run model download); tag `v0.1.0` published; Windows real-machine re-verification passed (selection capture, CJK fonts, single instance)

4. macOS real-machine verification deferred (no Mac hardware; dmg is arm64-only); AppImage deferred


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
- Pending: `apt install` on a real user account (needs admin rights), shipping the deb with the next release; AppImage deferred
