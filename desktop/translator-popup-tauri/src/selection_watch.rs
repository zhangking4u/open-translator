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
//! Wayland. Windows/macOS watchers follow in later phases; until then
//! [`selection_supported`] is false there and nothing is started.

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

/// Whether the current platform has a selection watcher at all. Linux landed
/// first; Windows (mouse hook + UI Automation) and macOS (NSEvent + AX) are
/// staged follow-ups, see docs/SELECTION_TRANSLATION.md.
pub fn selection_supported() -> bool {
    #[cfg(target_os = "linux")]
    {
        true
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

/// Whether the platform can anchor the ball next to the selection. Native
/// Wayland has no global pointer position and no window placement, so the ball
/// degrades to direct translation there.
pub fn ball_supported() -> bool {
    #[cfg(target_os = "linux")]
    {
        matches!(session(), Session::X11)
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

pub fn spawn(app: AppHandle) {
    #[cfg(target_os = "linux")]
    {
        if selection_supported() {
            std::thread::spawn(move || linux::run(app));
        }
    }

    #[cfg(not(target_os = "linux"))]
    let _ = app;
}

/// Guards shared by the platform watchers: a pinned card, one of our own
/// windows focused (the user is selecting inside the card) or a recent hotkey
/// trigger all silence the watcher.
#[cfg(target_os = "linux")]
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

#[cfg(target_os = "linux")]
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

    use tauri::{AppHandle, Manager};

    use super::{
        POLL_INTERVAL, SelectionMode, blocked, handle, read_primary,
    };
    use crate::AppState;

    pub fn run(app: AppHandle) {
        // `last_seen` dedupes: the same selected text is only acted upon once
        // until the selection changes (an empty selection resets it).
        let mut last_seen: Option<String> = None;
        // `pending` waits out the settle delay so a drag does not fire.
        let mut pending: Option<(String, Instant)> = None;

        loop {
            std::thread::sleep(POLL_INTERVAL);

            let settings = *app.state::<AppState>().selection.lock().unwrap();

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
