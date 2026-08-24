//! Fretboard mapping: scale and chord positions on any tuning.
//!
//! A [`Fretboard`] is a [`Tuning`] plus a fret count. It answers:
//!
//! - **Where does a scale appear?** [`Fretboard::positions_for_scale`]
//!   returns every (string, fret) where a scale degree sounds, with
//!   the scale's preferred spelling.
//! - **Where do chord tones appear?** [`Fretboard::positions_for_chord`]
//!   does the same for chord tones.
//! - **What chord shapes are playable?** [`Fretboard::find_chord_voicings`]
//!   enumerates voicings within a fret window — combinations of one
//!   position per string (or muted) where all chord tones appear and
//!   the root is in the bass.
//!
//! The first two are pure data layout. The third is a search problem
//! that scales with `(options_per_string)^string_count` — bounded but
//! non-trivial. Voicings are returned in discovery order; callers can
//! sort by `fret_span()`, lowest-fret, etc.
//!
//! # Example
//!
//! ```
//! use woodshedding::pitch::{NoteName, Pitch};
//! use woodshedding::tuning::{Instrument, Tuning};
//! use woodshedding::scale::catalog as scale_catalog;
//! use woodshedding::fretboard::Fretboard;
//!
//! let std_guitar = Tuning::find_for("Standard", Instrument::Guitar).unwrap();
//! let board = Fretboard::new(std_guitar, 22);
//! let major = scale_catalog().iter().find(|s| s.name == "Major").unwrap();
//! let positions = board.positions_for_scale(major, Pitch::natural(NoteName::C, 4)).unwrap();
//! assert!(!positions.is_empty());
//! ```

use crate::chord::ChordFormula;
use crate::interval::Interval;
use crate::pitch::{Accidental, NoteName, Pitch, TranspositionError};
use crate::scale::ScaleFormula;
use crate::tuning::Tuning;

/// A fret position that produces a particular pitch.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Position {
    pub string_index: usize,
    pub fret: u8,
    pub pitch: Pitch,
    /// If derived from a scale or chord, the interval from the named root.
    pub interval_from_root: Option<Interval>,
}

/// One string's contribution to a chord voicing.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum StringPlay {
    Muted,
    /// `fret == 0` means open string. `interval_from_root` is `None`
    /// when the played pitch is a slash-chord bass that is not a
    /// chord tone (e.g. the F# in Cmaj7/F#).
    Played {
        fret: u8,
        pitch: Pitch,
        interval_from_root: Option<Interval>,
    },
}

impl StringPlay {
    pub fn is_played(self) -> bool {
        matches!(self, StringPlay::Played { .. })
    }
    pub fn fret(self) -> Option<u8> {
        match self {
            StringPlay::Played { fret, .. } => Some(fret),
            StringPlay::Muted => None,
        }
    }
    pub fn pitch(self) -> Option<Pitch> {
        match self {
            StringPlay::Played { pitch, .. } => Some(pitch),
            StringPlay::Muted => None,
        }
    }
}

/// A chord voicing: one [`StringPlay`] per string in tuning order.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ChordVoicing {
    pub strings: Vec<StringPlay>,
}

impl ChordVoicing {
    /// Distance between the lowest and highest fretted strings (open
    /// strings excluded). Returns 0 if no strings are fretted.
    pub fn fret_span(&self) -> u8 {
        let frets: Vec<u8> = self
            .strings
            .iter()
            .filter_map(|s| match s {
                StringPlay::Played { fret, .. } if *fret > 0 => Some(*fret),
                _ => None,
            })
            .collect();
        if frets.is_empty() {
            return 0;
        }
        frets.iter().max().copied().unwrap() - frets.iter().min().copied().unwrap()
    }

    /// Lowest fret used by any fretted string (open strings excluded).
    /// Returns 0 if no fretted notes (e.g. all open or muted).
    pub fn lowest_fretted_position(&self) -> u8 {
        self.strings
            .iter()
            .filter_map(|s| match s {
                StringPlay::Played { fret, .. } if *fret > 0 => Some(*fret),
                _ => None,
            })
            .min()
            .unwrap_or(0)
    }

    pub fn played_string_count(&self) -> usize {
        self.strings.iter().filter(|s| s.is_played()).count()
    }

    /// Convenience for tests: `[Some(0), None, Some(2), ...]` per string.
    pub fn fret_pattern(&self) -> Vec<Option<u8>> {
        self.strings.iter().map(|s| s.fret()).collect()
    }
}

/// What note is allowed in the bass (lowest sounding string) of a chord
/// voicing.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum BassConstraint {
    /// Bass must be the chord root. The traditional "root position" voicing.
    Root,
    /// Bass must be a chord tone — any inversion (1st = 3rd in bass,
    /// 2nd = 5th in bass, 3rd = 7th in bass for sevenths).
    AnyChordTone,
    /// Bass must have this specific pitch class. Used for slash chords
    /// like C/G (pitch class 7) or Cmaj7/F# (pitch class 6, not a
    /// chord tone). The bass pitch class need not be a chord tone.
    Pitch(u8),
}

#[derive(Clone, Debug)]
pub struct Fretboard {
    pub tuning: Tuning,
    pub fret_count: u8,
}

impl Fretboard {
    pub fn new(tuning: Tuning, fret_count: u8) -> Self {
        Self { tuning, fret_count }
    }

    /// Pitch produced by stopping `string_index` at `fret`. Spelling
    /// uses sharps for chromatic positions; for spelling derived from
    /// a specific scale or chord, use [`Self::positions_for_scale`] or
    /// [`Self::positions_for_chord`].
    pub fn pitch_at(&self, string_index: usize, fret: u8) -> Pitch {
        let open = self.tuning.strings[string_index];
        let target_midi = open.midi() + fret as i32;
        Pitch::from_midi(target_midi, crate::pitch::Spelling::Sharps)
    }

    /// All positions on the fretboard whose pitch class matches a
    /// degree of the scale rooted at `root`. Spelling is taken from
    /// the scale's pitches (so D major scale positions display F# not Gb).
    pub fn positions_for_scale(
        &self,
        scale: &ScaleFormula,
        root: Pitch,
    ) -> Result<Vec<Position>, TranspositionError> {
        let pitches = scale.apply_to(root)?;
        Ok(self.positions_for_pitch_set(&pitches, scale.intervals))
    }

    /// All positions whose pitch class matches a chord tone of `chord`
    /// rooted at `root`. Spelling is taken from the chord's pitches.
    pub fn positions_for_chord(
        &self,
        chord: &ChordFormula,
        root: Pitch,
    ) -> Result<Vec<Position>, TranspositionError> {
        let pitches = chord.apply_to(root)?;
        Ok(self.positions_for_pitch_set(&pitches, chord.intervals))
    }

    fn positions_for_pitch_set(&self, pitches: &[Pitch], intervals: &[Interval]) -> Vec<Position> {
        let mut result = Vec::new();
        for (string_index, &open) in self.tuning.strings.iter().enumerate() {
            for fret in 0..=self.fret_count {
                let target_midi = open.midi() + fret as i32;
                let target_pc = target_midi.rem_euclid(12) as u8;
                if let Some(degree_idx) = pitches.iter().position(|p| p.pitch_class() == target_pc)
                {
                    let canonical = pitches[degree_idx];
                    let pitch = respell_at_midi(canonical.name, canonical.accidental, target_midi);
                    result.push(Position {
                        string_index,
                        fret,
                        pitch,
                        interval_from_root: Some(intervals[degree_idx]),
                    });
                }
            }
        }
        result
    }

    /// Enumerate chord voicings playable in the fret window
    /// `[lowest_fret, lowest_fret + max_fret_span]` with the bass note
    /// being the chord root. Convenience wrapper around
    /// [`Self::find_chord_voicings_for_bass`] with `BassConstraint::Root`.
    pub fn find_chord_voicings(
        &self,
        chord: &ChordFormula,
        root: Pitch,
        lowest_fret: u8,
        max_fret_span: u8,
    ) -> Result<Vec<ChordVoicing>, TranspositionError> {
        self.find_chord_voicings_for_bass(
            chord,
            root,
            lowest_fret,
            max_fret_span,
            BassConstraint::Root,
        )
    }

    /// Enumerate chord voicings with a specific bass requirement.
    ///
    /// - [`BassConstraint::Root`] — root in bass (default voicings)
    /// - [`BassConstraint::AnyChordTone`] — any inversion (1st, 2nd,
    ///   3rd inversion all allowed)
    /// - [`BassConstraint::Pitch(pc)`] — slash chord with that pitch
    ///   class as bass; the slash bass need not be a chord tone
    ///   (e.g. Cmaj7/F# uses pitch class 6)
    ///
    /// Constraints:
    /// - All chord pitch classes appear somewhere in the voicing
    /// - Lowest sounding string satisfies the [`BassConstraint`]
    /// - Each fretted note is in `[lowest_fret, lowest_fret + max_fret_span]`
    /// - Fret span among fretted notes ≤ `max_fret_span`
    /// - At least 3 strings played
    pub fn find_chord_voicings_for_bass(
        &self,
        chord: &ChordFormula,
        root: Pitch,
        lowest_fret: u8,
        max_fret_span: u8,
        bass: BassConstraint,
    ) -> Result<Vec<ChordVoicing>, TranspositionError> {
        let chord_pitches = chord.apply_to(root)?;
        let required_pcs: Vec<u8> = {
            let mut s: Vec<u8> = chord_pitches.iter().map(|p| p.pitch_class()).collect();
            s.sort_unstable();
            s.dedup();
            s
        };
        // Pitch classes positions can use: chord tones, plus the slash
        // bass if specified and not already a chord tone.
        let mut allowed_pcs = required_pcs.clone();
        if let BassConstraint::Pitch(pc) = bass {
            if !allowed_pcs.contains(&pc) {
                allowed_pcs.push(pc);
            }
        }

        let upper = lowest_fret
            .saturating_add(max_fret_span)
            .min(self.fret_count);

        let per_string: Vec<Vec<StringPlay>> = self
            .tuning
            .strings
            .iter()
            .map(|&open| {
                let mut opts = vec![StringPlay::Muted];
                let lowest_in_window = if lowest_fret == 0 { 0 } else { lowest_fret };
                for fret in 0..=upper {
                    if fret > 0 && fret < lowest_in_window {
                        continue;
                    }
                    let target_midi = open.midi() + fret as i32;
                    let target_pc = target_midi.rem_euclid(12) as u8;
                    if !allowed_pcs.contains(&target_pc) {
                        continue;
                    }
                    // Use the chord's spelling if this is a chord tone;
                    // otherwise (slash bass non-chord-tone) fall back
                    // to the root's accidental preference for spelling.
                    let (pitch, interval_from_root) = match chord_pitches
                        .iter()
                        .position(|p| p.pitch_class() == target_pc)
                    {
                        Some(idx) => {
                            let canonical = chord_pitches[idx];
                            (
                                respell_at_midi(canonical.name, canonical.accidental, target_midi),
                                Some(chord.intervals[idx]),
                            )
                        }
                        None => {
                            // Slash bass non-chord-tone. Spell with sharps by default.
                            (
                                Pitch::from_midi(target_midi, crate::pitch::Spelling::Sharps),
                                None,
                            )
                        }
                    };
                    opts.push(StringPlay::Played {
                        fret,
                        pitch,
                        interval_from_root,
                    });
                }
                opts
            })
            .collect();

        let counts: Vec<usize> = per_string.iter().map(|v| v.len()).collect();
        let total: usize = counts.iter().product();
        let root_pc = root.pitch_class();
        let mut result = Vec::new();
        for i in 0..total {
            let mut idx = i;
            let combo: Vec<StringPlay> = per_string
                .iter()
                .zip(counts.iter())
                .map(|(opts, &count)| {
                    let pick = idx % count;
                    idx /= count;
                    opts[pick]
                })
                .collect();
            if let Some(v) =
                self.validate_voicing(combo, &required_pcs, root_pc, max_fret_span, bass)
            {
                result.push(v);
            }
        }
        Ok(result)
    }

    fn validate_voicing(
        &self,
        combo: Vec<StringPlay>,
        required_pcs: &[u8],
        root_pc: u8,
        max_fret_span: u8,
        bass: BassConstraint,
    ) -> Option<ChordVoicing> {
        let played_count = combo.iter().filter(|s| s.is_played()).count();
        if played_count < 3 {
            return None;
        }
        let lowest_played = combo.iter().find_map(|s| match s {
            StringPlay::Played { pitch, .. } => Some(pitch.pitch_class()),
            _ => None,
        })?;
        let bass_ok = match bass {
            BassConstraint::Root => lowest_played == root_pc,
            BassConstraint::AnyChordTone => required_pcs.contains(&lowest_played),
            BassConstraint::Pitch(pc) => lowest_played == pc,
        };
        if !bass_ok {
            return None;
        }
        let mut present: Vec<u8> = combo
            .iter()
            .filter_map(|s| match s {
                StringPlay::Played { pitch, .. } => Some(pitch.pitch_class()),
                _ => None,
            })
            .collect();
        present.sort_unstable();
        present.dedup();
        for pc in required_pcs {
            if !present.contains(pc) {
                return None;
            }
        }
        let voicing = ChordVoicing { strings: combo };
        if voicing.fret_span() > max_fret_span {
            return None;
        }
        Some(voicing)
    }
}

/// Construct a [`Pitch`] with the given letter+accidental that sounds at
/// `target_midi`. The octave is chosen so the resulting pitch's MIDI
/// number equals `target_midi`.
fn respell_at_midi(name: NoteName, accidental: Accidental, target_midi: i32) -> Pitch {
    let semitone_in_octave = name.semitone_offset() + accidental.semitone_offset();
    let octave = (target_midi - semitone_in_octave).div_euclid(12) - 1;
    Pitch::new(name, accidental, octave as i8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chord::catalog as chord_catalog;
    use crate::pitch::Spelling;
    use crate::scale::catalog as scale_catalog;
    use crate::tuning::{Instrument, Tuning};

    fn standard_guitar() -> Fretboard {
        Fretboard::new(
            Tuning::find_for("Standard", Instrument::Guitar).unwrap(),
            22,
        )
    }

    fn nat(name: NoteName, octave: i8) -> Pitch {
        Pitch::natural(name, octave)
    }

    fn find_scale(name: &str) -> &'static ScaleFormula {
        scale_catalog().iter().find(|s| s.name == name).unwrap()
    }

    fn find_chord(name: &str) -> &'static ChordFormula {
        chord_catalog().iter().find(|c| c.name == name).unwrap()
    }

    #[test]
    fn pitch_at_open_strings_matches_tuning() {
        let board = standard_guitar();
        for (i, &open) in board.tuning.strings.iter().enumerate() {
            assert_eq!(board.pitch_at(i, 0).midi(), open.midi());
        }
    }

    #[test]
    fn pitch_at_fret_5_string_6_is_a2() {
        let board = standard_guitar();
        // Standard 6th string (index 0) is E2; fret 5 is A2 (MIDI 45).
        assert_eq!(board.pitch_at(0, 5).midi(), 45);
    }

    #[test]
    fn c_major_scale_positions_cover_every_string() {
        let board = standard_guitar();
        let major = find_scale("Major");
        let positions = board
            .positions_for_scale(major, nat(NoteName::C, 4))
            .unwrap();
        for s in 0..board.tuning.strings.len() {
            assert!(
                positions.iter().any(|p| p.string_index == s),
                "string {s} has no scale positions"
            );
        }
    }

    #[test]
    fn c_major_scale_positions_only_use_pitch_classes_in_scale() {
        let board = standard_guitar();
        let major = find_scale("Major");
        let positions = board
            .positions_for_scale(major, nat(NoteName::C, 4))
            .unwrap();
        // C major pitch classes: 0,2,4,5,7,9,11
        let allowed = [0u8, 2, 4, 5, 7, 9, 11];
        for p in &positions {
            assert!(allowed.contains(&p.pitch.pitch_class()));
        }
    }

    #[test]
    fn d_major_scale_positions_use_sharp_spelling() {
        // D major has F# and C#. Fretboard positions should preserve those.
        let board = standard_guitar();
        let major = find_scale("Major");
        let positions = board
            .positions_for_scale(major, nat(NoteName::D, 4))
            .unwrap();
        let f_sharp_count = positions
            .iter()
            .filter(|p| p.pitch.name == NoteName::F && p.pitch.accidental == Accidental::Sharp)
            .count();
        assert!(
            f_sharp_count > 0,
            "expected F# spellings in D major scale positions"
        );
    }

    #[test]
    fn b_flat_major_scale_positions_use_flat_spelling() {
        let board = standard_guitar();
        let major = find_scale("Major");
        let bb4 = Pitch::new(NoteName::B, Accidental::Flat, 4);
        let positions = board.positions_for_scale(major, bb4).unwrap();
        let e_flat_count = positions
            .iter()
            .filter(|p| p.pitch.name == NoteName::E && p.pitch.accidental == Accidental::Flat)
            .count();
        assert!(
            e_flat_count > 0,
            "expected Eb spellings in Bb major scale positions"
        );
    }

    #[test]
    fn c_major_chord_positions_only_contain_chord_tones() {
        let board = standard_guitar();
        let major = find_chord("Major");
        let positions = board
            .positions_for_chord(major, nat(NoteName::C, 4))
            .unwrap();
        let allowed = [0u8, 4, 7]; // C E G
        for p in &positions {
            assert!(allowed.contains(&p.pitch.pitch_class()));
        }
    }

    #[test]
    fn first_position_e_major_voicing_is_found() {
        // The classic open E major chord: 0-2-2-1-0-0
        let board = standard_guitar();
        let e_major = find_chord("Major");
        let voicings = board
            .find_chord_voicings(e_major, nat(NoteName::E, 2), 0, 4)
            .unwrap();
        let target = vec![Some(0), Some(2), Some(2), Some(1), Some(0), Some(0)];
        let found = voicings.iter().any(|v| v.fret_pattern() == target);
        assert!(
            found,
            "expected open E major shape (0-2-2-1-0-0); voicings = {:?}",
            voicings
                .iter()
                .map(|v| v.fret_pattern())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn voicings_satisfy_root_in_bass() {
        let board = standard_guitar();
        let e_major = find_chord("Major");
        let voicings = board
            .find_chord_voicings(e_major, nat(NoteName::E, 2), 0, 4)
            .unwrap();
        for v in &voicings {
            let lowest = v
                .strings
                .iter()
                .find_map(|s| match s {
                    StringPlay::Played { pitch, .. } => Some(pitch.pitch_class()),
                    _ => None,
                })
                .unwrap();
            assert_eq!(lowest, nat(NoteName::E, 2).pitch_class());
        }
    }

    #[test]
    fn voicings_contain_all_chord_tones() {
        let board = standard_guitar();
        let e_major = find_chord("Major");
        let voicings = board
            .find_chord_voicings(e_major, nat(NoteName::E, 2), 0, 4)
            .unwrap();
        for v in &voicings {
            let mut pcs: Vec<u8> = v
                .strings
                .iter()
                .filter_map(|s| match s {
                    StringPlay::Played { pitch, .. } => Some(pitch.pitch_class()),
                    _ => None,
                })
                .collect();
            pcs.sort_unstable();
            pcs.dedup();
            for pc in &[4u8, 8, 11] {
                assert!(pcs.contains(pc), "voicing missing pc {pc}: {:?}", v);
            }
        }
    }

    #[test]
    fn voicings_respect_fret_span() {
        let board = standard_guitar();
        let g_major = find_chord("Major");
        let voicings = board
            .find_chord_voicings(g_major, nat(NoteName::G, 2), 3, 4)
            .unwrap();
        for v in &voicings {
            assert!(v.fret_span() <= 4, "span exceeds 4: {:?}", v);
        }
    }

    #[test]
    fn fretboard_works_with_alternative_tunings() {
        // Drop D should let us find a D power chord shape across the
        // bottom three strings (0-0-0) at standard hand position.
        let drop_d = Tuning::find_for("Drop D", Instrument::Guitar).unwrap();
        let board = Fretboard::new(drop_d, 22);
        let major = find_chord("Major");
        let positions = board
            .positions_for_chord(major, nat(NoteName::D, 2))
            .unwrap();
        // The lowest string is D2 (open); fret 0 should yield a D position.
        assert!(positions.iter().any(|p| p.string_index == 0 && p.fret == 0));
    }

    #[test]
    fn ukulele_c_major_positions_exist() {
        let uke = Tuning::find_for("Standard (high-G)", Instrument::Ukulele).unwrap();
        let board = Fretboard::new(uke, 18);
        let major = find_chord("Major");
        let positions = board
            .positions_for_chord(major, nat(NoteName::C, 4))
            .unwrap();
        assert!(!positions.is_empty());
    }

    #[test]
    fn pitch_at_uses_sharps_by_default() {
        let board = standard_guitar();
        // String 6 fret 1 is F (= MIDI 41 = F)
        assert_eq!(board.pitch_at(0, 1), Pitch::natural(NoteName::F, 2));
        // String 6 fret 2 is F# (sharps spelling)
        let fs2 = board.pitch_at(0, 2);
        assert_eq!(fs2, Pitch::from_midi(42, Spelling::Sharps));
    }

    #[test]
    fn voicing_count_is_finite_and_reasonable() {
        // Sanity check: E major in window 0..4 should produce a small
        // double-digit number of voicings, not thousands.
        let board = standard_guitar();
        let e_major = find_chord("Major");
        let voicings = board
            .find_chord_voicings(e_major, nat(NoteName::E, 2), 0, 4)
            .unwrap();
        assert!(!voicings.is_empty());
        assert!(
            voicings.len() < 200,
            "too many voicings: {}",
            voicings.len()
        );
    }

    // === Inversions and slash chords ===

    #[test]
    fn any_chord_tone_bass_finds_inversions() {
        // C major = C E G. With AnyChordTone bass, we should find
        // voicings with E in bass (1st inversion) AND G in bass (2nd
        // inversion), in addition to root position.
        let board = standard_guitar();
        let c_major = find_chord("Major");
        let voicings = board
            .find_chord_voicings_for_bass(
                c_major,
                nat(NoteName::C, 4),
                0,
                4,
                BassConstraint::AnyChordTone,
            )
            .unwrap();
        let bass_pcs: Vec<u8> = voicings
            .iter()
            .map(|v| {
                v.strings
                    .iter()
                    .find_map(|s| match s {
                        StringPlay::Played { pitch, .. } => Some(pitch.pitch_class()),
                        _ => None,
                    })
                    .unwrap()
            })
            .collect();
        // C=0, E=4, G=7
        assert!(bass_pcs.contains(&0), "expected root-position voicings");
        assert!(
            bass_pcs.contains(&4),
            "expected 1st-inversion voicings (E bass)"
        );
        assert!(
            bass_pcs.contains(&7),
            "expected 2nd-inversion voicings (G bass)"
        );
    }

    #[test]
    fn slash_chord_with_chord_tone_bass_works() {
        // C/G — C major with G in the bass. Same notes as 2nd
        // inversion C, but the slash-chord notation is more explicit.
        let board = standard_guitar();
        let c_major = find_chord("Major");
        let voicings = board
            .find_chord_voicings_for_bass(
                c_major,
                nat(NoteName::C, 4),
                0,
                4,
                BassConstraint::Pitch(7), // G = pitch class 7
            )
            .unwrap();
        assert!(!voicings.is_empty());
        for v in &voicings {
            let bass_pc = v
                .strings
                .iter()
                .find_map(|s| match s {
                    StringPlay::Played { pitch, .. } => Some(pitch.pitch_class()),
                    _ => None,
                })
                .unwrap();
            assert_eq!(bass_pc, 7, "bass must be G in C/G");
        }
    }

    #[test]
    fn slash_chord_with_non_chord_tone_bass_works() {
        // Cmaj7/F# — Cmaj7 (C E G B) with F# in the bass. F# is NOT a
        // chord tone, but a slash chord allows arbitrary bass.
        let board = standard_guitar();
        let cmaj7 = find_chord("Major 7");
        let voicings = board
            .find_chord_voicings_for_bass(
                cmaj7,
                nat(NoteName::C, 4),
                0,
                5,
                BassConstraint::Pitch(6), // F# = pitch class 6
            )
            .unwrap();
        assert!(
            !voicings.is_empty(),
            "expected at least one Cmaj7/F# voicing"
        );
        for v in &voicings {
            let bass_pc = v
                .strings
                .iter()
                .find_map(|s| match s {
                    StringPlay::Played { pitch, .. } => Some(pitch.pitch_class()),
                    _ => None,
                })
                .unwrap();
            assert_eq!(bass_pc, 6, "bass must be F# (pc 6) in Cmaj7/F#");
            // All chord tones (C E G B = 0 4 7 11) must still appear.
            let mut pcs: Vec<u8> = v
                .strings
                .iter()
                .filter_map(|s| match s {
                    StringPlay::Played { pitch, .. } => Some(pitch.pitch_class()),
                    _ => None,
                })
                .collect();
            pcs.sort_unstable();
            pcs.dedup();
            for required in &[0u8, 4, 7, 11] {
                assert!(
                    pcs.contains(required),
                    "voicing missing chord tone {required}: {:?}",
                    v
                );
            }
        }
    }

    #[test]
    fn slash_chord_bass_uses_none_interval_for_non_chord_tone() {
        // The F# in Cmaj7/F# should have interval_from_root = None on
        // the StringPlay, since F# is not a Cmaj7 chord tone.
        let board = standard_guitar();
        let cmaj7 = find_chord("Major 7");
        let voicings = board
            .find_chord_voicings_for_bass(
                cmaj7,
                nat(NoteName::C, 4),
                0,
                5,
                BassConstraint::Pitch(6),
            )
            .unwrap();
        let v = voicings.first().expect("at least one voicing");
        let bass_play = v.strings.iter().find(|s| s.is_played()).unwrap();
        match bass_play {
            StringPlay::Played {
                interval_from_root,
                pitch,
                ..
            } => {
                assert_eq!(pitch.pitch_class(), 6);
                assert_eq!(*interval_from_root, None);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn root_constraint_default_excludes_inversions() {
        // The original find_chord_voicings (defaults to Root) should
        // never produce voicings with non-root bass.
        let board = standard_guitar();
        let c_major = find_chord("Major");
        let voicings = board
            .find_chord_voicings(c_major, nat(NoteName::C, 4), 0, 4)
            .unwrap();
        for v in &voicings {
            let bass_pc = v
                .strings
                .iter()
                .find_map(|s| match s {
                    StringPlay::Played { pitch, .. } => Some(pitch.pitch_class()),
                    _ => None,
                })
                .unwrap();
            assert_eq!(
                bass_pc, 0,
                "Root constraint must produce only C-bass voicings"
            );
        }
    }
}
