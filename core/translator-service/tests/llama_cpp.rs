use translator_service::domain::prompt::PromptStyle;
use translator_service::domain::translation::TranslationRequest;
use translator_service::engine::TranslationEngine;
use translator_service::engine::llama_cpp::LlamaCppEngine;

#[test]
fn reports_missing_model() {
    let result = LlamaCppEngine::load("/nonexistent/model.gguf", PromptStyle::HunYuanMt, 4096);

    assert!(result.is_err());
}

#[tokio::test]
async fn translates_with_env_model() {
    let Ok(model_path) = std::env::var("TRANSLATOR_TEST_MODEL") else {
        return;
    };

    let engine = LlamaCppEngine::load(&model_path, PromptStyle::HunYuanMt, 4096).unwrap();

    let result = engine
        .translate(TranslationRequest {
            text: "kernel panic".to_string(),
            source: "en".to_string(),
            target: "zh".to_string(),
            glossary: Vec::new(),
        })
        .await
        .unwrap();

    assert!(
        result.translated_text.contains("内核"),
        "unexpected output: {}",
        result.translated_text
    );
}
