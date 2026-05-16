//! Sequencer pattern data.
//!
//! Pure data + math. No audio dependencies. The audio engine consumes
//! these types and renders them.
//!
//! All public types derive `serde::Serialize` and `serde::Deserialize`,
//! so patterns can be saved as JSON presets and round-tripped. Sample
//! buffers are not embedded — only sample ids — so a deserialized
//! pattern that uses [`Sound::Sample`] must be rehydrated by looking
//! ids up in a [`crate::samples::SampleBank`].

use core::time::Duration;

use serde::{Deserialize, Serialize};

use crate::sound::Sound;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Subdivision {
    /// Steps per beat. 1=quarter, 2=eighth, 4=sixteenth, 8=thirty-second,
    /// 3=eighth-note triplet, 6=sixteenth-note triplet.
    pub divisions_per_beat: u8,
}

impl Subdivision {
    pub const QUARTER: Self = Self { divisions_per_beat: 1 };
    pub const EIGHTH: Self = Self { divisions_per_beat: 2 };
    pub const SIXTEENTH: Self = Self { divisions_per_beat: 4 };
    pub const THIRTY_SECOND: Self = Self { divisions_per_beat: 8 };
    pub const EIGHTH_TRIPLET: Self = Self { divisions_per_beat: 3 };
    pub const SIXTEENTH_TRIPLET: Self = Self { divisions_per_beat: 6 };
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TimeSignature {
    /// Beats per bar.
    pub numerator: u8,
    /// Beat unit; 4 means quarter note. Currently informational — the
    /// sequencer treats one beat as one quarter note regardless.
    pub denominator: u8,
}

impl TimeSignature {
    pub const fn new(numerator: u8, denominator: u8) -> Self {
        Self { numerator, denominator }
    }
}

impl Default for TimeSignature {
    fn default() -> Self {
        Self::new(4, 4)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Step {
    Empty,
    Active { accent: bool },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Track {
    pub name: String,
    pub steps: Vec<Step>,
    pub sound: Sound,
    pub muted: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SequencerPattern {
    pub bpm: f32,
    pub time_signature: TimeSignature,
    pub subdivision: Subdivision,
    pub tracks: Vec<Track>,
}

impl SequencerPattern {
    /// Total step count in one bar.
    pub fn steps_per_bar(&self) -> usize {
        self.time_signature.numerator as usize
            * self.subdivision.divisions_per_beat as usize
    }

    /// Wall-clock duration of one step at the current BPM.
    pub fn step_duration(&self) -> Duration {
        let secs =
            60.0 / (self.bpm * self.subdivision.divisions_per_beat as f32);
        Duration::from_secs_f32(secs)
    }

    /// Standard metronome: clicks on each beat, downbeat accented.
    /// Subdivision determines internal resolution but only beat-aligned
    /// steps fire.
    pub fn metronome(
        bpm: f32,
        time_signature: TimeSignature,
        subdivision: Subdivision,
    ) -> Self {
        let dpb = subdivision.divisions_per_beat as usize;
        let mut steps = Vec::with_capacity(time_signature.numerator as usize * dpb);
        for beat in 0..time_signature.numerator as usize {
            for div in 0..dpb {
                let on_beat = div == 0;
                let downbeat = beat == 0 && div == 0;
                steps.push(if on_beat {
                    Step::Active { accent: downbeat }
                } else {
                    Step::Empty
                });
            }
        }
        Self {
            bpm,
            time_signature,
            subdivision,
            tracks: vec![Track {
                name: "Click".to_string(),
                steps,
                sound: Sound::click(),
                muted: false,
            }],
        }
    }

    /// Dense metronome: clicks on every subdivision (not just beats),
    /// downbeat accented.
    pub fn metronome_dense(
        bpm: f32,
        time_signature: TimeSignature,
        subdivision: Subdivision,
    ) -> Self {
        let mut p = Self::metronome(bpm, time_signature, subdivision);
        for (i, step) in p.tracks[0].steps.iter_mut().enumerate() {
            *step = Step::Active {
                accent: i == 0,
            };
        }
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_signature_defaults_to_4_4() {
        let ts = TimeSignature::default();
        assert_eq!(ts.numerator, 4);
        assert_eq!(ts.denominator, 4);
    }

    #[test]
    fn subdivision_constants_are_correct() {
        assert_eq!(Subdivision::QUARTER.divisions_per_beat, 1);
        assert_eq!(Subdivision::EIGHTH.divisions_per_beat, 2);
        assert_eq!(Subdivision::SIXTEENTH.divisions_per_beat, 4);
        assert_eq!(Subdivision::THIRTY_SECOND.divisions_per_beat, 8);
        assert_eq!(Subdivision::EIGHTH_TRIPLET.divisions_per_beat, 3);
        assert_eq!(Subdivision::SIXTEENTH_TRIPLET.divisions_per_beat, 6);
    }

    #[test]
    fn steps_per_bar_4_4_quarter_is_4() {
        let p = SequencerPattern::metronome(
            120.0,
            TimeSignature::new(4, 4),
            Subdivision::QUARTER,
        );
        assert_eq!(p.steps_per_bar(), 4);
    }

    #[test]
    fn steps_per_bar_4_4_sixteenth_is_16() {
        let p = SequencerPattern::metronome(
            120.0,
            TimeSignature::new(4, 4),
            Subdivision::SIXTEENTH,
        );
        assert_eq!(p.steps_per_bar(), 16);
    }

    #[test]
    fn steps_per_bar_3_4_thirty_second_is_24() {
        let p = SequencerPattern::metronome(
            120.0,
            TimeSignature::new(3, 4),
            Subdivision::THIRTY_SECOND,
        );
        assert_eq!(p.steps_per_bar(), 24);
    }

    #[test]
    fn steps_per_bar_6_8_eighth_triplet_is_18() {
        let p = SequencerPattern::metronome(
            120.0,
            TimeSignature::new(6, 8),
            Subdivision::EIGHTH_TRIPLET,
        );
        assert_eq!(p.steps_per_bar(), 18);
    }

    #[test]
    fn step_duration_120_bpm_quarter_is_500ms() {
        let p = SequencerPattern::metronome(
            120.0,
            TimeSignature::default(),
            Subdivision::QUARTER,
        );
        let ms = p.step_duration().as_millis();
        assert_eq!(ms, 500);
    }

    #[test]
    fn step_duration_60_bpm_eighth_is_500ms() {
        let p = SequencerPattern::metronome(
            60.0,
            TimeSignature::default(),
            Subdivision::EIGHTH,
        );
        let ms = p.step_duration().as_millis();
        assert_eq!(ms, 500);
    }

    #[test]
    fn metronome_only_beats_fire() {
        let p = SequencerPattern::metronome(
            120.0,
            TimeSignature::new(4, 4),
            Subdivision::SIXTEENTH,
        );
        // Steps 0, 4, 8, 12 should be Active; rest Empty
        for (i, step) in p.tracks[0].steps.iter().enumerate() {
            if i % 4 == 0 {
                assert!(matches!(step, Step::Active { .. }), "step {i} should fire");
            } else {
                assert!(matches!(step, Step::Empty), "step {i} should be empty");
            }
        }
    }

    #[test]
    fn metronome_downbeat_is_accented() {
        let p = SequencerPattern::metronome(
            120.0,
            TimeSignature::new(4, 4),
            Subdivision::QUARTER,
        );
        match p.tracks[0].steps[0] {
            Step::Active { accent } => assert!(accent),
            _ => panic!("step 0 should be active"),
        }
        // Other beats are not accented.
        for i in 1..4 {
            match p.tracks[0].steps[i] {
                Step::Active { accent } => assert!(!accent, "step {i} should not be accented"),
                _ => panic!("step {i} should be active"),
            }
        }
    }

    #[test]
    fn pattern_round_trips_through_serde_json() {
        let original = SequencerPattern::metronome(
            128.0,
            TimeSignature::new(7, 8),
            Subdivision::EIGHTH,
        );
        let json = serde_json::to_string(&original).unwrap();
        let restored: SequencerPattern = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.bpm, original.bpm);
        assert_eq!(restored.time_signature, original.time_signature);
        assert_eq!(restored.subdivision, original.subdivision);
        assert_eq!(restored.tracks.len(), original.tracks.len());
        assert_eq!(restored.tracks[0].name, original.tracks[0].name);
        assert_eq!(restored.tracks[0].steps, original.tracks[0].steps);
        assert_eq!(restored.tracks[0].muted, original.tracks[0].muted);
    }

    #[test]
    fn metronome_dense_fires_every_step() {
        let p = SequencerPattern::metronome_dense(
            120.0,
            TimeSignature::new(4, 4),
            Subdivision::SIXTEENTH,
        );
        for (i, step) in p.tracks[0].steps.iter().enumerate() {
            assert!(matches!(step, Step::Active { .. }), "step {i} should fire");
        }
    }
}
