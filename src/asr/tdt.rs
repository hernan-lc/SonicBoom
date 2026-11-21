//! FastConformer encoder + TDT (token-and-duration transducer) decoding.
//!
//! Graph contract read from the pinned int8 ONNX files (opset 17) and from
//! `onnx-asr/src/onnx_asr/models/nemo.py`:
//!
//! * `encoder-model.int8.onnx`: inputs `audio_signal` `[1,128,frames]` f32 and
//!   `length` `[1]` i64 (frames of 10 ms); outputs `outputs` `[1,1024,T]` f32
//!   (onnx-asr transposes it to `[1,T,1024]`) and `encoded_lengths` `[1]` i64.
//! * `decoder_joint-model.int8.onnx`: inputs `encoder_outputs` `[1,1024,1]`,
//!   `targets` `[1,1]` i64, `target_length` `[1]` i64, `input_states_1` and
//!   `input_states_2` `[2,1,640]` f32; outputs `outputs` `[1,1,8198]`
//!   (8193 vocab tokens + 5 duration classes), `output_states_1/2` and an
//!   unused `prednet_lengths`.
//!
//! Greedy loop follows onnx-asr's `_AsrWithTransducerDecoding`: emit the
//! argmax token, advance by the argmax duration when it is positive,
//! otherwise advance by one on blank or after `max_tokens_per_step` (10,
//! `config.json` has no override). Decoder states are committed only for
//! non-blank emissions. All sessions are CPU-only by design: the user's GPU
//! returns silence for these convolutional graphs.

use std::path::Path;

use ort::session::Session;
use ort::value::TensorRef;

use super::AsrError;
use super::mel::Features;

const ENCODER_DIM: usize = 1024;
const STATE_LAYERS: usize = 2;
const STATE_DIM: usize = 640;
const DURATION_CLASSES: usize = 5;
const MAX_TOKENS_PER_STEP: usize = 10;
const MAX_STEPS: usize = 100_000;

fn session(path: &Path) -> Result<Session, AsrError> {
    crate::onnx_runtime::initialize()
        .map_err(|e| AsrError::Model(format!("ONNX runtime: {e:#}")))?;
    Session::builder()
        .map_err(|e| AsrError::Model(format!("session setup: {e}")))?
        .with_intra_threads(4)
        .map_err(|e| AsrError::Model(format!("thread setup: {e}")))?
        .commit_from_file(path)
        .map_err(|e| AsrError::Model(format!("cannot load {}: {e}", path.display())))
}

/// Pure argmax over vocab logits followed by argmax over duration logits.
pub(crate) fn greedy_step(logits: &[f32], vocab_size: usize) -> (usize, usize) {
    let token = argmax(&logits[..vocab_size]);
    let duration = argmax(&logits[vocab_size..]);
    (token, duration)
}

fn argmax(values: &[f32]) -> usize {
    values
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.total_cmp(b))
        .map(|(i, _)| i)
        .unwrap_or(0)
}

/// Greedy transducer loop, independent of ONNX so it can be tested with
/// synthetic logits. `step(frame, previous_token)` returns `(token, duration)`;
/// decoder states are owned and updated by the caller's closure.
pub(crate) fn decode_greedy(
    frame_count: usize,
    vocab_size: usize,
    blank: usize,
    mut step: impl FnMut(usize, usize) -> Result<(usize, usize), AsrError>,
) -> Result<Vec<usize>, AsrError> {
    let mut tokens = Vec::new();
    let mut previous = blank;
    let mut t = 0usize;
    let mut emitted = 0usize;
    let mut steps = 0usize;
    while t < frame_count {
        steps += 1;
        if steps > MAX_STEPS {
            return Err(AsrError::Decode("TDT decoding did not converge".into()));
        }
        let (token, duration) = step(t, previous)?;
        if token >= vocab_size {
            return Err(AsrError::Decode(
                "TDT returned an out-of-range token".into(),
            ));
        }
        if token != blank {
            tokens.push(token);
            emitted += 1;
            // The decoder consumes the last *emitted* token, never the blank
            // (see onnx-asr `_AsrWithTransducerDecoding._decode`).
            previous = token;
        }
        if duration > 0 {
            t += duration;
            emitted = 0;
        } else if token == blank || emitted >= MAX_TOKENS_PER_STEP {
            t += 1;
            emitted = 0;
        }
    }
    Ok(tokens)
}

pub struct Tdt {
    encoder: Session,
    joint: Session,
    vocab_size: usize,
}

impl Tdt {
    pub fn open(dir: &Path, vocab_size: usize) -> Result<Self, AsrError> {
        let encoder = session(&dir.join("encoder-model.int8.onnx"))?;
        let joint = session(&dir.join("decoder_joint-model.int8.onnx"))?;
        Ok(Self {
            encoder,
            joint,
            vocab_size,
        })
    }

    /// Run both graphs and return token ids (special tokens excluded by the caller).
    pub fn transcribe(
        &mut self,
        features: &Features,
        blank: usize,
    ) -> Result<Vec<usize>, AsrError> {
        let mut audio_signal = vec![0.0f32; super::mel::MELS * features.frames];
        for frame in 0..features.frames {
            for mel in 0..super::mel::MELS {
                audio_signal[mel * features.frames + frame] =
                    features.data[frame * super::mel::MELS + mel];
            }
        }
        let length = [features.valid_frames as i64];
        let audio_view = TensorRef::from_array_view((
            [1usize, super::mel::MELS, features.frames],
            &audio_signal[..],
        ))
        .map_err(|e| AsrError::Decode(format!("encoder input: {e}")))?;
        let length_view = TensorRef::from_array_view(([1usize], &length[..]))
            .map_err(|e| AsrError::Decode(format!("encoder length input: {e}")))?;
        let encoded = self
            .encoder
            .run(ort::inputs![
                "audio_signal" => audio_view,
                "length" => length_view,
            ])
            .map_err(|e| AsrError::Decode(format!("encoder pass: {e}")))?;
        let (shape, values) = encoded["outputs"]
            .try_extract_tensor::<f32>()
            .map_err(|e| AsrError::Decode(format!("encoder output: {e}")))?;
        let (_, lengths) = encoded["encoded_lengths"]
            .try_extract_tensor::<i64>()
            .map_err(|e| AsrError::Decode(format!("encoder length output: {e}")))?;
        // The graph emits `[1, 1024, T]`; transpose to one contiguous vector
        // per encoder step, as `NemoConformerRnnt._encode` does upstream.
        let dim = shape.get(1).copied().unwrap_or(ENCODER_DIM as i64) as usize;
        let frames = shape.get(2).copied().unwrap_or(0) as usize;
        let count = (lengths.first().copied().unwrap_or(0) as usize).min(frames);
        if count == 0 || dim != ENCODER_DIM {
            return Err(AsrError::Decode(format!(
                "unexpected encoder output shape {shape:?} (length {count})"
            )));
        }
        let frames_data: Vec<Vec<f32>> = (0..count)
            .map(|t| (0..dim).map(|d| values[d * frames + t]).collect())
            .collect();
        drop(encoded);

        let mut state1 = vec![0.0f32; STATE_LAYERS * STATE_DIM];
        let mut state2 = vec![0.0f32; STATE_LAYERS * STATE_DIM];
        let vocab_size = self.vocab_size;
        decode_greedy(count, vocab_size, blank, |t, previous| {
            let mut encoder_outputs = vec![0.0f32; ENCODER_DIM];
            encoder_outputs.copy_from_slice(&frames_data[t]);
            // The joint graph declares `targets`/`target_length` as INT32
            // (ONNX element type 6), unlike the encoder's INT64 `length`.
            let target = [previous as i32];
            let target_length = [1i32];
            let encoder_view =
                TensorRef::from_array_view(([1usize, ENCODER_DIM, 1], &encoder_outputs[..]))
                    .map_err(|e| AsrError::Decode(format!("joint encoder input: {e}")))?;
            let targets_view = TensorRef::from_array_view(([1usize, 1], &target[..]))
                .map_err(|e| AsrError::Decode(format!("joint target input: {e}")))?;
            let length_view = TensorRef::from_array_view(([1usize], &target_length[..]))
                .map_err(|e| AsrError::Decode(format!("joint length input: {e}")))?;
            let s1_view = TensorRef::from_array_view(([STATE_LAYERS, 1, STATE_DIM], &state1[..]))
                .map_err(|e| AsrError::Decode(format!("joint state input: {e}")))?;
            let s2_view = TensorRef::from_array_view(([STATE_LAYERS, 1, STATE_DIM], &state2[..]))
                .map_err(|e| AsrError::Decode(format!("joint state input: {e}")))?;
            let outputs = self
                .joint
                .run(ort::inputs![
                    "encoder_outputs" => encoder_view,
                    "targets" => targets_view,
                    "target_length" => length_view,
                    "input_states_1" => s1_view,
                    "input_states_2" => s2_view,
                ])
                .map_err(|e| AsrError::Decode(format!("joint pass: {e}")))?;
            let (_, logits) = outputs["outputs"]
                .try_extract_tensor::<f32>()
                .map_err(|e| AsrError::Decode(format!("joint output: {e}")))?;
            if logits.len() != vocab_size + DURATION_CLASSES {
                return Err(AsrError::Decode(format!(
                    "unexpected joint output size {} (expected {})",
                    logits.len(),
                    vocab_size + DURATION_CLASSES
                )));
            }
            let (token, duration) = greedy_step(logits, vocab_size);
            if token != blank {
                // NeMo commits decoder states only on non-blank emissions.
                state1 = outputs["output_states_1"]
                    .try_extract_tensor::<f32>()
                    .map_err(|e| AsrError::Decode(format!("joint state output: {e}")))?
                    .1
                    .to_vec();
                state2 = outputs["output_states_2"]
                    .try_extract_tensor::<f32>()
                    .map_err(|e| AsrError::Decode(format!("joint state output: {e}")))?
                    .1
                    .to_vec();
            }
            Ok((token, duration))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greedy_step_matches_synthetic_logits() {
        let mut logits = vec![0.0f32; 8198];
        logits[10] = 2.0;
        logits[8193 + 3] = 1.0;
        assert_eq!(greedy_step(&logits, 8193), (10, 3));
    }

    #[test]
    fn greedy_decoder_emits_tokens_and_honours_durations() {
        // Frame 0 emits "a" with duration 2; frame 2 emits blank with
        // duration 0 (advance 1); frame 3 emits "b" with duration 1.
        let script = [(7usize, 2usize), (0, 0), (9, 1), (0, 0)];
        let ids = decode_greedy(4, 16, 0, |t, _previous| {
            let (token, duration) = script[t.min(script.len() - 1)];
            Ok((token, duration))
        })
        .expect("decode");
        assert_eq!(ids, vec![7, 9]);
    }

    #[test]
    fn greedy_decoder_caps_tokens_per_frame() {
        let ids = decode_greedy(1, 16, 0, |_, _| Ok((3, 0))).expect("decode");
        assert_eq!(ids.len(), MAX_TOKENS_PER_STEP);
    }
}
