use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

use translator_service::api;
use translator_service::engine::mock::MockEngine;

fn app() -> axum::Router {
    api::router(Arc::new(MockEngine))
}

async fn body_json(response: axum::response::Response) -> Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
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
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/translate")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"text":"hello world","source":"en","target":"zh"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let json = body_json(response).await;
    assert_eq!(json["translation"], "[Mock Translation] hello world");
}
