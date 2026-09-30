use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use crate::domain::translation::{TranslationError, TranslationRequest, TranslationResult};

pub mod mock;
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
}

impl EngineKind {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "mock" => Ok(Self::Mock),
            other => Err(format!("unknown engine kind: {other}")),
        }
    }
}

pub fn build(kind: EngineKind, timeout: Duration) -> EngineRef {
    let engine: EngineRef = match kind {
        EngineKind::Mock => Arc::new(mock::MockEngine),
    };

    Arc::new(TimeoutEngine::new(engine, timeout))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mock_engine_kind() {
        assert_eq!(EngineKind::parse("mock"), Ok(EngineKind::Mock));
    }

    #[test]
    fn rejects_unknown_engine_kind() {
        assert!(EngineKind::parse("nope").is_err());
    }
}
