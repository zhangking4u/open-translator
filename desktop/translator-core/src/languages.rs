//! Shared language list used by the desktop clients.

pub const LANGUAGES: &[(&str, &str)] = &[
    ("zh", "中文"),
    ("en", "英语"),
    ("ja", "日语"),
    ("ko", "韩语"),
    ("fr", "法语"),
    ("de", "德语"),
    ("es", "西班牙语"),
    ("ru", "俄语"),
];

/// Human-readable name for a language tag, falling back to the tag itself.
pub fn label(code: &str) -> &str {
    LANGUAGES
        .iter()
        .find(|(candidate, _)| *candidate == code)
        .map(|(_, name)| *name)
        .unwrap_or(code)
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
}
