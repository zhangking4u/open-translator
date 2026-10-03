/// Shared marker for "the user has no selection", used to show the empty hint
/// instead of an error; keep producers and consumers on this constant.
pub const NO_SELECTION: &str = "no selected text found";

#[cfg(target_os = "macos")]
fn wait_for_modifiers_released() {
    use std::time::{Duration, Instant};

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGEventSourceKeyState(state: i32, key: u16) -> u8;
    }

    const HID_SYSTEM_STATE: i32 = 1;
    const COMMAND_LEFT: u16 = 0x37;
    const COMMAND_RIGHT: u16 = 0x36;
    const SHIFT_LEFT: u16 = 0x38;
    const SHIFT_RIGHT: u16 = 0x3C;
    const OPTION_LEFT: u16 = 0x3A;
    const OPTION_RIGHT: u16 = 0x3D;
    const CONTROL_LEFT: u16 = 0x3B;
    const CONTROL_RIGHT: u16 = 0x3E;

    const MODIFIER_KEYS: [u16; 8] = [
        COMMAND_LEFT,
        COMMAND_RIGHT,
        SHIFT_LEFT,
        SHIFT_RIGHT,
        OPTION_LEFT,
        OPTION_RIGHT,
        CONTROL_LEFT,
        CONTROL_RIGHT,
    ];

    let deadline = Instant::now() + Duration::from_millis(750);

    while Instant::now() < deadline {
        let held = MODIFIER_KEYS
            .iter()
            .any(|key| unsafe { CGEventSourceKeyState(HID_SYSTEM_STATE, *key) } != 0);

        if !held {
            return;
        }

        std::thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(target_os = "windows")]
fn wait_for_modifiers_released() {
    use std::time::{Duration, Instant};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_RCONTROL, VK_RMENU,
        VK_RSHIFT, VK_RWIN,
    };

    const MODIFIERS: [u16; 8] = [
        VK_LCONTROL, VK_RCONTROL, VK_LMENU, VK_RMENU, VK_LSHIFT, VK_RSHIFT, VK_LWIN, VK_RWIN,
    ];

    let deadline = Instant::now() + Duration::from_millis(750);

    while Instant::now() < deadline {
        let held = MODIFIERS
            .iter()
            .any(|key| unsafe { GetAsyncKeyState(*key as i32) } < 0);

        if !held {
            return;
        }

        std::thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
pub fn capture_selection() -> Result<String, String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    use std::time::Duration;
    #[cfg(target_os = "windows")]
    use std::time::Instant;

    let mut clipboard =
        arboard::Clipboard::new().map_err(|error| format!("failed to open clipboard: {error}"))?;

    let before = clipboard.get_text().ok();

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    wait_for_modifiers_released();

    #[cfg(target_os = "windows")]
    let clipboard_sequence =
        unsafe { windows_sys::Win32::System::DataExchange::GetClipboardSequenceNumber() };

    let mut enigo =
        Enigo::new(&Settings::default()).map_err(|error| format!("failed to init input: {error}"))?;

    #[cfg(target_os = "windows")]
    let modifier = Key::Control;
    #[cfg(target_os = "macos")]
    let modifier = Key::Meta;

    enigo
        .key(modifier, Direction::Press)
        .map_err(|error| format!("failed to press modifier: {error}"))?;

    std::thread::sleep(Duration::from_millis(15));

    let click = enigo.key(Key::Unicode('c'), Direction::Click);
    let release = enigo.key(modifier, Direction::Release);
    click.map_err(|error| format!("failed to send C: {error}"))?;
    release.map_err(|error| format!("failed to release modifier: {error}"))?;

    #[cfg(target_os = "windows")]
    {
        let deadline = Instant::now() + Duration::from_millis(1500);

        while unsafe { windows_sys::Win32::System::DataExchange::GetClipboardSequenceNumber() }
            == clipboard_sequence
        {
            if Instant::now() >= deadline {
                return Err(
                    "未能取到选中文本（复制未生效）。请确认已选中文字；若目标窗口以管理员身份运行，请尝试以管理员身份启动 OpenTranslator。"
                        .to_string(),
                );
            }

            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[cfg(target_os = "macos")]
    std::thread::sleep(Duration::from_millis(150));

    let text = clipboard
        .get_text()
        .map_err(|error| format!("failed to read clipboard: {error}"))?
        .trim()
        .to_string();

    if text.is_empty() {
        return Err(NO_SELECTION.to_string());
    }

    #[cfg(target_os = "macos")]
    if before.as_deref().map(str::trim).filter(|value| !value.is_empty()) == Some(text.as_str()) {
        return Err(
            "未能取到选中文本。请确认已选中文字；若为首次使用，请在「系统设置 → 隐私与安全性 → 辅助功能」中允许 OpenTranslator"
                .to_string(),
        );
    }

    restore_clipboard(&mut clipboard, before, &text);

    Ok(text)
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn restore_clipboard(
    clipboard: &mut arboard::Clipboard,
    previous: Option<String>,
    captured: &str,
) {
    let Some(previous) = previous else {
        return;
    };

    if previous.trim() == captured {
        return;
    }

    let _ = clipboard.set_text(previous);
}

#[cfg(target_os = "windows")]
pub fn foreground_window() -> Option<isize> {
    let window = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow() };

    if window.is_null() {
        None
    } else {
        Some(window as isize)
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::time::{Duration, Instant};

    use super::NO_SELECTION;
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{
        Atom, AtomEnum, ConnectionExt, InputFocus, WindowClass,
    };
    use x11rb::protocol::Event;
    use x11rb::rust_connection::RustConnection;
    use x11rb::CURRENT_TIME;

    const SELECTION_PROPERTY: &str = "OPEN_TRANSLATOR_SELECTION";

    fn atom(conn: &RustConnection, name: &str) -> Result<Atom, String> {
        conn.intern_atom(false, name.as_bytes())
            .map_err(|error| error.to_string())?
            .reply()
            .map(|reply| reply.atom)
            .map_err(|error| error.to_string())
    }

    /// Read the PRIMARY selection over ICCCM. This is the fallback for Xorg
    /// sessions, where `wl-paste` (Wayland) is not available; under Wayland
    /// the XWayland bridge usually serves the same selection.
    pub fn primary_selection() -> Result<String, String> {
        let (conn, screen_num) = x11rb::connect(None).map_err(|error| error.to_string())?;
        let screen = &conn.setup().roots[screen_num];
        let window = conn.generate_id().map_err(|error| error.to_string())?;

        conn.create_window(
            x11rb::COPY_FROM_PARENT as u8,
            window,
            screen.root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_OUTPUT,
            x11rb::COPY_FROM_PARENT,
            &Default::default(),
        )
        .map_err(|error| error.to_string())?;
        conn.flush().map_err(|error| error.to_string())?;

        let result = read_primary(&conn, window);

        let _ = conn.destroy_window(window);
        let _ = conn.flush();

        result
    }

    fn read_primary(conn: &RustConnection, window: x11rb::protocol::xproto::Window) -> Result<String, String> {
        let primary = atom(conn, "PRIMARY")?;
        let utf8 = atom(conn, "UTF8_STRING")?;
        let property = atom(conn, SELECTION_PROPERTY)?;

        conn.convert_selection(window, primary, utf8, property, CURRENT_TIME)
            .map_err(|error| error.to_string())?;
        conn.flush().map_err(|error| error.to_string())?;

        let deadline = Instant::now() + Duration::from_millis(1500);
        let mut notified = false;

        while Instant::now() < deadline {
            match conn.poll_for_event().map_err(|error| error.to_string())? {
                Some(Event::SelectionNotify(event))
                    if event.requestor == window && event.selection == primary =>
                {
                    if event.property == x11rb::NONE {
                        return Err(NO_SELECTION.to_string());
                    }

                    notified = true;
                    break;
                }
                _ => std::thread::sleep(Duration::from_millis(20)),
            }
        }

        if !notified {
            return Err(NO_SELECTION.to_string());
        }

        let reply = conn
            .get_property(true, window, property, AtomEnum::ANY, 0, u32::MAX)
            .map_err(|error| error.to_string())?
            .reply()
            .map_err(|error| error.to_string())?;

        let text = String::from_utf8_lossy(&reply.value).trim().to_string();

        if text.is_empty() {
            return Err(NO_SELECTION.to_string());
        }

        Ok(text)
    }

    /// The focused X11 window if a window manager owns it. Wayland-native apps
    /// leave the X focus on Mutter's guard window (no WM_STATE), and our own
    /// card is rejected by PID, so replace-in-place stays unavailable there.
    pub fn foreground_window() -> Option<isize> {
        let (conn, screen_num) = x11rb::connect(None).ok()?;
        let root = conn.setup().roots[screen_num].root;
        let focus = conn.get_input_focus().ok()?.reply().ok()?.focus;

        if focus == x11rb::NONE {
            return None;
        }

        // Focus usually sits on a toolkit child; the WM_STATE marker lives on
        // the managed top-level ancestor.
        let ancestor = managed_ancestor(&conn, focus, root)?;

        if is_ours(&conn, ancestor) {
            return None;
        }

        // Refocus the exact window that had focus so the paste lands where the
        // user was typing.
        Some(focus as isize)
    }

    fn managed_ancestor(
        conn: &RustConnection,
        window: x11rb::protocol::xproto::Window,
        root: x11rb::protocol::xproto::Window,
    ) -> Option<x11rb::protocol::xproto::Window> {
        let wm_state = atom(conn, "WM_STATE").ok()?;
        let mut current = window;

        for _ in 0..16 {
            if let Ok(cookie) = conn.get_property(false, current, wm_state, AtomEnum::ANY, 0, 0) {
                if matches!(cookie.reply(), Ok(reply) if reply.type_ != x11rb::NONE) {
                    return Some(current);
                }
            }

            let tree = conn.query_tree(current).ok()?.reply().ok()?;
            if tree.parent == root || tree.parent == x11rb::NONE {
                return None;
            }

            current = tree.parent;
        }

        None
    }

    fn is_ours(conn: &RustConnection, window: x11rb::protocol::xproto::Window) -> bool {
        let Ok(pid_atom) = atom(conn, "_NET_WM_PID") else {
            return false;
        };
        let Ok(cookie) = conn.get_property(false, window, pid_atom, AtomEnum::CARDINAL, 0, 1) else {
            return false;
        };
        let Ok(reply) = cookie.reply() else {
            return false;
        };

        reply.value32().and_then(|mut values| values.next()) == Some(std::process::id())
    }

    fn restore_clipboard(
        clipboard: &mut arboard::Clipboard,
        before: &Option<String>,
        replacement: &str,
    ) {
        if let Some(previous) = before {
            if previous != replacement {
                let _ = clipboard.set_text(previous);
            }
        }
    }

    /// Focus the recorded X11 window and paste the translated text with
    /// Ctrl+V. The focus move is verified because a compositor may deny it
    /// (GNOME focus-stealing prevention); in that case the paste is cancelled
    /// instead of typing into the wrong window. On every exit a guard releases
    /// the modifier and restores the previous clipboard content.
    pub fn replace_selection(
        window: isize,
        text: &str,
        clipboard: &mut arboard::Clipboard,
    ) -> Result<(), String> {
        use enigo::{Direction, Enigo, Key, Keyboard, Settings};

        let before = clipboard.get_text().ok();
        clipboard
            .set_text(text)
            .map_err(|error| format!("failed to write clipboard: {error}"))?;

        let enigo = Enigo::new(&Settings::default()).map_err(|error| {
            restore_clipboard(clipboard, &before, text);
            format!("failed to init input: {error}")
        })?;

        struct ReplaceGuard<'a> {
            clipboard: &'a mut arboard::Clipboard,
            before: Option<String>,
            replacement: String,
            enigo: Enigo,
            ctrl_pressed: bool,
        }

        impl Drop for ReplaceGuard<'_> {
            fn drop(&mut self) {
                if self.ctrl_pressed {
                    let _ = self
                        .enigo
                        .key(enigo::Key::Control, enigo::Direction::Release);
                }

                restore_clipboard(self.clipboard, &self.before, &self.replacement);
            }
        }

        let mut guard = ReplaceGuard {
            clipboard,
            before,
            replacement: text.to_string(),
            enigo,
            ctrl_pressed: false,
        };

        let (conn, _) = x11rb::connect(None).map_err(|error| error.to_string())?;
        let target = window as x11rb::protocol::xproto::Window;

        conn.set_input_focus(InputFocus::PARENT, target, CURRENT_TIME)
            .map_err(|error| error.to_string())?;
        conn.flush().map_err(|error| error.to_string())?;
        std::thread::sleep(Duration::from_millis(120));

        let focus = conn
            .get_input_focus()
            .map_err(|error| error.to_string())?
            .reply()
            .map_err(|error| error.to_string())?
            .focus;

        if focus != target {
            return Err("无法把焦点切回原窗口，替换已取消".to_string());
        }

        guard
            .enigo
            .key(Key::Control, Direction::Press)
            .map_err(|error| format!("failed to press modifier: {error}"))?;
        guard.ctrl_pressed = true;
        std::thread::sleep(Duration::from_millis(15));
        guard
            .enigo
            .key(Key::Unicode('v'), Direction::Click)
            .map_err(|error| format!("failed to send V: {error}"))?;
        guard
            .enigo
            .key(Key::Control, Direction::Release)
            .map_err(|error| format!("failed to release modifier: {error}"))?;
        guard.ctrl_pressed = false;
        std::thread::sleep(Duration::from_millis(250));

        Ok(())
    }
}

#[cfg(target_os = "linux")]
pub fn foreground_window() -> Option<isize> {
    linux::foreground_window()
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
pub fn foreground_window() -> Option<isize> {
    None
}

#[cfg(target_os = "windows")]
pub fn replace_selection(
    window: isize,
    text: &str,
    clipboard: &mut arboard::Clipboard,
) -> Result<(), String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    use std::time::Duration;
    use windows_sys::Win32::UI::WindowsAndMessaging::SetForegroundWindow;

    let before = clipboard.get_text().ok();
    clipboard
        .set_text(text)
        .map_err(|error| format!("failed to write clipboard: {error}"))?;

    if unsafe { SetForegroundWindow(window as _) } == 0 {
        return Err("无法把焦点切回原窗口，替换已取消".to_string());
    }

    std::thread::sleep(Duration::from_millis(120));

    let mut enigo =
        Enigo::new(&Settings::default()).map_err(|error| format!("failed to init input: {error}"))?;

    enigo
        .key(Key::Control, Direction::Press)
        .map_err(|error| format!("failed to press modifier: {error}"))?;

    std::thread::sleep(Duration::from_millis(15));

    let paste = enigo.key(Key::V, Direction::Click);
    let release = enigo.key(Key::Control, Direction::Release);
    paste.map_err(|error| format!("failed to send V: {error}"))?;
    release.map_err(|error| format!("failed to release modifier: {error}"))?;

    std::thread::sleep(Duration::from_millis(250));

    if let Some(previous) = before {
        if previous != text {
            let _ = clipboard.set_text(previous);
        }
    }

    Ok(())
}

#[cfg(target_os = "linux")]
pub fn replace_selection(
    window: isize,
    text: &str,
    clipboard: &mut arboard::Clipboard,
) -> Result<(), String> {
    linux::replace_selection(window, text, clipboard)
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn wayland_primary() -> Result<String, String> {
    let output = std::process::Command::new("wl-paste")
        .args(["--primary", "--no-newline"])
        .output()
        .map_err(|error| format!("failed to run wl-paste: {error}"))?;

    if !output.status.success() {
        return Err(NO_SELECTION.to_string());
    }

    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        return Err(NO_SELECTION.to_string());
    }

    Ok(text)
}

#[cfg(target_os = "linux")]
pub fn capture_selection() -> Result<String, String> {
    match wayland_primary() {
        Ok(text) => Ok(text),
        Err(wayland_error) => match linux::primary_selection() {
            Ok(text) => Ok(text),
            Err(x11_error) if x11_error == NO_SELECTION => Err(x11_error),
            Err(_) => Err(wayland_error),
        },
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
pub fn capture_selection() -> Result<String, String> {
    wayland_primary()
}
