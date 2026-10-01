use std::io::Read;

use crate::paths::config_path;
use crate::settings::{FileConfig, load_file_config};

pub const DEFAULT_SERVICE_URL: &str = "http://127.0.0.1:17890";

#[derive(Debug, Clone, PartialEq)]
pub struct Args {
    pub source: String,
    pub target: String,
    pub recent_targets: Vec<String>,
    pub clipboard: bool,
    pub stdin: bool,
    pub print: bool,
    pub no_start: bool,
    pub autostart: bool,
    pub service_url: String,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            source: crate::languages::AUTO_CODE.to_string(),
            target: "zh".to_string(),
            recent_targets: Vec::new(),
            clipboard: false,
            stdin: false,
            print: false,
            no_start: false,
            autostart: false,
            service_url: std::env::var("TRANSLATOR_SERVICE_URL")
                .unwrap_or_else(|_| DEFAULT_SERVICE_URL.to_string()),
        }
    }
}

#[derive(Debug, Default, PartialEq)]
pub struct CliArgs {
    pub source: Option<String>,
    pub target: Option<String>,
    pub service_url: Option<String>,
    pub clipboard: bool,
    pub stdin: bool,
    pub print: bool,
    pub no_start: bool,
    pub autostart: bool,
}

impl Args {
    pub fn resolve(cli: CliArgs, file: FileConfig) -> Self {
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
        if let Some(value) = file.recent_targets {
            args.recent_targets = value;
        }
        if let Some(value) = file.clipboard.as_deref() {
            args.clipboard = value.eq_ignore_ascii_case("true");
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

        if cli.clipboard {
            args.clipboard = true;
        }

        args.stdin = cli.stdin;
        args.print = cli.print;
        args.no_start = cli.no_start;
        args.autostart = cli.autostart;

        args
    }

    pub fn from_process_env() -> Result<Self, String> {
        let cli = parse_args(std::env::args().skip(1))?;
        let file_config = config_path()
            .map(|path| load_file_config(&path))
            .unwrap_or_default();

        Ok(Args::resolve(cli, file_config))
    }
}

pub fn parse_args(args: impl Iterator<Item = String>) -> Result<CliArgs, String> {
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
            "--autostart" => parsed.autostart = true,
            other => return Err(format!("unknown argument: {other}")),
        }
    }

    Ok(parsed)
}

pub fn read_stdin() -> Result<String, String> {
    let mut text = String::new();
    std::io::stdin()
        .read_to_string(&mut text)
        .map_err(|error| format!("读取标准输入失败：{error}"))?;
    Ok(text.trim().to_string())
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
        assert_eq!(args.source, "auto");
        assert_eq!(args.target, "zh");
    }

    #[test]
    fn config_file_fills_missing_values() {
        let file = FileConfig {
            service_url: Some("http://127.0.0.1:9999".to_string()),
            source: Some("fr".to_string()),
            target: Some("de".to_string()),
            recent_targets: Some(vec!["zh".to_string(), "en".to_string()]),
            ..FileConfig::default()
        };

        let args = Args::resolve(CliArgs::default(), file);

        assert_eq!(args.service_url, "http://127.0.0.1:9999");
        assert_eq!(args.source, "fr");
        assert_eq!(args.target, "de");
        assert_eq!(args.recent_targets, vec!["zh".to_string(), "en".to_string()]);
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
            ..FileConfig::default()
        };

        let args = Args::resolve(cli, file);

        assert_eq!(args.source, "ja");
        assert_eq!(args.target, "de");
        assert!(args.print);
    }

    #[test]
    fn config_file_controls_clipboard_mode() {
        let file = FileConfig {
            clipboard: Some("true".to_string()),
            ..FileConfig::default()
        };

        let args = Args::resolve(CliArgs::default(), file);
        assert!(args.clipboard);

        let file = FileConfig {
            clipboard: Some("false".to_string()),
            ..FileConfig::default()
        };

        let args = Args::resolve(CliArgs::default(), file);
        assert!(!args.clipboard);
    }

    #[test]
    fn clipboard_flag_overrides_the_config_file() {
        let cli = CliArgs {
            clipboard: true,
            ..CliArgs::default()
        };
        let file = FileConfig {
            clipboard: Some("false".to_string()),
            ..FileConfig::default()
        };

        let args = Args::resolve(cli, file);
        assert!(args.clipboard);
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
            "--autostart",
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
        assert!(parsed.autostart);
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
}
