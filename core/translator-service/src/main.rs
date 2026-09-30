use translator_service::{api, config::Config, engine};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = Config::from_env().unwrap_or_else(|error| {
        tracing::error!(error = %error, "configuration error");
        std::process::exit(1);
    });

    let engine_ref = engine::build(&config);

    if config.warmup {
        engine::warmup(&engine_ref).await;
    }

    let app = api::router(api::AppState::new(
        engine_ref,
        config.engine.as_str(),
        config.model.clone(),
    ));

    let listener = tokio::net::TcpListener::bind(&config.bind_addr)
        .await
        .unwrap_or_else(|error| {
            tracing::error!(bind_addr = %config.bind_addr, error = %error, "failed to bind");
            std::process::exit(1);
        });

    tracing::info!(
        bind_addr = %config.bind_addr,
        engine = config.engine.as_str(),
        model = %config.model,
        "OpenTranslator Core started"
    );

    axum::serve(listener, app).await.unwrap();
}
