//! The storage seam (serval-host plan W0.2) and the persisted session.
//!
//! The core describes what survives a restart as one serde struct; a host
//! supplies a [`Storage`] that moves the serialized form — filesystem on
//! desktop, OPFS in the browser. Every field is `#[serde(default)]`-safe
//! so old files keep loading as the struct grows.

use serde::{Deserialize, Serialize};

use crate::arpeggio::ArpeggioDirection;
use crate::{Lens, StageState};

/// The host-supplied persistence realization.
pub trait Storage {
    /// The serialized session from the previous run, if any.
    fn load(&self) -> Option<String>;
    /// Persist the serialized session. Failures should be logged by the
    /// implementation, not surfaced as app errors — a broken disk must
    /// not strand practice.
    fn save(&self, contents: &str);
}

/// The top-level app tab. Stage carries the lens strip; the others
/// migrate through S4.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Tab {
    #[default]
    Stage,
    Practice,
    Song,
    Rehearsal,
    Settings,
}

impl Tab {
    pub const ALL: [Tab; 5] = [
        Tab::Stage,
        Tab::Practice,
        Tab::Song,
        Tab::Rehearsal,
        Tab::Settings,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Tab::Stage => "Stage",
            Tab::Practice => "Practice",
            Tab::Song => "Song",
            Tab::Rehearsal => "Rehearsal",
            Tab::Settings => "Settings",
        }
    }
}

/// Everything that survives a restart. Mirrors woodshed-xilem's persisted
/// subset: selections and dials persist, transport cursors and playing
/// flags do not.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PersistedSession {
    pub tab: Tab,
    pub lens: Lens,
    pub tuning_idx: usize,
    pub root_idx: usize,
    pub scale_idx: usize,
    pub chord_idx: usize,
    pub arpeggio_idx: usize,
    pub arpeggio_position_idx: usize,
    pub arpeggio_direction: ArpeggioDirection,
    pub arpeggio_inversion: u8,
    pub progression_idx: Option<usize>,
    pub progression_expanded: usize,
    pub exercise_idx: usize,
    pub exercise_starting_fret: u8,
    pub bpm: f32,
    /// Theme name, opaque to the core (the view layer owns the theme
    /// vocabulary).
    pub theme: String,
    /// The rehearsal set (cards + cursor + loop mode).
    pub set: woodshedding::rehearsal::Set,
}

impl Default for PersistedSession {
    fn default() -> Self {
        Self::capture(
            &StageState::new(),
            Tab::Stage,
            120.0,
            "Slate",
            &woodshedding::rehearsal::Set::default(),
        )
    }
}

impl PersistedSession {
    /// Snapshot the persistable subset of the app state.
    pub fn capture(
        stage: &StageState,
        tab: Tab,
        bpm: f32,
        theme: &str,
        set: &woodshedding::rehearsal::Set,
    ) -> Self {
        Self {
            set: set.clone(),
            tab,
            lens: stage.lens,
            tuning_idx: stage.tuning_idx,
            root_idx: stage.root_idx,
            scale_idx: stage.scale_idx,
            chord_idx: stage.chord_idx,
            arpeggio_idx: stage.arpeggio_idx,
            arpeggio_position_idx: stage.arpeggio_position_idx,
            arpeggio_direction: stage.arpeggio_direction,
            arpeggio_inversion: stage.arpeggio_inversion,
            progression_idx: stage.progression_idx,
            progression_expanded: stage.progression_expanded,
            exercise_idx: stage.exercise_idx,
            exercise_starting_fret: stage.exercise_starting_fret,
            bpm,
            theme: theme.to_string(),
        }
    }

    /// Restore the persisted subset onto a fresh state. Indices route
    /// through the clamping setters so a session written against a larger
    /// future catalog degrades instead of panicking.
    pub fn restore(&self, stage: &mut StageState) {
        stage.set_lens(self.lens);
        stage.set_tuning(self.tuning_idx);
        stage.set_root(self.root_idx);
        stage.select_scale(self.scale_idx);
        stage.select_chord(self.chord_idx);
        stage.select_arpeggio(self.arpeggio_idx);
        stage.arpeggio_position_idx = self.arpeggio_position_idx;
        stage.arpeggio_direction = self.arpeggio_direction;
        stage.arpeggio_inversion = self.arpeggio_inversion;
        if let Some(i) = self.progression_idx {
            stage.select_progression(i);
            stage.progression_expand(self.progression_expanded);
        }
        stage.select_exercise(self.exercise_idx);
        stage.exercise_starting_fret = self.exercise_starting_fret;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_round_trips_through_json() {
        let mut stage = StageState::new();
        stage.set_lens(Lens::Arpeggios);
        stage.set_root(3);
        stage.select_arpeggio(5);
        stage.arpeggio_inversion = 2;
        stage.select_progression(1);
        let mut set = woodshedding::rehearsal::Set::default();
        set.push(stage.card_from_lens().expect("arpeggio card"));
        let snap = PersistedSession::capture(&stage, Tab::Settings, 96.0, "Ember", &set);
        let json = serde_json::to_string(&snap).unwrap();
        let back: PersistedSession = serde_json::from_str(&json).unwrap();
        let mut restored = StageState::new();
        back.restore(&mut restored);
        assert_eq!(restored.lens, Lens::Arpeggios);
        assert_eq!(restored.root_idx, 3);
        assert_eq!(restored.arpeggio_idx, 5);
        assert_eq!(restored.arpeggio_inversion, 2);
        assert_eq!(restored.progression_idx, Some(1));
        assert_eq!(back.tab, Tab::Settings);
        assert_eq!(back.bpm, 96.0);
        assert_eq!(back.theme, "Ember");
        assert_eq!(back.set.cards.len(), 1, "the rehearsal set round-trips");
        assert_eq!(back.set.cards[0].label, "Csus4 arpeggio");
    }

    #[test]
    fn unknown_fields_and_missing_fields_tolerated() {
        let back: PersistedSession =
            serde_json::from_str(r#"{"lens":"Chords","bpm":88.0}"#).unwrap();
        assert_eq!(back.lens, Lens::Chords);
        assert_eq!(back.bpm, 88.0);
        assert_eq!(back.tab, Tab::Stage, "missing fields default");
        // Out-of-range indices clamp through the setters.
        let huge: PersistedSession =
            serde_json::from_str(r#"{"scale_idx":99999}"#).unwrap();
        let mut s = StageState::new();
        huge.restore(&mut s);
        assert!(s.scale_idx < s.scales().len());
    }
}
