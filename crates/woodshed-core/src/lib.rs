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

use arpeggio::{generate_shapes, ArpeggioDirection, ArpeggioRun};
use woodshedding::chord::{catalog as chord_catalog, ChordFormula};
use woodshedding::fretboard::{Fretboard, Position};
use woodshedding::interval::Interval;
use woodshedding::pitch::{Pitch, Spelling};
use woodshedding::scale::{catalog as scale_catalog, ScaleFormula};
use woodshedding::tuning::{catalog as tuning_catalog, Tuning, TuningSpec};

/// The fretboard lens strip (redesign-plan vocabulary). Scales and Chords
/// resolve on the board today; the other three arrive with S4.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lens {
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

    /// True when the lens resolves dots on the board today.
    pub fn implemented(self) -> bool {
        matches!(self, Lens::Scales | Lens::Chords | Lens::Arpeggios)
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
            other => other.label().to_string(),
        }
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
    fn unimplemented_lenses_render_empty() {
        let mut s = StageState::new();
        s.set_lens(Lens::Progressions);
        assert!(s.dots().is_empty());
        assert!(!s.lens.implemented());
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
