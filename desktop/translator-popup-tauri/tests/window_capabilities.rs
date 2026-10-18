//! Guards the Tauri window/permission pairing: a window declared in
//! tauri.conf.json but missing from every capability's `windows` list can be
//! shown, yet its JavaScript cannot listen to events or use plugin commands.
//! Platform configs replace `app.windows` wholesale, so they must keep every
//! base window label as well.

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

fn capability_patterns(manifest: &Path) -> Vec<String> {
    let mut patterns = Vec::new();
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

    patterns
}

fn assert_covered(source: &str, windows: &[&str], patterns: &[String]) {
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
            "window `{window}` from {source} is missing from every capability \
             `windows` list; its JavaScript would have no event/plugin permission"
        );
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

    assert_covered(
        "tauri.conf.json",
        &windows,
        &capability_patterns(manifest),
    );
}

#[test]
fn platform_configs_keep_every_base_window() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let base = read_json(&manifest.join("tauri.conf.json"));

    let base_windows: HashSet<String> = base["app"]["windows"]
        .as_array()
        .expect("app.windows must be an array")
        .iter()
        .filter_map(|window| window["label"].as_str().map(str::to_string))
        .collect();

    let patterns = capability_patterns(manifest);
    let mut checked = 0;

    for entry in std::fs::read_dir(manifest).expect("crate directory") {
        let path = entry.expect("crate entry").path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };

        if !(name.starts_with("tauri.") && name.ends_with(".conf.json")) {
            continue;
        }

        let config = read_json(&path);
        let Some(windows) = config["app"]["windows"].as_array() else {
            continue;
        };

        checked += 1;

        let labels: HashSet<String> = windows
            .iter()
            .filter_map(|window| window["label"].as_str().map(str::to_string))
            .collect();

        for missing in base_windows.difference(&labels) {
            panic!(
                "{name} overrides app.windows but drops `{missing}`; the window would \
                 never be created on that platform"
            );
        }

        let platform_labels: Vec<&str> = windows
            .iter()
            .filter_map(|window| window["label"].as_str())
            .collect();
        assert_covered(name, &platform_labels, &patterns);
    }

    assert!(checked > 0, "no platform config files found to check");
}
