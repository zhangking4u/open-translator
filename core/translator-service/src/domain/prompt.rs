use super::language::{display_name, normalize_tag};
use super::translation::{TranslationError, TranslationRequest};

pub fn translation_prompt(request: &TranslationRequest) -> Result<String, TranslationError> {
    let source = normalize_tag(&request.source).map_err(TranslationError::InvalidRequest)?;
    let target = normalize_tag(&request.target).map_err(TranslationError::InvalidRequest)?;

    Ok(format!(
        "Translate the following text from {} ({source}) to {} ({target}).\n\
         Return only the translation without explanations.\n\n{}",
        display_name(&source),
        display_name(&target),
        request.text
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_model_agnostic_prompt() {
        let prompt = translation_prompt(&TranslationRequest {
            text: "hello".to_string(),
            source: "EN".to_string(),
            target: "zh_Hans".to_string(),
        })
        .unwrap();

        assert!(prompt.contains("English (en)"));
        assert!(prompt.contains("Chinese (zh-hans)"));
        assert!(prompt.contains("hello"));
    }

    #[test]
    fn rejects_invalid_language_tags() {
        let error = translation_prompt(&TranslationRequest {
            text: "hello".to_string(),
            source: "en".to_string(),
            target: "!!".to_string(),
        })
        .unwrap_err();

        assert!(matches!(error, TranslationError::InvalidRequest(_)));
    }
}
