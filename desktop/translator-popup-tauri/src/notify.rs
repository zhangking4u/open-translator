//! System notifications for events that happen while the window is hidden
//! (model download completion, model errors, hotkey registration failures).

#[cfg(target_os = "windows")]
mod platform {
    use tauri_winrt_notification::{Duration, Toast};

    pub fn show(title: &str, body: &str) {
        let _ = Toast::new("OpenTranslator")
            .title(title)
            .text1(body)
            .duration(Duration::Short)
            .show();
    }
}

#[cfg(target_os = "macos")]
mod platform {
    pub fn show(title: &str, body: &str) {
        let _ = mac_notification_sys::send_notification(title, None, body, None);
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
mod platform {
    pub fn show(_title: &str, _body: &str) {}
}

pub use platform::show;
