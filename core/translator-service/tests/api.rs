use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

use translator_service::api::{self, AppState};
use translator_service::domain::translation::{
    TranslationError, TranslationRequest, TranslationResult,
};
use translator_service::engine::mock::MockEngine;
use translator_service::engine::{EngineRef, TimeoutEngine, TranslationEngine, TranslationFuture};

fn router_for(engine: EngineRef) -> axum::Router {
    api::router(AppState::new(engine, "mock", ""))
}

fn app() -> axum::Router {
    router_for(Arc::new(MockEngine))
}

struct FailingEngine;

impl TranslationEngine for FailingEngine {
    fn translate(&self, _request: TranslationRequest) -> TranslationFuture<'_> {
        Box::pin(async {
            Err(TranslationError::EngineUnavailable(
                "model offline".to_string(),
            ))
        })
    }
}

fn failing_app() -> axum::Router {
    router_for(Arc::new(FailingEngine))
}

struct SlowEngine;

impl TranslationEngine for SlowEngine {
    fn translate(&self, _request: TranslationRequest) -> TranslationFuture<'_> {
        Box::pin(async {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(TranslationResult {
                translated_text: "late".to_string(),
            })
        })
    }
}

async fn body_json(response: axum::response::Response) -> Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

fn translate_request(body: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/translate")
        .header("content-type", "application/json")
        .body(Body::from(body.to_owned()))
        .unwrap()
}

#[tokio::test]
async fn health_returns_service_status() {
    let response = app()
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let json = body_json(response).await;
    assert_eq!(json["status"], "ok");
    assert_eq!(json["service"], "translator-core");
    assert_eq!(json["engine"], "mock");
    assert_eq!(json["model"], "");
}

#[tokio::test]
async fn translate_returns_mock_translation() {
    let response = app()
        .oneshot(translate_request(
            r#"{"text":"hello world","source":"en","target":"zh"}"#,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let json = body_json(response).await;
    assert_eq!(json["translation"], "[Mock Translation] hello world");
}

#[tokio::test]
async fn empty_text_is_rejected() {
    let response = app()
        .oneshot(translate_request(
            r#"{"text":"   ","source":"en","target":"zh"}"#,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let json = body_json(response).await;
    assert_eq!(json["error"]["kind"], "invalid_request");
}

#[tokio::test]
async fn engine_failure_returns_bad_gateway() {
    let response = failing_app()
        .oneshot(translate_request(
            r#"{"text":"hello","source":"en","target":"zh"}"#,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);

    let json = body_json(response).await;
    assert_eq!(json["error"]["kind"], "engine_unavailable");
}

#[tokio::test]
async fn slow_engine_returns_gateway_timeout() {
    let slow: EngineRef = Arc::new(TimeoutEngine::new(
        Arc::new(SlowEngine),
        Duration::from_millis(10),
    ));

    let response = router_for(slow)
        .oneshot(translate_request(
            r#"{"text":"hello","source":"en","target":"zh"}"#,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::GATEWAY_TIMEOUT);

    let json = body_json(response).await;
    assert_eq!(json["error"]["kind"], "timeout");
}
