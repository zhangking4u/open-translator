//! Image translation pipeline shared by the HTTP endpoint and embedded
//! clients: the desktop screenshot translation calls this in-process instead
//! of round-tripping through the extension API.
//!
//! The OCR provider is injected (see [`OcrEngineRef`]); the text engine is the
//! configured [`EngineRef`]. Blocks go through one joined translation request
//! (the engine is serialized) with a per-block fallback when the model does
//! not keep one line per input line.

use std::io::Cursor;
use std::sync::Arc;

use translator_ocr::OcrEngine;

use crate::domain::translation::{GlossaryTerm, TranslationError, TranslationRequest};
use crate::engine::EngineRef;

/// OCR provider used by image translation. `None` at the call site means the
/// feature is unavailable and should be reported as such.
pub type OcrEngineRef = Arc<dyn OcrEngine>;

/// Largest accepted image in pixels (a 4K screenshot is ~8.3 MP).
pub const MAX_IMAGE_PIXELS: u64 = 16_000_000;

/// Failure while recognizing or translating an image.
#[derive(Debug)]
pub enum ImageError {
    /// The image was empty, undecodable, too large, or its text exceeded the
    /// request budget.
    InvalidRequest(String),
    /// No OCR provider is configured.
    OcrUnavailable,
    /// The OCR provider failed.
    OcrFailed(String),
    /// The text translation failed.
    Translation(TranslationError),
}

impl std::fmt::Display for ImageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRequest(message) => write!(formatter, "{message}"),
            Self::OcrUnavailable => write!(formatter, "no OCR provider is configured"),
            Self::OcrFailed(message) => write!(formatter, "OCR failed: {message}"),
            Self::Translation(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for ImageError {}

impl From<TranslationError> for ImageError {
    fn from(error: TranslationError) -> Self {
        Self::Translation(error)
    }
}

/// One translated text block. `quad` coordinates are image pixels relative to
/// the submitted image.
#[derive(Debug, Clone, PartialEq)]
pub struct TranslatedBlock {
    pub text: String,
    pub translation: String,
    pub score: f32,
    pub quad: [[f32; 2]; 4],
}

/// A finished image translation.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageTranslation {
    pub width: u32,
    pub height: u32,
    pub blocks: Vec<TranslatedBlock>,
}

/// Decodes `bytes` as PNG/JPEG, recognizes text with `ocr`, translates every
/// block and returns the blocks in recognition order.
pub async fn translate_image_bytes(
    engine: &EngineRef,
    ocr: &OcrEngineRef,
    bytes: &[u8],
    source: &str,
    target: &str,
    glossary: &[GlossaryTerm],
    max_chars: usize,
) -> Result<ImageTranslation, ImageError> {
    let (width, height, pixels) = decode_rgb(bytes)?;

    let ocr = Arc::clone(ocr);
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

    if joined_chars > max_chars {
        return Err(ImageError::InvalidRequest(format!(
            "image text is too long: {joined_chars} chars (max {max_chars})"
        )));
    }

    let translations = translate_blocks(engine, texts, source, target, glossary).await?;

    let blocks = blocks
        .into_iter()
        .zip(translations)
        .map(|(block, translation)| TranslatedBlock {
            text: block.text,
            translation,
            score: block.score,
            quad: block.quad.points,
        })
        .collect();

    Ok(ImageTranslation {
        width,
        height,
        blocks,
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

    // One request for the whole image keeps the interactive latency budget
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
