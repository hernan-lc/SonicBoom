//! NeMo log-mel frontend, ported from onnx-asr's `nemo128` preprocessor.
//!
//! Values come from `onnx-asr/preprocessors/nemo.py` and
//! `preprocessors/fbanks.py` (reviewed on 2026-11-20): 16 kHz input,
//! `n_fft=512`, `win_length=400` (symmetric Hann, zero-padded to 512),
//! `hop_length=160`, pre-emphasis 0.97, log guard `2^-24`, 128 mel bins
//! (slaney scale, slaney normalization, 0..8000 Hz), then per-mel-bin
//! mean/variance normalization over the valid frames with epsilon `1e-5`.
//! Frames at or beyond `samples / hop` are zeroed, matching the upstream
//! mask (the extra padded frame is passed to the encoder with a shorter
//! `length`). No dependency is added: a radix-2 real FFT is implemented here.

use std::sync::OnceLock;

use super::AsrError;

const N_FFT: usize = 512;
const WIN_LENGTH: usize = 400;
const HOP: usize = 160;
pub const MELS: usize = 128;
const PREEMPH: f32 = 0.97;
const LOG_GUARD: f32 = 1.0 / 16_777_216.0; // 2^-24
const NORM_EPS: f32 = 1e-5;

/// Features laid out row-major `[frames][MELS]`.
#[derive(Debug, Clone)]
pub struct Features {
    pub data: Vec<f32>,
    pub frames: usize,
    /// `samples / HOP`; the encoder's `length` input.
    pub valid_frames: usize,
}

pub fn nemo_features(samples: &[f32]) -> Result<Features, AsrError> {
    if samples.len() < HOP * 2 {
        return Err(AsrError::Decode("audio is too short to transcribe".into()));
    }
    let valid_frames = samples.len() / HOP;
    let frames = valid_frames + 1;
    let mut data = vec![0.0f32; frames * MELS];

    let mut pre = Vec::with_capacity(samples.len());
    pre.push(samples[0]);
    for i in 1..samples.len() {
        // Upstream keeps sample 0 and zeroes the tail beyond `waveforms_lens`.
        pre.push(samples[i] - PREEMPH * samples[i - 1]);
    }

    let window = window();
    let filters = filterbank();
    let mut padded = vec![0.0f32; samples.len() + N_FFT];
    padded[N_FFT / 2..N_FFT / 2 + pre.len()].copy_from_slice(&pre);
    for (frame, row) in data.as_chunks_mut::<MELS>().0.iter_mut().enumerate() {
        let start = frame * HOP;
        let power = rfft_power(&padded[start..start + N_FFT], &window);
        for (mel, value) in row.iter_mut().enumerate() {
            let mut energy = 0.0f32;
            for (bin, magnitude) in power.iter().enumerate() {
                energy += magnitude * filters[mel * power.len() + bin];
            }
            *value = (energy + LOG_GUARD).ln();
        }
    }

    // Per-feature normalization over valid frames only. `f32` arithmetic
    // mirrors NumPy's `np.divide(..., dtype=np.float32)` in the reference.
    for mel in 0..MELS {
        let mut mean = 0.0f32;
        for frame in 0..valid_frames {
            mean += data[frame * MELS + mel];
        }
        mean /= valid_frames as f32;
        let mut variance = 0.0f32;
        for frame in 0..valid_frames {
            let delta = data[frame * MELS + mel] - mean;
            variance += delta * delta;
        }
        variance /= (valid_frames - 1) as f32;
        let denom = variance.sqrt() + NORM_EPS;
        for frame in 0..valid_frames {
            data[frame * MELS + mel] = (data[frame * MELS + mel] - mean) / denom;
        }
        for frame in valid_frames..frames {
            data[frame * MELS + mel] = 0.0;
        }
    }

    Ok(Features {
        data,
        frames,
        valid_frames,
    })
}

/// Symmetric Hann(400) zero-padded with 56 zeros on each side to length 512.
fn window() -> Vec<f32> {
    let mut w = vec![0.0f32; N_FFT];
    for n in 0..WIN_LENGTH {
        let hann = 0.5 - 0.5 * (std::f64::consts::TAU * n as f64 / (WIN_LENGTH - 1) as f64).cos();
        w[(N_FFT - WIN_LENGTH) / 2 + n] = hann as f32;
    }
    w
}

/// Slaney/slaney mel filterbank, shape `[128][257]`, computed from
/// `fbanks.py::melscale_fbanks(257, 0, 8000, 128, 16000, "slaney", "slaney")`.
fn filterbank() -> &'static [f32] {
    static FB: OnceLock<Vec<f32>> = OnceLock::new();
    FB.get_or_init(|| {
        const BINS: usize = N_FFT / 2 + 1;
        const EPS: f64 = f32::EPSILON as f64;
        let hz_to_mel = |f: f64| -> f64 {
            if f < 1000.0 {
                3.0 * f / 200.0
            } else {
                15.0 + 27.0 * (f / 1000.0 + EPS).ln() / 6.4_f64.ln()
            }
        };
        let mel_to_hz = |m: f64| -> f64 {
            if m < 15.0 {
                200.0 * m / 3.0
            } else {
                1000.0 * 6.4_f64.powf((m - 15.0) / 27.0)
            }
        };
        let freqs: Vec<f64> = (0..BINS).map(|i| i as f64 * 8000.0 / (BINS - 1) as f64).collect();
        let points: Vec<f64> = (0..MELS + 2)
            .map(|i| mel_to_hz(hz_to_mel(8000.0) * i as f64 / (MELS + 1) as f64))
            .collect();
        let mut fb = vec![0.0f32; MELS * BINS];
        for mel in 0..MELS {
            let (left, center, right) = (points[mel], points[mel + 1], points[mel + 2]);
            for (bin, frequency) in freqs.iter().enumerate() {
                let up = (frequency - left) / (center - left);
                let down = (right - frequency) / (right - center);
                let weight = up.min(down).max(0.0) * 2.0 / (right - left);
                fb[mel * BINS + bin] = weight as f32;
            }
        }
        fb
    })
}

fn bit_reverse(mut value: usize, bits: usize) -> usize {
    let mut reversed = 0usize;
    for _ in 0..bits {
        reversed = (reversed << 1) | (value & 1);
        value >>= 1;
    }
    reversed
}

/// Power spectrum `|rfft(frame)|^2` for a 512-sample real frame.
fn rfft_power(frame: &[f32], window: &[f32]) -> Vec<f32> {
    debug_assert_eq!(frame.len(), N_FFT);
    let mut re = [0.0f64; N_FFT];
    let mut im = [0.0f64; N_FFT];
    for i in 0..N_FFT {
        re[i] = frame[i] as f64 * window[i] as f64;
    }
    for i in 0..N_FFT {
        let j = bit_reverse(i, 9);
        if j > i {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut size = 2;
    while size <= N_FFT {
        let half = size / 2;
        let angle = -std::f64::consts::TAU / size as f64;
        let (wr, wi) = (angle.cos(), angle.sin());
        for start in (0..N_FFT).step_by(size) {
            let (mut cr, mut ci) = (1.0, 0.0);
            for k in 0..half {
                let (ur, ui) = (re[start + k], im[start + k]);
                let (vr, vi) = (
                    re[start + k + half] * cr - im[start + k + half] * ci,
                    re[start + k + half] * ci + im[start + k + half] * cr,
                );
                re[start + k] = ur + vr;
                im[start + k] = ui + vi;
                re[start + k + half] = ur - vr;
                im[start + k + half] = ui - vi;
                let next = (cr * wr - ci * wi, cr * wi + ci * wr);
                cr = next.0;
                ci = next.1;
            }
        }
        size *= 2;
    }
    (0..=N_FFT / 2)
        .map(|i| (re[i] * re[i] + im[i] * im[i]) as f32)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filterbank_matches_numpy_reference() {
        let fb = filterbank();
        assert_eq!(fb.len(), MELS * 257);
        let expected = [0.0, 0.014389, 0.013989, 0.0, 0.0, 0.0];
        for (i, want) in expected.into_iter().enumerate() {
            assert!((fb[257 + i] - want).abs() < 1e-5, "bin {i}: {}", fb[257 + i]);
        }
    }

    #[test]
    fn features_match_nemo_reference_and_are_normalized() {
        let samples: Vec<f32> = (0..1600)
            .map(|i| {
                (std::f64::consts::TAU * 440.0 * i as f64 / 16_000.0).sin() as f32 * 0.5
                    + (std::f64::consts::TAU * 1300.0 * i as f64 / 16_000.0).sin() as f32 * 0.25
            })
            .collect();
        let features = nemo_features(&samples).expect("features");
        assert_eq!(features.frames, 11);
        assert_eq!(features.valid_frames, 10);
        // Mel bin 0 over the first six frames, from the NumPy reference.
        let expected = [2.101285, 0.363256, -0.399572, -0.508561, -0.811661, -0.799731];
        for (frame, want) in expected.into_iter().enumerate() {
            assert!(
                (features.data[frame * MELS] - want).abs() < 1e-3,
                "frame {frame}: {}",
                features.data[frame * MELS]
            );
        }
        // The trailing masked frame is zero, and valid columns have unit variance.
        assert!(features.data[10 * MELS..11 * MELS].iter().all(|v| *v == 0.0));
        let mean: f32 = (0..10).map(|f| features.data[f * MELS]).sum::<f32>() / 10.0;
        assert!(mean.abs() < 1e-5, "mean {mean}");
    }

    #[test]
    fn too_short_audio_is_rejected() {
        assert!(nemo_features(&[0.0; 100]).is_err());
    }
}
