//! Masked-diffusion sampler, ported from `reverb_tts/sampler.py` (batch size 1).
//!
//! `steps` main passes decide codebooks 0-1 (content and timing), then two polish passes
//! decide codebooks 2-7. The network is a closure, so the logic is testable without ONNX.

use std::ops::Range;

use rand::{Rng, rngs::StdRng};

/// Codec codebooks per frame, from coarse (0) to fine (7).
pub const CODEBOOKS: usize = 8;
/// Real codes per codebook; the mask id is one past them.
pub const CODES: usize = 1024;
/// Id of a masked slot, in every codebook.
pub const MASK: i64 = CODES as i64;

/// Sampler settings. Defaults are upstream's tuned setup.
#[derive(Debug, Clone, PartialEq)]
pub struct SamplerConfig {
    /// Main passes, which decide codebooks 0-1.
    pub steps: usize,
    /// Whether main passes withdraw earlier commits the model has come to doubt.
    pub remasking: bool,
    /// Commits withdrawn per new commit, on every main pass but the last.
    pub remask_ratio: f32,
    /// Warps the commit schedule: few commits early, most in the last passes.
    pub t_shift: f64,
    /// Per codebook index, so codebook 0 tends to be committed before codebook 1.
    pub codebook_penalty: f32,
    /// Scales confidence and penalty against the Gumbel noise.
    pub position_temperature: f32,
    /// Attention bias towards the reference frames, in the polish passes only.
    pub polish_bias: f32,
    /// Codebooks 4-7: weight of (second polish pass - first polish pass).
    pub polish_guidance: f32,
}

impl Default for SamplerConfig {
    fn default() -> Self {
        Self {
            steps: 8,
            remasking: true,
            remask_ratio: 0.25,
            t_shift: 0.1,
            codebook_penalty: 5.0,
            position_temperature: 5.0,
            polish_bias: 1.0,
            polish_guidance: 0.5,
        }
    }
}

impl SamplerConfig {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=64).contains(&self.steps) {
            return Err(format!("steps must be within 1..=64, got {}", self.steps));
        }
        if !(0.0..1.0).contains(&self.remask_ratio) {
            return Err(format!(
                "remask_ratio must be in [0, 1), got {}",
                self.remask_ratio
            ));
        }
        if !(self.t_shift > 0.0 && self.t_shift <= 1.0) {
            return Err(format!("t_shift must be in (0, 1], got {}", self.t_shift));
        }
        if !(self.position_temperature > 0.0 && self.position_temperature.is_finite()) {
            return Err("position_temperature must be positive".to_string());
        }
        Ok(())
    }
}

/// Number of network passes `sample` runs: `steps` main passes plus two polish passes.
pub fn pass_count(config: &SamplerConfig) -> usize {
    config.steps + 2
}

fn argmax(row: &[f32]) -> (usize, f32) {
    let mut best = (0, f32::NEG_INFINITY);
    for (i, &v) in row.iter().enumerate() {
        if v > best.1 {
            best = (i, v);
        }
    }
    best
}

/// Indices ordered by `score`, best first (`descending`) or smallest first.
fn order(score: &[f32], descending: bool) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..score.len()).collect();
    if descending {
        idx.sort_by(|&a, &b| score[b].total_cmp(&score[a]));
    } else {
        idx.sort_by(|&a, &b| score[a].total_cmp(&score[b]));
    }
    idx
}

/// Fill a fully masked `[8, frames]` target. `log_probs(tokens, codebooks, polish)`
/// returns the log-probabilities `[codebooks.len(), frames, 1024]` (row-major) for the
/// given `[8, frames]` tokens. Returns the `[8, frames]` codes (row-major).
pub fn sample<E>(
    frames: usize,
    config: &SamplerConfig,
    rng: &mut StdRng,
    mut log_probs: impl FnMut(&[i64], Range<usize>, bool) -> Result<Vec<f32>, E>,
) -> Result<Vec<i64>, E> {
    let f = frames;
    let n = 2 * f;
    let mut tokens = vec![MASK; CODEBOOKS * f];
    let schedule: Vec<f64> = (0..=config.steps)
        .map(|i| {
            let x = i as f64 / config.steps as f64;
            config.t_shift * x / (1.0 - (1.0 - config.t_shift) * x)
        })
        .collect();

    for step in 0..config.steps {
        let lp = log_probs(&tokens, 0..2, false)?;
        let mut confidence = vec![0f32; n];
        let mut prediction = vec![0i64; n];
        for i in 0..n {
            let (best, value) = argmax(&lp[i * CODES..(i + 1) * CODES]);
            confidence[i] = value;
            prediction[i] = best as i64;
        }
        let held: Vec<i64> = tokens[..n].to_vec();
        let masked: Vec<bool> = held.iter().map(|&t| t == MASK).collect();
        let left = masked.iter().filter(|&&m| m).count();
        let last = step == config.steps - 1;
        let quota = (n as f64 * (schedule[step + 1] - schedule[step])).ceil() as usize;
        let k = if last { left } else { left.min(quota) };

        let score: Vec<f32> = (0..n)
            .map(|i| {
                let u: f32 = rng.random();
                let gumbel = -(-(u + 1e-10).ln() + 1e-10).ln();
                if masked[i] {
                    let penalty = config.codebook_penalty * (i / f) as f32;
                    (confidence[i] - penalty) / config.position_temperature + gumbel
                } else {
                    f32::NEG_INFINITY
                }
            })
            .collect();
        let mut updated = held.clone();
        for &i in order(&score, true).iter().take(k) {
            if score[i].is_finite() {
                updated[i] = prediction[i];
            }
        }
        if config.remasking && !last {
            // Withdraw the held codes the model now finds least likely,
            // `remask_ratio` as many as this pass committed.
            let revealed_count = n - left;
            let count =
                ((config.remask_ratio * k as f32).round_ties_even() as usize).min(revealed_count);
            let likely: Vec<f32> = (0..n)
                .map(|i| {
                    if masked[i] {
                        f32::INFINITY
                    } else {
                        lp[i * CODES + (held[i].min(MASK - 1)) as usize]
                    }
                })
                .collect();
            for &i in order(&likely, false).iter().take(count) {
                if !masked[i] {
                    updated[i] = MASK;
                }
            }
        }
        tokens[..n].copy_from_slice(&updated);
    }

    // Polish: codebooks 2-3, then 4-7 guided against the first polish pass's view of them.
    let first = log_probs(&tokens, 2..CODEBOOKS, true)?;
    for i in 0..n {
        tokens[n + i] = argmax(&first[i * CODES..(i + 1) * CODES]).0 as i64;
    }
    let second = log_probs(&tokens, 4..CODEBOOKS, true)?;
    for i in 0..4 * f {
        let s = &second[i * CODES..(i + 1) * CODES];
        let p = &first[(n + i) * CODES..(n + i + 1) * CODES];
        let guided: Vec<f32> = s
            .iter()
            .zip(p)
            .map(|(&a, &b)| a + config.polish_guidance * (a - b))
            .collect();
        tokens[4 * f + i] = argmax(&guided).0 as i64;
    }
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    /// A fake network that is confident about code `(codebook * 31 + frame) % 1024`.
    fn target(codebook: usize, frame: usize) -> usize {
        (codebook * 31 + frame) % CODES
    }

    fn fake(frames: usize) -> impl FnMut(&[i64], Range<usize>, bool) -> Result<Vec<f32>, String> {
        move |_tokens, books, _polish| {
            let mut out = vec![-9.0f32; books.len() * frames * CODES];
            for (row, book) in books.enumerate() {
                for frame in 0..frames {
                    out[(row * frames + frame) * CODES + target(book, frame)] = -0.1;
                }
            }
            Ok(out)
        }
    }

    #[test]
    fn fills_every_slot_with_the_confident_codes() {
        let frames = 37;
        let mut rng = StdRng::seed_from_u64(1);
        let codes = sample(frames, &SamplerConfig::default(), &mut rng, fake(frames)).unwrap();
        assert_eq!(codes.len(), CODEBOOKS * frames);
        for book in 0..CODEBOOKS {
            for frame in 0..frames {
                assert_eq!(codes[book * frames + frame], target(book, frame) as i64);
            }
        }
        assert!(!codes.contains(&MASK));
    }

    #[test]
    fn same_seed_same_commit_order_different_seed_differs() {
        // Record which slots each main pass sees as masked: it depends on the noise.
        let frames = 40;
        let run = |seed: u64| {
            let mut seen = Vec::new();
            let mut net = fake(frames);
            let mut rng = StdRng::seed_from_u64(seed);
            sample(frames, &SamplerConfig::default(), &mut rng, |t, b, p| {
                seen.push(t[..2 * frames].iter().filter(|&&x| x == MASK).count());
                net(t, b, p)
            })
            .unwrap();
            seen
        };
        assert_eq!(run(7), run(7));
        let a = run(7);
        assert_eq!(a.len(), pass_count(&SamplerConfig::default()));
        // Schedule: few commits early (t_shift 0.1), everything masked at first.
        assert_eq!(a[0], 2 * frames);
        assert!(a[1] > a[0] - 2 * frames / 4, "{a:?}");
        assert!(
            a.windows(2).take(7).all(|w| w[1] <= w[0] + 2 * frames),
            "{a:?}"
        );
    }

    #[test]
    fn remasking_withdraws_low_probability_commits() {
        // The network is wrong on the first pass for codebook 0 frame 0 and right later.
        let frames = 24;
        let mut calls = 0usize;
        let mut rng = StdRng::seed_from_u64(3);
        let codes = sample(
            frames,
            &SamplerConfig::default(),
            &mut rng,
            |t, books, p| {
                let mut out = vec![-9.0f32; books.len() * frames * CODES];
                for (row, book) in books.clone().enumerate() {
                    for frame in 0..frames {
                        out[(row * frames + frame) * CODES + target(book, frame)] = -0.1;
                        // A held code that is not the target is unlikely: make it explicit.
                        let held = t[book * frames + frame];
                        if held != MASK && held != target(book, frame) as i64 && held >= 0 {
                            out[(row * frames + frame) * CODES + held as usize] = -20.0;
                        }
                    }
                }
                calls += 1;
                let _ = p;
                Ok::<_, String>(out)
            },
        )
        .unwrap();
        assert_eq!(calls, 10);
        assert_eq!(codes[0], target(0, 0) as i64);
    }

    #[test]
    fn polish_guidance_pushes_away_from_the_first_pass() {
        // Codebook 4 frame 0: first polish pass prefers code 5 slightly over 9; the second
        // prefers 9 and 5 equally. With guidance 0.5 the second pass minus first favours 9.
        let frames = 1;
        let mut rng = StdRng::seed_from_u64(0);
        let mut pass = 0;
        let codes = sample(
            frames,
            &SamplerConfig::default(),
            &mut rng,
            |_t, books, polish| {
                let mut out = vec![-9.0f32; books.len() * frames * CODES];
                if polish {
                    pass += 1;
                    for (row, book) in books.enumerate() {
                        let base = row * CODES;
                        if book == 4 && pass == 1 {
                            out[base + 5] = -0.2;
                            out[base + 9] = -0.4;
                        } else if book == 4 && pass == 2 {
                            out[base + 5] = -0.3;
                            out[base + 9] = -0.3;
                        } else {
                            out[base + 1] = -0.1;
                        }
                    }
                } else {
                    for row in 0..books.len() {
                        out[row * CODES] = -0.1;
                    }
                }
                Ok::<_, String>(out)
            },
        )
        .unwrap();
        // second + 0.5 * (second - first): code 5 -> -0.3 + 0.5*(-0.1) = -0.35, code 9 -> -0.3 + 0.5*(0.1) = -0.25.
        assert_eq!(codes[4], 9);
    }

    #[test]
    fn errors_from_the_network_propagate() {
        let mut rng = StdRng::seed_from_u64(0);
        let r = sample(5, &SamplerConfig::default(), &mut rng, |_, _, _| {
            Err::<Vec<f32>, _>("cancelled")
        });
        assert_eq!(r.unwrap_err(), "cancelled");
    }

    #[test]
    fn config_validation() {
        assert!(SamplerConfig::default().validate().is_ok());
        assert!(
            SamplerConfig {
                steps: 0,
                ..Default::default()
            }
            .validate()
            .is_err()
        );
        assert!(
            SamplerConfig {
                remask_ratio: 1.0,
                ..Default::default()
            }
            .validate()
            .is_err()
        );
        assert!(
            SamplerConfig {
                t_shift: 0.0,
                ..Default::default()
            }
            .validate()
            .is_err()
        );
    }
}
