use std::sync::Arc;
use std::time::Duration;

use translator_service::api::{AppState, OcrEngineRef, router};
use translator_service::engine::llama_cpp::LlamaCppEngine;
use translator_service::engine::{EngineRef, TimeoutEngine};

const TIMEOUT: Duration = Duration::from_secs(30);
const MAX_CHARS: usize = 1500;

pub enum ServerError {
    AddrInUse(String),
    Other(String),
}

impl std::fmt::Display for ServerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AddrInUse(address) => write!(formatter, "{address} 已被占用"),
            Self::Other(error) => write!(formatter, "{error}"),
        }
    }
}

pub fn start(
    engine: Arc<LlamaCppEngine>,
    ocr: Option<OcrEngineRef>,
    bind_addr: String,
    model_name: String,
) -> Result<(), ServerError> {
    let listener = std::net::TcpListener::bind(&bind_addr).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AddrInUse {
            ServerError::AddrInUse(bind_addr.clone())
        } else {
            ServerError::Other(format!("cannot bind {bind_addr}: {error}"))
        }
    })?;

    listener
        .set_nonblocking(true)
        .map_err(|error| ServerError::Other(format!("cannot configure listener: {error}")))?;

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
                let mut state = AppState::new(engine_ref, "llama-cpp", model_name, MAX_CHARS);

                if let Some(ocr) = ocr {
                    state = state.with_ocr(ocr);
                }

                if let Err(error) = axum::serve(listener, router(state)).await {
                    eprintln!("extension server stopped: {error}");
                }
            });
        })
        .map_err(|error| ServerError::Other(format!("cannot start extension server thread: {error}")))?;

    Ok(())
}
