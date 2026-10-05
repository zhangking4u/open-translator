//! Live captions (实时字幕): capture the default output monitor with
//! `pw-record`, segment speech with Silero VAD (via `translator-asr`),
//! transcribe with SenseVoice and push the text to the caption overlay window
//! (`caption.html`).
//!
//! Linux-only for now: PipeWire monitor capture is the M1 target. Other
//! platforms keep a compiling stub so the tray item can report unavailability.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager};

pub const EVENT_SEGMENT: &str = "caption-segment";
pub const EVENT_STATUS: &str = "caption-status";
const WINDOW: &str = "caption";

#[derive(Clone, serde::Serialize)]
struct SegmentPayload {
    text: String,
}

#[derive(Clone, serde::Serialize)]
struct StatusPayload {
    state: &'static str,
    message: Option<String>,
}

/// Owns the running capture task. Cloned handles live in `AppState`; the task
/// itself is aborted (and the `pw-record` child killed) by [`Self::stop`].
#[derive(Default)]
pub struct CaptionRuntime {
    task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    active: AtomicBool,
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

#[cfg(target_os = "linux")]
mod linux {
    use std::path::PathBuf;
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

    pub fn spawn(app: &tauri::AppHandle) -> Result<JoinHandle<()>, String> {
        let model_dir = resolve_model_dir()?;
        let model = model_dir.join("model.int8.onnx");
        let tokens = model_dir.join("tokens.txt");
        let vad_model = model_dir.join("silero_vad.onnx");

        for path in [&model, &tokens, &vad_model] {
            if !path.is_file() {
                return Err(format!("缺少语音模型文件：{}", path.display()));
            }
        }

        let language = resolve_language();
        let app = app.clone();

        Ok(tauri::async_runtime::spawn(async move {
            emit_status(&app, "starting", None);

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
                    return;
                }
                Err(error) => {
                    emit_status(&app, "error", Some(format!("语音模型加载失败：{error}")));
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
                            emit_segment(&worker_app, transcript.text);
                        }
                        Ok(Ok(_)) => {}
                        Ok(Err(error)) => eprintln!("caption: transcription failed: {error}"),
                        Err(error) => eprintln!("caption: worker failed: {error}"),
                    }
                }
            });

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
