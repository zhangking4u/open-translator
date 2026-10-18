# Selection Translation (划词翻译) — Design & Landing

Status: Linux uses the docked confirm ball (Wayland and X11); Windows/macOS offer
the selection-following ball, the compact selection toolbar and direct
translation. The Windows toolbar real-machine check passed 2026-10-18; the
X11-session and Linux toolbar checks are still pending; macOS cannot be
real-machine-verified (no Mac hardware) and ships as an experimental watcher.


## 1. Goal

A mouse-only, continuous confirmation flow: select text, then the next mouse
action confirms the translation — without leaving the mouse and without the
tool ever interrupting the selection or the current activity.

Selecting text is ambiguous (copy, edit, highlight), so the tool never treats a
selection itself as translation intent. Confirmation is always an explicit
small gesture: hover the ball (Linux/Windows/macOS), click 翻译 in the compact
toolbar, or, where a mouse-release signal is observable, the opt-in direct mode
(Windows/macOS only).


## 2. Modes

| value | Linux | Windows/macOS |
| --- | --- | --- |
| `off` (default) | nothing | nothing |
| `ball` | docked confirm ball | selection-following ball |
| `toolbar` | not offered; a config value falls back to `ball` | compact action bar above the selection; 翻译 opens the card |
| `auto` | not offered; a config value falls back to `ball` | direct card after the mouse-up settle |

The settings UI exposes a 划词翻译 master switch plus a 翻译方式 choice
(划词工具栏/悬浮球翻译/立即翻译); unsupported options are hidden per platform.
The tray has the same choice as a 划词翻译 submenu and both stay in sync. New
configs default to `toolbar` on Windows/macOS and `ball` on Linux; an explicit
`selection_method` from an older config is preserved.


## 3. Docked confirm ball (Linux)

Native Wayland gives regular applications no global pointer events, no reliable
selection anchor and no arbitrary window placement, so a ball or card next to
the selection cannot be trusted. The only reliable capabilities are reading the
PRIMARY selection and owning a fixed-position window — so confirmation lives at
a fixed dock instead of beside the selection.

- **Dock**: right edge, vertically centered on the primary monitor by default,
  inset 8 px. The ball is draggable (`ball.js` reports deltas via
  `move_ball_by`, `save_ball_position` persists them); the position is clamped
  to the work area on every placement and after monitor changes. Dropping the
  ball within ~48 px of its default dock resets the custom position
  (the `ball_x`/`ball_y` keys are removed), so no "restore default" control is
  needed in the settings.
- **Visibility** (`ball_visibility`, config file only): `always` (default)
  keeps the ball visible and dim while nothing is selected; `selection` shows
  it only once a selection is armed.
- **States** (`ball-state` event consumed by `ball.js`): `idle` (dim), `armed`
  (accent, a settled selection is ready), `busy` (pulse while translating).
- **Confirm**: hover only, 150 ms dwell; leaving cancels. A drag never counts
  as a hover, and a click is not a confirm. The ball is non-focusable
  (`set_focusable(false)`), so interacting with it cannot steal the selection
  from the source application.
- **Card**: opens beside the ball (side facing the work-area center, vertically
  centered, clamped), shown non-focusable so the source app keeps focus; the
  first click on the card makes it focusable again (`card_engaged`). Every
  commit raises the card with the 700 ms always-on-top pulse (no focus), so a
  card that was already visible cannot stay hidden behind the active window.
  Hovering the same selection again only re-shows the card; it does not
  retranslate. When a translation finishes, the ball goes back to `idle`
  (dim) unless a newer selection was armed meanwhile.
- **Selection watcher**: event-driven through XFixes. GNOME mirrors every
  Wayland PRIMARY update to X11, and `xfixes_select_selection_input` reports
  each update (verified on GNOME 50). The watcher never polls and **never
  reads the selection while it is changing**: after the last event it waits
  `selection_delay` (default 900 ms on Linux), then reads PRIMARY once over the
  XWayland bridge. The earlier 400 ms `wl-paste` polling was measured to
  interrupt native drag selection and was removed; only a session without an
  X display falls back to slow `wl-paste` polling.
- **Hover commit**: hovering is explicit confirmation, so `ball_hover` reads
  the current PRIMARY on the spot (one X11 read) instead of waiting for the
  settle timer or a previously armed text. Hovering before the ball lights up
  therefore works, and typed text is only translated when the user asks for it.
- **Typing filter (AT-SPI)**: many toolkits mirror typed text to PRIMARY.
  When the accessibility bus is available, the watcher tracks the focused text
  object and asks whether it really has a selection (`GetNSelections`); caret
  and text-change events without a nearby selection change count as typing and
  keep the ball dim. Apps that expose no text events at all (Electron/VS Code
  with accessibility off) provide no signal, so the watcher falls back to the
  PRIMARY-only behaviour there — typing can still light the ball in those
  apps; enabling `editor.accessibilitySupport` in VS Code makes the filter
  work. The filter is best-effort: a missing connection, object or Text
  interface is "unknown" and never blocks the feature.
- **Hotkey** (`Ctrl+Alt+T`) still captures the selection and opens the focused
  card (cursor placement, always-on-top pulse) exactly as before.


## 4. Selection-following ball (Windows/macOS)

- Windows: a `WH_MOUSE_LL` hook records left-button down/up (drag = >4 px) and
  forwards mouse-up to a worker that settles for `selection_delay` (default
  400 ms). Text comes from UI Automation (`TextPattern.GetSelection`), password
  fields are skipped; only drags outside a terminal-class blocklist fall back to
  the synthetic Ctrl+C capture with clipboard restore.
- macOS: a listen-only `CGEventTap` feeds the same settle flow; raw
  `AXUIElementCopyAttributeValue` reads the selected text, and Cmd+C (a plain
  copy) covers drags in apps that do not expose AX.
- The ball appears near the selection anchor, commits on hover, hides after the
  commit, and auto-hides after 5 s. `auto` mode opens the card directly after
  the settle.

Both watchers share the same settle/guard pipeline, and translations are
single-flight: while one request streams, newer text only replaces a one-slot
queued follow-up, so bursts cannot interleave streams or stack engine work.


## 5. Selection toolbar (Windows/macOS)

A compact 300×44 action bar appears above the selection end after the same
settle as the ball. It follows the Yuanbao desktop pattern: the selection offers
actions first and the result opens in place only after 翻译 is chosen.

- **Actions**: 翻译 (primary, filled accent), 复制 (source text), 朗读 (original
  text through the configured local TTS engine or the system voice fallback)
  and a `−` menu with 在此应用禁用 / 关闭划词工具栏. There is deliberately no web
  search or general chat entry.
- **Window**: separate `toolbar` window, transparent, frameless, always on top,
  no taskbar entry, non-focusable (`set_focusable(false)`), so clicking a button
  cannot steal the selection or the source application's focus. Placement
  centers on the mouse-release anchor, prefers above with a 10 px gap, flips
  below when there is no room and clamps to the monitor work area.
- **Result panel**: 翻译 hides the toolbar, records the source window for
  replace-in-place, and reuses the existing card (`show_main_popup` +
  `translate_text`), including streaming, copy, retranslate, alternatives and
  replace. 复制 writes the captured text and closes after a short toast; 朗读
  uses `speechSynthesis` with the detected source language and toggles to 停止.
- **Dismissal**: clicking anywhere outside the toolbar, a new selection,
  changing the mode, or a 10 s timeout hides it. The mouse hook/tap only touches
  atomics and a channel; a worker thread performs the window hit-test and hide,
  so the low-level callback never blocks on the Tauri event loop. The `−` menu
  grows the same window downwards (44 → 128 px) instead of opening a second
  window, and collapses it again when an action or a new selection resets it.
- **Per-app opt-out**: `toolbar_disabled_apps` is a JSON string array of
  executable names (legacy comma/semicolon values are still accepted,
  case-insensitive). The `−` menu stores the current foreground executable; the
  settings page renders the list as removable chips. The app is resolved before
  the selection is read, so a disabled app is never inspected and never hits the
  synthetic Ctrl+C/Cmd+C clipboarding fallback; it does not fall back to the
  ball or direct mode. Windows resolves the executable through
  `GetWindowThreadProcessId` + `QueryFullProcessImageNameW`; macOS currently
  returns no name, so the per-app item stays hidden there.
- **Intent memory**: there is no “即划即用” toggle inside the toolbar. Selecting
  立即翻译 in the settings/tray is the equivalent and keeps the toolbar
  action-first by default.


## 6. Configuration

| key | values | default |
| --- | --- | --- |
| `selection_mode` | `off`, `ball`, `toolbar`, `auto` (toolbar/auto: Windows/macOS only) | `off` |
| `selection_method` | `ball`, `toolbar`, `auto`: remembered for the next switch-on | `toolbar` (Windows/macOS) / `ball` (Linux) |
| `selection_delay` | 100–3000 ms settle | 900 (Linux) / 400 |
| `selection_min_length` | 1–50 chars | 2 |
| `ball_visibility` | `always`, `selection` | `always` |
| `ball_x`, `ball_y` | custom dock position (physical px) | right-edge center |
| `toolbar_disabled_apps` | JSON array of executable names (legacy comma/semicolon accepted) | empty |

The settings UI intentionally exposes only `selection_mode`, the method, the
per-app opt-out list and the hotkey; the other keys are defaults for almost
everyone and are edited in the config file by power users. The settings page
structure (minimal rows plus a collapsed 关于 block) is described in
docs/ARCHITECTURE.md → UI Design Language.


## 7. Flow (Linux docked ball)

```mermaid
stateDiagram-v2
    [*] --> Idle: no usable selection (dim)
    Idle --> Armed: PRIMARY settles, >= min length
    Armed --> Busy: hover 150 ms -> ball_hover
    Busy --> Idle: translation done (card stays; dim again)
    Armed --> Idle: selection cleared / feature off
    Armed --> Armed: new selection replaces the pending text
```


## 8. Platform matrix

| platform | sensor | confirm UI | state |
| --- | --- | --- | --- |
| Linux/Wayland | XFixes events + one X11 read after quiet (AT-SPI typing filter) | docked ball | landed, GNOME Wayland manual pass passed 2026-10-04 |
| Linux/X11 | PRIMARY via X11 | docked ball | landed, X11-session manual pass pending |
| Windows | WH_MOUSE_LL + UIA | ball / toolbar / auto | landed; ball verified 2026-10-04, toolbar verified 2026-10-18 |
| macOS | CGEventTap + AX | ball / toolbar / auto | landed, cross-target compile-checked only (no Mac hardware to verify), experimental watcher; toolbar has no per-app opt-out yet |


## 9. Files

- `desktop/translator-popup-tauri/src/selection_watch.rs` — modes, watcher,
  settle/guards, platform modules.
- `desktop/translator-popup-tauri/src/main.rs` — ball/toolbar windows, dock and
  selection placement, states, hover/action commits, card placement, settings
  commands, per-app opt-out persistence.
- `desktop/translator-popup-tauri/ui/ball.{html,css,js}` — ball UI, hover dwell,
  drag handling.
- `desktop/translator-popup-tauri/ui/toolbar.{html,css,js}` — compact toolbar,
  `−` menu, copy/toast and WebView speech.
- `desktop/translator-core/src/settings.rs` — `selection_*`, `ball_*`,
  `toolbar_disabled_apps` keys and `persist_remove`.


## 10. Verification

- `cargo test` in `desktop/translator-core` and
  `desktop/translator-popup-tauri`.
- `node --check ui/main.js ui/ball.js ui/toolbar.js ui/dropdown.js`;
  `python3 -m json.tool` for the capabilities/config;
  `bash shared/sync-ui.sh --check`.
- Manual (Windows): choose 划词工具栏, select text in a browser/Notepad/VS Code
  and confirm the pill appears above the selection end without stealing focus;
  翻译 opens the streaming card, 复制 copies the source, 朗读 speaks it, and
  `− → 在此应用禁用` makes the next selection in that app do nothing; the app
  then appears in 设置 → 已禁用应用 and can be removed; clicking outside, a new
  selection, or 10 s idle hides the pill; repeat near the top edge and on a
  second monitor to check flipping/clamping.
- Manual (Linux): choose 悬浮球翻译 in 选中文字后, confirm the ball appears at
  the right edge and is dim; select text → it lights up; move onto it → after
  ~150 ms the card opens beside it with the translation; hovering again
  re-shows the card without retranslating; drag the ball somewhere else and
  reload to confirm the position persists, then drop it back near the default
  dock and reload to confirm it reset; check that selecting text is never
  interrupted, and that typing in an input only lights the ball in apps without
  AT-SPI text events (e.g. VS Code with accessibility off).
