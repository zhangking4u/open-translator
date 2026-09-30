use std::path::Path;

use crate::paths::config_path;

#[derive(Debug, Default, PartialEq)]
pub struct FileConfig {
    pub service_url: Option<String>,
    pub source: Option<String>,
    pub target: Option<String>,
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
            _ => {}
        }
    }

    config
}

pub fn persist_target(target: &str) {
    let Some(path) = config_path() else {
        return;
    };

    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }

    let contents = std::fs::read_to_string(&path).unwrap_or_default();
    let _ = std::fs::write(&path, apply_target(&contents, target));
}

pub fn apply_target(contents: &str, target: &str) -> String {
    let mut lines: Vec<String> = contents.lines().map(|line| line.to_string()).collect();
    let mut replaced = false;

    for line in &mut lines {
        let trimmed = line.trim_start();
        if trimmed.starts_with("target") && trimmed.contains('=') {
            *line = format!("target = {target}");
            replaced = true;
        }
    }

    if !replaced {
        lines.push(format!("target = {target}"));
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
            "# comment\nsource = ja\n\ntarget=ko\nservice_url = \"http://127.0.0.1:1\"\nunknown = x\n",
        )
        .unwrap();

        let config = load_file_config(&path);
        let _ = std::fs::remove_file(&path);

        assert_eq!(config.source.as_deref(), Some("ja"));
        assert_eq!(config.target.as_deref(), Some("ko"));
        assert_eq!(config.service_url.as_deref(), Some("http://127.0.0.1:1"));
    }

    #[test]
    fn missing_config_file_is_empty() {
        let config = load_file_config(Path::new("/nonexistent/open-translator/config"));

        assert_eq!(config, FileConfig::default());
    }

    #[test]
    fn apply_target_replaces_existing_key() {
        let updated = apply_target("source = en\ntarget = zh\n", "ja");

        assert!(updated.contains("target = ja"));
        assert!(!updated.contains("target = zh"));
    }

    #[test]
    fn apply_target_appends_when_missing() {
        let updated = apply_target("# comment\nsource = en\n", "ko");

        assert!(updated.ends_with("target = ko\n"));
    }
}
