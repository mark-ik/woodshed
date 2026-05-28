//! Pitches, octaves, MIDI numbers, frequency.
//!
//! [`Pitch`] preserves enharmonic spelling: `C#4` and `Db4` are distinct
//! values that share the same MIDI number and frequency. Use
//! [`Pitch::is_enharmonic_to`] to compare by sounding pitch.

use core::fmt;

use crate::interval::Interval;

/// A chromatic pitch class, 0..=11 (C=0, C#/Db=1, ..., B=11) — pitch
/// without octave or enharmonic spelling. The portable, serializable
/// pitch-class the rehearsal card model stores; richer/UI types (an app's
/// spelled pitch-class enum) convert in and out at the edge. Serializes as
/// the bare `u8`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct PitchClass(u8);

impl PitchClass {
    /// Construct from any integer, wrapping into 0..=11.
    pub const fn new(pc: u8) -> Self {
        PitchClass(pc % 12)
    }

    /// The 0..=11 value.
    pub const fn value(self) -> u8 {
        self.0
    }
}

/// The seven natural note names of Western music.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NoteName {
    C,
    D,
    E,
    F,
    G,
    A,
    B,
}

impl NoteName {
    /// Semitone offset from C within an octave: C=0, D=2, ..., B=11.
    pub const fn semitone_offset(self) -> i32 {
        match self {
            NoteName::C => 0,
            NoteName::D => 2,
            NoteName::E => 4,
            NoteName::F => 5,
            NoteName::G => 7,
            NoteName::A => 9,
            NoteName::B => 11,
        }
    }

    /// Letter index in the diatonic order: C=0, D=1, E=2, F=3, G=4, A=5, B=6.
    pub const fn index(self) -> usize {
        match self {
            NoteName::C => 0,
            NoteName::D => 1,
            NoteName::E => 2,
            NoteName::F => 3,
            NoteName::G => 4,
            NoteName::A => 5,
            NoteName::B => 6,
        }
    }

    /// Note name from a letter index modulo 7.
    pub const fn from_index(idx: usize) -> Self {
        match idx % 7 {
            0 => NoteName::C,
            1 => NoteName::D,
            2 => NoteName::E,
            3 => NoteName::F,
            4 => NoteName::G,
            5 => NoteName::A,
            6 => NoteName::B,
            _ => unreachable!(),
        }
    }
}

impl fmt::Display for NoteName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            NoteName::C => "C",
            NoteName::D => "D",
            NoteName::E => "E",
            NoteName::F => "F",
            NoteName::G => "G",
            NoteName::A => "A",
            NoteName::B => "B",
        };
        f.write_str(s)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Accidental {
    DoubleFlat,
    Flat,
    Natural,
    Sharp,
    DoubleSharp,
}

impl Accidental {
    pub const fn semitone_offset(self) -> i32 {
        match self {
            Accidental::DoubleFlat => -2,
            Accidental::Flat => -1,
            Accidental::Natural => 0,
            Accidental::Sharp => 1,
            Accidental::DoubleSharp => 2,
        }
    }
}

impl fmt::Display for Accidental {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Accidental::DoubleFlat => "bb",
            Accidental::Flat => "b",
            Accidental::Natural => "",
            Accidental::Sharp => "#",
            Accidental::DoubleSharp => "##",
        };
        f.write_str(s)
    }
}

/// Spelling preference for [`Pitch::from_midi`].
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Spelling {
    Sharps,
    Flats,
}

/// A spelled pitch: name + accidental + octave (Scientific Pitch Notation).
///
/// `C4` is middle C (MIDI 60). `A4` is concert A (MIDI 69, 440 Hz).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Pitch {
    pub name: NoteName,
    pub accidental: Accidental,
    pub octave: i8,
}

impl Pitch {
    pub const fn new(name: NoteName, accidental: Accidental, octave: i8) -> Self {
        Self {
            name,
            accidental,
            octave,
        }
    }

    /// Natural pitch (no accidental).
    pub const fn natural(name: NoteName, octave: i8) -> Self {
        Self::new(name, Accidental::Natural, octave)
    }

    /// MIDI note number. C-1 = 0, A4 = 69, G9 = 127. Values outside
    /// 0..=127 are mathematically valid but outside the standard MIDI range.
    pub const fn midi(self) -> i32 {
        (self.octave as i32 + 1) * 12
            + self.name.semitone_offset()
            + self.accidental.semitone_offset()
    }

    /// Frequency in Hz under 12-TET with A4 = 440 Hz.
    pub fn frequency(self) -> f64 {
        440.0 * 2f64.powf((self.midi() as f64 - 69.0) / 12.0)
    }

    /// Pitch class (0..12), independent of octave and spelling.
    /// `C` = 0, `C#` = `Db` = 1, ..., `B` = 11.
    pub const fn pitch_class(self) -> u8 {
        let raw = self.name.semitone_offset() + self.accidental.semitone_offset();
        raw.rem_euclid(12) as u8
    }

    /// True iff `self` and `other` produce the same MIDI number.
    /// Distinct from `==`, which also requires matching spelling.
    pub const fn is_enharmonic_to(self, other: Pitch) -> bool {
        self.midi() == other.midi()
    }

    /// Construct a [`Pitch`] from a MIDI number using the given spelling.
    /// Black keys are spelled as sharps or flats per the preference;
    /// white keys are always natural.
    pub const fn from_midi(midi: i32, spelling: Spelling) -> Self {
        let octave = midi.div_euclid(12) - 1;
        let semi = midi.rem_euclid(12);
        let (name, accidental) = match (semi, spelling) {
            (0, _) => (NoteName::C, Accidental::Natural),
            (1, Spelling::Sharps) => (NoteName::C, Accidental::Sharp),
            (1, Spelling::Flats) => (NoteName::D, Accidental::Flat),
            (2, _) => (NoteName::D, Accidental::Natural),
            (3, Spelling::Sharps) => (NoteName::D, Accidental::Sharp),
            (3, Spelling::Flats) => (NoteName::E, Accidental::Flat),
            (4, _) => (NoteName::E, Accidental::Natural),
            (5, _) => (NoteName::F, Accidental::Natural),
            (6, Spelling::Sharps) => (NoteName::F, Accidental::Sharp),
            (6, Spelling::Flats) => (NoteName::G, Accidental::Flat),
            (7, _) => (NoteName::G, Accidental::Natural),
            (8, Spelling::Sharps) => (NoteName::G, Accidental::Sharp),
            (8, Spelling::Flats) => (NoteName::A, Accidental::Flat),
            (9, _) => (NoteName::A, Accidental::Natural),
            (10, Spelling::Sharps) => (NoteName::A, Accidental::Sharp),
            (10, Spelling::Flats) => (NoteName::B, Accidental::Flat),
            (11, _) => (NoteName::B, Accidental::Natural),
            _ => unreachable!(),
        };
        Self::new(name, accidental, octave as i8)
    }
}

/// Failure mode for spelling-aware transposition.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TranspositionError {
    /// The result requires an accidental beyond DoubleSharp/DoubleFlat
    /// (a triple-sharp/flat). Held semitone offset is reported.
    ExtremeAccidental(i32),
}

impl fmt::Display for TranspositionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TranspositionError::ExtremeAccidental(n) => {
                write!(f, "transposition produced an extreme accidental (offset {n})")
            }
        }
    }
}

impl std::error::Error for TranspositionError {}

impl Pitch {
    /// Transpose `self` upward by `interval`, preserving spelling
    /// according to the interval's diatonic number. So `D + M3 = F#`,
    /// while `D + d4 = Gb` (same MIDI, different spelling).
    pub fn transposed_by(self, interval: Interval) -> Result<Pitch, TranspositionError> {
        let target_midi = self.midi() + interval.semitones();
        let letter_advance = (interval.number() - 1) as usize;
        let total_idx = self.name.index() + letter_advance;
        let target_letter = NoteName::from_index(total_idx);
        let octave_advance = (total_idx / 7) as i8;
        let target_octave = self.octave + octave_advance;
        Self::with_target_letter_octave_and_midi(target_letter, target_octave, target_midi)
    }

    /// Transpose `self` downward by `interval`, preserving spelling.
    pub fn transposed_down_by(self, interval: Interval) -> Result<Pitch, TranspositionError> {
        let target_midi = self.midi() - interval.semitones();
        let letter_step_down = (interval.number() - 1) as i32;
        let total_idx = self.name.index() as i32 - letter_step_down;
        let target_letter = NoteName::from_index(total_idx.rem_euclid(7) as usize);
        let octave_change = total_idx.div_euclid(7) as i8;
        let target_octave = self.octave + octave_change;
        Self::with_target_letter_octave_and_midi(target_letter, target_octave, target_midi)
    }

    fn with_target_letter_octave_and_midi(
        letter: NoteName,
        octave: i8,
        target_midi: i32,
    ) -> Result<Pitch, TranspositionError> {
        let natural_midi = (octave as i32 + 1) * 12 + letter.semitone_offset();
        let offset = target_midi - natural_midi;
        let accidental = match offset {
            -2 => Accidental::DoubleFlat,
            -1 => Accidental::Flat,
            0 => Accidental::Natural,
            1 => Accidental::Sharp,
            2 => Accidental::DoubleSharp,
            other => return Err(TranspositionError::ExtremeAccidental(other)),
        };
        Ok(Pitch::new(letter, accidental, octave))
    }
}

impl fmt::Display for Pitch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}{}", self.name, self.accidental, self.octave)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn middle_c_is_midi_60() {
        assert_eq!(Pitch::natural(NoteName::C, 4).midi(), 60);
    }

    #[test]
    fn concert_a_is_midi_69_and_440_hz() {
        let a4 = Pitch::natural(NoteName::A, 4);
        assert_eq!(a4.midi(), 69);
        assert!((a4.frequency() - 440.0).abs() < 1e-9);
    }

    #[test]
    fn low_e_guitar_is_midi_40() {
        assert_eq!(Pitch::natural(NoteName::E, 2).midi(), 40);
    }

    #[test]
    fn standard_midi_range() {
        assert_eq!(Pitch::natural(NoteName::C, -1).midi(), 0);
        assert_eq!(Pitch::natural(NoteName::G, 9).midi(), 127);
    }

    #[test]
    fn enharmonic_spellings_have_same_midi() {
        let c_sharp = Pitch::new(NoteName::C, Accidental::Sharp, 4);
        let d_flat = Pitch::new(NoteName::D, Accidental::Flat, 4);
        assert_eq!(c_sharp.midi(), d_flat.midi());
        assert!(c_sharp.is_enharmonic_to(d_flat));
        assert_ne!(c_sharp, d_flat);
    }

    #[test]
    fn b_sharp_3_equals_c_4_enharmonic() {
        let b_sharp_3 = Pitch::new(NoteName::B, Accidental::Sharp, 3);
        let c_4 = Pitch::natural(NoteName::C, 4);
        assert_eq!(b_sharp_3.midi(), c_4.midi());
        assert!(b_sharp_3.is_enharmonic_to(c_4));
    }

    #[test]
    fn c_flat_4_equals_b_3_enharmonic() {
        let c_flat_4 = Pitch::new(NoteName::C, Accidental::Flat, 4);
        let b_3 = Pitch::natural(NoteName::B, 3);
        assert_eq!(c_flat_4.midi(), b_3.midi());
    }

    #[test]
    fn pitch_class_ignores_octave_and_spelling() {
        let c4 = Pitch::natural(NoteName::C, 4);
        let c5 = Pitch::natural(NoteName::C, 5);
        let c_sharp = Pitch::new(NoteName::C, Accidental::Sharp, 4);
        let d_flat = Pitch::new(NoteName::D, Accidental::Flat, 4);
        assert_eq!(c4.pitch_class(), 0);
        assert_eq!(c5.pitch_class(), 0);
        assert_eq!(c_sharp.pitch_class(), d_flat.pitch_class());
        assert_eq!(c_sharp.pitch_class(), 1);
    }

    #[test]
    fn from_midi_round_trips_for_full_range() {
        for midi in 0..=127 {
            let p_sharp = Pitch::from_midi(midi, Spelling::Sharps);
            let p_flat = Pitch::from_midi(midi, Spelling::Flats);
            assert_eq!(p_sharp.midi(), midi);
            assert_eq!(p_flat.midi(), midi);
        }
    }

    #[test]
    fn from_midi_sharps_uses_sharps_for_black_keys() {
        let cs4 = Pitch::from_midi(61, Spelling::Sharps);
        assert_eq!(cs4, Pitch::new(NoteName::C, Accidental::Sharp, 4));
    }

    #[test]
    fn from_midi_flats_uses_flats_for_black_keys() {
        let db4 = Pitch::from_midi(61, Spelling::Flats);
        assert_eq!(db4, Pitch::new(NoteName::D, Accidental::Flat, 4));
    }

    #[test]
    fn display_format() {
        assert_eq!(Pitch::natural(NoteName::C, 4).to_string(), "C4");
        assert_eq!(
            Pitch::new(NoteName::F, Accidental::Sharp, 3).to_string(),
            "F#3"
        );
        assert_eq!(
            Pitch::new(NoteName::B, Accidental::Flat, 4).to_string(),
            "Bb4"
        );
        assert_eq!(
            Pitch::new(NoteName::G, Accidental::DoubleSharp, 2).to_string(),
            "G##2"
        );
    }

    #[test]
    fn frequency_octave_doubles() {
        let a4 = Pitch::natural(NoteName::A, 4);
        let a5 = Pitch::natural(NoteName::A, 5);
        assert!((a5.frequency() / a4.frequency() - 2.0).abs() < 1e-9);
    }

    #[test]
    fn transpose_c_up_major_third_is_e() {
        let c4 = Pitch::natural(NoteName::C, 4);
        let e4 = c4.transposed_by(Interval::MAJOR_THIRD).unwrap();
        assert_eq!(e4, Pitch::natural(NoteName::E, 4));
    }

    #[test]
    fn transpose_d_up_major_third_is_f_sharp() {
        let d4 = Pitch::natural(NoteName::D, 4);
        let result = d4.transposed_by(Interval::MAJOR_THIRD).unwrap();
        assert_eq!(result, Pitch::new(NoteName::F, Accidental::Sharp, 4));
    }

    #[test]
    fn transpose_c_up_diminished_fourth_is_f_flat() {
        // d4 = 4 semitones (same as M3), but spelled as a fourth.
        let c4 = Pitch::natural(NoteName::C, 4);
        let dim4 = Interval::try_new(4, super::super::interval::Quality::Diminished).unwrap();
        let result = c4.transposed_by(dim4).unwrap();
        assert_eq!(result, Pitch::new(NoteName::F, Accidental::Flat, 4));
    }

    #[test]
    fn transpose_b_up_minor_second_is_c() {
        let b3 = Pitch::natural(NoteName::B, 3);
        let result = b3.transposed_by(Interval::MINOR_SECOND).unwrap();
        assert_eq!(result, Pitch::natural(NoteName::C, 4));
    }

    #[test]
    fn transpose_c_up_perfect_octave_is_c_next_octave() {
        let c4 = Pitch::natural(NoteName::C, 4);
        let result = c4.transposed_by(Interval::PERFECT_OCTAVE).unwrap();
        assert_eq!(result, Pitch::natural(NoteName::C, 5));
    }

    #[test]
    fn transpose_f_sharp_up_major_third_is_a_sharp() {
        let fs4 = Pitch::new(NoteName::F, Accidental::Sharp, 4);
        let result = fs4.transposed_by(Interval::MAJOR_THIRD).unwrap();
        assert_eq!(result, Pitch::new(NoteName::A, Accidental::Sharp, 4));
    }

    #[test]
    fn transpose_b_flat_up_minor_third_is_d_flat() {
        let bf4 = Pitch::new(NoteName::B, Accidental::Flat, 4);
        let result = bf4.transposed_by(Interval::MINOR_THIRD).unwrap();
        assert_eq!(result, Pitch::new(NoteName::D, Accidental::Flat, 5));
    }

    #[test]
    fn transpose_down_e_minor_third_is_c_sharp() {
        let e4 = Pitch::natural(NoteName::E, 4);
        let result = e4.transposed_down_by(Interval::MINOR_THIRD).unwrap();
        assert_eq!(result, Pitch::new(NoteName::C, Accidental::Sharp, 4));
    }

    #[test]
    fn transpose_down_c_perfect_octave_crosses_octave_boundary() {
        let c5 = Pitch::natural(NoteName::C, 5);
        let result = c5.transposed_down_by(Interval::PERFECT_OCTAVE).unwrap();
        assert_eq!(result, Pitch::natural(NoteName::C, 4));
    }

    #[test]
    fn transpose_preserves_midi_in_addition_to_spelling() {
        // Two enharmonic results should have the same MIDI.
        let c4 = Pitch::natural(NoteName::C, 4);
        let dim4 = Interval::try_new(4, super::super::interval::Quality::Diminished).unwrap();
        let major3 = Interval::MAJOR_THIRD;
        let fb4 = c4.transposed_by(dim4).unwrap();
        let e4 = c4.transposed_by(major3).unwrap();
        assert_eq!(fb4.midi(), e4.midi());
        assert_ne!(fb4, e4);
    }

    #[test]
    fn note_name_index_round_trip() {
        for n in [
            NoteName::C,
            NoteName::D,
            NoteName::E,
            NoteName::F,
            NoteName::G,
            NoteName::A,
            NoteName::B,
        ] {
            assert_eq!(NoteName::from_index(n.index()), n);
        }
    }
}
