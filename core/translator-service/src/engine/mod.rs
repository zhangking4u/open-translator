use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::domain::translation::{TranslationError, TranslationRequest, TranslationResult};

pub mod mock;

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

pub fn build(kind: EngineKind) -> EngineRef {
    match kind {
        EngineKind::Mock => Arc::new(mock::MockEngine),
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
    fn rejects_unknown_engine_kind() {
        assert!(EngineKind::parse("nope").is_err());
    }
}
