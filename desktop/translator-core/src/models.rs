use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};

pub const DEFAULT_MODEL_URL: &str = "https://modelscope.cn/models/Tencent-Hunyuan/HY-MT1.5-1.8B-GGUF/resolve/master/HY-MT1.5-1.8B-Q4_K_M.gguf";
pub const DEFAULT_MODEL_SHA256: &str =
    "4383ac0c3c8e476de98ff979c2a3f069f8c4fb385e7860cf2d28da896cc477c7";
pub const DEFAULT_MODEL_SIZE: u64 = 1_133_080_512;

/// One file of a downloadable model set.
pub struct ModelFile {
    pub name: &'static str,
    /// Tried in order; the first one that succeeds wins.
    pub urls: &'static [&'static str],
    pub sha256: &'static str,
    pub size: u64,
}

/// SenseVoice (zh/en/ja/ko/yue) + Silero VAD for live captions. The files are
/// served through the Hugging Face mirror (`hf-mirror.com`); ModelScope has no
/// mirror of this sherpa-onnx conversion. The VAD file comes from the
/// sherpa-onnx GitHub release with a gh-proxy fallback.
pub const SENSE_VOICE_MODEL_URL: &str = "https://hf-mirror.com/csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/resolve/main/model.int8.onnx";
pub const SENSE_VOICE_MODEL_SHA256: &str =
    "c71f0ce00bec95b07744e116345e33d8cbbe08cef896382cf907bf4b51a2cd51";
pub const SENSE_VOICE_TOKENS_URL: &str = "https://hf-mirror.com/csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/resolve/main/tokens.txt";
pub const SENSE_VOICE_TOKENS_SHA256: &str =
    "f449eb28dc567533d7fa59be34e2abca8784f771850c78a47fb731a31429a1dc";
pub const SILERO_VAD_URL: &str =
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/silero_vad.onnx";
pub const SILERO_VAD_FALLBACK_URL: &str =
    "https://gh-proxy.com/https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/silero_vad.onnx";
pub const SILERO_VAD_SHA256: &str =
    "9e2449e1087496d8d4caba907f23e0bd3f78d91fa552479bb9c23ac09cbb1fd6";

pub const ASR_MODEL_FILES: [ModelFile; 3] = [
    ModelFile {
        name: "model.int8.onnx",
        urls: &[SENSE_VOICE_MODEL_URL],
        sha256: SENSE_VOICE_MODEL_SHA256,
        size: 239_233_841,
    },
    ModelFile {
        name: "tokens.txt",
        urls: &[SENSE_VOICE_TOKENS_URL],
        sha256: SENSE_VOICE_TOKENS_SHA256,
        size: 315_894,
    },
    ModelFile {
        name: "silero_vad.onnx",
        urls: &[SILERO_VAD_URL, SILERO_VAD_FALLBACK_URL],
        sha256: SILERO_VAD_SHA256,
        size: 643_854,
    },
];

pub fn asr_model_total_size() -> u64 {
    ASR_MODEL_FILES.iter().map(|file| file.size).sum()
}

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

pub fn format_download_status(downloaded: u64, total: Option<u64>) -> String {
    const MB: f64 = 1_000_000.0;

    match total {
        Some(total) if total > 0 => format!(
            "正在下载模型：{:.0}%（{:.0}/{:.0} MB）",
            downloaded as f64 / total as f64 * 100.0,
            downloaded as f64 / MB,
            total as f64 / MB
        ),
        _ => format!("正在下载模型：已下载 {:.0} MB", downloaded as f64 / MB),
    }
}

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

/// Whether `path` exists and hashes to `expected` (hex, case-insensitive).
pub fn verify_sha256(path: &Path, expected: &str) -> Result<bool, ModelError> {
    let mut hasher = Sha256::new();
    hash_file(path, &mut hasher)?;

    Ok(hex(hasher.finalize()).eq_ignore_ascii_case(expected))
}

/// Downloads one [`ModelFile`], trying its URLs in order. An existing file
/// that already matches the digest is kept without touching the network.
pub async fn download_model_file<F>(
    client: &reqwest::Client,
    file: &ModelFile,
    dest_dir: &Path,
    mut progress: F,
) -> Result<PathBuf, ModelError>
where
    F: FnMut(u64, Option<u64>),
{
    let dest = dest_dir.join(file.name);

    if dest.is_file() && verify_sha256(&dest, file.sha256).unwrap_or(false) {
        return Ok(dest);
    }

    let mut last_error = None;

    for url in file.urls {
        match download(client, url, &dest, Some(file.sha256), |downloaded, total| {
            progress(downloaded, total)
        })
        .await
        {
            Ok(()) => return Ok(dest),
            Err(error) => last_error = Some(error),
        }
    }

    Err(last_error.unwrap_or_else(|| ModelError::Http("no download source".to_string())))
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

    #[test]
    fn formats_download_status() {
        assert_eq!(
            format_download_status(500_000_000, Some(1_000_000_000)),
            "正在下载模型：50%（500/1000 MB）"
        );
        assert_eq!(
            format_download_status(123_000_000, None),
            "正在下载模型：已下载 123 MB"
        );
    }

    #[test]
    fn asr_model_specs_are_well_formed() {
        assert_eq!(ASR_MODEL_FILES.len(), 3);

        for file in &ASR_MODEL_FILES {
            assert!(!file.name.is_empty());
            assert_eq!(file.sha256.len(), 64);
            assert!(file.sha256.chars().all(|c| c.is_ascii_hexdigit()));
            assert!(!file.urls.is_empty());
            assert!(file.urls.iter().all(|url| url.starts_with("https://")));
            assert!(file.size > 0);
        }

        assert!(asr_model_total_size() > 200_000_000);
    }

    #[tokio::test]
    async fn download_model_file_falls_back_to_the_second_url() {
        let payload = payload();
        let url = serve(payload.clone(), true).await;
        let good_url: &'static str = Box::leak(url.into_boxed_str());
        let urls: &'static [&'static str] =
            Box::leak(vec!["http://127.0.0.1:9/refused", good_url].into_boxed_slice());
        let sha: &'static str = Box::leak(sha256_hex(&payload).into_boxed_str());
        let file = ModelFile {
            name: "fallback.bin",
            urls,
            sha256: sha,
            size: payload.len() as u64,
        };
        let dest_dir = std::env::temp_dir().join(format!(
            "translator-core-asr-download-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dest_dir);

        let client = reqwest::Client::new();
        let path = download_model_file(&client, &file, &dest_dir, |_, _| {})
            .await
            .unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), payload);

        // A second call must keep the verified file without downloading again.
        let path_again = download_model_file(&client, &file, &dest_dir, |_, _| {
            panic!("must not download a verified file");
        })
        .await
        .unwrap();
        assert_eq!(path, path_again);

        let _ = std::fs::remove_dir_all(&dest_dir);
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
