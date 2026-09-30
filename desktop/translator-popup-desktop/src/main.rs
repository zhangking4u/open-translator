use std::path::PathBuf;
use std::sync::Arc;

use translator_core::args::{Args, read_stdin};
use translator_core::paths::default_model_path;
use translator_core::services::bind_addr_from_service_url;
use translator_core::settings::{FileConfig, load_config};
use translator_service::domain::prompt::PromptStyle;
use translator_service::domain::translation::TranslationRequest;
use translator_service::engine::llama_cpp::LlamaCppEngine;

mod app;
mod capture;
mod hotkey;
mod server;
mod tray;

const DEFAULT_N_CTX: u32 = 4096;
const DEFAULT_BIND_ADDR: &str = "127.0.0.1:17890";

const HELP: &str = "\
Usage: translator-popup-desktop [OPTIONS]

Desktop popup for OpenTranslator (Windows / macOS): select text and press
Ctrl+Alt+T to translate it with the embedded model. The app stays resident;
Esc hides the window, 退出 quits. While running it also serves the local HTTP
API on service_url for the browser extension.

Options:
  -s, --source <LANG>   Source language tag (default: en)
  -t, --target <LANG>   Target language tag (default: zh)
      --stdin           Use stdin instead of the current selection (testing)
      --print           Print the translation and exit (debug builds; Windows
                        release builds are GUI-only without a console)
      --service <URL>   Also used as the bind address for the extension API
                        (default: http://127.0.0.1:17890)
  -h, --help            Show this help

Config file (Windows: %APPDATA%\\open-translator\\config, macOS:
~/Library/Application Support/open-translator/config):

  service_url = http://127.0.0.1:17890
  source = en
  target = zh
  hotkey = Ctrl+Alt+T
  model_path = <path to a .gguf model>      (default: per-user models dir)
  prompt_style = hymt                       (generic / translategemma / hymt)
  serve_extension = true                    (disable to skip the HTTP endpoint)

Environment overrides: TRANSLATOR_HOTKEY, TRANSLATOR_MODEL_PATH,
TRANSLATOR_PROMPT_STYLE. Default model location:
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

    let request = TranslationRequest {
        text: text.to_string(),
        source: args.source.clone(),
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

    let engine = LlamaCppEngine::load(
        model_path.to_string_lossy().as_ref(),
        prompt_style,
        DEFAULT_N_CTX,
    )
    .map(Arc::new);

    if args.print {
        let engine = match &engine {
            Ok(engine) => engine.clone(),
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

    if serve_extension {
        if let Ok(engine) = &engine {
            let bind_addr = bind_addr_from_service_url(&args.service_url)
                .unwrap_or_else(|| DEFAULT_BIND_ADDR.to_string());
            let model_name = model_path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default();

            if let Err(error) = server::start(engine.clone(), bind_addr, model_name) {
                eprintln!("extension server disabled: {error}");
            }
        }
    }

    let hotkey_spec = config
        .hotkey
        .or_else(|| std::env::var("TRANSLATOR_HOTKEY").ok())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "Ctrl+Alt+T".to_string());

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(format!("OpenTranslator ({} → {})", args.source, args.target))
            .with_inner_size([560.0, 320.0])
            .with_visible(false),
        ..Default::default()
    };

    if let Err(error) = eframe::run_native(
        "OpenTranslator",
        options,
        Box::new(move |cc| Ok(Box::new(app::PopupApp::new(cc, args, &hotkey_spec, engine)))),
    ) {
        eprintln!("failed to start UI: {error}");
        std::process::exit(1);
    }
}
