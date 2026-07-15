//! String instrument tunings.
//!
//! A [`Tuning`] is an ordered list of open-string [`Pitch`]es plus a name,
//! instrument, and category. String count is implicit in the list length,
//! so 4-string bass, 6-string guitar, 7-string guitar, ukulele, banjo, and
//! arbitrary custom tunings all use the same type.
//!
//! Strings are conventionally listed **as written**: for monotonic
//! tunings (guitar, bass) this is low pitch to high pitch (e.g.
//! "EADGBE" for standard guitar). For re-entrant tunings (high-G
//! ukulele, 5-string banjo) the order matches the conventional written
//! order even when the pitches are not monotonic.
//!
//! The canonical catalog of named tunings lives in [`catalog`]. Use
//! [`Tuning::find`] or [`Tuning::find_for`] to retrieve a tuning by name,
//! or [`Tuning::custom`] to build an arbitrary one.

use core::fmt;

use crate::interval::Interval;
use crate::pitch::{Accidental, NoteName, Pitch, Spelling};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Instrument {
    Guitar,
    Bass,
    Ukulele,
    Banjo,
    Mandolin,
    Violin,
    Viola,
    Cello,
    DoubleBass,
    Bouzouki,
    Charango,
    Cavaquinho,
    Balalaika,
    MountainDulcimer,
    Other,
}

impl Instrument {
    pub const ALL: [Self; 15] = [
        Self::Guitar,
        Self::Bass,
        Self::Ukulele,
        Self::Banjo,
        Self::Mandolin,
        Self::Violin,
        Self::Viola,
        Self::Cello,
        Self::DoubleBass,
        Self::Bouzouki,
        Self::Charango,
        Self::Cavaquinho,
        Self::Balalaika,
        Self::MountainDulcimer,
        Self::Other,
    ];
}

impl fmt::Display for Instrument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Instrument::Guitar => "Guitar",
            Instrument::Bass => "Bass",
            Instrument::Ukulele => "Ukulele",
            Instrument::Banjo => "Banjo",
            Instrument::Mandolin => "Mandolin",
            Instrument::Violin => "Violin",
            Instrument::Viola => "Viola",
            Instrument::Cello => "Cello",
            Instrument::DoubleBass => "Double Bass",
            Instrument::Bouzouki => "Bouzouki",
            Instrument::Charango => "Charango",
            Instrument::Cavaquinho => "Cavaquinho",
            Instrument::Balalaika => "Balalaika",
            Instrument::MountainDulcimer => "Mountain Dulcimer",
            Instrument::Other => "Other",
        })
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TuningCategory {
    Standard,
    AlternativeStandard,
    Dropped,
    Open,
    Modal,
    Regular,
    ExtendedRange,
    Baritone,
    Specialized,
    Custom,
}

impl fmt::Display for TuningCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            TuningCategory::Standard => "Standard",
            TuningCategory::AlternativeStandard => "Alternative Standard",
            TuningCategory::Dropped => "Dropped",
            TuningCategory::Open => "Open",
            TuningCategory::Modal => "Modal",
            TuningCategory::Regular => "Regular",
            TuningCategory::ExtendedRange => "Extended Range",
            TuningCategory::Baritone => "Baritone",
            TuningCategory::Specialized => "Specialized",
            TuningCategory::Custom => "Custom",
        })
    }
}

/// A static, canonical tuning specification. Backed by `&'static` slices
/// so the catalog incurs no allocation.
#[derive(Copy, Clone, Debug)]
pub struct TuningSpec {
    pub name: &'static str,
    pub strings: &'static [Pitch],
    pub instrument: Instrument,
    pub category: TuningCategory,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Tuning {
    pub name: String,
    pub strings: Vec<Pitch>,
    pub instrument: Instrument,
    pub category: TuningCategory,
}

impl Tuning {
    /// Build an arbitrary user-defined tuning. Category is set to
    /// [`TuningCategory::Custom`].
    pub fn custom(
        name: impl Into<String>,
        strings: Vec<Pitch>,
        instrument: Instrument,
    ) -> Self {
        Self {
            name: name.into(),
            strings,
            instrument,
            category: TuningCategory::Custom,
        }
    }

    /// Convert a catalog [`TuningSpec`] into an owned [`Tuning`].
    pub fn from_spec(spec: &TuningSpec) -> Self {
        Self {
            name: spec.name.to_string(),
            strings: spec.strings.to_vec(),
            instrument: spec.instrument,
            category: spec.category,
        }
    }

    /// Find a tuning by name, searching all instruments. Returns the
    /// first match — names that collide across instruments may be
    /// disambiguated with [`Tuning::find_for`].
    pub fn find(name: &str) -> Option<Tuning> {
        catalog()
            .iter()
            .find(|s| s.name == name)
            .map(Tuning::from_spec)
    }

    /// Find a tuning by name and instrument.
    pub fn find_for(name: &str, instrument: Instrument) -> Option<Tuning> {
        catalog()
            .iter()
            .find(|s| s.name == name && s.instrument == instrument)
            .map(Tuning::from_spec)
    }

    pub fn string_count(&self) -> usize {
        self.strings.len()
    }

    // === Generators / transformations ===

    /// Build a tuning from a starting low pitch and the intervals between
    /// adjacent strings. Result has `intervals_between.len() + 1` strings.
    /// Spelling is applied to the generated upper strings; the lowest
    /// string keeps its original spelling.
    pub fn from_pattern(
        name: impl Into<String>,
        instrument: Instrument,
        lowest: Pitch,
        intervals_between: &[Interval],
        spelling: Spelling,
    ) -> Tuning {
        let mut strings = Vec::with_capacity(intervals_between.len() + 1);
        let mut midi = lowest.midi();
        strings.push(lowest);
        for iv in intervals_between {
            midi += iv.semitones();
            strings.push(Pitch::from_midi(midi, spelling));
        }
        Tuning {
            name: name.into(),
            strings,
            instrument,
            category: TuningCategory::Custom,
        }
    }

    /// Build a "regular" tuning where every adjacent string pair is the
    /// same `interval` apart. `All Fourths` = `regular(P4, 6)` from E2.
    pub fn regular(
        name: impl Into<String>,
        instrument: Instrument,
        lowest: Pitch,
        between: Interval,
        strings: usize,
        spelling: Spelling,
    ) -> Tuning {
        assert!(strings >= 1, "tuning must have at least one string");
        let pattern = vec![between; strings - 1];
        Tuning::from_pattern(name, instrument, lowest, &pattern, spelling)
    }

    /// Transpose every string by `semitones`. Spelling for the transposed
    /// pitches follows the given preference. Result is categorized as
    /// [`TuningCategory::Custom`].
    pub fn transposed(&self, semitones: i32, spelling: Spelling) -> Tuning {
        let strings = self
            .strings
            .iter()
            .map(|p| Pitch::from_midi(p.midi() + semitones, spelling))
            .collect();
        Tuning {
            name: format!("{} (transposed {:+})", self.name, semitones),
            strings,
            instrument: self.instrument,
            category: TuningCategory::Custom,
        }
    }

    /// Replace one string by index, leaving the rest untouched.
    pub fn with_string(&self, index: usize, pitch: Pitch) -> Tuning {
        assert!(index < self.strings.len(), "string index out of range");
        let mut strings = self.strings.clone();
        strings[index] = pitch;
        Tuning {
            name: format!("{} (modified)", self.name),
            strings,
            instrument: self.instrument,
            category: TuningCategory::Custom,
        }
    }

    /// Lower the string at `index` by `semitones` (negative raises).
    pub fn dropped(&self, index: usize, semitones: i32, spelling: Spelling) -> Tuning {
        let original = self.strings[index];
        let new_pitch = Pitch::from_midi(original.midi() - semitones, spelling);
        self.with_string(index, new_pitch)
    }
}

impl fmt::Display for Tuning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.name, self.instrument)
    }
}

/// Return all tuning specs in the catalog.
pub fn catalog() -> &'static [TuningSpec] {
    CATALOG
}

/// Return all catalog entries for a given instrument.
pub fn catalog_for(instrument: Instrument) -> impl Iterator<Item = &'static TuningSpec> {
    catalog().iter().filter(move |s| s.instrument == instrument)
}

// === Catalog ===

const fn p(name: NoteName, octave: i8) -> Pitch {
    Pitch::natural(name, octave)
}

const fn ps(name: NoteName, octave: i8) -> Pitch {
    Pitch::new(name, Accidental::Sharp, octave)
}

const fn pf(name: NoteName, octave: i8) -> Pitch {
    Pitch::new(name, Accidental::Flat, octave)
}

static CATALOG: &[TuningSpec] = &[
    // === GUITAR — Standard ===
    TuningSpec {
        name: "Standard",
        strings: &[
            p(NoteName::E, 2),
            p(NoteName::A, 2),
            p(NoteName::D, 3),
            p(NoteName::G, 3),
            p(NoteName::B, 3),
            p(NoteName::E, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::Standard,
    },
    // === GUITAR — AlternativeStandard ===
    TuningSpec {
        name: "Eb Standard",
        strings: &[
            pf(NoteName::E, 2),
            pf(NoteName::A, 2),
            pf(NoteName::D, 3),
            pf(NoteName::G, 3),
            pf(NoteName::B, 3),
            pf(NoteName::E, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::AlternativeStandard,
    },
    TuningSpec {
        name: "D Standard",
        strings: &[
            p(NoteName::D, 2),
            p(NoteName::G, 2),
            p(NoteName::C, 3),
            p(NoteName::F, 3),
            p(NoteName::A, 3),
            p(NoteName::D, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::AlternativeStandard,
    },
    TuningSpec {
        name: "C# Standard",
        strings: &[
            ps(NoteName::C, 2),
            ps(NoteName::F, 2),
            p(NoteName::B, 2),
            p(NoteName::E, 3),
            ps(NoteName::G, 3),
            ps(NoteName::C, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::AlternativeStandard,
    },
    TuningSpec {
        name: "C Standard",
        strings: &[
            p(NoteName::C, 2),
            p(NoteName::F, 2),
            pf(NoteName::B, 2),
            pf(NoteName::E, 3),
            p(NoteName::G, 3),
            p(NoteName::C, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::AlternativeStandard,
    },
    TuningSpec {
        name: "B Standard",
        strings: &[
            p(NoteName::B, 1),
            p(NoteName::E, 2),
            p(NoteName::A, 2),
            p(NoteName::D, 3),
            ps(NoteName::F, 3),
            p(NoteName::B, 3),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::AlternativeStandard,
    },
    TuningSpec {
        name: "A Standard",
        strings: &[
            p(NoteName::A, 1),
            p(NoteName::D, 2),
            p(NoteName::G, 2),
            p(NoteName::C, 3),
            p(NoteName::E, 3),
            p(NoteName::A, 3),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::AlternativeStandard,
    },
    TuningSpec {
        name: "F# Standard",
        strings: &[
            ps(NoteName::F, 2),
            p(NoteName::B, 2),
            p(NoteName::E, 3),
            p(NoteName::A, 3),
            ps(NoteName::C, 4),
            ps(NoteName::F, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::AlternativeStandard,
    },
    TuningSpec {
        name: "G Standard (up)",
        strings: &[
            p(NoteName::G, 2),
            p(NoteName::C, 3),
            p(NoteName::F, 3),
            pf(NoteName::B, 3),
            p(NoteName::D, 4),
            p(NoteName::G, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::AlternativeStandard,
    },
    // === GUITAR — Dropped ===
    TuningSpec {
        name: "Drop D",
        strings: &[
            p(NoteName::D, 2),
            p(NoteName::A, 2),
            p(NoteName::D, 3),
            p(NoteName::G, 3),
            p(NoteName::B, 3),
            p(NoteName::E, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::Dropped,
    },
    TuningSpec {
        name: "Drop C#",
        strings: &[
            ps(NoteName::C, 2),
            ps(NoteName::G, 2),
            ps(NoteName::C, 3),
            ps(NoteName::F, 3),
            ps(NoteName::A, 3),
            ps(NoteName::D, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::Dropped,
    },
    TuningSpec {
        name: "Drop C",
        strings: &[
            p(NoteName::C, 2),
            p(NoteName::G, 2),
            p(NoteName::C, 3),
            p(NoteName::F, 3),
            p(NoteName::A, 3),
            p(NoteName::D, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::Dropped,
    },
    TuningSpec {
        name: "Drop B",
        strings: &[
            p(NoteName::B, 1),
            ps(NoteName::F, 2),
            p(NoteName::B, 2),
            p(NoteName::E, 3),
            ps(NoteName::G, 3),
            ps(NoteName::C, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::Dropped,
    },
    TuningSpec {
        name: "Drop A",
        strings: &[
            p(NoteName::A, 1),
            p(NoteName::E, 2),
            p(NoteName::A, 2),
            p(NoteName::D, 3),
            ps(NoteName::F, 3),
            p(NoteName::B, 3),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::Dropped,
    },
    TuningSpec {
        name: "Drop G",
        strings: &[
            p(NoteName::G, 1),
            p(NoteName::D, 2),
            p(NoteName::G, 2),
            p(NoteName::C, 3),
            p(NoteName::E, 3),
            p(NoteName::A, 3),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::Dropped,
    },
    TuningSpec {
        name: "Double Drop D",
        strings: &[
            p(NoteName::D, 2),
            p(NoteName::A, 2),
            p(NoteName::D, 3),
            p(NoteName::G, 3),
            p(NoteName::B, 3),
            p(NoteName::D, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::Dropped,
    },
    // === GUITAR — Open ===
    TuningSpec {
        name: "Open A",
        strings: &[
            p(NoteName::E, 2),
            p(NoteName::A, 2),
            ps(NoteName::C, 3),
            p(NoteName::E, 3),
            p(NoteName::A, 3),
            p(NoteName::E, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::Open,
    },
    TuningSpec {
        name: "Open B",
        strings: &[
            p(NoteName::B, 1),
            ps(NoteName::F, 2),
            p(NoteName::B, 2),
            ps(NoteName::F, 3),
            p(NoteName::B, 3),
            ps(NoteName::D, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::Open,
    },
    TuningSpec {
        name: "Open C",
        strings: &[
            p(NoteName::C, 2),
            p(NoteName::G, 2),
            p(NoteName::C, 3),
            p(NoteName::G, 3),
            p(NoteName::C, 4),
            p(NoteName::E, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::Open,
    },
    TuningSpec {
        name: "Open C (spread)",
        strings: &[
            p(NoteName::C, 2),
            p(NoteName::E, 2),
            p(NoteName::G, 2),
            p(NoteName::C, 3),
            p(NoteName::E, 3),
            p(NoteName::G, 3),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::Open,
    },
    TuningSpec {
        name: "Open D",
        strings: &[
            p(NoteName::D, 2),
            p(NoteName::A, 2),
            p(NoteName::D, 3),
            ps(NoteName::F, 3),
            p(NoteName::A, 3),
            p(NoteName::D, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::Open,
    },
    TuningSpec {
        name: "Open E",
        strings: &[
            p(NoteName::E, 2),
            p(NoteName::B, 2),
            p(NoteName::E, 3),
            ps(NoteName::G, 3),
            p(NoteName::B, 3),
            p(NoteName::E, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::Open,
    },
    TuningSpec {
        name: "Open F",
        strings: &[
            p(NoteName::F, 2),
            p(NoteName::A, 2),
            p(NoteName::C, 3),
            p(NoteName::F, 3),
            p(NoteName::C, 4),
            p(NoteName::F, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::Open,
    },
    TuningSpec {
        name: "Open G",
        strings: &[
            p(NoteName::D, 2),
            p(NoteName::G, 2),
            p(NoteName::D, 3),
            p(NoteName::G, 3),
            p(NoteName::B, 3),
            p(NoteName::D, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::Open,
    },
    TuningSpec {
        name: "Cross-note A (Am)",
        strings: &[
            p(NoteName::E, 2),
            p(NoteName::A, 2),
            p(NoteName::E, 3),
            p(NoteName::A, 3),
            p(NoteName::C, 4),
            p(NoteName::E, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::Open,
    },
    TuningSpec {
        name: "Cross-note C (Cm)",
        strings: &[
            p(NoteName::C, 2),
            p(NoteName::G, 2),
            p(NoteName::C, 3),
            p(NoteName::G, 3),
            p(NoteName::C, 4),
            pf(NoteName::E, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::Open,
    },
    TuningSpec {
        name: "Cross-note D (Dm)",
        strings: &[
            p(NoteName::D, 2),
            p(NoteName::A, 2),
            p(NoteName::D, 3),
            p(NoteName::F, 3),
            p(NoteName::A, 3),
            p(NoteName::D, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::Open,
    },
    TuningSpec {
        name: "Cross-note E (Em)",
        strings: &[
            p(NoteName::E, 2),
            p(NoteName::B, 2),
            p(NoteName::E, 3),
            p(NoteName::G, 3),
            p(NoteName::B, 3),
            p(NoteName::E, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::Open,
    },
    TuningSpec {
        name: "Cross-note G (Gm)",
        strings: &[
            p(NoteName::D, 2),
            p(NoteName::G, 2),
            p(NoteName::D, 3),
            p(NoteName::G, 3),
            pf(NoteName::B, 3),
            p(NoteName::D, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::Open,
    },
    TuningSpec {
        name: "Open Dm7",
        strings: &[
            p(NoteName::D, 2),
            p(NoteName::A, 2),
            p(NoteName::D, 3),
            p(NoteName::F, 3),
            p(NoteName::A, 3),
            p(NoteName::C, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::Open,
    },
    TuningSpec {
        name: "Open Dmaj7",
        strings: &[
            p(NoteName::D, 2),
            p(NoteName::A, 2),
            p(NoteName::D, 3),
            ps(NoteName::F, 3),
            p(NoteName::A, 3),
            ps(NoteName::C, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::Open,
    },
    TuningSpec {
        name: "Open Gmaj7",
        strings: &[
            p(NoteName::D, 2),
            p(NoteName::G, 2),
            p(NoteName::D, 3),
            ps(NoteName::F, 3),
            p(NoteName::B, 3),
            p(NoteName::D, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::Open,
    },
    // === GUITAR — Modal ===
    TuningSpec {
        name: "Asus2",
        strings: &[
            p(NoteName::E, 2),
            p(NoteName::A, 2),
            p(NoteName::B, 2),
            p(NoteName::E, 3),
            p(NoteName::A, 3),
            p(NoteName::E, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::Modal,
    },
    TuningSpec {
        name: "Asus4",
        strings: &[
            p(NoteName::E, 2),
            p(NoteName::A, 2),
            p(NoteName::D, 3),
            p(NoteName::E, 3),
            p(NoteName::A, 3),
            p(NoteName::E, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::Modal,
    },
    TuningSpec {
        name: "DADGAD",
        strings: &[
            p(NoteName::D, 2),
            p(NoteName::A, 2),
            p(NoteName::D, 3),
            p(NoteName::G, 3),
            p(NoteName::A, 3),
            p(NoteName::D, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::Modal,
    },
    TuningSpec {
        name: "Dsus2",
        strings: &[
            p(NoteName::D, 2),
            p(NoteName::A, 2),
            p(NoteName::D, 3),
            p(NoteName::E, 3),
            p(NoteName::A, 3),
            p(NoteName::D, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::Modal,
    },
    TuningSpec {
        name: "Esus2",
        strings: &[
            p(NoteName::E, 2),
            p(NoteName::B, 2),
            p(NoteName::E, 3),
            ps(NoteName::F, 3),
            p(NoteName::B, 3),
            p(NoteName::E, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::Modal,
    },
    TuningSpec {
        name: "Gsus2",
        strings: &[
            p(NoteName::D, 2),
            p(NoteName::G, 2),
            p(NoteName::D, 3),
            p(NoteName::G, 3),
            p(NoteName::A, 3),
            p(NoteName::D, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::Modal,
    },
    TuningSpec {
        name: "Gsus4",
        strings: &[
            p(NoteName::D, 2),
            p(NoteName::G, 2),
            p(NoteName::D, 3),
            p(NoteName::G, 3),
            p(NoteName::C, 4),
            p(NoteName::D, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::Modal,
    },
    // === GUITAR — Regular ===
    TuningSpec {
        name: "All Fourths",
        strings: &[
            p(NoteName::E, 2),
            p(NoteName::A, 2),
            p(NoteName::D, 3),
            p(NoteName::G, 3),
            p(NoteName::C, 4),
            p(NoteName::F, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::Regular,
    },
    TuningSpec {
        name: "All Fifths",
        strings: &[
            p(NoteName::C, 2),
            p(NoteName::G, 2),
            p(NoteName::D, 3),
            p(NoteName::A, 3),
            p(NoteName::E, 4),
            p(NoteName::B, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::Regular,
    },
    TuningSpec {
        name: "Major Thirds",
        strings: &[
            p(NoteName::E, 2),
            ps(NoteName::G, 2),
            p(NoteName::C, 3),
            p(NoteName::E, 3),
            ps(NoteName::G, 3),
            p(NoteName::C, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::Regular,
    },
    TuningSpec {
        name: "Minor Thirds",
        strings: &[
            p(NoteName::C, 2),
            ps(NoteName::D, 2),
            ps(NoteName::F, 2),
            p(NoteName::A, 2),
            p(NoteName::C, 3),
            ps(NoteName::D, 3),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::Regular,
    },
    TuningSpec {
        name: "New Standard Tuning",
        strings: &[
            p(NoteName::C, 2),
            p(NoteName::G, 2),
            p(NoteName::D, 3),
            p(NoteName::A, 3),
            p(NoteName::E, 4),
            p(NoteName::G, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::Regular,
    },
    TuningSpec {
        name: "Ostrich",
        strings: &[
            p(NoteName::E, 2),
            p(NoteName::E, 2),
            p(NoteName::E, 3),
            p(NoteName::E, 3),
            p(NoteName::E, 4),
            p(NoteName::E, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::Regular,
    },
    // === GUITAR — ExtendedRange ===
    TuningSpec {
        name: "7-string Standard",
        strings: &[
            p(NoteName::B, 1),
            p(NoteName::E, 2),
            p(NoteName::A, 2),
            p(NoteName::D, 3),
            p(NoteName::G, 3),
            p(NoteName::B, 3),
            p(NoteName::E, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::ExtendedRange,
    },
    TuningSpec {
        name: "7-string Drop A",
        strings: &[
            p(NoteName::A, 1),
            p(NoteName::E, 2),
            p(NoteName::A, 2),
            p(NoteName::D, 3),
            p(NoteName::G, 3),
            p(NoteName::B, 3),
            p(NoteName::E, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::ExtendedRange,
    },
    TuningSpec {
        name: "7-string Drop B",
        strings: &[
            p(NoteName::B, 1),
            ps(NoteName::F, 2),
            p(NoteName::B, 2),
            p(NoteName::E, 3),
            ps(NoteName::G, 3),
            ps(NoteName::C, 4),
            ps(NoteName::F, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::ExtendedRange,
    },
    TuningSpec {
        name: "8-string Standard",
        strings: &[
            ps(NoteName::F, 1),
            p(NoteName::B, 1),
            p(NoteName::E, 2),
            p(NoteName::A, 2),
            p(NoteName::D, 3),
            p(NoteName::G, 3),
            p(NoteName::B, 3),
            p(NoteName::E, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::ExtendedRange,
    },
    TuningSpec {
        name: "8-string Drop E",
        strings: &[
            p(NoteName::E, 1),
            p(NoteName::B, 1),
            p(NoteName::E, 2),
            p(NoteName::A, 2),
            p(NoteName::D, 3),
            p(NoteName::G, 3),
            p(NoteName::B, 3),
            p(NoteName::E, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::ExtendedRange,
    },
    // === GUITAR — Baritone ===
    // Pitch sets duplicate AlternativeStandard but the names target
    // baritone-scale guitars.
    TuningSpec {
        name: "Baritone B Standard",
        strings: &[
            p(NoteName::B, 1),
            p(NoteName::E, 2),
            p(NoteName::A, 2),
            p(NoteName::D, 3),
            ps(NoteName::F, 3),
            p(NoteName::B, 3),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::Baritone,
    },
    TuningSpec {
        name: "Baritone A Standard",
        strings: &[
            p(NoteName::A, 1),
            p(NoteName::D, 2),
            p(NoteName::G, 2),
            p(NoteName::C, 3),
            p(NoteName::E, 3),
            p(NoteName::A, 3),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::Baritone,
    },
    TuningSpec {
        name: "Baritone G Standard",
        strings: &[
            p(NoteName::G, 1),
            p(NoteName::C, 2),
            p(NoteName::F, 2),
            pf(NoteName::B, 2),
            p(NoteName::D, 3),
            p(NoteName::G, 3),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::Baritone,
    },
    // === GUITAR — Specialized ===
    TuningSpec {
        name: "Nashville (high-strung)",
        strings: &[
            p(NoteName::E, 3),
            p(NoteName::A, 3),
            p(NoteName::D, 4),
            p(NoteName::G, 4),
            p(NoteName::B, 3),
            p(NoteName::E, 4),
        ],
        instrument: Instrument::Guitar,
        category: TuningCategory::Specialized,
    },
    // === BASS ===
    TuningSpec {
        name: "Standard 4",
        strings: &[
            p(NoteName::E, 1),
            p(NoteName::A, 1),
            p(NoteName::D, 2),
            p(NoteName::G, 2),
        ],
        instrument: Instrument::Bass,
        category: TuningCategory::Standard,
    },
    TuningSpec {
        name: "Standard 5",
        strings: &[
            p(NoteName::B, 0),
            p(NoteName::E, 1),
            p(NoteName::A, 1),
            p(NoteName::D, 2),
            p(NoteName::G, 2),
        ],
        instrument: Instrument::Bass,
        category: TuningCategory::ExtendedRange,
    },
    TuningSpec {
        name: "Standard 6",
        strings: &[
            p(NoteName::B, 0),
            p(NoteName::E, 1),
            p(NoteName::A, 1),
            p(NoteName::D, 2),
            p(NoteName::G, 2),
            p(NoteName::C, 3),
        ],
        instrument: Instrument::Bass,
        category: TuningCategory::ExtendedRange,
    },
    TuningSpec {
        name: "Drop D",
        strings: &[
            p(NoteName::D, 1),
            p(NoteName::A, 1),
            p(NoteName::D, 2),
            p(NoteName::G, 2),
        ],
        instrument: Instrument::Bass,
        category: TuningCategory::Dropped,
    },
    TuningSpec {
        name: "D Standard",
        strings: &[
            p(NoteName::D, 1),
            p(NoteName::G, 1),
            p(NoteName::C, 2),
            p(NoteName::F, 2),
        ],
        instrument: Instrument::Bass,
        category: TuningCategory::AlternativeStandard,
    },
    TuningSpec {
        name: "Eb Standard",
        strings: &[
            pf(NoteName::E, 1),
            pf(NoteName::A, 1),
            pf(NoteName::D, 2),
            pf(NoteName::G, 2),
        ],
        instrument: Instrument::Bass,
        category: TuningCategory::AlternativeStandard,
    },
    TuningSpec {
        name: "Drop C",
        strings: &[
            p(NoteName::C, 1),
            p(NoteName::G, 1),
            p(NoteName::C, 2),
            p(NoteName::F, 2),
        ],
        instrument: Instrument::Bass,
        category: TuningCategory::Dropped,
    },
    TuningSpec {
        name: "Drop B",
        strings: &[
            p(NoteName::B, 0),
            ps(NoteName::F, 1),
            p(NoteName::B, 1),
            p(NoteName::E, 2),
        ],
        instrument: Instrument::Bass,
        category: TuningCategory::Dropped,
    },
    TuningSpec {
        name: "Drop A 5-string",
        strings: &[
            p(NoteName::A, 0),
            p(NoteName::E, 1),
            p(NoteName::A, 1),
            p(NoteName::D, 2),
            p(NoteName::G, 2),
        ],
        instrument: Instrument::Bass,
        category: TuningCategory::Dropped,
    },
    TuningSpec {
        // CGDA: tuned in fifths, one fifth lower than mandolin
        name: "Tenor (fifths)",
        strings: &[
            p(NoteName::A, 1),
            p(NoteName::D, 2),
            p(NoteName::G, 2),
            p(NoteName::C, 3),
        ],
        instrument: Instrument::Bass,
        category: TuningCategory::Regular,
    },
    TuningSpec {
        // Standard pitches one octave up: same range as guitar bottom 4
        name: "Piccolo",
        strings: &[
            p(NoteName::E, 2),
            p(NoteName::A, 2),
            p(NoteName::D, 3),
            p(NoteName::G, 3),
        ],
        instrument: Instrument::Bass,
        category: TuningCategory::Specialized,
    },
    TuningSpec {
        name: "Standard 7",
        strings: &[
            p(NoteName::B, 0),
            p(NoteName::E, 1),
            p(NoteName::A, 1),
            p(NoteName::D, 2),
            p(NoteName::G, 2),
            p(NoteName::C, 3),
            p(NoteName::F, 3),
        ],
        instrument: Instrument::Bass,
        category: TuningCategory::ExtendedRange,
    },
    // === UKULELE ===
    TuningSpec {
        name: "Standard (high-G)",
        strings: &[
            p(NoteName::G, 4),
            p(NoteName::C, 4),
            p(NoteName::E, 4),
            p(NoteName::A, 4),
        ],
        instrument: Instrument::Ukulele,
        category: TuningCategory::Standard,
    },
    TuningSpec {
        name: "Low-G",
        strings: &[
            p(NoteName::G, 3),
            p(NoteName::C, 4),
            p(NoteName::E, 4),
            p(NoteName::A, 4),
        ],
        instrument: Instrument::Ukulele,
        category: TuningCategory::Specialized,
    },
    TuningSpec {
        name: "Baritone",
        strings: &[
            p(NoteName::D, 3),
            p(NoteName::G, 3),
            p(NoteName::B, 3),
            p(NoteName::E, 4),
        ],
        instrument: Instrument::Ukulele,
        category: TuningCategory::Baritone,
    },
    TuningSpec {
        // Older "high-A" D tuning: standard transposed up a whole step
        name: "D Tuning (high-A)",
        strings: &[
            p(NoteName::A, 4),
            p(NoteName::D, 4),
            ps(NoteName::F, 4),
            p(NoteName::B, 4),
        ],
        instrument: Instrument::Ukulele,
        category: TuningCategory::AlternativeStandard,
    },
    TuningSpec {
        // U-bass: bass-pitched ukulele body, EADG range
        name: "Bass (U-bass)",
        strings: &[
            p(NoteName::E, 1),
            p(NoteName::A, 1),
            p(NoteName::D, 2),
            p(NoteName::G, 2),
        ],
        instrument: Instrument::Ukulele,
        category: TuningCategory::Specialized,
    },
    // === BANJO ===
    TuningSpec {
        name: "Open G (5-string)",
        strings: &[
            p(NoteName::G, 4),
            p(NoteName::D, 3),
            p(NoteName::G, 3),
            p(NoteName::B, 3),
            p(NoteName::D, 4),
        ],
        instrument: Instrument::Banjo,
        category: TuningCategory::Standard,
    },
    TuningSpec {
        name: "Double C",
        strings: &[
            p(NoteName::G, 4),
            p(NoteName::C, 3),
            p(NoteName::G, 3),
            p(NoteName::C, 4),
            p(NoteName::D, 4),
        ],
        instrument: Instrument::Banjo,
        category: TuningCategory::Specialized,
    },
    TuningSpec {
        name: "D Tuning",
        strings: &[
            ps(NoteName::F, 4),
            p(NoteName::D, 3),
            ps(NoteName::F, 3),
            p(NoteName::A, 3),
            p(NoteName::D, 4),
        ],
        instrument: Instrument::Banjo,
        category: TuningCategory::Open,
    },
    TuningSpec {
        // Mountain modal / sawmill 5-string tuning (Gsus4 voicing)
        name: "G Modal (sawmill)",
        strings: &[
            p(NoteName::G, 4),
            p(NoteName::D, 3),
            p(NoteName::G, 3),
            p(NoteName::C, 4),
            p(NoteName::D, 4),
        ],
        instrument: Instrument::Banjo,
        category: TuningCategory::Modal,
    },
    TuningSpec {
        // 4-string tenor in fifths, one fifth below mandolin
        name: "Tenor (CGDA)",
        strings: &[
            p(NoteName::C, 3),
            p(NoteName::G, 3),
            p(NoteName::D, 4),
            p(NoteName::A, 4),
        ],
        instrument: Instrument::Banjo,
        category: TuningCategory::Regular,
    },
    TuningSpec {
        // 4-string Irish tenor in fifths, one octave below violin
        name: "Irish Tenor (GDAE)",
        strings: &[
            p(NoteName::G, 2),
            p(NoteName::D, 3),
            p(NoteName::A, 3),
            p(NoteName::E, 4),
        ],
        instrument: Instrument::Banjo,
        category: TuningCategory::Regular,
    },
    TuningSpec {
        // 4-string plectrum banjo standard
        name: "Plectrum (CGBD)",
        strings: &[
            p(NoteName::C, 3),
            p(NoteName::G, 3),
            p(NoteName::B, 3),
            p(NoteName::D, 4),
        ],
        instrument: Instrument::Banjo,
        category: TuningCategory::Standard,
    },
    // === MANDOLIN ===
    TuningSpec {
        name: "Standard",
        strings: &[
            p(NoteName::G, 3),
            p(NoteName::D, 4),
            p(NoteName::A, 4),
            p(NoteName::E, 5),
        ],
        instrument: Instrument::Mandolin,
        category: TuningCategory::Standard,
    },
    TuningSpec {
        // Cross-tuning for old-time fiddle tunes — first string up to A
        name: "Cross-tuning (AEAE)",
        strings: &[
            p(NoteName::A, 3),
            p(NoteName::E, 4),
            p(NoteName::A, 4),
            p(NoteName::E, 5),
        ],
        instrument: Instrument::Mandolin,
        category: TuningCategory::Specialized,
    },
    TuningSpec {
        // Octave mandolin — standard mandolin one octave down
        name: "Octave Mandolin",
        strings: &[
            p(NoteName::G, 2),
            p(NoteName::D, 3),
            p(NoteName::A, 3),
            p(NoteName::E, 4),
        ],
        instrument: Instrument::Mandolin,
        category: TuningCategory::Specialized,
    },
    TuningSpec {
        // Mandola — one fifth below mandolin (viola of the family)
        name: "Mandola (CGDA)",
        strings: &[
            p(NoteName::C, 3),
            p(NoteName::G, 3),
            p(NoteName::D, 4),
            p(NoteName::A, 4),
        ],
        instrument: Instrument::Mandolin,
        category: TuningCategory::Specialized,
    },
    TuningSpec {
        // Mandocello — one octave below mandola (cello of the family)
        name: "Mandocello (CGDA)",
        strings: &[
            p(NoteName::C, 2),
            p(NoteName::G, 2),
            p(NoteName::D, 3),
            p(NoteName::A, 3),
        ],
        instrument: Instrument::Mandolin,
        category: TuningCategory::Specialized,
    },
    // === BOWED FAMILY ===
    // The orchestral string family. Violin/viola/cello are tuned in fifths and
    // share the mandolin family's pitches (mandolin=violin, mandola=viola,
    // mandocello=cello); the double bass is the outlier, tuned in fourths. The
    // model does not care that these are fretless — that is a rendering concern.
    TuningSpec {
        // GDAE, in fifths — identical pitches to the mandolin
        name: "Standard",
        strings: &[
            p(NoteName::G, 3),
            p(NoteName::D, 4),
            p(NoteName::A, 4),
            p(NoteName::E, 5),
        ],
        instrument: Instrument::Violin,
        category: TuningCategory::Standard,
    },
    TuningSpec {
        // Old-time fiddle cross-tuning: first string up to A (AEAE)
        name: "Cross-tuning (AEAE)",
        strings: &[
            p(NoteName::A, 3),
            p(NoteName::E, 4),
            p(NoteName::A, 4),
            p(NoteName::E, 5),
        ],
        instrument: Instrument::Violin,
        category: TuningCategory::Specialized,
    },
    TuningSpec {
        // "Sawmill"/"Calico" cross-tuning (ADAE)
        name: "Cross-tuning (ADAE)",
        strings: &[
            p(NoteName::A, 3),
            p(NoteName::D, 4),
            p(NoteName::A, 4),
            p(NoteName::E, 5),
        ],
        instrument: Instrument::Violin,
        category: TuningCategory::Specialized,
    },
    TuningSpec {
        // CGDA, in fifths — a fifth below the violin, identical to the mandola
        name: "Standard",
        strings: &[
            p(NoteName::C, 3),
            p(NoteName::G, 3),
            p(NoteName::D, 4),
            p(NoteName::A, 4),
        ],
        instrument: Instrument::Viola,
        category: TuningCategory::Standard,
    },
    TuningSpec {
        // CGDA, in fifths — one octave below the viola, identical to mandocello
        name: "Standard",
        strings: &[
            p(NoteName::C, 2),
            p(NoteName::G, 2),
            p(NoteName::D, 3),
            p(NoteName::A, 3),
        ],
        instrument: Instrument::Cello,
        category: TuningCategory::Standard,
    },
    TuningSpec {
        // 5-string cello adds a high E above the A
        name: "5-string (high E)",
        strings: &[
            p(NoteName::C, 2),
            p(NoteName::G, 2),
            p(NoteName::D, 3),
            p(NoteName::A, 3),
            p(NoteName::E, 4),
        ],
        instrument: Instrument::Cello,
        category: TuningCategory::ExtendedRange,
    },
    TuningSpec {
        // Orchestral double bass: EADG in fourths (the bowed-family outlier).
        // Same pitches as a bass guitar's low four, a different instrument.
        name: "Orchestral (fourths)",
        strings: &[
            p(NoteName::E, 1),
            p(NoteName::A, 1),
            p(NoteName::D, 2),
            p(NoteName::G, 2),
        ],
        instrument: Instrument::DoubleBass,
        category: TuningCategory::Standard,
    },
    TuningSpec {
        // Solo tuning: a whole step up (F#BEA), still in fourths
        name: "Solo (fourths)",
        strings: &[
            ps(NoteName::F, 1),
            p(NoteName::B, 1),
            p(NoteName::E, 2),
            p(NoteName::A, 2),
        ],
        instrument: Instrument::DoubleBass,
        category: TuningCategory::AlternativeStandard,
    },
    TuningSpec {
        // 5-string double bass adds a low B below the E
        name: "5-string (low B)",
        strings: &[
            p(NoteName::B, 0),
            p(NoteName::E, 1),
            p(NoteName::A, 1),
            p(NoteName::D, 2),
            p(NoteName::G, 2),
        ],
        instrument: Instrument::DoubleBass,
        category: TuningCategory::ExtendedRange,
    },
    // === WORLD / FOLK ===
    // Common fretted 12-TET instruments. Multi-course instruments (bouzouki,
    // charango, cavaquinho) are modelled as one string per course, as the
    // mandolin is; the doubled/octave strings are a courses-phase refinement.
    // The mountain dulcimer's tuning is faithful, but its real fretboard is
    // diatonic, which is a fret-pattern refinement for a later phase.
    // --- Bouzouki (Irish 4-course and Greek) ---
    TuningSpec {
        // Irish bouzouki, the most common Irish tuning (GDAD)
        name: "Irish (GDAD)",
        strings: &[
            p(NoteName::G, 2),
            p(NoteName::D, 3),
            p(NoteName::A, 3),
            p(NoteName::D, 4),
        ],
        instrument: Instrument::Bouzouki,
        category: TuningCategory::Standard,
    },
    TuningSpec {
        // Irish bouzouki in fifths (GDAE) — same pitches as octave mandolin
        name: "Irish (GDAE)",
        strings: &[
            p(NoteName::G, 2),
            p(NoteName::D, 3),
            p(NoteName::A, 3),
            p(NoteName::E, 4),
        ],
        instrument: Instrument::Bouzouki,
        category: TuningCategory::AlternativeStandard,
    },
    TuningSpec {
        // Modern Greek bouzouki, four-course (tetrachordo): CFAD
        name: "Greek Tetrachordo (CFAD)",
        strings: &[
            p(NoteName::C, 3),
            p(NoteName::F, 3),
            p(NoteName::A, 3),
            p(NoteName::D, 4),
        ],
        instrument: Instrument::Bouzouki,
        category: TuningCategory::Standard,
    },
    TuningSpec {
        // Older Greek bouzouki, three-course (trichordo): DAD
        name: "Greek Trichordo (DAD)",
        strings: &[
            p(NoteName::D, 3),
            p(NoteName::A, 3),
            p(NoteName::D, 4),
        ],
        instrument: Instrument::Bouzouki,
        category: TuningCategory::AlternativeStandard,
    },
    // --- Charango (Andean 5-course, re-entrant) ---
    TuningSpec {
        // Standard GCEAE; the centre E course is a re-entrant octave pair on a
        // real charango (the "little bird"), flattened to one string here.
        name: "Standard (GCEAE)",
        strings: &[
            p(NoteName::G, 4),
            p(NoteName::C, 5),
            p(NoteName::E, 5),
            p(NoteName::A, 4),
            p(NoteName::E, 5),
        ],
        instrument: Instrument::Charango,
        category: TuningCategory::Standard,
    },
    // --- Cavaquinho (Brazilian 4-string) ---
    TuningSpec {
        // Brazilian cavaquinho standard: DGBD
        name: "Brazilian (DGBD)",
        strings: &[
            p(NoteName::D, 4),
            p(NoteName::G, 4),
            p(NoteName::B, 4),
            p(NoteName::D, 5),
        ],
        instrument: Instrument::Cavaquinho,
        category: TuningCategory::Standard,
    },
    // --- Balalaika (prima, 3-string) ---
    TuningSpec {
        // Prima balalaika academic tuning: two unison E strings and an A (EEA)
        name: "Prima (EEA)",
        strings: &[
            p(NoteName::E, 4),
            p(NoteName::E, 4),
            p(NoteName::A, 4),
        ],
        instrument: Instrument::Balalaika,
        category: TuningCategory::Standard,
    },
    // --- Mountain dulcimer (Appalachian, diatonic frets) ---
    TuningSpec {
        // Modern Ionian DAD
        name: "D-A-D",
        strings: &[
            p(NoteName::D, 3),
            p(NoteName::A, 3),
            p(NoteName::D, 4),
        ],
        instrument: Instrument::MountainDulcimer,
        category: TuningCategory::Standard,
    },
    TuningSpec {
        // Older Ionian D-A-A
        name: "D-A-A",
        strings: &[
            p(NoteName::D, 3),
            p(NoteName::A, 3),
            p(NoteName::A, 3),
        ],
        instrument: Instrument::MountainDulcimer,
        category: TuningCategory::AlternativeStandard,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    fn standard_guitar() -> Tuning {
        Tuning::find_for("Standard", Instrument::Guitar).unwrap()
    }

    #[test]
    fn standard_guitar_has_six_strings_in_eadgbe() {
        let t = standard_guitar();
        assert_eq!(t.string_count(), 6);
        assert_eq!(t.strings[0].midi(), 40); // E2
        assert_eq!(t.strings[1].midi(), 45); // A2
        assert_eq!(t.strings[2].midi(), 50); // D3
        assert_eq!(t.strings[3].midi(), 55); // G3
        assert_eq!(t.strings[4].midi(), 59); // B3
        assert_eq!(t.strings[5].midi(), 64); // E4
    }

    #[test]
    fn drop_d_lowers_only_the_sixth_string() {
        let std = standard_guitar();
        let drop_d = Tuning::find_for("Drop D", Instrument::Guitar).unwrap();
        assert_eq!(drop_d.strings[0].midi(), std.strings[0].midi() - 2);
        for i in 1..6 {
            assert_eq!(drop_d.strings[i], std.strings[i]);
        }
    }

    #[test]
    fn open_d_includes_f_sharp() {
        let t = Tuning::find_for("Open D", Instrument::Guitar).unwrap();
        let third = t.strings[3];
        assert_eq!(third.name, NoteName::F);
        assert_eq!(third.accidental, Accidental::Sharp);
    }

    #[test]
    fn five_string_bass_has_low_b() {
        let t = Tuning::find_for("Standard 5", Instrument::Bass).unwrap();
        assert_eq!(t.string_count(), 5);
        assert_eq!(t.strings[0], Pitch::natural(NoteName::B, 0));
    }

    #[test]
    fn ukulele_high_g_is_above_c() {
        let t = Tuning::find_for("Standard (high-G)", Instrument::Ukulele).unwrap();
        assert_eq!(t.string_count(), 4);
        assert!(t.strings[0].midi() > t.strings[1].midi());
        assert_eq!(t.strings[1], Pitch::natural(NoteName::C, 4));
    }

    #[test]
    fn arbitrary_seven_string_tuning_constructs_cleanly() {
        let seven = Tuning::custom(
            "Custom 7-string",
            vec![
                Pitch::natural(NoteName::B, 1),
                Pitch::natural(NoteName::E, 2),
                Pitch::natural(NoteName::A, 2),
                Pitch::natural(NoteName::D, 3),
                Pitch::natural(NoteName::G, 3),
                Pitch::natural(NoteName::B, 3),
                Pitch::natural(NoteName::E, 4),
            ],
            Instrument::Guitar,
        );
        assert_eq!(seven.string_count(), 7);
        assert_eq!(seven.category, TuningCategory::Custom);
    }

    #[test]
    fn arbitrary_five_string_bass_tuning_constructs_cleanly() {
        let bass = Tuning::custom(
            "Custom Bass 5",
            vec![
                Pitch::natural(NoteName::F, 0),
                Pitch::natural(NoteName::B, 0),
                Pitch::natural(NoteName::E, 1),
                Pitch::natural(NoteName::A, 1),
                Pitch::natural(NoteName::D, 2),
            ],
            Instrument::Bass,
        );
        assert_eq!(bass.string_count(), 5);
    }

    #[test]
    fn strings_are_ordered_as_written() {
        let std = standard_guitar();
        let names: Vec<NoteName> = std.strings.iter().map(|p| p.name).collect();
        assert_eq!(
            names,
            vec![
                NoteName::E,
                NoteName::A,
                NoteName::D,
                NoteName::G,
                NoteName::B,
                NoteName::E
            ]
        );
    }

    #[test]
    fn catalog_is_nonempty_and_includes_standard_guitar() {
        assert!(!catalog().is_empty());
        assert!(catalog()
            .iter()
            .any(|s| s.name == "Standard" && s.instrument == Instrument::Guitar));
    }

    #[test]
    fn find_disambiguates_across_instruments() {
        let g = Tuning::find_for("Standard", Instrument::Guitar).unwrap();
        let m = Tuning::find_for("Standard", Instrument::Mandolin).unwrap();
        assert_eq!(g.string_count(), 6);
        assert_eq!(m.string_count(), 4);
        assert_ne!(g.strings, m.strings);
    }

    #[test]
    fn catalog_for_filters_by_instrument() {
        let bass_count = catalog_for(Instrument::Bass).count();
        let uke_count = catalog_for(Instrument::Ukulele).count();
        let guitar_count = catalog_for(Instrument::Guitar).count();
        assert!(bass_count >= 4);
        assert!(uke_count >= 3);
        assert!(guitar_count >= 40);
    }

    #[test]
    fn baritone_category_has_entries() {
        let baritones: Vec<_> = catalog()
            .iter()
            .filter(|s| s.category == TuningCategory::Baritone)
            .collect();
        assert!(!baritones.is_empty());
    }

    #[test]
    fn extended_range_has_seven_and_eight_strings() {
        let er: Vec<_> = catalog()
            .iter()
            .filter(|s| {
                s.category == TuningCategory::ExtendedRange
                    && s.instrument == Instrument::Guitar
            })
            .collect();
        assert!(er.iter().any(|s| s.strings.len() == 7));
        assert!(er.iter().any(|s| s.strings.len() == 8));
    }

    #[test]
    fn nashville_high_strung_has_octave_4_g_string() {
        let nash = Tuning::find_for("Nashville (high-strung)", Instrument::Guitar).unwrap();
        // 4th string (index 3) is G — and it is raised to G4 (the
        // signature of high-strung tuning).
        assert_eq!(nash.strings[3], Pitch::natural(NoteName::G, 4));
    }

    #[test]
    fn ostrich_tuning_is_all_e() {
        let o = Tuning::find_for("Ostrich", Instrument::Guitar).unwrap();
        for s in &o.strings {
            assert_eq!(s.name, NoteName::E);
        }
    }

    #[test]
    fn custom_tuning_uses_custom_category() {
        let c = Tuning::custom(
            "Test",
            vec![Pitch::natural(NoteName::E, 2)],
            Instrument::Other,
        );
        assert_eq!(c.category, TuningCategory::Custom);
    }

    #[test]
    fn display_format() {
        let t = standard_guitar();
        assert_eq!(t.to_string(), "Standard (Guitar)");
    }

    // === Generator tests ===

    fn pitch_midis(t: &Tuning) -> Vec<i32> {
        t.strings.iter().map(|p| p.midi()).collect()
    }

    #[test]
    fn from_pattern_reproduces_standard_guitar_pitches() {
        // Standard guitar interval pattern: P4 P4 P4 M3 P4
        let pattern = [
            Interval::PERFECT_FOURTH,
            Interval::PERFECT_FOURTH,
            Interval::PERFECT_FOURTH,
            Interval::MAJOR_THIRD,
            Interval::PERFECT_FOURTH,
        ];
        let generated = Tuning::from_pattern(
            "Generated Standard",
            Instrument::Guitar,
            Pitch::natural(NoteName::E, 2),
            &pattern,
            Spelling::Sharps,
        );
        assert_eq!(pitch_midis(&generated), pitch_midis(&standard_guitar()));
    }

    #[test]
    fn regular_all_fourths_reproduces_catalog_pitches() {
        let generated = Tuning::regular(
            "All Fourths Generated",
            Instrument::Guitar,
            Pitch::natural(NoteName::E, 2),
            Interval::PERFECT_FOURTH,
            6,
            Spelling::Sharps,
        );
        let from_catalog = Tuning::find_for("All Fourths", Instrument::Guitar).unwrap();
        assert_eq!(pitch_midis(&generated), pitch_midis(&from_catalog));
    }

    #[test]
    fn regular_all_fifths_reproduces_catalog_pitches() {
        let generated = Tuning::regular(
            "All Fifths Generated",
            Instrument::Guitar,
            Pitch::natural(NoteName::C, 2),
            Interval::PERFECT_FIFTH,
            6,
            Spelling::Sharps,
        );
        let from_catalog = Tuning::find_for("All Fifths", Instrument::Guitar).unwrap();
        assert_eq!(pitch_midis(&generated), pitch_midis(&from_catalog));
    }

    #[test]
    fn transposed_minus_two_reproduces_d_standard_pitches() {
        let d_standard_generated = standard_guitar().transposed(-2, Spelling::Sharps);
        let d_standard_catalog = Tuning::find_for("D Standard", Instrument::Guitar).unwrap();
        assert_eq!(
            pitch_midis(&d_standard_generated),
            pitch_midis(&d_standard_catalog)
        );
    }

    #[test]
    fn transposed_minus_one_with_flats_matches_eb_standard_spelling() {
        let eb_generated = standard_guitar().transposed(-1, Spelling::Flats);
        let eb_catalog = Tuning::find_for("Eb Standard", Instrument::Guitar).unwrap();
        // Pitches must match exactly (same MIDI + same spelling under
        // Flats preference).
        assert_eq!(eb_generated.strings, eb_catalog.strings);
    }

    #[test]
    fn with_string_reproduces_drop_d_pitches() {
        let drop_d = standard_guitar().with_string(0, Pitch::natural(NoteName::D, 2));
        let from_catalog = Tuning::find_for("Drop D", Instrument::Guitar).unwrap();
        assert_eq!(pitch_midis(&drop_d), pitch_midis(&from_catalog));
    }

    #[test]
    fn dropped_lowers_string_by_amount() {
        let dropped = standard_guitar().dropped(0, 2, Spelling::Sharps);
        assert_eq!(dropped.strings[0].midi(), 38); // D2
    }

    #[test]
    fn generators_default_to_custom_category() {
        let g = Tuning::regular(
            "Test",
            Instrument::Guitar,
            Pitch::natural(NoteName::E, 2),
            Interval::PERFECT_FOURTH,
            6,
            Spelling::Sharps,
        );
        assert_eq!(g.category, TuningCategory::Custom);
    }

    // === Expanded catalog ===

    #[test]
    fn bass_catalog_includes_extended_range_and_dropped() {
        assert!(Tuning::find_for("Standard 7", Instrument::Bass).is_some());
        assert!(Tuning::find_for("Drop B", Instrument::Bass).is_some());
        assert!(Tuning::find_for("Drop A 5-string", Instrument::Bass).is_some());
        assert!(Tuning::find_for("Piccolo", Instrument::Bass).is_some());
        assert!(Tuning::find_for("Tenor (fifths)", Instrument::Bass).is_some());
    }

    #[test]
    fn banjo_catalog_includes_modal_and_fifths_variants() {
        assert!(Tuning::find_for("G Modal (sawmill)", Instrument::Banjo).is_some());
        assert!(Tuning::find_for("Tenor (CGDA)", Instrument::Banjo).is_some());
        assert!(Tuning::find_for("Irish Tenor (GDAE)", Instrument::Banjo).is_some());
        assert!(Tuning::find_for("Plectrum (CGBD)", Instrument::Banjo).is_some());
    }

    #[test]
    fn ukulele_catalog_includes_d_tuning_and_u_bass() {
        assert!(Tuning::find_for("D Tuning (high-A)", Instrument::Ukulele).is_some());
        assert!(Tuning::find_for("Bass (U-bass)", Instrument::Ukulele).is_some());
    }

    #[test]
    fn piccolo_bass_pitches_are_octave_above_standard_4() {
        let std4 = Tuning::find_for("Standard 4", Instrument::Bass).unwrap();
        let piccolo = Tuning::find_for("Piccolo", Instrument::Bass).unwrap();
        for (s, p) in std4.strings.iter().zip(piccolo.strings.iter()) {
            assert_eq!(p.midi() - s.midi(), 12);
        }
    }

    #[test]
    fn irish_tenor_banjo_is_octave_below_violin() {
        // Violin: G3 D4 A4 E5 (mandolin tuning)
        let irish = Tuning::find_for("Irish Tenor (GDAE)", Instrument::Banjo).unwrap();
        let mandolin = Tuning::find_for("Standard", Instrument::Mandolin).unwrap();
        for (i, m) in irish.strings.iter().zip(mandolin.strings.iter()) {
            assert_eq!(m.midi() - i.midi(), 12);
        }
    }

    // === Bowed family (first-pass instrument variety) ===

    #[test]
    fn bowed_family_are_named_instruments() {
        assert!(Tuning::find_for("Standard", Instrument::Violin).is_some());
        assert!(Tuning::find_for("Standard", Instrument::Viola).is_some());
        assert!(Tuning::find_for("Standard", Instrument::Cello).is_some());
        assert!(Tuning::find_for("Orchestral (fourths)", Instrument::DoubleBass).is_some());
    }

    #[test]
    fn violin_viola_cello_match_the_mandolin_family_pitches() {
        // Same fifths tunings, so the pitches are identical to the
        // mandolin-family entries they were previously only reachable through.
        let violin = Tuning::find_for("Standard", Instrument::Violin).unwrap();
        let mandolin = Tuning::find_for("Standard", Instrument::Mandolin).unwrap();
        assert_eq!(pitch_midis(&violin), pitch_midis(&mandolin));

        let viola = Tuning::find_for("Standard", Instrument::Viola).unwrap();
        let mandola = Tuning::find_for("Mandola (CGDA)", Instrument::Mandolin).unwrap();
        assert_eq!(pitch_midis(&viola), pitch_midis(&mandola));

        let cello = Tuning::find_for("Standard", Instrument::Cello).unwrap();
        let mandocello = Tuning::find_for("Mandocello (CGDA)", Instrument::Mandolin).unwrap();
        assert_eq!(pitch_midis(&cello), pitch_midis(&mandocello));
    }

    #[test]
    fn bowed_quartet_standard_tunings_are_in_fifths() {
        for inst in [Instrument::Violin, Instrument::Viola, Instrument::Cello] {
            let t = Tuning::find_for("Standard", inst).unwrap();
            for w in pitch_midis(&t).windows(2) {
                assert_eq!(w[1] - w[0], 7, "{inst} strings should be a perfect fifth apart");
            }
        }
    }

    #[test]
    fn cello_is_an_octave_below_viola() {
        let viola = Tuning::find_for("Standard", Instrument::Viola).unwrap();
        let cello = Tuning::find_for("Standard", Instrument::Cello).unwrap();
        for (c, v) in cello.strings.iter().zip(viola.strings.iter()) {
            assert_eq!(v.midi() - c.midi(), 12);
        }
    }

    #[test]
    fn double_bass_orchestral_is_in_fourths() {
        let db = Tuning::find_for("Orchestral (fourths)", Instrument::DoubleBass).unwrap();
        for w in pitch_midis(&db).windows(2) {
            assert_eq!(w[1] - w[0], 5, "double bass orchestral strings should be a perfect fourth apart");
        }
    }

    #[test]
    fn catalog_for_covers_the_bowed_family() {
        assert!(catalog_for(Instrument::Violin).count() >= 1);
        assert!(catalog_for(Instrument::Viola).count() >= 1);
        assert!(catalog_for(Instrument::Cello).count() >= 1);
        assert!(catalog_for(Instrument::DoubleBass).count() >= 1);
    }

    #[test]
    fn find_disambiguates_bass_from_double_bass() {
        // "Orchestral (fourths)" double bass and a bass guitar's "Standard 4"
        // share pitches but are different instruments.
        let db = Tuning::find_for("Orchestral (fourths)", Instrument::DoubleBass).unwrap();
        let bass = Tuning::find_for("Standard 4", Instrument::Bass).unwrap();
        assert_eq!(pitch_midis(&db), pitch_midis(&bass));
        assert_ne!(db.instrument, bass.instrument);
    }

    // === World / folk (second-pass instrument variety) ===

    #[test]
    fn world_folk_are_named_instruments() {
        assert!(Tuning::find_for("Irish (GDAD)", Instrument::Bouzouki).is_some());
        assert!(Tuning::find_for("Standard (GCEAE)", Instrument::Charango).is_some());
        assert!(Tuning::find_for("Brazilian (DGBD)", Instrument::Cavaquinho).is_some());
        assert!(Tuning::find_for("Prima (EEA)", Instrument::Balalaika).is_some());
        assert!(Tuning::find_for("D-A-D", Instrument::MountainDulcimer).is_some());
    }

    #[test]
    fn irish_bouzouki_gdae_shares_octave_mandolin_pitches() {
        // GDAE Irish bouzouki is tuned one octave below the mandolin, which is
        // exactly the octave mandolin already in the catalog.
        let bouzouki = Tuning::find_for("Irish (GDAE)", Instrument::Bouzouki).unwrap();
        let octave_mando = Tuning::find_for("Octave Mandolin", Instrument::Mandolin).unwrap();
        assert_eq!(pitch_midis(&bouzouki), pitch_midis(&octave_mando));
    }

    #[test]
    fn balalaika_prima_has_two_unison_e_strings() {
        let t = Tuning::find_for("Prima (EEA)", Instrument::Balalaika).unwrap();
        assert_eq!(t.string_count(), 3);
        assert_eq!(t.strings[0], t.strings[1]);
        assert_eq!(t.strings[0], Pitch::natural(NoteName::E, 4));
    }

    #[test]
    fn charango_standard_is_re_entrant() {
        // The centre course sits above the one after it (GCEAE): not monotonic.
        let t = Tuning::find_for("Standard (GCEAE)", Instrument::Charango).unwrap();
        assert_eq!(t.string_count(), 5);
        let midis = pitch_midis(&t);
        assert!(
            midis.windows(2).any(|w| w[1] < w[0]),
            "charango is re-entrant, some string dips below its predecessor"
        );
    }

    #[test]
    fn cavaquinho_brazilian_is_dgbd() {
        let t = Tuning::find_for("Brazilian (DGBD)", Instrument::Cavaquinho).unwrap();
        let names: Vec<NoteName> = t.strings.iter().map(|p| p.name).collect();
        assert_eq!(
            names,
            vec![NoteName::D, NoteName::G, NoteName::B, NoteName::D]
        );
    }

    #[test]
    fn catalog_for_covers_world_folk() {
        assert!(catalog_for(Instrument::Bouzouki).count() >= 1);
        assert!(catalog_for(Instrument::Charango).count() >= 1);
        assert!(catalog_for(Instrument::Cavaquinho).count() >= 1);
        assert!(catalog_for(Instrument::Balalaika).count() >= 1);
        assert!(catalog_for(Instrument::MountainDulcimer).count() >= 1);
    }
}
