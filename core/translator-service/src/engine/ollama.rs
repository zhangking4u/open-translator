use serde::{Deserialize, Serialize};

use super::{TranslationEngine, TranslationFuture};
use crate::domain::prompt::{PromptStyle, translation_prompt};
use crate::domain::translation::{TranslationError, TranslationRequest, TranslationResult};

pub struct OllamaEngine {
    client: reqwest::Client,
    base_url: String,
    model: String,
    prompt_style: PromptStyle,
    keep_alive: String,
}

impl OllamaEngine {
    pub fn new(
        base_url: String,
        model: String,
        prompt_style: PromptStyle,
        keep_alive: String,
    ) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url,
            model,
            prompt_style,
            keep_alive,
        }
    }
}

#[derive(Serialize)]
struct GenerateRequest<'a> {
    model: &'a str,
    prompt: &'a str,
    stream: bool,
    keep_alive: &'a str,
    options: GenerateOptions,
}

#[derive(Serialize)]
struct GenerateOptions {
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_k: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    repeat_penalty: Option<f32>,
}

fn sampling_options(style: PromptStyle) -> GenerateOptions {
    let sampling = style.sampling();

    GenerateOptions {
        temperature: sampling.temperature,
        top_p: sampling.top_p,
        top_k: sampling.top_k,
        repeat_penalty: sampling.repeat_penalty,
    }
}

#[derive(Deserialize)]
struct GenerateResponse {
    response: String,
}

impl TranslationEngine for OllamaEngine {
    fn translate(&self, request: TranslationRequest) -> TranslationFuture<'_> {
        Box::pin(async move {
            let prompt = translation_prompt(self.prompt_style, &request)?;
            let url = format!("{}/api/generate", self.base_url);

            let response = self
                .client
                .post(&url)
                .json(&GenerateRequest {
                    model: &self.model,
                    prompt: &prompt,
                    stream: false,
                    keep_alive: &self.keep_alive,
                    options: sampling_options(self.prompt_style),
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
