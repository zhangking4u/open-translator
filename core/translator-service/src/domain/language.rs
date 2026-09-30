const MAX_TAG_LEN: usize = 35;

pub fn normalize_tag(input: &str) -> Result<String, String> {
    let tag = input.trim().replace('_', "-").to_ascii_lowercase();

    if tag.is_empty() {
        return Err("language tag must not be empty".to_string());
    }

    if tag.len() > MAX_TAG_LEN {
        return Err(format!("language tag too long: {input}"));
    }

    let valid = tag.split('-').all(|part| {
        !part.is_empty() && part.len() <= 8 && part.chars().all(|c| c.is_ascii_alphanumeric())
    });

    if !valid {
        return Err(format!("invalid language tag: {input}"));
    }

    Ok(tag)
}

pub fn display_name(tag: &str) -> String {
    match base_tag(tag) {
        "ar" => "Arabic",
        "de" => "German",
        "en" => "English",
        "es" => "Spanish",
        "fr" => "French",
        "hi" => "Hindi",
        "it" => "Italian",
        "ja" => "Japanese",
        "ko" => "Korean",
        "pt" => "Portuguese",
        "ru" => "Russian",
        "th" => "Thai",
        "vi" => "Vietnamese",
        "zh" => "Chinese",
        _ => tag,
    }
    .to_string()
}

pub fn display_name_zh(tag: &str) -> String {
    match base_tag(tag) {
        "ar" => "阿拉伯语",
        "de" => "德语",
        "en" => "英语",
        "es" => "西班牙语",
        "fr" => "法语",
        "hi" => "印地语",
        "it" => "意大利语",
        "ja" => "日语",
        "ko" => "韩语",
        "pt" => "葡萄牙语",
        "ru" => "俄语",
        "th" => "泰语",
        "vi" => "越南语",
        "zh" => "中文",
        _ => tag,
    }
    .to_string()
}

fn base_tag(tag: &str) -> &str {
    tag.split('-').next().unwrap_or(tag)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_case_and_separators() {
        assert_eq!(normalize_tag(" ZH_CN ").unwrap(), "zh-cn");
        assert_eq!(normalize_tag("en-US").unwrap(), "en-us");
    }

    #[test]
    fn rejects_empty_tags() {
        assert!(normalize_tag("").is_err());
        assert!(normalize_tag("  ").is_err());
    }

    #[test]
    fn rejects_invalid_tags() {
        assert!(normalize_tag("en!").is_err());
        assert!(normalize_tag("en--us").is_err());
    }

    #[test]
    fn maps_common_languages_to_names() {
        assert_eq!(display_name("zh-hans"), "Chinese");
        assert_eq!(display_name("en"), "English");
        assert_eq!(display_name("xx"), "xx");
    }

    #[test]
    fn maps_common_languages_to_chinese_names() {
        assert_eq!(display_name_zh("zh-hans"), "中文");
        assert_eq!(display_name_zh("en"), "英语");
        assert_eq!(display_name_zh("xx"), "xx");
    }
}
