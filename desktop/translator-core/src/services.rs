use std::env;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use tokio::sync::Mutex;

use crate::models;
use crate::paths::{default_core_bin, default_model_path, default_ollama_bin, log_path};
use crate::settings::load_config;

const DEFAULT_MODEL_URL: &str = "http://127.0.0.1:11434";
const DEFAULT_OLLAMA_MODEL: &str = "hy-mt1.5-1.8b";
const DEFAULT_PROMPT_STYLE: &str = "hymt";

const HEALTH_TIMEOUT: Duration = Duration::from_millis(1000);
const POLL_INTERVAL: Duration = Duration::from_millis(500);
const OLLAMA_POLL_ATTEMPTS: usize = 40;
const CORE_POLL_ATTEMPTS: usize = 120;

// Only one model download may run per process; concurrent writers share the
// same `.part` file and can corrupt the final model.
static DOWNLOAD_LOCK: Mutex<()> = Mutex::const_new(());

#[derive(Debug, Clone, PartialEq)]
pub enum EngineChoice {
    Ollama,
    LlamaCpp {
        model_path: PathBuf,
        default_path: bool,
    },
}

#[derive(Debug, Clone)]
pub struct ServiceConfig {
    pub service_url: String,
    pub model_url: String,
    pub core_bin: Option<PathBuf>,
    pub ollama_bin: Option<PathBuf>,
    pub start: bool,
    pub engine: EngineChoice,
    pub prompt_style: String,
    pub auto_download: bool,
    pub ollama_model: String,
}

impl ServiceConfig {
    pub fn from_env(service_url: &str, start: bool) -> Result<Self, String> {
        let file = load_config();

        let engine = resolve_engine(
            env::var("TRANSLATOR_ENGINE").ok().as_deref(),
            env_model_path(),
            file.model_path.as_deref(),
            default_model_path(),
        )?;

        let prompt_style = env::var("TRANSLATOR_PROMPT_STYLE")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| file.prompt_style.clone())
            .unwrap_or_else(|| DEFAULT_PROMPT_STYLE.to_string());

        let auto_download = resolve_auto_download(
            env::var("TRANSLATOR_AUTO_DOWNLOAD").ok().as_deref(),
            file.auto_download.as_deref(),
        )?;

        Ok(Self {
            service_url: service_url.to_string(),
            model_url: env_or("TRANSLATOR_MODEL_URL", DEFAULT_MODEL_URL),
            core_bin: env::var_os("TRANSLATOR_CORE_BIN")
                .map(PathBuf::from)
                .or_else(default_core_bin),
            ollama_bin: env::var_os("TRANSLATOR_OLLAMA_BIN")
                .map(PathBuf::from)
                .or_else(default_ollama_bin),
            start,
            engine,
            prompt_style,
            auto_download,
            ollama_model: env_or("TRANSLATOR_MODEL", DEFAULT_OLLAMA_MODEL),
        })
    }
}

fn env_model_path() -> Option<PathBuf> {
    env::var_os("TRANSLATOR_MODEL_PATH")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn resolve_engine(
    explicit: Option<&str>,
    env_model_path: Option<PathBuf>,
    file_model_path: Option<&str>,
    fallback_default: Option<PathBuf>,
) -> Result<EngineChoice, String> {
    match explicit {
        Some("ollama") => Ok(EngineChoice::Ollama),
        Some("llama-cpp") => {
            let (model_path, default_path) =
                model_path_from(env_model_path, file_model_path, fallback_default)
                    .ok_or_else(|| "cannot determine the default model directory".to_string())?;
            Ok(EngineChoice::LlamaCpp {
                model_path,
                default_path,
            })
        }
        Some(other) => Err(format!(
            "unsupported TRANSLATOR_ENGINE: {other} (expected ollama or llama-cpp)"
        )),
        None => match model_path_from(env_model_path, file_model_path, fallback_default) {
            Some((model_path, default_path)) => Ok(EngineChoice::LlamaCpp {
                model_path,
                default_path,
            }),
            None => Ok(EngineChoice::Ollama),
        },
    }
}

fn model_path_from(
    env_model_path: Option<PathBuf>,
    file_model_path: Option<&str>,
    fallback_default: Option<PathBuf>,
) -> Option<(PathBuf, bool)> {
    if let Some(path) = env_model_path {
        return Some((path, false));
    }

    if let Some(path) = file_model_path {
        return Some((PathBuf::from(path), false));
    }

    fallback_default.map(|path| (path, true))
}

fn resolve_auto_download(env_value: Option<&str>, file_value: Option<&str>) -> Result<bool, String> {
    match env_value {
        Some(value) => parse_bool("TRANSLATOR_AUTO_DOWNLOAD", value),
        None => match file_value {
            Some(value) => parse_bool("auto_download", value),
            None => Ok(true),
        },
    }
}

fn parse_bool(key: &str, value: &str) -> Result<bool, String> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(format!("invalid {key}: {other}")),
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

    match &config.engine {
        EngineChoice::Ollama => {
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
        }
        EngineChoice::LlamaCpp {
            model_path,
            default_path,
        } => {
            if !model_path.is_file() {
                return Err(model_missing_hint(
                    model_path,
                    *default_path,
                    config.auto_download,
                ));
            }
        }
    }

    let Some(bin) = &config.core_bin else {
        return Err(
            "translator service is not running and no core binary was found; build it with \
             `cargo build --release` in core/translator-service or set TRANSLATOR_CORE_BIN"
                .to_string(),
        );
    };

    spawn_detached(
        bin,
        &[],
        &core_env(config, &config.service_url),
        "translator-service.log",
    )?;

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

pub async fn ensure_with_download<F>(
    client: &reqwest::Client,
    config: &ServiceConfig,
    mut progress: F,
) -> Result<(), String>
where
    F: FnMut(u64, Option<u64>),
{
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

    if let EngineChoice::LlamaCpp {
        model_path,
        default_path,
    } = &config.engine
    {
        if !model_path.is_file() {
            if config.auto_download && *default_path {
                let _guard = DOWNLOAD_LOCK.lock().await;

                // Another caller may have completed the download while this one
                // waited for the lock.
                if !model_path.is_file() {
                    let download_client = models::download_client()
                        .map_err(|error| format!("failed to initialise the download: {error}"))?;

                    models::download(
                        &download_client,
                        models::DEFAULT_MODEL_URL,
                        model_path,
                        Some(models::DEFAULT_MODEL_SHA256),
                        &mut progress,
                    )
                    .await
                    .map_err(|error| format!("model download failed: {error}"))?;
                }
            } else {
                return Err(model_missing_hint(
                    model_path,
                    *default_path,
                    config.auto_download,
                ));
            }
        }
    }

    ensure(client, config).await
}

fn model_missing_hint(model_path: &Path, default_path: bool, auto_download: bool) -> String {
    if !default_path {
        format!(
            "model file not found: {} (auto-download only fills the default model path; provide \
             this file or unset model_path)",
            model_path.display()
        )
    } else if auto_download {
        format!(
            "model file not found: {} (the download did not complete; check the network and retry)",
            model_path.display()
        )
    } else {
        format!(
            "model file not found: {} (enable auto_download or provide the file)",
            model_path.display()
        )
    }
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

fn core_env(config: &ServiceConfig, service_url: &str) -> Vec<(String, String)> {
    let mut envs = vec![(
        "TRANSLATOR_PROMPT_STYLE".to_string(),
        config.prompt_style.clone(),
    )];

    match &config.engine {
        EngineChoice::Ollama => {
            envs.push(("TRANSLATOR_ENGINE".to_string(), "ollama".to_string()));
            envs.push(("TRANSLATOR_MODEL".to_string(), config.ollama_model.clone()));
            envs.push((
                "TRANSLATOR_MODEL_URL".to_string(),
                config.model_url.clone(),
            ));
        }
        EngineChoice::LlamaCpp { model_path, .. } => {
            envs.push(("TRANSLATOR_ENGINE".to_string(), "llama-cpp".to_string()));
            envs.push((
                "TRANSLATOR_MODEL_PATH".to_string(),
                model_path.to_string_lossy().to_string(),
            ));
        }
    }

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

pub fn bind_addr_from_service_url(url: &str) -> Option<String> {
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

    fn llama_config(model_path: &str, default_path: bool) -> ServiceConfig {
        ServiceConfig {
            service_url: "http://127.0.0.1:17890".to_string(),
            model_url: DEFAULT_MODEL_URL.to_string(),
            core_bin: None,
            ollama_bin: None,
            start: true,
            engine: EngineChoice::LlamaCpp {
                model_path: PathBuf::from(model_path),
                default_path,
            },
            prompt_style: "hymt".to_string(),
            auto_download: true,
            ollama_model: DEFAULT_OLLAMA_MODEL.to_string(),
        }
    }

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
    fn defaults_to_llama_cpp_with_the_default_model_path() {
        let engine = resolve_engine(None, None, None, Some(PathBuf::from("/models/default.gguf")))
            .unwrap();

        assert_eq!(
            engine,
            EngineChoice::LlamaCpp {
                model_path: PathBuf::from("/models/default.gguf"),
                default_path: true,
            }
        );
    }

    #[test]
    fn explicit_model_path_disables_the_default_flag() {
        let engine = resolve_engine(
            None,
            Some(PathBuf::from("/env/model.gguf")),
            None,
            Some(PathBuf::from("/models/default.gguf")),
        )
        .unwrap();

        assert_eq!(
            engine,
            EngineChoice::LlamaCpp {
                model_path: PathBuf::from("/env/model.gguf"),
                default_path: false,
            }
        );

        let engine = resolve_engine(
            Some("llama-cpp"),
            None,
            Some("/config/model.gguf"),
            Some(PathBuf::from("/models/default.gguf")),
        )
        .unwrap();

        assert_eq!(
            engine,
            EngineChoice::LlamaCpp {
                model_path: PathBuf::from("/config/model.gguf"),
                default_path: false,
            }
        );
    }

    #[test]
    fn explicit_ollama_engine_wins() {
        let engine = resolve_engine(
            Some("ollama"),
            Some(PathBuf::from("/env/model.gguf")),
            None,
            None,
        )
        .unwrap();

        assert_eq!(engine, EngineChoice::Ollama);
    }

    #[test]
    fn llama_cpp_without_any_model_path_fails() {
        assert!(resolve_engine(Some("llama-cpp"), None, None, None).is_err());
        assert!(resolve_engine(Some("other"), None, None, None).is_err());
    }

    #[test]
    fn auto_download_defaults_to_true_and_parses_overrides() {
        assert!(resolve_auto_download(None, None).unwrap());
        assert!(!resolve_auto_download(None, Some("false")).unwrap());
        assert!(!resolve_auto_download(Some("false"), Some("true")).unwrap());
        assert!(resolve_auto_download(Some("true"), None).unwrap());
        assert!(resolve_auto_download(Some("nope"), None).is_err());
    }

    #[test]
    fn core_env_sets_service_bind_addr() {
        let config = llama_config("/models/model.gguf", true);
        let envs = core_env(&config, "http://127.0.0.1:17999");

        assert!(
            envs.contains(&("TRANSLATOR_BIND_ADDR".to_string(), "127.0.0.1:17999".to_string()))
        );
        assert!(envs.contains(&("TRANSLATOR_ENGINE".to_string(), "llama-cpp".to_string())));
        assert!(envs.contains(&(
            "TRANSLATOR_MODEL_PATH".to_string(),
            "/models/model.gguf".to_string()
        )));
        assert!(envs.contains(&("TRANSLATOR_PROMPT_STYLE".to_string(), "hymt".to_string())));
    }

    #[test]
    fn core_env_keeps_the_ollama_legacy_path() {
        let mut config = llama_config("/models/model.gguf", true);
        config.engine = EngineChoice::Ollama;

        let envs = core_env(&config, "http://127.0.0.1:17890");

        assert!(envs.contains(&("TRANSLATOR_ENGINE".to_string(), "ollama".to_string())));
        assert!(envs.contains(&("TRANSLATOR_MODEL".to_string(), DEFAULT_OLLAMA_MODEL.to_string())));
        assert!(envs.contains(&("TRANSLATOR_MODEL_URL".to_string(), DEFAULT_MODEL_URL.to_string())));
        assert!(!envs.iter().any(|(key, _)| key == "TRANSLATOR_MODEL_PATH"));
    }
}
