#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod capture;
mod hotkey;
mod notify;
mod server;
mod single_instance;
mod tray;

use std::path::PathBuf;
use std::sync::Arc;

use translator_core::args::{Args, read_stdin};
use translator_core::models;
use translator_core::paths::default_model_path;
use translator_core::services::bind_addr_from_service_url;
use translator_core::settings::{FileConfig, load_config};
use translator_core::update;
use translator_service::domain::prompt::PromptStyle;
use translator_service::domain::translation::TranslationRequest;
use translator_service::engine::llama_cpp::LlamaCppEngine;

const DEFAULT_N_CTX: u32 = 4096;
const DEFAULT_BIND_ADDR: &str = "127.0.0.1:17890";

const HELP: &str = "\
Usage: translator-popup-desktop [OPTIONS]

Desktop popup for OpenTranslator (Windows / macOS): select text and press
Ctrl+Alt+T to translate it with the embedded model. The app stays resident;
Esc hides the window, 退出 quits. Launching it manually shows the window;
the login autostart (--autostart) starts silently in the tray. On first run
the model is downloaded automatically. While running it also serves the
local HTTP API on service_url for the browser extension.

Options:
  -s, --source <LANG>   Source language tag (default: auto; auto detects
                        the language of the selected text)
  -t, --target <LANG>   Target language tag (default: zh)
      --stdin           Use stdin instead of the current selection (testing)
      --print           Print the translation and exit (debug builds; Windows
                        release builds are GUI-only without a console)
      --autostart       Start silently in the tray (used by the login
                        autostart shortcut; without it the window is shown)
      --service <URL>   Also used as the bind address for the extension API
                        (default: http://127.0.0.1:17890)
  -h, --help            Show this help

Config file (Windows: %APPDATA%\\open-translator\\config, macOS:
~/Library/Application Support/open-translator/config):

  service_url = http://127.0.0.1:17890
  source = auto
  target = zh
  hotkey = Ctrl+Alt+T
  model_path = <path to a .gguf model>      (default: per-user models dir)
  prompt_style = hymt                       (generic / translategemma / hymt)
  serve_extension = true                    (disable to skip the HTTP endpoint)
  auto_download = true                      (download the model on first run)
  check_updates = true                      (check GitHub for a newer release)

Environment overrides: TRANSLATOR_HOTKEY, TRANSLATOR_MODEL_PATH,
TRANSLATOR_PROMPT_STYLE, TRANSLATOR_CHECK_UPDATES, TRANSLATOR_UPDATE_URL.
Default model location:
  Windows  %LOCALAPPDATA%\\open-translator\\models\\hy-mt1.5-1.8b-q4_k_m.gguf
  macOS    ~/Library/Application Support/open-translator/models/hy-mt1.5-1.8b-q4_k_m.gguf

The hotkey sends Ctrl+C (Cmd+C on macOS) to the focused window and reads the
clipboard, so the selection must come from an application that supports copy.
On macOS, allow OpenTranslator under System Settings -> Privacy & Security ->
Accessibility on first use.
";

fn parse_bool(key: &str, value: &str) -> Result<bool, String> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(format!("invalid {key}: {other}")),
    }
}

fn resolve_model_path(config: &FileConfig) -> Result<PathBuf, String> {
    if let Some(value) = std::env::var_os("TRANSLATOR_MODEL_PATH") {
        return Ok(PathBuf::from(value));
    }

    if let Some(value) = &config.model_path {
        return Ok(PathBuf::from(value));
    }

    default_model_path().ok_or_else(|| "cannot determine the default model directory".to_string())
}

fn resolve_prompt_style(config: &FileConfig) -> Result<PromptStyle, String> {
    let value = std::env::var("TRANSLATOR_PROMPT_STYLE")
        .ok()
        .or_else(|| config.prompt_style.clone());

    match value {
        Some(value) => PromptStyle::parse(&value),
        None => Ok(PromptStyle::HunYuanMt),
    }
}

fn run_headless(engine: &Arc<LlamaCppEngine>, args: &Args, text: &str) -> i32 {
    if text.is_empty() {
        eprintln!("no selected text found");
        return 1;
    }

    let (source, _) = translator_core::detect::resolve_source(&args.source, text);

    let request = TranslationRequest {
        text: text.to_string(),
        source,
        target: args.target.clone(),
    };

    match engine.translate_blocking(&request) {
        Ok(result) => {
            println!("{}", result.translated_text);
            0
        }
        Err(error) => {
            eprintln!("{error}");
            1
        }
    }
}

fn main() {
    #[cfg(target_os = "windows")]
    if std::env::var_os("WGPU_BACKEND").is_none() {
        // SAFETY: called before any other thread is spawned.
        unsafe { std::env::set_var("WGPU_BACKEND", "dx12") };
    }

    if std::env::args().any(|arg| arg == "--help" || arg == "-h") {
        print!("{HELP}");
        return;
    }

    let args = match Args::from_process_env() {
        Ok(args) => args,
        Err(error) => {
            eprintln!("{error}\n\n{HELP}");
            std::process::exit(2);
        }
    };

    let config = load_config();

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

    let check_updates = match update::enabled(&config) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };

    if args.print {
        let engine = match LlamaCppEngine::load(
            model_path.to_string_lossy().as_ref(),
            prompt_style,
            DEFAULT_N_CTX,
        ) {
            Ok(engine) => Arc::new(engine),
            Err(error) => {
                eprintln!("failed to load {}: {error}", model_path.display());
                std::process::exit(1);
            }
        };

        let text = if args.stdin {
            read_stdin().unwrap_or_default()
        } else {
            capture::capture_selection().unwrap_or_default()
        };

        std::process::exit(run_headless(&engine, &args, &text));
    }

    let _instance = match single_instance::acquire() {
        Some(guard) => guard,
        None => {
            single_instance::notify_existing_instance();
            return;
        }
    };

    let startup = if model_path.is_file() {
        app::Startup::Loaded(
            LlamaCppEngine::load(
                model_path.to_string_lossy().as_ref(),
                prompt_style,
                DEFAULT_N_CTX,
            )
            .map(Arc::new),
        )
    } else if auto_download {
        app::Startup::Download {
            dest: model_path.clone(),
            url: models::DEFAULT_MODEL_URL.to_string(),
            sha256: models::DEFAULT_MODEL_SHA256.to_string(),
            prompt_style,
        }
    } else {
        app::Startup::Loaded(Err(format!(
            "模型文件不存在：{}（可设置 model_path，或开启 auto_download）",
            model_path.display()
        )))
    };

    let serve_extension = match config.serve_extension.as_deref() {
        Some(value) => match parse_bool("serve_extension", value) {
            Ok(value) => value,
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        },
        None => true,
    };

    let server_plan = if serve_extension {
        Some(app::ServerPlan {
            bind_addr: bind_addr_from_service_url(&args.service_url)
                .unwrap_or_else(|| DEFAULT_BIND_ADDR.to_string()),
            model_name: model_path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default(),
        })
    } else {
        None
    };

    let hotkey_spec = config
        .hotkey
        .or_else(|| std::env::var("TRANSLATOR_HOTKEY").ok())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "Ctrl+Alt+T".to_string());

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(format!(
                "OpenTranslator ({} → {})",
                translator_core::languages::source_label(&args.source),
                translator_core::languages::label(&args.target)
            ))
            .with_inner_size(app::WINDOW_SIZE)
            .with_decorations(false)
            // wgpu's DX12 backend only offers an opaque swapchain on Win32
            // HWNDs, so a "transparent" window shows black margins there.
            // Windows uses an opaque window with DWM-rounded corners instead.
            .with_transparent(!cfg!(target_os = "windows"))
            .with_has_shadow(false)
            .with_resizable(false)
            .with_visible(false),
        ..Default::default()
    };

    if let Err(error) = eframe::run_native(
        "OpenTranslator",
        options,
        Box::new(move |cc| {
            Ok(Box::new(app::PopupApp::new(
                cc,
                args,
                &hotkey_spec,
                startup,
                server_plan,
                check_updates,
            )))
        }),
    ) {
        eprintln!("failed to start UI: {error}");
        std::process::exit(1);
    }
}
