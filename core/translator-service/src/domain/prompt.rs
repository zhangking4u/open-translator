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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SamplingOptions {
    pub temperature: f32,
    pub top_p: Option<f32>,
    pub top_k: Option<u32>,
    pub repeat_penalty: Option<f32>,
}

impl PromptStyle {
    pub fn sampling(self) -> SamplingOptions {
        match self {
            PromptStyle::HunYuanMt => SamplingOptions {
                temperature: 0.7,
                top_p: Some(0.6),
                top_k: Some(20),
                repeat_penalty: Some(1.05),
            },
            PromptStyle::Generic | PromptStyle::TranslateGemma => SamplingOptions {
                temperature: 0.0,
                top_p: None,
                top_k: None,
                repeat_penalty: None,
            },
        }
    }

    pub fn stop_strings(self) -> Vec<String> {
        match self {
            PromptStyle::HunYuanMt => vec![
                "<｜hy_place▁holder▁no▁2｜>".to_string(),
                "<｜hy_end▁of▁sentence｜>".to_string(),
            ],
            PromptStyle::Generic | PromptStyle::TranslateGemma => Vec::new(),
        }
    }

    /// Wraps a prompt for engines that tokenize raw text (no chat template applied by the runtime).
    pub fn raw_prompt(self, prompt: &str) -> String {
        match self {
            PromptStyle::HunYuanMt => {
                format!("<｜hy_begin▁of▁sentence｜><｜hy_User｜>{prompt}<｜hy_Assistant｜>")
            }
            PromptStyle::Generic | PromptStyle::TranslateGemma => prompt.to_string(),
        }
    }
}

pub fn translation_prompt(
    style: PromptStyle,
    request: &TranslationRequest,
) -> Result<String, TranslationError> {
    let source = normalize_tag(&request.source).map_err(TranslationError::InvalidRequest)?;
    let target = normalize_tag(&request.target).map_err(TranslationError::InvalidRequest)?;

    let automatic = source == "auto";

    Ok(match style {
        PromptStyle::Generic if automatic => generic_auto_prompt(request, &target),
        PromptStyle::Generic => generic_prompt(request, &source, &target),
        PromptStyle::TranslateGemma if automatic => translategemma_auto_prompt(request, &target),
        PromptStyle::TranslateGemma => translategemma_prompt(request, &source, &target),
        PromptStyle::HunYuanMt => hunyuan_mt_prompt(request, &target),
    })
}

fn generic_auto_prompt(request: &TranslationRequest, target: &str) -> String {
    format!(
        "Translate the following text into {} ({target}).\n\
         Return only the translation without explanations.\n\n{}",
        display_name(target),
        request.text
    )
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

fn translategemma_auto_prompt(request: &TranslationRequest, target: &str) -> String {
    let target_name = display_name(target);

    format!(
        "You are a professional translator. Your goal is to accurately convey the meaning \
         and nuances of the original text while adhering to {target_name} grammar, vocabulary, \
         and cultural sensitivities.\nProduce only the {target_name} translation, without any \
         additional explanations or commentary. Translate the following text into {target_name} \
         ({target}):\n\n\n{}",
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
    fn exposes_sampling_and_stop_strings_per_style() {
        let hymt = PromptStyle::HunYuanMt.sampling();
        assert_eq!(hymt.temperature, 0.7);
        assert_eq!(hymt.top_p, Some(0.6));
        assert_eq!(hymt.top_k, Some(20));
        assert_eq!(hymt.repeat_penalty, Some(1.05));
        assert!(!PromptStyle::HunYuanMt.stop_strings().is_empty());

        let generic = PromptStyle::Generic.sampling();
        assert_eq!(generic.temperature, 0.0);
        assert_eq!(generic.top_p, None);
        assert!(PromptStyle::Generic.stop_strings().is_empty());
    }

    #[test]
    fn wraps_raw_prompts_per_style() {
        let wrapped = PromptStyle::HunYuanMt.raw_prompt("hello");
        assert!(wrapped.starts_with("<｜hy_begin▁of▁sentence｜><｜hy_User｜>"));
        assert!(wrapped.ends_with("<｜hy_Assistant｜>"));

        assert_eq!(PromptStyle::Generic.raw_prompt("hello"), "hello");
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
    fn builds_auto_source_prompts() {
        let generic =
            translation_prompt(PromptStyle::Generic, &request("hello", "auto", "zh")).unwrap();
        assert!(generic.contains("into Chinese (zh)"));
        assert!(!generic.contains("from"));

        let gemma =
            translation_prompt(PromptStyle::TranslateGemma, &request("hello", "auto", "zh"))
                .unwrap();
        assert!(gemma.contains("professional translator"));
        assert!(!gemma.contains(" from "));

        let hymt =
            translation_prompt(PromptStyle::HunYuanMt, &request("hello", "auto", "zh")).unwrap();
        assert!(hymt.contains("将以下文本翻译为中文"));
    }

    #[test]
    fn rejects_invalid_language_tags() {
        let error = translation_prompt(PromptStyle::Generic, &request("hello", "en", "!!"))
            .unwrap_err();

        assert!(matches!(error, TranslationError::InvalidRequest(_)));
    }
}
