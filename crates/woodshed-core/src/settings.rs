//! Canonical durable application settings.
//!
//! Subsections match the product's Settings routes. Each subsection is
//! flattened for serialization so sessions written before `AppSettings`
//! continue to read the same `theme`, `tuning_idx`, `bpm`, and related keys.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use woodshedding::rehearsal::SetGraphEdgeKind;

use crate::arrangement::GraphArrangement;

pub const DEFAULT_SET_GRAPH_WIDTH: u32 = 520;
pub const DEFAULT_SET_GRAPH_HEIGHT: u32 = 260;
pub const MIN_SET_GRAPH_WIDTH: u32 = 360;
pub const MIN_SET_GRAPH_HEIGHT: u32 = 240;
pub const MAX_SET_GRAPH_WIDTH: u32 = 960;
pub const MAX_SET_GRAPH_HEIGHT: u32 = 720;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SettingsPage {
    #[default]
    General,
    Appearance,
    Instrument,
    Tuning,
    Stage,
    Fretboard,
    Metronome,
    Tuner,
    Rehearsal,
    Looper,
    AudioMidi,
    Accessibility,
}

impl SettingsPage {
    pub const ALL: [(SettingsPage, &'static str); 12] = [
        (Self::General, "General"),
        (Self::Appearance, "Appearance"),
        (Self::Instrument, "Instrument"),
        (Self::Tuning, "Tuning"),
        (Self::Stage, "Stage"),
        (Self::Fretboard, "Fretboard"),
        (Self::Metronome, "Metronome"),
        (Self::Tuner, "Tuner"),
        (Self::Rehearsal, "Rehearsal"),
        (Self::Looper, "Looper"),
        (Self::AudioMidi, "Audio and MIDI"),
        (Self::Accessibility, "Accessibility"),
    ];
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppearanceSettings {
    pub theme: String,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            theme: "Slate".into(),
        }
    }
}

/// Instrument-family preferences gain fields when the picker lands. Keeping
/// the typed home now prevents them being scattered into host state later.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstrumentSettings {}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TuningSettings {
    pub tuning_idx: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RelatedGraphScope {
    Mere,
    #[default]
    Selection,
}

impl RelatedGraphScope {
    pub fn label(self) -> &'static str {
        match self {
            Self::Mere => "Whole mere",
            Self::Selection => "Selection",
        }
    }

    pub fn toggle(self) -> Self {
        match self {
            Self::Mere => Self::Selection,
            Self::Selection => Self::Mere,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RelatedSettings {
    pub use_history: bool,
    pub show_neighborhood: bool,
    pub dismissed_ids: Vec<String>,
    /// Whether the swatch shows the joined whole mere or an N-depth
    /// neighborhood around the current material.
    pub graph_scope: RelatedGraphScope,
    pub relation_depth: u8,
    pub arrangement: GraphArrangement,
}

impl Default for RelatedSettings {
    fn default() -> Self {
        Self {
            use_history: true,
            show_neighborhood: true,
            dismissed_ids: Vec::new(),
            graph_scope: RelatedGraphScope::Selection,
            relation_depth: 1,
            arrangement: GraphArrangement::Radial,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct StageSettings {
    pub related: RelatedSettings,
    /// Arrangement for the staged Set surface. The same ten-item catalog is
    /// available to the joined-mere swatch above.
    pub set_arrangement: GraphArrangement,
    /// User-sized expanded Set canvas in logical pixels. The direct resize
    /// handle and the Settings reset both write these durable values.
    pub set_graph_width: u32,
    pub set_graph_height: u32,
    /// Which projected relation families the Set graph draws. A set rather
    /// than a flag per family, so the harmonic, evidence, and suggestion
    /// layers join it by becoming members. Visibility is a view state: hiding
    /// a family draws fewer edges and changes no Set truth.
    pub visible_set_relations: BTreeSet<SetGraphEdgeKind>,
    /// Legacy key (sessions written 2026-07-21..24 carried one boolean for
    /// all sequence edges). Read once by
    /// [`StageSettings::adopt_legacy_relation_visibility`] and dropped on the
    /// next save.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_set_sequence_edges: Option<bool>,
}

impl Default for StageSettings {
    fn default() -> Self {
        Self {
            related: RelatedSettings::default(),
            set_arrangement: GraphArrangement::Snake,
            set_graph_width: DEFAULT_SET_GRAPH_WIDTH,
            set_graph_height: DEFAULT_SET_GRAPH_HEIGHT,
            visible_set_relations: SetGraphEdgeKind::ALL.into_iter().collect(),
            show_set_sequence_edges: None,
        }
    }
}

impl StageSettings {
    pub fn set_graph_size(&self) -> (u32, u32) {
        (
            self.set_graph_width
                .clamp(MIN_SET_GRAPH_WIDTH, MAX_SET_GRAPH_WIDTH),
            self.set_graph_height
                .clamp(MIN_SET_GRAPH_HEIGHT, MAX_SET_GRAPH_HEIGHT),
        )
    }

    pub fn resize_set_graph(&mut self, width: u32, height: u32) {
        self.set_graph_width = width.clamp(MIN_SET_GRAPH_WIDTH, MAX_SET_GRAPH_WIDTH);
        self.set_graph_height = height.clamp(MIN_SET_GRAPH_HEIGHT, MAX_SET_GRAPH_HEIGHT);
    }

    pub fn reset_set_graph_size(&mut self) {
        self.set_graph_width = DEFAULT_SET_GRAPH_WIDTH;
        self.set_graph_height = DEFAULT_SET_GRAPH_HEIGHT;
    }

    /// Fold the pre-P4a boolean into the relation set. Idempotent: the legacy
    /// field is taken, so a later save writes only the set.
    pub fn adopt_legacy_relation_visibility(&mut self) {
        let Some(shown) = self.show_set_sequence_edges.take() else {
            return;
        };
        if shown {
            self.visible_set_relations.insert(SetGraphEdgeKind::Next);
        } else {
            self.visible_set_relations.remove(&SetGraphEdgeKind::Next);
        }
    }

    pub fn shows_relation(&self, kind: SetGraphEdgeKind) -> bool {
        self.visible_set_relations.contains(&kind)
    }

    pub fn toggle_relation(&mut self, kind: SetGraphEdgeKind) {
        if !self.visible_set_relations.remove(&kind) {
            self.visible_set_relations.insert(kind);
        }
    }

    /// The visible families, in a stable order, for filtering a projection.
    pub fn visible_relations(&self) -> Vec<SetGraphEdgeKind> {
        self.visible_set_relations.iter().copied().collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FretboardSettings {
    pub board_layout: String,
    /// How the painted board draws its note markers: "Sharp" or "Rounded".
    pub marker_style: String,
    /// The neck window's first fret — the board shows `neck_start ..= neck_end`.
    /// 0 includes the open strings and the nut.
    #[serde(default)]
    pub neck_start: u8,
    /// The neck window's last fret, or `None` for the instrument's full standard
    /// neck ([`woodshedding::tuning::Instrument::standard_fret_count`]). A
    /// per-instrument default that a player can override with any range
    /// (0-12, 8-16, 2-22).
    #[serde(default)]
    pub neck_end: Option<u8>,
    /// How the neck is laid out: "Horizontal" (default) or "Vertical".
    #[serde(default = "default_orientation")]
    pub orientation: String,
}

fn default_orientation() -> String {
    "Horizontal".into()
}

impl Default for FretboardSettings {
    fn default() -> Self {
        Self {
            board_layout: "Two pane".into(),
            marker_style: "Sharp".into(),
            neck_start: 0,
            // The instrument's own full neck, until a player picks a range.
            neck_end: None,
            orientation: default_orientation(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MetronomeSettings {
    pub bpm: f32,
}

impl Default for MetronomeSettings {
    fn default() -> Self {
        Self { bpm: 120.0 }
    }
}

// These routes do not own durable knobs yet. Empty typed subsections make that
// boundary explicit without inventing settings the runtimes do not implement.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TunerSettings {}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RehearsalSettings {}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LooperSettings {}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioMidiSettings {}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AccessibilitySettings {
    /// Suppress non-essential motion (the CSS hover/active fades). Functional
    /// feedback like the stepping run stays.
    #[serde(default)]
    pub reduce_motion: bool,
    /// Distinguish the root note by an outline, not color alone, for colorblind
    /// players.
    #[serde(default)]
    pub distinguish_root: bool,
    /// UI text size: "Normal", "Large", or "Larger".
    #[serde(default = "default_text_scale")]
    pub text_scale: String,
}

fn default_text_scale() -> String {
    "Normal".into()
}

impl Default for AccessibilitySettings {
    fn default() -> Self {
        Self {
            reduce_motion: false,
            distinguish_root: false,
            text_scale: default_text_scale(),
        }
    }
}

/// Desktop window placement remembered as an application preference.
///
/// This stays free of host types so the portable core can serialize it while
/// the desktop adapter converts at its boundary.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowSettings {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub maximized: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    #[serde(rename = "settings_page")]
    pub page: SettingsPage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window: Option<WindowSettings>,
    #[serde(flatten)]
    pub appearance: AppearanceSettings,
    #[serde(flatten)]
    pub instrument: InstrumentSettings,
    #[serde(flatten)]
    pub tuning: TuningSettings,
    #[serde(flatten)]
    pub stage: StageSettings,
    #[serde(flatten)]
    pub fretboard: FretboardSettings,
    #[serde(flatten)]
    pub metronome: MetronomeSettings,
    #[serde(flatten)]
    pub tuner: TunerSettings,
    #[serde(flatten)]
    pub rehearsal: RehearsalSettings,
    #[serde(flatten)]
    pub looper: LooperSettings,
    #[serde(flatten)]
    pub audio_midi: AudioMidiSettings,
    #[serde(flatten)]
    pub accessibility: AccessibilitySettings,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_geometry_is_optional_and_round_trips() {
        let default_json = serde_json::to_value(AppSettings::default()).unwrap();
        assert!(default_json.get("window").is_none());

        let mut settings = AppSettings::default();
        settings.window = Some(WindowSettings {
            x: 120.0,
            y: 80.0,
            width: 900.0,
            height: 640.0,
            maximized: true,
        });
        let json = serde_json::to_string(&settings).unwrap();
        assert_eq!(
            serde_json::from_str::<AppSettings>(&json).unwrap(),
            settings
        );
    }

    #[test]
    fn stage_canvas_size_migrates_and_clamps_at_the_settings_boundary() {
        let mut settings = serde_json::from_str::<AppSettings>("{}").unwrap();
        assert_eq!(
            settings.stage.set_graph_size(),
            (DEFAULT_SET_GRAPH_WIDTH, DEFAULT_SET_GRAPH_HEIGHT)
        );

        settings.stage.resize_set_graph(1, u32::MAX);
        assert_eq!(
            settings.stage.set_graph_size(),
            (MIN_SET_GRAPH_WIDTH, MAX_SET_GRAPH_HEIGHT)
        );
        settings.stage.reset_set_graph_size();
        assert_eq!(
            settings.stage.set_graph_size(),
            (DEFAULT_SET_GRAPH_WIDTH, DEFAULT_SET_GRAPH_HEIGHT)
        );
    }
}
