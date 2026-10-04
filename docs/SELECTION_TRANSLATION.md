# Selection Translation (划词翻译) — Design & Landing Plan

Status: Phase 1–5 landed (Linux, Windows, macOS cross-target compile-checked); real-machine checks pending on X11/Windows/macOS (see §7).


## 1. Goal

Add browser-extension-style selection translation to the desktop client:

- Select text anywhere, then either
  - **悬浮球 (ball)**: a small floating ball appears next to the selection; moving
    the mouse onto it starts the translation, or
  - **直接翻译 (auto)**: the translation card opens as soon as the selection settles.
- The card titlebar gets a gear button that jumps to the settings page, where the
  mode is chosen.
- The existing global hotkey (`Ctrl+Alt+T`) keeps working in every mode.

The feature reuses the existing capture paths and the translation card; the new
parts are a selection watcher, a second always-on-top window for the ball, and
the settings plumbing.


## 2. Interaction modes

`selection_mode` config key, three values:

| value | behavior |
| --- | --- |
| `off` (default) | current behavior; only the hotkey translates |
| `ball` | selection settles → ball appears near the selection; mouse enters the ball (or clicks it) → card opens with the translation; ball auto-hides after 5 s, on a new selection, or when the card shows |
| `auto` | selection settles → card opens immediately |

Parameters (`selection_min_length` default 2 chars, `selection_delay` default
400 ms, clamped 100–3000 ms). A selection shorter than the minimum is ignored.
The delay is a settle time: the selection must be unchanged for that long before
anything happens, so dragging across text does not fire.

Priority/conflict rules:

- The hotkey always wins. It hides the ball, suppresses the watcher for 1.2 s and
  captures the selection itself.
- While the card is focused (the user is selecting/copying inside the card) the
  watcher ignores selections.
- A pinned card is never disturbed by the watcher.
- The same text is only translated once until the selection changes (empty
  selection resets the dedupe), so moving the mouse does not retrigger.
- Ball and auto translations are recorded in history exactly like hotkey ones.

The ball is intentionally a separate small window, not part of the card: it must
not steal focus, must sit above other windows, and must disappear without
touching the card until the user commits.


## 3. Ball UX

- 44×44 px transparent window, circular accent button (shared `--ot-*` tokens,
  white translate glyph, hairline + shadow, light/dark parity).
- Appears offset to the bottom-right of the selection anchor, clamped to the
  monitor work area (reuses `card_position`).
- Hover intent: 120 ms dwell in `ball.js` before invoking `ball_hover`; leaving
  early cancels. A click triggers immediately.
- Hovering translates the pending selection and hides the ball; the card then
  opens near the cursor (i.e. near the ball).
- No drag/keyboard interaction in v1; no focus, no taskbar entry, no shadow.


## 4. Config & state

`translator-core::settings::FileConfig` gains:

```
selection_mode = off | ball | auto
selection_delay = 400          # ms, 100..3000
selection_min_length = 2       # characters, 1..50
```

Runtime state (`AppState` in the Tauri client):

| field | meaning |
| --- | --- |
| `selection: Mutex<SelectionSettings>` | live mode/delay/min-length (updated by the settings commands, seeded from the config file at startup) |
| `pending_selection: Mutex<Option<PendingSelection>>` | text + replace-window waiting for a ball hover |
| `ball_generation: AtomicU64` | invalidates stale auto-hide timers |
| `suppress_until: Mutex<Option<Instant>>` | hotkey suppression window |
| `focused_window: Mutex<Option<String>>` | which of our windows has focus (self-selection filter) |

`SettingsPayload` exposes `selection_mode`, `selection_delay_ms`,
`selection_min_length`, `selection_supported`, `selection_ball_supported`.
The settings group is hidden when `selection_supported` is false, and shows a
note when `selection_ball_supported` is false (Wayland degrades `ball` to
`auto`).


## 5. Runtime architecture

```
selection_watch (thread)          main window (card)         ball window
  poll / hook                       translate_text            ball.html/js
      |                                  ^                        |
      v                                  |                     hover/click
  settle + guard                     show_main                    |
      |                                  |                        v
      +-- mode=auto ----------------> translate_text          ball_hover
      |                                                           |
      +-- mode=ball --> pending_selection + show_ball ------------+
```

- `selection_watch.rs` owns: config parsing, mode enum, settle/guards, and the
  platform watcher. Linux polls the PRIMARY selection (no clipboard writes, no
  synthetic keys); the poll interval is 400 ms and the configured delay is
  applied as a settle window on top.
- Guards before acting: mode != off, length >= min, the watcher has not already
  acted on this exact selection, not pinned, our window not focused, not
  suppressed, selection not empty. Empty selections reset the watcher dedupe.
- `ball_hover`/`ball_click` take the pending selection, hide the ball, set
  `replace_window`, suppress the watcher briefly, show the card and translate.
- `hide_ball` bumps `ball_generation`, hides the window and drops the pending
  selection; a 5 s timer only hides when the generation still matches.
- `trigger_translation` (hotkey) hides the ball first and suppresses the watcher.

Linux specifics:

- Session detection: `XDG_SESSION_TYPE` (fallback: `WAYLAND_DISPLAY`/`DISPLAY`).
  - X11 → read PRIMARY over X11 (`capture::primary_selection_x11`), anchor from
    `capture::pointer_position` (XQueryPointer); ball fully supported.
  - Wayland → read PRIMARY through `wl-paste` (`capture::primary_selection_wayland`);
    global pointer coordinates are unavailable, so `ball` degrades to `auto`.
  - Unknown → best-effort `capture_selection`.
- GNOME/Wayland cannot watch selection changes (`wl-paste --watch` needs
  wlr-data-control, which Mutter does not implement — verified on the dev
  machine), hence polling. This also keeps one code path for X11 and Wayland.
- 400 ms polling of `wl-paste` (Wayland) / X11 selection reads costs a process
  spawn per tick only while a non-off mode is enabled; while off, the loop skips
  reading entirely.

No clipboard access and no synthesized keystrokes happen on Linux, so selection
translation is side-effect free there (PRIMARY is read only). On Windows the
UI Automation read comes first and the existing capture fallback only runs after
a detected drag outside known terminal windows, where Ctrl+C would be SIGINT
(see §7).


## 6. Files

- `docs/SELECTION_TRANSLATION.md` — this document.
- `desktop/translator-core/src/settings.rs` — new keys + tests.
- `desktop/translator-popup-tauri/src/selection_watch.rs` — new module.
- `desktop/translator-popup-tauri/src/capture.rs` — public PRIMARY/pointer
  helpers (`primary_selection_x11`, `primary_selection_wayland`,
  `pointer_position`).
- `desktop/translator-popup-tauri/src/main.rs` — state, commands (`ball_hover`,
  `ball_leave`, `ball_click`, `save_selection_mode`, `save_selection_options`),
  window creation, focus tracking, watcher startup.
- `desktop/translator-popup-tauri/ui/ball.{html,css,js}` — ball window.
- `desktop/translator-popup-tauri/ui/{index.html,main.js,style.css}` — settings
  group, gear button, mode-aware empty-state hint.
- `desktop/translator-popup-tauri/capabilities/default.json` — add the `ball`
  window.


## 7. Platform phases

| phase | platform | mechanism | state |
| --- | --- | --- | --- |
| 1 | translator-core | config keys + parsing | landed |
| 2 | Linux | PRIMARY polling + settle/guards | landed |
| 3 | Linux/X11 | ball window + hover flow + settings UI | landed |
| 4 | Windows | `WH_MOUSE_LL` hook thread (mouse-up, drag detected by distance, settle wait), UI Automation `TextPattern.GetSelection()` for text, cursor position as the anchor, enigo Ctrl+C fallback only after a detected drag, with clipboard restore and a terminal-class blocklist (Ctrl+C is SIGINT there); password fields are skipped through `CurrentIsPassword` | landed (cross-target compile-checked; real-machine check pending) |
| 5 | macOS | Listen-only session `CGEventTap` (core-graphics) for mouse down/up, cursor point as the anchor (`LogicalPosition`), raw `AXUIElementCopyAttributeValue` reads for the selected text (accessibility permission is already required by the capture path); for drags without AX text, the existing Cmd+C capture (a plain copy, safe without a selection); password/secure fields expose no AX text | landed (cross-target compile-checked; real-machine check pending) |

All three desktop targets now report `selection_supported()`; `ball_supported()`
is still false on native Wayland, where the ball degrades to direct translation
and the settings page says so.


## 8. Verification

- `cargo test` in `desktop/translator-core` (config parsing) and in
  `desktop/translator-popup-tauri` (settle/guard/placement unit tests).
- `node --check ui/main.js ui/ball.js ui/dropdown.js`.
- `python3 -m json.tool` for `capabilities/default.json` / `tauri.conf.json`.
- `bash shared/sync-ui.sh --check` (tokens/dropdown copies untouched).
- Manual on Linux X11: enable 悬浮球, select text, move onto the ball, card opens
  and history records the translation; enable 直接翻译 and check the card opens
  after the settle delay; hotkey still suppresses the watcher; settings gear on
  the card opens the settings page.
- Manual on GNOME Wayland: the settings note explains the degrade and selections
  translate directly (no ball).
