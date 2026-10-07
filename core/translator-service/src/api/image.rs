use std::time::Instant;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use base64::Engine as _;
use serde::{Deserialize, Serialize};

use super::{translation_status, AppState, ErrorBody, ErrorResponse};
use crate::domain::translation::{GlossaryTerm, TranslationError};
use crate::image::{self, ImageError as PipelineError};

/// Largest accepted base64 payload (before decoding): 32 MiB.
const MAX_IMAGE_BYTES: usize = 32 * 1024 * 1024;

#[derive(Deserialize)]
pub(crate) struct ImageRequest {
    /// Base64 image bytes, optionally as a `data:` URL payload.
    image: String,
    source: String,
    target: String,
    #[serde(default)]
    glossary: Vec<GlossaryTerm>,
}

#[derive(Serialize)]
struct ImageSize {
    width: u32,
    height: u32,
}

#[derive(Serialize)]
struct ImageBlock {
    text: String,
    translation: String,
    score: f32,
    quad: [[f32; 2]; 4],
}

#[derive(Serialize)]
pub(crate) struct ImageResponse {
    image: ImageSize,
    blocks: Vec<ImageBlock>,
}

pub(crate) enum ImageError {
    InvalidRequest(String),
    OcrUnavailable,
    OcrFailed(String),
    Translation(TranslationError),
}

impl From<PipelineError> for ImageError {
    fn from(error: PipelineError) -> Self {
        match error {
            PipelineError::InvalidRequest(message) => Self::InvalidRequest(message),
            PipelineError::OcrUnavailable => Self::OcrUnavailable,
            PipelineError::OcrFailed(message) => Self::OcrFailed(message),
            PipelineError::Translation(error) => Self::Translation(error),
        }
    }
}

impl IntoResponse for ImageError {
    fn into_response(self) -> Response {
        let (status, kind, message) = match self {
            Self::InvalidRequest(message) => (StatusCode::BAD_REQUEST, "invalid_request", message),
            Self::OcrUnavailable => (
                StatusCode::NOT_IMPLEMENTED,
                "ocr_unavailable",
                "no OCR provider is configured for this service".to_string(),
            ),
            Self::OcrFailed(message) => (
                StatusCode::BAD_GATEWAY,
                "ocr_failed",
                format!("OCR failed: {message}"),
            ),
            Self::Translation(error) => {
                let status = translation_status(&error);
                let message = error.to_string();
                (status, error.kind(), message)
            }
        };

        (
            status,
            Json(ErrorResponse {
                error: ErrorBody { kind, message },
            }),
        )
            .into_response()
    }
}

/// Recognizes text in an uploaded image and translates every block with the
/// configured text engine (`crate::image::translate_image_bytes`).
pub(crate) async fn translate_image(
    State(state): State<AppState>,
    Json(payload): Json<ImageRequest>,
) -> Result<Json<ImageResponse>, ImageError> {
    let started = Instant::now();

    let Some(ocr) = state.ocr.clone() else {
        return Err(ImageError::OcrUnavailable);
    };

    let bytes = decode_image_payload(&payload.image)?;

    let translation = image::translate_image_bytes(
        &state.engine,
        &ocr,
        &bytes,
        &payload.source,
        &payload.target,
        &payload.glossary,
        state.max_chars,
    )
    .await?;

    let blocks: Vec<ImageBlock> = translation
        .blocks
        .into_iter()
        .map(|block| ImageBlock {
            text: block.text,
            translation: block.translation,
            score: block.score,
            quad: block.quad,
        })
        .collect();

    tracing::info!(
        source = %payload.source,
        target = %payload.target,
        image_width = translation.width,
        image_height = translation.height,
        blocks = blocks.len(),
        elapsed_ms = started.elapsed().as_millis() as u64,
        "image translation completed"
    );

    Ok(Json(ImageResponse {
        image: ImageSize {
            width: translation.width,
            height: translation.height,
        },
        blocks,
    }))
}

fn decode_image_payload(value: &str) -> Result<Vec<u8>, ImageError> {
    let cleaned: String = value
        .chars()
        .filter(|character| !character.is_ascii_whitespace())
        .collect();

    if cleaned.is_empty() {
        return Err(ImageError::InvalidRequest(
            "image must not be empty".to_string(),
        ));
    }

    let encoded = if cleaned.starts_with("data:") {
        cleaned
            .split_once(',')
            .map(|(_, data)| data)
            .unwrap_or(&cleaned)
    } else {
        &cleaned
    };

    if encoded.len() > MAX_IMAGE_BYTES {
        return Err(ImageError::InvalidRequest(format!(
            "image is too large: {} bytes of base64 (max {MAX_IMAGE_BYTES})",
            encoded.len()
        )));
    }

    base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|error| {
            ImageError::InvalidRequest(format!("image is not valid base64: {error}"))
        })
}
