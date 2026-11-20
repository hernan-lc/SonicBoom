//! Reference-audio measurements and take trimming, ported from `reverb_tts/audio.py`.

/// A 10 ms window quieter than this (dBFS) is silence.
const SILENCE_DB: f64 = -50.0;
/// A reference quieter than this (RMS, dBFS) is raised to [`LEVEL_DB`] before encoding.
const QUIET_DB: f64 = -40.0;
const LEVEL_DB: f64 = -20.0;
/// The raise stops short of taking the loudest sample past this.
const PEAK: f64 = 0.99;

/// The recording raised to `LEVEL_DB` if it is quieter than `QUIET_DB` (RMS),
/// otherwise unchanged. A loud click limits the raise so the peak stays at `PEAK`.
pub fn raise_quiet(audio: &[f32]) -> Vec<f32> {
    if audio.is_empty() {
        return Vec::new();
    }
    let rms =
        (audio.iter().map(|&v| f64::from(v).powi(2)).sum::<f64>() / audio.len() as f64).sqrt();
    if rms == 0.0 || 20.0 * rms.log10() >= QUIET_DB {
        return audio.to_vec();
    }
    let peak = audio.iter().fold(0.0f64, |m, &v| m.max(f64::from(v).abs()));
    let gain = (10f64.powf(LEVEL_DB / 20.0) / rms).min(PEAK / peak.max(1e-12));
    audio
        .iter()
        .map(|&v| (f64::from(v) * gain) as f32)
        .collect()
}

/// Level of every whole `window` of samples, in dBFS.
fn levels(audio: &[f64], window: usize) -> Vec<f64> {
    audio
        .chunks_exact(window)
        .map(|w| {
            20.0 * ((w.iter().map(|v| v * v).sum::<f64>() / window as f64).sqrt() + 1e-10).log10()
        })
        .collect()
}

/// Cut leading and trailing silence (10 ms windows under `SILENCE_DB`), keeping
/// `keep` seconds of it on each side.
pub fn trim_silence(audio: &[f32], sample_rate: u32, keep: f64) -> Vec<f32> {
    let window = (sample_rate / 100) as usize;
    if window == 0 {
        return audio.to_vec();
    }
    let wide: Vec<f64> = audio.iter().map(|&v| f64::from(v)).collect();
    let level = levels(&wide, window);
    let loud: Vec<usize> = level
        .iter()
        .enumerate()
        .filter(|(_, l)| **l >= SILENCE_DB)
        .map(|(i, _)| i)
        .collect();
    if loud.is_empty() {
        return audio.to_vec();
    }
    // Runs of loud windows separated by more than 200 ms (20 windows) of silence.
    let mut sounds: Vec<Vec<usize>> = vec![vec![loud[0]]];
    for pair in loud.windows(2) {
        if pair[1] - pair[0] > 20 {
            sounds.push(Vec::new());
        }
        sounds.last_mut().expect("non-empty").push(pair[1]);
    }
    // A sound of at most 120 ms, alone at either edge, is a codec burst.
    if sounds.len() > 1 && sounds[0].len() <= 12 {
        sounds.remove(0);
    }
    if sounds.len() > 1 && sounds[sounds.len() - 1].len() <= 12 {
        sounds.pop();
    }
    let margin = (keep * 100.0).round_ties_even() as usize;
    let first = sounds[0][0];
    let last = *sounds[sounds.len() - 1].last().expect("non-empty");
    let from = first.saturating_sub(margin) * window;
    let to = (last + 1 + margin).min(level.len()) * window;
    audio[from..to.min(audio.len())].to_vec()
}

/// Seconds of a reference recording that set the speaker's pace: the recording
/// without its silent edges (keeping `lead` and `trail` seconds) and with every
/// pause inside the speech counted as at most `max_pause` seconds.
pub fn speech_length(audio: &[f32], sample_rate: u32) -> f64 {
    const MAX_PAUSE: f64 = 0.5;
    const LEAD: f64 = 0.1;
    const TRAIL: f64 = 0.2;
    let total = audio.len() as f64 / f64::from(sample_rate);
    let window = (sample_rate / 100) as usize;
    if window == 0 {
        return total;
    }
    let count = audio.len() / window;
    if count == 0 {
        return total;
    }
    let x: Vec<f64> = audio[..count * window]
        .iter()
        .map(|&v| f64::from(v))
        .collect();
    let rms = (x.iter().map(|v| v * v).sum::<f64>() / x.len() as f64)
        .sqrt()
        .max(1e-8);
    let scaled: Vec<f64> = x.iter().map(|v| v * 0.1 / rms).collect();
    let level = levels(&scaled, window);
    let loud: Vec<usize> = level
        .iter()
        .enumerate()
        .filter(|(_, l)| **l >= SILENCE_DB)
        .map(|(i, _)| i)
        .collect();
    if loud.is_empty() {
        return total;
    }
    let (first, last) = (loud[0], loud[loud.len() - 1]);
    let cap = (MAX_PAUSE * 100.0).round_ties_even() as usize;
    let kept = loud.len()
        + loud
            .windows(2)
            .map(|p| (p[1] - p[0] - 1).min(cap))
            .sum::<usize>();
    let edges = first.min((LEAD * 100.0).round_ties_even() as usize)
        + (count - 1 - last).min((TRAIL * 100.0).round_ties_even() as usize);
    (kept + edges) as f64 / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(secs: f64, amp: f32) -> Vec<f32> {
        (0..(secs * 24_000.0) as usize)
            .map(|i| (i as f32 * 0.3).sin() * amp)
            .collect()
    }

    #[test]
    fn quiet_references_are_raised_loud_ones_untouched() {
        let loud = tone(1.0, 0.3);
        assert_eq!(raise_quiet(&loud), loud);
        let quiet = tone(1.0, 0.002);
        let raised = raise_quiet(&quiet);
        let rms = (raised.iter().map(|&v| f64::from(v).powi(2)).sum::<f64>() / raised.len() as f64)
            .sqrt();
        assert!(
            (20.0 * rms.log10() + 20.0).abs() < 0.2,
            "{}",
            20.0 * rms.log10()
        );
        assert!(raised.iter().all(|v| v.abs() <= 0.99 + 1e-6));
        assert!(raise_quiet(&[]).is_empty());
        assert_eq!(raise_quiet(&[0.0; 100]), vec![0.0; 100]);
    }

    #[test]
    fn trim_keeps_speech_and_a_margin() {
        let mut audio = vec![0.0f32; 24_000]; // 1 s silence
        audio.extend(tone(1.0, 0.3));
        audio.extend(vec![0.0f32; 24_000]);
        let out = trim_silence(&audio, 24_000, 0.1);
        // 1 s of tone + 0.1 s margin each side.
        assert_eq!(out.len(), 24_000 + 2 * 2_400);
        // All-silent audio is returned unchanged.
        assert_eq!(trim_silence(&vec![0.0f32; 4800], 24_000, 0.1).len(), 4800);
    }

    #[test]
    fn trim_drops_a_short_burst_at_the_edge() {
        let mut audio = tone(0.05, 0.3); // 50 ms burst
        audio.extend(vec![0.0f32; 24_000]); // 1 s gap
        audio.extend(tone(1.0, 0.3));
        let out = trim_silence(&audio, 24_000, 0.0);
        assert!(out.len() <= 24_000 + 480, "{}", out.len());
        assert!(out.len() >= 24_000 - 480, "{}", out.len());
    }

    #[test]
    fn speech_length_caps_pauses_and_edges() {
        let mut audio = vec![0.0f32; 24_000]; // 1 s leading silence
        audio.extend(tone(1.0, 0.3));
        audio.extend(vec![0.0f32; 48_000]); // 2 s pause
        audio.extend(tone(1.0, 0.3));
        audio.extend(vec![0.0f32; 24_000]); // 1 s trailing silence
        // 0.1 lead + 1 + 0.5 capped pause + 1 + 0.2 trail.
        let secs = speech_length(&audio, 24_000);
        assert!((secs - 2.8).abs() < 0.03, "{secs}");
        assert!((speech_length(&[0.0; 24_000], 24_000) - 1.0).abs() < 1e-9);
        assert!((speech_length(&[0.1; 10], 24_000) - 10.0 / 24_000.0).abs() < 1e-9);
    }
}
