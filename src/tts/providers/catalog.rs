//! Model identity is independent of compiled backends and runtime readiness.
//! Configuration, installed weights and provider status remain registry concerns.

pub const QWEN_GGUF_MODELS: &[&str] = &["qwen3-tts-1.7b-base"];
pub const POCKET_TTS_MODELS: &[&str] = &[
    "pocket-tts-en",
    "pocket-tts-es",
    "pocket-tts-pt",
    "pocket-tts-de",
    "pocket-tts-it",
    "pocket-tts-fr-24l",
];
pub const REVERB_TTS_MODELS: &[&str] = &["reverb-tts"];
pub const SUPPORTED_OPENAI_MODELS: &[&str] = &[
    "tts-1",
    "tts-1-hd",
    "supertonic-3",
    QWEN_GGUF_MODELS[0],
    POCKET_TTS_MODELS[0],
    POCKET_TTS_MODELS[1],
    REVERB_TTS_MODELS[0],
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelProvider {
    Supertonic,
    QwenGguf,
    PocketTts,
    ReverbTts,
}

impl ModelProvider {
    pub fn id(self) -> &'static str {
        match self {
            Self::Supertonic => "supertonic",
            Self::QwenGguf => "qwen3-gguf",
            Self::PocketTts => "pocket-tts",
            Self::ReverbTts => "reverb-tts",
        }
    }

    pub fn compiled(self) -> bool {
        match self {
            Self::Supertonic => true,
            Self::QwenGguf => cfg!(feature = "qwen-gguf"),
            Self::PocketTts => cfg!(feature = "pocket-tts"),
            Self::ReverbTts => cfg!(feature = "reverb-tts"),
        }
    }
}

pub fn model_provider(model: &str) -> Option<ModelProvider> {
    if QWEN_GGUF_MODELS.contains(&model) {
        Some(ModelProvider::QwenGguf)
    } else if POCKET_TTS_MODELS.contains(&model) {
        Some(ModelProvider::PocketTts)
    } else if REVERB_TTS_MODELS.contains(&model) {
        Some(ModelProvider::ReverbTts)
    } else if ["tts-1", "tts-1-hd", "supertonic-3"].contains(&model) {
        Some(ModelProvider::Supertonic)
    } else {
        None
    }
}
