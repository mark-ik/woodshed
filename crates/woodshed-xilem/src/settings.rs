// Copyright 2026 the Woodshed Authors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Persistent user settings.
//!
//! A small, explicit subset of [`AppState`] that survives across
//! restarts: which tab the user was on, instrument/tuning, picker
//! indices, tempos, label modes, sidebar collapsed state, tuner
//! detector + threshold. The runtime-only fields — audio engines,
//! input bundle, tuner snapshot, text-input buffers, combobox
//! open-state, transient progression voicing arrays — deliberately
//! don't round-trip; those are reconstructed from defaults at
//! startup.
//!
//! Stored as JSON under the platform's user-config directory:
//!
//!   - Windows:  `%APPDATA%\Woodshed\Woodshed\config\state.json`
//!   - macOS:    `~/Library/Application Support/dev.woodshed.Woodshed/state.json`
//!   - Linux:    `~/.config/woodshed/state.json`
//!
//! Load failure (file missing, parse error, type mismatch after a
//! field rename) is non-fatal — the app falls back to defaults and
//! logs the cause. Saves are best-effort: a write failure logs and
//! moves on rather than blocking the UI thread.

use std::fs;
use std::path::PathBuf;

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use woodshed_audio::DetectorKind;
use woodshedding::tuning::Instrument;

use audio_widgets::theme::{Seeds, color_from_hex, color_to_hex};

use crate::theme::ThemeMode;
use crate::{
    ChromaticPc, ClickPattern, AccentMode, LabelMode, Set, SidebarVisibility, SurfaceModule,
    Tab, default_surface,
};

/// Serde default for [`Settings::fret_span`] — the full 12-fret neck.
fn default_fret_span() -> u8 {
    12
}

/// Serde default for [`Settings::arpeggio_bpm`].
fn default_arpeggio_bpm() -> f32 {
    80.0
}

/// Serde default for additive bool fields that should default on.
fn default_true() -> bool {
    true
}

/// One step of a user exercise: which string (0-based, low → high),
/// fret, and fretting finger (1–4; 0 = unspecified).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExStepSpec {
    pub string: u8,
    pub fret: u8,
    pub finger: u8,
}

/// A user-authored exercise: a name + a fixed recorded step sequence.
/// (Catalog exercises are *generators*; user ones are explicit steps,
/// since a generator closure can't be serialized.)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserExerciseDef {
    pub name: String,
    pub steps: Vec<ExStepSpec>,
}

/// One chord in a user progression: scale degree (1–7), an alteration
/// index (into `DegreeAlteration::ALL`), and a quality index (into
/// `RoleQuality::ALL`). Stored as small ints so the theory enums don't
/// need serde.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProgRoleSpec {
    pub degree: u8,
    pub alteration: u8,
    pub quality: u8,
}

/// A user-authored chord progression: a name + an ordered list of
/// degree-based roles (key-agnostic, like the catalog progressions).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserProgressionDef {
    pub name: String,
    pub roles: Vec<ProgRoleSpec>,
}

/// A user-authored tuning, persisted as open-string MIDI note numbers
/// (low → high). `name` doubles as its id. Built-in tunings live in the
/// theory crate's catalog; these are the user's own. The string-count is
/// implicit in `midi.len()`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserTuningDef {
    pub name: String,
    /// Instrument string-adapter (see `instrument_to_str`).
    pub instrument: String,
    /// Open-string MIDI notes, low string → high.
    pub midi: Vec<i32>,
}

/// A user-authored theme, persisted as hex seed strings (peniko `Color`
/// isn't serde-friendly, and hex round-trips legibly in the JSON). The
/// `name` doubles as its id — selection in [`Settings::active_user_theme`]
/// refers to it by name. Built-in themes are not stored here; they live
/// in code as [`ThemeMode`] variants.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserThemeDef {
    pub name: String,
    pub primary: String,
    pub secondary: String,
    pub tertiary: String,
    pub neutral: String,
    pub success: String,
    pub danger: String,
    pub dark: bool,
    /// Optional explicit heading / body text colors (hex). `None`
    /// derives them from `neutral`. Additive (`#[serde(default)]`) so
    /// older saved themes without them still load.
    #[serde(default)]
    pub text_header: Option<String>,
    #[serde(default)]
    pub text_body: Option<String>,
}

impl UserThemeDef {
    /// Build from runtime [`Seeds`] (e.g. cloning the active built-in
    /// as the starting point for a new custom theme).
    pub fn from_seeds(name: impl Into<String>, s: &Seeds) -> Self {
        Self {
            name: name.into(),
            primary: color_to_hex(s.primary),
            secondary: color_to_hex(s.secondary),
            tertiary: color_to_hex(s.tertiary),
            neutral: color_to_hex(s.neutral),
            success: color_to_hex(s.success),
            danger: color_to_hex(s.danger),
            dark: s.dark,
            text_header: s.text_header.map(color_to_hex),
            text_body: s.text_body.map(color_to_hex),
        }
    }

    /// Resolve to runtime [`Seeds`]. Any unparseable hex falls back to
    /// mid-grey so a hand-corrupted field can't crash theming.
    pub fn to_seeds(&self) -> Seeds {
        let c = |h: &str| color_from_hex(h).unwrap_or(masonry::peniko::Color::from_rgb8(0x80, 0x80, 0x80));
        Seeds {
            primary: c(&self.primary),
            secondary: c(&self.secondary),
            tertiary: c(&self.tertiary),
            neutral: c(&self.neutral),
            text_header: self.text_header.as_deref().and_then(color_from_hex),
            text_body: self.text_body.as_deref().and_then(color_from_hex),
            success: c(&self.success),
            danger: c(&self.danger),
            dark: self.dark,
        }
    }
}

/// String adapter for [`Instrument`] — the upstream enum doesn't
/// derive serde and we don't want to add a feature flag to the
/// theory crate just for one persistence field. Serializes to the
/// variant name as written; unknown strings round-trip to
/// [`Instrument::Guitar`].
pub fn instrument_to_str(i: Instrument) -> &'static str {
    match i {
        Instrument::Guitar => "Guitar",
        Instrument::Bass => "Bass",
        Instrument::Ukulele => "Ukulele",
        Instrument::Banjo => "Banjo",
        Instrument::Mandolin => "Mandolin",
        Instrument::Other => "Other",
    }
}

pub fn instrument_from_str(s: &str) -> Instrument {
    match s {
        "Guitar" => Instrument::Guitar,
        "Bass" => Instrument::Bass,
        "Ukulele" => Instrument::Ukulele,
        "Banjo" => Instrument::Banjo,
        "Mandolin" => Instrument::Mandolin,
        "Other" => Instrument::Other,
        _ => Instrument::Guitar,
    }
}

/// String adapter for [`DetectorKind`] — same rationale as
/// `instrument_*`. Unknown strings round-trip to FFT.
pub fn detector_to_str(d: DetectorKind) -> &'static str {
    match d {
        DetectorKind::Fft => "Fft",
        DetectorKind::Cepstrum => "Cepstrum",
        DetectorKind::McLeod => "McLeod",
    }
}

pub fn detector_from_str(s: &str) -> DetectorKind {
    match s {
        "Fft" => DetectorKind::Fft,
        "Cepstrum" => DetectorKind::Cepstrum,
        "McLeod" => DetectorKind::McLeod,
        _ => DetectorKind::Fft,
    }
}

/// User-facing durable settings. Every field is `#[serde(default)]`
/// so a new field added between releases doesn't invalidate an
/// existing config file — old files simply pick up the default for
/// any unknown field.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub tab: Tab,
    pub theme_mode: ThemeMode,
    /// User-authored themes (additive; built-ins live in code).
    /// Fraction (0..1) of the fretboard↔info pane split, shared across
    /// all fretboard tabs. `0.0` (the default) clamps to the minimum
    /// fretboard width — a narrow neck that "looks like the instrument."
    #[serde(default)]
    pub split_ratio: f64,
    /// Visible fretboard span (frets shown from the nut), 4..=12. The
    /// fretboard's scope dial; shared across the fretboard tabs.
    #[serde(default = "default_fret_span")]
    pub fret_span: u8,
    /// First visible fret of the windowed display (0 = nut). Additive;
    /// older saves default to nut-anchored.
    #[serde(default)]
    pub fret_start: u8,
    /// The instrument-surface composition (mounted widget modules, in
    /// order, with visibility + size weight). Additive: an older save
    /// without it loads the fretboard-only default. Sanitized on load
    /// (see `sanitize_surface`) to keep the "exactly one Fretboard"
    /// invariant.
    #[serde(default = "default_surface")]
    pub surface: Vec<SurfaceModule>,
    pub user_themes: Vec<UserThemeDef>,
    /// User-authored tunings (additive; built-ins live in the catalog).
    #[serde(default)]
    pub user_tunings: Vec<UserTuningDef>,
    /// User-authored chord progressions (additive).
    #[serde(default)]
    pub user_progressions: Vec<UserProgressionDef>,
    /// User-authored exercises (additive).
    #[serde(default)]
    pub user_exercises: Vec<UserExerciseDef>,
    /// The set (redesign U1). Additive: older saves (which had a
    /// differently-shaped `rehearsal` field) load an empty set.
    #[serde(default)]
    pub set: Set,
    /// Name of the active user theme, or `None` to use the built-in
    /// `theme_mode`. A name that no longer resolves falls back to the
    /// built-in on load.
    pub active_user_theme: Option<String>,

    // Shared / global. `active_instrument` is a string adapter for
    // [`Instrument`] (see `instrument_to_str` / `_from_str`) so we
    // don't have to add a serde feature flag to the theory crate.
    pub active_instrument: String,
    /// Active tuning, looked up by name in the catalog on load. We
    /// persist the *name* rather than the full pitch sequence so a
    /// future catalog edit (e.g. respelling a string) propagates to
    /// already-saved sessions. `None` means "default tuning for the
    /// instrument" — same path the cold-start uses.
    pub tuning_name: Option<String>,
    pub sidebars: SidebarVisibility,
    /// Shared musical root/key — one current pitch class the theory
    /// lenses (Scales / Chords / Progressions) all read, so they're
    /// coherent views of one musical moment. (Was three separate
    /// per-tab roots; unified 2026-05-21.)
    #[serde(default, alias = "scale_root_pc")]
    pub root: ChromaticPc,

    // Scales tab
    pub scale_idx: usize,
    pub scale_label_mode: LabelMode,

    // Chords tab
    pub chord_idx: usize,
    pub chord_label_mode: LabelMode,
    pub chord_show_voicing: bool,
    pub chord_voicing_idx: usize,

    // Progressions tab
    pub progression_idx: Option<usize>,
    pub progression_overlay_mode: bool,

    // Exercises tab
    pub exercise_idx: usize,
    pub exercise_starting_fret: u8,
    pub exercise_bpm: f32,

    // Arpeggios lens. `arpeggio_idx` indexes the chord catalog (the
    // quality). Additive — older saves default these.
    #[serde(default)]
    pub arpeggio_idx: usize,
    #[serde(default)]
    pub arpeggio_position_idx: usize,
    #[serde(default = "default_arpeggio_bpm")]
    pub arpeggio_bpm: f32,
    #[serde(default)]
    pub arpeggio_direction: crate::ArpeggioDirection,
    #[serde(default)]
    pub arpeggio_label: crate::ArpeggioLabel,
    #[serde(default)]
    pub arpeggio_inversion: u8,
    /// Whether the arpeggio/exercise step-through sounds each note.
    #[serde(default = "default_true")]
    pub transport_sound: bool,

    // Metronome tab
    pub bpm: f32,
    pub metronome_time_sig_num: u8,
    pub metronome_click: ClickPattern,
    pub metronome_accent: AccentMode,

    // Practice tab
    pub practice_selected_set: usize,
    pub practice_bpm: f32,
    pub practice_bars_per_item: u8,

    // Tuner tab. Same string-adapter rationale as `active_instrument`.
    pub tuner_threshold: f64,
    pub tuner_detector: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            tab: Tab::default(),
            theme_mode: ThemeMode::default(),
            split_ratio: 0.0,
            fret_span: default_fret_span(),
            fret_start: 0,
            surface: default_surface(),
            user_themes: Vec::new(),
            user_tunings: Vec::new(),
            user_progressions: Vec::new(),
            user_exercises: Vec::new(),
            set: Set::default(),
            active_user_theme: None,
            active_instrument: instrument_to_str(Instrument::Guitar).to_string(),
            tuning_name: None,
            sidebars: SidebarVisibility::default(),
            root: ChromaticPc::default(),
            scale_idx: 0,
            scale_label_mode: LabelMode::default(),
            chord_idx: 0,
            chord_label_mode: LabelMode::default(),
            chord_show_voicing: false,
            chord_voicing_idx: 0,
            progression_idx: None,
            progression_overlay_mode: false,
            exercise_idx: 0,
            exercise_starting_fret: 0,
            exercise_bpm: 60.0,
            arpeggio_idx: 0,
            arpeggio_position_idx: 0,
            arpeggio_bpm: default_arpeggio_bpm(),
            arpeggio_direction: crate::ArpeggioDirection::default(),
            arpeggio_label: crate::ArpeggioLabel::default(),
            arpeggio_inversion: 0,
            transport_sound: true,
            bpm: 100.0,
            metronome_time_sig_num: 4,
            metronome_click: ClickPattern::default(),
            metronome_accent: AccentMode::default(),
            practice_selected_set: 0,
            practice_bpm: 60.0,
            practice_bars_per_item: 4,
            tuner_threshold: 0.0010,
            tuner_detector: detector_to_str(DetectorKind::Fft).to_string(),
        }
    }
}

/// Resolve the on-disk path for `state.json`. Returns `None` if the
/// platform doesn't expose a user config dir (extremely unusual, but
/// possible on stripped-down embedded targets) — the caller treats
/// `None` as "persistence disabled" and runs purely in-memory.
pub fn state_path() -> Option<PathBuf> {
    let dirs = ProjectDirs::from("dev", "Woodshed", "Woodshed")?;
    Some(dirs.config_dir().join("state.json"))
}

impl Settings {
    /// Load from disk. Missing file → `Default::default()` silently
    /// (first run). Parse / type-mismatch failures log and also fall
    /// back to defaults rather than crashing the app — a corrupt
    /// config shouldn't strand the user.
    pub fn load() -> Self {
        let Some(path) = state_path() else {
            return Self::default();
        };
        let bytes = match fs::read(&path) {
            Ok(b) => b,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Self::default();
            }
            Err(err) => {
                eprintln!(
                    "Couldn't read settings at {}: {err}",
                    path.display()
                );
                return Self::default();
            }
        };
        match serde_json::from_slice::<Self>(&bytes) {
            Ok(settings) => settings,
            Err(err) => {
                eprintln!(
                    "Couldn't parse settings at {} (falling back to \
                     defaults): {err}",
                    path.display()
                );
                Self::default()
            }
        }
    }

    /// Best-effort save. Creates the config directory if it doesn't
    /// exist; write failures log and return. The UI never blocks on
    /// this — call it on quit or whenever the user explicitly asks.
    pub fn save(&self) {
        let Some(path) = state_path() else {
            return;
        };
        if let Some(parent) = path.parent() {
            if let Err(err) = fs::create_dir_all(parent) {
                eprintln!(
                    "Couldn't create settings dir {}: {err}",
                    parent.display()
                );
                return;
            }
        }
        let json = match serde_json::to_vec_pretty(self) {
            Ok(b) => b,
            Err(err) => {
                eprintln!("Couldn't serialize settings: {err}");
                return;
            }
        };
        if let Err(err) = fs::write(&path, &json) {
            eprintln!(
                "Couldn't write settings at {}: {err}",
                path.display()
            );
        }
    }
}
