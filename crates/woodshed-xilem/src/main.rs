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
use xilem::core::one_of::{OneOf2, OneOf3};
use xilem::style::Style;
use xilem::view::{
    AnyFlexChild, FlexExt, FlexSpacer, Label, button, flex_col, flex_row,
    label as xilem_label, portal, prose, resize_observer, sized_box, slider,
    task_raw, text_input,
};
use xilem::{AnyWidgetView, AppState as XilemAppState, WidgetView, WindowId, Xilem, window};

use woodshed_audio::{
    Bar as SongBar, ChordRef, DetectedNote, DetectedNoteName, DetectorKind, EngineHandle,
    InputEngine, InputEngineBuilder, LooperCaptureHandle, OnsetAnalyzer, OnsetHandle,
    PendingChange, SequencerEngine, SequencerPattern, Song, SongEngine, SongEngineHandle,
    Sound, Step, Subdivision, TimeSignature, Track, TunerHandle, TunerSnapshot,
};

use woodshedding::chord::{ChordFormula, catalog as chord_catalog};
use woodshedding::exercise::{
    ExerciseDirection, ExerciseParams, ExerciseStep, catalog as exercise_catalog,
};
use woodshedding::fretboard::{
    BassConstraint, ChordVoicing, Fretboard, Position, StringPlay,
};
use woodshedding::practice::{PracticeItem, PracticeSet, catalog as practice_catalog};
use woodshedding::progression::{
    ChordRole, DegreeAlteration, ProgressionChord, RoleQuality,
    apply_roles_in_key, catalog as progression_catalog,
};
use woodshedding::pitch::{NoteName, Pitch, PitchClass};
use woodshedding::rehearsal::{
    ArpeggioDirection, Card, Clock, FretWindow, Hold, LoopMode, Material, Recipe, Set, Setting,
    Timing, Touch,
};
use woodshedding::scale::{ScaleFormula, catalog as scale_catalog};
use woodshedding::tuning::{Instrument, Tuning, catalog as tuning_catalog};

mod combobox;
// Vendored upstream split widget+view (full API kept intentionally).
#[allow(dead_code)]
mod pane_split;
#[allow(dead_code)]
mod pane_split_widget;
mod settings;
mod theme;
mod widgets;
mod window_chrome;

use window_chrome::{window_chrome, window_frame, ChromeRole, RESIZE_MARGIN};

use combobox::combobox;
use pane_split::pane_split;
use settings::Settings;
use theme::{
    Palette, SP_0, SP_1, SP_2, SP_3, SP_4, TS_2XL, TS_LG, TS_MD, TS_SM, TS_XL, TS_XS,
    mono_family, ui_family,
};
use audio_widgets::waveform_view;
use widgets::{
    SectionBand, SectionColors, StringMark, chord_lane_view,
    fretboard_view, section_lane_view,
};

// =================================================================
// Themed view wrappers: `label` / `text_button`.
//
// The framework default text family is `GenericFamily::SystemUi`
// (masonry's `default_text_styles`, and xilem's `label` view hardcodes
// it too). On this stack `SystemUi` lacks the Dingbats / Misc-Symbols
// / geometric-arrow blocks, so symbol glyphs (× ‹ › ★ ♯ ♭ ☰ …) render
// as tofu boxes. We route every label and text button through
// `ui_family()` (`SansSerif` → the platform's glyph-complete system
// sans, Segoe UI on Windows) instead. Font choice is an application
// decision, so it lives here rather than in the lean xilem fork.
//
// These shadow the `xilem::view` originals (imported as `xilem_label`
// / `button`): every helper below (`button_sm`, `dim_label`, the
// `*_prose` wrappers, …) and every bare call site picks up the UI font
// for free. Call sites that want a different family still chain
// `.font(mono_family())` afterwards, which overrides the default set
// here.
// =================================================================

/// A label in the UI font. Drop-in for `xilem::view::label`.
fn label(text: impl Into<ArcStr>) -> Label {
    xilem_label(text).font(ui_family())
}

/// A text button in the UI font. Equivalent to
/// `xilem::view::text_button` (`button(label(text), callback)`) but
/// built on our themed [`label`].
fn text_button<State: 'static, Action: 'static>(
    text: impl Into<ArcStr>,
    callback: impl Fn(&mut State) -> Action + Send + Sync + 'static,
) -> impl WidgetView<State, Action> {
    button(label(text), callback)
}

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
    Arpeggios,
    Metronome,
    Practice,
    Song,
    Rehearsal,
    Settings,
}

impl Tab {
    const ALL: [Self; 11] = [
        Self::Scales,
        Self::Chords,
        Self::Tuner,
        Self::Progressions,
        Self::Exercises,
        Self::Arpeggios,
        Self::Metronome,
        Self::Practice,
        Self::Song,
        Self::Rehearsal,
        Self::Settings,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Scales => "Scales",
            Self::Chords => "Chords",
            Self::Tuner => "Tuner",
            Self::Progressions => "Progressions",
            Self::Exercises => "Exercises",
            Self::Arpeggios => "Arpeggios",
            Self::Metronome => "Metronome",
            Self::Practice => "Practice",
            Self::Song => "Song",
            Self::Rehearsal => "Rehearsal",
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
            | Tab::Arpeggios
    )
}

/// Whether the tab renders a fretboard (so the header offers the
/// fret-span scope control). Tuner has a list but shows a meter, not a
/// fretboard, so it's excluded.
fn tab_has_fretboard(tab: Tab) -> bool {
    matches!(
        tab,
        Tab::Scales | Tab::Chords | Tab::Progressions | Tab::Exercises | Tab::Arpeggios
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
    #[serde(default)]
    arpeggios: bool,
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
            Tab::Arpeggios => self.arpeggios,
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
            Tab::Arpeggios => self.arpeggios = !self.arpeggios,
            _ => {}
        }
    }
}

// `ArpeggioDirection` now lives in `woodshedding::rehearsal` (U7) and is
// imported below alongside the card/set vocabulary.

/// What to print inside the arpeggio's fretboard dots. Adds `Steps`
/// (the note's order in the ascending arpeggio) and `Blank` to the
/// usual note/interval choices.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
enum ArpeggioLabel {
    #[default]
    Notes,
    Intervals,
    Steps,
    Blank,
}

impl ArpeggioLabel {
    fn next(self) -> Self {
        match self {
            Self::Notes => Self::Intervals,
            Self::Intervals => Self::Steps,
            Self::Steps => Self::Blank,
            Self::Blank => Self::Notes,
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Notes => "Labels: notes",
            Self::Intervals => "Labels: intervals",
            Self::Steps => "Labels: steps",
            Self::Blank => "Labels: blank",
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

    /// 0..=11 chromatic value (C=0 … B=11). Bridges to the portable
    /// `PitchClass` the rehearsal card model stores (U7).
    fn pc(self) -> u8 {
        Self::ALL.iter().position(|&p| p == self).unwrap_or(0) as u8
    }

    fn to_pitch_class(self) -> PitchClass {
        PitchClass::new(self.pc())
    }

    fn from_pitch_class(pc: PitchClass) -> Self {
        Self::from_pc(pc.value())
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

/// One kind of widget that can be mounted on the instrument surface
/// (the composable left pane — see
/// `design_docs/2026-05-21_composable_instrument_surface_plan.md`).
/// `Fretboard` is the always-present primary (carries the active lens
/// via [`AppState::tab`]); the rest are optional companions the user
/// mounts/unmounts. Persisted by name so reordering the variants
/// doesn't scramble saved compositions.
#[derive(Copy, Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
enum ModuleKind {
    Fretboard,
    Tuner,
    Metronome,
}

impl ModuleKind {
    fn label(self) -> &'static str {
        match self {
            Self::Fretboard => "Fretboard",
            Self::Tuner => "Tuner",
            Self::Metronome => "Metronome",
        }
    }
}

/// A widget mounted in the instrument-surface stack: which kind, whether
/// it's currently shown, and its relative vertical size among the
/// visible modules. `weight` is a share (not pixels) so the stack
/// reflows as the window resizes; persisted so a user's composition
/// restores on launch. Phase 3a introduces the model; rendering still
/// only mounts `Fretboard` until 3b.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct SurfaceModule {
    kind: ModuleKind,
    #[serde(default = "default_module_visible")]
    visible: bool,
    #[serde(default = "default_module_weight")]
    weight: f64,
}

fn default_module_visible() -> bool {
    true
}

fn default_module_weight() -> f64 {
    1.0
}

impl SurfaceModule {
    fn new(kind: ModuleKind) -> Self {
        Self {
            kind,
            visible: true,
            weight: 1.0,
        }
    }
}

/// What a card puts on the neck (U5): the resolved dots + labels, an
/// optional pinned window, and a warning when the material no longer
/// resolves. The one shape every stage consumer renders, computed by
/// [`AppState::resolve_card_for_stage`].
struct StageRender {
    positions: Vec<Position>,
    labels: Vec<String>,
    /// Window the card asks for (`None` = use the live fret window).
    fret_window: Option<FretWindow>,
    /// Set when the card's material couldn't be resolved (renamed /
    /// removed catalog entry); the caller shows it instead of an empty neck.
    warning: Option<String>,
}

impl StageRender {
    fn empty(warning: String) -> Self {
        Self {
            positions: Vec::new(),
            labels: Vec::new(),
            fret_window: None,
            warning: Some(warning),
        }
    }
}

/// Total frets in the fretboard *model* (positions are computed up to
/// here). The visible display is a window of `fret_span` frets starting
/// at `fret_start`; this is the ceiling that window can slide to.
const FRETBOARD_MODEL_FRETS: u8 = 24;

/// Build a runtime [`Tuning`] from a persisted [`settings::UserTuningDef`]
/// (open-string MIDI → spelled pitches, sharps spelling).
fn user_tuning_to_tuning(def: &settings::UserTuningDef) -> Tuning {
    let strings: Vec<woodshedding::pitch::Pitch> = def
        .midi
        .iter()
        .map(|&m| woodshedding::pitch::Pitch::from_midi(m, woodshedding::pitch::Spelling::Sharps))
        .collect();
    Tuning::custom(
        def.name.clone(),
        strings,
        settings::instrument_from_str(&def.instrument),
    )
}

/// Convert a persisted [`settings::UserProgressionDef`] into owned
/// theory [`ChordRole`]s (clamping the small-int indices to valid enum
/// values).
fn user_progression_roles(def: &settings::UserProgressionDef) -> Vec<ChordRole> {
    def.roles
        .iter()
        .map(|r| ChordRole {
            degree: r.degree.clamp(1, 7),
            alteration: DegreeAlteration::ALL
                [(r.alteration as usize).min(DegreeAlteration::ALL.len() - 1)],
            quality: RoleQuality::ALL[(r.quality as usize).min(RoleQuality::ALL.len() - 1)],
        })
        .collect()
}

/// Convert a persisted [`settings::UserExerciseDef`] into runtime
/// [`ExerciseStep`]s.
fn user_exercise_steps(def: &settings::UserExerciseDef) -> Vec<ExerciseStep> {
    def.steps
        .iter()
        .map(|s| ExerciseStep {
            string_index: s.string as usize,
            fret: s.fret,
            finger: s.finger,
        })
        .collect()
}

/// Convert one [`PracticeItem`] into a rehearsal [`Card`] (U2). Scales and
/// chords carry their root + name; an exercise becomes a `Riff`. The
/// item's hand `position` isn't pinned yet (the stage uses the live fret
/// window); a per-card fret window arrives with the U5 resolver.
fn practice_item_to_card(
    item: &PracticeItem,
    instrument: &str,
    bpm: f32,
    bars: u8,
    set_name: &str,
) -> Card {
    let timing = Timing {
        bpm: Some(bpm),
        hold: Hold::Bars(bars),
    };
    let from = Some(Recipe::PracticeSet {
        name: set_name.to_string(),
    });
    // Scale/chord items pin their 5-fret hand position as the card's
    // fret window; exercises span the neck (no window).
    let (material, fret_window) = match item {
        PracticeItem::Scale { formula, root, position } => (
            Material::Scale {
                name: formula.name.to_string(),
                root: PitchClass::new(root.midi().rem_euclid(12) as u8),
            },
            Some(FretWindow { start: *position, span: 5 }),
        ),
        PracticeItem::Chord { formula, root, position } => (
            Material::Chord {
                name: formula.name.to_string(),
                root: PitchClass::new(root.midi().rem_euclid(12) as u8),
            },
            Some(FretWindow { start: *position, span: 5 }),
        ),
        PracticeItem::Exercise { exercise, .. } => (
            Material::Riff {
                name: exercise.name.to_string(),
            },
            None,
        ),
    };
    Card {
        label: item.label(),
        material,
        setting: Setting {
            instrument: instrument.to_string(),
            tuning: None,
            capo: None,
            voicing_idx: None,
            fret_window,
        },
        touch: Touch::Block,
        timing,
        from,
    }
}

/// Project a [`Song`] into rehearsal cards (U4a). Each bar that carries a
/// chord becomes a chord card with the bar's tempo and a bars-per-block
/// `Hold`, tagged `from` the song; silent bars (no chord) are skipped
/// since they hold no neck material. The song engine still owns recorded
/// audio playback; this is a one-way projection, not absorption. (The
/// live cursor/clock sync between a playing song and the set cursor lands
/// with the U5 resolver / U6 timeline.)
fn song_to_cards(song: &Song, instrument: &str) -> Vec<Card> {
    let name = song.name.clone();
    song.bars
        .iter()
        .enumerate()
        .filter_map(|(bar_idx, bar)| {
            let chord = bar.chord_ref.as_ref()?;
            // Frequency → pitch class (round to nearest MIDI note).
            let midi = (69.0 + 12.0 * (chord.root_freq_hz / 440.0).log2()).round() as i32;
            let root = PitchClass::new(midi.rem_euclid(12) as u8);
            let label = {
                let chord_label = if chord.label.is_empty() {
                    chord.formula_name.clone()
                } else {
                    chord.label.clone()
                };
                if bar.label.is_empty() {
                    chord_label
                } else {
                    format!("{} · {}", bar.label, chord_label)
                }
            };
            Some(Card {
                label,
                material: Material::Chord {
                    name: chord.formula_name.clone(),
                    root,
                },
                setting: Setting {
                    instrument: instrument.to_string(),
                    tuning: None,
                    capo: None,
                    voicing_idx: None,
                    fret_window: None,
                },
                touch: Touch::Block,
                timing: Timing {
                    bpm: Some(bar.bpm),
                    hold: Hold::Bars(bar.length.max(1)),
                },
                from: Some(Recipe::Song { name: name.clone(), bar: bar_idx }),
            })
        })
        .collect()
}

/// The default instrument-surface composition: just the fretboard.
/// Tuner/Metronome are mounted by the user (3b+).
fn default_surface() -> Vec<SurfaceModule> {
    vec![SurfaceModule::new(ModuleKind::Fretboard)]
}

/// Normalize a loaded surface so the invariant "exactly one Fretboard,
/// always present" holds regardless of what's on disk: drop duplicate
/// Fretboard entries, and prepend one if a hand-edited or older config
/// omitted it. A bad weight (non-finite or non-positive) is reset to
/// `1.0`. Order is otherwise preserved.
fn sanitize_surface(mut modules: Vec<SurfaceModule>) -> Vec<SurfaceModule> {
    let mut seen_fretboard = false;
    modules.retain(|m| {
        if m.kind == ModuleKind::Fretboard {
            if seen_fretboard {
                return false;
            }
            seen_fretboard = true;
        }
        true
    });
    for m in &mut modules {
        if !m.weight.is_finite() || m.weight <= 0.0 {
            m.weight = 1.0;
        }
    }
    if !seen_fretboard {
        modules.insert(0, SurfaceModule::new(ModuleKind::Fretboard));
    }
    modules
}

/// Top-level app state. Owned by Xilem, mutated by event handlers,
/// read by the view function on each diff pass.
///
/// Starts deliberately thin — fields get added as tabs are ported in.
/// Anything that lives in `woodshed-audio` (engines, handles) lives
/// in `AppState` once the audio integration step lands.
struct AppState {
    tab: Tab,
    /// Last active fretboard lens (one of the Scales/Chords/
    /// Progressions/Exercises tabs). The collapsed "Fretboard" tab
    /// button returns here; the lens switcher updates it. Transient
    /// (not persisted — `tab` already is).
    last_lens: Tab,
    // === Scales tab ===
    /// Shared musical root/key — one current pitch class the theory
    /// lenses (Scales / Chords / Progressions) all read + write, so
    /// they're coherent views of one musical moment rather than three
    /// independent pickers. (Unified from per-tab roots 2026-05-21.)
    root: ChromaticPc,
    /// Index into the scale catalog. The iced version picks by name;
    /// using an index here is slightly more idiomatic with Xilem's
    /// reactive diffing model (cheap equality check).
    scale_idx: usize,
    /// What to label fretboard dots with on the Scales tab.
    scale_label_mode: LabelMode,

    // === Chords tab ===
    chord_idx: usize,
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

    // === Arpeggios lens ===
    /// Index into the *chord* catalog — the arpeggio's quality (an
    /// arpeggio's notes are a chord's tones). Root is the shared
    /// `root`. Persisted.
    arpeggio_idx: usize,
    /// Which generated position/shape is active (A2+). Persisted.
    arpeggio_position_idx: usize,
    /// Transport cursor through the active shape's up/down note
    /// sequence (A3). Transient.
    arpeggio_step_idx: usize,
    /// True while the arpeggio transport is auto-advancing. Transient.
    arpeggio_playing: bool,
    /// Arpeggio transport tempo (BPM). Persisted.
    arpeggio_bpm: f32,
    /// Up / Down / UpDown walk direction. Persisted.
    arpeggio_direction: ArpeggioDirection,
    /// What the arpeggio fretboard dots are labelled with. Persisted.
    arpeggio_label: ArpeggioLabel,
    /// Inversion: which chord tone the run starts on. `0` = root, `1` =
    /// the next tone (3rd), etc. Indexes the chord's intervals; clamped
    /// to the tone count. Rotates the transport sequence. Persisted.
    arpeggio_inversion: u8,
    /// Observed width of the arpeggio position-cards pane (drives the
    /// shape-card reflow grid). Set via `resize_observer`; transient.
    arpeggio_cards_panel_width: f64,
    /// Last sounded step index for the arpeggio / exercise transports —
    /// so audio fires once per step change, not every poll tick. Transient.
    arpeggio_last_sounded: Option<usize>,
    exercise_last_sounded: Option<usize>,
    /// Whether the arpeggio/exercise step-through plays a note per step
    /// (the chord-render voice). Persisted; toggled per transport.
    transport_sound: bool,

    // === Metronome tab ===
    /// Tempo. Edited directly on the big readout (double-click) plus
    /// the slider / ± buttons — no separate text-input buffer needed
    /// since the readout itself is now the editable surface.
    bpm: f32,
    metronome_playing: bool,
    /// When the metronome started — anchors the shared beat grid used by
    /// the arpeggio/exercise transports (3d). `None` when stopped.
    /// Transient (not persisted).
    metronome_started_at: Option<std::time::Instant>,
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
    /// Browse cursor for previewing a set's items on the Practice tab.
    practice_item_idx: usize,
    /// Practice-mode tempo + bars-per-item. They parameterize the
    /// "Rehearse this set" recipe (each card's `Timing.bpm` / `Hold::Bars`);
    /// the old inline runner was retired in U8.
    practice_bpm: f32,
    practice_bars_per_item: u8,

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
    /// True when arming the tuner paused a running metronome, so disarming
    /// can restore it (the session resource arbiter, 3c). Transient.
    tuner_paused_metronome: bool,
    /// Same, for a running Song playback. Transient.
    tuner_paused_song: bool,
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
    /// Fretboard↔info pane split fraction, shared across the fretboard
    /// tabs and persisted. `0.0` = minimum fretboard width.
    split_ratio: f64,
    /// Visible fretboard span (frets from the nut), 4..=12 — the
    /// fretboard scope dial, shared across the fretboard tabs.
    fret_span: u8,
    /// First visible fret of the windowed display (0 = from the nut).
    /// The up/down arrows on the fretboard widget slide this so a
    /// ≤12-fret window can move up the neck past the 12th fret. Clamped
    /// so `fret_start + fret_span` stays within the 24-fret model.
    fret_start: u8,
    /// The instrument-surface composition: which widget modules are
    /// mounted in the left stack, in order, with per-module visibility
    /// + size weight. Always contains exactly one `Fretboard`. Persisted
    /// so the user's chosen layout restores. (Phase 3a: model only —
    /// rendering still mounts just the fretboard until 3b.)
    surface: Vec<SurfaceModule>,
    /// User-authored themes (runtime mirror of `Settings.user_themes`).
    user_themes: Vec<settings::UserThemeDef>,
    /// User-authored tunings (runtime mirror of `Settings.user_tunings`).
    user_tunings: Vec<settings::UserTuningDef>,
    /// User-authored progressions (runtime mirror of
    /// `Settings.user_progressions`).
    user_progressions: Vec<settings::UserProgressionDef>,
    /// User-authored exercises (runtime mirror of `Settings.user_exercises`).
    user_exercises: Vec<settings::UserExerciseDef>,
    /// The set (redesign U1): cards collected from the lenses for stepped
    /// practice. Persisted via `Settings.set`.
    set: Set,
    /// True while the set is auto-advancing through its cards (U6d).
    /// Transient.
    set_playing: bool,
    /// Wall-clock seconds elapsed on the current card during auto-advance.
    /// Transient; drives the per-card `Hold` timer.
    set_elapsed_secs: f32,
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
            last_lens: Tab::Scales,
            root: ChromaticPc::C,
            scale_idx,
            scale_label_mode: LabelMode::default(),
            chord_idx,
            chord_label_mode: LabelMode::default(),
            chord_show_voicing: false,
            chord_voicing_idx: 0,
            progression_idx: None,
            progression_expanded_chord: None,
            progression_overlay_mode: false,
            progression_voicing_idx: Vec::new(),
            progression_cards_panel_width: 0.0,
            exercise_idx: 0,
            exercise_starting_fret: 1,
            exercise_step_idx: 0,
            exercise_playing: false,
            exercise_bpm: 80.0,
            // Arpeggios share the chord catalog; default to the same
            // entry the Chords lens starts on (Major).
            arpeggio_idx: chord_idx,
            arpeggio_position_idx: 0,
            arpeggio_step_idx: 0,
            arpeggio_playing: false,
            arpeggio_bpm: 80.0,
            arpeggio_direction: ArpeggioDirection::default(),
            arpeggio_label: ArpeggioLabel::default(),
            arpeggio_inversion: 0,
            arpeggio_cards_panel_width: 0.0,
            arpeggio_last_sounded: None,
            exercise_last_sounded: None,
            transport_sound: true,
            bpm,
            metronome_playing: false,
            metronome_started_at: None,
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
            practice_bpm: 60.0,
            practice_bars_per_item: 4,
            engine,
            active_instrument,
            sidebars: SidebarVisibility::default(),
            // Full 24-fret model so the windowed display can slide up
            // the neck past the 12th (the visible span stays ≤12 — see
            // `fret_start` / `fret_span`). Positions repeat an octave
            // above the 12th, which is exactly what the upper window shows.
            fretboard: Fretboard::new(tuning, 24),
            input,
            tuner_active: false,
            tuner_paused_metronome: false,
            tuner_paused_song: false,
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
            split_ratio: 0.0,
            fret_span: 12,
            fret_start: 0,
            surface: default_surface(),
            user_themes: Vec::new(),
            user_tunings: Vec::new(),
            user_progressions: Vec::new(),
            user_exercises: Vec::new(),
            set: Set::default(),
            set_playing: false,
            set_elapsed_secs: 0.0,
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
        // Tuner / Metronome are no longer reachable destinations (they're
        // surface modules now); a config saved on one of those tabs lands
        // on the Fretboard surface instead of an unreachable view.
        self.tab = match s.tab {
            Tab::Tuner | Tab::Metronome => Tab::default(),
            other => other,
        };
        // Themes: restore user themes, then resolve the active one. An
        // `active_user_theme` naming a theme that still exists wins;
        // otherwise fall back to the built-in `theme_mode`.
        self.split_ratio = s.split_ratio;
        self.fret_span = s.fret_span.clamp(4, 12);
        self.fret_start = s.fret_start.min(FRETBOARD_MODEL_FRETS.saturating_sub(self.fret_span));
        self.surface = sanitize_surface(s.surface);
        self.user_themes = s.user_themes;
        self.user_tunings = s.user_tunings;
        self.user_progressions = s.user_progressions;
        self.user_exercises = s.user_exercises;
        self.set = s.set;
        if self.set.cursor >= self.set.cards.len() {
            self.set.cursor = self.set.cards.len().saturating_sub(1);
        }
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
            self.fretboard = Fretboard::new(Tuning::from_spec(spec), 24);
        }
        self.sidebars = s.sidebars;

        // Scales / Chords / Progressions — clamp every index to the
        // catalog length so a stale save against an older catalog
        // version doesn't panic at first render.
        // Shared musical root for all theory lenses.
        self.root = s.root;

        let sc_len = scale_catalog().len().max(1);
        self.scale_idx = s.scale_idx.min(sc_len - 1);
        self.scale_label_mode = s.scale_label_mode;

        let ch_len = chord_catalog().len().max(1);
        self.chord_idx = s.chord_idx.min(ch_len - 1);
        self.chord_label_mode = s.chord_label_mode;
        self.chord_show_voicing = s.chord_show_voicing;
        self.chord_voicing_idx = s.chord_voicing_idx;

        // Selection spans the catalog then the user progressions.
        let pg_len = progression_catalog().len() + self.user_progressions.len();
        self.progression_idx = s
            .progression_idx
            .filter(|i| *i < pg_len);
        self.progression_overlay_mode = s.progression_overlay_mode;

        let ex_len = (exercise_catalog().len() + self.user_exercises.len()).max(1);
        self.exercise_idx = s.exercise_idx.min(ex_len - 1);
        self.exercise_starting_fret = s.exercise_starting_fret;
        self.exercise_bpm = s.exercise_bpm;

        // Arpeggio quality indexes the chord catalog; clamp like the others.
        let ch_arp_len = chord_catalog().len().max(1);
        self.arpeggio_idx = s.arpeggio_idx.min(ch_arp_len - 1);
        self.arpeggio_position_idx = s.arpeggio_position_idx;
        self.arpeggio_bpm = s.arpeggio_bpm;
        self.arpeggio_direction = s.arpeggio_direction;
        self.arpeggio_label = s.arpeggio_label;
        self.arpeggio_inversion = s.arpeggio_inversion;
        self.transport_sound = s.transport_sound;

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
            split_ratio: self.split_ratio,
            fret_span: self.fret_span,
            fret_start: self.fret_start,
            surface: self.surface.clone(),
            user_themes: self.user_themes.clone(),
            user_tunings: self.user_tunings.clone(),
            user_progressions: self.user_progressions.clone(),
            user_exercises: self.user_exercises.clone(),
            set: self.set.clone(),
            active_user_theme: self.active_user.clone(),
            active_instrument: settings::instrument_to_str(self.active_instrument).to_string(),
            tuning_name: Some(self.fretboard.tuning.name.clone()),
            sidebars: self.sidebars,
            root: self.root,
            scale_idx: self.scale_idx,
            scale_label_mode: self.scale_label_mode,
            chord_idx: self.chord_idx,
            chord_label_mode: self.chord_label_mode,
            chord_show_voicing: self.chord_show_voicing,
            chord_voicing_idx: self.chord_voicing_idx,
            progression_idx: self.progression_idx,
            progression_overlay_mode: self.progression_overlay_mode,
            exercise_idx: self.exercise_idx,
            exercise_starting_fret: self.exercise_starting_fret,
            exercise_bpm: self.exercise_bpm,
            arpeggio_idx: self.arpeggio_idx,
            arpeggio_position_idx: self.arpeggio_position_idx,
            arpeggio_bpm: self.arpeggio_bpm,
            arpeggio_direction: self.arpeggio_direction,
            arpeggio_label: self.arpeggio_label,
            arpeggio_inversion: self.arpeggio_inversion,
            transport_sound: self.transport_sound,
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

    /// Is a module of this kind currently mounted *and* visible in the
    /// instrument surface? Drives the mount-toggle chip state.
    fn module_shown(&self, kind: ModuleKind) -> bool {
        self.surface.iter().any(|m| m.kind == kind && m.visible)
    }

    /// Toggle a companion module on/off in the surface. First mount adds
    /// it (appended after the fretboard); subsequent toggles flip its
    /// visibility, preserving its position + size weight so unmount /
    /// remount doesn't lose where it sat. Fretboard can't be toggled —
    /// it's the always-on primary.
    fn toggle_module(&mut self, kind: ModuleKind) {
        if kind == ModuleKind::Fretboard {
            return;
        }
        if let Some(m) = self.surface.iter_mut().find(|m| m.kind == kind) {
            m.visible = !m.visible;
        } else {
            self.surface.push(SurfaceModule::new(kind));
        }
    }

    /// Highest `fret_start` that keeps the visible window within the
    /// 24-fret model.
    fn max_fret_start(&self) -> u8 {
        FRETBOARD_MODEL_FRETS.saturating_sub(self.fret_span)
    }

    /// Slide the visible fret window up/down the neck, clamped so it
    /// stays within the model. Driven by the fretboard widget's
    /// up/down arrows.
    fn nudge_fret_start(&mut self, delta: i32) {
        let max = self.max_fret_start() as i32;
        self.fret_start = (self.fret_start as i32 + delta).clamp(0, max) as u8;
    }

    /// Adjust the size weight of the module at `surface[idx]` so its
    /// divider sits at fraction `p` of the space it shares with the
    /// modules below it (whose relative sizes are held fixed). Called
    /// from the surface stack's `on_split_changed`.
    fn set_module_split(&mut self, idx: usize, p: f64) {
        let p = p.clamp(0.05, 0.95);
        let tail: f64 = self
            .surface
            .iter()
            .enumerate()
            .filter(|(j, m)| *j > idx && m.visible)
            .map(|(_, m)| m.weight)
            .sum();
        if tail > 0.0 {
            if let Some(m) = self.surface.get_mut(idx) {
                m.weight = p * tail / (1.0 - p);
            }
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

    /// Set one HSL component (0=H in degrees, 1=S in %, 2=L in %) of one
    /// seed (0=primary, 1=secondary, 2=tertiary, 3=neutral) on the
    /// active user theme, then re-derive. Drives the live color sliders.
    fn set_seed_hsl(&mut self, field: u8, comp: u8, value: f64) {
        self.edit_active_user(|d| {
            let hex = match field {
                0 => &mut d.primary,
                1 => &mut d.secondary,
                2 => &mut d.tertiary,
                _ => &mut d.neutral,
            };
            let col = audio_widgets::theme::color_from_hex(hex)
                .unwrap_or(Color::from_rgb8(0x80, 0x80, 0x80));
            let (mut h, mut s, mut l) = audio_widgets::theme::color_to_hsl(col);
            match comp {
                0 => h = value,
                1 => s = value / 100.0,
                _ => l = value / 100.0,
            }
            *hex = audio_widgets::theme::color_to_hex(audio_widgets::theme::color_from_hsl(h, s, l));
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

    /// Set one HSL component of a custom text tier (no-op if derived).
    fn set_text_hsl(&mut self, header: bool, comp: u8, value: f64) {
        self.edit_active_user(|d| {
            let slot = if header { &mut d.text_header } else { &mut d.text_body };
            if let Some(hex) = slot {
                let col = audio_widgets::theme::color_from_hex(hex)
                    .unwrap_or(Color::from_rgb8(0x80, 0x80, 0x80));
                let (mut h, mut s, mut l) = audio_widgets::theme::color_to_hsl(col);
                match comp {
                    0 => h = value,
                    1 => s = value / 100.0,
                    _ => l = value / 100.0,
                }
                *hex = audio_widgets::theme::color_to_hex(audio_widgets::theme::color_from_hsl(
                    h, s, l,
                ));
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

    // === Custom tunings (Phase 4) ===

    /// Create a new custom tuning cloned from the current fretboard, name
    /// it "Custom N", and apply it.
    fn new_user_tuning(&mut self) {
        let mut n = 1;
        let name = loop {
            let candidate = format!("Custom {n}");
            if !self.user_tunings.iter().any(|t| t.name == candidate) {
                break candidate;
            }
            n += 1;
        };
        let midi: Vec<i32> = self.fretboard.tuning.strings.iter().map(|p| p.midi()).collect();
        self.user_tunings.push(settings::UserTuningDef {
            name: name.clone(),
            instrument: settings::instrument_to_str(self.active_instrument).to_string(),
            midi,
        });
        self.apply_user_tuning(&name);
    }

    /// Cycle the active tuning through the catalog tunings for the
    /// current instrument followed by the user's custom ones (wrapping).
    /// Driven by the header ‹/›.
    fn cycle_tuning(&mut self, delta: i32) {
        let inst = self.active_instrument;
        let inst_str = settings::instrument_to_str(inst);
        let mut names: Vec<String> = tuning_catalog()
            .iter()
            .filter(|t| t.instrument == inst)
            .map(|t| t.name.to_string())
            .collect();
        let cat_count = names.len();
        for t in self.user_tunings.iter().filter(|t| t.instrument == inst_str) {
            names.push(t.name.clone());
        }
        if names.is_empty() {
            return;
        }
        let cur = names
            .iter()
            .position(|n| *n == self.fretboard.tuning.name)
            .unwrap_or(0) as i32;
        let next = (cur + delta).rem_euclid(names.len() as i32) as usize;
        if next < cat_count {
            if let Some(spec) = tuning_catalog()
                .iter()
                .filter(|t| t.instrument == inst)
                .nth(next)
            {
                self.fretboard = Fretboard::new(Tuning::from_spec(spec), 24);
            }
        } else {
            let name = names[next].clone();
            self.apply_user_tuning(&name);
        }
    }

    /// Apply a user tuning by name — rebuilds the fretboard against it
    /// (and switches the active instrument to match).
    fn apply_user_tuning(&mut self, name: &str) {
        if let Some(def) = self.user_tunings.iter().find(|t| t.name == name) {
            let tuning = user_tuning_to_tuning(def);
            self.active_instrument = tuning.instrument;
            self.fretboard = Fretboard::new(tuning, 24);
        }
    }

    /// Mutate a user tuning in place; if it's the active fretboard tuning,
    /// re-apply so the neck updates live.
    fn edit_user_tuning(&mut self, name: &str, f: impl FnOnce(&mut settings::UserTuningDef)) {
        if let Some(def) = self.user_tunings.iter_mut().find(|t| t.name == name) {
            f(def);
        }
        if self.fretboard.tuning.name == name {
            self.apply_user_tuning(name);
        }
    }

    /// Nudge one open string of a user tuning by `delta` semitones.
    fn nudge_user_string(&mut self, name: &str, idx: usize, delta: i32) {
        self.edit_user_tuning(name, |d| {
            if let Some(m) = d.midi.get_mut(idx) {
                *m = (*m + delta).clamp(12, 108);
            }
        });
    }

    /// Append a string (a fourth above the current top) to a user tuning.
    fn add_user_string(&mut self, name: &str) {
        self.edit_user_tuning(name, |d| {
            let top = d.midi.last().copied().unwrap_or(40);
            d.midi.push((top + 5).min(108));
        });
    }

    /// Drop the top string of a user tuning (keeps at least one).
    fn remove_user_string(&mut self, name: &str) {
        self.edit_user_tuning(name, |d| {
            if d.midi.len() > 1 {
                d.midi.pop();
            }
        });
    }

    /// Remove a user tuning. If it's the active fretboard tuning, fall
    /// back to the instrument's default catalog tuning.
    fn remove_user_tuning(&mut self, name: &str) {
        let was_active = self.fretboard.tuning.name == name;
        self.user_tunings.retain(|t| t.name != name);
        if was_active {
            if let Some(spec) = tuning_catalog()
                .iter()
                .find(|t| t.instrument == self.active_instrument)
            {
                self.fretboard = Fretboard::new(Tuning::from_spec(spec), 24);
            }
        }
    }

    // === Custom progressions (Phase 4) ===

    /// Create a new custom progression (a single I-major chord) and select
    /// it on the Progression lens.
    fn new_user_progression(&mut self) {
        let mut n = 1;
        let name = loop {
            let candidate = format!("Progression {n}");
            if !self.user_progressions.iter().any(|p| p.name == candidate) {
                break candidate;
            }
            n += 1;
        };
        self.user_progressions.push(settings::UserProgressionDef {
            name,
            roles: vec![settings::ProgRoleSpec {
                degree: 1,
                alteration: 0,
                quality: 0,
            }],
        });
        // Select it (so the editor marks it active) but stay on Settings
        // — `Apply` is what jumps to the Progression lens.
        let pos = self.user_progressions.len() - 1;
        self.progression_idx = Some(progression_catalog().len() + pos);
        self.progression_expanded_chord = Some(0);
        self.progression_voicing_idx = vec![0; 1];
    }

    fn edit_user_progression(
        &mut self,
        name: &str,
        f: impl FnOnce(&mut settings::UserProgressionDef),
    ) {
        if let Some(p) = self.user_progressions.iter_mut().find(|p| p.name == name) {
            f(p);
        }
    }

    /// Remove a user progression; clears the selection if it pointed past
    /// the (now shorter) combined list.
    fn remove_user_progression(&mut self, name: &str) {
        self.user_progressions.retain(|p| p.name != name);
        let len = progression_catalog().len() + self.user_progressions.len();
        if matches!(self.progression_idx, Some(i) if i >= len) {
            self.progression_idx = None;
        }
    }

    fn add_prog_chord(&mut self, name: &str) {
        self.edit_user_progression(name, |p| {
            p.roles.push(settings::ProgRoleSpec {
                degree: 1,
                alteration: 0,
                quality: 0,
            });
        });
    }

    fn remove_prog_chord(&mut self, name: &str, idx: usize) {
        self.edit_user_progression(name, |p| {
            if p.roles.len() > 1 && idx < p.roles.len() {
                p.roles.remove(idx);
            }
        });
    }

    fn nudge_prog_degree(&mut self, name: &str, idx: usize, delta: i32) {
        self.edit_user_progression(name, |p| {
            if let Some(r) = p.roles.get_mut(idx) {
                r.degree = ((r.degree as i32 + delta).clamp(1, 7)) as u8;
            }
        });
    }

    fn cycle_prog_alteration(&mut self, name: &str, idx: usize) {
        self.edit_user_progression(name, |p| {
            if let Some(r) = p.roles.get_mut(idx) {
                r.alteration = (r.alteration + 1) % 3;
            }
        });
    }

    fn cycle_prog_quality(&mut self, name: &str, idx: usize, delta: i32) {
        let n = RoleQuality::ALL.len() as i32;
        self.edit_user_progression(name, |p| {
            if let Some(r) = p.roles.get_mut(idx) {
                r.quality = ((r.quality as i32 + delta).rem_euclid(n)) as u8;
            }
        });
    }

    // === Custom exercises (Phase 4) ===

    /// Create a new custom exercise (one step) and select it, staying on
    /// the current tab (Apply is what jumps to the lens).
    fn new_user_exercise(&mut self) {
        let mut n = 1;
        let name = loop {
            let candidate = format!("Exercise {n}");
            if !self.user_exercises.iter().any(|e| e.name == candidate) {
                break candidate;
            }
            n += 1;
        };
        self.user_exercises.push(settings::UserExerciseDef {
            name,
            steps: vec![settings::ExStepSpec {
                string: 0,
                fret: 1,
                finger: 1,
            }],
        });
        let pos = self.user_exercises.len() - 1;
        self.exercise_idx = exercise_catalog().len() + pos;
        self.exercise_step_idx = 0;
        self.exercise_playing = false;
    }

    fn edit_user_exercise(&mut self, name: &str, f: impl FnOnce(&mut settings::UserExerciseDef)) {
        if let Some(e) = self.user_exercises.iter_mut().find(|e| e.name == name) {
            f(e);
        }
    }

    fn remove_user_exercise(&mut self, name: &str) {
        self.user_exercises.retain(|e| e.name != name);
        let len = exercise_catalog().len() + self.user_exercises.len();
        if self.exercise_idx >= len {
            self.exercise_idx = 0;
        }
    }

    fn add_ex_step(&mut self, name: &str) {
        self.edit_user_exercise(name, |e| {
            // New step copies the last (a sensible starting point).
            let last = e.steps.last().cloned().unwrap_or(settings::ExStepSpec {
                string: 0,
                fret: 1,
                finger: 1,
            });
            e.steps.push(last);
        });
    }

    fn remove_ex_step(&mut self, name: &str, idx: usize) {
        self.edit_user_exercise(name, |e| {
            if e.steps.len() > 1 && idx < e.steps.len() {
                e.steps.remove(idx);
            }
        });
    }

    /// Nudge one field of one step. `field`: 0=string, 1=fret, 2=finger.
    fn nudge_ex_step(&mut self, name: &str, idx: usize, field: u8, delta: i32) {
        let strings = self.fretboard.tuning.strings.len().max(1) as i32;
        self.edit_user_exercise(name, |e| {
            if let Some(st) = e.steps.get_mut(idx) {
                match field {
                    0 => st.string = ((st.string as i32 + delta).rem_euclid(strings)) as u8,
                    1 => st.fret = (st.fret as i32 + delta).clamp(0, 24) as u8,
                    _ => st.finger = (st.finger as i32 + delta).clamp(0, 4) as u8,
                }
            }
        });
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

    fn cycle_exercise(&mut self, direction: i32) {
        // Span the catalog then the user exercises.
        let len = exercise_catalog().len() + self.user_exercises.len();
        if len == 0 {
            return;
        }
        let cur = self.exercise_idx as i32;
        let next = (cur + direction).rem_euclid(len as i32);
        self.exercise_idx = next as usize;
    }

    /// Name of the currently-selected progression (catalog or user), if
    /// one is selected.
    fn current_progression_name(&self) -> Option<String> {
        let idx = self.progression_idx?;
        let cat = progression_catalog();
        if idx < cat.len() {
            Some(cat[idx].name.to_string())
        } else {
            self.user_progressions
                .get(idx - cat.len())
                .map(|p| p.name.clone())
        }
    }

    /// Name of the currently-selected exercise (catalog or user).
    fn current_exercise_name(&self) -> Option<String> {
        let cat = exercise_catalog();
        if self.exercise_idx < cat.len() {
            Some(cat[self.exercise_idx].name.to_string())
        } else {
            self.user_exercises
                .get(self.exercise_idx - cat.len())
                .map(|e| e.name.clone())
        }
    }

    /// Build a [`Card`] from the material the active lens is showing
    /// (redesign R1). Returns `None` on a non-lens tab, or when the lens
    /// has nothing selected (e.g. Progressions cold-start). The card
    /// stores selections by name + the shared root/instrument so it can
    /// be re-realized on the stage later.
    fn capture_cards(&self) -> Vec<Card> {
        let root = self.root;
        let root_label = root.display();
        // Fresh default setting for the active instrument (U1 leaves
        // tuning/capo unset; the stage uses the live tuning).
        let base_setting = || Setting {
            instrument: settings::instrument_to_str(self.active_instrument).to_string(),
            tuning: None,
            capo: None,
            voicing_idx: None,
            fret_window: None,
        };
        match self.tab {
            Tab::Scales => {
                let scale = self.current_scale().name.to_string();
                vec![Card {
                    label: format!("{root_label} — {scale} scale"),
                    material: Material::Scale { name: scale, root: root.to_pitch_class() },
                    setting: base_setting(),
                    touch: Touch::Block,
                    timing: Timing::default(),
                    from: None,
                }]
            }
            Tab::Chords => {
                let chord = self.current_chord().name.to_string();
                let voicing = self.chord_voicing_idx;
                let (label, voicing_idx) = if self.chord_show_voicing {
                    (format!("{root_label}{chord} · voicing {}", voicing + 1), Some(voicing))
                } else {
                    (format!("{root_label}{chord} · tones"), None)
                };
                vec![Card {
                    label,
                    material: Material::Chord { name: chord, root: root.to_pitch_class() },
                    setting: Setting { voicing_idx, ..base_setting() },
                    touch: Touch::Block,
                    timing: Timing::default(),
                    from: None,
                }]
            }
            Tab::Arpeggios => {
                let chord = self.current_chord_for_arpeggio().name.to_string();
                let inv = self.arpeggio_inversion;
                let inv_label = if inv == 0 { String::new() } else { format!(" · inv {inv}") };
                vec![Card {
                    label: format!("{root_label}{chord} arp{inv_label}"),
                    material: Material::Chord { name: chord, root: root.to_pitch_class() },
                    setting: base_setting(),
                    touch: Touch::Arpeggiate { direction: self.arpeggio_direction, inversion: inv },
                    timing: Timing::default(),
                    from: None,
                }]
            }
            // A progression is a recipe: it fills the set with one chord
            // card per role (U3-lite; uses the same apply-in-key the lens
            // already does). Each card is `from` the progression.
            Tab::Progressions => {
                let Some(name) = self.current_progression_name() else {
                    return Vec::new();
                };
                let key_root = self.root.to_pitch(4);
                let Some(major) = scale_catalog().iter().find(|s| s.name == "Major") else {
                    return Vec::new();
                };
                let cat = progression_catalog();
                let chords = if let Some(p) = cat.iter().find(|p| p.name == name) {
                    p.apply_in_key(key_root, major).ok()
                } else if let Some(def) = self.user_progressions.iter().find(|p| p.name == name) {
                    apply_roles_in_key(&user_progression_roles(def), key_root, major).ok()
                } else {
                    None
                };
                let Some(chords) = chords else {
                    return Vec::new();
                };
                chords
                    .into_iter()
                    .map(|c| {
                        let croot = PitchClass::new(c.root.midi().rem_euclid(12) as u8);
                        Card {
                            label: format_progression_chord_symbol(&c),
                            material: Material::Chord {
                                name: c.formula.name.to_string(),
                                root: croot,
                            },
                            setting: base_setting(),
                            touch: Touch::Block,
                            timing: Timing::default(),
                            from: Some(Recipe::Progression {
                                name: name.clone(),
                                key: root.to_pitch_class(),
                            }),
                        }
                    })
                    .collect()
            }
            Tab::Exercises => {
                let Some(exercise) = self.current_exercise_name() else {
                    return Vec::new();
                };
                vec![Card {
                    label: exercise.clone(),
                    material: Material::Riff { name: exercise.clone() },
                    setting: base_setting(),
                    touch: Touch::Block,
                    timing: Timing::default(),
                    from: Some(Recipe::Exercise { name: exercise }),
                }]
            }
            _ => Vec::new(),
        }
    }

    /// The chord quality the arpeggio lens is built on (its `arpeggio_idx`
    /// indexes the chord catalog).
    fn current_chord_for_arpeggio(&self) -> &'static ChordFormula {
        let cat = chord_catalog();
        &cat[self.arpeggio_idx.min(cat.len() - 1)]
    }

    /// Capture the active lens's material as one or more cards and push
    /// them onto the set. No-op when there's nothing to capture.
    fn rehearse_current(&mut self) {
        for card in self.capture_cards() {
            self.set.push(card);
        }
    }

    /// Recipe: turn the selected practice set into cards on the set (U2).
    /// Each item becomes a card holding the practice tempo and a
    /// bars-per-item `Hold`, tagged `from` the practice set. The Practice
    /// tab is a way to *fill* a set, not its own runner.
    fn fill_set_from_practice(&mut self) {
        let Some(set) = self.current_practice_set() else {
            return;
        };
        let name = set.name.clone();
        let instrument = settings::instrument_to_str(self.active_instrument).to_string();
        let bpm = self.practice_bpm;
        let bars = self.practice_bars_per_item;
        let cards: Vec<Card> = set
            .items
            .iter()
            .map(|item| practice_item_to_card(item, &instrument, bpm, bars, &name))
            .collect();
        for card in cards {
            self.set.push(card);
        }
    }

    /// Recipe: project the current song's chord bars into cards on the set
    /// (U4a). The song engine keeps owning recorded-audio playback; this
    /// is a one-way projection.
    fn fill_set_from_song(&mut self) {
        let instrument = settings::instrument_to_str(self.active_instrument).to_string();
        for card in song_to_cards(&self.song_view, &instrument) {
            self.set.push(card);
        }
    }

    /// Realize the queued card at `idx` onto the instrument stage
    /// (redesign R2): restore its root + instrument context, apply its
    /// lens-specific selection (resolved by name), and switch to the
    /// matching lens. Moves the rehearsal cursor to `idx`.
    fn load_card(&mut self, idx: usize) {
        let Some(card) = self.set.cards.get(idx).cloned() else {
            return;
        };
        self.set.cursor = idx;
        // Restore the instrument family (best-effort: retunes to that
        // instrument's default tuning — a card stores the family, not a
        // specific custom tuning).
        let instrument = settings::instrument_from_str(&card.setting.instrument);
        if instrument != self.active_instrument {
            if let Some(spec) = tuning_catalog().iter().find(|s| s.instrument == instrument) {
                self.active_instrument = instrument;
                self.fretboard = Fretboard::new(Tuning::from_spec(spec), 24);
            }
        }
        match &card.material {
            Material::Scale { name, root } => {
                self.root = ChromaticPc::from_pitch_class(*root);
                if let Some(i) = scale_catalog().iter().position(|s| s.name == *name) {
                    self.scale_idx = i;
                }
                self.tab = Tab::Scales;
            }
            // A chord card lands on the Arpeggio lens when its touch is
            // arpeggiate, otherwise the Chord lens.
            Material::Chord { name, root } => {
                self.root = ChromaticPc::from_pitch_class(*root);
                match &card.touch {
                    Touch::Arpeggiate { direction, inversion } => {
                        if let Some(i) = chord_catalog().iter().position(|c| c.name == *name) {
                            self.arpeggio_idx = i;
                        }
                        self.arpeggio_direction = *direction;
                        self.arpeggio_inversion = *inversion;
                        self.arpeggio_position_idx = 0;
                        self.arpeggio_step_idx = 0;
                        self.arpeggio_playing = false;
                        self.tab = Tab::Arpeggios;
                    }
                    Touch::Block => {
                        if let Some(i) = chord_catalog().iter().position(|c| c.name == *name) {
                            self.chord_idx = i;
                        }
                        match card.setting.voicing_idx {
                            Some(v) => {
                                self.chord_voicing_idx = v;
                                self.chord_show_voicing = true;
                            }
                            None => self.chord_show_voicing = false,
                        }
                        self.tab = Tab::Chords;
                    }
                }
            }
            // A riff currently maps to an exercise (catalog or user) by name.
            Material::Riff { name } => {
                let cat = exercise_catalog();
                if let Some(i) = cat.iter().position(|e| e.name == *name) {
                    self.exercise_idx = i;
                } else if let Some(pos) = self.user_exercises.iter().position(|e| e.name == *name) {
                    self.exercise_idx = cat.len() + pos;
                }
                self.exercise_step_idx = 0;
                self.exercise_playing = false;
                self.tab = Tab::Exercises;
            }
        }
        if tab_has_fretboard(self.tab) {
            self.last_lens = self.tab;
        }
    }

    /// The canonical "card → neck" path (U5): given a card, compute the
    /// dots + labels + window to draw, resolving its material by name
    /// against the live fretboard. One resolver the stage consumers share,
    /// instead of each lens recomputing positions. (U7 will lift the
    /// portable core into `woodshedding`; the user-exercise lookup stays
    /// app-side.)
    fn resolve_card_for_stage(&self, card: &Card) -> StageRender {
        let fb = &self.fretboard;
        match &card.material {
            Material::Scale { name, root } => {
                let Some(formula) = scale_catalog().iter().find(|s| s.name == *name) else {
                    return StageRender::empty(format!("scale \"{name}\" not found"));
                };
                let mut positions = fb
                    .positions_for_scale(formula, ChromaticPc::from_pitch_class(*root).to_pitch(4))
                    .unwrap_or_default();
                apply_fret_window(&mut positions, card.setting.fret_window);
                let labels = compute_labels(LabelMode::Notes, &positions);
                StageRender {
                    positions,
                    labels,
                    fret_window: card.setting.fret_window,
                    warning: None,
                }
            }
            Material::Chord { name, root } => {
                let Some(formula) = chord_catalog().iter().find(|c| c.name == *name) else {
                    return StageRender::empty(format!("chord \"{name}\" not found"));
                };
                let root_pitch = ChromaticPc::from_pitch_class(*root).to_pitch(4);
                // A specific voicing renders that shape, windowed to it;
                // otherwise every chord tone across the neck (the "tones"
                // view), optionally clamped to a pinned window.
                if let Some(vidx) = card.setting.voicing_idx {
                    let voicings = enumerate_voicings(fb, formula, root_pitch);
                    if let Some(v) = voicings.get(vidx) {
                        let positions: Vec<Position> = voicing_to_positions(v)
                            .into_iter()
                            .filter(|p| p.fret > 0)
                            .collect();
                        let labels = compute_labels(LabelMode::Notes, &positions);
                        let start = positions
                            .iter()
                            .map(|p| p.fret)
                            .min()
                            .unwrap_or(1)
                            .saturating_sub(1);
                        return StageRender {
                            positions,
                            labels,
                            fret_window: Some(FretWindow { start, span: 5 }),
                            warning: None,
                        };
                    }
                }
                let mut positions = fb
                    .positions_for_chord(formula, root_pitch)
                    .unwrap_or_default();
                apply_fret_window(&mut positions, card.setting.fret_window);
                let labels = compute_labels(LabelMode::Notes, &positions);
                StageRender {
                    positions,
                    labels,
                    fret_window: card.setting.fret_window,
                    warning: None,
                }
            }
            Material::Riff { name } => {
                let steps: Vec<ExerciseStep> = if let Some(ex) =
                    exercise_catalog().iter().find(|e| e.name == *name)
                {
                    ex.generate(
                        &fb.tuning,
                        &ExerciseParams {
                            starting_fret: 1,
                            direction: ExerciseDirection::Both,
                            trill_repeats: 8,
                        },
                    )
                } else if let Some(def) = self.user_exercises.iter().find(|e| e.name == *name) {
                    user_exercise_steps(def)
                } else {
                    return StageRender::empty(format!("exercise \"{name}\" not found"));
                };
                let mut seen = std::collections::HashSet::new();
                let positions: Vec<Position> = steps
                    .into_iter()
                    .filter(|s| seen.insert((s.string_index, s.fret)))
                    .map(|s| Position {
                        string_index: s.string_index,
                        fret: s.fret,
                        pitch: fb.pitch_at(s.string_index, s.fret),
                        interval_from_root: None,
                    })
                    .collect();
                let labels = positions
                    .iter()
                    .map(|p| format!("{}{}", p.pitch.name, accidental_short(p.pitch.accidental)))
                    .collect();
                StageRender {
                    positions,
                    labels,
                    fret_window: None,
                    warning: None,
                }
            }
        }
    }

    /// Point the set cursor at `idx` without switching lenses. The
    /// Rehearsal tab's own stage renders the cursor card via the resolver,
    /// so selecting a card there doesn't need a tab switch (unlike
    /// `load_card`, which jumps to a lens for authoring).
    fn set_cursor(&mut self, idx: usize) {
        if idx < self.set.cards.len() {
            self.set.cursor = idx;
            self.sound_current_card();
        }
    }

    /// Move the set cursor by `dir` (wrapping when looping, else clamping)
    /// without switching lenses. For the Rehearsal stage's prev/next.
    fn cursor_step(&mut self, dir: i32) {
        let len = self.set.cards.len();
        if len == 0 {
            return;
        }
        let raw = self.set.cursor as i32 + dir;
        self.set.cursor = match self.set.loop_mode {
            LoopMode::All => raw.rem_euclid(len as i32) as usize,
            LoopMode::Off => raw.clamp(0, len as i32 - 1) as usize,
        };
        self.sound_current_card();
    }

    /// Sound the card under the cursor (set-card audio): a chord card plays
    /// its tones as a block, a scale card plays its root so you hear the
    /// key, a riff is silent for now. Respects the `transport_sound`
    /// (Sound/Muted) toggle. One-shot voices via the song engine, mixed even when
    /// the song isn't playing.
    fn sound_current_card(&mut self) {
        if !self.transport_sound {
            return;
        }
        let freqs: Vec<f32> = {
            let Some(card) = self.set.cards.get(self.set.cursor) else {
                return;
            };
            match &card.material {
                Material::Chord { name, root } => chord_catalog()
                    .iter()
                    .find(|c| c.name == *name)
                    .and_then(|f| {
                        f.apply_to(ChromaticPc::from_pitch_class(*root).to_pitch(3)).ok()
                    })
                    .map(|ps| ps.iter().map(|p| p.frequency() as f32).collect())
                    .unwrap_or_default(),
                Material::Scale { root, .. } => {
                    vec![ChromaticPc::from_pitch_class(*root).to_pitch(3).frequency() as f32]
                }
                Material::Riff { .. } => Vec::new(),
            }
        };
        if freqs.is_empty() {
            return;
        }
        if let Some(h) = self.ensure_song_engine() {
            for f in freqs {
                h.play_note_now(f, 0.8);
            }
        }
    }

    fn start_set_playback(&mut self) {
        if !self.set.cards.is_empty() {
            self.set_playing = true;
            self.set_elapsed_secs = 0.0;
            self.sound_current_card();
        }
    }

    fn stop_set_playback(&mut self) {
        self.set_playing = false;
    }

    /// Advance the set-playback timer by `dt` seconds; step the cursor when
    /// the current card's dwell elapses. Stops at the end when not looping
    /// (U6d). Visual for now — the neck reframes per card; sounding each
    /// card is a later pass.
    fn tick_set_playback(&mut self, dt: f32) {
        if !self.set_playing {
            return;
        }
        let len = self.set.cards.len();
        if len == 0 {
            self.set_playing = false;
            return;
        }
        let cursor = self.set.cursor.min(len - 1);
        // When a song owns the clock (a song card playing on the engine),
        // the engine drives the cursor via `follow_song_cursor`; don't
        // double-advance with our own timer (U4b).
        if self.card_clock(&self.set.cards[cursor]) == Clock::Song {
            return;
        }
        let dur = card_duration_secs(&self.set.cards[cursor]);
        self.set_elapsed_secs += dt;
        if self.set_elapsed_secs >= dur {
            self.set_elapsed_secs = 0.0;
            if cursor + 1 >= len && self.set.loop_mode == LoopMode::Off {
                self.set_playing = false; // reached the end, not looping
            } else {
                self.cursor_step(1);
            }
        }
    }

    /// Which clock governs `card` right now (U4b, derived not stored): the
    /// song engine when a song card is playing, the metronome when it's
    /// running, else manual stepping.
    fn card_clock(&self, card: &Card) -> Clock {
        if matches!(card.from, Some(Recipe::Song { .. })) && self.song_view.playing {
            Clock::Song
        } else if self.metronome_playing {
            Clock::Metronome
        } else {
            Clock::Manual
        }
    }

    /// While a song plays, point the set cursor at the card whose source
    /// bar matches the engine's bar cursor — the song owns time, the set
    /// follows (U4b). No-op when no song card matches the current bar.
    fn follow_song_cursor(&mut self) {
        if !self.song_view.playing {
            return;
        }
        let bar = self.song_view.cursor.bar_idx;
        if let Some(i) = self
            .set
            .cards
            .iter()
            .position(|c| matches!(&c.from, Some(Recipe::Song { bar: b, .. }) if *b == bar))
        {
            self.set.cursor = i;
        }
    }

    /// Step the set cursor and load that card onto the stage. Wraps at the
    /// ends when the set is looping, otherwise clamps. No-op on an empty set.
    fn rehearse_step(&mut self, dir: i32) {
        let len = self.set.cards.len();
        if len == 0 {
            return;
        }
        let raw = self.set.cursor as i32 + dir;
        let next = match self.set.loop_mode {
            LoopMode::All => raw.rem_euclid(len as i32) as usize,
            LoopMode::Off => raw.clamp(0, len as i32 - 1) as usize,
        };
        self.load_card(next);
    }

    /// Cycle a chord card's touch (Block ⇄ Arpeggiate). No-op for material
    /// where it doesn't apply (scales/riffs play as written). Lets a
    /// progression's chord run be switched to an arpeggiated one (U3).
    fn cycle_card_touch(&mut self, idx: usize) {
        if let Some(card) = self.set.cards.get_mut(idx) {
            if matches!(card.material, Material::Chord { .. }) {
                card.touch = match card.touch {
                    Touch::Block => Touch::Arpeggiate {
                        direction: ArpeggioDirection::UpDown,
                        inversion: 0,
                    },
                    Touch::Arpeggiate { .. } => Touch::Block,
                };
            }
        }
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
            self.fretboard = Fretboard::new(Tuning::from_spec(spec), 24);
        }
    }

    fn play_metronome(&mut self) {
        if let Ok((_, handle)) = &self.engine {
            handle.play();
            self.metronome_playing = true;
            // Anchor the shared beat grid so transports (arpeggio /
            // exercise) can phase-lock to the click (3d).
            self.metronome_started_at = Some(std::time::Instant::now());
        }
    }

    fn stop_metronome(&mut self) {
        if let Ok((_, handle)) = &self.engine {
            handle.stop();
            self.metronome_playing = false;
            self.metronome_started_at = None;
        }
    }

    /// Beats elapsed since the metronome started (quarter-note grid),
    /// or `None` when it isn't running. The shared transport clock —
    /// arpeggio / exercise runs advance one note per beat off this so
    /// they line up with the audible click.
    fn metronome_beat(&self) -> Option<u64> {
        let start = self.metronome_started_at?;
        if !self.metronome_playing {
            return None;
        }
        let beat_dur = (60.0 / self.bpm.max(1.0)) as f64;
        Some((start.elapsed().as_secs_f64() / beat_dur.max(0.01)) as u64)
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
        // Resource arbiter (3c): tuning claims focus — running output
        // (metronome click, Song playback) would fight the pitch read, so
        // pause it and remember, to restore on disarm.
        if self.tuner_active && self.metronome_playing {
            self.stop_metronome();
            self.tuner_paused_metronome = true;
        }
        if self.tuner_active && self.song_view.playing {
            if let Some(Ok((_, h))) = self.song_engine.as_ref() {
                h.stop();
            }
            self.tuner_paused_song = true;
        }
    }

    /// Disable the pitch analyzer and clear the cached snapshot.
    fn stop_tuner(&mut self) {
        if let Ok(b) = &self.input {
            b.tuner.set_enabled(false);
        }
        self.tuner_active = false;
        self.tuner_snapshot = None;
        // Restore the metronome / Song if the tuner had paused them.
        if self.tuner_paused_metronome {
            self.tuner_paused_metronome = false;
            self.play_metronome();
        }
        if self.tuner_paused_song {
            self.tuner_paused_song = false;
            if let Some(Ok((_, h))) = self.song_engine.as_ref() {
                h.play();
            }
        }
    }
}

// =================================================================
// View — assembled top-down from `app_logic`.
// =================================================================

fn app_logic(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    // Header + the tab/lens bars are fixed-height "chrome" rows; the
    // tab content fills the remaining window height (`flex(1.0)`). This
    // bounded height is what lets the instrument surface behave as
    // resizable panes sharing one viewport rather than an ever-growing
    // page that scrolls. Each tab now owns its own internal scrolling
    // (tall tabs wrap their body in a portal inside `tab_content`); the
    // fretboard surface fills the height and scrolls per-module.
    // Wrap the whole UI in a resize frame: native decorations are off, so its
    // outer margin band is what makes the borderless window resizable (with a
    // directional resize cursor). The content is inset by that margin.
    let body = window_frame(
        flex_col((
            header(state).flex(0.0),
            tab_bar(state).flex(0.0),
            lens_bar(state).flex(0.0),
            tab_content(state).flex(1.0),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_2),
        RESIZE_MARGIN,
    );

    // Shared-clock heartbeat (3d): while the metronome runs, tick ~30ms
    // so views that derive a cursor from elapsed time (the arpeggio /
    // exercise transports phase-locked to `metronome_beat`) keep
    // re-rendering. The message handler is a no-op — its only job is to
    // drive Xilem's rebuild so the time-based cursor advances.
    let beat_poll = state.metronome_playing.then(|| {
        task_raw(
            move |proxy, _| async move {
                let mut tick = time::interval(Duration::from_millis(30));
                loop {
                    tick.tick().await;
                    if proxy.message(()).is_err() {
                        break;
                    }
                }
            },
            |_s: &mut AppState, _: ()| {},
        )
    });
    // Set auto-advance (U6d): while the set is playing, tick ~50ms and let
    // `tick_set_playback` accumulate elapsed time, stepping the cursor when
    // the current card's `Hold` dwell elapses. The neck reframes off the
    // cursor change.
    let set_poll = state.set_playing.then(|| {
        task_raw(
            move |proxy, _| async move {
                let mut tick = time::interval(Duration::from_millis(50));
                tick.tick().await; // skip the immediate first tick
                loop {
                    tick.tick().await;
                    if proxy.message(()).is_err() {
                        break;
                    }
                }
            },
            |s: &mut AppState, _: ()| s.tick_set_playback(0.05),
        )
    });
    // Song-follow (U4b): while a song plays, refresh the cached song view
    // and point the set cursor at the card matching the engine's bar — the
    // song owns time, the set follows. Active across tabs while playing.
    let song_follow = state.song_view.playing.then(|| {
        task_raw(
            move |proxy, _| async move {
                let mut tick = time::interval(Duration::from_millis(60));
                tick.tick().await;
                loop {
                    tick.tick().await;
                    if proxy.message(()).is_err() {
                        break;
                    }
                }
            },
            |s: &mut AppState, _: ()| {
                s.refresh_song_view();
                s.follow_song_cursor();
            },
        )
    });
    fork(fork(fork(body, beat_poll), set_poll), song_follow)
}

fn header(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    let instrument_name = format!("{}", state.active_instrument);
    // Tuning picker as a ‹› cycle (a dropdown in a fixed header strip
    // blows the layout open). Cycles catalog tunings for the current
    // instrument then the user's custom ones; the full browsable list
    // lives in Settings → Custom tunings.
    let tuning_name = state.fretboard.tuning.name.clone();
    // Hamburger reads as "expanded" when sidebar is open, "collapsed"
    // when closed — text label keeps it accessible without an icon
    // font. Only rendered on tabs that actually have a list to hide;
    // other tabs get a zero-width SizedBox so the flex_row tuple
    // type stays stable. Each tab's collapsed state is tracked
    // independently on [`SidebarVisibility`], so flipping the
    // Scales sidebar doesn't also flip Progressions'.
    let current_tab = state.tab;
    let hamburger_label = if state.sidebars.is_collapsed(current_tab) {
        "Show list"
    } else {
        "Hide list"
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
    // Instrument is changed via the ‹/› cycle arrows below (every
    // fretboard re-renders against the new tuning's strings).

    // Fret-span scope dial — shorten the neck toward the chord-card
    // form or open it to the full 12. Only on fretboard tabs.
    let span = state.fret_span;
    let fret_ctl: OneOf2<_, _> = if tab_has_fretboard(current_tab) {
        OneOf2::A(
            flex_row((
                dim_label(state.palette, format!("Frets {span}"), TS_XS),
                button_sm("−", |s: &mut AppState| {
                    s.fret_span = s.fret_span.saturating_sub(1).max(4);
                    s.fret_start = s.fret_start.min(s.max_fret_start());
                }),
                button_sm("+", |s: &mut AppState| {
                    s.fret_span = (s.fret_span + 1).min(12);
                    s.fret_start = s.fret_start.min(s.max_fret_start());
                }),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .gap(SP_1),
        )
    } else {
        OneOf2::B(sized_box(label("")).fixed_width(SP_0))
    };
    // "Add to rehearsal" — captures the active lens's material as a Card
    // and pushes it onto the rehearsal queue (redesign R1). Only on lens
    // tabs; the count badge shows the queue depth so the capture is
    // visible without leaving the lens.
    let queue_len = state.set.cards.len();
    let rehearse_ctl: OneOf2<_, _> = if tab_has_fretboard(current_tab) {
        let rehearse_label = if queue_len > 0 {
            format!("+ Rehearse ({queue_len})")
        } else {
            "+ Rehearse".to_string()
        };
        OneOf2::A(text_button(rehearse_label, |s: &mut AppState| {
            s.rehearse_current()
        }))
    } else {
        OneOf2::B(sized_box(label("")).fixed_width(SP_0))
    };
    // Client-side window chrome (native decorations are off — see `run`).
    // Right cluster of the header: a transparent draggable filler that
    // moves the window, then the Rehearse control, then the
    // minimize / maximize / close glyphs. Grouped into one flex child so
    // the outer header tuple stays small. Glyphs use `palette.text` so they
    // retheme with everything else; close flips `running` to quit.
    let chrome_fg = state.palette.text;
    let right_group = flex_row((
        window_chrome(ChromeRole::Drag, chrome_fg).flex(1.0),
        rehearse_ctl,
        window_chrome(ChromeRole::Minimize, chrome_fg),
        window_chrome(ChromeRole::Maximize, chrome_fg),
        window_chrome(ChromeRole::Close, chrome_fg)
            .on_close(|s: &mut AppState| s.running = false),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .gap(SP_1);

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
            button_sm("‹", |s: &mut AppState| s.cycle_instrument(-1)),
            label(instrument_name).text_size(TS_SM),
            button_sm("›", |s: &mut AppState| s.cycle_instrument(1)),
            dim_label(state.palette, "·", TS_SM),
            button_sm("‹", |s: &mut AppState| s.cycle_tuning(-1)),
            label(tuning_name).text_size(TS_SM),
            button_sm("›", |s: &mut AppState| s.cycle_tuning(1)),
            fret_ctl,
            right_group.flex(1.0),
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
    let palette = state.palette;
    // Active tab: tertiary "you-are-here" color + a bracket cue (so it
    // reads without relying on color alone); inactive tabs take the
    // header text color. Built with `button` so the label color is ours.
    let tab_button = move |text: String, is_active: bool, on_click: fn(&mut AppState)| {
        let (txt, color) = if is_active {
            (format!("[{text}]"), palette.tertiary)
        } else {
            (text, palette.text_header)
        };
        button(label(txt).text_size(TS_SM).color(color), on_click).into_any_flex()
    };

    let mut buttons: Vec<AnyFlexChild<AppState>> = Vec::new();
    // The five fretboard lenses collapse into one "Stage" destination —
    // the instrument stage is the center of gravity; the material-kind
    // selector below picks what's on it (it's card selection, not five
    // separate pages).
    buttons.push(tab_button(
        "Stage".to_string(),
        tab_has_fretboard(active),
        |s: &mut AppState| {
            if !tab_has_fretboard(s.tab) {
                s.tab = s.last_lens;
            }
        },
    ));
    // Tuner + Metronome are no longer top-level destinations — they're
    // surface modules (mount via the "Show:" toggles), and the header
    // tuning combobox covers tuning selection. Remaining destinations:
    buttons.push(tab_button("Practice".to_string(), active == Tab::Practice, |s| {
        s.tab = Tab::Practice
    }));
    buttons.push(tab_button("Song".to_string(), active == Tab::Song, |s| s.tab = Tab::Song));
    buttons.push(tab_button("Rehearsal".to_string(), active == Tab::Rehearsal, |s| {
        s.tab = Tab::Rehearsal
    }));
    buttons.push(tab_button("Settings".to_string(), active == Tab::Settings, |s| {
        s.tab = Tab::Settings
    }));
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

/// Lens switcher — sub-navigation shown only on the Fretboard surface.
/// Picks which lens (Scale / Chord / Progression / Exercise) the one
/// fretboard surface shows; each is a theory tab under the hood, so
/// the shared root + tuning carry across. Zero-height elsewhere.
fn lens_bar(
    state: &mut AppState,
) -> OneOf2<impl WidgetView<AppState> + use<>, impl WidgetView<AppState> + use<>> {
    if !tab_has_fretboard(state.tab) {
        return OneOf2::B(sized_box(label("")).fixed_height(SP_0));
    }
    let active = state.tab;
    let palette = state.palette;
    // Each entry picks what *material kind* the stage shows. Reads as
    // card selection (which kind of card is on the stage), not a page
    // switch — the surface, root, and tuning carry across unchanged.
    let lens = move |text: &str, tab: Tab| {
        // Active kind pops in tertiary with a ● cue; inactive kinds
        // take body `text` (legible — not the disabled-looking dim).
        let (txt, color) = if tab == active {
            (format!("● {text}"), palette.tertiary)
        } else {
            (format!("  {text}"), palette.text)
        };
        button(label(txt).text_size(TS_XS).color(color), move |s: &mut AppState| {
            s.tab = tab;
            s.last_lens = tab;
        })
        .into_any_flex()
    };
    // Companion-module mount toggles. Filled (●) when shown, hollow (○)
    // when not; clicking mounts/unmounts the module in the surface stack.
    let module_toggle = move |text: &str, kind: ModuleKind, shown: bool| {
        let (txt, color) = if shown {
            (format!("● {text}"), palette.tertiary)
        } else {
            (format!("○ {text}"), palette.text)
        };
        button(label(txt).text_size(TS_XS).color(color), move |s: &mut AppState| {
            s.toggle_module(kind);
        })
        .into_any_flex()
    };
    let tuner_shown = state.module_shown(ModuleKind::Tuner);
    let metro_shown = state.module_shown(ModuleKind::Metronome);
    // "Now rehearsing" strip — when the queue holds cards, the stage
    // shows which one the cursor is on and steps through them with
    // ‹/›, so the rehearsal queue drives the stage as a live practice
    // flow (not just a list you Load from). Right-aligned, only when
    // the queue is non-empty.
    // Compact on purpose: just the cursor counter + ‹/›. The card's
    // name/material is already shown prominently on the stage, so
    // repeating it here only makes this strip overflow narrow windows.
    let queue_len = state.set.cards.len();
    let rehearse_strip: OneOf2<_, _> = if queue_len > 0 {
        let cursor = state.set.cursor;
        OneOf2::A(
            flex_row((
                button_sm("‹", |s: &mut AppState| s.rehearse_step(-1)),
                dim_label(palette, format!("{}/{queue_len}", cursor + 1), TS_XS),
                button_sm("›", |s: &mut AppState| s.rehearse_step(1)),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .gap(SP_1),
        )
    } else {
        OneOf2::B(sized_box(label("")).fixed_width(SP_0))
    };
    OneOf2::A(
        sized_box(
            flex_row((
                dim_label(palette, "Material:", TS_XS),
                lens("Scale", Tab::Scales),
                lens("Chord", Tab::Chords),
                lens("Arpeggio", Tab::Arpeggios),
                lens("Progression", Tab::Progressions),
                lens("Exercise", Tab::Exercises),
                // Visual break, then the surface-module mount toggles.
                dim_label(palette, "   Show:", TS_XS),
                module_toggle("Tuner", ModuleKind::Tuner, tuner_shown),
                module_toggle("Metronome", ModuleKind::Metronome, metro_shown),
                // Rehearsal cursor, pushed to the right edge.
                FlexSpacer::Flex(1.0),
                rehearse_strip,
            ))
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .main_axis_alignment(MainAxisAlignment::Start)
            .gap(SP_2),
        )
        // Left inset so "Material:" aligns with the content below instead
        // of hugging the window edge.
        .padding(masonry::properties::Padding::from_vh(SP_0, SP_2)),
    )
}

fn tab_content(state: &mut AppState) -> Box<AnyWidgetView<AppState>> {
    // Boxed dispatch (not `OneOf9`) so the tab count isn't capped at
    // xilem's 9-arm `OneOf` — the fretboard lenses alone now number
    // five. Fretboard lens views fill the bounded height and scroll
    // per-module (via `surface_left`); the other destinations are
    // ordinary tall forms wrapped in a vertical scroll portal
    // (`scroll_tab`) to fit the bounded tab-content area.
    match state.tab {
        Tab::Scales => scales_view(state).boxed(),
        Tab::Chords => chords_view(state).boxed(),
        Tab::Tuner => scroll_tab(tuner_view(state)).boxed(),
        Tab::Progressions => progressions_view(state).boxed(),
        Tab::Exercises => exercises_view(state).boxed(),
        Tab::Arpeggios => arpeggios_view(state).boxed(),
        Tab::Metronome => scroll_tab(metronome_view(state)).boxed(),
        Tab::Practice => scroll_tab(practice_view(state)).boxed(),
        Tab::Song => scroll_tab(song_view_render(state)).boxed(),
        // Not scroll_tab: the set stage fills the bounded height (the neck
        // flexes), like the fretboard lenses.
        Tab::Rehearsal => rehearsal_view(state).boxed(),
        Tab::Settings => scroll_tab(settings_view(state)).boxed(),
    }
}

/// Wrap a tab body in a vertical scroll viewport sized to the bounded
/// tab-content area. `constrain_horizontal` keeps content at the
/// viewport width (only vertical scroll); the auto-hide scrollbar
/// stays out of the way until needed.
fn scroll_tab<V>(view: V) -> impl WidgetView<AppState> + use<V>
where
    V: WidgetView<AppState>,
{
    // Wrap in a concrete single-child `flex_col` so the portal's child
    // widget type is the (Sized) `Flex` widget — `.prop()` requires a
    // Sized child, which an opaque `impl WidgetView` can't prove. The
    // wrapper also lets us stretch the child to the viewport width.
    // `AutoHideScrollBar(true)` keeps the scrollbar hidden until the
    // pointer moves over the pane (no ever-present bars).
    portal(
        flex_col((view,))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .main_axis_alignment(MainAxisAlignment::Start),
    )
    .constrain_horizontal(true)
    .prop(masonry::properties::AutoHideScrollBar(true))
}

/// The rehearsal queue projection (redesign R2). Lists the cards the user
/// has collected from the lenses, in order, with a cursor marking the one
/// last loaded onto the stage. Each row can be loaded (› — applies the
/// card's selection and jumps to its lens), reordered (▲/▼), or removed
/// (×). This is the first non-lens projection of the card vocabulary; it
/// will grow into the practice-flow backbone (queue → stage stepping).
fn rehearsal_view(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    let palette = state.palette;
    let len = state.set.cards.len();

    let title = flex_col((
        header_label(palette, "Rehearsal", TS_LG),
        dim_label(
            palette,
            "Your set: cards played in sequence. Click a card in the lane to put \
             it on the neck; ‹ › scrub; the row below edits the current card.",
            TS_XS,
        ),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(SP_1);

    // Empty set vs the stage+timeline — different view types, OneOf2.
    let body: OneOf2<_, _> = if len == 0 {
        OneOf2::A(card(
            palette,
            flex_col((
                label("No cards yet.").text_size(TS_MD).color(palette.text),
                dim_label(
                    palette,
                    "Press “+ Rehearse” on any lens, or “Rehearse this set / \
                     this song” on the Practice / Song tabs, to fill your set.",
                    TS_SM,
                ),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .gap(SP_2),
        ))
    } else {
        use masonry::layout::Length as MLen;
        let cursor = state.set.cursor.min(len - 1);
        let card_now = state.set.cards[cursor].clone();
        let render = state.resolve_card_for_stage(&card_now);
        let clock_label = match state.card_clock(&card_now) {
            Clock::Song => "song",
            Clock::Metronome => "metronome",
            Clock::Manual => "manual",
        };

        // Caption: current card + an unresolved-material warning.
        let warn: OneOf2<_, _> = match render.warning.clone() {
            Some(w) => OneOf2::A(danger_label(palette, format!("⚠ {w}"), TS_XS)),
            None => OneOf2::B(sized_box(label("")).fixed_height(SP_0)),
        };
        let caption = flex_col((
            header_label(palette, card_now.label.clone(), TS_MD),
            dim_label(
                palette,
                format!(
                    "{} · card {}/{} · clock: {}",
                    card_now.material.tag(),
                    cursor + 1,
                    len,
                    clock_label
                ),
                TS_XS,
            ),
            warn,
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(SP_0);

        // Stage: the neck, rendering the cursor card (uses the card's
        // pinned window, else the live one).
        let (start, span) = card_now
            .setting
            .fret_window
            .map(|w| (w.start, w.span))
            .unwrap_or((state.fret_start, state.fret_span));
        let neck = thin_card(
            palette,
            fretboard_view(
                state.fretboard.clone(),
                render.positions,
                render.labels,
                state.diagram_colors(),
                None,
                (start, span),
                Vec::new(),
            ),
        );

        // Timeline lane: a horizontal stream of card chips; click to put
        // one on the neck. The cursor card pops in tertiary with a ›.
        let mut chips: Vec<AnyFlexChild<AppState>> = Vec::new();
        for (i, c) in state.set.cards.iter().enumerate() {
            let is_cur = i == cursor;
            let col = if is_cur { palette.tertiary } else { palette.text };
            let prefix = if is_cur { "› " } else { "" };
            chips.push(
                button(
                    label(format!("{prefix}{}", c.label)).text_size(TS_XS).color(col),
                    move |s: &mut AppState| s.set_cursor(i),
                )
                .into_any_flex(),
            );
        }
        let lane = sized_box(
            portal(
                flex_row(chips)
                    .cross_axis_alignment(CrossAxisAlignment::Center)
                    .gap(SP_1),
            )
            .constrain_vertical(true)
            .prop(masonry::properties::AutoHideScrollBar(true)),
        )
        .fixed_height(MLen::px(44.0));

        // Inspector + transport for the current card.
        let loop_on = state.set.loop_mode == LoopMode::All;
        let touch_ctl: OneOf2<_, _> = if matches!(card_now.material, Material::Chord { .. }) {
            let t = match card_now.touch {
                Touch::Block => "Touch: block",
                Touch::Arpeggiate { .. } => "Touch: arp",
            };
            OneOf2::A(button_sm(t, move |s: &mut AppState| s.cycle_card_touch(cursor)))
        } else {
            OneOf2::B(sized_box(label("")).fixed_width(SP_0))
        };
        let play_ctl: OneOf2<_, _> = if state.set_playing {
            OneOf2::A(text_button("■ Stop", |s: &mut AppState| s.stop_set_playback()))
        } else {
            OneOf2::B(text_button("› Play", |s: &mut AppState| s.start_set_playback()))
        };
        let sound_on = state.transport_sound;
        let transport = flex_row((
            play_ctl,
            button_sm("‹", |s: &mut AppState| s.cursor_step(-1)),
            button_sm("›", |s: &mut AppState| s.cursor_step(1)),
            button_sm(if sound_on { "Sound" } else { "Muted" }, |s: &mut AppState| {
                s.transport_sound = !s.transport_sound;
            }),
            dim_label(palette, "·", TS_XS),
            touch_ctl,
            button_sm("‹ move", move |s: &mut AppState| s.set.move_card(cursor, -1)),
            button_sm("move ›", move |s: &mut AppState| s.set.move_card(cursor, 1)),
            button_sm("Duplicate", move |s: &mut AppState| s.set.duplicate(cursor)),
            text_button("Edit on lens", move |s: &mut AppState| s.load_card(cursor)),
            button_sm("× Remove", move |s: &mut AppState| s.set.remove(cursor)),
            FlexSpacer::Flex(1.0),
            text_button(if loop_on { "Loop: on" } else { "Loop: off" }, |s: &mut AppState| {
                s.set.loop_mode = if s.set.loop_mode == LoopMode::All {
                    LoopMode::Off
                } else {
                    LoopMode::All
                };
            }),
            text_button("Clear all", |s: &mut AppState| {
                s.set.cards.clear();
                s.set.cursor = 0;
            }),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_2);

        OneOf2::B(
            flex_col((caption, neck.flex(1.0), lane, transport))
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .main_axis_alignment(MainAxisAlignment::Start)
                .gap(SP_2),
        )
    };

    flex_col((title.flex(0.0), body.flex(1.0)))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_3)
}

/// Frets a single arpeggio position/shape spans on screen
/// (`start_fret ..= start_fret + ARP_SHAPE_SPAN`).
const ARP_SHAPE_SPAN: u8 = 4;

/// One CAGED-style arpeggio shape: the chord tones inside a ~5-fret
/// neck window under one hand position.
struct ArpeggioShape {
    start_fret: u8,
    positions: Vec<Position>,
}

/// Generate the arpeggio's neck-position shapes (A2 + bass-anchored
/// inversions). Anchor a window a fret below each place the **bass tone**
/// (the inversion's chord-tone interval from root) lands on the two
/// lowest strings, collect the chord tones inside it, and keep windows
/// that form a usable box. For root position `bass` is the unison; for
/// inversions it's the 3rd / 5th / 7th — so each inversion yields its
/// own set of shapes whose lowest note is its bass, giving full-length
/// runs rather than a truncated rotation. Falls back to a whole-neck
/// shape if nothing qualifies.
fn generate_arpeggio_shapes(
    fretboard: &Fretboard,
    formula: &ChordFormula,
    root: Pitch,
    bass: woodshedding::interval::Interval,
) -> Vec<ArpeggioShape> {
    let all = fretboard.positions_for_chord(formula, root).unwrap_or_default();
    // Anchor frets: a fret below each occurrence of the bass tone on the
    // lowest two strings, within a playable stretch of neck.
    let mut anchors: Vec<u8> = all
        .iter()
        .filter(|p| {
            p.interval_from_root == Some(bass) && p.string_index <= 1 && p.fret <= 15
        })
        .map(|p| p.fret.saturating_sub(1))
        .collect();
    anchors.sort_unstable();
    anchors.dedup();
    if anchors.is_empty() {
        // Bass tone isn't on the low strings in range — fall back to any
        // low occurrence so the lens still shows something.
        anchors = all
            .iter()
            .filter(|p| p.interval_from_root == Some(bass) && p.fret <= 15)
            .map(|p| p.fret.saturating_sub(1))
            .collect();
        anchors.sort_unstable();
        anchors.dedup();
    }
    if anchors.is_empty() {
        anchors.push(0);
    }

    // A complete box wants at least min(chord-tones, 3) distinct pitch
    // classes so a near-empty window doesn't masquerade as a shape.
    let want_pcs = formula.intervals.len().min(3).max(1);
    let mut shapes: Vec<ArpeggioShape> = Vec::new();
    for lo in anchors {
        let hi = lo + ARP_SHAPE_SPAN;
        let positions: Vec<Position> = all
            .iter()
            .filter(|p| p.fret >= lo && p.fret <= hi)
            .cloned()
            .collect();
        let distinct_pcs = {
            let mut pcs: Vec<u8> = positions.iter().map(|p| p.pitch.pitch_class()).collect();
            pcs.sort_unstable();
            pcs.dedup();
            pcs.len()
        };
        if positions.len() >= 4 && distinct_pcs >= want_pcs {
            shapes.push(ArpeggioShape {
                start_fret: lo,
                positions,
            });
        }
    }
    if shapes.is_empty() {
        shapes.push(ArpeggioShape {
            start_fret: 0,
            positions: all,
        });
    }
    shapes
}

/// Arpeggio lens — an arpeggio's notes are a chord's tones, so the
/// quality comes from the chord catalog and the shared `root`. Renders
/// the active CAGED-style **position shape** on the surface neck with an
/// up/down/loop step-through transport, plus a grid of shape cards (one
/// per neck position) on the right; clicking a card loads that shape.
/// Tempo follows the metronome.
fn arpeggios_view(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    let cat = chord_catalog();
    let idx = state.arpeggio_idx.min(cat.len().saturating_sub(1));
    let formula = cat[idx];
    let root = state.root.to_pitch(4);
    let intervals: String = formula
        .intervals
        .iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let symbol = if formula.symbol.is_empty() {
        String::new()
    } else {
        formula.symbol.to_string()
    };
    let display_root = state.root.display();
    let arp_label = format!("{display_root}{symbol} arpeggio ({})", formula.name);

    let board = state.fretboard.clone();
    let unison = woodshedding::interval::Interval::PERFECT_UNISON;

    // The inversion picks the bass chord tone; shapes are anchored to it
    // (bass-anchored inversions) so each inversion has its own positions
    // and a full-length ascending run.
    let inv = (state.arpeggio_inversion as usize).min(formula.intervals.len().saturating_sub(1));
    let bass = formula.intervals.get(inv).copied().unwrap_or(unison);

    // Generate the position shapes for this bass; the active one drives
    // the surface neck (its window is the display window) and the transport.
    let shapes = generate_arpeggio_shapes(&state.fretboard, &formula, root, bass);
    let shape_count = shapes.len();
    let pos_idx = state.arpeggio_position_idx.min(shape_count.saturating_sub(1));
    let active_start = shapes.get(pos_idx).map(|s| s.start_fret).unwrap_or(0);
    let positions: Vec<Position> = shapes
        .get(pos_idx)
        .map(|s| s.positions.clone())
        .unwrap_or_default();

    // === Transport sequence ===
    // The arpeggio run = the active (bass-anchored) shape's notes ordered
    // by pitch, ascending from the bass tone. `seq` holds indices into
    // `positions`. Since the shape is anchored to the bass, dropping the
    // few notes below the bass's lowest occurrence still leaves a
    // full-length run (unlike the old root-anchored rotation).
    let mut seq: Vec<usize> = (0..positions.len()).collect();
    seq.sort_by_key(|&i| (positions[i].pitch.midi(), positions[i].string_index));
    let inv_start = seq
        .iter()
        .position(|&i| positions[i].interval_from_root == Some(bass))
        .unwrap_or(0);
    if inv_start > 0 && inv_start < seq.len() {
        seq.drain(0..inv_start);
    }
    let n = seq.len();
    let inv_text = match inv {
        0 => "Inv: Root".to_string(),
        1 => "Inv: 1st".to_string(),
        2 => "Inv: 2nd".to_string(),
        3 => "Inv: 3rd".to_string(),
        k => format!("Inv: {k}th"),
    };

    // Walk order over `ordered` per direction. UpDown ping-pongs without
    // repeating the turnaround notes.
    let walk: Vec<usize> = match state.arpeggio_direction {
        _ if n == 0 => Vec::new(),
        ArpeggioDirection::Up => (0..n).collect(),
        ArpeggioDirection::Down => (0..n).rev().collect(),
        ArpeggioDirection::UpDown => {
            let mut v: Vec<usize> = (0..n).collect();
            if n > 1 {
                v.extend((1..n - 1).rev());
            }
            v
        }
    };
    let walk_len = walk.len().max(1);
    // Cursor: when the metronome is running it's the clock (phase-locked
    // to the click via the shared beat grid, 3d); otherwise the arpeggio's
    // own Play/Step drives `arpeggio_step_idx`.
    let metro_driving = state.metronome_beat().is_some();
    let cursor = match state.metronome_beat() {
        Some(beat) => beat as usize,
        None => state.arpeggio_step_idx,
    };
    let cur_rank = if walk.is_empty() {
        None
    } else {
        Some(walk[cursor % walk_len])
    };
    let cur_pos = cur_rank.map(|r| seq[r]);
    // Frequencies in walk order — for the step-through audio task.
    let walk_freqs: Vec<f32> = walk
        .iter()
        .map(|&r| positions[seq[r]].pitch.frequency() as f32)
        .collect();

    // Dot colors: the current step pops in `secondary` — a third triad
    // hue distinct from BOTH resting colors (root dots follow `tertiary`,
    // note dots follow `primary`), so the cursor is visible even when it
    // lands on a root (which would otherwise already be tertiary-colored).
    let dc = state.diagram_colors();
    let dot_colors: Vec<Color> = positions
        .iter()
        .enumerate()
        .map(|(i, p)| {
            if Some(i) == cur_pos {
                state.palette.secondary
            } else if p.interval_from_root
                == Some(woodshedding::interval::Interval::PERFECT_UNISON)
            {
                dc.root_dot
            } else {
                dc.note_dot
            }
        })
        .collect();

    // Labels per the chosen mode. `Steps` numbers the dots in ascending
    // arpeggio order; `Blank` draws none.
    let labels: Vec<String> = match state.arpeggio_label {
        ArpeggioLabel::Notes => compute_labels(LabelMode::Notes, &positions),
        ArpeggioLabel::Intervals => compute_labels(LabelMode::Intervals, &positions),
        ArpeggioLabel::Blank => Vec::new(),
        ArpeggioLabel::Steps => {
            let mut v = vec![String::new(); positions.len()];
            for (rank, &pi) in seq.iter().enumerate() {
                v[pi] = format!("{}", rank + 1);
            }
            v
        }
    };

    let playing = state.arpeggio_playing;
    let step_indicator = match cur_rank {
        Some(r) => format!("Note {}/{}", r + 1, n),
        None => "—".to_string(),
    };
    let bpm_for_arp = state.bpm;

    // Spelled arpeggio notes (root emphasized) — same shape as the
    // Scale lens's Degrees panel.
    let arp_pitches = formula.apply_to(root).unwrap_or_default();
    let note_rows: Vec<_> = arp_pitches
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let is_root = i == 0;
            let note = format!("{}{}", p.name, p.accidental);
            let note_color = if is_root {
                state.palette.tertiary
            } else {
                state.palette.text
            };
            flex_row((
                sized_box(
                    label(format!("{}", i + 1))
                        .text_size(TS_XS)
                        .color(state.palette.text_dim),
                )
                .fixed_width(masonry::layout::Length::px(22.0)),
                label(note).text_size(TS_SM).color(note_color),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .main_axis_alignment(MainAxisAlignment::Start)
            .gap(SP_2)
        })
        .collect();
    let notes_section = flex_col((
        dim_label(state.palette, "Notes", TS_XS),
        flex_col(note_rows)
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_alignment(MainAxisAlignment::Start)
            .gap(SP_1),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .main_axis_alignment(MainAxisAlignment::Start)
    .gap(SP_1);

    // Position/shape cards — one mini neck per generated shape; click to
    // load it (also resets the transport cursor). Active card's caption
    // pops in `tertiary`.
    let shape_cards: Vec<_> = shapes
        .iter()
        .enumerate()
        .map(|(i, sh)| {
            let active = i == pos_idx;
            let sp = sh.positions.clone();
            let cdots: Vec<Color> = sp
                .iter()
                .map(|p| {
                    if p.interval_from_root == Some(unison) {
                        dc.root_dot
                    } else {
                        dc.note_dot
                    }
                })
                .collect();
            let start = sh.start_fret;
            let caption = if start == 0 {
                format!("Pos {} · open", i + 1)
            } else {
                format!("Pos {} · fret {}", i + 1, start + 1)
            };
            let cap_color = if active {
                state.palette.tertiary
            } else {
                state.palette.text
            };
            flex_col((
                label(caption).text_size(TS_XS).color(cap_color),
                button(
                    sized_box(fretboard_view(
                        board.clone(),
                        sp,
                        Vec::new(),
                        dc,
                        Some(cdots),
                        (start, ARP_SHAPE_SPAN + 1),
                        Vec::new(),
                    ))
                    .fixed_width(masonry::layout::Length::px(150.0))
                    .fixed_height(masonry::layout::Length::px(180.0)),
                    move |s: &mut AppState| {
                        s.arpeggio_position_idx = i;
                        s.arpeggio_step_idx = 0;
                    },
                )
                .padding(masonry::properties::Padding::from_vh(SP_1, SP_1)),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_alignment(MainAxisAlignment::Start)
            .gap(SP_1)
        })
        .collect();
    // Reflow the shape cards into a grid: rows of `cards_per_row` from
    // the observed pane width (capped at 4), mirroring the Progression
    // chord-card grid. `arpeggio_cards_panel_width` is reported by the
    // tracker added to `info_panel` (which is cross-Stretch so it gets
    // the full pane width).
    const ARP_CARD_W: f64 = 162.0;
    let arp_panel_w = state.arpeggio_cards_panel_width;
    let cards_per_row = if arp_panel_w < 1.0 {
        2
    } else {
        (((arp_panel_w + 8.0) / (ARP_CARD_W + 8.0)).floor() as usize).clamp(1, 4)
    };
    let mut card_rows: Vec<_> = Vec::new();
    let mut card_buf: Vec<_> = Vec::new();
    for c in shape_cards {
        card_buf.push(c);
        if card_buf.len() == cards_per_row {
            card_rows.push(
                flex_row(std::mem::take(&mut card_buf))
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_alignment(MainAxisAlignment::Start)
                    .gap(SP_2),
            );
        }
    }
    if !card_buf.is_empty() {
        card_rows.push(
            flex_row(card_buf)
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_alignment(MainAxisAlignment::Start)
                .gap(SP_2),
        );
    }
    let cards_width_tracker = resize_observer(
        |s: &mut AppState, size: masonry::kurbo::Size| {
            s.arpeggio_cards_panel_width = size.width;
        },
        sized_box(label("")).fixed_height(masonry::layout::Length::const_px(0.0)),
    );
    let positions_section = flex_col((
        dim_label(state.palette, format!("Positions ({shape_count})"), TS_XS),
        flex_col(card_rows)
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_alignment(MainAxisAlignment::Start)
            .gap(SP_2),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .main_axis_alignment(MainAxisAlignment::Start)
    .gap(SP_1);

    let quality_options: Vec<ArcStr> = cat.iter().map(|f| ArcStr::from(f.name)).collect();
    let root_options: Vec<ArcStr> = ChromaticPc::ALL
        .iter()
        .map(|pc| ArcStr::from(pc.display()))
        .collect();
    let root_selected = ChromaticPc::ALL
        .iter()
        .position(|&pc| pc == state.root)
        .unwrap_or(0);
    let open_combo = state.open_combobox;

    let info_panel = flex_col((
        // Invisible full-width strut: cross-Stretch gives it the pane
        // width, which `resize_observer` reports to drive the shape-card
        // reflow grid below.
        cards_width_tracker,
        header_label(state.palette, arp_label, TS_LG),
        dim_prose(state.palette, format!("Intervals: {intervals}"), TS_SM),
        // Transport — walks the notes up/down/loop in time. Tempo follows
        // the metronome (state.bpm), so there's no separate tempo control
        // here; mount the Metronome widget to set the pace.
        flex_row((
            if playing {
                OneOf2::A(button_sm("■ Stop", |s: &mut AppState| {
                    s.arpeggio_playing = false;
                }))
            } else {
                OneOf2::B(button_sm("› Play", |s: &mut AppState| {
                    s.arpeggio_playing = true;
                }))
            },
            button_sm("‹ Step", |s: &mut AppState| {
                s.arpeggio_playing = false;
                s.arpeggio_step_idx = s.arpeggio_step_idx.saturating_sub(1);
            }),
            button_sm("Step ›", |s: &mut AppState| {
                s.arpeggio_playing = false;
                s.arpeggio_step_idx = s.arpeggio_step_idx.wrapping_add(1);
            }),
            button_sm("‹‹", |s: &mut AppState| s.arpeggio_step_idx = 0),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_1),
        flex_row((
            button_sm(state.arpeggio_direction.label(), |s: &mut AppState| {
                s.arpeggio_direction = s.arpeggio_direction.next();
            }),
            button_sm(inv_text, |s: &mut AppState| {
                let n = chord_catalog()
                    .get(s.arpeggio_idx)
                    .map(|f| f.intervals.len())
                    .unwrap_or(1)
                    .max(1);
                s.arpeggio_inversion = ((s.arpeggio_inversion as usize + 1) % n) as u8;
                // New inversion → new (bass-anchored) shapes; reset both
                // the active shape and the transport cursor.
                s.arpeggio_position_idx = 0;
                s.arpeggio_step_idx = 0;
            }),
            button_sm(state.arpeggio_label.label(), |s: &mut AppState| {
                s.arpeggio_label = s.arpeggio_label.next();
            }),
            button_sm(
                if state.transport_sound { "Sound" } else { "Muted" },
                |s: &mut AppState| s.transport_sound = !s.transport_sound,
            ),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_1),
        dim_label(
            state.palette,
            if metro_driving {
                format!("{step_indicator} · synced to metronome ({bpm_for_arp:.0})")
            } else {
                format!("{step_indicator} · tempo {bpm_for_arp:.0} · run Metronome to sync")
            },
            TS_XS,
        ),
        flex_row((
            combobox(
                "arpeggios.quality",
                "Arpeggio: ",
                &quality_options,
                idx,
                open_combo,
                |s: &mut AppState, i: usize| s.arpeggio_idx = i,
            ),
            button_sm("‹", |s: &mut AppState| {
                let n = chord_catalog().len().max(1);
                s.arpeggio_idx = (s.arpeggio_idx + n - 1) % n;
            }),
            button_sm("›", |s: &mut AppState| {
                let n = chord_catalog().len().max(1);
                s.arpeggio_idx = (s.arpeggio_idx + 1) % n;
            }),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_2),
        flex_row((
            combobox(
                "arpeggios.root",
                "Root: ",
                &root_options,
                root_selected,
                open_combo,
                |s: &mut AppState, i: usize| {
                    s.root = ChromaticPc::ALL[i];
                },
            ),
            button_sm("‹", |s: &mut AppState| s.root = s.root.cycle(-1)),
            button_sm("›", |s: &mut AppState| s.root = s.root.cycle(1)),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_2),
        notes_section,
        positions_section,
    ))
    // Stretch so the width tracker spans the pane (drives the card grid);
    // each row's own `main_axis: Start` keeps its content left-anchored.
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .main_axis_alignment(MainAxisAlignment::Start)
    .gap(SP_2);

    // Quality catalog sidebar — reuses the chord catalog (each chord
    // quality is an arpeggio). Mirrors the Chords/Scales browse list.
    let list_items: Vec<_> = cat
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let active = idx == i;
            list_item_button(state.palette, active, f.name, move |s: &mut AppState| {
                s.arpeggio_idx = i
            })
        })
        .collect();
    let list_card = nav_card(
        state.palette,
        flex_col((
            header_label(state.palette, "Arpeggios", TS_MD),
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
    let sidebar: OneOf2<_, _> = if state.sidebars.is_collapsed(Tab::Arpeggios) {
        OneOf2::A(sized_box(label("")).fixed_width(SP_0))
    } else {
        OneOf2::B(sized_box(list_card).fixed_width(masonry::layout::Length::px(220.0)))
    };

    use masonry::layout::Length as MLen;
    // The active shape sets the display window — the position cards drive
    // it, so no start-fret arrows here; wrap in a plain thin card.
    let fretboard_card = thin_card(
        state.palette,
        fretboard_view(
            board,
            positions,
            labels,
            state.diagram_colors(),
            Some(dot_colors),
            (active_start, ARP_SHAPE_SPAN + 1),
            Vec::new(),
        ),
    )
    .boxed();
    let surface = surface_left(state, fretboard_card);
    let body = flex_row((
        sidebar,
        pane_split(surface, scroll_tab(card(state.palette, info_panel)))
            .split_point(state.split_ratio)
            .bar_color(state.palette.surface_hover)
            .on_split_changed(|s: &mut AppState, f: f64| s.split_ratio = f)
            .min_lengths(MLen::const_px(240.0), MLen::const_px(240.0))
            .flex(1.0),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .main_axis_alignment(MainAxisAlignment::Start)
    .gap(SP_4);

    // Auto-advance timer at the metronome tempo (exercise-style). True
    // beat-phase lock to the metronome's audible click is Phase 3d; for
    // now the step runs on its own interval at the shared `bpm`.
    let interval_ms = (60_000.0 / bpm_for_arp.max(1.0)) as u64;
    // Own timer only when the metronome isn't driving (otherwise the
    // shared beat grid + app-level heartbeat advance the cursor).
    let auto_task = (playing && !metro_driving && walk_len > 0).then(|| {
        task_raw(
            move |proxy, _| async move {
                let mut tick = time::interval(Duration::from_millis(interval_ms.max(50)));
                tick.tick().await;
                loop {
                    tick.tick().await;
                    if proxy.message(()).is_err() {
                        break;
                    }
                }
            },
            move |s: &mut AppState, _: ()| {
                s.arpeggio_step_idx = s.arpeggio_step_idx.wrapping_add(1);
            },
        )
    });

    // Step-through audio: poll ~20ms while the run is active and sound a
    // note (chord-render voice) whenever the cursor lands on a new step —
    // works for both the own-timer and metronome-driven cases (reads the
    // same effective cursor).
    let audio_active =
        state.transport_sound && (playing || metro_driving) && !walk_freqs.is_empty();
    let audio_task = audio_active.then(|| {
        task_raw(
            move |proxy, _| async move {
                let mut tick = time::interval(Duration::from_millis(20));
                loop {
                    tick.tick().await;
                    if proxy.message(()).is_err() {
                        break;
                    }
                }
            },
            move |s: &mut AppState, _: ()| {
                let cursor = s
                    .metronome_beat()
                    .map(|b| b as usize)
                    .unwrap_or(s.arpeggio_step_idx);
                let idx = cursor % walk_freqs.len().max(1);
                if s.arpeggio_last_sounded != Some(idx) {
                    s.arpeggio_last_sounded = Some(idx);
                    let f = walk_freqs[idx];
                    if f > 0.0 {
                        if let Some(h) = s.ensure_song_engine() {
                            h.play_note_now(f, 0.18);
                        }
                    }
                }
            },
        )
    });

    fork(fork(body, auto_task), audio_task)
}

// =================================================================
// Instrument surface — the composable left pane (Phase 3b).
// =================================================================

/// Wrap the fretboard card with whatever companion modules (tuner /
/// metronome) the user has mounted, producing the **left pane** of the
/// main split: a vertical stack of resizable instrument modules sharing
/// the one right edge (the main split bar).
///
/// The fretboard is always present; tuner/metronome stack above/below
/// it per the `surface` order. With only the fretboard mounted this
/// returns the fretboard card unchanged (identical to the pre-3b
/// layout). Multiple modules fold into right-leaning nested vertical
/// `split`s; each divider persists its position into the module size
/// weights via [`AppState::set_module_split`].
///
/// `fretboard_card` is built by the caller (it's lens-specific) and
/// handed in already boxed so this stays lens-agnostic.
fn surface_left(
    state: &mut AppState,
    fretboard_card: Box<AnyWidgetView<AppState>>,
) -> Box<AnyWidgetView<AppState>> {
    use masonry::kurbo::Axis;
    use masonry::layout::Length as MLen;

    // Visible modules in surface order, with their state index + weight.
    let visible: Vec<(usize, ModuleKind, f64)> = state
        .surface
        .iter()
        .enumerate()
        .filter(|(_, m)| m.visible)
        .map(|(i, m)| (i, m.kind, m.weight))
        .collect();

    // Render each to a boxed view. The single fretboard entry consumes
    // the passed-in card; companions render their self-contained views.
    let mut fretboard_card = Some(fretboard_card);
    let mut rendered: Vec<(usize, f64, Box<AnyWidgetView<AppState>>)> = Vec::new();
    for (idx, kind, weight) in visible {
        // Modules are *widgets*, not scrolling sub-pages: each is built
        // to fit its pane. The fretboard scales its drawing to the pane;
        // the tuner/metronome use compact module forms (`*_module`) dense
        // enough to fit without a scrollbar. (The full Tuner/Metronome
        // tabs keep the verbose page layouts.)
        let view: Box<AnyWidgetView<AppState>> = match kind {
            ModuleKind::Fretboard => fretboard_card
                .take()
                .expect("surface holds exactly one Fretboard (sanitize_surface)"),
            ModuleKind::Tuner => tuner_module(state).boxed(),
            ModuleKind::Metronome => metronome_module(state).boxed(),
        };
        rendered.push((idx, weight, view));
    }

    // Fold from the bottom up into nested vertical splits. `running_tail`
    // is the summed weight of everything already folded below the current
    // module, which sets each divider's initial fraction.
    let (_, last_w, mut acc) = rendered.pop().expect("at least the fretboard is visible");
    let mut running_tail = last_w;
    while let Some((idx, w, view)) = rendered.pop() {
        let point = (w / (w + running_tail)).clamp(0.05, 0.95);
        acc = pane_split(view, acc)
            .split_axis(Axis::Vertical)
            .bar_color(state.palette.surface_hover)
            // Floors so a module can't be dragged so small it clips:
            // ~a chord-card-diagram tall for the top pane, enough for a
            // compact widget's rows below.
            .min_lengths(MLen::const_px(190.0), MLen::const_px(170.0))
            .on_split_changed(move |s: &mut AppState, p: f64| s.set_module_split(idx, p))
            .boxed();
        running_tail += w;
    }
    acc
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
    let root = state.root.to_pitch(4);
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

    // Spelled scale notes from the shared root — one row per degree,
    // root emphasized. Fills the info pane with something useful
    // (you read "A B C# D E F# G#" for A Major) instead of dead space.
    let scale_pitches = formula.apply_to(root).unwrap_or_default();
    let degree_rows: Vec<_> = scale_pitches
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let is_root = i == 0;
            let note = format!("{}{}", p.name, p.accidental);
            let note_color = if is_root {
                state.palette.tertiary
            } else {
                state.palette.text
            };
            flex_row((
                sized_box(
                    label(format!("{}", i + 1))
                        .text_size(TS_XS)
                        .color(state.palette.text_dim),
                )
                .fixed_width(masonry::layout::Length::px(22.0)),
                label(note).text_size(TS_SM).color(note_color),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .main_axis_alignment(MainAxisAlignment::Start)
            .gap(SP_2)
        })
        .collect();
    let degrees_section = flex_col((
        dim_label(state.palette, "Degrees", TS_XS),
        flex_col(degree_rows)
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_alignment(MainAxisAlignment::Start)
            .gap(SP_1),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .main_axis_alignment(MainAxisAlignment::Start)
    .gap(SP_1);

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
        .position(|&pc| pc == state.root)
        .unwrap_or(0);
    let open_combo = state.open_combobox;

    // Right-hand info panel: title + intervals + control rows + a
    // bottom-aligned label-mode cycler. Each picker now pairs a
    // combobox (jump to any) with ‹/› arrows (walk to adjacent).
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
            button_sm("‹", |s: &mut AppState| s.cycle_scale(-1)),
            button_sm("›", |s: &mut AppState| s.cycle_scale(1)),
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
                    s.root = ChromaticPc::ALL[i];
                },
            ),
            button_sm("‹", |s: &mut AppState| {
                s.root = s.root.cycle(-1);
            }),
            button_sm("›", |s: &mut AppState| {
                s.root = s.root.cycle(1);
            }),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_2),
        degrees_section,
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
            list_item_button(state.palette, active, f.name, move |s: &mut AppState| {
                s.scale_idx = i
            })
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
    let scale_list_card = nav_card(state.palette,
        flex_col((
            header_label(state.palette, "Scales", TS_MD),
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
    // The fretboard side goes through `surface_left` so any mounted
    // tuner/metronome modules stack with it.
    use masonry::layout::Length as MLen;
    // Fill the surface pane (no fixed height, no scroll) — the canvas
    // scales its drawing to whatever vertical share the pane gives it.
    // The widget wrapper adds the start-fret arrows; the window slides
    // with `fret_start`.
    let fretboard_card = fretboard_widget(
        state,
        fretboard_view(
            board,
            positions,
            labels,
            state.diagram_colors(),
            None,
            (state.fret_start, state.fret_span),
            Vec::new(),
        ),
    )
    .boxed();
    let surface = surface_left(state, fretboard_card);
    flex_row((
        sidebar,
        pane_split(surface, scroll_tab(card(state.palette, info_panel)))
            .split_point(state.split_ratio)
            .bar_color(state.palette.surface_hover)
            .on_split_changed(|s: &mut AppState, f: f64| s.split_ratio = f)
            .min_lengths(MLen::const_px(240.0), MLen::const_px(240.0))
            .flex(1.0),
    ))
    // Cross-axis Stretch so the split fills the bounded tab-content
    // height; the surface modules + info pane each scroll internally
    // (`scroll_tab`) rather than the page growing.
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .main_axis_alignment(MainAxisAlignment::Start)
    .gap(SP_4)
}

/// Chords tab — symmetric to Scales but with a voicing/scale toggle.
/// When `chord_show_voicing` is on, the fretboard shows just the
/// selected playable voicing (5-6 specific positions). When off, it
/// shows every chord tone across the fretboard (the chord-tone scale).
fn chords_view(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    let formula = state.current_chord();
    let root = state.root.to_pitch(4);
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
    let display_root = state.root.display();
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

    // Voicing mode frames the shape: anchor the visible window to the
    // voicing's frets so it can never fall off-screen (the live
    // `fret_start` slide and its ▼▲ arrows are for the "all chord tones"
    // view). Mirrors `resolve_card_for_stage`, but keeps open strings
    // visible — when the shape uses an open string or sits at the low
    // frets we hold the window at the nut; otherwise we slide up to the
    // shape and widen the span to reach its highest fretted note. "All
    // chord tones" keeps the user's manual window unchanged.
    let fret_window = if state.chord_show_voicing && !voicings.is_empty() {
        let max_fret = positions.iter().map(|p| p.fret).max().unwrap_or(0);
        let has_open = positions.iter().any(|p| p.fret == 0);
        let min_fretted = positions.iter().map(|p| p.fret).filter(|&f| f > 0).min();
        let start = match min_fretted {
            Some(m) if !has_open && m > 1 => m - 1,
            _ => 0,
        };
        let span = state.fret_span.max(max_fret.saturating_sub(start)).max(1);
        (start, span)
    } else {
        (state.fret_start, state.fret_span)
    };
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
        .position(|&pc| pc == state.root)
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
            button_sm("‹", |s: &mut AppState| s.cycle_chord(-1)),
            button_sm("›", |s: &mut AppState| s.cycle_chord(1)),
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
                    s.root = ChromaticPc::ALL[i];
                },
            ),
            button_sm("‹", |s: &mut AppState| {
                s.root = s.root.cycle(-1);
            }),
            button_sm("›", |s: &mut AppState| {
                s.root = s.root.cycle(1);
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
            button_sm("‹", move |s: &mut AppState| {
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
            button_sm("›", move |s: &mut AppState| {
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
            list_item_button(state.palette, active, f.name, move |s: &mut AppState| {
                s.chord_idx = i
            })
        })
        .collect();
    let chord_list_card = nav_card(
        state.palette,
        flex_col((
            header_label(state.palette, "Chords", TS_MD),
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
    // Fill the surface pane (no fixed height, no scroll) — the canvas
    // scales its drawing to whatever vertical share the pane gives it.
    // The widget wrapper adds the start-fret arrows; the window slides
    // with `fret_start`.
    let fretboard_card = fretboard_widget(
        state,
        fretboard_view(
            board,
            positions,
            labels,
            state.diagram_colors(),
            None,
            fret_window,
            Vec::new(),
        ),
    )
    .boxed();
    let surface = surface_left(state, fretboard_card);
    flex_row((
        chord_sidebar,
        pane_split(surface, scroll_tab(card(state.palette, info_panel)))
            .split_point(state.split_ratio)
            .bar_color(state.palette.surface_hover)
            .on_split_changed(|s: &mut AppState, f: f64| s.split_ratio = f)
            .min_lengths(MLen::const_px(240.0), MLen::const_px(240.0))
            .flex(1.0),
    ))
    // Cross-axis Stretch so the split fills the bounded tab-content
    // height; surface modules + info pane scroll internally.
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
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
    let rewind_btn = text_button("‹‹ Rewind", |s: &mut AppState| {
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
        OneOf2::B(text_button("› Play", |s: &mut AppState| {
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

    // Recipe action (U4a): project the song's chord bars into cards on
    // the set, then jump to the Rehearsal tab.
    let rehearse_song_btn = text_button("+ Rehearse this song", |s: &mut AppState| {
        s.fill_set_from_song();
        s.tab = Tab::Rehearsal;
    });

    let transport = flex_row((
        rewind_btn,
        play_btn,
        record_btn,
        loop_btn,
        click_btn,
        rec_mode_btn,
        cursor_label,
        FlexSpacer::Flex(1.0),
        rehearse_song_btn,
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
            let prefix = if is_cursor { "› " } else if active { "● " } else { "" };
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
            .unwrap_or((state.root, 4));
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
                    button_sm("‹", |s: &mut AppState| {
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
                    button_sm("›", |s: &mut AppState| {
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
                    button_sm("‹", |s: &mut AppState| {
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
                    button_sm("›", |s: &mut AppState| {
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
        text_button("‹ Move left", |s: &mut AppState| {
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
        text_button("Move right ›", |s: &mut AppState| {
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

    // Fretboard preview of the browsed item via the shared resolver (U5).
    let (positions, labels) = state
        .current_practice_item()
        .map(|item| {
            let instrument = settings::instrument_to_str(state.active_instrument).to_string();
            let card = practice_item_to_card(item, &instrument, bpm, bars, "");
            let render = state.resolve_card_for_stage(&card);
            (render.positions, render.labels)
        })
        .unwrap_or_default();
    let board = state.fretboard.clone();

    let set_options: Vec<ArcStr> = state
        .practice_sets
        .iter()
        .map(|set| ArcStr::from(set.name.clone()))
        .collect();
    let set_selected = state.practice_selected_set.min(set_count.saturating_sub(1).max(0));
    let practice_open_combo = state.open_combobox;

    // The Practice tab is a recipe/browser now (U8): pick a set, set its
    // tempo + bars-per-item, preview the items, and "Rehearse this set" to
    // fill the set on the Rehearsal tab — that's where you play through it.
    // The old inline runner (Play/Stop, auto-advance) was retired; the set
    // stage subsumes it.
    let info_panel = flex_col((
        header_label(state.palette, set_name, TS_LG),
        prose(set_desc).text_size(TS_XS),
        dim_prose(
            state.palette,
            "Practice sets are recipes. Pick one, set the tempo and bars per \
             item, then “Rehearse this set” to fill your set — the Rehearsal \
             tab is where you play through it.",
            TS_XS,
        ),
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
                },
            ),
            button_sm("‹", move |s: &mut AppState| {
                if set_count > 0 {
                    let cur = s.practice_selected_set.min(set_count - 1) as i32;
                    s.practice_selected_set = ((cur - 1).rem_euclid(set_count as i32)) as usize;
                    s.practice_item_idx = 0;
                }
            }),
            button_sm("›", move |s: &mut AppState| {
                if set_count > 0 {
                    let cur = s.practice_selected_set.min(set_count - 1) as i32;
                    s.practice_selected_set = ((cur + 1).rem_euclid(set_count as i32)) as usize;
                    s.practice_item_idx = 0;
                }
            }),
            label(format!("({set_idx} of {set_count})")).text_size(TS_XS),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_2),
        // Recipe action (U2): fill the set from this practice set, then jump
        // to the Rehearsal tab to play it.
        text_button("+ Rehearse this set", |s: &mut AppState| {
            s.fill_set_from_practice();
            s.tab = Tab::Rehearsal;
        }),
        // Tempo + bars-per-item parameterize the recipe (they become each
        // card's `Timing.bpm` and `Hold::Bars`).
        editable_big_number(
            state,
            "practice.bpm",
            format!("Tempo: {:.0} BPM", bpm),
            format!("{:.0}", bpm),
            TS_SM,
            |s: &mut AppState, v: f64| {
                s.practice_bpm = (v as f32).clamp(30.0, 240.0);
            },
        ),
        sized_box(slider(30.0, 240.0, bpm as f64, |s: &mut AppState, v: f64| {
            s.practice_bpm = (v as f32).clamp(30.0, 240.0);
        }))
        .fixed_width(masonry::layout::Length::px(360.0)),
        flex_row((
            text_button("−", |s: &mut AppState| {
                s.practice_bpm = (s.practice_bpm - 1.0).clamp(30.0, 240.0);
            }),
            text_button("+", |s: &mut AppState| {
                s.practice_bpm = (s.practice_bpm + 1.0).clamp(30.0, 240.0);
            }),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_2),
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
        // Item browse (preview only — no transport/playback here).
        flex_row((
            button_sm("‹‹ Prev", move |s: &mut AppState| {
                if item_count > 0 {
                    let cur = s.practice_item_idx.min(item_count - 1) as i32;
                    s.practice_item_idx = ((cur - 1).rem_euclid(item_count as i32)) as usize;
                }
            }),
            dim_label(
                state.palette,
                if item_count > 0 {
                    format!("Item {} / {}", item_idx + 1, item_count)
                } else {
                    "no items".to_string()
                },
                TS_XS,
            ),
            button_sm("Next ››", move |s: &mut AppState| {
                if item_count > 0 {
                    let cur = s.practice_item_idx.min(item_count - 1) as i32;
                    s.practice_item_idx = ((cur + 1).rem_euclid(item_count as i32)) as usize;
                }
            }),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_2),
        header_label(state.palette, item_label, TS_XL),
        FlexSpacer::Flex(1.0),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .main_axis_alignment(MainAxisAlignment::Start)
    .gap(SP_2);

    use masonry::layout::Length as MLen;
    pane_split(
        card(
            state.palette,
            sized_box(fretboard_view(
                board,
                positions,
                labels,
                state.diagram_colors(),
                None,
                (0, state.fret_span),
                Vec::new(),
            ))
            .fixed_height(masonry::layout::Length::px(660.0)),
        ),
        card(state.palette, info_panel),
    )
    .split_point(state.split_ratio)
    .on_split_changed(|s: &mut AppState, f: f64| s.split_ratio = f)
    .min_lengths(MLen::const_px(240.0), MLen::const_px(240.0))
}

/// Translate a [`PracticeItem`] into the fretboard positions + labels
/// the visualization should display. Each variant has its own filter
/// rules (scale / chord position window, exercise dedup).
/// Clamp positions to a pinned neck window (open strings always shown).
/// No-op when the window is `None`. Used by `resolve_card_for_stage`.
fn apply_fret_window(positions: &mut Vec<Position>, window: Option<FretWindow>) {
    if let Some(w) = window {
        let end = w.start.saturating_add(w.span.saturating_sub(1));
        positions.retain(|p| p.fret == 0 || (p.fret >= w.start && p.fret <= end));
    }
}

/// Default tempo for set auto-advance when a card pins no BPM.
const REHEARSAL_DEFAULT_BPM: f32 = 90.0;

/// How long to dwell on a card during auto-advance (U6d), from its `Hold`
/// + tempo. A `Manual` card gets a sensible default (two bars) so playback
/// still flows; meter isn't on the card yet, so a bar is four beats.
fn card_duration_secs(card: &Card) -> f32 {
    let bpm = card.timing.bpm.unwrap_or(REHEARSAL_DEFAULT_BPM).max(1.0);
    let bar_secs = 4.0 * 60.0 / bpm;
    match card.timing.hold {
        Hold::Bars(n) => n.max(1) as f32 * bar_secs,
        Hold::Reps(r) => r.max(1) as f32 * bar_secs, // a rep ≈ a bar for now
        Hold::Seconds(s) => s.max(0.1),
        Hold::Manual => 2.0 * bar_secs,
    }
}

/// Progressions tab — left column lists the catalog; middle shows the
/// fretboard for the currently-selected chord; right shows the
/// progression details + key picker + clickable chord cards.
///
/// Key controls live on the right column to match the Scales/Chords
/// layout. Scale (Major / Minor / mode) is hardcoded to Major for
/// now — a key-mode picker is a follow-up if needed.
/// Inline editor for a user-authored progression (redesign R4) — lives
/// on the Progression lens (where you pick the card) rather than in
/// Settings. One row per degree-based chord (degree ± / #b / quality
/// cycle / remove) plus + chord / Delete. The progression is already the
/// selected one on the lens, so there's no "Apply" here. Fully owns its
/// data (clones the name into each closure), so it doesn't hold a borrow
/// of `state`.
fn user_progression_editor(
    palette: Palette,
    def: &settings::UserProgressionDef,
) -> impl WidgetView<AppState> + use<> {
    let name = def.name.clone();
    let mut chord_rows: Vec<AnyFlexChild<AppState>> = Vec::new();
    for (ci, r) in def.roles.iter().enumerate() {
        let alt = DegreeAlteration::ALL[(r.alteration as usize).min(DegreeAlteration::ALL.len() - 1)];
        let qual = RoleQuality::ALL[(r.quality as usize).min(RoleQuality::ALL.len() - 1)];
        let lbl = format!("{}{} · {}", alt.symbol(), r.degree, qual.chord_formula_name());
        let (n1, n2, n3, n4, n5, n6) = (
            name.clone(),
            name.clone(),
            name.clone(),
            name.clone(),
            name.clone(),
            name.clone(),
        );
        chord_rows.push(
            flex_row((
                sized_box(label(lbl).text_size(TS_XS))
                    .fixed_width(masonry::layout::Length::px(150.0)),
                button_sm("deg −", move |s: &mut AppState| s.nudge_prog_degree(&n1, ci, -1)),
                button_sm("deg +", move |s: &mut AppState| s.nudge_prog_degree(&n2, ci, 1)),
                button_sm("#/b", move |s: &mut AppState| s.cycle_prog_alteration(&n3, ci)),
                button_sm("‹", move |s: &mut AppState| s.cycle_prog_quality(&n4, ci, -1)),
                button_sm("›", move |s: &mut AppState| s.cycle_prog_quality(&n5, ci, 1)),
                button_sm("×", move |s: &mut AppState| s.remove_prog_chord(&n6, ci)),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .main_axis_alignment(MainAxisAlignment::Start)
            .gap(SP_1)
            .into_any_flex(),
        );
    }
    let (nadd, ndel) = (name.clone(), name.clone());
    card(
        palette,
        flex_col((
            flex_row((
                label(format!("* Editing: {name}"))
                    .text_size(TS_SM)
                    .color(palette.tertiary),
                FlexSpacer::Flex(1.0),
                button_sm("+ chord", move |s: &mut AppState| s.add_prog_chord(&nadd)),
                button_sm("× Delete", move |s: &mut AppState| s.remove_user_progression(&ndel)),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .main_axis_alignment(MainAxisAlignment::Start)
            .gap(SP_2),
            flex_col(chord_rows)
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_alignment(MainAxisAlignment::Start)
                .gap(SP_1),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_1),
    )
}

fn progressions_view(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    let key_root = state.root.to_pitch(4);
    // Use the catalog's first scale as the key — that's "Major".
    // Hardcoding for now; mode picker can come later.
    let major_scale: &'static ScaleFormula = woodshedding::scale::catalog()
        .iter()
        .find(|s| s.name == "Major")
        .expect("woodshedding catalog has a Major scale");

    // Left: progression list — buttons stacked vertically.
    // Selecting a progression resets the per-chord voicing index
    // vec to the new length so voicing arrows stay in-bounds.
    let cat_count = progression_catalog().len();
    let mut list_items: Vec<AnyFlexChild<AppState>> = progression_catalog()
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let chord_count = p.roles.len();
            let active = state.progression_idx == Some(i);
            list_item_button(state.palette, active, p.name, move |s: &mut AppState| {
                s.progression_idx = Some(i);
                s.progression_expanded_chord = Some(0);
                s.progression_voicing_idx = vec![0; chord_count];
            })
            .into_any_flex()
        })
        .collect();
    // User progressions follow the catalog in the combined selection.
    for (j, def) in state.user_progressions.iter().enumerate() {
        let combined = cat_count + j;
        let chord_count = def.roles.len();
        let active = state.progression_idx == Some(combined);
        list_items.push(
            list_item_button(
                state.palette,
                active,
                format!("* {}", def.name),
                move |s: &mut AppState| {
                    s.progression_idx = Some(combined);
                    s.progression_expanded_chord = Some(0);
                    s.progression_voicing_idx = vec![0; chord_count];
                },
            )
            .into_any_flex(),
        );
    }
    let list_card = nav_card(state.palette,
        flex_col((
            header_label(state.palette, "Progressions", TS_MD),
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
            // Author a new custom progression right where you pick one
            // (redesign R4) — selected immediately, editor opens in the
            // right pane.
            text_button("+ New progression", |s: &mut AppState| s.new_user_progression()),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_2),
    );

    // Build the materialized chord list + the currently-expanded chord.
    // Owned `(name, description, chords)` so it works for both catalog
    // progressions and user-authored ones.
    let materialized: Option<(String, String, Vec<ProgressionChord>)> =
        state.progression_idx.and_then(|idx| {
            if idx < cat_count {
                let p = progression_catalog().get(idx)?;
                p.apply_in_key(key_root, major_scale)
                    .ok()
                    .map(|chords| (p.name.to_string(), p.description.to_string(), chords))
            } else {
                let def = state.user_progressions.get(idx - cat_count)?;
                let roles = user_progression_roles(def);
                apply_roles_in_key(&roles, key_root, major_scale)
                    .ok()
                    .map(|chords| {
                        (def.name.clone(), "Custom progression.".to_string(), chords)
                    })
            }
        });

    let expanded_chord: Option<&ProgressionChord> =
        materialized
            .as_ref()
            .and_then(|(_, _, chords)| {
                let idx = state.progression_expanded_chord.unwrap_or(0);
                chords.get(idx)
            });

    // Middle: fretboard for the expanded chord's currently-selected
    // voicing. Empty when no progression is picked.
    let board = state.fretboard.clone();
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
        (_, Some((_, _, chords))) if state.progression_overlay_mode => {
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
    // Fill the surface pane (no fixed size, no scroll) — canvas scales.
    let fretboard_card = fretboard_widget(
        state,
        fretboard_view(
            board,
            positions,
            labels,
            state.diagram_colors(),
            dot_colors,
            (state.fret_start, state.fret_span),
            Vec::new(),
        ),
    );

    // Right: progression info + key picker + chord cards column.
    let display_key = state.root.display();
    let chord_cards = match &materialized {
        Some((prog_name_s, prog_desc_s, chords)) => {
            let prog_name = prog_name_s.clone();
            let prog_desc = prog_desc_s.clone();
            // Build one mini-card per chord. Each card carries:
            //   - chord symbol + role label + voicing N/M
            //   - the visual chord diagram (clickable: selects this
            //     chord as the "expanded" one on the main fretboard)
            //   - ‹ / › arrows to cycle this chord's voicing
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
                            .fixed_width(masonry::layout::Length::px(150.0))
                            .fixed_height(masonry::layout::Length::px(180.0)),
                        )
                    } else {
                        // Chord card = the fretboard at a tight,
                        // anchored 4-fret window: fretted dots in the
                        // chord's hue (root pops), open/muted markers,
                        // and a "Nfr" label when above the nut.
                        let v = voicings[v_idx].clone();
                        let lowest = v.lowest_fretted_position();
                        let start_fret = if lowest <= 1 { 0u8 } else { lowest - 1 };
                        let positions: Vec<Position> = voicing_to_positions(&v)
                            .into_iter()
                            .filter(|p| p.fret > 0)
                            .collect();
                        let dot_colors: Vec<Color> = positions
                            .iter()
                            .map(|p| {
                                if p.interval_from_root
                                    == Some(woodshedding::interval::Interval::PERFECT_UNISON)
                                {
                                    state.palette.root_dot
                                } else {
                                    chord_hue
                                }
                            })
                            .collect();
                        let marks = voicing_to_marks(&v);
                        OneOf2::B(
                            button(
                                sized_box(fretboard_view(
                                    state.fretboard.clone(),
                                    positions,
                                    Vec::new(),
                                    state.diagram_colors(),
                                    Some(dot_colors),
                                    (start_fret, 4),
                                    marks,
                                ))
                                .fixed_width(masonry::layout::Length::px(150.0))
                                .fixed_height(masonry::layout::Length::px(180.0)),
                                move |s: &mut AppState| {
                                    s.progression_expanded_chord = Some(i);
                                },
                            )
                            // Keep the card-style button background (the
                            // per-card surface Mark likes) but tighten the
                            // padding so the bigger diagram fills more of the
                            // card instead of floating in pillowy margin.
                            .padding(masonry::properties::Padding::from_vh(SP_1, SP_1)),
                        )
                    };

                    let arrows = flex_row((
                        button_sm("‹", move |s: &mut AppState| {
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
                        button_sm("›", move |s: &mut AppState| {
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
                // Cap at 4 per row — past that the diagrams get too
                // small a share of attention and the eye loses the
                // progression's left-to-right reading order.
                n.clamp(1, 4)
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
                .position(|&pc| pc == state.root)
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
                    // the old `‹ Key: C ›` cycle row. The ▲/▼ arrows on
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
                                s.root = ChromaticPc::ALL[i];
                            },
                        ),
                        // Keep the ‹/› cycle as a fine-tune affordance
                        // — chromatic neighbour walking is faster than
                        // re-opening the picker.
                        button_sm("‹", |s: &mut AppState| {
                            s.root = s.root.cycle(-1);
                        }),
                        button_sm("›", |s: &mut AppState| {
                            s.root = s.root.cycle(1);
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
                    button_sm("‹", |s: &mut AppState| {
                        s.root = s.root.cycle(-1);
                    }),
                    button_sm("›", |s: &mut AppState| {
                        s.root = s.root.cycle(1);
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
    // Custom-progression editor (redesign R4): when the selected
    // progression is a user one (*), its editor opens in the right pane
    // below the chord grid — authoring lives where you pick the card,
    // not in Settings. Owns its data, so it doesn't hold a borrow of
    // `state` across the surface_left `&mut`.
    let palette = state.palette;
    let prog_editor: OneOf2<_, _> = match state.progression_idx {
        Some(idx) if idx >= cat_count => match state.user_progressions.get(idx - cat_count) {
            Some(def) => OneOf2::A(user_progression_editor(palette, def)),
            None => OneOf2::B(sized_box(label("")).fixed_height(SP_0)),
        },
        _ => OneOf2::B(sized_box(label("")).fixed_height(SP_0)),
    };
    let right_pane = scroll_tab(card(
        palette,
        flex_col((chord_cards, prog_editor))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .main_axis_alignment(MainAxisAlignment::Start)
            .gap(SP_3),
    ));
    let surface = surface_left(state, fretboard_card.boxed());
    flex_row((
        sidebar,
        pane_split(surface, right_pane)
            .split_point(state.split_ratio)
            .bar_color(state.palette.surface_hover)
        .on_split_changed(|s: &mut AppState, f: f64| s.split_ratio = f)
            .min_lengths(MLen::const_px(240.0), MLen::const_px(280.0))
            .flex(1.0),
    ))
    // Cross-axis Stretch so the split fills the bounded tab-content
    // height; surface modules + chord-cards pane scroll internally.
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
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
        .unwrap_or((s.root, 4))
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
/// - **Manual**: ‹ Step / Step › buttons advance one position at a
///   time. Best for learning the pattern note-by-note.
/// - **Auto**: Play button starts a task that advances at the
///   chosen BPM. Best for practicing the exercise at tempo.
/// Inline editor for a user-authored exercise (redesign R4) — lives on
/// the Exercise lens (where you pick the card) rather than in Settings.
/// One row per step (string ± / fret ± / finger ± / remove) plus + step /
/// Delete. Fully owns its data so it doesn't hold a borrow of `state`.
fn user_exercise_editor(
    palette: Palette,
    def: &settings::UserExerciseDef,
) -> impl WidgetView<AppState> + use<> {
    let name = def.name.clone();
    let mut step_rows: Vec<AnyFlexChild<AppState>> = Vec::new();
    for (si, st) in def.steps.iter().enumerate() {
        let finger = if st.finger == 0 {
            "–".to_string()
        } else {
            st.finger.to_string()
        };
        let lbl = format!("str {} · {}fr · f{}", st.string + 1, st.fret, finger);
        let (a, b, c, d, ee, ff, g) = (
            name.clone(),
            name.clone(),
            name.clone(),
            name.clone(),
            name.clone(),
            name.clone(),
            name.clone(),
        );
        step_rows.push(
            flex_row((
                sized_box(label(lbl).text_size(TS_XS))
                    .fixed_width(masonry::layout::Length::px(150.0)),
                button_sm("str −", move |s: &mut AppState| s.nudge_ex_step(&a, si, 0, -1)),
                button_sm("str +", move |s: &mut AppState| s.nudge_ex_step(&b, si, 0, 1)),
                button_sm("fr −", move |s: &mut AppState| s.nudge_ex_step(&c, si, 1, -1)),
                button_sm("fr +", move |s: &mut AppState| s.nudge_ex_step(&d, si, 1, 1)),
                button_sm("f −", move |s: &mut AppState| s.nudge_ex_step(&ee, si, 2, -1)),
                button_sm("f +", move |s: &mut AppState| s.nudge_ex_step(&ff, si, 2, 1)),
                button_sm("×", move |s: &mut AppState| s.remove_ex_step(&g, si)),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .main_axis_alignment(MainAxisAlignment::Start)
            .gap(SP_1)
            .into_any_flex(),
        );
    }
    let (nadd, ndel) = (name.clone(), name.clone());
    card(
        palette,
        flex_col((
            flex_row((
                label(format!("* Editing: {name}"))
                    .text_size(TS_SM)
                    .color(palette.tertiary),
                FlexSpacer::Flex(1.0),
                button_sm("+ step", move |s: &mut AppState| s.add_ex_step(&nadd)),
                button_sm("× Delete", move |s: &mut AppState| s.remove_user_exercise(&ndel)),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .main_axis_alignment(MainAxisAlignment::Start)
            .gap(SP_2),
            flex_col(step_rows)
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_alignment(MainAxisAlignment::Start)
                .gap(SP_1),
            dim_prose(
                palette,
                "Steps are string + fret + finger. Build with the ± buttons; \
                 the transport above steps through them.",
                TS_XS,
            ),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_1),
    )
}

fn exercises_view(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    /// How many previous steps to render as a fading trail behind
    /// the current step. 4 = current + 3 history.
    const TRAIL_LEN: usize = 4;

    // Steps + name come from the catalog (a generator) or a user
    // exercise (stored steps), per the combined selection index.
    let ex_cat_count = exercise_catalog().len();
    let (steps, exercise_name, exercise_desc): (Vec<ExerciseStep>, String, String) =
        if state.exercise_idx < ex_cat_count {
            let ex = &exercise_catalog()[state.exercise_idx];
            let params = ExerciseParams {
                starting_fret: state.exercise_starting_fret,
                direction: ExerciseDirection::Both,
                trill_repeats: 8,
            };
            (
                ex.generate(&state.fretboard.tuning, &params),
                ex.name.to_string(),
                ex.description.to_string(),
            )
        } else {
            match state.user_exercises.get(state.exercise_idx - ex_cat_count) {
                Some(def) => (
                    user_exercise_steps(def),
                    def.name.clone(),
                    "Custom exercise.".to_string(),
                ),
                None => (Vec::new(), "—".to_string(), String::new()),
            }
        };
    let step_count = steps.len();
    // Cursor follows the metronome beat when it's running (shared clock,
    // 3d) — phase-locked to the click — otherwise the exercise's own
    // Play/Step drives `exercise_step_idx`.
    let metro_beat = state.metronome_beat();
    let current_idx = if step_count == 0 {
        0
    } else if let Some(beat) = metro_beat {
        (beat as usize) % step_count
    } else {
        state.exercise_step_idx.min(step_count - 1)
    };
    // Per-step frequencies (open string pitch + fret semitones) for the
    // step-through audio task.
    let step_freqs: Vec<f32> = steps
        .iter()
        .map(|st| {
            match state.fretboard.tuning.strings.get(st.string_index) {
                Some(p) => {
                    let midi = p.midi() + st.fret as i32;
                    440.0 * 2f32.powf((midi as f32 - 69.0) / 12.0)
                }
                None => 0.0,
            }
        })
        .collect();

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
            text_button("› Play", |s: &mut AppState| s.exercise_playing = true),
        )
    };

    // Exercise picker — combobox for jump + ‹/› for adjacent. Both
    // arms reset the step index and pause playback so switching
    // exercises doesn't strand the trail highlight on a stale step.
    let mut exercise_options: Vec<ArcStr> = exercise_catalog()
        .iter()
        .map(|e| ArcStr::from(e.name))
        .collect();
    for e in &state.user_exercises {
        exercise_options.push(ArcStr::from(format!("* {}", e.name)));
    }
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
            button_sm("‹", |s: &mut AppState| {
                s.cycle_exercise(-1);
                s.exercise_step_idx = 0;
                s.exercise_playing = false;
            }),
            button_sm("›", |s: &mut AppState| {
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
            button_sm("‹", |s: &mut AppState| {
                s.exercise_starting_fret =
                    s.exercise_starting_fret.saturating_sub(1).max(1);
                s.exercise_step_idx = 0;
            }),
            button_sm("›", |s: &mut AppState| {
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
            text_button("‹ Step", move |s: &mut AppState| {
                if step_count > 0 {
                    let cur = s.exercise_step_idx.min(step_count - 1) as i32;
                    s.exercise_step_idx =
                        ((cur - 1).rem_euclid(step_count as i32)) as usize;
                    s.exercise_playing = false;
                }
            }),
            play_button,
            text_button("Step ›", move |s: &mut AppState| {
                if step_count > 0 {
                    let cur = s.exercise_step_idx.min(step_count - 1) as i32;
                    s.exercise_step_idx =
                        ((cur + 1).rem_euclid(step_count as i32)) as usize;
                    s.exercise_playing = false;
                }
            }),
            text_button("‹‹ Reset", |s: &mut AppState| {
                s.exercise_step_idx = 0;
            }),
            button_sm(
                if state.transport_sound { "Sound" } else { "Muted" },
                |s: &mut AppState| s.transport_sound = !s.transport_sound,
            ),
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
    // Own timer only when the metronome isn't driving the cursor.
    let auto_task = (playing && metro_beat.is_none() && step_count > 0).then(|| {
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
    let mut exercise_list_items: Vec<AnyFlexChild<AppState>> = exercise_catalog()
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let active = state.exercise_idx == i;
            list_item_button(state.palette, active, e.name, move |s: &mut AppState| {
                s.exercise_idx = i;
                s.exercise_step_idx = 0;
                s.exercise_playing = false;
            })
            .into_any_flex()
        })
        .collect();
    for (j, def) in state.user_exercises.iter().enumerate() {
        let combined = ex_cat_count + j;
        let active = state.exercise_idx == combined;
        exercise_list_items.push(
            list_item_button(
                state.palette,
                active,
                format!("* {}", def.name),
                move |s: &mut AppState| {
                    s.exercise_idx = combined;
                    s.exercise_step_idx = 0;
                    s.exercise_playing = false;
                },
            )
            .into_any_flex(),
        );
    }
    let exercise_list_card = nav_card(
        state.palette,
        flex_col((
            header_label(state.palette, "Exercises", TS_MD),
            portal(
                flex_col(exercise_list_items)
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_alignment(MainAxisAlignment::Start)
                    .gap(SP_1),
            )
            .constrain_horizontal(true)
            .flex(1.0),
            // Author a new custom exercise right where you pick one
            // (redesign R4) — editor opens in the right pane.
            text_button("+ New exercise", |s: &mut AppState| s.new_user_exercise()),
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
    // Fill the surface pane (no fixed height, no scroll) — canvas scales.
    let fretboard_card = fretboard_widget(
        state,
        fretboard_view(
            board_for_widget,
            positions,
            labels,
            state.diagram_colors(),
            Some(dot_colors),
            (state.fret_start, state.fret_span),
            Vec::new(),
        ),
    )
    .boxed();
    // Custom-exercise editor (redesign R4): when the selected exercise is
    // a user one (*), its editor opens in the right pane below the info
    // panel — authoring lives where you pick the card, not in Settings.
    let palette = state.palette;
    let ex_editor: OneOf2<_, _> = if state.exercise_idx >= ex_cat_count {
        match state.user_exercises.get(state.exercise_idx - ex_cat_count) {
            Some(def) => OneOf2::A(user_exercise_editor(palette, def)),
            None => OneOf2::B(sized_box(label("")).fixed_height(SP_0)),
        }
    } else {
        OneOf2::B(sized_box(label("")).fixed_height(SP_0))
    };
    let right_pane = scroll_tab(card(
        palette,
        flex_col((info_panel, ex_editor))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .main_axis_alignment(MainAxisAlignment::Start)
            .gap(SP_3),
    ));
    let surface = surface_left(state, fretboard_card);
    let visible = flex_row((
        exercise_sidebar,
        pane_split(surface, right_pane)
            .split_point(state.split_ratio)
            .bar_color(state.palette.surface_hover)
            .on_split_changed(|s: &mut AppState, f: f64| s.split_ratio = f)
            .min_lengths(MLen::const_px(240.0), MLen::const_px(240.0))
            .flex(1.0),
    ))
    // Cross-axis Stretch so the split fills the bounded tab-content
    // height; surface modules + info pane scroll internally.
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .main_axis_alignment(MainAxisAlignment::Start)
    .gap(SP_4);

    // Step-through audio: sound each step's note (open-string pitch +
    // fret) on change, when running + sound enabled. Mirrors the arpeggio.
    let metro_driving = metro_beat.is_some();
    let audio_active =
        state.transport_sound && (playing || metro_driving) && step_count > 0;
    let audio_task = audio_active.then(|| {
        task_raw(
            move |proxy, _| async move {
                let mut tick = time::interval(Duration::from_millis(20));
                loop {
                    tick.tick().await;
                    if proxy.message(()).is_err() {
                        break;
                    }
                }
            },
            move |s: &mut AppState, _: ()| {
                let cursor = s
                    .metronome_beat()
                    .map(|b| b as usize)
                    .unwrap_or(s.exercise_step_idx);
                let idx = cursor % step_freqs.len().max(1);
                if s.exercise_last_sounded != Some(idx) {
                    s.exercise_last_sounded = Some(idx);
                    let f = step_freqs[idx];
                    if f > 0.0 {
                        if let Some(h) = s.ensure_song_engine() {
                            h.play_note_now(f, 0.18);
                        }
                    }
                }
            },
        )
    });

    fork(fork(visible, auto_task), audio_task)
}

/// Compact metronome **widget** for the instrument-surface stack.
/// Same controls as the Metronome tab, but dense enough to sit in a
/// stacked pane without scrolling: title · BPM · transport on one
/// line, a tempo slider, one row of ± steppers, and one row folding
/// time-sig / subdivision / click / accent into compact cycle buttons.
fn metronome_module(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    let bpm_text = format!("{:.0} BPM", state.bpm);
    let playing = state.metronome_playing;
    let engine_error = match &state.engine {
        Err(e) => Some(e.clone()),
        _ => None,
    };
    let time_sig_text = format!("{}/4", state.metronome_time_sig_num);
    let click_text = state.metronome_click.label();
    let accent_text = state.metronome_accent.label();
    let sub_text = subdivision_label(state.metronome_subdivision);

    let transport = if let Some(err) = engine_error {
        OneOf3::A(danger_prose(state.palette, format!("Audio: {err}"), TS_XS))
    } else if playing {
        OneOf3::B(button_sm("■ Stop", |s: &mut AppState| s.stop_metronome()))
    } else {
        OneOf3::C(button_sm("› Play", |s: &mut AppState| s.play_metronome()))
    };

    let panel = flex_col((
        // Title + transport on the top line; the BPM readout sits on its
        // own line below so a narrow pane doesn't clip the trio.
        flex_row((
            header_label(state.palette, "Metronome", TS_MD),
            FlexSpacer::Flex(1.0),
            transport,
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_2),
        label(bpm_text)
            .text_size(TS_LG)
            .font(mono_family())
            .color(state.palette.text_header),
        sized_box(slider(40.0, 240.0, state.bpm as f64, |s: &mut AppState, v: f64| {
            let b = (v as f32).clamp(40.0, 240.0);
            s.bpm = b;
            if let Ok((_, h)) = &s.engine {
                h.set_bpm(b);
            }
        }))
        .fixed_width(masonry::layout::Length::px(200.0)),
        flex_row((
            button_sm("−10", |s: &mut AppState| {
                s.bpm = (s.bpm - 10.0).clamp(40.0, 240.0);
                if let Ok((_, h)) = &s.engine {
                    h.set_bpm(s.bpm);
                }
            }),
            button_sm("−", |s: &mut AppState| {
                s.bpm = (s.bpm - 1.0).clamp(40.0, 240.0);
                if let Ok((_, h)) = &s.engine {
                    h.set_bpm(s.bpm);
                }
            }),
            button_sm("+", |s: &mut AppState| {
                s.bpm = (s.bpm + 1.0).clamp(40.0, 240.0);
                if let Ok((_, h)) = &s.engine {
                    h.set_bpm(s.bpm);
                }
            }),
            button_sm("+10", |s: &mut AppState| {
                s.bpm = (s.bpm + 10.0).clamp(40.0, 240.0);
                if let Ok((_, h)) = &s.engine {
                    h.set_bpm(s.bpm);
                }
            }),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_1),
        // Settings wrap onto two rows so four buttons don't overrun a
        // narrow pane.
        flex_row((
            button_sm(format!("Time {time_sig_text}"), |s: &mut AppState| {
                s.metronome_time_sig_num = if s.metronome_time_sig_num >= 12 {
                    1
                } else {
                    s.metronome_time_sig_num + 1
                };
                s.apply_metronome_pattern();
            }),
            button_sm(format!("{sub_text}"), |s: &mut AppState| {
                s.metronome_subdivision = cycle_subdivision(s.metronome_subdivision);
                s.apply_metronome_pattern();
            }),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_1),
        flex_row((
            button_sm(click_text, |s: &mut AppState| {
                s.metronome_click = s.metronome_click.next();
                s.apply_metronome_pattern();
            }),
            button_sm(accent_text, |s: &mut AppState| {
                s.metronome_accent = s.metronome_accent.next();
                s.apply_metronome_pattern();
            }),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_1),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .main_axis_alignment(MainAxisAlignment::Start)
    .gap(SP_1);

    card(state.palette, panel)
}

/// Compact tuner **widget** for the instrument-surface stack. The full
/// Tuner tab keeps the tunings catalog + threshold editor + help prose;
/// this dense form is title · note · transport on one line, the cents
/// needle + offset, a thin level bar, the string-target row, and a
/// detector cycle — enough to tune by while watching the fretboard,
/// without scrolling. Carries its own polling `fork` like the tab.
fn tuner_module(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    let listening = state.tuner_active;
    let tuner_handle = state.input.as_ref().ok().map(|b| b.tuner.clone());
    let error_text = match &state.input {
        Err(e) => Some(e.clone()),
        _ => None,
    };
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
        .and_then(|s| s.note.as_ref().map(|n| format!("{:+.1}¢", n.cents_offset)))
        .unwrap_or_else(|| "—".to_string());
    let cents_raw: Option<f64> = state
        .tuner_snapshot
        .as_ref()
        .and_then(|s| s.note.as_ref())
        .map(|n| n.cents_offset);
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

    let transport = if let Some(err) = error_text {
        OneOf3::A(danger_prose(state.palette, format!("Mic: {err}"), TS_XS))
    } else if listening {
        OneOf3::B(button_sm("■ Stop", |s: &mut AppState| s.stop_tuner()))
    } else {
        OneOf3::C(button_sm("› Tune", |s: &mut AppState| s.start_tuner()))
    };

    let mut string_btns: Vec<AnyFlexChild<AppState>> = Vec::new();
    string_btns.push(
        button_sm("Free", |s: &mut AppState| {
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
            button_sm(label_text, move |s: &mut AppState| {
                let hint = ChromaticPc::from_pc(pitch.pitch_class() as u8).to_detected();
                s.tuner_target = Some(hint.clone());
                if let Ok(b) = &s.input {
                    b.tuner.set_target_hint(Some(hint));
                }
            })
            .into_any_flex(),
        );
    }

    let detector_label = match state.tuner_detector {
        DetectorKind::Fft => "FFT",
        DetectorKind::Cepstrum => "Cepstrum",
        DetectorKind::McLeod => "McLeod",
    };

    let panel = flex_col((
        flex_row((
            header_label(state.palette, "Tuner", TS_MD),
            FlexSpacer::Flex(1.0),
            label(note_text)
                .text_size(TS_LG)
                .font(mono_family())
                .color(state.palette.text_header),
            transport,
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_2),
        flex_row((
            sized_box(widgets::cents_meter_view(
                cents_raw,
                in_tune,
                widgets::MeterColors::from_palette(&state.palette),
            ))
            .fixed_width(masonry::layout::Length::px(200.0))
            .fixed_height(masonry::layout::Length::px(30.0)),
            label(cents_text).text_size(TS_XS).font(mono_family()),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(SP_2),
        sized_box(widgets::level_meter_view(
            level_raw,
            widgets::MeterColors::from_palette(&state.palette),
        ))
        .fixed_width(masonry::layout::Length::px(200.0))
        .fixed_height(masonry::layout::Length::px(12.0)),
        {
            // String-target buttons chunked into rows of 4 so the row
            // (Free + 6 strings on guitar) doesn't clip the card border.
            let mut rows: Vec<_> = Vec::new();
            let mut buf: Vec<AnyFlexChild<AppState>> = Vec::new();
            for b in string_btns {
                buf.push(b);
                if buf.len() == 4 {
                    rows.push(
                        flex_row(std::mem::take(&mut buf))
                            .cross_axis_alignment(CrossAxisAlignment::Center)
                            .main_axis_alignment(MainAxisAlignment::Start)
                            .gap(SP_1),
                    );
                }
            }
            if !buf.is_empty() {
                rows.push(
                    flex_row(buf)
                        .cross_axis_alignment(CrossAxisAlignment::Center)
                        .main_axis_alignment(MainAxisAlignment::Start)
                        .gap(SP_1),
                );
            }
            flex_col(rows)
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_alignment(MainAxisAlignment::Start)
                .gap(SP_1)
        },
        button_sm(format!("Detector: {detector_label}"), |s: &mut AppState| {
            s.tuner_detector = match s.tuner_detector {
                DetectorKind::Fft => DetectorKind::Cepstrum,
                DetectorKind::Cepstrum => DetectorKind::McLeod,
                DetectorKind::McLeod => DetectorKind::Fft,
            };
            if let Ok(b) = &s.input {
                b.tuner.set_detector_kind(s.tuner_detector);
            }
        }),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .main_axis_alignment(MainAxisAlignment::Start)
    .gap(SP_1);

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

    fork(card(state.palette, panel), polling_task)
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
            text_button("› Play", |s: &mut AppState| s.play_metronome()),
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
            button_sm("‹", |s: &mut AppState| {
                s.metronome_time_sig_num =
                    (s.metronome_time_sig_num.saturating_sub(1)).max(1);
                s.apply_metronome_pattern();
            }),
            button_sm("›", |s: &mut AppState| {
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
            button_sm("×", move |s: &mut AppState| {
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
                let (h, s, l) = audio_widgets::theme::color_to_hsl(col);
                let swatch = sized_box(label(""))
                    .fixed_width(px(28.0))
                    .fixed_height(px(20.0))
                    .background_color(col)
                    .corner_radius(px(4.0))
                    .border(border, px(1.0));
                // H 0..360, S/L 0..100 — more intuitive than raw RGB.
                let chan = move |comp: u8, value: f64, max: f64| {
                    sized_box(slider(0.0, max, value, move |s: &mut AppState, v: f64| {
                        s.set_seed_hsl(idx, comp, v);
                    }))
                    .fixed_width(px(80.0))
                };
                rows.push(
                    flex_row((
                        sized_box(dim_label(state.palette, lbl, TS_XS)).fixed_width(px(70.0)),
                        swatch,
                        chan(0, h, 360.0),
                        chan(1, s * 100.0, 100.0),
                        chan(2, l * 100.0, 100.0),
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
                        let (h, sat, lt) = audio_widgets::theme::color_to_hsl(col);
                        let swatch = sized_box(label(""))
                            .fixed_width(px(28.0))
                            .fixed_height(px(20.0))
                            .background_color(col)
                            .corner_radius(px(4.0))
                            .border(border, px(1.0));
                        let chan = move |comp: u8, value: f64, max: f64| {
                            sized_box(slider(0.0, max, value, move |s: &mut AppState, v: f64| {
                                s.set_text_hsl(is_header, comp, v);
                            }))
                            .fixed_width(px(80.0))
                        };
                        text_rows.push(
                            flex_row((
                                sized_box(toggle).fixed_width(px(120.0)),
                                swatch,
                                chan(0, h, 360.0),
                                chan(1, sat * 100.0, 100.0),
                                chan(2, lt * 100.0, 100.0),
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

    // Custom-tunings editor — one card per user tuning: name, per-string
    // ‹ note › semitone nudges, and Apply / ± string / Delete.
    let mut tuning_cards: Vec<_> = Vec::new();
    for t in &state.user_tunings {
        let name = t.name.clone();
        let active = state.fretboard.tuning.name == name;
        let prefix = if active { "● " } else { "  " };
        let header = label(format!("{prefix}{name} ({})", t.instrument))
            .text_size(TS_SM)
            .color(if active {
                state.palette.tertiary
            } else {
                state.palette.text
            });
        // One tight `‹ note ›` group per string (content-sized label so
        // there's no dead gap before the ›), with the spacing *between*
        // groups via the outer row's gap.
        let mut string_btns: Vec<AnyFlexChild<AppState>> = Vec::new();
        for (i, &m) in t.midi.iter().enumerate() {
            let p = woodshedding::pitch::Pitch::from_midi(m, woodshedding::pitch::Spelling::Sharps);
            let note = format!("{}{}{}", p.name, p.accidental, p.octave);
            let nd = name.clone();
            let nu = name.clone();
            string_btns.push(
                flex_row((
                    button_sm("‹", move |s: &mut AppState| s.nudge_user_string(&nd, i, -1)),
                    label(note).text_size(TS_XS).font(mono_family()),
                    button_sm("›", move |s: &mut AppState| s.nudge_user_string(&nu, i, 1)),
                ))
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .main_axis_alignment(MainAxisAlignment::Start)
                .gap(SP_1)
                .into_any_flex(),
            );
        }
        let na = name.clone();
        let nadd = name.clone();
        let nrem = name.clone();
        let ndel = name.clone();
        let card_view = card(
            state.palette,
            flex_col((
                header,
                flex_row(string_btns)
                    .cross_axis_alignment(CrossAxisAlignment::Center)
                    .main_axis_alignment(MainAxisAlignment::Start)
                    .gap(SP_3),
                flex_row((
                    button_sm("Apply", move |s: &mut AppState| s.apply_user_tuning(&na)),
                    button_sm("+ string", move |s: &mut AppState| s.add_user_string(&nadd)),
                    button_sm("− string", move |s: &mut AppState| s.remove_user_string(&nrem)),
                    button_sm("× Delete", move |s: &mut AppState| s.remove_user_tuning(&ndel)),
                ))
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .main_axis_alignment(MainAxisAlignment::Start)
                .gap(SP_2),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_alignment(MainAxisAlignment::Start)
            .gap(SP_1),
        );
        tuning_cards.push(card_view);
    }
    let tuning_section = flex_col((
        label("Custom tunings").text_size(TS_MD),
        text_button("+ New from current tuning", |s: &mut AppState| s.new_user_tuning()),
        flex_col(tuning_cards)
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_alignment(MainAxisAlignment::Start)
            .gap(SP_2),
        dim_prose(
            state.palette,
            "New tunings clone the current one; nudge each string with \
             ‹ ›, then Apply. They show up in the header tuning picker \
             for the matching instrument.",
            TS_XS,
        ),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .main_axis_alignment(MainAxisAlignment::Start)
    .gap(SP_1);

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
            // Custom tunings section. (Progression + exercise authoring
            // moved to their lenses in redesign R4 — Settings is for
            // preferences + tunings, which are shared context, not cards.)
            tuning_section,
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
            tunings_items.push(
                list_item_button(state.palette, active, spec.name, move |s: &mut AppState| {
                    let tuning = Tuning::from_spec(&spec);
                    s.fretboard = Fretboard::new(tuning, 24);
                })
                .into_any_flex(),
            );
        }
    }
    let tunings_list_card = nav_card(
        state.palette,
        flex_col((
            header_label(state.palette, "Tunings", TS_MD),
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
            // updates frame-to-frame. Without this the "Ab4" / "E2"
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
                OneOf2::A(success_label(state.palette, "in tune", TS_XS))
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

/// Per-string open/muted markers for the chord-card fretboard form.
fn voicing_to_marks(voicing: &ChordVoicing) -> Vec<StringMark> {
    voicing
        .strings
        .iter()
        .map(|sp| match sp {
            StringPlay::Played { fret: 0, .. } => StringMark::Open,
            StringPlay::Played { .. } => StringMark::None,
            StringPlay::Muted => StringMark::Muted,
        })
        .collect()
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

/// Like [`card`], but lighter-weight: tighter `SP_1` padding so a large
/// canvas (the fretboard neck) reads as a clean panel rather than a
/// thickly-matted one. Same hairline border + surface.
fn thin_card<V>(palette: Palette, inner: V) -> impl WidgetView<AppState>
where
    V: WidgetView<AppState>,
{
    sized_box(inner)
        .padding(SP_1)
        .corner_radius(SP_2)
        .background_color(palette.surface)
        .border(palette.surface_2, masonry::layout::Length::const_px(1.0))
}

/// Wrap a fretboard neck canvas as a surface widget: a thin card with a
/// compact start-fret control strip above the neck (`▼` toward the nut,
/// `▲` up the neck). Lets a ≤12-fret window slide past the 12th fret
/// while the neck flexes to fill the rest of the pane.
fn fretboard_widget<V>(state: &AppState, neck: V) -> impl WidgetView<AppState> + use<V>
where
    V: WidgetView<AppState>,
{
    let start = state.fret_start;
    let caption = if start == 0 {
        "from nut".to_string()
    } else {
        format!("fret {start}+")
    };
    let strip = flex_row((
        dim_label(state.palette, caption, TS_XS),
        FlexSpacer::Flex(1.0),
        button_sm("▼", |s: &mut AppState| s.nudge_fret_start(-1)),
        button_sm("▲", |s: &mut AppState| s.nudge_fret_start(1)),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .main_axis_alignment(MainAxisAlignment::Start)
    .gap(SP_1);
    thin_card(
        state.palette,
        flex_col((strip, neck.flex(1.0)))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .main_axis_alignment(MainAxisAlignment::Start)
            .gap(SP_1),
    )
}

/// Like [`card`], but the surface carries a faint `secondary` tint —
/// for "chrome" surfaces (browse-list sidebars) that read as the
/// support hue rather than neutral content. Matches the header strip's
/// tint, so navigation chrome shares one secondary identity.
fn nav_card<V>(palette: Palette, inner: V) -> impl WidgetView<AppState>
where
    V: WidgetView<AppState>,
{
    let surface = audio_widgets::theme::mix(palette.secondary, palette.surface, 0.85);
    sized_box(inner)
        .padding(SP_2)
        .corner_radius(SP_2)
        .background_color(surface)
        .border(palette.surface_2, masonry::layout::Length::const_px(1.0))
}

/// A selectable browse-list row. Selected rows read in `tertiary` (the
/// "you are here" emphasis hue) with a `●` cue; others in body `text`.
/// Built with `button` so the label color is ours to set.
fn list_item_button<F>(
    palette: Palette,
    active: bool,
    text: impl Into<String>,
    callback: F,
) -> impl WidgetView<AppState>
where
    F: Fn(&mut AppState) + Send + Sync + 'static,
{
    let prefix = if active { "● " } else { "  " };
    let color = if active { palette.tertiary } else { palette.text };
    button(
        label(format!("{prefix}{}", text.into()))
            .text_size(TS_SM)
            .color(color),
        move |s: &mut AppState| callback(s),
    )
}

// `ACTIVE_PALETTE` const stand-in lived here until 2026-05-18.
// Replaced by threading `state.palette` (by value — `Palette` is
// `Copy`) through `card()` and the semantic-color label helpers.
// A theme picker can now re-skin every card and dimmed label at
// the next rebuild pass by mutating `state.palette` in place.

/// Empirically-measured per-card width used by the chord-card
/// reflow chunking on the Progressions tab.
///
/// Sized to the card's actual content — a 120px chord diagram in a
/// button (≈32px padding + border) plus the chord-name / role labels
/// and the ‹ counter › arrows row, all left-aligned. The old 370px
/// value left ~200px of dead space inside every card (sparse,
/// inefficient), so cards reflow to compact columns and
/// `cards_per_row = floor((panel_w + gap) / (CHORD_CARD_W + gap))`
/// (capped at 4) packs up to four across a wide pane.
const CHORD_CARD_W: f64 = 172.0;

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
/// effects); × cancels.
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
// `‹/›` cycle arrows next to comboboxes) and cases where the primary
// action wants a heavier weight (transport controls). These wrap
// `button(label(...), cb)` so we can scale the inner label's
// `text_size` without touching every call site.
// =================================================================

use xilem::view::button as button_view;

/// Small button — `TS_XS` text. Use for cycle arrows, secondary
/// micro-actions, and any control that's "in service of" a larger
/// picker (the ‹/› flanking a combobox, for instance).
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
    // Tight padding so single-glyph cycle arrows (‹/›) read as
    // compact affordances rather than chunky buttons.
    .padding(masonry::properties::Padding::from_vh(SP_1, SP_2))
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
/// ("› Play", "■ Stop", "Start tuner") that anchor a panel.
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
            window(window_id, "Woodshed", root)
                .with_base_color(base_color)
                .with_default_properties(default_properties)
                .with_options(|o| {
                    // Native decorations off: we draw our own chrome (drag
                    // region + min/max/close in the header). See `window_chrome`.
                    o.with_decorations(false)
                        .with_min_inner_size(LogicalSize::new(640.0, 480.0))
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
