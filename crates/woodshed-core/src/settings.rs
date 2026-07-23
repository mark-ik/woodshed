//! Canonical durable application settings.
//!
//! Subsections match the product's Settings routes. Each subsection is
//! flattened for serialization so sessions written before `AppSettings`
//! continue to read the same `theme`, `tuning_idx`, `bpm`, and related keys.

use serde::{Deserialize, Serialize};

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
        Self { theme: "Slate".into() }
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RelatedSettings {
    pub use_history: bool,
    pub show_neighborhood: bool,
    pub dismissed_ids: Vec<String>,
}

impl Default for RelatedSettings {
    fn default() -> Self {
        Self {
            use_history: true,
            show_neighborhood: true,
            dismissed_ids: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct StageSettings {
    pub related: RelatedSettings,
    /// Show the Set's authored order as `Next` edges in its graph projection.
    pub show_set_sequence_edges: bool,
}

impl Default for StageSettings {
    fn default() -> Self {
        Self {
            related: RelatedSettings::default(),
            show_set_sequence_edges: true,
        }
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

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    #[serde(rename = "settings_page")]
    pub page: SettingsPage,
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
