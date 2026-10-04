//! Selection watcher for 划词翻译.
//!
//! The watcher observes the user's selection while the feature is enabled and
//! either shows the floating ball (`ball`) or opens the translation card
//! directly (`auto`). It never writes the clipboard and never synthesizes
//! keys; the platform readers only inspect the current selection.
//!
//! Linux subscribes to XFixes selection-owner updates and reads PRIMARY only
//! once the selection has been quiet for the settle delay. GNOME mirrors every
//! Wayland selection update to X11 (XFixes events fire on each update, verified
//! on GNOME 50), so this replaces the earlier 400 ms `wl-paste` polling that
//! disturbed native selection drags; only a session without an X display falls
//! back to slow `wl-paste` polling. Windows and macOS use native mouse hooks (a
//! `WH_MOUSE_LL` hook and a listen-only `CGEventTap`) feeding the same
//! settle/guard flow. The
//! platform readers inspect the current selection; the synthetic copy only
//! runs as a last resort (Windows drags outside terminals, macOS drags without
//! AX text) and Linux never touches the clipboard at all.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::AppHandle;

/// Linux sees no mouse-release signal (native Wayland especially), so a
/// selection needs a longer settle before it counts as committed; the hook
/// platforms see the actual mouse-up and can stay snappier.
#[cfg(target_os = "linux")]
pub const DEFAULT_DELAY_MS: u64 = 900;
#[cfg(not(target_os = "linux"))]
pub const DEFAULT_DELAY_MS: u64 = 400;
pub const DEFAULT_MIN_LENGTH: usize = 2;
pub const MIN_DELAY_MS: u64 = 100;
pub const MAX_DELAY_MS: u64 = 3000;
pub const MIN_LENGTH: usize = 1;
pub const MAX_LENGTH: usize = 50;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SelectionMode {
    Off,
    Ball,
    Auto,
}

impl SelectionMode {
    /// Unknown values fall back to `Off` so a hand-edited config file cannot
    /// turn the watcher on by accident.
    pub fn parse(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            Some("ball") => Self::Ball,
            Some("auto") => Self::Auto,
            _ => Self::Off,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Ball => "ball",
            Self::Auto => "auto",
        }
    }
}

/// When the docked ball is on screen: always (dim without a selection) or
/// only after a selection was armed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BallVisibility {
    Always,
    Selection,
}

impl BallVisibility {
    /// `None` for unknown/missing values, so callers can fall back to the
    /// default (config) or reject the input (settings command).
    pub fn parse(value: Option<&str>) -> Option<Self> {
        match value.map(str::trim) {
            Some("always") => Some(Self::Always),
            Some("selection") => Some(Self::Selection),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Always => "always",
            Self::Selection => "selection",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SelectionSettings {
    pub mode: SelectionMode,
    pub delay: Duration,
    pub min_length: usize,
    pub ball_visibility: BallVisibility,
}

impl SelectionSettings {
    pub fn from_config(config: &translator_core::settings::FileConfig) -> Self {
        let delay_ms = config
            .selection_delay
            .as_deref()
            .and_then(|value| value.trim().parse::<u64>().ok())
            .unwrap_or(DEFAULT_DELAY_MS)
            .clamp(MIN_DELAY_MS, MAX_DELAY_MS);

        let min_length = config
            .selection_min_length
            .as_deref()
            .and_then(|value| value.trim().parse::<usize>().ok())
            .unwrap_or(DEFAULT_MIN_LENGTH)
            .clamp(MIN_LENGTH, MAX_LENGTH);

        let mode = SelectionMode::parse(config.selection_mode.as_deref());

        // Direct translation has no mouse-release signal on Linux and is not
        // offered in the UI there; an old config value falls back to the ball.
        #[cfg(target_os = "linux")]
        let mode = if mode == SelectionMode::Auto {
            SelectionMode::Ball
        } else {
            mode
        };

        let ball_visibility =
            BallVisibility::parse(config.ball_visibility.as_deref()).unwrap_or(BallVisibility::Always);

        Self {
            mode,
            delay: Duration::from_millis(delay_ms),
            min_length,
            ball_visibility,
        }
    }

    pub fn delay_ms(self) -> u64 {
        self.delay.as_millis() as u64
    }

    /// Whether a settled selection is worth translating at all.
    pub fn accepts(self, text: &str) -> bool {
        self.mode != SelectionMode::Off && text.chars().count() >= self.min_length
    }
}

/// Whether the current platform has a selection watcher. All three desktop
/// targets are implemented; see docs/SELECTION_TRANSLATION.md.
pub fn selection_supported() -> bool {
    cfg!(any(
        target_os = "linux",
        target_os = "windows",
        target_os = "macos"
    ))
}

/// Whether the platform has a selection ball: a fixed docked ball on Linux
/// (the pointer anchor is unreliable on native Wayland) and a
/// selection-following ball on Windows/macOS.
pub fn ball_supported() -> bool {
    cfg!(any(
        target_os = "linux",
        target_os = "windows",
        target_os = "macos"
    ))
}

/// The Linux ball is docked at a fixed, draggable position; the hook platforms
/// keep the ball next to the selection.
pub fn ball_docked() -> bool {
    cfg!(target_os = "linux")
}

/// Direct translation on selection needs a mouse-release signal to be safe;
/// Linux watchers only have PRIMARY polling, so the UI does not offer it there.
pub fn auto_supported() -> bool {
    #[cfg(target_os = "linux")]
    {
        false
    }
    #[cfg(not(target_os = "linux"))]
    {
        true
    }
}

/// Mirror of "the feature is enabled", checked by the native hook/tap
/// callbacks so they stay near-free while `selection_mode = off` (the global
/// hook itself is only installed the first time the feature is enabled).
static WATCHER_ACTIVE: AtomicBool = AtomicBool::new(false);
static WATCHER_STARTED: AtomicBool = AtomicBool::new(false);

/// Starts the platform watcher the first time the feature is enabled and
/// mirrors the current mode into [`WATCHER_ACTIVE`]. Called from setup and
/// after every selection-mode change.
pub fn ensure(app: AppHandle) {
    if !selection_supported() {
        return;
    }

    let mode = settings(&app).mode;
    WATCHER_ACTIVE.store(mode != SelectionMode::Off, Ordering::SeqCst);

    if mode == SelectionMode::Off || WATCHER_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }

    #[cfg(target_os = "linux")]
    std::thread::spawn(move || linux::run(app));

    #[cfg(target_os = "windows")]
    windows::spawn(app);

    #[cfg(target_os = "macos")]
    {
        if !macos::spawn(app) {
            // The tap needs the accessibility permission; leave the one-shot
            // flag unset so a later settings change can retry the install.
            WATCHER_STARTED.store(false, Ordering::SeqCst);
        }
    }
}

/// Live 划词 settings (updated by the settings commands).
fn settings(app: &AppHandle) -> SelectionSettings {
    use tauri::Manager;

    *app.state::<crate::AppState>().selection.lock().unwrap()
}

/// Guards shared by the platform watchers: a pinned card, one of our own
/// windows focused (the user is selecting inside the card) or a recent hotkey
/// trigger all silence the watcher.
#[cfg(any(
    target_os = "linux",
    target_os = "windows",
    target_os = "macos"
))]
fn blocked(app: &AppHandle) -> bool {
    use tauri::Manager;

    let state = app.state::<crate::AppState>();

    if *state.pinned.lock().unwrap() {
        return true;
    }

    // Selections made inside our own visible card must not trigger the
    // watcher. `is_focused` is read from the runtime's cached focus state, so
    // a hidden card can never leave stale focus behind. The ball is
    // intentionally non-focusable and has no selectable content, so it is not
    // part of this check (some WMs still report it as active, which used to
    // make the watcher block itself).
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) && window.is_focused().unwrap_or(false) {
            return true;
        }
    }

    if let Some(until) = *state.suppress_until.lock().unwrap() {
        if std::time::Instant::now() < until {
            return true;
        }
    }

    false
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn handle(
    app: &AppHandle,
    settings: SelectionSettings,
    text: String,
    anchor: Option<(i32, i32)>,
    window: Option<isize>,
) {
    use tauri::Manager;

    if settings.mode == SelectionMode::Ball && ball_supported() {
        if let Some(anchor) = anchor {
            crate::show_ball(app, text, anchor, window);
            return;
        }
    }

    // `ball` on Wayland (no global anchor) and any unknown future platform
    // degrades to the direct path instead of silently doing nothing.
    crate::hide_ball(app);
    *app.state::<crate::AppState>().replace_window.lock().unwrap() = window;
    crate::show_main_popup(app);
    crate::translate_text(app, text);
}

/// A left-button release that may have completed a selection (Windows/macOS).
#[cfg(any(target_os = "windows", target_os = "macos"))]
struct MouseUp {
    x: i32,
    y: i32,
    dragged: bool,
}

/// Shared settle/guard pipeline for the hook/tap platforms: wait until the
/// mouse has been quiet for the configured delay, read the selection through
/// the platform reader, then apply the same dedupe and guards as Linux.
#[cfg(any(target_os = "windows", target_os = "macos"))]
fn run_events(
    app: AppHandle,
    receiver: std::sync::mpsc::Receiver<MouseUp>,
    read_selection: fn(&MouseUp) -> Option<String>,
) {
    use std::sync::mpsc::RecvTimeoutError;

    let mut last_seen: Option<String> = None;

    loop {
        let Ok(mut event) = receiver.recv() else {
            return;
        };

        let delay = settings(&app).delay;

        loop {
            match receiver.recv_timeout(delay) {
                Ok(newer) => event = newer,
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }

        let Some(text) = read_selection(&event) else {
            // An empty selection resets the dedupe, so re-selecting the same
            // text later triggers again.
            last_seen = None;
            continue;
        };

        if last_seen.as_deref() == Some(text.as_str()) {
            continue;
        }

        last_seen = Some(text.clone());

        // Re-read after the (possibly slow) cross-process read: a hotkey
        // suppression or a card focus that happened meanwhile must be honored.
        let settings = settings(&app);

        if settings.mode == SelectionMode::Off || !settings.accepts(&text) || blocked(&app) {
            continue;
        }

        let window = crate::capture::foreground_window();
        handle(&app, settings, text, Some((event.x, event.y)), window);
    }
}

/// Drag-gated synthetic-copy fallback shared by Windows and macOS; callers
/// decide when it is safe (Windows also blocks terminals).
#[cfg(any(target_os = "windows", target_os = "macos"))]
fn capture_fallback(event: &MouseUp) -> Option<String> {
    if !event.dragged {
        return None;
    }

    crate::capture::capture_selection()
        .ok()
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

/// AT-SPI focus tracking (Linux): many toolkits mirror typed text to PRIMARY,
/// so the watcher asks the accessibility layer whether the focused text object
/// really has a selection. Best-effort: without a connection, a focused object
/// or a Text interface the result is "unknown" and the watcher keeps its
/// PRIMARY-only behaviour.
#[cfg(target_os = "linux")]
mod atspi {
    use std::sync::{LazyLock, Mutex, OnceLock};
    use std::time::{Duration, Instant};

    use atspi::AccessibilityConnection;
    use atspi::events::ObjectEvents;
    use atspi::events::object::{StateChangedEvent, TextCaretMovedEvent, TextChangedEvent, TextSelectionChangedEvent};
    use atspi::proxy::accessible::ObjectRefExt;
    use atspi::proxy::proxy_ext::ProxyExt;
    use atspi::ObjectRefOwned;
    use futures_lite::stream::StreamExt;

    /// A stale "no selection" verdict must not survive long: after a few
    /// seconds without AT-SPI events the watcher falls back to PRIMARY.
    const FRESHNESS: Duration = Duration::from_secs(5);

    struct Verdict {
        selection: Option<bool>,
        updated: Instant,
    }

    static STATE: LazyLock<Mutex<Verdict>> = LazyLock::new(|| {
        Mutex::new(Verdict {
            selection: None,
            updated: Instant::now(),
        })
    });
    /// When the last `TextSelectionChanged` event was seen; caret/text events
    /// shortly after a selection change must not flip the verdict to "typing".
    static LAST_SELECTION: LazyLock<Mutex<Option<Instant>>> = LazyLock::new(|| Mutex::new(None));
    static FOCUSED: Mutex<Option<ObjectRefOwned>> = Mutex::new(None);
    static CONNECTION: OnceLock<AccessibilityConnection> = OnceLock::new();

    /// How long a selection-change event keeps "typing" inference at bay.
    const SELECTION_GRACE: Duration = Duration::from_millis(1000);

    pub fn spawn() {
        std::thread::spawn(|| tauri::async_runtime::block_on(run()));
    }

    async fn run() {
        if crate::selection_debug() {
            eprintln!("SELDBG atspi thread start");
        }

        let connection = match AccessibilityConnection::new().await {
            Ok(connection) => connection,
            Err(error) => {
                if crate::selection_debug() {
                    eprintln!("SELDBG atspi connect failed: {error}");
                }

                return;
            }
        };

        if let Err(error) = connection.register_event::<ObjectEvents>().await {
            if crate::selection_debug() {
                eprintln!("SELDBG atspi register failed: {error}");
            }

            return;
        }

        let _ = CONNECTION.set(connection.clone());
        let mut events = connection.event_stream();

        if crate::selection_debug() {
            eprintln!("SELDBG atspi connected");
        }

        while let Some(Ok(event)) = events.next().await {
            if let Ok(change) = StateChangedEvent::try_from(event.clone()) {
                if change.state == "focused".into() {
                    if change.enabled {
                        *FOCUSED.lock().unwrap() = Some(change.item.clone());
                        update(&connection, &change.item, "focus").await;
                    } else {
                        let is_current =
                            FOCUSED.lock().unwrap().as_ref() == Some(&change.item);

                        if is_current {
                            *FOCUSED.lock().unwrap() = None;
                        }
                    }
                }

                continue;
            }

            // Text events also reveal the active text object (covers focus
            // events missed before the listener started).
            if let Ok(selection) = TextSelectionChangedEvent::try_from(event.clone()) {
                *FOCUSED.lock().unwrap() = Some(selection.item.clone());
                update(&connection, &selection.item, "selection").await;
                continue;
            }

            if let Ok(caret) = TextCaretMovedEvent::try_from(event.clone()) {
                *FOCUSED.lock().unwrap() = Some(caret.item.clone());
                update(&connection, &caret.item, "caret").await;
                continue;
            }

            if let Ok(changed) = TextChangedEvent::try_from(event.clone()) {
                update(&connection, &changed.item, "text").await;
            }
        }
    }

    async fn update(connection: &AccessibilityConnection, item: &ObjectRefOwned, kind: &str) {
        let verdict = selection_state(connection, item).await;
        let now = Instant::now();

        if kind == "selection" {
            *LAST_SELECTION.lock().unwrap() = Some(now);
        }

        let mut state = STATE.lock().unwrap();

        match verdict {
            Some(value) => state.selection = Some(value),
            None => match kind {
                // A selection-change event is strong evidence even when the
                // element exposes no queryable Text interface.
                "selection" => state.selection = Some(true),
                // Caret/text updates without a nearby selection change are
                // the typing signature (Electron apps expose no Text
                // interface, but do report these).
                "caret" | "text" => {
                    let recent_selection = LAST_SELECTION
                        .lock()
                        .unwrap()
                        .map(|at| at.elapsed() < SELECTION_GRACE)
                        .unwrap_or(false);

                    if !recent_selection {
                        state.selection = Some(false);
                    }
                }
                // Focus of an unqueryable element: unknown, fall back.
                _ => state.selection = None,
            },
        }

        state.updated = now;

        if crate::selection_debug() {
            eprintln!("SELDBG atspi event={kind} selection={:?}", state.selection);
        }
    }

    async fn selection_state(
        connection: &AccessibilityConnection,
        item: &ObjectRefOwned,
    ) -> Option<bool> {
        let text = item
            .clone()
            .into_accessible_proxy(connection.connection())
            .await
            .ok()?
            .proxies()
            .await
            .ok()?
            .text()
            .await
            .ok()?;

        let count = text.get_n_selections().await.ok()?;

        if count <= 0 {
            return Some(false);
        }

        let (start, end) = text.get_selection(0).await.ok()?;
        let selected = text.get_text(start, end).await.ok()?;

        Some(!selected.trim().is_empty())
    }

    /// `false` only when AT-SPI positively says the focused text object has no
    /// selection (typing/caret only); unknown allows the old behaviour.
    pub fn selection_ok() -> bool {
        let Ok(state) = STATE.lock() else {
            return true;
        };

        if state.updated.elapsed() > FRESHNESS {
            return true;
        }

        state.selection != Some(false)
    }
}

/// See [`atspi::selection_ok`].
#[cfg(target_os = "linux")]
pub(crate) fn atspi_selection_ok() -> bool {
    atspi::selection_ok()
}

#[cfg(target_os = "linux")]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Session {
    X11,
    Wayland,
    Unknown,
}

#[cfg(target_os = "linux")]
fn session() -> Session {
    match std::env::var("XDG_SESSION_TYPE").as_deref().map(str::trim) {
        Ok("x11") => Session::X11,
        Ok("wayland") => Session::Wayland,
        _ => match (
            std::env::var_os("DISPLAY").is_some(),
            std::env::var_os("WAYLAND_DISPLAY").is_some(),
        ) {
            (true, false) => Session::X11,
            (false, true) => Session::Wayland,
            _ => Session::Unknown,
        },
    }
}

/// Reads the current PRIMARY selection on demand (used by the hover commit):
/// the user explicitly confirmed, so there is no need to wait for the settle
/// timer, and a fresh selection works even before it was armed.
#[cfg(target_os = "linux")]
pub(crate) fn read_primary_now() -> Option<String> {
    read_primary()
        .ok()
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

#[cfg(target_os = "linux")]
fn read_primary() -> Result<String, String> {
    // Diagnostic switch: keep the watcher running but never touch the
    // selection, to isolate PRIMARY polling as an interference source.
    if std::env::var_os("TRANSLATOR_SELECTION_NO_PRIMARY").is_some() {
        return Err("disabled".to_string());
    }

    match session() {
        Session::X11 => crate::capture::primary_selection_x11(),
        // Under Wayland the XWayland bridge serves the same PRIMARY selection
        // and reading it over X11 avoids spawning `wl-paste` every poll, which
        // was shown to disturb native selection drags on GNOME. Only fall back
        // to `wl-paste` when there is no X display at all.
        Session::Wayland if std::env::var_os("DISPLAY").is_some() => {
            crate::capture::primary_selection_x11()
        }
        Session::Wayland => crate::capture::primary_selection_wayland(),
        Session::Unknown => crate::capture::capture_selection(),
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::time::{Duration, Instant};

    use tauri::AppHandle;
    use x11rb::connection::Connection;
    use x11rb::protocol::xfixes::{ConnectionExt as _, SelectionEventMask};
    use x11rb::protocol::xproto::{Atom, ConnectionExt as _};
    use x11rb::rust_connection::RustConnection;

    use super::{SelectionMode, blocked, read_primary, settings};

    /// How often the event connection is drained; events arrive
    /// asynchronously, so this only bounds the reaction latency.
    const TICK: Duration = Duration::from_millis(100);
    /// Without an X display there are no selection events; fall back to a slow
    /// `wl-paste` poll (last resort: it can disturb native drags).
    const FALLBACK_INTERVAL: Duration = Duration::from_millis(1500);

    pub fn run(app: AppHandle) {
        super::atspi::spawn();

        match watch_connection() {
            Some((conn, primary)) => run_xfixes(app, conn, primary),
            None => run_fallback(app),
        }
    }

    /// Subscribe to XFixes selection-owner updates for PRIMARY. Every Wayland
    /// selection update is mirrored to X11 (verified on GNOME), so this
    /// replaces polling: the watcher only *reads* the selection once it has
    /// been quiet for the settle delay, which keeps native drags undisturbed.
    fn watch_connection() -> Option<(RustConnection, Atom)> {
        let (conn, screen_num) = x11rb::connect(None).ok()?;
        let root = conn.setup().roots[screen_num].root;
        let primary = conn
            .intern_atom(false, b"PRIMARY")
            .ok()?
            .reply()
            .ok()?
            .atom;

        conn.xfixes_query_version(5, 0).ok()?.reply().ok()?;
        conn.xfixes_select_selection_input(root, primary, SelectionEventMask::SET_SELECTION_OWNER)
            .ok()?;
        conn.flush().ok()?;

        if crate::selection_debug() {
            eprintln!("SELDBG watcher xfixes ready");
        }

        Some((conn, primary))
    }

    fn run_xfixes(app: AppHandle, conn: RustConnection, primary: Atom) {
        let mut last_change: Option<Instant> = None;
        let mut last_seen: Option<String> = None;

        loop {
            while let Ok(Some(event)) = conn.poll_for_event() {
                if let x11rb::protocol::Event::XfixesSelectionNotify(event) = event {
                    if event.selection == primary {
                        last_change = Some(Instant::now());

                        if crate::selection_debug() {
                            eprintln!("SELDBG xfix event");
                        }
                    }
                }
            }

            std::thread::sleep(TICK);

            let settings = settings(&app);

            if settings.mode == SelectionMode::Off {
                last_change = None;
                last_seen = None;
                crate::set_selection_pending(&app, None);
                continue;
            }

            let Some(changed_at) = last_change else {
                continue;
            };

            if changed_at.elapsed() < settings.delay {
                continue;
            }

            // Quiet: this is the only selection read, and it happens after the
            // user stopped changing the selection.
            last_change = None;
            let text = read_primary().unwrap_or_default();
            apply_candidate(&app, text, &mut last_seen);
        }
    }

    fn run_fallback(app: AppHandle) {
        let mut last_seen: Option<String> = None;

        loop {
            std::thread::sleep(FALLBACK_INTERVAL);

            let settings = settings(&app);

            if settings.mode == SelectionMode::Off {
                last_seen = None;
                crate::set_selection_pending(&app, None);
                continue;
            }

            let text = read_primary().unwrap_or_default();
            apply_candidate(&app, text, &mut last_seen);
        }
    }

    /// Dedupe + guards + arm; shared so the event and fallback loops behave
    /// identically.
    fn apply_candidate(app: &AppHandle, text: String, last_seen: &mut Option<String>) {
        if text.trim().is_empty() {
            *last_seen = None;
            crate::set_selection_pending(app, None);
            return;
        }

        if last_seen.as_deref() == Some(text.as_str()) {
            return;
        }

        *last_seen = Some(text.clone());

        // Typing in an input mirrors the text to PRIMARY on many toolkits; the
        // accessibility layer tells a real selection apart from a caret.
        if !super::atspi::selection_ok() {
            if crate::selection_debug() {
                eprintln!("SELDBG skip atspi (no text selection)");
            }

            return;
        }

        let settings = settings(app);

        if !settings.accepts(&text) || blocked(app) {
            if crate::selection_debug() {
                eprintln!(
                    "SELDBG skip accepts={} blocked={}",
                    settings.accepts(&text),
                    blocked(app)
                );
            }

            return;
        }

        if crate::selection_debug() {
            eprintln!("SELDBG arm len={}", text.chars().count());
        }

        let window = crate::capture::foreground_window();
        crate::set_selection_pending(app, Some(crate::PendingSelection { text, window }));
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use std::cell::RefCell;
    use std::sync::atomic::{AtomicI32, Ordering};
    use std::sync::mpsc::{self, Sender};
    use std::sync::OnceLock;

    use tauri::AppHandle;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationTextPattern, UIA_TextPatternId,
    };
    use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetClassNameW, GetForegroundWindow, GetMessageW,
        SetWindowsHookExW, TranslateMessage, MSLLHOOKSTRUCT, MSG, WH_MOUSE_LL, WM_LBUTTONDOWN,
        WM_LBUTTONUP,
    };

    use super::{MouseUp, WATCHER_ACTIVE, capture_fallback, run_events};

    /// What the UI Automation probe found for the focused element.
    enum Probe {
        Text(String),
        /// A text pattern exists but holds no selection.
        Empty,
        /// A password element: the synthetic copy fallback must never run.
        Blocked,
        /// No usable text pattern (apps without UI Automation support).
        NoProvider,
    }

    static MOUSE_EVENTS: OnceLock<Sender<MouseUp>> = OnceLock::new();
    static DOWN_X: AtomicI32 = AtomicI32::new(0);
    static DOWN_Y: AtomicI32 = AtomicI32::new(0);

    /// Low-level mouse hook. It must only record the event and hand it to the
    /// worker thread: Windows silently unhooks a callback that takes too long.
    unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 && WATCHER_ACTIVE.load(Ordering::Relaxed) {
            let message = wparam as u32;

            if message == WM_LBUTTONDOWN {
                let info = unsafe { &*(lparam as *const MSLLHOOKSTRUCT) };
                DOWN_X.store(info.pt.x, Ordering::Relaxed);
                DOWN_Y.store(info.pt.y, Ordering::Relaxed);
            } else if message == WM_LBUTTONUP {
                let info = unsafe { &*(lparam as *const MSLLHOOKSTRUCT) };
                let dx = info.pt.x - DOWN_X.load(Ordering::Relaxed);
                let dy = info.pt.y - DOWN_Y.load(Ordering::Relaxed);
                let dragged = dx * dx + dy * dy > 16;

                if let Some(sender) = MOUSE_EVENTS.get() {
                    let _ = sender.send(MouseUp {
                        x: info.pt.x,
                        y: info.pt.y,
                        dragged,
                    });
                }
            }
        }

        unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
    }

    pub fn spawn(app: AppHandle) {
        let (sender, receiver) = mpsc::channel::<MouseUp>();

        if MOUSE_EVENTS.set(sender).is_err() {
            return;
        }

        // The hook lives on its own thread with a message pump for the app
        // lifetime; the worker then settles and reads the selection.
        std::thread::spawn(move || unsafe {
            let module =
                windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null());
            let hook = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), module, 0);

            if hook.is_null() {
                eprintln!("failed to install the selection mouse hook");
                return;
            }

            let mut message: MSG = std::mem::zeroed();
            while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        });

        std::thread::spawn(move || run_events(app, receiver, read_selection));
    }

    /// UI Automation first (works in browsers, Office and most edit controls
    /// without touching the clipboard). The synthetic copy only runs when the
    /// focused element has no text provider at all — never for password
    /// elements and never in a terminal, where Ctrl+C is SIGINT.
    fn read_selection(event: &MouseUp) -> Option<String> {
        match probe_selection() {
            Probe::Text(text) => Some(text),
            Probe::Empty | Probe::Blocked => None,
            Probe::NoProvider => {
                if terminal_in_front() {
                    return None;
                }

                capture_fallback(event)
            }
        }
    }

    /// One COM apartment and one UI Automation client per worker thread; the
    /// thread lives for the whole app, so there is nothing to uninitialize.
    fn with_automation<R>(f: impl FnOnce(&IUIAutomation) -> Option<R>) -> Option<R> {
        thread_local! {
            static AUTOMATION: RefCell<Option<IUIAutomation>> = const { RefCell::new(None) };
        }

        AUTOMATION.with(|cell| {
            if cell.borrow().is_none() {
                unsafe {
                    let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
                }

                let automation: IUIAutomation =
                    unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_ALL) }.ok()?;

                *cell.borrow_mut() = Some(automation);
            }

            let automation = cell.borrow();
            let automation = automation.as_ref()?;

            f(automation)
        })
    }

    fn probe_selection() -> Probe {
        with_automation(|automation| unsafe {
            let element = automation.GetFocusedElement().ok()?;

            if element
                .CurrentIsPassword()
                .map(|value| value.as_bool())
                .unwrap_or(false)
            {
                return Some(Probe::Blocked);
            }

            let pattern: IUIAutomationTextPattern =
                element.GetCurrentPatternAs(UIA_TextPatternId).ok()?;
            let ranges = pattern.GetSelection().ok()?;
            let count = ranges.Length().ok()?;
            let mut text = String::new();

            for index in 0..count {
                let range = ranges.GetElement(index).ok()?;
                text.push_str(&range.GetText(-1).ok()?.to_string());
            }

            let text = text.trim().to_string();

            Some(if text.is_empty() {
                Probe::Empty
            } else {
                Probe::Text(text)
            })
        })
        .unwrap_or(Probe::NoProvider)
    }

    /// Known terminal window classes: the clipboard fallback must never send
    /// Ctrl+C there because a terminal without a selection interprets it as
    /// SIGINT for the foreground process.
    fn terminal_in_front() -> bool {
        const TERMINALS: [&str; 8] = [
            "ConsoleWindowClass",
            "CASCADIA_HOSTING_WINDOW_CLASS",
            "mintty",
            "PuTTY",
            "Alacritty",
            "org.wezfurlong.wezterm",
            "kitty",
            "wezterm",
        ];

        let window = unsafe { GetForegroundWindow() };

        if window.is_null() {
            return false;
        }

        let mut buffer = [0u16; 128];
        let length = unsafe { GetClassNameW(window, buffer.as_mut_ptr(), buffer.len() as i32) };

        if length <= 0 {
            return false;
        }

        let class = String::from_utf16_lossy(&buffer[..length as usize]);
        TERMINALS.iter().any(|name| class.eq_ignore_ascii_case(name))
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::sync::atomic::{AtomicI32, Ordering};
    use std::sync::mpsc::{self, Sender};
    use std::sync::OnceLock;

    use core_foundation::base::CFTypeRef;
    use core_foundation::runloop::CFRunLoop;
    use core_foundation::string::CFStringRef;
    use core_graphics::event::{
        CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
        CallbackResult,
    };
    use tauri::AppHandle;

    use super::{MouseUp, WATCHER_ACTIVE, capture_fallback, run_events};

    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn AXIsProcessTrusted() -> u8;
        fn AXUIElementCreateSystemWide() -> CFTypeRef;
        fn AXUIElementCopyAttributeValue(
            element: CFTypeRef,
            attribute: CFStringRef,
            value: *mut CFTypeRef,
        ) -> i32;
        static kAXFocusedUIElementAttribute: CFStringRef;
        static kAXSelectedTextAttribute: CFStringRef;
    }

    static MOUSE_EVENTS: OnceLock<Sender<MouseUp>> = OnceLock::new();
    static DOWN_X: AtomicI32 = AtomicI32::new(0);
    static DOWN_Y: AtomicI32 = AtomicI32::new(0);

    /// Returns false when the accessibility permission is missing, so the
    /// caller can retry the tap install on the next settings change.
    pub fn spawn(app: AppHandle) -> bool {
        if unsafe { AXIsProcessTrusted() } == 0 {
            return false;
        }

        let (sender, receiver) = mpsc::channel::<MouseUp>();

        if MOUSE_EVENTS.set(sender).is_err() {
            return true;
        }

        std::thread::spawn(move || {
            let result = CGEventTap::with_enabled(
                CGEventTapLocation::Session,
                CGEventTapPlacement::HeadInsertEventTap,
                CGEventTapOptions::ListenOnly,
                vec![CGEventType::LeftMouseDown, CGEventType::LeftMouseUp],
                |_proxy, event_type, event| {
                    if !WATCHER_ACTIVE.load(Ordering::Relaxed) {
                        return CallbackResult::Keep;
                    }

                    let location = event.location();

                    match event_type {
                        CGEventType::LeftMouseDown => {
                            DOWN_X.store(location.x as i32, Ordering::Relaxed);
                            DOWN_Y.store(location.y as i32, Ordering::Relaxed);
                        }
                        CGEventType::LeftMouseUp => {
                            let dx = location.x as i32 - DOWN_X.load(Ordering::Relaxed);
                            let dy = location.y as i32 - DOWN_Y.load(Ordering::Relaxed);

                            if let Some(sender) = MOUSE_EVENTS.get() {
                                let _ = sender.send(MouseUp {
                                    x: location.x as i32,
                                    y: location.y as i32,
                                    dragged: dx * dx + dy * dy > 16,
                                });
                            }
                        }
                        _ => {}
                    }

                    CallbackResult::Keep
                },
                CFRunLoop::run_current,
            );

            if result.is_err() {
                eprintln!(
                    "failed to install the selection event tap (accessibility permission missing?)"
                );
            }
        });

        std::thread::spawn(move || run_events(app, receiver, read_selection));
        true
    }

    /// AX first (any selection, no clipboard); Cmd+C is a plain copy on macOS,
    /// so the drag fallback cannot send a signal to a terminal.
    fn read_selection(event: &MouseUp) -> Option<String> {
        if let Some(text) = ax_selected_text() {
            return Some(text);
        }

        capture_fallback(event)
    }

    fn ax_selected_text() -> Option<String> {
        use core_foundation::base::{CFRelease, TCFType};
        use core_foundation::string::CFString;

        unsafe {
            if AXIsProcessTrusted() == 0 {
                return None;
            }

            let system = AXUIElementCreateSystemWide();

            if system.is_null() {
                return None;
            }

            let mut focused: CFTypeRef = std::ptr::null();
            let focused_status =
                AXUIElementCopyAttributeValue(system, kAXFocusedUIElementAttribute, &mut focused);
            CFRelease(system);

            if focused_status != 0 || focused.is_null() {
                return None;
            }

            let mut value: CFTypeRef = std::ptr::null();
            let value_status =
                AXUIElementCopyAttributeValue(focused, kAXSelectedTextAttribute, &mut value);
            CFRelease(focused);

            if value_status != 0 || value.is_null() {
                return None;
            }

            let text = CFString::wrap_under_create_rule(value as CFStringRef)
                .to_string()
                .trim()
                .to_string();

            (!text.is_empty()).then_some(text)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use translator_core::settings::FileConfig;

    #[test]
    fn parses_selection_modes() {
        assert_eq!(SelectionMode::parse(Some("ball")), SelectionMode::Ball);
        assert_eq!(SelectionMode::parse(Some(" auto ")), SelectionMode::Auto);
        assert_eq!(SelectionMode::parse(Some("off")), SelectionMode::Off);
        assert_eq!(SelectionMode::parse(Some("bogus")), SelectionMode::Off);
        assert_eq!(SelectionMode::parse(None), SelectionMode::Off);
    }

    #[test]
    fn config_defaults_to_off_with_settle_defaults() {
        let settings = SelectionSettings::from_config(&FileConfig::default());

        assert_eq!(settings.mode, SelectionMode::Off);
        assert_eq!(settings.delay_ms(), DEFAULT_DELAY_MS);
        assert_eq!(settings.min_length, DEFAULT_MIN_LENGTH);
    }

    #[test]
    fn config_values_are_parsed_and_clamped() {
        let config = FileConfig {
            selection_mode: Some("ball".to_string()),
            selection_delay: Some("900".to_string()),
            selection_min_length: Some("4".to_string()),
            ..FileConfig::default()
        };

        let settings = SelectionSettings::from_config(&config);
        assert_eq!(settings.mode, SelectionMode::Ball);
        assert_eq!(settings.delay_ms(), 900);
        assert_eq!(settings.min_length, 4);

        let config = FileConfig {
            selection_delay: Some("5".to_string()),
            selection_min_length: Some("9999".to_string()),
            ..FileConfig::default()
        };

        let settings = SelectionSettings::from_config(&config);
        assert_eq!(settings.delay_ms(), MIN_DELAY_MS);
        assert_eq!(settings.min_length, MAX_LENGTH);

        let config = FileConfig {
            selection_delay: Some("99999".to_string()),
            selection_min_length: Some("0".to_string()),
            ..FileConfig::default()
        };

        let settings = SelectionSettings::from_config(&config);
        assert_eq!(settings.delay_ms(), MAX_DELAY_MS);
        assert_eq!(settings.min_length, MIN_LENGTH);
    }

    #[test]
    fn parses_ball_visibility() {
        assert_eq!(
            BallVisibility::parse(Some("always")),
            Some(BallVisibility::Always)
        );
        assert_eq!(
            BallVisibility::parse(Some(" selection ")),
            Some(BallVisibility::Selection)
        );
        assert_eq!(BallVisibility::parse(Some("bogus")), None);
        assert_eq!(BallVisibility::parse(None), None);
    }

    #[test]
    fn accepts_only_matching_mode_and_length() {
        let off = SelectionSettings {
            mode: SelectionMode::Off,
            delay: Duration::from_millis(DEFAULT_DELAY_MS),
            min_length: 2,
            ball_visibility: BallVisibility::Always,
        };
        assert!(!off.accepts("hello"));

        let auto = SelectionSettings {
            mode: SelectionMode::Auto,
            ..off
        };
        assert!(!auto.accepts("a"));
        assert!(auto.accepts("ab"));
        // CJK characters count as single characters.
        assert!(auto.accepts("你好"));
        assert!(auto.accepts("  hi  "));
    }
}
