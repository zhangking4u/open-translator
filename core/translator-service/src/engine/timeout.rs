use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::{DeltaCallback, EngineRef, TranslationEngine, TranslationFuture};
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

    /// Streaming uses an idle timeout: the timer restarts whenever a delta
    /// arrives, so a long generation is fine while a stalled one still fails
    /// with [`TranslationError::Timeout`].
    fn translate_streaming<'a>(
        &'a self,
        request: TranslationRequest,
        mut on_delta: DeltaCallback,
    ) -> TranslationFuture<'a> {
        let timeout = self.timeout;

        Box::pin(async move {
            let activity = Arc::new(Mutex::new(tokio::time::Instant::now()));
            let tracker = activity.clone();

            let inner = self.inner.translate_streaming(
                request,
                Box::new(move |delta| {
                    if let Ok(mut last) = tracker.lock() {
                        *last = tokio::time::Instant::now();
                    }

                    on_delta(delta);
                }),
            );

            tokio::pin!(inner);

            loop {
                let deadline = match activity.lock() {
                    Ok(last) => *last + timeout,
                    Err(_) => tokio::time::Instant::now() + timeout,
                };

                tokio::select! {
                    result = &mut inner => return result,
                    _ = tokio::time::sleep_until(deadline) => {
                        let expired = match activity.lock() {
                            Ok(last) => tokio::time::Instant::now() >= *last + timeout,
                            Err(_) => true,
                        };

                        if expired {
                            return Err(TranslationError::Timeout);
                        }
                    }
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::domain::translation::TranslationResult;
    use crate::engine::mock::MockEngine;

    fn request() -> TranslationRequest {
        TranslationRequest {
            text: "hi".to_string(),
            source: "en".to_string(),
            target: "zh".to_string(),
        }
    }

    #[tokio::test]
    async fn passes_through_fast_engine() {
        let engine = TimeoutEngine::new(Arc::new(MockEngine), Duration::from_millis(100));

        let result = engine.translate(request()).await.unwrap();

        assert_eq!(result.translated_text, "[Mock Translation] hi");
    }

    #[tokio::test]
    async fn streaming_forwards_deltas() {
        let engine = TimeoutEngine::new(Arc::new(MockEngine), Duration::from_millis(100));
        let deltas = Arc::new(Mutex::new(Vec::new()));
        let sink = deltas.clone();

        let result = engine
            .translate_streaming(
                request(),
                Box::new(move |delta| sink.lock().unwrap().push(delta.to_string())),
            )
            .await
            .unwrap();

        assert_eq!(result.translated_text, "[Mock Translation] hi");
        assert_eq!(*deltas.lock().unwrap(), vec!["[Mock Translation] hi"]);
    }

    struct SilentEngine;

    impl TranslationEngine for SilentEngine {
        fn translate(&self, _request: TranslationRequest) -> TranslationFuture<'_> {
            Box::pin(async {
                Ok(TranslationResult {
                    translated_text: "late".to_string(),
                })
            })
        }

        fn translate_streaming<'a>(
            &'a self,
            _request: TranslationRequest,
            _on_delta: DeltaCallback,
        ) -> TranslationFuture<'a> {
            Box::pin(async {
                tokio::time::sleep(Duration::from_millis(200)).await;

                Ok(TranslationResult {
                    translated_text: "late".to_string(),
                })
            })
        }
    }

    #[tokio::test]
    async fn streaming_times_out_between_deltas() {
        let engine = TimeoutEngine::new(Arc::new(SilentEngine), Duration::from_millis(30));

        let error = engine
            .translate_streaming(request(), Box::new(|_| {}))
            .await
            .unwrap_err();

        assert!(matches!(error, TranslationError::Timeout));
    }
}
