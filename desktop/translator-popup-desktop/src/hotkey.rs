use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modifier {
    Control,
    Alt,
    Shift,
    Meta,
}

impl fmt::Display for Modifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Control => "Ctrl",
            Self::Alt => "Alt",
            Self::Shift => "Shift",
            Self::Meta => "Meta",
        };
        write!(formatter, "{name}")
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct HotkeySpec {
    pub modifiers: Vec<Modifier>,
    pub key: String,
}

pub fn parse_spec(spec: &str) -> Result<HotkeySpec, String> {
    let mut modifiers: Vec<Modifier> = Vec::new();
    let mut key: Option<String> = None;

    for raw in spec.split('+') {
        let part = raw.trim().to_ascii_lowercase();

        if part.is_empty() {
            return Err(format!("invalid hotkey: {spec}"));
        }

        let modifier = match part.as_str() {
            "ctrl" | "control" => Some(Modifier::Control),
            "alt" | "option" => Some(Modifier::Alt),
            "shift" => Some(Modifier::Shift),
            "meta" | "super" | "win" | "cmd" | "command" => Some(Modifier::Meta),
            _ => None,
        };

        if let Some(modifier) = modifier {
            if !modifiers.contains(&modifier) {
                modifiers.push(modifier);
            }
            continue;
        }

        if key.is_some() {
            return Err(format!("invalid hotkey (multiple keys): {spec}"));
        }

        if !is_supported_key(&part) {
            return Err(format!("unsupported key in hotkey: {part}"));
        }

        key = Some(part);
    }

    let Some(key) = key else {
        return Err(format!("hotkey has no key: {spec}"));
    };

    if modifiers.is_empty() {
        return Err(format!("hotkey needs at least one modifier: {spec}"));
    }

    Ok(HotkeySpec { modifiers, key })
}

fn is_supported_key(key: &str) -> bool {
    if key.len() == 1 {
        return key.chars().all(|c| c.is_ascii_alphanumeric());
    }

    matches!(
        key,
        "space"
            | "enter"
            | "tab"
            | "esc"
            | "escape"
            | "f1"
            | "f2"
            | "f3"
            | "f4"
            | "f5"
            | "f6"
            | "f7"
            | "f8"
            | "f9"
            | "f10"
            | "f11"
            | "f12"
    )
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
mod platform {
    use global_hotkey::hotkey::{Code, HotKey, Modifiers};

    use super::{Modifier, parse_spec};

    const LETTERS: [Code; 26] = [
        Code::KeyA,
        Code::KeyB,
        Code::KeyC,
        Code::KeyD,
        Code::KeyE,
        Code::KeyF,
        Code::KeyG,
        Code::KeyH,
        Code::KeyI,
        Code::KeyJ,
        Code::KeyK,
        Code::KeyL,
        Code::KeyM,
        Code::KeyN,
        Code::KeyO,
        Code::KeyP,
        Code::KeyQ,
        Code::KeyR,
        Code::KeyS,
        Code::KeyT,
        Code::KeyU,
        Code::KeyV,
        Code::KeyW,
        Code::KeyX,
        Code::KeyY,
        Code::KeyZ,
    ];

    const DIGITS: [Code; 10] = [
        Code::Digit0,
        Code::Digit1,
        Code::Digit2,
        Code::Digit3,
        Code::Digit4,
        Code::Digit5,
        Code::Digit6,
        Code::Digit7,
        Code::Digit8,
        Code::Digit9,
    ];

    const FUNCTION_KEYS: [Code; 12] = [
        Code::F1,
        Code::F2,
        Code::F3,
        Code::F4,
        Code::F5,
        Code::F6,
        Code::F7,
        Code::F8,
        Code::F9,
        Code::F10,
        Code::F11,
        Code::F12,
    ];

    pub fn parse_hotkey(spec: &str) -> Result<HotKey, String> {
        let parsed = parse_spec(spec)?;

        let mut flags = Modifiers::empty();
        for modifier in parsed.modifiers {
            flags |= match modifier {
                Modifier::Control => Modifiers::CONTROL,
                Modifier::Alt => Modifiers::ALT,
                Modifier::Shift => Modifiers::SHIFT,
                Modifier::Meta => Modifiers::META,
            };
        }

        let code = key_code(&parsed.key)
            .ok_or_else(|| format!("unsupported key in hotkey: {}", parsed.key))?;

        Ok(HotKey::new(Some(flags), code))
    }

    fn key_code(key: &str) -> Option<Code> {
        if let Some(letter) = key
            .chars()
            .next()
            .filter(|_| key.len() == 1 && key.chars().all(|c| c.is_ascii_alphabetic()))
        {
            return Some(LETTERS[(letter.to_ascii_lowercase() as u8 - b'a') as usize]);
        }

        if key.len() == 1 && key.chars().all(|c| c.is_ascii_digit()) {
            return Some(DIGITS[(key.as_bytes()[0] - b'0') as usize]);
        }

        Some(match key {
            "space" => Code::Space,
            "enter" => Code::Enter,
            "tab" => Code::Tab,
            "esc" | "escape" => Code::Escape,
            "f1" => FUNCTION_KEYS[0],
            "f2" => FUNCTION_KEYS[1],
            "f3" => FUNCTION_KEYS[2],
            "f4" => FUNCTION_KEYS[3],
            "f5" => FUNCTION_KEYS[4],
            "f6" => FUNCTION_KEYS[5],
            "f7" => FUNCTION_KEYS[6],
            "f8" => FUNCTION_KEYS[7],
            "f9" => FUNCTION_KEYS[8],
            "f10" => FUNCTION_KEYS[9],
            "f11" => FUNCTION_KEYS[10],
            "f12" => FUNCTION_KEYS[11],
            _ => return None,
        })
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
pub struct Hotkey {
    _manager: global_hotkey::GlobalHotKeyManager,
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
impl Hotkey {
    pub fn register(spec: &str) -> Result<Self, String> {
        let hotkey = platform::parse_hotkey(spec)?;
        let manager = global_hotkey::GlobalHotKeyManager::new()
            .map_err(|error| format!("failed to init hotkey manager: {error}"))?;

        manager
            .register(hotkey)
            .map_err(|error| format!("failed to register {spec}: {error}"))?;

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

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub struct Hotkey;

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
impl Hotkey {
    pub fn register(_spec: &str) -> Result<Self, String> {
        Ok(Self)
    }

    pub fn pressed(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_specs() {
        let parsed = parse_spec("Ctrl+Alt+T").unwrap();
        assert_eq!(parsed.modifiers, vec![Modifier::Control, Modifier::Alt]);
        assert_eq!(parsed.key, "t");

        let parsed = parse_spec(" ctrl + shift + space ").unwrap();
        assert_eq!(parsed.modifiers, vec![Modifier::Control, Modifier::Shift]);
        assert_eq!(parsed.key, "space");

        let parsed = parse_spec("cmd+f5").unwrap();
        assert_eq!(parsed.modifiers, vec![Modifier::Meta]);
        assert_eq!(parsed.key, "f5");
    }

    #[test]
    fn rejects_invalid_specs() {
        assert!(parse_spec("T").is_err());
        assert!(parse_spec("Ctrl+Alt").is_err());
        assert!(parse_spec("Ctrl+Foo").is_err());
        assert!(parse_spec("Ctrl++T").is_err());
        assert!(parse_spec("Ctrl+Alt+T+U").is_err());
    }
}
