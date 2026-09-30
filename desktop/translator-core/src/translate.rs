use std::time::Duration;

use serde::{Deserialize, Serialize};

pub fn build_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|error| format!("failed to create HTTP client: {error}"))
}

#[derive(Serialize)]
struct TranslateRequest<'a> {
    text: &'a str,
    source: &'a str,
    target: &'a str,
}

#[derive(Deserialize)]
struct TranslateResponse {
    translation: String,
}

#[derive(Deserialize)]
struct ErrorResponse {
    error: ErrorBody,
}

#[derive(Deserialize)]
struct ErrorBody {
    kind: String,
    message: String,
}

pub async fn translate(
    client: &reqwest::Client,
    service_url: &str,
    source: &str,
    target: &str,
    text: &str,
) -> Result<String, String> {
    let url = format!("{}/translate", service_url.trim_end_matches('/'));

    let response = client
        .post(&url)
        .json(&TranslateRequest {
            text,
            source,
            target,
        })
        .send()
        .await
        .map_err(|error| format!("cannot reach translator service at {url}: {error}"))?;

    let status = response.status();

    if status.is_success() {
        let payload: TranslateResponse = response
            .json()
            .await
            .map_err(|error| format!("invalid service response: {error}"))?;
        return Ok(payload.translation);
    }

    let body = response.text().await.unwrap_or_default();

    if let Ok(error) = serde_json::from_str::<ErrorResponse>(&body) {
        return Err(format!("{}: {}", error.error.kind, error.error.message));
    }

    Err(format!("service returned {status}: {body}"))
}
