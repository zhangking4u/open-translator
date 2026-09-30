use serde::{Deserialize, Serialize};

use super::{TranslationEngine, TranslationFuture};
use crate::domain::prompt::translation_prompt;
use crate::domain::translation::{TranslationError, TranslationRequest, TranslationResult};

pub struct OllamaEngine {
    client: reqwest::Client,
    base_url: String,
    model: String,
}

impl OllamaEngine {
    pub fn new(base_url: String, model: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url,
            model,
        }
    }
}

#[derive(Serialize)]
struct GenerateRequest<'a> {
    model: &'a str,
    prompt: &'a str,
    stream: bool,
    options: GenerateOptions,
}

#[derive(Serialize)]
struct GenerateOptions {
    temperature: f32,
}

#[derive(Deserialize)]
struct GenerateResponse {
    response: String,
}

impl TranslationEngine for OllamaEngine {
    fn translate(&self, request: TranslationRequest) -> TranslationFuture<'_> {
        Box::pin(async move {
            let prompt = translation_prompt(&request)?;
            let url = format!("{}/api/generate", self.base_url);

            let response = self
                .client
                .post(&url)
                .json(&GenerateRequest {
                    model: &self.model,
                    prompt: &prompt,
                    stream: false,
                    options: GenerateOptions { temperature: 0.0 },
                })
                .send()
                .await
                .map_err(|error| {
                    TranslationError::EngineUnavailable(format!("ollama request failed: {error}"))
                })?;

            let status = response.status();

            if !status.is_success() {
                let body = response.text().await.unwrap_or_default();
                return Err(TranslationError::EngineUnavailable(format!(
                    "ollama returned {status}: {body}"
                )));
            }

            let payload: GenerateResponse = response.json().await.map_err(|error| {
                TranslationError::Internal(format!("invalid ollama response: {error}"))
            })?;

            Ok(TranslationResult {
                translated_text: payload.response.trim().to_string(),
            })
        })
    }
}
