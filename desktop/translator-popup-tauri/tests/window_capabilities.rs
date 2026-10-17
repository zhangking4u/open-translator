//! Guards the Tauri window/permission pairing: a window declared in
//! tauri.conf.json but missing from every capability's `windows` list can be
//! shown, yet its JavaScript cannot listen to events or use plugin commands.

use std::collections::HashSet;
use std::path::Path;

fn read_json(path: &Path) -> serde_json::Value {
    let contents = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));

    serde_json::from_str(&contents)
        .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()))
}

fn pattern_matches(pattern: &str, label: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => label.starts_with(prefix),
        None => pattern == label,
    }
}

#[test]
fn every_config_window_is_covered_by_a_capability() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let config = read_json(&manifest.join("tauri.conf.json"));

    let windows: Vec<&str> = config["app"]["windows"]
        .as_array()
        .expect("app.windows must be an array")
        .iter()
        .filter_map(|window| window["label"].as_str())
        .collect();
    assert!(!windows.is_empty(), "no windows declared in tauri.conf.json");

    let mut patterns: Vec<String> = Vec::new();
    let capabilities = manifest.join("capabilities");

    for entry in std::fs::read_dir(&capabilities).expect("capabilities directory") {
        let path = entry.expect("capabilities entry").path();

        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
            continue;
        }

        let capability = read_json(&path);

        for pattern in capability["windows"].as_array().into_iter().flatten() {
            if let Some(pattern) = pattern.as_str() {
                patterns.push(pattern.to_string());
            }
        }
    }

    let covered: HashSet<&str> = patterns
        .iter()
        .flat_map(|pattern| {
            windows
                .iter()
                .copied()
                .filter(move |label| pattern_matches(pattern, label))
        })
        .collect();

    for window in windows {
        assert!(
            covered.contains(window),
            "window `{window}` is declared in tauri.conf.json but missing from every \
             capability `windows` list; its JavaScript would have no event/plugin permission"
        );
    }
}
