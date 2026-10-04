#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]

mod capture;
mod notify;
mod selection_watch;
mod server;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use translator_core::update::ReleaseInfo;
use tauri::tray::TrayIconBuilder;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use translator_core::history::HistoryEntry;
use translator_service::domain::prompt::PromptStyle;
use translator_service::domain::translation::TranslationRequest;
use translator_service::engine::llama_cpp::LlamaCppEngine;

const DEFAULT_HOTKEY: &str = "Ctrl+Alt+T";
const DEFAULT_N_CTX: u32 = 4096;

struct InitialView(Mutex<Option<String>>);

#[derive(Clone)]
struct ServerPlan {
    bind_addr: String,
    model_name: String,
}

#[derive(Clone)]
struct UpdateInfo {
    version: String,
    url: String,
    asset_url: Option<String>,
    asset_digest: Option<String>,
    can_install: bool,
}

#[derive(Default)]
struct UpdateSlot {
    item: Mutex<Option<MenuItem<tauri::Wry>>>,
    info: Mutex<Option<UpdateInfo>>,
    running: Mutex<bool>,
}

#[derive(Clone, serde::Serialize)]
struct UpdatePayload {
    version: String,
    can_install: bool,
}

#[derive(serde::Serialize)]
struct UpdateStatePayload {
    version: Option<String>,
    can_install: bool,
}

struct AppState {
    engine: Mutex<Option<Arc<LlamaCppEngine>>>,
    pending: Mutex<Option<String>>,
    last_text: Mutex<Option<String>>,
    last_translation: Mutex<Option<String>>,
    recents: Mutex<Vec<String>>,
    history: Mutex<Vec<HistoryEntry>>,
    pinned: Mutex<bool>,
    /// Bumped on every show so only the newest always-on-top pulse clears the
    /// state; an older timer must not cut a newer pulse short.
    pulse_generation: AtomicU64,
    replace_window: Mutex<Option<isize>>,
    /// Address the extension HTTP service would bind to (shown in 关于).
    extension_addr: String,
    /// Whether the extension HTTP service was started for this run; the
    /// settings switch only takes effect on the next start.
    extension_running: bool,
    /// Tray "划词翻译" check items, kept for check-state updates.
    selection_menu: Mutex<Option<SelectionMenu>>,
    /// Live 划词 settings; updated by the settings commands and read by the
    /// watcher on every poll.
    selection: Mutex<selection_watch::SelectionSettings>,
    /// A settled selection waiting for the floating ball to be hovered/clicked.
    pending_selection: Mutex<Option<PendingSelection>>,
    /// Bumped whenever the ball is shown/hidden so stale auto-hide timers do
    /// nothing.
    ball_generation: AtomicU64,
    /// The watcher ignores selections until this instant after a hotkey
    /// trigger (the hotkey captures the same selection itself).
    suppress_until: Mutex<Option<Instant>>,
    /// True while a translation is streaming. The engine is serialized, so
    /// watcher bursts must not stack translations; instead the newest text
    /// waits in `queued` and replaces any previous follow-up.
    translating: AtomicBool,
    queued: Mutex<Option<String>>,
    /// The card is currently on screen because the selection watcher showed
    /// it (as opposed to a hotkey/tray show); disabling the feature hides it.
    popup: AtomicBool,
    /// Text of the last committed (hovered) selection, so hovering the docked
    /// ball again only re-shows the card instead of translating twice.
    last_translated: Mutex<Option<String>>,
    /// Docked ball presentation state (see `BALL_*`).
    ball_state: AtomicU64,
    /// X11 destroys the selection owner with the last clipboard instance, so
    /// Linux keeps one alive for the app lifetime; Windows/macOS own the
    /// clipboard in the OS and use a short-lived instance per call.
    #[cfg(target_os = "linux")]
    clipboard: Mutex<Option<arboard::Clipboard>>,
}

/// Text captured by the watcher in `ball` mode, kept until the user moves the
/// mouse onto the ball (or clicks it).
#[derive(Clone)]
struct PendingSelection {
    text: String,
    /// Foreground window recorded at selection time, for replace-in-place.
    window: Option<isize>,
}

/// Tray menu handles for the 划词翻译 submenu, so the checkmarks can follow the
/// current mode.
struct SelectionMenu {
    off: CheckMenuItem<tauri::Wry>,
    ball: CheckMenuItem<tauri::Wry>,
    auto: CheckMenuItem<tauri::Wry>,
}

/// Docked ball states: dim when nothing is selected, lit when text is ready,
/// pulsing while the translation runs.
const BALL_IDLE: u64 = 0;
const BALL_ARMED: u64 = 1;
const BALL_BUSY: u64 = 2;

#[derive(Clone)]
struct TranslateConfig {
    source: String,
    target: String,
    detected: Option<String>,
    hotkey_label: String,
}

#[derive(Clone, serde::Serialize)]
struct LanguageOption {
    tag: String,
    label: String,
}

#[derive(Clone, serde::Serialize)]
struct LanguageState {
    source: String,
    target: String,
    detected: Option<String>,
    recent_targets: Vec<String>,
}

#[derive(Clone, serde::Serialize)]
struct SettingsPayload {
    hotkey: String,
    model_path: String,
    default_model_path: String,
    auto_download: bool,
    check_updates: bool,
    serve_extension: bool,
    pinned: bool,
    config_path: Option<String>,
    app_version: String,
    selection_mode: String,
    selection_method: String,
    selection_delay_ms: u64,
    selection_min_length: usize,
    selection_supported: bool,
    selection_ball_supported: bool,
    selection_auto_supported: bool,
    model_exists: bool,
    extension_addr: String,
    extension_running: bool,
}

#[derive(Clone, serde::Serialize)]
struct SourcePayload {
    text: String,
    replaceable: bool,
    detected: Option<String>,
}

#[derive(Clone, serde::Serialize)]
struct ProgressPayload {
    downloaded: u64,
    total: Option<u64>,
}

#[cfg(target_os = "linux")]
#[derive(Clone, serde::Serialize)]
struct SpeechEndedPayload {
    generation: u64,
}

#[derive(Clone, serde::Serialize)]
struct ErrorPayload {
    message: String,
}

/// Wayland has no keep-above protocol, so GTK's always-on-top (固定) is a
/// no-op under a native Wayland backend; XWayland supports it. When a Wayland
/// session offers X11, prefer the X11 backend unless the user explicitly chose
/// one with GDK_BACKEND (they can opt out with GDK_BACKEND=wayland and pinned
/// windows then only keep Esc/× from hiding them).
#[cfg(target_os = "linux")]
fn prefer_x11_backend() {
    let x11_available = std::env::var_os("DISPLAY").is_some();
    let on_wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
    let backend_forced = std::env::var_os("GDK_BACKEND").is_some();

    if x11_available && on_wayland && !backend_forced {
        unsafe {
            std::env::set_var("GDK_BACKEND", "x11");
        }
    }

    if selection_debug() || std::env::var_os("TRANSLATOR_SELECTION_DEBUG").is_some() {
        eprintln!(
            "SELDBG backend x11_available={x11_available} on_wayland={on_wayland} forced={backend_forced} gdk_now={:?}",
            std::env::var("GDK_BACKEND")
        );
    }
}

fn main() {
    #[cfg(target_os = "linux")]
    prefer_x11_backend();

    let args = match translator_core::args::Args::from_process_env() {
        Ok(args) => args,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };

    let config = translator_core::settings::load_config();

    let model_path = match resolve_model_path(&config) {
        Ok(path) => path,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };

    let prompt_style = match resolve_prompt_style(&config) {
        Ok(style) => style,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };

    let auto_download = match config.auto_download.as_deref() {
        Some(value) => match parse_bool("auto_download", value) {
            Ok(value) => value,
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        },
        None => true,
    };

    let start_pinned = match config.pinned.as_deref() {
        Some(value) => match parse_bool("pinned", value) {
            Ok(value) => value,
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        },
        None => false,
    };

    if args.print {
        run_print(&args, &model_path, prompt_style);
        return;
    }

    let hotkey_spec = config
        .hotkey
        .clone()
        .or_else(|| std::env::var("TRANSLATOR_HOTKEY").ok())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_HOTKEY.to_string());

    let serve_extension = config.serve_extension.as_deref() != Some("false");
    let server_plan = serve_extension.then(|| ServerPlan {
        bind_addr: translator_core::services::bind_addr_from_service_url(&args.service_url)
            .unwrap_or_else(|| "http://127.0.0.1:17890".to_string()),
        model_name: model_path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default(),
    });

    let extension_addr = server_plan
        .as_ref()
        .map(|plan| plan.bind_addr.clone())
        .unwrap_or_else(|| "http://127.0.0.1:17890".to_string());

    let translate_on_start = args.translate;
    let autostart = args.autostart;
    // A --translate cold start must not show the window before the capture
    // thread runs, or the synthesized Ctrl+C would land on our own window.
    let show_on_start = !autostart || args.stdin || args.settings || args.history;
    let initial_view = if args.settings {
        Some("settings".to_string())
    } else if args.history {
        Some("history".to_string())
    } else {
        None
    };
    let stdin_text = if args.stdin {
        translator_core::args::read_stdin().unwrap_or_default()
    } else {
        String::new()
    };
    let translate_config = TranslateConfig {
        source: args.source.clone(),
        target: args.target.clone(),
        detected: None,
        hotkey_label: hotkey_spec.clone(),
    };
    let recent_targets = args.recent_targets.clone();

    tauri::Builder::default()
        .manage(AppState {
            engine: Mutex::new(None),
            pending: Mutex::new(None),
            last_text: Mutex::new(None),
            last_translation: Mutex::new(None),
            recents: Mutex::new(recent_targets),
            history: Mutex::new(translator_core::history::load()),
            pinned: Mutex::new(start_pinned),
            pulse_generation: AtomicU64::new(0),
            replace_window: Mutex::new(None),
            extension_addr,
            extension_running: server_plan.is_some(),
            selection_menu: Mutex::new(None),
            selection: Mutex::new(selection_watch::SelectionSettings::from_config(&config)),
            pending_selection: Mutex::new(None),
            ball_generation: AtomicU64::new(0),
            suppress_until: Mutex::new(None),
            translating: AtomicBool::new(false),
            queued: Mutex::new(None),
            popup: AtomicBool::new(false),
            last_translated: Mutex::new(None),
            ball_state: AtomicU64::new(BALL_IDLE),
            #[cfg(target_os = "linux")]
            clipboard: Mutex::new(None),
        })
        .manage(Mutex::new(translate_config))
        .manage(InitialView(Mutex::new(initial_view)))
        .manage(UpdateSlot::default())
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if argv.iter().any(|arg| arg == "--translate") {
                trigger_translation(app);
            } else if argv.iter().any(|arg| arg == "--settings") {
                show_main_in_place(app);
                let _ = app.emit("open-settings", ());
            } else if argv.iter().any(|arg| arg == "--history") {
                show_main_in_place(app);
                let _ = app.emit("open-history", ());
            } else {
                show_main_in_place(app);
            }
        }))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        trigger_translation(app);
                    }
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            hide_window,
            quit_app,
            resize_window,
            center_window,
            copy_text,
            tts_backend,
            tts_voice_available,
            speak_text,
            stop_speaking,
            retranslate,
            ball_hover,
            ball_click,
            card_engaged,
            save_selection_mode,
            save_selection_method,
            autostart_enabled,
            set_autostart,
            move_ball_by,
            save_ball_position,
            get_settings,
            save_hotkey,
            save_model_path,
            save_switch,
            open_config_dir,
            open_model_location,
            get_history,
            load_history_entry,
            clear_history,
            set_pinned,
            get_initial_view,
            platform,
            start_update,
            open_release_page,
            check_update_now,
            get_update_state,
            replace_text,
            get_languages,
            get_language_state,
            set_source,
            set_target,
            swap_languages
        ])
        .setup(move |app| {
            let handle = app.handle().clone();

            build_tray(&handle, &hotkey_spec)?;

            if let Err(error) = build_ball_window(&handle) {
                eprintln!("failed to create the selection ball window: {error}");
            }

            selection_watch::ensure(handle.clone());
            apply_ball_visibility(&handle);

            if start_pinned {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.set_always_on_top(true);
                }
            }

            if autostart {
                // The AppIndicator extension can drop our menu labels when the
                // app starts together with the session (a concurrent layout
                // update cancels its property fetch and there is no retry), and
                // it only re-reads the properties when an update arrives while
                // the menu is closed. Nudge the update item once the Shell has
                // settled so the next menu open refills the labels.
                let nudged = handle.clone();
                std::thread::spawn(move || {
                    // Nudge at ~4 s and ~15 s after startup.
                    for delay in [4u64, 11] {
                        std::thread::sleep(std::time::Duration::from_secs(delay));

                        let slot = nudged.state::<UpdateSlot>();

                        if slot.info.lock().unwrap().is_some() {
                            return;
                        }

                        if let Some(item) = slot.item.lock().unwrap().as_ref() {
                            let _ = item.set_enabled(true);
                            let _ = item.set_enabled(false);
                        }
                    }
                });
            }

            if let Err(error) = app.global_shortcut().register(hotkey_spec.as_str()) {
                eprintln!("failed to register {hotkey_spec}: {error}");
                let message = format!("快捷键注册失败：{error}");

                if window_hidden(&handle) {
                    notify::show("OpenTranslator", &message);
                }

                let _ = app.emit("error", ErrorPayload { message });
            }

            spawn_model_startup(
                handle.clone(),
                model_path,
                auto_download,
                prompt_style,
                server_plan,
            );

            if config.check_updates.as_deref() != Some("false") {
                spawn_update_check(handle.clone(), false);
            } else if let Some(item) = app.state::<UpdateSlot>().item.lock().unwrap().as_ref() {
                let _ = item.set_text("更新检查已关闭");
            }

            if show_on_start {
                show_main(&handle);
            }

            if !stdin_text.trim().is_empty() {
                translate_text(&handle, stdin_text);
            } else if translate_on_start {
                trigger_translation(&handle);
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();

                if !*window.state::<AppState>().pinned.lock().unwrap() {
                    let _ = window.hide();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running OpenTranslator");
}

fn run_print(args: &translator_core::args::Args, model_path: &PathBuf, prompt_style: PromptStyle) {
    let engine = match LlamaCppEngine::load(
        model_path.to_string_lossy().as_ref(),
        prompt_style,
        DEFAULT_N_CTX,
    ) {
        Ok(engine) => engine,
        Err(error) => {
            eprintln!("failed to load {}: {error}", model_path.display());
            std::process::exit(1);
        }
    };

    let text = if args.stdin {
        translator_core::args::read_stdin().unwrap_or_default()
    } else {
        String::new()
    };

    if text.trim().is_empty() {
        eprintln!("no text to translate");
        std::process::exit(1);
    }

    let request = TranslationRequest {
        text,
        source: args.source.clone(),
        target: args.target.clone(),
    };

    match engine.translate_blocking(&request) {
        Ok(result) => println!("{}", result.translated_text),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}

fn spawn_model_startup(
    app: AppHandle,
    model_path: PathBuf,
    auto_download: bool,
    prompt_style: PromptStyle,
    server_plan: Option<ServerPlan>,
) {
    std::thread::spawn(move || {
        let mut downloaded_now = false;

        if !model_path.is_file() {
            if !auto_download {
                fail_model(
                    &app,
                    format!(
                        "模型文件不存在：{}（可设置 model_path，或开启 auto_download）",
                        model_path.display()
                    ),
                );
                return;
            }

            let client = match translator_core::models::download_client() {
                Ok(client) => client,
                Err(error) => {
                    fail_model(&app, format!("下载初始化失败：{error}"));
                    return;
                }
            };

            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    fail_model(&app, format!("运行时启动失败：{error}"));
                    return;
                }
            };

            let progress_app = app.clone();
            let result = runtime.block_on(translator_core::models::download(
                &client,
                translator_core::models::DEFAULT_MODEL_URL,
                &model_path,
                Some(translator_core::models::DEFAULT_MODEL_SHA256),
                move |downloaded, total| {
                    let _ = progress_app.emit(
                        "model-progress",
                        ProgressPayload { downloaded, total },
                    );
                    set_tooltip(
                        &progress_app,
                        &format!(
                            "OpenTranslator · {}",
                            translator_core::models::format_download_status(downloaded, total)
                        ),
                    );
                },
            ));

            if let Err(error) = result {
                fail_model(&app, format!("模型下载失败：{error}"));
                return;
            }

            downloaded_now = true;
        }

        match LlamaCppEngine::load(
            model_path.to_string_lossy().as_ref(),
            prompt_style,
            DEFAULT_N_CTX,
        ) {
            Ok(engine) => {
                let engine = Arc::new(engine);

                {
                    let state = app.state::<AppState>();
                    *state.engine.lock().unwrap() = Some(Arc::clone(&engine));
                }

                if let Some(plan) = server_plan {
                    if let Err(error) =
                        server::start(engine, plan.bind_addr, plan.model_name)
                    {
                        let message = format!("扩展接口启动失败：{error}");

                        if window_hidden(&app) {
                            notify::show("OpenTranslator", &message);
                        }

                        let _ = app.emit("error", ErrorPayload { message });
                    }
                }

                set_tooltip(&app, &tooltip_ready(&app));
                let _ = app.emit("model-ready", ());

                if downloaded_now && window_hidden(&app) {
                    let hotkey = app
                        .state::<Mutex<TranslateConfig>>()
                        .lock()
                        .unwrap()
                        .hotkey_label
                        .clone();
                    notify::show(
                        "OpenTranslator",
                        &format!("模型下载完成，按 {hotkey} 开始翻译"),
                    );
                }

                let pending = app.state::<AppState>().pending.lock().unwrap().take();
                if let Some(text) = pending {
                    translate_text(&app, text);
                }
            }
            Err(error) => fail_model(&app, format!("模型加载失败：{error}")),
        }
    });
}

fn fail_model(app: &AppHandle, message: String) {
    set_tooltip(app, &format!("OpenTranslator · {message}"));
    let _ = app.emit(
        "model-error",
        ErrorPayload {
            message: message.clone(),
        },
    );

    if window_hidden(app) {
        notify::show("OpenTranslator", &message);
    }
}

fn trigger_translation(app: &AppHandle) {
    // The hotkey captures the selection itself; drop a pending follow-ball on
    // the selection-following platforms and keep the watcher quiet so the same
    // selection is not translated twice. The docked ball stays put.
    if !selection_watch::ball_docked() {
        hide_ball(app);
    }

    mark_suppressed(app, Duration::from_millis(1200));

    let app = app.clone();
    std::thread::spawn(move || {
        // Capture before showing the window: the synthesized Ctrl+C must go to
        // the app the user selected text in, not to our own window.
        let window = capture::foreground_window();
        *app.state::<AppState>().replace_window.lock().unwrap() = window;
        let captured = capture::capture_selection();

        show_main(&app);

        match captured {
            Ok(text) if text.trim().is_empty() => {
                let _ = app.emit("empty", ());
            }
            Ok(text) => {
                translate_text(&app, text);
            }
            Err(error) if error == capture::NO_SELECTION => {
                let _ = app.emit("empty", ());
            }
            Err(error) => {
                let _ = app.emit("error", ErrorPayload { message: error });
            }
        }
    });
}

/// The ball stays visible for a while after a selection; ignore any timer that
/// belongs to a superseded show.
#[cfg(any(target_os = "windows", target_os = "macos"))]
const BALL_TIMEOUT: Duration = Duration::from_secs(5);

fn mark_suppressed(app: &AppHandle, duration: Duration) {
    *app.state::<AppState>().suppress_until.lock().unwrap() = Some(Instant::now() + duration);
}

/// `TRANSLATOR_SELECTION_DEBUG=1` enables SELDBG diagnostics for the selection
/// feature (used to bisect real-machine interference).
fn selection_debug() -> bool {
    std::env::var_os("TRANSLATOR_SELECTION_DEBUG").is_some()
}

fn hide_ball(app: &AppHandle) {
    let state = app.state::<AppState>();
    state.ball_generation.fetch_add(1, Ordering::SeqCst);
    state.ball_state.store(BALL_IDLE, Ordering::SeqCst);
    *state.pending_selection.lock().unwrap() = None;
    *state.last_translated.lock().unwrap() = None;

    if let Some(ball) = app.get_webview_window("ball") {
        let _ = ball.hide();
    }
}

/// Docked-ball state change: store it and refresh visibility + the ball UI.
fn set_ball_state(app: &AppHandle, state_value: u64) {
    app.state::<AppState>()
        .ball_state
        .store(state_value, Ordering::SeqCst);
    apply_ball_visibility(app);
}

/// The Linux watcher arms the docked ball instead of translating directly:
/// store the settled selection and light the ball; clearing the selection
/// returns it to idle. Only actual state changes touch the ball window, so an
/// idle watcher cannot make it flicker or move.
#[cfg(target_os = "linux")]
fn set_selection_pending(app: &AppHandle, pending: Option<PendingSelection>) {
    let state = app.state::<AppState>();
    let armed = pending.is_some();

    if !armed {
        *state.last_translated.lock().unwrap() = None;
    }

    let had_pending = {
        let mut current = state.pending_selection.lock().unwrap();
        let had_pending = current.is_some();
        *current = pending;
        had_pending
    };

    let target = if armed { BALL_ARMED } else { BALL_IDLE };

    if had_pending != armed || state.ball_state.load(Ordering::SeqCst) != target {
        set_ball_state(app, target);
    }
}

/// Show/hide the docked ball based on the mode and the user's visibility
/// setting, and push the state to `ball.js` (`idle`/`armed`/`busy`).
fn apply_ball_visibility(app: &AppHandle) {
    #[cfg(target_os = "linux")]
    {
        let state = app.state::<AppState>();
        let settings = *state.selection.lock().unwrap();
        let ball_state = state.ball_state.load(Ordering::SeqCst);

        let state_name = match ball_state {
            BALL_ARMED => "armed",
            BALL_BUSY => "busy",
            _ => "idle",
        };
        let _ = app.emit_to("ball", "ball-state", state_name);

        let Some(ball) = app.get_webview_window("ball") else {
            return;
        };

        let visible = settings.mode != selection_watch::SelectionMode::Off
            && (settings.ball_visibility == selection_watch::BallVisibility::Always
                || ball_state != BALL_IDLE);
        let was_visible = ball.is_visible().unwrap_or(false);

        if visible && !was_visible {
            // Belt and braces against GTK default-size locking: enforce the
            // ball size right before it is realized for the first time.
            let size = tauri::LogicalSize::new(44.0, 44.0);
            let _ = ball.set_min_size(Some(size));
            let _ = ball.set_max_size(Some(size));
            let _ = ball.set_size(size);

            place_docked_ball(app);
            let _ = ball.show();

            if crate::selection_debug() {
                eprintln!(
                    "SELDBG ball shown size={:?}",
                    ball.outer_size().map(|size| (size.width, size.height))
                );
            }
        } else if !visible && was_visible {
            let _ = ball.hide();
        }

        if crate::selection_debug() && visible != was_visible {
            eprintln!("SELDBG ball visible={visible} was={was_visible} state={state_name}");
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        // The selection-following platforms keep their own ball show/hide
        // flow; reading the state here also keeps it shared across platforms.
        let _ = app.state::<AppState>().ball_state.load(Ordering::SeqCst);
    }
}

/// Default dock on a given monitor: right edge, vertically centered.
#[cfg(target_os = "linux")]
fn default_ball_on(monitor: &tauri::Monitor) -> (i32, i32) {
    let scale = monitor.scale_factor();
    let edge = (44.0 * scale).round() as i32;
    let margin = (8.0 * scale) as i32;
    let work = monitor.work_area();

    (
        work.position.x + work.size.width as i32 - edge - margin,
        work.position.y + (work.size.height as i32 - edge) / 2,
    )
}

/// The default dock position: right edge, vertically centered on the primary
/// monitor. Dragging the ball back near this point resets the custom position.
#[cfg(target_os = "linux")]
fn default_ball_position(ball: &WebviewWindow) -> Option<(i32, i32)> {
    ball.primary_monitor()
        .ok()
        .flatten()
        .map(|monitor| default_ball_on(&monitor))
}

/// Places the docked ball at its configured (or default) position, clamped to
/// the monitor work area so a monitor change cannot lose it off-screen. The
/// ball is a fixed 44-logical-pixel square, so the geometry does not depend on
/// GTK's possibly-stale reported size before the first realization.
#[cfg(target_os = "linux")]
fn place_docked_ball(app: &AppHandle) {
    let Some(ball) = app.get_webview_window("ball") else {
        return;
    };

    let Some(monitor) = ball
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| ball.primary_monitor().ok().flatten())
    else {
        return;
    };

    let scale = monitor.scale_factor();
    let edge = (44.0 * scale).round() as i32;
    let inset = (4.0 * scale) as i32;
    let work = monitor.work_area();

    let config = translator_core::settings::load_config();
    let custom = config
        .ball_x
        .as_deref()
        .and_then(|value| value.trim().parse::<i32>().ok())
        .zip(
            config
                .ball_y
                .as_deref()
                .and_then(|value| value.trim().parse::<i32>().ok()),
        );

    let (mut x, mut y) = custom.unwrap_or_else(|| {
        default_ball_position(&ball).unwrap_or_else(|| default_ball_on(&monitor))
    });

    let min_x = work.position.x + inset;
    let min_y = work.position.y + inset;
    let max_x = work.position.x + work.size.width as i32 - edge - inset;
    let max_y = work.position.y + work.size.height as i32 - edge - inset;

    x = x.clamp(min_x, max_x.max(min_x));
    y = y.clamp(min_y, max_y.max(min_y));

    let _ = ball.set_position(PhysicalPosition::new(x, y));
}

/// The card appears beside the docked ball (predictable position): on the
/// side of the ball that faces the work-area center, vertically centered on it.
#[cfg(target_os = "linux")]
fn show_card_next_to_ball(app: &AppHandle) {
    let Some(card) = app.get_webview_window("main") else {
        return;
    };

    app.state::<AppState>()
        .popup
        .store(true, Ordering::SeqCst);

    let was_visible = card.is_visible().unwrap_or(false);

    if !was_visible {
        let _ = card.set_focusable(false);
    }

    let _ = card.show();

    if crate::selection_debug() {
        eprintln!("SELDBG card popup was_visible={was_visible}");
    }

    if was_visible {
        // Already visible but likely behind the app the user is working in:
        // raise it without stealing focus.
        pulse_above(app, &card);
        return;
    }

    let Ok(card_size) = card.outer_size().or_else(|_| card.inner_size()) else {
        return;
    };

    let Some(ball) = app.get_webview_window("ball") else {
        place_popup_corner(&card);
        return;
    };

    let (Ok(ball_position), Ok(ball_size)) =
        (ball.outer_position(), ball.outer_size().or_else(|_| ball.inner_size()))
    else {
        place_popup_corner(&card);
        return;
    };

    let gap = 12;
    let ball_center_x = ball_position.x + ball_size.width as i32 / 2;
    let ball_center_y = ball_position.y + ball_size.height as i32 / 2;

    let (work_x, work_y, work_right, work_bottom) = ball
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| ball.primary_monitor().ok().flatten())
        .map(|monitor| {
            let work = monitor.work_area();

            (
                work.position.x,
                work.position.y,
                work.position.x + work.size.width as i32,
                work.position.y + work.size.height as i32,
            )
        })
        .unwrap_or((0, 0, i32::MAX, i32::MAX));

    let margin = 12;
    let mut x = if ball_center_x * 2 >= work_x + work_right {
        ball_position.x - card_size.width as i32 - gap
    } else {
        ball_position.x + ball_size.width as i32 + gap
    };
    let mut y = ball_center_y - card_size.height as i32 / 2;

    x = x.clamp(work_x + margin, work_right - card_size.width as i32 - margin);
    y = y.clamp(work_y + margin, work_bottom - card_size.height as i32 - margin);

    let _ = card.set_position(PhysicalPosition::new(x, y));
    pulse_above(app, &card);
}

/// After a translation finishes: if the translated selection is still the
/// pending one, the confirmation is complete and the ball goes back to dim
/// (idle) so it can signal the next selection; a newer selection armed while
/// the translation ran keeps it lit. Re-hovering the same selection only
/// re-shows the card (`last_translated` is kept for that).
fn refresh_ball_state(app: &AppHandle) {
    let state = app.state::<AppState>();
    let translated = state.last_translated.lock().unwrap().clone();
    let pending_text = state
        .pending_selection
        .lock()
        .unwrap()
        .as_ref()
        .map(|pending| pending.text.clone());

    match (translated, pending_text) {
        (Some(translated), Some(pending)) if pending == translated => {
            *state.pending_selection.lock().unwrap() = None;
            set_ball_state(app, BALL_IDLE);
        }
        (_, Some(_)) => set_ball_state(app, BALL_ARMED),
        (_, None) => set_ball_state(app, BALL_IDLE),
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn show_ball(app: &AppHandle, text: String, anchor: (i32, i32), window: Option<isize>) {
    let state = app.state::<AppState>();
    let generation = state.ball_generation.fetch_add(1, Ordering::SeqCst) + 1;
    *state.pending_selection.lock().unwrap() = Some(PendingSelection { text, window });
    drop(state);

    place_ball_window(app, anchor);

    // The selection-following platforms never dock the ball in an idle state:
    // whenever it is mapped, a selection is ready, so it renders armed (accent
    // colour, full opacity) instead of the faint grey idle look.
    let _ = app.emit_to("ball", "ball-state", "armed");

    if let Some(ball) = app.get_webview_window("ball") {
        let _ = ball.show();
    }

    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(BALL_TIMEOUT);

        let state = handle.state::<AppState>();

        if state.ball_generation.load(Ordering::SeqCst) != generation {
            return;
        }

        state.ball_generation.fetch_add(1, Ordering::SeqCst);
        *state.pending_selection.lock().unwrap() = None;

        if let Some(ball) = handle.get_webview_window("ball") {
            let _ = ball.hide();
        }
    });
}

/// The user hovered the ball: this is the commit point.
fn hover_ball(app: &AppHandle) {
    #[cfg(target_os = "linux")]
    {
        // Typing in an input mirrors text to PRIMARY; never translate that.
        if !crate::selection_watch::atspi_selection_ok() {
            return;
        }

        // Docked ball: it stays visible and the card appears beside it.
        // Hovering the same selection again only re-shows the existing card.
        // The current PRIMARY is read right here: hovering is an explicit
        // confirmation, so a fresh selection must not wait for the settle
        // timer (which used to make the card appear "late" or not at all).
        let armed = app
            .state::<AppState>()
            .pending_selection
            .lock()
            .unwrap()
            .clone();

        let settings = *app.state::<AppState>().selection.lock().unwrap();
        let fresh = crate::selection_watch::read_primary_now().filter(|text| settings.accepts(text));
        let text = fresh.or_else(|| armed.as_ref().map(|pending| pending.text.clone()));

        let Some(text) = text else {
            return;
        };

        if crate::selection_debug() {
            eprintln!("SELDBG hover len={}", text.chars().count());
        }

        let already_translated = app
            .state::<AppState>()
            .last_translated
            .lock()
            .unwrap()
            .as_deref()
            == Some(text.as_str());

        if already_translated {
            show_card_next_to_ball(app);
            return;
        }

        let window = armed.and_then(|pending| pending.window);
        *app.state::<AppState>().replace_window.lock().unwrap() = window;
        *app.state::<AppState>().last_translated.lock().unwrap() = Some(text.clone());
        set_ball_state(app, BALL_BUSY);
        show_card_next_to_ball(app);
        translate_text(app, text);
    }

    #[cfg(not(target_os = "linux"))]
    {
        let pending = app
            .state::<AppState>()
            .pending_selection
            .lock()
            .unwrap()
            .take();

        hide_ball(app);

        let Some(pending) = pending else {
            return;
        };

        *app.state::<AppState>().replace_window.lock().unwrap() = pending.window;
        mark_suppressed(app, Duration::from_millis(1200));
        show_main_popup(app);
        translate_text(app, pending.text);
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn place_ball_window(app: &AppHandle, anchor: (i32, i32)) {
    let Some(ball) = app.get_webview_window("ball") else {
        return;
    };

    // CGEvent locations are global points, which is exactly what
    // `LogicalPosition` expects on macOS; Windows/Linux anchors are physical
    // screen pixels. Monitor geometry is physical, so the macOS path converts
    // the ball size and the work area to points before clamping.
    #[cfg(target_os = "macos")]
    {
        let scale = ball.scale_factor().unwrap_or(1.0);
        let Ok(size) = ball.outer_size().or_else(|_| ball.inner_size()) else {
            return;
        };
        let size = (
            (size.width as f64 / scale).round() as i32,
            (size.height as f64 / scale).round() as i32,
        );

        let (x, y) = match monitor_work_area_logical(app, anchor) {
            Some(work) => card_position(anchor, size, work, 12, 14),
            None => (anchor.0 + 14, anchor.1 + 14),
        };

        let _ = ball.set_position(tauri::LogicalPosition::new(x as f64, y as f64));
    }

    #[cfg(not(target_os = "macos"))]
    {
        let Ok(size) = ball.outer_size().or_else(|_| ball.inner_size()) else {
            return;
        };

        let Some(monitor) =
            monitor_at(app, anchor).or_else(|| ball.primary_monitor().ok().flatten())
        else {
            return;
        };

        let work = monitor.work_area();
        let scale = monitor.scale_factor();
        let margin = (12.0 * scale) as i32;
        let offset = (14.0 * scale) as i32;

        let (x, y) = card_position(
            anchor,
            (size.width as i32, size.height as i32),
            WorkArea {
                x: work.position.x,
                y: work.position.y,
                width: work.size.width,
                height: work.size.height,
            },
            margin,
            offset,
        );

        let _ = ball.set_position(PhysicalPosition::new(x, y));
    }
}

#[cfg(target_os = "windows")]
fn monitor_at(app: &AppHandle, point: (i32, i32)) -> Option<tauri::Monitor> {
    app.available_monitors().ok()?.into_iter().find(|monitor| {
        let position = monitor.position();
        let size = monitor.size();
        let x = point.0 >= position.x && point.0 < position.x + size.width as i32;
        let y = point.1 >= position.y && point.1 < position.y + size.height as i32;

        x && y
    })
}

/// Monitor work area in logical points for the macOS ball placement.
#[cfg(target_os = "macos")]
fn monitor_work_area_logical(app: &AppHandle, point: (i32, i32)) -> Option<WorkArea> {
    app.available_monitors().ok()?.into_iter().find_map(|monitor| {
        let position = monitor.position();
        let size = monitor.size();
        let scale = monitor.scale_factor();

        let x = (position.x as f64 / scale).round() as i32;
        let y = (position.y as f64 / scale).round() as i32;
        let width = (size.width as f64 / scale).round() as i32;
        let height = (size.height as f64 / scale).round() as i32;

        if point.0 < x || point.0 >= x + width || point.1 < y || point.1 >= y + height {
            return None;
        }

        let work = monitor.work_area();

        Some(WorkArea {
            x: (work.position.x as f64 / scale).round() as i32,
            y: (work.position.y as f64 / scale).round() as i32,
            width: (work.size.width as f64 / scale).round() as u32,
            height: (work.size.height as f64 / scale).round() as u32,
        })
    })
}

fn translate_text(app: &AppHandle, text: String) {
    let state = app.state::<AppState>();
    *state.last_text.lock().unwrap() = Some(text.clone());

    // The engine processes requests serially. A burst of watcher triggers must
    // not queue translations whose streams keep rewriting the card after it
    // has moved on; remember only the newest follow-up text instead and let
    // the running translation chain into it.
    if state.translating.swap(true, Ordering::SeqCst) {
        *state.queued.lock().unwrap() = Some(text);
        return;
    }

    start_translation(app, text);
}

fn start_translation(app: &AppHandle, text: String) {
    let engine = app.state::<AppState>().engine.lock().unwrap().clone();
    let Some(engine) = engine else {
        *app.state::<AppState>().pending.lock().unwrap() = Some(text);
        app.state::<AppState>().translating.store(false, Ordering::SeqCst);
        return;
    };

    let (effective_source, detected, target, configured_source) = {
        let state = app.state::<Mutex<TranslateConfig>>();
        let mut config = state.lock().unwrap();
        let (source, detected) = translator_core::detect::resolve_source(&config.source, &text);
        config.detected = detected.map(str::to_string);

        (
            source,
            config.detected.clone(),
            config.target.clone(),
            config.source.clone(),
        )
    };

    if crate::selection_debug() {
        eprintln!("SELDBG translate start len={}", text.chars().count());
    }

    let replaceable = app
        .state::<AppState>()
        .replace_window
        .lock()
        .unwrap()
        .is_some();
    let app = app.clone();
    let _ = app.emit(
        "source",
        SourcePayload {
            text: text.clone(),
            replaceable,
            detected: detected.clone(),
        },
    );
    let _ = app.emit("language-state", language_state(&app));

    let history_source = detected.unwrap_or(configured_source);

    std::thread::spawn(move || {
        let request = TranslationRequest {
            text: text.clone(),
            source: effective_source,
            target,
        };

        let delta_app = app.clone();
        let result = engine.translate_blocking_streaming(&request, move |piece| {
            let _ = delta_app.emit("delta", piece.to_string());
        });
        let ok = result.is_ok();

        match result {
            Ok(result) => {
                *app.state::<AppState>().last_translation.lock().unwrap() =
                    Some(result.translated_text.clone());

                {
                    let state = app.state::<AppState>();
                    let mut history = state.history.lock().unwrap();

                    translator_core::history::push(
                        &mut history,
                        HistoryEntry {
                            source: history_source,
                            target: request.target.clone(),
                            text,
                            translation: result.translated_text.clone(),
                            at: Some(
                                SystemTime::now()
                                    .duration_since(UNIX_EPOCH)
                                    .map(|elapsed| elapsed.as_millis() as u64)
                                    .unwrap_or(0),
                            ),
                        },
                    );
                    translator_core::history::save(&history);
                }

                let _ = app.emit("done", result.translated_text);
                let _ = app.emit("history-changed", ());
            }
            Err(error) => {
                let _ = app.emit(
                    "error",
                    ErrorPayload {
                        message: error.to_string(),
                    },
                );
            }
        }

        if crate::selection_debug() {
            eprintln!("SELDBG translate done ok={ok}");
        }

        // Chain into the newest follow-up that arrived while this translation
        // was running (watcher bursts coalesce into at most one extra pass).
        let state = app.state::<AppState>();
        let next = state.queued.lock().unwrap().take();
        state.translating.store(false, Ordering::SeqCst);
        drop(state);

        if let Some(next) = next {
            translate_text(&app, next);
        } else {
            refresh_ball_state(&app);
        }
    });
}

fn show_main(app: &AppHandle) {
    show_main_with(app, true, true);
}

/// Tray/CLI entry points center the window instead of following the cursor,
/// which would otherwise leave it stuck against the taskbar where the tray
/// menu is anchored.
fn show_main_in_place(app: &AppHandle) {
    show_main_with(app, false, true);
}

/// Selection-watcher card: it must not steal focus (the source application
/// must keep the selection and the drag) and it must not jump to a new spot on
/// every refresh while it is already visible — only the content updates then.
/// It does pulse above other windows, so the card is not left behind the app
/// the user just selected text in.
#[cfg(not(target_os = "linux"))]
fn show_main_popup(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        app.state::<AppState>()
            .popup
            .store(true, Ordering::SeqCst);

        let was_visible = window.is_visible().unwrap_or(false);

        if !was_visible {
            // Never let a watcher popup take focus, not even on map: the
            // source application must keep the selection and the drag. The
            // card becomes focusable again when the user clicks it.
            let _ = window.set_focusable(false);
            let _ = window.show();
            place_popup_near_cursor(app, &window);
        } else {
            let _ = window.show();
        }

        pulse_above(app, &window);
    }
}

/// Bottom-right corner of the current monitor's work area (fallback when the
/// docked ball is not available).
#[cfg(target_os = "linux")]
fn place_popup_corner(window: &WebviewWindow) {
    let Ok(size) = window.outer_size().or_else(|_| window.inner_size()) else {
        return;
    };

    let Some(monitor) = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten())
    else {
        return;
    };

    let work = monitor.work_area();
    let margin = (16.0 * monitor.scale_factor()) as i32;
    let x = work.position.x + work.size.width as i32 - size.width as i32 - margin;
    let y = work.position.y + work.size.height as i32 - size.height as i32 - margin;

    let _ = window.set_position(PhysicalPosition::new(
        x.max(work.position.x + margin),
        y.max(work.position.y + margin),
    ));
}

/// Popup placement that stays out of the user's way: prefer above the cursor
/// (selections usually drag downward/right), fall back below with a wider gap,
/// so starting the next selection in the same area does not hit the card.
#[cfg(not(target_os = "linux"))]
fn place_popup_near_cursor(app: &AppHandle, window: &WebviewWindow) {
    let Ok(cursor) = app.cursor_position() else {
        return;
    };

    let Ok(size) = window.outer_size().or_else(|_| window.inner_size()) else {
        return;
    };

    let Some(monitor) = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten())
    else {
        return;
    };

    let work = monitor.work_area();
    let scale = monitor.scale_factor();
    let margin = (12.0 * scale) as i32;
    let gap = (18.0 * scale) as i32;

    let min_x = work.position.x + margin;
    let min_y = work.position.y + margin;
    let max_x = work.position.x + work.size.width as i32 - size.width as i32 - margin;
    let max_y = work.position.y + work.size.height as i32 - size.height as i32 - margin;

    let mut x = cursor.x as i32 + margin;
    let mut y = cursor.y as i32 - size.height as i32 - gap;

    if y < min_y {
        y = cursor.y as i32 + gap;
    }

    x = x.clamp(min_x, max_x.max(min_x));
    y = y.clamp(min_y, max_y.max(min_y));

    let _ = window.set_position(PhysicalPosition::new(x, y));
}

fn show_main_with(app: &AppHandle, follow_cursor: bool, activate: bool) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();

        if follow_cursor {
            place_near_cursor(app, &window);
        } else {
            center_window_position(&window);
        }

        if !activate {
            return;
        }

        app.state::<AppState>()
            .popup
            .store(false, Ordering::SeqCst);
        let _ = window.set_focusable(true);
        let _ = window.set_focus();
        pulse_above(app, &window);
    }
}

/// GNOME denies focus and raise to a background app whose tray click carries no
/// activation token, so the card can stay behind the active window and look
/// unresponsive — including when it was hidden and the new map lands under a
/// fullscreen window. Briefly lift it with always-on-top (without touching
/// keyboard focus), then restore the pin state.
fn pulse_above(app: &AppHandle, window: &WebviewWindow) {
    let state = app.state::<AppState>();

    if *state.pinned.lock().unwrap()
        || std::env::var_os("TRANSLATOR_SELECTION_NO_PULSE").is_some()
    {
        return;
    }

    if crate::selection_debug() {
        eprintln!("SELDBG pulse always-on-top");
    }

    let generation = state.pulse_generation.fetch_add(1, Ordering::SeqCst) + 1;
    let _ = window.set_always_on_top(true);

    let handle = window.clone();
    let app_handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(700));

        let state = app_handle.state::<AppState>();
        if state.pulse_generation.load(Ordering::SeqCst) == generation
            && !*state.pinned.lock().unwrap()
        {
            let _ = handle.set_always_on_top(false);
        }
    });
}

fn center_window_position(window: &WebviewWindow) {
    let Ok(size) = window.outer_size().or_else(|_| window.inner_size()) else {
        return;
    };

    let Some(monitor) = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten())
    else {
        return;
    };

    let work = monitor.work_area();
    let margin = (12.0 * monitor.scale_factor()) as i32;
    let work_width = work.size.width as i32;
    let work_height = work.size.height as i32;

    let x = work.position.x + ((work_width - size.width as i32) / 2).max(margin);
    let y = work.position.y + ((work_height - size.height as i32) / 2).max(margin);

    let _ = window.set_position(PhysicalPosition::new(x, y));
}

fn window_hidden(app: &AppHandle) -> bool {
    app.get_webview_window("main")
        .and_then(|window| window.is_visible().ok())
        .map(|visible| !visible)
        .unwrap_or(true)
}

fn place_near_cursor(app: &AppHandle, window: &WebviewWindow) {
    let Ok(cursor) = app.cursor_position() else {
        return;
    };

    let Ok(size) = window.outer_size().or_else(|_| window.inner_size()) else {
        return;
    };

    let Some(monitor) = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten())
    else {
        return;
    };

    let work = monitor.work_area();
    let scale = monitor.scale_factor();
    let margin = (12.0 * scale) as i32;
    let offset = (18.0 * scale) as i32;

    let (x, y) = card_position(
        (cursor.x as i32, cursor.y as i32),
        (size.width as i32, size.height as i32),
        WorkArea {
            x: work.position.x,
            y: work.position.y,
            width: work.size.width,
            height: work.size.height,
        },
        margin,
        offset,
    );

    let _ = window.set_position(PhysicalPosition::new(x, y));
}

#[derive(Clone, Copy)]
struct WorkArea {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

/// Places the card beside the cursor and keeps it inside the work area,
/// sliding it above the cursor when the bottom edge would overflow.
fn card_position(
    cursor: (i32, i32),
    size: (i32, i32),
    work: WorkArea,
    margin: i32,
    offset: i32,
) -> (i32, i32) {
    let work_right = work.x + work.width as i32;
    let work_bottom = work.y + work.height as i32;

    let x = cursor.0 + margin;
    let mut y = cursor.1 + offset;

    if y + size.1 + margin > work_bottom {
        y = cursor.1 - size.1 - margin;
    }

    let min_x = work.x + margin;
    let min_y = work.y + margin;
    let max_x = work_right - size.0 - margin;
    let max_y = work_bottom - size.1 - margin;

    (
        x.clamp(min_x, max_x.max(min_x)),
        y.clamp(min_y, max_y.max(min_y)),
    )
}

/// The floating ball is a hidden always-on-top window without decorations,
/// taskbar entry or focus; it is only mapped while a selection is pending.
fn build_ball_window(app: &AppHandle) -> tauri::Result<()> {
    // Diagnostic switch: run the watcher without the ball window at all.
    if std::env::var_os("TRANSLATOR_SELECTION_NO_BALL").is_some() {
        return Ok(());
    }

    // Linux: no `resizable(false)` at build time — GTK applies it before the
    // size request is realized, which locks an undecorated window at its
    // 200x200 default and turns the ball into a 200x200 click-blocking window.
    let builder = WebviewWindowBuilder::new(app, "ball", WebviewUrl::App("ball.html".into()))
        .title("OpenTranslator")
        .inner_size(44.0, 44.0)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .shadow(false)
        .visible(false);

    // Windows/macOS: a decorated-style window carries the OS minimum tracking
    // size (136 px wide at 100%), which inflates the ball into a wide pill.
    // Pinning min and max inner size makes the size constraint override it;
    // Linux sets the constraints right before the first show instead, because
    // GTK applies build-time constraints before the size request is realized.
    #[cfg(not(target_os = "linux"))]
    let builder = builder
        .resizable(false)
        .min_inner_size(44.0, 44.0)
        .max_inner_size(44.0, 44.0);

    let ball = builder.build()?;

    // The first `WM_GETMINMAXINFO` of a window arrives during `CreateWindowEx`,
    // before tao attaches its window proc, so Windows clamps the requested
    // 44 px width to the caption minimum (136 px at 100%) and tao does not
    // re-apply `inner_size` afterwards. Resize once the size constraints are
    // active, so the ball is a square.
    #[cfg(not(target_os = "linux"))]
    {
        let _ = ball.set_size(tauri::LogicalSize::new(44.0, 44.0));
    }

    // The ball is dragged and hovered, never typed into: keeping it
    // non-focusable means interacting with it cannot steal the selection from
    // the source application.
    let _ = ball.set_focusable(false);

    if selection_debug() {
        eprintln!(
            "SELDBG ball built size={:?}",
            ball.outer_size().map(|size| (size.width, size.height))
        );
    }

    Ok(())
}

fn build_tray(app: &AppHandle, hotkey_spec: &str) -> tauri::Result<()> {
    let show_item = MenuItem::with_id(app, "show", "显示窗口", true, None::<&str>)?;
    let history_item = MenuItem::with_id(app, "history", "历史…", true, None::<&str>)?;
    let settings_item = MenuItem::with_id(app, "settings", "设置…", true, None::<&str>)?;
    let update_item = MenuItem::with_id(app, "update", "正在检查更新…", false, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;

    // Quick mode switch without opening the card; the settings switch stays in
    // sync through the "selection-mode" event.
    let mode = *app.state::<AppState>().selection.lock().unwrap();
    let selection_off = CheckMenuItem::with_id(
        app,
        "selection-off",
        "不自动翻译",
        true,
        mode.mode == selection_watch::SelectionMode::Off,
        None::<&str>,
    )?;
    let selection_ball = CheckMenuItem::with_id(
        app,
        "selection-ball",
        "悬浮球翻译",
        true,
        mode.mode == selection_watch::SelectionMode::Ball,
        None::<&str>,
    )?;
    let selection_auto = CheckMenuItem::with_id(
        app,
        "selection-auto",
        "立即翻译",
        selection_watch::auto_supported(),
        mode.mode == selection_watch::SelectionMode::Auto,
        None::<&str>,
    )?;
    // 立即翻译 only exists where the watcher sees a mouse release.
    let mut selection_items: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> =
        vec![&selection_off, &selection_ball];

    if selection_watch::auto_supported() {
        selection_items.push(&selection_auto);
    }

    let selection_menu = Submenu::with_items(app, "划词翻译", true, &selection_items)?;

    let menu = Menu::with_items(
        app,
        &[
            &show_item,
            &PredefinedMenuItem::separator(app)?,
            &history_item,
            &settings_item,
            &selection_menu,
            &update_item,
            &PredefinedMenuItem::separator(app)?,
            &quit_item,
        ],
    )?;

    *app.state::<UpdateSlot>().item.lock().unwrap() = Some(update_item);
    *app.state::<AppState>().selection_menu.lock().unwrap() = Some(SelectionMenu {
        off: selection_off,
        ball: selection_ball,
        auto: selection_auto,
    });

    let tray = TrayIconBuilder::with_id("main")
        .icon(Image::new_owned(make_icon_rgba(), 32, 32))
        .tooltip(format!("OpenTranslator（{hotkey_spec}）"))
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main_in_place(app),
            "history" => {
                show_main_in_place(app);
                let _ = app.emit("open-history", ());
            }
            "settings" => {
                show_main_in_place(app);
                let _ = app.emit("open-settings", ());
            }
            "update" => {
                let info = app.state::<UpdateSlot>().info.lock().unwrap().clone();

                if let Some(info) = info {
                    if info.can_install {
                        start_update_install(app);
                    } else {
                        open_url(&info.url);
                    }
                }
            }
            "selection-off" => {
                set_selection_mode_from_menu(app, selection_watch::SelectionMode::Off)
            }
            "selection-ball" => {
                set_selection_mode_from_menu(app, selection_watch::SelectionMode::Ball)
            }
            "selection-auto" => {
                set_selection_mode_from_menu(app, selection_watch::SelectionMode::Auto)
            }
            "quit" => app.exit(0),
            _ => {}
        });

    #[cfg(target_os = "macos")]
    let tray = tray.icon_as_template(true);

    tray.build(app)?;
    Ok(())
}

/// Tray entry point for the mode submenu: applies the mode, moves the
/// checkmark and tells an open card so its switch stays in sync.
fn set_selection_mode_from_menu(app: &AppHandle, mode: selection_watch::SelectionMode) {
    apply_selection_mode(app, mode);
    let _ = app.emit("selection-mode", mode.as_str());
}

fn set_tooltip(app: &AppHandle, text: &str) {
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some(text));
    }
}

fn tooltip_ready(app: &AppHandle) -> String {
    let hotkey = app
        .state::<Mutex<TranslateConfig>>()
        .lock()
        .unwrap()
        .hotkey_label
        .clone();
    format!("OpenTranslator（{hotkey}）")
}

#[tauri::command]
fn hide_window(app: AppHandle, window: WebviewWindow) {
    if *app.state::<AppState>().pinned.lock().unwrap() {
        return;
    }

    app.state::<AppState>()
        .popup
        .store(false, Ordering::SeqCst);
    let _ = window.hide();
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

/// The user clicked the watcher popup: make it a normal focusable card so
/// keyboard shortcuts work again.
#[tauri::command]
fn card_engaged(app: AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_focusable(true);
        let _ = window.set_focus();
    }
}

/// Called by `ball.js` when the mouse dwells on the ball.
#[tauri::command]
fn ball_hover(app: AppHandle) {
    hover_ball(&app);
}

/// Called by `ball.js` on a direct click (the dwell timer may not have fired).
#[tauri::command]
fn ball_click(app: AppHandle) {
    hover_ball(&app);
}

#[tauri::command]
fn save_selection_mode(app: AppHandle, mode: String) -> Result<(), String> {
    let parsed = match mode.trim() {
        "off" => selection_watch::SelectionMode::Off,
        "ball" => selection_watch::SelectionMode::Ball,
        "auto" => selection_watch::SelectionMode::Auto,
        other => return Err(format!("unknown selection mode: {other}")),
    };

    apply_selection_mode(&app, parsed);
    Ok(())
}

/// Applies a selection mode from any entry point (settings switch, tray menu):
/// updates the runtime state, persists it and adjusts the watcher/ball.
fn apply_selection_mode(app: &AppHandle, parsed: selection_watch::SelectionMode) {
    let state = app.state::<AppState>();
    state.selection.lock().unwrap().mode = parsed;
    translator_core::settings::persist_value("selection_mode", parsed.as_str());

    // Remember the method for the next switch-on and keep the tray checkmarks
    // in sync, whichever entry point changed the mode.
    if parsed != selection_watch::SelectionMode::Off {
        translator_core::settings::persist_value("selection_method", parsed.as_str());
    }

    if let Some(menu) = state.selection_menu.lock().unwrap().as_ref() {
        let _ = menu
            .off
            .set_checked(parsed == selection_watch::SelectionMode::Off);
        let _ = menu
            .ball
            .set_checked(parsed == selection_watch::SelectionMode::Ball);
        let _ = menu
            .auto
            .set_checked(parsed == selection_watch::SelectionMode::Auto);
    }

    if parsed == selection_watch::SelectionMode::Off {
        hide_ball(app);
        *state.queued.lock().unwrap() = None;

        // A card that the watcher popped up should not linger once the feature
        // is disabled.
        if state.popup.swap(false, Ordering::SeqCst) && !*state.pinned.lock().unwrap() {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.hide();
            }
        }
    }

    selection_watch::ensure(app.clone());
    apply_ball_visibility(app);
}

/// Remembers the preferred mode for the next time the master switch is turned
/// on (`ball` or `auto`).
#[tauri::command]
fn save_selection_method(value: String) -> Result<(), String> {
    let method = match value.trim() {
        "ball" | "auto" => value.trim().to_string(),
        other => return Err(format!("unknown selection method: {other}")),
    };

    translator_core::settings::persist_value("selection_method", &method);
    Ok(())
}

/// Live drag of the docked ball (deltas in logical pixels from `ball.js`).
#[tauri::command]
fn move_ball_by(app: AppHandle, dx: f64, dy: f64) {
    let Some(ball) = app.get_webview_window("ball") else {
        return;
    };

    let Ok(position) = ball.outer_position() else {
        return;
    };

    let scale = ball.scale_factor().unwrap_or(1.0);
    let mut x = position.x + (dx * scale).round() as i32;
    let mut y = position.y + (dy * scale).round() as i32;

    if let Some(monitor) = ball
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| ball.primary_monitor().ok().flatten())
    {
        let work = monitor.work_area();
        let Ok(size) = ball.outer_size().or_else(|_| ball.inner_size()) else {
            return;
        };
        let margin = (4.0 * monitor.scale_factor()) as i32;
        let max_x = work.position.x + work.size.width as i32 - size.width as i32 - margin;
        let max_y = work.position.y + work.size.height as i32 - size.height as i32 - margin;

        x = x.clamp(work.position.x + margin, max_x.max(work.position.x + margin));
        y = y.clamp(work.position.y + margin, max_y.max(work.position.y + margin));
    }

    let _ = ball.set_position(PhysicalPosition::new(x, y));
}

/// Persists the docked ball position after a drag. Dropping the ball near its
/// default dock resets the custom position instead of saving it, so no
/// "restore default" control is needed in the settings.
#[tauri::command]
fn save_ball_position(app: AppHandle) {
    let Some(ball) = app.get_webview_window("ball") else {
        return;
    };

    let Ok(position) = ball.outer_position() else {
        return;
    };

    #[cfg(target_os = "linux")]
    {
        let scale = ball.scale_factor().unwrap_or(1.0);
        let snap = (48.0 * scale).round() as i32;

        if let Some((default_x, default_y)) = default_ball_position(&ball) {
            if (position.x - default_x).abs() <= snap && (position.y - default_y).abs() <= snap {
                translator_core::settings::persist_remove("ball_x");
                translator_core::settings::persist_remove("ball_y");
                let _ = ball.set_position(PhysicalPosition::new(default_x, default_y));
                apply_ball_visibility(&app);
                return;
            }
        }
    }

    translator_core::settings::persist_value("ball_x", &position.x.to_string());
    translator_core::settings::persist_value("ball_y", &position.y.to_string());
    apply_ball_visibility(&app);
}

#[tauri::command]
fn resize_window(window: WebviewWindow, height: f64) {
    let Ok(size) = window.inner_size() else {
        return;
    };

    let scale = window.scale_factor().unwrap_or(1.0);

    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten());

    let (max_height, margin) = match &monitor {
        Some(monitor) => {
            let work = monitor.work_area();
            let margin = (12.0 * monitor.scale_factor()) as i32;
            let max = (work.size.height as i32 - 2 * margin).max(120) as u32;
            (max, margin)
        }
        None => (2000, 12),
    };

    let height = ((height * scale).ceil() as u32).clamp(120, max_height);
    let _ = window.set_size(tauri::PhysicalSize::new(size.width, height));

    // Keep the growing card inside the work area: when it would run past the
    // bottom edge, slide it up so the translation stays visible while it
    // streams in.
    if let (Ok(position), Some(monitor)) = (window.outer_position(), monitor) {
        let work = monitor.work_area();
        let work_top = work.position.y + margin;
        let work_bottom = work.position.y + work.size.height as i32 - margin;

        if position.y + height as i32 > work_bottom {
            let y = (work_bottom - height as i32).max(work_top);
            let _ = window.set_position(PhysicalPosition::new(position.x, y));
        }
    }
}

#[tauri::command]
fn center_window(window: WebviewWindow) {
    center_window_position(&window);
}

/// One clipboard instance for the whole app: on X11 the owner window is
/// destroyed with the last `Clipboard`, which would drop the copied text
/// before the user can paste it.
#[cfg(target_os = "linux")]
fn with_clipboard<R>(
    app: &AppHandle,
    action: impl FnOnce(&mut arboard::Clipboard) -> R,
) -> Option<R> {
    let state = app.state::<AppState>();
    let mut slot = state.clipboard.lock().unwrap();

    if slot.is_none() {
        *slot = arboard::Clipboard::new().ok();
    }

    slot.as_mut().map(action)
}

#[cfg(not(target_os = "linux"))]
fn with_clipboard<R>(
    _app: &AppHandle,
    action: impl FnOnce(&mut arboard::Clipboard) -> R,
) -> Option<R> {
    arboard::Clipboard::new()
        .ok()
        .map(|mut clipboard| action(&mut clipboard))
}

#[tauri::command]
fn copy_text(app: AppHandle, text: String) {
    with_clipboard(&app, |clipboard| {
        let _ = clipboard.set_text(text);
    });
}

/// The Linux client cannot use the WebView's speech synthesis (WebKitGTK does
/// not expose it), so it speaks through speech-dispatcher when available.
#[cfg(target_os = "linux")]
fn command_in_path(name: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(name).is_file()))
        .unwrap_or(false)
}

#[tauri::command]
fn tts_backend() -> &'static str {
    #[cfg(target_os = "linux")]
    {
        if command_in_path("spd-say") {
            "spd-say"
        } else {
            "none"
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        "webview"
    }
}

#[cfg(target_os = "linux")]
fn spawn_reaped(command: &mut std::process::Command) {
    if let Ok(mut child) = command.spawn() {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}

/// Map a UI language tag to the speech-dispatcher language code (`zh` speaks
/// through the Mandarin `cmn` voice); `auto`/empty means "use the default".
#[cfg(target_os = "linux")]
fn spd_language(tag: &str) -> Option<String> {
    let tag = tag.trim().to_ascii_lowercase();

    if tag.is_empty() || tag == "auto" {
        return None;
    }

    if tag.starts_with("zh") {
        return Some("cmn".to_string());
    }

    if tag.starts_with("yue") {
        return Some("yue".to_string());
    }

    Some(tag.split(['-', '_']).next().unwrap_or(&tag).to_string())
}

/// `spd-say -L` prints a fixed-width table whose long names overflow into the
/// language column; language codes are the lowercase 2-3 letter tokens.
#[cfg(target_os = "linux")]
fn parse_voice_languages(output: &str) -> std::collections::HashSet<String> {
    let mut languages = std::collections::HashSet::new();

    for line in output.lines().skip(1) {
        for token in line.split_whitespace() {
            let base = token.split('-').next().unwrap_or(token);

            if (2..=3).contains(&base.len())
                && base.bytes().all(|byte| byte.is_ascii_lowercase())
            {
                languages.insert(base.to_string());
            }
        }
    }

    languages
}

#[cfg(target_os = "linux")]
fn spd_voice_languages() -> &'static std::collections::HashSet<String> {
    static LANGUAGES: std::sync::OnceLock<std::collections::HashSet<String>> =
        std::sync::OnceLock::new();

    LANGUAGES.get_or_init(|| {
        std::process::Command::new("spd-say")
            .arg("-L")
            .output()
            .map(|output| parse_voice_languages(&String::from_utf8_lossy(&output.stdout)))
            .unwrap_or_default()
    })
}

/// The utterance command: `--wait` keeps the child alive until the message is
/// spoken or stopped (without it `spd-say` exits right after queueing, so the
/// UI could never show or cancel a running utterance); `--pipe-mode` keeps the
/// text out of argv so a translation starting with `-` is not parsed as an
/// option; `important` interrupts the previous message.
#[cfg(target_os = "linux")]
fn spd_speak_command(lang: &str) -> std::process::Command {
    let mut command = std::process::Command::new("spd-say");
    command.args(["--priority", "important", "--wait", "--pipe-mode"]);

    if let Some(code) = spd_language(lang) {
        command.arg("-l").arg(code);
    }

    command
}

#[tauri::command]
fn tts_voice_available(lang: String) -> bool {
    #[cfg(target_os = "linux")]
    {
        let Some(code) = spd_language(&lang) else {
            return true;
        };
        let languages = spd_voice_languages();

        // An empty set means `spd-say -L` failed; do not block speech then.
        languages.is_empty() || languages.contains(&code)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = lang;
        true
    }
}

#[tauri::command]
fn speak_text(app: AppHandle, text: String, lang: String, generation: u64) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        if text.trim().is_empty() {
            let _ = app.emit("speech-ended", SpeechEndedPayload { generation });
            return Ok(());
        }

        let mut child = spd_speak_command(&lang)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|error| format!("启动朗读失败：{error}"))?;

        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            let _ = stdin.write_all(text.as_bytes());
        }

        std::thread::spawn(move || {
            let _ = child.wait();
            let _ = app.emit("speech-ended", SpeechEndedPayload { generation });
        });

        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (app, text, lang, generation);
        Ok(())
    }
}

#[tauri::command]
fn stop_speaking() {
    #[cfg(target_os = "linux")]
    {
        // `--stop` stops the currently spoken message, `--cancel` clears
        // anything still queued for the connection.
        spawn_reaped(std::process::Command::new("spd-say").arg("--stop"));
        spawn_reaped(std::process::Command::new("spd-say").arg("--cancel"));
    }
}

#[tauri::command]
fn retranslate(app: AppHandle) {
    let text = app.state::<AppState>().last_text.lock().unwrap().clone();

    if let Some(text) = text {
        translate_text(&app, text);
    }
}

#[tauri::command]
fn get_settings(app: AppHandle) -> SettingsPayload {
    let config = translator_core::settings::load_config();
    let hotkey = app
        .state::<Mutex<TranslateConfig>>()
        .lock()
        .unwrap()
        .hotkey_label
        .clone();
    let selection = *app.state::<AppState>().selection.lock().unwrap();

    SettingsPayload {
        hotkey,
        model_path: config.model_path.clone().unwrap_or_default(),
        default_model_path: translator_core::paths::default_model_path()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_default(),
        auto_download: config.auto_download.as_deref() != Some("false"),
        check_updates: config.check_updates.as_deref() != Some("false"),
        serve_extension: config.serve_extension.as_deref() != Some("false"),
        pinned: *app.state::<AppState>().pinned.lock().unwrap(),
        config_path: translator_core::paths::config_path()
            .map(|path| path.to_string_lossy().to_string()),
        app_version: translator_core::update::current_version().to_string(),
        selection_mode: selection.mode.as_str().to_string(),
        selection_method: config
            .selection_method
            .as_deref()
            .map(str::trim)
            .filter(|value| *value == "auto")
            .unwrap_or("ball")
            .to_string(),
        selection_delay_ms: selection.delay_ms(),
        selection_min_length: selection.min_length,
        selection_supported: selection_watch::selection_supported(),
        selection_ball_supported: selection_watch::ball_supported(),
        selection_auto_supported: selection_watch::auto_supported(),
        model_exists: resolve_model_path(&config)
            .map(|path| path.is_file())
            .unwrap_or(false),
        extension_addr: app.state::<AppState>().extension_addr.clone(),
        extension_running: app.state::<AppState>().extension_running,
    }
}

#[tauri::command]
fn save_hotkey(app: AppHandle, spec: String) -> Result<(), String> {
    let spec = spec.trim().to_string();
    let previous = app
        .state::<Mutex<TranslateConfig>>()
        .lock()
        .unwrap()
        .hotkey_label
        .clone();

    let _ = app.global_shortcut().unregister_all();

    match app.global_shortcut().register(spec.as_str()) {
        Ok(()) => {
            app.state::<Mutex<TranslateConfig>>()
                .lock()
                .unwrap()
                .hotkey_label = spec.clone();
            translator_core::settings::persist_value("hotkey", &spec);
            set_tooltip(&app, &tooltip_ready(&app));
            Ok(())
        }
        Err(error) => {
            let _ = app.global_shortcut().register(previous.as_str());
            Err(format!("{error}（已保留 {previous}）"))
        }
    }
}

#[tauri::command]
fn save_model_path(value: String) {
    translator_core::settings::persist_value("model_path", value.trim());
}

#[tauri::command]
fn save_switch(key: String, value: bool) -> Result<(), String> {
    let key = match key.as_str() {
        "auto_download" | "check_updates" | "serve_extension" => key,
        other => return Err(format!("unknown setting: {other}")),
    };

    translator_core::settings::persist_value(&key, if value { "true" } else { "false" });
    Ok(())
}

/// Login-startup registration managed by the client itself, so the settings
/// switch works without re-running the installers.
///
/// NOTE: the three entry artifacts must stay in sync with the installers that
/// write the same paths and `--autostart` flag:
/// `packaging/linux/open-translator-setup`, `desktop/install-macos.sh`,
/// `desktop/install-windows.ps1` and `packaging/windows/install.ps1`.
mod autostart {
    use std::path::PathBuf;

    /// Quotes a path for the Desktop Entry `Exec=` field: wrap in double
    /// quotes and escape the four characters the spec reserves. A path with a
    /// line break cannot be represented and is rejected instead.
    #[cfg(any(target_os = "linux", test))]
    fn desktop_exec_quote(path: &str) -> Result<String, String> {
        if path.contains(['\n', '\r']) {
            return Err("the executable path contains a line break".to_string());
        }

        let mut quoted = String::with_capacity(path.len() + 2);
        quoted.push('"');

        for character in path.chars() {
            if matches!(character, '"' | '`' | '$' | '\\') {
                quoted.push('\\');
            }

            quoted.push(character);
        }

        quoted.push('"');
        Ok(quoted)
    }

    /// Escapes text for the XML plist written on macOS.
    #[cfg(any(target_os = "macos", test))]
    fn xml_escape(value: &str) -> String {
        let mut escaped = String::with_capacity(value.len());

        for character in value.chars() {
            match character {
                '&' => escaped.push_str("&amp;"),
                '<' => escaped.push_str("&lt;"),
                '>' => escaped.push_str("&gt;"),
                '"' => escaped.push_str("&quot;"),
                '\'' => escaped.push_str("&apos;"),
                _ => escaped.push(character),
            }
        }

        escaped
    }

    /// Escapes a value for a single-quoted PowerShell string.
    #[cfg(any(target_os = "windows", test))]
    fn powershell_quote(value: &str) -> String {
        value.replace('\'', "''")
    }

    #[cfg(target_os = "linux")]
    fn entry_path() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;

        Some(base.join("autostart").join("open-translator.desktop"))
    }

    #[cfg(target_os = "linux")]
    pub fn is_enabled() -> bool {
        entry_path().map(|path| path.exists()).unwrap_or(false)
    }

    #[cfg(target_os = "linux")]
    pub fn set(enabled: bool) -> Result<(), String> {
        let path = entry_path().ok_or("cannot determine the config directory")?;

        if !enabled {
            return match std::fs::remove_file(&path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error.to_string()),
            };
        }

        let exe = std::env::current_exe().map_err(|error| error.to_string())?;

        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
        }

        let exec = desktop_exec_quote(&exe.to_string_lossy())?;
        let contents = format!(
            "[Desktop Entry]\nType=Application\nName=OpenTranslator\nComment=Local-first AI selection translation\nExec={exec} --autostart\nX-GNOME-Autostart-enabled=true\n"
        );

        std::fs::write(&path, contents).map_err(|error| error.to_string())
    }

    #[cfg(target_os = "macos")]
    const LABEL: &str = "io.github.opentranslator.popup";

    #[cfg(target_os = "macos")]
    fn entry_path() -> Option<PathBuf> {
        let home = std::env::var_os("HOME")?;
        Some(
            PathBuf::from(home)
                .join("Library/LaunchAgents")
                .join(format!("{LABEL}.plist")),
        )
    }

    #[cfg(target_os = "macos")]
    pub fn is_enabled() -> bool {
        entry_path().map(|path| path.exists()).unwrap_or(false)
    }

    #[cfg(target_os = "macos")]
    pub fn set(enabled: bool) -> Result<(), String> {
        let path = entry_path().ok_or("cannot determine the LaunchAgents directory")?;
        let uid = unsafe { libc::getuid() };
        let domain = format!("gui/{uid}");
        let plist = path.to_string_lossy().to_string();

        if !enabled {
            let _ = std::process::Command::new("launchctl")
                .args(["bootout", &domain, &plist])
                .status();

            return match std::fs::remove_file(&path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error.to_string()),
            };
        }

        let exe = std::env::current_exe().map_err(|error| error.to_string())?;

        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
        }

        let contents = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n<dict>\n    <key>Label</key>\n    <string>{LABEL}</string>\n    <key>ProgramArguments</key>\n    <array>\n        <string>{}</string>\n        <string>--autostart</string>\n    </array>\n    <key>RunAtLoad</key>\n    <true/>\n</dict>\n</plist>\n",
            xml_escape(&exe.to_string_lossy())
        );

        std::fs::write(&path, contents).map_err(|error| error.to_string())?;
        let _ = std::process::Command::new("launchctl")
            .args(["bootstrap", &domain, &plist])
            .status();
        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn entry_path() -> Option<PathBuf> {
        let appdata = std::env::var_os("APPDATA")?;
        Some(PathBuf::from(appdata).join(
            "Microsoft\\Windows\\Start Menu\\Programs\\Startup\\OpenTranslator.lnk",
        ))
    }

    #[cfg(target_os = "windows")]
    pub fn is_enabled() -> bool {
        entry_path().map(|path| path.exists()).unwrap_or(false)
    }

    #[cfg(target_os = "windows")]
    pub fn set(enabled: bool) -> Result<(), String> {
        let path = entry_path().ok_or("cannot determine the Startup folder")?;

        if !enabled {
            return match std::fs::remove_file(&path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error.to_string()),
            };
        }

        let exe = std::env::current_exe().map_err(|error| error.to_string())?;
        let script = format!(
            "$s=(New-Object -ComObject WScript.Shell).CreateShortcut('{}');$s.TargetPath='{}';$s.Arguments='--autostart';$s.Save()",
            powershell_quote(&path.to_string_lossy()),
            powershell_quote(&exe.to_string_lossy())
        );
        let status = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .status()
            .map_err(|error| error.to_string())?;

        if status.success() {
            Ok(())
        } else {
            Err("failed to create the startup shortcut".to_string())
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    pub fn is_enabled() -> bool {
        false
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    pub fn set(_enabled: bool) -> Result<(), String> {
        Err("unsupported platform".to_string())
    }

    #[cfg(test)]
    mod tests {
        use super::{desktop_exec_quote, powershell_quote, xml_escape};

        #[test]
        fn escapes_entry_artifacts() {
            assert_eq!(
                desktop_exec_quote("/opt/My App/ot").unwrap(),
                "\"/opt/My App/ot\""
            );
            assert_eq!(desktop_exec_quote("/opt/a'b").unwrap(), "\"/opt/a'b\"");
            assert_eq!(desktop_exec_quote("/opt/a$b").unwrap(), "\"/opt/a\\$b\"");
            assert!(desktop_exec_quote("/tmp/a\nb").is_err());

            assert_eq!(xml_escape("A&B<C>\"D'"), "A&amp;B&lt;C&gt;&quot;D&apos;");

            assert_eq!(
                powershell_quote("C:\\Users\\O'Brien\\a.exe"),
                "C:\\Users\\O''Brien\\a.exe"
            );
        }
    }
}

#[tauri::command]
fn autostart_enabled() -> bool {
    autostart::is_enabled()
}

#[tauri::command]
async fn set_autostart(enabled: bool) -> Result<(), String> {
    // Creating the entry can spawn PowerShell/launchctl; keep it off the UI
    // thread.
    tauri::async_runtime::spawn_blocking(move || autostart::set(enabled))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
fn open_config_dir() {
    if let Some(dir) = translator_core::paths::config_path()
        .and_then(|path| path.parent().map(|dir| dir.to_path_buf()))
    {
        open_path(&dir);
    }
}

/// Opens the directory that holds the active model file (or the default model
/// directory when none is configured/downloaded yet).
#[tauri::command]
fn open_model_location() {
    let config = translator_core::settings::load_config();
    let models_dir = translator_core::paths::models_dir();

    let target = config
        .model_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from)
        .filter(|path| path.exists())
        .and_then(|path| path.parent().map(|dir| dir.to_path_buf()))
        .or_else(|| models_dir.filter(|dir| dir.exists()));

    if let Some(dir) = target {
        open_path(&dir);
        return;
    }

    open_config_dir();
}

fn open_path(path: &std::path::Path) {
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", ""])
        .arg(path)
        .spawn();

    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(path).spawn();

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
}

#[tauri::command]
fn get_history(app: AppHandle) -> Vec<HistoryEntry> {
    app.state::<AppState>().history.lock().unwrap().clone()
}

#[tauri::command]
fn load_history_entry(app: AppHandle, index: usize) -> Result<(), String> {
    let entry = app
        .state::<AppState>()
        .history
        .lock()
        .unwrap()
        .get(index)
        .cloned()
        .ok_or_else(|| "history entry not found".to_string())?;

    *app.state::<AppState>().last_text.lock().unwrap() = Some(entry.text.clone());

    {
        let state = app.state::<Mutex<TranslateConfig>>();
        let mut config = state.lock().unwrap();
        config.source = entry.source.clone();
        config.target = entry.target.clone();
    }

    translator_core::settings::persist_source(&entry.source);
    translator_core::settings::persist_target(&entry.target);

    let _ = app.emit("language-state", language_state(&app));
    let _ = app.emit(
        "source",
        SourcePayload {
            text: entry.text,
            replaceable: false,
            detected: None,
        },
    );
    let _ = app.emit("done", entry.translation);

    Ok(())
}

#[tauri::command(async)]
fn replace_text(app: AppHandle, text: String) -> Result<(), String> {
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    {
        let window = *app.state::<AppState>().replace_window.lock().unwrap();
        let Some(window) = window else {
            return Err("没有可替换的原窗口".to_string());
        };

        with_clipboard(&app, |clipboard| {
            capture::replace_selection(window, &text, clipboard)
        })
        .unwrap_or_else(|| Err("无法访问系统剪贴板".to_string()))
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        let _ = (app, text);
        Err("替换原文仅支持 Windows 和 Linux（X11）".to_string())
    }
}

#[tauri::command]
fn clear_history(app: AppHandle) {
    {
        let state = app.state::<AppState>();
        state.history.lock().unwrap().clear();
        translator_core::history::save(&state.history.lock().unwrap());
    }

    let _ = app.emit("history-changed", ());
}

#[tauri::command]
fn set_pinned(app: AppHandle, window: WebviewWindow, pinned: bool) {
    *app.state::<AppState>().pinned.lock().unwrap() = pinned;
    // 固定 means the card stays in front of every other window until unpinned.
    let _ = window.set_always_on_top(pinned);
    translator_core::settings::persist_value("pinned", if pinned { "true" } else { "false" });
}

#[tauri::command]
fn get_initial_view(app: AppHandle) -> Option<String> {
    app.state::<InitialView>().0.lock().unwrap().clone()
}

#[tauri::command]
fn platform() -> &'static str {
    std::env::consts::OS
}

fn language_state(app: &AppHandle) -> LanguageState {
    let (source, target, detected) = {
        let state = app.state::<Mutex<TranslateConfig>>();
        let config = state.lock().unwrap();

        (
            config.source.clone(),
            config.target.clone(),
            config.detected.clone(),
        )
    };

    LanguageState {
        source,
        target,
        detected,
        recent_targets: app.state::<AppState>().recents.lock().unwrap().clone(),
    }
}

fn retranslate_last(app: &AppHandle) {
    let text = app.state::<AppState>().last_text.lock().unwrap().clone();

    if let Some(text) = text {
        translate_text(app, text);
    }
}

#[tauri::command]
fn get_languages() -> Vec<LanguageOption> {
    let mut options = vec![LanguageOption {
        tag: translator_core::languages::AUTO_CODE.to_string(),
        label: translator_core::languages::AUTO_LABEL.to_string(),
    }];

    options.extend(
        translator_core::languages::LANGUAGES
            .iter()
            .map(|(tag, label)| LanguageOption {
                tag: tag.to_string(),
                label: label.to_string(),
            }),
    );

    options
}

#[tauri::command]
fn get_language_state(app: AppHandle) -> LanguageState {
    language_state(&app)
}

#[tauri::command]
fn set_source(app: AppHandle, source: String) -> Result<(), String> {
    if source != translator_core::languages::AUTO_CODE
        && !translator_core::languages::is_supported(&source)
    {
        return Err(format!("unsupported language: {source}"));
    }

    {
        let state = app.state::<Mutex<TranslateConfig>>();
        let mut config = state.lock().unwrap();
        config.source = source.clone();
        config.detected = None;
    }

    translator_core::settings::persist_source(&source);
    let _ = app.emit("language-state", language_state(&app));
    retranslate_last(&app);

    Ok(())
}

#[tauri::command]
fn set_target(app: AppHandle, target: String) -> Result<(), String> {
    if !translator_core::languages::is_supported(&target) {
        return Err(format!("unsupported language: {target}"));
    }

    {
        let state = app.state::<Mutex<TranslateConfig>>();
        let mut config = state.lock().unwrap();
        config.target = target.clone();
    }

    translator_core::settings::persist_target(&target);

    {
        let state = app.state::<AppState>();
        let mut recents = state.recents.lock().unwrap();
        *recents = translator_core::languages::recent_target_list(&recents, &target);
        translator_core::settings::persist_recent_targets(&recents);
    }

    let _ = app.emit("language-state", language_state(&app));
    retranslate_last(&app);

    Ok(())
}

#[tauri::command]
fn swap_languages(app: AppHandle) -> Result<(), String> {
    let (source, target, detected) = {
        let state = app.state::<Mutex<TranslateConfig>>();
        let config = state.lock().unwrap();

        (
            config.source.clone(),
            config.target.clone(),
            config.detected.clone(),
        )
    };

    let Some((new_source, new_target)) =
        translator_core::languages::swapped_pair(&source, &target, detected.as_deref())
    else {
        return Err("当前语言对无法交换".to_string());
    };

    {
        let state = app.state::<Mutex<TranslateConfig>>();
        let mut config = state.lock().unwrap();
        config.source = new_source.clone();
        config.target = new_target.clone();
        config.detected = None;
    }

    translator_core::settings::persist_source(&new_source);
    translator_core::settings::persist_target(&new_target);
    let _ = app.emit("language-state", language_state(&app));

    // Like the eframe client, the previous translation becomes the new source
    // text so ⇄ turns the result back into something to translate.
    let previous = app
        .state::<AppState>()
        .last_translation
        .lock()
        .unwrap()
        .clone();

    if let Some(text) = previous.filter(|text| !text.trim().is_empty()) {
        *app.state::<AppState>().last_text.lock().unwrap() = Some(text.clone());
        translate_text(&app, text);
    } else {
        retranslate_last(&app);
    }

    Ok(())
}

fn parse_bool(key: &str, value: &str) -> Result<bool, String> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(format!("invalid {key}: {other}")),
    }
}

fn resolve_model_path(config: &translator_core::settings::FileConfig) -> Result<PathBuf, String> {
    if let Some(value) = std::env::var_os("TRANSLATOR_MODEL_PATH") {
        return Ok(PathBuf::from(value));
    }

    if let Some(value) = &config.model_path {
        let value = value.trim();

        // Empty means "use the default location" (the settings input clears to
        // this instead of pinning an empty path, which cannot be loaded).
        if !value.is_empty() {
            return Ok(PathBuf::from(value));
        }
    }

    translator_core::paths::default_model_path()
        .ok_or_else(|| "cannot determine the default model directory".to_string())
}

fn resolve_prompt_style(
    config: &translator_core::settings::FileConfig,
) -> Result<PromptStyle, String> {
    let value = std::env::var("TRANSLATOR_PROMPT_STYLE")
        .ok()
        .or_else(|| config.prompt_style.clone());

    match value {
        Some(value) => PromptStyle::parse(&value),
        None => Ok(PromptStyle::HunYuanMt),
    }
}

/// `manual` additionally reports the quiet outcomes (nothing found, check
/// failure); the startup check stays silent about them.
fn spawn_update_check(app: AppHandle, manual: bool) {
    std::thread::spawn(move || {
        if manual {
            let _ = app.emit("update-checking", ());
        }

        let info = match run_update_check() {
            Ok(Some(info)) => info,
            Ok(None) => {
                if let Some(item) = app.state::<UpdateSlot>().item.lock().unwrap().as_ref() {
                    let _ = item.set_text("已是最新版本");
                }

                if manual {
                    let _ = app.emit("update-none", ());
                }

                return;
            }
            Err(message) => {
                if let Some(item) = app.state::<UpdateSlot>().item.lock().unwrap().as_ref() {
                    let _ = item.set_text("更新检查失败");
                }

                if manual {
                    let _ = app.emit("update-check-failed", ErrorPayload { message });
                }

                return;
            }
        };

        let asset = update_asset(&info);
        let asset_url = asset.map(|asset| asset.url.clone());
        let asset_digest = asset
            .and_then(|asset| asset.digest.as_deref())
            .and_then(sha256_hex);
        let can_install =
            asset_url.is_some() && asset_digest.is_some() && install_supported();

        {
            let slot = app.state::<UpdateSlot>();

            *slot.info.lock().unwrap() = Some(UpdateInfo {
                version: info.version.clone(),
                url: info.url.clone(),
                asset_url,
                asset_digest,
                can_install,
            });

            if let Some(item) = slot.item.lock().unwrap().as_ref() {
                let _ = item.set_text(format!("有新版本 v{}…", info.version));
                let _ = item.set_enabled(true);
            }
        }

        let _ = app.emit(
            "update-available",
            UpdatePayload {
                version: info.version,
                can_install,
            },
        );
    });
}

fn run_update_check() -> Result<Option<ReleaseInfo>, String> {
    let client = translator_core::translate::build_client()?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;

    runtime
        .block_on(translator_core::update::check(
            &client,
            translator_core::update::current_version(),
            &translator_core::update::api_url(),
        ))
        .map_err(|error| error.to_string())
}

fn update_asset(info: &ReleaseInfo) -> Option<&translator_core::update::ReleaseAsset> {
    let name = if cfg!(target_os = "windows") {
        "OpenTranslator-windows-x64.zip"
    } else if cfg!(target_os = "macos") {
        "OpenTranslator-macos-arm64.dmg"
    } else {
        "OpenTranslator-linux-x64.deb"
    };

    info.assets.iter().find(|asset| asset.name == name)
}

/// GitHub returns asset digests as `sha256:<hex>`; accept only a full,
/// well-formed SHA-256 so an install never runs without integrity data.
fn sha256_hex(digest: &str) -> Option<String> {
    let hex = digest.strip_prefix("sha256:")?;

    if hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Some(hex.to_ascii_lowercase())
    } else {
        None
    }
}

/// Whether this build can install the downloaded package itself. Linux uses
/// pkexec + apt-get for the deb and falls back to the release page otherwise;
/// macOS keeps the release-page link.
fn install_supported() -> bool {
    #[cfg(target_os = "windows")]
    {
        true
    }
    #[cfg(target_os = "linux")]
    {
        command_in_path("pkexec") && command_in_path("apt-get")
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        false
    }
}

fn start_update_install(app: &AppHandle) {
    let slot = app.state::<UpdateSlot>();

    {
        let mut running = slot.running.lock().unwrap();

        if *running {
            return;
        }

        *running = true;
    }

    let Some((asset_url, asset_digest)) = slot
        .info
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|info| Some((info.asset_url.clone()?, info.asset_digest.clone())))
    else {
        *slot.running.lock().unwrap() = false;
        return;
    };

    let app = app.clone();

    #[cfg(target_os = "windows")]
    std::thread::spawn(move || match run_update_install(&app, &asset_url, asset_digest) {
        Ok(()) => {
            let _ = app.emit("update-installed", ());
            app.exit(0);
        }
        Err(error) => {
            *app.state::<UpdateSlot>().running.lock().unwrap() = false;

            if window_hidden(&app) {
                notify::show("OpenTranslator", &error);
            }

            let _ = app.emit("update-error", ErrorPayload { message: error });
        }
    });

    #[cfg(target_os = "linux")]
    std::thread::spawn(move || match run_update_install(&app, &asset_url, asset_digest) {
        Ok(()) => {
            let _ = app.emit("update-installed", ());
            // `apt` replaced the binary on disk while this process keeps the
            // old inode, so exit first and let a delayed shell start the new
            // build in the tray.
            let _ = std::process::Command::new("sh")
                .args(["-c", "sleep 1; exec /usr/bin/translator-popup --autostart"])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
            app.exit(0);
        }
        Err(error) => {
            *app.state::<UpdateSlot>().running.lock().unwrap() = false;

            if window_hidden(&app) {
                notify::show("OpenTranslator", &error);
            }

            let _ = app.emit("update-error", ErrorPayload { message: error });
        }
    });

    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    let _ = (app, asset_url, asset_digest);
}

/// Downloads the release package, extracts it, verifies the installer and the
/// Tauri binary and starts `install.ps1`, which replaces this build and
/// restarts the new one in the tray.
#[cfg(target_os = "windows")]
fn run_update_install(
    app: &AppHandle,
    asset_url: &str,
    expected_sha256: Option<String>,
) -> Result<(), String> {
    run_update_install_with(asset_url, expected_sha256.as_deref(), &mut |downloaded, total| {
        let _ = app.emit(
            "update-progress",
            ProgressPayload { downloaded, total },
        );
    })
}

#[cfg(target_os = "windows")]
fn run_update_install_with(
    asset_url: &str,
    expected_sha256: Option<&str>,
    on_progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<(), String> {
    let client = translator_core::models::download_client().map_err(|error| error.to_string())?;

    let dir = std::env::temp_dir().join("open-translator-update");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|error| format!("创建更新目录失败：{error}"))?;

    let archive = dir.join("OpenTranslator-windows-x64.zip");

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("初始化更新下载失败：{error}"))?;

    runtime
        .block_on(translator_core::models::download(
            &client,
            asset_url,
            &archive,
            expected_sha256,
            on_progress,
        ))
        .map_err(|error| format!("下载更新失败：{error}"))?;

    let package = dir.join("package");
    let expand = format!(
        "Expand-Archive -LiteralPath {} -DestinationPath {} -Force",
        ps_quote(&archive.to_string_lossy()),
        ps_quote(&package.to_string_lossy())
    );

    let status = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
        ])
        .arg(&expand)
        .status()
        .map_err(|error| format!("解压更新失败：{error}"))?;

    if !status.success() {
        return Err("解压更新失败".to_string());
    }

    let installer = package.join("install.ps1");
    let exe = package.join("translator-popup-tauri.exe");

    if !installer.is_file() || !exe.is_file() {
        return Err("更新包内容不完整（该版本尚未包含 Tauri 客户端）".to_string());
    }

    std::process::Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&installer)
        .spawn()
        .map_err(|error| format!("启动安装程序失败：{error}"))?;

    Ok(())
}

#[cfg(target_os = "windows")]
fn ps_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

/// Private staging directory for the downloaded package. `mkdir` never
/// follows an existing symlink and the unique name plus 0700 mode keeps other
/// local users away from the path that is later installed as root.
#[cfg(target_os = "linux")]
struct UpdateStagingDir(PathBuf);

#[cfg(target_os = "linux")]
impl UpdateStagingDir {
    fn create() -> Result<Self, String> {
        use std::os::unix::fs::DirBuilderExt;

        let base = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .filter(|path| path.is_dir())
            .unwrap_or_else(std::env::temp_dir);
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let dir = base.join(format!(
            "open-translator-update-{}-{unique}",
            std::process::id()
        ));

        let mut builder = std::fs::DirBuilder::new();
        builder.mode(0o700);

        builder
            .create(&dir)
            .map_err(|error| format!("创建更新目录失败：{error}"))?;

        Ok(Self(dir))
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

#[cfg(target_os = "linux")]
impl Drop for UpdateStagingDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Downloads the release deb and installs it through pkexec (the policykit
/// agent asks for the user's password); the caller restarts the client.
#[cfg(target_os = "linux")]
fn run_update_install(
    app: &AppHandle,
    asset_url: &str,
    expected_sha256: Option<String>,
) -> Result<(), String> {
    run_update_install_with(asset_url, expected_sha256.as_deref(), &mut |downloaded, total| {
        let _ = app.emit(
            "update-progress",
            ProgressPayload { downloaded, total },
        );
    })
}

#[cfg(target_os = "linux")]
fn run_update_install_with(
    asset_url: &str,
    expected_sha256: Option<&str>,
    on_progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<(), String> {
    let client = translator_core::models::download_client().map_err(|error| error.to_string())?;

    let staging = UpdateStagingDir::create()?;
    let deb = staging.path().join("OpenTranslator-linux-x64.deb");

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("初始化更新下载失败：{error}"))?;

    runtime
        .block_on(translator_core::models::download(
            &client,
            asset_url,
            &deb,
            expected_sha256,
            on_progress,
        ))
        .map_err(|error| format!("下载更新失败：{error}"))?;

    let status = std::process::Command::new("pkexec")
        .args(pkexec_apt_args())
        .arg(&deb)
        .status()
        .map_err(|error| format!("启动安装程序失败：{error}"))?;

    if !status.success() {
        return Err("安装更新失败（认证被取消或 apt 出错）".to_string());
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn pkexec_apt_args() -> [&'static str; 6] {
    [
        "env",
        "DEBIAN_FRONTEND=noninteractive",
        "apt-get",
        "install",
        "-y",
        "--allow-downgrades",
    ]
}

fn open_url(url: &str) {
    #[cfg(target_os = "windows")]
    let (program, args): (&str, Vec<&str>) = ("cmd", vec!["/C", "start", "", url]);
    #[cfg(target_os = "macos")]
    let (program, args): (&str, Vec<&str>) = ("open", vec![url]);
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let (program, args): (&str, Vec<&str>) = ("xdg-open", vec![url]);

    let _ = std::process::Command::new(program).args(args).spawn();
}

#[tauri::command]
fn start_update(app: AppHandle) {
    start_update_install(&app);
}

#[tauri::command]
fn open_release_page(app: AppHandle) {
    let url = app
        .state::<UpdateSlot>()
        .info
        .lock()
        .unwrap()
        .as_ref()
        .map(|info| info.url.clone());

    if let Some(url) = url {
        open_url(&url);
    }
}

#[tauri::command]
fn check_update_now(app: AppHandle) {
    spawn_update_check(app, true);
}

#[tauri::command]
fn get_update_state(app: AppHandle) -> UpdateStatePayload {
    let slot = app.state::<UpdateSlot>();
    let info = slot.info.lock().unwrap();

    UpdateStatePayload {
        version: info.as_ref().map(|info| info.version.clone()),
        can_install: info.as_ref().map_or(false, |info| info.can_install),
    }
}

fn make_icon_rgba() -> Vec<u8> {
    const SIZE: u32 = 32;
    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];

    let center = (SIZE as f32 - 1.0) / 2.0;
    let radius = 15.0;
    // macOS menu bar icons are template images: draw the glyph in black with
    // transparency and let the system tint it for light/dark menu bars.
    let template = cfg!(target_os = "macos");

    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let distance = (dx * dx + dy * dy).sqrt();

            let inside = if template {
                distance <= radius && distance >= radius - 2.0
            } else {
                distance <= radius
            };

            if inside {
                let index = ((y * SIZE + x) * 4) as usize;

                if template {
                    rgba[index] = 0x00;
                    rgba[index + 1] = 0x00;
                    rgba[index + 2] = 0x00;
                } else {
                    rgba[index] = 0x25;
                    rgba[index + 1] = 0x63;
                    rgba[index + 2] = 0xeb;
                }

                rgba[index + 3] = 0xff;
            }
        }
    }

    for top in [11u32, 19] {
        for y in top..(top + 2) {
            for x in 9..23 {
                let index = ((y * SIZE + x) * 4) as usize;
                let value = if template { 0x00 } else { 0xff };

                rgba[index] = value;
                rgba[index + 1] = value;
                rgba[index + 2] = value;
                rgba[index + 3] = 0xff;
            }
        }
    }

    rgba
}

#[cfg(test)]
mod tests {
    use super::{WorkArea, card_position};

    #[cfg(target_os = "linux")]
    #[test]
    fn maps_speech_languages() {
        assert_eq!(super::spd_language("zh"), Some("cmn".to_string()));
        assert_eq!(super::spd_language("zh-TW"), Some("cmn".to_string()));
        assert_eq!(super::spd_language("en-US"), Some("en".to_string()));
        assert_eq!(super::spd_language("ja"), Some("ja".to_string()));
        assert_eq!(super::spd_language("auto"), None);
        assert_eq!(super::spd_language(""), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn autostart_toggles_the_desktop_entry() {
        let dir = std::env::temp_dir().join(format!("ot-autostart-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        unsafe {
            std::env::set_var("XDG_CONFIG_HOME", &dir);
        }

        assert!(!super::autostart::is_enabled());
        super::autostart::set(true).expect("enable autostart");
        assert!(super::autostart::is_enabled());
        super::autostart::set(false).expect("disable autostart");
        assert!(!super::autostart::is_enabled());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn speech_command_waits_and_selects_the_language() {
        let args: Vec<String> = super::spd_speak_command("zh")
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();

        assert!(args.contains(&"--wait".to_string()));
        assert!(args.contains(&"--pipe-mode".to_string()));
        let language = args.iter().position(|arg| arg == "-l");
        assert_eq!(language.map(|index| args[index + 1].as_str()), Some("cmn"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn parses_voice_languages_from_listing() {
        let sample = "                     NAME                 LANGUAGE                  VARIANT\n\
                      Afrikaans af none\n\
                      English (Caribbean)+Alicia en-029 Alicia\n\
                      Chinese (Mandarin, latin as English) cmn none\n\
                      German de none\n\
                      German+Half-LifeAnnouncementSystem deHalf-LifeAnnouncementSystem\n";
        let languages = super::parse_voice_languages(sample);

        assert!(languages.contains("af"));
        assert!(languages.contains("en"));
        assert!(languages.contains("cmn"));
        assert!(languages.contains("de"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn pkexec_install_args_are_noninteractive() {
        let args = super::pkexec_apt_args();

        assert_eq!(args[0], "env");
        assert_eq!(args[2], "apt-get");
        assert!(args.contains(&"DEBIAN_FRONTEND=noninteractive"));
        assert!(args.contains(&"--allow-downgrades"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn staging_dirs_are_private_and_unique() {
        use std::os::unix::fs::PermissionsExt;

        let first = super::UpdateStagingDir::create().unwrap();
        let second = super::UpdateStagingDir::create().unwrap();

        assert_ne!(first.path(), second.path());
        let mode = std::fs::metadata(first.path())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700);
    }

    #[test]
    fn accepts_only_well_formed_sha256_digests() {
        let valid = format!("sha256:{}", "AB12".repeat(16));
        assert_eq!(
            super::sha256_hex(&valid),
            Some("ab12".repeat(16))
        );

        assert_eq!(super::sha256_hex("sha256:1234"), None);
        assert_eq!(super::sha256_hex("md5:abcd"), None);
        assert_eq!(
            super::sha256_hex(&format!("sha256:{}", "zz".repeat(32))),
            None
        );
    }

    fn work() -> WorkArea {
        WorkArea {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        }
    }

    #[test]
    fn places_below_and_right_of_the_cursor() {
        assert_eq!(
            card_position((100, 200), (520, 300), work(), 12, 18),
            (112, 218)
        );
    }

    #[test]
    fn slides_above_the_cursor_near_the_bottom_edge() {
        assert_eq!(
            card_position((100, 900), (520, 300), work(), 12, 18),
            (112, 588)
        );
    }

    #[test]
    fn clamps_to_the_work_area_corners() {
        assert_eq!(
            card_position((-50, -50), (520, 300), work(), 12, 18),
            (12, 12)
        );
        assert_eq!(
            card_position((2000, 2000), (520, 300), work(), 12, 18),
            (1388, 768)
        );
    }

    #[test]
    fn handles_negative_monitor_origins() {
        let left = WorkArea {
            x: -1920,
            y: 0,
            width: 1920,
            height: 1080,
        };

        assert_eq!(
            card_position((-2000, 50), (520, 300), left, 12, 18),
            (-1908, 68)
        );
    }

    #[test]
    fn keeps_the_margin_when_the_card_is_larger_than_the_work_area() {
        let small = WorkArea {
            x: 0,
            y: 0,
            width: 400,
            height: 200,
        };

        assert_eq!(
            card_position((100, 100), (520, 300), small, 12, 18),
            (12, 12)
        );
    }
}

#[cfg(all(test, target_os = "windows"))]
mod update_tests {
    use super::run_update_install_with;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    #[test]
    fn downloads_extracts_and_launches_the_installer() {
        let marker = std::env::temp_dir().join("open-translator-update-test-marker.txt");
        let _ = std::fs::remove_file(&marker);

        let staging = std::env::temp_dir().join("open-translator-update-test-staging");
        let _ = std::fs::remove_dir_all(&staging);
        std::fs::create_dir_all(&staging).unwrap();
        std::fs::write(
            staging.join("install.ps1"),
            format!("Set-Content -LiteralPath '{}' -Value ok", marker.display()),
        )
        .unwrap();
        std::fs::write(staging.join("translator-popup-tauri.exe"), b"stub").unwrap();

        let archive = std::env::temp_dir().join("open-translator-update-test.zip");
        let _ = std::fs::remove_file(&archive);

        let status = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command"])
            .arg(format!(
                "Compress-Archive -Path '{}' -DestinationPath '{}' -Force",
                staging.join("*").display(),
                archive.display()
            ))
            .status()
            .unwrap();
        assert!(status.success());

        let payload = std::fs::read(&archive).unwrap();
        let expected_total = payload.len() as u64;
        let app = axum::Router::new().route(
            "/pkg.zip",
            axum::routing::get(move || {
                let payload = payload.clone();
                async move { payload }
            }),
        );
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();

        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async move {
                let listener = tokio::net::TcpListener::from_std(listener).unwrap();
                axum::serve(listener, app).await.unwrap();
            });
        });

        let progress = Arc::new(Mutex::new(Vec::new()));
        let recorded = progress.clone();

        run_update_install_with(
            &format!("http://{address}/pkg.zip"),
            None,
            &mut |downloaded, total| {
                recorded.lock().unwrap().push((downloaded, total));
            },
        )
        .unwrap();

        let deadline = Instant::now() + Duration::from_secs(10);
        while !marker.is_file() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(marker.is_file(), "the installer script did not run");

        let progress = progress.lock().unwrap();
        assert!(!progress.is_empty(), "no download progress was reported");

        assert!(
            progress
                .iter()
                .any(|(_, total)| total.is_some_and(|total| total == expected_total)),
            "the reported total did not match the package size"
        );
    }
}
