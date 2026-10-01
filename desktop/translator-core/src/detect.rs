//! Lightweight source-language detection for the `auto` source mode.

use whatlang::Lang;

use crate::languages::{AUTO_CODE, is_supported};

/// Minimum trigram confidence for trusting a detected language.
///
/// Languages with a unique script (Chinese, Japanese, Korean, Russian) come
/// back with a confidence of 1.0, so the threshold mainly filters ambiguous
/// Latin-script snippets.
const MIN_CONFIDENCE: f64 = 0.5;

/// Detects the language of `text` and maps it to a supported language tag.
///
/// Returns `None` when the detection is unsure or the language is not in the
/// shared desktop language list, in which case callers should keep `auto`.
pub fn detect(text: &str) -> Option<&'static str> {
    let info = whatlang::detect(text)?;
    if info.confidence() < MIN_CONFIDENCE {
        return None;
    }

    let tag = tag_of(info.lang())?;

    is_supported(tag).then_some(tag)
}

/// Resolves the effective source tag for a translation request.
///
/// Explicit tags pass through untouched; `auto` is replaced by a detected
/// language when one is found and stays `auto` otherwise. The second value is
/// the detected tag when detection was used, for display purposes.
pub fn resolve_source(configured: &str, text: &str) -> (String, Option<&'static str>) {
    if configured != AUTO_CODE {
        return (configured.to_string(), None);
    }

    match detect(text) {
        Some(tag) => (tag.to_string(), Some(tag)),
        None => (AUTO_CODE.to_string(), None),
    }
}

fn tag_of(lang: Lang) -> Option<&'static str> {
    match lang {
        Lang::Cmn => Some("zh"),
        Lang::Eng => Some("en"),
        Lang::Jpn => Some("ja"),
        Lang::Kor => Some("ko"),
        Lang::Fra => Some("fr"),
        Lang::Deu => Some("de"),
        Lang::Spa => Some("es"),
        Lang::Rus => Some("ru"),
        Lang::Tha => Some("th"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_common_languages() {
        assert_eq!(detect("This is a short sentence written in English."), Some("en"));
        assert_eq!(detect("这是一段用中文写的话，用来测试语言检测。"), Some("zh"));
        assert_eq!(detect("これは日本語で書かれた文章です。"), Some("ja"));
        assert_eq!(detect("이 문장은 한국어로 작성되었습니다."), Some("ko"));
        assert_eq!(detect("Это предложение написано на русском языке."), Some("ru"));
        assert_eq!(
            detect("นี่คือประโยคที่เขียนเป็นภาษาไทยเพื่อทดสอบการตรวจจับภาษา"),
            Some("th")
        );
    }

    #[test]
    fn ignores_unknown_or_empty_text() {
        assert_eq!(detect(""), None);
        assert_eq!(detect("1234 !?"), None);
    }

    #[test]
    fn resolves_auto_source() {
        let (source, detected) = resolve_source(AUTO_CODE, "This is an English sentence.");
        assert_eq!(source, "en");
        assert_eq!(detected, Some("en"));

        let (source, detected) = resolve_source(AUTO_CODE, "1234 !?");
        assert_eq!(source, AUTO_CODE);
        assert_eq!(detected, None);
    }

    #[test]
    fn keeps_explicit_source() {
        let (source, detected) = resolve_source("fr", "This is an English sentence.");
        assert_eq!(source, "fr");
        assert_eq!(detected, None);
    }
}
