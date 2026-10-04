# OpenTranslator Architecture


## 1. Overview

OpenTranslator is an open-source local AI translation platform.

The system aims to provide universal translation capability across different environments:

- Desktop applications
- Browsers
- Online meetings
- Mobile devices


The core design principle:

Local-first, modular, extensible.


---

# 2. High Level Architecture

             User

              |

    +-------------------+
    | Client Interfaces |
    +-------------------+

      |       |       |

  Desktop  Browser  Mobile


              |

              |

    Translation Core Service


              |

    +----------------+

    | Translation API |

    +----------------+

              |

              |

    Translation Engine Layer


      |          |          |

   HY-MT      NLLB       Qwen


              |

              |

      Model Runtime Layer


---

# 3. Core Components


## 3.1 Desktop Client

Responsibility:

- Capture selected text
- Display translation popup
- Communicate with Translation Core


Status:

- Windows/macOS/Linux: `desktop/translator-popup-tauri` — Tauri v2 client, resident with a tray/menu-bar icon and a global shortcut (`Ctrl+Alt+T`; Linux/GNOME via `open-translator-setup`); embeds the llama.cpp engine (no external service), downloads the model on first run and serves the HTTP API for the browser extension while running; Wayland sessions prefer the X11 backend so keep-above/raise works


---

## 3.2 Translation Core

Technology:

Rust


Responsibility:

- Provide translation API
- Manage translation workflow
- Coordinate translation engines


Current implementation:

translator-service


---

## 3.3 Translation API

Protocol:

REST API


Current endpoints:


GET /health

Purpose:

Check service availability.


POST /translate

Purpose:

Submit translation request.


Error responses:

JSON `{"error":{"kind","message"}}` with 400 invalid request, 500 internal, 502 engine unavailable, 504 timeout.


---

## 3.4 Translation Engine

The engine layer provides abstraction between business logic and AI models.


Design goal:

The core system should not depend on a specific model.


Possible implementations:

- Mock Engine
- HY-MT Engine
- NLLB Engine
- Qwen Engine


Current implementation:

MockEngine, OllamaEngine and LlamaCppEngine behind `engine::build` (building can fail while loading a model); every engine is wrapped in a timeout guard. The trait is async and fallible. `core/inference` wraps llama.cpp (`llama-cpp-2`) with a serialized worker thread and is the default engine for the consumer edition (embedded in the Windows/macOS desktop client; the service can use it via `TRANSLATOR_ENGINE=llama-cpp`). Prompt styles, sampling options, stop strings and language tag normalization live in the domain layer (`domain::prompt`, `domain::language`).


---

## 3.5 Browser Extension

Status:

Implemented: `browser/extension` (shared JS, no bundler). Firefox ships `manifest.json` (MV2, version 0.1.0 signed on AMO) and Chrome `manifest.chrome.json` (MV3). Context menu / keyboard shortcut → content-script bubble (target-language switch, optional auto-translate, copy) → local `/translate` through the background page (host permission, no CORS changes to the service).


---

# 4. Design Principles


## Modularity

Each component should have clear responsibility.


## Model Independence

AI models can be replaced without changing upper layers.


## Local First

Translation should work without external services whenever possible.


## Cross Platform

Core logic should remain platform independent.


## Selection Translation

The desktop client translates the user's selection in three ways: the global hotkey (always available), a floating ball that appears next to the selection and translates on hover/click, and a direct mode that opens the card after the selection settles (`selection_mode = off | ball | auto` in `docs/SELECTION_TRANSLATION.md`).

- The watcher is deliberately local-first and side-effect free: on Linux it only reads the PRIMARY selection (X11 direct or `wl-paste`), never writes the clipboard and never synthesizes keys; on Windows it reads through UI Automation first and only after a detected drag (and outside terminals, where Ctrl+C is SIGINT) falls back to the existing capture; on macOS a listen-only CGEventTap drives the same settle flow, AX reads the selection and a drag-only Cmd+C fallback covers apps without AX.
- The floating ball is its own hidden, undecorated, always-on-top, non-focusable window (`ui/ball.*`) so committing to a translation never steals focus from the source application; the card only opens on the commit.
- Platform honesty over fake features: native Wayland offers no global pointer coordinates, so `ball` degrades to `auto` there and the settings page says so; the watcher only starts on platforms where it works.


## UI Design Language

All user-facing surfaces (desktop client, browser-extension bubbles and pages) follow one Apple-flavoured visual language, so the product reads as a single native-feeling app on every platform. The tokens below are the contract; components across the repo mirror each other instead of re-inventing local styles.

- **Tokens** (CSS custom properties, `--ot-*`): neutral label ramp (`--ot-label`, `-2`, `-3`), system fills (`--ot-fill`, `--ot-fill-hover`), separators/hairlines, accent (`#007aff` light / `#0a84ff` dark), status colors, `--ot-radius` 12 / `--ot-radius-sm` 8, a layered soft shadow, and the system font stack. Light and dark values live together and `color-scheme: light dark` follows the OS. The values live once in `shared/ui/tokens.css`; `shared/sync-ui.sh` generates the extension `tokens.css`/`tokens.js` (the latter rewrites `:root` to `:host` for shadow roots) and the desktop `ui/tokens.css` copy.
- **Controls**: filled, borderless, rounded controls (no 1px outlines); focus is a 2px accent ring, hover steps up the fill, disabled fades to the tertiary label. Native form widgets stay only as hidden value holders; they are rendered by the shared `OTSelect` component (chip button + checkmarked menu), generated from `shared/ui/dropdown.js` by `shared/sync-ui.sh` into `browser/extension/dropdown.js` and `desktop/translator-popup-tauri/ui/dropdown.js` (the `shared-ui` CI job rejects out-of-sync copies). Menus flip above the button and clamp their height to the free viewport space when the space below is tight, so long lists stay reachable.
- **Surfaces**: hairline plus soft layered shadow instead of hard borders; grouped settings lists; secondary text on the label ramp.
- **Motion**: 0.15–0.2 s eases; streaming cursors are 2px rounded bars with an opacity pulse; `prefers-reduced-motion` disables animations.
- **Typography**: `system-ui` (SF on macOS, Segoe UI on Windows) carries the type; no decorative fonts.

Enforcement: extension changes keep `web-ext lint` clean and the Chrome e2e green; the desktop UI is checked by the `tauri-frontend` CI job (`node --check ui/*.js`) and should be previewed in light and dark before shipping.


---

# 5. Current Architecture Status


Completed:

- Rust core service (Axum API; engines: Mock / Ollama / in-process llama.cpp via `core/inference`)
- Model evaluation (HY-MT1.5-1.8B default; TranslateGemma 4B quality option) with per-style prompts/sampling
- API layer split (`src/api`) with unit and integration tests
- Desktop client: Tauri v2 (`desktop/translator-popup-tauri`) on Windows/macOS/Linux, embedding the engine, tray/menu-bar, `Ctrl+Alt+T`, first-run model download, in-process HTTP for the extension; the legacy GTK/eframe clients were removed in v0.3.0
- Browser extension: Firefox MV2 (signed) + Chrome MV3, bubble language switch and auto-translate
- Unified Apple-style UI language across the desktop client and the extension (shared `--ot-*` tokens, shared `OTSelect` dropdown component)
- CI on ubuntu/windows/macos plus a Chrome e2e job; release packaging (Windows zip installer, macOS dmg, Linux deb)


In Progress:

- Distribution: tag `v0.1.0`, real-machine verification, code signing/notarization


Next:

- Meeting translation (parked)
- Mobile client

