//! Scale formulas.
//!
//! A [`ScaleFormula`] is an ordered list of intervals from the root,
//! including the unison and excluding the upper octave. Apply a formula
//! to any [`Pitch`] to generate the scale's pitches with correct
//! enharmonic spelling.
//!
//! The catalog covers diatonic modes, pentatonic, blues, harmonic minor
//! and modes, melodic minor and modes, symmetric scales, exotic scales,
//! and bebop. Non-tertiary scales are a future expansion.
//!
//! # Example
//!
//! ```
//! use music_theory::pitch::{NoteName, Pitch};
//! use music_theory::scale::{catalog, ScaleFormula};
//!
//! let major = catalog().iter().find(|s| s.name == "Major").unwrap();
//! let c_major = major.apply_to(Pitch::natural(NoteName::C, 4)).unwrap();
//! // c_major == [C4, D4, E4, F4, G4, A4, B4]
//! ```

use core::fmt;

use crate::interval::Interval;
use crate::pitch::{Pitch, TranspositionError};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ScaleCategory {
    Diatonic,
    Pentatonic,
    Blues,
    Bebop,
    Symmetric,
    /// Harmonic minor, melodic minor, and their modes.
    AlteredMinor,
    /// World, gypsy, neapolitan, byzantine, etc.
    Exotic,
    /// Built from non-third intervals — placeholder for future entries.
    NonTertiary,
}

impl fmt::Display for ScaleCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ScaleCategory::Diatonic => "Diatonic",
            ScaleCategory::Pentatonic => "Pentatonic",
            ScaleCategory::Blues => "Blues",
            ScaleCategory::Bebop => "Bebop",
            ScaleCategory::Symmetric => "Symmetric",
            ScaleCategory::AlteredMinor => "Altered Minor",
            ScaleCategory::Exotic => "Exotic",
            ScaleCategory::NonTertiary => "Non-Tertiary",
        })
    }
}

#[derive(Copy, Clone, Debug)]
pub struct ScaleFormula {
    pub name: &'static str,
    /// Intervals from the root, including unison, excluding the octave.
    pub intervals: &'static [Interval],
    pub category: ScaleCategory,
}

impl ScaleFormula {
    /// Apply this formula to a root pitch, producing the scale pitches
    /// in ascending order. Includes the root, excludes the upper octave.
    pub fn apply_to(&self, root: Pitch) -> Result<Vec<Pitch>, TranspositionError> {
        self.intervals
            .iter()
            .map(|iv| root.transposed_by(*iv))
            .collect()
    }

    /// Number of unique scale degrees (length of `intervals`).
    pub fn degree_count(&self) -> usize {
        self.intervals.len()
    }
}

/// Return all scale formulas in the catalog.
pub fn catalog() -> &'static [ScaleFormula] {
    CATALOG
}

/// Iterate the catalog filtered by category.
pub fn catalog_in_category(
    category: ScaleCategory,
) -> impl Iterator<Item = &'static ScaleFormula> {
    catalog().iter().filter(move |f| f.category == category)
}

/// Recognize which scale formula(s) match a sequence of ascending pitches,
/// taking the first pitch as the root. Comparison is chromatic
/// (pitch-class), so spelling differences don't affect the match.
/// Multiple formulas may match for ambiguous patterns (rare).
pub fn recognize(pitches: &[Pitch]) -> Vec<&'static ScaleFormula> {
    if pitches.is_empty() {
        return vec![];
    }
    let root_midi = pitches[0].midi();
    let observed: Vec<i32> = pitches
        .iter()
        .map(|p| (p.midi() - root_midi).rem_euclid(12))
        .collect();
    catalog()
        .iter()
        .filter(|f| {
            if f.intervals.len() != observed.len() {
                return false;
            }
            f.intervals
                .iter()
                .zip(observed.iter())
                .all(|(iv, &s)| iv.semitones().rem_euclid(12) == s)
        })
        .collect()
}

// === Catalog ===
//
// Convention: each formula starts with PERFECT_UNISON. Comments give
// the conventional scale-degree shorthand (1, b3, #5, etc.).

use crate::interval::Interval as I;

static CATALOG: &[ScaleFormula] = &[
    // ── Diatonic modes ──
    ScaleFormula {
        // 1 2 3 4 5 6 7
        name: "Major",
        intervals: &[
            I::PERFECT_UNISON,
            I::MAJOR_SECOND,
            I::MAJOR_THIRD,
            I::PERFECT_FOURTH,
            I::PERFECT_FIFTH,
            I::MAJOR_SIXTH,
            I::MAJOR_SEVENTH,
        ],
        category: ScaleCategory::Diatonic,
    },
    ScaleFormula {
        // 1 2 b3 4 5 6 b7
        name: "Dorian",
        intervals: &[
            I::PERFECT_UNISON,
            I::MAJOR_SECOND,
            I::MINOR_THIRD,
            I::PERFECT_FOURTH,
            I::PERFECT_FIFTH,
            I::MAJOR_SIXTH,
            I::MINOR_SEVENTH,
        ],
        category: ScaleCategory::Diatonic,
    },
    ScaleFormula {
        // 1 b2 b3 4 5 b6 b7
        name: "Phrygian",
        intervals: &[
            I::PERFECT_UNISON,
            I::MINOR_SECOND,
            I::MINOR_THIRD,
            I::PERFECT_FOURTH,
            I::PERFECT_FIFTH,
            I::MINOR_SIXTH,
            I::MINOR_SEVENTH,
        ],
        category: ScaleCategory::Diatonic,
    },
    ScaleFormula {
        // 1 2 3 #4 5 6 7
        name: "Lydian",
        intervals: &[
            I::PERFECT_UNISON,
            I::MAJOR_SECOND,
            I::MAJOR_THIRD,
            I::AUGMENTED_FOURTH,
            I::PERFECT_FIFTH,
            I::MAJOR_SIXTH,
            I::MAJOR_SEVENTH,
        ],
        category: ScaleCategory::Diatonic,
    },
    ScaleFormula {
        // 1 2 3 4 5 6 b7
        name: "Mixolydian",
        intervals: &[
            I::PERFECT_UNISON,
            I::MAJOR_SECOND,
            I::MAJOR_THIRD,
            I::PERFECT_FOURTH,
            I::PERFECT_FIFTH,
            I::MAJOR_SIXTH,
            I::MINOR_SEVENTH,
        ],
        category: ScaleCategory::Diatonic,
    },
    ScaleFormula {
        // 1 2 b3 4 5 b6 b7  (natural minor)
        name: "Minor",
        intervals: &[
            I::PERFECT_UNISON,
            I::MAJOR_SECOND,
            I::MINOR_THIRD,
            I::PERFECT_FOURTH,
            I::PERFECT_FIFTH,
            I::MINOR_SIXTH,
            I::MINOR_SEVENTH,
        ],
        category: ScaleCategory::Diatonic,
    },
    ScaleFormula {
        // 1 b2 b3 4 b5 b6 b7
        name: "Locrian",
        intervals: &[
            I::PERFECT_UNISON,
            I::MINOR_SECOND,
            I::MINOR_THIRD,
            I::PERFECT_FOURTH,
            I::DIMINISHED_FIFTH,
            I::MINOR_SIXTH,
            I::MINOR_SEVENTH,
        ],
        category: ScaleCategory::Diatonic,
    },
    // ── Pentatonic ──
    ScaleFormula {
        // 1 2 3 5 6
        name: "Major Pentatonic",
        intervals: &[
            I::PERFECT_UNISON,
            I::MAJOR_SECOND,
            I::MAJOR_THIRD,
            I::PERFECT_FIFTH,
            I::MAJOR_SIXTH,
        ],
        category: ScaleCategory::Pentatonic,
    },
    ScaleFormula {
        // 1 b3 4 5 b7
        name: "Minor Pentatonic",
        intervals: &[
            I::PERFECT_UNISON,
            I::MINOR_THIRD,
            I::PERFECT_FOURTH,
            I::PERFECT_FIFTH,
            I::MINOR_SEVENTH,
        ],
        category: ScaleCategory::Pentatonic,
    },
    // ── Blues ──
    ScaleFormula {
        // 1 b3 4 b5 5 b7  (six-note minor blues)
        name: "Blues",
        intervals: &[
            I::PERFECT_UNISON,
            I::MINOR_THIRD,
            I::PERFECT_FOURTH,
            I::DIMINISHED_FIFTH,
            I::PERFECT_FIFTH,
            I::MINOR_SEVENTH,
        ],
        category: ScaleCategory::Blues,
    },
    ScaleFormula {
        // 1 2 b3 3 5 6  (six-note major blues)
        name: "Major Blues",
        intervals: &[
            I::PERFECT_UNISON,
            I::MAJOR_SECOND,
            I::MINOR_THIRD,
            I::MAJOR_THIRD,
            I::PERFECT_FIFTH,
            I::MAJOR_SIXTH,
        ],
        category: ScaleCategory::Blues,
    },
    // ── Harmonic minor and modes ──
    ScaleFormula {
        // 1 2 b3 4 5 b6 7
        name: "Harmonic Minor",
        intervals: &[
            I::PERFECT_UNISON,
            I::MAJOR_SECOND,
            I::MINOR_THIRD,
            I::PERFECT_FOURTH,
            I::PERFECT_FIFTH,
            I::MINOR_SIXTH,
            I::MAJOR_SEVENTH,
        ],
        category: ScaleCategory::AlteredMinor,
    },
    ScaleFormula {
        // 1 b2 b3 4 b5 6 b7  (mode 2 of harmonic minor)
        name: "Locrian #6",
        intervals: &[
            I::PERFECT_UNISON,
            I::MINOR_SECOND,
            I::MINOR_THIRD,
            I::PERFECT_FOURTH,
            I::DIMINISHED_FIFTH,
            I::MAJOR_SIXTH,
            I::MINOR_SEVENTH,
        ],
        category: ScaleCategory::AlteredMinor,
    },
    ScaleFormula {
        // 1 2 3 4 #5 6 7  (mode 3 of harmonic minor)
        name: "Ionian #5",
        intervals: &[
            I::PERFECT_UNISON,
            I::MAJOR_SECOND,
            I::MAJOR_THIRD,
            I::PERFECT_FOURTH,
            I::AUGMENTED_FIFTH,
            I::MAJOR_SIXTH,
            I::MAJOR_SEVENTH,
        ],
        category: ScaleCategory::AlteredMinor,
    },
    ScaleFormula {
        // 1 2 b3 #4 5 6 b7  (mode 4 of harmonic minor)
        name: "Dorian #4",
        intervals: &[
            I::PERFECT_UNISON,
            I::MAJOR_SECOND,
            I::MINOR_THIRD,
            I::AUGMENTED_FOURTH,
            I::PERFECT_FIFTH,
            I::MAJOR_SIXTH,
            I::MINOR_SEVENTH,
        ],
        category: ScaleCategory::AlteredMinor,
    },
    ScaleFormula {
        // 1 b2 3 4 5 b6 b7  (mode 5 of harmonic minor)
        name: "Phrygian Dominant",
        intervals: &[
            I::PERFECT_UNISON,
            I::MINOR_SECOND,
            I::MAJOR_THIRD,
            I::PERFECT_FOURTH,
            I::PERFECT_FIFTH,
            I::MINOR_SIXTH,
            I::MINOR_SEVENTH,
        ],
        category: ScaleCategory::AlteredMinor,
    },
    ScaleFormula {
        // 1 #2 3 #4 5 6 7  (mode 6 of harmonic minor)
        name: "Lydian #2",
        intervals: &[
            I::PERFECT_UNISON,
            I::AUGMENTED_SECOND,
            I::MAJOR_THIRD,
            I::AUGMENTED_FOURTH,
            I::PERFECT_FIFTH,
            I::MAJOR_SIXTH,
            I::MAJOR_SEVENTH,
        ],
        category: ScaleCategory::AlteredMinor,
    },
    ScaleFormula {
        // 1 b2 b3 b4 b5 b6 bb7  (mode 7 of harmonic minor)
        name: "Super Locrian bb7",
        intervals: &[
            I::PERFECT_UNISON,
            I::MINOR_SECOND,
            I::MINOR_THIRD,
            I::DIMINISHED_FOURTH,
            I::DIMINISHED_FIFTH,
            I::MINOR_SIXTH,
            I::DIMINISHED_SEVENTH,
        ],
        category: ScaleCategory::AlteredMinor,
    },
    // ── Melodic minor and modes (ascending form) ──
    ScaleFormula {
        // 1 2 b3 4 5 6 7
        name: "Melodic Minor",
        intervals: &[
            I::PERFECT_UNISON,
            I::MAJOR_SECOND,
            I::MINOR_THIRD,
            I::PERFECT_FOURTH,
            I::PERFECT_FIFTH,
            I::MAJOR_SIXTH,
            I::MAJOR_SEVENTH,
        ],
        category: ScaleCategory::AlteredMinor,
    },
    ScaleFormula {
        // 1 b2 b3 4 5 6 b7  (mode 2 of melodic minor)
        name: "Dorian b2",
        intervals: &[
            I::PERFECT_UNISON,
            I::MINOR_SECOND,
            I::MINOR_THIRD,
            I::PERFECT_FOURTH,
            I::PERFECT_FIFTH,
            I::MAJOR_SIXTH,
            I::MINOR_SEVENTH,
        ],
        category: ScaleCategory::AlteredMinor,
    },
    ScaleFormula {
        // 1 2 3 #4 #5 6 7  (mode 3 of melodic minor)
        name: "Lydian Augmented",
        intervals: &[
            I::PERFECT_UNISON,
            I::MAJOR_SECOND,
            I::MAJOR_THIRD,
            I::AUGMENTED_FOURTH,
            I::AUGMENTED_FIFTH,
            I::MAJOR_SIXTH,
            I::MAJOR_SEVENTH,
        ],
        category: ScaleCategory::AlteredMinor,
    },
    ScaleFormula {
        // 1 2 3 #4 5 6 b7  (mode 4 of melodic minor)
        name: "Lydian Dominant",
        intervals: &[
            I::PERFECT_UNISON,
            I::MAJOR_SECOND,
            I::MAJOR_THIRD,
            I::AUGMENTED_FOURTH,
            I::PERFECT_FIFTH,
            I::MAJOR_SIXTH,
            I::MINOR_SEVENTH,
        ],
        category: ScaleCategory::AlteredMinor,
    },
    ScaleFormula {
        // 1 2 3 4 5 b6 b7  (mode 5 of melodic minor — Hindu / Aeolian dominant)
        name: "Mixolydian b6",
        intervals: &[
            I::PERFECT_UNISON,
            I::MAJOR_SECOND,
            I::MAJOR_THIRD,
            I::PERFECT_FOURTH,
            I::PERFECT_FIFTH,
            I::MINOR_SIXTH,
            I::MINOR_SEVENTH,
        ],
        category: ScaleCategory::AlteredMinor,
    },
    ScaleFormula {
        // 1 2 b3 4 b5 b6 b7  (mode 6 of melodic minor)
        name: "Locrian #2",
        intervals: &[
            I::PERFECT_UNISON,
            I::MAJOR_SECOND,
            I::MINOR_THIRD,
            I::PERFECT_FOURTH,
            I::DIMINISHED_FIFTH,
            I::MINOR_SIXTH,
            I::MINOR_SEVENTH,
        ],
        category: ScaleCategory::AlteredMinor,
    },
    ScaleFormula {
        // 1 b2 b3 b4 b5 b6 b7  (mode 7 of melodic minor — the "altered scale")
        name: "Altered",
        intervals: &[
            I::PERFECT_UNISON,
            I::MINOR_SECOND,
            I::MINOR_THIRD,
            I::DIMINISHED_FOURTH,
            I::DIMINISHED_FIFTH,
            I::MINOR_SIXTH,
            I::MINOR_SEVENTH,
        ],
        category: ScaleCategory::AlteredMinor,
    },
    // ── Symmetric ──
    ScaleFormula {
        // 1 2 3 #4 #5 #6  (six tones, all whole steps)
        name: "Whole Tone",
        intervals: &[
            I::PERFECT_UNISON,
            I::MAJOR_SECOND,
            I::MAJOR_THIRD,
            I::AUGMENTED_FOURTH,
            I::AUGMENTED_FIFTH,
            I::AUGMENTED_SIXTH,
        ],
        category: ScaleCategory::Symmetric,
    },
    ScaleFormula {
        // 1 2 b3 4 b5 b6 bb7 7  (W H W H W H W H from root)
        name: "Diminished (whole-half)",
        intervals: &[
            I::PERFECT_UNISON,
            I::MAJOR_SECOND,
            I::MINOR_THIRD,
            I::PERFECT_FOURTH,
            I::DIMINISHED_FIFTH,
            I::MINOR_SIXTH,
            I::DIMINISHED_SEVENTH,
            I::MAJOR_SEVENTH,
        ],
        category: ScaleCategory::Symmetric,
    },
    ScaleFormula {
        // 1 b2 b3 3 #4 5 6 b7  (H W H W H W H W from root — common over dominant 7 chords)
        name: "Diminished (half-whole)",
        intervals: &[
            I::PERFECT_UNISON,
            I::MINOR_SECOND,
            I::MINOR_THIRD,
            I::MAJOR_THIRD,
            I::AUGMENTED_FOURTH,
            I::PERFECT_FIFTH,
            I::MAJOR_SIXTH,
            I::MINOR_SEVENTH,
        ],
        category: ScaleCategory::Symmetric,
    },
    ScaleFormula {
        // 1 #2 3 5 #5 7  (alternating m3 and m2)
        name: "Augmented",
        intervals: &[
            I::PERFECT_UNISON,
            I::AUGMENTED_SECOND,
            I::MAJOR_THIRD,
            I::PERFECT_FIFTH,
            I::AUGMENTED_FIFTH,
            I::MAJOR_SEVENTH,
        ],
        category: ScaleCategory::Symmetric,
    },
    ScaleFormula {
        // 1 b2 2 b3 3 4 #4 5 b6 6 b7 7  (all 12 notes)
        name: "Chromatic",
        intervals: &[
            I::PERFECT_UNISON,
            I::MINOR_SECOND,
            I::MAJOR_SECOND,
            I::MINOR_THIRD,
            I::MAJOR_THIRD,
            I::PERFECT_FOURTH,
            I::AUGMENTED_FOURTH,
            I::PERFECT_FIFTH,
            I::MINOR_SIXTH,
            I::MAJOR_SIXTH,
            I::MINOR_SEVENTH,
            I::MAJOR_SEVENTH,
        ],
        category: ScaleCategory::Symmetric,
    },
    // ── Bebop ──
    ScaleFormula {
        // 1 2 3 4 5 6 b7 7
        name: "Bebop Dominant",
        intervals: &[
            I::PERFECT_UNISON,
            I::MAJOR_SECOND,
            I::MAJOR_THIRD,
            I::PERFECT_FOURTH,
            I::PERFECT_FIFTH,
            I::MAJOR_SIXTH,
            I::MINOR_SEVENTH,
            I::MAJOR_SEVENTH,
        ],
        category: ScaleCategory::Bebop,
    },
    ScaleFormula {
        // 1 2 3 4 5 #5 6 7
        name: "Bebop Major",
        intervals: &[
            I::PERFECT_UNISON,
            I::MAJOR_SECOND,
            I::MAJOR_THIRD,
            I::PERFECT_FOURTH,
            I::PERFECT_FIFTH,
            I::AUGMENTED_FIFTH,
            I::MAJOR_SIXTH,
            I::MAJOR_SEVENTH,
        ],
        category: ScaleCategory::Bebop,
    },
    ScaleFormula {
        // 1 2 b3 3 4 5 6 b7
        name: "Bebop Dorian",
        intervals: &[
            I::PERFECT_UNISON,
            I::MAJOR_SECOND,
            I::MINOR_THIRD,
            I::MAJOR_THIRD,
            I::PERFECT_FOURTH,
            I::PERFECT_FIFTH,
            I::MAJOR_SIXTH,
            I::MINOR_SEVENTH,
        ],
        category: ScaleCategory::Bebop,
    },
    // ── Exotic ──
    ScaleFormula {
        // 1 2 b3 #4 5 b6 7
        name: "Hungarian Minor",
        intervals: &[
            I::PERFECT_UNISON,
            I::MAJOR_SECOND,
            I::MINOR_THIRD,
            I::AUGMENTED_FOURTH,
            I::PERFECT_FIFTH,
            I::MINOR_SIXTH,
            I::MAJOR_SEVENTH,
        ],
        category: ScaleCategory::Exotic,
    },
    ScaleFormula {
        // 1 b2 3 4 5 b6 7  (Byzantine / Arabic)
        name: "Double Harmonic Major",
        intervals: &[
            I::PERFECT_UNISON,
            I::MINOR_SECOND,
            I::MAJOR_THIRD,
            I::PERFECT_FOURTH,
            I::PERFECT_FIFTH,
            I::MINOR_SIXTH,
            I::MAJOR_SEVENTH,
        ],
        category: ScaleCategory::Exotic,
    },
    ScaleFormula {
        // 1 b2 b3 4 5 6 7
        name: "Neapolitan Major",
        intervals: &[
            I::PERFECT_UNISON,
            I::MINOR_SECOND,
            I::MINOR_THIRD,
            I::PERFECT_FOURTH,
            I::PERFECT_FIFTH,
            I::MAJOR_SIXTH,
            I::MAJOR_SEVENTH,
        ],
        category: ScaleCategory::Exotic,
    },
    ScaleFormula {
        // 1 b2 b3 4 5 b6 7
        name: "Neapolitan Minor",
        intervals: &[
            I::PERFECT_UNISON,
            I::MINOR_SECOND,
            I::MINOR_THIRD,
            I::PERFECT_FOURTH,
            I::PERFECT_FIFTH,
            I::MINOR_SIXTH,
            I::MAJOR_SEVENTH,
        ],
        category: ScaleCategory::Exotic,
    },
    ScaleFormula {
        // 1 2 b3 5 b6
        name: "Hirajoshi",
        intervals: &[
            I::PERFECT_UNISON,
            I::MAJOR_SECOND,
            I::MINOR_THIRD,
            I::PERFECT_FIFTH,
            I::MINOR_SIXTH,
        ],
        category: ScaleCategory::Exotic,
    },
    ScaleFormula {
        // 1 b2 4 5 b7
        name: "In Sen",
        intervals: &[
            I::PERFECT_UNISON,
            I::MINOR_SECOND,
            I::PERFECT_FOURTH,
            I::PERFECT_FIFTH,
            I::MINOR_SEVENTH,
        ],
        category: ScaleCategory::Exotic,
    },
    ScaleFormula {
        // 1 b2 3 4 b5 b6 7
        name: "Persian",
        intervals: &[
            I::PERFECT_UNISON,
            I::MINOR_SECOND,
            I::MAJOR_THIRD,
            I::PERFECT_FOURTH,
            I::DIMINISHED_FIFTH,
            I::MINOR_SIXTH,
            I::MAJOR_SEVENTH,
        ],
        category: ScaleCategory::Exotic,
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pitch::{Accidental, NoteName};

    fn pitch(name: NoteName, acc: Accidental, octave: i8) -> Pitch {
        Pitch::new(name, acc, octave)
    }

    fn nat(name: NoteName, octave: i8) -> Pitch {
        Pitch::natural(name, octave)
    }

    fn find(name: &str) -> &'static ScaleFormula {
        catalog().iter().find(|s| s.name == name).unwrap()
    }

    #[test]
    fn c_major_produces_c_d_e_f_g_a_b() {
        let major = find("Major");
        let scale = major.apply_to(nat(NoteName::C, 4)).unwrap();
        let expected = vec![
            nat(NoteName::C, 4),
            nat(NoteName::D, 4),
            nat(NoteName::E, 4),
            nat(NoteName::F, 4),
            nat(NoteName::G, 4),
            nat(NoteName::A, 4),
            nat(NoteName::B, 4),
        ];
        assert_eq!(scale, expected);
    }

    #[test]
    fn d_major_produces_correct_sharps() {
        // D major: D E F# G A B C#
        let major = find("Major");
        let scale = major.apply_to(nat(NoteName::D, 4)).unwrap();
        let expected = vec![
            nat(NoteName::D, 4),
            nat(NoteName::E, 4),
            pitch(NoteName::F, Accidental::Sharp, 4),
            nat(NoteName::G, 4),
            nat(NoteName::A, 4),
            nat(NoteName::B, 4),
            pitch(NoteName::C, Accidental::Sharp, 5),
        ];
        assert_eq!(scale, expected);
    }

    #[test]
    fn b_flat_major_produces_correct_flats() {
        // Bb major: Bb C D Eb F G A
        let major = find("Major");
        let bb4 = pitch(NoteName::B, Accidental::Flat, 4);
        let scale = major.apply_to(bb4).unwrap();
        let names: Vec<_> = scale.iter().map(|p| (p.name, p.accidental)).collect();
        assert_eq!(
            names,
            vec![
                (NoteName::B, Accidental::Flat),
                (NoteName::C, Accidental::Natural),
                (NoteName::D, Accidental::Natural),
                (NoteName::E, Accidental::Flat),
                (NoteName::F, Accidental::Natural),
                (NoteName::G, Accidental::Natural),
                (NoteName::A, Accidental::Natural),
            ]
        );
    }

    #[test]
    fn c_minor_produces_c_d_eb_f_g_ab_bb() {
        let minor = find("Minor");
        let scale = minor.apply_to(nat(NoteName::C, 4)).unwrap();
        let expected = vec![
            nat(NoteName::C, 4),
            nat(NoteName::D, 4),
            pitch(NoteName::E, Accidental::Flat, 4),
            nat(NoteName::F, 4),
            nat(NoteName::G, 4),
            pitch(NoteName::A, Accidental::Flat, 4),
            pitch(NoteName::B, Accidental::Flat, 4),
        ];
        assert_eq!(scale, expected);
    }

    #[test]
    fn lydian_has_sharp_four() {
        let lydian = find("Lydian");
        let c_lydian = lydian.apply_to(nat(NoteName::C, 4)).unwrap();
        // 4th degree should be F#
        assert_eq!(
            c_lydian[3],
            pitch(NoteName::F, Accidental::Sharp, 4)
        );
    }

    #[test]
    fn phrygian_dominant_is_flamenco_scale() {
        // E phrygian dominant: E F G# A B C D
        let pd = find("Phrygian Dominant");
        let scale = pd.apply_to(nat(NoteName::E, 4)).unwrap();
        let expected = vec![
            nat(NoteName::E, 4),
            nat(NoteName::F, 4),
            pitch(NoteName::G, Accidental::Sharp, 4),
            nat(NoteName::A, 4),
            nat(NoteName::B, 4),
            nat(NoteName::C, 5),
            nat(NoteName::D, 5),
        ];
        assert_eq!(scale, expected);
    }

    #[test]
    fn altered_scale_pitches_match_jazz_convention() {
        // C altered: C Db Eb Fb Gb Ab Bb
        let altered = find("Altered");
        let scale = altered.apply_to(nat(NoteName::C, 4)).unwrap();
        let expected = vec![
            nat(NoteName::C, 4),
            pitch(NoteName::D, Accidental::Flat, 4),
            pitch(NoteName::E, Accidental::Flat, 4),
            pitch(NoteName::F, Accidental::Flat, 4),
            pitch(NoteName::G, Accidental::Flat, 4),
            pitch(NoteName::A, Accidental::Flat, 4),
            pitch(NoteName::B, Accidental::Flat, 4),
        ];
        assert_eq!(scale, expected);
    }

    #[test]
    fn whole_tone_has_six_pitches() {
        let wt = find("Whole Tone");
        let scale = wt.apply_to(nat(NoteName::C, 4)).unwrap();
        assert_eq!(scale.len(), 6);
    }

    #[test]
    fn diminished_scales_have_eight_pitches() {
        let wh = find("Diminished (whole-half)");
        let hw = find("Diminished (half-whole)");
        assert_eq!(wh.degree_count(), 8);
        assert_eq!(hw.degree_count(), 8);
    }

    #[test]
    fn chromatic_has_twelve_pitches() {
        let chrom = find("Chromatic");
        assert_eq!(chrom.degree_count(), 12);
    }

    #[test]
    fn pentatonic_has_five_pitches() {
        let pent = find("Major Pentatonic");
        assert_eq!(pent.degree_count(), 5);
    }

    #[test]
    fn round_trip_recognize_c_major() {
        let major = find("Major");
        let pitches = major.apply_to(nat(NoteName::C, 4)).unwrap();
        let recognized = recognize(&pitches);
        assert!(recognized.iter().any(|f| f.name == "Major"));
    }

    #[test]
    fn round_trip_recognize_d_dorian() {
        let dorian = find("Dorian");
        let pitches = dorian.apply_to(nat(NoteName::D, 4)).unwrap();
        let recognized = recognize(&pitches);
        assert!(recognized.iter().any(|f| f.name == "Dorian"));
    }

    #[test]
    fn round_trip_recognize_minor_pentatonic() {
        let pent = find("Minor Pentatonic");
        let pitches = pent.apply_to(nat(NoteName::A, 4)).unwrap();
        let recognized = recognize(&pitches);
        assert!(recognized.iter().any(|f| f.name == "Minor Pentatonic"));
    }

    #[test]
    fn catalog_in_category_filters_correctly() {
        let diatonic_count = catalog_in_category(ScaleCategory::Diatonic).count();
        assert_eq!(diatonic_count, 7); // 7 diatonic modes
        let pent_count = catalog_in_category(ScaleCategory::Pentatonic).count();
        assert_eq!(pent_count, 2);
    }

    #[test]
    fn all_catalog_scales_apply_cleanly_to_c() {
        // Sanity: every formula produces a result for C4 root.
        for f in catalog() {
            let result = f.apply_to(nat(NoteName::C, 4));
            assert!(
                result.is_ok(),
                "scale {} failed to apply: {:?}",
                f.name,
                result
            );
        }
    }
}
