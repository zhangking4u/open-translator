use super::language::{display_name, display_name_zh, normalize_tag};
use super::translation::{TranslationError, TranslationRequest};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptStyle {
    Generic,
    TranslateGemma,
    HunYuanMt,
}

impl PromptStyle {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "generic" => Ok(Self::Generic),
            "translategemma" => Ok(Self::TranslateGemma),
            "hymt" => Ok(Self::HunYuanMt),
            other => Err(format!("unknown prompt style: {other}")),
        }
    }
}

pub fn translation_prompt(
    style: PromptStyle,
    request: &TranslationRequest,
) -> Result<String, TranslationError> {
    let source = normalize_tag(&request.source).map_err(TranslationError::InvalidRequest)?;
    let target = normalize_tag(&request.target).map_err(TranslationError::InvalidRequest)?;

    Ok(match style {
        PromptStyle::Generic => generic_prompt(request, &source, &target),
        PromptStyle::TranslateGemma => translategemma_prompt(request, &source, &target),
        PromptStyle::HunYuanMt => hunyuan_mt_prompt(request, &target),
    })
}

fn generic_prompt(request: &TranslationRequest, source: &str, target: &str) -> String {
    format!(
        "Translate the following text from {} ({source}) to {} ({target}).\n\
         Return only the translation without explanations.\n\n{}",
        display_name(source),
        display_name(target),
        request.text
    )
}

fn translategemma_prompt(request: &TranslationRequest, source: &str, target: &str) -> String {
    let source_name = display_name(source);
    let target_name = display_name(target);

    format!(
        "You are a professional {source_name} to {target_name} translator. \
         Your goal is to accurately convey the meaning and nuances of the original \
         {source_name} text while adhering to {target_name} grammar, vocabulary, and \
         cultural sensitivities.\nProduce only the {target_name} translation, without any \
         additional explanations or commentary. Please translate the following {source_name} \
         text into {target_name}:\n\n\n{}",
        request.text
    )
}

fn hunyuan_mt_prompt(request: &TranslationRequest, target: &str) -> String {
    format!(
        "将以下文本翻译为{}，注意只需要输出翻译后的结果，不要额外解释：\n\n{}",
        display_name_zh(target),
        request.text
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(text: &str, source: &str, target: &str) -> TranslationRequest {
        TranslationRequest {
            text: text.to_string(),
            source: source.to_string(),
            target: target.to_string(),
        }
    }

    #[test]
    fn parses_prompt_styles() {
        assert_eq!(PromptStyle::parse("generic"), Ok(PromptStyle::Generic));
        assert_eq!(
            PromptStyle::parse("translategemma"),
            Ok(PromptStyle::TranslateGemma)
        );
        assert_eq!(PromptStyle::parse("hymt"), Ok(PromptStyle::HunYuanMt));
        assert!(PromptStyle::parse("nope").is_err());
    }

    #[test]
    fn builds_generic_prompt() {
        let prompt =
            translation_prompt(PromptStyle::Generic, &request("hello", "EN", "zh_Hans")).unwrap();

        assert!(prompt.contains("English (en)"));
        assert!(prompt.contains("Chinese (zh-hans)"));
        assert!(prompt.contains("hello"));
    }

    #[test]
    fn builds_translategemma_prompt() {
        let prompt =
            translation_prompt(PromptStyle::TranslateGemma, &request("hello", "en", "zh")).unwrap();

        assert!(prompt.contains("professional English to Chinese translator"));
        assert!(prompt.contains("hello"));
    }

    #[test]
    fn builds_hymt_prompt() {
        let prompt =
            translation_prompt(PromptStyle::HunYuanMt, &request("hello", "en", "zh")).unwrap();

        assert!(prompt.contains("将以下文本翻译为中文"));
        assert!(prompt.contains("hello"));
    }

    #[test]
    fn rejects_invalid_language_tags() {
        let error = translation_prompt(PromptStyle::Generic, &request("hello", "en", "!!"))
            .unwrap_err();

        assert!(matches!(error, TranslationError::InvalidRequest(_)));
    }
}
