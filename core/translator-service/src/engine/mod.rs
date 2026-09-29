pub mod mock;

use crate::domain::translation::{
    TranslationRequest,
    TranslationResult,
};

pub trait TranslationEngine {
    fn translate(
        &self,
        request: TranslationRequest
    ) -> TranslationResult;
}
