use std::io::Cursor;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use base64::Engine as _;
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

use translator_ocr::{OcrBlock, OcrEngine, OcrError, Quad};
use translator_service::api::{self, AppState};
use translator_service::domain::translation::{
    TranslationError, TranslationRequest, TranslationResult,
};
use translator_service::engine::mock::MockEngine;
use translator_service::engine::{EngineRef, TimeoutEngine, TranslationEngine, TranslationFuture};

fn router_for(engine: EngineRef) -> axum::Router {
    api::router(AppState::new(engine, "mock", "", 1500))
}

fn app() -> axum::Router {
    router_for(Arc::new(MockEngine))
}

struct FailingEngine;

impl TranslationEngine for FailingEngine {
    fn translate(&self, _request: TranslationRequest) -> TranslationFuture<'_> {
        Box::pin(async {
            Err(TranslationError::EngineUnavailable(
                "model offline".to_string(),
            ))
        })
    }
}

fn failing_app() -> axum::Router {
    router_for(Arc::new(FailingEngine))
}

struct SlowEngine;

impl TranslationEngine for SlowEngine {
    fn translate(&self, _request: TranslationRequest) -> TranslationFuture<'_> {
        Box::pin(async {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(TranslationResult {
                translated_text: "late".to_string(),
            })
        })
    }
}

async fn body_json(response: axum::response::Response) -> Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

fn translate_request(body: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/translate")
        .header("content-type", "application/json")
        .body(Body::from(body.to_owned()))
        .unwrap()
}

#[tokio::test]
async fn health_returns_service_status() {
    let response = app()
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let json = body_json(response).await;
    assert_eq!(json["status"], "ok");
    assert_eq!(json["service"], "translator-core");
    assert_eq!(json["engine"], "mock");
    assert_eq!(json["model"], "");
}

#[tokio::test]
async fn translate_returns_mock_translation() {
    let response = app()
        .oneshot(translate_request(
            r#"{"text":"hello world","source":"en","target":"zh"}"#,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let json = body_json(response).await;
    assert_eq!(json["translation"], "[Mock Translation] hello world");
}

#[tokio::test]
async fn translate_accepts_a_glossary() {
    let response = app()
        .oneshot(translate_request(
            r#"{"text":"kernel panic","source":"en","target":"zh","glossary":[{"source":"kernel","target":"内核"}]}"#,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let json = body_json(response).await;
    assert_eq!(json["translation"], "[Mock Translation] kernel panic");
}

#[tokio::test]
async fn malformed_glossary_is_rejected() {
    let response = app()
        .oneshot(translate_request(
            r#"{"text":"hello","source":"en","target":"zh","glossary":[{"source":"kernel"}]}"#,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn empty_text_is_rejected() {
    let response = app()
        .oneshot(translate_request(
            r#"{"text":"   ","source":"en","target":"zh"}"#,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let json = body_json(response).await;
    assert_eq!(json["error"]["kind"], "invalid_request");
}

#[tokio::test]
async fn oversized_text_is_rejected() {
    let small: EngineRef = Arc::new(MockEngine);
    let app = api::router(AppState::new(small, "mock", "", 10));

    let response = app
        .oneshot(translate_request(
            r#"{"text":"this text is definitely longer than ten chars","source":"en","target":"zh"}"#,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let json = body_json(response).await;
    assert_eq!(json["error"]["kind"], "invalid_request");
}

#[tokio::test]
async fn engine_failure_returns_bad_gateway() {
    let response = failing_app()
        .oneshot(translate_request(
            r#"{"text":"hello","source":"en","target":"zh"}"#,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);

    let json = body_json(response).await;
    assert_eq!(json["error"]["kind"], "engine_unavailable");
}

#[tokio::test]
async fn slow_engine_returns_gateway_timeout() {
    let slow: EngineRef = Arc::new(TimeoutEngine::new(
        Arc::new(SlowEngine),
        Duration::from_millis(10),
    ));

    let response = router_for(slow)
        .oneshot(translate_request(
            r#"{"text":"hello","source":"en","target":"zh"}"#,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::GATEWAY_TIMEOUT);

    let json = body_json(response).await;
    assert_eq!(json["error"]["kind"], "timeout");
}

fn stream_request(body: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/translate/stream")
        .header("content-type", "application/json")
        .body(Body::from(body.to_owned()))
        .unwrap()
}

async fn body_text(response: axum::response::Response) -> String {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).to_string()
}

#[tokio::test]
async fn stream_returns_sse_deltas_and_done() {
    let response = app()
        .oneshot(stream_request(
            r#"{"text":"hello","source":"en","target":"zh"}"#,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response.headers()["content-type"]
            .to_str()
            .unwrap()
            .starts_with("text/event-stream")
    );

    let body = body_text(response).await;

    assert!(body.contains("\"type\":\"delta\""));
    assert!(body.contains("[Mock Translation] hello"));
    assert!(body.contains("\"type\":\"done\""));
    assert!(body.contains("\"translation\":\"[Mock Translation] hello\""));
}

#[tokio::test]
async fn stream_rejects_empty_text() {
    let response = app()
        .oneshot(stream_request(r#"{"text":"   ","source":"en","target":"zh"}"#))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let json = body_json(response).await;
    assert_eq!(json["error"]["kind"], "invalid_request");
}

#[tokio::test]
async fn stream_reports_engine_errors() {
    let response = failing_app()
        .oneshot(stream_request(
            r#"{"text":"hello","source":"en","target":"zh"}"#,
        ))
        .await
        .unwrap();

    // The response has already started as SSE, so the failure is an event.
    assert_eq!(response.status(), StatusCode::OK);

    let body = body_text(response).await;

    assert!(body.contains("\"type\":\"error\""));
    assert!(body.contains("engine_unavailable"));
}

fn block(text: &str) -> OcrBlock {
    OcrBlock {
        text: text.to_string(),
        score: 0.99,
        quad: Quad {
            points: [[0.0, 0.0], [10.0, 0.0], [10.0, 5.0], [0.0, 5.0]],
        },
    }
}

struct FixedOcr {
    blocks: Vec<OcrBlock>,
}

impl OcrEngine for FixedOcr {
    fn recognize(
        &self,
        _pixels: &[u8],
        _width: u32,
        _height: u32,
    ) -> Result<Vec<OcrBlock>, OcrError> {
        Ok(self.blocks.clone())
    }
}

fn ocr_router(engine: EngineRef, blocks: Vec<OcrBlock>, max_chars: usize) -> axum::Router {
    api::router(
        AppState::new(engine, "mock", "", max_chars).with_ocr(Arc::new(FixedOcr { blocks })),
    )
}

fn tiny_png_base64() -> String {
    let image = image::RgbImage::from_pixel(2, 2, image::Rgb([255, 255, 255]));
    let mut bytes = Vec::new();

    image::DynamicImage::ImageRgb8(image)
        .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
        .unwrap();

    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn image_request(body: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/translate/image")
        .header("content-type", "application/json")
        .body(Body::from(body.to_owned()))
        .unwrap()
}

/// Prefixes every input line with `[zh] `, so joined-line round trips are
/// visible in the response.
struct EchoEngine;

impl TranslationEngine for EchoEngine {
    fn translate(&self, request: TranslationRequest) -> TranslationFuture<'_> {
        Box::pin(async move {
            let text = request
                .text
                .split('\n')
                .map(|line| format!("[zh] {line}"))
                .collect::<Vec<_>>()
                .join("\n");

            Ok(TranslationResult {
                translated_text: text,
            })
        })
    }
}

/// Merges multi-line input into a single line, forcing the per-block
/// fallback.
struct MergingEngine;

impl TranslationEngine for MergingEngine {
    fn translate(&self, request: TranslationRequest) -> TranslationFuture<'_> {
        Box::pin(async move {
            Ok(TranslationResult {
                translated_text: format!("[zh] {}", request.text.replace('\n', " ")),
            })
        })
    }
}

#[tokio::test]
async fn image_endpoint_requires_an_ocr_provider() {
    let body = format!(
        r#"{{"image":"{}","source":"en","target":"zh"}}"#,
        tiny_png_base64()
    );

    let response = app().oneshot(image_request(&body)).await.unwrap();

    assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);

    let json = body_json(response).await;
    assert_eq!(json["error"]["kind"], "ocr_unavailable");
}

#[tokio::test]
async fn image_endpoint_translates_recognized_blocks() {
    let app = ocr_router(
        Arc::new(EchoEngine),
        vec![block("START GAME"), block("设置")],
        1500,
    );
    let body = format!(
        r#"{{"image":"{}","source":"en","target":"zh"}}"#,
        tiny_png_base64()
    );

    let response = app.oneshot(image_request(&body)).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let json = body_json(response).await;
    assert_eq!(json["image"]["width"], 2);
    assert_eq!(json["image"]["height"], 2);
    assert_eq!(json["blocks"][0]["text"], "START GAME");
    assert_eq!(json["blocks"][0]["translation"], "[zh] START GAME");
    assert_eq!(json["blocks"][1]["translation"], "[zh] 设置");
    assert_eq!(json["blocks"][0]["score"], 0.99);
    assert_eq!(json["blocks"][0]["quad"][0][0], 0.0);
}

#[tokio::test]
async fn image_endpoint_accepts_a_data_url_payload() {
    let app = ocr_router(Arc::new(EchoEngine), vec![block("BACK")], 1500);
    let body = format!(
        r#"{{"image":"data:image/png;base64,{}","source":"en","target":"zh"}}"#,
        tiny_png_base64()
    );

    let response = app.oneshot(image_request(&body)).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let json = body_json(response).await;
    assert_eq!(json["blocks"][0]["translation"], "[zh] BACK");
}

#[tokio::test]
async fn image_endpoint_falls_back_to_per_block_translation() {
    let app = ocr_router(Arc::new(MergingEngine), vec![block("A"), block("B")], 1500);
    let body = format!(
        r#"{{"image":"{}","source":"en","target":"zh"}}"#,
        tiny_png_base64()
    );

    let response = app.oneshot(image_request(&body)).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let json = body_json(response).await;
    assert_eq!(json["blocks"][0]["translation"], "[zh] A");
    assert_eq!(json["blocks"][1]["translation"], "[zh] B");
}

#[tokio::test]
async fn image_endpoint_rejects_bad_base64() {
    let app = ocr_router(Arc::new(MockEngine), Vec::new(), 1500);
    let response = app
        .oneshot(image_request(
            r#"{"image":"!!!not-base64!!!","source":"en","target":"zh"}"#,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let json = body_json(response).await;
    assert_eq!(json["error"]["kind"], "invalid_request");
}

#[tokio::test]
async fn image_endpoint_rejects_a_non_image_payload() {
    let payload = base64::engine::general_purpose::STANDARD.encode(b"hello");
    let app = ocr_router(Arc::new(MockEngine), Vec::new(), 1500);
    let body = format!(r#"{{"image":"{payload}","source":"en","target":"zh"}}"#);

    let response = app.oneshot(image_request(&body)).await.unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let json = body_json(response).await;
    assert_eq!(json["error"]["kind"], "invalid_request");
    assert!(
        json["error"]["message"]
            .as_str()
            .unwrap()
            .contains("unrecognized image format")
    );
}

#[tokio::test]
async fn image_endpoint_rejects_too_much_text() {
    let app = ocr_router(
        Arc::new(MockEngine),
        vec![block("a block that exceeds the cap")],
        5,
    );
    let body = format!(
        r#"{{"image":"{}","source":"en","target":"zh"}}"#,
        tiny_png_base64()
    );

    let response = app.oneshot(image_request(&body)).await.unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let json = body_json(response).await;
    assert_eq!(json["error"]["kind"], "invalid_request");
    assert!(
        json["error"]["message"]
            .as_str()
            .unwrap()
            .contains("image text is too long")
    );
}

#[tokio::test]
async fn image_endpoint_accepts_an_empty_recognition() {
    let app = ocr_router(Arc::new(MockEngine), Vec::new(), 1500);
    let body = format!(
        r#"{{"image":"{}","source":"en","target":"zh"}}"#,
        tiny_png_base64()
    );

    let response = app.oneshot(image_request(&body)).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let json = body_json(response).await;
    assert!(json["blocks"].as_array().unwrap().is_empty());
}
