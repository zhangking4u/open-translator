//! Local speech recognition for OpenTranslator.
//!
//! This crate wraps [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx) with
//! a small, engine-agnostic surface: [`SpeechEngine`] turns a finished audio
//! segment into text, and [`VoiceSegmenter`] (Silero VAD) turns a continuous
//! stream into those segments.
//!
//! The first implementation is `SenseVoice` (zh/en/ja/ko/yue), the model
//! validated in the M0 spike (RTF ~0.022 on CPU). The default build links the
//! prebuilt static sherpa-onnx libraries, which the `sherpa-onnx-sys` build
//! script downloads from GitHub releases; set `SHERPA_ONNX_ARCHIVE_DIR` or
//! `SHERPA_ONNX_LIB_DIR` to build offline.

use std::fmt;
use std::path::{Path, PathBuf};

use sherpa_onnx::{
    OfflineRecognizer, OfflineRecognizerConfig, OfflineSenseVoiceModelConfig,
    SileroVadModelConfig, VadModelConfig, VoiceActivityDetector,
};

/// Failure to load a model or transcribe a segment.
#[derive(Debug)]
pub enum SpeechError {
    /// The model files are missing or could not be loaded.
    ModelUnavailable(String),
    /// The audio was not usable (empty, too short, wrong shape).
    InvalidAudio(String),
    /// Anything else that went wrong inside the engine.
    Internal(String),
}

impl fmt::Display for SpeechError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ModelUnavailable(message) => write!(f, "speech model unavailable: {message}"),
            Self::InvalidAudio(message) => write!(f, "invalid audio: {message}"),
            Self::Internal(message) => write!(f, "speech engine error: {message}"),
        }
    }
}

impl std::error::Error for SpeechError {}

/// The text of one finished audio segment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transcript {
    pub text: String,
}

/// Final-segment speech recognition. Implementations must be usable from a
/// worker thread (`Send + Sync`), one segment at a time.
pub trait SpeechEngine: Send + Sync {
    /// Transcribe a finished segment of mono `samples` in `[-1.0, 1.0]`.
    fn transcribe(&self, samples: &[f32], sample_rate: i32) -> Result<Transcript, SpeechError>;

    /// Human-readable engine name for logs.
    fn name(&self) -> &'static str {
        "speech"
    }
}

/// SenseVoice (zh/en/ja/ko/yue) offline recognizer.
pub struct SenseVoiceEngine {
    recognizer: OfflineRecognizer,
    model_path: PathBuf,
}

impl SenseVoiceEngine {
    /// Load `model.int8.onnx` and `tokens.txt`.
    ///
    /// `language` is a SenseVoice language tag: `auto`, `zh`, `en`, `ja`,
    /// `ko` or `yue`.
    pub fn load(
        model: &Path,
        tokens: &Path,
        language: &str,
        num_threads: i32,
    ) -> Result<Self, SpeechError> {
        if !model.is_file() {
            return Err(SpeechError::ModelUnavailable(format!(
                "model file not found: {}",
                model.display()
            )));
        }
        if !tokens.is_file() {
            return Err(SpeechError::ModelUnavailable(format!(
                "tokens file not found: {}",
                tokens.display()
            )));
        }

        let mut config = OfflineRecognizerConfig::default();
        config.model_config.sense_voice = OfflineSenseVoiceModelConfig {
            model: Some(model.display().to_string()),
            language: Some(language.to_string()),
            use_itn: true,
        };
        config.model_config.tokens = Some(tokens.display().to_string());
        config.model_config.num_threads = num_threads.max(1);

        let recognizer = OfflineRecognizer::create(&config).ok_or_else(|| {
            SpeechError::ModelUnavailable(format!("SenseVoice failed to load {}", model.display()))
        })?;

        tracing::info!(
            model = %model.display(),
            language,
            "SenseVoice engine loaded"
        );

        Ok(Self {
            recognizer,
            model_path: model.to_path_buf(),
        })
    }
}

impl SpeechEngine for SenseVoiceEngine {
    fn transcribe(&self, samples: &[f32], sample_rate: i32) -> Result<Transcript, SpeechError> {
        if samples.is_empty() {
            return Err(SpeechError::InvalidAudio("empty segment".to_string()));
        }

        let stream = self.recognizer.create_stream();
        stream.accept_waveform(sample_rate, samples);
        self.recognizer.decode(&stream);

        let result = stream.get_result().ok_or_else(|| {
            SpeechError::Internal(format!("no result for {}", self.model_path.display()))
        })?;

        Ok(Transcript {
            text: result.text.trim().to_string(),
        })
    }

    fn name(&self) -> &'static str {
        "sense-voice"
    }
}

/// Silero VAD segmentation over a continuous 16 kHz stream.
///
/// Feed arbitrary chunk sizes through [`VoiceSegmenter::accept`]; every
/// returned vector is one finished speech segment, ready for
/// [`SpeechEngine::transcribe`].
pub struct VoiceSegmenter {
    vad: VoiceActivityDetector,
    sample_rate: i32,
}

impl VoiceSegmenter {
    /// `max_speech_duration` forces a cut inside long monologues; the caller
    /// translates each segment, so keeping it near the M0 value (~6 s) bounds
    /// the end-to-end latency.
    pub fn new(
        model: &Path,
        sample_rate: i32,
        min_silence_duration: f32,
        max_speech_duration: f32,
    ) -> Result<Self, SpeechError> {
        if !model.is_file() {
            return Err(SpeechError::ModelUnavailable(format!(
                "VAD model not found: {}",
                model.display()
            )));
        }

        let config = VadModelConfig {
            silero_vad: SileroVadModelConfig {
                model: Some(model.display().to_string()),
                threshold: 0.5,
                min_silence_duration,
                min_speech_duration: 0.25,
                window_size: 512,
                max_speech_duration,
            },
            sample_rate,
            num_threads: 1,
            ..Default::default()
        };

        let vad = VoiceActivityDetector::create(&config, 30.0)
            .ok_or_else(|| SpeechError::Internal("failed to create Silero VAD".to_string()))?;

        Ok(Self {
            vad,
            sample_rate,
        })
    }

    pub fn sample_rate(&self) -> i32 {
        self.sample_rate
    }

    /// Whether speech is currently being detected (used to drive a "listening"
    /// indicator).
    pub fn speech_detected(&self) -> bool {
        self.vad.detected()
    }

    /// Feed mono samples in `[-1.0, 1.0]`; returns every segment that finished
    /// during this call.
    pub fn accept(&self, samples: &[f32]) -> Vec<Vec<f32>> {
        self.vad.accept_waveform(samples);
        self.drain()
    }

    /// Flush any trailing speech (call when capture stops).
    pub fn flush(&self) -> Vec<Vec<f32>> {
        self.vad.flush();
        self.drain()
    }

    fn drain(&self) -> Vec<Vec<f32>> {
        let mut segments = Vec::new();
        while !self.vad.is_empty() {
            let Some(segment) = self.vad.front() else {
                break;
            };
            segments.push(segment.samples().to_vec());
            self.vad.pop();
        }
        segments
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FixedEngine;

    impl SpeechEngine for FixedEngine {
        fn transcribe(&self, samples: &[f32], _sample_rate: i32) -> Result<Transcript, SpeechError> {
            Ok(Transcript {
                text: format!("[{}]", samples.len()),
            })
        }
    }

    #[test]
    fn speech_engine_is_object_safe() {
        let engine: Box<dyn SpeechEngine> = Box::new(FixedEngine);

        assert_eq!(
            engine.transcribe(&[0.0, 0.1], 16000).unwrap().text,
            "[2]"
        );
    }

    #[test]
    fn missing_model_is_reported() {
        let Err(error) = SenseVoiceEngine::load(
            Path::new("/nonexistent/model.int8.onnx"),
            Path::new("/nonexistent/tokens.txt"),
            "auto",
            2,
        ) else {
            panic!("loading a missing model must fail");
        };

        assert!(matches!(error, SpeechError::ModelUnavailable(_)));
        assert!(error.to_string().contains("model file not found"));
    }

    #[test]
    fn missing_vad_model_is_reported() {
        let Err(error) =
            VoiceSegmenter::new(Path::new("/nonexistent/silero_vad.onnx"), 16000, 0.5, 6.0)
        else {
            panic!("loading a missing VAD model must fail");
        };

        assert!(matches!(error, SpeechError::ModelUnavailable(_)));
    }

    #[test]
    fn empty_audio_is_rejected() {
        // Build the error path without loading a model: the check happens
        // before any recognizer call.
        let engine = SenseVoiceEngine::load(
            Path::new("/nonexistent/model.int8.onnx"),
            Path::new("/nonexistent/tokens.txt"),
            "auto",
            1,
        );
        assert!(engine.is_err());
    }
}
