use super::TranslationEngine;

use crate::domain::translation::{
    TranslationRequest,
    TranslationResult,
};

pub struct MockEngine;

impl TranslationEngine for MockEngine {
    fn translate(
        &self,
        request: TranslationRequest
    ) 
    -> TranslationResult {
        TranslationResult {
            translated_text:
                format!(
                    "[Mock Translation] {}",
                    request.text
                ),
        }
    }
}
