mod api;
mod domain;
mod engine;


use axum:: {
    routing::{get, post},
    Json, Router,
};

use serde::{Deserialize, Serialize};

use domain::translation::{
    TranslationRequest as DomainTranslationRequest,
};

use engine::{
    TranslationEngine,
};

use engine::mock::MockEngine;

use std::sync::Arc;

#[tokio::main]
async fn main() {
    let engine = Arc::new(MockEngine);

    let app = Router::new()
        .route("/health", get(health))
        .route(
            "/translate", 
            post(translate)
        )
        .with_state(engine);

    let listener = 
        tokio::net::TcpListener::bind("127.0.0.1:17890")
        .await
        .unwrap();

    println!("OpenTranslator Core Started");
    println!("Listening on http://127.0.0.1:17890");

    axum::serve(listener, app)
        .await
        .unwrap();
}

async fn health() -> Json<HealthResponse>{
    Json(
        HealthResponse{
            status:"ok",
            service:"translator-core",
        }
    )
}

#[derive(Serialize)]
struct HealthResponse {
    status:&'static str,
    service:&'static str,
}

#[derive(Deserialize)]
struct TranslateRequest {
    text:String,
    source:String,
    target:String,
}

#[derive(Serialize)]
struct TranslateResponse {
    translation:String,
}

async fn translate(
    axum::extract::State(engine):
        axum::extract::State<Arc<MockEngine>>,

    Json(payload):Json<TranslateRequest>
)
-> Json<TranslateResponse>{
    println!(
        "Translate request: {}",
        payload.text
    );

    let request = 
        DomainTranslationRequest{
            text: payload.text,
            source: payload.source,
            target: payload.target,
        };

    let result = 
        engine.translate(request);

    Json(
        TranslateResponse{
            translation:
                result.translated_text,
        }
    )
}
