use std::fmt;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;

const DEFAULT_N_CTX: u32 = 4096;

#[derive(Debug, Clone)]
pub struct LoadOptions {
    pub n_ctx: u32,
    pub n_gpu_layers: u32,
}

impl Default for LoadOptions {
    fn default() -> Self {
        Self {
            n_ctx: DEFAULT_N_CTX,
            n_gpu_layers: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GenerateOptions {
    pub max_tokens: usize,
    pub temperature: f32,
    pub top_k: i32,
    pub top_p: f32,
    pub repeat_penalty: f32,
    pub seed: u32,
}

impl Default for GenerateOptions {
    fn default() -> Self {
        Self {
            max_tokens: 256,
            temperature: 0.0,
            top_k: 40,
            top_p: 0.95,
            repeat_penalty: 1.0,
            seed: 42,
        }
    }
}

#[derive(Debug)]
pub struct Generation {
    pub text: String,
    pub tokens: usize,
    pub elapsed: Duration,
}

#[derive(Debug)]
pub enum InferenceError {
    Load(String),
    Generate(String),
    Worker(String),
}

impl fmt::Display for InferenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Load(message) => write!(formatter, "model load failed: {message}"),
            Self::Generate(message) => write!(formatter, "generation failed: {message}"),
            Self::Worker(message) => write!(formatter, "inference worker failed: {message}"),
        }
    }
}

impl std::error::Error for InferenceError {}

pub struct InferenceEngine {
    sender: Mutex<Sender<Request>>,
}

struct Request {
    prompt: String,
    stop_strings: Vec<String>,
    options: GenerateOptions,
    respond: Sender<Result<Generation, InferenceError>>,
    on_delta: Option<Box<dyn FnMut(&str) + Send>>,
}

impl InferenceEngine {
    pub fn load(model_path: impl AsRef<Path>, options: LoadOptions) -> Result<Self, InferenceError> {
        let model_path = model_path.as_ref().to_path_buf();
        let (sender, receiver) = channel();
        let (ready_tx, ready_rx) = channel();

        std::thread::Builder::new()
            .name("translator-inference".to_string())
            .spawn(move || worker(model_path, options, receiver, ready_tx))
            .map_err(|error| InferenceError::Load(format!("failed to spawn worker: {error}")))?;

        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                sender: Mutex::new(sender),
            }),
            Ok(Err(error)) => Err(error),
            Err(_) => Err(InferenceError::Load(
                "inference worker exited during startup".to_string(),
            )),
        }
    }

    pub fn generate(
        &self,
        prompt: &str,
        stop_strings: &[String],
        options: &GenerateOptions,
    ) -> Result<Generation, InferenceError> {
        self.request(prompt, stop_strings, options, None)
    }

    /// Like [`Self::generate`], but calls `on_delta` with every decoded token
    /// piece so callers can render the answer incrementally.
    pub fn generate_streaming(
        &self,
        prompt: &str,
        stop_strings: &[String],
        options: &GenerateOptions,
        on_delta: impl FnMut(&str) + Send + 'static,
    ) -> Result<Generation, InferenceError> {
        self.request(prompt, stop_strings, options, Some(Box::new(on_delta)))
    }

    fn request(
        &self,
        prompt: &str,
        stop_strings: &[String],
        options: &GenerateOptions,
        on_delta: Option<Box<dyn FnMut(&str) + Send>>,
    ) -> Result<Generation, InferenceError> {
        let (respond, response) = channel();

        let sender = self
            .sender
            .lock()
            .map_err(|_| InferenceError::Worker("inference worker lock poisoned".to_string()))?;

        sender
            .send(Request {
                prompt: prompt.to_string(),
                stop_strings: stop_strings.to_vec(),
                options: options.clone(),
                respond,
                on_delta,
            })
            .map_err(|_| InferenceError::Worker("inference worker is not running".to_string()))?;

        response
            .recv()
            .map_err(|_| InferenceError::Worker("inference worker dropped the request".to_string()))?
    }
}

static BACKEND: OnceLock<Result<LlamaBackend, String>> = OnceLock::new();

fn shared_backend() -> Result<&'static LlamaBackend, InferenceError> {
    let result = BACKEND.get_or_init(|| {
        LlamaBackend::init()
            .map(|mut backend| {
                backend.void_logs();
                backend
            })
            .map_err(|error| error.to_string())
    });

    result.as_ref().map_err(|error| {
        InferenceError::Load(format!("failed to init llama backend: {error}"))
    })
}

fn worker(
    model_path: PathBuf,
    options: LoadOptions,
    receiver: Receiver<Request>,
    ready: Sender<Result<(), InferenceError>>,
) {
    let backend = match shared_backend() {
        Ok(backend) => backend,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };

    if !model_path.is_file() {
        let _ = ready.send(Err(InferenceError::Load(format!(
            "model file not found: {}",
            model_path.display()
        ))));
        return;
    }

    let model = match LlamaModel::load_from_file(
        backend,
        &model_path,
        &LlamaModelParams::default().with_n_gpu_layers(options.n_gpu_layers),
    ) {
        Ok(model) => model,
        Err(error) => {
            let _ = ready.send(Err(InferenceError::Load(format!(
                "failed to load {}: {error}",
                model_path.display()
            ))));
            return;
        }
    };

    let n_ctx = NonZeroU32::new(options.n_ctx.max(1)).unwrap_or(NonZeroU32::new(DEFAULT_N_CTX).unwrap());

    let mut context = match model.new_context(
        backend,
        LlamaContextParams::default().with_n_ctx(Some(n_ctx)),
    ) {
        Ok(context) => context,
        Err(error) => {
            let _ = ready.send(Err(InferenceError::Load(format!(
                "failed to create context: {error}"
            ))));
            return;
        }
    };

    if ready.send(Ok(())).is_err() {
        return;
    }

    while let Ok(mut request) = receiver.recv() {
        context.clear_kv_cache();
        let result = generate_once(&model, &mut context, n_ctx.get() as usize, &mut request);
        let _ = request.respond.send(result);
    }
}

fn generate_once(
    model: &LlamaModel,
    context: &mut LlamaContext<'_>,
    n_ctx: usize,
    request: &mut Request,
) -> Result<Generation, InferenceError> {
    let tokens = model
        .str_to_token(&request.prompt, AddBos::Never)
        .map_err(|error| InferenceError::Generate(format!("failed to tokenize prompt: {error}")))?;

    if tokens.is_empty() {
        return Err(InferenceError::Generate("prompt produced no tokens".to_string()));
    }

    let max_new = request
        .options
        .max_tokens
        .min(n_ctx.saturating_sub(tokens.len()).saturating_sub(1));

    if max_new == 0 {
        return Err(InferenceError::Generate(
            "prompt does not fit into the model context".to_string(),
        ));
    }

    let mut batch = LlamaBatch::new(tokens.len() + 64, 1);
    let last = (tokens.len() - 1) as i32;

    for (index, token) in tokens.iter().enumerate() {
        batch
            .add(*token, index as i32, &[0], index as i32 == last)
            .map_err(|error| InferenceError::Generate(format!("failed to queue prompt: {error}")))?;
    }

    context
        .decode(&mut batch)
        .map_err(|error| InferenceError::Generate(format!("failed to decode prompt: {error}")))?;

    let options = &request.options;
    let mut sampler = if options.temperature <= 0.0 {
        LlamaSampler::greedy()
    } else {
        LlamaSampler::chain_simple([
            LlamaSampler::penalties(model.n_vocab(), 64, options.repeat_penalty, 0.0, 0.0),
            LlamaSampler::top_k(options.top_k),
            LlamaSampler::top_p(options.top_p, 1),
            LlamaSampler::temp(options.temperature),
            LlamaSampler::dist(options.seed),
        ])
    };

    let started = Instant::now();
    let mut text = String::new();
    let mut decoded = 0usize;
    let mut position = tokens.len() as i32;
    let mut decoder = encoding_rs::UTF_8.new_decoder();

    for _ in 0..max_new {
        let token = sampler.sample(context, -1);

        if model.is_eog_token(token) {
            break;
        }

        let piece = model
            .token_to_piece(token, &mut decoder, true, None)
            .map_err(|error| InferenceError::Generate(format!("failed to decode token: {error}")))?;

        text.push_str(&piece);
        decoded += 1;

        if let Some(stop_at) = request
            .stop_strings
            .iter()
            .filter_map(|stop| text.find(stop.as_str()))
            .min()
        {
            text.truncate(stop_at);
            break;
        }

        if let Some(on_delta) = request.on_delta.as_mut() {
            on_delta(&piece);
        }

        batch.clear();
        batch
            .add(token, position, &[0], true)
            .map_err(|error| InferenceError::Generate(format!("failed to queue token: {error}")))?;
        position += 1;
        context
            .decode(&mut batch)
            .map_err(|error| InferenceError::Generate(format!("failed to decode token: {error}")))?;
    }

    Ok(Generation {
        text: text.trim().to_string(),
        tokens: decoded,
        elapsed: started.elapsed(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_missing_model() {
        let result = InferenceEngine::load("/nonexistent/model.gguf", LoadOptions::default());

        assert!(matches!(result, Err(InferenceError::Load(_))));
    }

    #[test]
    fn default_options_are_greedy() {
        assert_eq!(GenerateOptions::default().temperature, 0.0);
        assert_eq!(LoadOptions::default().n_ctx, DEFAULT_N_CTX);
    }

    #[test]
    fn translates_with_env_model() {
        let Ok(model_path) = std::env::var("TRANSLATOR_TEST_MODEL") else {
            return;
        };

        let engine = InferenceEngine::load(model_path, LoadOptions::default()).unwrap();
        let prompt = "<｜hy_begin▁of▁sentence｜><｜hy_User｜>将以下文本翻译为中文，注意只需要输出翻译后的结果，不要额外解释：\n\nkernel panic<｜hy_Assistant｜>";
        let stops = ["<｜hy_place▁holder▁no▁2｜>".to_string()];
        let options = GenerateOptions {
            temperature: 0.7,
            top_k: 20,
            top_p: 0.6,
            repeat_penalty: 1.05,
            ..Default::default()
        };

        let generation = engine.generate(prompt, &stops, &options).unwrap();

        assert!(
            generation.text.contains("内核"),
            "unexpected output: {}",
            generation.text
        );

        let streamed = std::sync::Arc::new(Mutex::new(String::new()));
        let sink = std::sync::Arc::clone(&streamed);
        let streamed_generation = engine
            .generate_streaming(prompt, &stops, &options, move |piece| {
                if let Ok(mut buffer) = sink.lock() {
                    buffer.push_str(piece);
                }
            })
            .unwrap();
        let streamed = streamed.lock().unwrap().clone();

        assert!(
            streamed_generation.text.contains("内核"),
            "unexpected streamed output: {}",
            streamed_generation.text
        );
        assert!(!streamed.is_empty(), "no streaming deltas were emitted");
    }
}
