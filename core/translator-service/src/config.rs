use std::env;

use crate::engine::EngineKind;

pub const DEFAULT_BIND_ADDR: &str = "127.0.0.1:17890";

#[derive(Debug, Clone)]
pub struct Config {
    pub bind_addr: String,
    pub engine: EngineKind,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        let bind_addr =
            env::var("TRANSLATOR_BIND_ADDR").unwrap_or_else(|_| DEFAULT_BIND_ADDR.to_string());

        let engine = match env::var("TRANSLATOR_ENGINE") {
            Ok(value) => EngineKind::parse(&value)?,
            Err(_) => EngineKind::Mock,
        };

        Ok(Self { bind_addr, engine })
    }
}
