use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};

pub const DEFAULT_MODEL_URL: &str = "https://modelscope.cn/models/Tencent-Hunyuan/Hy-MT2-1.8B-GGUF/resolve/master/Hy-MT2-1.8B-Q4_K_M.gguf";
pub const DEFAULT_MODEL_SHA256: &str =
    "dc5f44fcf1fa496ee7ad725982c0c8c553a4de00259b53af84c4b89fb0c06699";
pub const DEFAULT_MODEL_SIZE: u64 = 1_133_080_448;

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
/// mirror of this sherpa-onnx conversion. The VAD file prefers an hf-mirror
/// copy of the exact k2-fsa export (the pinned SHA-256 keeps any mirror
/// honest), then falls back to the GitHub release and its gh-proxy copy.
pub const SENSE_VOICE_MODEL_URL: &str = "https://hf-mirror.com/csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/resolve/main/model.int8.onnx";
pub const SENSE_VOICE_MODEL_SHA256: &str =
    "c71f0ce00bec95b07744e116345e33d8cbbe08cef896382cf907bf4b51a2cd51";
pub const SENSE_VOICE_TOKENS_URL: &str = "https://hf-mirror.com/csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/resolve/main/tokens.txt";
pub const SENSE_VOICE_TOKENS_SHA256: &str =
    "f449eb28dc567533d7fa59be34e2abca8784f771850c78a47fb731a31429a1dc";
pub const SILERO_VAD_MIRROR_URL: &str =
    "https://hf-mirror.com/R4kSo1997/sherpa-onnx-silero-vad-v5/resolve/main/silero_vad.onnx";
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
        urls: &[SILERO_VAD_MIRROR_URL, SILERO_VAD_URL, SILERO_VAD_FALLBACK_URL],
        sha256: SILERO_VAD_SHA256,
        size: 643_854,
    },
];

pub fn asr_model_total_size() -> u64 {
    ASR_MODEL_FILES.iter().map(|file| file.size).sum()
}

/// Kokoro v1.1 (Chinese + English, 103 speakers) text-to-speech model.
///
/// The Hugging Face mirror repository serves the converted sherpa-onnx files
/// individually, so the client can reuse the normal resume/checksum downloader
/// without unpacking an archive. Only the files needed for Chinese and English
/// are listed; `dict/` and the unused eSpeak voice variants are skipped.
macro_rules! kokoro_urls {
    ($name:literal) => {
        &[
            concat!(
                "https://hf-mirror.com/csukuangfj/kokoro-int8-multi-lang-v1_1/resolve/main/",
                $name
            ),
            concat!(
                "https://huggingface.co/csukuangfj/kokoro-int8-multi-lang-v1_1/resolve/main/",
                $name
            ),
        ]
    };
}

pub const KOKORO_MODEL_FILES: [ModelFile; 19] = [
    ModelFile {
        name: "model.int8.onnx",
        urls: kokoro_urls!("model.int8.onnx"),
        sha256: "bda15858163726a492d02a9a727bc263551b86ac77f90812c4b30ff41d380e26",
        size: 114_299_010,
    },
    ModelFile {
        name: "voices.bin",
        urls: kokoro_urls!("voices.bin"),
        sha256: "e64a5a581d8c2a350d848f51c3121657cd83aa07ed6109172177345874a7244c",
        size: 53_790_720,
    },
    ModelFile {
        name: "tokens.txt",
        urls: kokoro_urls!("tokens.txt"),
        sha256: "931ab2df2400cd65d580a22402024c2347ced8ae9ea300e545144b1aacc48e14",
        size: 1_111,
    },
    ModelFile {
        name: "lexicon-us-en.txt",
        urls: kokoro_urls!("lexicon-us-en.txt"),
        sha256: "7daaab53a181be9885b853a8582bf1838186317e5dadacbcef9c426d6fa0da14",
        size: 5_956_885,
    },
    ModelFile {
        name: "lexicon-zh.txt",
        urls: kokoro_urls!("lexicon-zh.txt"),
        sha256: "11111d8cd695fba2ace1367a1d0a708b586e6ef5c1f9be91da5d7eef129b651c",
        size: 2_119_465,
    },
    ModelFile {
        name: "date-zh.fst",
        urls: kokoro_urls!("date-zh.fst"),
        sha256: "eb8aa079ae3cb81d8f4404992f39d61a0cb990947512b5b8d1e54d1f6980e718",
        size: 59_154,
    },
    ModelFile {
        name: "number-zh.fst",
        urls: kokoro_urls!("number-zh.fst"),
        sha256: "743f402181fcfebf76cc2f0546b71fa26476e626fbe4e460fb7b4c3a7a8bd5bd",
        size: 64_482,
    },
    ModelFile {
        name: "phone-zh.fst",
        urls: kokoro_urls!("phone-zh.fst"),
        sha256: "1ac2b6fa56b1442320c4de7db08353bab8963a2b57f365eebcdd3a2d3562f8d7",
        size: 88_630,
    },
    ModelFile {
        name: "espeak-ng-data/phondata",
        urls: kokoro_urls!("espeak-ng-data/phondata"),
        sha256: "4e0288957874029a8c3c9f41a8f517ad4bf18127046decbdd4b9d1d6807ce3a3",
        size: 550_424,
    },
    ModelFile {
        name: "espeak-ng-data/phonindex",
        urls: kokoro_urls!("espeak-ng-data/phonindex"),
        sha256: "3ca7b8fa3b42624e4b0f152707e7a39245fce569aa99ea47c055d9e622fcf0c4",
        size: 39_074,
    },
    ModelFile {
        name: "espeak-ng-data/phontab",
        urls: kokoro_urls!("espeak-ng-data/phontab"),
        sha256: "886f3fa402cb0ba73d483aa8ad000af47a6b7cc06293c75a97913fba68a530f6",
        size: 55_796,
    },
    ModelFile {
        name: "espeak-ng-data/intonations",
        urls: kokoro_urls!("espeak-ng-data/intonations"),
        sha256: "3f8af65fd3eda9759a10f021d61361c120871f463515229c925995c7f90918cc",
        size: 2_040,
    },
    ModelFile {
        name: "espeak-ng-data/en_dict",
        urls: kokoro_urls!("espeak-ng-data/en_dict"),
        sha256: "71bd330ba8a2e3e8076e631508208ef49449d6147c17b7bd2b4b1e1468292e35",
        size: 166_944,
    },
    ModelFile {
        name: "espeak-ng-data/cmn_dict",
        urls: kokoro_urls!("espeak-ng-data/cmn_dict"),
        sha256: "109aaa7708d3727382acb3ae41d8e2094a7e2bb9f651a81835be22a6f08071fe",
        size: 1_566_335,
    },
    ModelFile {
        name: "espeak-ng-data/yue_dict",
        urls: kokoro_urls!("espeak-ng-data/yue_dict"),
        sha256: "1d26afa203034698772107abfff1b53acbee30434700a4d2b75dee98951588f8",
        size: 563_571,
    },
    ModelFile {
        name: "espeak-ng-data/lang/gmw/en",
        urls: kokoro_urls!("espeak-ng-data/lang/gmw/en"),
        sha256: "4605d5330801de3641c6e366d15f129ea1f5ffbce8722642aba01ace07ab9c83",
        size: 140,
    },
    ModelFile {
        name: "espeak-ng-data/lang/gmw/en-US",
        urls: kokoro_urls!("espeak-ng-data/lang/gmw/en-US"),
        sha256: "41534c2a22df5dd4f1052ff9e1a33a3ea7bff5a26b5c02bdad5ba8ddb7524704",
        size: 257,
    },
    ModelFile {
        name: "espeak-ng-data/lang/sit/cmn",
        urls: kokoro_urls!("espeak-ng-data/lang/sit/cmn"),
        sha256: "a130a38e3b622735c2401cb37f6719f46995e84a133729185cef9a543ec0b072",
        size: 686,
    },
    ModelFile {
        name: "espeak-ng-data/lang/sit/yue",
        urls: kokoro_urls!("espeak-ng-data/lang/sit/yue"),
        sha256: "83f3dac92af5b4aaecc119515698a6979524be236b28ea8f1024ec707d99ea5b",
        size: 194,
    },
];

pub fn kokoro_model_total_size() -> u64 {
    KOKORO_MODEL_FILES.iter().map(|file| file.size).sum()
}

/// MeloTTS zh_en: a fast Chinese + English VITS model (MIT), used as the
/// "流畅" local option. It runs at 44.1 kHz and is much faster than Kokoro on
/// small laptop CPUs.
macro_rules! vits_zh_urls {
    ($name:literal) => {
        &[
            concat!(
                "https://hf-mirror.com/csukuangfj/vits-melo-tts-zh_en/resolve/main/",
                $name
            ),
            concat!(
                "https://huggingface.co/csukuangfj/vits-melo-tts-zh_en/resolve/main/",
                $name
            ),
        ]
    };
}

pub const VITS_ZH_MODEL_DIR: &str = "vits-melo-tts-zh_en";
pub const VITS_ZH_MODEL_FILES: [ModelFile; 6] = [
    ModelFile {
        name: "model.onnx",
        urls: vits_zh_urls!("model.onnx"),
        sha256: "bf30582eb1b012250a35b1a4a80e7dfbcf8485e7bb9de0d95efbbeef0e4ad86d",
        size: 170_429_550,
    },
    ModelFile {
        name: "tokens.txt",
        urls: vits_zh_urls!("tokens.txt"),
        sha256: "d18664a7e12bd7ea1022ddaf951e534e136815016c5a809d6b64156bffb4369d",
        size: 655,
    },
    ModelFile {
        name: "lexicon.txt",
        urls: vits_zh_urls!("lexicon.txt"),
        sha256: "7236884b02435ac5d10cf69b4be40a61b45aa676b5300f0e412f185748fee528",
        size: 6_837_671,
    },
    ModelFile {
        name: "date.fst",
        urls: vits_zh_urls!("date.fst"),
        sha256: "eb8aa079ae3cb81d8f4404992f39d61a0cb990947512b5b8d1e54d1f6980e718",
        size: 59_154,
    },
    ModelFile {
        name: "number.fst",
        urls: vits_zh_urls!("number.fst"),
        sha256: "743f402181fcfebf76cc2f0546b71fa26476e626fbe4e460fb7b4c3a7a8bd5bd",
        size: 64_482,
    },
    ModelFile {
        name: "phone.fst",
        urls: vits_zh_urls!("phone.fst"),
        sha256: "1ac2b6fa56b1442320c4de7db08353bab8963a2b57f365eebcdd3a2d3562f8d7",
        size: 88_630,
    },
];

pub fn vits_zh_model_total_size() -> u64 {
    VITS_ZH_MODEL_FILES.iter().map(|file| file.size).sum()
}

/// Supertonic 3: 31 languages (no Chinese), 10 speakers, 44.1 kHz, OpenRAIL.
/// This is the default engine for non-Chinese languages.
macro_rules! supertonic_urls {
    ($name:literal) => {
        &[
            concat!(
                "https://hf-mirror.com/csukuangfj2/sherpa-onnx-supertonic-3-tts-int8-2026-05-11/resolve/main/",
                $name
            ),
            concat!(
                "https://huggingface.co/csukuangfj2/sherpa-onnx-supertonic-3-tts-int8-2026-05-11/resolve/main/",
                $name
            ),
        ]
    };
}

pub const SUPERTONIC_MODEL_DIR: &str = "sherpa-onnx-supertonic-3-tts-int8-2026-05-11";
pub const SUPERTONIC_MODEL_FILES: [ModelFile; 8] = [
    ModelFile {
        name: "duration_predictor.int8.onnx",
        urls: supertonic_urls!("duration_predictor.int8.onnx"),
        sha256: "c3eb91414d5ff8a7a239b7fe9e34e7e2bf8a8140d8375ffb14718b1c639325db",
        size: 3_700_147,
    },
    ModelFile {
        name: "text_encoder.int8.onnx",
        urls: supertonic_urls!("text_encoder.int8.onnx"),
        sha256: "c7befd5ea8c3119769e8a6c1486c4edc6a3bc8365c67621c881bbb774b9902ff",
        size: 36_416_150,
    },
    ModelFile {
        name: "vector_estimator.int8.onnx",
        urls: supertonic_urls!("vector_estimator.int8.onnx"),
        sha256: "20cd86fa5c6effedfda0e7cffe5b0569ca401c440a0c3a1d72bf39286c0db3fd",
        size: 78_400_833,
    },
    ModelFile {
        name: "vocoder.int8.onnx",
        urls: supertonic_urls!("vocoder.int8.onnx"),
        sha256: "e923d60f53f95eb1ce235f1dc33ec56d9c057823c96fa6f8acf98f32b0da6152",
        size: 25_991_073,
    },
    ModelFile {
        name: "tts.json",
        urls: supertonic_urls!("tts.json"),
        sha256: "42078d3aef1cd43ab43021f3c54f47d2d75ceb4e75f627f118890128b06a0d09",
        size: 8_253,
    },
    ModelFile {
        name: "unicode_indexer.bin",
        urls: supertonic_urls!("unicode_indexer.bin"),
        sha256: "8402ca48e5189a8950138580b0fff64db6f072f24ac07cd54ba8b2fbb9883b30",
        size: 262_144,
    },
    ModelFile {
        name: "voice.bin",
        urls: supertonic_urls!("voice.bin"),
        sha256: "67d5209b0ee8ce6c74105ffbe12fe6a7628aea3b4ba2fcb308a4a67938a93ce8",
        size: 517_168,
    },
    ModelFile {
        name: "LICENSE",
        urls: supertonic_urls!("LICENSE"),
        sha256: "0dfe0d0ba84416fe3879d9a34f4909d8d0137c78d1e95834177b0414ac096fa2",
        size: 1_070,
    },
];

pub fn supertonic_model_total_size() -> u64 {
    SUPERTONIC_MODEL_FILES.iter().map(|file| file.size).sum()
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

fn is_safe_archive_path(path: &Path) -> bool {
    !path.is_absolute()
        && !path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::RootDir))
}

/// Extracts a `.tar.bz2` archive into `dest_dir`.
///
/// Only regular files and directories are unpacked; absolute paths, `..`
/// components and links are rejected so a model archive cannot escape the
/// destination directory.
pub fn extract_tar_bz2(archive: &Path, dest_dir: &Path) -> Result<(), ModelError> {    let file = std::fs::File::open(archive).map_err(|error| ModelError::Io(error.to_string()))?;
    let decoder = bzip2::read::BzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);

    std::fs::create_dir_all(dest_dir).map_err(|error| ModelError::Io(error.to_string()))?;

    let entries = archive
        .entries()
        .map_err(|error| ModelError::Io(format!("invalid archive: {error}")))?;

    for entry in entries {
        let mut entry =
            entry.map_err(|error| ModelError::Io(format!("invalid archive entry: {error}")))?;
        let path = entry
            .path()
            .map_err(|error| ModelError::Io(format!("invalid archive path: {error}")))?
            .into_owned();

        if !is_safe_archive_path(&path) {
            return Err(ModelError::Io(format!(
                "unsafe archive path: {}",
                path.display()
            )));
        }

        let kind = entry.header().entry_type();

        if kind.is_symlink() || kind.is_hard_link() {
            return Err(ModelError::Io(format!(
                "archive link is not allowed: {}",
                path.display()
            )));
        }

        if kind.is_dir() || kind.is_file() {
            entry.unpack_in(dest_dir).map_err(|error| {
                ModelError::Io(format!("failed to unpack {}: {error}", path.display()))
            })?;
        }
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

    fn temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "translator-core-model-test-{}-{name}",
            std::process::id()
        ))
    }

    fn write_tar_bz2(path: &Path, files: &[(&str, &[u8])]) {
        let file = std::fs::File::create(path).unwrap();
        let encoder = bzip2::write::BzEncoder::new(file, bzip2::Compression::best());
        let mut builder = tar::Builder::new(encoder);

        for (name, bytes) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append_data(&mut header, name, *bytes).unwrap();
        }

        let encoder = builder.into_inner().unwrap();
        encoder.finish().unwrap();
    }

    #[test]
    fn kokoro_model_specs_are_well_formed() {
        assert_eq!(KOKORO_MODEL_FILES.len(), 19);
        assert!(kokoro_model_total_size() > 170_000_000);

        for file in &KOKORO_MODEL_FILES {
            assert_eq!(file.sha256.len(), 64);
            assert!(file.sha256.chars().all(|c| c.is_ascii_hexdigit()));
            assert!(!file.urls.is_empty());
            assert!(file.urls.iter().all(|url| url.starts_with("https://")));
            assert!(file.size > 0);
        }
    }

    #[test]
    fn vits_model_specs_are_well_formed() {
        assert_eq!(VITS_ZH_MODEL_FILES.len(), 6);
        assert!(vits_zh_model_total_size() > 170_000_000);

        for file in &VITS_ZH_MODEL_FILES {
            assert_eq!(file.sha256.len(), 64);
            assert!(file.sha256.chars().all(|c| c.is_ascii_hexdigit()));
            assert!(!file.urls.is_empty());
            assert!(file.urls.iter().all(|url| url.starts_with("https://")));
            assert!(file.size > 0);
        }
    }

    #[test]
    fn supertonic_model_specs_are_well_formed() {
        assert_eq!(SUPERTONIC_MODEL_FILES.len(), 8);
        assert!(supertonic_model_total_size() > 140_000_000);

        for file in &SUPERTONIC_MODEL_FILES {
            assert_eq!(file.sha256.len(), 64);
            assert!(file.sha256.chars().all(|c| c.is_ascii_hexdigit()));
            assert!(!file.urls.is_empty());
            assert!(file.urls.iter().all(|url| url.starts_with("https://")));
            assert!(file.size > 0);
        }
    }

    #[test]
    fn extracts_tar_bz2_archives() {
        let dir = temp_dir("archive");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let archive = dir.join("model.tar.bz2");
        write_tar_bz2(
            &archive,
            &[
                ("kokoro/model.int8.onnx", b"model"),
                ("kokoro/voices.bin", b"voices"),
                ("kokoro/espeak-ng-data/phontab", b"phontab"),
            ],
        );

        let output = dir.join("out");
        extract_tar_bz2(&archive, &output).unwrap();

        assert_eq!(
            std::fs::read(output.join("kokoro/model.int8.onnx")).unwrap(),
            b"model"
        );
        assert_eq!(
            std::fs::read(output.join("kokoro/voices.bin")).unwrap(),
            b"voices"
        );
        assert!(output.join("kokoro/espeak-ng-data/phontab").is_file());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_archive_path_traversal() {
        assert!(!is_safe_archive_path(Path::new("../escape.txt")));
        assert!(!is_safe_archive_path(Path::new("kokoro/../../escape.txt")));
        assert!(!is_safe_archive_path(Path::new("/etc/passwd")));
        assert!(is_safe_archive_path(Path::new(
            "kokoro/model.int8.onnx"
        )));
    }
}
