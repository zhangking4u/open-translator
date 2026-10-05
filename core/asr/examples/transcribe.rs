//! Decode a wav file with the SenseVoice engine.
//!
//! ```text
//! cargo run --example transcribe -- /tmp/kilo/models/sense-voice test_wavs/en.wav en
//! ```

use std::path::PathBuf;

use translator_asr::{SenseVoiceEngine, SpeechEngine};

fn main() {
    let dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/kilo/models/sense-voice".to_string());
    let wav_arg = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "test_wavs/en.wav".to_string());
    let language = std::env::args().nth(3).unwrap_or_else(|| "en".to_string());

    let dir = PathBuf::from(dir);
    let wav = if std::path::Path::new(&wav_arg).is_absolute() {
        PathBuf::from(&wav_arg)
    } else {
        dir.join(&wav_arg)
    };
    let wave = sherpa_onnx::Wave::read(&wav.to_string_lossy()).expect("read wav");

    let engine = SenseVoiceEngine::load(
        &dir.join("model.int8.onnx"),
        &dir.join("tokens.txt"),
        &language,
        4,
    )
    .expect("load engine");

    let started = std::time::Instant::now();
    let transcript = engine
        .transcribe(wave.samples(), wave.sample_rate())
        .expect("transcribe");

    println!("text: {}", transcript.text);
    println!(
        "audio_ms: {} decode_ms: {}",
        wave.samples().len() as u128 * 1000 / wave.sample_rate().max(1) as u128,
        started.elapsed().as_millis()
    );
}
