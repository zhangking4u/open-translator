use std::env;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::paths::{default_core_bin, default_ollama_bin, log_path};

const DEFAULT_MODEL_URL: &str = "http://127.0.0.1:11434";
const DEFAULT_ENGINE: &str = "ollama";
const DEFAULT_MODEL: &str = "hy-mt1.5-1.8b";
const DEFAULT_PROMPT_STYLE: &str = "hymt";

const HEALTH_TIMEOUT: Duration = Duration::from_millis(1000);
const POLL_INTERVAL: Duration = Duration::from_millis(500);
const OLLAMA_POLL_ATTEMPTS: usize = 40;
const CORE_POLL_ATTEMPTS: usize = 120;

#[derive(Debug, Clone)]
pub struct ServiceConfig {
    pub service_url: String,
    pub model_url: String,
    pub core_bin: Option<PathBuf>,
    pub ollama_bin: Option<PathBuf>,
    pub start: bool,
}

impl ServiceConfig {
    pub fn from_env(service_url: &str, start: bool) -> Self {
        let core_bin = env::var_os("TRANSLATOR_CORE_BIN")
            .map(PathBuf::from)
            .or_else(default_core_bin);

        let ollama_bin = env::var_os("TRANSLATOR_OLLAMA_BIN")
            .map(PathBuf::from)
            .or_else(default_ollama_bin);

        Self {
            service_url: service_url.to_string(),
            model_url: env_or("TRANSLATOR_MODEL_URL", DEFAULT_MODEL_URL),
            core_bin,
            ollama_bin,
            start,
        }
    }
}

pub async fn ensure(client: &reqwest::Client, config: &ServiceConfig) -> Result<(), String> {
    if service_ok(client, &config.service_url).await {
        return Ok(());
    }

    if !config.start {
        return Err(not_running_hint(config));
    }

    if !is_local_url(&config.service_url) {
        return Err(format!(
            "{} is not reachable (remote services are not auto-started)",
            config.service_url
        ));
    }

    if is_local_url(&config.model_url) && !ollama_ok(client, &config.model_url).await {
        let Some(bin) = &config.ollama_bin else {
            return Err(
                "ollama is not running and no binary was found; start it manually or set \
                 TRANSLATOR_OLLAMA_BIN"
                    .to_string(),
            );
        };

        spawn_detached(bin, &["serve"], &[], "ollama.log")?;

        if !wait_until(|| ollama_ok(client, &config.model_url), OLLAMA_POLL_ATTEMPTS).await {
            return Err(format!(
                "ollama did not become ready in time; see {}",
                log_path("ollama.log").display()
            ));
        }
    }

    let Some(bin) = &config.core_bin else {
        return Err(
            "translator service is not running and no core binary was found; build it with \
             `cargo build --release` in core/translator-service or set TRANSLATOR_CORE_BIN"
                .to_string(),
        );
    };

    spawn_detached(bin, &[], &core_env(&config.service_url), "translator-service.log")?;

    if !wait_until(
        || service_ok(client, &config.service_url),
        CORE_POLL_ATTEMPTS,
    )
    .await
    {
        return Err(format!(
            "translator service did not become ready in time; see {}",
            log_path("translator-service.log").display()
        ));
    }

    Ok(())
}

fn not_running_hint(config: &ServiceConfig) -> String {
    format!(
        "translator service at {} is not running (auto-start disabled with --no-start)",
        config.service_url
    )
}

async fn service_ok(client: &reqwest::Client, base: &str) -> bool {
    let url = format!("{}/health", base.trim_end_matches('/'));
    matches!(
        client.get(url).timeout(HEALTH_TIMEOUT).send().await,
        Ok(response) if response.status().is_success()
    )
}

async fn ollama_ok(client: &reqwest::Client, base: &str) -> bool {
    let url = format!("{}/api/tags", base.trim_end_matches('/'));
    matches!(
        client.get(url).timeout(HEALTH_TIMEOUT).send().await,
        Ok(response) if response.status().is_success()
    )
}

async fn wait_until<F, Fut>(mut check: F, attempts: usize) -> bool
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    for _ in 0..attempts {
        if check().await {
            return true;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }

    check().await
}

fn core_env(service_url: &str) -> Vec<(String, String)> {
    let mut envs = vec![
        (
            "TRANSLATOR_ENGINE".to_string(),
            env_or("TRANSLATOR_ENGINE", DEFAULT_ENGINE),
        ),
        (
            "TRANSLATOR_MODEL".to_string(),
            env_or("TRANSLATOR_MODEL", DEFAULT_MODEL),
        ),
        (
            "TRANSLATOR_PROMPT_STYLE".to_string(),
            env_or("TRANSLATOR_PROMPT_STYLE", DEFAULT_PROMPT_STYLE),
        ),
        (
            "TRANSLATOR_MODEL_URL".to_string(),
            env_or("TRANSLATOR_MODEL_URL", DEFAULT_MODEL_URL),
        ),
    ];

    if let Some(bind_addr) = bind_addr_from_service_url(service_url) {
        envs.push((
            "TRANSLATOR_BIND_ADDR".to_string(),
            env_or("TRANSLATOR_BIND_ADDR", &bind_addr),
        ));
    }

    envs
}

fn spawn_detached(
    program: &Path,
    args: &[&str],
    envs: &[(String, String)],
    log_name: &str,
) -> Result<(), String> {
    let stdout = open_log(log_name)?;
    let stderr = stdout
        .try_clone()
        .map_err(|error| format!("failed to open log handle: {error}"))?;

    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .envs(envs.iter().map(|(key, value)| (key, value)))
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("failed to start {}: {error}", program.display()))
}

fn open_log(name: &str) -> Result<File, String> {
    let path = log_path(name);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|error| format!("failed to create {}: {error}", dir.display()))?;
    }

    OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|error| format!("failed to open {}: {error}", path.display()))
}

fn env_or(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

fn url_host(url: &str) -> Option<&str> {
    let rest = url.trim_end_matches('/');
    let authority = rest
        .strip_prefix("http://")
        .or_else(|| rest.strip_prefix("https://"))?;
    let authority = authority.split('/').next()?;
    Some(
        authority
            .rsplit_once(':')
            .map(|(host, _)| host)
            .unwrap_or(authority),
    )
}

fn is_local_url(url: &str) -> bool {
    matches!(url_host(url), Some("127.0.0.1") | Some("localhost"))
}

fn bind_addr_from_service_url(url: &str) -> Option<String> {
    if !is_local_url(url) {
        return None;
    }

    let rest = url.trim_end_matches('/');
    let authority = rest
        .strip_prefix("http://")
        .or_else(|| rest.strip_prefix("https://"))?;
    let authority = authority.split('/').next()?;

    authority
        .contains(':')
        .then(|| authority.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_url_hosts() {
        assert_eq!(url_host("http://127.0.0.1:17890"), Some("127.0.0.1"));
        assert_eq!(url_host("http://localhost:11434/"), Some("localhost"));
        assert_eq!(
            url_host("https://translate.example.com"),
            Some("translate.example.com")
        );
        assert_eq!(url_host("ftp://example.com"), None);
    }

    #[test]
    fn detects_local_urls() {
        assert!(is_local_url("http://127.0.0.1:17890"));
        assert!(is_local_url("http://localhost:11434"));
        assert!(!is_local_url("http://192.168.1.10:17890"));
        assert!(!is_local_url("https://example.com"));
    }

    #[test]
    fn derives_bind_addr_for_local_urls() {
        assert_eq!(
            bind_addr_from_service_url("http://127.0.0.1:17890"),
            Some("127.0.0.1:17890".to_string())
        );
        assert_eq!(bind_addr_from_service_url("http://127.0.0.1"), None);
        assert_eq!(bind_addr_from_service_url("https://example.com:443"), None);
    }

    #[test]
    fn core_env_sets_service_bind_addr() {
        let envs = core_env("http://127.0.0.1:17999");

        assert!(
            envs.contains(&("TRANSLATOR_BIND_ADDR".to_string(), "127.0.0.1:17999".to_string()))
        );
    }
}
