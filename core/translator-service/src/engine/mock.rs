use super::{TranslationEngine, TranslationFuture};

use crate::domain::translation::{TranslationRequest, TranslationResult};

pub struct MockEngine;

impl TranslationEngine for MockEngine {
    fn translate(&self, request: TranslationRequest) -> TranslationFuture<'_> {
        Box::pin(async move {
            Ok(TranslationResult {
                translated_text: format!("[Mock Translation] {}", request.text),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn prefixes_input_text() {
        let result = MockEngine
            .translate(TranslationRequest {
                text: "hello world".to_string(),
                source: "en".to_string(),
                target: "zh".to_string(),
            })
            .await
            .unwrap();

        assert_eq!(result.translated_text, "[Mock Translation] hello world");
    }
}
