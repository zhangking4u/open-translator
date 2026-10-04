//! Selection watcher for 划词翻译.
//!
//! The watcher observes the user's selection while the feature is enabled and
//! either shows the floating ball (`ball`) or opens the translation card
//! directly (`auto`). It never writes the clipboard and never synthesizes
//! keys; the platform readers only inspect the current selection.
//!
//! Linux polls the PRIMARY selection because GNOME/Wayland offers no selection
//! change events to regular clients (`wl-paste --watch` needs the wlroots
//! data-control protocol) and because the same loop then works on X11 and
//! Wayland. Windows and macOS use native mouse hooks (a `WH_MOUSE_LL` hook and
//! a listen-only `CGEventTap`) feeding the same settle/guard flow. The
//! platform readers inspect the current selection; the synthetic copy only
//! runs as a last resort (Windows drags outside terminals, macOS drags without
//! AX text) and Linux never touches the clipboard at all.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::AppHandle;

pub const DEFAULT_DELAY_MS: u64 = 400;
pub const DEFAULT_MIN_LENGTH: usize = 2;
pub const MIN_DELAY_MS: u64 = 100;
pub const MAX_DELAY_MS: u64 = 3000;
pub const MIN_LENGTH: usize = 1;
pub const MAX_LENGTH: usize = 50;

#[cfg(target_os = "linux")]
const POLL_INTERVAL: Duration = Duration::from_millis(400);

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

#[derive(Clone, Copy, Debug)]
pub struct SelectionSettings {
    pub mode: SelectionMode,
    pub delay: Duration,
    pub min_length: usize,
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

        Self {
            mode: SelectionMode::parse(config.selection_mode.as_deref()),
            delay: Duration::from_millis(delay_ms),
            min_length,
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

/// Whether the platform can anchor the ball next to the selection. Native
/// Wayland has no global pointer position and no window placement, so the ball
/// degrades to direct translation there.
pub fn ball_supported() -> bool {
    #[cfg(target_os = "linux")]
    {
        matches!(session(), Session::X11)
    }
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        true
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        false
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

    // Selections made inside our own visible windows must not trigger the
    // watcher. `is_focused` is read from the runtime's cached focus state, so
    // a hidden card can never leave stale focus behind.
    for label in ["main", "ball"] {
        if let Some(window) = app.get_webview_window(label) {
            if window.is_visible().unwrap_or(false) && window.is_focused().unwrap_or(false) {
                return true;
            }
        }
    }

    if let Some(until) = *state.suppress_until.lock().unwrap() {
        if std::time::Instant::now() < until {
            return true;
        }
    }

    false
}

#[cfg(any(
    target_os = "linux",
    target_os = "windows",
    target_os = "macos"
))]
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
    crate::show_main(app);
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

#[cfg(target_os = "linux")]
fn read_primary() -> Result<String, String> {
    match session() {
        Session::X11 => crate::capture::primary_selection_x11(),
        Session::Wayland => crate::capture::primary_selection_wayland(),
        Session::Unknown => crate::capture::capture_selection(),
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::time::Instant;

    use tauri::AppHandle;

    use super::{POLL_INTERVAL, SelectionMode, blocked, handle, read_primary, settings};

    pub fn run(app: AppHandle) {
        // `last_seen` dedupes: the same selected text is only acted upon once
        // until the selection changes (an empty selection resets it).
        let mut last_seen: Option<String> = None;
        // `pending` waits out the settle delay so a drag does not fire.
        let mut pending: Option<(String, Instant)> = None;

        loop {
            std::thread::sleep(POLL_INTERVAL);

            let settings = settings(&app);

            if settings.mode == SelectionMode::Off {
                last_seen = None;
                pending = None;
                continue;
            }

            let text = read_primary().unwrap_or_default();

            if text.is_empty() {
                last_seen = None;
                pending = None;
                continue;
            }

            if last_seen.as_deref() != Some(text.as_str()) {
                last_seen = Some(text.clone());
                pending = Some((text, Instant::now()));
                continue;
            }

            let Some((pending_text, since)) = pending.as_ref() else {
                continue;
            };

            if since.elapsed() < settings.delay {
                continue;
            }

            let text = pending_text.clone();
            pending = None;

            if !settings.accepts(&text) || blocked(&app) {
                continue;
            }

            let anchor = crate::capture::pointer_position();
            let window = crate::capture::foreground_window();
            handle(&app, settings, text, anchor, window);
        }
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
    fn accepts_only_matching_mode_and_length() {
        let off = SelectionSettings {
            mode: SelectionMode::Off,
            delay: Duration::from_millis(DEFAULT_DELAY_MS),
            min_length: 2,
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
