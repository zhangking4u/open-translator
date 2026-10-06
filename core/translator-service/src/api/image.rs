use std::io::Cursor;
use std::time::Instant;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use base64::Engine as _;
use serde::{Deserialize, Serialize};

use super::{translation_status, AppState, ErrorBody, ErrorResponse};
use crate::domain::translation::{GlossaryTerm, TranslationError, TranslationRequest};
use crate::engine::EngineRef;

/// Largest accepted base64 payload (before decoding): 32 MiB.
const MAX_IMAGE_BYTES: usize = 32 * 1024 * 1024;
/// Largest accepted image in pixels (a 4K screenshot is ~8.3 MP).
const MAX_IMAGE_PIXELS: u64 = 16_000_000;

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

impl From<TranslationError> for ImageError {
    fn from(error: TranslationError) -> Self {
        Self::Translation(error)
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
/// configured text engine. The engine is serialized, so all blocks go through
/// one joined request; a per-block fallback covers models that do not keep
/// one line per input line.
pub(crate) async fn translate_image(
    State(state): State<AppState>,
    Json(payload): Json<ImageRequest>,
) -> Result<Json<ImageResponse>, ImageError> {
    let started = Instant::now();

    let Some(ocr) = state.ocr.clone() else {
        return Err(ImageError::OcrUnavailable);
    };

    let bytes = decode_image_payload(&payload.image)?;
    let (width, height, pixels) = decode_rgb(&bytes)?;

    let blocks = tokio::task::spawn_blocking(move || ocr.recognize(&pixels, width, height))
        .await
        .map_err(|error| ImageError::OcrFailed(format!("OCR task failed: {error}")))?
        .map_err(|error| ImageError::OcrFailed(error.to_string()))?;

    let blocks: Vec<translator_ocr::OcrBlock> = blocks
        .into_iter()
        .filter(|block| !block.text.trim().is_empty())
        .collect();

    let texts: Vec<String> = blocks
        .iter()
        .map(|block| block.text.trim().to_string())
        .collect();
    let joined_chars = texts
        .iter()
        .map(|text| text.chars().count())
        .sum::<usize>()
        + texts.len().saturating_sub(1);

    if joined_chars > state.max_chars {
        return Err(ImageError::InvalidRequest(format!(
            "image text is too long: {joined_chars} chars (max {})",
            state.max_chars
        )));
    }

    let translations = translate_blocks(
        &state.engine,
        texts,
        &payload.source,
        &payload.target,
        &payload.glossary,
    )
    .await?;

    let blocks: Vec<ImageBlock> = blocks
        .into_iter()
        .zip(translations)
        .map(|(block, translation)| ImageBlock {
            text: block.text,
            translation,
            score: block.score,
            quad: block.quad.points,
        })
        .collect();

    tracing::info!(
        source = %payload.source,
        target = %payload.target,
        image_width = width,
        image_height = height,
        blocks = blocks.len(),
        elapsed_ms = started.elapsed().as_millis() as u64,
        "image translation completed"
    );

    Ok(Json(ImageResponse {
        image: ImageSize { width, height },
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

fn decode_rgb(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), ImageError> {
    let format = image::guess_format(bytes).map_err(|error| {
        ImageError::InvalidRequest(format!("unrecognized image format: {error}"))
    })?;

    let (width, height) = image::ImageReader::with_format(Cursor::new(bytes), format)
        .into_dimensions()
        .map_err(|error| ImageError::InvalidRequest(format!("unreadable image: {error}")))?;

    if width == 0 || height == 0 {
        return Err(ImageError::InvalidRequest(
            "image has zero size".to_string(),
        ));
    }

    if u64::from(width) * u64::from(height) > MAX_IMAGE_PIXELS {
        return Err(ImageError::InvalidRequest(format!(
            "image is too large: {width}x{height} (max {MAX_IMAGE_PIXELS} pixels)"
        )));
    }

    let rgb = image::load_from_memory_with_format(bytes, format)
        .map_err(|error| ImageError::InvalidRequest(format!("undecodable image: {error}")))?
        .to_rgb8();

    Ok((width, height, rgb.into_raw()))
}

async fn translate_blocks(
    engine: &EngineRef,
    texts: Vec<String>,
    source: &str,
    target: &str,
    glossary: &[GlossaryTerm],
) -> Result<Vec<String>, TranslationError> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }

    // One request for the whole page keeps the interactive latency budget
    // (the engine is serialized); the model is asked to keep one line per
    // input line.
    let translation = engine
        .translate(TranslationRequest {
            text: texts.join("\n"),
            source: source.to_string(),
            target: target.to_string(),
            glossary: glossary.to_vec(),
        })
        .await?;

    let lines: Vec<String> = translation
        .translated_text
        .lines()
        .map(|line| line.trim().to_string())
        .collect();

    if lines.len() == texts.len() {
        return Ok(lines);
    }

    // Some models merge or split lines; translate block by block so every
    // block still gets its own translation (slower, but correctly paired).
    let mut translations = Vec::with_capacity(texts.len());

    for text in &texts {
        let result = engine
            .translate(TranslationRequest {
                text: text.clone(),
                source: source.to_string(),
                target: target.to_string(),
                glossary: glossary.to_vec(),
            })
            .await?;
        translations.push(result.translated_text.trim().to_string());
    }

    Ok(translations)
}
