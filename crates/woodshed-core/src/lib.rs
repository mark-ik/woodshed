//! Woodshed's portable state/logic core.
//!
//! Pure state over the `woodshedding` theory crate — no UI, no host, no
//! audio dependency, so the same core feeds the desktop host, the web host,
//! and tests. This is the serval-host plan's W1.1 split.
//!
//! S2 ships the Stage slice ([`StageState`]) with the lens model: tuning +
//! root + active lens, with the Scales and Chords lenses resolving to
//! fretboard dots. Arpeggios / Progressions / Exercises are placeholders
//! until their engines migrate from woodshed-xilem's `AppState` (S4).

pub mod arpeggio;
pub mod audio;
pub mod search;
pub mod song;
pub mod storage;

use arpeggio::{generate_shapes, ArpeggioDirection, ArpeggioRun};
use woodshedding::chord::{catalog as chord_catalog, ChordFormula};
use woodshedding::exercise::{
    catalog as exercise_catalog, Exercise, ExerciseParams,
};
use woodshedding::fretboard::{Fretboard, Position};
use woodshedding::interval::Interval;
use woodshedding::pitch::{Pitch, Spelling};
use woodshedding::pitch::PitchClass;
use woodshedding::progression::{
    catalog as progression_catalog, ChordRole, Progression, RoleQuality,
};
use woodshedding::practice::{PracticeItem, PracticeSet};
use woodshedding::rehearsal::{
    Card, FretWindow, LoopMode, Material, Recipe, Set, Setting, Timing, Touch,
};
use woodshedding::scale::{catalog as scale_catalog, ScaleFormula};
use woodshedding::tuning::{catalog as tuning_catalog, Tuning, TuningSpec};

/// The fretboard lens strip (redesign-plan vocabulary).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum Lens {
    #[default]
    Scales,
    Chords,
    Arpeggios,
    Progressions,
    Exercises,
}

impl Lens {
    pub const ALL: [Lens; 5] = [
        Lens::Scales,
        Lens::Chords,
        Lens::Arpeggios,
        Lens::Progressions,
        Lens::Exercises,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Lens::Scales => "Scale",
            Lens::Chords => "Chord",
            Lens::Arpeggios => "Arpeggio",
            Lens::Progressions => "Progression",
            Lens::Exercises => "Exercise",
        }
    }

    /// True when the lens resolves dots on the board today. All five do
    /// as of S4 slice 3; kept for the S4 tab placeholders' sake.
    pub fn implemented(self) -> bool {
        true
    }
}

/// Root pitch-class names, indexed by semitones above A.
pub const ROOT_NAMES: [&str; 12] = [
    "A", "A#", "B", "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#",
];

/// Midi of root index 0 (A3) — the octave is irrelevant to position
/// resolution (pitch-class matching) but midi space needs one.
const ROOT_BASE_MIDI: i32 = 57;

/// One rendered fretboard dot: a position resolved against the current
/// tuning, ready for a view layer to place.
#[derive(Clone, Debug)]
pub struct FretDot {
    pub string_index: usize,
    pub fret: u8,
    /// True when this position is the root (unison from the named root).
    pub is_root: bool,
    /// Note name without octave ("A", "C#") — the dot label.
    pub label: String,
}

impl FretDot {
    fn from_position(p: Position) -> Self {
        Self {
            string_index: p.string_index,
            fret: p.fret,
            is_root: p.interval_from_root.is_some_and(|i| i.semitones() == 0),
            label: format!("{}{}", p.pitch.name, p.pitch.accidental),
        }
    }
}

/// The Stage state: what the fretboard is showing right now.
pub struct StageState {
    pub lens: Lens,
    /// Index into [`tunings`].
    pub tuning_idx: usize,
    /// Index into [`ROOT_NAMES`] (semitones above A).
    pub root_idx: usize,
    /// Index into the scale catalog (Scales lens).
    pub scale_idx: usize,
    /// Index into the chord catalog (Chords lens; chord-tone view — the
    /// voicing browser migrates in S4).
    pub chord_idx: usize,
    /// Highest fret shown (inclusive, from the nut at 0).
    pub fret_count: u8,

    // === Arpeggio lens (S4 slice 1) ===
    /// Index into the *chord* catalog — the arpeggio's quality.
    pub arpeggio_idx: usize,
    /// Which generated position shape is active.
    pub arpeggio_position_idx: usize,
    /// Transport cursor through the walk.
    pub arpeggio_step_idx: usize,
    /// True while the step-through transport is auto-advancing.
    pub arpeggio_playing: bool,
    pub arpeggio_direction: ArpeggioDirection,
    /// Inversion: which chord tone the run starts on (0 = root),
    /// clamped to the tone count.
    pub arpeggio_inversion: u8,

    // === Progression lens (S4 slice 2) ===
    /// Index into the progression catalog; `None` = nothing selected
    /// (cold-start state, prompts the user to pick from the list).
    pub progression_idx: Option<usize>,
    /// Which chord card is expanded onto the board.
    pub progression_expanded: usize,

    // === Exercise lens (S4 slice 3) ===
    pub exercise_idx: usize,
    /// Lowest fret of the four-fret hand position.
    pub exercise_starting_fret: u8,
    /// Transport cursor through the step sequence.
    pub exercise_step_idx: usize,
    /// True while auto-advancing.
    pub exercise_playing: bool,
}

/// How many trailing steps the exercise board keeps visible behind the
/// current one (the fading-motion presentation from woodshed-xilem).
pub const EXERCISE_TRAIL: usize = 3;

/// One exercise-board dot: the current step or one of its trail.
#[derive(Clone, Debug)]
pub struct ExerciseDot {
    pub string_index: usize,
    pub fret: u8,
    /// 0 = the current step; 1..=EXERCISE_TRAIL = steps behind it.
    pub recency: usize,
    /// Suggested fingering label ("1"-"4", empty when unspecified).
    pub label: String,
}

/// Everything the view needs to draw the Exercise lens.
#[derive(Clone, Debug)]
pub struct ExerciseBoard {
    pub dots: Vec<ExerciseDot>,
    pub step: usize,
    pub total: usize,
    pub starting_fret: u8,
    pub name: &'static str,
    pub description: &'static str,
}

/// One chord card of a materialized progression.
#[derive(Clone, Debug)]
pub struct ProgressionCard {
    /// Roman-numeral role ("I", "ii", "V7", "♭VII").
    pub numeral: String,
    /// Concrete chord in the current key ("A", "Dm7", "E7").
    pub chord_label: String,
    pub is_expanded: bool,
}

/// Everything the view needs to draw the Progression lens.
#[derive(Clone, Debug)]
pub struct ProgressionBoard {
    pub name: &'static str,
    pub description: &'static str,
    pub cards: Vec<ProgressionCard>,
    /// Chord-tone dots for the expanded card's chord.
    pub dots: Vec<FretDot>,
    /// The expanded chord's label (caption use).
    pub expanded_label: String,
}

/// One arpeggio-board dot: a shape position with the transport highlight.
#[derive(Clone, Debug)]
pub struct ArpDot {
    pub string_index: usize,
    pub fret: u8,
    pub is_root: bool,
    /// Under the transport cursor right now.
    pub is_current: bool,
    pub label: String,
}

/// Everything the view needs to draw the Arpeggio lens.
#[derive(Clone, Debug)]
pub struct ArpeggioBoard {
    pub dots: Vec<ArpDot>,
    pub shape_count: usize,
    pub position_idx: usize,
    pub start_fret: u8,
    pub walk_len: usize,
    pub step: usize,
    pub direction: ArpeggioDirection,
    pub inversion_label: String,
}

/// The tuning catalog (all instruments; instrument filtering arrives with
/// the instrument picker in S4).
pub fn tunings() -> &'static [TuningSpec] {
    tuning_catalog()
}

/// One practice item as a rehearsal card, stamped with its recipe (the
/// P6 "Fill set" conversion). The item's hand position pins the card's
/// fret window.
pub fn card_from_practice_item(item: &PracticeItem, set_name: &str) -> Card {
    let recipe = Some(Recipe::PracticeSet {
        name: set_name.to_string(),
    });
    let window = |position: u8| {
        Some(FretWindow {
            start: position,
            span: 4,
        })
    };
    match item {
        PracticeItem::Scale {
            formula,
            root,
            position,
        } => Card {
            label: item.label(),
            material: Material::Scale {
                name: formula.name.to_string(),
                root: PitchClass::new(root.pitch_class()),
            },
            setting: Setting {
                fret_window: window(*position),
                ..Setting::default()
            },
            touch: Touch::Block,
            timing: Timing::default(),
            from: recipe,
        },
        PracticeItem::Chord {
            formula,
            root,
            position,
        } => Card {
            label: item.label(),
            material: Material::Chord {
                name: formula.name.to_string(),
                root: PitchClass::new(root.pitch_class()),
            },
            setting: Setting {
                fret_window: window(*position),
                ..Setting::default()
            },
            touch: Touch::Block,
            timing: Timing::default(),
            from: recipe,
        },
        PracticeItem::Exercise {
            exercise,
            starting_fret,
        } => Card {
            label: item.label(),
            material: Material::Riff {
                name: exercise.name.to_string(),
            },
            setting: Setting {
                fret_window: window(*starting_fret),
                ..Setting::default()
            },
            touch: Touch::Block,
            timing: Timing::default(),
            from: recipe,
        },
    }
}

/// Materialize a practice set as a rehearsal [`Set`] (cursor at the top).
pub fn set_from_practice(ps: &PracticeSet) -> Set {
    Set {
        cards: ps
            .items
            .iter()
            .map(|item| card_from_practice_item(item, &ps.name))
            .collect(),
        cursor: 0,
        loop_mode: LoopMode::All,
    }
}

/// How long the rehearsal transport dwells on `card` before advancing.
/// `None` = manual (no auto-advance). The card's own bpm wins over the
/// transport's; `Reps` counts as bars (one rep per bar until per-rep
/// audio lands).
pub fn card_dwell(card: &Card, fallback_bpm: f32) -> Option<std::time::Duration> {
    use woodshedding::rehearsal::Hold;
    let bpm = card.timing.bpm.unwrap_or(fallback_bpm).max(30.0);
    let bar_secs = 4.0 * 60.0 / bpm;
    match card.timing.hold {
        Hold::Manual => None,
        Hold::Bars(n) => Some(std::time::Duration::from_secs_f32(
            bar_secs * n.max(1) as f32,
        )),
        Hold::Seconds(s) => Some(std::time::Duration::from_secs_f32(s.max(0.5))),
        Hold::Reps(r) => Some(std::time::Duration::from_secs_f32(
            bar_secs * r.max(1) as f32,
        )),
    }
}

/// Step a rehearsal set's cursor by `dir`, honoring its loop mode.
/// Returns false when the step hit the end with looping off.
pub fn step_set(set: &mut Set, dir: i32) -> bool {
    if set.cards.is_empty() {
        return false;
    }
    let n = set.cards.len() as i32;
    let next = set.cursor as i32 + dir;
    match set.loop_mode {
        LoopMode::All => {
            set.cursor = next.rem_euclid(n) as usize;
            true
        }
        LoopMode::Off => {
            if next < 0 || next >= n {
                false
            } else {
                set.cursor = next as usize;
                true
            }
        }
    }
}

/// Roman-numeral label for a progression role ("I", "ii", "V7", "♭VII").
/// Ported from woodshed-xilem `format_role`, plus the degree-alteration
/// prefix (the theory crate supplies the symbol; the old app dropped it).
pub fn format_role(role: &ChordRole) -> String {
    let lowercase = matches!(
        role.quality,
        RoleQuality::Minor
            | RoleQuality::Diminished
            | RoleQuality::Minor7
            | RoleQuality::HalfDiminished7
            | RoleQuality::Diminished7
            | RoleQuality::Minor6
            | RoleQuality::MinorMajor7
            | RoleQuality::Minor9
    );
    let numeral = match (role.degree, lowercase) {
        (1, false) => "I",
        (1, true) => "i",
        (2, false) => "II",
        (2, true) => "ii",
        (3, false) => "III",
        (3, true) => "iii",
        (4, false) => "IV",
        (4, true) => "iv",
        (5, false) => "V",
        (5, true) => "v",
        (6, false) => "VI",
        (6, true) => "vi",
        (7, false) => "VII",
        (7, true) => "vii",
        _ => "?",
    };
    let suffix = match role.quality {
        RoleQuality::Major | RoleQuality::Minor => "",
        RoleQuality::Diminished => "°",
        RoleQuality::Augmented => "+",
        RoleQuality::Dominant7 => "7",
        RoleQuality::Major7 => "M7",
        RoleQuality::Minor7 => "m7",
        RoleQuality::HalfDiminished7 => "ø7",
        RoleQuality::Diminished7 => "°7",
        RoleQuality::Sus2 => "sus2",
        RoleQuality::Sus4 => "sus4",
        RoleQuality::Major6 => "6",
        RoleQuality::Minor6 => "m6",
        RoleQuality::MinorMajor7 => "mM7",
        RoleQuality::Major9 => "M9",
        RoleQuality::Minor9 => "m9",
        RoleQuality::Dominant9 => "9",
    };
    format!("{}{numeral}{suffix}", role.alteration.symbol())
}

impl Default for StageState {
    fn default() -> Self {
        Self::new()
    }
}

impl StageState {
    pub fn new() -> Self {
        // Default: the catalog's first tuning (standard 6-string guitar),
        // A root, minor pentatonic — the guitarist's hello-world.
        let scale_idx = scale_catalog()
            .iter()
            .position(|s| s.name == "Minor Pentatonic")
            .unwrap_or(0);
        Self {
            lens: Lens::Scales,
            tuning_idx: 0,
            root_idx: 0,
            scale_idx,
            chord_idx: 0,
            fret_count: 12,
            arpeggio_idx: 0,
            arpeggio_position_idx: 0,
            arpeggio_step_idx: 0,
            arpeggio_playing: false,
            arpeggio_direction: ArpeggioDirection::default(),
            arpeggio_inversion: 0,
            progression_idx: None,
            progression_expanded: 0,
            exercise_idx: 0,
            exercise_starting_fret: 1,
            exercise_step_idx: 0,
            exercise_playing: false,
        }
    }

    pub fn tuning(&self) -> Tuning {
        let specs = tunings();
        Tuning::from_spec(&specs[self.tuning_idx.min(specs.len() - 1)])
    }

    pub fn root(&self) -> Pitch {
        Pitch::from_midi(ROOT_BASE_MIDI + self.root_idx as i32, Spelling::Sharps)
    }

    pub fn root_name(&self) -> &'static str {
        ROOT_NAMES[self.root_idx.min(ROOT_NAMES.len() - 1)]
    }

    pub fn scales(&self) -> &'static [ScaleFormula] {
        scale_catalog()
    }

    pub fn chords(&self) -> &'static [ChordFormula] {
        chord_catalog()
    }

    pub fn scale(&self) -> &'static ScaleFormula {
        &scale_catalog()[self.scale_idx.min(scale_catalog().len() - 1)]
    }

    pub fn chord(&self) -> &'static ChordFormula {
        &chord_catalog()[self.chord_idx.min(chord_catalog().len() - 1)]
    }

    pub fn set_lens(&mut self, lens: Lens) {
        self.lens = lens;
    }

    pub fn select_scale(&mut self, idx: usize) {
        if idx < scale_catalog().len() {
            self.scale_idx = idx;
        }
    }

    pub fn select_chord(&mut self, idx: usize) {
        if idx < chord_catalog().len() {
            self.chord_idx = idx;
        }
    }

    pub fn set_tuning(&mut self, idx: usize) {
        if idx < tunings().len() {
            self.tuning_idx = idx;
        }
    }

    pub fn set_root(&mut self, idx: usize) {
        if idx < ROOT_NAMES.len() {
            self.root_idx = idx;
        }
    }

    pub fn string_count(&self) -> usize {
        self.tuning().string_count()
    }

    /// The active lens's material name for captions ("A Minor Pentatonic",
    /// "A m7").
    pub fn material_name(&self) -> String {
        match self.lens {
            Lens::Scales => format!("{} {}", self.root_name(), self.scale().name),
            Lens::Chords => {
                let c = self.chord();
                if c.symbol.is_empty() {
                    format!("{} {}", self.root_name(), c.name)
                } else {
                    format!("{}{} ({})", self.root_name(), c.symbol, c.name)
                }
            }
            Lens::Arpeggios => {
                let c = self.arpeggio_chord();
                if c.symbol.is_empty() {
                    format!("{} {} arpeggio", self.root_name(), c.name)
                } else {
                    format!("{}{} arpeggio", self.root_name(), c.symbol)
                }
            }
            Lens::Progressions => match self.progression_idx {
                Some(i) => format!(
                    "{} in {}",
                    progression_catalog()[i.min(progression_catalog().len() - 1)].name,
                    self.root_name(),
                ),
                None => "Progression".to_string(),
            },
            Lens::Exercises => self.exercise().name.to_string(),
        }
    }

    // === Exercise lens ===

    pub fn exercises(&self) -> &'static [Exercise] {
        exercise_catalog()
    }

    pub fn exercise(&self) -> &'static Exercise {
        &exercise_catalog()[self.exercise_idx.min(exercise_catalog().len() - 1)]
    }

    pub fn select_exercise(&mut self, idx: usize) {
        if idx < exercise_catalog().len() {
            self.exercise_idx = idx;
            self.exercise_step_idx = 0;
        }
    }

    pub fn exercise_advance(&mut self) {
        self.exercise_step_idx = self.exercise_step_idx.wrapping_add(1);
    }

    /// Shift the four-fret hand position, clamped to the board.
    pub fn exercise_nudge_fret(&mut self, delta: i32) {
        let max = self.fret_count.saturating_sub(3);
        let next = (self.exercise_starting_fret as i32 + delta).clamp(0, max as i32);
        if next as u8 != self.exercise_starting_fret {
            self.exercise_starting_fret = next as u8;
            self.exercise_step_idx = 0;
        }
    }

    /// Resolve the Exercise lens: the current step plus its fading trail
    /// (the sequence-aware presentation — motion order, not a static
    /// rectangle of positions).
    pub fn exercise_board(&self) -> ExerciseBoard {
        let ex = self.exercise();
        let params = ExerciseParams {
            starting_fret: self.exercise_starting_fret,
            ..ExerciseParams::default()
        };
        let steps = ex.generate(&self.tuning(), &params);
        let total = steps.len().max(1);
        let step = self.exercise_step_idx % total;
        let mut dots = Vec::new();
        for recency in 0..=EXERCISE_TRAIL.min(step) {
            let s = steps[step - recency];
            // A trail entry under a newer dot on the same position would
            // repaint over it; keep the newest only.
            if dots
                .iter()
                .any(|d: &ExerciseDot| d.string_index == s.string_index && d.fret == s.fret)
            {
                continue;
            }
            dots.push(ExerciseDot {
                string_index: s.string_index,
                fret: s.fret,
                recency,
                label: if s.finger == 0 {
                    String::new()
                } else {
                    s.finger.to_string()
                },
            });
        }
        ExerciseBoard {
            dots,
            step,
            total,
            starting_fret: self.exercise_starting_fret,
            name: ex.name,
            description: ex.description,
        }
    }

    // === Rehearsal (R1 material portability) ===

    /// The current lens's material as a rehearsal [`Card`] — the "+
    /// Rehearse" action (redesign R1: rehearse from any lens). Carries the
    /// tuning name and, for arpeggios, the touch; progression cards stamp
    /// their recipe provenance.
    pub fn card_from_lens(&self) -> Option<Card> {
        let root_pc = PitchClass::new(9 + self.root_idx as u8); // A = pc 9
        let setting = Setting {
            instrument: String::new(),
            tuning: Some(self.tuning().name.clone()),
            ..Setting::default()
        };
        let card = match self.lens {
            Lens::Scales => Card {
                label: format!("{} {}", self.root_name(), self.scale().name),
                material: Material::Scale {
                    name: self.scale().name.to_string(),
                    root: root_pc,
                },
                setting,
                touch: Touch::Block,
                timing: Timing::default(),
                from: None,
            },
            Lens::Chords => Card {
                label: self.material_name(),
                material: Material::Chord {
                    name: self.chord().name.to_string(),
                    root: root_pc,
                },
                setting,
                touch: Touch::Block,
                timing: Timing::default(),
                from: None,
            },
            Lens::Arpeggios => Card {
                label: self.material_name(),
                material: Material::Chord {
                    name: self.arpeggio_chord().name.to_string(),
                    root: root_pc,
                },
                setting,
                touch: Touch::Arpeggiate {
                    direction: self.arpeggio_direction,
                    inversion: self.arpeggio_inversion,
                },
                timing: Timing::default(),
                from: None,
            },
            Lens::Progressions => {
                let board = self.progression_board()?;
                let prog = progression_catalog().get(self.progression_idx?)?;
                let major = scale_catalog().iter().find(|s| s.name == "Major")?;
                let chords = prog.apply_in_key(self.root(), major).ok()?;
                let expanded = self.progression_expanded.min(chords.len() - 1);
                let chord = &chords[expanded];
                Card {
                    label: board.expanded_label.clone(),
                    material: Material::Chord {
                        name: chord.formula.name.to_string(),
                        root: PitchClass::new(chord.root.pitch_class()),
                    },
                    setting,
                    touch: Touch::Block,
                    timing: Timing::default(),
                    from: Some(Recipe::Progression {
                        name: prog.name.to_string(),
                        key: root_pc,
                    }),
                }
            }
            Lens::Exercises => Card {
                label: self.exercise().name.to_string(),
                material: Material::Riff {
                    name: self.exercise().name.to_string(),
                },
                setting,
                touch: Touch::Block,
                timing: Timing::default(),
                from: Some(Recipe::Exercise {
                    name: self.exercise().name.to_string(),
                }),
            },
        };
        Some(card)
    }

    /// Resolve a card's material to fretboard dots against the current
    /// tuning. (Setting fidelity — capo, pinned windows, per-card tuning —
    /// arrives with the card editor; tracked in the plan.)
    pub fn dots_for_card(&self, card: &Card) -> Vec<FretDot> {
        let board = Fretboard::new(self.tuning(), self.fret_count);
        let root_of = |pc: &PitchClass| Pitch::from_midi(48 + pc.value() as i32, Spelling::Sharps);
        let positions = match &card.material {
            Material::Scale { name, root } => scale_catalog()
                .iter()
                .find(|s| s.name == name.as_str())
                .and_then(|s| board.positions_for_scale(s, root_of(root)).ok()),
            Material::Chord { name, root } => chord_catalog()
                .iter()
                .find(|c| c.name == name.as_str())
                .and_then(|c| board.positions_for_chord(c, root_of(root)).ok()),
            Material::Riff { name } => {
                // A riff card references an exercise; show its full
                // position set (the step-through runs on the Exercise lens).
                return exercise_catalog()
                    .iter()
                    .find(|e| e.name == name.as_str())
                    .map(|e| {
                        e.generate(&self.tuning(), &ExerciseParams::default())
                            .into_iter()
                            .map(|s| FretDot {
                                string_index: s.string_index,
                                fret: s.fret,
                                is_root: false,
                                label: if s.finger == 0 {
                                    String::new()
                                } else {
                                    s.finger.to_string()
                                },
                            })
                            .collect()
                    })
                    .unwrap_or_default();
            }
        };
        // Setting fidelity: a pinned fret window filters the positions to
        // the hand position (capo + per-card tuning still deferred).
        let window = card.setting.fret_window;
        let in_window = |fret: u8| {
            window.is_none_or(|w| fret >= w.start && fret <= w.start.saturating_add(w.span))
        };
        positions
            .map(|ps| {
                ps.into_iter()
                    .filter(|p| in_window(p.fret))
                    .map(|p| FretDot {
                        string_index: p.string_index,
                        fret: p.fret,
                        is_root: p
                            .interval_from_root
                            .is_some_and(|iv| iv.semitones() == 0),
                        label: format!("{}{}", p.pitch.name, p.pitch.accidental),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    // === Progression lens ===

    pub fn progressions(&self) -> &'static [Progression] {
        progression_catalog()
    }

    pub fn select_progression(&mut self, idx: usize) {
        if idx < progression_catalog().len() {
            self.progression_idx = Some(idx);
            self.progression_expanded = 0;
        }
    }

    pub fn progression_expand(&mut self, idx: usize) {
        self.progression_expanded = idx;
    }

    /// Materialize the selected progression in the current key (major
    /// scale of the shared root, matching woodshed-xilem) and resolve the
    /// expanded chord's tones. `None` until a progression is picked.
    pub fn progression_board(&self) -> Option<ProgressionBoard> {
        let idx = self.progression_idx?;
        let prog = progression_catalog().get(idx)?;
        let major = scale_catalog().iter().find(|s| s.name == "Major")?;
        let chords = prog.apply_in_key(self.root(), major).ok()?;
        if chords.is_empty() {
            return None;
        }
        let expanded = self.progression_expanded.min(chords.len() - 1);
        let cards: Vec<ProgressionCard> = chords
            .iter()
            .enumerate()
            .map(|(i, c)| ProgressionCard {
                numeral: format_role(&c.role),
                chord_label: format!(
                    "{}{}{}",
                    c.root.name, c.root.accidental, c.formula.symbol
                ),
                is_expanded: i == expanded,
            })
            .collect();
        let chord = &chords[expanded];
        let board = Fretboard::new(self.tuning(), self.fret_count);
        let dots = board
            .positions_for_chord(chord.formula, chord.root)
            .map(|ps| {
                ps.into_iter()
                    .map(|p| FretDot {
                        string_index: p.string_index,
                        fret: p.fret,
                        is_root: p
                            .interval_from_root
                            .is_some_and(|iv| iv.semitones() == 0),
                        label: format!("{}{}", p.pitch.name, p.pitch.accidental),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let expanded_label = format!(
            "{} ({})",
            cards[expanded].chord_label, cards[expanded].numeral
        );
        Some(ProgressionBoard {
            name: prog.name,
            description: prog.description,
            cards,
            dots,
            expanded_label,
        })
    }

    // === Arpeggio lens ===

    pub fn arpeggio_chord(&self) -> &'static ChordFormula {
        &chord_catalog()[self.arpeggio_idx.min(chord_catalog().len() - 1)]
    }

    pub fn select_arpeggio(&mut self, idx: usize) {
        if idx < chord_catalog().len() {
            self.arpeggio_idx = idx;
            self.arpeggio_position_idx = 0;
            self.arpeggio_step_idx = 0;
        }
    }

    pub fn arpeggio_select_position(&mut self, idx: usize) {
        self.arpeggio_position_idx = idx;
        self.arpeggio_step_idx = 0;
    }

    pub fn arpeggio_cycle_direction(&mut self) {
        self.arpeggio_direction = self.arpeggio_direction.next();
        self.arpeggio_step_idx = 0;
    }

    pub fn arpeggio_cycle_inversion(&mut self) {
        let tones = self.arpeggio_chord().intervals.len().max(1) as u8;
        self.arpeggio_inversion = (self.arpeggio_inversion + 1) % tones;
        self.arpeggio_position_idx = 0;
        self.arpeggio_step_idx = 0;
    }

    pub fn arpeggio_advance(&mut self) {
        self.arpeggio_step_idx = self.arpeggio_step_idx.wrapping_add(1);
    }

    /// The inversion's bass tone (the run's starting chord tone).
    fn arpeggio_bass(&self) -> Interval {
        let formula = self.arpeggio_chord();
        let inv = (self.arpeggio_inversion as usize)
            .min(formula.intervals.len().saturating_sub(1));
        formula
            .intervals
            .get(inv)
            .copied()
            .unwrap_or(Interval::PERFECT_UNISON)
    }

    /// Resolve the Arpeggio lens: the active shape's dots with the
    /// transport highlight, plus everything the deck controls display.
    pub fn arpeggio_board(&self) -> ArpeggioBoard {
        let formula = self.arpeggio_chord();
        let bass = self.arpeggio_bass();
        let board = Fretboard::new(self.tuning(), self.fret_count);
        let shapes = generate_shapes(&board, formula, self.root(), bass);
        let shape_count = shapes.len();
        let position_idx = self.arpeggio_position_idx.min(shape_count.saturating_sub(1));
        let shape = &shapes[position_idx];
        let run = ArpeggioRun::new(&shape.positions, bass, self.arpeggio_direction);
        let current = run.position_at(self.arpeggio_step_idx);
        let dots = shape
            .positions
            .iter()
            .enumerate()
            .map(|(i, p)| ArpDot {
                string_index: p.string_index,
                fret: p.fret,
                is_root: p.interval_from_root.is_some_and(|iv| iv.semitones() == 0),
                is_current: current == Some(i),
                label: format!("{}{}", p.pitch.name, p.pitch.accidental),
            })
            .collect();
        let inv = (self.arpeggio_inversion as usize)
            .min(formula.intervals.len().saturating_sub(1));
        let inversion_label = match inv {
            0 => "Inv: Root".to_string(),
            1 => "Inv: 1st".to_string(),
            2 => "Inv: 2nd".to_string(),
            3 => "Inv: 3rd".to_string(),
            k => format!("Inv: {k}th"),
        };
        ArpeggioBoard {
            dots,
            shape_count,
            position_idx,
            start_fret: shape.start_fret,
            walk_len: run.walk_len(),
            step: self.arpeggio_step_idx % run.walk_len(),
            direction: self.arpeggio_direction,
            inversion_label,
        }
    }

    /// Resolve the active lens to fretboard dots. Empty for the
    /// not-yet-migrated lenses (and on a theoretically impossible
    /// transposition failure, rather than panicking in a render path).
    pub fn dots(&self) -> Vec<FretDot> {
        let board = Fretboard::new(self.tuning(), self.fret_count);
        let positions = match self.lens {
            Lens::Scales => board.positions_for_scale(self.scale(), self.root()),
            Lens::Chords => board.positions_for_chord(self.chord(), self.root()),
            _ => return Vec::new(),
        };
        positions
            .map(|ps| ps.into_iter().map(FretDot::from_position).collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state_resolves_dots() {
        let s = StageState::new();
        let dots = s.dots();
        assert!(!dots.is_empty(), "minor pentatonic on standard tuning");
        assert!(dots.iter().any(|d| d.is_root), "root positions present");
        assert!(dots.iter().all(|d| d.fret <= s.fret_count));
        assert!(dots.iter().all(|d| d.string_index < s.string_count()));
    }

    #[test]
    fn select_scale_changes_dots() {
        let mut s = StageState::new();
        let before = s.dots().len();
        let major = s
            .scales()
            .iter()
            .position(|f| f.name == "Major")
            .expect("Major in catalog");
        s.select_scale(major);
        assert_eq!(s.scale().name, "Major");
        assert_ne!(s.dots().len(), before, "major has more tones than pentatonic");
    }

    #[test]
    fn chord_lens_resolves_chord_tones() {
        let mut s = StageState::new();
        s.set_lens(Lens::Chords);
        let dots = s.dots();
        assert!(!dots.is_empty(), "chord tones on standard tuning");
        assert!(dots.iter().any(|d| d.is_root));
    }

    #[test]
    fn root_change_transposes() {
        let mut s = StageState::new();
        let a_dots: Vec<_> = s.dots().iter().map(|d| (d.string_index, d.fret)).collect();
        s.set_root(3); // C
        let c_dots: Vec<_> = s.dots().iter().map(|d| (d.string_index, d.fret)).collect();
        assert_ne!(a_dots, c_dots, "different root, different positions");
        assert_eq!(s.root_name(), "C");
    }

    #[test]
    fn fret_window_filters_card_dots() {
        let mut s = StageState::new();
        s.set_lens(Lens::Scales);
        let mut card = s.card_from_lens().unwrap();
        let all = s.dots_for_card(&card).len();
        card.setting.fret_window = Some(woodshedding::rehearsal::FretWindow {
            start: 5,
            span: 4,
        });
        let windowed = s.dots_for_card(&card);
        assert!(!windowed.is_empty());
        assert!(windowed.len() < all, "window narrows the position set");
        assert!(windowed.iter().all(|d| d.fret >= 5 && d.fret <= 9));
    }

    #[test]
    fn card_dwell_follows_hold() {
        use woodshedding::rehearsal::Hold;
        let s = StageState::new();
        let mut card = s.card_from_lens().unwrap();
        assert!(card_dwell(&card, 120.0).is_none(), "manual by default");
        card.timing.hold = Hold::Bars(2);
        let d = card_dwell(&card, 120.0).unwrap();
        assert!((d.as_secs_f32() - 4.0).abs() < 0.01, "2 bars at 120 = 4s");
        card.timing.bpm = Some(60.0);
        let d = card_dwell(&card, 120.0).unwrap();
        assert!((d.as_secs_f32() - 8.0).abs() < 0.01, "card bpm wins");
    }

    #[test]
    fn practice_sets_fill_rehearsal_sets() {
        let catalog = woodshedding::practice::catalog();
        assert!(!catalog.is_empty());
        let ps = &catalog[0];
        let set = set_from_practice(ps);
        assert_eq!(set.cards.len(), ps.items.len());
        assert!(set.cards.iter().all(|c| matches!(
            c.from,
            Some(Recipe::PracticeSet { .. })
        )));
        assert!(set.cards.iter().all(|c| c.setting.fret_window.is_some()));
        // Every card resolves on the board.
        let s = StageState::new();
        assert!(set.cards.iter().all(|c| !s.dots_for_card(c).is_empty()));
    }

    #[test]
    fn all_lenses_implemented() {
        assert!(Lens::ALL.iter().all(|l| l.implemented()));
    }

    #[test]
    fn exercise_board_steps_with_trail() {
        let mut s = StageState::new();
        s.set_lens(Lens::Exercises);
        let b0 = s.exercise_board();
        assert!(b0.total > 1, "exercise generates a sequence");
        assert_eq!(b0.dots.len(), 1, "cold start shows only the current step");
        for _ in 0..EXERCISE_TRAIL + 2 {
            s.exercise_advance();
        }
        let b = s.exercise_board();
        assert!(b.dots.len() > 1, "trail appears behind the cursor");
        assert!(b.dots.len() <= EXERCISE_TRAIL + 1);
        assert_eq!(b.dots.iter().filter(|d| d.recency == 0).count(), 1);
        // Fret nudge clamps and resets the cursor.
        s.exercise_nudge_fret(100);
        assert_eq!(s.exercise_starting_fret, s.fret_count - 3);
        assert_eq!(s.exercise_step_idx, 0);
        s.exercise_nudge_fret(-100);
        assert_eq!(s.exercise_starting_fret, 0);
    }

    #[test]
    fn progression_board_materializes_in_key() {
        let mut s = StageState::new();
        s.set_lens(Lens::Progressions);
        assert!(s.progression_board().is_none(), "cold start prompts");
        s.select_progression(0);
        let b = s.progression_board().expect("board after selection");
        assert!(!b.cards.is_empty());
        assert_eq!(b.cards.iter().filter(|c| c.is_expanded).count(), 1);
        assert!(!b.dots.is_empty(), "expanded chord resolves tones");
        // Expanding another card moves the expansion and changes the label.
        if b.cards.len() > 1 {
            let first = b.expanded_label.clone();
            s.progression_expand(1);
            let b2 = s.progression_board().unwrap();
            assert!(b2.cards[1].is_expanded);
            assert_ne!(b2.expanded_label, first);
        }
    }

    #[test]
    fn format_role_covers_common_shapes() {
        use woodshedding::progression::{ChordRole, DegreeAlteration, RoleQuality};
        assert_eq!(format_role(&ChordRole::new(1, RoleQuality::Major)), "I");
        assert_eq!(format_role(&ChordRole::new(2, RoleQuality::Minor7)), "iim7");
        assert_eq!(
            format_role(&ChordRole::new(5, RoleQuality::Dominant7)),
            "V7"
        );
        assert_eq!(
            format_role(&ChordRole::altered(
                7,
                DegreeAlteration::Flat,
                RoleQuality::Major
            )),
            "♭VII"
        );
    }

    #[test]
    fn arpeggio_board_resolves_and_steps() {
        let mut s = StageState::new();
        s.set_lens(Lens::Arpeggios);
        let b0 = s.arpeggio_board();
        assert!(!b0.dots.is_empty());
        assert!(b0.walk_len >= 1);
        assert_eq!(b0.dots.iter().filter(|d| d.is_current).count(), 1);
        let cur0: Vec<_> = b0
            .dots
            .iter()
            .map(|d| d.is_current)
            .collect();
        s.arpeggio_advance();
        let b1 = s.arpeggio_board();
        let cur1: Vec<_> = b1.dots.iter().map(|d| d.is_current).collect();
        assert_ne!(cur0, cur1, "advance moves the highlight");
        s.arpeggio_cycle_inversion();
        let b2 = s.arpeggio_board();
        assert_eq!(b2.step, 0, "inversion change resets the transport");
    }
}
