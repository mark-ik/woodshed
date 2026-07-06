//! The Song lane's portable model (S4 slice 7).
//!
//! [`SongBar`] is the neutral bar description the core and views share:
//! everything the timeline renders and everything an audio backend needs
//! to voice the bar (pre-computed chord-tone frequencies, so backends
//! stay theory-free — the same posture as woodshed-audio's `ChordRef`).
//! The desktop backend converts these into `woodshed_audio::Song` bars;
//! the web backend will voice them through Web Audio.

use serde::{Deserialize, Serialize};

use crate::{format_role, StageState};
use woodshedding::progression::catalog as progression_catalog;
use woodshedding::scale::catalog as scale_catalog;

/// One timeline bar: display strings + the voicing data a backend needs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SongBar {
    /// Section / role label ("I", "Chorus"); doubles as the section
    /// marker in the timeline.
    pub label: String,
    /// Chord display label ("A", "Dm7"); empty = silent bar.
    pub chord_label: String,
    /// Chord catalog key for backends that re-derive ("Major", "Minor 7").
    pub formula_name: String,
    /// Root frequency in Hz (0.0 = no chord).
    pub root_freq_hz: f32,
    /// Pre-computed chord-tone frequencies, lowest first.
    pub pitches_hz: Vec<f32>,
    pub bpm: f32,
    /// Time-signature numerator (denominator 4 until meter editing lands).
    pub beats: u8,
    /// Measures this bar block spans (>= 1).
    pub length: u8,
}

/// Materialize the selected progression as song bars — the "Send to
/// Song" flow (one bar block per chord, roman-numeral section labels).
/// `None` until a progression is selected.
pub fn song_from_progression(
    state: &StageState,
    bpm: f32,
) -> Option<(String, Vec<SongBar>)> {
    let idx = state.progression_idx?;
    let prog = progression_catalog().get(idx)?;
    let major = scale_catalog().iter().find(|s| s.name == "Major")?;
    let chords = prog.apply_in_key(state.root(), major).ok()?;
    if chords.is_empty() {
        return None;
    }
    let name = format!("{} in {}", prog.name, state.root_name());
    let bars = chords
        .iter()
        .map(|c| {
            let pitches_hz: Vec<f32> = std::iter::once(c.root.frequency() as f32)
                .chain(
                    c.pitches
                        .iter()
                        .skip(1)
                        .map(|p| p.frequency() as f32),
                )
                .collect();
            SongBar {
                label: format_role(&c.role),
                chord_label: format!(
                    "{}{}{}",
                    c.root.name, c.root.accidental, c.formula.symbol
                ),
                formula_name: c.formula.name.to_string(),
                root_freq_hz: c.root.frequency() as f32,
                pitches_hz,
                bpm,
                beats: 4,
                length: 1,
            }
        })
        .collect();
    Some((name, bars))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Lens;

    #[test]
    fn progression_becomes_song_bars() {
        let mut s = StageState::new();
        s.set_lens(Lens::Progressions);
        assert!(song_from_progression(&s, 100.0).is_none(), "cold start");
        s.select_progression(0);
        let (name, bars) = song_from_progression(&s, 100.0).expect("bars");
        assert!(name.contains(" in A"));
        assert!(!bars.is_empty());
        for b in &bars {
            assert!(b.root_freq_hz > 0.0);
            assert!(b.pitches_hz.len() >= 2, "chord tones voiced");
            assert_eq!(b.bpm, 100.0);
            assert!(!b.label.is_empty(), "roman numeral label");
        }
    }
}
