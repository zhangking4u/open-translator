use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::domain::translation::{TranslationError, TranslationRequest};
use crate::engine::EngineRef;

pub fn router(engine: EngineRef) -> Router {
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
    State(engine): State<EngineRef>,
    Json(payload): Json<TranslateRequest>,
) -> Result<Json<TranslateResponse>, TranslationError> {
    if payload.text.trim().is_empty() {
        return Err(TranslationError::InvalidRequest(
            "text must not be empty".to_string(),
        ));
    }

    println!("Translate request: {}", payload.text);

    let request = TranslationRequest {
        text: payload.text,
        source: payload.source,
        target: payload.target,
    };

    let result = engine.translate(request).await?;

    Ok(Json(TranslateResponse {
        translation: result.translated_text,
    }))
}

#[derive(Serialize)]
struct ErrorResponse {
    error: ErrorBody,
}

#[derive(Serialize)]
struct ErrorBody {
    kind: &'static str,
    message: String,
}

impl IntoResponse for TranslationError {
    fn into_response(self) -> Response {
        let (status, kind) = match &self {
            TranslationError::InvalidRequest(_) => (StatusCode::BAD_REQUEST, "invalid_request"),
            TranslationError::EngineUnavailable(_) => {
                (StatusCode::BAD_GATEWAY, "engine_unavailable")
            }
            TranslationError::Timeout => (StatusCode::GATEWAY_TIMEOUT, "timeout"),
            TranslationError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        };

        let body = ErrorResponse {
            error: ErrorBody {
                kind,
                message: self.to_string(),
            },
        };

        (status, Json(body)).into_response()
    }
}
