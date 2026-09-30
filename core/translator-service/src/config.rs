use std::env;
use std::time::Duration;

use crate::engine::EngineKind;

pub const DEFAULT_BIND_ADDR: &str = "127.0.0.1:17890";
pub const DEFAULT_TIMEOUT_MS: u64 = 30_000;

#[derive(Debug, Clone)]
pub struct Config {
    pub bind_addr: String,
    pub engine: EngineKind,
    pub timeout: Duration,
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

        Ok(Self {
            bind_addr,
            engine,
            timeout,
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
}
