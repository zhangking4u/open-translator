//! Live captions (实时字幕): capture the default output monitor with
//! `pw-record`, segment speech with Silero VAD (via `translator-asr`),
//! transcribe with SenseVoice and push the text to the caption overlay window
//! (`caption.html`).
//!
//! Linux-only for now: PipeWire monitor capture is the M1 target. Other
//! platforms keep a compiling stub so the tray item can report unavailability.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager};
use translator_service::domain::translation::{GlossaryTerm, TranslationRequest};

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
#[derive(Default)]
pub struct CaptionRuntime {
    task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
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

        #[cfg(target_os = "linux")]
        {
            let task = linux::spawn(app)?;
            self.active.store(true, Ordering::SeqCst);
            *self.task.lock().unwrap() = Some(task);
            Ok(())
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = app;
            Err("实时字幕目前仅支持 Linux".to_string())
        }
    }

    pub fn stop(&self) {
        self.active.store(false, Ordering::SeqCst);

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

#[cfg(target_os = "linux")]
mod linux {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use tauri::async_runtime::JoinHandle;
    use tokio::io::AsyncReadExt;
    use tokio::process::Command;
    use translator_asr::{SenseVoiceEngine, SpeechEngine, VoiceSegmenter};

    use super::{emit_segment, emit_status};

    const SAMPLE_RATE: i32 = 16_000;
    const FRAME_SAMPLES: usize = 1_600; // 100 ms
    const QUEUE: usize = 8;
    const MIN_SEGMENT_SAMPLES: usize = (SAMPLE_RATE as usize) / 4; // 250 ms
    const MODEL_FILES: [&str; 3] = ["model.int8.onnx", "tokens.txt", "silero_vad.onnx"];

    pub fn spawn(app: &tauri::AppHandle) -> Result<JoinHandle<()>, String> {
        let model_dir = resolve_model_dir()?;
        let missing: Vec<PathBuf> = MODEL_FILES
            .iter()
            .map(|name| model_dir.join(name))
            .filter(|path| !path.is_file())
            .collect();

        if !missing.is_empty() && !auto_download_enabled() {
            return Err(format!(
                "缺少语音模型文件：{}（可开启 auto_download 自动下载）",
                missing[0].display()
            ));
        }

        let language = resolve_language();
        let app = app.clone();

        Ok(tauri::async_runtime::spawn(async move {
            emit_status(&app, "starting", None);
            super::emit_config(&app);

            if let Err(error) = ensure_models(&app, &model_dir).await {
                let message = format!("实时字幕已停止：{error}");
                emit_status(&app, "error", Some(message.clone()));
                super::stop_after_failure(&app, &message);
                return;
            }

            let model = model_dir.join("model.int8.onnx");
            let tokens = model_dir.join("tokens.txt");
            let vad_model = model_dir.join("silero_vad.onnx");

            let loaded = {
                let model = model.clone();
                let tokens = tokens.clone();
                let vad_model = vad_model.clone();
                let language = language.clone();

                tauri::async_runtime::spawn_blocking(move || {
                    let engine = SenseVoiceEngine::load(&model, &tokens, &language, 4)?;
                    let vad = VoiceSegmenter::new(&vad_model, SAMPLE_RATE, 0.5, 6.0)?;
                    Ok::<_, translator_asr::SpeechError>((engine, vad))
                })
                .await
            };

            let (engine, vad) = match loaded {
                Ok(Ok(value)) => value,
                Ok(Err(error)) => {
                    emit_status(&app, "error", Some(error.to_string()));
                    super::stop_after_failure(&app, &format!("实时字幕已停止：{error}"));
                    return;
                }
                Err(error) => {
                    let message = format!("语音模型加载失败：{error}");
                    emit_status(&app, "error", Some(message.clone()));
                    super::stop_after_failure(&app, &message);
                    return;
                }
            };

            let engine = Arc::new(engine);

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

            let (sender, mut receiver) = tokio::sync::mpsc::channel::<Vec<f32>>(QUEUE);
            let worker_app = app.clone();
            let worker_engine = engine.clone();

            let worker = tauri::async_runtime::spawn(async move {
                while let Some(samples) = receiver.recv().await {
                    let engine = worker_engine.clone();
                    let result = tauri::async_runtime::spawn_blocking(move || {
                        engine.transcribe(&samples, SAMPLE_RATE)
                    })
                    .await;

                    match result {
                        Ok(Ok(transcript)) if !transcript.text.is_empty() => {
                            emit_segment(&worker_app, transcript.text.clone());
                            super::translate_segment(&worker_app, transcript.text);
                        }
                        Ok(Ok(_)) => {}
                        Ok(Err(error)) => eprintln!("caption: transcription failed: {error}"),
                        Err(error) => eprintln!("caption: worker failed: {error}"),
                    }
                }
            });

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

    /// Downloads any missing ASR/VAD file (existing files are verified and
    /// reused); progress is reported through `caption-status` with state
    /// `downloading`.
    async fn ensure_models(app: &tauri::AppHandle, model_dir: &Path) -> Result<(), String> {
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

    fn auto_download_enabled() -> bool {
        let config = translator_core::settings::load_config();

        match config.auto_download.as_deref() {
            Some(value) => crate::parse_bool("auto_download", value).unwrap_or(true),
            None => true,
        }
    }

    fn resolve_model_dir() -> Result<PathBuf, String> {
        let config = translator_core::settings::load_config();

        config
            .asr_model_dir
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from)
            .or_else(translator_core::paths::default_asr_model_dir)
            .ok_or_else(|| "无法确定语音模型目录".to_string())
    }

    fn resolve_language() -> String {
        let config = translator_core::settings::load_config();
        let language = config.caption_language.unwrap_or_default();

        match language.as_str() {
            "zh" | "en" | "ja" | "ko" | "yue" => language.clone(),
            _ => "auto".to_string(),
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
