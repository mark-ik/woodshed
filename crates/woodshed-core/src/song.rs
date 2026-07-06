//! The Song lane's portable model (S4 slice 7 + slice 10 editor).
//!
//! [`SongBar`] is the neutral bar description the core and views share:
//! everything the timeline renders and everything an audio backend needs
//! to voice the bar (pre-computed chord-tone frequencies, so backends
//! stay theory-free — the same posture as woodshed-audio's `ChordRef`).
//! [`SongDoc`] wraps the bar list with the song-level transport flags.
//! The desktop backend converts these into `woodshed_audio::Song`; the
//! web backend will voice them through Web Audio.

use serde::{Deserialize, Serialize};

use crate::{format_role, StageState};
use woodshedding::chord::{catalog as chord_catalog, ChordFormula};
use woodshedding::pitch::{Pitch, Spelling};
use woodshedding::progression::catalog as progression_catalog;
use woodshedding::scale::catalog as scale_catalog;

/// One timeline bar: display strings + the voicing data a backend needs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SongBar {
    /// Section / role label ("I", "Chorus"); doubles as the section
    /// marker in the timeline.
    pub label: String,
    /// Chord display label ("A", "Dm7"); empty = silent bar.
    pub chord_label: String,
    /// Chord catalog key for backends that re-derive ("Major", "Minor 7").
    pub formula_name: String,
    /// Root pitch class (0 = C .. 11 = B), the editor's handle. Serde
    /// default keeps older saves (which predate the field) loading.
    #[serde(default)]
    pub root_pc: u8,
    /// Root frequency in Hz (0.0 = silent bar, click only).
    pub root_freq_hz: f32,
    /// Pre-computed chord-tone frequencies, lowest first.
    pub pitches_hz: Vec<f32>,
    pub bpm: f32,
    /// Time-signature numerator (denominator 4 until meter editing lands).
    pub beats: u8,
    /// Measures this bar block spans (>= 1).
    pub length: u8,
}

/// Base MIDI for a root pitch class — C3 (matches `dots_for_card`).
const ROOT_BASE_MIDI: i32 = 48;

impl Default for SongBar {
    fn default() -> Self {
        let mut bar = Self {
            label: String::new(),
            chord_label: String::new(),
            formula_name: "Major".to_string(),
            root_pc: 0,
            root_freq_hz: 0.0,
            pitches_hz: Vec::new(),
            bpm: 120.0,
            beats: 4,
            length: 1,
        };
        bar.revoice();
        bar
    }
}

impl SongBar {
    /// The root pitch for this bar's `root_pc` (C-relative, octave 3).
    fn root_pitch(&self) -> Pitch {
        Pitch::from_midi(ROOT_BASE_MIDI + (self.root_pc % 12) as i32, Spelling::Sharps)
    }

    /// Note name for the current root ("C", "F#") — no octave.
    pub fn root_name(&self) -> String {
        let p = self.root_pitch();
        format!("{}{}", p.name, p.accidental)
    }

    fn formula(&self) -> &'static ChordFormula {
        chord_catalog()
            .iter()
            .find(|c| c.name == self.formula_name.as_str())
            .unwrap_or(&chord_catalog()[0])
    }

    /// Recompute `root_freq_hz`, `pitches_hz`, and `chord_label` from the
    /// current `root_pc` + `formula_name`. Called after any chord edit; a
    /// silent bar (empty formula) clears the voicing.
    pub fn revoice(&mut self) {
        if self.formula_name.is_empty() {
            self.root_freq_hz = 0.0;
            self.pitches_hz.clear();
            self.chord_label.clear();
            return;
        }
        let root = self.root_pitch();
        let formula = self.formula();
        let pitches: Vec<f32> = formula
            .apply_to(root)
            .map(|ps| ps.iter().map(|p| p.frequency() as f32).collect())
            .unwrap_or_else(|_| vec![root.frequency() as f32]);
        self.root_freq_hz = root.frequency() as f32;
        self.pitches_hz = pitches;
        self.chord_label = format!("{}{}{}", root.name, root.accidental, formula.symbol);
    }

    /// Cycle the root up a semitone (wraps C→B→C), revoicing.
    pub fn cycle_root(&mut self) {
        self.root_pc = (self.root_pc + 1) % 12;
        if !self.formula_name.is_empty() {
            self.revoice();
        }
    }

    /// Cycle the chord quality through the catalog (revoicing). An empty
    /// `formula_name` (silent) re-enters at the first catalog entry.
    pub fn cycle_formula(&mut self) {
        let names: Vec<&str> = chord_catalog().iter().map(|c| c.name).collect();
        let next = match names.iter().position(|n| *n == self.formula_name.as_str()) {
            Some(i) => names[(i + 1) % names.len()],
            None => names[0],
        };
        self.formula_name = next.to_string();
        self.revoice();
    }

    /// Toggle between the current chord and a silent (click-only) bar,
    /// remembering the chord so toggling back restores it.
    pub fn toggle_silent(&mut self) {
        if self.formula_name.is_empty() {
            self.formula_name = "Major".to_string();
        } else {
            self.formula_name.clear();
        }
        self.revoice();
    }

    pub fn nudge_bpm(&mut self, delta: f32) {
        self.bpm = (self.bpm + delta).clamp(30.0, 300.0);
    }

    pub fn cycle_beats(&mut self) {
        self.beats = match self.beats {
            3 => 4,
            4 => 5,
            5 => 6,
            6 => 7,
            _ => 3,
        };
    }

    pub fn cycle_length(&mut self) {
        self.length = if self.length >= 4 { 1 } else { self.length + 1 };
    }
}

/// Common section names the editor cycles through (empty = no section).
pub const SECTION_LABELS: [&str; 6] =
    ["", "Intro", "Verse", "Chorus", "Bridge", "Outro"];

/// The song document: bars plus the song-level transport flags.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SongDoc {
    pub name: String,
    pub bars: Vec<SongBar>,
    /// Stop at the last bar instead of looping.
    pub one_shot: bool,
    /// Metronome click sounds during playback.
    pub click: bool,
}

impl Default for SongDoc {
    fn default() -> Self {
        Self {
            name: String::new(),
            bars: Vec::new(),
            one_shot: false,
            click: true,
        }
    }
}

impl SongDoc {
    pub fn is_empty(&self) -> bool {
        self.bars.is_empty()
    }

    /// Insert a fresh blank bar after `idx` (or at the end), inheriting
    /// the neighbour's tempo so a new bar reads as a continuation.
    pub fn add_bar_after(&mut self, idx: usize) -> usize {
        let mut bar = SongBar::default();
        if let Some(src) = self.bars.get(idx) {
            bar.bpm = src.bpm;
            bar.beats = src.beats;
        }
        let at = (idx + 1).min(self.bars.len());
        self.bars.insert(at, bar);
        at
    }

    /// Duplicate the bar at `idx` right after it. No-op on an empty doc.
    pub fn duplicate(&mut self, idx: usize) -> usize {
        if let Some(bar) = self.bars.get(idx).cloned() {
            let at = idx + 1;
            self.bars.insert(at, bar);
            at
        } else {
            idx
        }
    }

    /// Remove the bar at `idx`; returns the clamped new cursor.
    pub fn remove(&mut self, idx: usize) -> usize {
        if idx < self.bars.len() {
            self.bars.remove(idx);
        }
        idx.min(self.bars.len().saturating_sub(1))
    }

    /// Swap the bar at `idx` with its neighbour in `dir` (-1 / +1);
    /// returns the moved bar's new index.
    pub fn move_bar(&mut self, idx: usize, dir: i32) -> usize {
        let target = idx as i32 + dir;
        if idx >= self.bars.len() || target < 0 || target as usize >= self.bars.len() {
            return idx;
        }
        let target = target as usize;
        self.bars.swap(idx, target);
        target
    }
}

/// Materialize the selected progression as a song document — the "From
/// progression" flow (one bar block per chord, roman-numeral section
/// labels). `None` until a progression is selected.
pub fn song_from_progression(state: &StageState, bpm: f32) -> Option<SongDoc> {
    let idx = state.progression_idx?;
    let prog = progression_catalog().get(idx)?;
    let major = scale_catalog().iter().find(|s| s.name == "Major")?;
    let chords = prog.apply_in_key(state.root(), major).ok()?;
    if chords.is_empty() {
        return None;
    }
    let name = format!("{} in {}", prog.name, state.root_name());
    let bars = chords
        .iter()
        .map(|c| {
            let mut bar = SongBar {
                label: format_role(&c.role),
                chord_label: String::new(),
                formula_name: c.formula.name.to_string(),
                root_pc: c.root.pitch_class(),
                root_freq_hz: 0.0,
                pitches_hz: Vec::new(),
                bpm,
                beats: 4,
                length: 1,
            };
            bar.revoice();
            bar
        })
        .collect();
    Some(SongDoc {
        name,
        bars,
        one_shot: false,
        click: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Lens;

    #[test]
    fn progression_becomes_song_doc() {
        let mut s = StageState::new();
        s.set_lens(Lens::Progressions);
        assert!(song_from_progression(&s, 100.0).is_none(), "cold start");
        s.select_progression(0);
        let doc = song_from_progression(&s, 100.0).expect("doc");
        assert!(doc.name.contains(" in A"));
        assert!(!doc.bars.is_empty());
        assert!(doc.click, "click on by default");
        for b in &doc.bars {
            assert!(b.root_freq_hz > 0.0);
            assert!(b.pitches_hz.len() >= 2, "chord tones voiced");
            assert_eq!(b.bpm, 100.0);
            assert!(!b.label.is_empty(), "roman numeral label");
            assert!(!b.chord_label.is_empty(), "chord label revoiced");
        }
    }

    #[test]
    fn bar_editing_revoices() {
        let mut bar = SongBar::default();
        assert_eq!(bar.root_name(), "C");
        assert_eq!(bar.chord_label, "C");
        bar.cycle_root(); // C -> C#
        assert_eq!(bar.root_name(), "C#");
        assert!(bar.chord_label.starts_with("C#"));
        bar.cycle_formula(); // Major -> next (Minor)
        assert_ne!(bar.formula_name, "Major");
        assert!(bar.root_freq_hz > 0.0);
        bar.toggle_silent();
        assert_eq!(bar.root_freq_hz, 0.0, "silent clears voicing");
        assert!(bar.chord_label.is_empty());
        bar.toggle_silent();
        assert!(bar.root_freq_hz > 0.0, "toggle back restores a chord");
    }

    #[test]
    fn doc_bar_ops() {
        let mut doc = SongDoc {
            name: "t".into(),
            bars: vec![SongBar::default(), SongBar::default()],
            one_shot: false,
            click: true,
        };
        doc.bars[0].cycle_root(); // C#, distinguishes bar 0
        let at = doc.add_bar_after(0);
        assert_eq!(at, 1);
        assert_eq!(doc.bars.len(), 3);
        let dup = doc.duplicate(0);
        assert_eq!(dup, 1);
        assert_eq!(doc.bars[1].root_name(), "C#", "dup copies the edited bar");
        let moved = doc.move_bar(0, 1);
        assert_eq!(moved, 1);
        let cursor = doc.remove(3);
        assert!(cursor < doc.bars.len());
    }
}
