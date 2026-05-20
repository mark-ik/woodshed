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

use crate::theme::ThemeMode;
use crate::{ChromaticPc, ClickPattern, AccentMode, LabelMode, SidebarVisibility, Tab};

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

    // Scales tab
    pub scale_idx: usize,
    pub scale_root_pc: ChromaticPc,
    pub scale_label_mode: LabelMode,

    // Chords tab
    pub chord_idx: usize,
    pub chord_root_pc: ChromaticPc,
    pub chord_label_mode: LabelMode,
    pub chord_show_voicing: bool,
    pub chord_voicing_idx: usize,

    // Progressions tab
    pub progression_idx: Option<usize>,
    pub progression_key_pc: ChromaticPc,
    pub progression_overlay_mode: bool,

    // Exercises tab
    pub exercise_idx: usize,
    pub exercise_starting_fret: u8,
    pub exercise_bpm: f32,

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
            active_instrument: instrument_to_str(Instrument::Guitar).to_string(),
            tuning_name: None,
            sidebars: SidebarVisibility::default(),
            scale_idx: 0,
            scale_root_pc: ChromaticPc::default(),
            scale_label_mode: LabelMode::default(),
            chord_idx: 0,
            chord_root_pc: ChromaticPc::default(),
            chord_label_mode: LabelMode::default(),
            chord_show_voicing: false,
            chord_voicing_idx: 0,
            progression_idx: None,
            progression_key_pc: ChromaticPc::default(),
            progression_overlay_mode: false,
            exercise_idx: 0,
            exercise_starting_fret: 0,
            exercise_bpm: 60.0,
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
