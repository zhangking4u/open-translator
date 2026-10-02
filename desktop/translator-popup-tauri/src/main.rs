#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]

mod capture;
mod notify;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewWindow, WindowEvent};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use translator_core::history::HistoryEntry;
use translator_service::domain::prompt::PromptStyle;
use translator_service::domain::translation::TranslationRequest;
use translator_service::engine::llama_cpp::LlamaCppEngine;

const DEFAULT_HOTKEY: &str = "Ctrl+Alt+T";
const DEFAULT_N_CTX: u32 = 4096;

struct InitialView(Mutex<Option<String>>);

struct AppState {
    engine: Mutex<Option<Arc<LlamaCppEngine>>>,
    pending: Mutex<Option<String>>,
    last_text: Mutex<Option<String>>,
    history: Mutex<Vec<HistoryEntry>>,
    pinned: Mutex<bool>,
}

#[derive(Clone)]
struct TranslateConfig {
    source: String,
    target: String,
    hotkey_label: String,
}

#[derive(Clone, serde::Serialize)]
struct SettingsPayload {
    hotkey: String,
    model_path: String,
    auto_download: bool,
    check_updates: bool,
    serve_extension: bool,
    config_path: Option<String>,
}

#[derive(Clone, serde::Serialize)]
struct SourcePayload {
    text: String,
}

#[derive(Clone, serde::Serialize)]
struct ProgressPayload {
    downloaded: u64,
    total: Option<u64>,
}

#[derive(Clone, serde::Serialize)]
struct ErrorPayload {
    message: String,
}

fn main() {
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

    let show_on_start = !args.autostart || args.stdin || args.settings || args.history;
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
        hotkey_label: hotkey_spec.clone(),
    };

    tauri::Builder::default()
        .manage(AppState {
            engine: Mutex::new(None),
            pending: Mutex::new(None),
            last_text: Mutex::new(None),
            history: Mutex::new(translator_core::history::load()),
            pinned: Mutex::new(false),
        })
        .manage(Mutex::new(translate_config))
        .manage(InitialView(Mutex::new(initial_view)))
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            show_main(app);
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
            copy_text,
            retranslate,
            get_settings,
            save_hotkey,
            save_model_path,
            save_switch,
            open_config_dir,
            get_history,
            load_history_entry,
            clear_history,
            set_pinned,
            get_initial_view
        ])
        .setup(move |app| {
            let handle = app.handle().clone();

            let menu = Menu::with_items(
                &handle,
                &[
                    &MenuItem::with_id(&handle, "show", "显示窗口", true, None::<&str>)?,
                    &MenuItem::with_id(
                        &handle,
                        "translate",
                        "立即翻译选中文本",
                        true,
                        None::<&str>,
                    )?,
                    &MenuItem::with_id(&handle, "history", "历史…", true, None::<&str>)?,
                    &MenuItem::with_id(&handle, "settings", "设置…", true, None::<&str>)?,
                    &MenuItem::with_id(&handle, "quit", "退出", true, None::<&str>)?,
                ],
            )?;

            TrayIconBuilder::with_id("main")
                .icon(Image::new_owned(make_icon_rgba(), 32, 32))
                .tooltip(format!("OpenTranslator（{hotkey_spec}）"))
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_main(app),
                    "translate" => trigger_translation(app),
                    "history" => {
                        show_main(app);
                        let _ = app.emit("open-history", ());
                    }
                    "settings" => {
                        show_main(app);
                        let _ = app.emit("open-settings", ());
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(&handle)?;

            if let Err(error) = app.global_shortcut().register(hotkey_spec.as_str()) {
                eprintln!("failed to register {hotkey_spec}: {error}");
                let message = format!("快捷键注册失败：{error}");

                if window_hidden(&handle) {
                    notify::show("OpenTranslator", &message);
                }

                let _ = app.emit("error", ErrorPayload { message });
            }

            spawn_model_startup(handle.clone(), model_path, auto_download, prompt_style);

            if show_on_start {
                show_main(&handle);
            }

            if !stdin_text.trim().is_empty() {
                translate_text(&handle, stdin_text);
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
                {
                    let state = app.state::<AppState>();
                    *state.engine.lock().unwrap() = Some(Arc::new(engine));
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
    show_main(app);

    let app = app.clone();
    std::thread::spawn(move || match capture::capture_selection() {
        Ok(text) if text.trim().is_empty() => {
            let _ = app.emit("empty", ());
        }
        Ok(text) => {
            translate_text(&app, text);
        }
        Err(error) if error == "no selected text found" => {
            let _ = app.emit("empty", ());
        }
        Err(error) => {
            let _ = app.emit("error", ErrorPayload { message: error });
        }
    });
}

fn translate_text(app: &AppHandle, text: String) {
    *app.state::<AppState>().last_text.lock().unwrap() = Some(text.clone());

    let engine = app.state::<AppState>().engine.lock().unwrap().clone();
    let Some(engine) = engine else {
        *app.state::<AppState>().pending.lock().unwrap() = Some(text);
        return;
    };

    let config = app
        .state::<Mutex<TranslateConfig>>()
        .lock()
        .unwrap()
        .clone();
    let app = app.clone();
    let _ = app.emit("source", SourcePayload { text: text.clone() });

    std::thread::spawn(move || {
        let request = TranslationRequest {
            text: text.clone(),
            source: config.source.clone(),
            target: config.target.clone(),
        };

        let delta_app = app.clone();
        let result = engine.translate_blocking_streaming(&request, move |piece| {
            let _ = delta_app.emit("delta", piece.to_string());
        });

        match result {
            Ok(result) => {
                {
                    let state = app.state::<AppState>();
                    let mut history = state.history.lock().unwrap();

                    translator_core::history::push(
                        &mut history,
                        HistoryEntry {
                            source: config.source.clone(),
                            target: config.target.clone(),
                            text,
                            translation: result.translated_text.clone(),
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
    });
}

fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        place_near_cursor(app, &window);
        let _ = window.set_focus();
    }
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

    let work_right = work.position.x + work.size.width as i32;
    let work_bottom = work.position.y + work.size.height as i32;

    let mut x = cursor.x as i32 + margin;
    let mut y = cursor.y as i32 + offset;

    // Slide the card above the cursor when it would overflow the bottom edge.
    if y + size.height as i32 + margin > work_bottom {
        y = cursor.y as i32 - size.height as i32 - margin;
    }

    let min_x = work.position.x + margin;
    let min_y = work.position.y + margin;
    let max_x = work_right - size.width as i32 - margin;
    let max_y = work_bottom - size.height as i32 - margin;

    x = x.clamp(min_x, max_x.max(min_x));
    y = y.clamp(min_y, max_y.max(min_y));

    let _ = window.set_position(PhysicalPosition::new(x, y));
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

    let _ = window.hide();
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn resize_window(window: WebviewWindow, height: f64) {
    if let Ok(size) = window.inner_size() {
        let height = height.clamp(120.0, 2000.0) as u32;
        let _ = window.set_size(tauri::PhysicalSize::new(size.width, height));
    }
}

#[tauri::command]
fn copy_text(text: String) {
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    if let Ok(mut clipboard) = arboard::Clipboard::new() {
        let _ = clipboard.set_text(text);
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let _ = text;
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

    SettingsPayload {
        hotkey,
        model_path: config.model_path.unwrap_or_default(),
        auto_download: config.auto_download.as_deref() != Some("false"),
        check_updates: config.check_updates.as_deref() != Some("false"),
        serve_extension: config.serve_extension.as_deref() != Some("false"),
        config_path: translator_core::paths::config_path()
            .map(|path| path.to_string_lossy().to_string()),
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

#[tauri::command]
fn open_config_dir() {
    if let Some(dir) = translator_core::paths::config_path()
        .and_then(|path| path.parent().map(|dir| dir.to_path_buf()))
    {
        open_path(&dir);
    }
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

    let _ = app.emit("source", SourcePayload { text: entry.text });
    let _ = app.emit("done", entry.translation);

    Ok(())
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
fn set_pinned(app: AppHandle, pinned: bool) {
    *app.state::<AppState>().pinned.lock().unwrap() = pinned;
}

#[tauri::command]
fn get_initial_view(app: AppHandle) -> Option<String> {
    app.state::<InitialView>().0.lock().unwrap().clone()
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
        return Ok(PathBuf::from(value));
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

fn make_icon_rgba() -> Vec<u8> {
    const SIZE: u32 = 32;
    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];

    let center = (SIZE as f32 - 1.0) / 2.0;
    let radius = 15.0;

    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f32 - center;
            let dy = y as f32 - center;

            if (dx * dx + dy * dy).sqrt() <= radius {
                let index = ((y * SIZE + x) * 4) as usize;
                rgba[index] = 0x25;
                rgba[index + 1] = 0x63;
                rgba[index + 2] = 0xeb;
                rgba[index + 3] = 0xff;
            }
        }
    }

    for top in [11u32, 19] {
        for y in top..(top + 2) {
            for x in 9..23 {
                let index = ((y * SIZE + x) * 4) as usize;
                rgba[index] = 0xff;
                rgba[index + 1] = 0xff;
                rgba[index + 2] = 0xff;
                rgba[index + 3] = 0xff;
            }
        }
    }

    rgba
}
