use translator_service::{api, config::Config, engine};

#[tokio::main]
async fn main() {
    let config = Config::from_env().unwrap_or_else(|error| {
        eprintln!("Configuration error: {error}");
        std::process::exit(1);
    });

    let app = api::router(engine::build(&config));

    let listener = tokio::net::TcpListener::bind(&config.bind_addr)
        .await
        .unwrap_or_else(|error| {
            eprintln!("Failed to bind {}: {error}", config.bind_addr);
            std::process::exit(1);
        });

    println!("OpenTranslator Core Started");
    println!("Listening on http://{}", config.bind_addr);

    axum::serve(listener, app).await.unwrap();
}
