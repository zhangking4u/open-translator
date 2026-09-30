use std::time::Instant;

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

#[derive(Clone)]
pub struct AppState {
    pub engine: EngineRef,
    pub engine_name: String,
    pub model: String,
    pub max_chars: usize,
}

impl AppState {
    pub fn new(
        engine: EngineRef,
        engine_name: impl Into<String>,
        model: impl Into<String>,
        max_chars: usize,
    ) -> Self {
        Self {
            engine,
            engine_name: engine_name.into(),
            model: model.into(),
            max_chars,
        }
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/translate", post(translate))
        .with_state(state)
}

async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "translator-core",
        engine: state.engine_name,
        model: state.model,
    })
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    service: &'static str,
    engine: String,
    model: String,
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
    State(state): State<AppState>,
    Json(payload): Json<TranslateRequest>,
) -> Result<Json<TranslateResponse>, TranslationError> {
    if payload.text.trim().is_empty() {
        return Err(TranslationError::InvalidRequest(
            "text must not be empty".to_string(),
        ));
    }

    let text_chars = payload.text.chars().count();
    if text_chars > state.max_chars {
        return Err(TranslationError::InvalidRequest(format!(
            "text is too long: {text_chars} chars (max {})",
            state.max_chars
        )));
    }

    let started = Instant::now();
    let source = payload.source;
    let target = payload.target;

    let result = state
        .engine
        .translate(TranslationRequest {
            text: payload.text,
            source: source.clone(),
            target: target.clone(),
        })
        .await;

    let elapsed_ms = started.elapsed().as_millis() as u64;

    match &result {
        Ok(_) => tracing::info!(
            source = %source,
            target = %target,
            text_chars,
            elapsed_ms,
            "translation completed"
        ),
        Err(error) => tracing::warn!(
            source = %source,
            target = %target,
            text_chars,
            elapsed_ms,
            error = %error,
            "translation failed"
        ),
    }

    Ok(Json(TranslateResponse {
        translation: result?.translated_text,
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
