use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{Value, json};

use translator_service::domain::translation::{TranslationError, TranslationRequest};
use translator_service::engine::TranslationEngine;
use translator_service::engine::ollama::OllamaEngine;

#[derive(Clone, Default)]
struct Recorder {
    requests: Arc<Mutex<Vec<Value>>>,
}

async fn spawn_stub(recorder: Recorder, response: Value, status: u16) -> String {
    let app = Router::new()
        .route(
            "/api/generate",
            post(
                move |State(recorder): State<Recorder>, Json(payload): Json<Value>| async move {
                    recorder.requests.lock().unwrap().push(payload);
                    (
                        StatusCode::from_u16(status).unwrap(),
                        Json(response.clone()),
                    )
                },
            ),
        )
        .with_state(recorder);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    format!("http://{address}")
}

fn engine(base_url: &str) -> OllamaEngine {
    OllamaEngine::new(base_url.to_string(), "test-model".to_string())
}

fn request() -> TranslationRequest {
    TranslationRequest {
        text: "kernel panic".to_string(),
        source: "en".to_string(),
        target: "zh".to_string(),
    }
}

#[tokio::test]
async fn translates_via_ollama_generate_api() {
    let recorder = Recorder::default();
    let base_url = spawn_stub(recorder.clone(), json!({ "response": " 内核崩溃 " }), 200).await;

    let result = engine(&base_url).translate(request()).await.unwrap();

    assert_eq!(result.translated_text, "内核崩溃");

    let recorded = recorder.requests.lock().unwrap();
    let payload = recorded.first().expect("request was recorded");
    assert_eq!(payload["model"], "test-model");
    assert_eq!(payload["stream"], false);

    let prompt = payload["prompt"].as_str().unwrap();
    assert!(prompt.contains("English (en)"));
    assert!(prompt.contains("Chinese (zh)"));
    assert!(prompt.contains("kernel panic"));
}

#[tokio::test]
async fn maps_http_errors_to_engine_unavailable() {
    let base_url = spawn_stub(
        Recorder::default(),
        json!({ "error": "model not found" }),
        404,
    )
    .await;

    let error = engine(&base_url).translate(request()).await.unwrap_err();

    assert!(matches!(error, TranslationError::EngineUnavailable(_)));
}

#[tokio::test]
async fn maps_invalid_response_to_internal_error() {
    let base_url = spawn_stub(Recorder::default(), json!({ "unexpected": true }), 200).await;

    let error = engine(&base_url).translate(request()).await.unwrap_err();

    assert!(matches!(error, TranslationError::Internal(_)));
}

#[tokio::test]
async fn maps_connection_errors_to_engine_unavailable() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);

    let error = engine(&format!("http://{address}"))
        .translate(request())
        .await
        .unwrap_err();

    assert!(matches!(error, TranslationError::EngineUnavailable(_)));
}
