use std::env;
use std::time::Duration;

use crate::domain::prompt::PromptStyle;
use crate::engine::EngineKind;

pub const DEFAULT_BIND_ADDR: &str = "127.0.0.1:17890";
pub const DEFAULT_MODEL_URL: &str = "http://127.0.0.1:11434";
pub const DEFAULT_TIMEOUT_MS: u64 = 30_000;
pub const DEFAULT_KEEP_ALIVE: &str = "30m";

#[derive(Debug, Clone)]
pub struct Config {
    pub bind_addr: String,
    pub engine: EngineKind,
    pub timeout: Duration,
    pub model_url: String,
    pub model: String,
    pub prompt_style: PromptStyle,
    pub warmup: bool,
    pub keep_alive: String,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        let bind_addr =
            env::var("TRANSLATOR_BIND_ADDR").unwrap_or_else(|_| DEFAULT_BIND_ADDR.to_string());

        let engine = match env::var("TRANSLATOR_ENGINE") {
            Ok(value) => EngineKind::parse(&value)?,
            Err(_) => EngineKind::Mock,
        };

        let timeout = match env::var("TRANSLATOR_TIMEOUT_MS") {
            Ok(value) => parse_timeout_ms(&value)?,
            Err(_) => Duration::from_millis(DEFAULT_TIMEOUT_MS),
        };

        let model_url = resolve_model_url(env::var("TRANSLATOR_MODEL_URL").ok())?;
        let model = resolve_model(engine, env::var("TRANSLATOR_MODEL").ok())?;

        let prompt_style = match env::var("TRANSLATOR_PROMPT_STYLE") {
            Ok(value) => PromptStyle::parse(&value)?,
            Err(_) => PromptStyle::Generic,
        };

        let warmup = match env::var("TRANSLATOR_WARMUP") {
            Ok(value) => parse_bool(&value)?,
            Err(_) => true,
        };

        let keep_alive = resolve_keep_alive(env::var("TRANSLATOR_KEEP_ALIVE").ok())?;

        Ok(Self {
            bind_addr,
            engine,
            timeout,
            model_url,
            model,
            prompt_style,
            warmup,
            keep_alive,
        })
    }
}

fn parse_timeout_ms(value: &str) -> Result<Duration, String> {
    let milliseconds = value
        .parse::<u64>()
        .map_err(|_| format!("invalid TRANSLATOR_TIMEOUT_MS: {value}"))?;

    if milliseconds == 0 {
        return Err("TRANSLATOR_TIMEOUT_MS must be greater than 0".to_string());
    }

    Ok(Duration::from_millis(milliseconds))
}

fn parse_bool(value: &str) -> Result<bool, String> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(format!("invalid TRANSLATOR_WARMUP: {other}")),
    }
}

fn resolve_keep_alive(value: Option<String>) -> Result<String, String> {
    let keep_alive = value.unwrap_or_else(|| DEFAULT_KEEP_ALIVE.to_string());
    let keep_alive = keep_alive.trim().to_string();

    if keep_alive.is_empty() {
        return Err("TRANSLATOR_KEEP_ALIVE must not be empty".to_string());
    }

    Ok(keep_alive)
}

fn resolve_model_url(value: Option<String>) -> Result<String, String> {
    let url = value.unwrap_or_else(|| DEFAULT_MODEL_URL.to_string());

    if url.starts_with("http://") || url.starts_with("https://") {
        Ok(url.trim_end_matches('/').to_string())
    } else {
        Err(format!("invalid TRANSLATOR_MODEL_URL: {url}"))
    }
}

fn resolve_model(engine: EngineKind, value: Option<String>) -> Result<String, String> {
    let model = value.unwrap_or_default().trim().to_string();

    if engine == EngineKind::Ollama && model.is_empty() {
        return Err("TRANSLATOR_MODEL is required when TRANSLATOR_ENGINE=ollama".to_string());
    }

    Ok(model)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_timeout_ms() {
        assert_eq!(
            parse_timeout_ms("1500").unwrap(),
            Duration::from_millis(1500)
        );
    }

    #[test]
    fn rejects_invalid_timeout() {
        assert!(parse_timeout_ms("abc").is_err());
        assert!(parse_timeout_ms("0").is_err());
    }

    #[test]
    fn parses_warmup_flag() {
        assert!(parse_bool("true").unwrap());
        assert!(!parse_bool("false").unwrap());
        assert!(parse_bool("yes").is_err());
    }

    #[test]
    fn resolves_keep_alive() {
        assert_eq!(resolve_keep_alive(None).unwrap(), DEFAULT_KEEP_ALIVE);
        assert_eq!(resolve_keep_alive(Some(" 1h ".to_string())).unwrap(), "1h");
        assert!(resolve_keep_alive(Some("".to_string())).is_err());
    }

    #[test]
    fn resolves_model_url_with_default_and_validation() {
        assert_eq!(resolve_model_url(None).unwrap(), DEFAULT_MODEL_URL);
        assert_eq!(
            resolve_model_url(Some("http://localhost:1234/".to_string())).unwrap(),
            "http://localhost:1234"
        );
        assert!(resolve_model_url(Some("localhost:1234".to_string())).is_err());
    }

    #[test]
    fn requires_model_for_ollama_engine() {
        assert_eq!(resolve_model(EngineKind::Mock, None).unwrap(), "");
        assert!(resolve_model(EngineKind::Ollama, None).is_err());
        assert_eq!(
            resolve_model(EngineKind::Ollama, Some(" qwen2.5:7b ".to_string())).unwrap(),
            "qwen2.5:7b"
        );
    }
}
