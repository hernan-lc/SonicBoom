//! Model identity is independent of compiled backends and runtime readiness.
//! Configuration, installed weights and provider status remain registry concerns.

pub const POCKET_TTS_MODELS: &[&str] = &[
    "pocket-tts-en",
    "pocket-tts-es",
    "pocket-tts-pt",
    "pocket-tts-de",
    "pocket-tts-it",
    "pocket-tts-fr-24l",
];
/// Model IDs of retired providers (GPT-SoVITS, Qwen3-TTS). Requests naming
/// them get an explicit retired error and are never rerouted to another
/// provider.
pub const RETIRED_MODELS: &[&str] = &[
    "gpt-sovits-v2-onnx",
    "gpt-sovits-v2pro-onnx",
    "gpt-sovits-v2proplus-onnx",
    "qwen3-tts-0.6b-base",
    "qwen3-tts-1.7b-base",
];
/// Provider IDs that stored voice profiles may still reference. Their
/// profiles keep loading and can be listed or deleted, never synthesized.
pub const RETIRED_PROVIDERS: &[&str] = &["gpt-sovits", "qwen3-candle", "qwen3-gguf"];
pub const REVERB_TTS_MODELS: &[&str] = &["reverb-tts"];
pub const SUPPORTED_OPENAI_MODELS: &[&str] = &[
    "tts-1",
    "tts-1-hd",
    "supertonic-3",
    POCKET_TTS_MODELS[0],
    POCKET_TTS_MODELS[1],
    REVERB_TTS_MODELS[0],
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelProvider {
    Supertonic,
    PocketTts,
    ReverbTts,
}

impl ModelProvider {
    pub fn id(self) -> &'static str {
        match self {
            Self::Supertonic => "supertonic",
            Self::PocketTts => "pocket-tts",
            Self::ReverbTts => "reverb-tts",
        }
    }

    pub fn compiled(self) -> bool {
        match self {
            Self::Supertonic => true,
            Self::PocketTts => cfg!(feature = "pocket-tts"),
            Self::ReverbTts => cfg!(feature = "reverb-tts"),
        }
    }
}

pub fn model_provider(model: &str) -> Option<ModelProvider> {
    if POCKET_TTS_MODELS.contains(&model) {
        Some(ModelProvider::PocketTts)
    } else if REVERB_TTS_MODELS.contains(&model) {
        Some(ModelProvider::ReverbTts)
    } else if ["tts-1", "tts-1-hd", "supertonic-3"].contains(&model) {
        Some(ModelProvider::Supertonic)
    } else {
        None
    }
}

pub fn is_retired_model(model: &str) -> bool {
    RETIRED_MODELS.contains(&model)
}

pub fn is_retired_provider(provider: &str) -> bool {
    RETIRED_PROVIDERS.contains(&provider)
}
