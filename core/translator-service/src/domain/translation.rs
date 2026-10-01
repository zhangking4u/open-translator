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

#[derive(Debug)]
pub enum TranslationError {
    InvalidRequest(String),
    EngineUnavailable(String),
    Timeout,
    Internal(String),
}

impl TranslationError {
    /// Stable machine-readable kind used by the HTTP error body and the
    /// streaming error event.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::InvalidRequest(_) => "invalid_request",
            Self::EngineUnavailable(_) => "engine_unavailable",
            Self::Timeout => "timeout",
            Self::Internal(_) => "internal",
        }
    }
}

impl std::fmt::Display for TranslationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TranslationError::InvalidRequest(message) => {
                write!(formatter, "invalid request: {message}")
            }
            TranslationError::EngineUnavailable(message) => {
                write!(formatter, "engine unavailable: {message}")
            }
            TranslationError::Timeout => write!(formatter, "translation timed out"),
            TranslationError::Internal(message) => write!(formatter, "internal error: {message}"),
        }
    }
}

impl std::error::Error for TranslationError {}
