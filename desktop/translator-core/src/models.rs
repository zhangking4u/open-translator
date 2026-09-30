use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};

pub const DEFAULT_MODEL_URL: &str = "https://modelscope.cn/models/Tencent-Hunyuan/HY-MT1.5-1.8B-GGUF/resolve/master/HY-MT1.5-1.8B-Q4_K_M.gguf";
pub const DEFAULT_MODEL_SHA256: &str =
    "4383ac0c3c8e476de98ff979c2a3f069f8c4fb385e7860cf2d28da896cc477c7";
pub const DEFAULT_MODEL_SIZE: u64 = 1_133_080_512;

pub fn download_client() -> Result<reqwest::Client, ModelError> {
    reqwest::Client::builder()
        .user_agent(concat!("OpenTranslator/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| ModelError::Http(error.to_string()))
}

#[derive(Debug)]
pub enum ModelError {
    Http(String),
    Io(String),
    Incomplete { expected: u64, actual: u64 },
    Checksum { expected: String, actual: String },
}

impl std::fmt::Display for ModelError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http(message) => write!(formatter, "download failed: {message}"),
            Self::Io(message) => write!(formatter, "file error: {message}"),
            Self::Incomplete { expected, actual } => write!(
                formatter,
                "download incomplete: got {actual} of {expected} bytes"
            ),
            Self::Checksum { expected, actual } => write!(
                formatter,
                "checksum mismatch: expected {expected}, got {actual}"
            ),
        }
    }
}

impl std::error::Error for ModelError {}

pub fn partial_path(dest: &Path) -> PathBuf {
    let mut name = dest.as_os_str().to_os_string();
    name.push(".part");
    PathBuf::from(name)
}

pub async fn download<F>(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    expected_sha256: Option<&str>,
    mut progress: F,
) -> Result<(), ModelError>
where
    F: FnMut(u64, Option<u64>),
{
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir).map_err(|error| ModelError::Io(error.to_string()))?;
    }

    let partial = partial_path(dest);
    let mut offset = std::fs::metadata(&partial).map(|meta| meta.len()).unwrap_or(0);
    let mut hasher = Sha256::new();

    if offset > 0 {
        hash_file(&partial, &mut hasher)?;
        progress(offset, None);
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&partial)
        .map_err(|error| ModelError::Io(error.to_string()))?;

    let total = loop {
        let mut request = client.get(url);

        if offset > 0 {
            request = request.header(reqwest::header::RANGE, format!("bytes={offset}-"));
        }

        let mut response = request
            .send()
            .await
            .map_err(|error| ModelError::Http(format!("request failed: {error}")))?;

        let status = response.status();

        if status == reqwest::StatusCode::PARTIAL_CONTENT {
            // resuming where the partial file left off
        } else if status.is_success() {
            if offset > 0 {
                // the server ignored the range request; start over
                offset = 0;
                hasher = Sha256::new();
                file = OpenOptions::new()
                    .create(true)
                    .write(true)
                    .truncate(true)
                    .open(&partial)
                    .map_err(|error| ModelError::Io(error.to_string()))?;
            }
        } else if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
            let _ = std::fs::remove_file(&partial);
            return Err(ModelError::Http(
                "resume out of sync; removed the partial file, please retry".to_string(),
            ));
        } else {
            return Err(ModelError::Http(format!("server returned {status}")));
        }

        let total = response.content_length().map(|length| length + offset);

        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| ModelError::Http(format!("transfer interrupted: {error}")))?
        {
            file.write_all(&chunk)
                .map_err(|error| ModelError::Io(error.to_string()))?;
            hasher.update(&chunk);
            offset += chunk.len() as u64;
            progress(offset, total);
        }

        break total;
    };

    file.flush()
        .map_err(|error| ModelError::Io(error.to_string()))?;
    drop(file);

    if let Some(total) = total {
        if offset != total {
            return Err(ModelError::Incomplete {
                expected: total,
                actual: offset,
            });
        }
    }

    if let Some(expected) = expected_sha256 {
        let actual = hex(hasher.finalize());
        if !actual.eq_ignore_ascii_case(expected) {
            let _ = std::fs::remove_file(&partial);
            return Err(ModelError::Checksum { expected: expected.to_string(), actual });
        }
    }

    std::fs::rename(&partial, dest).map_err(|error| ModelError::Io(error.to_string()))?;

    Ok(())
}

fn hash_file(path: &Path, hasher: &mut Sha256) -> Result<(), ModelError> {
    let mut file = std::fs::File::open(path).map_err(|error| ModelError::Io(error.to_string()))?;
    let mut buffer = [0u8; 64 * 1024];

    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| ModelError::Io(error.to_string()))?;

        if read == 0 {
            break;
        }

        hasher.update(&buffer[..read]);
    }

    Ok(())
}

pub fn hex(bytes: impl AsRef<[u8]>) -> String {
    bytes.as_ref().iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    use axum::Router;
    use axum::body::Body;
    use axum::http::{HeaderMap, StatusCode, header};
    use axum::routing::get;

    fn payload() -> Vec<u8> {
        (0..200_000u32).map(|index| (index % 251) as u8).collect()
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        hex(hasher.finalize())
    }

    async fn serve(payload: Vec<u8>, support_range: bool) -> String {
        let app = Router::new().route(
            "/model",
            get(move |headers: HeaderMap| {
                let payload = payload.clone();

                async move {
                    if support_range {
                        if let Some(range) = headers
                            .get(header::RANGE)
                            .and_then(|value| value.to_str().ok())
                        {
                            let start = range
                                .trim_start_matches("bytes=")
                                .split('-')
                                .next()
                                .and_then(|value| value.parse::<usize>().ok())
                                .unwrap_or(0);

                            let start = start.min(payload.len());
                            let body = payload[start..].to_vec();

                            let mut response = axum::response::Response::new(Body::from(body));
                            *response.status_mut() = StatusCode::PARTIAL_CONTENT;
                            response.headers_mut().insert(
                                header::CONTENT_RANGE,
                                format!("bytes {}-{}/{}", start, payload.len() - 1, payload.len())
                                    .parse()
                                    .unwrap(),
                            );

                            return response;
                        }
                    }

                    axum::response::Response::new(Body::from(payload))
                }
            }),
        );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        format!("http://{address}/model")
    }

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "translator-core-model-test-{}-{name}",
            std::process::id()
        ))
    }

    #[tokio::test]
    async fn downloads_and_verifies() {
        let payload = payload();
        let url = serve(payload.clone(), true).await;
        let dest = temp_path("full.gguf");
        let _ = std::fs::remove_file(&dest);

        let client = reqwest::Client::new();
        let mut last = (0u64, None);

        download(
            &client,
            &url,
            &dest,
            Some(&sha256_hex(&payload)),
            |downloaded, total| last = (downloaded, total),
        )
        .await
        .unwrap();

        assert_eq!(std::fs::read(&dest).unwrap(), payload);
        assert_eq!(last.0, payload.len() as u64);
        assert_eq!(last.1, Some(payload.len() as u64));
        assert!(!partial_path(&dest).exists());

        let _ = std::fs::remove_file(&dest);
    }

    #[tokio::test]
    async fn rejects_checksum_mismatch() {
        let payload = payload();
        let url = serve(payload.clone(), true).await;
        let dest = temp_path("bad.gguf");
        let _ = std::fs::remove_file(&dest);

        let client = reqwest::Client::new();
        let error = download(&client, &url, &dest, Some("deadbeef"), |_, _| {})
            .await
            .unwrap_err();

        assert!(matches!(error, ModelError::Checksum { .. }));
        assert!(!dest.exists());
        assert!(!partial_path(&dest).exists());
    }

    #[tokio::test]
    async fn resumes_partial_download() {
        let payload = payload();
        let url = serve(payload.clone(), true).await;
        let dest = temp_path("resume.gguf");
        let partial = partial_path(&dest);
        let _ = std::fs::remove_file(&dest);

        let half = payload.len() / 2;
        std::fs::write(&partial, &payload[..half]).unwrap();

        let client = reqwest::Client::new();
        download(&client, &url, &dest, Some(&sha256_hex(&payload)), |_, _| {})
            .await
            .unwrap();

        assert_eq!(std::fs::read(&dest).unwrap(), payload);

        let _ = std::fs::remove_file(&dest);
    }

    #[tokio::test]
    async fn restarts_when_server_ignores_range() {
        let payload = payload();
        let url = serve(payload.clone(), false).await;
        let dest = temp_path("restart.gguf");
        let partial = partial_path(&dest);
        let _ = std::fs::remove_file(&dest);

        std::fs::write(&partial, &payload[..100]).unwrap();

        let client = reqwest::Client::new();
        download(&client, &url, &dest, Some(&sha256_hex(&payload)), |_, _| {})
            .await
            .unwrap();

        assert_eq!(std::fs::read(&dest).unwrap(), payload);

        let _ = std::fs::remove_file(&dest);
    }
}
