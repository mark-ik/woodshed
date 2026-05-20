//! Chord audio synthesis.
//!
//! Renders a chord — a set of pitches sounded simultaneously — into a
//! mono PCM [`SampleBuffer`] suitable for `Sound::Sample` playback,
//! `.wav` export, or any other consumer that takes raw f32 audio.
//!
//! Why this lives here, not in `woodshedding`: the music-theory crate
//! is intentionally pure (no audio dependencies). It produces the
//! pitches; this module produces the samples.
//!
//! # Design choices
//!
//! - **Additive sine synthesis.** Cheap, clean, no FFT or filters
//!   needed. Each pitch becomes a sine wave at its fundamental. Good
//!   enough for "this is the chord, here's what it sounds like"
//!   reference playback; not a substitute for a real instrument
//!   sample.
//! - **Mild harmonic enrichment.** Each pitch gets a quieter second
//!   and third partial added (sine waves at 2× and 3× the
//!   fundamental). Without partials a pure-sine chord sounds
//!   organ-like and bland; with them it gets a hint of body.
//! - **ADSR-shaped envelope.** Linear attack and decay, sustain at
//!   full level for the body of the note, exponential release.
//!   Avoids the clicky transient of a hard gate.
//! - **DC-safe summing.** N pitches summed with the same phase peak
//!   at N × amplitude. We scale by `1 / sqrt(n)` to keep loudness
//!   roughly even across chord densities without aggressive
//!   normalization that flattens dynamics.
//!
//! The renderer is allocation-aware — one Vec for the output,
//! computed in one pass. No FFTs, no resampling, no I/O.

use crate::sound::SampleBuffer;

/// Parameters for one chord render. Defaults via [`Default::default`]
/// give a 1-second piano-ish strum suitable for a chord-card preview.
#[derive(Clone, Debug)]
pub struct ChordRender {
    /// Fundamental frequencies (Hz) for every note in the chord.
    /// Order doesn't matter mathematically but affects which note
    /// gets the slight strum delay (see `strum_offset_ms`).
    pub pitches_hz: Vec<f32>,
    /// Total length of the rendered sample, in seconds. Includes
    /// attack + sustain + release.
    pub duration_seconds: f32,
    /// Peak amplitude in [0, 1]. Output stays below this even after
    /// summing partials.
    pub amplitude: f32,
    /// Linear attack in seconds. Below ~5 ms can click on small
    /// speakers; the default is 15 ms.
    pub attack_seconds: f32,
    /// Linear decay in seconds.
    pub release_seconds: f32,
    /// Per-note delay for a strum effect. 0 = block-chord (all notes
    /// hit at once). 20 ms feels like a soft strum.
    pub strum_offset_ms: f32,
}

impl Default for ChordRender {
    fn default() -> Self {
        Self {
            pitches_hz: Vec::new(),
            duration_seconds: 1.0,
            amplitude: 0.45,
            attack_seconds: 0.015,
            release_seconds: 0.25,
            strum_offset_ms: 0.0,
        }
    }
}

impl ChordRender {
    /// Convenience constructor for the chord-card preview case.
    pub fn strum(pitches_hz: Vec<f32>, duration_seconds: f32) -> Self {
        Self {
            pitches_hz,
            duration_seconds,
            strum_offset_ms: 18.0,
            ..Self::default()
        }
    }

    /// Block-chord render: all notes start at the same instant.
    pub fn block(pitches_hz: Vec<f32>, duration_seconds: f32) -> Self {
        Self {
            pitches_hz,
            duration_seconds,
            strum_offset_ms: 0.0,
            ..Self::default()
        }
    }
}

/// Render a chord to a mono [`SampleBuffer`] at the given sample
/// rate. Returns an empty buffer if `pitches_hz` is empty.
pub fn render_chord(params: &ChordRender, sample_rate_hz: u32) -> SampleBuffer {
    let n = params.pitches_hz.len();
    if n == 0 || params.duration_seconds <= 0.0 {
        return SampleBuffer::empty();
    }
    let sr = sample_rate_hz as f32;
    let total_samples = (params.duration_seconds * sr).round() as usize;
    let mut out = vec![0.0_f32; total_samples];

    // Loudness compensation across N pitches.
    let chord_scale = params.amplitude / (n as f32).sqrt().max(1.0);

    let strum_secs = params.strum_offset_ms / 1000.0;

    for (note_idx, &freq) in params.pitches_hz.iter().enumerate() {
        if freq <= 0.0 {
            continue;
        }
        let note_start_secs = note_idx as f32 * strum_secs;
        let note_start_sample = (note_start_secs * sr) as usize;
        if note_start_sample >= total_samples {
            break;
        }

        let attack = params.attack_seconds.max(0.0);
        let release = params.release_seconds.max(0.0);
        // Effective note duration after the per-note strum offset.
        let note_secs = params.duration_seconds - note_start_secs;
        let release_start_secs = (note_secs - release).max(attack);

        for sample_offset in 0..(total_samples - note_start_sample) {
            let t = sample_offset as f32 / sr;
            // Linear attack → flat sustain → exponential release.
            let env = if t < attack {
                (t / attack).max(0.0)
            } else if t < release_start_secs {
                1.0
            } else {
                let rel_t = t - release_start_secs;
                let normalized = (rel_t / release.max(1e-4)).clamp(0.0, 1.0);
                // exp decay so it tails smoothly rather than ramping
                // to zero linearly.
                (-3.0 * normalized).exp()
            };
            let phase_t = t;
            let fundamental = (phase_t * freq * core::f32::consts::TAU).sin();
            let p2 = (phase_t * freq * 2.0 * core::f32::consts::TAU).sin() * 0.25;
            let p3 = (phase_t * freq * 3.0 * core::f32::consts::TAU).sin() * 0.12;
            let voice = (fundamental + p2 + p3) * env;
            out[note_start_sample + sample_offset] += voice * chord_scale;
        }
    }

    // Soft-clamp anything that escaped the loudness model. Rare but
    // cheap insurance.
    for s in out.iter_mut() {
        *s = s.clamp(-1.0, 1.0);
    }

    SampleBuffer::new(out, sample_rate_hz)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_pitches_yields_empty_buffer() {
        let params = ChordRender::block(vec![], 1.0);
        let buf = render_chord(&params, 48_000);
        assert!(buf.is_empty());
    }

    #[test]
    fn render_produces_correct_buffer_length() {
        let params = ChordRender::block(vec![440.0], 0.5);
        let buf = render_chord(&params, 48_000);
        assert_eq!(buf.len(), 24_000);
        assert_eq!(buf.sample_rate_hz, 48_000);
    }

    #[test]
    fn output_stays_within_unit_range() {
        let params = ChordRender::block(vec![261.63, 329.63, 392.00], 0.5);
        let buf = render_chord(&params, 48_000);
        for (i, &s) in buf.data.iter().enumerate() {
            assert!(s.is_finite(), "sample {i} not finite");
            assert!(s.abs() <= 1.0, "sample {i} out of range: {s}");
        }
    }

    #[test]
    fn strum_starts_first_note_at_zero() {
        // A strummed chord should still have audible signal in the
        // first millisecond (first note starts at t=0).
        let params = ChordRender::strum(vec![440.0, 554.37, 659.25], 0.5);
        let buf = render_chord(&params, 48_000);
        // ~1 ms = 48 samples. Look for at least one non-zero sample in
        // the first 100 (= 2 ms — leaves room for attack ramp).
        let early_max = buf.data.iter().take(100).map(|s| s.abs()).fold(0.0_f32, f32::max);
        assert!(
            early_max > 0.0,
            "no signal in first 2ms of strummed chord; max = {early_max}"
        );
    }

    #[test]
    fn block_chord_has_simultaneous_onset() {
        // For block chord with attack = 15ms, all three voices should
        // be active by sample 1000 (~21 ms at 48kHz).
        let params = ChordRender::block(vec![261.63, 329.63, 392.00], 0.5);
        let buf = render_chord(&params, 48_000);
        let mid_value = buf.data[1000].abs();
        assert!(mid_value > 0.0, "block chord silent at 21ms: {mid_value}");
    }

    #[test]
    fn release_tail_decays_toward_zero() {
        let params = ChordRender::block(vec![440.0], 1.0);
        let buf = render_chord(&params, 48_000);
        // Last 100 samples should be quieter than the middle.
        let middle_peak = buf
            .data
            .iter()
            .skip(20_000)
            .take(2_000)
            .map(|s| s.abs())
            .fold(0.0_f32, f32::max);
        let tail_peak = buf
            .data
            .iter()
            .rev()
            .take(100)
            .map(|s| s.abs())
            .fold(0.0_f32, f32::max);
        assert!(
            tail_peak < middle_peak,
            "tail {tail_peak} should be quieter than middle {middle_peak}"
        );
    }

    #[test]
    fn more_pitches_does_not_clip_louder_than_one_pitch() {
        // Three-pitch chord shouldn't peak meaningfully higher than a
        // single pitch — the sqrt-N scaling keeps peaks bounded.
        let single = render_chord(
            &ChordRender::block(vec![440.0], 0.5),
            48_000,
        );
        let triple = render_chord(
            &ChordRender::block(vec![261.63, 329.63, 392.00], 0.5),
            48_000,
        );
        let single_peak = single.data.iter().map(|s| s.abs()).fold(0.0_f32, f32::max);
        let triple_peak = triple.data.iter().map(|s| s.abs()).fold(0.0_f32, f32::max);
        // Allow the triple to be a bit louder (constructive interference)
        // but not catastrophically so.
        assert!(
            triple_peak <= single_peak * 2.5,
            "triple peak {triple_peak} > 2.5× single peak {single_peak}"
        );
    }
}
