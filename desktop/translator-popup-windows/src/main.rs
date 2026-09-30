#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod capture;
mod hotkey;

use translator_core::args::{Args, read_stdin};
use translator_core::services::{self, ServiceConfig};
use translator_core::translate;

const HELP: &str = "\
Usage: translator-popup-windows [OPTIONS]

Windows desktop popup for OpenTranslator: select text and press Ctrl+Alt+T to
translate it with the local service. The app stays resident; Esc hides the
window, 退出 quits.

Options:
  -s, --source <LANG>   Source language tag (default: en)
  -t, --target <LANG>   Target language tag (default: zh)
      --stdin           Use stdin instead of the current selection (testing)
      --print           Print the translation and exit (debug builds; release
                        builds are GUI-only without a console)
      --service <URL>   Service base URL (default: http://127.0.0.1:17890)
      --no-start        Do not auto-start services
  -h, --help            Show this help

Config file (Windows: %APPDATA%\\open-translator\\config) is applied when no
CLI flag is given; CLI > config file > environment > defaults.

Auto-start environment: TRANSLATOR_CORE_BIN, TRANSLATOR_OLLAMA_BIN,
TRANSLATOR_MODEL, TRANSLATOR_PROMPT_STYLE (same as the Linux popup).

The hotkey sends Ctrl+C to the focused window and reads the clipboard, so the
selection must come from an application that supports Ctrl+C.
";

async fn run_headless(args: &Args, text: &str) -> i32 {
    if text.is_empty() {
        eprintln!("no selected text found");
        return 1;
    }

    let client = match translate::build_client() {
        Ok(client) => client,
        Err(error) => {
            eprintln!("{error}");
            return 1;
        }
    };

    let config = ServiceConfig::from_env(&args.service_url, !args.no_start);
    if let Err(error) = services::ensure(&client, &config).await {
        eprintln!("{error}");
        return 1;
    }

    match translate::translate(
        &client,
        &args.service_url,
        &args.source,
        &args.target,
        text,
    )
    .await
    {
        Ok(translation) => {
            println!("{translation}");
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

    if args.print {
        let text = if args.stdin {
            read_stdin().unwrap_or_default()
        } else {
            capture::capture_selection().unwrap_or_default()
        };

        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                eprintln!("failed to start runtime: {error}");
                std::process::exit(1);
            }
        };

        std::process::exit(runtime.block_on(run_headless(&args, &text)));
    }

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
        Box::new(move |cc| Ok(Box::new(app::PopupApp::new(cc, args)))),
    ) {
        eprintln!("failed to start UI: {error}");
        std::process::exit(1);
    }
}
