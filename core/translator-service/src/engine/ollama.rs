use serde::{Deserialize, Serialize};

use super::{TranslationEngine, TranslationFuture};
use crate::domain::prompt::{PromptStyle, translation_prompt};
use crate::domain::translation::{TranslationError, TranslationRequest, TranslationResult};

pub struct OllamaEngine {
    client: reqwest::Client,
    base_url: String,
    model: String,
    prompt_style: PromptStyle,
}

impl OllamaEngine {
    pub fn new(base_url: String, model: String, prompt_style: PromptStyle) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url,
            model,
            prompt_style,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_k: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    repeat_penalty: Option<f32>,
}

fn sampling_options(style: PromptStyle) -> GenerateOptions {
    match style {
        PromptStyle::HunYuanMt => GenerateOptions {
            temperature: 0.7,
            top_p: Some(0.6),
            top_k: Some(20),
            repeat_penalty: Some(1.05),
        },
        PromptStyle::Generic | PromptStyle::TranslateGemma => GenerateOptions {
            temperature: 0.0,
            top_p: None,
            top_k: None,
            repeat_penalty: None,
        },
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
