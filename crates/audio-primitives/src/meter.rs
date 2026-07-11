//! Frame-rate-independent display ballistics for normalized audio meters.

/// User-configurable meter timing, in seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeterBallistics {
    /// Time constant while the displayed level rises.
    pub attack_seconds: f32,
    /// Time constant while the displayed level falls.
    pub release_seconds: f32,
    /// Duration the peak marker remains fixed after a new peak.
    pub peak_hold_seconds: f32,
    /// Time constant for peak-marker decay after the hold expires.
    pub peak_release_seconds: f32,
}

impl Default for MeterBallistics {
    fn default() -> Self {
        Self {
            attack_seconds: 0.015,
            release_seconds: 0.25,
            peak_hold_seconds: 0.7,
            peak_release_seconds: 0.6,
        }
    }
}

/// Smoothed level and held peak, both normalized to `0..=1`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MeterReading {
    pub level: f32,
    pub peak: f32,
}

/// Stateful meter display smoother. Audio measurement remains the engine's job;
/// this type controls only how normalized readings move on screen.
#[derive(Clone, Debug)]
pub struct PeakMeterSmoother {
    ballistics: MeterBallistics,
    reading: MeterReading,
    hold_remaining: f32,
}

impl PeakMeterSmoother {
    pub fn new(ballistics: MeterBallistics) -> Self {
        Self {
            ballistics,
            reading: MeterReading::default(),
            hold_remaining: 0.0,
        }
    }

    pub fn ballistics(&self) -> MeterBallistics {
        self.ballistics
    }

    pub fn set_ballistics(&mut self, ballistics: MeterBallistics) {
        self.ballistics = ballistics;
    }

    pub fn reading(&self) -> MeterReading {
        self.reading
    }

    pub fn reset(&mut self) {
        self.reading = MeterReading::default();
        self.hold_remaining = 0.0;
    }

    /// Advance toward `input` by `delta_seconds` and return the new reading.
    pub fn update(&mut self, input: f32, delta_seconds: f32) -> MeterReading {
        let input = if input.is_finite() {
            input.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let dt = if delta_seconds.is_finite() {
            delta_seconds.max(0.0)
        } else {
            0.0
        };
        let time = if input >= self.reading.level {
            self.ballistics.attack_seconds
        } else {
            self.ballistics.release_seconds
        };
        self.reading.level += (input - self.reading.level) * smoothing_alpha(dt, time);

        if input >= self.reading.peak {
            self.reading.peak = input;
            self.hold_remaining = self.ballistics.peak_hold_seconds.max(0.0);
        } else if self.hold_remaining > 0.0 {
            self.hold_remaining = (self.hold_remaining - dt).max(0.0);
        } else {
            self.reading.peak += (self.reading.level - self.reading.peak)
                * smoothing_alpha(dt, self.ballistics.peak_release_seconds);
            self.reading.peak = self.reading.peak.max(self.reading.level);
        }
        self.reading
    }
}

impl Default for PeakMeterSmoother {
    fn default() -> Self {
        Self::new(MeterBallistics::default())
    }
}

fn smoothing_alpha(delta_seconds: f32, time_constant: f32) -> f32 {
    if time_constant <= 0.0 {
        1.0
    } else {
        1.0 - (-delta_seconds / time_constant).exp()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ballistics() -> MeterBallistics {
        MeterBallistics {
            attack_seconds: 0.1,
            release_seconds: 0.5,
            peak_hold_seconds: 0.2,
            peak_release_seconds: 0.4,
        }
    }

    #[test]
    fn attack_is_faster_than_release() {
        let mut meter = PeakMeterSmoother::new(ballistics());
        let risen = meter.update(1.0, 0.1).level;
        let fallen = meter.update(0.0, 0.1).level;
        assert!(risen > 0.6);
        assert!(fallen > risen * 0.7);
    }

    #[test]
    fn peak_holds_then_decays_without_crossing_level() {
        let mut meter = PeakMeterSmoother::new(ballistics());
        meter.update(1.0, 0.1);
        let held = meter.update(0.0, 0.1);
        assert_eq!(held.peak, 1.0);
        meter.update(0.0, 0.11);
        let decayed = meter.update(0.0, 0.1);
        assert!(decayed.peak < 1.0);
        assert!(decayed.peak >= decayed.level);
    }

    #[test]
    fn timing_is_frame_rate_independent() {
        let mut one_step = PeakMeterSmoother::new(ballistics());
        let mut ten_steps = PeakMeterSmoother::new(ballistics());
        let a = one_step.update(0.8, 0.1).level;
        for _ in 0..10 {
            ten_steps.update(0.8, 0.01);
        }
        assert!((a - ten_steps.reading().level).abs() < 1.0e-5);
    }

    #[test]
    fn invalid_input_becomes_silence_and_reset_clears_state() {
        let mut meter = PeakMeterSmoother::new(ballistics());
        meter.update(1.0, 1.0);
        meter.update(f32::NAN, 0.1);
        meter.reset();
        assert_eq!(meter.reading(), MeterReading::default());
    }
}
