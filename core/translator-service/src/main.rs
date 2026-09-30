use std::sync::Arc;

use translator_service::{api, engine::mock::MockEngine};

#[tokio::main]
async fn main() {
    let app = api::router(Arc::new(MockEngine));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:17890")
        .await
        .unwrap();

    println!("OpenTranslator Core Started");
    println!("Listening on http://127.0.0.1:17890");

    axum::serve(listener, app).await.unwrap();
}
