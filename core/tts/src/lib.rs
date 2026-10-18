//! Local text-to-speech for the desktop client.
//!
//! The first engine wraps sherpa-onnx's Kokoro model (Chinese + English),
//! which is downloaded by the client on demand. Keeping the model handling in
//! this crate means the Tauri layer only deals with paths, cancellation and
//! WAV bytes.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use sherpa_onnx::{
    GenerationConfig, OfflineTts, OfflineTtsConfig, OfflineTtsKokoroModelConfig,
    OfflineTtsModelConfig, OfflineTtsSupertonicModelConfig, OfflineTtsVitsModelConfig,
};

/// Curated Kokoro v1.1 speakers exposed by the settings UI.
///
/// The model has 103 voices (55 Chinese female, 45 Chinese male, 3 English).
/// The list is intentionally short; the config accepts any id in `0..=102`.
pub const KOKORO_SPEAKERS: &[(i32, &str, &str)] = &[
    (0, "af_maple", "en"),
    (1, "af_sol", "en"),
    (2, "bf_vale", "en"),
    (3, "zf_001", "zh"),
    (4, "zf_002", "zh"),
    (5, "zf_003", "zh"),
    (6, "zf_004", "zh"),
    (7, "zf_005", "zh"),
    (8, "zf_006", "zh"),
    (47, "zf_083", "zh"),
    (57, "zf_099", "zh"),
    (58, "zm_009", "zh"),
    (59, "zm_010", "zh"),
    (60, "zm_011", "zh"),
    (102, "zm_100", "zh"),
];

#[derive(Debug)]
pub enum TtsError {
    ModelUnavailable(String),
    InvalidRequest(String),
    Cancelled,
    Internal(String),
}

impl std::fmt::Display for TtsError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ModelUnavailable(message) => write!(formatter, "语音模型不可用：{message}"),
            Self::InvalidRequest(message) => write!(formatter, "朗读请求无效：{message}"),
            Self::Cancelled => write!(formatter, "朗读已取消"),
            Self::Internal(message) => write!(formatter, "语音合成失败：{message}"),
        }
    }
}

impl std::error::Error for TtsError {}

#[derive(Debug, Clone)]
pub struct Audio {
    pub samples: Vec<f32>,
    pub sample_rate: i32,
}

impl Audio {
    /// Encodes the samples as a mono 16-bit PCM WAV file.
    pub fn to_wav(&self) -> Vec<u8> {
        let data_len = self.samples.len().saturating_mul(2) as u32;
        let mut output = Vec::with_capacity(44 + data_len as usize);

        output.extend_from_slice(b"RIFF");
        output.extend_from_slice(&(36 + data_len).to_le_bytes());
        output.extend_from_slice(b"WAVEfmt ");
        output.extend_from_slice(&16u32.to_le_bytes());
        output.extend_from_slice(&1u16.to_le_bytes());
        output.extend_from_slice(&1u16.to_le_bytes());
        output.extend_from_slice(&(self.sample_rate.max(1) as u32).to_le_bytes());
        output.extend_from_slice(&(self.sample_rate.max(1) as u32 * 2).to_le_bytes());
        output.extend_from_slice(&2u16.to_le_bytes());
        output.extend_from_slice(&16u16.to_le_bytes());
        output.extend_from_slice(b"data");
        output.extend_from_slice(&data_len.to_le_bytes());

        for sample in &self.samples {
            let value = (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16;
            output.extend_from_slice(&value.to_le_bytes());
        }

        output
    }

    pub fn duration_seconds(&self) -> f32 {
        if self.sample_rate <= 0 {
            return 0.0;
        }

        self.samples.len() as f32 / self.sample_rate as f32
    }
}

/// A loaded Kokoro TTS model.
pub struct KokoroEngine {
    tts: OfflineTts,
    model_dir: PathBuf,
}

impl KokoroEngine {
    pub fn load(model_dir: &Path) -> Result<Self, TtsError> {
        Self::load_with_threads(model_dir, default_threads())
    }

    pub fn load_with_threads(model_dir: &Path, num_threads: i32) -> Result<Self, TtsError> {
        let root = find_model_root(model_dir).ok_or_else(|| {
            TtsError::ModelUnavailable(format!(
                "未找到 Kokoro 模型文件（{}）",
                model_dir.display()
            ))
        })?;

        let model = find_model_file(&root).ok_or_else(|| {
            TtsError::ModelUnavailable(format!("缺少模型文件（{}）", root.display()))
        })?;
        let voices = root.join("voices.bin");
        let tokens = root.join("tokens.txt");
        let data_dir = root.join("espeak-ng-data");

        if !voices.is_file() || !tokens.is_file() || !data_dir.is_dir() {
            return Err(TtsError::ModelUnavailable(format!(
                "Kokoro 模型目录不完整（{}）",
                root.display()
            )));
        }

        let lexicons = existing_files(&root, &["lexicon-us-en.txt", "lexicon-zh.txt"]);
        let rule_fsts = existing_files(
            &root,
            &[
                "phone.fst",
                "date.fst",
                "number.fst",
                "phone-zh.fst",
                "date-zh.fst",
                "number-zh.fst",
            ],
        );
        let dict_dir = root.join("dict");

        let config = OfflineTtsConfig {
            model: OfflineTtsModelConfig {
                kokoro: OfflineTtsKokoroModelConfig {
                    model: Some(path_string(&model)?),
                    voices: Some(path_string(&voices)?),
                    tokens: Some(path_string(&tokens)?),
                    data_dir: Some(path_string(&data_dir)?),
                    lexicon: lexicons.map(|paths| paths.join(",")),
                    dict_dir: dict_dir.is_dir().then(|| path_string(&dict_dir)).transpose()?,
                    ..Default::default()
                },
                num_threads: num_threads.max(1),
                debug: false,
                ..Default::default()
            },
            rule_fsts: rule_fsts.map(|paths| paths.join(",")),
            max_num_sentences: 1,
            silence_scale: 0.2,
            ..Default::default()
        };

        let tts = OfflineTts::create(&config).ok_or_else(|| {
            TtsError::Internal(format!("无法加载 Kokoro 模型（{}）", root.display()))
        })?;

        Ok(Self {
            tts,
            model_dir: root,
        })
    }

    pub fn model_dir(&self) -> &Path {
        &self.model_dir
    }

    pub fn sample_rate(&self) -> i32 {
        self.tts.sample_rate()
    }

    pub fn speaker_count(&self) -> i32 {
        self.tts.num_speakers()
    }

    /// Synthesizes `text`. The callback stops generation when `cancel` is set.
    pub fn synthesize(
        &self,
        text: &str,
        speaker: i32,
        speed: f32,
        cancel: Arc<AtomicBool>,
    ) -> Result<Audio, TtsError> {
        let text = text.trim();

        if text.is_empty() {
            return Err(TtsError::InvalidRequest("文本为空".to_string()));
        }

        let speaker = speaker.clamp(0, self.speaker_count().saturating_sub(1));
        let config = GenerationConfig {
            sid: speaker,
            speed: speed.clamp(0.5, 2.0),
            silence_scale: 0.2,
            ..Default::default()
        };
        let callback_cancel = cancel.clone();

        let audio = self
            .tts
            .generate_with_config(
                text,
                &config,
                Some(move |_samples: &[f32], _progress: f32| {
                    !callback_cancel.load(Ordering::Relaxed)
                }),
            )
            .ok_or_else(|| TtsError::Internal("模型没有返回音频".to_string()))?;

        if cancel.load(Ordering::Relaxed) {
            return Err(TtsError::Cancelled);
        }

        let samples = audio.samples();

        if samples.is_empty() {
            return Err(TtsError::Internal("生成结果为空".to_string()));
        }

        Ok(Audio {
            samples: samples.to_vec(),
            sample_rate: audio.sample_rate(),
        })
    }
}

/// A loaded VITS model (MeloTTS zh_en or Piper).
///
/// VITS is much faster than Kokoro on small CPUs; it is kept as a separate
/// engine so the UI can offer a speed/quality choice.
pub struct VitsEngine {
    tts: OfflineTts,
    model_dir: PathBuf,
}

impl VitsEngine {
    pub fn load(model_dir: &Path) -> Result<Self, TtsError> {
        Self::load_with_threads(model_dir, default_threads())
    }

    pub fn load_with_threads(model_dir: &Path, num_threads: i32) -> Result<Self, TtsError> {
        let root = find_vits_root(model_dir).ok_or_else(|| {
            TtsError::ModelUnavailable(format!(
                "未找到 VITS 模型文件（{}）",
                model_dir.display()
            ))
        })?;
        let model = find_vits_model_file(&root).ok_or_else(|| {
            TtsError::ModelUnavailable(format!("缺少 VITS 模型文件（{}）", root.display()))
        })?;
        let tokens = root.join("tokens.txt");
        let lexicon = root.join("lexicon.txt");
        let data_dir = root.join("espeak-ng-data");
        let dict_dir = root.join("dict");
        let rule_fsts = existing_files(
            &root,
            &["phone.fst", "date.fst", "number.fst", "new_heteronym.fst"],
        );

        let config = OfflineTtsConfig {
            model: OfflineTtsModelConfig {
                vits: OfflineTtsVitsModelConfig {
                    model: Some(path_string(&model)?),
                    lexicon: lexicon.is_file().then(|| path_string(&lexicon)).transpose()?,
                    tokens: Some(path_string(&tokens)?),
                    data_dir: data_dir.is_dir().then(|| path_string(&data_dir)).transpose()?,
                    noise_scale: 0.667,
                    noise_scale_w: 0.8,
                    length_scale: 1.0,
                    dict_dir: dict_dir.is_dir().then(|| path_string(&dict_dir)).transpose()?,
                },
                num_threads: num_threads.max(1),
                debug: false,
                ..Default::default()
            },
            rule_fsts: rule_fsts.map(|paths| paths.join(",")),
            max_num_sentences: 1,
            silence_scale: 0.2,
            ..Default::default()
        };

        let tts = OfflineTts::create(&config).ok_or_else(|| {
            TtsError::Internal(format!("无法加载 VITS 模型（{}）", root.display()))
        })?;

        Ok(Self {
            tts,
            model_dir: root,
        })
    }

    pub fn model_dir(&self) -> &Path {
        &self.model_dir
    }

    pub fn sample_rate(&self) -> i32 {
        self.tts.sample_rate()
    }

    pub fn speaker_count(&self) -> i32 {
        self.tts.num_speakers()
    }

    pub fn synthesize(
        &self,
        text: &str,
        speaker: i32,
        speed: f32,
        cancel: Arc<AtomicBool>,
    ) -> Result<Audio, TtsError> {
        let text = text.trim();

        if text.is_empty() {
            return Err(TtsError::InvalidRequest("文本为空".to_string()));
        }

        let speaker = speaker.clamp(0, self.speaker_count().saturating_sub(1));
        let config = GenerationConfig {
            sid: speaker,
            speed: speed.clamp(0.5, 2.0),
            silence_scale: 0.2,
            ..Default::default()
        };
        let callback_cancel = cancel.clone();

        let audio = self
            .tts
            .generate_with_config(
                text,
                &config,
                Some(move |_samples: &[f32], _progress: f32| {
                    !callback_cancel.load(Ordering::Relaxed)
                }),
            )
            .ok_or_else(|| TtsError::Internal("模型没有返回音频".to_string()))?;

        if cancel.load(Ordering::Relaxed) {
            return Err(TtsError::Cancelled);
        }

        let samples = audio.samples();

        if samples.is_empty() {
            return Err(TtsError::Internal("生成结果为空".to_string()));
        }

        Ok(Audio {
            samples: samples.to_vec(),
            sample_rate: audio.sample_rate(),
        })
    }
}

/// Finds the directory that contains a VITS model.
pub fn find_vits_root(dir: &Path) -> Option<PathBuf> {
    let preferred = dir.join("vits-melo-tts-zh_en");

    if looks_like_vits_root(&preferred) {
        return Some(preferred);
    }

    if looks_like_vits_root(dir) {
        return Some(dir.to_path_buf());
    }

    let entries = std::fs::read_dir(dir).ok()?;

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_dir() && looks_like_vits_root(&path) {
            return Some(path);
        }
    }

    None
}

fn looks_like_vits_root(dir: &Path) -> bool {
    find_vits_model_file(dir).is_some()
        && dir.join("tokens.txt").is_file()
        && (dir.join("lexicon.txt").is_file() || dir.join("espeak-ng-data").is_dir())
}

fn find_vits_model_file(root: &Path) -> Option<PathBuf> {
    for name in ["zh_CN-huayan-medium.onnx", "model.onnx"] {
        let path = root.join(name);

        if path.is_file() {
            return Some(path);
        }
    }

    let entries = std::fs::read_dir(root).ok()?;

    entries
        .flatten()
        .map(|entry| entry.path())
        .find(|path| {
            let name = path.file_name().and_then(|value| value.to_str());
            name.is_some_and(|value| {
                value.ends_with(".onnx")
                    && (value.contains("vits") || value.contains("huayan"))
            })
        })
}

/// A loaded Supertonic 3 model (31 languages, no Chinese).
///
/// Supertonic is the default engine for non-Chinese languages: one compact
/// model covers English, Japanese, Korean, French, German, Spanish, Russian
/// and the other supported languages.
pub struct SupertonicEngine {
    tts: OfflineTts,
    model_dir: PathBuf,
}

impl SupertonicEngine {
    pub fn load(model_dir: &Path) -> Result<Self, TtsError> {
        Self::load_with_threads(model_dir, default_threads())
    }

    pub fn load_with_threads(model_dir: &Path, num_threads: i32) -> Result<Self, TtsError> {
        let root = find_supertonic_root(model_dir).ok_or_else(|| {
            TtsError::ModelUnavailable(format!(
                "未找到 Supertonic 模型文件（{}）",
                model_dir.display()
            ))
        })?;

        let config = OfflineTtsConfig {
            model: OfflineTtsModelConfig {
                supertonic: OfflineTtsSupertonicModelConfig {
                    duration_predictor: Some(path_string(&root.join("duration_predictor.int8.onnx"))?),
                    text_encoder: Some(path_string(&root.join("text_encoder.int8.onnx"))?),
                    vector_estimator: Some(path_string(&root.join("vector_estimator.int8.onnx"))?),
                    vocoder: Some(path_string(&root.join("vocoder.int8.onnx"))?),
                    tts_json: Some(path_string(&root.join("tts.json"))?),
                    unicode_indexer: Some(path_string(&root.join("unicode_indexer.bin"))?),
                    voice_style: Some(path_string(&root.join("voice.bin"))?),
                },
                num_threads: num_threads.max(1),
                debug: false,
                ..Default::default()
            },
            max_num_sentences: 1,
            silence_scale: 0.2,
            ..Default::default()
        };

        let tts = OfflineTts::create(&config).ok_or_else(|| {
            TtsError::Internal(format!("无法加载 Supertonic 模型（{}）", root.display()))
        })?;

        Ok(Self {
            tts,
            model_dir: root,
        })
    }

    pub fn model_dir(&self) -> &Path {
        &self.model_dir
    }

    pub fn sample_rate(&self) -> i32 {
        self.tts.sample_rate()
    }

    pub fn speaker_count(&self) -> i32 {
        self.tts.num_speakers()
    }

    /// Synthesizes `text` in `language` (an ISO 639-1 code such as `en`/`ja`).
    pub fn synthesize(
        &self,
        text: &str,
        language: &str,
        speaker: i32,
        speed: f32,
        cancel: Arc<AtomicBool>,
    ) -> Result<Audio, TtsError> {
        let text = text.trim();

        if text.is_empty() {
            return Err(TtsError::InvalidRequest("文本为空".to_string()));
        }

        let language = language
            .to_ascii_lowercase()
            .split(['-', '_'])
            .next()
            .unwrap_or("en")
            .to_string();
        let mut extra = HashMap::new();
        extra.insert("lang".to_string(), serde_json::json!(language));

        let speaker = speaker.clamp(0, self.speaker_count().saturating_sub(1));
        let config = GenerationConfig {
            sid: speaker,
            speed: speed.clamp(0.5, 2.0),
            num_steps: 8,
            silence_scale: 0.2,
            extra: Some(extra),
            ..Default::default()
        };
        let callback_cancel = cancel.clone();

        let audio = self
            .tts
            .generate_with_config(
                text,
                &config,
                Some(move |_samples: &[f32], _progress: f32| {
                    !callback_cancel.load(Ordering::Relaxed)
                }),
            )
            .ok_or_else(|| TtsError::Internal("模型没有返回音频".to_string()))?;

        if cancel.load(Ordering::Relaxed) {
            return Err(TtsError::Cancelled);
        }

        let samples = audio.samples();

        if samples.is_empty() {
            return Err(TtsError::Internal("生成结果为空".to_string()));
        }

        Ok(Audio {
            samples: samples.to_vec(),
            sample_rate: audio.sample_rate(),
        })
    }
}

/// Finds the directory that contains a Supertonic model.
pub fn find_supertonic_root(dir: &Path) -> Option<PathBuf> {
    if looks_like_supertonic_root(dir) {
        return Some(dir.to_path_buf());
    }

    let entries = std::fs::read_dir(dir).ok()?;

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_dir() && looks_like_supertonic_root(&path) {
            return Some(path);
        }
    }

    None
}

fn looks_like_supertonic_root(dir: &Path) -> bool {
    dir.join("duration_predictor.int8.onnx").is_file()
        && dir.join("text_encoder.int8.onnx").is_file()
        && dir.join("vector_estimator.int8.onnx").is_file()
        && dir.join("vocoder.int8.onnx").is_file()
        && dir.join("tts.json").is_file()
        && dir.join("unicode_indexer.bin").is_file()
        && dir.join("voice.bin").is_file()
}

/// Finds the directory that contains a Kokoro model. The downloaded archive
/// has a single top-level directory, so both the model directory itself and
/// its immediate children are checked.
pub fn find_model_root(dir: &Path) -> Option<PathBuf> {
    if looks_like_model_root(dir) {
        return Some(dir.to_path_buf());
    }

    let preferred = dir.join("kokoro-int8-multi-lang-v1_1");

    if looks_like_model_root(&preferred) {
        return Some(preferred);
    }

    let entries = std::fs::read_dir(dir).ok()?;

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_dir() && looks_like_model_root(&path) {
            return Some(path);
        }
    }

    None
}

pub fn speaker_name(id: i32) -> Option<&'static str> {
    KOKORO_SPEAKERS
        .iter()
        .find(|(speaker, _, _)| *speaker == id)
        .map(|(_, name, _)| *name)
}

/// Picks a sensible default voice for a language tag.
pub fn default_speaker(language: &str) -> i32 {
    let language = language.to_ascii_lowercase();

    if language.starts_with("zh") || language.starts_with("cmn") || language.starts_with("yue") {
        3
    } else {
        0
    }
}

fn looks_like_model_root(dir: &Path) -> bool {
    find_model_file(dir).is_some()
        && dir.join("voices.bin").is_file()
        && dir.join("tokens.txt").is_file()
        && dir.join("espeak-ng-data").is_dir()
        && dir.join("lexicon-zh.txt").is_file()
        && dir.join("lexicon-us-en.txt").is_file()
}

fn find_model_file(root: &Path) -> Option<PathBuf> {
    for name in ["model.int8.onnx", "model.onnx"] {
        let path = root.join(name);

        if path.is_file() {
            return Some(path);
        }
    }

    let entries = std::fs::read_dir(root).ok()?;

    entries
        .flatten()
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|extension| extension == "onnx"))
}

fn existing_files(root: &Path, names: &[&str]) -> Option<Vec<String>> {
    let mut paths = Vec::new();

    for name in names {
        let path = root.join(name);

        if path.is_file() {
            paths.push(path_string(&path).ok()?);
        }
    }

    (!paths.is_empty()).then_some(paths)
}

fn path_string(path: &Path) -> Result<String, TtsError> {
    path.to_str()
        .map(str::to_string)
        .ok_or_else(|| TtsError::ModelUnavailable(format!("模型路径不是 UTF-8：{}", path.display())))
}

fn default_threads() -> i32 {
    std::thread::available_parallelism()
        .map(|count| count.get().min(4) as i32)
        .unwrap_or(2)
        .max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "translator-tts-test-{}-{name}",
            std::process::id()
        ))
    }

    fn fake_model_dir(root: &Path) {
        std::fs::create_dir_all(root.join("espeak-ng-data")).unwrap();
        std::fs::write(root.join("model.int8.onnx"), b"model").unwrap();
        std::fs::write(root.join("voices.bin"), b"voices").unwrap();
        std::fs::write(root.join("tokens.txt"), b"tokens").unwrap();
        std::fs::write(root.join("lexicon-zh.txt"), b"lexicon").unwrap();
        std::fs::write(root.join("lexicon-us-en.txt"), b"lexicon").unwrap();
    }

    #[test]
    fn finds_a_nested_model_root() {
        let dir = temp_dir("nested");
        let _ = std::fs::remove_dir_all(&dir);
        let root = dir.join("kokoro-int8-multi-lang-v1_1");
        fake_model_dir(&root);

        assert_eq!(find_model_root(&dir), Some(root));
        assert_eq!(find_model_root(&dir.join("missing")), None);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn default_speaker_follows_the_language() {
        assert_eq!(default_speaker("zh"), 3);
        assert_eq!(default_speaker("zh-CN"), 3);
        assert_eq!(default_speaker("yue"), 3);
        assert_eq!(default_speaker("en"), 0);
    }

    #[test]
    fn wav_encoding_has_a_pcm_header() {
        let audio = Audio {
            samples: vec![0.0, 0.5, -0.5],
            sample_rate: 24_000,
        };
        let wav = audio.to_wav();

        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(wav.len(), 44 + 6);
        assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 24_000);
        assert!((audio.duration_seconds() - 0.000125).abs() < f32::EPSILON);
    }

    #[test]
    fn benchmark_thread_counts() {
        let Some(dir) = std::env::var_os("TRANSLATOR_TEST_TTS_MODEL") else {
            return;
        };

        if std::env::var_os("TRANSLATOR_TTS_BENCH").is_none() {
            return;
        }

        for threads in [1, 2, 4, 8] {
            let load_started = std::time::Instant::now();
            let engine = KokoroEngine::load_with_threads(Path::new(&dir), threads).unwrap();
            let load_ms = load_started.elapsed().as_millis();
            let synthesis_started = std::time::Instant::now();
            let audio = engine
                .synthesize(
                    "这是一段用于测试连续朗读速度的中文句子。",
                    3,
                    1.0,
                    Arc::new(AtomicBool::new(false)),
                )
                .unwrap();
            eprintln!(
                "Kokoro threads={threads}: load {load_ms} ms, synthesis {} ms for {:.2}s audio",
                synthesis_started.elapsed().as_millis(),
                audio.duration_seconds()
            );
        }
    }

    #[test]
    fn loads_a_configured_model() {
        let Some(dir) = std::env::var_os("TRANSLATOR_TEST_TTS_MODEL") else {
            return;
        };

        let load_started = std::time::Instant::now();
        let engine = KokoroEngine::load(Path::new(&dir)).unwrap();
        eprintln!(
            "Kokoro load: {} ms",
            load_started.elapsed().as_millis()
        );
        assert_eq!(engine.sample_rate(), 24_000);
        assert!(engine.speaker_count() > 0);

        let synthesis_started = std::time::Instant::now();
        let audio = engine
            .synthesize(
                "你好，世界。",
                3,
                1.0,
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap();
        eprintln!(
            "Kokoro synthesis: {} ms for {:.2}s of audio",
            synthesis_started.elapsed().as_millis(),
            audio.duration_seconds()
        );
        assert_eq!(audio.sample_rate, 24_000);
        assert!(audio.samples.len() > 1_000);
        assert!(audio.to_wav().len() > 44);

        let second_started = std::time::Instant::now();
        let second = engine
            .synthesize(
                "这是一段用于测试连续朗读速度的中文句子。",
                3,
                1.0,
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap();
        eprintln!(
            "Kokoro second synthesis: {} ms for {:.2}s of audio",
            second_started.elapsed().as_millis(),
            second.duration_seconds()
        );
        assert!(second.samples.len() > 1_000);
    }

    #[test]
    fn loads_a_configured_vits() {
        let Some(dir) = std::env::var_os("TRANSLATOR_TEST_VITS_MODEL") else {
            return;
        };

        let engine = VitsEngine::load(Path::new(&dir)).unwrap();
        assert!(engine.speaker_count() > 0);
        assert_eq!(engine.sample_rate(), 44_100);

        let audio = engine
            .synthesize(
                "你好，世界。",
                0,
                1.0,
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap();
        assert_eq!(audio.sample_rate, 44_100);
        assert!(audio.samples.len() > 1_000);
        assert!(audio.to_wav().len() > 44);
    }

    #[test]
    fn loads_a_configured_supertonic() {
        let Some(dir) = std::env::var_os("TRANSLATOR_TEST_SUPERTONIC_MODEL") else {
            return;
        };

        let engine = SupertonicEngine::load(Path::new(&dir)).unwrap();
        eprintln!(
            "Supertonic: {} speakers, {} Hz",
            engine.speaker_count(),
            engine.sample_rate()
        );

        let started = std::time::Instant::now();
        let audio = engine
            .synthesize(
                "Today as always, men fall into two groups: slaves and free men.",
                "en",
                6,
                1.0,
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap();
        eprintln!(
            "Supertonic English: {} ms for {:.2}s audio",
            started.elapsed().as_millis(),
            audio.duration_seconds()
        );
        assert!(audio.samples.len() > 1_000);

        let japanese = engine
            .synthesize(
                "こんにちは。今日はいい天気ですね。",
                "ja",
                6,
                1.0,
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap();
        assert!(japanese.samples.len() > 1_000);
    }

    #[test]
    fn benchmark_vits() {
        let Some(dir) = std::env::var_os("TRANSLATOR_TEST_VITS_MODEL") else {
            return;
        };

        if std::env::var_os("TRANSLATOR_TTS_BENCH").is_none() {
            return;
        }

        for threads in [1, 2, 4] {
            let load_started = std::time::Instant::now();
            let engine = VitsEngine::load_with_threads(Path::new(&dir), threads).unwrap();
            let load_ms = load_started.elapsed().as_millis();
            let synthesis_started = std::time::Instant::now();
            let audio = engine
                .synthesize(
                    "这是一段用于测试连续朗读速度的中文句子。",
                    0,
                    1.0,
                    Arc::new(AtomicBool::new(false)),
                )
                .unwrap();
            eprintln!(
                "VITS threads={threads}: load {load_ms} ms, synthesis {} ms for {:.2}s audio",
                synthesis_started.elapsed().as_millis(),
                audio.duration_seconds()
            );
        }
    }
}
