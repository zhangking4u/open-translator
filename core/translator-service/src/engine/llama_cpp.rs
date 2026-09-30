use std::sync::Arc;

use translator_inference::{GenerateOptions as InferenceOptions, InferenceEngine, LoadOptions};

use super::{TranslationEngine, TranslationFuture};
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

    pub fn translate_blocking(
        &self,
        request: &TranslationRequest,
    ) -> Result<TranslationResult, TranslationError> {
        let prompt = self
            .prompt_style
            .raw_prompt(&translation_prompt(self.prompt_style, request)?);
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

        let generation = self
            .engine
            .generate(&prompt, &stop_strings, &options)
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
}
