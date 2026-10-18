//! Local TTS: two engines behind one command surface.
//!
//! - Kokoro v1.1-zh: Chinese + English, natural, ~1.5x real-time on a small
//!   laptop CPU.
//! - AISHELL-3 VITS: Chinese-only, ~0.4x real-time, used as the "流畅" mode.
//!
//! The frontend plays the returned WAV through an `Audio` element, so no
//! platform-specific audio backend is needed.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use base64::Engine as _;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::local_tts_supported;

const MODEL_NAME: &str = "Kokoro / AISHELL3";
const KOKORO_DIR_NAME: &str = "kokoro-int8-multi-lang-v1_1";

#[derive(Default)]
pub struct LocalTtsState {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    kokoro: Mutex<Option<Arc<translator_tts::KokoroEngine>>>,
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    vits: Mutex<Option<Arc<translator_tts::VitsEngine>>>,
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    supertonic: Mutex<Option<Arc<translator_tts::SupertonicEngine>>>,
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    load: tokio::sync::Mutex<()>,
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    synth: tokio::sync::Mutex<()>,
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    download: tokio::sync::Mutex<()>,
    downloading: AtomicBool,
    cancel: Arc<AtomicBool>,
}

#[derive(Clone, Serialize)]
pub struct TtsModelStatus {
    pub installed: bool,
    pub downloading: bool,
    pub supported: bool,
    pub engine: &'static str,
    pub dir: String,
    pub kokoro_installed: bool,
    pub vits_installed: bool,
    pub supertonic_installed: bool,
    pub kokoro_size: u64,
    pub vits_size: u64,
    pub supertonic_size: u64,
}

#[derive(Clone, Serialize)]
pub struct TtsModelProgress {
    pub model: String,
    pub downloaded: u64,
    pub total: Option<u64>,
}

#[derive(Clone, Serialize)]
pub struct TtsSpeaker {
    pub id: i32,
    pub name: String,
    pub language: String,
}

#[derive(Clone, Serialize)]
pub struct TtsAudio {
    pub wav_base64: String,
    pub sample_rate: i32,
    pub duration_ms: u64,
    pub generation: u64,
}

pub(crate) fn model_dir() -> Result<PathBuf, String> {
    translator_core::settings::load_config()
        .tts_model_dir
        .map(PathBuf::from)
        .or_else(translator_core::paths::default_tts_model_dir)
        .ok_or_else(|| "无法确定本地语音模型目录".to_string())
}

pub(crate) fn model_installed(dir: &Path) -> bool {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        translator_tts::find_model_root(dir).is_some()
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = dir;
        false
    }
}

pub(crate) fn kokoro_installed(dir: &Path) -> bool {
    model_installed(dir)
}

pub(crate) fn vits_installed(dir: &Path) -> bool {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        translator_tts::find_vits_root(dir).is_some()
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = dir;
        false
    }
}

pub(crate) fn supertonic_installed(dir: &Path) -> bool {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        translator_tts::find_supertonic_root(dir).is_some()
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = dir;
        false
    }
}

/// Removes one downloaded voice pack. The caller confirms first; unloaded
/// engines release their file handles before the directory is deleted.
#[tauri::command]
pub fn delete_tts_model(app: AppHandle, model: String) -> Result<TtsModelStatus, String> {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};

        let dir = model_dir()?;
        let (subdir, label) = match model.as_str() {
            "kokoro" => (
                dir.join(KOKORO_DIR_NAME),
                "Kokoro 中英自然模型",
            ),
            "vits" => (
                dir.join(translator_core::models::VITS_ZH_MODEL_DIR),
                "MeloTTS 中文快速模型",
            ),
            "supertonic" => (
                dir.join(translator_core::models::SUPERTONIC_MODEL_DIR),
                "Supertonic 多语种模型",
            ),
            other => return Err(format!("unknown tts model: {other}")),
        };

        if !subdir.is_dir() {
            return Err("未找到可删除的语音包目录".to_string());
        }

        let confirmed = app
            .dialog()
            .message(format!(
                "删除{label}？删除后需要重新下载，当前本地朗读会回退到其他语音。"
            ))
            .title("删除语音包")
            .buttons(MessageDialogButtons::OkCancelCustom(
                "删除".to_string(),
                "取消".to_string(),
            ))
            .blocking_show();

        if !confirmed {
            return status(&app);
        }

        unload_tts_models(app.clone());
        std::fs::remove_dir_all(&subdir)
            .map_err(|error| format!("删除语音包失败：{error}"))?;
        emit_status(&app);
        status(&app)
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = (app, model);
        Err("本地朗读暂不支持当前系统".to_string())
    }
}

fn status(app: &AppHandle) -> Result<TtsModelStatus, String> {
    let state = app.state::<LocalTtsState>();
    let dir = model_dir()?;
    let kokoro = kokoro_installed(&dir);
    let vits = vits_installed(&dir);
    let supertonic = supertonic_installed(&dir);

    Ok(TtsModelStatus {
        installed: kokoro || vits || supertonic,
        downloading: state.downloading.load(Ordering::Relaxed),
        supported: local_tts_supported(),
        engine: MODEL_NAME,
        dir: dir.display().to_string(),
        kokoro_installed: kokoro,
        vits_installed: vits,
        supertonic_installed: supertonic,
        kokoro_size: translator_core::models::kokoro_model_total_size(),
        vits_size: translator_core::models::vits_zh_model_total_size(),
        supertonic_size: translator_core::models::supertonic_model_total_size(),
    })
}

fn emit_status(app: &AppHandle) {
    if let Ok(status) = status(app) {
        let _ = app.emit("tts-model-status", status);
    }
}

#[tauri::command]
pub fn tts_engine_supported() -> bool {
    local_tts_supported()
}

#[tauri::command]
pub fn tts_speakers() -> Vec<TtsSpeaker> {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        translator_tts::KOKORO_SPEAKERS
            .iter()
            .map(|(id, name, language)| TtsSpeaker {
                id: *id,
                name: (*name).to_string(),
                language: (*language).to_string(),
            })
            .collect()
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        Vec::new()
    }
}

#[tauri::command]
pub fn tts_multi_speakers() -> Vec<TtsSpeaker> {
    (0..10)
        .map(|id| TtsSpeaker {
            id,
            name: format!("音色 {id}"),
            language: "multi".to_string(),
        })
        .collect()
}

#[tauri::command]
pub fn tts_model_status(app: AppHandle) -> Result<TtsModelStatus, String> {
    status(&app)
}

#[tauri::command]
pub async fn preload_tts_model(app: AppHandle, model: String) -> Result<(), String> {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        preload_impl(app, model).await
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = (app, model);
        Err("本地朗读暂不支持当前系统".to_string())
    }
}

#[tauri::command]
pub async fn download_tts_model(app: AppHandle, model: String) -> Result<TtsModelStatus, String> {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        download_tts_model_impl(app, model).await
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = (app, model);
        Err("本地朗读暂不支持当前系统".to_string())
    }
}

#[tauri::command]
pub async fn tts_synthesize(
    app: AppHandle,
    text: String,
    lang: String,
    rate: f32,
    speaker: i32,
    local_engine: Option<String>,
    generation: u64,
) -> Result<TtsAudio, String> {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        synthesize_impl(app, text, lang, rate, speaker, local_engine, generation).await
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = (app, text, lang, rate, speaker, local_engine, generation);
        Err("本地朗读暂不支持当前系统".to_string())
    }
}

#[tauri::command]
pub fn stop_local_tts(app: AppHandle) {
    app.state::<LocalTtsState>()
        .cancel
        .store(true, Ordering::Relaxed);
}

#[tauri::command]
pub fn unload_tts_models(app: AppHandle) {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        let state = app.state::<LocalTtsState>();

        if let Ok(mut engine) = state.kokoro.lock() {
            *engine = None;
        }

        if let Ok(mut engine) = state.vits.lock() {
            *engine = None;
        }

        if let Ok(mut engine) = state.supertonic.lock() {
            *engine = None;
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = app;
    }
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
async fn preload_impl(app: AppHandle, model: String) -> Result<(), String> {
    let dir = model_dir()?;
    let state = app.state::<LocalTtsState>();

    match model.as_str() {
        "kokoro" => {
            if !kokoro_installed(&dir) {
                return Err("Kokoro 模型未下载".to_string());
            }
            let _ = ensure_kokoro(state.inner(), &dir).await?;
        }
        "vits" => {
            if !vits_installed(&dir) {
                return Err("MeloTTS 模型未下载".to_string());
            }
            let _ = ensure_vits(state.inner(), &dir).await?;
        }
        "supertonic" => {
            if !supertonic_installed(&dir) {
                return Err("Supertonic 模型未下载".to_string());
            }
            let _ = ensure_supertonic(state.inner(), &dir).await?;
        }
        other => return Err(format!("unknown tts model: {other}")),
    }

    Ok(())
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
async fn download_tts_model_impl(app: AppHandle, model: String) -> Result<TtsModelStatus, String> {
    let state = app.state::<LocalTtsState>();
    let _guard = state.download.lock().await;
    let dir = model_dir()?;

    let (model_name, files, subdir) = match model.as_str() {
        "kokoro" => (
            "kokoro",
            &translator_core::models::KOKORO_MODEL_FILES[..],
            KOKORO_DIR_NAME,
        ),
        "vits" => (
            "vits",
            &translator_core::models::VITS_ZH_MODEL_FILES[..],
            translator_core::models::VITS_ZH_MODEL_DIR,
        ),
        "supertonic" => (
            "supertonic",
            &translator_core::models::SUPERTONIC_MODEL_FILES[..],
            translator_core::models::SUPERTONIC_MODEL_DIR,
        ),
        other => return Err(format!("unknown tts model: {other}")),
    };

    state.downloading.store(true, Ordering::Relaxed);
    emit_status(&app);

    let result = download_model_files(&app, &dir, model_name, files, subdir).await;

    state.downloading.store(false, Ordering::Relaxed);
    emit_status(&app);

    result?;

    match model_name {
        "kokoro" => {
            if let Ok(mut engine) = state.kokoro.lock() {
                *engine = None;
            }
        }
        "vits" => {
            if let Ok(mut engine) = state.vits.lock() {
                *engine = None;
            }
        }
        "supertonic" => {
            if let Ok(mut engine) = state.supertonic.lock() {
                *engine = None;
            }
        }
        _ => {}
    }

    status(&app)
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
async fn download_model_files(
    app: &AppHandle,
    dir: &Path,
    model_name: &str,
    files: &[translator_core::models::ModelFile],
    subdir: &str,
) -> Result<(), String> {
    let client = translator_core::models::download_client()
        .map_err(|error| format!("下载初始化失败：{error}"))?;
    let total: u64 = files.iter().map(|file| file.size).sum();
    let total = total.max(1);
    let model_root = dir.join(subdir);
    std::fs::create_dir_all(&model_root)
        .map_err(|error| format!("创建语音模型目录失败：{error}"))?;
    let mut done_before = 0u64;

    for file in files {
        let dest = model_root.join(file.name);

        if dest.is_file()
            && translator_core::models::verify_sha256(&dest, file.sha256).unwrap_or(false)
        {
            done_before += file.size;
            continue;
        }

        let base = done_before;
        let mut last_emit = std::time::Instant::now();

        translator_core::models::download_model_file(&client, file, &model_root, |downloaded, _| {
            if last_emit.elapsed() >= std::time::Duration::from_millis(200) {
                last_emit = std::time::Instant::now();
                let _ = app.emit(
                    "tts-model-progress",
                    TtsModelProgress {
                        model: model_name.to_string(),
                        downloaded: base + downloaded,
                        total: Some(total),
                    },
                );
            }
        })
        .await
        .map_err(|error| format!("{}: {error}", file.name))?;

        done_before += file.size;
    }

    let installed = match model_name {
        "kokoro" => kokoro_installed(dir),
        "vits" => vits_installed(dir),
        "supertonic" => supertonic_installed(dir),
        _ => false,
    };

    if !installed {
        return Err("语音模型下载后校验失败".to_string());
    }

    Ok(())
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
async fn ensure_kokoro(
    state: &LocalTtsState,
    dir: &Path,
) -> Result<Arc<translator_tts::KokoroEngine>, String> {
    if let Some(engine) = state
        .kokoro
        .lock()
        .map_err(|_| "语音引擎状态异常".to_string())?
        .clone()
    {
        return Ok(engine);
    }

    let _guard = state.load.lock().await;

    if let Some(engine) = state
        .kokoro
        .lock()
        .map_err(|_| "语音引擎状态异常".to_string())?
        .clone()
    {
        return Ok(engine);
    }

    let load_dir = dir.to_path_buf();
    let loaded = tauri::async_runtime::spawn_blocking(move || {
        translator_tts::KokoroEngine::load(&load_dir)
    })
    .await
    .map_err(|error| format!("语音模型加载失败：{error}"))?
    .map_err(|error| error.to_string())?;
    let loaded = Arc::new(loaded);

    *state
        .kokoro
        .lock()
        .map_err(|_| "语音引擎状态异常".to_string())? = Some(loaded.clone());

    Ok(loaded)
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
async fn ensure_vits(
    state: &LocalTtsState,
    dir: &Path,
) -> Result<Arc<translator_tts::VitsEngine>, String> {
    if let Some(engine) = state
        .vits
        .lock()
        .map_err(|_| "语音引擎状态异常".to_string())?
        .clone()
    {
        return Ok(engine);
    }

    let _guard = state.load.lock().await;

    if let Some(engine) = state
        .vits
        .lock()
        .map_err(|_| "语音引擎状态异常".to_string())?
        .clone()
    {
        return Ok(engine);
    }

    let load_dir = dir.to_path_buf();
    let loaded = tauri::async_runtime::spawn_blocking(move || {
        translator_tts::VitsEngine::load(&load_dir)
    })
    .await
    .map_err(|error| format!("语音模型加载失败：{error}"))?
    .map_err(|error| error.to_string())?;
    let loaded = Arc::new(loaded);

    *state
        .vits
        .lock()
        .map_err(|_| "语音引擎状态异常".to_string())? = Some(loaded.clone());

    Ok(loaded)
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
async fn ensure_supertonic(
    state: &LocalTtsState,
    dir: &Path,
) -> Result<Arc<translator_tts::SupertonicEngine>, String> {
    if let Some(engine) = state
        .supertonic
        .lock()
        .map_err(|_| "语音引擎状态异常".to_string())?
        .clone()
    {
        return Ok(engine);
    }

    let _guard = state.load.lock().await;

    if let Some(engine) = state
        .supertonic
        .lock()
        .map_err(|_| "语音引擎状态异常".to_string())?
        .clone()
    {
        return Ok(engine);
    }

    let load_dir = dir.to_path_buf();
    let loaded = tauri::async_runtime::spawn_blocking(move || {
        translator_tts::SupertonicEngine::load(&load_dir)
    })
    .await
    .map_err(|error| format!("语音模型加载失败：{error}"))?
    .map_err(|error| error.to_string())?;
    let loaded = Arc::new(loaded);

    *state
        .supertonic
        .lock()
        .map_err(|_| "语音引擎状态异常".to_string())? = Some(loaded.clone());

    Ok(loaded)
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
enum EngineRef {
    Kokoro(Arc<translator_tts::KokoroEngine>),
    Vits(Arc<translator_tts::VitsEngine>),
    Supertonic(Arc<translator_tts::SupertonicEngine>),
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
impl EngineRef {
    fn speaker_count(&self) -> i32 {
        match self {
            Self::Kokoro(engine) => engine.speaker_count(),
            Self::Vits(engine) => engine.speaker_count(),
            Self::Supertonic(engine) => engine.speaker_count(),
        }
    }

    fn sample_rate(&self) -> i32 {
        match self {
            Self::Kokoro(engine) => engine.sample_rate(),
            Self::Vits(engine) => engine.sample_rate(),
            Self::Supertonic(engine) => engine.sample_rate(),
        }
    }

    fn synthesize(
        &self,
        text: &str,
        lang: &str,
        speaker: i32,
        speed: f32,
        cancel: Arc<AtomicBool>,
    ) -> Result<translator_tts::Audio, translator_tts::TtsError> {
        match self {
            Self::Kokoro(engine) => engine.synthesize(text, speaker, speed, cancel),
            Self::Vits(engine) => engine.synthesize(text, speaker, speed, cancel),
            Self::Supertonic(engine) => {
                engine.synthesize(text, lang, speaker, speed, cancel)
            }
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
async fn resolve_engine(
    state: &LocalTtsState,
    dir: &Path,
    lang: &str,
    local_engine: Option<&str>,
) -> Result<EngineRef, String> {
    let base = lang.to_ascii_lowercase();
    let base = base.split('-').next().unwrap_or("");
    let chinese = matches!(base, "zh" | "cmn" | "yue");
    let english = base == "en";

    if chinese {
        let prefer_fast = local_engine
            .filter(|value| matches!(*value, "fast" | "natural"))
            .map(|value| value != "natural")
            .unwrap_or_else(|| {
                translator_core::settings::load_config()
                    .tts_local_engine
                    .as_deref()
                    != Some("natural")
            });
        let has_kokoro = kokoro_installed(dir);
        let has_vits = vits_installed(dir);

        if prefer_fast && has_vits {
            return Ok(EngineRef::Vits(ensure_vits(state, dir).await?));
        }

        if has_kokoro {
            return Ok(EngineRef::Kokoro(ensure_kokoro(state, dir).await?));
        }

        if has_vits {
            return Ok(EngineRef::Vits(ensure_vits(state, dir).await?));
        }

        return Err("中文本地语音模型未下载，请在设置中下载".to_string());
    }

    if supertonic_installed(dir) {
        return Ok(EngineRef::Supertonic(
            ensure_supertonic(state, dir).await?,
        ));
    }

    if english && kokoro_installed(dir) {
        return Ok(EngineRef::Kokoro(ensure_kokoro(state, dir).await?));
    }

    Err("当前语言没有本地语音模型，请下载 Supertonic 或使用系统语音".to_string())
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
async fn synthesize_impl(
    app: AppHandle,
    text: String,
    lang: String,
    rate: f32,
    speaker: i32,
    local_engine: Option<String>,
    generation: u64,
) -> Result<TtsAudio, String> {
    if text.trim().is_empty() {
        return Err("朗读文本为空".to_string());
    }

    let dir = model_dir()?;
    let state = app.state::<LocalTtsState>();
    let engine =
        resolve_engine(state.inner(), &dir, &lang, local_engine.as_deref()).await?;

    let speaker = match &engine {
        EngineRef::Vits(_) => 0,
        EngineRef::Supertonic(_) => {
            if (0..=9).contains(&speaker) {
                speaker
            } else {
                6
            }
        }
        EngineRef::Kokoro(_) => {
            if (0..=102).contains(&speaker) {
                speaker
            } else {
                translator_tts::default_speaker(&lang)
            }
        }
    };
    let speaker = if (0..engine.speaker_count()).contains(&speaker) {
        speaker
    } else if lang.to_ascii_lowercase().starts_with("zh") {
        3
    } else {
        0
    };

    let _synth_guard = state.synth.lock().await;

    state.cancel.store(false, Ordering::Relaxed);
    let cancel = state.cancel.clone();
    let text = text.clone();
    let sample_rate = engine.sample_rate();

    let audio = tauri::async_runtime::spawn_blocking(move || {
        engine.synthesize(&text, &lang, speaker, rate, cancel)
    })
    .await
    .map_err(|error| format!("语音合成失败：{error}"))?
    .map_err(|error| error.to_string())?;

    let duration_ms = (audio.duration_seconds() * 1000.0) as u64;
    let wav_base64 = base64::engine::general_purpose::STANDARD.encode(audio.to_wav());

    Ok(TtsAudio {
        wav_base64,
        sample_rate: if sample_rate > 0 {
            sample_rate
        } else {
            audio.sample_rate
        },
        duration_ms,
        generation,
    })
}

#[cfg(all(test, any(target_os = "linux", target_os = "windows")))]
mod tests {
    use super::*;

    #[test]
    fn detects_a_flat_model_layout() {
        let dir = std::env::temp_dir().join(format!(
            "translator-local-tts-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("espeak-ng-data")).unwrap();
        std::fs::write(dir.join("model.int8.onnx"), b"model").unwrap();
        std::fs::write(dir.join("voices.bin"), b"voices").unwrap();
        std::fs::write(dir.join("tokens.txt"), b"tokens").unwrap();
        std::fs::write(dir.join("lexicon-zh.txt"), b"zh").unwrap();
        std::fs::write(dir.join("lexicon-us-en.txt"), b"en").unwrap();

        assert!(model_installed(&dir));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn multi_speakers_cover_the_model_ids() {
        let speakers = tts_multi_speakers();

        assert_eq!(speakers.len(), 10);
        assert_eq!(speakers[0].id, 0);
        assert_eq!(speakers[9].id, 9);
        assert!(speakers.iter().all(|speaker| speaker.language == "multi"));
    }
}
