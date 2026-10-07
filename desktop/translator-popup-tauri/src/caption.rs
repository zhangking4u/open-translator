//! Live captions (实时字幕): capture the default output device in loopback
//! (PipeWire monitor on Linux, WASAPI on Windows), segment speech with Silero
//! VAD (via `translator-asr`), transcribe with SenseVoice and push the text to
//! the caption overlay window (`caption.html`).
//!
//! macOS keeps a compiling stub so the tray item can report unavailability.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter, Manager};
use translator_service::domain::translation::{GlossaryTerm, TranslationRequest};

#[cfg(any(target_os = "linux", target_os = "windows"))]
use translator_asr::{SenseVoiceEngine, SpeechEngine};

pub const EVENT_SEGMENT: &str = "caption-segment";
pub const EVENT_TRANSLATION: &str = "caption-translation";
pub const EVENT_STATUS: &str = "caption-status";
pub const EVENT_CONFIG: &str = "caption-config";
pub const EVENT_EDITING: &str = "caption-editing";
const WINDOW: &str = "caption";
const MAX_GLOSSARY_TERMS: usize = 50;
const EDITING_SECONDS: u64 = 30;

#[derive(Clone, serde::Serialize)]
struct SegmentPayload {
    text: String,
}

#[derive(Clone, serde::Serialize)]
struct TranslationPayload {
    text: String,
    done: bool,
}

#[derive(Clone, serde::Serialize)]
struct StatusPayload {
    state: &'static str,
    message: Option<String>,
}

#[derive(Clone, serde::Serialize)]
struct ConfigPayload {
    layout: String,
}

#[derive(Clone, serde::Serialize)]
struct EditingPayload {
    active: bool,
}

/// True while the user repositions the overlay (`caption-editing`); window
/// moves are persisted only in that mode.
static EDITING: AtomicBool = AtomicBool::new(false);

/// Owns the running capture task. Cloned handles live in `AppState`; the task
/// itself is aborted (and the `pw-record` child killed) by [`Self::stop`].
/// `stop` also carries the cooperative flag read by capture loops that cannot
/// be cancelled by aborting the task (WASAPI).
#[derive(Default)]
pub struct CaptionRuntime {
    task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    stop: Mutex<Option<Arc<AtomicBool>>>,
    active: AtomicBool,
    /// Single-flight slot for the translation line: while one segment is
    /// translating, only the newest follow-up is kept.
    queue: translator_core::latest_wins::LatestWins<String>,
    session: Mutex<Option<PathBuf>>,
    started: Mutex<Option<std::time::Instant>>,
}

impl CaptionRuntime {
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    pub fn start(&self, app: &AppHandle) -> Result<(), String> {
        if self.is_active() {
            return Ok(());
        }

        let stop = Arc::new(AtomicBool::new(false));

        #[cfg(target_os = "linux")]
        let task = linux::spawn(app)?;

        #[cfg(target_os = "windows")]
        let task = windows::spawn(app, Arc::clone(&stop))?;

        #[cfg(any(target_os = "linux", target_os = "windows"))]
        {
            *self.stop.lock().unwrap() = Some(stop);
            self.active.store(true, Ordering::SeqCst);
            *self.task.lock().unwrap() = Some(task);
            Ok(())
        }

        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            let _ = (app, stop);
            Err("实时字幕目前仅支持 Linux/Windows".to_string())
        }
    }

    pub fn stop(&self) {
        self.active.store(false, Ordering::SeqCst);

        // Cooperative stop for capture loops that survive task abort.
        if let Some(stop) = self.stop.lock().unwrap().take() {
            stop.store(true, Ordering::SeqCst);
        }

        if let Some(task) = self.task.lock().unwrap().take() {
            task.abort();
        }
    }
}

fn caption_debug() -> bool {
    std::env::var_os("TRANSLATOR_CAPTION_DEBUG").is_some()
}

fn emit_status(app: &AppHandle, state: &'static str, message: Option<String>) {
    if caption_debug() {
        eprintln!("CAPDBG status={state} message={message:?}");
    }

    if let Err(error) = app.emit_to(WINDOW, EVENT_STATUS, StatusPayload { state, message }) {
        if caption_debug() {
            eprintln!("CAPDBG emit status failed: {error}");
        }
    }
}

fn emit_segment(app: &AppHandle, text: String) {
    if caption_debug() {
        eprintln!("CAPDBG segment chars={} text={text}", text.chars().count());
    }

    raise_caption(app);
    // Re-send the layout with every segment: it arrives in order before the
    // text, so the overlay never renders with a stale layout even if the
    // initial `caption-config` raced the webview load.
    emit_config(app);

    if layout() == "source" {
        append_transcript(app, &text, "");
    }

    if let Err(error) = app.emit_to(WINDOW, EVENT_SEGMENT, SegmentPayload { text }) {
        if caption_debug() {
            eprintln!("CAPDBG emit segment failed: {error}");
        }
    }
}

/// GNOME Wayland can leave a newly mapped XWayland window below the active
/// native window even with always-on-top set, so toggle the flag to force a
/// restack whenever there is something new to read.
fn raise_caption(app: &AppHandle) {
    let Some(window) = app.get_webview_window(WINDOW) else {
        return;
    };

    let _ = window.set_always_on_top(false);
    let _ = window.set_always_on_top(true);
}

/// Makes the overlay interactive so the user can drag it; click-through is
/// restored after [`EDITING_SECONDS`] or when the drag finishes. The overlay
/// is shown even when capture is off, so the position can be set early.
pub fn start_move(app: &AppHandle) {
    let Some(window) = app.get_webview_window(WINDOW) else {
        return;
    };

    EDITING.store(true, Ordering::SeqCst);
    let _ = window.show();
    let _ = window.set_ignore_cursor_events(false);
    // Force a restack so the overlay is actually visible while editing.
    let _ = window.set_always_on_top(false);
    let _ = window.set_always_on_top(true);
    let _ = app.emit_to(WINDOW, EVENT_EDITING, EditingPayload { active: true });

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(EDITING_SECONDS)).await;
        finish_move(&app);
    });
}

/// Ends the interactive repositioning mode; safe to call more than once.
pub fn finish_move(app: &AppHandle) {
    if !EDITING.swap(false, Ordering::SeqCst) {
        return;
    }

    let Some(window) = app.get_webview_window(WINDOW) else {
        return;
    };

    let _ = window.set_ignore_cursor_events(true);
    let _ = app.emit_to(WINDOW, EVENT_EDITING, EditingPayload { active: false });

    if !app.state::<crate::AppState>().caption.is_active() {
        let _ = window.hide();
    }
}

/// A background failure (model download or load) stops capture, hides the
/// overlay and puts the tray back to the off state.
fn stop_after_failure(app: &AppHandle, message: &str) {
    let state = app.state::<crate::AppState>();
    state.caption.active.store(false, Ordering::SeqCst);
    translator_core::settings::persist_value("caption_enabled", "false");

    if let Some(menu) = state.caption_menu.lock().unwrap().as_ref() {
        let _ = menu.toggle.set_checked(false);
    }

    if let Some(window) = app.get_webview_window(WINDOW) {
        let _ = window.hide();
    }

    crate::notify::show("OpenTranslator", message);
}

/// Reports a fatal caption startup failure (model download/load) through the
/// overlay and the tray, then stops capture.
#[cfg(any(target_os = "linux", target_os = "windows"))]
fn caption_failure(app: &AppHandle, error: String) {
    let message = format!("实时字幕已停止：{error}");
    emit_status(app, "error", Some(message.clone()));
    stop_after_failure(app, &message);
}

/// Caption layout: `bilingual` (source + translation), `translation`
/// (translation only) or `source` (no translation).
pub fn layout() -> String {
    let config = translator_core::settings::load_config();

    match config
        .caption_layout
        .unwrap_or_default()
        .trim()
        .to_lowercase()
        .as_str()
    {
        "translation" => "translation".to_string(),
        "source" => "source".to_string(),
        _ => "bilingual".to_string(),
    }
}

/// Pushes the current layout to the overlay (on start and on every change).
pub fn emit_config(app: &AppHandle) {
    let _ = app.emit_to(
        WINDOW,
        EVENT_CONFIG,
        ConfigPayload {
            layout: layout(),
        },
    );
}

/// Queues a finished segment for translation. Single-flight: while one
/// translation runs, only the newest follow-up is kept (the shared
/// `latest_wins` slot).
fn translate_segment(app: &AppHandle, text: String) {
    if layout() == "source" || text.trim().is_empty() {
        return;
    }

    let runtime = &app.state::<crate::AppState>().caption;
    let job = runtime.queue.submit(text);

    if let Some(text) = job {
        let app = app.clone();
        std::thread::spawn(move || run_caption_translation(&app, text));
    }
}

fn run_caption_translation(app: &AppHandle, text: String) {
    let engine = app.state::<crate::AppState>().engine.lock().unwrap().clone();

    let Some(engine) = engine else {
        // The translation model is not ready yet; keep showing the source line.
        finish_caption_translation(app);
        return;
    };

    let target = app
        .state::<Mutex<crate::TranslateConfig>>()
        .lock()
        .unwrap()
        .target
        .clone();
    let configured_source = translator_core::settings::load_config()
        .caption_language
        .unwrap_or_default();

    let request = TranslationRequest {
        text,
        source: if configured_source.trim().is_empty() {
            "auto".to_string()
        } else {
            configured_source
        },
        target,
        glossary: load_glossary(),
    };

    if caption_debug() {
        eprintln!(
            "CAPDBG translation glossary={} source={}",
            request.glossary.len(),
            request.source
        );
    }

    let started = std::time::Instant::now();
    let mut first_logged = false;

    let result = engine.translate_blocking_streaming(&request, {
        let app = app.clone();

        move |piece| {
            if !first_logged {
                first_logged = true;

                if caption_debug() {
                    eprintln!(
                        "CAPDBG translation first_ms={}",
                        started.elapsed().as_millis()
                    );
                }
            }

            let _ = app.emit_to(
                WINDOW,
                EVENT_TRANSLATION,
                TranslationPayload {
                    text: piece.to_string(),
                    done: false,
                },
            );
        }
    });

    match result {
        Ok(result) => {
            append_transcript(app, &request.text, &result.translated_text);

            let _ = app.emit_to(
                WINDOW,
                EVENT_TRANSLATION,
                TranslationPayload {
                    text: result.translated_text,
                    done: true,
                },
            );

            if caption_debug() {
                eprintln!("CAPDBG translation done_ms={}", started.elapsed().as_millis());
            }
        }
        Err(error) => {
            append_transcript(app, &request.text, "");

            if caption_debug() {
                eprintln!("CAPDBG translation failed: {error}");
            }
        }
    }

    finish_caption_translation(app);
}

fn finish_caption_translation(app: &AppHandle) {
    // The slot's take and idle transition share one lock, so a segment that
    // arrives concurrently is either handed over here or starts its own run;
    // the old take/store race cannot lose it.
    let next = app.state::<crate::AppState>().caption.queue.finish();

    if let Some(next) = next {
        let app = app.clone();
        std::thread::spawn(move || run_caption_translation(&app, next));
    }
}

/// `source=target` per line (comments start with `#`), capped so a runaway
/// file cannot blow up the prompt. Reads the configured path or
/// `glossary.txt` next to the config.
fn load_glossary() -> Vec<GlossaryTerm> {
    let config = translator_core::settings::load_config();
    let path = config
        .caption_glossary
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .or_else(translator_core::paths::glossary_path);
    let Some(path) = path else {
        return Vec::new();
    };
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };

    content
        .lines()
        .filter_map(|line| {
            let line = line.trim();

            if line.is_empty() || line.starts_with('#') {
                return None;
            }

            let (source, target) = line.split_once('=')?;
            let source = source.trim().to_string();
            let target = target.trim().to_string();

            (!source.is_empty() && !target.is_empty()).then_some(GlossaryTerm { source, target })
        })
        .take(MAX_GLOSSARY_TERMS)
        .collect()
}

/// HH:MM:SS since the session started.
fn format_timestamp(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;

    format!(
        "{:02}:{:02}:{:02}",
        total / 3600,
        (total / 60) % 60,
        total % 60
    )
}

/// One transcript entry: the source line and, when present, the translation.
fn transcript_entry(seconds: f64, source: &str, translation: &str) -> String {
    let stamp = format_timestamp(seconds);
    let mut entry = format!("[{stamp}] {source}\n");

    if !translation.trim().is_empty() {
        entry.push_str(&format!("[{stamp}] {translation}\n"));
    }

    entry.push('\n');
    entry
}

/// Creates the session transcript file and remembers it for
/// [`append_transcript`].
fn start_session(app: &AppHandle) {
    let Some(dir) = translator_core::paths::captions_dir() else {
        return;
    };

    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }

    let epoch_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or(0);
    let path = dir.join(format!("captions-{epoch_ms}.txt"));
    let header = format!("# OpenTranslator 实时字幕记录\n# 开始时间（Unix 毫秒）：{epoch_ms}\n\n");

    if std::fs::write(&path, header).is_err() {
        return;
    }

    let runtime = &app.state::<crate::AppState>().caption;
    *runtime.session.lock().unwrap() = Some(path);
    *runtime.started.lock().unwrap() = Some(std::time::Instant::now());
}

/// Appends a finalized entry to the session transcript; no-op without an
/// active session.
fn append_transcript(app: &AppHandle, source: &str, translation: &str) {
    let runtime = &app.state::<crate::AppState>().caption;
    let path = runtime.session.lock().unwrap().clone();
    let Some(path) = path else {
        return;
    };

    let seconds = runtime
        .started
        .lock()
        .unwrap()
        .map(|started| started.elapsed().as_secs_f64())
        .unwrap_or(0.0);

    if let Ok(mut file) = std::fs::OpenOptions::new().append(true).open(&path) {
        use std::io::Write;
        let _ = file.write_all(transcript_entry(seconds, source, translation).as_bytes());
    }
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
const SAMPLE_RATE: i32 = 16_000;

#[cfg(any(target_os = "linux", target_os = "windows"))]
const FRAME_SAMPLES: usize = 1_600; // 100 ms at 16 kHz

#[cfg(any(target_os = "linux", target_os = "windows"))]
const QUEUE: usize = 8;

#[cfg(any(target_os = "linux", target_os = "windows"))]
const MIN_SEGMENT_SAMPLES: usize = (SAMPLE_RATE as usize) / 4; // 250 ms

/// Fast synchronous model check run by `spawn` before any capture starts, so a
/// missing model fails `start()` with an actionable message. Returns the model
/// directory and the SenseVoice language tag.
#[cfg(any(target_os = "linux", target_os = "windows"))]
fn check_models() -> Result<(PathBuf, String), String> {
    let model_dir = resolve_model_dir()?;
    let missing: Vec<&str> = translator_core::models::ASR_MODEL_FILES
        .iter()
        .map(|file| file.name)
        .filter(|name| !model_dir.join(name).is_file())
        .collect();

    if !missing.is_empty() && !auto_download_enabled() {
        return Err(format!(
            "缺少语音模型文件：{}（可开启 auto_download 自动下载）",
            model_dir.join(missing[0]).display()
        ));
    }

    Ok((model_dir, resolve_language()))
}

/// Downloads any missing ASR/VAD file (existing files are verified and
/// reused); progress is reported through `caption-status` with state
/// `downloading`.
#[cfg(any(target_os = "linux", target_os = "windows"))]
async fn ensure_models(app: &AppHandle, model_dir: &std::path::Path) -> Result<(), String> {
    let client = translator_core::models::download_client()
        .map_err(|error| format!("下载初始化失败：{error}"))?;
    let total = translator_core::models::asr_model_total_size().max(1);
    let mut done_before = 0u64;

    for file in &translator_core::models::ASR_MODEL_FILES {
        let dest = model_dir.join(file.name);

        if dest.is_file()
            && translator_core::models::verify_sha256(&dest, file.sha256).unwrap_or(false)
        {
            done_before += file.size;
            continue;
        }

        let mut last_percent = u64::MAX;

        let result = translator_core::models::download_model_file(
            &client,
            file,
            model_dir,
            |downloaded, _| {
                let overall = done_before + downloaded;
                let percent = overall * 100 / total;

                if percent != last_percent {
                    last_percent = percent;
                    emit_status(
                        app,
                        "downloading",
                        Some(format!(
                            "正在下载语音模型：{percent}%（{}/{} MB）",
                            overall / 1_000_000,
                            total / 1_000_000
                        )),
                    );
                }
            },
        )
        .await;

        result.map_err(|error| format!("{}: {error}", file.name))?;
        done_before += file.size;
    }

    Ok(())
}

/// Ensures the models are present, then loads the recognizer and the VAD on a
/// blocking thread. Shared by the Linux and Windows capture modules.
#[cfg(any(target_os = "linux", target_os = "windows"))]
async fn prepare_engine(
    app: &AppHandle,
    model_dir: &std::path::Path,
    language: &str,
) -> Result<(Arc<SenseVoiceEngine>, translator_asr::VoiceSegmenter), String> {
    use translator_asr::VoiceSegmenter;

    ensure_models(app, model_dir).await?;

    let model = model_dir.join("model.int8.onnx");
    let tokens = model_dir.join("tokens.txt");
    let vad_model = model_dir.join("silero_vad.onnx");
    let language = language.to_string();

    tauri::async_runtime::spawn_blocking(move || {
        let engine = SenseVoiceEngine::load(&model, &tokens, &language, 4)?;
        let vad = VoiceSegmenter::new(&vad_model, SAMPLE_RATE, 0.5, 6.0)?;
        Ok::<_, translator_asr::SpeechError>((engine, vad))
    })
    .await
    .map_err(|error| format!("语音模型加载失败：{error}"))?
    .map(|(engine, vad)| (Arc::new(engine), vad))
    .map_err(|error| format!("语音模型加载失败：{error}"))
}

/// Consumes finished VAD segments: transcribes each on a blocking thread,
/// renders the source line and queues the translation. Shared by both capture
/// modules.
#[cfg(any(target_os = "linux", target_os = "windows"))]
fn spawn_transcription_worker(
    app: &AppHandle,
    engine: Arc<SenseVoiceEngine>,
    mut receiver: tokio::sync::mpsc::Receiver<Vec<f32>>,
) -> tauri::async_runtime::JoinHandle<()> {
    let app = app.clone();

    tauri::async_runtime::spawn(async move {
        while let Some(samples) = receiver.recv().await {
            let engine = engine.clone();
            let result =
                tauri::async_runtime::spawn_blocking(move || engine.transcribe(&samples, SAMPLE_RATE))
                    .await;

            match result {
                Ok(Ok(transcript)) if !transcript.text.is_empty() => {
                    emit_segment(&app, transcript.text.clone());
                    translate_segment(&app, transcript.text);
                }
                Ok(Ok(_)) => {}
                Ok(Err(error)) => eprintln!("caption: transcription failed: {error}"),
                Err(error) => eprintln!("caption: worker failed: {error}"),
            }
        }
    })
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn auto_download_enabled() -> bool {
    let config = translator_core::settings::load_config();

    match config.auto_download.as_deref() {
        Some(value) => crate::parse_bool("auto_download", value).unwrap_or(true),
        None => true,
    }
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn resolve_model_dir() -> Result<PathBuf, String> {
    let config = translator_core::settings::load_config();

    config
        .asr_model_dir
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .or_else(translator_core::paths::default_asr_model_dir)
        .ok_or_else(|| "无法确定语音模型目录".to_string())
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn resolve_language() -> String {
    let config = translator_core::settings::load_config();
    let language = config.caption_language.unwrap_or_default();

    match language.as_str() {
        "zh" | "en" | "ja" | "ko" | "yue" => language.clone(),
        _ => "auto".to_string(),
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use tauri::async_runtime::JoinHandle;
    use tokio::io::AsyncReadExt;
    use tokio::process::Command;

    use super::{emit_status, FRAME_SAMPLES, MIN_SEGMENT_SAMPLES, QUEUE};

    pub fn spawn(app: &tauri::AppHandle) -> Result<JoinHandle<()>, String> {
        let (model_dir, language) = super::check_models()?;
        let app = app.clone();

        Ok(tauri::async_runtime::spawn(async move {
            emit_status(&app, "starting", None);
            super::emit_config(&app);

            let (engine, vad) = match super::prepare_engine(&app, &model_dir, &language).await {
                Ok(value) => value,
                Err(error) => {
                    super::caption_failure(&app, error);
                    return;
                }
            };

            let mut child = match Command::new("pw-record")
                .args([
                    "--target",
                    "@DEFAULT_AUDIO_SINK@",
                    "--rate",
                    "16000",
                    "--channels",
                    "1",
                    "--format",
                    "s16",
                    "-a",
                    "-",
                ])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .kill_on_drop(true)
                .spawn()
            {
                Ok(child) => child,
                Err(error) => {
                    emit_status(
                        &app,
                        "error",
                        Some(format!("无法启动音频采集（pw-record）：{error}")),
                    );
                    return;
                }
            };

            let Some(mut stdout) = child.stdout.take() else {
                emit_status(&app, "error", Some("音频采集没有输出流".to_string()));
                return;
            };

            let (sender, receiver) = tokio::sync::mpsc::channel::<Vec<f32>>(QUEUE);
            let worker = super::spawn_transcription_worker(&app, engine, receiver);

            super::start_session(&app);
            emit_status(&app, "listening", None);

            let mut buffer = vec![0u8; FRAME_SAMPLES * 2];

            loop {
                if stdout.read_exact(&mut buffer).await.is_err() {
                    break;
                }

                let samples: Vec<f32> = buffer
                    .chunks_exact(2)
                    .map(|pair| i16::from_le_bytes([pair[0], pair[1]]) as f32 / 32768.0)
                    .collect();

                for segment in vad.accept(&samples) {
                    if segment.len() < MIN_SEGMENT_SAMPLES {
                        continue;
                    }

                    if sender.try_send(segment).is_err() {
                        eprintln!("caption: segment queue full; dropping audio");
                    }
                }
            }

            for segment in vad.flush() {
                let _ = sender.send(segment).await;
            }

            drop(sender);
            let _ = worker.await;
            emit_status(&app, "error", Some("音频采集已停止".to_string()));
        }))
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use tauri::async_runtime::JoinHandle;
    use translator_asr::VoiceSegmenter;
    use wasapi::{
        initialize_mta, AudioCaptureClient, AudioClient, Direction, DeviceEnumerator, Handle,
        SampleType, StreamMode, WasapiError, WaveFormat,
    };

    use super::{emit_status, FRAME_SAMPLES, MIN_SEGMENT_SAMPLES, QUEUE, SAMPLE_RATE};

    /// Consecutive capture-session failures before captions give up. A device
    /// switch (headphones plugged/unplugged) invalidates the stream and needs
    /// a fresh client, which the retry loop handles.
    const MAX_SESSION_FAILURES: u32 = 5;

    pub fn spawn(app: &tauri::AppHandle, stop: Arc<AtomicBool>) -> Result<JoinHandle<()>, String> {
        let (model_dir, language) = super::check_models()?;
        let app = app.clone();

        Ok(tauri::async_runtime::spawn(async move {
            emit_status(&app, "starting", None);
            super::emit_config(&app);

            let (engine, vad) = match super::prepare_engine(&app, &model_dir, &language).await {
                Ok(value) => value,
                Err(error) => {
                    super::caption_failure(&app, error);
                    return;
                }
            };

            let (sender, receiver) = tokio::sync::mpsc::channel::<Vec<f32>>(QUEUE);
            let worker = super::spawn_transcription_worker(&app, engine, receiver);

            // The wasapi client types are !Send, so construction, capture and
            // teardown all happen on this one blocking thread. The stop flag
            // cannot cancel blocking work, so the loop polls it.
            let capture_app = app.clone();
            let capture_stop = Arc::clone(&stop);
            let capture = tauri::async_runtime::spawn_blocking(move || {
                run_capture(&capture_app, &capture_stop, vad, sender)
            })
            .await;

            // The channel sender was dropped when the capture closure returned,
            // which ends the transcription worker.
            let _ = worker.await;

            match capture {
                Ok(Ok(())) => emit_status(&app, "error", Some("音频采集已停止".to_string())),
                Ok(Err(error)) => emit_status(&app, "error", Some(error)),
                Err(error) => emit_status(&app, "error", Some(format!("音频采集失败：{error}"))),
            }
        }))
    }

    fn run_capture(
        app: &tauri::AppHandle,
        stop: &AtomicBool,
        vad: VoiceSegmenter,
        sender: tokio::sync::mpsc::Sender<Vec<f32>>,
    ) -> Result<(), String> {
        let mut failures = 0u32;
        let mut session_started = false;

        while !stop.load(Ordering::SeqCst) {
            match capture_session(app, stop, &vad, &sender, &mut session_started) {
                Ok(()) => return Ok(()),
                Err(error) => {
                    if stop.load(Ordering::SeqCst) {
                        return Ok(());
                    }

                    failures += 1;

                    if failures >= MAX_SESSION_FAILURES {
                        return Err(format!("音频采集失败：{error}"));
                    }

                    emit_status(
                        app,
                        "starting",
                        Some(format!("音频设备连接中断，正在重试…（{error}）")),
                    );

                    for _ in 0..10 {
                        if stop.load(Ordering::SeqCst) {
                            return Ok(());
                        }

                        std::thread::sleep(Duration::from_millis(100));
                    }
                }
            }
        }

        Ok(())
    }

    fn capture_session(
        app: &tauri::AppHandle,
        stop: &AtomicBool,
        vad: &VoiceSegmenter,
        sender: &tokio::sync::mpsc::Sender<Vec<f32>>,
        session_started: &mut bool,
    ) -> Result<(), String> {
        initialize_mta()
            .ok()
            .map_err(|error| format!("COM 初始化失败：{error}"))?;

        let enumerator =
            DeviceEnumerator::new().map_err(|error| format!("设备枚举失败：{error}"))?;
        let device = enumerator
            .get_default_device(&Direction::Render)
            .map_err(|error| format!("无法打开默认输出设备：{error}"))?;
        let mut client = device
            .get_iaudioclient()
            .map_err(|error| format!("无法打开音频客户端：{error}"))?;

        // The audio engine resamples and mixes to 16 kHz mono f32 for us
        // (AUTOCONVERTPCM + SRC_DEFAULT_QUALITY). Drivers that reject the
        // combination fall back to the device mix format plus a local
        // converter.
        let desired = WaveFormat::new(32, 32, &SampleType::Float, SAMPLE_RATE as usize, 1, None);
        let format = match open_loopback(&mut client, &desired, true) {
            Ok(()) => desired,
            Err(_) => {
                client = device
                    .get_iaudioclient()
                    .map_err(|error| format!("无法打开音频客户端：{error}"))?;
                let mix = client
                    .get_mixformat()
                    .map_err(|error| format!("无法读取设备格式：{error}"))?;
                open_loopback(&mut client, &mix, false)
                    .map_err(|error| format!("回环采集初始化失败：{error}"))?;

                if super::caption_debug() {
                    eprintln!(
                        "CAPDBG wasapi fallback mix format rate={} ch={} bits={}",
                        mix.get_samplespersec(),
                        mix.get_nchannels(),
                        mix.get_bitspersample()
                    );
                }

                mix
            }
        };

        let converter = FormatConverter::new(&format)?;
        let event = client
            .set_get_eventhandle()
            .map_err(|error| format!("音频事件创建失败：{error}"))?;
        let capture = client
            .get_audiocaptureclient()
            .map_err(|error| format!("采集客户端创建失败：{error}"))?;

        client
            .start_stream()
            .map_err(|error| format!("音频采集启动失败：{error}"))?;

        if !*session_started {
            super::start_session(app);
            *session_started = true;
        }

        emit_status(app, "listening", None);

        let result = capture_loop(stop, vad, sender, &converter, &event, &capture);

        let _ = client.stop_stream();
        result
    }

    fn open_loopback(
        client: &mut AudioClient,
        format: &WaveFormat,
        autoconvert: bool,
    ) -> Result<(), WasapiError> {
        let mode = StreamMode::EventsShared {
            autoconvert,
            buffer_duration_hns: 100_000,
        };
        client.initialize_client(format, &Direction::Capture, &mode)
    }

    fn capture_loop(
        stop: &AtomicBool,
        vad: &VoiceSegmenter,
        sender: &tokio::sync::mpsc::Sender<Vec<f32>>,
        converter: &FormatConverter,
        event: &Handle,
        capture: &AudioCaptureClient,
    ) -> Result<(), String> {
        let mut deque: VecDeque<u8> = VecDeque::new();
        let mut pending: Vec<f32> = Vec::new();

        while !stop.load(Ordering::SeqCst) {
            match event.wait_for_event(100) {
                Ok(()) => {}
                Err(WasapiError::EventTimeout) => continue,
                Err(error) => return Err(format!("音频事件等待失败：{error}")),
            }

            loop {
                capture
                    .read_from_device_to_deque(&mut deque)
                    .map_err(|error| format!("读取音频失败：{error}"))?;

                let frames = deque.len() / converter.blockalign;

                if frames == 0 {
                    break;
                }

                let bytes: Vec<u8> = deque.drain(..frames * converter.blockalign).collect();
                pending.extend_from_slice(&converter.to_mono_16k(&bytes));

                while pending.len() >= FRAME_SAMPLES {
                    let frame: Vec<f32> = pending.drain(..FRAME_SAMPLES).collect();
                    push_segments(vad, sender, &frame);
                }
            }
        }

        // Flush the samples buffered after the last full frame plus whatever
        // the VAD is still holding, so the final sentence is not lost.
        if !pending.is_empty() {
            push_segments(vad, sender, &pending);
        }

        for segment in vad.flush() {
            if segment.len() < MIN_SEGMENT_SAMPLES {
                continue;
            }

            if sender.try_send(segment).is_err() {
                eprintln!("caption: segment queue full; dropping audio");
            }
        }

        Ok(())
    }

    fn push_segments(
        vad: &VoiceSegmenter,
        sender: &tokio::sync::mpsc::Sender<Vec<f32>>,
        samples: &[f32],
    ) {
        for segment in vad.accept(samples) {
            if segment.len() < MIN_SEGMENT_SAMPLES {
                continue;
            }

            if sender.try_send(segment).is_err() {
                eprintln!("caption: segment queue full; dropping audio");
            }
        }
    }

    /// Converts the initialized WASAPI format to 16 kHz mono f32. The common
    /// path is the engine's own 16 kHz mono f32 output; the converter only
    /// does real work when `AUTOCONVERTPCM` was rejected and the device mix
    /// format had to be used.
    struct FormatConverter {
        channels: usize,
        rate: u32,
        bits: u16,
        float: bool,
        blockalign: usize,
    }

    impl FormatConverter {
        fn new(format: &WaveFormat) -> Result<Self, String> {
            let bits = format.get_bitspersample();
            let float = matches!(format.get_subformat(), Ok(SampleType::Float));

            match (bits, float) {
                (16, false) | (32, true) | (32, false) => {}
                _ => return Err(format!("不支持的音频格式：{bits} 位（float={float}）")),
            }

            Ok(Self {
                channels: format.get_nchannels().max(1) as usize,
                rate: format.get_samplespersec().max(1),
                bits,
                float,
                blockalign: format.get_blockalign().max(1) as usize,
            })
        }

        fn sample(&self, bytes: &[u8]) -> f32 {
            match (self.bits, self.float) {
                (16, false) => i16::from_le_bytes([bytes[0], bytes[1]]) as f32 / 32_768.0,
                (32, true) => f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
                _ => i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as f32
                    / 2_147_483_648.0,
            }
        }

        /// Decodes, downmixes and resamples one packet to 16 kHz mono f32.
        fn to_mono_16k(&self, bytes: &[u8]) -> Vec<f32> {
            let bytes_per_sample = (self.bits / 8) as usize;
            let frames = bytes.len() / self.blockalign;
            let mut mono = Vec::with_capacity(frames);

            for frame in 0..frames {
                let base = frame * self.blockalign;
                let mut sum = 0.0f32;

                for channel in 0..self.channels {
                    let at = base + channel * bytes_per_sample;
                    sum += self.sample(&bytes[at..at + bytes_per_sample]);
                }

                mono.push(sum / self.channels as f32);
            }

            if mono.is_empty() || self.rate == SAMPLE_RATE as u32 {
                return mono;
            }

            // Box pre-filter + decimation/interpolation, only on the fallback
            // path when the audio engine refused to resample.
            let step = self.rate as f64 / SAMPLE_RATE as f64;
            let out_len = (mono.len() as f64 / step).floor() as usize;
            let mut out = Vec::with_capacity(out_len);
            let mut position = 0.0f64;

            for _ in 0..out_len {
                let end = position + step;
                let first = position.floor() as usize;
                let last = (end.ceil() as usize).max(first + 1).min(mono.len());
                let sum: f64 = mono[first..last].iter().map(|&value| value as f64).sum();
                out.push((sum / (last - first) as f64) as f32);
                position = end;
            }

            out
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn decodes_stereo_i16_at_48k_to_16k_mono() {
            // 48 frames at 48 kHz = 1 ms -> 16 output samples.
            let format = WaveFormat::new(16, 16, &SampleType::Int, 48_000, 2, None);
            let converter = FormatConverter::new(&format).unwrap();
            let mut bytes = Vec::new();

            for _ in 0..48 {
                for value in [16_384i16, 0i16] {
                    bytes.extend_from_slice(&value.to_le_bytes());
                }
            }

            let mono = converter.to_mono_16k(&bytes);
            assert_eq!(mono.len(), 16);

            for value in mono {
                assert!((value - 0.25).abs() < 0.001);
            }
        }

        #[test]
        fn passes_16k_mono_f32_through() {
            let format = WaveFormat::new(32, 32, &SampleType::Float, 16_000, 1, None);
            let converter = FormatConverter::new(&format).unwrap();
            let samples = [0.5f32, -0.25, 1.0];
            let mut bytes = Vec::new();

            for value in samples {
                bytes.extend_from_slice(&value.to_le_bytes());
            }

            assert_eq!(converter.to_mono_16k(&bytes), samples);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{format_timestamp, transcript_entry};

    #[test]
    fn formats_session_timestamps() {
        assert_eq!(format_timestamp(0.0), "00:00:00");
        assert_eq!(format_timestamp(61.9), "00:01:01");
        assert_eq!(format_timestamp(3_661.0), "01:01:01");
        assert_eq!(format_timestamp(-5.0), "00:00:00");
    }

    #[test]
    fn renders_bilingual_and_source_only_entries() {
        assert_eq!(
            transcript_entry(5.0, "hello", "你好"),
            "[00:00:05] hello\n[00:00:05] 你好\n\n"
        );
        assert_eq!(transcript_entry(5.0, "hello", ""), "[00:00:05] hello\n\n");
    }
}
