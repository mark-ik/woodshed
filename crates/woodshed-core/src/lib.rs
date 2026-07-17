//! Woodshed's portable state/logic core.
//!
//! Pure state over the `woodshedding` theory crate — no UI, no host, no
//! audio dependency, so the same core feeds the desktop host, the web host,
//! and tests. This is the genet-host plan's W1.1 split.
//!
//! S2 ships the Stage slice ([`StageState`]) with the lens model: tuning +
//! root + active lens, with the Scales and Chords lenses resolving to
//! fretboard dots. Arpeggios / Progressions / Exercises are placeholders
//! until their engines migrate from woodshed-xilem's `AppState` (S4).

pub mod arpeggio;
pub mod audio;
pub mod history;
pub mod midi;
pub mod search;
pub mod settings;
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
    Card, FretWindow, LoopMode, MarkMode, Material, Recipe, Set, Setting, Timing, Touch,
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

/// A catalog selection surfaced by the related-material projection. It is
/// intentionally small and copyable so views can route it through a click
/// without carrying graph implementation types.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelatedTarget {
    Scale(usize),
    Chord(usize),
    Arpeggio(usize),
    Progression(usize),
    Exercise(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelatedSuggestion {
    pub title: String,
    pub kind: &'static str,
    pub reason: String,
    pub score: u16,
    pub target: RelatedTarget,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NeighborhoodNode {
    pub id: String,
    pub title: String,
    pub kind: &'static str,
    pub score: u16,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NeighborhoodSnapshot {
    pub nodes: Vec<NeighborhoodNode>,
    pub edges: Vec<(u16, u16)>,
}

/// Root pitch-class names, indexed by semitones above A.
pub const ROOT_NAMES: [&str; 12] = [
    "A", "A#", "B", "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#",
];

/// Midi of root index 0 (A3) — the octave is irrelevant to position
/// resolution (pitch-class matching) but midi space needs one.
const ROOT_BASE_MIDI: i32 = 57;

/// Scale-degree label for an interval measured in semitones from the root
/// ("1", "b3", "5"). Empty for anything outside an octave slot.
pub fn degree_label(semitones: i32) -> &'static str {
    match semitones.rem_euclid(12) {
        0 => "1",
        1 => "b2",
        2 => "2",
        3 => "b3",
        4 => "3",
        5 => "4",
        6 => "b5",
        7 => "5",
        8 => "b6",
        9 => "6",
        10 => "b7",
        11 => "7",
        _ => "",
    }
}

/// Interval name from the root ("Root", "Minor 3rd") for a semitone distance.
pub fn interval_name(semitones: i32) -> &'static str {
    match semitones.rem_euclid(12) {
        0 => "Root",
        1 => "Minor 2nd",
        2 => "Major 2nd",
        3 => "Minor 3rd",
        4 => "Major 3rd",
        5 => "Perfect 4th",
        6 => "Tritone",
        7 => "Perfect 5th",
        8 => "Minor 6th",
        9 => "Major 6th",
        10 => "Minor 7th",
        11 => "Major 7th",
        _ => "",
    }
}

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
    /// Pitch octave, for the detail card.
    pub octave: i8,
    /// Scale degree relative to the root ("1", "b3"), empty if unknown.
    pub degree: String,
    /// Interval name from the root ("Root", "Minor 3rd"), empty if unknown.
    pub interval_name: String,
    /// The note's frequency in Hz, for the card's play button (0.0 if unknown).
    pub frequency: f32,
}

impl FretDot {
    fn from_position(p: Position) -> Self {
        let semis = p.interval_from_root.map(|i| i.semitones());
        Self {
            string_index: p.string_index,
            fret: p.fret,
            is_root: semis == Some(0),
            label: format!("{}{}", p.pitch.name, p.pitch.accidental),
            octave: p.pitch.octave,
            degree: semis.map(degree_label).unwrap_or("").to_string(),
            interval_name: semis.map(interval_name).unwrap_or("").to_string(),
            frequency: p.pitch.frequency() as f32,
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
    /// The neck window's last fret. The board shows `fret_start ..= fret_count`;
    /// [`Self::apply_neck`] sets both from the settings and the instrument.
    pub fret_count: u8,
    /// The neck window's first fret. 0 includes the open strings and the nut;
    /// a higher start is a window onto the middle of the neck (8-16, 2-22).
    pub fret_start: u8,

    // === Arpeggio lens (S4 slice 1) ===
    /// Index into the *chord* catalog — the arpeggio's quality.
    pub arpeggio_idx: usize,
    /// Which generated position shape is active.
    pub arpeggio_position_idx: usize,
    /// Transport cursor through the walk.
    pub arpeggio_step_idx: usize,
    /// True while the step-through transport is auto-advancing.
    pub arpeggio_playing: bool,
    /// Scale-run transport (Scale/Chord lens): step through the board's notes at
    /// tempo, sounding and highlighting each. The keystone of touch-as-behavior;
    /// transient (not persisted).
    pub scale_run_playing: bool,
    pub scale_run_step: usize,
    /// The note sounding right now during a run, for the board highlight.
    pub scale_run_active: Option<(usize, u8)>,
    /// Whether to draw the touch's path as a trail over the markers (the touch
    /// editor: show the treatment, not just name it). Transient.
    pub path_shown: bool,
    /// Draw mode: clicking a marker appends it to `authored_path` instead of
    /// pinning its detail card, so the player draws the touch's path by hand.
    /// Transient.
    pub draw_mode: bool,
    /// The hand-drawn path, an ordered visit list of `(string, fret)`. When
    /// non-empty it *is* the run's path (the trail and the stepping run follow
    /// it), overriding the derived pitch-order. Transient (persisting it as a
    /// saved exercise is the next step). Duplicates allowed — a path may revisit
    /// a note.
    pub authored_path: Vec<(usize, u8)>,
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

/// Voicing shape for `count` tones as `(duration secs, strum offset ms)`.
/// A scale becomes an ascending cascade (a wide per-note offset, so you
/// hear it climb); a chord strums tight (all tones near-together).
fn voicing_shape(count: usize, scale_like: bool) -> (f32, f32) {
    if scale_like {
        ((count as f32 * 0.13 + 0.4).min(3.0), 130.0)
    } else {
        (1.4, 18.0)
    }
}

/// Pitch class (0..=11) of an equal-tempered frequency, via its nearest MIDI
/// note. Used to fold board positions and voicing pitches onto pitch classes
/// for the Mute mode (drop a note by its class, at any octave).
fn pc_from_hz(hz: f32) -> u8 {
    if hz <= 0.0 {
        return 0;
    }
    let midi = (69.0 + 12.0 * (hz / 440.0).log2()).round() as i32;
    midi.rem_euclid(12) as u8
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
            // Overwritten by apply_neck from the settings + instrument; the
            // literal only covers a StageState built before that runs.
            fret_count: 22,
            fret_start: 0,
            arpeggio_idx: 0,
            arpeggio_position_idx: 0,
            arpeggio_step_idx: 0,
            arpeggio_playing: false,
            scale_run_playing: false,
            scale_run_step: 0,
            scale_run_active: None,
            path_shown: false,
            draw_mode: false,
            authored_path: Vec::new(),
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

    /// Explainable immediate neighbors of the current catalog material. The
    /// graph owns relations; core maps stable catalog identity back into the
    /// selection handles the product already uses.
    pub fn related_material(&self, limit: usize) -> Vec<RelatedSuggestion> {
        let selected_id = match self.lens {
            Lens::Scales => woodshed_graph::scale_id(self.scale().name),
            Lens::Chords => woodshed_graph::chord_id(self.chord().name),
            Lens::Arpeggios => woodshed_graph::arpeggio_id(self.arpeggio_chord().name),
            Lens::Progressions => {
                let Some(idx) = self.progression_idx else { return Vec::new() };
                woodshed_graph::progression_id(self.progressions()[idx].name)
            }
            Lens::Exercises => woodshed_graph::exercise_id(self.exercise().name),
        };

        woodshed_graph::related_material(&selected_id, limit)
            .into_iter()
            .filter_map(|item| {
                use woodshed_graph::CatalogKind;
                let (kind, target) = match item.kind {
                    CatalogKind::Scale => (
                        "Scale",
                        RelatedTarget::Scale(self.scales().iter().position(|x| x.name == item.name)?),
                    ),
                    CatalogKind::Chord => (
                        "Chord",
                        RelatedTarget::Chord(self.chords().iter().position(|x| x.name == item.name)?),
                    ),
                    CatalogKind::Arpeggio => (
                        "Arpeggio",
                        RelatedTarget::Arpeggio(
                            self.chords().iter().position(|x| x.name == item.name)?,
                        ),
                    ),
                    CatalogKind::Progression => (
                        "Progression",
                        RelatedTarget::Progression(
                            self.progressions().iter().position(|x| x.name == item.name)?,
                        ),
                    ),
                    CatalogKind::Exercise => (
                        "Exercise",
                        RelatedTarget::Exercise(
                            self.exercises().iter().position(|x| x.name == item.name)?,
                        ),
                    ),
                };
                Some(RelatedSuggestion {
                    title: item.name,
                    kind,
                    reason: item.reason,
                    score: item.score,
                    target,
                })
            })
            .collect()
    }

    pub fn catalog_id(&self) -> Option<String> {
        match self.lens {
            Lens::Scales => Some(woodshed_graph::scale_id(self.scale().name)),
            Lens::Chords => Some(woodshed_graph::chord_id(self.chord().name)),
            Lens::Arpeggios => Some(woodshed_graph::arpeggio_id(self.arpeggio_chord().name)),
            Lens::Progressions => self
                .progression_idx
                .map(|idx| woodshed_graph::progression_id(self.progressions()[idx].name)),
            Lens::Exercises => Some(woodshed_graph::exercise_id(self.exercise().name)),
        }
    }

    pub fn related_material_with_history(
        &self,
        history: &history::PracticeHistory,
        limit: usize,
    ) -> Vec<RelatedSuggestion> {
        let Some(selected_id) = self.catalog_id() else { return Vec::new() };
        let mut ranked: Vec<(usize, usize, RelatedSuggestion)> = self
            .related_material(usize::MAX)
            .into_iter()
            .enumerate()
            .map(|(stable_order, mut suggestion)| {
                let target_id = match suggestion.target {
                    RelatedTarget::Scale(idx) => woodshed_graph::scale_id(self.scales()[idx].name),
                    RelatedTarget::Chord(idx) => woodshed_graph::chord_id(self.chords()[idx].name),
                    RelatedTarget::Arpeggio(idx) => {
                        woodshed_graph::arpeggio_id(self.chords()[idx].name)
                    }
                    RelatedTarget::Progression(idx) => {
                        woodshed_graph::progression_id(self.progressions()[idx].name)
                    }
                    RelatedTarget::Exercise(idx) => {
                        woodshed_graph::exercise_id(self.exercises()[idx].name)
                    }
                };
                let count = history.related_transition_count(&selected_id, &target_id);
                if count > 0 {
                    suggestion.reason = if count == 1 {
                        "Previously staged from here".to_string()
                    } else {
                        format!("Staged from here {count} times")
                    };
                }
                (count, stable_order, suggestion)
            })
            .collect();
        ranked.sort_by_key(|(count, stable_order, _)| (std::cmp::Reverse(*count), *stable_order));
        ranked
            .into_iter()
            .take(limit)
            .map(|(_, _, suggestion)| suggestion)
            .collect()
    }

    pub fn related_target_id(&self, target: RelatedTarget) -> String {
        match target {
            RelatedTarget::Scale(idx) => woodshed_graph::scale_id(self.scales()[idx].name),
            RelatedTarget::Chord(idx) => woodshed_graph::chord_id(self.chords()[idx].name),
            RelatedTarget::Arpeggio(idx) => woodshed_graph::arpeggio_id(self.chords()[idx].name),
            RelatedTarget::Progression(idx) => {
                woodshed_graph::progression_id(self.progressions()[idx].name)
            }
            RelatedTarget::Exercise(idx) => woodshed_graph::exercise_id(self.exercises()[idx].name),
        }
    }

    pub fn related_material_configured(
        &self,
        history: &history::PracticeHistory,
        settings: &storage::RelatedSettings,
        limit: usize,
    ) -> Vec<RelatedSuggestion> {
        let suggestions = if settings.use_history {
            self.related_material_with_history(history, usize::MAX)
        } else {
            self.related_material(usize::MAX)
        };
        suggestions
            .into_iter()
            .filter(|suggestion| {
                !settings
                    .dismissed_ids
                    .contains(&self.related_target_id(suggestion.target))
            })
            .take(limit)
            .collect()
    }

    pub fn neighborhood_snapshot(
        &self,
        history: &history::PracticeHistory,
        settings: &storage::RelatedSettings,
        limit: usize,
    ) -> NeighborhoodSnapshot {
        let Some(center_id) = self.catalog_id() else { return NeighborhoodSnapshot::default() };
        let center_title = center_id
            .split_once(':')
            .map(|(_, title)| title)
            .unwrap_or(center_id.as_str())
            .to_string();
        let mut nodes = vec![NeighborhoodNode {
            id: center_id,
            title: center_title,
            kind: self.lens.label(),
            score: 100,
        }];
        nodes.extend(
            self.related_material_configured(history, settings, limit)
                .into_iter()
                .map(|suggestion| NeighborhoodNode {
                    id: self.related_target_id(suggestion.target),
                    title: suggestion.title,
                    kind: suggestion.kind,
                    score: suggestion.score,
                }),
        );
        let edges = (1..nodes.len()).map(|idx| (0, idx as u16)).collect();
        NeighborhoodSnapshot { nodes, edges }
    }

    pub fn select_related(&mut self, target: RelatedTarget) {
        match target {
            RelatedTarget::Scale(idx) => {
                self.set_lens(Lens::Scales);
                self.select_scale(idx);
            }
            RelatedTarget::Chord(idx) => {
                self.set_lens(Lens::Chords);
                self.select_chord(idx);
            }
            RelatedTarget::Arpeggio(idx) => {
                self.set_lens(Lens::Arpeggios);
                self.select_arpeggio(idx);
            }
            RelatedTarget::Progression(idx) => {
                self.set_lens(Lens::Progressions);
                self.select_progression(idx);
            }
            RelatedTarget::Exercise(idx) => {
                self.set_lens(Lens::Exercises);
                self.select_exercise(idx);
            }
        }
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

    /// Set the neck window shown on the board from the settings. `end` of `None`
    /// means the current instrument's full standard neck. The window is
    /// `fret_start ..= fret_count` (inclusive); a start of 0 includes the open
    /// strings. Restores an old woodshed capability: pick the fret range
    /// (0-12, 8-16, 2-22). Callers re-apply this on a tuning change too, so the
    /// full-neck default tracks the instrument.
    pub fn apply_neck(&mut self, start: u8, end: Option<u8>) {
        let specs = tunings();
        let standard = specs[self.tuning_idx.min(specs.len() - 1)]
            .instrument
            .standard_fret_count();
        // Cap generously (24-fret basses, 24-position bowed necks) but bound the
        // painted board.
        let end = end.unwrap_or(standard).clamp(1, 30);
        self.fret_start = start.min(end);
        self.fret_count = end;
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

    /// The hand-drawn path as a rehearsal [`Card`] — Draw mode's Save. Unlike
    /// every other card, it names no catalog formula: the positions ride inline
    /// in a [`Material::Path`], with the root they were drawn over so degrees
    /// still resolve. This is the draw → save → practice loop's last leg.
    /// `None` when nothing is drawn.
    pub fn card_from_drawn_path(&self) -> Option<Card> {
        if self.authored_path.is_empty() {
            return None;
        }
        Some(Card {
            label: format!(
                "{} path — {} notes",
                self.material_name(),
                self.authored_path.len()
            ),
            material: Material::Path {
                positions: self.authored_path.clone(),
                root: PitchClass::new(9 + self.root_idx as u8), // A = pc 9
            },
            setting: Setting {
                instrument: String::new(),
                tuning: Some(self.tuning().name.clone()),
                ..Setting::default()
            },
            // A drawn path carries its own order, so it walks — the honest touch
            // for it, where Block would sound the notes on top of each other.
            touch: Touch::Walk,
            timing: Timing::default(),
            from: None,
        })
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
                                // Exercise steps carry a finger, not a resolved
                                // pitch, so there is no note detail to show.
                                octave: 0,
                                degree: String::new(),
                                interval_name: String::new(),
                                frequency: 0.0,
                            })
                            .collect()
                    })
                    .unwrap_or_default();
            }
            Material::Path { positions, root } => {
                // A drawn path carries its own notes: resolve each position
                // against the current tuning and name its degree from the root
                // it was drawn over. Positions off the neck (a saved path opened
                // under a narrower tuning) are dropped rather than panicking.
                let strings = self.string_count();
                let root_pc = root.value() as i32;
                return positions
                    .iter()
                    .filter(|(s, f)| {
                        *s < strings && *f >= self.fret_start && *f <= self.fret_count
                    })
                    .map(|&(s, f)| {
                        let pitch = board.pitch_at(s, f);
                        let semis = (pitch.pitch_class() as i32 - root_pc).rem_euclid(12);
                        FretDot {
                            string_index: s,
                            fret: f,
                            is_root: semis == 0,
                            label: format!("{}{}", pitch.name, pitch.accidental),
                            octave: pitch.octave,
                            degree: degree_label(semis).to_string(),
                            interval_name: interval_name(semis).to_string(),
                            frequency: pitch.frequency() as f32,
                        }
                    })
                    .collect();
            }
        };
        // Setting fidelity: a pinned fret window filters the positions to
        // the hand position (capo + per-card tuning still deferred). The board's
        // neck window (fret_start..) applies on top, since the board is drawn to
        // that extent.
        let window = card.setting.fret_window;
        let neck_start = self.fret_start;
        let in_window = |fret: u8| {
            fret >= neck_start
                && window.is_none_or(|w| fret >= w.start && fret <= w.start.saturating_add(w.span))
        };
        positions
            .map(|ps| {
                ps.into_iter()
                    .filter(|p| in_window(p.fret))
                    .map(FretDot::from_position)
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
                    .map(FretDot::from_position)
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

    /// Show or hide the touch's path trail on the board.
    pub fn toggle_path(&mut self) {
        self.path_shown = !self.path_shown;
    }

    /// Enter or leave draw mode (clicking markers draws the path by hand). The
    /// drawn path is kept when leaving, so the run keeps following it.
    pub fn toggle_draw_mode(&mut self) {
        self.draw_mode = !self.draw_mode;
    }

    /// Append a marker to the drawn path (the next step). Duplicates are allowed
    /// so a path can revisit a note.
    pub fn append_to_path(&mut self, string_index: usize, fret: u8) {
        self.authored_path.push((string_index, fret));
    }

    /// Undo the last drawn step.
    pub fn undo_path(&mut self) {
        self.authored_path.pop();
    }

    /// Clear the drawn path (the run reverts to the derived pitch-order).
    pub fn clear_path(&mut self) {
        self.authored_path.clear();
        self.scale_run_step = 0;
    }

    /// Reverse the drawn path (retrograde).
    pub fn reverse_path(&mut self) {
        self.authored_path.reverse();
        self.scale_run_step = 0;
    }

    /// Rotate the drawn path's start forward by one step.
    pub fn rotate_path(&mut self) {
        if !self.authored_path.is_empty() {
            self.authored_path.rotate_left(1);
            self.scale_run_step = 0;
        }
    }

    /// The run's visit order as `(string, fret)` — the path trail the leaf draws.
    pub fn run_positions(&self) -> Vec<(usize, u8)> {
        self.effective_run_path()
            .into_iter()
            .map(|(s, f, _)| (s, f))
            .collect()
    }

    /// The path the run actually walks: the hand-drawn `authored_path` when one
    /// exists (frequencies looked up from the current board), else the derived
    /// scale-run order. This is the seam where a drawn touch overrides the preset.
    pub fn effective_run_path(&self) -> Vec<(usize, u8, f32)> {
        if self.authored_path.is_empty() {
            return self.scale_run_path();
        }
        // Resolve each step's pitch from the neck itself rather than looking it
        // up among the current dots: a drawn note is a real position whatever
        // the lens shows, and a missed lookup would silently step in silence.
        let board = Fretboard::new(self.tuning(), self.fret_count);
        let strings = self.string_count();
        self.authored_path
            .iter()
            .filter(|(s, f)| *s < strings && *f <= self.fret_count)
            .map(|&(s, f)| (s, f, board.pitch_at(s, f).frequency() as f32))
            .collect()
    }

    /// Shift the whole drawn path along the neck by `delta` frets — move the
    /// shape, keep its intervals. A no-op if any note would leave the neck, so
    /// the shape never distorts by clamping. An octave (±12) is the shift that
    /// keeps every note's pitch class, so the path stays on the material's tones
    /// and its degrees hold — which is exactly the octave spider's generator.
    pub fn shift_path(&mut self, delta: i32) {
        if !self.can_shift_path(delta) {
            return;
        }
        for (_, f) in self.authored_path.iter_mut() {
            *f = (*f as i32 + delta) as u8;
        }
        self.scale_run_step = 0;
    }

    /// Whether [`Self::shift_path`] would fit on the neck — every note lands in
    /// `0..=fret_count`. The view gates the shift controls on this rather than
    /// offering a button that silently does nothing: on the default 12-fret neck
    /// an octave shift rarely fits at all.
    pub fn can_shift_path(&self, delta: i32) -> bool {
        !self.authored_path.is_empty()
            && self.authored_path.iter().all(|&(_, f)| {
                let shifted = f as i32 + delta;
                shifted >= 0 && shifted <= self.fret_count as i32
            })
    }

    /// The scale-run path: the current board dots sorted ascending by pitch, each
    /// as `(string, fret, frequency)`. A run climbs the neck through them.
    pub fn scale_run_path(&self) -> Vec<(usize, u8, f32)> {
        let mut dots: Vec<(usize, u8, f32)> = self
            .dots()
            .into_iter()
            .map(|d| (d.string_index, d.fret, d.frequency))
            .collect();
        dots.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
        dots
    }

    /// Advance the run one step: mark the note now sounding (for the board
    /// highlight), return its frequency to play, and move the cursor on. Wraps.
    pub fn scale_run_tick(&mut self) -> Option<f32> {
        let path = self.effective_run_path();
        if path.is_empty() {
            self.scale_run_active = None;
            return None;
        }
        let (s, f, freq) = path[self.scale_run_step % path.len()];
        self.scale_run_active = Some((s, f));
        self.scale_run_step = self.scale_run_step.wrapping_add(1);
        Some(freq)
    }

    pub fn toggle_scale_run(&mut self) {
        self.scale_run_playing = !self.scale_run_playing;
        self.scale_run_step = 0;
        self.scale_run_active = None;
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
        let start = self.fret_start;
        positions
            .map(|ps| {
                ps.into_iter()
                    .filter(|p| p.fret >= start)
                    .map(FretDot::from_position)
                    .collect()
            })
            .unwrap_or_default()
    }

    // === Voicing (S4 slice 12: "hear the theory") ===

    /// The chord / scale tones of the active lens as frequencies (Hz),
    /// lowest first — the raw material for the "♪ Hear" preview. Empty
    /// for the Exercise lens (a fingering pattern, not one voiceable
    /// chord) and before a progression is picked.
    pub fn voicing_pitches(&self) -> Vec<f32> {
        fn to_hz(ps: Vec<Pitch>) -> Vec<f32> {
            ps.iter().map(|p| p.frequency() as f32).collect()
        }
        match self.lens {
            Lens::Scales => self.scale().apply_to(self.root()).map(to_hz).unwrap_or_default(),
            Lens::Chords => self.chord().apply_to(self.root()).map(to_hz).unwrap_or_default(),
            Lens::Arpeggios => {
                self.arpeggio_chord().apply_to(self.root()).map(to_hz).unwrap_or_default()
            }
            Lens::Progressions => self.progression_expanded_pitches().unwrap_or_default(),
            Lens::Exercises => Vec::new(),
        }
    }

    /// The active lens's material as an on-demand preview: `(pitches Hz,
    /// duration secs, strum offset ms)`. Empty pitches = nothing to voice.
    pub fn voicing_preview(&self) -> (Vec<f32>, f32, f32) {
        let pitches = self.voicing_pitches();
        if pitches.is_empty() {
            return (pitches, 0.0, 0.0);
        }
        let (dur, strum) = voicing_shape(pitches.len(), self.lens == Lens::Scales);
        (pitches, dur, strum)
    }

    /// Tones of the expanded progression chord (Hz) — the Progression
    /// arm of [`Self::voicing_pitches`].
    fn progression_expanded_pitches(&self) -> Option<Vec<f32>> {
        let idx = self.progression_idx?;
        let prog = progression_catalog().get(idx)?;
        let major = scale_catalog().iter().find(|s| s.name == "Major")?;
        let chords = prog.apply_in_key(self.root(), major).ok()?;
        if chords.is_empty() {
            return None;
        }
        let expanded = self.progression_expanded.min(chords.len() - 1);
        let chord = &chords[expanded];
        let ps = chord.formula.apply_to(chord.root).ok()?;
        Some(ps.iter().map(|p| p.frequency() as f32).collect())
    }

    /// A rehearsal card's material as a preview `(pitches Hz, duration
    /// secs, strum offset ms)` — the Rehearsal-tab counterpart to
    /// [`Self::voicing_preview`]. Riff cards don't voice (empty).
    pub fn card_voicing(&self, card: &Card) -> (Vec<f32>, f32, f32) {
        fn to_hz(ps: Vec<Pitch>) -> Vec<f32> {
            ps.iter().map(|p| p.frequency() as f32).collect()
        }
        let root_of =
            |pc: &PitchClass| Pitch::from_midi(48 + pc.value() as i32, Spelling::Sharps);
        let (pitches, scale_like) = match &card.material {
            Material::Scale { name, root } => (
                scale_catalog()
                    .iter()
                    .find(|s| s.name == name.as_str())
                    .and_then(|s| s.apply_to(root_of(root)).ok())
                    .map(to_hz)
                    .unwrap_or_default(),
                true,
            ),
            Material::Chord { name, root } => (
                chord_catalog()
                    .iter()
                    .find(|c| c.name == name.as_str())
                    .and_then(|c| c.apply_to(root_of(root)).ok())
                    .map(to_hz)
                    .unwrap_or_default(),
                false,
            ),
            Material::Riff { .. } => (Vec::new(), false),
            Material::Path { positions, .. } => {
                // A drawn path is inherently sequential: sound it as an
                // ascending-style cascade *in drawn order* (its order is the
                // material), not as a block.
                let board = Fretboard::new(self.tuning(), self.fret_count);
                let strings = self.string_count();
                let hz: Vec<f32> = positions
                    .iter()
                    .filter(|(s, f)| *s < strings && *f <= self.fret_count)
                    .map(|&(s, f)| board.pitch_at(s, f).frequency() as f32)
                    .collect();
                (hz, true)
            }
        };
        if pitches.is_empty() {
            return (pitches, 0.0, 0.0);
        }
        // Walk is a behaviour, not a label: it visits the notes one at a time
        // (the cascade shape) rather than sounding them together.
        let scale_like = scale_like || matches!(card.touch, Touch::Walk);
        let (dur, strum) = voicing_shape(pitches.len(), scale_like);
        (pitches, dur, strum)
    }

    /// The card's *effective* sound, after its mark mode is applied — the
    /// audible half of mark + solo/mute. Off (or nothing marked) is the plain
    /// voicing; Solo plays only the marked positions' pitches; Mute plays the
    /// voicing minus the marked notes' pitch classes. Same shape tuple as
    /// [`Self::card_voicing`]. This is what the "hear it" paths resolve to.
    pub fn card_sounding_pitches(&self, card: &Card) -> (Vec<f32>, f32, f32) {
        let marked = &card.setting.marked;
        if marked.is_empty() || card.setting.mark_mode == MarkMode::Off {
            return self.card_voicing(card);
        }
        match card.setting.mark_mode {
            MarkMode::Off => self.card_voicing(card),
            MarkMode::Solo => {
                // Play only the marked positions, as their actual fretboard
                // pitches — isolate the shape you selected.
                let set: std::collections::HashSet<(usize, u8)> =
                    marked.iter().copied().collect();
                let mut hz: Vec<f32> = self
                    .dots_for_card(card)
                    .into_iter()
                    .filter(|d| set.contains(&(d.string_index, d.fret)) && d.frequency > 0.0)
                    .map(|d| d.frequency)
                    .collect();
                hz.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                hz.dedup();
                if hz.is_empty() {
                    return self.card_voicing(card);
                }
                let (dur, strum) = voicing_shape(hz.len(), false);
                (hz, dur, strum)
            }
            MarkMode::Mute => {
                // Drop the marked notes' pitch classes from the clean voicing,
                // so the chord keeps voicing nicely minus the muted note(s).
                let dots = self.dots_for_card(card);
                let muted_pcs: std::collections::HashSet<u8> = marked
                    .iter()
                    .filter_map(|pos| {
                        dots.iter()
                            .find(|d| (d.string_index, d.fret) == *pos)
                            .map(|d| pc_from_hz(d.frequency))
                    })
                    .collect();
                let (pitches, _, _) = self.card_voicing(card);
                let kept: Vec<f32> = pitches
                    .into_iter()
                    .filter(|&f| !muted_pcs.contains(&pc_from_hz(f)))
                    .collect();
                if kept.is_empty() {
                    return (Vec::new(), 0.0, 0.0);
                }
                let (dur, strum) = voicing_shape(kept.len(), false);
                (kept, dur, strum)
            }
        }
    }

    /// Frequency (Hz) of the arpeggio step-through's current tone — the
    /// step-sonification source. `None` if the walk is empty.
    pub fn arpeggio_current_pitch_hz(&self) -> Option<f32> {
        let formula = self.arpeggio_chord();
        let bass = self.arpeggio_bass();
        let board = Fretboard::new(self.tuning(), self.fret_count);
        let shapes = generate_shapes(&board, formula, self.root(), bass);
        if shapes.is_empty() {
            return None;
        }
        let position_idx = self.arpeggio_position_idx.min(shapes.len() - 1);
        let shape = &shapes[position_idx];
        let run = ArpeggioRun::new(&shape.positions, bass, self.arpeggio_direction);
        let current = run.position_at(self.arpeggio_step_idx)?;
        shape.positions.get(current).map(|p| p.pitch.frequency() as f32)
    }

    /// Frequency (Hz) of the exercise step-through's current note.
    /// `None` if the exercise generates no steps.
    pub fn exercise_current_pitch_hz(&self) -> Option<f32> {
        let ex = self.exercise();
        let params = ExerciseParams {
            starting_fret: self.exercise_starting_fret,
            ..ExerciseParams::default()
        };
        let steps = ex.generate(&self.tuning(), &params);
        if steps.is_empty() {
            return None;
        }
        let step = self.exercise_step_idx % steps.len();
        let s = &steps[step];
        let board = Fretboard::new(self.tuning(), self.fret_count);
        Some(board.pitch_at(s.string_index, s.fret).frequency() as f32)
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

    #[test]
    fn voicing_pitches_per_lens() {
        let mut s = StageState::new();
        s.set_lens(Lens::Scales);
        assert!(s.voicing_pitches().len() >= 5, "scale tones voiced");
        assert!(
            s.voicing_pitches().iter().all(|f| *f > 20.0 && *f < 5000.0),
            "frequencies in audible range"
        );
        s.set_lens(Lens::Chords);
        assert!(s.voicing_pitches().len() >= 3, "chord tones voiced");
        s.set_lens(Lens::Arpeggios);
        assert!(s.voicing_pitches().len() >= 3, "arpeggio tones voiced");
        s.set_lens(Lens::Exercises);
        assert!(s.voicing_pitches().is_empty(), "exercises don't voice a chord");
    }

    #[test]
    fn voicing_preview_shapes_scale_vs_chord() {
        let mut s = StageState::new();
        s.set_lens(Lens::Scales);
        let (p, dur, strum) = s.voicing_preview();
        assert!(!p.is_empty() && dur > 0.0);
        assert!(strum > 100.0, "scale cascades with a wide strum offset");
        s.set_lens(Lens::Chords);
        let (_p, _dur, strum) = s.voicing_preview();
        assert!(strum < 50.0, "chord strums tight");
    }

    #[test]
    fn progression_voicing_after_selection() {
        let mut s = StageState::new();
        s.set_lens(Lens::Progressions);
        assert!(s.voicing_pitches().is_empty(), "cold start, nothing to voice");
        s.select_progression(0);
        assert!(s.voicing_pitches().len() >= 2, "expanded chord voiced");
    }

    #[test]
    fn arpeggio_step_pitch_tracks_transport() {
        let mut s = StageState::new();
        s.set_lens(Lens::Arpeggios);
        let p0 = s.arpeggio_current_pitch_hz().expect("a current tone");
        assert!(p0 > 20.0);
        let mut seen_change = false;
        for _ in 0..8 {
            s.arpeggio_advance();
            if let Some(p) = s.arpeggio_current_pitch_hz() {
                if (p - p0).abs() > 0.1 {
                    seen_change = true;
                }
            }
        }
        assert!(seen_change, "arpeggio walk visits multiple pitches");
    }

    #[test]
    fn card_voicing_matches_material() {
        let mut s = StageState::new();
        s.set_lens(Lens::Chords);
        let card = s.card_from_lens().unwrap();
        let (pitches, dur, strum) = s.card_voicing(&card);
        assert!(pitches.len() >= 3, "chord card voices its tones");
        assert!(dur > 0.0 && strum < 50.0);
        // A riff (exercise) card doesn't voice.
        s.set_lens(Lens::Exercises);
        let riff = s.card_from_lens().unwrap();
        assert!(s.card_voicing(&riff).0.is_empty());
    }

    #[test]
    fn drawn_path_saves_as_a_playable_card() {
        let mut s = StageState::new();
        assert!(
            s.card_from_drawn_path().is_none(),
            "nothing drawn, nothing to save"
        );
        for &(string, fret) in &[(0u8, 5u8), (1, 7), (2, 5)] {
            s.append_to_path(string as usize, fret);
        }
        let card = s.card_from_drawn_path().expect("a drawn path makes a card");
        assert_eq!(card.material.tag(), "Path");
        // A drawn path carries its own order, so it walks — Block would sound
        // the notes on top of each other.
        assert!(
            matches!(card.touch, Touch::Walk),
            "a drawn path's touch is walk, not block"
        );

        // The saved card resolves back to exactly the drawn positions, in the
        // drawn order — the path *is* the material, not a re-derived set.
        let dots = s.dots_for_card(&card);
        assert_eq!(
            dots.iter()
                .map(|d| (d.string_index, d.fret))
                .collect::<Vec<_>>(),
            vec![(0, 5), (1, 7), (2, 5)],
            "path card resolves to the drawn positions in drawn order"
        );
        // Unlike riff steps, a drawn path's notes carry real pitch, so the
        // detail card and the run's audio have something to say.
        assert!(
            dots.iter()
                .all(|d| !d.label.is_empty() && d.frequency > 0.0),
            "every drawn note resolves to a named, sounding pitch"
        );
        // And it sounds every drawn note (a riff card voices nothing).
        assert_eq!(
            s.card_voicing(&card).0.len(),
            3,
            "the voicing sounds every drawn note"
        );
        // A hand-drawn path is its own content, not a catalog subject.
        assert!(
            crate::history::catalog_id_for_card(&card).is_none(),
            "a drawn path has no catalog identity"
        );
    }

    #[test]
    fn neck_window_bounds_the_board_and_the_dots() {
        let mut s = StageState::new();
        s.set_lens(Lens::Scales);

        // Full neck: the instrument's standard count, open strings included.
        s.apply_neck(0, None);
        assert_eq!(s.fret_start, 0);
        assert!(s.fret_count >= 22, "a guitar's full neck reaches fret 22+");
        assert!(
            s.dots().iter().any(|d| d.fret == 0),
            "a nut-anchored window shows open strings"
        );

        // A mid-neck window (8-16): every dot lands inside it, none below.
        s.apply_neck(8, Some(16));
        assert_eq!((s.fret_start, s.fret_count), (8, 16));
        let dots = s.dots();
        assert!(!dots.is_empty(), "the window still has notes");
        assert!(
            dots.iter().all(|d| d.fret >= 8 && d.fret <= 16),
            "no dot falls outside the neck window"
        );

        // A start past the end is clamped to the end, not left inverted.
        s.apply_neck(30, Some(12));
        assert!(s.fret_start <= s.fret_count);
    }

    #[test]
    fn octave_shift_moves_the_shape_and_keeps_its_notes() {
        let mut s = StageState::new();
        // An octave shift needs neck to land on: on the default 12-fret board it
        // never fits, which is why the controls are gated on can_shift_path.
        s.fret_count = 24;
        for &(string, fret) in &[(0usize, 5u8), (1, 7), (2, 5)] {
            s.append_to_path(string, fret);
        }
        assert!(s.can_shift_path(12), "a 24-fret neck has room for the octave");
        assert!(!s.can_shift_path(-12), "there is no room below fret 5");
        let before = s.effective_run_path();
        s.shift_path(12);
        assert_eq!(
            s.authored_path,
            vec![(0, 17), (1, 19), (2, 17)],
            "an octave shift moves every note 12 frets, keeping the shape"
        );
        // Same notes an octave up: pitch classes hold, so the shape stays on the
        // material's tones and its degrees still read.
        let after = s.effective_run_path();
        for (b, a) in before.iter().zip(after.iter()) {
            assert_eq!(
                pc_from_hz(b.2),
                pc_from_hz(a.2),
                "an octave shift preserves each step's pitch class"
            );
            assert!(
                (a.2 / b.2 - 2.0).abs() < 0.01,
                "each step lands exactly one octave up"
            );
        }
        // Refuses rather than distorting when the shape would run off the neck.
        let at_top = s.authored_path.clone();
        assert!(!s.can_shift_path(12), "no room for a second octave");
        s.shift_path(12);
        assert_eq!(
            s.authored_path, at_top,
            "a shift that would leave the neck is a no-op, not a clamp"
        );
    }

    #[test]
    fn card_sounding_pitches_respects_mark_mode() {
        let mut s = StageState::new();
        s.set_lens(Lens::Chords);
        let mut card = s.card_from_lens().unwrap();

        // Off (nothing marked): the whole voicing.
        let full = s.card_sounding_pitches(&card).0;
        assert!(full.len() >= 3, "off plays the whole voicing");

        // Mark one board position.
        let dots = s.dots_for_card(&card);
        let target = dots.first().expect("chord card has positions");
        let target_pc = pc_from_hz(target.frequency);
        card.setting.marked = vec![(target.string_index, target.fret)];

        // Mute: the marked note's pitch class drops from the voicing.
        card.setting.mark_mode = MarkMode::Mute;
        let muted = s.card_sounding_pitches(&card).0;
        assert!(
            muted.iter().all(|&f| pc_from_hz(f) != target_pc),
            "mute drops the marked note's pitch class"
        );
        assert!(muted.len() < full.len(), "mute removes at least one tone");

        // Solo: only the marked position sounds.
        card.setting.mark_mode = MarkMode::Solo;
        let solo = s.card_sounding_pitches(&card).0;
        assert_eq!(solo.len(), 1, "solo plays only the marked position");
        assert!(
            (solo[0] - target.frequency).abs() < 0.5,
            "solo plays the marked pitch"
        );
    }

    #[test]
    fn exercise_step_pitch_available() {
        let mut s = StageState::new();
        s.set_lens(Lens::Exercises);
        let p = s.exercise_current_pitch_hz().expect("exercise has notes");
        assert!(p > 20.0 && p < 5000.0);
    }

    #[test]
    fn related_material_selects_through_catalog_identity() {
        let mut s = StageState::new();
        s.set_lens(Lens::Chords);
        let major_7 = s.chords().iter().position(|c| c.name == "Major 7").unwrap();
        s.select_chord(major_7);
        let related = s.related_material(64);
        let major = related
            .iter()
            .find(|item| item.kind == "Scale" && item.title == "Major")
            .expect("Major 7 relates to Major");
        assert!(major.reason.contains("fits this scale"));
        s.select_related(major.target);
        assert_eq!(s.lens, Lens::Scales);
        assert_eq!(s.scale().name, "Major");
    }

    #[test]
    fn related_history_promotes_a_prior_stage_path() {
        let mut s = StageState::new();
        s.set_lens(Lens::Scales);
        let dorian = s.scales().iter().position(|scale| scale.name == "Dorian").unwrap();
        s.select_scale(dorian);
        let mut history = history::PracticeHistory::default();
        history.record(
            woodshed_graph::chord_id("Minor 7"),
            history::EngagementKind::Staged,
            Some(woodshed_graph::scale_id("Dorian")),
        );
        let ranked = s.related_material_with_history(&history, 5);
        assert_eq!(ranked[0].title, "Minor 7");
        assert_eq!(ranked[0].reason, "Previously staged from here");
    }

    #[test]
    fn related_settings_disable_history_and_hide_stable_identities() {
        let mut s = StageState::new();
        s.set_lens(Lens::Scales);
        let dorian = s.scales().iter().position(|scale| scale.name == "Dorian").unwrap();
        s.select_scale(dorian);
        let mut history = history::PracticeHistory::default();
        history.record(
            woodshed_graph::chord_id("Minor 7"),
            history::EngagementKind::Staged,
            Some(woodshed_graph::scale_id("Dorian")),
        );
        let settings = storage::RelatedSettings {
            use_history: false,
            show_neighborhood: true,
            dismissed_ids: vec![woodshed_graph::chord_id("Minor 7")],
        };
        let related = s.related_material_configured(&history, &settings, 5);
        assert!(related.iter().all(|item| item.title != "Minor 7"));
        assert!(related.iter().all(|item| !item.reason.contains("Previously")));
    }
}
