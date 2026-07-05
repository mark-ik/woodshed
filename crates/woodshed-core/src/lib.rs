//! Woodshed's portable state/logic core.
//!
//! Pure state over the `woodshedding` theory crate — no UI, no host, no
//! audio dependency, so the same core feeds the desktop host, the web host,
//! and tests. This is the serval-host plan's W1.1 split.
//!
//! S1 ships the Stage slice ([`StageState`]): tuning + root + selected
//! scale, resolved to fretboard dots. The remaining lenses (chords,
//! progressions, exercises, arpeggios) migrate here from woodshed-xilem's
//! `AppState` during S4.

use woodshedding::fretboard::Fretboard;
use woodshedding::pitch::{Pitch, Spelling};
use woodshedding::scale::{catalog as scale_catalog, ScaleFormula};
use woodshedding::tuning::{catalog as tuning_catalog, Tuning};

/// One rendered fretboard dot: a scale position resolved against the
/// current tuning, ready for a view layer to place.
#[derive(Clone, Debug)]
pub struct FretDot {
    pub string_index: usize,
    pub fret: u8,
    /// True when this position is the scale root (unison from the root).
    pub is_root: bool,
    /// Note name without octave ("A", "C#") — the dot label.
    pub label: String,
}

/// The Stage lens state: what the fretboard is showing right now.
pub struct StageState {
    pub tuning: Tuning,
    /// Shared musical root. Kept as a concrete pitch (octave included)
    /// because position resolution works in midi space; the octave does
    /// not affect which positions match (pitch-class matching).
    pub root: Pitch,
    /// Index into [`scale_catalog`].
    pub scale_idx: usize,
    /// Highest fret shown (inclusive, from the nut at 0).
    pub fret_count: u8,
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
        let spec = tuning_catalog()
            .first()
            .expect("tuning catalog is non-empty");
        let scale_idx = scale_catalog()
            .iter()
            .position(|s| s.name == "Minor Pentatonic")
            .unwrap_or(0);
        Self {
            tuning: Tuning::from_spec(spec),
            root: Pitch::from_midi(57, Spelling::Sharps), // A3
            scale_idx,
            fret_count: 12,
        }
    }

    pub fn scales(&self) -> &'static [ScaleFormula] {
        scale_catalog()
    }

    pub fn scale(&self) -> &'static ScaleFormula {
        &scale_catalog()[self.scale_idx.min(scale_catalog().len() - 1)]
    }

    pub fn select_scale(&mut self, idx: usize) {
        if idx < scale_catalog().len() {
            self.scale_idx = idx;
        }
    }

    pub fn string_count(&self) -> usize {
        self.tuning.string_count()
    }

    /// Resolve the current scale to fretboard dots. Ordered by string
    /// then fret; empty on a (theoretically impossible) transposition
    /// failure rather than panicking in a render path.
    pub fn dots(&self) -> Vec<FretDot> {
        let board = Fretboard::new(self.tuning.clone(), self.fret_count);
        board
            .positions_for_scale(self.scale(), self.root)
            .map(|positions| {
                positions
                    .into_iter()
                    .map(|p| FretDot {
                        string_index: p.string_index,
                        fret: p.fret,
                        is_root: p
                            .interval_from_root
                            .is_some_and(|i| i.semitones() == 0),
                        label: format!("{}{}", p.pitch.name, p.pitch.accidental),
                    })
                    .collect()
            })
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
}
