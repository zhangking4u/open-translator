use std::process::Command;

use translator_core::args::{Args, read_stdin};
use translator_core::services::{self, ServiceConfig};
use translator_core::translate;

mod ui;

const HELP: &str = "\
Usage: translator-popup [OPTIONS]

Reads the Wayland primary selection (or clipboard), sends it to the local
translator service and shows the translation in a popup window. Starts ollama
and the translator service automatically when they are not running.

Options:
  -s, --source <LANG>   Source language tag (default: en)
  -t, --target <LANG>   Target language tag (default: zh)
      --clipboard       Read the clipboard instead of the primary selection
      --stdin           Read text from stdin instead of the selection
      --print           Print the translation to stdout instead of a popup
      --service <URL>   Service base URL
                        (default: $TRANSLATOR_SERVICE_URL or http://127.0.0.1:17890)
      --no-start        Do not auto-start services
  -h, --help            Show this help

Config file (Linux: ~/.config/open-translator/config) is applied when no CLI
flag is given; CLI > config file > environment > defaults:

  service_url = http://127.0.0.1:17890
  source = en
  target = zh

Auto-start configuration (environment):
  TRANSLATOR_CORE_BIN    Path to the translator-service binary (default: derived
                         from the popup location, core/translator-service/target/release)
  TRANSLATOR_OLLAMA_BIN  Path to the ollama binary (default: ~/.local/opt/ollama/bin/ollama)
  TRANSLATOR_MODEL       Model for the started service (default: hy-mt1.5-1.8b)
  TRANSLATOR_PROMPT_STYLE  Prompt style (default: hymt)

Selection reading uses wl-paste from the wl-clipboard package
(Debian/Ubuntu: sudo apt install wl-clipboard).
";

pub(crate) fn read_selection(clipboard: bool) -> Result<String, String> {
    let mut command = Command::new("wl-paste");
    command.arg("--no-newline");

    if !clipboard {
        command.arg("--primary");
    }

    let output = command.output().map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => {
            "wl-paste not found; install the wl-clipboard package (sudo apt install wl-clipboard)"
                .to_string()
        }
        _ => format!("failed to run wl-paste: {error}"),
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("wl-paste failed: {}", stderr.trim()));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

async fn run_headless(args: &Args, text: &str) -> i32 {
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
        let text = match if args.stdin {
            read_stdin()
        } else {
            read_selection(args.clipboard)
        } {
            Ok(text) if !text.is_empty() => text,
            Ok(_) => {
                eprintln!("No selected text found.");
                std::process::exit(1);
            }
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
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

    std::process::exit(ui::run(args));
}
