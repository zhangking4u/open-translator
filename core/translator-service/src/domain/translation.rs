#[derive(Debug)]
pub struct TranslationRequest {
    pub text: String,
    pub source: String,
    pub target: String,
}

#[derive(Debug)]
pub struct TranslationResult {
    pub translated_text: String,
}
