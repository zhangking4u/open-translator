//! Local OCR for OpenTranslator.
//!
//! Engine-agnostic surface: [`OcrEngine`] turns a raw RGB image into text
//! blocks with quads, the structure the screenshot-translation overlay needs.
//! The crate has no engine implementation yet — the first adapter wraps
//! RapidOCR (PP-OCR v6 mobile) through ONNX Runtime, pending the runtime
//! packaging decision recorded in `docs/IMAGE_TRANSLATION.md` §5.1 (Linux
//! reuses the sherpa-shipped runtime via `ort` `load-dynamic`; Windows/macOS
//! still need a bundled one).

use std::fmt;

/// Failure to load a model or recognize an image.
#[derive(Debug)]
pub enum OcrError {
    /// The model files are missing or could not be loaded.
    ModelUnavailable(String),
    /// The image was not usable (empty, wrong shape, undecodable).
    InvalidImage(String),
    /// Anything else that went wrong inside the engine.
    Internal(String),
}

impl fmt::Display for OcrError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ModelUnavailable(message) => write!(formatter, "ocr model unavailable: {message}"),
            Self::InvalidImage(message) => write!(formatter, "invalid image: {message}"),
            Self::Internal(message) => write!(formatter, "ocr engine error: {message}"),
        }
    }
}

impl std::error::Error for OcrError {}

/// A four-point box in image pixels, clockwise from the top-left.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quad {
    pub points: [[f32; 2]; 4],
}

impl Quad {
    /// Axis-aligned `[x, y, width, height]` covering the quad.
    pub fn bounding_box(&self) -> [f32; 4] {
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;

        for [x, y] in self.points {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }

        [min_x, min_y, max_x - min_x, max_y - min_y]
    }
}

/// One recognized text block.
#[derive(Debug, Clone, PartialEq)]
pub struct OcrBlock {
    pub text: String,
    pub score: f32,
    pub quad: Quad,
}

/// Full-image text recognition. Implementations must be usable from a worker
/// thread (`Send + Sync`), one image at a time.
pub trait OcrEngine: Send + Sync {
    /// Recognize text in an RGB8 row-major image; `pixels.len()` must be
    /// `width * height * 3`.
    fn recognize(&self, pixels: &[u8], width: u32, height: u32)
    -> Result<Vec<OcrBlock>, OcrError>;

    /// Human-readable engine name for logs.
    fn name(&self) -> &'static str {
        "ocr"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FixedEngine;

    impl OcrEngine for FixedEngine {
        fn recognize(
            &self,
            pixels: &[u8],
            width: u32,
            height: u32,
        ) -> Result<Vec<OcrBlock>, OcrError> {
            Ok(vec![OcrBlock {
                text: format!("{width}x{height}:{}", pixels.len()),
                score: 1.0,
                quad: Quad {
                    points: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
                },
            }])
        }
    }

    #[test]
    fn ocr_engine_is_object_safe() {
        let engine: Box<dyn OcrEngine> = Box::new(FixedEngine);

        let blocks = engine.recognize(&[0, 0, 0], 1, 1).unwrap();

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].text, "1x1:3");
    }

    #[test]
    fn quad_bounding_box_covers_all_points() {
        let quad = Quad {
            points: [[10.0, 20.0], [30.0, 18.0], [32.0, 40.0], [8.0, 42.0]],
        };

        assert_eq!(quad.bounding_box(), [8.0, 18.0, 24.0, 24.0]);
    }

    #[test]
    fn errors_describe_the_stage() {
        let error = OcrError::ModelUnavailable("missing model.onnx".to_string());

        assert!(error.to_string().contains("model unavailable"));
        assert!(error.to_string().contains("missing model.onnx"));
    }
}
