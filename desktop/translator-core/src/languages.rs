//! Shared language list used by the desktop clients.

/// Source-language tag meaning "detect the language automatically".
pub const AUTO_CODE: &str = "auto";

/// Display name of the automatic source-language option.
pub const AUTO_LABEL: &str = "自动检测";

pub const LANGUAGES: &[(&str, &str)] = &[
    ("zh", "中文"),
    ("en", "英语"),
    ("ja", "日语"),
    ("ko", "韩语"),
    ("fr", "法语"),
    ("de", "德语"),
    ("es", "西班牙语"),
    ("ru", "俄语"),
    ("th", "泰语"),
];

/// Human-readable name for a language tag, falling back to the tag itself.
pub fn label(code: &str) -> &str {
    LANGUAGES
        .iter()
        .find(|(candidate, _)| *candidate == code)
        .map(|(_, name)| *name)
        .unwrap_or(code)
}

/// Display name for a source selector entry (includes `auto`).
pub fn source_label(code: &str) -> &str {
    if code == AUTO_CODE {
        AUTO_LABEL
    } else {
        label(code)
    }
}

/// Source selector options: `auto` followed by the shared language list.
pub fn source_options() -> impl Iterator<Item = (&'static str, &'static str)> {
    std::iter::once((AUTO_CODE, AUTO_LABEL)).chain(LANGUAGES.iter().copied())
}

/// Whether `code` is one of the shared language tags.
pub fn is_supported(code: &str) -> bool {
    LANGUAGES.iter().any(|(candidate, _)| *candidate == code)
}

/// Most recent targets first, excluding the current one and unsupported or
/// duplicate entries, capped at three.
pub fn recent_target_list(recents: &[String], current: &str) -> Vec<String> {
    let mut list: Vec<String> = Vec::new();

    for entry in recents {
        if entry != current && is_supported(entry) && !list.contains(entry) {
            list.push(entry.clone());
        }

        if list.len() == 3 {
            break;
        }
    }

    list
}

/// Language pair after a swap: the old target becomes the new source and the
/// language of the original text (the detected tag for `auto`) becomes the new
/// target. Returns `None` when the pair cannot be swapped.
pub fn swapped_pair(
    source: &str,
    target: &str,
    detected: Option<&str>,
) -> Option<(String, String)> {
    let next_target = if source == AUTO_CODE { detected? } else { source };

    if next_target == target || !is_supported(next_target) || !is_supported(target) {
        return None;
    }

    Some((target.to_string(), next_target.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_known_codes_to_names() {
        assert_eq!(label("zh"), "中文");
        assert_eq!(label("ja"), "日语");
    }

    #[test]
    fn unknown_codes_fall_back_to_the_code() {
        assert_eq!(label("xx"), "xx");
    }

    #[test]
    fn labels_the_auto_source() {
        assert_eq!(source_label("auto"), "自动检测");
        assert_eq!(source_label("ja"), "日语");
        assert_eq!(source_label("xx"), "xx");
        assert_eq!(source_options().next(), Some(("auto", "自动检测")));
    }

    #[test]
    fn checks_supported_tags() {
        assert!(is_supported("zh"));
        assert!(!is_supported("auto"));
        assert!(!is_supported("xx"));
    }

    #[test]
    fn excludes_the_current_and_duplicate_targets() {
        let recents = vec![
            "zh".to_string(),
            "en".to_string(),
            "zh".to_string(),
            "ja".to_string(),
        ];

        assert_eq!(
            recent_target_list(&recents, "zh"),
            vec!["en".to_string(), "ja".to_string()]
        );
    }

    #[test]
    fn caps_the_recent_targets_at_three_supported_entries() {
        let recents = vec![
            "en".to_string(),
            "de".to_string(),
            "fr".to_string(),
            "ja".to_string(),
            "xx".to_string(),
        ];

        assert_eq!(
            recent_target_list(&recents, "zh"),
            vec!["en".to_string(), "de".to_string(), "fr".to_string()]
        );
    }

    #[test]
    fn swaps_explicit_language_pairs() {
        assert_eq!(
            swapped_pair("en", "zh", None),
            Some(("zh".to_string(), "en".to_string()))
        );
    }

    #[test]
    fn swaps_auto_source_using_the_detected_tag() {
        assert_eq!(
            swapped_pair("auto", "zh", Some("en")),
            Some(("zh".to_string(), "en".to_string()))
        );
    }

    #[test]
    fn refuses_to_swap_identical_or_unknown_pairs() {
        assert_eq!(swapped_pair("auto", "zh", None), None);
        assert_eq!(swapped_pair("zh", "zh", None), None);
        assert_eq!(swapped_pair("xx", "zh", None), None);
        assert_eq!(swapped_pair("auto", "zh", Some("zh")), None);
    }
}
