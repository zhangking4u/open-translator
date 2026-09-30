use std::io::Read;
use std::process::Command;

mod services;
mod translate;
mod ui;

const DEFAULT_SERVICE_URL: &str = "http://127.0.0.1:17890";

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

Auto-start configuration (environment):
  TRANSLATOR_CORE_BIN    Path to the translator-service binary (default: derived
                         from the popup location, core/translator-service/target/release)
  TRANSLATOR_OLLAMA_BIN  Path to the ollama binary (default: ~/.local/opt/ollama/bin/ollama)
  TRANSLATOR_MODEL       Model for the started service (default: hy-mt1.5-1.8b)
  TRANSLATOR_PROMPT_STYLE  Prompt style (default: hymt)

Selection reading uses wl-paste from the wl-clipboard package
(Debian/Ubuntu: sudo apt install wl-clipboard).
";

#[derive(Debug, Clone, PartialEq)]
struct Args {
    source: String,
    target: String,
    clipboard: bool,
    stdin: bool,
    print: bool,
    no_start: bool,
    service_url: String,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            source: "en".to_string(),
            target: "zh".to_string(),
            clipboard: false,
            stdin: false,
            print: false,
            no_start: false,
            service_url: std::env::var("TRANSLATOR_SERVICE_URL")
                .unwrap_or_else(|_| DEFAULT_SERVICE_URL.to_string()),
        }
    }
}

fn parse_args(args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut parsed = Args::default();

    let mut args = args.peekable();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--source" | "-s" => {
                parsed.source = args
                    .next()
                    .ok_or_else(|| "--source requires a value".to_string())?;
            }
            "--target" | "-t" => {
                parsed.target = args
                    .next()
                    .ok_or_else(|| "--target requires a value".to_string())?;
            }
            "--service" => {
                parsed.service_url = args
                    .next()
                    .ok_or_else(|| "--service requires a value".to_string())?;
            }
            "--clipboard" => parsed.clipboard = true,
            "--stdin" => parsed.stdin = true,
            "--print" => parsed.print = true,
            "--no-start" => parsed.no_start = true,
            other => return Err(format!("unknown argument: {other}")),
        }
    }

    Ok(parsed)
}

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

pub(crate) fn read_stdin() -> Result<String, String> {
    let mut text = String::new();
    std::io::stdin()
        .read_to_string(&mut text)
        .map_err(|error| format!("failed to read stdin: {error}"))?;
    Ok(text.trim().to_string())
}

async fn run_headless(args: &Args, text: &str) -> i32 {
    let client = match translate::build_client() {
        Ok(client) => client,
        Err(error) => {
            eprintln!("{error}");
            return 1;
        }
    };

    let config = services::ServiceConfig::from_env(&args.service_url, !args.no_start);
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

    let args = match parse_args(std::env::args().skip(1)) {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Result<Args, String> {
        parse_args(values.iter().map(|value| value.to_string()))
    }

    #[test]
    fn uses_defaults() {
        let parsed = args(&[]).unwrap();
        let expected = Args::default();

        assert_eq!(parsed, expected);
    }

    #[test]
    fn parses_flags() {
        let parsed = args(&[
            "--source",
            "zh",
            "-t",
            "en",
            "--clipboard",
            "--stdin",
            "--print",
            "--no-start",
            "--service",
            "http://127.0.0.1:9999/",
        ])
        .unwrap();

        assert_eq!(parsed.source, "zh");
        assert_eq!(parsed.target, "en");
        assert!(parsed.clipboard);
        assert!(parsed.stdin);
        assert!(parsed.print);
        assert!(parsed.no_start);
        assert_eq!(parsed.service_url, "http://127.0.0.1:9999/");
    }

    #[test]
    fn rejects_unknown_arguments() {
        assert!(args(&["--nope"]).is_err());
    }

    #[test]
    fn rejects_missing_values() {
        assert!(args(&["--source"]).is_err());
    }
}
