use std::env;
use std::time::Duration;

use crate::settings::FileConfig;

pub const DEFAULT_RELEASES_URL: &str =
    "https://api.github.com/repos/zhangking4u/open-translator/releases/latest";

const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseAsset {
    pub name: String,
    pub url: String,
    /// GitHub asset digest, e.g. `sha256:...`, when the API provides one.
    pub digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseInfo {
    pub version: String,
    pub url: String,
    pub assets: Vec<ReleaseAsset>,
}

#[derive(Debug)]
pub enum UpdateError {
    Http(String),
    Parse(String),
}

impl std::fmt::Display for UpdateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http(message) => write!(formatter, "update check failed: {message}"),
            Self::Parse(message) => write!(formatter, "invalid release response: {message}"),
        }
    }
}

impl std::error::Error for UpdateError {}

/// Version compiled into the binary: the release tag when the release workflow
/// builds it, otherwise the crate version. The tag's `v` prefix is stripped so
/// callers can add their own without showing `vv0.4.0`.
pub fn current_version() -> &'static str {
    let value = option_env!("OPEN_TRANSLATOR_VERSION")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(env!("CARGO_PKG_VERSION"));
    let trimmed = value.trim();

    trimmed
        .strip_prefix('v')
        .or_else(|| trimmed.strip_prefix('V'))
        .unwrap_or(trimmed)
}

pub fn enabled(file: &FileConfig) -> Result<bool, String> {
    match env::var("TRANSLATOR_CHECK_UPDATES") {
        Ok(value) => parse_bool("TRANSLATOR_CHECK_UPDATES", &value),
        Err(_) => match file.check_updates.as_deref() {
            Some(value) => parse_bool("check_updates", value),
            None => Ok(true),
        },
    }
}

pub fn api_url() -> String {
    env::var("TRANSLATOR_UPDATE_URL").unwrap_or_else(|_| DEFAULT_RELEASES_URL.to_string())
}

pub async fn check(
    client: &reqwest::Client,
    current: &str,
    api_url: &str,
) -> Result<Option<ReleaseInfo>, UpdateError> {
    let response = client
        .get(api_url)
        .header(
            reqwest::header::USER_AGENT,
            concat!("OpenTranslator/", env!("CARGO_PKG_VERSION")),
        )
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .timeout(REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|error| UpdateError::Http(format!("request failed: {error}")))?;

    let status = response.status();

    if !status.is_success() {
        return Err(UpdateError::Http(format!("server returned {status}")));
    }

    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|error| UpdateError::Parse(format!("invalid JSON: {error}")))?;

    let tag = body
        .get("tag_name")
        .and_then(|value| value.as_str())
        .ok_or_else(|| UpdateError::Parse("missing tag_name".to_string()))?;

    let version = tag
        .strip_prefix('v')
        .or_else(|| tag.strip_prefix('V'))
        .unwrap_or(tag)
        .trim()
        .to_string();

    if version.is_empty() {
        return Err(UpdateError::Parse("empty tag_name".to_string()));
    }

    let url = body
        .get("html_url")
        .and_then(|value| value.as_str())
        .unwrap_or(DEFAULT_RELEASES_URL);

    let assets = body
        .get("assets")
        .and_then(|value| value.as_array())
        .map(|assets| {
            assets
                .iter()
                .filter_map(|asset| {
                    let name = asset.get("name")?.as_str()?;
                    let url = asset.get("browser_download_url")?.as_str()?;
                    let digest = asset
                        .get("digest")
                        .and_then(|value| value.as_str())
                        .map(|value| value.to_string());

                    Some(ReleaseAsset {
                        name: name.to_string(),
                        url: url.to_string(),
                        digest,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    if is_newer(&version, current) {
        Ok(Some(ReleaseInfo {
            version,
            url: url.to_string(),
            assets,
        }))
    } else {
        Ok(None)
    }
}

pub fn is_newer(remote: &str, current: &str) -> bool {
    let (Some(remote), Some(current)) = (parse_version(remote), parse_version(current)) else {
        return false;
    };

    let length = remote.len().max(current.len());

    for index in 0..length {
        let remote_part = remote.get(index).copied().unwrap_or(0);
        let current_part = current.get(index).copied().unwrap_or(0);

        if remote_part != current_part {
            return remote_part > current_part;
        }
    }

    false
}

fn parse_version(value: &str) -> Option<Vec<u64>> {
    let value = value.trim();
    let value = value
        .strip_prefix('v')
        .or_else(|| value.strip_prefix('V'))
        .unwrap_or(value);
    let core = value.split(['-', '+']).next()?;

    if core.is_empty() {
        return None;
    }

    core.split('.').map(|part| part.parse::<u64>().ok()).collect()
}

fn parse_bool(key: &str, value: &str) -> Result<bool, String> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(format!("invalid {key}: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use axum::Router;
    use axum::routing::get;

    fn stub_url(body: String, status: u16) -> String {
        let app = Router::new().route(
            "/releases/latest",
            get(move || {
                let body = body.clone();
                async move {
                    (
                        axum::http::StatusCode::from_u16(status).unwrap(),
                        [(axum::http::header::CONTENT_TYPE, "application/json")],
                        body,
                    )
                }
            }),
        );

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();

        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async move {
                let listener = tokio::net::TcpListener::from_std(listener).unwrap();
                axum::serve(listener, app).await.unwrap();
            });
        });

        format!("http://{address}/releases/latest")
    }

    #[test]
    fn current_version_has_no_tag_prefix() {
        let version = current_version();

        assert!(!version.is_empty());
        assert!(!version.starts_with('v'));
        assert!(!version.starts_with('V'));
    }

    #[test]
    fn compares_versions() {
        assert!(is_newer("0.1.1", "0.1.0"));
        assert!(is_newer("v0.2.0", "0.1.9"));
        assert!(is_newer("1.0", "0.9.9"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.1.1"));
        assert!(!is_newer("0.1.0-beta", "0.1.0"));
        assert!(!is_newer("main", "0.1.0"));
        assert!(!is_newer("0.1.0", "unknown"));
    }

    #[tokio::test]
    async fn reports_a_newer_release() {
        let url = stub_url(
            r#"{"tag_name":"v0.2.0","html_url":"https://example.com/releases/v0.2.0","assets":[{"name":"OpenTranslator-windows-x64.zip","browser_download_url":"https://example.com/download/win.zip"},{"name":"OpenTranslator-macos-arm64.dmg"}]}"#.to_string(),
            200,
        );

        let client = reqwest::Client::new();
        let info = check(&client, "0.1.0", &url).await.unwrap().unwrap();

        assert_eq!(info.version, "0.2.0");
        assert_eq!(info.url, "https://example.com/releases/v0.2.0");
        assert_eq!(info.assets.len(), 1);
        assert_eq!(info.assets[0].name, "OpenTranslator-windows-x64.zip");
        assert_eq!(info.assets[0].url, "https://example.com/download/win.zip");
    }

    #[tokio::test]
    async fn ignores_the_current_release() {
        let url = stub_url(r#"{"tag_name":"v0.1.0"}"#.to_string(), 200);

        let client = reqwest::Client::new();

        assert!(check(&client, "0.1.0", &url).await.unwrap().is_none());
    }

    #[tokio::test]
    #[ignore = "hits the GitHub API"]
    async fn checks_the_real_api() {
        let client = reqwest::Client::new();
        let info = check(&client, "0.0.1", DEFAULT_RELEASES_URL)
            .await
            .unwrap()
            .expect("a release newer than 0.0.1 exists");

        assert!(is_newer(&info.version, "0.0.1"));
        assert!(info.url.starts_with("https://github.com/"));
    }

    #[tokio::test]
    async fn surfaces_http_and_parse_errors() {
        let error_url = stub_url("{}".to_string(), 500);
        let bad_json_url = stub_url("not json".to_string(), 200);
        let missing_tag_url = stub_url("{}".to_string(), 200);

        let client = reqwest::Client::new();

        assert!(matches!(
            check(&client, "0.1.0", &error_url).await,
            Err(UpdateError::Http(_))
        ));
        assert!(matches!(
            check(&client, "0.1.0", &bad_json_url).await,
            Err(UpdateError::Parse(_))
        ));
        assert!(matches!(
            check(&client, "0.1.0", &missing_tag_url).await,
            Err(UpdateError::Parse(_))
        ));
    }
}
