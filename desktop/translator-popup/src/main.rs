use std::io::Read;
use std::path::{Path, PathBuf};
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

Config file (~/.config/open-translator/config) is applied when no CLI flag
is given; CLI > config file > environment > defaults:

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

#[derive(Debug, Default, PartialEq)]
struct CliArgs {
    source: Option<String>,
    target: Option<String>,
    service_url: Option<String>,
    clipboard: bool,
    stdin: bool,
    print: bool,
    no_start: bool,
}

#[derive(Debug, Default, PartialEq)]
struct FileConfig {
    service_url: Option<String>,
    source: Option<String>,
    target: Option<String>,
}

impl Args {
    fn resolve(cli: CliArgs, file: FileConfig) -> Self {
        let mut args = Args::default();

        if let Some(value) = file.service_url {
            args.service_url = value;
        }
        if let Some(value) = file.source {
            args.source = value;
        }
        if let Some(value) = file.target {
            args.target = value;
        }

        if let Some(value) = cli.service_url {
            args.service_url = value;
        }
        if let Some(value) = cli.source {
            args.source = value;
        }
        if let Some(value) = cli.target {
            args.target = value;
        }

        args.clipboard = cli.clipboard;
        args.stdin = cli.stdin;
        args.print = cli.print;
        args.no_start = cli.no_start;

        args
    }
}

fn parse_args(args: impl Iterator<Item = String>) -> Result<CliArgs, String> {
    let mut parsed = CliArgs::default();

    let mut args = args.peekable();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--source" | "-s" => {
                parsed.source =
                    Some(args.next().ok_or_else(|| "--source requires a value".to_string())?);
            }
            "--target" | "-t" => {
                parsed.target =
                    Some(args.next().ok_or_else(|| "--target requires a value".to_string())?);
            }
            "--service" => {
                parsed.service_url = Some(
                    args.next()
                        .ok_or_else(|| "--service requires a value".to_string())?,
                );
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

fn config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;

    Some(base.join("open-translator").join("config"))
}

fn load_file_config(path: &Path) -> FileConfig {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return FileConfig::default();
    };

    let mut config = FileConfig::default();

    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };

        let key = key.trim();
        let value = value.trim().trim_matches('"');

        if value.is_empty() {
            continue;
        }

        match key {
            "service_url" => config.service_url = Some(value.to_string()),
            "source" => config.source = Some(value.to_string()),
            "target" => config.target = Some(value.to_string()),
            _ => {}
        }
    }

    config
}

pub(crate) fn persist_target(target: &str) {
    let Some(path) = config_path() else {
        return;
    };

    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }

    let contents = std::fs::read_to_string(&path).unwrap_or_default();
    let _ = std::fs::write(&path, apply_target(&contents, target));
}

fn apply_target(contents: &str, target: &str) -> String {
    let mut lines: Vec<String> = contents.lines().map(|line| line.to_string()).collect();
    let mut replaced = false;

    for line in &mut lines {
        let trimmed = line.trim_start();
        if trimmed.starts_with("target") && trimmed.contains('=') {
            *line = format!("target = {target}");
            replaced = true;
        }
    }

    if !replaced {
        lines.push(format!("target = {target}"));
    }

    let mut output = lines.join("\n");
    output.push('\n');
    output
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

    let cli = match parse_args(std::env::args().skip(1)) {
        Ok(cli) => cli,
        Err(error) => {
            eprintln!("{error}\n\n{HELP}");
            std::process::exit(2);
        }
    };

    let file_config = config_path()
        .map(|path| load_file_config(&path))
        .unwrap_or_default();
    let args = Args::resolve(cli, file_config);

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

    fn cli(values: &[&str]) -> Result<CliArgs, String> {
        parse_args(values.iter().map(|value| value.to_string()))
    }

    #[test]
    fn resolves_defaults() {
        let args = Args::resolve(CliArgs::default(), FileConfig::default());

        assert_eq!(args, Args::default());
    }

    #[test]
    fn config_file_fills_missing_values() {
        let file = FileConfig {
            service_url: Some("http://127.0.0.1:9999".to_string()),
            source: Some("fr".to_string()),
            target: Some("de".to_string()),
        };

        let args = Args::resolve(CliArgs::default(), file);

        assert_eq!(args.service_url, "http://127.0.0.1:9999");
        assert_eq!(args.source, "fr");
        assert_eq!(args.target, "de");
    }

    #[test]
    fn cli_overrides_config_file() {
        let cli = CliArgs {
            source: Some("ja".to_string()),
            print: true,
            ..CliArgs::default()
        };
        let file = FileConfig {
            service_url: None,
            source: Some("fr".to_string()),
            target: Some("de".to_string()),
        };

        let args = Args::resolve(cli, file);

        assert_eq!(args.source, "ja");
        assert_eq!(args.target, "de");
        assert!(args.print);
    }

    #[test]
    fn parses_flags() {
        let parsed = cli(&[
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

        assert_eq!(parsed.source.as_deref(), Some("zh"));
        assert_eq!(parsed.target.as_deref(), Some("en"));
        assert!(parsed.clipboard);
        assert!(parsed.stdin);
        assert!(parsed.print);
        assert!(parsed.no_start);
        assert_eq!(parsed.service_url.as_deref(), Some("http://127.0.0.1:9999/"));
    }

    #[test]
    fn rejects_unknown_arguments() {
        assert!(cli(&["--nope"]).is_err());
    }

    #[test]
    fn rejects_missing_values() {
        assert!(cli(&["--source"]).is_err());
    }

    #[test]
    fn parses_config_file() {
        let path = std::env::temp_dir().join(format!(
            "open-translator-config-test-{}.conf",
            std::process::id()
        ));

        std::fs::write(
            &path,
            "# comment\nsource = ja\n\ntarget=ko\nservice_url = \"http://127.0.0.1:1\"\nunknown = x\n",
        )
        .unwrap();

        let config = load_file_config(&path);
        let _ = std::fs::remove_file(&path);

        assert_eq!(config.source.as_deref(), Some("ja"));
        assert_eq!(config.target.as_deref(), Some("ko"));
        assert_eq!(config.service_url.as_deref(), Some("http://127.0.0.1:1"));
    }

    #[test]
    fn missing_config_file_is_empty() {
        let config = load_file_config(Path::new("/nonexistent/open-translator/config"));

        assert_eq!(config, FileConfig::default());
    }

    #[test]
    fn apply_target_replaces_existing_key() {
        let updated = apply_target("source = en\ntarget = zh\n", "ja");

        assert!(updated.contains("target = ja"));
        assert!(!updated.contains("target = zh"));
    }

    #[test]
    fn apply_target_appends_when_missing() {
        let updated = apply_target("# comment\nsource = en\n", "ko");

        assert!(updated.ends_with("target = ko\n"));
    }
}
