//! Sample-by-sample sound synthesis.
//!
//! Currently only [`Sound::Click`] — a sine burst with exponential
//! decay envelope. Future: drum samples, FM synthesis, etc.

use core::f32::consts::TAU;

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Sound {
    Click {
        frequency_hz: f32,
        accent_frequency_hz: f32,
        duration_seconds: f32,
        amplitude: f32,
    },
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

    /// Render one sample of this sound. `local_sample` is the sample
    /// index since the trigger (0 at trigger time). Returns `None`
    /// once the sound is past its duration.
    pub fn render_sample(
        &self,
        local_sample: u32,
        sample_rate: f32,
        accent: bool,
    ) -> Option<f32> {
        match self {
            Sound::Click {
                frequency_hz,
                accent_frequency_hz,
                duration_seconds,
                amplitude,
            } => {
                let t = local_sample as f32 / sample_rate;
                if t >= *duration_seconds {
                    return None;
                }
                let freq = if accent {
                    *accent_frequency_hz
                } else {
                    *frequency_hz
                };
                let decay_rate = 5.0 / *duration_seconds;
                let envelope = (-t * decay_rate).exp();
                let phase = t * freq * TAU;
                Some(phase.sin() * envelope * amplitude)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // We can't read the frequency, but we can verify that accented
        // and non-accented samples differ at the same time index.
        let click = Sound::click();
        let normal = click.render_sample(50, 48000.0, false).unwrap();
        let accented = click.render_sample(50, 48000.0, true).unwrap();
        assert_ne!(normal, accented);
    }
}
