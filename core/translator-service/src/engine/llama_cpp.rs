use std::sync::Arc;

use translator_inference::{GenerateOptions as InferenceOptions, InferenceEngine, LoadOptions};

use super::{DeltaCallback, TranslationEngine, TranslationFuture};
use crate::domain::prompt::{PromptStyle, translation_prompt};
use crate::domain::translation::{TranslationError, TranslationRequest, TranslationResult};

const MAX_NEW_TOKENS: usize = 512;

#[derive(Clone)]
pub struct LlamaCppEngine {
    engine: Arc<InferenceEngine>,
    prompt_style: PromptStyle,
}

impl LlamaCppEngine {
    pub fn load(
        model_path: &str,
        prompt_style: PromptStyle,
        n_ctx: u32,
    ) -> Result<Self, String> {
        let engine = InferenceEngine::load(
            model_path,
            LoadOptions {
                n_ctx,
                ..LoadOptions::default()
            },
        )
        .map_err(|error| error.to_string())?;

        Ok(Self {
            engine: Arc::new(engine),
            prompt_style,
        })
    }

    fn inference_request(
        &self,
        request: &TranslationRequest,
    ) -> Result<(String, Vec<String>, InferenceOptions), TranslationError> {
        self.inference_request_with_hint(request, None)
    }

    fn inference_request_with_hint(
        &self,
        request: &TranslationRequest,
        hint: Option<&str>,
    ) -> Result<(String, Vec<String>, InferenceOptions), TranslationError> {
        let mut prompt = translation_prompt(self.prompt_style, request)?;

        if let Some(hint) = hint {
            prompt = format!("{hint}\n{prompt}");
        }

        let prompt = self.prompt_style.raw_prompt(&prompt);
        let stop_strings = self.prompt_style.stop_strings();
        let sampling = self.prompt_style.sampling();

        let options = InferenceOptions {
            max_tokens: MAX_NEW_TOKENS,
            temperature: sampling.temperature,
            top_k: sampling.top_k.map(|value| value as i32).unwrap_or(40),
            top_p: sampling.top_p.unwrap_or(0.95),
            repeat_penalty: sampling.repeat_penalty.unwrap_or(1.0),
            ..InferenceOptions::default()
        };

        Ok((prompt, stop_strings, options))
    }

    pub fn translate_blocking(
        &self,
        request: &TranslationRequest,
    ) -> Result<TranslationResult, TranslationError> {
        let (prompt, stop_strings, options) = self.inference_request(request)?;

        let generation = self
            .engine
            .generate(&prompt, &stop_strings, &options)
            .map_err(|error| TranslationError::EngineUnavailable(error.to_string()))?;

        Ok(TranslationResult {
            translated_text: generation.text,
        })
    }

    /// Re-runs the same request with a "say it another way" hint and varied
    /// sampling, producing another plausible rendering from the same model
    /// rather than a different translation mode.
    pub fn translate_alternative(
        &self,
        request: &TranslationRequest,
        seed: u32,
    ) -> Result<TranslationResult, TranslationError> {
        let hint = match self.prompt_style {
            PromptStyle::HunYuanMt => {
                "请换一种更自然、更口语化的说法，不要逐字照搬原句结构。"
            }
            PromptStyle::Generic | PromptStyle::TranslateGemma => {
                "Give a different, more natural and idiomatic rendering than a literal translation."
            }
        };
        let (prompt, stop_strings, mut options) =
            self.inference_request_with_hint(request, Some(hint))?;

        options.temperature = 0.9;
        options.top_p = 0.9;
        options.top_k = 40;
        options.repeat_penalty = 1.02;
        options.seed = seed;

        let generation = self
            .engine
            .generate(&prompt, &stop_strings, &options)
            .map_err(|error| TranslationError::EngineUnavailable(error.to_string()))?;

        Ok(TranslationResult {
            translated_text: generation.text,
        })
    }

    /// Like [`Self::translate_blocking`], but reports the translation token by
    /// token through `on_delta` while it is generated.
    pub fn translate_blocking_streaming(
        &self,
        request: &TranslationRequest,
        on_delta: impl FnMut(&str) + Send + 'static,
    ) -> Result<TranslationResult, TranslationError> {
        let (prompt, stop_strings, options) = self.inference_request(request)?;

        let generation = self
            .engine
            .generate_streaming(&prompt, &stop_strings, &options, on_delta)
            .map_err(|error| TranslationError::EngineUnavailable(error.to_string()))?;

        Ok(TranslationResult {
            translated_text: generation.text,
        })
    }
}

impl TranslationEngine for LlamaCppEngine {
    fn translate(&self, request: TranslationRequest) -> TranslationFuture<'_> {
        let engine = self.clone();

        Box::pin(async move {
            tokio::task::spawn_blocking(move || engine.translate_blocking(&request))
                .await
                .map_err(|error| {
                    TranslationError::Internal(format!("inference task failed: {error}"))
                })?
        })
    }

    fn translate_streaming<'a>(
        &'a self,
        request: TranslationRequest,
        on_delta: DeltaCallback,
    ) -> TranslationFuture<'a> {
        let engine = self.clone();

        Box::pin(async move {
            tokio::task::spawn_blocking(move || {
                engine.translate_blocking_streaming(&request, on_delta)
            })
            .await
            .map_err(|error| TranslationError::Internal(format!("inference task failed: {error}")))?
        })
    }
}
