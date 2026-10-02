#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]

mod capture;
mod notify;
mod server;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use translator_core::update::ReleaseInfo;
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

#[derive(Clone)]
struct ServerPlan {
    bind_addr: String,
    model_name: String,
}

#[derive(Clone)]
struct UpdateInfo {
    url: String,
    asset_url: Option<String>,
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

struct AppState {
    engine: Mutex<Option<Arc<LlamaCppEngine>>>,
    pending: Mutex<Option<String>>,
    last_text: Mutex<Option<String>>,
    history: Mutex<Vec<HistoryEntry>>,
    pinned: Mutex<bool>,
    replace_window: Mutex<Option<isize>>,
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
    replaceable: bool,
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

    let serve_extension = config.serve_extension.as_deref() != Some("false");
    let server_plan = serve_extension.then(|| ServerPlan {
        bind_addr: translator_core::services::bind_addr_from_service_url(&args.service_url)
            .unwrap_or_else(|| "http://127.0.0.1:17890".to_string()),
        model_name: model_path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default(),
    });

    let translate_on_start = args.translate;
    // A --translate cold start must not show the window before the capture
    // thread runs, or the synthesized Ctrl+C would land on our own window.
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
            replace_window: Mutex::new(None),
        })
        .manage(Mutex::new(translate_config))
        .manage(InitialView(Mutex::new(initial_view)))
        .manage(UpdateSlot::default())
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if argv.iter().any(|arg| arg == "--translate") {
                trigger_translation(app);
            } else if argv.iter().any(|arg| arg == "--settings") {
                show_main(app);
                let _ = app.emit("open-settings", ());
            } else if argv.iter().any(|arg| arg == "--history") {
                show_main(app);
                let _ = app.emit("open-history", ());
            } else {
                show_main(app);
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
            get_initial_view,
            platform,
            start_update,
            open_release_page,
            replace_text
        ])
        .setup(move |app| {
            let handle = app.handle().clone();

            let show_item = MenuItem::with_id(&handle, "show", "显示窗口", true, None::<&str>)?;
            let translate_item = MenuItem::with_id(
                &handle,
                "translate",
                "立即翻译选中文本",
                true,
                None::<&str>,
            )?;
            let history_item =
                MenuItem::with_id(&handle, "history", "历史…", true, None::<&str>)?;
            let settings_item =
                MenuItem::with_id(&handle, "settings", "设置…", true, None::<&str>)?;
            let update_item =
                MenuItem::with_id(&handle, "update", "有新版本可用", false, None::<&str>)?;
            let quit_item = MenuItem::with_id(&handle, "quit", "退出", true, None::<&str>)?;

            let menu = Menu::with_items(
                &handle,
                &[
                    &show_item,
                    &translate_item,
                    &history_item,
                    &settings_item,
                    &update_item,
                    &quit_item,
                ],
            )?;

            *app.state::<UpdateSlot>().item.lock().unwrap() = Some(update_item);

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
                    "update" => {
                        let info = app.state::<UpdateSlot>().info.lock().unwrap().clone();

                        if let Some(info) = info {
                            if info.asset_url.is_some() && cfg!(target_os = "windows") {
                                start_update_install(app);
                            } else {
                                open_url(&info.url);
                            }
                        }
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

            spawn_model_startup(
                handle.clone(),
                model_path,
                auto_download,
                prompt_style,
                server_plan,
            );

            if config.check_updates.as_deref() != Some("false") {
                spawn_update_check(handle.clone());
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
            Err(error) if error == "no selected text found" => {
                let _ = app.emit("empty", ());
            }
            Err(error) => {
                let _ = app.emit("error", ErrorPayload { message: error });
            }
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
    let replaceable = cfg!(target_os = "windows")
        && app
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
        },
    );

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
    let Ok(size) = window.inner_size() else {
        return;
    };

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

    let height = (height as u32).clamp(120, max_height);
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

    let _ = app.emit(
        "source",
        SourcePayload {
            text: entry.text,
            replaceable: false,
        },
    );
    let _ = app.emit("done", entry.translation);

    Ok(())
}

#[tauri::command]
fn replace_text(app: AppHandle, text: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let window = *app.state::<AppState>().replace_window.lock().unwrap();
        let Some(window) = window else {
            return Err("没有可替换的原窗口".to_string());
        };

        capture::replace_selection(window, &text)
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app, text);
        Err("替换原文仅支持 Windows".to_string())
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
}

#[tauri::command]
fn get_initial_view(app: AppHandle) -> Option<String> {
    app.state::<InitialView>().0.lock().unwrap().clone()
}

#[tauri::command]
fn platform() -> &'static str {
    std::env::consts::OS
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

fn spawn_update_check(app: AppHandle) {
    std::thread::spawn(move || {
        let Ok(client) = translator_core::translate::build_client() else {
            return;
        };

        let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        else {
            return;
        };

        let Ok(Some(info)) = runtime.block_on(translator_core::update::check(
            &client,
            translator_core::update::current_version(),
            &translator_core::update::api_url(),
        )) else {
            return;
        };

        let asset_url = update_asset(&info).map(|asset| asset.url.clone());
        let can_install = asset_url.is_some() && cfg!(target_os = "windows");

        {
            let slot = app.state::<UpdateSlot>();

            *slot.info.lock().unwrap() = Some(UpdateInfo {
                url: info.url.clone(),
                asset_url,
            });

            if let Some(item) = slot.item.lock().unwrap().as_ref() {
                let _ = item.set_text(format!("有新版本 v{}", info.version));
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

fn update_asset(info: &ReleaseInfo) -> Option<&translator_core::update::ReleaseAsset> {
    info.assets
        .iter()
        .find(|asset| asset.name == "OpenTranslator-windows-x64.zip")
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

    let Some(asset_url) = slot
        .info
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|info| info.asset_url.clone())
    else {
        return;
    };

    let app = app.clone();

    #[cfg(target_os = "windows")]
    std::thread::spawn(move || match run_update_install(&app, &asset_url) {
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

    #[cfg(not(target_os = "windows"))]
    let _ = (app, asset_url);
}

/// Downloads the release package, extracts it, verifies the installer and the
/// Tauri binary and starts `install.ps1`, which replaces this build and
/// restarts the new one in the tray.
#[cfg(target_os = "windows")]
fn run_update_install(app: &AppHandle, asset_url: &str) -> Result<(), String> {
    let client = translator_core::models::download_client().map_err(|error| error.to_string())?;

    let dir = std::env::temp_dir().join("open-translator-update");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|error| format!("创建更新目录失败：{error}"))?;

    let archive = dir.join("OpenTranslator-windows-x64.zip");
    let progress_app = app.clone();

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("初始化更新下载失败：{error}"))?;

    runtime
        .block_on(translator_core::models::download(
            &client,
            asset_url,
            &archive,
            None,
            move |downloaded, total| {
                let _ = progress_app.emit(
                    "update-progress",
                    ProgressPayload { downloaded, total },
                );
            },
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
