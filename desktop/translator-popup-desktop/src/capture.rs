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
        return Err("no selected text found".to_string());
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

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub fn capture_selection() -> Result<String, String> {
    let output = std::process::Command::new("wl-paste")
        .args(["--primary", "--no-newline"])
        .output()
        .map_err(|error| format!("failed to run wl-paste: {error}"))?;

    if !output.status.success() {
        return Err("no selected text found".to_string());
    }

    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        return Err("no selected text found".to_string());
    }

    Ok(text)
}
