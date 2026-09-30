use std::sync::Arc;
use std::time::Duration;

use translator_service::api::{AppState, router};
use translator_service::engine::llama_cpp::LlamaCppEngine;
use translator_service::engine::{EngineRef, TimeoutEngine};

const TIMEOUT: Duration = Duration::from_secs(30);
const MAX_CHARS: usize = 1500;

pub fn start(
    engine: Arc<LlamaCppEngine>,
    bind_addr: String,
    model_name: String,
) -> Result<(), String> {
    let listener = std::net::TcpListener::bind(&bind_addr)
        .map_err(|error| format!("cannot bind {bind_addr}: {error}"))?;

    listener
        .set_nonblocking(true)
        .map_err(|error| format!("cannot configure listener: {error}"))?;

    std::thread::Builder::new()
        .name("translator-http".to_string())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    eprintln!("extension server runtime failed: {error}");
                    return;
                }
            };

            runtime.block_on(async move {
                let listener = match tokio::net::TcpListener::from_std(listener) {
                    Ok(listener) => listener,
                    Err(error) => {
                        eprintln!("extension server listener failed: {error}");
                        return;
                    }
                };

                let engine_ref: EngineRef = Arc::new(TimeoutEngine::new(engine, TIMEOUT));
                let state = AppState::new(engine_ref, "llama-cpp", model_name, MAX_CHARS);

                if let Err(error) = axum::serve(listener, router(state)).await {
                    eprintln!("extension server stopped: {error}");
                }
            });
        })
        .map_err(|error| format!("cannot start extension server thread: {error}"))?;

    Ok(())
}
