//! Chord progressions: ordered sequences of chord roles in a key.
//!
//! A [`Progression`] is a list of [`ChordRole`]s — each role is a
//! scale degree (1–7), an optional alteration (♯/♭), and a chord
//! quality (major, minor, dom7, etc.). Apply a progression to a key
//! (root pitch + scale formula) to materialize the actual chords.
//!
//! # Example
//!
//! ```
//! use woodshedding::pitch::{NoteName, Pitch};
//! use woodshedding::scale::catalog as scale_catalog;
//! use woodshedding::progression::catalog;
//!
//! let major_scale = scale_catalog().iter().find(|s| s.name == "Major").unwrap();
//! let prog = catalog().iter().find(|p| p.name == "ii-V-I (Jazz)").unwrap();
//! let chords = prog.apply_in_key(Pitch::natural(NoteName::C, 4), major_scale).unwrap();
//! assert_eq!(chords.len(), 3);
//! // Dm7, G7, Cmaj7
//! ```

use crate::chord::{catalog as chord_catalog, ChordFormula};
use crate::interval::Interval;
use crate::pitch::{Pitch, TranspositionError};
use crate::scale::ScaleFormula;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ProgressionCategory {
    Standard,
    Jazz,
    Blues,
    Pop,
    Symmetric,
    Custom,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum DegreeAlteration {
    Natural,
    Sharp,
    Flat,
}

impl DegreeAlteration {
    pub const ALL: [Self; 3] = [Self::Natural, Self::Sharp, Self::Flat];

    /// Prefix symbol for a degree (empty for natural).
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Natural => "",
            Self::Sharp => "♯",
            Self::Flat => "♭",
        }
    }
}

/// Chord quality for a role in a progression.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum RoleQuality {
    Major,
    Minor,
    Diminished,
    Augmented,
    Sus2,
    Sus4,
    Major6,
    Minor6,
    Major7,
    Minor7,
    Dominant7,
    MinorMajor7,
    HalfDiminished7,
    Diminished7,
    Major9,
    Minor9,
    Dominant9,
}

impl RoleQuality {
    /// All qualities, in editor-cycle order.
    pub const ALL: [Self; 17] = [
        Self::Major,
        Self::Minor,
        Self::Diminished,
        Self::Augmented,
        Self::Sus2,
        Self::Sus4,
        Self::Major6,
        Self::Minor6,
        Self::Major7,
        Self::Minor7,
        Self::Dominant7,
        Self::MinorMajor7,
        Self::HalfDiminished7,
        Self::Diminished7,
        Self::Major9,
        Self::Minor9,
        Self::Dominant9,
    ];

    /// Name of the matching [`ChordFormula`] in the chord catalog.
    pub fn chord_formula_name(self) -> &'static str {
        match self {
            Self::Major => "Major",
            Self::Minor => "Minor",
            Self::Diminished => "Diminished",
            Self::Augmented => "Augmented",
            Self::Sus2 => "Suspended 2nd",
            Self::Sus4 => "Suspended 4th",
            Self::Major6 => "Major 6",
            Self::Minor6 => "Minor 6",
            Self::Major7 => "Major 7",
            Self::Minor7 => "Minor 7",
            Self::Dominant7 => "Dominant 7",
            Self::MinorMajor7 => "Minor-Major 7",
            Self::HalfDiminished7 => "Half-Diminished 7",
            Self::Diminished7 => "Diminished 7",
            Self::Major9 => "Major 9",
            Self::Minor9 => "Minor 9",
            Self::Dominant9 => "Dominant 9",
        }
    }

    /// Look up the matching [`ChordFormula`] in the catalog.
    pub fn chord_formula(self) -> Option<&'static ChordFormula> {
        let name = self.chord_formula_name();
        chord_catalog().iter().find(|f| f.name == name)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ChordRole {
    /// 1-indexed scale degree (1 = tonic, 2 = supertonic, etc.).
    pub degree: u8,
    /// Chromatic alteration of the degree (♯, ♭, or natural).
    pub alteration: DegreeAlteration,
    pub quality: RoleQuality,
}

impl ChordRole {
    pub const fn new(degree: u8, quality: RoleQuality) -> Self {
        Self {
            degree,
            alteration: DegreeAlteration::Natural,
            quality,
        }
    }

    pub const fn altered(
        degree: u8,
        alteration: DegreeAlteration,
        quality: RoleQuality,
    ) -> Self {
        Self {
            degree,
            alteration,
            quality,
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct Progression {
    pub name: &'static str,
    pub description: &'static str,
    pub category: ProgressionCategory,
    pub roles: &'static [ChordRole],
}

#[derive(Clone, Debug)]
pub struct ProgressionChord {
    pub role: ChordRole,
    pub root: Pitch,
    pub formula: &'static ChordFormula,
    /// Materialized chord pitches (root, third, fifth, ...).
    pub pitches: Vec<Pitch>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgressionError {
    DegreeOutOfRange {
        degree: u8,
        scale_size: usize,
    },
    UnknownChordFormula(&'static str),
    Transposition(String),
}

impl core::fmt::Display for ProgressionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::DegreeOutOfRange { degree, scale_size } => write!(
                f,
                "scale degree {degree} out of range for scale of size {scale_size}"
            ),
            Self::UnknownChordFormula(n) => write!(f, "no chord formula named {n}"),
            Self::Transposition(s) => f.write_str(s),
        }
    }
}

impl std::error::Error for ProgressionError {}

impl From<TranspositionError> for ProgressionError {
    fn from(e: TranspositionError) -> Self {
        Self::Transposition(format!("{e}"))
    }
}

impl Progression {
    /// Apply this progression in the given key, producing the actual
    /// chords (each with root pitch and full pitch list).
    pub fn apply_in_key(
        &self,
        key_root: Pitch,
        key_scale: &ScaleFormula,
    ) -> Result<Vec<ProgressionChord>, ProgressionError> {
        apply_roles_in_key(self.roles, key_root, key_scale)
    }
}

/// Materialize an arbitrary (e.g. user-authored) sequence of roles in a
/// key — the same logic [`Progression::apply_in_key`] uses, but over a
/// borrowed slice instead of a `&'static` catalog progression.
pub fn apply_roles_in_key(
    roles: &[ChordRole],
    key_root: Pitch,
    key_scale: &ScaleFormula,
) -> Result<Vec<ProgressionChord>, ProgressionError> {
    let scale_pitches = key_scale.apply_to(key_root)?;
    let mut chords = Vec::with_capacity(roles.len());
    for &role in roles {
        if role.degree == 0 || role.degree as usize > scale_pitches.len() {
            return Err(ProgressionError::DegreeOutOfRange {
                degree: role.degree,
                scale_size: scale_pitches.len(),
            });
        }
        let mut chord_root = scale_pitches[role.degree as usize - 1];
        chord_root = match role.alteration {
            DegreeAlteration::Natural => chord_root,
            DegreeAlteration::Sharp => chord_root.transposed_by(Interval::AUGMENTED_UNISON)?,
            DegreeAlteration::Flat => chord_root.transposed_down_by(Interval::AUGMENTED_UNISON)?,
        };
        let formula = role.quality.chord_formula().ok_or_else(|| {
            ProgressionError::UnknownChordFormula(role.quality.chord_formula_name())
        })?;
        let pitches = formula.apply_to(chord_root)?;
        chords.push(ProgressionChord {
            role,
            root: chord_root,
            formula,
            pitches,
        });
    }
    Ok(chords)
}

pub fn catalog() -> &'static [Progression] {
    CATALOG
}

pub fn catalog_in_category(
    category: ProgressionCategory,
) -> impl Iterator<Item = &'static Progression> {
    catalog().iter().filter(move |p| p.category == category)
}

// === Catalog ===

use ChordRole as R;
use DegreeAlteration::*;
use RoleQuality::*;

static CATALOG: &[Progression] = &[
    // ── Standard / classic ──
    Progression {
        name: "I-IV-V",
        description: "Tonic, subdominant, dominant. The bedrock of \
                      Western popular music.",
        category: ProgressionCategory::Standard,
        roles: &[R::new(1, Major), R::new(4, Major), R::new(5, Major)],
    },
    Progression {
        name: "I-V-vi-IV",
        description: "The four-chord pop progression. Used in countless \
                      hit songs.",
        category: ProgressionCategory::Pop,
        roles: &[
            R::new(1, Major),
            R::new(5, Major),
            R::new(6, Minor),
            R::new(4, Major),
        ],
    },
    Progression {
        name: "vi-IV-I-V",
        description: "Pop punk / sensitive female chord progression.",
        category: ProgressionCategory::Pop,
        roles: &[
            R::new(6, Minor),
            R::new(4, Major),
            R::new(1, Major),
            R::new(5, Major),
        ],
    },
    Progression {
        name: "I-vi-IV-V",
        description: "1950s doo-wop progression.",
        category: ProgressionCategory::Pop,
        roles: &[
            R::new(1, Major),
            R::new(6, Minor),
            R::new(4, Major),
            R::new(5, Major),
        ],
    },
    Progression {
        name: "I-vi-ii-V",
        description: "Circle progression — common in jazz standards \
                      and tin pan alley.",
        category: ProgressionCategory::Standard,
        roles: &[
            R::new(1, Major),
            R::new(6, Minor),
            R::new(2, Minor),
            R::new(5, Major),
        ],
    },
    Progression {
        name: "iii-vi-ii-V",
        description: "Jazz turnaround, often used to lead back to I.",
        category: ProgressionCategory::Jazz,
        roles: &[
            R::new(3, Minor),
            R::new(6, Minor),
            R::new(2, Minor),
            R::new(5, Major),
        ],
    },
    // ── Jazz ──
    Progression {
        name: "ii-V-I (Jazz)",
        description: "The most ubiquitous jazz progression. Half-step \
                      down from ii to V to I, with sevenths.",
        category: ProgressionCategory::Jazz,
        roles: &[
            R::new(2, Minor7),
            R::new(5, Dominant7),
            R::new(1, Major7),
        ],
    },
    Progression {
        name: "iii-VI-ii-V-I (Jazz)",
        description: "Extended ii-V-I with the typical iii-vi prefix \
                      that many jazz standards use.",
        category: ProgressionCategory::Jazz,
        roles: &[
            R::new(3, Minor7),
            R::new(6, Dominant7),
            R::new(2, Minor7),
            R::new(5, Dominant7),
            R::new(1, Major7),
        ],
    },
    // ── Blues ──
    Progression {
        name: "12-Bar Blues",
        description: "I-I-I-I, IV-IV-I-I, V-IV-I-V — the canonical \
                      12-bar form. Each role here is one bar.",
        category: ProgressionCategory::Blues,
        roles: &[
            R::new(1, Dominant7),
            R::new(1, Dominant7),
            R::new(1, Dominant7),
            R::new(1, Dominant7),
            R::new(4, Dominant7),
            R::new(4, Dominant7),
            R::new(1, Dominant7),
            R::new(1, Dominant7),
            R::new(5, Dominant7),
            R::new(4, Dominant7),
            R::new(1, Dominant7),
            R::new(5, Dominant7),
        ],
    },
    Progression {
        name: "Quick Change Blues",
        description: "12-bar blues variant that goes to IV in bar 2.",
        category: ProgressionCategory::Blues,
        roles: &[
            R::new(1, Dominant7),
            R::new(4, Dominant7),
            R::new(1, Dominant7),
            R::new(1, Dominant7),
            R::new(4, Dominant7),
            R::new(4, Dominant7),
            R::new(1, Dominant7),
            R::new(1, Dominant7),
            R::new(5, Dominant7),
            R::new(4, Dominant7),
            R::new(1, Dominant7),
            R::new(5, Dominant7),
        ],
    },
    // ── Symmetric / interesting ──
    Progression {
        name: "Andalusian Cadence",
        description: "i-VII-VI-V — classic minor-key descending bassline \
                      heard in flamenco and rock (Sultans of Swing, etc.).",
        category: ProgressionCategory::Symmetric,
        roles: &[
            R::new(1, Minor),
            R::altered(7, Flat, Major),
            R::altered(6, Flat, Major),
            R::new(5, Major),
        ],
    },
    Progression {
        name: "Canon (Pachelbel)",
        description: "I-V-vi-iii-IV-I-IV-V — the eight-chord cycle \
                      from Pachelbel's Canon, which sneaks into pop too.",
        category: ProgressionCategory::Standard,
        roles: &[
            R::new(1, Major),
            R::new(5, Major),
            R::new(6, Minor),
            R::new(3, Minor),
            R::new(4, Major),
            R::new(1, Major),
            R::new(4, Major),
            R::new(5, Major),
        ],
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pitch::{Accidental, NoteName};
    use crate::scale::catalog as scale_catalog;

    fn major_scale() -> &'static ScaleFormula {
        scale_catalog().iter().find(|s| s.name == "Major").unwrap()
    }

    fn nat(n: NoteName, o: i8) -> Pitch {
        Pitch::natural(n, o)
    }

    #[test]
    fn ii_v_i_in_c_produces_dm7_g7_cmaj7() {
        let prog = catalog().iter().find(|p| p.name == "ii-V-I (Jazz)").unwrap();
        let chords = prog
            .apply_in_key(nat(NoteName::C, 4), major_scale())
            .unwrap();
        assert_eq!(chords.len(), 3);

        // Dm7
        assert_eq!(chords[0].formula.name, "Minor 7");
        assert_eq!(chords[0].root, nat(NoteName::D, 4));

        // G7
        assert_eq!(chords[1].formula.name, "Dominant 7");
        assert_eq!(chords[1].root, nat(NoteName::G, 4));

        // Cmaj7 — degree 1 is the same C as the key root.
        assert_eq!(chords[2].formula.name, "Major 7");
        assert_eq!(chords[2].root, nat(NoteName::C, 4));
    }

    #[test]
    fn ii_v_i_dm7_pitches_are_d_f_a_c() {
        let prog = catalog().iter().find(|p| p.name == "ii-V-I (Jazz)").unwrap();
        let chords = prog
            .apply_in_key(nat(NoteName::C, 4), major_scale())
            .unwrap();
        let dm7 = &chords[0];
        let names: Vec<NoteName> = dm7.pitches.iter().map(|p| p.name).collect();
        assert_eq!(names, vec![NoteName::D, NoteName::F, NoteName::A, NoteName::C]);
    }

    #[test]
    fn i_iv_v_in_c_produces_c_f_g() {
        let prog = catalog().iter().find(|p| p.name == "I-IV-V").unwrap();
        let chords = prog
            .apply_in_key(nat(NoteName::C, 4), major_scale())
            .unwrap();
        let roots: Vec<NoteName> = chords.iter().map(|c| c.root.name).collect();
        assert_eq!(roots, vec![NoteName::C, NoteName::F, NoteName::G]);
        for chord in &chords {
            assert_eq!(chord.formula.name, "Major");
        }
    }

    #[test]
    fn i_iv_v_in_d_produces_d_g_a() {
        let prog = catalog().iter().find(|p| p.name == "I-IV-V").unwrap();
        let chords = prog
            .apply_in_key(nat(NoteName::D, 4), major_scale())
            .unwrap();
        let roots: Vec<NoteName> = chords.iter().map(|c| c.root.name).collect();
        assert_eq!(roots, vec![NoteName::D, NoteName::G, NoteName::A]);
    }

    #[test]
    fn pop_i_v_vi_iv_in_g_produces_g_d_em_c() {
        let prog = catalog().iter().find(|p| p.name == "I-V-vi-IV").unwrap();
        let chords = prog
            .apply_in_key(nat(NoteName::G, 3), major_scale())
            .unwrap();
        assert_eq!(chords[0].root, nat(NoteName::G, 3));
        assert_eq!(chords[0].formula.name, "Major");
        assert_eq!(chords[1].root, nat(NoteName::D, 4));
        assert_eq!(chords[1].formula.name, "Major");
        assert_eq!(chords[2].root, nat(NoteName::E, 4));
        assert_eq!(chords[2].formula.name, "Minor");
        assert_eq!(chords[3].root, nat(NoteName::C, 4));
        assert_eq!(chords[3].formula.name, "Major");
    }

    #[test]
    fn andalusian_uses_flat_alterations() {
        // i-bVII-bVI-V in A minor: Am, G, F, E
        // (flat 7 of A is G, flat 6 of A is F)
        // Note: we apply this in the major-scale frame here. The
        // alterations carry the bVII and bVI semantics.
        let prog = catalog()
            .iter()
            .find(|p| p.name == "Andalusian Cadence")
            .unwrap();
        let chords = prog
            .apply_in_key(nat(NoteName::A, 3), major_scale())
            .unwrap();
        // i = Am
        assert_eq!(chords[0].root.name, NoteName::A);
        assert_eq!(chords[0].formula.name, "Minor");
        // bVII: A major scale's 7th degree is G#; flat → G
        assert_eq!(chords[1].root.name, NoteName::G);
        assert_eq!(chords[1].root.accidental, Accidental::Natural);
        // bVI: 6th of A major is F#; flat → F
        assert_eq!(chords[2].root.name, NoteName::F);
        assert_eq!(chords[2].root.accidental, Accidental::Natural);
        // V: 5th of A major = E
        assert_eq!(chords[3].root.name, NoteName::E);
        assert_eq!(chords[3].formula.name, "Major");
    }

    #[test]
    fn twelve_bar_blues_has_twelve_chords() {
        let prog = catalog().iter().find(|p| p.name == "12-Bar Blues").unwrap();
        let chords = prog
            .apply_in_key(nat(NoteName::E, 3), major_scale())
            .unwrap();
        assert_eq!(chords.len(), 12);
        // All chords should be Dominant 7
        for c in &chords {
            assert_eq!(c.formula.name, "Dominant 7");
        }
    }

    #[test]
    fn role_quality_chord_formula_lookup_works_for_every_variant() {
        // Every RoleQuality should map to a real chord in the catalog.
        use RoleQuality::*;
        for q in [
            Major, Minor, Diminished, Augmented, Sus2, Sus4, Major6, Minor6,
            Major7, Minor7, Dominant7, MinorMajor7, HalfDiminished7,
            Diminished7, Major9, Minor9, Dominant9,
        ] {
            assert!(
                q.chord_formula().is_some(),
                "RoleQuality::{q:?} → name {:?} not found in chord catalog",
                q.chord_formula_name()
            );
        }
    }

    #[test]
    fn catalog_in_category_filters() {
        let jazz_count = catalog_in_category(ProgressionCategory::Jazz).count();
        assert!(jazz_count >= 2);
        let blues_count = catalog_in_category(ProgressionCategory::Blues).count();
        assert!(blues_count >= 2);
    }

    #[test]
    fn out_of_range_degree_returns_error() {
        let major = major_scale();
        // Degree 8 is out of range for a 7-note major scale.
        static FAKE_ROLES: &[ChordRole] = &[ChordRole::new(8, RoleQuality::Major)];
        let fake = Progression {
            name: "fake",
            description: "",
            category: ProgressionCategory::Custom,
            roles: FAKE_ROLES,
        };
        let err = fake.apply_in_key(nat(NoteName::C, 4), major).unwrap_err();
        assert!(matches!(err, ProgressionError::DegreeOutOfRange { .. }));
    }
}
