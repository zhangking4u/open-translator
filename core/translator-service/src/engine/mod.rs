use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::config::Config;
use crate::domain::translation::{TranslationError, TranslationRequest, TranslationResult};

pub mod mock;
pub mod ollama;
mod timeout;

pub use timeout::TimeoutEngine;

pub type TranslationFuture<'a> = Pin<
    Box<dyn Future<Output = Result<TranslationResult, TranslationError>> + Send + 'a>,
>;

pub type EngineRef = Arc<dyn TranslationEngine>;

pub trait TranslationEngine: Send + Sync {
    fn translate(&self, request: TranslationRequest) -> TranslationFuture<'_>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineKind {
    Mock,
    Ollama,
}

impl EngineKind {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "mock" => Ok(Self::Mock),
            "ollama" => Ok(Self::Ollama),
            other => Err(format!("unknown engine kind: {other}")),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Mock => "mock",
            Self::Ollama => "ollama",
        }
    }
}

pub fn build(config: &Config) -> EngineRef {
    let engine: EngineRef = match config.engine {
        EngineKind::Mock => Arc::new(mock::MockEngine),
        EngineKind::Ollama => Arc::new(ollama::OllamaEngine::new(
            config.model_url.clone(),
            config.model.clone(),
            config.prompt_style,
            config.keep_alive.clone(),
        )),
    };

    Arc::new(TimeoutEngine::new(engine, config.timeout))
}

pub async fn warmup(engine: &EngineRef) {
    let request = TranslationRequest {
        text: "hello".to_string(),
        source: "en".to_string(),
        target: "zh".to_string(),
    };

    match engine.translate(request).await {
        Ok(_) => tracing::info!("engine warmup completed"),
        Err(error) => {
            tracing::warn!(error = %error, "engine warmup failed; continuing without warmup");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mock_engine_kind() {
        assert_eq!(EngineKind::parse("mock"), Ok(EngineKind::Mock));
    }

    #[test]
    fn parses_ollama_engine_kind() {
        assert_eq!(EngineKind::parse("ollama"), Ok(EngineKind::Ollama));
    }

    #[test]
    fn rejects_unknown_engine_kind() {
        assert!(EngineKind::parse("nope").is_err());
    }

    #[test]
    fn engine_kind_names_match_config() {
        assert_eq!(EngineKind::Mock.as_str(), "mock");
        assert_eq!(EngineKind::Ollama.as_str(), "ollama");
    }

    #[tokio::test]
    async fn warmup_survives_failing_engine() {
        struct FailingEngine;

        impl TranslationEngine for FailingEngine {
            fn translate(&self, _request: TranslationRequest) -> TranslationFuture<'_> {
                Box::pin(async { Err(TranslationError::EngineUnavailable("down".to_string())) })
            }
        }

        let engine: EngineRef = Arc::new(FailingEngine);
        warmup(&engine).await;
    }
}
