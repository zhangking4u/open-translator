use std::time::Duration;

use super::{EngineRef, TranslationEngine, TranslationFuture};
use crate::domain::translation::{TranslationError, TranslationRequest};

pub struct TimeoutEngine {
    inner: EngineRef,
    timeout: Duration,
}

impl TimeoutEngine {
    pub fn new(inner: EngineRef, timeout: Duration) -> Self {
        Self { inner, timeout }
    }
}

impl TranslationEngine for TimeoutEngine {
    fn translate(&self, request: TranslationRequest) -> TranslationFuture<'_> {
        Box::pin(async move {
            match tokio::time::timeout(self.timeout, self.inner.translate(request)).await {
                Ok(result) => result,
                Err(_) => Err(TranslationError::Timeout),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::engine::mock::MockEngine;

    #[tokio::test]
    async fn passes_through_fast_engine() {
        let engine = TimeoutEngine::new(Arc::new(MockEngine), Duration::from_millis(100));

        let result = engine
            .translate(TranslationRequest {
                text: "hi".to_string(),
                source: "en".to_string(),
                target: "zh".to_string(),
            })
            .await
            .unwrap();

        assert_eq!(result.translated_text, "[Mock Translation] hi");
    }
}
