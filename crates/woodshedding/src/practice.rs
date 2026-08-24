//! Practice sets — ordered sequences of musical items that the app
//! drives through at tempo, advancing automatically.
//!
//! A [`PracticeSet`] is a named sequence of [`PracticeItem`]s. Each
//! item carries enough information to render itself (which scale or
//! chord, in which key, optionally at which position). The runner
//! lives in the app crate and consumes this data through time.
//!
//! # Why this exists
//!
//! Without practice mode, the user manually rotates through scales /
//! chords / exercises by clicking. Practice mode turns the app from a
//! *reference* into a *tool*: the user picks a set ("major scale in
//! all 12 keys, circle of fifths, 4 bars each") and plays — the app
//! cycles through the rotation while they play.

use crate::chord::{catalog as chord_catalog, ChordFormula};
use crate::exercise::{catalog as exercise_catalog, Exercise};
use crate::pitch::{Accidental, NoteName, Pitch};
use crate::scale::{catalog as scale_catalog, ScaleFormula};

#[derive(Clone, Debug)]
pub enum PracticeItem {
    Scale {
        formula: &'static ScaleFormula,
        root: Pitch,
        /// Lowest fret of a 5-fret hand position. 0 = open / 1st position.
        position: u8,
    },
    Chord {
        formula: &'static ChordFormula,
        root: Pitch,
        position: u8,
    },
    Exercise {
        exercise: &'static Exercise,
        starting_fret: u8,
    },
}

impl PracticeItem {
    /// Short user-facing label like "C Major" or "Dm7" or "Chromatic 1-2-3-4 (fret 5)".
    pub fn label(&self) -> String {
        match self {
            Self::Scale { formula, root, .. } => format!(
                "{}{} {}",
                root.name,
                accidental_short(root.accidental),
                formula.name
            ),
            Self::Chord { formula, root, .. } => {
                let r = format!("{}{}", root.name, accidental_short(root.accidental));
                if formula.symbol.is_empty() {
                    r
                } else {
                    format!("{}{}", r, formula.symbol)
                }
            }
            Self::Exercise {
                exercise,
                starting_fret,
            } => {
                format!("{} (fret {})", exercise.name, starting_fret)
            }
        }
    }
}

fn accidental_short(a: Accidental) -> &'static str {
    match a {
        Accidental::DoubleFlat => "bb",
        Accidental::Flat => "b",
        Accidental::Natural => "",
        Accidental::Sharp => "#",
        Accidental::DoubleSharp => "##",
    }
}

#[derive(Clone, Debug)]
pub struct PracticeSet {
    pub name: String,
    pub description: String,
    pub items: Vec<PracticeItem>,
}

// === Pitch class lists for common rotations ===

const CHROMATIC_ROOTS: [(NoteName, Accidental); 12] = [
    (NoteName::C, Accidental::Natural),
    (NoteName::C, Accidental::Sharp),
    (NoteName::D, Accidental::Natural),
    (NoteName::D, Accidental::Sharp),
    (NoteName::E, Accidental::Natural),
    (NoteName::F, Accidental::Natural),
    (NoteName::F, Accidental::Sharp),
    (NoteName::G, Accidental::Natural),
    (NoteName::G, Accidental::Sharp),
    (NoteName::A, Accidental::Natural),
    (NoteName::A, Accidental::Sharp),
    (NoteName::B, Accidental::Natural),
];

/// Circle of fifths starting at C, ascending by perfect fifths.
const CIRCLE_OF_FIFTHS_ROOTS: [(NoteName, Accidental); 12] = [
    (NoteName::C, Accidental::Natural),
    (NoteName::G, Accidental::Natural),
    (NoteName::D, Accidental::Natural),
    (NoteName::A, Accidental::Natural),
    (NoteName::E, Accidental::Natural),
    (NoteName::B, Accidental::Natural),
    (NoteName::F, Accidental::Sharp),
    (NoteName::C, Accidental::Sharp),
    (NoteName::G, Accidental::Sharp),
    (NoteName::D, Accidental::Sharp),
    (NoteName::A, Accidental::Sharp),
    (NoteName::F, Accidental::Natural),
];

/// The seven diatonic modes of the major-scale family, in order.
const DIATONIC_MODES: [&str; 7] = [
    "Major",
    "Dorian",
    "Phrygian",
    "Lydian",
    "Mixolydian",
    "Minor",
    "Locrian",
];

/// Standard CAGED positions for major / minor chords.
const CAGED_POSITIONS: [u8; 5] = [0, 3, 5, 7, 10];

// === Generators ===

fn scale_by_name(name: &str) -> Option<&'static ScaleFormula> {
    scale_catalog().iter().find(|s| s.name == name)
}

fn chord_by_name(name: &str) -> Option<&'static ChordFormula> {
    chord_catalog().iter().find(|c| c.name == name)
}

fn exercise_by_name(name: &str) -> Option<&'static Exercise> {
    exercise_catalog().iter().find(|e| e.name == name)
}

/// Build a scale set traversing every key in chromatic order at the
/// given starting position.
pub fn scales_chromatic(formula: &'static ScaleFormula, position: u8) -> PracticeSet {
    let items = CHROMATIC_ROOTS
        .iter()
        .map(|(n, a)| PracticeItem::Scale {
            formula,
            root: Pitch::new(*n, *a, 4),
            position,
        })
        .collect();
    PracticeSet {
        name: format!("{} — all 12 keys, chromatic", formula.name),
        description: format!(
            "The {} scale in every key, chromatic order. Builds key fluency.",
            formula.name
        ),
        items,
    }
}

/// Build a scale set traversing every key in circle-of-fifths order.
pub fn scales_circle_of_fifths(formula: &'static ScaleFormula, position: u8) -> PracticeSet {
    let items = CIRCLE_OF_FIFTHS_ROOTS
        .iter()
        .map(|(n, a)| PracticeItem::Scale {
            formula,
            root: Pitch::new(*n, *a, 4),
            position,
        })
        .collect();
    PracticeSet {
        name: format!("{} — circle of fifths", formula.name),
        description: format!(
            "The {} scale moving through the circle of fifths. Trains \
             the ear and key relationships.",
            formula.name
        ),
        items,
    }
}

/// Build a set running through the seven diatonic modes rooted on the
/// same pitch ("C Major, C Dorian, C Phrygian, …").
pub fn diatonic_modes(root: Pitch, position: u8) -> PracticeSet {
    let mut items = Vec::with_capacity(DIATONIC_MODES.len());
    for name in DIATONIC_MODES {
        if let Some(formula) = scale_by_name(name) {
            items.push(PracticeItem::Scale {
                formula,
                root,
                position,
            });
        }
    }
    PracticeSet {
        name: format!(
            "Modes from {}{}",
            root.name,
            accidental_short(root.accidental)
        ),
        description: "All seven diatonic modes rooted on the same pitch. \
                      Hear how mode-mixture changes color."
            .to_string(),
        items,
    }
}

/// Build a chord set across the five CAGED-system positions.
pub fn chord_caged(formula: &'static ChordFormula, root: Pitch) -> PracticeSet {
    let items = CAGED_POSITIONS
        .iter()
        .map(|p| PracticeItem::Chord {
            formula,
            root,
            position: *p,
        })
        .collect();
    let r = format!("{}{}", root.name, accidental_short(root.accidental));
    let symbol = if formula.symbol.is_empty() {
        r.clone()
    } else {
        format!("{}{}", r, formula.symbol)
    };
    PracticeSet {
        name: format!("{} — CAGED positions", symbol),
        description: format!(
            "Play {} at every standard hand position across the neck.",
            symbol
        ),
        items,
    }
}

/// Build an exercise set that walks the same exercise across multiple
/// fret positions.
pub fn exercise_positions(exercise: &'static Exercise, positions: &[u8]) -> PracticeSet {
    let items = positions
        .iter()
        .map(|fret| PracticeItem::Exercise {
            exercise,
            starting_fret: *fret,
        })
        .collect();
    PracticeSet {
        name: format!("{} — across positions", exercise.name),
        description: format!(
            "{} run through multiple hand positions. {}",
            exercise.name, exercise.description
        ),
        items,
    }
}

// === Built-in catalog ===

/// A curated catalog of practice sets, ready to use. Computed lazily
/// because formulas come from runtime catalogs.
pub fn catalog() -> Vec<PracticeSet> {
    let mut sets = Vec::new();
    let c4 = Pitch::natural(NoteName::C, 4);
    let a4 = Pitch::natural(NoteName::A, 4);

    if let Some(major) = scale_by_name("Major") {
        sets.push(scales_chromatic(major, 0));
        sets.push(scales_circle_of_fifths(major, 0));
    }
    if let Some(minor) = scale_by_name("Minor") {
        sets.push(scales_circle_of_fifths(minor, 0));
    }
    if let Some(pent) = scale_by_name("Minor Pentatonic") {
        sets.push(scales_chromatic(pent, 5));
    }
    sets.push(diatonic_modes(c4, 0));
    sets.push(diatonic_modes(a4, 5));

    if let Some(major) = chord_by_name("Major") {
        sets.push(chord_caged(major, c4));
    }
    if let Some(maj7) = chord_by_name("Major 7") {
        sets.push(chord_caged(maj7, c4));
    }
    if let Some(dom7) = chord_by_name("Dominant 7") {
        sets.push(chord_caged(dom7, c4));
    }

    if let Some(chrom) = exercise_by_name("Chromatic 1-2-3-4") {
        sets.push(exercise_positions(chrom, &[1, 5, 7, 9, 12]));
    }
    if let Some(spider) = exercise_by_name("Spider 1-3-2-4") {
        sets.push(exercise_positions(spider, &[1, 5, 7, 9, 12]));
    }

    sets
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chromatic_scales_set_has_12_items() {
        let major = scale_by_name("Major").unwrap();
        let set = scales_chromatic(major, 0);
        assert_eq!(set.items.len(), 12);
    }

    #[test]
    fn circle_of_fifths_has_12_items_and_starts_with_c() {
        let major = scale_by_name("Major").unwrap();
        let set = scales_circle_of_fifths(major, 0);
        assert_eq!(set.items.len(), 12);
        if let PracticeItem::Scale { root, .. } = &set.items[0] {
            assert_eq!(root.name, NoteName::C);
        } else {
            panic!("first item should be a scale");
        }
    }

    #[test]
    fn diatonic_modes_has_seven_items() {
        let c4 = Pitch::natural(NoteName::C, 4);
        let set = diatonic_modes(c4, 0);
        assert_eq!(set.items.len(), 7);
    }

    #[test]
    fn caged_chord_set_has_five_items() {
        let major = chord_by_name("Major").unwrap();
        let c4 = Pitch::natural(NoteName::C, 4);
        let set = chord_caged(major, c4);
        assert_eq!(set.items.len(), 5);
    }

    #[test]
    fn item_labels_are_human_readable() {
        let major = scale_by_name("Major").unwrap();
        let set = scales_chromatic(major, 0);
        // First item: C Major
        assert_eq!(set.items[0].label(), "C Major");
        // Second item: C# Major
        assert_eq!(set.items[1].label(), "C# Major");
    }

    #[test]
    fn catalog_is_nonempty() {
        let sets = catalog();
        assert!(sets.len() >= 5);
    }

    #[test]
    fn exercise_positions_produces_one_item_per_fret() {
        let chrom = exercise_by_name("Chromatic 1-2-3-4").unwrap();
        let set = exercise_positions(chrom, &[1, 5, 7]);
        assert_eq!(set.items.len(), 3);
    }
}
