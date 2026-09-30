use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

use translator_service::api;
use translator_service::domain::translation::{TranslationError, TranslationRequest};
use translator_service::engine::mock::MockEngine;
use translator_service::engine::{TranslationEngine, TranslationFuture};

fn app() -> axum::Router {
    api::router(Arc::new(MockEngine))
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
    api::router(Arc::new(FailingEngine))
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
