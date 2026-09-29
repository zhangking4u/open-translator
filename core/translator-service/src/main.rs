use axum:: {
    routing::{get, post},
    Json, Router,
};

use serde::{Deserialize, Serialize};

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/health", get(health))
        .route("/translate", post(translate));

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
    Json(payload):Json<TranslateRequest>
)
-> Json<TranslateResponse>{
    println!(
        "Translate request: {}",
        payload.text
    );

    Json(
        TranslateResponse{
            translation:
            format!(
                "[TODO] {}",
                payload.text
            ),
        }
    )
}
