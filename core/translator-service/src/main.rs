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

    let engine_ref = match engine::build(&config) {
        Ok(engine) => engine,
        Err(error) => {
            tracing::error!(error = %error, "failed to initialise engine");
            std::process::exit(1);
        }
    };

    if config.warmup {
        engine::warmup(&engine_ref).await;
    }

    let model_display = if config.model.is_empty() {
        std::path::Path::new(&config.model_path)
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default()
    } else {
        config.model.clone()
    };

    let app = api::router(api::AppState::new(
        engine_ref,
        config.engine.as_str(),
        model_display.clone(),
        config.max_chars,
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
        model = %model_display,
        "OpenTranslator Core started"
    );

    axum::serve(listener, app).await.unwrap();
}
