use std::io::Read;
use std::process::{Command, Stdio};
use std::time::Duration;

use serde::{Deserialize, Serialize};

mod services;

const DEFAULT_SERVICE_URL: &str = "http://127.0.0.1:17890";

const HELP: &str = "\
Usage: translator-popup [OPTIONS]

Reads the Wayland primary selection (or clipboard), sends it to the local
translator service and shows the translation. Starts ollama and the
translator service automatically when they are not running.

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

#[derive(Debug, PartialEq)]
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

fn read_selection(clipboard: bool) -> Result<String, String> {
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

fn read_stdin() -> Result<String, String> {
    let mut text = String::new();
    std::io::stdin()
        .read_to_string(&mut text)
        .map_err(|error| format!("failed to read stdin: {error}"))?;
    Ok(text.trim().to_string())
}

#[derive(Serialize)]
struct TranslateRequest<'a> {
    text: &'a str,
    source: &'a str,
    target: &'a str,
}

#[derive(Deserialize)]
struct TranslateResponse {
    translation: String,
}

#[derive(Deserialize)]
struct ErrorResponse {
    error: ErrorBody,
}

#[derive(Deserialize)]
struct ErrorBody {
    kind: String,
    message: String,
}

async fn translate(client: &reqwest::Client, args: &Args, text: &str) -> Result<String, String> {
    let url = format!("{}/translate", args.service_url.trim_end_matches('/'));

    let response = client
        .post(&url)
        .json(&TranslateRequest {
            text,
            source: &args.source,
            target: &args.target,
        })
        .send()
        .await
        .map_err(|error| format!("cannot reach translator service at {url}: {error}"))?;

    let status = response.status();

    if status.is_success() {
        let payload: TranslateResponse = response
            .json()
            .await
            .map_err(|error| format!("invalid service response: {error}"))?;
        return Ok(payload.translation);
    }

    let body = response.text().await.unwrap_or_default();

    if let Ok(error) = serde_json::from_str::<ErrorResponse>(&body) {
        return Err(format!("{}: {}", error.error.kind, error.error.message));
    }

    Err(format!("service returned {status}: {body}"))
}

fn show_popup(title: &str, text: &str) {
    let spawned = Command::new("zenity")
        .args([
            "--text-info",
            "--title",
            title,
            "--width",
            "560",
            "--height",
            "220",
        ])
        .stdin(Stdio::piped())
        .spawn();

    match spawned {
        Ok(mut child) => {
            if let Some(mut stdin) = child.stdin.take() {
                use std::io::Write;
                let _ = stdin.write_all(text.as_bytes());
            }
            let _ = child.wait();
        }
        Err(_) => {
            println!("{text}");
            let _ = Command::new("notify-send").args([title, text]).status();
        }
    }
}

fn show_error(message: &str) {
    let _ = Command::new("zenity")
        .args([
            "--error",
            "--title",
            "OpenTranslator",
            "--text",
            message,
            "--width",
            "480",
        ])
        .status();
}

fn fail(args: &Args, message: &str) -> ! {
    if args.print {
        eprintln!("{message}");
    } else {
        show_error(message);
    }
    std::process::exit(1);
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
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

    let text = if args.stdin {
        match read_stdin() {
            Ok(text) => text,
            Err(error) => fail(&args, &error),
        }
    } else {
        match read_selection(args.clipboard) {
            Ok(text) => text,
            Err(error) => fail(&args, &error),
        }
    };

    if text.is_empty() {
        fail(&args, "No selected text found.");
    }

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
    {
        Ok(client) => client,
        Err(error) => fail(&args, &format!("failed to create HTTP client: {error}")),
    };

    let service_config = services::ServiceConfig::from_env(&args.service_url, !args.no_start);
    if let Err(error) = services::ensure(&client, &service_config).await {
        fail(&args, &error);
    }

    match translate(&client, &args, &text).await {
        Ok(translation) => {
            if args.print {
                println!("{translation}");
            } else {
                let title = format!("OpenTranslator ({} → {})", args.source, args.target);
                show_popup(&title, &translation);
            }
        }
        Err(error) => fail(&args, &error),
    }
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
