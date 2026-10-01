use std::convert::Infallible;
use std::time::Instant;

use axum::{
    extract::State,
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{get, post},
    Json, Router,
};
use futures_util::stream::{self, Stream};
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
        .route("/translate/stream", post(translate_stream))
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

fn validate(text: &str, max_chars: usize) -> Result<usize, TranslationError> {
    if text.trim().is_empty() {
        return Err(TranslationError::InvalidRequest(
            "text must not be empty".to_string(),
        ));
    }

    let text_chars = text.chars().count();
    if text_chars > max_chars {
        return Err(TranslationError::InvalidRequest(format!(
            "text is too long: {text_chars} chars (max {max_chars})"
        )));
    }

    Ok(text_chars)
}

async fn translate(
    State(state): State<AppState>,
    Json(payload): Json<TranslateRequest>,
) -> Result<Json<TranslateResponse>, TranslationError> {
    let text_chars = validate(&payload.text, state.max_chars)?;

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

/// Events emitted by `POST /translate/stream` as server-sent events:
/// `delta` chunks while the model decodes, then a terminal `done` (with the
/// complete translation) or `error` event.
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum StreamEvent {
    Delta { delta: String },
    Done { translation: String, elapsed_ms: u64 },
    Error { kind: &'static str, message: String },
}

async fn translate_stream(
    State(state): State<AppState>,
    Json(payload): Json<TranslateRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, TranslationError> {
    let text_chars = validate(&payload.text, state.max_chars)?;

    let (sender, receiver) = tokio::sync::mpsc::unbounded_channel::<StreamEvent>();

    let callback_sender = sender.clone();
    let engine = state.engine.clone();
    let source = payload.source.clone();
    let target = payload.target.clone();

    tokio::spawn(async move {
        let started = Instant::now();

        let result = engine
            .translate_streaming(
                TranslationRequest {
                    text: payload.text,
                    source: source.clone(),
                    target: target.clone(),
                },
                Box::new(move |delta| {
                    let _ = callback_sender.send(StreamEvent::Delta {
                        delta: delta.to_string(),
                    });
                }),
            )
            .await;

        let elapsed_ms = started.elapsed().as_millis() as u64;

        let event = match &result {
            Ok(translation) => {
                tracing::info!(
                    source = %source,
                    target = %target,
                    text_chars,
                    elapsed_ms,
                    "translation completed (stream)"
                );

                StreamEvent::Done {
                    translation: translation.translated_text.clone(),
                    elapsed_ms,
                }
            }
            Err(error) => {
                tracing::warn!(
                    source = %source,
                    target = %target,
                    text_chars,
                    elapsed_ms,
                    error = %error,
                    "translation failed (stream)"
                );

                StreamEvent::Error {
                    kind: error.kind(),
                    message: error.to_string(),
                }
            }
        };

        let _ = sender.send(event);
    });

    // The unfold state ends the stream right after the terminal event, so a
    // late decoder callback cannot keep the response open.
    let stream = stream::unfold((receiver, false), |(mut receiver, finished)| async move {
        if finished {
            return None;
        }

        let event = receiver.recv().await?;
        let finished = matches!(event, StreamEvent::Done { .. } | StreamEvent::Error { .. });
        let event = Event::default()
            .json_data(event)
            .unwrap_or_else(|_| Event::default().data("{\"type\":\"error\"}"));

        Some((Ok::<_, Infallible>(event), (receiver, finished)))
    });

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
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
        let status = match &self {
            TranslationError::InvalidRequest(_) => StatusCode::BAD_REQUEST,
            TranslationError::EngineUnavailable(_) => StatusCode::BAD_GATEWAY,
            TranslationError::Timeout => StatusCode::GATEWAY_TIMEOUT,
            TranslationError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };

        let body = ErrorResponse {
            error: ErrorBody {
                kind: self.kind(),
                message: self.to_string(),
            },
        };

        (status, Json(body)).into_response()
    }
}
