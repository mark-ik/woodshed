//! Sample-by-sample sound synthesis.
//!
//! Two variants today:
//! - [`Sound::Click`] — synthesized sine burst with exponential decay,
//!   for metronomes and beat-counting clicks.
//! - [`Sound::Sample`] — playback of a PCM buffer (e.g. a drum sample
//!   loaded from a `.wav`), with gain and pitch-shift via linear-interp
//!   resampling.
//!
//! # Serialization
//!
//! Both variants are `serde`-serializable. The `Sample` variant
//! **does not embed** its PCM buffer — only an `id` string. Loading a
//! preset that uses a sample requires the consumer to load matching
//! samples into a [`crate::samples::SampleBank`] and then call
//! [`Sound::rehydrate_from`] (or `SampleBank::rehydrate_pattern`) to
//! attach the buffer.
//!
//! # No-buffer voice handling
//!
//! If a `Sound::Sample` is triggered without a buffer attached (e.g.
//! because rehydration was skipped or failed), it silently renders
//! nothing rather than panicking. This keeps the engine resilient to
//! missing samples.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// A pre-loaded PCM buffer. Mono `f32` samples at a known sample rate.
/// Cheap to clone via `Arc`.
#[derive(Clone, Debug)]
pub struct SampleBuffer {
    pub data: Arc<Vec<f32>>,
    pub sample_rate_hz: u32,
}

impl SampleBuffer {
    pub fn new(data: Vec<f32>, sample_rate_hz: u32) -> Self {
        Self {
            data: Arc::new(data),
            sample_rate_hz,
        }
    }

    pub fn empty() -> Self {
        Self {
            data: Arc::new(Vec::new()),
            sample_rate_hz: 48_000,
        }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    // --- Loop-shaping ops (sampler) ---
    //
    // Thin wrappers over `audio_primitives::buffer` — the shared
    // pure-slice kernels, so Strophe gets the same DSP. All
    // length-preserving, so a bar-locked loop stays the right length
    // for `sample_in_bar % len` playback. Mutate through
    // `Arc::make_mut` (clones the Vec only if another Arc — a UI
    // snapshot, say — still references it).

    /// Multiply every sample by `gain`, soft-clamping to `[-1, 1]`.
    /// A negative gain also inverts phase.
    pub fn apply_gain(&mut self, gain: f32) {
        audio_primitives::buffer::apply_gain(Arc::make_mut(&mut self.data).as_mut_slice(), gain);
    }

    /// Scale so the loudest sample sits at `peak` (e.g. `1.0`). No-op
    /// for a silent buffer.
    pub fn normalize(&mut self, peak: f32) {
        audio_primitives::buffer::normalize(Arc::make_mut(&mut self.data).as_mut_slice(), peak);
    }

    /// Reverse the buffer in place.
    pub fn reverse(&mut self) {
        audio_primitives::buffer::reverse(Arc::make_mut(&mut self.data).as_mut_slice());
    }
}

impl Default for SampleBuffer {
    fn default() -> Self {
        Self::empty()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Sound {
    Click {
        frequency_hz: f32,
        accent_frequency_hz: f32,
        duration_seconds: f32,
        amplitude: f32,
    },
    /// Playback of a pre-loaded PCM buffer. The `id` is the bank key;
    /// the `buffer` is filled at runtime and skipped during serde so
    /// presets stay small and portable.
    Sample {
        /// Bank key (e.g. `"kick"`, `"snare"`, or a path stem). Used
        /// for rehydration on preset load.
        id: String,
        /// Linear gain multiplier (1.0 = unity).
        gain: f32,
        /// Accent gain multiplier — applied on top of `gain` when the
        /// step is accented. 1.0 = no accent boost.
        accent_gain: f32,
        /// Playback speed offset in semitones. 0 = original pitch,
        /// +12 = octave up (2x speed), -12 = octave down (0.5x speed).
        pitch_semitones: f32,
        /// Runtime: the actual PCM data. Empty until rehydrated.
        #[serde(skip)]
        buffer: SampleBuffer,
    },
}

impl PartialEq for Sound {
    fn eq(&self, other: &Self) -> bool {
        // PartialEq is used in tests (e.g. pattern round-trip equality).
        // Compare the serializable fields only; the buffer is runtime
        // state and intentionally excluded.
        match (self, other) {
            (
                Self::Click {
                    frequency_hz: a1,
                    accent_frequency_hz: a2,
                    duration_seconds: a3,
                    amplitude: a4,
                },
                Self::Click {
                    frequency_hz: b1,
                    accent_frequency_hz: b2,
                    duration_seconds: b3,
                    amplitude: b4,
                },
            ) => a1 == b1 && a2 == b2 && a3 == b3 && a4 == b4,
            (
                Self::Sample {
                    id: a1,
                    gain: a2,
                    accent_gain: a3,
                    pitch_semitones: a4,
                    ..
                },
                Self::Sample {
                    id: b1,
                    gain: b2,
                    accent_gain: b3,
                    pitch_semitones: b4,
                    ..
                },
            ) => a1 == b1 && a2 == b2 && a3 == b3 && a4 == b4,
            _ => false,
        }
    }
}

impl Sound {
    /// Default click for a metronome: 800 Hz regular, 1200 Hz accent,
    /// 50ms long.
    pub const fn click() -> Self {
        Self::Click {
            frequency_hz: 800.0,
            accent_frequency_hz: 1200.0,
            duration_seconds: 0.05,
            amplitude: 0.4,
        }
    }

    /// Construct a sample-based sound with the given id. The buffer
    /// is empty until rehydrated via [`Self::rehydrate_from`] or a
    /// `SampleBank`.
    pub fn sample(id: impl Into<String>) -> Self {
        Self::Sample {
            id: id.into(),
            gain: 1.0,
            accent_gain: 1.3,
            pitch_semitones: 0.0,
            buffer: SampleBuffer::empty(),
        }
    }

    /// Sample-based sound with an already-loaded buffer.
    pub fn sample_with_buffer(id: impl Into<String>, buffer: SampleBuffer) -> Self {
        Self::Sample {
            id: id.into(),
            gain: 1.0,
            accent_gain: 1.3,
            pitch_semitones: 0.0,
            buffer,
        }
    }

    /// If this is a `Sample`, return its id. Otherwise `None`.
    pub fn sample_id(&self) -> Option<&str> {
        match self {
            Self::Sample { id, .. } => Some(id.as_str()),
            _ => None,
        }
    }

    /// If this is a `Sample`, attach the given buffer. No-op for other
    /// variants.
    pub fn attach_buffer(&mut self, new_buffer: SampleBuffer) {
        if let Self::Sample { buffer, .. } = self {
            *buffer = new_buffer;
        }
    }

    /// Look this sound's sample id up in a closure-style resolver and
    /// attach the resulting buffer. Returns true if a buffer was
    /// attached. No-op (returns false) for non-sample variants and for
    /// ids the resolver doesn't have.
    pub fn rehydrate_from<F>(&mut self, mut resolver: F) -> bool
    where
        F: FnMut(&str) -> Option<SampleBuffer>,
    {
        if let Self::Sample { id, buffer, .. } = self {
            if let Some(b) = resolver(id) {
                *buffer = b;
                return true;
            }
        }
        false
    }

    /// Render one sample of this sound. `local_sample` is the sample
    /// index since the trigger (0 at trigger time). `sample_rate` is
    /// the engine's output rate. Returns `None` once the sound is
    /// past its duration.
    pub fn render_sample(&self, local_sample: u32, sample_rate: f32, accent: bool) -> Option<f32> {
        match self {
            Sound::Click {
                frequency_hz,
                accent_frequency_hz,
                duration_seconds,
                amplitude,
            } => {
                // Click synthesis (sine burst + exponential decay) is
                // shared with Strophe via audio-primitives. This voice
                // renders it one sample at a time for the mixer.
                let t = local_sample as f32 / sample_rate;
                let freq = if accent {
                    *accent_frequency_hz
                } else {
                    *frequency_hz
                };
                audio_primitives::click::click_sample(t, freq, *duration_seconds, *amplitude)
            }
            Sound::Sample {
                gain,
                accent_gain,
                pitch_semitones,
                buffer,
                ..
            } => {
                if buffer.is_empty() {
                    // Missing or not-yet-rehydrated buffer — render
                    // silence and let the voice be reaped immediately.
                    return None;
                }
                // Speed factor: 2^(semitones / 12). Combine with the
                // ratio between the buffer's source rate and the
                // engine's output rate so playback matches the original
                // tempo of the recording when pitch_semitones = 0.
                let speed = 2.0_f32.powf(*pitch_semitones / 12.0);
                let source_rate = buffer.sample_rate_hz as f32;
                let step = speed * source_rate / sample_rate;
                let pos = local_sample as f32 * step;
                let idx = pos as usize;
                if idx + 1 >= buffer.len() {
                    return None;
                }
                // Linear interpolation between adjacent source samples.
                let frac = pos - idx as f32;
                let s0 = buffer.data[idx];
                let s1 = buffer.data[idx + 1];
                let interpolated = s0 + (s1 - s0) * frac;
                let g = *gain * if accent { *accent_gain } else { 1.0 };
                Some(interpolated * g)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gain_scales_and_clamps() {
        let mut b = SampleBuffer::new(vec![0.25, -0.5, 0.8], 48_000);
        b.apply_gain(2.0);
        assert!((b.data[0] - 0.5).abs() < 1e-6);
        assert!((b.data[1] - -1.0).abs() < 1e-6); // -1.0 (clamped from -1.0)
        assert!((b.data[2] - 1.0).abs() < 1e-6); // clamped from 1.6
    }

    #[test]
    fn normalize_brings_peak_to_target() {
        let mut b = SampleBuffer::new(vec![0.1, -0.4, 0.2], 48_000);
        b.normalize(1.0);
        let max = b.data.iter().fold(0.0_f32, |m, &s| m.max(s.abs()));
        assert!((max - 1.0).abs() < 1e-6);
    }

    #[test]
    fn normalize_silent_is_noop() {
        let mut b = SampleBuffer::new(vec![0.0, 0.0], 48_000);
        b.normalize(1.0);
        assert!(b.data.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn reverse_flips_order() {
        let mut b = SampleBuffer::new(vec![1.0, 2.0, 3.0], 48_000);
        b.reverse();
        assert_eq!(*b.data, vec![3.0, 2.0, 1.0]);
    }

    #[test]
    fn click_render_starts_nonzero() {
        let click = Sound::click();
        let s = click.render_sample(1, 48000.0, false);
        assert!(s.is_some());
        assert!(s.unwrap().abs() > 0.0);
    }

    #[test]
    fn click_render_returns_none_past_duration() {
        let click = Sound::click();
        // 50ms at 48kHz = 2400 samples; sample 3000 is past duration.
        let s = click.render_sample(3000, 48000.0, false);
        assert!(s.is_none());
    }

    #[test]
    fn click_envelope_decays() {
        let click = Sound::click();
        let early = click.render_sample(48, 48000.0, false).unwrap().abs();
        let late = click.render_sample(2000, 48000.0, false).unwrap().abs();
        assert!(early > late, "envelope should decay over time");
    }

    #[test]
    fn click_amplitude_is_bounded() {
        let click = Sound::click();
        // Sweep through duration and verify samples stay within [-amp, amp].
        for sample in 0..2400 {
            if let Some(v) = click.render_sample(sample, 48000.0, false) {
                assert!(v.abs() <= 0.4 + 1e-6);
            }
        }
    }

    #[test]
    fn accent_uses_different_frequency() {
        let click = Sound::click();
        let normal = click.render_sample(50, 48000.0, false).unwrap();
        let accented = click.render_sample(50, 48000.0, true).unwrap();
        assert_ne!(normal, accented);
    }

    // === Sample variant ===

    fn impulse_buffer() -> SampleBuffer {
        // Tiny test buffer: 4 samples [1.0, 0.5, -0.5, -1.0].
        SampleBuffer::new(vec![1.0, 0.5, -0.5, -1.0], 48_000)
    }

    #[test]
    fn sample_without_buffer_renders_none() {
        let s = Sound::sample("kick");
        assert!(s.render_sample(0, 48000.0, false).is_none());
    }

    #[test]
    fn sample_with_buffer_renders_value_at_start() {
        let s = Sound::sample_with_buffer("kick", impulse_buffer());
        let v = s.render_sample(0, 48000.0, false).unwrap();
        assert!((v - 1.0).abs() < 1e-6, "expected ~1.0 at start, got {v}");
    }

    #[test]
    fn sample_returns_none_past_end() {
        let s = Sound::sample_with_buffer("kick", impulse_buffer());
        // 4 samples; index 4 is past end.
        assert!(s.render_sample(10, 48000.0, false).is_none());
    }

    #[test]
    fn sample_accent_boosts_gain() {
        let s = Sound::sample_with_buffer("kick", impulse_buffer());
        let normal = s.render_sample(0, 48000.0, false).unwrap();
        let accented = s.render_sample(0, 48000.0, true).unwrap();
        assert!(
            accented.abs() > normal.abs(),
            "accent {accented} should exceed normal {normal}"
        );
    }

    #[test]
    fn sample_pitch_up_consumes_buffer_faster() {
        // At +12 semitones (2x speed), a 4-sample buffer is consumed in
        // ~2 output samples instead of ~4.
        let mut s = Sound::sample_with_buffer("kick", impulse_buffer());
        if let Sound::Sample {
            pitch_semitones, ..
        } = &mut s
        {
            *pitch_semitones = 12.0;
        }
        // At sample 2 (output), source idx = 2 * 2.0 = 4 → out of range.
        assert!(s.render_sample(2, 48000.0, false).is_none());
    }

    #[test]
    fn sample_round_trip_via_serde_preserves_id_and_drops_buffer() {
        let s = Sound::sample_with_buffer("kick", impulse_buffer());
        let json = serde_json::to_string(&s).unwrap();
        // Encoded form should mention id but not the buffer payload.
        assert!(json.contains("\"id\":\"kick\""));
        assert!(!json.contains("0.5"));
        let restored: Sound = serde_json::from_str(&json).unwrap();
        match restored {
            Sound::Sample { id, buffer, .. } => {
                assert_eq!(id, "kick");
                assert!(
                    buffer.is_empty(),
                    "buffer should be empty after deserialize"
                );
            }
            _ => panic!("expected Sound::Sample"),
        }
    }
}
