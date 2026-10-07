use std::path::Path;
use std::sync::Mutex;

use rapidocr_core::config::{InferenceOptions, PipelineConfig};
use rapidocr_core::model::{
    available_model_sets, model_set_by_name, ModelCache, ModelDownloadMode,
};
use rapidocr_core::RapidOcr;

use crate::{OcrBlock, OcrEngine, OcrError, Quad};

/// Model set used when the caller does not pick one (PP-OCR v6 mobile).
pub const DEFAULT_MODEL_SET: &str = "ppocrv6-tiny";

/// Intra-op threads. The M0 spike measured 4 as the sweet spot on the dev
/// machine (1 thread: 515 ms, 4: 240 ms, 8: 274 ms on a 41-line fixture).
pub const DEFAULT_THREADS: usize = 4;

/// RapidOCR (PP-OCR v6) engine on ONNX Runtime.
///
/// Model files live in `model_dir`; [`Self::load`] downloads missing assets
/// from ModelScope, [`Self::load_offline`] fails instead (the desktop client
/// pre-downloads through `translator-core::models`). ONNX Runtime itself is
/// never bundled by the build: `ort` is compiled with `load-dynamic` and the
/// process loads the runtime provided by the host — on Linux the
/// sherpa-shipped `libonnxruntime.so`, on Windows/macOS the dylib shipped
/// next to the binary.
pub struct RapidOcrEngine {
    ocr: Mutex<RapidOcr>,
}

impl RapidOcrEngine {
    /// Loads `model_set`, downloading missing files from ModelScope.
    pub fn load(model_set: &str, model_dir: &Path, threads: usize) -> Result<Self, OcrError> {
        Self::build(model_set, model_dir, threads, ModelDownloadMode::Missing)
    }

    /// Loads `model_set` from `model_dir`; missing files are an error.
    pub fn load_offline(
        model_set: &str,
        model_dir: &Path,
        threads: usize,
    ) -> Result<Self, OcrError> {
        Self::build(model_set, model_dir, threads, ModelDownloadMode::Never)
    }

    fn build(
        model_set: &str,
        model_dir: &Path,
        threads: usize,
        download: ModelDownloadMode,
    ) -> Result<Self, OcrError> {
        let Some(spec) = model_set_by_name(model_set) else {
            let names: Vec<&str> = available_model_sets()
                .iter()
                .map(|set| set.name)
                .collect();

            return Err(OcrError::ModelUnavailable(format!(
                "unknown OCR model set {model_set:?}; available: {}",
                names.join(", ")
            )));
        };

        let cache = ModelCache::new(model_dir);
        let pipeline = PipelineConfig::without_cls();

        cache
            .ensure_model_set_for_pipeline(spec, pipeline, download)
            .map_err(|error| OcrError::ModelUnavailable(error.to_string()))?;

        let config = cache
            .config_for(spec)
            .with_pipeline(pipeline)
            .with_inference_options(InferenceOptions {
                intra_threads: threads.max(1),
                inter_threads: 1,
                ..Default::default()
            });

        let ocr = RapidOcr::from_config(config)
            .map_err(|error| OcrError::ModelUnavailable(error.to_string()))?;

        tracing::info!(
            model_set,
            model_dir = %model_dir.display(),
            "RapidOCR engine loaded"
        );

        Ok(Self {
            ocr: Mutex::new(ocr),
        })
    }
}

impl OcrEngine for RapidOcrEngine {
    fn recognize(&self, pixels: &[u8], width: u32, height: u32) -> Result<Vec<OcrBlock>, OcrError> {
        let image = rgb_image(pixels, width, height)?;

        let mut ocr = self
            .ocr
            .lock()
            .map_err(|_| OcrError::Internal("OCR engine lock poisoned".to_string()))?;

        let output = ocr
            .run_image(&image)
            .map_err(|error| OcrError::Internal(error.to_string()))?;

        Ok(output
            .lines
            .into_iter()
            .map(|line| OcrBlock {
                text: line.text,
                score: line.score,
                quad: Quad {
                    points: line.bbox.points,
                },
            })
            .collect())
    }

    fn name(&self) -> &'static str {
        "rapidocr"
    }
}

fn rgb_image(pixels: &[u8], width: u32, height: u32) -> Result<image::RgbImage, OcrError> {
    if width == 0 || height == 0 {
        return Err(OcrError::InvalidImage("image has zero size".to_string()));
    }

    let expected = width as usize * height as usize * 3;

    if pixels.len() != expected {
        return Err(OcrError::InvalidImage(format!(
            "expected {expected} RGB bytes for {width}x{height}, got {}",
            pixels.len()
        )));
    }

    image::RgbImage::from_raw(width, height, pixels.to_vec())
        .ok_or_else(|| OcrError::Internal("failed to build an RGB image".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_model_set_lists_the_available_ones() {
        let Err(error) = RapidOcrEngine::load_offline("nope", Path::new("/nonexistent"), 1) else {
            panic!("an unknown model set must fail");
        };

        assert!(matches!(error, OcrError::ModelUnavailable(_)));
        assert!(error.to_string().contains("unknown OCR model set"));
    }

    #[test]
    fn missing_local_models_are_reported_without_downloading() {
        let dir = std::env::temp_dir().join("translator-ocr-missing-models-test");
        let Err(error) = RapidOcrEngine::load_offline(DEFAULT_MODEL_SET, &dir, 1) else {
            panic!("missing local models must fail in offline mode");
        };

        assert!(matches!(error, OcrError::ModelUnavailable(_)));
    }

    #[test]
    fn rgb_image_validates_the_pixel_count() {
        assert!(rgb_image(&[0, 0, 0], 1, 1).is_ok());

        assert!(matches!(
            rgb_image(&[0, 0, 0], 2, 2),
            Err(OcrError::InvalidImage(_))
        ));
        assert!(matches!(
            rgb_image(&[], 0, 1),
            Err(OcrError::InvalidImage(_))
        ));
    }

    /// Real OCR run against locally prepared models, enabled with
    /// `TRANSLATOR_TEST_OCR_MODEL_DIR` + `TRANSLATOR_TEST_OCR_IMAGE`. The
    /// process needs a loadable ONNX Runtime (`ORT_DYLIB_PATH` or a default
    /// search path).
    #[test]
    fn recognizes_with_env_models() {
        let Ok(model_dir) = std::env::var("TRANSLATOR_TEST_OCR_MODEL_DIR") else {
            return;
        };
        let Ok(image_path) = std::env::var("TRANSLATOR_TEST_OCR_IMAGE") else {
            return;
        };

        let engine = RapidOcrEngine::load_offline(
            DEFAULT_MODEL_SET,
            Path::new(&model_dir),
            DEFAULT_THREADS,
        )
        .unwrap();
        let image = image::open(&image_path).unwrap().to_rgb8();
        let blocks = engine
            .recognize(image.as_raw(), image.width(), image.height())
            .unwrap();

        assert!(!blocks.is_empty(), "expected at least one text block");

        for block in &blocks {
            println!("{:.3}\t{}\t{:?}", block.score, block.text, block.quad.points);
        }
    }
}
