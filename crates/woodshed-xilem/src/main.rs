// Copyright 2026 the Woodshed Authors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Xilem + Masonry frontend for Woodshed.
//!
//! Currently a migration scaffold — proves the framework works against
//! our pure crates (`woodshedding`, `woodshed-audio`) and establishes
//! the tab-and-state shape we'll fill in tab by tab.
//!
//! See `design_docs/2026-05-16_xilem_migration_plan.md` for the
//! roadmap from this scaffold to feature parity with the iced build.

use std::time::Duration;

use std::sync::Arc;

use masonry::core::{ArcStr, DefaultProperties};
use masonry::dpi::LogicalSize;
use masonry::peniko::Color;
use masonry::properties::types::{CrossAxisAlignment, MainAxisAlignment};
use masonry_winit::app::{EventLoop, EventLoopBuilder};
use tokio::time;
use winit::error::EventLoopError;
use xilem::core::fork;
use xilem::core::one_of::{OneOf2, OneOf3, OneOf4, OneOf9};
use xilem::style::Style;
use xilem::view::{
    AnyFlexChild, FlexExt, FlexSpacer, button, flex_col, flex_row, label, portal,
    progress_bar, prose, resize_observer, sized_box, slider, task_raw, text_button,
    text_input,
};
use xilem::{AppState as XilemAppState, WidgetView, WindowId, Xilem, window};

use woodshed_audio::{
    Bar as SongBar, ChordRef, DetectedNote, DetectedNoteName, DetectorKind, EngineHandle,
    InputEngine, InputEngineBuilder, LooperCaptureHandle, OnsetAnalyzer, OnsetHandle,
    PendingChange, SequencerEngine, SequencerPattern, Song, SongEngine, SongEngineHandle,
    Sound, Step, Subdivision, TimeSignature, Track, TunerHandle, TunerSnapshot,
};

use woodshedding::chord::{ChordFormula, catalog as chord_catalog};
use woodshedding::exercise::{
    Exercise, ExerciseDirection, ExerciseParams, catalog as exercise_catalog,
};
use woodshedding::fretboard::{
    BassConstraint, ChordVoicing, Fretboard, Position, StringPlay,
};
use woodshedding::practice::{PracticeItem, PracticeSet, catalog as practice_catalog};
use woodshedding::progression::{
    Progression, ProgressionChord, catalog as progression_catalog,
};
use woodshedding::pitch::{NoteName, Pitch};
use woodshedding::scale::{ScaleFormula, catalog as scale_catalog};
use woodshedding::tuning::{Instrument, Tuning, catalog as tuning_catalog};

mod combobox;
mod settings;
mod theme;
mod widgets;

use combobox::combobox;
use settings::Settings;
use theme::{
    Palette, SP_0, SP_1, SP_2, SP_3, SP_4, TS_2XL, TS_LG, TS_MD, TS_SM, TS_XL, TS_XS,
    mono_family,
};
use audio_widgets::waveform_view;
use widgets::{
    DiagramColors, SectionBand, SectionColors, chord_diagram_view, chord_lane_view,
    fretboard_view, section_lane_view,
};

/// The active tab. Matches the iced build's `Tab` shape so the
/// migration is a one-tab-at-a-time port, not a redesign.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
enum Tab {
    #[default]
    Scales,
    Chords,
    Tuner,
    Progressions,
    Exercises,
    Metronome,
    Practice,
    Song,
    Settings,
}

impl Tab {
    const ALL: [Self; 9] = [
        Self::Scales,
        Self::Chords,
        Self::Tuner,
        Self::Progressions,
        Self::Exercises,
        Self::Metronome,
        Self::Practice,
        Self::Song,
        Self::Settings,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Scales => "Scales",
            Self::Chords => "Chords",
            Self::Tuner => "Tuner",
            Self::Progressions => "Progressions",
            Self::Exercises => "Exercises",
            Self::Metronome => "Metronome",
            Self::Practice => "Practice",
            Self::Song => "Song",
            Self::Settings => "Settings",
        }
    }
}

/// Whether the given tab renders a collapsible browse-list sidebar.
/// Drives whether the header hamburger button appears — there's no
/// point offering "Hide list" on tabs that don't have one. Update
/// alongside [`SidebarVisibility`] as new tabs grow catalogs.
fn tab_has_list(tab: Tab) -> bool {
    matches!(
        tab,
        Tab::Scales
            | Tab::Chords
            | Tab::Tuner
            | Tab::Progressions
            | Tab::Exercises
    )
}

/// Per-tab catalog sidebar visibility. Decoupled from `Tab` so each
/// tab remembers its own collapsed state independently — collapsing
/// the Scales catalog sidebar shouldn't also collapse Progressions'.
///
/// Tabs without a sidebar (Metronome) simply have no field here;
/// the hamburger doesn't render for them (`tab_has_list`).
#[derive(Copy, Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
struct SidebarVisibility {
    scales: bool,
    chords: bool,
    tuner: bool,
    progressions: bool,
    exercises: bool,
}

impl SidebarVisibility {
    /// Is the sidebar for `tab` currently collapsed?
    fn is_collapsed(self, tab: Tab) -> bool {
        match tab {
            Tab::Scales => self.scales,
            Tab::Chords => self.chords,
            Tab::Tuner => self.tuner,
            Tab::Progressions => self.progressions,
            Tab::Exercises => self.exercises,
            _ => false,
        }
    }

    /// Flip the sidebar for `tab`. No-op for tabs that don't have a
    /// sidebar — the hamburger only renders on tabs that do, so this
    /// path shouldn't be hit, but be defensive.
    fn toggle(&mut self, tab: Tab) {
        match tab {
            Tab::Scales => self.scales = !self.scales,
            Tab::Chords => self.chords = !self.chords,
            Tab::Tuner => self.tuner = !self.tuner,
            Tab::Progressions => self.progressions = !self.progressions,
            Tab::Exercises => self.exercises = !self.exercises,
            _ => {}
        }
    }
}

/// What labels to print inside fretboard dots.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
enum LabelMode {
    None,
    #[default]
    Notes,
    Intervals,
}

impl LabelMode {
    fn next(self) -> Self {
        match self {
            Self::None => Self::Notes,
            Self::Notes => Self::Intervals,
            Self::Intervals => Self::None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::None => "No labels",
            Self::Notes => "Note names",
            Self::Intervals => "Intervals",
        }
    }
}

/// Click density for the metronome — every beat only, or every
/// subdivision. App-side enum because the audio crate doesn't model
/// it (it just takes the resulting [`SequencerPattern`]).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
enum ClickPattern {
    #[default]
    BeatOnly,
    EverySubdivision,
}

impl ClickPattern {
    fn next(self) -> Self {
        match self {
            Self::BeatOnly => Self::EverySubdivision,
            Self::EverySubdivision => Self::BeatOnly,
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::BeatOnly => "Click: beat only",
            Self::EverySubdivision => "Click: every note",
        }
    }
}

/// Accent strategy — which clicks get the louder/higher accent voice.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
enum AccentMode {
    #[default]
    Downbeat,
    EveryBeat,
    None,
}

impl AccentMode {
    fn next(self) -> Self {
        match self {
            Self::Downbeat => Self::EveryBeat,
            Self::EveryBeat => Self::None,
            Self::None => Self::Downbeat,
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Downbeat => "Accent: downbeat",
            Self::EveryBeat => "Accent: every beat",
            Self::None => "Accent: none",
        }
    }
}

/// Build a [`SequencerPattern`] from the metronome settings. One
/// "Click" track with steps marked active/empty per subdivision.
fn build_metronome_pattern(
    bpm: f32,
    num: u8,
    subdivision: Subdivision,
    click: ClickPattern,
    accent: AccentMode,
) -> SequencerPattern {
    let dpb = subdivision.divisions_per_beat as usize;
    let beats = num.max(1) as usize;
    let mut steps = Vec::with_capacity(beats * dpb);
    for beat in 0..beats {
        for div in 0..dpb {
            let on_beat = div == 0;
            let is_downbeat = beat == 0 && div == 0;
            let active = match click {
                ClickPattern::BeatOnly => on_beat,
                ClickPattern::EverySubdivision => true,
            };
            let accented = active
                && match accent {
                    AccentMode::Downbeat => is_downbeat,
                    AccentMode::EveryBeat => on_beat,
                    AccentMode::None => false,
                };
            steps.push(if active {
                Step::Active { accent: accented }
            } else {
                Step::Empty
            });
        }
    }
    SequencerPattern {
        bpm,
        time_signature: TimeSignature::new(num, 4),
        subdivision,
        tracks: vec![Track {
            name: "Click".to_string(),
            steps,
            sound: Sound::click(),
            muted: false,
        }],
    }
}

/// 12-tone chromatic pitch-class picker, sharp spelling. Mirrors the
/// iced build's ChromaticPc; living here so the Xilem crate doesn't
/// depend on the iced crate.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
enum ChromaticPc {
    #[default]
    C,
    CSharp,
    D,
    DSharp,
    E,
    F,
    FSharp,
    G,
    GSharp,
    A,
    ASharp,
    B,
}

impl ChromaticPc {
    const ALL: [Self; 12] = [
        Self::C, Self::CSharp, Self::D, Self::DSharp, Self::E, Self::F,
        Self::FSharp, Self::G, Self::GSharp, Self::A, Self::ASharp, Self::B,
    ];

    fn cycle(self, direction: i32) -> Self {
        let idx = Self::ALL.iter().position(|&p| p == self).unwrap_or(0) as i32;
        Self::ALL[((idx + direction).rem_euclid(12)) as usize]
    }

    /// Build from a chromatic pitch class (0 = C, 1 = C#, ..., 11 = B).
    fn from_pc(pc: u8) -> Self {
        Self::ALL[(pc as usize) % 12]
    }

    /// Convert to `pitch-detector`'s NoteName via the woodshed-audio
    /// re-export. Used to set the tuner's target-pitch hint.
    fn to_detected(self) -> DetectedNoteName {
        match self {
            Self::C => DetectedNoteName::C,
            Self::CSharp => DetectedNoteName::CSharp,
            Self::D => DetectedNoteName::D,
            Self::DSharp => DetectedNoteName::DSharp,
            Self::E => DetectedNoteName::E,
            Self::F => DetectedNoteName::F,
            Self::FSharp => DetectedNoteName::FSharp,
            Self::G => DetectedNoteName::G,
            Self::GSharp => DetectedNoteName::GSharp,
            Self::A => DetectedNoteName::A,
            Self::ASharp => DetectedNoteName::ASharp,
            Self::B => DetectedNoteName::B,
        }
    }

    fn to_pitch(self, octave: i8) -> Pitch {
        use woodshedding::pitch::Accidental;
        match self {
            Self::C => Pitch::natural(NoteName::C, octave),
            Self::CSharp => Pitch::new(NoteName::C, Accidental::Sharp, octave),
            Self::D => Pitch::natural(NoteName::D, octave),
            Self::DSharp => Pitch::new(NoteName::D, Accidental::Sharp, octave),
            Self::E => Pitch::natural(NoteName::E, octave),
            Self::F => Pitch::natural(NoteName::F, octave),
            Self::FSharp => Pitch::new(NoteName::F, Accidental::Sharp, octave),
            Self::G => Pitch::natural(NoteName::G, octave),
            Self::GSharp => Pitch::new(NoteName::G, Accidental::Sharp, octave),
            Self::A => Pitch::natural(NoteName::A, octave),
            Self::ASharp => Pitch::new(NoteName::A, Accidental::Sharp, octave),
            Self::B => Pitch::natural(NoteName::B, octave),
        }
    }

    fn display(self) -> String {
        let p = self.to_pitch(4);
        format!("{}{}", p.name, accidental_short(p.accidental))
    }

    fn note_name(self) -> &'static str {
        match self {
            Self::C | Self::CSharp => "C",
            Self::D | Self::DSharp => "D",
            Self::E => "E",
            Self::F | Self::FSharp => "F",
            Self::G | Self::GSharp => "G",
            Self::A | Self::ASharp => "A",
            Self::B => "B",
        }
    }

    fn accidental_str(self) -> &'static str {
        match self {
            Self::CSharp | Self::DSharp | Self::FSharp | Self::GSharp | Self::ASharp => "#",
            _ => "",
        }
    }
}

/// Top-level app state. Owned by Xilem, mutated by event handlers,
/// read by the view function on each diff pass.
///
/// Starts deliberately thin — fields get added as tabs are ported in.
/// Anything that lives in `woodshed-audio` (engines, handles) lives
/// in `AppState` once the audio integration step lands.
struct AppState {
    tab: Tab,
    // === Scales tab ===
    /// Index into the scale catalog. The iced version picks by name;
    /// using an index here is slightly more idiomatic with Xilem's
    /// reactive diffing model (cheap equality check).
    scale_idx: usize,
    /// Pitch-class for the Scales view's root note.
    scale_root_pc: ChromaticPc,
    /// What to label fretboard dots with on the Scales tab.
    scale_label_mode: LabelMode,

    // === Chords tab ===
    chord_idx: usize,
    chord_root_pc: ChromaticPc,
    chord_label_mode: LabelMode,
    /// When true, the chord fretboard shows only the selected voicing
    /// (5-6 specific string/fret positions). When false, every place
    /// the chord tones appear across the fretboard is shown — the
    /// "chord-tone scale."
    chord_show_voicing: bool,
    /// Index into the voicing list for the currently-selected
    /// (chord, root) pair. Recomputed on each render — Vec is small.
    chord_voicing_idx: usize,

    // === Progressions tab ===
    /// Index into the progression catalog; `None` = nothing selected
    /// (cold-start state, prompts the user to pick from the list).
    progression_idx: Option<usize>,
    /// Tonic pitch class for the progression. The progression's
    /// Roman numerals get materialized against this key.
    progression_key_pc: ChromaticPc,
    /// Index of the chord card currently expanded on the fretboard
    /// below the card row. `None` = no chord chosen yet.
    progression_expanded_chord: Option<usize>,
    /// When true, the main fretboard shows ALL of the progression's
    /// chord voicings simultaneously, each colored by its chord
    /// hue. When false (default), only the expanded chord's voicing
    /// renders — quieter view, faster to read a single shape.
    progression_overlay_mode: bool,
    /// Last-observed horizontal pixel width of the chord-cards
    /// container. Used to compute how many cards fit per row so they
    /// reflow to additional rows when the window narrows. Set via
    /// `resize_observer`; defaults to 0 until the first frame lands.
    progression_cards_panel_width: f64,
    /// One voicing index per chord in the currently-selected
    /// progression. Resized on progression-select; preserved across
    /// key changes so the user's voicing choices survive
    /// transposition.
    progression_voicing_idx: Vec<usize>,

    // === Exercises tab ===
    exercise_idx: usize,
    exercise_starting_fret: u8,
    /// Current position in the exercise's step sequence. Drives the
    /// step-through highlight + trail fade.
    exercise_step_idx: usize,
    /// True while auto-advancing through steps at `exercise_bpm`.
    exercise_playing: bool,
    /// Playback tempo for auto-play, in BPM. Separate from the
    /// metronome's BPM so the user can practice exercises at a
    /// slower-than-target tempo while leaving the metronome alone.
    exercise_bpm: f32,

    // === Metronome tab ===
    /// Tempo. Edited directly on the big readout (double-click) plus
    /// the slider / ± buttons — no separate text-input buffer needed
    /// since the readout itself is now the editable surface.
    bpm: f32,
    metronome_playing: bool,
    metronome_time_sig_num: u8,
    metronome_subdivision: Subdivision,
    metronome_click: ClickPattern,
    metronome_accent: AccentMode,

    // === Song tab ===
    /// Lazily-constructed song engine. None = not yet attempted;
    /// Some(Err(...)) = construction failed (device unavailable).
    /// Stored as `(engine, handle)` so the cpal output stream stays
    /// alive — dropping the engine would kill it.
    song_engine: Option<Result<(SongEngine, SongEngineHandle), String>>,
    /// Cached song shape for UI rendering. Refreshed from the engine
    /// on each SongTick (and immediately after edits via
    /// `refresh_song_view`).
    song_view: Song,
    /// Bar index the user has selected in the bar list. Drives the
    /// per-bar editor.
    song_selected_bar: usize,
    /// Bar armed for recording — flips to `recording` at the bar
    /// boundary via the SR-16 pending-change pattern.
    song_arm_bar: Option<usize>,
    /// Clipboard for copy/paste of bars.
    song_clipboard: Option<SongBar>,
    /// Scratch buffer for the per-bar "type a chord quality" power
    /// input. Not persisted; cleared on a successful commit.
    song_formula_buf: String,

    // === Practice tab ===
    /// Cached practice sets — computed once at startup since the
    /// catalog isn't constant data (depends on the woodshedding
    /// catalogs being initialized).
    practice_sets: Vec<PracticeSet>,
    practice_selected_set: usize,
    practice_item_idx: usize,
    practice_playing: bool,
    /// Practice-mode tempo; independent from the metronome tab's BPM
    /// so practice can run at a slower learning tempo.
    practice_bpm: f32,
    /// How many bars to spend on each item before auto-advancing.
    practice_bars_per_item: u8,
    /// Wall-clock seconds elapsed in the current item. Drives the
    /// auto-advance and the progress display.
    practice_elapsed_secs: f32,

    // === Shared ===
    /// Active instrument (drives which tuning catalog entry the
    /// fretboard uses). Cycled via the header pickers — every tab's
    /// fretboard re-renders against the new tuning.
    active_instrument: Instrument,
    /// Per-tab catalog sidebar visibility. Each tab that owns a
    /// browse-list sidebar gets its own collapsed-bool here; the
    /// header hamburger queries / toggles the field for the current
    /// tab. Tabs that don't have a sidebar (Tuner, Metronome) ignore
    /// this struct entirely. Defaults to all-expanded.
    sidebars: SidebarVisibility,
    /// The fretboard (tuning + fret count) the visualization paints
    /// against. Cloned into each fretboard_view we construct; cheap
    /// because Fretboard's tuning is a Vec of Pitches.
    fretboard: Fretboard,
    /// Input audio pipeline — one cpal stream, multiple analyzers
    /// (pitch + onset). Constructed at startup; if mic isn't
    /// available, this is Err and the tuner / future onset features
    /// surface "unavailable."
    input: Result<InputBundle, String>,
    /// Output audio pipeline (metronome / future song-mode). Held as
    /// `(engine, handle)` so the cpal stream stays alive; handle is
    /// the clone-able control point.
    engine: Result<(SequencerEngine, EngineHandle), String>,

    // === Tuner tab ===
    /// True when the user has the tuner "on" (analyzer is enabled
    /// and the polling task is alive).
    tuner_active: bool,
    /// Cached most-recent snapshot. Refreshed by the polling task
    /// while `tuner_active` is true.
    tuner_snapshot: Option<TunerSnapshot>,
    /// RMS silence-gate threshold. Below this, the detector reports
    /// no note. Live-tunable via the sensitivity slider.
    tuner_threshold: f64,
    /// Active pitch-detection algorithm — FFT (default) or Cepstrum.
    tuner_detector: DetectorKind,
    /// Optional target pitch — when set, hinted detection biases
    /// toward this note's pitch class so harmonic confusion can't
    /// kick us off the intended string.
    tuner_target: Option<DetectedNoteName>,
    /// Which combobox (if any) is currently expanded. `None` means
    /// every combobox is collapsed. Stored as a `&'static str` ID so
    /// we don't have to maintain a parallel enum as new pickers grow
    /// in across tabs; convention is `"<tab>.<field>"` e.g.
    /// `"progressions.key"`. Only one combobox can be open at a time,
    /// which keeps the UI from stacking competing option lists.
    open_combobox: Option<&'static str>,
    /// Double-click tracking — which field was last clicked and
    /// when. If the next click on the same field arrives within
    /// `DOUBLE_CLICK_MS`, that's a double-click and the field
    /// enters edit mode (see `editing_field`).
    last_click_field: Option<&'static str>,
    last_click_at: Option<std::time::Instant>,
    /// Which numeric field (if any) is currently in inline-edit
    /// mode. `editing_buffer` holds the in-progress text. Only one
    /// field edits at a time — opening edit on a new field commits
    /// the previous one. Field IDs are `&'static str` for the same
    /// reason as `open_combobox`: no parallel enum to maintain.
    editing_field: Option<&'static str>,
    editing_buffer: String,
    /// Active color palette. Every view reads colors through this;
    /// the theme picker on the Settings tab swaps it in-place. Kept
    /// alongside `theme_mode` so view code can grab either (palette
    /// for direct color use, theme_mode for the picker's "is this
    /// option active?" check) without an extra `.palette()` call
    /// per frame.
    palette: Palette,
    /// Which theme `palette` was built from. Round-tripped through
    /// `Settings` and toggled by the Settings tab. Kept in sync with
    /// `palette` via `AppState::set_theme`.
    theme_mode: theme::ThemeMode,
    /// User-authored themes (runtime mirror of `Settings.user_themes`).
    user_themes: Vec<settings::UserThemeDef>,
    /// Name of the active user theme, or `None` = use the built-in
    /// `theme_mode`.
    active_user: Option<String>,
    /// Cached default-properties set for the active theme, rebuilt in
    /// `set_theme`. Held as an `Arc` so the per-frame window view can
    /// hand the same pointer to Xilem each frame (cheap `ptr_eq`
    /// change-detection); a new `Arc` on theme change triggers the
    /// runtime swap in the render root.
    default_properties: Arc<DefaultProperties>,
    /// Stable window id for the single main window (windowed Xilem API).
    window_id: WindowId,
    /// Cleared to `false` by the window's `on_close` so
    /// `XilemAppState::keep_running` stops the event loop.
    running: bool,
}

/// Holds the always-on input engine + clone-able handles for each
/// registered analyzer. Mirror of the iced build's InputBundle.
struct InputBundle {
    _engine: InputEngine,
    tuner: TunerHandle,
    /// Onset handle is registered but not consumed by any UI yet —
    /// here for the future timing-feedback / loop-record features.
    #[allow(dead_code)]
    onset: OnsetHandle,
    /// Loop-capture analyzer for Song-mode recording.
    capture: LooperCaptureHandle,
}

impl AppState {
    fn new() -> Self {
        let scale_idx = scale_catalog()
            .iter()
            .position(|s| s.name == "Major")
            .unwrap_or(0);
        let chord_idx = chord_catalog()
            .iter()
            .position(|c| c.name == "Major")
            .unwrap_or(0);
        // Default to standard 6-string guitar; cycler in the header
        // can move between instruments live.
        let initial_spec = tuning_catalog()
            .first()
            .expect("tuning catalog should be non-empty");
        let tuning = Tuning::from_spec(initial_spec);
        let active_instrument = initial_spec.instrument;
        // Build the shared input pipeline once. Both pitch and onset
        // analyzers register; each carries its own enable flag so the
        // FFT/onset DSP only runs when its feature is on.
        let input: Result<InputBundle, String> = (|| -> Result<InputBundle, String> {
            let onset_analyzer = OnsetAnalyzer::new();
            let onset_handle = onset_analyzer.handle();
            let (builder, capture_handle) = InputEngineBuilder::new()
                .with_analyzer(onset_analyzer)
                .with_looper_capture();
            let (builder, tuner_handle) = builder.with_pitch();
            tuner_handle.set_enabled(false);
            let engine = builder.build().map_err(|e| e.to_string())?;
            Ok(InputBundle {
                _engine: engine,
                tuner: tuner_handle,
                onset: onset_handle,
                capture: capture_handle,
            })
        })();

        // Build the output engine with a default 4/4 quarter-note
        // metronome pattern. Like input, this is eager — cpal output
        // stream allocation happens once at startup, transport is
        // toggled later.
        let bpm = 120.0_f32;
        let initial_pattern = build_metronome_pattern(
            bpm,
            4,
            Subdivision::QUARTER,
            ClickPattern::BeatOnly,
            AccentMode::Downbeat,
        );
        let engine = match SequencerEngine::new(initial_pattern) {
            Ok(eng) => {
                let h = eng.handle();
                Ok((eng, h))
            }
            Err(e) => Err(e.to_string()),
        };

        Self {
            tab: Tab::default(),
            scale_idx,
            scale_root_pc: ChromaticPc::C,
            scale_label_mode: LabelMode::default(),
            chord_idx,
            chord_root_pc: ChromaticPc::C,
            chord_label_mode: LabelMode::default(),
            chord_show_voicing: false,
            chord_voicing_idx: 0,
            progression_idx: None,
            progression_key_pc: ChromaticPc::C,
            progression_expanded_chord: None,
            progression_overlay_mode: false,
            progression_voicing_idx: Vec::new(),
            progression_cards_panel_width: 0.0,
            exercise_idx: 0,
            exercise_starting_fret: 1,
            exercise_step_idx: 0,
            exercise_playing: false,
            exercise_bpm: 80.0,
            bpm,
            metronome_playing: false,
            metronome_time_sig_num: 4,
            metronome_subdivision: Subdivision::QUARTER,
            metronome_click: ClickPattern::default(),
            metronome_accent: AccentMode::default(),
            song_engine: None,
            song_view: Song::new(),
            song_selected_bar: 0,
            song_arm_bar: None,
            song_clipboard: None,
            song_formula_buf: String::new(),
            practice_sets: practice_catalog(),
            practice_selected_set: 0,
            practice_item_idx: 0,
            practice_playing: false,
            practice_bpm: 60.0,
            practice_bars_per_item: 4,
            practice_elapsed_secs: 0.0,
            engine,
            active_instrument,
            sidebars: SidebarVisibility::default(),
            // 12 frets only — past the 12th the pattern just repeats
            // an octave higher, so the visual real estate is better
            // spent giving 0-12 generous spacing.
            fretboard: Fretboard::new(tuning, 12),
            input,
            tuner_active: false,
            tuner_snapshot: None,
            tuner_threshold: woodshed_audio::input::DEFAULT_SILENCE_RMS_THRESHOLD,
            tuner_detector: DetectorKind::Fft,
            tuner_target: None,
            open_combobox: None,
            last_click_field: None,
            last_click_at: None,
            editing_field: None,
            editing_buffer: String::new(),
            palette: Palette::default(),
            theme_mode: theme::ThemeMode::default(),
            user_themes: Vec::new(),
            active_user: None,
            default_properties: Arc::new(build_default_properties(&Palette::default())),
            window_id: WindowId::next(),
            running: true,
        }
    }

    /// Overlay a loaded [`Settings`] onto a freshly-constructed
    /// [`AppState`]. Anything not in `Settings` (audio engines,
    /// snapshots, text-input buffers, combobox open-state) is left at
    /// its constructed value. Out-of-range indices are clamped to
    /// catalog bounds so a config saved against an older catalog
    /// version doesn't strand the user on a deleted entry.
    fn apply_settings(&mut self, s: Settings) {
        self.tab = s.tab;
        // Themes: restore user themes, then resolve the active one. An
        // `active_user_theme` naming a theme that still exists wins;
        // otherwise fall back to the built-in `theme_mode`.
        self.user_themes = s.user_themes;
        self.theme_mode = s.theme_mode;
        match s
            .active_user_theme
            .filter(|name| self.user_themes.iter().any(|t| &t.name == name))
        {
            Some(name) => self.set_user_theme(name),
            None => self.set_theme(s.theme_mode),
        }
        self.active_instrument = settings::instrument_from_str(&s.active_instrument);
        // Rebuild the fretboard against the persisted tuning if it
        // exists in the current catalog, otherwise fall back to the
        // instrument's default tuning. The two-step lookup means a
        // session that last saved with a custom or removed tuning
        // doesn't strand the user on a missing entry — they land on
        // the instrument default and can re-pick.
        let resolved_tuning = s
            .tuning_name
            .as_deref()
            .and_then(|name| {
                tuning_catalog()
                    .iter()
                    .find(|spec| spec.instrument == self.active_instrument && spec.name == name)
            })
            .or_else(|| {
                tuning_catalog()
                    .iter()
                    .find(|spec| spec.instrument == self.active_instrument)
            });
        if let Some(spec) = resolved_tuning {
            self.fretboard = Fretboard::new(Tuning::from_spec(spec), 12);
        }
        self.sidebars = s.sidebars;

        // Scales / Chords / Progressions — clamp every index to the
        // catalog length so a stale save against an older catalog
        // version doesn't panic at first render.
        let sc_len = scale_catalog().len().max(1);
        self.scale_idx = s.scale_idx.min(sc_len - 1);
        self.scale_root_pc = s.scale_root_pc;
        self.scale_label_mode = s.scale_label_mode;

        let ch_len = chord_catalog().len().max(1);
        self.chord_idx = s.chord_idx.min(ch_len - 1);
        self.chord_root_pc = s.chord_root_pc;
        self.chord_label_mode = s.chord_label_mode;
        self.chord_show_voicing = s.chord_show_voicing;
        self.chord_voicing_idx = s.chord_voicing_idx;

        let pg_len = progression_catalog().len();
        self.progression_idx = s
            .progression_idx
            .filter(|i| *i < pg_len);
        self.progression_key_pc = s.progression_key_pc;
        self.progression_overlay_mode = s.progression_overlay_mode;

        let ex_len = exercise_catalog().len().max(1);
        self.exercise_idx = s.exercise_idx.min(ex_len - 1);
        self.exercise_starting_fret = s.exercise_starting_fret;
        self.exercise_bpm = s.exercise_bpm;

        self.bpm = s.bpm;
        self.metronome_time_sig_num = s.metronome_time_sig_num;
        self.metronome_click = s.metronome_click;
        self.metronome_accent = s.metronome_accent;

        let pset_len = self.practice_sets.len().max(1);
        self.practice_selected_set = s.practice_selected_set.min(pset_len - 1);
        self.practice_bpm = s.practice_bpm;
        self.practice_bars_per_item = s.practice_bars_per_item;

        self.tuner_threshold = s.tuner_threshold;
        self.tuner_detector = settings::detector_from_str(&s.tuner_detector);
    }

    /// Capture a durable snapshot for persistence. Mirror of
    /// [`apply_settings`] — every field that round-trips must appear
    /// in both. Runtime-only state (engines, snapshots, text buffers,
    /// open combobox) is deliberately excluded.
    fn snapshot_settings(&self) -> Settings {
        Settings {
            tab: self.tab,
            theme_mode: self.theme_mode,
            user_themes: self.user_themes.clone(),
            active_user_theme: self.active_user.clone(),
            active_instrument: settings::instrument_to_str(self.active_instrument).to_string(),
            tuning_name: Some(self.fretboard.tuning.name.clone()),
            sidebars: self.sidebars,
            scale_idx: self.scale_idx,
            scale_root_pc: self.scale_root_pc,
            scale_label_mode: self.scale_label_mode,
            chord_idx: self.chord_idx,
            chord_root_pc: self.chord_root_pc,
            chord_label_mode: self.chord_label_mode,
            chord_show_voicing: self.chord_show_voicing,
            chord_voicing_idx: self.chord_voicing_idx,
            progression_idx: self.progression_idx,
            progression_key_pc: self.progression_key_pc,
            progression_overlay_mode: self.progression_overlay_mode,
            exercise_idx: self.exercise_idx,
            exercise_starting_fret: self.exercise_starting_fret,
            exercise_bpm: self.exercise_bpm,
            bpm: self.bpm,
            metronome_time_sig_num: self.metronome_time_sig_num,
            metronome_click: self.metronome_click,
            metronome_accent: self.metronome_accent,
            practice_selected_set: self.practice_selected_set,
            practice_bpm: self.practice_bpm,
            practice_bars_per_item: self.practice_bars_per_item,
            tuner_threshold: self.tuner_threshold,
            tuner_detector: settings::detector_to_str(self.tuner_detector).to_string(),
        }
    }

    /// Project the active palette down to the chord-diagram /
    /// fretboard color subset. Call this at the view layer when
    /// passing colors to [`fretboard_view`] /
    /// [`chord_diagram_view`] — saves the per-site
    /// `DiagramColors::from_palette(&state.palette)` boilerplate.
    fn diagram_colors(&self) -> widgets::DiagramColors {
        widgets::DiagramColors::from_palette(&self.palette)
    }

    /// Seeds for the currently-active theme — the active user theme if
    /// one is selected and resolves, else the built-in `theme_mode`.
    fn current_seeds(&self) -> audio_widgets::theme::Seeds {
        self.active_user
            .as_ref()
            .and_then(|name| self.user_themes.iter().find(|t| &t.name == name))
            .map(|t| t.to_seeds())
            .unwrap_or_else(|| self.theme_mode.seeds())
    }

    /// Rebuild the palette + default-properties from the active seeds.
    /// The new `Arc<DefaultProperties>` is what the window view detects
    /// (by `Arc` identity) to re-theme masonry-default widgets live.
    fn rebuild_palette(&mut self) {
        self.palette = theme::palette_from_seeds(&self.current_seeds());
        self.default_properties = Arc::new(build_default_properties(&self.palette));
    }

    /// Select a built-in theme (clears any active user theme).
    fn set_theme(&mut self, mode: theme::ThemeMode) {
        self.theme_mode = mode;
        self.active_user = None;
        self.rebuild_palette();
    }

    /// Select a user theme by name.
    fn set_user_theme(&mut self, name: String) {
        self.active_user = Some(name);
        self.rebuild_palette();
    }

    /// Duplicate the active theme's seeds into a new editable user
    /// theme and select it. Names it "Custom N" (next free index).
    fn new_user_theme(&mut self) {
        let seeds = self.current_seeds();
        let mut n = 1;
        let name = loop {
            let candidate = format!("Custom {n}");
            if !self.user_themes.iter().any(|t| t.name == candidate) {
                break candidate;
            }
            n += 1;
        };
        self.user_themes
            .push(settings::UserThemeDef::from_seeds(name.clone(), &seeds));
        self.set_user_theme(name);
    }

    /// Mutate the active user theme in place (no-op if a built-in is
    /// active), then rebuild. Used by the seed hex editors + dark
    /// toggle + rename.
    fn edit_active_user(&mut self, f: impl FnOnce(&mut settings::UserThemeDef)) {
        let Some(name) = self.active_user.clone() else {
            return;
        };
        if let Some(def) = self.user_themes.iter_mut().find(|t| t.name == name) {
            f(def);
            // A rename changes the def's name; keep `active_user` in sync.
            self.active_user = Some(def.name.clone());
            self.rebuild_palette();
        }
    }

    /// Set one RGB channel (0=R, 1=G, 2=B) of one seed (0=primary,
    /// 1=secondary, 2=tertiary, 3=neutral) on the active user theme,
    /// then re-derive. Drives the live color sliders.
    fn set_seed_channel(&mut self, field: u8, channel: u8, value: u8) {
        self.edit_active_user(|d| {
            let hex = match field {
                0 => &mut d.primary,
                1 => &mut d.secondary,
                2 => &mut d.tertiary,
                _ => &mut d.neutral,
            };
            let mut rgb = audio_widgets::theme::color_from_hex(hex)
                .unwrap_or(Color::from_rgb8(0x80, 0x80, 0x80))
                .to_rgba8()
                .to_u8_array();
            rgb[channel.min(2) as usize] = value;
            *hex = format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]);
        });
    }

    /// Toggle a text tier between derived (`None`) and custom. Enabling
    /// custom seeds it with the current derived color so it starts
    /// where it was. `header` true = `text_header`, false = `text_body`.
    fn toggle_text_override(&mut self, header: bool) {
        let seed_hex = audio_widgets::theme::color_to_hex(if header {
            self.palette.text_header
        } else {
            self.palette.text
        });
        self.edit_active_user(|d| {
            let slot = if header { &mut d.text_header } else { &mut d.text_body };
            *slot = if slot.is_some() { None } else { Some(seed_hex) };
        });
    }

    /// Set one RGB channel of a custom text tier (no-op if that tier is
    /// derived, i.e. `None`).
    fn set_text_channel(&mut self, header: bool, channel: u8, value: u8) {
        self.edit_active_user(|d| {
            let slot = if header { &mut d.text_header } else { &mut d.text_body };
            if let Some(hex) = slot {
                let mut rgb = audio_widgets::theme::color_from_hex(hex)
                    .unwrap_or(Color::from_rgb8(0x80, 0x80, 0x80))
                    .to_rgba8()
                    .to_u8_array();
                rgb[channel.min(2) as usize] = value;
                *hex = format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]);
            }
        });
    }

    /// Remove a user theme. If it was active, fall back to the built-in.
    fn remove_user_theme(&mut self, name: &str) {
        self.user_themes.retain(|t| t.name != name);
        if self.active_user.as_deref() == Some(name) {
            self.active_user = None;
        }
        self.rebuild_palette();
    }

    fn current_scale(&self) -> &'static ScaleFormula {
        let cat = scale_catalog();
        &cat[self.scale_idx.min(cat.len() - 1)]
    }

    fn cycle_scale(&mut self, direction: i32) {
        let len = scale_catalog().len();
        if len == 0 {
            return;
        }
        let cur = self.scale_idx as i32;
        let next = (cur + direction).rem_euclid(len as i32);
        self.scale_idx = next as usize;
    }

    fn current_chord(&self) -> &'static ChordFormula {
        let cat = chord_catalog();
        &cat[self.chord_idx.min(cat.len() - 1)]
    }

    fn cycle_chord(&mut self, direction: i32) {
        let len = chord_catalog().len();
        if len == 0 {
            return;
        }
        let cur = self.chord_idx as i32;
        let next = (cur + direction).rem_euclid(len as i32);
        self.chord_idx = next as usize;
    }

    fn current_exercise(&self) -> &'static Exercise {
        let cat = exercise_catalog();
        &cat[self.exercise_idx.min(cat.len() - 1)]
    }

    fn cycle_exercise(&mut self, direction: i32) {
        let len = exercise_catalog().len();
        if len == 0 {
            return;
        }
        let cur = self.exercise_idx as i32;
        let next = (cur + direction).rem_euclid(len as i32);
        self.exercise_idx = next as usize;
    }

    /// Rebuild the metronome pattern from the current settings and
    /// push it to the engine. If the metronome is playing, the
    /// pattern restarts from beat 1 (set_pattern resets sample/step
    /// position).
    fn apply_metronome_pattern(&self) {
        if let Ok((_, handle)) = &self.engine {
            let pattern = build_metronome_pattern(
                self.bpm,
                self.metronome_time_sig_num,
                self.metronome_subdivision,
                self.metronome_click,
                self.metronome_accent,
            );
            handle.set_pattern(pattern);
            if self.metronome_playing {
                handle.play();
            }
        }
    }

    /// Practice set currently selected (clamped to catalog bounds).
    /// Returns `None` if the catalog is empty.
    fn current_practice_set(&self) -> Option<&PracticeSet> {
        if self.practice_sets.is_empty() {
            return None;
        }
        let idx = self.practice_selected_set.min(self.practice_sets.len() - 1);
        Some(&self.practice_sets[idx])
    }

    /// Item currently being practiced inside the selected set.
    fn current_practice_item(&self) -> Option<&PracticeItem> {
        let set = self.current_practice_set()?;
        if set.items.is_empty() {
            return None;
        }
        let idx = self.practice_item_idx.min(set.items.len() - 1);
        Some(&set.items[idx])
    }

    /// Construct the song engine on first use. Returns `None` if
    /// audio output is unavailable. Subsequent calls return the
    /// cached handle.
    fn ensure_song_engine(&mut self) -> Option<&SongEngineHandle> {
        if self.song_engine.is_none() {
            let initial = self.song_view.clone();
            self.song_engine = Some(match SongEngine::new(initial) {
                Ok(engine) => {
                    let handle = engine.handle();
                    if let Ok(b) = &self.input {
                        handle.set_capture(&b.capture);
                    }
                    Ok((engine, handle))
                }
                Err(e) => Err(e.to_string()),
            });
        }
        match self.song_engine.as_ref() {
            Some(Ok((_, h))) => Some(h),
            _ => None,
        }
    }

    /// Refresh the cached song_view from the engine. Call after any
    /// edit so the UI sees the change without waiting for the next
    /// SongTick.
    fn refresh_song_view(&mut self) {
        if let Some(Ok((_, h))) = self.song_engine.as_ref() {
            self.song_view = h.song();
        }
    }

    /// Start the practice click track using the practice BPM. Reuses
    /// the existing SequencerEngine — practice mode takes over the
    /// audio output for the duration.
    fn start_practice_click(&mut self) {
        if let Ok((_, handle)) = &self.engine {
            let pattern = build_metronome_pattern(
                self.practice_bpm,
                4,
                Subdivision::QUARTER,
                ClickPattern::BeatOnly,
                AccentMode::Downbeat,
            );
            handle.set_pattern(pattern);
            handle.play();
        }
    }

    fn stop_practice_click(&mut self) {
        if let Ok((_, handle)) = &self.engine {
            handle.stop();
        }
    }

    /// Cycle the active instrument. Picks the first tuning spec
    /// matching the new instrument and rebuilds the fretboard.
    fn cycle_instrument(&mut self, direction: i32) {
        let order = Instrument::ALL;
        let cur = order
            .iter()
            .position(|i| *i == self.active_instrument)
            .unwrap_or(0) as i32;
        let next_idx = (cur + direction).rem_euclid(order.len() as i32) as usize;
        let next = order[next_idx];
        if let Some(spec) = tuning_catalog().iter().find(|s| s.instrument == next) {
            self.active_instrument = next;
            self.fretboard = Fretboard::new(Tuning::from_spec(spec), 12);
        }
    }

    fn play_metronome(&mut self) {
        if let Ok((_, handle)) = &self.engine {
            handle.play();
            self.metronome_playing = true;
        }
    }

    fn stop_metronome(&mut self) {
        if let Ok((_, handle)) = &self.engine {
            handle.stop();
            self.metronome_playing = false;
        }
    }

    /// Enable the pitch analyzer and start the tuner UI's polling
    /// task. No-op if the input engine failed to construct. Pushes
    /// the latest threshold / detector / target settings to the
    /// analyzer so they take effect immediately.
    fn start_tuner(&mut self) {
        if let Ok(b) = &self.input {
            b.tuner.set_threshold(self.tuner_threshold);
            b.tuner.set_detector_kind(self.tuner_detector);
            b.tuner.set_target_hint(self.tuner_target.clone());
            b.tuner.set_enabled(true);
            self.tuner_active = true;
        }
    }

    /// Disable the pitch analyzer and clear the cached snapshot.
    fn stop_tuner(&mut self) {
        if let Ok(b) = &self.input {
            b.tuner.set_enabled(false);
        }
        self.tuner_active = false;
        self.tuner_snapshot = None;
    }
}

// =================================================================
// View — assembled top-down from `app_logic`.
// =================================================================

fn app_logic(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    flex_col((
        header(state).flex(0.0),
        // Tab bar + tab content live inside the SAME portal so they
        // scroll together. When the user horizontal-scrolls to reach
        // off-screen content, the tab bar slides along with it —
        // reads as "tab bar is the header of the scrolling page"
        // rather than a separate sticky strip with its own scrollbar.
        //
        // Constraints intentionally OFF on this portal: horizontal
        // scroll is allowed so wide content (and wide tab bar with
        // 9 tabs) stays reachable. Trade-off: prose without explicit
        // width constraints can extend past the viewport — fix at
        // the *tab content's* level with per-section `sized_box` or
        // an inner constrained portal where text wrap matters.
        //
        // `AutoHideScrollBar(true)` keeps both scrollbars hidden
        // until hover/scroll so they don't sit over labels and
        // chord-card edges all the time.
        portal(
            flex_col((
                tab_bar(state),
                tab_content(state),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_alignment(MainAxisAlignment::Start)
            .gap(SP_2),
        )
        .prop(masonry::properties::AutoHideScrollBar(true))
        .flex(1.0),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .main_axis_alignment(MainAxisAlignment::Start)
    .gap(SP_2)
}

fn header(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    let tuning_summary = format!(
        "{} · {}",
        state.active_instrument,
        state.fretboard.tuning.name
    );
    // Hamburger reads as "expanded" when sidebar is open, "collapsed"
    // when closed — text label keeps it accessible without an icon
    // font. Only rendered on tabs that actually have a list to hide;
    // other tabs get a zero-width SizedBox so the flex_row tuple
    // type stays stable. Each tab's collapsed state is tracked
    // independently on [`SidebarVisibility`], so flipping the
    // Scales sidebar doesn't also flip Progressions'.
    let current_tab = state.tab;
    let hamburger_label = if state.sidebars.is_collapsed(current_tab) {
        "☰ Show list"
    } else {
        "☰ Hide list"
    };
    let hamburger: OneOf2<_, _> = if tab_has_list(current_tab) {
        OneOf2::A(text_button(hamburger_label, move |s: &mut AppState| {
            s.sidebars.toggle(current_tab);
        }))
    } else {
        OneOf2::B(
            sized_box(label(""))
                .fixed_width(SP_0),
        )
    };
    // Instrument picker — applies globally; every fretboard re-renders
    // against the new tuning's strings. Combobox lets the user jump to
    // (e.g.) Banjo without three cycle clicks; arrows still flank it for
    // the common "adjacent instrument" case.
    let instrument_options: Vec<ArcStr> = Instrument::ALL
        .iter()
        .map(|i| ArcStr::from(format!("{}", i)))
        .collect();
    let instrument_selected = Instrument::ALL
        .iter()
        .position(|&i| i == state.active_instrument)
        .unwrap_or(0);
    let open_combo = state.open_combobox;
    // Wrap the header in a sized_box with a palette-tracked surface
    // so the toolbar strip respects the active theme. Without this
    // the header sits directly on the unthemed window background
    // (masonry's default) — looks fine in dark mode where the window
    // happens to be dark, but reads as a "dark plate" in light mode
    // even though everything around it is light.
    sized_box(
        flex_row((
            hamburger,
            header_label(state.palette, "Woodshed", TS_LG),
            combobox(
                "header.instrument",
                "",
                &instrument_options,
                instrument_selected,
                open_combo,
                |s: &mut AppState, i: usize| {
                    let next = Instrument::ALL[i];
                    if let Some(spec) = tuning_catalog().iter().find(|sp| sp.instrument == next) {
                        s.active_instrument = next;
                        s.fretboard = Fretboard::new(Tuning::from_spec(spec), 12);
                    }
                },
            ),
            button_sm("◀", |s: &mut AppState| s.cycle_instrument(-1)),
            label(tuning_summary).text_size(TS_SM),
            button_sm("▶", |s: &mut AppState| s.cycle_instrument(1)),
            FlexSpacer::Flex(1.0),
            dim_label(state.palette, "Xilem migration scaffold", TS_XS),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_3),
    )
    .padding(SP_2)
    // Header strip carries a faint `secondary` tint (mostly surface),
    // giving the support hue a visible, app-wide home distinct from
    // the neutral cards.
    .background_color(audio_widgets::theme::mix(
        state.palette.secondary,
        state.palette.surface,
        0.82,
    ))
}

fn tab_bar(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    // Build as a Vec so the loop over `Tab::ALL` types-check; Xilem's
    // tuple views are heterogeneous but a Vec of one type works fine
    // for a list of same-shape buttons.
    let active = state.tab;
    let buttons: Vec<_> = Tab::ALL
        .iter()
        .map(|&tab| {
            let is_active = tab == active;
            // Each button captures its target tab and updates state.tab
            // on click. Active tab gets a slightly different label
            // until we wire proper button-style theming.
            let label_text = if is_active {
                format!("[{}]", tab.label())
            } else {
                tab.label().to_string()
            };
            text_button(label_text, move |s: &mut AppState| {
                s.tab = tab;
            })
        })
        .collect();
    // No inner portal — tab bar lives inside the page-level scrolling
    // portal (see `app_logic`), so horizontal scroll for off-screen
    // tabs falls out of the page's own horizontal scroll. The tab bar
    // reads as the page's "scrollable header" rather than a separate
    // sticky strip with its own scrollbar.
    flex_row(buttons)
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_2)
}

fn tab_content(state: &mut AppState) -> OneOf9<
    impl WidgetView<AppState> + use<>,
    impl WidgetView<AppState> + use<>,
    impl WidgetView<AppState> + use<>,
    impl WidgetView<AppState> + use<>,
    impl WidgetView<AppState> + use<>,
    impl WidgetView<AppState> + use<>,
    impl WidgetView<AppState> + use<>,
    impl WidgetView<AppState> + use<>,
    impl WidgetView<AppState> + use<>,
> {
    match state.tab {
        Tab::Scales => OneOf9::A(scales_view(state)),
        Tab::Chords => OneOf9::B(chords_view(state)),
        Tab::Tuner => OneOf9::C(tuner_view(state)),
        Tab::Progressions => OneOf9::D(progressions_view(state)),
        Tab::Exercises => OneOf9::E(exercises_view(state)),
        Tab::Metronome => OneOf9::F(metronome_view(state)),
        Tab::Practice => OneOf9::G(practice_view(state)),
        Tab::Song => OneOf9::H(song_view_render(state)),
        Tab::Settings => OneOf9::I(settings_view(state)),
    }
}

// =================================================================
// Per-tab views
// =================================================================

/// Scales tab — the first vertical-slice port. Currently shows
/// scale-formula selection (cycle through catalog) and renders the
/// scale's intervals + spelling. The fretboard visualization is the
/// next big piece; it lands once the FretboardWidget is implemented
/// as a Masonry custom widget.
fn scales_view(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    let formula = state.current_scale();
    let root = state.scale_root_pc.to_pitch(4);
    let intervals: String = formula
        .intervals
        .iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let scale_name = format!("{} ({} degrees)", formula.name, formula.degree_count());

    let positions = state
        .fretboard
        .positions_for_scale(formula, root)
        .unwrap_or_default();
    let labels = compute_labels(state.scale_label_mode, &positions);
    let board = state.fretboard.clone();

    let label_mode_text = state.scale_label_mode.label();

    // Combobox option lists. Built every frame — cheap (12 PCs + ~30
    // scales), but could be cached if it ever surfaces in a profile.
    let scale_options: Vec<ArcStr> = scale_catalog()
        .iter()
        .map(|f| ArcStr::from(f.name))
        .collect();
    let root_options: Vec<ArcStr> = ChromaticPc::ALL
        .iter()
        .map(|pc| ArcStr::from(pc.display()))
        .collect();
    let scale_selected = state.scale_idx.min(scale_options.len().saturating_sub(1));
    let root_selected = ChromaticPc::ALL
        .iter()
        .position(|&pc| pc == state.scale_root_pc)
        .unwrap_or(0);
    let open_combo = state.open_combobox;

    // Right-hand info panel: title + intervals + control rows + a
    // bottom-aligned label-mode cycler. Each picker now pairs a
    // combobox (jump to any) with ◀/▶ arrows (walk to adjacent).
    let info_panel = flex_col((
        header_label(state.palette, scale_name, TS_LG),
        dim_prose(state.palette, format!("Intervals: {intervals}"), TS_SM),
        flex_row((
            combobox(
                "scales.scale",
                "Scale: ",
                &scale_options,
                scale_selected,
                open_combo,
                |s: &mut AppState, i: usize| s.scale_idx = i,
            ),
            button_sm("◀", |s: &mut AppState| s.cycle_scale(-1)),
            button_sm("▶", |s: &mut AppState| s.cycle_scale(1)),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_2),
        flex_row((
            combobox(
                "scales.root",
                "Root: ",
                &root_options,
                root_selected,
                open_combo,
                |s: &mut AppState, i: usize| {
                    s.scale_root_pc = ChromaticPc::ALL[i];
                },
            ),
            button_sm("◀", |s: &mut AppState| {
                s.scale_root_pc = s.scale_root_pc.cycle(-1);
            }),
            button_sm("▶", |s: &mut AppState| {
                s.scale_root_pc = s.scale_root_pc.cycle(1);
            }),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_2),
        // Push the label cycler to the bottom of the card.
        FlexSpacer::Flex(1.0),
        text_button(label_mode_text.to_string(), |s: &mut AppState| {
            s.scale_label_mode = s.scale_label_mode.next();
        }),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .main_axis_alignment(MainAxisAlignment::Start)
    .gap(SP_2);

    // Scales catalog browse-list. Mirrors the Progressions list_card
    // shape: one click-to-select button per catalog entry, with a ●
    // marker on the active one. Collapsible via the header hamburger
    // through `state.sidebars.scales`.
    let scale_list_items: Vec<_> = scale_catalog()
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let active = state.scale_idx == i;
            let prefix = if active { "● " } else { "  " };
            text_button(
                format!("{}{}", prefix, f.name),
                move |s: &mut AppState| s.scale_idx = i,
            )
        })
        .collect();
    // Wrap the item flex_col in a portal so the catalog scrolls
    // independently of the rest of the tab when it grows past the
    // sidebar height. `constrain_horizontal` keeps the buttons sized
    // to the sidebar's width (no horizontal scrollbar appears even
    // when a long scale name like "Hungarian Minor" would otherwise
    // exceed it). The portal takes `flex(1.0)` of remaining vertical
    // space so the heading sticks at the top and the scroll viewport
    // gets everything below it.
    let scale_list_card = card(state.palette, 
        flex_col((
            label("Scales").text_size(TS_MD),
            portal(
                flex_col(scale_list_items)
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_alignment(MainAxisAlignment::Start)
                    .gap(SP_1),
            )
            .constrain_horizontal(true)
            .flex(1.0),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_2),
    );

    let sidebar: OneOf2<_, _> = if state.sidebars.is_collapsed(Tab::Scales) {
        OneOf2::A(
            sized_box(label(""))
                .fixed_width(SP_0),
        )
    } else {
        OneOf2::B(
            sized_box(scale_list_card)
                .fixed_width(masonry::layout::Length::px(220.0)),
        )
    };

    // Fretboard + info panel use `split` for hard-fractional sharing,
    // same pattern as Progressions. Default 0.5/0.5; user can drag
    // the bar to adjust. Min lengths keep each side from collapsing.
    use masonry::layout::Length as MLen;
    flex_row((
        sidebar,
        xilem::view::split(
            card(
                state.palette,
                sized_box(fretboard_view(
                    board,
                    positions,
                    labels,
                    state.diagram_colors(),
                    None,
                ))
                .fixed_height(masonry::layout::Length::px(660.0)),
            ),
            card(state.palette, info_panel),
        )
        .split_point(0.5)
        .min_lengths(MLen::const_px(240.0), MLen::const_px(240.0))
        .flex(1.0),
    ))
    // Cross-axis Start (not Stretch) — so the fretboard side stays
    // at its natural height instead of growing whenever the other
    // side gets taller (e.g. when a combobox dropdown opens, the
    // chord_cards card grows vertically and would otherwise drag the
    // fretboard along with it).
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .main_axis_alignment(MainAxisAlignment::Start)
    .gap(SP_4)
}

/// Chords tab — symmetric to Scales but with a voicing/scale toggle.
/// When `chord_show_voicing` is on, the fretboard shows just the
/// selected playable voicing (5-6 specific positions). When off, it
/// shows every chord tone across the fretboard (the chord-tone scale).
fn chords_view(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    let formula = state.current_chord();
    let root = state.chord_root_pc.to_pitch(4);
    let intervals: String = formula
        .intervals
        .iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let chord_symbol = if formula.symbol.is_empty() {
        String::new()
    } else {
        formula.symbol.to_string()
    };
    let display_root = state.chord_root_pc.display();
    let chord_label = format!("{display_root}{chord_symbol} ({})", formula.name);

    // Enumerate voicings up front — used both to pick the current
    // voicing's positions and to render the prev/next "Voicing N/M"
    // label.
    let voicings = enumerate_voicings(&state.fretboard, formula, root);
    let voicing_count = voicings.len();
    let voicing_idx = if voicing_count == 0 {
        0
    } else {
        state.chord_voicing_idx.min(voicing_count - 1)
    };

    // Positions depend on mode: voicing → just that voicing's 5-6
    // notes; scale → every chord tone across the fretboard.
    let positions = if state.chord_show_voicing && !voicings.is_empty() {
        voicing_to_positions(&voicings[voicing_idx])
    } else {
        state
            .fretboard
            .positions_for_chord(formula, root)
            .unwrap_or_default()
    };
    let labels = compute_labels(state.chord_label_mode, &positions);
    let board = state.fretboard.clone();
    let label_mode_text = state.chord_label_mode.label();

    // Voicing-row middle button text reflects the mode + index.
    let voicing_mid_text = if voicing_count == 0 {
        "no voicings".to_string()
    } else if state.chord_show_voicing {
        format!("Voicing {}/{}", voicing_idx + 1, voicing_count)
    } else {
        "All chord tones".to_string()
    };

    // Combobox option lists for chord name + root. See `scales_view`
    // for the same pattern + rationale.
    let chord_options: Vec<ArcStr> = chord_catalog()
        .iter()
        .map(|f| ArcStr::from(f.name))
        .collect();
    let chord_root_options: Vec<ArcStr> = ChromaticPc::ALL
        .iter()
        .map(|pc| ArcStr::from(pc.display()))
        .collect();
    let chord_selected = state.chord_idx.min(chord_options.len().saturating_sub(1));
    let chord_root_selected = ChromaticPc::ALL
        .iter()
        .position(|&pc| pc == state.chord_root_pc)
        .unwrap_or(0);
    let chord_open_combo = state.open_combobox;
    let _ = display_root; // surfaced as the combobox trigger label

    let info_panel = flex_col((
        header_label(state.palette, chord_label, TS_LG),
        dim_prose(state.palette, format!("Intervals: {intervals}"), TS_SM),
        flex_row((
            combobox(
                "chords.chord",
                "Chord: ",
                &chord_options,
                chord_selected,
                chord_open_combo,
                |s: &mut AppState, i: usize| s.chord_idx = i,
            ),
            button_sm("◀", |s: &mut AppState| s.cycle_chord(-1)),
            button_sm("▶", |s: &mut AppState| s.cycle_chord(1)),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_2),
        flex_row((
            combobox(
                "chords.root",
                "Root: ",
                &chord_root_options,
                chord_root_selected,
                chord_open_combo,
                |s: &mut AppState, i: usize| {
                    s.chord_root_pc = ChromaticPc::ALL[i];
                },
            ),
            button_sm("◀", |s: &mut AppState| {
                s.chord_root_pc = s.chord_root_pc.cycle(-1);
            }),
            button_sm("▶", |s: &mut AppState| {
                s.chord_root_pc = s.chord_root_pc.cycle(1);
            }),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_2),
        // Voicing row — middle button toggles mode; arrows cycle.
        // Cycling either arrow auto-enables voicing mode, so a user
        // who just wants to flip through voicings doesn't have to
        // hit the toggle first.
        flex_row((
            button_sm("◀", move |s: &mut AppState| {
                if voicing_count > 0 {
                    s.chord_show_voicing = true;
                    let cur = s.chord_voicing_idx.min(voicing_count - 1) as i32;
                    s.chord_voicing_idx =
                        ((cur - 1).rem_euclid(voicing_count as i32)) as usize;
                }
            }),
            text_button(voicing_mid_text, |s: &mut AppState| {
                s.chord_show_voicing = !s.chord_show_voicing;
            }),
            button_sm("▶", move |s: &mut AppState| {
                if voicing_count > 0 {
                    s.chord_show_voicing = true;
                    let cur = s.chord_voicing_idx.min(voicing_count - 1) as i32;
                    s.chord_voicing_idx =
                        ((cur + 1).rem_euclid(voicing_count as i32)) as usize;
                }
            }),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_2),
        FlexSpacer::Flex(1.0),
        text_button(label_mode_text.to_string(), |s: &mut AppState| {
            s.chord_label_mode = s.chord_label_mode.next();
        }),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .main_axis_alignment(MainAxisAlignment::Start)
    .gap(SP_2);

    // Chords catalog browse-list sidebar. Same shape as the Scales
    // sidebar: ● marker on the active chord, click to select,
    // scrollable when the catalog overflows.
    let chord_list_items: Vec<_> = chord_catalog()
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let active = state.chord_idx == i;
            let prefix = if active { "● " } else { "  " };
            text_button(
                format!("{}{}", prefix, f.name),
                move |s: &mut AppState| s.chord_idx = i,
            )
        })
        .collect();
    let chord_list_card = card(
        state.palette,
        flex_col((
            label("Chords").text_size(TS_MD),
            portal(
                flex_col(chord_list_items)
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_alignment(MainAxisAlignment::Start)
                    .gap(SP_1),
            )
            .constrain_horizontal(true)
            .flex(1.0),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_2),
    );
    let chord_sidebar: OneOf2<_, _> = if state.sidebars.is_collapsed(Tab::Chords) {
        OneOf2::A(sized_box(label("")).fixed_width(SP_0))
    } else {
        OneOf2::B(
            sized_box(chord_list_card)
                .fixed_width(masonry::layout::Length::px(220.0)),
        )
    };

    // Same split-view pattern as Scales / Progressions.
    use masonry::layout::Length as MLen;
    flex_row((
        chord_sidebar,
        xilem::view::split(
            card(
                state.palette,
                sized_box(fretboard_view(
                    board,
                    positions,
                    labels,
                    state.diagram_colors(),
                    None,
                ))
                .fixed_height(masonry::layout::Length::px(660.0)),
            ),
            card(state.palette, info_panel),
        )
        .split_point(0.5)
        .min_lengths(MLen::const_px(240.0), MLen::const_px(240.0))
        .flex(1.0),
    ))
    // Cross-axis Start (not Stretch) — so the fretboard side stays
    // at its natural height instead of growing whenever the other
    // side gets taller (e.g. when a combobox dropdown opens, the
    // chord_cards card grows vertically and would otherwise drag the
    // fretboard along with it).
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .main_axis_alignment(MainAxisAlignment::Start)
    .gap(SP_4)
}

/// Song tab — multi-bar arrangement with per-bar settings.
///
/// Three cards horizontally:
/// - Transport + bar list strip on top (full width)
/// - Per-bar editor on the left
/// - Bar ops + clipboard on the right
///
/// Engine is lazy — constructed on first interaction so the cpal
/// output stream isn't opened until the user actually visits the tab.
/// The cached `song_view` drives all rendering; live SongTick refresh
/// only runs while the tab is active.
/// Group the song's bars into section bands for the timeline lane. A
/// bar with a non-empty `label` opens a section that runs until the
/// next labeled bar (or song end); a leading run of unlabeled bars is
/// left uncovered (renders as track). Reuses `Bar.label` as the
/// section marker rather than carrying a redundant field.
fn compute_section_bands(song: &Song) -> Vec<SectionBand> {
    let mut bands: Vec<SectionBand> = Vec::new();
    for (i, bar) in song.bars.iter().enumerate() {
        if !bar.label.is_empty() {
            bands.push(SectionBand {
                start_bar: i,
                len: 1,
                label: bar.label.clone(),
            });
        } else if let Some(open) = bands.last_mut() {
            open.len += 1;
        }
    }
    bands
}

fn song_view_render(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    let song = state.song_view.clone();
    let engine_available = matches!(state.song_engine.as_ref(), None | Some(Ok(_)));
    let engine_error = match state.song_engine.as_ref() {
        Some(Err(e)) => Some(e.clone()),
        _ => None,
    };
    let bar_count = song.len();
    let selected = state.song_selected_bar.min(bar_count.saturating_sub(1));

    // === Playhead position (fraction across the timeline) ===
    // Cells in the lanes are equal-width per bar, so the playhead is
    // `(bar_idx + within-bar fraction) / bar_count`. Within-bar
    // fraction comes from `cursor.sample_in_bar` over the bar's full
    // (multi-measure) sample length at the engine's sample rate. The
    // bar-to-bar stepping is always exact; only intra-cell motion
    // depends on the rate. `None` when stopped.
    let playhead = if song.playing && bar_count > 0 {
        let sr = match state.song_engine.as_ref() {
            Some(Ok((_, h))) => h.sample_rate(),
            _ => 48_000.0,
        };
        let idx = song.cursor.bar_idx.min(bar_count - 1);
        let bar_dur = song.bars.get(idx).map(|b| b.duration_samples(sr)).unwrap_or(0);
        let within = if bar_dur > 0 {
            (song.cursor.sample_in_bar as f64 / bar_dur as f64).clamp(0.0, 1.0)
        } else {
            0.0
        };
        Some((idx as f64 + within) / bar_count as f64)
    } else {
        None
    };

    // === Section lane (timeline top lane) ===
    // Labeled bands spanning bar ranges, derived from each bar's
    // `label`. Pure structure — see `compute_section_bands`.
    let section_bands = compute_section_bands(&song);
    let has_sections = !section_bands.is_empty();
    let section_lane = sized_box(section_lane_view(
        section_bands,
        bar_count,
        playhead,
        SectionColors::from_palette(&state.palette),
    ))
    .fixed_height(masonry::layout::Length::px(34.0));

    // === Chord lane (timeline) ===
    // One cell per bar showing its chord; selected bar outlined,
    // playhead bar filled. Aligns column-for-column with the section
    // lane above.
    let chord_labels: Vec<String> = song
        .bars
        .iter()
        .map(|b| {
            b.chord_ref
                .as_ref()
                .map(|c| c.label.clone())
                .filter(|s| !s.is_empty())
                .unwrap_or_default()
        })
        .collect();
    let cursor_bar = song.playing.then_some(song.cursor.bar_idx);
    let chord_lane = sized_box(chord_lane_view(
        chord_labels,
        Some(selected),
        cursor_bar,
        playhead,
        SectionColors::from_palette(&state.palette),
    ))
    .fixed_height(masonry::layout::Length::px(30.0));

    // === Transport row ===
    let rewind_btn = text_button("⏮ Rewind", |s: &mut AppState| {
        if let Some(h) = s.ensure_song_engine() {
            h.rewind();
        }
        s.song_arm_bar = None;
    });
    let playing = song.playing;
    let play_btn = if playing {
        OneOf2::A(text_button("■ Stop", |s: &mut AppState| {
            if let Some(Ok((_, h))) = s.song_engine.as_ref() {
                h.stop();
            }
            s.song_arm_bar = None;
        }))
    } else {
        OneOf2::B(text_button("▶ Play", |s: &mut AppState| {
            if let Some(h) = s.ensure_song_engine() {
                h.play();
            }
        }))
    };
    let recording = song.recording;
    let record_label = if recording {
        "● Recording"
    } else if state.song_arm_bar.is_some() {
        "● Armed"
    } else {
        "● Record"
    };
    let record_btn = text_button(record_label, |s: &mut AppState| {
        let already = s.song_view.recording;
        let target = s.song_selected_bar;
        if let Some(h) = s.ensure_song_engine() {
            if already {
                h.queue(PendingChange::StopRecording);
            } else {
                h.queue(PendingChange::StartRecording { bar_idx: target });
            }
        }
        if let Ok(b) = &s.input {
            b.capture.set_enabled(!already);
        }
        s.song_arm_bar = if already { None } else { Some(target) };
    });
    let one_shot = song.one_shot;
    let loop_btn = text_button(
        if one_shot { "Loop: off" } else { "Loop: on" },
        move |s: &mut AppState| {
            let new_one_shot = !one_shot;
            if let Some(h) = s.ensure_song_engine() {
                h.with_song(|x| x.one_shot = new_one_shot);
            }
            s.refresh_song_view();
        },
    );
    let click_on = song.click_enabled;
    let click_btn = text_button(
        if click_on { "Click: on" } else { "Click: off" },
        move |s: &mut AppState| {
            let next = !click_on;
            if let Some(h) = s.ensure_song_engine() {
                h.with_song(move |x| x.click_enabled = next);
            }
            s.refresh_song_view();
        },
    );
    let replace_on = song.record_replace;
    let rec_mode_btn = text_button(
        if replace_on { "Rec: replace" } else { "Rec: overdub" },
        move |s: &mut AppState| {
            let next = !replace_on;
            if let Some(h) = s.ensure_song_engine() {
                h.with_song(move |x| x.record_replace = next);
            }
            s.refresh_song_view();
        },
    );
    let cursor_label = label(format!(
        "Bar {} / {}  ·  bars {}",
        song.cursor.bar_idx + 1,
        bar_count,
        bar_count
    ))
    .text_size(TS_XS);

    let transport = flex_row((
        rewind_btn,
        play_btn,
        record_btn,
        loop_btn,
        click_btn,
        rec_mode_btn,
        cursor_label,
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .main_axis_alignment(MainAxisAlignment::Start)
    .gap(SP_2);

    // === Bar list strip ===
    let bar_buttons: Vec<_> = song
        .bars
        .iter()
        .enumerate()
        .map(|(i, bar)| {
            let active = i == selected;
            let is_cursor = song.playing && i == song.cursor.bar_idx;
            let is_armed = Some(i) == state.song_arm_bar;
            let prefix = if is_cursor { "▶ " } else if active { "● " } else { "" };
            let chord_label = bar
                .chord_ref
                .as_ref()
                .map(|c| c.label.clone())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "—".to_string());
            let audio_marker = if bar.audio_buffer.is_some() {
                "●"
            } else {
                "○"
            };
            let armed_mark = if is_armed { "  ⚠" } else { "" };
            let len_marker = if bar.length > 1 {
                format!("  ×{}", bar.length)
            } else {
                String::new()
            };
            let text = format!(
                "{prefix}Bar {}{len_marker}  {:.0}bpm  {}  loop{}{armed_mark}",
                i + 1,
                bar.bpm,
                chord_label,
                audio_marker,
            );
            text_button(text, move |s: &mut AppState| {
                s.song_selected_bar = i;
            })
        })
        .collect();
    let add_btn = text_button("+ Add bar", |s: &mut AppState| {
        if let Some(h) = s.ensure_song_engine() {
            let new_idx = h.with_song(|x| x.add_bar());
            s.song_selected_bar = new_idx;
        }
        s.refresh_song_view();
    });
    let bar_strip_inner = flex_col(bar_buttons)
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_1);

    // === Per-bar editor ===
    let editor: OneOf2<_, _> = if let Some(bar) = song.bars.get(selected) {
        let bpm = bar.bpm;
        let num = bar.time_signature.numerator;
        let denom = bar.time_signature.denominator;
        let length = bar.length.max(1);
        let chord_label = bar
            .chord_ref
            .as_ref()
            .map(|c| c.label.clone())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "—".to_string());
        let audio_state = if bar.audio_buffer.is_some() {
            "Loop: recorded"
        } else {
            "Loop: empty"
        };
        // Waveform peaks for the recorded loop (empty = zero-line).
        let wave_peaks = bar
            .audio_buffer
            .as_ref()
            .map(|b| audio_widgets::compute_peaks(b.data.as_slice(), 240))
            .unwrap_or_default();
        // Full-catalog chord picker for this bar. Options are the
        // woodshedding chord catalog (Major, m7, dom7, sus4, 13ths,
        // …); the selected index reflects the bar's current chord
        // formula, defaulting to Major when the bar has no chord set.
        let chord_options: Vec<ArcStr> = chord_catalog()
            .iter()
            .map(|f| ArcStr::from(f.name))
            .collect();
        let chord_selected = bar
            .chord_ref
            .as_ref()
            .and_then(|c| {
                chord_catalog().iter().position(|f| f.name == c.formula_name)
            })
            .unwrap_or(0);
        let open_combo = state.open_combobox;
        let section_label = bar.label.clone();
        // Root pitch + octave for the bar's chord. Recovered from the
        // stored root frequency when a chord is set; otherwise defaults
        // to the Progressions key at octave 4.
        let (current_root, current_oct) = bar
            .chord_ref
            .as_ref()
            .map(|c| chord_root_from_freq(c.root_freq_hz))
            .unwrap_or((state.progression_key_pc, 4));
        let root_options: Vec<ArcStr> = ChromaticPc::ALL
            .iter()
            .map(|pc| ArcStr::from(pc.display()))
            .collect();
        let root_selected = ChromaticPc::ALL
            .iter()
            .position(|&p| p == current_root)
            .unwrap_or(0);
        let octave_options: Vec<ArcStr> = CHORD_OCTAVE_RANGE
            .map(|o| ArcStr::from(format!("Oct {o}")))
            .collect();
        let octave_selected =
            (current_oct - *CHORD_OCTAVE_RANGE.start()).clamp(0, i8::MAX) as usize;
        let formula_buf = state.song_formula_buf.clone();
        OneOf2::A(
            flex_col((
                label(format!("Editing bar {}", selected + 1)).text_size(TS_MD),
                // Section marker: a non-empty label opens a section
                // band in the lane above, spanning to the next
                // labeled bar. Commits straight to the bar's label.
                flex_row((
                    label("Section:").text_size(TS_SM),
                    text_input(section_label, |s: &mut AppState, t| {
                        let target = s.song_selected_bar;
                        if let Some(h) = s.ensure_song_engine() {
                            h.with_song(move |x| {
                                if let Ok(b) = x.bar_mut(target) {
                                    b.label = t;
                                }
                            });
                        }
                        s.refresh_song_view();
                    })
                    .placeholder("(unlabeled)"),
                ))
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .main_axis_alignment(MainAxisAlignment::Start)
                .gap(SP_2),
                // Tempo — slider for quick sweeps + a mono readout.
                flex_row((
                    label(format!("Tempo: {bpm:.0} BPM"))
                        .text_size(TS_SM)
                        .font(mono_family()),
                    sized_box(slider(40.0, 240.0, bpm as f64, |s: &mut AppState, v: f64| {
                        let target = s.song_selected_bar;
                        if let Some(h) = s.ensure_song_engine() {
                            h.with_song(move |x| {
                                if let Ok(b) = x.bar_mut(target) {
                                    b.bpm = (v as f32).clamp(40.0, 240.0);
                                }
                            });
                        }
                        s.refresh_song_view();
                    }))
                    .fixed_width(masonry::layout::Length::px(220.0)),
                ))
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .main_axis_alignment(MainAxisAlignment::Start)
                .gap(SP_2),
                // Time signature — numerator (beats/bar) and
                // denominator (beat unit) both adjustable.
                flex_row((
                    label(format!("Time: {num}/{denom}")).text_size(TS_SM),
                    button_sm("◀", |s: &mut AppState| {
                        let target = s.song_selected_bar;
                        if let Some(h) = s.ensure_song_engine() {
                            h.with_song(|x| {
                                if let Ok(b) = x.bar_mut(target) {
                                    b.time_signature.numerator =
                                        b.time_signature.numerator.saturating_sub(1).max(1);
                                }
                            });
                        }
                        s.refresh_song_view();
                    }),
                    button_sm("▶", |s: &mut AppState| {
                        let target = s.song_selected_bar;
                        if let Some(h) = s.ensure_song_engine() {
                            h.with_song(|x| {
                                if let Ok(b) = x.bar_mut(target) {
                                    b.time_signature.numerator =
                                        (b.time_signature.numerator + 1).min(12);
                                }
                            });
                        }
                        s.refresh_song_view();
                    }),
                    label("/").text_size(TS_SM),
                    button_sm("◀", |s: &mut AppState| {
                        let target = s.song_selected_bar;
                        if let Some(h) = s.ensure_song_engine() {
                            h.with_song(|x| {
                                if let Ok(b) = x.bar_mut(target) {
                                    b.time_signature.denominator =
                                        prev_denominator(b.time_signature.denominator);
                                }
                            });
                        }
                        s.refresh_song_view();
                    }),
                    button_sm("▶", |s: &mut AppState| {
                        let target = s.song_selected_bar;
                        if let Some(h) = s.ensure_song_engine() {
                            h.with_song(|x| {
                                if let Ok(b) = x.bar_mut(target) {
                                    b.time_signature.denominator =
                                        next_denominator(b.time_signature.denominator);
                                }
                            });
                        }
                        s.refresh_song_view();
                    }),
                ))
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .main_axis_alignment(MainAxisAlignment::Start)
                .gap(SP_2),
                // Length — how many measures this bar/block spans.
                // A length-N bar is one timeline cell holding N
                // measures of the same tempo/meter/chord.
                flex_row((
                    label(format!(
                        "Length: {length} {}",
                        if length == 1 { "bar" } else { "bars" }
                    ))
                    .text_size(TS_SM),
                    button_sm("−", |s: &mut AppState| {
                        let target = s.song_selected_bar;
                        if let Some(h) = s.ensure_song_engine() {
                            h.with_song(|x| {
                                if let Ok(b) = x.bar_mut(target) {
                                    b.length = b.length.saturating_sub(1).max(1);
                                }
                            });
                        }
                        s.refresh_song_view();
                    }),
                    button_sm("+", |s: &mut AppState| {
                        let target = s.song_selected_bar;
                        if let Some(h) = s.ensure_song_engine() {
                            h.with_song(|x| {
                                if let Ok(b) = x.bar_mut(target) {
                                    b.length = (b.length.max(1) + 1).min(16);
                                }
                            });
                        }
                        s.refresh_song_view();
                    }),
                ))
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .main_axis_alignment(MainAxisAlignment::Start)
                .gap(SP_2),
                label(format!("Chord: {chord_label}")).text_size(TS_SM),
                // Chord entry — two paths (decision #3):
                //   • Root + Quality comboboxes (reliable, always
                //     resolves). Root is the bar's own pitch class now,
                //     not the global Progressions key.
                //   • A typed "quality" power input below, validated
                //     against the catalog so anything accepted renders.
                // Root + octave pickers.
                flex_row((
                    combobox(
                        "song.chord.root",
                        "Root: ",
                        &root_options,
                        root_selected,
                        open_combo,
                        |s: &mut AppState, i: usize| {
                            let target = s.song_selected_bar;
                            let (_, oct) = bar_chord_root(s, target);
                            let new_pc = ChromaticPc::ALL[i];
                            let name = s
                                .song_view
                                .bars
                                .get(target)
                                .and_then(|b| b.chord_ref.as_ref())
                                .map(|c| c.formula_name.clone())
                                .unwrap_or_else(|| "Major".to_string());
                            set_bar_chord(s, target, Some(make_chord_ref(new_pc, oct, &name)));
                        },
                    ),
                    combobox(
                        "song.chord.octave",
                        "",
                        &octave_options,
                        octave_selected,
                        open_combo,
                        |s: &mut AppState, i: usize| {
                            let target = s.song_selected_bar;
                            let (pc, _) = bar_chord_root(s, target);
                            let new_oct = *CHORD_OCTAVE_RANGE.start() + i as i8;
                            let name = s
                                .song_view
                                .bars
                                .get(target)
                                .and_then(|b| b.chord_ref.as_ref())
                                .map(|c| c.formula_name.clone())
                                .unwrap_or_else(|| "Major".to_string());
                            set_bar_chord(s, target, Some(make_chord_ref(pc, new_oct, &name)));
                        },
                    ),
                    text_button("Clear chord", |s: &mut AppState| {
                        let target = s.song_selected_bar;
                        set_bar_chord(s, target, None);
                    }),
                ))
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .main_axis_alignment(MainAxisAlignment::Start)
                .gap(SP_2),
                // Quality picker — full catalog. Keeps the bar's
                // current root + octave, swaps the formula.
                combobox(
                    "song.chord",
                    "Quality: ",
                    &chord_options,
                    chord_selected,
                    open_combo,
                    |s: &mut AppState, i: usize| {
                        let target = s.song_selected_bar;
                        let (pc, oct) = bar_chord_root(s, target);
                        let name = chord_catalog()[i].name;
                        set_bar_chord(s, target, Some(make_chord_ref(pc, oct, name)));
                    },
                ),
                // Typed power path — commit on Enter. Invalid text is
                // a no-op (buffer stays for correction); valid catalog
                // names or symbols (e.g. "m7", "maj7", "sus4") apply
                // using the bar's current root + octave, then clear.
                flex_row((
                    label("Type quality:").text_size(TS_SM),
                    text_input(formula_buf, |s: &mut AppState, t| {
                        s.song_formula_buf = t;
                    })
                    .on_enter(|s: &mut AppState, _final| {
                        let target = s.song_selected_bar;
                        if let Some(idx) = formula_index_from_input(&s.song_formula_buf) {
                            let (pc, oct) = bar_chord_root(s, target);
                            let name = chord_catalog()[idx].name;
                            set_bar_chord(s, target, Some(make_chord_ref(pc, oct, name)));
                            s.song_formula_buf.clear();
                        }
                    })
                    .placeholder("e.g. m7, maj7, sus4"),
                ))
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .main_axis_alignment(MainAxisAlignment::Start)
                .gap(SP_2),
                // === Sampler: recorded-loop waveform + shaping ops ===
                label(audio_state).text_size(TS_XS),
                sized_box(waveform_view(
                    wave_peaks,
                    state.palette.primary,
                    state.palette.text_dim,
                ))
                .fixed_height(masonry::layout::Length::px(56.0)),
                // Loop-shaping ops act on the bar's recorded buffer
                // (length-preserving so the loop stays bar-locked).
                flex_row((
                    button_sm("Normalize", move |s: &mut AppState| {
                        sample_op(s, |b| b.normalize(1.0));
                    }),
                    button_sm("Reverse", move |s: &mut AppState| {
                        sample_op(s, |b| b.reverse());
                    }),
                    button_sm("Gain −", move |s: &mut AppState| {
                        sample_op(s, |b| b.apply_gain(0.8));
                    }),
                    button_sm("Gain +", move |s: &mut AppState| {
                        sample_op(s, |b| b.apply_gain(1.25));
                    }),
                    text_button("Clear audio", |s: &mut AppState| {
                        let target = s.song_selected_bar;
                        if let Some(h) = s.ensure_song_engine() {
                            h.with_song(|x| {
                                let _ = x.detach_buffer(target);
                            });
                        }
                        s.refresh_song_view();
                    }),
                ))
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .main_axis_alignment(MainAxisAlignment::Start)
                .gap(SP_2),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_alignment(MainAxisAlignment::Start)
            .gap(SP_2),
        )
    } else {
        OneOf2::B(label("No bar selected.").text_size(TS_SM))
    };

    // === Bar ops ===
    let ops = flex_col((
        label("Bar ops").text_size(TS_MD),
        text_button("Copy", |s: &mut AppState| {
            let target = s.song_selected_bar;
            if let Some(h) = s.ensure_song_engine()
                && let Ok(bar) = h.with_song(|x| x.copy_bar(target))
            {
                s.song_clipboard = Some(bar);
            }
        }),
        text_button(
            if state.song_clipboard.is_some() {
                "Paste"
            } else {
                "Paste (empty)"
            },
            |s: &mut AppState| {
                let target = s.song_selected_bar;
                let clip = s.song_clipboard.clone();
                if let (Some(h), Some(bar)) = (s.ensure_song_engine(), clip) {
                    let _ = h.with_song(|x| x.paste_bar(target, bar));
                }
                s.refresh_song_view();
            },
        ),
        text_button("◀ Move left", |s: &mut AppState| {
            let target = s.song_selected_bar;
            if target == 0 {
                return;
            }
            if let Some(h) = s.ensure_song_engine() {
                let _ = h.with_song(|x| x.move_bar(target, target - 1));
            }
            s.song_selected_bar = target - 1;
            s.refresh_song_view();
        }),
        text_button("Move right ▶", |s: &mut AppState| {
            let target = s.song_selected_bar;
            let len = s.song_view.len();
            if target + 1 >= len {
                return;
            }
            if let Some(h) = s.ensure_song_engine() {
                let _ = h.with_song(|x| x.move_bar(target, target + 1));
            }
            s.song_selected_bar = target + 1;
            s.refresh_song_view();
        }),
        text_button("Remove bar", |s: &mut AppState| {
            let target = s.song_selected_bar;
            if let Some(h) = s.ensure_song_engine() {
                let _ = h.with_song(|x| x.remove_bar(target));
            }
            s.refresh_song_view();
            if s.song_selected_bar >= s.song_view.len() {
                s.song_selected_bar = s.song_view.len().saturating_sub(1);
            }
        }),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .main_axis_alignment(MainAxisAlignment::Start)
    .gap(SP_2);

    let header_label = if let Some(err) = engine_error {
        OneOf2::A(danger_prose(state.palette, format!("Song engine unavailable: {err}"), TS_XS))
    } else {
        OneOf2::B(label(format!("{} · {} bars", song.name, bar_count)).text_size(TS_MD))
    };

    // SongTick — only when the engine is up; refreshes the cached
    // song_view so the cursor highlight follows playback.
    let tick_task = (engine_available && state.song_engine.is_some()).then(|| {
        task_raw(
            move |proxy, _| async move {
                let mut tick = time::interval(Duration::from_millis(50));
                tick.tick().await;
                loop {
                    tick.tick().await;
                    if proxy.message(()).is_err() {
                        break;
                    }
                }
            },
            |s: &mut AppState, _: ()| {
                if let Some(Ok((_, h))) = s.song_engine.as_ref() {
                    s.song_view = h.song();
                }
            },
        )
    });

    let section_caption = if has_sections {
        "Sections"
    } else {
        "Sections — label a bar below to start one"
    };
    let visible = flex_col((
        header_label,
        card(state.palette, transport),
        card(state.palette, flex_col((
            label(section_caption).text_size(TS_XS),
            section_lane,
            label("Chords").text_size(TS_XS),
            chord_lane,
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_1)),
        flex_row((
            // Bar ops: compact, natural width — its own backdrop on
            // the left so it doesn't strand a gap beside the bar list.
            card(state.palette, ops),
            // Bars: flexes to fill the rest. The wide area is reserved
            // for the horizontal grid the playhead sweeps in T3.
            card(state.palette, flex_col((
                label("Bars (click to select):").text_size(TS_XS),
                bar_strip_inner,
                add_btn,
            ))
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_alignment(MainAxisAlignment::Start)
            .gap(SP_2))
            .flex(1.0),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_3),
        card(state.palette, editor),
        FlexSpacer::Flex(1.0),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .main_axis_alignment(MainAxisAlignment::Start)
    .gap(SP_3);

    fork(visible, tick_task)
}

/// Practice tab — the keystone "drive me through material" mode. A
/// practice set is an ordered list of [`PracticeItem`]s (scales,
/// chords, or exercises) that the app cycles through at tempo while
/// a click plays.
fn practice_view(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    let set_count = state.practice_sets.len();
    let set_idx = state.practice_selected_set.min(set_count.saturating_sub(1));
    let set_name = state
        .current_practice_set()
        .map(|s| s.name.clone())
        .unwrap_or_else(|| "(no set)".to_string());
    let set_desc = state
        .current_practice_set()
        .map(|s| s.description.clone())
        .unwrap_or_default();

    let item_count = state
        .current_practice_set()
        .map(|s| s.items.len())
        .unwrap_or(0);
    let item_idx = state.practice_item_idx.min(item_count.saturating_sub(1));
    let item_label = state
        .current_practice_item()
        .map(|item| item.label())
        .unwrap_or_else(|| "(no item)".to_string());

    let bpm = state.practice_bpm;
    let bars = state.practice_bars_per_item;
    let playing = state.practice_playing;

    // Compute the fretboard positions for the current item.
    let (positions, labels) = state
        .current_practice_item()
        .map(|item| positions_for_practice_item(item, &state.fretboard))
        .unwrap_or_default();
    let board = state.fretboard.clone();

    // Progress within the item: how far through the bars we are.
    let secs_per_item = (60.0 / bpm.max(1.0)) * 4.0 * bars as f32;
    let progress_text = if playing {
        let bar_now = ((state.practice_elapsed_secs / secs_per_item.max(0.001) * bars as f32)
            .floor() as u32
            + 1)
        .min(bars as u32);
        format!(
            "Item {} / {}  ·  bar {} / {}",
            item_idx + 1,
            item_count,
            bar_now,
            bars
        )
    } else if item_count > 0 {
        format!("Item {} / {}", item_idx + 1, item_count)
    } else {
        "no items".to_string()
    };

    // Next-item preview so users can mentally prepare for the change.
    let next_preview = state
        .current_practice_set()
        .filter(|s| s.items.len() > 1)
        .map(|s| {
            let next_idx = (item_idx + 1) % s.items.len();
            format!("Up next: {}", s.items[next_idx].label())
        })
        .unwrap_or_default();

    // Four distinct shapes for the transport slot now that the engine-
    // unavailable arm uses `danger_label` and the empty-set arm uses
    // `disabled_label` — different opaque view types, so they need
    // separate `OneOf4` variants.
    let transport = if let Err(e) = &state.engine {
        OneOf4::A(danger_prose(state.palette, format!("Audio engine unavailable: {e}"), TS_XS))
    } else if playing {
        OneOf4::B(text_button("■ Stop", |s: &mut AppState| {
            s.practice_playing = false;
            s.stop_practice_click();
        }))
    } else if item_count == 0 {
        OneOf4::C(disabled_label(state.palette, "Pick a set first.", TS_XS))
    } else {
        OneOf4::D(text_button("▶ Play", |s: &mut AppState| {
            s.practice_playing = true;
            s.practice_elapsed_secs = 0.0;
            s.start_practice_click();
        }))
    };

    // Practice-set combobox — jump-pick from the catalog. Walking is
    // less useful here than for scales/chords (sets are coarser units)
    // but the ◀/▶ are kept for parity with other tabs.
    let set_options: Vec<ArcStr> = state
        .practice_sets
        .iter()
        .map(|set| ArcStr::from(set.name.clone()))
        .collect();
    let set_selected = state.practice_selected_set.min(set_count.saturating_sub(1).max(0));
    let practice_open_combo = state.open_combobox;

    let info_panel = flex_col((
        header_label(state.palette, set_name, TS_LG),
        prose(set_desc).text_size(TS_XS),
        flex_row((
            combobox(
                "practice.set",
                "Set: ",
                &set_options,
                set_selected,
                practice_open_combo,
                |s: &mut AppState, i: usize| {
                    s.practice_selected_set = i;
                    s.practice_item_idx = 0;
                    s.practice_elapsed_secs = 0.0;
                },
            ),
            button_sm("◀", move |s: &mut AppState| {
                if set_count > 0 {
                    let cur = s.practice_selected_set.min(set_count - 1) as i32;
                    s.practice_selected_set =
                        ((cur - 1).rem_euclid(set_count as i32)) as usize;
                    s.practice_item_idx = 0;
                    s.practice_elapsed_secs = 0.0;
                }
            }),
            button_sm("▶", move |s: &mut AppState| {
                if set_count > 0 {
                    let cur = s.practice_selected_set.min(set_count - 1) as i32;
                    s.practice_selected_set =
                        ((cur + 1).rem_euclid(set_count as i32)) as usize;
                    s.practice_item_idx = 0;
                    s.practice_elapsed_secs = 0.0;
                }
            }),
            label(format!("({set_idx} of {set_count})")).text_size(TS_XS),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_2),
        // BPM picker — double-click the readout to edit, slider for
        // drag, ± for clicky tweaks.
        editable_big_number(
            state,
            "practice.bpm",
            format!("Tempo: {:.0} BPM", bpm),
            format!("{:.0}", bpm),
            TS_SM,
            |s: &mut AppState, v: f64| {
                let b = (v as f32).clamp(30.0, 240.0);
                s.practice_bpm = b;
                if let (true, Ok((_, h))) = (s.practice_playing, &s.engine) {
                    h.set_bpm(b);
                }
            },
        ),
        sized_box(slider(30.0, 240.0, bpm as f64, |s: &mut AppState, v: f64| {
            let b = (v as f32).clamp(30.0, 240.0);
            s.practice_bpm = b;
            if let (true, Ok((_, h))) = (s.practice_playing, &s.engine) {
                h.set_bpm(b);
            }
        }))
        .fixed_width(masonry::layout::Length::px(360.0)),
        flex_row((
            text_button("−", |s: &mut AppState| {
                s.practice_bpm = (s.practice_bpm - 1.0).clamp(30.0, 240.0);
                if let (true, Ok((_, h))) = (s.practice_playing, &s.engine) {
                    h.set_bpm(s.practice_bpm);
                }
            }),
            text_button("+", |s: &mut AppState| {
                s.practice_bpm = (s.practice_bpm + 1.0).clamp(30.0, 240.0);
                if let (true, Ok((_, h))) = (s.practice_playing, &s.engine) {
                    h.set_bpm(s.practice_bpm);
                }
            }),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_2),
        // Bars-per-item picker.
        flex_row((
            label(format!("Bars per item: {bars}")).text_size(TS_SM),
            text_button("−", |s: &mut AppState| {
                s.practice_bars_per_item = s.practice_bars_per_item.saturating_sub(1).max(1);
            }),
            text_button("+", |s: &mut AppState| {
                s.practice_bars_per_item = (s.practice_bars_per_item + 1).min(8);
            }),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_2),
        // Item transport.
        flex_row((
            text_button("◀◀ Prev", move |s: &mut AppState| {
                if item_count > 0 {
                    let cur = s.practice_item_idx.min(item_count - 1) as i32;
                    s.practice_item_idx =
                        ((cur - 1).rem_euclid(item_count as i32)) as usize;
                    s.practice_elapsed_secs = 0.0;
                }
            }),
            transport,
            text_button("Next ▶▶", move |s: &mut AppState| {
                if item_count > 0 {
                    let cur = s.practice_item_idx.min(item_count - 1) as i32;
                    s.practice_item_idx =
                        ((cur + 1).rem_euclid(item_count as i32)) as usize;
                    s.practice_elapsed_secs = 0.0;
                }
            }),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_2),
        // Current item label + progress.
        header_label(state.palette, item_label, TS_XL),
        dim_label(state.palette, progress_text, TS_XS),
        dim_label(state.palette, next_preview, TS_XS),
        FlexSpacer::Flex(1.0),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .main_axis_alignment(MainAxisAlignment::Start)
    .gap(SP_2);

    // Auto-advance task — fires every 50ms while playing, accumulates
    // elapsed_secs, advances item when full duration elapsed.
    let auto_task = playing.then(|| {
        task_raw(
            move |proxy, _| async move {
                let mut tick = time::interval(Duration::from_millis(50));
                tick.tick().await; // immediate first tick — skip
                loop {
                    tick.tick().await;
                    if proxy.message(()).is_err() {
                        break;
                    }
                }
            },
            move |s: &mut AppState, _: ()| {
                if !s.practice_playing {
                    return;
                }
                s.practice_elapsed_secs += 0.05;
                let secs_per_item =
                    (60.0 / s.practice_bpm.max(1.0)) * 4.0 * s.practice_bars_per_item as f32;
                if s.practice_elapsed_secs >= secs_per_item {
                    let count = s
                        .current_practice_set()
                        .map(|x| x.items.len())
                        .unwrap_or(0);
                    if count > 0 {
                        s.practice_item_idx = (s.practice_item_idx + 1) % count;
                    }
                    s.practice_elapsed_secs = 0.0;
                }
            },
        )
    });

    // Fretboard ↔ practice info use a draggable split, same as the
    // other fretboard tabs (Scales / Chords / Progressions /
    // Exercises). Cross-axis Start so opening anything tall on the
    // info side doesn't stretch the fretboard.
    use masonry::layout::Length as MLen;
    let visible = xilem::view::split(
        card(
            state.palette,
            sized_box(fretboard_view(
                board,
                positions,
                labels,
                state.diagram_colors(),
                None,
            ))
            .fixed_height(masonry::layout::Length::px(660.0)),
        ),
        card(state.palette, info_panel),
    )
    .split_point(0.5)
    .min_lengths(MLen::const_px(240.0), MLen::const_px(240.0));

    fork(visible, auto_task)
}

/// Translate a [`PracticeItem`] into the fretboard positions + labels
/// the visualization should display. Each variant has its own filter
/// rules (scale / chord position window, exercise dedup).
fn positions_for_practice_item(
    item: &PracticeItem,
    fretboard: &Fretboard,
) -> (Vec<Position>, Vec<String>) {
    match item {
        PracticeItem::Scale { formula, root, position } => {
            let all = fretboard
                .positions_for_scale(formula, *root)
                .unwrap_or_default();
            let window_end = position.saturating_add(4);
            let pos: Vec<Position> = all
                .into_iter()
                .filter(|p| p.fret == 0 || (p.fret >= *position && p.fret <= window_end))
                .collect();
            let labels = compute_labels(LabelMode::Notes, &pos);
            (pos, labels)
        }
        PracticeItem::Chord { formula, root, position } => {
            let all = fretboard
                .positions_for_chord(formula, *root)
                .unwrap_or_default();
            let window_end = position.saturating_add(4);
            let pos: Vec<Position> = all
                .into_iter()
                .filter(|p| p.fret == 0 || (p.fret >= *position && p.fret <= window_end))
                .collect();
            let labels = compute_labels(LabelMode::Notes, &pos);
            (pos, labels)
        }
        PracticeItem::Exercise { exercise, starting_fret } => {
            let steps = exercise.generate(
                &fretboard.tuning,
                &ExerciseParams {
                    starting_fret: *starting_fret,
                    direction: ExerciseDirection::Both,
                    trill_repeats: 8,
                },
            );
            let mut seen = std::collections::HashSet::new();
            let pos: Vec<Position> = steps
                .into_iter()
                .filter(|s| seen.insert((s.string_index, s.fret)))
                .map(|s| Position {
                    string_index: s.string_index,
                    fret: s.fret,
                    pitch: fretboard.pitch_at(s.string_index, s.fret),
                    interval_from_root: None,
                })
                .collect();
            let labels: Vec<String> = pos
                .iter()
                .map(|p| format!("{}{}", p.pitch.name, accidental_short(p.pitch.accidental)))
                .collect();
            (pos, labels)
        }
    }
}

/// Progressions tab — left column lists the catalog; middle shows the
/// fretboard for the currently-selected chord; right shows the
/// progression details + key picker + clickable chord cards.
///
/// Key controls live on the right column to match the Scales/Chords
/// layout. Scale (Major / Minor / mode) is hardcoded to Major for
/// now — a key-mode picker is a follow-up if needed.
fn progressions_view(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    let key_root = state.progression_key_pc.to_pitch(4);
    // Use the catalog's first scale as the key — that's "Major".
    // Hardcoding for now; mode picker can come later.
    let major_scale: &'static ScaleFormula = woodshedding::scale::catalog()
        .iter()
        .find(|s| s.name == "Major")
        .expect("woodshedding catalog has a Major scale");

    // Left: progression list — buttons stacked vertically.
    // Selecting a progression resets the per-chord voicing index
    // vec to the new length so voicing arrows stay in-bounds.
    let list_items: Vec<_> = progression_catalog()
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let chord_count = p.roles.len();
            let active = state.progression_idx == Some(i);
            let prefix = if active { "● " } else { "  " };
            text_button(format!("{}{}", prefix, p.name), move |s: &mut AppState| {
                s.progression_idx = Some(i);
                s.progression_expanded_chord = Some(0);
                s.progression_voicing_idx = vec![0; chord_count];
            })
        })
        .collect();
    let list_card = card(state.palette, 
        flex_col((
            label("Progressions").text_size(TS_MD),
            // Vertically-scrollable catalog — same pattern as the
            // Scales sidebar; see that comment for the rationale.
            portal(
                flex_col(list_items)
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_alignment(MainAxisAlignment::Start)
                    .gap(SP_1),
            )
            .constrain_horizontal(true)
            .flex(1.0),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_2),
    );

    // Build the materialized chord list + the currently-expanded chord.
    let materialized: Option<(&'static Progression, Vec<ProgressionChord>)> = state
        .progression_idx
        .and_then(|idx| progression_catalog().get(idx).copied().map(|p| (p, idx)))
        .and_then(|(prog, _)| {
            let prog_ref: &'static Progression = progression_catalog()
                .get(state.progression_idx.unwrap())
                .unwrap();
            match prog.apply_in_key(key_root, major_scale) {
                Ok(chords) => Some((prog_ref, chords)),
                Err(_) => None,
            }
        });

    let expanded_chord: Option<&ProgressionChord> =
        materialized
            .as_ref()
            .and_then(|(_, chords)| {
                let idx = state.progression_expanded_chord.unwrap_or(0);
                chords.get(idx)
            });

    // Middle: fretboard for the expanded chord's currently-selected
    // voicing. Empty when no progression is picked.
    let board = state.fretboard.clone();
    let n_strings = state.fretboard.tuning.strings.len();
    let expanded_idx = state.progression_expanded_chord.unwrap_or(0);
    let (positions, labels, dot_colors): (
        Vec<Position>,
        Vec<String>,
        Option<Vec<masonry::peniko::Color>>,
    ) = match (expanded_chord, materialized.as_ref()) {
        // Overlay mode: stack every chord's currently-selected
        // voicing on the same fretboard, each in its own hue. Reads
        // as a visual map of the whole progression rather than a
        // single chord shape. Selected voicing per chord follows the
        // per-card `progression_voicing_idx`, so what you've dialed
        // in on each card is what shows up in the overlay.
        (_, Some((_, chords))) if state.progression_overlay_mode => {
            let mut all_pos: Vec<Position> = Vec::new();
            let mut all_lbl: Vec<String> = Vec::new();
            let mut all_col: Vec<masonry::peniko::Color> = Vec::new();
            for (i, chord) in chords.iter().enumerate() {
                let voicings =
                    enumerate_voicings(&state.fretboard, chord.formula, chord.root);
                if voicings.is_empty() {
                    continue;
                }
                let v_idx = state
                    .progression_voicing_idx
                    .get(i)
                    .copied()
                    .unwrap_or(0)
                    .min(voicings.len() - 1);
                let pos = voicing_to_positions(&voicings[v_idx]);
                let lbl = compute_labels(LabelMode::Notes, &pos);
                let hue = chord_color(i);
                all_col.extend(std::iter::repeat_n(hue, pos.len()));
                all_pos.extend(pos);
                all_lbl.extend(lbl);
            }
            (all_pos, all_lbl, Some(all_col))
        }
        (Some(chord), Some(_)) => {
            let voicings = enumerate_voicings(&state.fretboard, chord.formula, chord.root);
            if voicings.is_empty() {
                (Vec::new(), Vec::new(), None)
            } else {
                let v_idx = state
                    .progression_voicing_idx
                    .get(expanded_idx)
                    .copied()
                    .unwrap_or(0)
                    .min(voicings.len() - 1);
                let pos = voicing_to_positions(&voicings[v_idx]);
                let lbl = compute_labels(LabelMode::Notes, &pos);
                let chord_hue = chord_color(expanded_idx);
                // Color every dot in the chord's assigned hue — same
                // color as that chord's card on the right.
                let colors: Vec<masonry::peniko::Color> =
                    (0..pos.len()).map(|_| chord_hue).collect();
                (pos, lbl, Some(colors))
            }
        }
        _ => (Vec::new(), Vec::new(), None),
    };
    let fretboard_card = card(state.palette, 
        sized_box(fretboard_view(
            board,
            positions,
            labels,
            state.diagram_colors(),
            dot_colors,
        ))
        .fixed_width(masonry::layout::Length::px(340.0))
        .fixed_height(masonry::layout::Length::px(660.0)),
    );

    // Right: progression info + key picker + chord cards column.
    let display_key = state.progression_key_pc.display();
    let chord_cards = match &materialized {
        Some((prog, chords)) => {
            let prog_name = prog.name.to_string();
            let prog_desc = prog.description.to_string();
            // Build one mini-card per chord. Each card carries:
            //   - chord symbol + role label + voicing N/M
            //   - the visual chord diagram (clickable: selects this
            //     chord as the "expanded" one on the main fretboard)
            //   - ◀ / ▶ arrows to cycle this chord's voicing
            let voicing_idx_vec = state.progression_voicing_idx.clone();
            let cards: Vec<_> = chords
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    let symbol = format_progression_chord_symbol(c);
                    let role = format_role(&c.role);
                    let active = state.progression_expanded_chord == Some(i);
                    let prefix = if active { "● " } else { "" };
                    let chord_hue = chord_color(i);
                    let voicings = enumerate_voicings(&state.fretboard, c.formula, c.root);
                    let voicing_count = voicings.len();
                    let v_idx = voicing_idx_vec
                        .get(i)
                        .copied()
                        .unwrap_or(0)
                        .min(voicing_count.saturating_sub(1));
                    let counter_text = if voicing_count == 0 {
                        "—".to_string()
                    } else {
                        format!("{}/{}", v_idx + 1, voicing_count)
                    };

                    // The diagram (or a placeholder) — wrapped in a
                    // button so clicking selects this chord.
                    let diagram_btn: OneOf2<_, _> = if voicings.is_empty() {
                        OneOf2::A(
                            sized_box(
                                label("no voicing").text_size(TS_XS),
                            )
                            .fixed_width(masonry::layout::Length::px(120.0))
                            .fixed_height(masonry::layout::Length::px(150.0)),
                        )
                    } else {
                        let v = voicings[v_idx].clone();
                        OneOf2::B(
                            button(
                                sized_box(chord_diagram_view(
                                    n_strings,
                                    v,
                                    chord_hue,
                                    state.diagram_colors(),
                                ))
                                .fixed_width(masonry::layout::Length::px(120.0))
                                .fixed_height(masonry::layout::Length::px(150.0)),
                                move |s: &mut AppState| {
                                    s.progression_expanded_chord = Some(i);
                                },
                            ),
                        )
                    };

                    let arrows = flex_row((
                        button_sm("◀", move |s: &mut AppState| {
                            if voicing_count > 0 {
                                while s.progression_voicing_idx.len() <= i {
                                    s.progression_voicing_idx.push(0);
                                }
                                let cur = s.progression_voicing_idx[i] as i32;
                                s.progression_voicing_idx[i] =
                                    ((cur - 1).rem_euclid(voicing_count as i32))
                                        as usize;
                                s.progression_expanded_chord = Some(i);
                            }
                        }),
                        dim_label(state.palette, counter_text, TS_XS),
                        button_sm("▶", move |s: &mut AppState| {
                            if voicing_count > 0 {
                                while s.progression_voicing_idx.len() <= i {
                                    s.progression_voicing_idx.push(0);
                                }
                                let cur = s.progression_voicing_idx[i] as i32;
                                s.progression_voicing_idx[i] =
                                    ((cur + 1).rem_euclid(voicing_count as i32))
                                        as usize;
                                s.progression_expanded_chord = Some(i);
                            }
                        }),
                    ))
                    .cross_axis_alignment(CrossAxisAlignment::Center)
                    .main_axis_alignment(MainAxisAlignment::Start)
                    .gap(SP_1);

                    // Wrap each chord card in a sized_box that hard-
                    // clamps its width. Without this, the natural
                    // width of the card column is max(chord-name,
                    // role, button-wrapped-diagram, arrows-row)
                    // which varies per-chord and includes button-
                    // default padding around the 120px diagram. The
                    // reflow math downstream needs a *known* per-card
                    // width to chunk correctly; otherwise rows
                    // overflow even when chunking says they should
                    // fit. 168 = 120 diagram + button padding/border
                    // (16px h + 2px border) + a few px breathing room.
                    sized_box(
                        flex_col((
                            label(format!("{prefix}{symbol}")).text_size(TS_MD),
                            dim_label(state.palette, role, TS_XS),
                            diagram_btn,
                            arrows,
                        ))
                        .cross_axis_alignment(CrossAxisAlignment::Start)
                        .main_axis_alignment(MainAxisAlignment::Start)
                        .gap(SP_1),
                    )
                    .fixed_width(masonry::layout::Length::px(CHORD_CARD_W))
                })
                .collect();
            // Pick the number of cards per row from the panel's
            // observed width. Each card is roughly 150px wide with
            // 10px gap; clamp to at least 1.
            // Adaptive chunking — `panel_width_tracker` reports the
            // column's allocated cross-width on the previous frame
            // (see definition below), and we divide by the actual
            // observed card-render width to pick `cards_per_row`.
            //
            // Budget = `CHORD_CARD_W + SP_2` (the row's gap). Each
            // row uses up to `cards_per_row * CHORD_CARD_W +
            // (cards_per_row - 1) * SP_2` pixels — the `+ SP_2` in
            // the numerator accounts for the missing trailing gap.
            let panel_w = state.progression_cards_panel_width;
            let cards_per_row = if panel_w < 1.0 {
                2 // Conservative default before resize_observer fires
            } else {
                let n = ((panel_w + 8.0) / (CHORD_CARD_W + 8.0)).floor() as usize;
                n.max(1)
            };
            // Chunk the card vec into rows of `cards_per_row`. Each
            // row is its own flex_row; the outer is a flex_col.
            let mut rows: Vec<_> = Vec::new();
            let mut buf: Vec<_> = Vec::with_capacity(cards_per_row);
            for c in cards {
                buf.push(c);
                if buf.len() == cards_per_row {
                    rows.push(
                        flex_row(std::mem::take(&mut buf))
                            .cross_axis_alignment(CrossAxisAlignment::Start)
                            .main_axis_alignment(MainAxisAlignment::Start)
                            .gap(SP_2),
                    );
                }
            }
            if !buf.is_empty() {
                rows.push(
                    flex_row(buf)
                        .cross_axis_alignment(CrossAxisAlignment::Start)
                        .main_axis_alignment(MainAxisAlignment::Start)
                        .gap(SP_2),
                );
            }
            let chord_grid = flex_col(rows)
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_alignment(MainAxisAlignment::Start)
                .gap(SP_2);

            // Width tracker — invisible 0-height stretchy widget that
            // gets allocated the parent column's full cross-width.
            // Decoupled from `chord_grid` because the grid's natural
            // width is determined by its fixed-width card children,
            // which masonry's flex layout treats as an explicit length
            // that overrides Stretch. So observing the grid reports
            // the *content's* width, not the column's allocation.
            //
            // sized_box(label("")) has natural width = 0; with Stretch
            // it gets stretched to the column's cross-width, and
            // resize_observer reports that as `size.width`. The actual
            // reflow chunking happens above using the value stashed
            // on state from the previous frame.
            let panel_width_tracker = resize_observer(
                |s: &mut AppState, size: masonry::kurbo::Size| {
                    s.progression_cards_panel_width = size.width;
                },
                sized_box(label(""))
                    .fixed_height(masonry::layout::Length::const_px(0.0)),
            );

            // Build the 12-PC option list for the key picker. Cheap —
            // 12 small `ArcStr`s per frame; could be cached if it ever
            // shows up in a profile.
            let key_options: Vec<ArcStr> = ChromaticPc::ALL
                .iter()
                .map(|pc| ArcStr::from(pc.display()))
                .collect();
            let key_selected = ChromaticPc::ALL
                .iter()
                .position(|&pc| pc == state.progression_key_pc)
                .unwrap_or(0);
            let open_combo = state.open_combobox;

            OneOf2::A(
                flex_col((
                    // Width tracker sits at the top of the column. It's
                    // invisible (0 height, empty label) but participates
                    // in cross-axis Stretch — gets the column's full
                    // width and reports it through resize_observer.
                    // Drives the chord-card reflow.
                    panel_width_tracker,
                    header_label(state.palette, prog_name, TS_LG),
                    prose(prog_desc).text_size(TS_SM),
                    // Combobox picker for the progression key — replaces
                    // the old `◀ Key: C ▶` cycle row. The ▲/▼ arrows on
                    // the trigger toggle the inline option list; clicking
                    // an option commits + closes.
                    flex_row((
                        combobox(
                            "progressions.key",
                            "Key: ",
                            &key_options,
                            key_selected,
                            open_combo,
                            |s: &mut AppState, i: usize| {
                                s.progression_key_pc = ChromaticPc::ALL[i];
                            },
                        ),
                        // Keep the ◀/▶ cycle as a fine-tune affordance
                        // — chromatic neighbour walking is faster than
                        // re-opening the picker.
                        button_sm("◀", |s: &mut AppState| {
                            s.progression_key_pc = s.progression_key_pc.cycle(-1);
                        }),
                        button_sm("▶", |s: &mut AppState| {
                            s.progression_key_pc = s.progression_key_pc.cycle(1);
                        }),
                        FlexSpacer::Fixed(SP_2),
                        // Overlay-mode toggle. Label encodes current
                        // state so the button's own text reads as the
                        // affordance ("Overlay: off" / "Overlay: on")
                        // — same pattern used by the detector switch
                        // on the Tuner tab.
                        text_button(
                            if state.progression_overlay_mode {
                                "Overlay: on"
                            } else {
                                "Overlay: off"
                            },
                            |s: &mut AppState| {
                                s.progression_overlay_mode = !s.progression_overlay_mode;
                            },
                        ),
                    ))
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_alignment(MainAxisAlignment::Start)
                    .gap(SP_2),
                    label("Chords").text_size(TS_MD),
                    chord_grid,
                    FlexSpacer::Flex(1.0),
                ))
                // Stretch so the chord_grid resize_observer sees the
                // *allocated* width (the column's full width) and not
                // the natural width of the row of cards (which would
                // overflow and report >parent).
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .main_axis_alignment(MainAxisAlignment::Start)
                .gap(SP_2),
            )
        }
        None => OneOf2::B(
            flex_col((
                label("Pick a progression").text_size(TS_MD),
                prose(
                    "Choose a chord progression from the list on the left. \
                     The chords get materialized in the chosen key, and \
                     you can expand any chord onto the fretboard.",
                )
                .text_size(TS_XS),
                flex_row((
                    label(format!("Key: {display_key}")).text_size(TS_SM),
                    button_sm("◀", |s: &mut AppState| {
                        s.progression_key_pc = s.progression_key_pc.cycle(-1);
                    }),
                    button_sm("▶", |s: &mut AppState| {
                        s.progression_key_pc = s.progression_key_pc.cycle(1);
                    }),
                ))
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .main_axis_alignment(MainAxisAlignment::Start)
                .gap(SP_2),
                FlexSpacer::Flex(1.0),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_alignment(MainAxisAlignment::Start)
            .gap(SP_2),
        ),
    };

    // Sidebar visibility — collapsed = render an empty zero-width
    // container so the flex_row tuple type stays stable; expanded =
    // 220px-wide list card. Both branches are `SizedBox<...>` so the
    // OneOf2 wrapper type-checks inside the flex_row tuple.
    let sidebar: OneOf2<_, _> = if state.sidebars.is_collapsed(Tab::Progressions) {
        OneOf2::A(
            sized_box(label(""))
                .fixed_width(SP_0),
        )
    } else {
        OneOf2::B(
            sized_box(list_card)
                .fixed_width(masonry::layout::Length::px(220.0)),
        )
    };

    // Fretboard + chord_cards use xilem's `split` view — a hard,
    // fraction-based two-pane split with a draggable bar between.
    // We tried `flex(1.0)` on both sections in a `flex_row`; the
    // chord_grid's natural width (sum of fixed-width chord cards)
    // dominated the flex distribution and pushed chord-cards content
    // off-screen. `split` ignores natural widths and just gives each
    // child a fixed fraction of the available main-axis space.
    //
    // 0.5 fraction = 50/50 split; user can drag the bar to adjust.
    // `min_lengths` keeps each side from being collapsed to zero
    // (which would hide content entirely with no way to recover
    // without resizing the window).
    use masonry::layout::Length as MLen;
    flex_row((
        sidebar,
        xilem::view::split(fretboard_card, card(state.palette, chord_cards))
            .split_point(0.5)
            .min_lengths(MLen::const_px(240.0), MLen::const_px(280.0))
            .flex(1.0),
    ))
    // Cross-axis Start (not Stretch) — so the fretboard side stays
    // at its natural height instead of growing whenever the other
    // side gets taller (e.g. when a combobox dropdown opens, the
    // chord_cards card grows vertically and would otherwise drag the
    // fretboard along with it).
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .main_axis_alignment(MainAxisAlignment::Start)
    .gap(SP_4)
}

/// "C", "Dm", "G7", "Am7" etc. — root pitch class + chord symbol.
fn format_progression_chord_symbol(c: &ProgressionChord) -> String {
    format!(
        "{}{}{}",
        c.root.name,
        accidental_short(c.root.accidental),
        c.formula.symbol
    )
}

/// Compact role label: "I", "ii", "V7", etc. Reads the Progression's
/// stored Roman numeral if available; falls back to the degree number.
fn format_role(role: &woodshedding::progression::ChordRole) -> String {
    use woodshedding::progression::RoleQuality;
    let lowercase = matches!(
        role.quality,
        RoleQuality::Minor
            | RoleQuality::Diminished
            | RoleQuality::Minor7
            | RoleQuality::HalfDiminished7
            | RoleQuality::Diminished7
            | RoleQuality::Minor6
    );
    let numeral = roman_numeral(role.degree, lowercase);
    let suffix = match role.quality {
        RoleQuality::Major => "",
        RoleQuality::Minor => "",
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
    format!("{numeral}{suffix}")
}

/// Translate (pitch-class, formula-name) into a Song-mode [`ChordRef`].
/// Looks the formula up in the woodshedding catalog and computes the
/// chord-tone frequencies — the audio crate doesn't know about
/// woodshedding's theory model, so the app bridges them.
fn make_chord_ref(pc: ChromaticPc, octave: i8, formula_name: &str) -> ChordRef {
    let root = pc.to_pitch(octave);
    let root_freq = root.frequency() as f32;
    let formula = chord_catalog()
        .iter()
        .find(|f| f.name == formula_name);
    let mut pitches = vec![root_freq];
    if let Some(formula) = formula {
        for interval in formula.intervals.iter() {
            if let Ok(p) = root.transposed_by(*interval) {
                pitches.push(p.frequency() as f32);
            }
        }
    }
    let symbol = formula
        .map(|f| f.symbol)
        .filter(|s| !s.is_empty())
        .unwrap_or(match formula_name {
            "Major" => "",
            "Minor" => "m",
            "Dominant 7" => "7",
            _ => "",
        });
    let label = format!("{}{}{}", pc.note_name(), pc.accidental_str(), symbol);
    ChordRef {
        formula_name: formula_name.to_string(),
        root_freq_hz: root_freq,
        pitches_hz: pitches,
        label,
    }
}

/// Power-of-two time-signature denominators we cycle through (whole
/// .. sixteenth-note beat units).
const TIME_DENOMINATORS: [u8; 5] = [1, 2, 4, 8, 16];

/// Step the denominator down/up within [`TIME_DENOMINATORS`], clamping
/// at the ends (no wraparound — clicking past the edge is a no-op).
fn prev_denominator(d: u8) -> u8 {
    let i = TIME_DENOMINATORS.iter().position(|&x| x == d).unwrap_or(2);
    TIME_DENOMINATORS[i.saturating_sub(1)]
}

fn next_denominator(d: u8) -> u8 {
    let i = TIME_DENOMINATORS.iter().position(|&x| x == d).unwrap_or(2);
    TIME_DENOMINATORS[(i + 1).min(TIME_DENOMINATORS.len() - 1)]
}

/// Range of root octaves offered in the chord-root picker (C1..C6).
const CHORD_OCTAVE_RANGE: std::ops::RangeInclusive<i8> = 1..=6;

/// Current (root pc, octave) of the bar's chord, or the Progressions
/// key at octave 4 when the bar has no chord yet.
fn bar_chord_root(s: &AppState, idx: usize) -> (ChromaticPc, i8) {
    s.song_view
        .bars
        .get(idx)
        .and_then(|b| b.chord_ref.as_ref())
        .map(|c| chord_root_from_freq(c.root_freq_hz))
        .unwrap_or((s.progression_key_pc, 4))
}

/// Apply an in-place loop-shaping op to the selected bar's recorded
/// audio buffer (no-op when the bar has no audio), then refresh the
/// cached view so the waveform redraws.
fn sample_op(s: &mut AppState, op: impl FnOnce(&mut woodshed_audio::SampleBuffer)) {
    let target = s.song_selected_bar;
    if let Some(h) = s.ensure_song_engine() {
        h.with_song(move |x| {
            if let Ok(b) = x.bar_mut(target) {
                if let Some(buf) = b.audio_buffer.as_mut() {
                    op(buf);
                }
            }
        });
    }
    s.refresh_song_view();
}

/// Write a chord (or clear it) onto bar `idx` through the engine and
/// refresh the cached view so the lanes + label update immediately.
fn set_bar_chord(s: &mut AppState, idx: usize, chord: Option<ChordRef>) {
    if let Some(h) = s.ensure_song_engine() {
        h.with_song(move |x| {
            if let Ok(b) = x.bar_mut(idx) {
                b.chord_ref = chord;
            }
        });
    }
    s.refresh_song_view();
}

/// Recover the (pitch class, octave) a chord's root frequency came
/// from. `make_chord_ref` builds roots at a chosen octave, so we scan
/// every pc across the offered octave range and take the nearest
/// frequency — robust to the tiny float drift in stored
/// `root_freq_hz`.
fn chord_root_from_freq(freq: f32) -> (ChromaticPc, i8) {
    let mut best = (ChromaticPc::ALL[0], 4_i8);
    let mut best_diff = f32::INFINITY;
    for oct in CHORD_OCTAVE_RANGE {
        for pc in ChromaticPc::ALL {
            let diff = (pc.to_pitch(oct).frequency() as f32 - freq).abs();
            if diff < best_diff {
                best_diff = diff;
                best = (pc, oct);
            }
        }
    }
    best
}

/// Resolve a typed chord-quality string to a catalog index. Matches
/// (case-insensitively, trimmed) against each formula's full name or
/// its symbol, so "m7", "Minor 7", and "min7"-style entries all land
/// as long as they equal a catalog name or symbol. Returns `None` for
/// anything the catalog doesn't recognize — the caller rejects it,
/// guaranteeing every accepted entry renders.
fn formula_index_from_input(input: &str) -> Option<usize> {
    let needle = input.trim();
    if needle.is_empty() {
        return None;
    }
    let lower = needle.to_ascii_lowercase();
    chord_catalog().iter().position(|f| {
        f.name.eq_ignore_ascii_case(needle) || f.symbol.eq_ignore_ascii_case(&lower)
    })
}

/// Per-chord-position color palette for progressions.
///
/// Each chord card + its dots on the main fretboard share a hue so
/// the eye can track "which chord is this voicing for" across the
/// visual layout. Seven distinct hues — wraps around for
/// progressions longer than seven chords (rare).
fn chord_color(idx: usize) -> masonry::peniko::Color {
    const PALETTE: [(u8, u8, u8); 7] = [
        (0xE7, 0x6F, 0x51), // coral
        (0xE8, 0x9C, 0x3A), // amber
        (0xE2, 0xC5, 0x4B), // honey
        (0x6F, 0xC3, 0x70), // moss
        (0x4F, 0xB8, 0xC2), // cyan
        (0x5C, 0x8E, 0xE6), // sky
        (0xB0, 0x6F, 0xD7), // amethyst
    ];
    let (r, g, b) = PALETTE[idx % PALETTE.len()];
    masonry::peniko::Color::from_rgba8(r, g, b, 0xFF)
}

fn roman_numeral(degree: u8, lower: bool) -> &'static str {
    match (degree, lower) {
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
    }
}

/// Exercises tab — sequence-aware. Renders only the current step plus
/// a fading trail of the last few, so the user sees the *order* of
/// motion rather than a static rectangle of unique positions.
///
/// Two ways to drive the sequence:
/// - **Manual**: ◀ Step / Step ▶ buttons advance one position at a
///   time. Best for learning the pattern note-by-note.
/// - **Auto**: Play button starts a task that advances at the
///   chosen BPM. Best for practicing the exercise at tempo.
fn exercises_view(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    /// How many previous steps to render as a fading trail behind
    /// the current step. 4 = current + 3 history.
    const TRAIL_LEN: usize = 4;

    let ex = state.current_exercise();
    let params = ExerciseParams {
        starting_fret: state.exercise_starting_fret,
        direction: ExerciseDirection::Both,
        trill_repeats: 8,
    };
    let steps = ex.generate(&state.fretboard.tuning, &params);
    let step_count = steps.len();
    let current_idx = if step_count == 0 {
        0
    } else {
        state.exercise_step_idx.min(step_count - 1)
    };

    // Build the visible window: TRAIL_LEN most-recent steps up to and
    // including current_idx. If the same (string, fret) appears more
    // than once in the window, keep only the most-recent visit
    // (lowest age) so dots don't ghost-overlap.
    let mut window: Vec<(usize, u8, u8, usize)> = Vec::new(); // (string, fret, finger, age)
    let mut seen: std::collections::HashSet<(usize, u8)> =
        std::collections::HashSet::new();
    let window_start = current_idx.saturating_sub(TRAIL_LEN - 1);
    for i in (window_start..=current_idx).rev() {
        if let Some(step) = steps.get(i) {
            let key = (step.string_index, step.fret);
            if seen.insert(key) {
                let age = current_idx - i;
                window.push((step.string_index, step.fret, step.finger, age));
            }
        }
    }
    // Window is iterated newest-first because of the `.rev()`. Reverse
    // back to oldest-first so the painter draws oldest underneath and
    // current on top.
    window.reverse();

    let board_for_widget = state.fretboard.clone();
    let positions: Vec<Position> = window
        .iter()
        .map(|&(string_index, fret, _, _)| Position {
            string_index,
            fret,
            pitch: board_for_widget.pitch_at(string_index, fret),
            interval_from_root: None,
        })
        .collect();
    let labels: Vec<String> = window
        .iter()
        .map(|&(_, fret, finger, _)| {
            if fret == 0 {
                "0".to_string()
            } else if finger == 0 {
                String::new()
            } else {
                finger.to_string()
            }
        })
        .collect();
    // Per-dot colors: current step in the root-dot color, trail
    // entries in note-dot color with alpha decaying by age.
    let dot_colors: Vec<masonry::peniko::Color> = window
        .iter()
        .map(|&(_, _, _, age)| {
            if age == 0 {
                masonry::peniko::Color::from_rgba8(0x33, 0x66, 0xC8, 0xFF)
            } else {
                // Linear fade from ~0.85 at age 1 down to ~0.25 at
                // age TRAIL_LEN-1.
                let span = (TRAIL_LEN.saturating_sub(1)) as f32;
                let t = age as f32 / span.max(1.0);
                let alpha = (220.0 - t * 165.0) as u8;
                masonry::peniko::Color::from_rgba8(0x77, 0xAA, 0xDD, alpha)
            }
        })
        .collect();

    let exercise_name = ex.name.to_string();
    let exercise_desc = ex.description.to_string();
    let starting_fret = state.exercise_starting_fret;
    let current_finger = steps
        .get(current_idx)
        .map(|s| s.finger)
        .unwrap_or(0);
    let step_label = if step_count == 0 {
        "No steps".to_string()
    } else if current_finger == 0 {
        format!("Step {} / {}", current_idx + 1, step_count)
    } else {
        format!(
            "Step {} / {} (finger {})",
            current_idx + 1,
            step_count,
            current_finger
        )
    };
    let bpm = state.exercise_bpm;
    let bpm_text = format!("Tempo: {:.0} BPM", bpm);
    let playing = state.exercise_playing;

    let play_button = if playing {
        OneOf3::A(
            text_button("■ Stop", |s: &mut AppState| s.exercise_playing = false),
        )
    } else if step_count == 0 {
        OneOf3::B(disabled_label(state.palette, "No playable steps", TS_XS))
    } else {
        OneOf3::C(
            text_button("▶ Play", |s: &mut AppState| s.exercise_playing = true),
        )
    };

    // Exercise picker — combobox for jump + ◀/▶ for adjacent. Both
    // arms reset the step index and pause playback so switching
    // exercises doesn't strand the trail highlight on a stale step.
    let exercise_options: Vec<ArcStr> = exercise_catalog()
        .iter()
        .map(|e| ArcStr::from(e.name))
        .collect();
    let exercise_selected = state
        .exercise_idx
        .min(exercise_options.len().saturating_sub(1));
    let exercise_open_combo = state.open_combobox;

    let info_panel = flex_col((
        header_label(state.palette, exercise_name, TS_LG),
        prose(exercise_desc).text_size(TS_SM),
        flex_row((
            combobox(
                "exercises.exercise",
                "Exercise: ",
                &exercise_options,
                exercise_selected,
                exercise_open_combo,
                |s: &mut AppState, i: usize| {
                    s.exercise_idx = i;
                    s.exercise_step_idx = 0;
                    s.exercise_playing = false;
                },
            ),
            button_sm("◀", |s: &mut AppState| {
                s.cycle_exercise(-1);
                s.exercise_step_idx = 0;
                s.exercise_playing = false;
            }),
            button_sm("▶", |s: &mut AppState| {
                s.cycle_exercise(1);
                s.exercise_step_idx = 0;
                s.exercise_playing = false;
            }),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_2),
        flex_row((
            label(format!("Start fret: {starting_fret}")).text_size(TS_SM),
            button_sm("◀", |s: &mut AppState| {
                s.exercise_starting_fret =
                    s.exercise_starting_fret.saturating_sub(1).max(1);
                s.exercise_step_idx = 0;
            }),
            button_sm("▶", |s: &mut AppState| {
                let max = s.fretboard.fret_count.saturating_sub(4).max(1);
                s.exercise_starting_fret = (s.exercise_starting_fret + 1).min(max);
                s.exercise_step_idx = 0;
            }),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_2),
        // Step counter — display only.
        label(step_label).text_size(TS_MD),
        // Manual step transport.
        flex_row((
            text_button("◀ Step", move |s: &mut AppState| {
                if step_count > 0 {
                    let cur = s.exercise_step_idx.min(step_count - 1) as i32;
                    s.exercise_step_idx =
                        ((cur - 1).rem_euclid(step_count as i32)) as usize;
                    s.exercise_playing = false;
                }
            }),
            play_button,
            text_button("Step ▶", move |s: &mut AppState| {
                if step_count > 0 {
                    let cur = s.exercise_step_idx.min(step_count - 1) as i32;
                    s.exercise_step_idx =
                        ((cur + 1).rem_euclid(step_count as i32)) as usize;
                    s.exercise_playing = false;
                }
            }),
            text_button("⏮ Reset", |s: &mut AppState| {
                s.exercise_step_idx = 0;
            }),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_2),
        // BPM control — double-click the readout to edit, slider for
        // drag, ± for clicky tweaks. Same pattern as Metronome /
        // Practice.
        editable_big_number(
            state,
            "exercise.bpm",
            bpm_text.clone(),
            format!("{:.0}", bpm),
            TS_SM,
            |s: &mut AppState, v: f64| {
                s.exercise_bpm = (v as f32).clamp(30.0, 240.0);
            },
        ),
        sized_box(slider(30.0, 240.0, bpm as f64, |s: &mut AppState, v: f64| {
            s.exercise_bpm = (v as f32).clamp(30.0, 240.0);
        }))
        .fixed_width(masonry::layout::Length::px(300.0)),
        flex_row((
            text_button("−", |s: &mut AppState| {
                s.exercise_bpm = (s.exercise_bpm - 1.0).clamp(30.0, 240.0);
            }),
            text_button("+", |s: &mut AppState| {
                s.exercise_bpm = (s.exercise_bpm + 1.0).clamp(30.0, 240.0);
            }),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_2),
        FlexSpacer::Flex(1.0),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .main_axis_alignment(MainAxisAlignment::Start)
    .gap(SP_2);

    // Auto-advance task — only present while `playing`. Driving the
    // tick via tokio::time::interval; on each tick, message the
    // state handler which increments the step index modulo
    // step_count.
    let auto_task = (playing && step_count > 0).then(|| {
        let interval_ms = (60_000.0 / bpm.max(1.0)) as u64;
        task_raw(
            move |proxy, _| async move {
                let mut tick =
                    time::interval(Duration::from_millis(interval_ms.max(50)));
                // First tick fires immediately — skip it so we hold
                // on the current step for one beat before advancing.
                tick.tick().await;
                loop {
                    tick.tick().await;
                    if proxy.message(()).is_err() {
                        break;
                    }
                }
            },
            move |s: &mut AppState, _: ()| {
                if step_count > 0 {
                    s.exercise_step_idx = (s.exercise_step_idx + 1) % step_count;
                }
            },
        )
    });

    // Exercises catalog sidebar — same shape as Scales / Chords.
    // Clicking an entry switches the exercise, resets the step
    // index, and pauses playback so the trail highlight doesn't
    // strand on a stale step from the previous exercise.
    let exercise_list_items: Vec<_> = exercise_catalog()
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let active = state.exercise_idx == i;
            let prefix = if active { "● " } else { "  " };
            text_button(
                format!("{}{}", prefix, e.name),
                move |s: &mut AppState| {
                    s.exercise_idx = i;
                    s.exercise_step_idx = 0;
                    s.exercise_playing = false;
                },
            )
        })
        .collect();
    let exercise_list_card = card(
        state.palette,
        flex_col((
            label("Exercises").text_size(TS_MD),
            portal(
                flex_col(exercise_list_items)
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_alignment(MainAxisAlignment::Start)
                    .gap(SP_1),
            )
            .constrain_horizontal(true)
            .flex(1.0),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_2),
    );
    let exercise_sidebar: OneOf2<_, _> = if state.sidebars.is_collapsed(Tab::Exercises) {
        OneOf2::A(sized_box(label("")).fixed_width(SP_0))
    } else {
        OneOf2::B(
            sized_box(exercise_list_card)
                .fixed_width(masonry::layout::Length::px(220.0)),
        )
    };

    // Same split-view pattern as the other fretboard tabs.
    use masonry::layout::Length as MLen;
    let visible = flex_row((
        exercise_sidebar,
        xilem::view::split(
            card(
                state.palette,
                sized_box(fretboard_view(
                    board_for_widget,
                    positions,
                    labels,
                    state.diagram_colors(),
                    Some(dot_colors),
                ))
                .fixed_height(masonry::layout::Length::px(660.0)),
            ),
            card(state.palette, info_panel),
        )
        .split_point(0.5)
        .min_lengths(MLen::const_px(240.0), MLen::const_px(240.0))
        .flex(1.0),
    ))
    // Cross-axis Start (not Stretch) — so the fretboard side stays
    // at its natural height instead of growing whenever the other
    // side gets taller (e.g. when a combobox dropdown opens, the
    // chord_cards card grows vertically and would otherwise drag the
    // fretboard along with it).
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .main_axis_alignment(MainAxisAlignment::Start)
    .gap(SP_4);

    fork(visible, auto_task)
}

/// Metronome tab — BPM display + transport + time-sig / subdivision /
/// click-pattern / accent pickers. Each settings change rebuilds the
/// pattern and pushes it to the engine; if currently playing, the
/// pattern restarts from beat 1.
fn metronome_view(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    let bpm_text = format!("{:.0} BPM", state.bpm);
    let playing = state.metronome_playing;
    let engine_error = match &state.engine {
        Err(e) => Some(e.clone()),
        _ => None,
    };
    let time_sig_text = format!("Time: {}/4", state.metronome_time_sig_num);
    let click_text = state.metronome_click.label();
    let accent_text = state.metronome_accent.label();
    let sub_text = subdivision_label(state.metronome_subdivision);

    let transport = if let Some(err) = engine_error {
        OneOf3::A(danger_prose(state.palette, format!("Audio engine unavailable: {err}"), TS_XS))
    } else if playing {
        OneOf3::B(
            text_button("■ Stop", |s: &mut AppState| s.stop_metronome()),
        )
    } else {
        OneOf3::C(
            text_button("▶ Play", |s: &mut AppState| s.play_metronome()),
        )
    };

    let panel = flex_col((
        header_label(state.palette, "Metronome", TS_LG),
        // Big BPM readout — double-click to edit in place at the
        // same size/font. Mono so digits don't reshuffle as the
        // tempo steps. `setter` owns the clamp + engine push so the
        // editable helper stays field-agnostic.
        editable_big_number(
            state,
            "metronome.bpm",
            bpm_text.clone(),
            format!("{:.0}", state.bpm),
            TS_2XL,
            |s: &mut AppState, v: f64| {
                let b = (v as f32).clamp(40.0, 240.0);
                s.bpm = b;
                if let Ok((_, h)) = &s.engine {
                    h.set_bpm(b);
                }
            },
        ),
        // Slider — fast continuous BPM change. Sliders emit f64; we
        // cast to f32 on the boundary. Sets BPM in real time so the
        // click follows the slider as you drag.
        sized_box(slider(
            40.0,
            240.0,
            state.bpm as f64,
            |s: &mut AppState, v: f64| {
                let b = (v as f32).clamp(40.0, 240.0);
                s.bpm = b;
                if let Ok((_, h)) = &s.engine {
                    h.set_bpm(b);
                }
            },
        ))
        .fixed_width(masonry::layout::Length::px(360.0)),
        // ± / ±10 buttons for quick clicky tweaks. The dedicated
        // text-input + "Set" row is gone — precise entry now lives
        // on the big BPM readout itself (double-click to edit).
        flex_row((
            text_button("− 10", |s: &mut AppState| {
                s.bpm = (s.bpm - 10.0).clamp(40.0, 240.0);
                if let Ok((_, h)) = &s.engine {
                    h.set_bpm(s.bpm);
                }
            }),
            text_button("−", |s: &mut AppState| {
                s.bpm = (s.bpm - 1.0).clamp(40.0, 240.0);
                if let Ok((_, h)) = &s.engine {
                    h.set_bpm(s.bpm);
                }
            }),
            text_button("+", |s: &mut AppState| {
                s.bpm = (s.bpm + 1.0).clamp(40.0, 240.0);
                if let Ok((_, h)) = &s.engine {
                    h.set_bpm(s.bpm);
                }
            }),
            text_button("+ 10", |s: &mut AppState| {
                s.bpm = (s.bpm + 10.0).clamp(40.0, 240.0);
                if let Ok((_, h)) = &s.engine {
                    h.set_bpm(s.bpm);
                }
            }),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_2),
        transport,
        // Time signature picker.
        flex_row((
            label(time_sig_text).text_size(TS_SM).font(mono_family()),
            button_sm("◀", |s: &mut AppState| {
                s.metronome_time_sig_num =
                    (s.metronome_time_sig_num.saturating_sub(1)).max(1);
                s.apply_metronome_pattern();
            }),
            button_sm("▶", |s: &mut AppState| {
                s.metronome_time_sig_num = (s.metronome_time_sig_num + 1).min(12);
                s.apply_metronome_pattern();
            }),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_2),
        // Subdivision cycler.
        text_button(format!("Notes: {sub_text}"), |s: &mut AppState| {
            s.metronome_subdivision = cycle_subdivision(s.metronome_subdivision);
            s.apply_metronome_pattern();
        }),
        // Click pattern toggle.
        text_button(click_text, |s: &mut AppState| {
            s.metronome_click = s.metronome_click.next();
            s.apply_metronome_pattern();
        }),
        // Accent toggle.
        text_button(accent_text, |s: &mut AppState| {
            s.metronome_accent = s.metronome_accent.next();
            s.apply_metronome_pattern();
        }),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .main_axis_alignment(MainAxisAlignment::Start)
    .gap(SP_2);

    card(state.palette, panel)
}

/// Human-readable name for each subdivision shipping today.
fn subdivision_label(s: Subdivision) -> &'static str {
    if s == Subdivision::QUARTER {
        "quarter"
    } else if s == Subdivision::EIGHTH {
        "eighth"
    } else if s == Subdivision::SIXTEENTH {
        "sixteenth"
    } else if s == Subdivision::THIRTY_SECOND {
        "32nd"
    } else if s == Subdivision::EIGHTH_TRIPLET {
        "8th triplet"
    } else if s == Subdivision::SIXTEENTH_TRIPLET {
        "16th triplet"
    } else {
        "custom"
    }
}

/// Cycle through the shipped subdivisions in tempo-natural order.
fn cycle_subdivision(s: Subdivision) -> Subdivision {
    if s == Subdivision::QUARTER {
        Subdivision::EIGHTH
    } else if s == Subdivision::EIGHTH {
        Subdivision::SIXTEENTH
    } else if s == Subdivision::SIXTEENTH {
        Subdivision::THIRTY_SECOND
    } else if s == Subdivision::THIRTY_SECOND {
        Subdivision::EIGHTH_TRIPLET
    } else if s == Subdivision::EIGHTH_TRIPLET {
        Subdivision::SIXTEENTH_TRIPLET
    } else {
        Subdivision::QUARTER
    }
}

/// Tuner tab — pitch detection readout with start/stop transport.
/// Forks the visible UI with a polling task that updates
/// Settings tab — application-wide preferences (theme, persistence
/// path display, future MIDI / audio device pickers). Lives as a
/// regular tab rather than a modal because Xilem's modal story is
/// immature and a tab fits the navigation pattern; can be promoted
/// to a popover later without churn at the call sites.
fn settings_view(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    use theme::ThemeMode;

    // Theme picker. Each mode renders as its own button with a ●
    // prefix when active — same affordance pattern as the tunings
    // sidebar so the "currently-selected" indicator reads
    // consistently across the app.
    let active_mode = state.theme_mode;
    let on_builtin = state.active_user.is_none();
    let mut theme_btns: Vec<AnyFlexChild<AppState>> = Vec::new();
    for mode in ThemeMode::ALL {
        let is_active = on_builtin && mode == active_mode;
        let prefix = if is_active { "● " } else { "  " };
        let label_text = format!("{}{}", prefix, mode.label());
        theme_btns.push(
            text_button(label_text, move |s: &mut AppState| {
                s.set_theme(mode);
            })
            .into_any_flex(),
        );
    }

    // User themes — selectable, each with a remove button. Plus a
    // "new custom" that clones the active theme's seeds into an
    // editable copy.
    let mut user_btns: Vec<AnyFlexChild<AppState>> = Vec::new();
    for t in &state.user_themes {
        let name = t.name.clone();
        let is_active = state.active_user.as_deref() == Some(name.as_str());
        let prefix = if is_active { "● " } else { "  " };
        let sel_name = name.clone();
        user_btns.push(
            text_button(format!("{prefix}{name}"), move |s: &mut AppState| {
                s.set_user_theme(sel_name.clone());
            })
            .into_any_flex(),
        );
        let rm_name = name.clone();
        user_btns.push(
            button_sm("✕", move |s: &mut AppState| {
                s.remove_user_theme(&rm_name);
            })
            .into_any_flex(),
        );
    }
    user_btns.push(
        text_button("+ New custom", |s: &mut AppState| {
            s.new_user_theme();
        })
        .into_any_flex(),
    );

    // Seed editors — shown only when a user theme is active. Hex inputs
    // (the MVP color picker) for the four seed hues + a mode toggle +
    // a rename field; each commits on Enter and re-derives the palette
    // live. A future swatch/HSV picker can replace the hex inputs.
    let theme_editor: OneOf2<_, _> = match state
        .active_user
        .as_ref()
        .and_then(|n| state.user_themes.iter().find(|t| &t.name == n))
    {
        Some(def) => {
            // Controlled inputs: while a field is focused (`editing_field`
            // matches its id) its contents come from `editing_buffer`, so
            // a rebuild mid-type doesn't reset the widget to the stored
            // value (masonry resets the textbox to `contents` on any
            // rebuild). On Enter we validate + commit, then clear the
            // edit so contents revert to the (re-derived) stored value.
            // One row per seed: name, a live swatch, R/G/B sliders that
            // re-derive the whole palette as you drag, and a hex
            // readout. (Sliders are controlled — `on_change` writes
            // state every tick, so they don't suffer the textbox
            // reset-on-rebuild that hex inputs did.)
            let seed_fields: [(&str, String, u8); 4] = [
                ("Primary", def.primary.clone(), 0),
                ("Secondary", def.secondary.clone(), 1),
                ("Tertiary", def.tertiary.clone(), 2),
                ("Neutral", def.neutral.clone(), 3),
            ];
            let px = masonry::layout::Length::const_px;
            let border = state.palette.surface_2;
            let mut rows: Vec<AnyFlexChild<AppState>> = Vec::new();
            for (lbl, stored, idx) in seed_fields {
                let col = audio_widgets::theme::color_from_hex(&stored)
                    .unwrap_or(Color::from_rgb8(0x80, 0x80, 0x80));
                let [r, g, b, _] = col.to_rgba8().to_u8_array();
                let swatch = sized_box(label(""))
                    .fixed_width(px(28.0))
                    .fixed_height(px(20.0))
                    .background_color(col)
                    .corner_radius(px(4.0))
                    .border(border, px(1.0));
                let chan = move |channel: u8, val: u8| {
                    sized_box(slider(0.0, 255.0, val as f64, move |s: &mut AppState, v: f64| {
                        s.set_seed_channel(idx, channel, v.round().clamp(0.0, 255.0) as u8);
                    }))
                    .fixed_width(px(96.0))
                };
                rows.push(
                    flex_row((
                        sized_box(dim_label(state.palette, lbl, TS_XS)).fixed_width(px(70.0)),
                        swatch,
                        chan(0, r),
                        chan(1, g),
                        chan(2, b),
                        label(stored).text_size(TS_XS).font(mono_family()),
                    ))
                    .cross_axis_alignment(CrossAxisAlignment::Center)
                    .gap(SP_2)
                    .into_any_flex(),
                );
            }
            // Text tiers — header + body. Each toggles Derived ↔ Custom;
            // when Custom, a swatch + R/G/B sliders edit the override
            // (which also cascades to the derived dim/disabled tiers).
            let mut text_rows: Vec<AnyFlexChild<AppState>> = Vec::new();
            for (is_header, lbl, ovr) in [
                (true, "Header", def.text_header.clone()),
                (false, "Body", def.text_body.clone()),
            ] {
                let custom = ovr.is_some();
                let toggle = text_button(
                    format!("{lbl}: {}", if custom { "Custom" } else { "Derived" }),
                    move |s: &mut AppState| s.toggle_text_override(is_header),
                );
                match ovr {
                    Some(hex) => {
                        let col = audio_widgets::theme::color_from_hex(&hex)
                            .unwrap_or(Color::from_rgb8(0x80, 0x80, 0x80));
                        let [r, g, b, _] = col.to_rgba8().to_u8_array();
                        let swatch = sized_box(label(""))
                            .fixed_width(px(28.0))
                            .fixed_height(px(20.0))
                            .background_color(col)
                            .corner_radius(px(4.0))
                            .border(border, px(1.0));
                        let chan = move |channel: u8, val: u8| {
                            sized_box(slider(0.0, 255.0, val as f64, move |s: &mut AppState, v: f64| {
                                s.set_text_channel(is_header, channel, v.round().clamp(0.0, 255.0) as u8);
                            }))
                            .fixed_width(px(96.0))
                        };
                        text_rows.push(
                            flex_row((
                                sized_box(toggle).fixed_width(px(120.0)),
                                swatch,
                                chan(0, r),
                                chan(1, g),
                                chan(2, b),
                                label(hex).text_size(TS_XS).font(mono_family()),
                            ))
                            .cross_axis_alignment(CrossAxisAlignment::Center)
                            .gap(SP_2)
                            .into_any_flex(),
                        );
                    }
                    None => text_rows.push(toggle.into_any_flex()),
                }
            }
            let dark = def.dark;
            let editing_name = state.editing_field == Some("theme.name");
            let name_contents = if editing_name {
                state.editing_buffer.clone()
            } else {
                def.name.clone()
            };
            OneOf2::A(
                flex_col((
                    flex_row((
                        dim_label(state.palette, "Name", TS_XS),
                        text_input(name_contents, |s: &mut AppState, t| {
                            s.editing_field = Some("theme.name");
                            s.editing_buffer = t;
                        })
                        .on_enter(|s: &mut AppState, text| {
                            let t = text.trim().to_string();
                            if !t.is_empty() {
                                s.edit_active_user(|d| d.name = t);
                            }
                            s.editing_field = None;
                        }),
                    ))
                    .cross_axis_alignment(CrossAxisAlignment::Center)
                    .gap(SP_2),
                    text_button(
                        if dark { "Mode: Dark" } else { "Mode: Light" },
                        |s: &mut AppState| s.edit_active_user(|d| d.dark = !d.dark),
                    ),
                    flex_col(rows)
                        .cross_axis_alignment(CrossAxisAlignment::Start)
                        .gap(SP_1),
                    flex_col(text_rows)
                        .cross_axis_alignment(CrossAxisAlignment::Start)
                        .gap(SP_1),
                ))
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .gap(SP_2),
            )
        }
        None => OneOf2::B(dim_prose(
            state.palette,
            "Pick a built-in, or \"+ New custom\" to fork the current \
             theme and edit its seed colors (hex) live.",
            TS_XS,
        )),
    };

    // Persistence path display — shows where Settings::load/save
    // reads/writes. Useful for power users who want to back up or
    // hand-edit the JSON. `None` means the platform didn't expose a
    // config dir, which is extremely rare (stripped embedded
    // targets); the line still renders so the user sees the
    // state-disabled case is real.
    let save_path_text = settings::state_path()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "(unavailable on this platform)".to_string());

    card(
        state.palette,
        flex_col((
            header_label(state.palette, "Settings", TS_LG),
            // Multi-sentence captions use `dim_prose` so they wrap
            // to the card's width rather than overflowing past the
            // right edge (the bug `dim_label` produced — labels
            // don't word-wrap).
            dim_prose(
                state.palette,
                "Application-wide preferences. Changes apply immediately \
                 and persist across restarts.",
                TS_XS,
            ),
            // Theme section.
            label("Theme").text_size(TS_MD),
            flex_row(theme_btns)
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .main_axis_alignment(MainAxisAlignment::Start)
                .gap(SP_2),
            flex_row(user_btns)
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .main_axis_alignment(MainAxisAlignment::Start)
                .gap(SP_2),
            theme_editor,
            dim_prose(
                state.palette,
                "Re-skins every card, label, and chord diagram from the \
                 active palette. Switches live — no restart. Built-ins \
                 can't be removed or edited directly; \"+ New custom\" \
                 forks the active theme into an editable copy.",
                TS_XS,
            ),
            // Persistence section. Reset and explicit save sit here
            // alongside the path so power users have one place to
            // think about durable state.
            label("Persistence").text_size(TS_MD),
            dim_label(state.palette, "Settings file:", TS_XS),
            // The path itself stays a `label` (mono) — it's a single
            // token, not prose, and we want it to read continuously
            // rather than wrap at arbitrary slashes.
            label(save_path_text)
                .text_size(TS_XS)
                .font(mono_family()),
            flex_row((
                text_button("Save now", |s: &mut AppState| {
                    s.snapshot_settings().save();
                }),
                text_button("Reset to defaults", |s: &mut AppState| {
                    let defaults = Settings::default();
                    s.apply_settings(defaults);
                }),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .main_axis_alignment(MainAxisAlignment::Start)
            .gap(SP_2),
            dim_prose(
                state.palette,
                "Settings auto-save on quit. \"Save now\" flushes \
                 immediately; \"Reset to defaults\" reverts every \
                 picker, tab, and tempo to the cold-start values \
                 (audio state, current tuning analysis, and \
                 in-flight playback are untouched).",
                TS_XS,
            ),
            FlexSpacer::Flex(1.0),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_3),
    )
}

/// `state.tuner_snapshot` every 50ms while the tuner is active.
///
/// Custom widgets (CentsMeter needle, LevelMeter bar) are the next
/// follow-up — this pass uses text readouts to validate the audio
/// pipeline end-to-end first.
fn tuner_view(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    let listening = state.tuner_active;
    let tuner_handle = state.input.as_ref().ok().map(|b| b.tuner.clone());
    let error_text = match &state.input {
        Err(e) => Some(e.clone()),
        _ => None,
    };

    // Build the readout from the cached snapshot. Text-only for now;
    // CentsMeter / LevelMeter come next.
    let snapshot = state.tuner_snapshot.clone();
    let note_text = match &snapshot {
        Some(s) => match &s.note {
            Some(n) => format_detected_note(n),
            None => "—".to_string(),
        },
        None => "—".to_string(),
    };
    let cents_text = snapshot
        .as_ref()
        .and_then(|s| s.note.as_ref().map(|n| format!("{:+.1} cents", n.cents_offset)))
        .unwrap_or_else(|| "no detection".to_string());
    let level_text = snapshot
        .as_ref()
        .map(|s| format!("level: {:.3}", s.input_level))
        .unwrap_or_else(|| "level: —".to_string());

    let transport = if let Some(err) = error_text {
        OneOf3::A(danger_prose(state.palette, format!("Tuner unavailable: {err}"), TS_XS))
    } else if listening {
        OneOf3::B(text_button("Stop tuner", |s: &mut AppState| s.stop_tuner()))
    } else {
        OneOf3::C(text_button("Start tuner", |s: &mut AppState| s.start_tuner()))
    };

    // Input level as a progress bar — 0..1 RMS mapped to 0..1
    // progress. Clamped because RMS can theoretically exceed 1.0
    // briefly under loud transients.
    let level_value = state
        .tuner_snapshot
        .as_ref()
        .map(|s| (s.input_level as f64).clamp(0.0, 1.0))
        .unwrap_or(0.0);
    // Raw cents offset for the cents meter widget. `None` triggers
    // the "no signal" rendering (track + ticks, no needle).
    let cents_raw: Option<f64> = state
        .tuner_snapshot
        .as_ref()
        .and_then(|s| s.note.as_ref())
        .map(|n| n.cents_offset);
    // Raw level for the level meter. `None` when no snapshot has
    // landed yet (tuner not started).
    let level_raw: Option<f64> = state
        .tuner_snapshot
        .as_ref()
        .map(|s| s.input_level as f64);
    let in_tune = state
        .tuner_snapshot
        .as_ref()
        .and_then(|s| s.note.as_ref())
        .map(|n| n.in_tune)
        .unwrap_or(false);

    // String-target row — one button per string of the current
    // tuning, plus a "Free" button to clear the target hint.
    // Each button has a distinct closure type, so we erase them
    // into `AnyFlexChild` to share a Vec.
    let mut string_btns: Vec<AnyFlexChild<AppState>> = Vec::new();
    string_btns.push(
        text_button("Free", |s: &mut AppState| {
            s.tuner_target = None;
            if let Ok(b) = &s.input {
                b.tuner.set_target_hint(None);
            }
        })
        .into_any_flex(),
    );
    for p in state.fretboard.tuning.strings.iter() {
        let pitch = *p;
        let lbl = format!(
            "{}{}{}",
            pitch.name,
            accidental_short(pitch.accidental),
            pitch.octave
        );
        let active = state.tuner_target
            == Some(ChromaticPc::from_pc(pitch.pitch_class() as u8).to_detected());
        let prefix = if active { "● " } else { "" };
        let label_text: ArcStr = format!("{prefix}{lbl}").into();
        string_btns.push(
            text_button(label_text, move |s: &mut AppState| {
                let hint =
                    ChromaticPc::from_pc(pitch.pitch_class() as u8).to_detected();
                s.tuner_target = Some(hint.clone());
                if let Ok(b) = &s.input {
                    b.tuner.set_target_hint(Some(hint));
                }
            })
            .into_any_flex(),
        );
    }

    let detector_label = match state.tuner_detector {
        DetectorKind::Fft => "Detector: FFT",
        DetectorKind::Cepstrum => "Detector: Cepstrum",
        DetectorKind::McLeod => "Detector: McLeod",
    };

    // Tunings catalog sidebar — list every named tuning in the
    // current instrument's catalog, grouped by category (Standard,
    // Dropped, Open, Modal, etc). Click applies. Active tuning gets
    // a ● prefix. Sidebar collapsible via the header hamburger;
    // collapsed state lives on `SidebarVisibility.tuner`.
    use std::collections::BTreeMap;
    let active_tuning_name = state.fretboard.tuning.name.clone();
    let active_instrument = state.active_instrument;
    let mut by_category: BTreeMap<
        woodshedding::tuning::TuningCategory,
        Vec<&'static woodshedding::tuning::TuningSpec>,
    > = BTreeMap::new();
    for spec in tuning_catalog().iter().filter(|s| s.instrument == active_instrument) {
        by_category.entry(spec.category).or_default().push(spec);
    }
    let mut tunings_items: Vec<AnyFlexChild<AppState>> = Vec::new();
    for (category, specs) in &by_category {
        // Category heading — dim, slightly indented from the buttons
        // visually by virtue of being a plain label not a button.
        tunings_items.push(
            dim_label(state.palette, category.to_string(), TS_XS).into_any_flex(),
        );
        for spec in specs {
            let spec = **spec; // copy out of the &&TuningSpec
            let active = spec.name == active_tuning_name;
            let prefix = if active { "● " } else { "  " };
            tunings_items.push(
                text_button(
                    format!("{}{}", prefix, spec.name),
                    move |s: &mut AppState| {
                        let tuning = Tuning::from_spec(&spec);
                        s.fretboard = Fretboard::new(tuning, 12);
                    },
                )
                .into_any_flex(),
            );
        }
    }
    let tunings_list_card = card(
        state.palette,
        flex_col((
            label("Tunings").text_size(TS_MD),
            // Vertical-scroll viewport so long catalogs (extended-
            // range / specialized) fit without pushing the rest of
            // the tab off-screen.
            portal(
                flex_col(tunings_items)
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_alignment(MainAxisAlignment::Start)
                    .gap(SP_1),
            )
            .constrain_horizontal(true)
            .flex(1.0),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_2),
    );
    let tunings_sidebar: OneOf2<_, _> = if state.sidebars.is_collapsed(Tab::Tuner) {
        OneOf2::A(
            sized_box(label(""))
                .fixed_width(masonry::layout::Length::px(0.0)),
        )
    } else {
        OneOf2::B(
            sized_box(tunings_list_card)
                .fixed_width(masonry::layout::Length::px(220.0)),
        )
    };

    let visible_content = card(state.palette,
        flex_col((
            header_label(state.palette, "Tuner", TS_LG),
            transport,
            // Mono on the big display readouts so digits / note
            // characters don't reshuffle horizontally as the detector
            // updates frame-to-frame. Without this the "A♭4" / "E2"
            // jump visibly when the variable-width glyphs swap.
            label(note_text)
                .text_size(TS_2XL)
                .font(mono_family())
                .color(state.palette.text_header),
            label(cents_text).text_size(TS_SM).font(mono_family()),
            // Cents needle — custom canvas widget with center-line
            // reference, ±5¢ in-tune zone, and tick marks at
            // -50/-25/0/+25/+50. Replaces the prior progress_bar
            // stand-in which couldn't express bidirectional offset
            // (it filled left-to-right, with "in tune" awkwardly at
            // 50% fill).
            sized_box(widgets::cents_meter_view(
                cents_raw,
                in_tune,
                widgets::MeterColors::from_palette(&state.palette),
            ))
            .fixed_width(masonry::layout::Length::px(320.0))
            .fixed_height(masonry::layout::Length::px(36.0)),
            // Status caption — green when in-tune (success), dimmed
            // grey otherwise so it doesn't compete with the big note
            // readout above.
            if in_tune {
                OneOf2::A(success_label(state.palette, "✓ in tune", TS_XS))
            } else {
                OneOf2::B(disabled_label(state.palette, "(adjust until cents is centered)", TS_XS))
            },
            // Level meter — zone-colored fill (safe / caution /
            // danger) so the user can tell at a glance whether the
            // input is sub-threshold, in the usable range, or
            // about to clip.
            label(level_text).text_size(TS_XS).font(mono_family()),
            sized_box(widgets::level_meter_view(
                level_raw,
                widgets::MeterColors::from_palette(&state.palette),
            ))
            .fixed_width(masonry::layout::Length::px(320.0))
            .fixed_height(masonry::layout::Length::px(20.0)),
            // String target buttons — bias detection toward a given
            // pitch class. Useful when tuning a specific string and
            // you don't want harmonic confusion.
            label("Tune to string:").text_size(TS_SM),
            flex_row(string_btns)
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .main_axis_alignment(MainAxisAlignment::Start)
                .gap(SP_2),
            // Detector picker — clicking cycles FFT ↔ Cepstrum; the
            // label-with-state lives on the button itself.
            text_button(detector_label, |s: &mut AppState| {
                // Cycle FFT → Cepstrum → McLeod → FFT.
                // Different algorithm families make different mistakes,
                // so cycling through gives the user a way to triangulate
                // when one detector mis-octaves on a particular string.
                // McLeod (NSDF) is time-domain — least prone to FFT-
                // style harmonic confusion.
                s.tuner_detector = match s.tuner_detector {
                    DetectorKind::Fft => DetectorKind::Cepstrum,
                    DetectorKind::Cepstrum => DetectorKind::McLeod,
                    DetectorKind::McLeod => DetectorKind::Fft,
                };
                if let Ok(b) = &s.input {
                    b.tuner.set_detector_kind(s.tuner_detector);
                }
            }),
            // Threshold readout — double-click to enter precise
            // edit mode. Single-click is a no-op (intentional: the
            // first click of a double-click pair shouldn't have any
            // side effects). When editing, the readout becomes a
            // text_input committed on Enter or click of "Set".
            if state.editing_field == Some("tuner.threshold") {
                OneOf2::A(
                    flex_row((
                        text_input(
                            state.editing_buffer.clone(),
                            |s: &mut AppState, t| {
                                s.editing_buffer = t;
                            },
                        )
                        .on_enter(|s: &mut AppState, _final| {
                            if let Ok(v) = s.editing_buffer.trim().parse::<f64>() {
                                let v = v.clamp(0.0, 0.05);
                                s.tuner_threshold = v;
                                if let Ok(b) = &s.input {
                                    b.tuner.set_threshold(v);
                                }
                            }
                            s.editing_field = None;
                        })
                        .placeholder("0.0010"),
                        text_button("Set", |s: &mut AppState| {
                            if let Ok(v) = s.editing_buffer.trim().parse::<f64>() {
                                let v = v.clamp(0.0, 0.05);
                                s.tuner_threshold = v;
                                if let Ok(b) = &s.input {
                                    b.tuner.set_threshold(v);
                                }
                            }
                            s.editing_field = None;
                        }),
                        text_button("Cancel", |s: &mut AppState| {
                            s.editing_field = None;
                        }),
                    ))
                    .cross_axis_alignment(CrossAxisAlignment::Center)
                    .main_axis_alignment(MainAxisAlignment::Start)
                    .gap(SP_2),
                )
            } else {
                let display = format!(
                    "Silence threshold: {:.4} (dbl-click to edit)",
                    state.tuner_threshold
                );
                let buf = format!("{:.4}", state.tuner_threshold);
                OneOf2::B(text_button(display, move |s: &mut AppState| {
                    handle_numeric_click(s, "tuner.threshold", buf.clone());
                }))
            },
            sized_box(slider(
                0.0,
                0.05,
                state.tuner_threshold,
                |s: &mut AppState, v: f64| {
                    s.tuner_threshold = v;
                    if let Ok(b) = &s.input {
                        b.tuner.set_threshold(v);
                    }
                },
            ))
            .fixed_width(masonry::layout::Length::px(320.0)),
            prose(
                "FFT picks the strongest spectral peak; switch to \
                 Cepstrum if your guitar's low-E gets octave-confused. \
                 The string buttons hint detection toward a target \
                 pitch class — useful when you know which string \
                 you're tuning.",
            )
            .text_size(TS_XS),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_2),
    );

    // Compose the final tab layout: collapsible tunings sidebar on
    // the left, tuner controls on the right.
    let visible = flex_row((tunings_sidebar, visible_content))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_4);

    // Polling task — only present when listening. `task_raw` (as
    // opposed to `task`) is the variant that lets us capture
    // non-zero-sized values like our handle clone into the async
    // closure.
    let polling_task = (listening && tuner_handle.is_some()).then(|| {
        let handle = tuner_handle.expect("handle present by guard");
        task_raw(
            move |proxy, _| {
                let handle = handle.clone();
                async move {
                    let mut interval = time::interval(Duration::from_millis(50));
                    loop {
                        interval.tick().await;
                        let snap = handle.snapshot();
                        if proxy.message(snap).is_err() {
                            break;
                        }
                    }
                }
            },
            |state: &mut AppState, snap: TunerSnapshot| {
                state.tuner_snapshot = Some(snap);
            },
        )
    });

    fork(visible, polling_task)
}

/// Format a detected note for display, e.g. `"A4 (440.0 Hz)"`.
fn format_detected_note(n: &DetectedNote) -> String {
    format!("{}{} ({:.1} Hz)", n.name, n.octave, n.actual_freq_hz)
}

/// Enumerate all playable voicings of `chord` rooted at `root` across
/// the fretboard's fret range. Scans every 4-fret window, prefers
/// root-in-bass voicings, falls back to any-chord-tone bass if no
/// root-bass voicings exist (covers thin chords like sus2 on certain
/// tunings). Dedups identical fret patterns and collapses functionally-
/// equivalent voicings by their fretted-skeleton.
fn enumerate_voicings(
    fretboard: &Fretboard,
    chord: &ChordFormula,
    root: woodshedding::pitch::Pitch,
) -> Vec<ChordVoicing> {
    use std::collections::{BTreeMap, HashSet};

    let mut all: Vec<ChordVoicing> = Vec::new();
    let max_window = fretboard.fret_count.saturating_sub(4);
    for window_start in 0..=max_window {
        if let Ok(vs) = fretboard.find_chord_voicings_for_bass(
            chord,
            root,
            window_start,
            4,
            BassConstraint::Root,
        ) {
            all.extend(vs);
        }
    }
    if all.is_empty() {
        for window_start in 0..=max_window {
            if let Ok(vs) = fretboard.find_chord_voicings_for_bass(
                chord,
                root,
                window_start,
                4,
                BassConstraint::AnyChordTone,
            ) {
                all.extend(vs);
            }
        }
    }
    // Dedup by exact fret pattern.
    let mut seen: HashSet<Vec<Option<u8>>> = HashSet::new();
    all.retain(|v| seen.insert(v.fret_pattern()));
    // Skeleton-collapse: same fretted positions, different open/mute
    // permutations → keep the most-strings-played version.
    let mut by_skeleton: BTreeMap<Vec<(usize, u8)>, ChordVoicing> = BTreeMap::new();
    for v in all {
        let skeleton: Vec<(usize, u8)> = v
            .strings
            .iter()
            .enumerate()
            .filter_map(|(i, s)| match s {
                StringPlay::Played { fret, .. } if *fret > 0 => Some((i, *fret)),
                _ => None,
            })
            .collect();
        by_skeleton
            .entry(skeleton)
            .and_modify(|existing| {
                if v.played_string_count() > existing.played_string_count() {
                    *existing = v.clone();
                }
            })
            .or_insert(v);
    }
    let mut result: Vec<ChordVoicing> = by_skeleton.into_values().collect();
    // Drop voicings that can't be rendered in the chord-card 4-fret
    // window — they look wrong on the cards (a fingered note silently
    // disappears) and there's no good UX for "this card can't show
    // the full voicing." Span is `max_fret - min_fret` over fretted
    // notes; open strings are exempt because they render as ○ above
    // the nut, outside the fret window. Span ≤ 3 means {min, min+1,
    // min+2, min+3} all fit in a 4-fret diagram.
    result.retain(|v| {
        let fretted: Vec<u8> = v
            .strings
            .iter()
            .filter_map(|s| match s {
                StringPlay::Played { fret, .. } if *fret > 0 => Some(*fret),
                _ => None,
            })
            .collect();
        match (fretted.iter().min(), fretted.iter().max()) {
            (Some(lo), Some(hi)) => hi - lo <= 3,
            _ => true, // all-open voicings always fit
        }
    });
    // Sort by lowest fretted position (open chords first, then up the
    // neck) — gives a sensible cycling order for users.
    result.sort_by_key(|v| v.lowest_fretted_position());
    result
}

/// Map a ChordVoicing to fretboard Positions, dropping muted strings.
fn voicing_to_positions(voicing: &ChordVoicing) -> Vec<Position> {
    voicing
        .strings
        .iter()
        .enumerate()
        .filter_map(|(i, sp)| match sp {
            StringPlay::Played {
                fret,
                pitch,
                interval_from_root,
            } => Some(Position {
                string_index: i,
                fret: *fret,
                pitch: *pitch,
                interval_from_root: *interval_from_root,
            }),
            StringPlay::Muted => None,
        })
        .collect()
}

/// Slightly darker tone than the default app background. Used as a
/// Wrap a view in a "card": padded, rounded, with a subtle darker
/// background. Use for grouping related controls/content so the eye
/// can pick out sections.
///
/// Padding is `SP_2` (8px) and corner radius matches — both come
/// from the theme module rather than ad-hoc literals so card density
/// stays consistent with the surrounding rhythm. Currently reads the
/// background from a const dark palette to keep the call sites
/// parameter-free; when the theme-picker UI lands this should switch
/// to taking `&Palette` so the active theme drives the surface
/// color. Tracked as part of the tier-1 visual refactor.
fn card<V>(palette: Palette, inner: V) -> impl WidgetView<AppState>
where
    V: WidgetView<AppState>,
{
    // Subtle 1px border at `surface_2` — gives each card a quiet
    // edge so adjacent cards don't blur into one another against
    // the dark background. Width matches modern design conventions
    // (shadcn / macOS / Win11): low-contrast hairline rather than
    // a hard contrast outline. Reads colors from the *runtime*
    // palette, not a const stand-in, so a theme swap re-skins every
    // card at the next rebuild pass.
    sized_box(inner)
        .padding(SP_2)
        .corner_radius(SP_2)
        .background_color(palette.surface)
        .border(palette.surface_2, masonry::layout::Length::const_px(1.0))
}

// `ACTIVE_PALETTE` const stand-in lived here until 2026-05-18.
// Replaced by threading `state.palette` (by value — `Palette` is
// `Copy`) through `card()` and the semantic-color label helpers.
// A theme picker can now re-skin every card and dimmed label at
// the next rebuild pass by mutating `state.palette` in place.

/// Empirically-measured per-card width used by the chord-card
/// reflow chunking on the Progressions tab.
///
/// Each card renders ~370px wide in practice — the
/// `sized_box.fixed_width` set on the inner flex_col below is
/// honored as a *preference*, not a hard clamp, and masonry's flex
/// layout tends to give the card the natural width of its contents
/// (chord-name label + role + diagram + arrows). Rather than fight
/// that, the chunking math here matches the observed render width,
/// so `cards_per_row = floor((panel_w + gap) / (CHORD_CARD_W + gap))`
/// produces sensible row counts that scale with window width.
///
/// Tune this if the wrap point feels wrong: bump higher to wrap
/// earlier (fewer cards per row at the same window width), lower
/// to fit more.
const CHORD_CARD_W: f64 = 370.0;

/// Max gap between two clicks to register as a double-click.
/// 400ms is the conventional desktop default (Windows uses ~500ms by
/// default but most apps clamp tighter). Click 1 and click 2 must
/// also be on the same field (tracked by `last_click_field`).
const DOUBLE_CLICK_MS: u128 = 400;

/// On click of a numeric readout: if this is the second click on
/// `field_id` within `DOUBLE_CLICK_MS`, switch the field into edit
/// mode with `initial_buffer` as the starting text. Otherwise just
/// record this click for the next one to compare against.
///
/// The single-click side-effect for these readouts is "do nothing,"
/// so a user clicking once just sees the value unchanged — no
/// accidental side-effects from the first click of a double-click
/// pair.
fn handle_numeric_click(
    s: &mut AppState,
    field_id: &'static str,
    initial_buffer: String,
) {
    let now = std::time::Instant::now();
    let is_double = matches!(
        (s.last_click_field, s.last_click_at),
        (Some(prev_id), Some(prev_t))
            if prev_id == field_id
                && now.duration_since(prev_t).as_millis() < DOUBLE_CLICK_MS
    );
    if is_double {
        s.editing_field = Some(field_id);
        s.editing_buffer = initial_buffer;
        s.last_click_field = None;
        s.last_click_at = None;
    } else {
        s.last_click_field = Some(field_id);
        s.last_click_at = Some(now);
    }
}

/// A big numeric readout that becomes editable in-place on
/// double-click — without losing its display styling.
///
/// Display mode: a button whose chrome (background, border) is
/// stripped to transparent so it reads as the plain styled label
/// it replaces, but it's clickable for double-click detection.
/// Edit mode: a `text_input` with the *same* `text_size` + mono
/// font, so the value stays visually anchored while you type —
/// no jarring shrink to a default-size input box. Enter / ✓ commit
/// (through `setter`, which owns the field-specific clamp + side
/// effects); ✕ cancels.
///
/// `field_id` matches `AppState::editing_field`; `display_text` is
/// the full readout ("90 BPM"); `edit_init` is what seeds the edit
/// buffer ("90", sans unit).
fn editable_big_number<F>(
    state: &AppState,
    field_id: &'static str,
    display_text: String,
    edit_init: String,
    text_size: f32,
    setter: F,
) -> impl WidgetView<AppState> + use<F>
where
    F: Fn(&mut AppState, f64) + Send + Sync + Clone + 'static,
{
    use xilem::view::button as button_view;
    let transparent = masonry::peniko::Color::from_rgba8(0, 0, 0, 0);

    if state.editing_field == Some(field_id) {
        let setter_enter = setter.clone();
        let setter_set = setter.clone();
        OneOf2::A(
            flex_row((
                text_input(state.editing_buffer.clone(), |s: &mut AppState, t| {
                    s.editing_buffer = t;
                })
                .text_size(text_size)
                .font(mono_family())
                .on_enter(move |s: &mut AppState, _final| {
                    if let Ok(v) = s.editing_buffer.trim().parse::<f64>() {
                        setter_enter(s, v);
                    }
                    s.editing_field = None;
                }),
                text_button("✓", move |s: &mut AppState| {
                    if let Ok(v) = s.editing_buffer.trim().parse::<f64>() {
                        setter_set(s, v);
                    }
                    s.editing_field = None;
                }),
                text_button("Cancel", |s: &mut AppState| {
                    s.editing_field = None;
                }),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .main_axis_alignment(MainAxisAlignment::Start)
            .gap(SP_2),
        )
    } else {
        // Chrome-stripped button — looks like the plain styled
        // label it stands in for, but captures clicks for the
        // double-click → edit gesture.
        OneOf2::B(
            button_view(
                label(display_text).text_size(text_size).font(mono_family()),
                move |s: &mut AppState| {
                    handle_numeric_click(s, field_id, edit_init.clone());
                },
            )
            .background_color(transparent)
            .border(transparent, masonry::layout::Length::const_px(0.0)),
        )
    }
}

// =================================================================
// Sized button helpers — `text_button` builds a default-size button,
// but the UI has cases where a smaller affordance reads better (the
// `◀/▶` cycle arrows next to comboboxes) and cases where the primary
// action wants a heavier weight (transport controls). These wrap
// `button(label(...), cb)` so we can scale the inner label's
// `text_size` without touching every call site.
// =================================================================

use xilem::view::button as button_view;

/// Small button — `TS_XS` text. Use for cycle arrows, secondary
/// micro-actions, and any control that's "in service of" a larger
/// picker (the ◀/▶ flanking a combobox, for instance).
fn button_sm<F>(
    text: impl Into<masonry::core::ArcStr>,
    callback: F,
) -> impl WidgetView<AppState>
where
    F: Fn(&mut AppState) + Send + Sync + 'static,
{
    button_view(label(text).text_size(TS_XS), move |s: &mut AppState| {
        callback(s);
    })
}

/// Medium button — `TS_SM` text. Equivalent to the default
/// `text_button` weight today; the helper exists so call sites
/// can be explicit about intent ("this is a regular control"
/// vs "this is small / large") rather than relying on the
/// platform default.
#[allow(dead_code)]
fn button_md<F>(
    text: impl Into<masonry::core::ArcStr>,
    callback: F,
) -> impl WidgetView<AppState>
where
    F: Fn(&mut AppState) + Send + Sync + 'static,
{
    button_view(label(text).text_size(TS_SM), move |s: &mut AppState| {
        callback(s);
    })
}

/// Large button — `TS_MD` text. Use for primary transport actions
/// ("▶ Play", "■ Stop", "Start tuner") that anchor a panel.
#[allow(dead_code)]
fn button_lg<F>(
    text: impl Into<masonry::core::ArcStr>,
    callback: F,
) -> impl WidgetView<AppState>
where
    F: Fn(&mut AppState) + Send + Sync + 'static,
{
    button_view(label(text).text_size(TS_MD), move |s: &mut AppState| {
        callback(s);
    })
}

// =================================================================
// Dimmed-label helpers — apply semantic hierarchy colors from the
// palette. View code reads `dim_label(state.palette, "text", TS_XS)` instead of
// stitching `.color(palette.text_dim)` onto every `label(...)`.
//
// Each takes `(text, size)`. `.color()` from the Style trait wraps
// the label into a `Prop<...>`, after which `.text_size()` is no
// longer chainable — so the size has to be applied *before* the
// color tint. Baking it into the helper signature is cleaner than
// asking call sites to remember the chain order.
// =================================================================

use xilem::style::Style as _;

/// Secondary metadata text — counters ("1/12"), role labels
/// ("iii"), "Up next: …" previews. Lower contrast than `text` so
/// the eye reads it as supporting information.
fn dim_label(    palette: Palette,
    text: impl Into<masonry::core::ArcStr>,
    size: f32,
) -> impl WidgetView<AppState> {
    label(text).text_size(size).color(palette.text_dim)
}

/// Heading / title text (the big type tiers: tab + section titles,
/// large readouts). Colored from `palette.text_header` so the header
/// text tier is themeable independently of body text.
fn header_label(
    palette: Palette,
    text: impl Into<masonry::core::ArcStr>,
    size: f32,
) -> impl WidgetView<AppState> {
    label(text).text_size(size).color(palette.text_header)
}

/// Disabled / empty-state text — "No playable steps", "Pick a
/// set first.", "(adjust until cents is centered)". Reads as
/// deactivated so the eye doesn't compete it with active controls.
fn disabled_label(    palette: Palette,
    text: impl Into<masonry::core::ArcStr>,
    size: f32,
) -> impl WidgetView<AppState> {
    label(text).text_size(size).color(palette.text_disabled)
}

/// Error / danger text — audio-engine unavailable, mic missing.
/// Used sparingly; the palette `danger` color carries enough
/// weight that scattering it dilutes the signal.
fn danger_label(    palette: Palette,
    text: impl Into<masonry::core::ArcStr>,
    size: f32,
) -> impl WidgetView<AppState> {
    label(text).text_size(size).color(palette.danger)
}

/// Success / "all good" text — "✓ in tune", confirmation states.
fn success_label(    palette: Palette,
    text: impl Into<masonry::core::ArcStr>,
    size: f32,
) -> impl WidgetView<AppState> {
    label(text).text_size(size).color(palette.success)
}

// =================================================================
// Wrapping (prose) variants of the dimmed-label helpers — same
// semantic palette but the underlying widget is `prose`, which
// word-wraps to its parent container's width instead of imposing
// its natural width on the layout.
//
// Rule of thumb: use `*_label` for short content (counters, role
// abbreviations, control labels) and `*_prose` for multi-sentence
// captions / descriptions / hint paragraphs. Mixing them up gives
// you either truncated short labels (wrap point falling inside
// "1/91") or overflowing paragraphs that extend past the card's
// right edge — the bug that made the Settings tab read wrong on
// narrow windows.
// =================================================================

// Implementation note: `*_prose` helpers use `label(...).prop(
// LineBreaking::WordWrap)` rather than xilem's `prose(...)` view
// because `Prose` doesn't implement `UsesProperty<ContentColor>`
// (no `.color()` chain works on it), so the dim/danger/success
// tint would be silently dropped. Label happens to support both
// wrap and color via separate property props.

fn dim_prose(
    palette: Palette,
    text: impl Into<masonry::core::ArcStr>,
    size: f32,
) -> impl WidgetView<AppState> {
    label(text)
        .text_size(size)
        .color(palette.text_dim)
        .prop(masonry::properties::LineBreaking::WordWrap)
}

#[allow(dead_code)]
fn disabled_prose(
    palette: Palette,
    text: impl Into<masonry::core::ArcStr>,
    size: f32,
) -> impl WidgetView<AppState> {
    label(text)
        .text_size(size)
        .color(palette.text_disabled)
        .prop(masonry::properties::LineBreaking::WordWrap)
}

fn danger_prose(
    palette: Palette,
    text: impl Into<masonry::core::ArcStr>,
    size: f32,
) -> impl WidgetView<AppState> {
    label(text)
        .text_size(size)
        .color(palette.danger)
        .prop(masonry::properties::LineBreaking::WordWrap)
}

#[allow(dead_code)]
fn success_prose(
    palette: Palette,
    text: impl Into<masonry::core::ArcStr>,
    size: f32,
) -> impl WidgetView<AppState> {
    label(text)
        .text_size(size)
        .color(palette.success)
        .prop(masonry::properties::LineBreaking::WordWrap)
}

/// Translate a slice of Positions into per-dot label strings per the
/// active LabelMode. Shared between Scales and Chords (and eventually
/// Exercises / Progressions).
fn compute_labels(
    mode: LabelMode,
    positions: &[woodshedding::fretboard::Position],
) -> Vec<String> {
    positions
        .iter()
        .map(|p| match mode {
            LabelMode::None => String::new(),
            LabelMode::Notes => format!(
                "{}{}",
                p.pitch.name,
                accidental_short(p.pitch.accidental)
            ),
            LabelMode::Intervals => p
                .interval_from_root
                .map(|iv| iv.number().to_string())
                .unwrap_or_default(),
        })
        .collect()
}

/// Short accidental string matching the iced build's `accidental_str`.
fn accidental_short(a: woodshedding::pitch::Accidental) -> &'static str {
    use woodshedding::pitch::Accidental;
    match a {
        Accidental::DoubleFlat => "bb",
        Accidental::Flat => "b",
        Accidental::Natural => "",
        Accidental::Sharp => "#",
        Accidental::DoubleSharp => "##",
    }
}


// =================================================================
// Tiny formatting helpers — match the iced build's spelling.
// =================================================================


// =================================================================
// Entry point
// =================================================================

/// Windowed-API hook: keep the event loop alive until the window's
/// `on_close` flips `running`. (Needed because we use `Xilem::new`
/// rather than `new_simple` so the window's base color + default
/// properties can track the theme reactively.)
impl XilemAppState for AppState {
    fn keep_running(&self) -> bool {
        self.running
    }
}

pub fn run(event_loop: EventLoopBuilder) -> Result<(), EventLoopError> {
    // Construct defaults, then overlay any persisted settings.
    // First-run (no file yet) silently uses defaults — see
    // `Settings::load` for the failure-handling model.
    let mut state = AppState::new();
    state.apply_settings(Settings::load());

    // Startup palette → startup base color + default properties.
    // These seed the render root at creation; the per-frame window
    // view below re-applies them reactively so a mid-session theme
    // toggle re-themes everything (including the masonry-default-driven
    // bare labels / buttons / prose) without a restart.
    let startup_palette = state.palette;

    // Windowed API (not `new_simple`) so the window's base color and
    // default-property set can follow `state.palette` each frame.
    let app = Xilem::new(state, move |state: &mut AppState| {
        let window_id = state.window_id;
        let base_color = state.palette.bg;
        let default_properties = state.default_properties.clone();
        let root = app_logic(state);
        std::iter::once(
            window(window_id, "Woodshed (Xilem)", root)
                .with_base_color(base_color)
                .with_default_properties(default_properties)
                .with_options(|o| {
                    o.with_min_inner_size(LogicalSize::new(640.0, 480.0))
                        .with_initial_inner_size(LogicalSize::new(960.0, 720.0))
                        .on_close(|s: &mut AppState| s.running = false)
                }),
        )
    })
    .with_default_properties(build_default_properties(&startup_palette))
    .with_default_base_color(startup_palette.bg);
    // AppState's Drop impl flushes the latest snapshot to disk when
    // the runtime drops the state at shutdown, so quit-without-save
    // still persists the most recent choices.
    app.run_in(event_loop)?;
    Ok(())
}

/// Build a [`DefaultProperties`] that overlays palette colors on top
/// of masonry's built-in defaults. The base set covers every widget
/// masonry ships defaults for; we override the entries whose colors
/// would otherwise stay locked to masonry's dark-theme assumption.
///
/// Coverage: Label content color, Button surfaces, TextArea content,
/// scrollbar visibility. Other widgets (Checkbox, Switch, Slider,
/// etc.) keep masonry's defaults — extend this function as those
/// widgets show palette-mismatch issues.
fn build_default_properties(palette: &Palette) -> masonry::core::DefaultProperties {
    use masonry::core::DefaultProperties;
    use masonry::layout::AsUnit;
    use masonry::properties::{
        Background, BorderColor, BorderWidth, ContentColor, CornerRadius, Padding,
    };
    use masonry::widgets::{Button, Label, TextArea, TextInput};

    // Start from masonry's full default set so we pick up everything
    // we don't override (padding values, corner radii, less-visible
    // properties on widgets we don't actively re-theme).
    let mut properties: DefaultProperties = masonry::theme::default_property_set();

    // Label — the big one. Plain `label("foo")` should pick the
    // palette's primary text color.
    properties.insert::<Label, _>(ContentColor::new(palette.text));

    // Button — surface and border. Hover/active states fall back to
    // masonry's selector stacks; the resting state gets palette colors
    // so light-theme buttons look like buttons, not dark plates.
    properties.insert::<Button, _>(Background::Color(palette.surface_2));
    properties.insert::<Button, _>(BorderColor { color: palette.surface_hover });
    properties.insert::<Button, _>(BorderWidth { width: 1.px() });
    properties.insert::<Button, _>(CornerRadius { radius: 6.px() });
    properties.insert::<Button, _>(Padding::from_vh(6.px(), 16.px()));

    // TextInput — caret + text + background, same rationale as Label.
    properties.insert::<TextInput, _>(ContentColor::new(palette.text));
    properties.insert::<TextInput, _>(Background::Color(palette.surface));
    properties.insert::<TextInput, _>(BorderColor { color: palette.surface_2 });

    // TextArea — the leaf widget that `prose(...)` paints through.
    // Without these overrides, prose stays at masonry's near-white
    // default and disappears against light surfaces (the "I-IV-V"
    // heading and progression description that read as ghosted text
    // in light mode).
    properties.insert::<TextArea<false>, _>(ContentColor::new(palette.text));
    properties.insert::<TextArea<true>, _>(ContentColor::new(palette.text));

    properties
}

/// Flush durable settings on shutdown. Best-effort: errors are
/// logged inside [`Settings::save`] and otherwise swallowed — drop
/// runs during teardown where panicking is unwelcome.
impl Drop for AppState {
    fn drop(&mut self) {
        self.snapshot_settings().save();
    }
}

fn main() -> Result<(), EventLoopError> {
    run(EventLoop::with_user_event())
}
