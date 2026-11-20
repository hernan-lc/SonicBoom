//! Cold-start probe for Supertonic: how long do session creation, the first
//! inference (GPU pipeline/shader compilation) and steady-state inference take?
//!
//! ```sh
//! cargo run --release --no-default-features --features webgpu,server \
//!   --example startup_probe -- [cpu|default] [steps] [warm]
//! ```
//!
//! Run it as a fresh process each time: the interesting numbers are the
//! first-run costs, which a warm-up loop would hide.

use std::{path::PathBuf, time::Instant};

use anyhow::Result;
use sonicboom::tts::{download::ModelPaths, inference, model::ModelHandle};

fn model_paths() -> ModelPaths {
    let models = PathBuf::from("models");
    let voices = ["F1", "F2", "F3", "F4", "F5", "M1", "M2", "M3", "M4", "M5"]
        .iter()
        .map(|name| {
            (
                name.to_string(),
                models.join("voice_styles").join(format!("{name}.json")),
            )
        })
        .collect();
    // `PROBE_ONNX_DIR` swaps in an alternative set of graphs (e.g. fp16
    // conversions) while keeping the voice styles from `models/`.
    let onnx = std::env::var_os("PROBE_ONNX_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| models.join("onnx"));
    ModelPaths {
        duration_predictor: onnx.join("duration_predictor.onnx"),
        text_encoder: onnx.join("text_encoder.onnx"),
        vector_estimator: onnx.join("vector_estimator.onnx"),
        vocoder: onnx.join("vocoder.onnx"),
        unicode_indexer: onnx.join("unicode_indexer.json"),
        tts_config: onnx.join("tts.json"),
        voice_files: voices,
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let ep = args.get(1).map(String::as_str).unwrap_or("default");
    let steps: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(5);
    // SAFETY: single-threaded setup before any sessions are created.
    unsafe {
        std::env::set_var("SONICBOOM_EP", ep);
        std::env::set_var("SONICBOOM_SEED", "42");
    }

    let t = Instant::now();
    sonicboom::onnx_runtime::initialize()?;
    println!("ort init            : {:?}", t.elapsed());

    let t = Instant::now();
    let handle = ModelHandle::load(&model_paths())?;
    println!("ModelHandle::load   : {:?}", t.elapsed());

    if args.get(3).is_some_and(|a| a == "warm") {
        let t = Instant::now();
        handle.warm_up();
        println!("warm_up             : {:?}", t.elapsed());
    }

    let short = "Hello world.";
    let mut last: Vec<f32> = Vec::new();
    let long = "A gentle breeze moved through the open window while everyone listened.";
    for (label, text) in [
        ("1st synth (short)", short),
        ("2nd synth (short)", short),
        ("3rd synth (long) ", long),
        ("4th synth (long) ", long),
        ("5th synth (long) ", long),
    ] {
        let t = Instant::now();
        let samples = inference::synthesize(&handle, text, "en", "F1", steps, 200)?;
        println!("{label}   : {:?} ({} samples)", t.elapsed(), samples.len());
        last = samples;
    }
    // `PROBE_OUT` dumps the last waveform as raw f32 LE for offline diffing.
    if let Some(path) = std::env::var_os("PROBE_OUT") {
        let bytes: Vec<u8> = last.iter().flat_map(|v| v.to_le_bytes()).collect();
        std::fs::write(path, bytes)?;
    }
    Ok(())
}
