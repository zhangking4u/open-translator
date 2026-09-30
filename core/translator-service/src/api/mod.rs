use std::sync::Arc;

use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::domain::translation::TranslationRequest;
use crate::engine::TranslationEngine;

pub type EngineState = Arc<dyn TranslationEngine>;

pub fn router(engine: EngineState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/translate", post(translate))
        .with_state(engine)
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "translator-core",
    })
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    service: &'static str,
}

#[derive(Deserialize)]
struct TranslateRequest {
    text: String,
    source: String,
    target: String,
}

#[derive(Serialize)]
struct TranslateResponse {
    translation: String,
}

async fn translate(
    State(engine): State<EngineState>,
    Json(payload): Json<TranslateRequest>,
) -> Json<TranslateResponse> {
    println!("Translate request: {}", payload.text);

    let request = TranslationRequest {
        text: payload.text,
        source: payload.source,
        target: payload.target,
    };

    let result = engine.translate(request).await;

    Json(TranslateResponse {
        translation: result.translated_text,
    })
}
