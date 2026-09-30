use std::env;
use std::path::{Path, PathBuf};

#[cfg(target_os = "windows")]
pub fn config_path() -> Option<PathBuf> {
    let base = env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("USERPROFILE").map(|home| {
                PathBuf::from(home)
                    .join("AppData")
                    .join("Roaming")
            })
        })?;

    Some(base.join("open-translator").join("config"))
}

#[cfg(target_os = "macos")]
pub fn config_path() -> Option<PathBuf> {
    let home = env::var_os("HOME")?;
    Some(
        PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("open-translator")
            .join("config"),
    )
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub fn config_path() -> Option<PathBuf> {
    let base = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;

    Some(base.join("open-translator").join("config"))
}

#[cfg(target_os = "windows")]
pub fn state_dir() -> PathBuf {
    env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("USERPROFILE").map(|home| {
                PathBuf::from(home)
                    .join("AppData")
                    .join("Local")
            })
        })
        .unwrap_or_else(env::temp_dir)
        .join("open-translator")
}

#[cfg(target_os = "macos")]
pub fn state_dir() -> PathBuf {
    env::var_os("HOME")
        .map(|home| PathBuf::from(home).join("Library").join("Logs"))
        .unwrap_or_else(env::temp_dir)
        .join("open-translator")
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub fn state_dir() -> PathBuf {
    let base = env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("HOME").map(|home| PathBuf::from(home).join(".local").join("state"))
        })
        .unwrap_or_else(env::temp_dir);

    base.join("open-translator")
}

pub const DEFAULT_MODEL_FILE: &str = "hy-mt1.5-1.8b-q4_k_m.gguf";

#[cfg(target_os = "windows")]
pub fn models_dir() -> Option<PathBuf> {
    env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("USERPROFILE").map(|home| {
                PathBuf::from(home)
                    .join("AppData")
                    .join("Local")
            })
        })
        .map(|base| base.join("open-translator").join("models"))
}

#[cfg(target_os = "macos")]
pub fn models_dir() -> Option<PathBuf> {
    let home = env::var_os("HOME")?;
    Some(
        PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("open-translator")
            .join("models"),
    )
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub fn models_dir() -> Option<PathBuf> {
    let base = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))?;

    Some(base.join("open-translator").join("models"))
}

pub fn default_model_path() -> Option<PathBuf> {
    Some(models_dir()?.join(DEFAULT_MODEL_FILE))
}

pub fn log_path(name: &str) -> PathBuf {
    state_dir().join(name)
}

pub fn default_core_bin() -> Option<PathBuf> {
    let exe = env::current_exe().ok()?;

    // Installed layouts (e.g. a macOS .app bundle) ship the service next to the popup.
    if let Some(dir) = exe.parent() {
        let sibling = dir.join(format!("translator-service{}", env::consts::EXE_SUFFIX));
        if sibling.is_file() {
            return Some(sibling);
        }
    }

    let candidate = core_bin_from_exe(&exe)?;
    candidate.is_file().then_some(candidate)
}

pub fn core_bin_from_exe(exe: &Path) -> Option<PathBuf> {
    let repo_root = exe.parent()?.parent()?.parent()?.parent()?.parent()?;
    Some(repo_root.join(format!(
        "core/translator-service/target/release/translator-service{}",
        env::consts::EXE_SUFFIX
    )))
}

#[cfg(target_os = "windows")]
pub fn default_ollama_bin() -> Option<PathBuf> {
    let base = env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("USERPROFILE").map(|home| {
                PathBuf::from(home)
                    .join("AppData")
                    .join("Local")
            })
        })?;

    let candidate = base
        .join("Programs")
        .join("Ollama")
        .join("ollama.exe");

    candidate.is_file().then_some(candidate)
}

#[cfg(target_os = "macos")]
pub fn default_ollama_bin() -> Option<PathBuf> {
    let candidate = PathBuf::from("/Applications/Ollama.app/Contents/Resources/ollama");
    candidate.is_file().then_some(candidate)
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub fn default_ollama_bin() -> Option<PathBuf> {
    let home = env::var_os("HOME")?;
    let candidate = PathBuf::from(home).join(".local/opt/ollama/bin/ollama");
    candidate.is_file().then_some(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_core_binary_from_popup_location() {
        let exe = Path::new("/repo/desktop/translator-popup/target/release/translator-popup");

        assert_eq!(
            core_bin_from_exe(exe).unwrap(),
            PathBuf::from(format!(
                "/repo/core/translator-service/target/release/translator-service{}",
                env::consts::EXE_SUFFIX
            ))
        );
    }

    #[test]
    fn default_model_path_ends_with_model_file() {
        let path = default_model_path().unwrap();

        assert_eq!(
            path.file_name().unwrap().to_string_lossy(),
            DEFAULT_MODEL_FILE
        );
    }
}
