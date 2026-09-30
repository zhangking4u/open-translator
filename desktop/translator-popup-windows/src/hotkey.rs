#[cfg(target_os = "windows")]
pub struct Hotkey {
    _manager: global_hotkey::GlobalHotKeyManager,
}

#[cfg(target_os = "windows")]
impl Hotkey {
    pub fn register() -> Result<Self, String> {
        use global_hotkey::hotkey::{Code, HotKey, Modifiers};

        let manager = global_hotkey::GlobalHotKeyManager::new()
            .map_err(|error| format!("failed to init hotkey manager: {error}"))?;
        let hotkey = HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyT);

        manager
            .register(hotkey)
            .map_err(|error| format!("failed to register Ctrl+Alt+T: {error}"))?;

        Ok(Self { _manager: manager })
    }

    pub fn pressed(&self) -> bool {
        use global_hotkey::{GlobalHotKeyEvent, HotKeyState};

        let mut pressed = false;
        while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
            if event.state == HotKeyState::Pressed {
                pressed = true;
            }
        }

        pressed
    }
}

#[cfg(not(target_os = "windows"))]
pub struct Hotkey;

#[cfg(not(target_os = "windows"))]
impl Hotkey {
    pub fn register() -> Result<Self, String> {
        Ok(Self)
    }

    pub fn pressed(&self) -> bool {
        false
    }
}
