use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::paths::history_path;

pub const MAX_ENTRIES: usize = 10;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub source: String,
    pub target: String,
    pub text: String,
    pub translation: String,
    #[serde(default)]
    pub at: Option<u64>,
}

/// Insert a translation, newest first, replacing an entry with the same text
/// and target and capping the list at [`MAX_ENTRIES`].
pub fn push(entries: &mut Vec<HistoryEntry>, entry: HistoryEntry) {
    entries.retain(|existing| !(existing.text == entry.text && existing.target == entry.target));
    entries.insert(0, entry);
    entries.truncate(MAX_ENTRIES);
}

pub fn load() -> Vec<HistoryEntry> {
    history_path().map(|path| load_from(&path)).unwrap_or_default()
}

pub fn save(entries: &[HistoryEntry]) {
    if let Some(path) = history_path() {
        save_to(&path, entries);
    }
}

pub fn load_from(path: &Path) -> Vec<HistoryEntry> {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return Vec::new();
    };

    serde_json::from_str(&contents).unwrap_or_default()
}

pub fn save_to(path: &Path, entries: &[HistoryEntry]) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }

    if let Ok(contents) = serde_json::to_string(entries) {
        let _ = std::fs::write(path, contents);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(text: &str, target: &str, translation: &str) -> HistoryEntry {
        HistoryEntry {
            source: "auto".to_string(),
            target: target.to_string(),
            text: text.to_string(),
            translation: translation.to_string(),
            at: None,
        }
    }

    #[test]
    fn pushes_newest_first_and_dedupes() {
        let mut entries = Vec::new();
        push(&mut entries, entry("a", "zh", "甲"));
        push(&mut entries, entry("b", "zh", "乙"));
        push(&mut entries, entry("a", "zh", "甲二"));

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].text, "a");
        assert_eq!(entries[0].translation, "甲二");
        assert_eq!(entries[1].text, "b");
    }

    #[test]
    fn keeps_the_same_text_for_another_target() {
        let mut entries = Vec::new();
        push(&mut entries, entry("a", "zh", "甲"));
        push(&mut entries, entry("a", "en", "A"));

        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn caps_the_list() {
        let mut entries = Vec::new();

        for index in 0..(MAX_ENTRIES + 5) {
            push(&mut entries, entry(&format!("text-{index}"), "zh", "x"));
        }

        assert_eq!(entries.len(), MAX_ENTRIES);
        assert_eq!(entries[0].text, format!("text-{}", MAX_ENTRIES + 4));
    }

    #[test]
    fn round_trips_through_a_file() {
        let path = std::env::temp_dir().join(format!(
            "open-translator-history-test-{}.json",
            std::process::id()
        ));
        let entries = vec![entry("hello", "zh", "你好")];

        save_to(&path, &entries);
        assert_eq!(load_from(&path), entries);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_files_load_empty() {
        let path = std::env::temp_dir().join("open-translator-history-missing.json");
        let _ = std::fs::remove_file(&path);

        assert!(load_from(&path).is_empty());
    }
}
