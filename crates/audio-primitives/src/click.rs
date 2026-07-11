//! Sine-burst click synthesis.
//!
//! A click is a single sine tone with an exponential-decay envelope —
//! the canonical metronome "tick." This module is the one source of
//! that math for the whole family:
//!
//! - Woodshed's `Sound::Click` voice renders it one sample at a time
//!   through its software mixer ([`click_sample`]).
//! - Strophe pre-renders a whole bar into a buffer for a Firewheel
//!   `SamplerNode` ([`render_click_bar`]).
//!
//! Both paths share the identical envelope/oscillator so a click
//! sounds the same wherever it plays.

use core::f32::consts::TAU;

/// The decay-rate constant: the envelope reaches `e^-5 ≈ 0.0067` of
/// its initial amplitude at `t == duration_s`, i.e. effectively
/// silent by the nominal end of the click. Expressed as a multiple of
/// `1 / duration_s` so the shape is duration-independent.
const DECAY_FACTOR: f32 = 5.0;

/// One sample of a decaying sine-burst click at elapsed time `t`
/// seconds since the click's start.
///
/// Returns `None` once `t >= duration_s` (the click is over), so a
/// per-sample render loop can stop as soon as it sees `None`.
///
/// - `freq_hz` — tone frequency (callers pick accent vs. base).
/// - `duration_s` — click length in seconds.
/// - `amplitude` — peak linear amplitude (the envelope starts here).
#[inline]
pub fn click_sample(t: f32, freq_hz: f32, duration_s: f32, amplitude: f32) -> Option<f32> {
    if t >= duration_s {
        return None;
    }
    let decay_rate = DECAY_FACTOR / duration_s;
    let envelope = (-t * decay_rate).exp();
    let phase = t * freq_hz * TAU;
    Some(phase.sin() * envelope * amplitude)
}

/// Integer audio frames in one beat of `beat_unit` at `bpm`.
///
/// BPM is expressed as quarter notes per minute, so a beat in 3/8 is half as
/// long as a beat in 3/4 at the same BPM. Rounding happens once per beat and
/// every caller derives a bar from this result, keeping click boundaries and
/// capture lengths in lockstep.
pub fn frames_per_beat(sample_rate: u32, bpm: f32, beat_unit: u8) -> usize {
    let bpm = if bpm.is_finite() { bpm.max(1.0) } else { 1.0 };
    let beat_unit = beat_unit.max(1);
    (sample_rate as f64 * 60.0 * 4.0 / (f64::from(bpm) * f64::from(beat_unit))).round() as usize
}

/// Integer audio frames in one bar at the specified meter.
pub fn frames_per_bar(sample_rate: u32, bpm: f32, beats_per_bar: u8, beat_unit: u8) -> usize {
    frames_per_beat(sample_rate, bpm, beat_unit).saturating_mul(beats_per_bar as usize)
}

/// Render one bar of metronome clicks into a mono `Vec<f32>` at
/// `sample_rate`, treating the beat unit as a quarter note.
///
/// Kept as a convenience wrapper for existing 4/4 callers. New consumers that
/// carry a full time signature should use [`render_click_bar_in_meter`].
pub fn render_click_bar(
    sample_rate: u32,
    bpm: f32,
    beats_per_bar: u8,
    base_freq_hz: f32,
    accent_freq_hz: f32,
    duration_s: f32,
    amplitude: f32,
) -> Vec<f32> {
    render_click_bar_in_meter(
        sample_rate,
        bpm,
        beats_per_bar,
        4,
        base_freq_hz,
        accent_freq_hz,
        duration_s,
        amplitude,
    )
}

/// Render one bar of metronome clicks into a mono `Vec<f32>` at a full time
/// signature.
///
/// The bar is `beats_per_bar` beats long at `bpm`; beat 0 is accented
/// (`accent_freq_hz`), the rest use `base_freq_hz`. Each click is a
/// `duration_s` burst; the gaps between clicks are silence. The
/// returned buffer is exactly one bar long (`samples_per_beat *
/// beats_per_bar`), so it loops seamlessly when fed to a looping
/// sampler.
pub fn render_click_bar_in_meter(
    sample_rate: u32,
    bpm: f32,
    beats_per_bar: u8,
    beat_unit: u8,
    base_freq_hz: f32,
    accent_freq_hz: f32,
    duration_s: f32,
    amplitude: f32,
) -> Vec<f32> {
    let sr = sample_rate as f32;
    let samples_per_beat = frames_per_beat(sample_rate, bpm, beat_unit);
    let total_samples = frames_per_bar(sample_rate, bpm, beats_per_bar, beat_unit);
    let mut buf = vec![0.0_f32; total_samples];

    for beat in 0..beats_per_bar as usize {
        let freq = if beat == 0 {
            accent_freq_hz
        } else {
            base_freq_hz
        };
        let beat_start = beat * samples_per_beat;
        // Walk samples within this beat until the click envelope ends
        // (click_sample returns None) or we run out of beat.
        for s in 0..samples_per_beat {
            let t = s as f32 / sr;
            match click_sample(t, freq, duration_s, amplitude) {
                Some(v) => buf[beat_start + s] = v,
                None => break,
            }
        }
    }

    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn click_sample_starts_nonzero_and_decays() {
        let early = click_sample(0.001, 800.0, 0.05, 0.4).unwrap().abs();
        let late = click_sample(0.04, 800.0, 0.05, 0.4).unwrap().abs();
        assert!(early > 0.0);
        assert!(early > late, "envelope should decay: {early} !> {late}");
    }

    #[test]
    fn click_sample_returns_none_past_duration() {
        assert!(click_sample(0.05, 800.0, 0.05, 0.4).is_none());
        assert!(click_sample(0.10, 800.0, 0.05, 0.4).is_none());
    }

    #[test]
    fn click_sample_amplitude_bounded() {
        let mut t = 0.0;
        while let Some(v) = click_sample(t, 800.0, 0.05, 0.4) {
            assert!(v.abs() <= 0.4 + 1e-6, "sample {v} exceeds amplitude");
            t += 1.0 / 48_000.0;
        }
    }

    #[test]
    fn render_click_bar_has_expected_length() {
        // 120 BPM, 4 beats, 48 kHz: 24000 samples/beat * 4 = 96000.
        let buf = render_click_bar(48_000, 120.0, 4, 800.0, 1200.0, 0.05, 0.4);
        assert_eq!(buf.len(), 96_000);
    }

    #[test]
    fn meter_aware_bar_length_honors_the_beat_unit() {
        assert_eq!(frames_per_bar(48_000, 120.0, 3, 8), 36_000);
        let buf = render_click_bar_in_meter(48_000, 120.0, 3, 8, 800.0, 1200.0, 0.05, 0.4);
        assert_eq!(buf.len(), 36_000);
    }

    #[test]
    fn render_click_bar_clicks_on_downbeat_then_silence() {
        let buf = render_click_bar(48_000, 120.0, 4, 800.0, 1200.0, 0.05, 0.4);
        // Energy near the downbeat (sample 0) ...
        let downbeat: f32 = buf[..2400].iter().map(|s| s.abs()).sum();
        assert!(downbeat > 0.0, "downbeat should have a click");
        // ... and silence in the gap before beat 2 (e.g. 0.2s in, well
        // past a 50ms click but before the next beat at 0.5s).
        let gap: f32 = buf[12_000..18_000].iter().map(|s| s.abs()).sum();
        assert_eq!(gap, 0.0, "gap between clicks should be silent");
    }

    #[test]
    fn render_click_bar_accents_downbeat() {
        // Downbeat uses accent_freq; later beats use base_freq. With
        // different frequencies the first-click waveform differs from
        // the second-click waveform at the same in-click offset.
        let buf = render_click_bar(48_000, 120.0, 4, 800.0, 1200.0, 0.05, 0.4);
        let samples_per_beat = 24_000;
        let off = 50; // a few samples into each click
        assert_ne!(buf[off], buf[samples_per_beat + off]);
    }
}
