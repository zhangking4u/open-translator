use std::fs::{File, OpenOptions};

pub struct Guard {
    _file: File,
}

pub fn acquire() -> Option<Guard> {
    let path = std::env::temp_dir().join("open-translator-desktop.lock");
    let file = OpenOptions::new().create(true).write(true).open(path).ok()?;

    match file.try_lock() {
        Ok(()) => Some(Guard { _file: file }),
        Err(_) => None,
    }
}

#[cfg(target_os = "windows")]
pub fn notify_existing_instance() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, MB_ICONINFORMATION, MB_OK, MB_SETFOREGROUND, MB_TOPMOST,
    };

    let text: Vec<u16> = "OpenTranslator 已在运行，请使用托盘图标，或直接按 Ctrl+Alt+T 翻译。\0"
        .encode_utf16()
        .collect();
    let caption: Vec<u16> = "OpenTranslator\0".encode_utf16().collect();

    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            caption.as_ptr(),
            MB_OK | MB_ICONINFORMATION | MB_TOPMOST | MB_SETFOREGROUND,
        );
    }
}

#[cfg(not(target_os = "windows"))]
pub fn notify_existing_instance() {
    eprintln!("OpenTranslator is already running");
}
