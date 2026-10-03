use std::path::Path;

use crate::paths::config_path;

#[derive(Debug, Default, PartialEq)]
pub struct FileConfig {
    pub service_url: Option<String>,
    pub source: Option<String>,
    pub target: Option<String>,
    pub clipboard: Option<String>,
    pub recent_targets: Option<Vec<String>>,
    pub hotkey: Option<String>,
    pub model_path: Option<String>,
    pub prompt_style: Option<String>,
    pub serve_extension: Option<String>,
    pub auto_download: Option<String>,
    pub check_updates: Option<String>,
    pub pinned: Option<String>,
}

pub fn load_config() -> FileConfig {
    config_path()
        .map(|path| load_file_config(&path))
        .unwrap_or_default()
}

pub fn load_file_config(path: &Path) -> FileConfig {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return FileConfig::default();
    };

    let mut config = FileConfig::default();

    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };

        let key = key.trim();
        let value = value.trim().trim_matches('"');

        if value.is_empty() {
            continue;
        }

        match key {
            "service_url" => config.service_url = Some(value.to_string()),
            "source" => config.source = Some(value.to_string()),
            "target" => config.target = Some(value.to_string()),
            "clipboard" => config.clipboard = Some(value.to_string()),
            "recent_targets" => {
                config.recent_targets = Some(
                    value
                        .split(',')
                        .map(str::trim)
                        .filter(|entry| !entry.is_empty())
                        .map(str::to_string)
                        .collect(),
                );
            }
            "hotkey" => config.hotkey = Some(value.to_string()),
            "model_path" => config.model_path = Some(value.to_string()),
            "prompt_style" => config.prompt_style = Some(value.to_string()),
            "serve_extension" => config.serve_extension = Some(value.to_string()),
            "auto_download" => config.auto_download = Some(value.to_string()),
            "check_updates" => config.check_updates = Some(value.to_string()),
            "pinned" => config.pinned = Some(value.to_string()),
            _ => {}
        }
    }

    config
}

pub fn persist_source(source: &str) {
    persist_value("source", source);
}

pub fn persist_target(target: &str) {
    persist_value("target", target);
}

pub fn persist_clipboard(clipboard: bool) {
    persist_value("clipboard", if clipboard { "true" } else { "false" });
}

pub fn persist_recent_targets(targets: &[String]) {
    persist_value("recent_targets", &targets.join(","));
}

/// Writes a single `key = value` pair back to the config file, keeping the
/// other lines untouched.
pub fn persist_value(key: &str, value: &str) {
    let Some(path) = config_path() else {
        return;
    };

    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }

    let contents = std::fs::read_to_string(&path).unwrap_or_default();
    let _ = std::fs::write(&path, apply_value(&contents, key, value));
}

pub fn apply_value(contents: &str, key: &str, value: &str) -> String {
    let mut lines: Vec<String> = contents.lines().map(|line| line.to_string()).collect();
    let mut replaced = false;

    for line in &mut lines {
        let trimmed = line.trim_start();
        let Some((candidate, _)) = trimmed.split_once('=') else {
            continue;
        };

        if candidate.trim() == key {
            *line = format!("{key} = {value}");
            replaced = true;
        }
    }

    if !replaced {
        lines.push(format!("{key} = {value}"));
    }

    let mut output = lines.join("\n");
    output.push('\n');
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_config_file() {
        let path = std::env::temp_dir().join(format!(
            "open-translator-config-test-{}.conf",
            std::process::id()
        ));

        std::fs::write(
            &path,
            "# comment\nsource = ja\n\ntarget=ko\nclipboard = true\nrecent_targets = zh, ja\nservice_url = \"http://127.0.0.1:1\"\nhotkey = Ctrl+Shift+T\nmodel_path = /models/hy-mt.gguf\nprompt_style = hymt\nserve_extension = false\nauto_download = false\ncheck_updates = false\nunknown = x\n",
        )
        .unwrap();

        let config = load_file_config(&path);
        let _ = std::fs::remove_file(&path);

        assert_eq!(config.source.as_deref(), Some("ja"));
        assert_eq!(config.target.as_deref(), Some("ko"));
        assert_eq!(config.clipboard.as_deref(), Some("true"));
        assert_eq!(
            config.recent_targets.as_deref(),
            Some(["zh".to_string(), "ja".to_string()].as_slice())
        );
        assert_eq!(config.service_url.as_deref(), Some("http://127.0.0.1:1"));
        assert_eq!(config.hotkey.as_deref(), Some("Ctrl+Shift+T"));
        assert_eq!(config.model_path.as_deref(), Some("/models/hy-mt.gguf"));
        assert_eq!(config.prompt_style.as_deref(), Some("hymt"));
        assert_eq!(config.serve_extension.as_deref(), Some("false"));
        assert_eq!(config.auto_download.as_deref(), Some("false"));
        assert_eq!(config.check_updates.as_deref(), Some("false"));
    }

    #[test]
    fn missing_config_file_is_empty() {
        let config = load_file_config(Path::new("/nonexistent/open-translator/config"));

        assert_eq!(config, FileConfig::default());
    }

    #[test]
    fn apply_value_replaces_existing_target() {
        let updated = apply_value("source = en\ntarget = zh\n", "target", "ja");

        assert!(updated.contains("target = ja"));
        assert!(!updated.contains("target = zh"));
        assert!(updated.contains("source = en"));
    }

    #[test]
    fn apply_value_appends_target_when_missing() {
        let updated = apply_value("# comment\nsource = en\n", "target", "ko");

        assert!(updated.ends_with("target = ko\n"));
    }

    #[test]
    fn apply_value_writes_recent_targets() {
        let updated = apply_value("target = zh\n", "recent_targets", "en,ja");

        assert!(updated.ends_with("recent_targets = en,ja\n"));
    }

    #[test]
    fn apply_value_replaces_source_only() {
        let updated = apply_value("source = en\ntarget = zh\n", "source", "auto");

        assert!(updated.contains("source = auto"));
        assert!(updated.contains("target = zh"));
        assert!(!updated.contains("source = en"));
    }

    #[test]
    fn apply_value_keeps_service_url() {
        let updated = apply_value("service_url = http://127.0.0.1:17890\n", "source", "ja");

        assert!(updated.contains("service_url = http://127.0.0.1:17890"));
        assert!(updated.ends_with("source = ja\n"));
    }
}
