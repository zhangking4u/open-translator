#[cfg(target_os = "windows")]
pub fn capture_selection() -> Result<String, String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};

    let mut enigo =
        Enigo::new(&Settings::default()).map_err(|error| format!("failed to init input: {error}"))?;

    enigo
        .key(Key::Control, Direction::Press)
        .map_err(|error| format!("failed to press Ctrl: {error}"))?;
    let click = enigo.key(Key::Unicode('c'), Direction::Click);
    let release = enigo.key(Key::Control, Direction::Release);
    click.map_err(|error| format!("failed to send C: {error}"))?;
    release.map_err(|error| format!("failed to release Ctrl: {error}"))?;

    std::thread::sleep(std::time::Duration::from_millis(120));

    let mut clipboard =
        arboard::Clipboard::new().map_err(|error| format!("failed to open clipboard: {error}"))?;
    let text = clipboard
        .get_text()
        .map_err(|error| format!("failed to read clipboard: {error}"))?;

    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("no selected text found".to_string());
    }

    Ok(text)
}

#[cfg(not(target_os = "windows"))]
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
