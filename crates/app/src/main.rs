use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::time::{Duration, Instant};

use iced::widget::{
    button, canvas, column, container, pick_list, row, scrollable, slider, text,
    text_input, Canvas,
};
use iced::{
    mouse, Color, Element, Length, Point, Rectangle, Renderer, Size, Subscription, Task,
    Theme,
};

use audio::{
    DetectedNote, DetectedNoteName, DetectorKind, EngineHandle, SequencerEngine,
    SequencerPattern, Sound, Step, Subdivision, TimeSignature, Track, TunerEngine,
    TunerHandle,
};
use audio::input::DEFAULT_SILENCE_RMS_THRESHOLD;
use music_theory::chord::{catalog as chord_catalog, ChordFormula};
use music_theory::exercise::{catalog as exercise_catalog, ExerciseParams};
use music_theory::fretboard::{
    BassConstraint, ChordVoicing, Fretboard, Position, StringPlay,
};
use music_theory::interval::Interval;
use music_theory::pitch::{Accidental, NoteName, Pitch};
use music_theory::practice::{catalog as practice_catalog, PracticeItem, PracticeSet};
use music_theory::progression::{
    catalog as progression_catalog, Progression, ProgressionChord,
};
use music_theory::scale::{catalog as scale_catalog, ScaleFormula};
use music_theory::tuning::{catalog as tuning_catalog, Instrument, Tuning};

/// 12-tone pitch class with sharp spelling, suitable for the tuner's
/// custom-target dropdown. Sharps only because pitch-detector exposes
/// the same convention.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
enum ChromaticPc {
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

    fn label(self) -> &'static str {
        match self {
            Self::C => "C",
            Self::CSharp => "C#",
            Self::D => "D",
            Self::DSharp => "D#",
            Self::E => "E",
            Self::F => "F",
            Self::FSharp => "F#",
            Self::G => "G",
            Self::GSharp => "G#",
            Self::A => "A",
            Self::ASharp => "A#",
            Self::B => "B",
        }
    }

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

    fn from_pc(pc: u8) -> Self {
        Self::ALL[(pc as usize) % 12]
    }

    fn to_pitch(self, octave: i8) -> Pitch {
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
}

impl fmt::Display for ChromaticPc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

const TUNER_OCTAVES: [i8; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
const SCALE_POSITIONS: [u8; 16] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];

/// Return the names of scales that have enough degrees to support
/// every role in the given progression. A pentatonic (5 degrees) can't
/// host a progression that uses degree 6 or 7, etc.
fn valid_key_scales_for_progression(prog: &Progression) -> Vec<&'static str> {
    let max_degree = prog
        .roles
        .iter()
        .map(|r| r.degree)
        .max()
        .unwrap_or(0) as usize;
    scale_catalog()
        .iter()
        .filter(|s| s.intervals.len() >= max_degree)
        .map(|s| s.name)
        .collect()
}

/// Compute all voicings for a chord across the whole neck, dedupe by
/// fret pattern, and bin by canonical position (lowest fretted fret;
/// 0 for all-opens). This is the "clean split" — each voicing belongs
/// to exactly one position rather than appearing in every window
/// whose range overlaps it.
fn voicings_by_position(
    fretboard: &Fretboard,
    chord: &music_theory::chord::ChordFormula,
    root: Pitch,
) -> BTreeMap<u8, Vec<ChordVoicing>> {
    let mut all: Vec<ChordVoicing> = Vec::new();
    for window_start in 0..=15u8 {
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
    // Fall back to AnyChordTone if no root-bass voicings are findable.
    if all.is_empty() {
        for window_start in 0..=15u8 {
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
    // Dedup by fret pattern.
    let mut seen: HashSet<Vec<Option<u8>>> = HashSet::new();
    all.retain(|v| seen.insert(v.fret_pattern()));

    // Collapse equivalent voicings: two voicings with the same set of
    // fretted positions (fret > 0) but different open/mute permutations
    // are functionally the same shape. Keep the one with the most
    // played strings (fullest chord) and drop the subsets.
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

    // Bin by canonical position.
    let mut by_pos: BTreeMap<u8, Vec<ChordVoicing>> = BTreeMap::new();
    for v in by_skeleton.into_values() {
        by_pos.entry(v.lowest_fretted_position()).or_default().push(v);
    }
    by_pos
}

/// Suggested finger assignments for a chord voicing's played strings.
/// Simple heuristic: sort unique fretted frets ascending, assign
/// fingers 1, 2, 3, 4 in order. Strings sharing a fret share a finger
/// (the basic barre case). Open strings get 0; muted gets None.
fn finger_assignments(voicing: &ChordVoicing) -> Vec<Option<u8>> {
    let mut unique_frets: Vec<u8> = voicing
        .strings
        .iter()
        .filter_map(|s| match s {
            StringPlay::Played { fret, .. } if *fret > 0 => Some(*fret),
            _ => None,
        })
        .collect();
    unique_frets.sort_unstable();
    unique_frets.dedup();
    let fret_to_finger: std::collections::HashMap<u8, u8> = unique_frets
        .iter()
        .enumerate()
        .map(|(i, &f)| (f, (i as u8 + 1).min(4)))
        .collect();

    voicing
        .strings
        .iter()
        .map(|s| match s {
            StringPlay::Played { fret, .. } if *fret > 0 => {
                fret_to_finger.get(fret).copied()
            }
            StringPlay::Played { fret: 0, .. } => Some(0),
            _ => None,
        })
        .collect()
}

fn tuning_names_for(instrument: Instrument) -> Vec<&'static str> {
    tuning_catalog()
        .iter()
        .filter(|s| s.instrument == instrument)
        .map(|s| s.name)
        .collect()
}

fn build_metronome_pattern(
    bpm: f32,
    num: u8,
    subdivision: Subdivision,
    click: ClickPattern,
    accent: AccentMode,
) -> SequencerPattern {
    let dpb = subdivision.divisions_per_beat as usize;
    let beats = num as usize;
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

fn pitch_to_pd_name(pitch: &Pitch) -> DetectedNoteName {
    ChromaticPc::from_pc(pitch.pitch_class()).to_detected()
}

/// Decompose a pitch into its sharps-spelled chromatic class plus the
/// octave that, combined, lands on the same MIDI number. Used to set
/// the dropdown state when a preset string-button is clicked.
fn pitch_to_chromatic_sharps(pitch: &Pitch) -> (ChromaticPc, i8) {
    let midi = pitch.midi();
    let pc = midi.rem_euclid(12) as u8;
    let octave = (midi.div_euclid(12) - 1) as i8;
    (ChromaticPc::from_pc(pc), octave)
}

fn main() -> iced::Result {
    iced::application("Guitar Toolkit", App::update, App::view)
        .subscription(App::subscription)
        .theme(|_| Theme::Dark)
        .window_size(Size::new(1100.0, 600.0))
        .run()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Tab {
    #[default]
    Scales,
    Chords,
    Tuner,
    Progressions,
    Exercises,
    Metronome,
    Practice,
}

struct App {
    tab: Tab,
    fretboard: Fretboard,
    scale_root: Pitch,
    scale_formula: &'static ScaleFormula,
    // Metronome state
    bpm: f32,
    metronome_playing: bool,
    metronome_time_sig_num: u8,
    metronome_subdivision: Subdivision,
    metronome_click: ClickPattern,
    metronome_accent: AccentMode,
    /// Editable mirror of `bpm` for the type-in input field.
    metronome_bpm_input: String,
    /// Recent tap-tempo timestamps (within a 2s window).
    metronome_tap_history: Vec<Instant>,
    /// Visual beat indicator counter (0..time_sig_num). Derived from
    /// elapsed time so it advances even when the audio happens to be
    /// muted by the OS or routed elsewhere.
    metronome_current_beat: u8,
    /// Seconds since the metronome started playing. Used to drive the
    /// visual beat indicator.
    metronome_elapsed_secs: f32,
    engine: Result<(SequencerEngine, EngineHandle), String>,
    // Tuner state. None = not listening; Some = mic active.
    tuner: Option<(TunerEngine, TunerHandle)>,
    tuner_error: Option<String>,
    tuner_latest: Option<DetectedNote>,
    tuner_level: f32,
    /// RMS threshold used by the silence gate. Live-tunable via the
    /// Sensitivity slider in the Tuner tab.
    tuner_threshold: f64,
    tuner_detector: DetectorKind,
    /// Optional target pitch (note name + octave). When set, the tuner
    /// uses hinted detection on the pitch class — bypasses harmonic
    /// confusion. The octave is for display and informational purposes;
    /// pitch-detector's hinted algorithm operates on note name only.
    tuner_target: Option<Pitch>,
    /// Dropdown state for the custom-target picker. Mirrors `tuner_target`
    /// when set, holds last-picked values otherwise.
    tuner_custom_pc: ChromaticPc,
    tuner_custom_octave: i8,
    selected_exercise: Option<usize>,
    selected_progression: Option<usize>,
    /// Tonic pitch class for progressions. Octave is fixed at 4
    /// internally (it doesn't change which chords the progression
    /// produces — only the absolute octave they'd play at).
    progression_key_pc: ChromaticPc,
    /// Derived from `progression_key_pc` + fixed octave 4. Cached so
    /// view code doesn't recompute on every render.
    progression_key_root: Pitch,
    /// Tonic pitch class for the scales view.
    scale_pc: ChromaticPc,
    /// Hand position: starting fret of the 5-fret window the scale
    /// is rendered in. 0 = open position (frets 0–4).
    scale_position: u8,
    /// What to label scale positions with on the fretboard.
    scale_label_mode: LabelMode,
    /// Cached scale-formula names for the scale picker dropdown.
    scale_names: Vec<&'static str>,

    chord_formula: &'static ChordFormula,
    chord_root: Pitch,
    chord_pc: ChromaticPc,
    chord_position: u8,
    chord_label_mode: LabelMode,
    /// Label mode for chord diagrams (in voicing cards and the
    /// progression chord-expansion diagram). Independent from the
    /// chord-tone fretboard's label mode above.
    diagram_label_mode: LabelMode,
    chord_names: Vec<&'static str>,

    /// Active tuning's instrument — drives the tuning name picker.
    active_instrument: Instrument,
    /// Active tuning's name. With `active_instrument` it uniquely keys
    /// into the tuning catalog.
    active_tuning_name: &'static str,
    /// Cached: tuning names available for `active_instrument`.
    available_tuning_names: Vec<&'static str>,
    /// Active scale-key for progressions (default: Major). When set
    /// to a minor mode, the catalog's roles map to the correct
    /// minor-key chords.
    progression_key_scale: &'static ScaleFormula,
    /// Diagram color palette — applies to fretboard dots, labels,
    /// and chord diagram markers across all visualization tabs.
    diagram_theme: DiagramTheme,

    // Practice mode (Fretwork) state.
    practice_sets: Vec<PracticeSet>,
    practice_set_names: Vec<String>,
    practice_selected_set: usize,
    practice_item_idx: usize,
    practice_playing: bool,
    practice_bpm: f32,
    practice_bars_per_item: u8,
    /// Wall-clock seconds elapsed in the current item, accumulated by
    /// the subscription tick. Resets on advance.
    practice_elapsed_secs: f32,
    /// Text-input buffer for BPM. Submitted on Enter to set practice_bpm.
    practice_bpm_input: String,
    /// Recent tap-tempo timestamps; cleared after 2s gap.
    practice_tap_history: Vec<Instant>,
    /// Audible click track during practice (drives the SequencerEngine
    /// at practice_bpm with a basic 4/4 quarter-note click).
    practice_click_enabled: bool,
    /// Current beat in the bar (0-based), driving the visual indicator.
    practice_current_beat: u8,
    /// Seconds since the click track was last (re)started. Drives the
    /// visual beat indicator independently of `practice_elapsed_secs`
    /// (which governs item progression and must not reset when the
    /// click toggles).
    practice_beat_phase_secs: f32,
    /// Cached: scale names that have enough degrees for the currently
    /// selected progression. Refreshed when progression changes.
    progression_valid_scale_names: Vec<&'static str>,

    /// Index into the current progression's chord list of the
    /// currently-expanded chord (showing diagram + voicing arrows).
    /// None = no chord expanded.
    progression_expanded_chord: Option<usize>,
    /// Index into `progression_available_positions`. Position arrows
    /// step this; voicing index resets when this changes.
    progression_position_idx: usize,
    /// Cached: positions (canonical, lowest-fretted-fret) that have at
    /// least one voicing for the currently expanded chord.
    progression_available_positions: Vec<u8>,
    /// Voicing index within the chosen position. Cycles via arrows.
    progression_voicing_idx: usize,
}

/// What to write inside each fretboard dot.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
enum LabelMode {
    None,
    Notes,
    Degrees,
    Fingers,
}

/// Color palette for fretboard / chord-diagram dots and labels.
/// The app's overall iced theme is fixed at Dark; this only governs
/// the visualization of musical content.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
enum DiagramTheme {
    Classic,
    HighContrast,
    Vivid,
    Pastel,
    Monochrome,
}

impl DiagramTheme {
    const ALL: [Self; 5] = [
        Self::Classic,
        Self::HighContrast,
        Self::Vivid,
        Self::Pastel,
        Self::Monochrome,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Classic => "Classic",
            Self::HighContrast => "High Contrast",
            Self::Vivid => "Vivid",
            Self::Pastel => "Pastel",
            Self::Monochrome => "Monochrome",
        }
    }

    fn colors(self) -> DiagramColors {
        match self {
            Self::Classic => DiagramColors {
                root_dot: Color::from_rgb(1.0, 0.6, 0.2),
                note_dot: Color::from_rgb(0.4, 0.6, 1.0),
                label_text: Color::from_rgb(0.08, 0.08, 0.10),
                open_marker: Color::from_rgb(0.5, 0.85, 0.5),
                muted_marker: Color::from_rgb(0.85, 0.5, 0.4),
            },
            Self::HighContrast => DiagramColors {
                root_dot: Color::from_rgb(1.0, 0.92, 0.0),
                note_dot: Color::from_rgb(0.95, 0.95, 0.95),
                label_text: Color::from_rgb(0.0, 0.0, 0.0),
                open_marker: Color::from_rgb(0.0, 0.95, 0.0),
                muted_marker: Color::from_rgb(1.0, 0.3, 0.3),
            },
            Self::Vivid => DiagramColors {
                root_dot: Color::from_rgb(0.95, 0.2, 0.35),
                note_dot: Color::from_rgb(0.25, 0.85, 0.85),
                label_text: Color::from_rgb(0.05, 0.05, 0.08),
                open_marker: Color::from_rgb(0.4, 0.95, 0.5),
                muted_marker: Color::from_rgb(1.0, 0.55, 0.3),
            },
            Self::Pastel => DiagramColors {
                root_dot: Color::from_rgb(0.95, 0.72, 0.78),
                note_dot: Color::from_rgb(0.72, 0.85, 0.95),
                label_text: Color::from_rgb(0.15, 0.15, 0.20),
                open_marker: Color::from_rgb(0.78, 0.92, 0.8),
                muted_marker: Color::from_rgb(0.95, 0.78, 0.7),
            },
            Self::Monochrome => DiagramColors {
                root_dot: Color::from_rgb(0.92, 0.92, 0.92),
                note_dot: Color::from_rgb(0.55, 0.55, 0.58),
                label_text: Color::from_rgb(0.08, 0.08, 0.10),
                open_marker: Color::from_rgb(0.78, 0.78, 0.78),
                muted_marker: Color::from_rgb(0.55, 0.55, 0.55),
            },
        }
    }
}

impl fmt::Display for DiagramTheme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Copy, Clone, Debug)]
struct DiagramColors {
    root_dot: Color,
    note_dot: Color,
    label_text: Color,
    open_marker: Color,
    muted_marker: Color,
}

/// Whether the metronome clicks on every beat or on every subdivision.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
enum ClickPattern {
    BeatOnly,
    EverySubdivision,
}

/// Accent placement on the metronome track.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
enum AccentMode {
    Downbeat,
    EveryBeat,
    None,
}

const TIME_SIG_NUMERATORS: [u8; 12] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];

const SUBDIVISIONS: [Subdivision; 6] = [
    Subdivision::QUARTER,
    Subdivision::EIGHTH,
    Subdivision::SIXTEENTH,
    Subdivision::THIRTY_SECOND,
    Subdivision::EIGHTH_TRIPLET,
    Subdivision::SIXTEENTH_TRIPLET,
];

fn subdivision_label(s: Subdivision) -> &'static str {
    if s == Subdivision::QUARTER {
        "1/4"
    } else if s == Subdivision::EIGHTH {
        "1/8"
    } else if s == Subdivision::SIXTEENTH {
        "1/16"
    } else if s == Subdivision::THIRTY_SECOND {
        "1/32"
    } else if s == Subdivision::EIGHTH_TRIPLET {
        "1/8 trip"
    } else if s == Subdivision::SIXTEENTH_TRIPLET {
        "1/16 trip"
    } else {
        "?"
    }
}

impl Default for App {
    fn default() -> Self {
        let tuning = Tuning::find_for("Standard", Instrument::Guitar)
            .expect("standard guitar tuning in catalog");
        let scale_formula = scale_catalog()
            .iter()
            .find(|s| s.name == "Major")
            .expect("major scale in catalog");
        let bpm = 120.0;
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
            fretboard: Fretboard::new(tuning, 22),
            scale_root: Pitch::natural(NoteName::C, 4),
            scale_formula,
            bpm,
            metronome_playing: false,
            metronome_time_sig_num: 4,
            metronome_subdivision: Subdivision::QUARTER,
            metronome_click: ClickPattern::BeatOnly,
            metronome_accent: AccentMode::Downbeat,
            metronome_bpm_input: format!("{:.0}", bpm),
            metronome_tap_history: Vec::new(),
            metronome_current_beat: 0,
            metronome_elapsed_secs: 0.0,
            engine,
            tuner: None,
            tuner_error: None,
            tuner_latest: None,
            tuner_level: 0.0,
            tuner_threshold: DEFAULT_SILENCE_RMS_THRESHOLD,
            tuner_detector: DetectorKind::Fft,
            tuner_target: None,
            tuner_custom_pc: ChromaticPc::A,
            tuner_custom_octave: 4,
            selected_exercise: Some(0),
            selected_progression: Some(0),
            progression_key_root: Pitch::natural(NoteName::C, 4),
            progression_key_pc: ChromaticPc::C,
            scale_pc: ChromaticPc::C,
            scale_position: 0,
            scale_label_mode: LabelMode::Notes,
            scale_names: scale_catalog().iter().map(|s| s.name).collect(),
            chord_formula: chord_catalog()
                .iter()
                .find(|c| c.name == "Major")
                .expect("major chord in catalog"),
            chord_root: Pitch::natural(NoteName::C, 4),
            chord_pc: ChromaticPc::C,
            chord_position: 0,
            chord_label_mode: LabelMode::Notes,
            diagram_label_mode: LabelMode::None,
            chord_names: chord_catalog().iter().map(|c| c.name).collect(),
            active_instrument: Instrument::Guitar,
            active_tuning_name: "Standard",
            available_tuning_names: tuning_names_for(Instrument::Guitar),
            progression_key_scale: scale_catalog()
                .iter()
                .find(|s| s.name == "Major")
                .expect("major scale in catalog"),
            diagram_theme: DiagramTheme::Classic,
            practice_sets: practice_catalog(),
            practice_set_names: practice_catalog()
                .into_iter()
                .map(|s| s.name)
                .collect(),
            practice_selected_set: 0,
            practice_item_idx: 0,
            practice_playing: false,
            practice_bpm: 60.0,
            practice_bars_per_item: 4,
            practice_elapsed_secs: 0.0,
            practice_bpm_input: "60".to_string(),
            practice_tap_history: Vec::new(),
            practice_click_enabled: true,
            practice_current_beat: 0,
            practice_beat_phase_secs: 0.0,
            progression_valid_scale_names: valid_key_scales_for_progression(
                &progression_catalog()[0],
            ),
            progression_expanded_chord: None,
            progression_position_idx: 0,
            progression_available_positions: Vec::new(),
            progression_voicing_idx: 0,
        }
    }
}

#[derive(Debug, Clone)]
enum Message {
    TabSelected(Tab),
    BpmChanged(f32),
    PlayMetronome,
    StopMetronome,
    MetronomeTimeSigChanged(u8),
    MetronomeSubdivisionChanged(Subdivision),
    MetronomeClickChanged(ClickPattern),
    MetronomeAccentChanged(AccentMode),
    MetronomeBpmInputChanged(String),
    MetronomeBpmInputSubmitted,
    MetronomeTap,
    MetronomeTick,
    StartTuner,
    StopTuner,
    TunerTick,
    TunerThresholdChanged(f32),
    TunerDetectorChanged(DetectorKind),
    TunerTargetSet(Option<Pitch>),
    TunerCustomPcChanged(ChromaticPc),
    TunerCustomOctaveChanged(i8),
    ExerciseSelected(usize),
    ProgressionSelected(usize),
    ProgressionKeyPcChanged(ChromaticPc),
    ScaleSelected(&'static str),
    ScalePcChanged(ChromaticPc),
    ScalePositionChanged(u8),
    ScaleLabelModeChanged(LabelMode),
    ChordSelected(&'static str),
    ChordPcChanged(ChromaticPc),
    ChordPositionChanged(u8),
    ChordLabelModeChanged(LabelMode),
    DiagramLabelModeChanged(LabelMode),
    DiagramThemeChanged(DiagramTheme),
    PracticeSetSelected(usize),
    PracticePlay,
    PracticePause,
    PracticeNext,
    PracticePrev,
    PracticeBpmChanged(f32),
    PracticeBpmInputChanged(String),
    PracticeBpmInputSubmitted,
    PracticeTap,
    PracticeClickToggled(bool),
    PracticeBarsChanged(u8),
    PracticeTick,
    InstrumentChanged(Instrument),
    TuningSelected(&'static str),
    ProgressionScaleSelected(&'static str),
    ProgressionChordExpanded(usize),
    ProgressionChordVoicingPrev,
    ProgressionChordVoicingNext,
    ProgressionChordPositionPrev,
    ProgressionChordPositionNext,
}

impl App {
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::TabSelected(t) => self.tab = t,
            Message::BpmChanged(bpm) => {
                self.bpm = bpm;
                self.metronome_bpm_input = format!("{:.0}", bpm);
                if let Ok((_, handle)) = &self.engine {
                    handle.set_bpm(bpm);
                }
            }
            Message::PlayMetronome => {
                if let Ok((_, handle)) = &self.engine {
                    handle.play();
                    self.metronome_playing = true;
                    self.metronome_elapsed_secs = 0.0;
                    self.metronome_current_beat = 0;
                }
            }
            Message::StopMetronome => {
                if let Ok((_, handle)) = &self.engine {
                    handle.stop();
                    self.metronome_playing = false;
                }
            }
            Message::MetronomeBpmInputChanged(s) => {
                self.metronome_bpm_input = s;
            }
            Message::MetronomeBpmInputSubmitted => {
                if let Ok(parsed) = self.metronome_bpm_input.trim().parse::<f32>() {
                    let bpm = parsed.clamp(40.0, 240.0);
                    self.bpm = bpm;
                    self.metronome_bpm_input = format!("{:.0}", bpm);
                    if let Ok((_, h)) = &self.engine {
                        h.set_bpm(bpm);
                    }
                } else {
                    self.metronome_bpm_input = format!("{:.0}", self.bpm);
                }
            }
            Message::MetronomeTap => {
                let now = Instant::now();
                self.metronome_tap_history
                    .retain(|t| now.duration_since(*t).as_secs_f32() < 2.0);
                self.metronome_tap_history.push(now);
                if self.metronome_tap_history.len() >= 2 {
                    let intervals: Vec<f32> = self
                        .metronome_tap_history
                        .windows(2)
                        .map(|w| w[1].duration_since(w[0]).as_secs_f32())
                        .collect();
                    let avg = intervals.iter().sum::<f32>() / intervals.len() as f32;
                    if avg > 0.001 {
                        let bpm = (60.0 / avg).clamp(40.0, 240.0);
                        self.bpm = bpm;
                        self.metronome_bpm_input = format!("{:.0}", bpm);
                        if let Ok((_, h)) = &self.engine {
                            h.set_bpm(bpm);
                        }
                    }
                }
            }
            Message::MetronomeTick => {
                if self.metronome_playing {
                    self.metronome_elapsed_secs += 0.05;
                    let secs_per_beat = 60.0 / self.bpm.max(1.0);
                    let beats = self.metronome_time_sig_num.max(1) as u32;
                    let beat = (self.metronome_elapsed_secs / secs_per_beat)
                        .floor() as u32
                        % beats;
                    self.metronome_current_beat = beat as u8;
                }
            }
            Message::MetronomeTimeSigChanged(n) => {
                self.metronome_time_sig_num = n;
                self.apply_metronome_pattern();
            }
            Message::MetronomeSubdivisionChanged(s) => {
                self.metronome_subdivision = s;
                self.apply_metronome_pattern();
            }
            Message::MetronomeClickChanged(c) => {
                self.metronome_click = c;
                self.apply_metronome_pattern();
            }
            Message::MetronomeAccentChanged(a) => {
                self.metronome_accent = a;
                self.apply_metronome_pattern();
            }
            Message::StartTuner => {
                self.tuner_error = None;
                match TunerEngine::new() {
                    Ok(eng) => {
                        let h = eng.handle();
                        // Apply user-configured threshold and detector
                        // to the new engine.
                        h.set_threshold(self.tuner_threshold);
                        h.set_detector_kind(self.tuner_detector);
                        h.set_target_hint(
                            self.tuner_target.as_ref().map(pitch_to_pd_name),
                        );
                        self.tuner = Some((eng, h));
                    }
                    Err(e) => self.tuner_error = Some(e.to_string()),
                }
            }
            Message::TunerThresholdChanged(val) => {
                self.tuner_threshold = val as f64;
                if let Some((_, handle)) = &self.tuner {
                    handle.set_threshold(self.tuner_threshold);
                }
            }
            Message::TunerDetectorChanged(kind) => {
                self.tuner_detector = kind;
                if let Some((_, handle)) = &self.tuner {
                    handle.set_detector_kind(kind);
                }
            }
            Message::TunerTargetSet(target) => {
                if let Some(p) = &target {
                    let (pc, oct) = pitch_to_chromatic_sharps(p);
                    self.tuner_custom_pc = pc;
                    self.tuner_custom_octave = oct.clamp(1, 8);
                }
                self.tuner_target = target;
                if let Some((_, handle)) = &self.tuner {
                    handle.set_target_hint(target.as_ref().map(pitch_to_pd_name));
                }
            }
            Message::TunerCustomPcChanged(pc) => {
                self.tuner_custom_pc = pc;
                let target = pc.to_pitch(self.tuner_custom_octave);
                self.tuner_target = Some(target);
                if let Some((_, handle)) = &self.tuner {
                    handle.set_target_hint(Some(pitch_to_pd_name(&target)));
                }
            }
            Message::TunerCustomOctaveChanged(oct) => {
                self.tuner_custom_octave = oct;
                let target = self.tuner_custom_pc.to_pitch(oct);
                self.tuner_target = Some(target);
                if let Some((_, handle)) = &self.tuner {
                    handle.set_target_hint(Some(pitch_to_pd_name(&target)));
                }
            }
            Message::ExerciseSelected(idx) => {
                self.selected_exercise = Some(idx);
            }
            Message::ProgressionSelected(idx) => {
                self.selected_progression = Some(idx);
                let prog = &progression_catalog()[idx];
                self.progression_valid_scale_names =
                    valid_key_scales_for_progression(prog);
                if !self
                    .progression_valid_scale_names
                    .contains(&self.progression_key_scale.name)
                {
                    if let Some(major) =
                        scale_catalog().iter().find(|s| s.name == "Major")
                    {
                        self.progression_key_scale = major;
                    }
                }
                self.progression_expanded_chord = None;
                self.progression_position_idx = 0;
                self.progression_voicing_idx = 0;
                self.progression_available_positions.clear();
            }
            Message::ProgressionKeyPcChanged(pc) => {
                self.progression_key_pc = pc;
                self.progression_key_root = pc.to_pitch(4);
                self.refresh_progression_voicings();
            }
            Message::ScaleSelected(name) => {
                if let Some(f) = scale_catalog().iter().find(|s| s.name == name) {
                    self.scale_formula = f;
                }
                // Defensive: re-set scale_root to its current value to
                // ensure the render pipeline picks up the formula change.
                // Reported case: changing only the scale dropdown didn't
                // visually update until the root changed too.
                self.scale_root = self.scale_pc.to_pitch(4);
            }
            Message::ScalePcChanged(pc) => {
                self.scale_pc = pc;
                self.scale_root = pc.to_pitch(4);
            }
            Message::ScalePositionChanged(p) => {
                self.scale_position = p;
            }
            Message::ScaleLabelModeChanged(m) => {
                self.scale_label_mode = m;
            }
            Message::ChordSelected(name) => {
                if let Some(f) = chord_catalog().iter().find(|c| c.name == name) {
                    self.chord_formula = f;
                }
                self.chord_root = self.chord_pc.to_pitch(4);
            }
            Message::ChordPcChanged(pc) => {
                self.chord_pc = pc;
                self.chord_root = pc.to_pitch(4);
            }
            Message::ChordPositionChanged(p) => {
                self.chord_position = p;
            }
            Message::ChordLabelModeChanged(m) => {
                self.chord_label_mode = m;
            }
            Message::DiagramLabelModeChanged(m) => {
                self.diagram_label_mode = m;
            }
            Message::DiagramThemeChanged(t) => {
                self.diagram_theme = t;
            }
            Message::PracticeSetSelected(idx) => {
                if idx < self.practice_sets.len() {
                    self.practice_selected_set = idx;
                    self.practice_item_idx = 0;
                    self.practice_elapsed_secs = 0.0;
                }
            }
            Message::PracticePlay => {
                if !self.practice_sets.is_empty() {
                    self.practice_playing = true;
                    self.practice_elapsed_secs = 0.0;
                    self.practice_beat_phase_secs = 0.0;
                    self.practice_current_beat = 0;
                    self.start_practice_click();
                }
            }
            Message::PracticePause => {
                self.practice_playing = false;
                if let Ok((_, h)) = &self.engine {
                    h.stop();
                }
            }
            Message::PracticeNext => {
                let count = self
                    .practice_sets
                    .get(self.practice_selected_set)
                    .map(|s| s.items.len())
                    .unwrap_or(0);
                if count > 0 {
                    self.practice_item_idx = (self.practice_item_idx + 1) % count;
                }
                self.practice_elapsed_secs = 0.0;
            }
            Message::PracticePrev => {
                let count = self
                    .practice_sets
                    .get(self.practice_selected_set)
                    .map(|s| s.items.len())
                    .unwrap_or(0);
                if count > 0 {
                    self.practice_item_idx =
                        (self.practice_item_idx + count - 1) % count;
                }
                self.practice_elapsed_secs = 0.0;
            }
            Message::PracticeBpmChanged(b) => {
                self.practice_bpm = b;
                self.practice_bpm_input = format!("{:.0}", b);
                if self.practice_playing && self.practice_click_enabled {
                    if let Ok((_, h)) = &self.engine {
                        h.set_bpm(b);
                    }
                }
            }
            Message::PracticeBpmInputChanged(s) => {
                self.practice_bpm_input = s;
            }
            Message::PracticeBpmInputSubmitted => {
                if let Ok(parsed) = self.practice_bpm_input.trim().parse::<f32>() {
                    let bpm = parsed.clamp(40.0, 240.0);
                    self.practice_bpm = bpm;
                    self.practice_bpm_input = format!("{:.0}", bpm);
                    if self.practice_playing && self.practice_click_enabled {
                        if let Ok((_, h)) = &self.engine {
                            h.set_bpm(bpm);
                        }
                    }
                } else {
                    // Invalid input — restore from current bpm.
                    self.practice_bpm_input = format!("{:.0}", self.practice_bpm);
                }
            }
            Message::PracticeTap => {
                let now = Instant::now();
                // Drop taps older than 2s — start a new tap session.
                self.practice_tap_history
                    .retain(|t| now.duration_since(*t).as_secs_f32() < 2.0);
                self.practice_tap_history.push(now);
                if self.practice_tap_history.len() >= 2 {
                    let intervals: Vec<f32> = self
                        .practice_tap_history
                        .windows(2)
                        .map(|w| w[1].duration_since(w[0]).as_secs_f32())
                        .collect();
                    let avg = intervals.iter().sum::<f32>() / intervals.len() as f32;
                    if avg > 0.001 {
                        let bpm = (60.0 / avg).clamp(40.0, 240.0);
                        self.practice_bpm = bpm;
                        self.practice_bpm_input = format!("{:.0}", bpm);
                        if self.practice_playing && self.practice_click_enabled {
                            if let Ok((_, h)) = &self.engine {
                                h.set_bpm(bpm);
                            }
                        }
                    }
                }
            }
            Message::PracticeClickToggled(on) => {
                self.practice_click_enabled = on;
                if self.practice_playing {
                    if on {
                        // Engine restart — realign the visual phase so
                        // the audio downbeat coincides with beat 1 of
                        // the indicator.
                        self.practice_beat_phase_secs = 0.0;
                        self.practice_current_beat = 0;
                        self.start_practice_click();
                    } else if let Ok((_, h)) = &self.engine {
                        h.stop();
                    }
                }
            }
            Message::PracticeBarsChanged(n) => {
                self.practice_bars_per_item = n;
            }
            Message::PracticeTick => {
                if self.practice_playing {
                    self.practice_elapsed_secs += 0.05;
                    let secs_per_item = 60.0 / self.practice_bpm
                        * 4.0
                        * self.practice_bars_per_item as f32;
                    if self.practice_elapsed_secs >= secs_per_item {
                        let count = self
                            .practice_sets
                            .get(self.practice_selected_set)
                            .map(|s| s.items.len())
                            .unwrap_or(0);
                        if count > 0 {
                            self.practice_item_idx =
                                (self.practice_item_idx + 1) % count;
                        }
                        self.practice_elapsed_secs = 0.0;
                    }
                    // Drive the visual beat from a separate phase
                    // counter so it (a) keeps moving when the click is
                    // muted, and (b) realigns with the audio whenever
                    // we restart the engine.
                    self.practice_beat_phase_secs += 0.05;
                    let secs_per_beat = 60.0 / self.practice_bpm.max(1.0);
                    let beats_per_bar = 4u8;
                    let beat = (self.practice_beat_phase_secs / secs_per_beat)
                        .floor() as u32
                        % beats_per_bar as u32;
                    self.practice_current_beat = beat as u8;
                }
            }
            Message::InstrumentChanged(inst) => {
                self.active_instrument = inst;
                self.available_tuning_names = tuning_names_for(inst);
                if let Some(spec) =
                    tuning_catalog().iter().find(|s| s.instrument == inst)
                {
                    self.active_tuning_name = spec.name;
                    self.fretboard = Fretboard::new(
                        Tuning::from_spec(spec),
                        self.fretboard.fret_count,
                    );
                }
                self.refresh_progression_voicings();
            }
            Message::TuningSelected(name) => {
                if let Some(spec) = tuning_catalog().iter().find(|s| {
                    s.name == name && s.instrument == self.active_instrument
                }) {
                    self.active_tuning_name = name;
                    self.fretboard = Fretboard::new(
                        Tuning::from_spec(spec),
                        self.fretboard.fret_count,
                    );
                }
                self.refresh_progression_voicings();
            }
            Message::ProgressionScaleSelected(name) => {
                if let Some(s) = scale_catalog().iter().find(|s| s.name == name) {
                    self.progression_key_scale = s;
                }
                self.refresh_progression_voicings();
            }
            Message::ProgressionChordExpanded(idx) => {
                if self.progression_expanded_chord == Some(idx) {
                    self.progression_expanded_chord = None;
                    self.progression_available_positions.clear();
                } else {
                    self.progression_expanded_chord = Some(idx);
                    self.progression_position_idx = 0;
                    self.progression_voicing_idx = 0;
                    self.refresh_progression_voicings();
                }
            }
            Message::ProgressionChordVoicingPrev => {
                if self.progression_voicing_idx > 0 {
                    self.progression_voicing_idx -= 1;
                }
            }
            Message::ProgressionChordVoicingNext => {
                self.progression_voicing_idx =
                    self.progression_voicing_idx.saturating_add(1);
            }
            Message::ProgressionChordPositionPrev => {
                if self.progression_position_idx > 0 {
                    self.progression_position_idx -= 1;
                    self.progression_voicing_idx = 0;
                }
            }
            Message::ProgressionChordPositionNext => {
                let max = self
                    .progression_available_positions
                    .len()
                    .saturating_sub(1);
                if self.progression_position_idx < max {
                    self.progression_position_idx += 1;
                    self.progression_voicing_idx = 0;
                }
            }
            Message::StopTuner => {
                self.tuner = None;
                self.tuner_latest = None;
            }
            Message::TunerTick => {
                if let Some((_, handle)) = &self.tuner {
                    let snap = handle.snapshot();
                    self.tuner_latest = snap.note;
                    self.tuner_level = snap.input_level;
                }
            }
        }
        Task::none()
    }

    /// Start the metronome click track at the practice tempo. Called
    /// when practice playback begins or when the click toggle is
    /// re-enabled mid-session.
    fn start_practice_click(&self) {
        if !self.practice_click_enabled {
            return;
        }
        if let Ok((_, h)) = &self.engine {
            let pattern = build_metronome_pattern(
                self.practice_bpm,
                4,
                Subdivision::QUARTER,
                ClickPattern::BeatOnly,
                AccentMode::Downbeat,
            );
            h.set_pattern(pattern);
            h.play();
        }
    }

    /// Refresh the cached list of available canonical positions for
    /// the currently-expanded progression chord. Called from any
    /// handler that affects chord identity (chord-expand, key-pc,
    /// scale-mode, tuning, instrument).
    fn refresh_progression_voicings(&mut self) {
        self.progression_available_positions.clear();
        let Some(chord_idx) = self.progression_expanded_chord else { return };
        let Some(prog_idx) = self.selected_progression else { return };
        let prog = &progression_catalog()[prog_idx];
        let Ok(chords) =
            prog.apply_in_key(self.progression_key_root, self.progression_key_scale)
        else {
            return;
        };
        let Some(chord) = chords.get(chord_idx) else { return };
        let by_pos = voicings_by_position(
            &self.fretboard,
            chord.formula,
            chord.root,
        );
        self.progression_available_positions = by_pos.keys().copied().collect();
        // Clamp index to new bounds.
        if self.progression_available_positions.is_empty() {
            self.progression_position_idx = 0;
        } else {
            self.progression_position_idx = self
                .progression_position_idx
                .min(self.progression_available_positions.len() - 1);
        }
        self.progression_voicing_idx = 0;
    }

    /// Rebuild the metronome pattern from current settings and push it
    /// to the engine. If currently playing, the pattern restarts from
    /// beat 1 (set_pattern resets sample/step position) — so we also
    /// reset the visual phase to keep the indicator aligned with the
    /// audio downbeat.
    fn apply_metronome_pattern(&mut self) {
        if let Ok((_, handle)) = &self.engine {
            let pattern = build_metronome_pattern(
                self.bpm,
                self.metronome_time_sig_num,
                self.metronome_subdivision,
                self.metronome_click,
                self.metronome_accent,
            );
            handle.set_pattern(pattern);
            // set_pattern resets playing=false implicitly (no, actually
            // it doesn't change `playing`). Re-engage if we were running.
            if self.metronome_playing {
                handle.play();
            }
        }
        self.metronome_elapsed_secs = 0.0;
        self.metronome_current_beat = 0;
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subs: Vec<Subscription<Message>> = Vec::new();
        if self.tuner.is_some() {
            subs.push(
                iced::time::every(Duration::from_millis(50)).map(|_| Message::TunerTick),
            );
        }
        if self.practice_playing {
            subs.push(
                iced::time::every(Duration::from_millis(50))
                    .map(|_| Message::PracticeTick),
            );
        }
        if self.metronome_playing {
            subs.push(
                iced::time::every(Duration::from_millis(50))
                    .map(|_| Message::MetronomeTick),
            );
        }
        Subscription::batch(subs)
    }

    fn view(&self) -> Element<'_, Message> {
        // Tabs wrap to a second row when the window is too narrow.
        let tabs = row![
            tab_btn("Scales", Tab::Scales, self.tab),
            tab_btn("Chords", Tab::Chords, self.tab),
            tab_btn("Tuner", Tab::Tuner, self.tab),
            tab_btn("Progressions", Tab::Progressions, self.tab),
            tab_btn("Exercises", Tab::Exercises, self.tab),
            tab_btn("Metronome", Tab::Metronome, self.tab),
            tab_btn("Practice", Tab::Practice, self.tab),
        ]
        .spacing(8)
        .wrap();

        // Global tuning picker — affects every fretboard-rendering tab
        // (Scales, Chords, Exercises) and the Tuner's string-targets row.
        let tuning_row = row![
            text("Instrument:").size(13),
            pick_list(
                &Instrument::ALL[..],
                Some(self.active_instrument),
                Message::InstrumentChanged,
            )
            .text_size(13),
            text("Tuning:").size(13),
            pick_list(
                &self.available_tuning_names[..],
                Some(self.active_tuning_name),
                Message::TuningSelected,
            )
            .text_size(13),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);

        let content: Element<Message> = match self.tab {
            Tab::Scales => self.scales_view(),
            Tab::Chords => self.chords_view(),
            Tab::Tuner => self.tuner_view(),
            Tab::Progressions => self.progressions_view(),
            Tab::Exercises => self.exercises_view(),
            Tab::Metronome => self.metronome_view(),
            Tab::Practice => self.practice_view(),
        };

        let theme_row = row![
            text("Theme:").size(13),
            pick_list(
                &DiagramTheme::ALL[..],
                Some(self.diagram_theme),
                Message::DiagramThemeChanged,
            )
            .text_size(13),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);

        // Tuning only meaningfully drives Scales / Chords / Tuner.
        // Progressions, Exercises, and Metronome are tuning-agnostic.
        let show_tuning = matches!(self.tab, Tab::Scales | Tab::Chords | Tab::Tuner);

        // Header is a fixed column at the top; content scrolls below
        // so narrow / short windows don't clip the voicing grid or
        // any other below-the-fold material.
        let mut header = column![tabs, theme_row].spacing(12);
        if show_tuning {
            header = header.push(tuning_row);
        }

        // Only the dense / vertically-tall tabs need a scrollable —
        // Tuner and Metronome have compact content that centers in the
        // available space and would conflict with scrollable's
        // restriction on Length::Fill children.
        let uses_scroll = matches!(
            self.tab,
            Tab::Scales
                | Tab::Chords
                | Tab::Progressions
                | Tab::Exercises
                | Tab::Practice
        );
        let final_content: Element<Message> = if uses_scroll {
            scrollable(content)
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        } else {
            content
        };

        column![header, final_content]
            .spacing(16)
            .padding(16)
            .into()
    }

    fn scales_view(&self) -> Element<'_, Message> {
        let all_positions = self
            .fretboard
            .positions_for_scale(self.scale_formula, self.scale_root)
            .unwrap_or_default();

        // Filter to a single 5-fret hand window starting at scale_position.
        // Open position (0) includes fret 0 plus frets 1–4.
        let window_start = self.scale_position;
        let window_end = window_start + 4;
        let positions: Vec<Position> = all_positions
            .into_iter()
            .filter(|p| p.fret >= window_start && p.fret <= window_end)
            .collect();

        // Compute labels per position based on mode.
        let labels: Vec<String> = positions
            .iter()
            .map(|p| match self.scale_label_mode {
                LabelMode::None => String::new(),
                LabelMode::Notes => format!(
                    "{}{}",
                    p.pitch.name,
                    accidental_str(p.pitch.accidental)
                ),
                LabelMode::Degrees => p
                    .interval_from_root
                    .map(|iv| iv.number().to_string())
                    .unwrap_or_default(),
                LabelMode::Fingers => {
                    if p.fret == 0 {
                        "0".to_string()
                    } else {
                        // In open position (window_start = 0), finger 1
                        // covers fret 1, not fret 0 — fret 0 is the
                        // open string. Effective first-finger fret is
                        // max(window_start, 1).
                        let effective_start = window_start.max(1);
                        let f = p.fret.saturating_sub(effective_start) + 1;
                        f.clamp(1, 4).to_string()
                    }
                }
            })
            .collect();

        let header = text(format!(
            "{}{} {}  ·  {}",
            self.scale_root.name,
            accidental_str(self.scale_root.accidental),
            self.scale_formula.name,
            self.fretboard.tuning.name,
        ))
        .size(20);

        let picker_row = row![
            text("Scale:").size(13),
            pick_list(
                &self.scale_names[..],
                Some(self.scale_formula.name),
                Message::ScaleSelected,
            )
            .text_size(13),
            text("Root:").size(13),
            pick_list(
                &ChromaticPc::ALL[..],
                Some(self.scale_pc),
                Message::ScalePcChanged,
            )
            .text_size(13),
            text("Position:").size(13),
            pick_list(
                &SCALE_POSITIONS[..],
                Some(self.scale_position),
                Message::ScalePositionChanged,
            )
            .text_size(13),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);

        let label_row = row![
            text("Labels:").size(13),
            label_btn("Off", LabelMode::None, self.scale_label_mode),
            label_btn("Notes", LabelMode::Notes, self.scale_label_mode),
            label_btn("Degrees", LabelMode::Degrees, self.scale_label_mode),
            label_btn("Fingers", LabelMode::Fingers, self.scale_label_mode),
        ]
        .spacing(6)
        .align_y(iced::Alignment::Center);

        let canvas_widget = Canvas::new(FretboardCanvas {
            fretboard: self.fretboard.clone(),
            positions,
            labels,
            colors: self.diagram_theme.colors(),
        })
        .width(Length::Fill)
        .height(Length::Fixed(280.0));

        column![header, picker_row, label_row, canvas_widget]
            .spacing(12)
            .into()
    }

    fn chords_view(&self) -> Element<'_, Message> {
        // Compute chord pitches and positions for the active chord/root.
        let all_positions = self
            .fretboard
            .positions_for_chord(self.chord_formula, self.chord_root)
            .unwrap_or_default();

        let window_start = self.chord_position;
        let window_end = window_start + 4;
        let positions: Vec<Position> = all_positions
            .into_iter()
            .filter(|p| p.fret >= window_start && p.fret <= window_end)
            .collect();

        let labels: Vec<String> = positions
            .iter()
            .map(|p| match self.chord_label_mode {
                LabelMode::None => String::new(),
                LabelMode::Notes => format!(
                    "{}{}",
                    p.pitch.name,
                    accidental_str(p.pitch.accidental)
                ),
                LabelMode::Degrees => p
                    .interval_from_root
                    .map(|iv| iv.number().to_string())
                    .unwrap_or_default(),
                LabelMode::Fingers => {
                    // Position-relative fingering. Same semantics as
                    // scales: open position uses fret 1 = finger 1
                    // (fret 0 stays "0" for open string).
                    if p.fret == 0 {
                        "0".to_string()
                    } else {
                        let effective_start = window_start.max(1);
                        let f = p.fret.saturating_sub(effective_start) + 1;
                        f.clamp(1, 4).to_string()
                    }
                }
            })
            .collect();

        // Header: build the chord symbol like "Cmaj7" using the formula's
        // symbol field plus the root's spelling.
        let root_label = format!(
            "{}{}",
            self.chord_root.name,
            accidental_str(self.chord_root.accidental)
        );
        let symbol = if self.chord_formula.symbol.is_empty() {
            root_label.clone()
        } else {
            format!("{}{}", root_label, self.chord_formula.symbol)
        };
        let header = text(format!(
            "{}  ·  {}  ·  {}",
            symbol, self.chord_formula.name, self.fretboard.tuning.name
        ))
        .size(20);

        // Show the chord's pitches as a quick reference.
        let chord_pitches = self
            .chord_formula
            .apply_to(self.chord_root)
            .unwrap_or_default();
        let pitch_list = chord_pitches
            .iter()
            .map(|p| format!("{}{}", p.name, accidental_str(p.accidental)))
            .collect::<Vec<_>>()
            .join(" ");
        let pitches_text = text(pitch_list).size(13);

        let picker_row = row![
            text("Chord:").size(13),
            pick_list(
                &self.chord_names[..],
                Some(self.chord_formula.name),
                Message::ChordSelected,
            )
            .text_size(13),
            text("Root:").size(13),
            pick_list(
                &ChromaticPc::ALL[..],
                Some(self.chord_pc),
                Message::ChordPcChanged,
            )
            .text_size(13),
            text("Position:").size(13),
            pick_list(
                &SCALE_POSITIONS[..],
                Some(self.chord_position),
                Message::ChordPositionChanged,
            )
            .text_size(13),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);

        // Chord-specific label modes — Fingers doesn't apply to a chord-tone
        // map (no canonical fingering across the whole window).
        let label_row = row![
            text("Labels:").size(13),
            chord_label_btn("Off", LabelMode::None, self.chord_label_mode),
            chord_label_btn("Notes", LabelMode::Notes, self.chord_label_mode),
            chord_label_btn("Degrees", LabelMode::Degrees, self.chord_label_mode),
            chord_label_btn("Fingers", LabelMode::Fingers, self.chord_label_mode),
        ]
        .spacing(6)
        .align_y(iced::Alignment::Center);

        let canvas_widget = Canvas::new(FretboardCanvas {
            fretboard: self.fretboard.clone(),
            positions,
            labels,
            colors: self.diagram_theme.colors(),
        })
        .width(Length::Fill)
        .height(Length::Fixed(220.0));

        // Voicing card grid: bin all voicings by canonical position,
        // then show only the ones at the user's selected position. Each
        // voicing belongs to exactly one position (no cross-position
        // duplicates).
        let by_pos = voicings_by_position(
            &self.fretboard,
            self.chord_formula,
            self.chord_root,
        );
        let voicings_here: Vec<ChordVoicing> = by_pos
            .get(&self.chord_position)
            .cloned()
            .unwrap_or_default();

        // Show available positions next to the heading so users know
        // where the shapes live.
        let avail_positions: Vec<String> = by_pos
            .keys()
            .map(|p| if *p == 0 { "Open".to_string() } else { p.to_string() })
            .collect();
        let positions_summary = if avail_positions.is_empty() {
            "no positions".to_string()
        } else {
            format!("available: {}", avail_positions.join(", "))
        };

        let cards: Element<Message> = if voicings_here.is_empty() {
            let pos_label = if self.chord_position == 0 {
                "open position".to_string()
            } else {
                format!("position {}", self.chord_position)
            };
            text(format!("No voicings at {pos_label} ({positions_summary})"))
                .size(13)
                .into()
        } else {
            let mut grid = row![].spacing(10);
            for v in voicings_here.iter().take(8) {
                let label_text = format!(
                    "{}{}{}",
                    self.chord_root.name,
                    accidental_str(self.chord_root.accidental),
                    self.chord_formula.symbol
                );
                let card = column![
                    Canvas::new(
                        ChordDiagram::from_voicing(
                            v.clone(),
                            self.diagram_theme.colors(),
                        )
                        .with_labels(self.diagram_label_mode)
                    )
                    .width(Length::Fixed(85.0))
                    .height(Length::Fixed(175.0)),
                    text(label_text).size(12),
                ]
                .spacing(4)
                .align_x(iced::Alignment::Center);
                grid = grid.push(card);
            }
            grid.wrap().into()
        };

        let diagram_label_row = row![
            text("Diagram labels:").size(13),
            diagram_label_btn("Off", LabelMode::None, self.diagram_label_mode),
            diagram_label_btn("Notes", LabelMode::Notes, self.diagram_label_mode),
            diagram_label_btn("Degrees", LabelMode::Degrees, self.diagram_label_mode),
            diagram_label_btn("Fingers", LabelMode::Fingers, self.diagram_label_mode),
        ]
        .spacing(6)
        .align_y(iced::Alignment::Center);

        column![
            header,
            pitches_text,
            picker_row,
            label_row,
            canvas_widget,
            text(format!("Voicings ({positions_summary})")).size(15),
            diagram_label_row,
            cards,
        ]
        .spacing(12)
        .into()
    }

    fn practice_view(&self) -> Element<'_, Message> {
        if self.practice_sets.is_empty() {
            return text("No practice sets available.").size(16).into();
        }

        let set_idx = self
            .practice_selected_set
            .min(self.practice_sets.len() - 1);
        let set = &self.practice_sets[set_idx];
        let item_idx = self.practice_item_idx.min(set.items.len().saturating_sub(1));
        let current = &set.items[item_idx];

        // Set picker.
        let picker_row = row![
            text("Set:").size(13),
            pick_list(
                &self.practice_set_names[..],
                Some(self.practice_set_names[set_idx].clone()),
                |name| {
                    let idx = practice_catalog()
                        .iter()
                        .position(|s| s.name == name)
                        .unwrap_or(0);
                    Message::PracticeSetSelected(idx)
                },
            )
            .text_size(13),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);

        // Tempo controls — slider + numeric text input + tap-tempo button.
        let bpm_input = text_input("BPM", &self.practice_bpm_input)
            .on_input(Message::PracticeBpmInputChanged)
            .on_submit(Message::PracticeBpmInputSubmitted)
            .width(Length::Fixed(60.0))
            .size(14);

        let tap_btn = button(text("Tap").size(14))
            .on_press(Message::PracticeTap)
            .padding([6, 16])
            .style(button::secondary);

        let click_btn = if self.practice_click_enabled {
            button(text("Click ●").size(13))
                .on_press(Message::PracticeClickToggled(false))
                .style(button::primary)
                .padding([4, 12])
        } else {
            button(text("Click ○").size(13))
                .on_press(Message::PracticeClickToggled(true))
                .style(button::secondary)
                .padding([4, 12])
        };

        let tempo_row = row![
            bpm_input,
            text("BPM").size(13),
            slider(40.0..=240.0, self.practice_bpm, Message::PracticeBpmChanged)
                .step(1.0)
                .width(Length::Fixed(180.0)),
            tap_btn,
            click_btn,
            text("Bars per item:").size(13),
            pick_list(
                &[1u8, 2, 4, 8, 16][..],
                Some(self.practice_bars_per_item),
                Message::PracticeBarsChanged,
            )
            .text_size(13),
        ]
        .spacing(10)
        .align_y(iced::Alignment::Center);

        // Visual beat indicator — row of filled/hollow circles, current
        // beat highlighted. Driven by the engine's reported step.
        let beats_per_bar = 4u8;
        let current_beat = if self.practice_playing {
            self.practice_current_beat % beats_per_bar
        } else {
            beats_per_bar // out of range = no highlight
        };
        let beat_glyphs: String = (0..beats_per_bar)
            .map(|i| if i == current_beat { "● " } else { "○ " })
            .collect();
        let beat_color = if self.practice_playing {
            self.diagram_theme.colors().root_dot
        } else {
            Color::from_rgb(0.4, 0.4, 0.4)
        };
        let beat_indicator: Element<Message> =
            text(beat_glyphs).size(28).color(beat_color).into();

        // Transport controls.
        let prev_btn = button(text("◀◀ Prev").size(14))
            .on_press(Message::PracticePrev)
            .padding([6, 14])
            .style(button::secondary);
        let play_btn = if self.practice_playing {
            button(text("Pause").size(14))
                .on_press(Message::PracticePause)
                .padding([6, 16])
                .style(button::danger)
        } else {
            button(text("Play").size(14))
                .on_press(Message::PracticePlay)
                .padding([6, 16])
                .style(button::primary)
        };
        let next_btn = button(text("Next ▶▶").size(14))
            .on_press(Message::PracticeNext)
            .padding([6, 14])
            .style(button::secondary);

        let transport = row![prev_btn, play_btn, next_btn]
            .spacing(8)
            .align_y(iced::Alignment::Center);

        // Current item display.
        let item_label = text(current.label()).size(36);
        let progress_label = {
            let secs_per_item = 60.0 / self.practice_bpm
                * 4.0
                * self.practice_bars_per_item as f32;
            let bar_now = ((self.practice_elapsed_secs / (secs_per_item / self.practice_bars_per_item as f32))
                .floor() as u32
                + 1)
                .min(self.practice_bars_per_item as u32);
            text(format!(
                "Item {} / {}  ·  bar {} / {}",
                item_idx + 1,
                set.items.len(),
                bar_now,
                self.practice_bars_per_item
            ))
            .size(13)
        };

        let next_preview = if set.items.len() > 1 {
            let next_idx = (item_idx + 1) % set.items.len();
            text(format!("Up next: {}", set.items[next_idx].label())).size(12)
        } else {
            text("").size(12)
        };

        // Render the current item on the fretboard.
        let item_canvas = self.practice_item_canvas(current);

        column![
            text(set.name.clone()).size(22),
            text(set.description.clone()).size(12),
            picker_row,
            tempo_row,
            transport,
            beat_indicator,
            item_label,
            progress_label,
            item_canvas,
            next_preview,
        ]
        .spacing(12)
        .into()
    }

    /// Produce a fretboard canvas for the current practice item.
    fn practice_item_canvas(&self, item: &PracticeItem) -> Element<'_, Message> {
        let (positions, label_color_meaningful) = match item {
            PracticeItem::Scale { formula, root, position } => {
                let all = self
                    .fretboard
                    .positions_for_scale(formula, *root)
                    .unwrap_or_default();
                let window_start = *position;
                let window_end = window_start + 4;
                let pos: Vec<Position> = all
                    .into_iter()
                    .filter(|p| p.fret == 0 || (p.fret >= window_start && p.fret <= window_end))
                    .collect();
                (pos, true)
            }
            PracticeItem::Chord { formula, root, position } => {
                let all = self
                    .fretboard
                    .positions_for_chord(formula, *root)
                    .unwrap_or_default();
                let window_start = *position;
                let window_end = window_start + 4;
                let pos: Vec<Position> = all
                    .into_iter()
                    .filter(|p| p.fret == 0 || (p.fret >= window_start && p.fret <= window_end))
                    .collect();
                (pos, true)
            }
            PracticeItem::Exercise { exercise, starting_fret } => {
                let steps = exercise.generate(
                    &self.fretboard.tuning,
                    &music_theory::exercise::ExerciseParams {
                        starting_fret: *starting_fret,
                        direction:
                            music_theory::exercise::ExerciseDirection::Both,
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
                        pitch: self.fretboard.pitch_at(s.string_index, s.fret),
                        interval_from_root: None,
                    })
                    .collect();
                (pos, false)
            }
        };

        let labels = if label_color_meaningful {
            positions
                .iter()
                .map(|p| {
                    format!("{}{}", p.pitch.name, accidental_str(p.pitch.accidental))
                })
                .collect()
        } else {
            vec![String::new(); positions.len()]
        };

        Canvas::new(FretboardCanvas {
            fretboard: self.fretboard.clone(),
            positions,
            labels,
            colors: self.diagram_theme.colors(),
        })
        .width(Length::Fill)
        .height(Length::Fixed(280.0))
        .into()
    }

    fn metronome_view(&self) -> Element<'_, Message> {
        match &self.engine {
            Err(err) => container(
                column![
                    text("Audio engine unavailable").size(24),
                    text(err.clone()).size(14),
                ]
                .spacing(8),
            )
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into(),
            Ok(_) => {
                let bpm_label = text(format!("{:.0} BPM", self.bpm)).size(48);
                let bpm_input = text_input("BPM", &self.metronome_bpm_input)
                    .on_input(Message::MetronomeBpmInputChanged)
                    .on_submit(Message::MetronomeBpmInputSubmitted)
                    .size(14)
                    .width(Length::Fixed(70.0));
                let tap_btn = button(text("Tap").size(13))
                    .on_press(Message::MetronomeTap)
                    .padding([6, 14])
                    .style(button::secondary);
                let bpm_input_row = row![
                    text("Set:").size(13),
                    bpm_input,
                    tap_btn,
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center);
                let bpm_slider =
                    slider(40.0..=240.0, self.bpm, Message::BpmChanged).step(1.0);

                // Visual beat indicator — derived from elapsed time so
                // it advances independently of the audio output.
                let beats_per_bar = self.metronome_time_sig_num.max(1);
                let current_beat = if self.metronome_playing {
                    self.metronome_current_beat % beats_per_bar
                } else {
                    beats_per_bar // out of range = no highlight
                };
                let beat_glyphs: String = (0..beats_per_bar)
                    .map(|i| if i == current_beat { "● " } else { "○ " })
                    .collect();
                let beat_color = if self.metronome_playing {
                    self.diagram_theme.colors().root_dot
                } else {
                    Color::from_rgb(0.4, 0.4, 0.4)
                };
                let beat_indicator: Element<Message> =
                    text(beat_glyphs).size(32).color(beat_color).into();

                let play_button = if self.metronome_playing {
                    button(text("Stop").size(20))
                        .on_press(Message::StopMetronome)
                        .style(button::danger)
                        .padding([12, 24])
                } else {
                    button(text("Play").size(20))
                        .on_press(Message::PlayMetronome)
                        .style(button::primary)
                        .padding([12, 24])
                };

                let time_sig_row = row![
                    text("Time:").size(13),
                    pick_list(
                        &TIME_SIG_NUMERATORS[..],
                        Some(self.metronome_time_sig_num),
                        Message::MetronomeTimeSigChanged,
                    )
                    .text_size(13),
                    text("/ 4").size(13),
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center);

                let mut sub_row = row![text("Notes:").size(13)]
                    .spacing(6)
                    .align_y(iced::Alignment::Center);
                for s in SUBDIVISIONS {
                    sub_row = sub_row.push(metronome_sub_btn(
                        subdivision_label(s),
                        s,
                        self.metronome_subdivision,
                    ));
                }

                let click_row = row![
                    text("Click:").size(13),
                    metronome_click_btn(
                        "Beat only",
                        ClickPattern::BeatOnly,
                        self.metronome_click,
                    ),
                    metronome_click_btn(
                        "Every note",
                        ClickPattern::EverySubdivision,
                        self.metronome_click,
                    ),
                ]
                .spacing(6)
                .align_y(iced::Alignment::Center);

                let accent_row = row![
                    text("Accent:").size(13),
                    metronome_accent_btn(
                        "Downbeat",
                        AccentMode::Downbeat,
                        self.metronome_accent,
                    ),
                    metronome_accent_btn(
                        "Every beat",
                        AccentMode::EveryBeat,
                        self.metronome_accent,
                    ),
                    metronome_accent_btn("None", AccentMode::None, self.metronome_accent),
                ]
                .spacing(6)
                .align_y(iced::Alignment::Center);

                container(
                    column![
                        bpm_label,
                        bpm_slider,
                        bpm_input_row,
                        beat_indicator,
                        play_button,
                        time_sig_row,
                        sub_row,
                        click_row,
                        accent_row,
                    ]
                    .spacing(16)
                    .max_width(540)
                    .align_x(iced::Alignment::Center),
                )
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into()
            }
        }
    }

    fn exercises_view(&self) -> Element<'_, Message> {
        let mut list = column![text("Exercises").size(20)].spacing(6);
        for (i, ex) in exercise_catalog().iter().enumerate() {
            let active = self.selected_exercise == Some(i);
            let style: fn(&Theme, button::Status) -> button::Style = if active {
                button::primary
            } else {
                button::secondary
            };
            list = list.push(
                button(text(ex.name).size(13))
                    .on_press(Message::ExerciseSelected(i))
                    .style(style)
                    .padding([6, 12])
                    .width(Length::Fill),
            );
        }

        let detail: Element<Message> = match self.selected_exercise {
            Some(idx) => {
                let ex = &exercise_catalog()[idx];
                let steps =
                    ex.generate(&self.fretboard.tuning, &ExerciseParams::default());
                // Dedup unique (string, fret) pairs for visualization.
                let mut seen = std::collections::HashSet::new();
                let positions: Vec<Position> = steps
                    .iter()
                    .filter(|s| seen.insert((s.string_index, s.fret)))
                    .map(|s| Position {
                        string_index: s.string_index,
                        fret: s.fret,
                        pitch: self.fretboard.pitch_at(s.string_index, s.fret),
                        interval_from_root: None,
                    })
                    .collect();
                let labels = vec![String::new(); positions.len()];
                let canvas_widget = Canvas::new(FretboardCanvas {
                    fretboard: self.fretboard.clone(),
                    positions,
                    labels,
                    colors: self.diagram_theme.colors(),
                })
                .width(Length::Fill)
                .height(Length::Fixed(280.0));
                column![
                    text(ex.name).size(22),
                    text(ex.description).size(12),
                    text(format!("{} steps · starting fret 1 · both directions", steps.len()))
                        .size(11),
                    canvas_widget,
                ]
                .spacing(8)
                .into()
            }
            None => container(text("Select an exercise.").size(16))
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into(),
        };

        row![
            container(list).width(Length::Fixed(220.0)).padding(8),
            container(detail).padding(8),
        ]
        .spacing(12)
        .into()
    }

    fn progressions_view(&self) -> Element<'_, Message> {
        let mut list = column![text("Progressions").size(20)].spacing(6);
        for (i, p) in progression_catalog().iter().enumerate() {
            let active = self.selected_progression == Some(i);
            let style: fn(&Theme, button::Status) -> button::Style = if active {
                button::primary
            } else {
                button::secondary
            };
            list = list.push(
                button(text(p.name).size(13))
                    .on_press(Message::ProgressionSelected(i))
                    .style(style)
                    .padding([6, 12])
                    .width(Length::Fill),
            );
        }

        let key_picker = row![
            text("Key:").size(13),
            pick_list(
                &ChromaticPc::ALL[..],
                Some(self.progression_key_pc),
                Message::ProgressionKeyPcChanged,
            )
            .text_size(13),
            pick_list(
                &self.progression_valid_scale_names[..],
                Some(self.progression_key_scale.name),
                Message::ProgressionScaleSelected,
            )
            .text_size(13),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);

        let detail: Element<Message> = match self.selected_progression {
            Some(idx) => {
                let prog = &progression_catalog()[idx];
                match prog.apply_in_key(
                    self.progression_key_root,
                    self.progression_key_scale,
                ) {
                    Ok(chords) => {
                        let mut chord_row = row![].spacing(12);
                        for (i, c) in chords.iter().enumerate() {
                            let active = self.progression_expanded_chord == Some(i);
                            chord_row = chord_row.push(chord_card_button(c, i, active));
                        }

                        // If a chord is expanded, render its diagram + arrows below.
                        let mut col = column![
                            text(prog.name).size(22),
                            text(prog.description).size(12),
                            key_picker,
                            chord_row.wrap(),
                        ]
                        .spacing(12);

                        if let Some(idx) = self.progression_expanded_chord {
                            if let Some(chord) = chords.get(idx) {
                                col = col.push(self.progression_voicing_view(chord));
                            }
                        }

                        col.into()
                    }
                    Err(e) => text(format!("Error: {e}")).size(13).into(),
                }
            }
            None => container(text("Select a progression.").size(16))
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into(),
        };

        row![
            container(list).width(Length::Fixed(240.0)).padding(8),
            container(detail).padding(8),
        ]
        .spacing(12)
        .into()
    }

    fn progression_voicing_view(&self, chord: &ProgressionChord) -> Element<'_, Message> {
        if self.progression_available_positions.is_empty() {
            return text("No voicings found for this chord on the active tuning.")
                .size(13)
                .into();
        }

        let by_pos = voicings_by_position(
            &self.fretboard,
            chord.formula,
            chord.root,
        );

        let positions = &self.progression_available_positions;
        let pos_idx = self.progression_position_idx.min(positions.len() - 1);
        let current_pos = positions[pos_idx];
        let voicings = match by_pos.get(&current_pos) {
            Some(v) if !v.is_empty() => v,
            _ => {
                return text("Voicing data drifted; reselect the chord.")
                    .size(13)
                    .into();
            }
        };

        let count = voicings.len();
        let v_idx = self.progression_voicing_idx.min(count - 1);
        let voicing = voicings[v_idx].clone();

        let position_label = if current_pos == 0 {
            "Open".to_string()
        } else {
            format!("Position {current_pos}")
        };
        let position_progress = format!("({} / {})", pos_idx + 1, positions.len());

        let pos_prev = button(text("◀").size(16))
            .on_press(Message::ProgressionChordPositionPrev)
            .padding([4, 10])
            .style(button::secondary);
        let pos_next = button(text("▶").size(16))
            .on_press(Message::ProgressionChordPositionNext)
            .padding([4, 10])
            .style(button::secondary);

        let position_row = row![
            text("Position:").size(12),
            pos_prev,
            column![
                text(position_label).size(13),
                text(position_progress).size(10),
            ]
            .spacing(2)
            .align_x(iced::Alignment::Center)
            .width(Length::Fixed(110.0)),
            pos_next,
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);

        let voicing_prev = button(text("◀").size(16))
            .on_press(Message::ProgressionChordVoicingPrev)
            .padding([4, 10])
            .style(button::secondary);
        let voicing_next = button(text("▶").size(16))
            .on_press(Message::ProgressionChordVoicingNext)
            .padding([4, 10])
            .style(button::secondary);

        let diagram = Canvas::new(
            ChordDiagram::from_voicing(voicing, self.diagram_theme.colors())
                .with_labels(self.diagram_label_mode),
        )
        .width(Length::Fixed(130.0))
        .height(Length::Fixed(200.0));

        let voicing_row = row![
            text("Voicing:").size(12),
            voicing_prev,
            text(format!("{} / {}", v_idx + 1, count))
                .size(13)
                .width(Length::Fixed(60.0))
                .align_x(iced::alignment::Horizontal::Center),
            voicing_next,
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);

        column![diagram, position_row, voicing_row]
            .spacing(8)
            .align_x(iced::Alignment::Center)
            .into()
    }

    fn tuner_view(&self) -> Element<'_, Message> {
        if self.tuner.is_none() {
            let mut col = column![
                text("Tuner").size(28),
                text("Free mode — listens to the microphone and reports the closest note.").size(14),
            ]
            .spacing(12)
            .max_width(500)
            .align_x(iced::Alignment::Center);

            col = col.push(
                button(text("Start Listening").size(18))
                    .on_press(Message::StartTuner)
                    .style(button::primary)
                    .padding([12, 24]),
            );

            if let Some(err) = &self.tuner_error {
                col = col.push(text(format!("Error: {err}")).size(13));
            }

            return container(col)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into();
        }

        let stop_btn = button(text("Stop").size(16))
            .on_press(Message::StopTuner)
            .style(button::danger)
            .padding([8, 20]);

        // Target picker row 1: open strings of the active tuning. Each
        // button hints that exact pitch. "Free" clears the target.
        let mut tuning_row = row![
            text("Strings:").size(13),
            string_btn("Free", None, &self.tuner_target),
        ]
        .spacing(6)
        .align_y(iced::Alignment::Center);
        for s in &self.fretboard.tuning.strings {
            let label = format!(
                "{}{}{}",
                s.name,
                accidental_str(s.accidental),
                s.octave
            );
            tuning_row = tuning_row.push(string_btn_owned(
                label,
                Some(*s),
                &self.tuner_target,
            ));
        }

        // Target picker row 2: arbitrary pitch via two pick_lists.
        let custom_row = row![
            text("Custom:").size(13),
            pick_list(
                &ChromaticPc::ALL[..],
                Some(self.tuner_custom_pc),
                Message::TunerCustomPcChanged,
            )
            .text_size(13),
            pick_list(
                &TUNER_OCTAVES[..],
                Some(self.tuner_custom_octave),
                Message::TunerCustomOctaveChanged,
            )
            .text_size(13),
        ]
        .spacing(6)
        .align_y(iced::Alignment::Center);

        let target_picker = column![tuning_row, custom_row]
            .spacing(8)
            .align_x(iced::Alignment::Center);

        // Detector picker — FFT is fast/sharp; Cepstrum is more robust
        // against missing fundamentals (laptop mic + low guitar strings).
        let detector_picker = row![
            text("Algorithm:").size(13),
            detector_btn("FFT", DetectorKind::Fft, self.tuner_detector),
            detector_btn("Cepstrum", DetectorKind::Cepstrum, self.tuner_detector),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);

        // Sensitivity slider — lower threshold for quiet rooms, raise
        // for noisy ones. Range covers ~1/3× to 5× the default.
        let sensitivity = column![
            text(format!(
                "Sensitivity threshold: {:.4} RMS",
                self.tuner_threshold
            ))
            .size(13),
            slider(
                0.0003_f32..=0.005_f32,
                self.tuner_threshold as f32,
                Message::TunerThresholdChanged
            )
            .step(0.0001_f32)
            .width(Length::Fixed(420.0)),
        ]
        .spacing(4)
        .align_x(iced::Alignment::Center);

        // Stable layout: same widgets render whether or not a note is
        // detected. Placeholders ("—" / blank / 0 cents) keep the rest
        // of the tuner UI from jumping when detection comes and goes.
        let dim = Color::from_rgb(0.5, 0.5, 0.5);
        let (note_label, cents_str, cents_color, freq_str, cents_value) =
            match &self.tuner_latest {
                Some(note) => {
                    let label = match &self.tuner_target {
                        Some(target) => format!(
                            "{}{}{}",
                            target.name,
                            accidental_str(target.accidental),
                            target.octave
                        ),
                        None => format!(
                            "{}{}",
                            detected_note_name_str(note.name.clone()),
                            note.octave
                        ),
                    };
                    let cents = if note.cents_offset.abs() < 1.0 {
                        "in tune".to_string()
                    } else if note.cents_offset > 0.0 {
                        format!("+{:.0} cents (sharp)", note.cents_offset)
                    } else {
                        format!("{:.0} cents (flat)", note.cents_offset)
                    };
                    let color = if note.in_tune {
                        Color::from_rgb(0.3, 0.85, 0.3)
                    } else if note.cents_offset.abs() < 20.0 {
                        Color::from_rgb(0.95, 0.85, 0.3)
                    } else {
                        Color::from_rgb(0.95, 0.45, 0.35)
                    };
                    let freq = format!(
                        "{:.1} Hz   ·   target {:.1} Hz",
                        note.actual_freq_hz, note.note_freq_hz
                    );
                    (label, cents, color, freq, note.cents_offset)
                }
                None => {
                    let label = match &self.tuner_target {
                        Some(target) => format!(
                            "{}{}{}",
                            target.name,
                            accidental_str(target.accidental),
                            target.octave
                        ),
                        None => "—".to_string(),
                    };
                    let hint = if self.tuner_level < 0.0003 {
                        "listening — no signal".to_string()
                    } else {
                        "listening — too quiet".to_string()
                    };
                    (label, hint, dim, " ".to_string(), 0.0)
                }
            };

        let body: Element<Message> = column![
            text(note_label).size(96).color(if self.tuner_latest.is_some() {
                Color::WHITE
            } else {
                dim
            }),
            text(cents_str).size(22).color(cents_color),
            text(freq_str).size(13),
            Canvas::new(CentsMeter {
                cents: cents_value,
                active: self.tuner_latest.is_some(),
            })
            .width(Length::Fixed(420.0))
            .height(Length::Fixed(60.0)),
            Canvas::new(LevelMeter {
                level: self.tuner_level,
                threshold: self.tuner_threshold as f32,
            })
            .width(Length::Fixed(420.0))
            .height(Length::Fixed(8.0)),
        ]
        .spacing(12)
        .align_x(iced::Alignment::Center)
        .into();

        // Algorithm description — short blurb that updates with the
        // current detector pick, since each has tradeoffs the user
        // should understand.
        let algo_desc = text(match self.tuner_detector {
            DetectorKind::Fft => {
                "FFT: picks the loudest spectral peak. Fast and accurate \
                 when the fundamental is the strongest peak. Can confuse \
                 harmonics when the fundamental is weak (e.g. laptop \
                 mics on low guitar strings)."
            }
            DetectorKind::Cepstrum => {
                "Cepstrum: infers the fundamental from harmonic spacing. \
                 Robust when the fundamental is missing or quiet, but \
                 can be noisier on cents accuracy."
            }
        })
        .size(12)
        .width(Length::Fixed(420.0));

        container(
            column![body, target_picker, detector_picker, algo_desc, sensitivity, stop_btn]
                .spacing(16)
                .align_x(iced::Alignment::Center),
        )
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
    }
}

fn detected_note_name_str(name: DetectedNoteName) -> &'static str {
    use DetectedNoteName::*;
    match name {
        A => "A",
        ASharp => "A#",
        B => "B",
        C => "C",
        CSharp => "C#",
        D => "D",
        DSharp => "D#",
        E => "E",
        F => "F",
        FSharp => "F#",
        G => "G",
        GSharp => "G#",
    }
}

// Cents-deviation meter — horizontal bar with center line + needle.
// When `active` is false, the needle is hidden so the meter still
// reserves space without showing a misleading 0-cents reading.
struct CentsMeter {
    cents: f64,
    active: bool,
}

impl<Message> canvas::Program<Message> for CentsMeter {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let w = bounds.width;
        let h = bounds.height;
        let mid_y = h / 2.0;
        let pad = 12.0_f32;
        let bar_left = pad;
        let bar_right = w - pad;
        let bar_w = bar_right - bar_left;
        let center_x = bar_left + bar_w / 2.0;

        // Bar
        let bar = canvas::Path::line(Point::new(bar_left, mid_y), Point::new(bar_right, mid_y));
        frame.stroke(
            &bar,
            canvas::Stroke::default()
                .with_width(2.0)
                .with_color(Color::from_rgb(0.4, 0.4, 0.4)),
        );

        // Center tick
        let center = canvas::Path::line(
            Point::new(center_x, mid_y - 14.0),
            Point::new(center_x, mid_y + 14.0),
        );
        frame.stroke(
            &center,
            canvas::Stroke::default()
                .with_width(2.0)
                .with_color(Color::from_rgb(0.6, 0.6, 0.6)),
        );

        // Side ticks at ±25 and ±50 cents
        for &cents in &[-50.0_f32, -25.0, 25.0, 50.0] {
            let x = center_x + (cents / 50.0) * (bar_w / 2.0);
            let tick = canvas::Path::line(
                Point::new(x, mid_y - 8.0),
                Point::new(x, mid_y + 8.0),
            );
            frame.stroke(
                &tick,
                canvas::Stroke::default()
                    .with_width(1.0)
                    .with_color(Color::from_rgb(0.5, 0.5, 0.5)),
            );
        }

        // Needle — only drawn when active. In the inactive state we
        // still draw the bar and ticks so the meter "reserves" its
        // layout space, but the needle is omitted to avoid showing a
        // stale or misleading 0-cents reading.
        if self.active {
            let cents_clamped = self.cents.clamp(-50.0, 50.0) as f32;
            let needle_x = center_x + (cents_clamped / 50.0) * (bar_w / 2.0);
            let needle_color = if self.cents.abs() < 5.0 {
                Color::from_rgb(0.3, 0.85, 0.3)
            } else if self.cents.abs() < 20.0 {
                Color::from_rgb(0.95, 0.85, 0.3)
            } else {
                Color::from_rgb(0.95, 0.45, 0.35)
            };
            let needle = canvas::Path::circle(Point::new(needle_x, mid_y), 8.0);
            frame.fill(&needle, needle_color);
        }

        vec![frame.into_geometry()]
    }
}

// Thin horizontal bar showing input RMS amplitude on a log-ish scale.
// Background grey, filled green up to the current level. Color shifts
// to yellow above ~0.3 RMS and red near clipping. The threshold tick
// shows where the silence gate sits.
struct LevelMeter {
    level: f32,
    threshold: f32,
}

impl<Message> canvas::Program<Message> for LevelMeter {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let w = bounds.width;
        let h = bounds.height;

        // Background bar
        let bg = canvas::Path::rectangle(Point::new(0.0, 0.0), Size::new(w, h));
        frame.fill(&bg, Color::from_rgb(0.18, 0.18, 0.20));

        // Mapped fill — RMS range [0, 0.5] maps to full bar (anything
        // louder is clamped). Slight curve so quiet signals are still
        // visible.
        let normalized = (self.level / 0.5).clamp(0.0, 1.0);
        let curved = normalized.powf(0.7);
        let fill_w = w * curved;

        if fill_w > 0.0 {
            let color = if self.level > 0.4 {
                Color::from_rgb(0.95, 0.45, 0.35)
            } else if self.level > 0.25 {
                Color::from_rgb(0.95, 0.85, 0.3)
            } else {
                Color::from_rgb(0.3, 0.75, 0.45)
            };
            let fill = canvas::Path::rectangle(Point::new(0.0, 0.0), Size::new(fill_w, h));
            frame.fill(&fill, color);
        }

        // Threshold tick at the live silence-gate value.
        let threshold_x = w * (self.threshold / 0.5).powf(0.7);
        let tick = canvas::Path::line(
            Point::new(threshold_x, 0.0),
            Point::new(threshold_x, h),
        );
        frame.stroke(
            &tick,
            canvas::Stroke::default()
                .with_width(1.0)
                .with_color(Color::from_rgb(0.6, 0.6, 0.6)),
        );

        vec![frame.into_geometry()]
    }
}

fn chord_card_button(
    c: &ProgressionChord,
    idx: usize,
    active: bool,
) -> Element<'static, Message> {
    let root_label = format!("{}{}", c.root.name, accidental_str(c.root.accidental));
    let chord_symbol = format!("{}{}", root_label, c.formula.symbol);
    let pitches = c
        .pitches
        .iter()
        .map(|p| format!("{}{}", p.name, accidental_str(p.accidental)))
        .collect::<Vec<_>>()
        .join(" ");
    let degree_label = format!("({})", c.formula.name);
    let inner = column![
        text(chord_symbol).size(28),
        text(degree_label).size(11),
        text(pitches).size(13),
    ]
    .spacing(4)
    .align_x(iced::Alignment::Center);

    button(inner)
        .padding(12)
        .style(move |theme: &Theme, _status| {
            let palette = theme.extended_palette();
            let bg = if active {
                palette.primary.weak.color
            } else {
                palette.background.weak.color
            };
            button::Style {
                background: Some(bg.into()),
                border: iced::Border {
                    color: palette.background.strong.color,
                    width: 1.0,
                    radius: 6.0.into(),
                },
                text_color: palette.background.strong.text,
                ..Default::default()
            }
        })
        .on_press(Message::ProgressionChordExpanded(idx))
        .into()
}

fn label_btn(
    label: &'static str,
    this: LabelMode,
    current: LabelMode,
) -> Element<'static, Message> {
    let style: fn(&Theme, button::Status) -> button::Style = if current == this {
        button::primary
    } else {
        button::secondary
    };
    button(text(label).size(12))
        .on_press(Message::ScaleLabelModeChanged(this))
        .style(style)
        .padding([3, 10])
        .into()
}

fn metronome_sub_btn(
    label: &'static str,
    this: Subdivision,
    current: Subdivision,
) -> Element<'static, Message> {
    let style: fn(&Theme, button::Status) -> button::Style = if current == this {
        button::primary
    } else {
        button::secondary
    };
    button(text(label).size(12))
        .on_press(Message::MetronomeSubdivisionChanged(this))
        .style(style)
        .padding([4, 10])
        .into()
}

fn metronome_click_btn(
    label: &'static str,
    this: ClickPattern,
    current: ClickPattern,
) -> Element<'static, Message> {
    let style: fn(&Theme, button::Status) -> button::Style = if current == this {
        button::primary
    } else {
        button::secondary
    };
    button(text(label).size(12))
        .on_press(Message::MetronomeClickChanged(this))
        .style(style)
        .padding([4, 10])
        .into()
}

fn metronome_accent_btn(
    label: &'static str,
    this: AccentMode,
    current: AccentMode,
) -> Element<'static, Message> {
    let style: fn(&Theme, button::Status) -> button::Style = if current == this {
        button::primary
    } else {
        button::secondary
    };
    button(text(label).size(12))
        .on_press(Message::MetronomeAccentChanged(this))
        .style(style)
        .padding([4, 10])
        .into()
}

fn chord_label_btn(
    label: &'static str,
    this: LabelMode,
    current: LabelMode,
) -> Element<'static, Message> {
    let style: fn(&Theme, button::Status) -> button::Style = if current == this {
        button::primary
    } else {
        button::secondary
    };
    button(text(label).size(12))
        .on_press(Message::ChordLabelModeChanged(this))
        .style(style)
        .padding([3, 10])
        .into()
}

fn diagram_label_btn(
    label: &'static str,
    this: LabelMode,
    current: LabelMode,
) -> Element<'static, Message> {
    let style: fn(&Theme, button::Status) -> button::Style = if current == this {
        button::primary
    } else {
        button::secondary
    };
    button(text(label).size(12))
        .on_press(Message::DiagramLabelModeChanged(this))
        .style(style)
        .padding([3, 10])
        .into()
}

fn detector_btn(
    label: &'static str,
    this: DetectorKind,
    current: DetectorKind,
) -> Element<'static, Message> {
    let style: fn(&Theme, button::Status) -> button::Style = if current == this {
        button::primary
    } else {
        button::secondary
    };
    button(text(label).size(13))
        .on_press(Message::TunerDetectorChanged(this))
        .style(style)
        .padding([4, 12])
        .into()
}

fn string_btn(
    label: &'static str,
    this: Option<Pitch>,
    current: &Option<Pitch>,
) -> Element<'static, Message> {
    let active = match (&this, current) {
        (None, None) => true,
        (Some(t), Some(c)) => t == c,
        _ => false,
    };
    let style: fn(&Theme, button::Status) -> button::Style = if active {
        button::primary
    } else {
        button::secondary
    };
    button(text(label).size(13))
        .on_press(Message::TunerTargetSet(this))
        .style(style)
        .padding([4, 10])
        .into()
}

fn string_btn_owned(
    label: String,
    this: Option<Pitch>,
    current: &Option<Pitch>,
) -> Element<'static, Message> {
    let active = match (&this, current) {
        (Some(t), Some(c)) => t == c,
        _ => false,
    };
    let style: fn(&Theme, button::Status) -> button::Style = if active {
        button::primary
    } else {
        button::secondary
    };
    button(text(label).size(13))
        .on_press(Message::TunerTargetSet(this))
        .style(style)
        .padding([4, 10])
        .into()
}

fn tab_btn(label: &'static str, this: Tab, current: Tab) -> Element<'static, Message> {
    let is_active = current == this;
    let style: fn(&Theme, button::Status) -> button::Style = if is_active {
        button::primary
    } else {
        button::secondary
    };
    button(text(label))
        .on_press(Message::TabSelected(this))
        .style(style)
        .into()
}

// === Chord diagram canvas (vertical, songbook-style) ===

/// Vertical chord diagram. Strings as vertical lines (lowest pitch on
/// the left, conventional songbook orientation). Frets as horizontal
/// lines, low frets at top, high frets at bottom. X above muted
/// strings, O above open strings, dots for fretted positions. Position
/// marker on the left if the diagram doesn't include the nut.
struct ChordDiagram {
    voicing: ChordVoicing,
    /// First fret displayed at the TOP of the diagram. 1 = nut at top.
    /// 5 = position 5 (frets 5–8 shown).
    top_fret: u8,
    /// Optional per-string label (parallel to voicing.strings). Empty
    /// string = no label drawn. Only shown for fretted positions; the
    /// header X / O markers are unaffected.
    labels: Vec<String>,
    colors: DiagramColors,
}

impl ChordDiagram {
    /// Build a diagram, automatically picking a top_fret based on the
    /// voicing's lowest and highest fretted notes so all dots fit
    /// within the visible window.
    fn from_voicing(voicing: ChordVoicing, colors: DiagramColors) -> Self {
        let highest = voicing
            .strings
            .iter()
            .filter_map(|s| match s {
                StringPlay::Played { fret, .. } if *fret > 0 => Some(*fret),
                _ => None,
            })
            .max()
            .unwrap_or(0);
        let lowest = voicing.lowest_fretted_position();
        // Fits in the nut window if the highest fret is at most 4.
        let top_fret = if lowest == 0 || highest <= 4 {
            1
        } else {
            lowest
        };
        let labels = vec![String::new(); voicing.strings.len()];
        Self { voicing, top_fret, labels, colors }
    }

    /// Generate labels appropriate for a label mode.
    fn with_labels(mut self, mode: LabelMode) -> Self {
        let fingers = if mode == LabelMode::Fingers {
            Some(finger_assignments(&self.voicing))
        } else {
            None
        };
        self.labels = self
            .voicing
            .strings
            .iter()
            .enumerate()
            .map(|(i, s)| match s {
                StringPlay::Played { fret, pitch, interval_from_root } if *fret > 0 => {
                    match mode {
                        LabelMode::None => String::new(),
                        LabelMode::Notes => format!(
                            "{}{}",
                            pitch.name,
                            accidental_str(pitch.accidental)
                        ),
                        LabelMode::Degrees => interval_from_root
                            .map(|iv| iv.number().to_string())
                            .unwrap_or_default(),
                        LabelMode::Fingers => fingers
                            .as_ref()
                            .and_then(|f| f.get(i).copied().flatten())
                            .map(|n| n.to_string())
                            .unwrap_or_default(),
                    }
                }
                _ => String::new(),
            })
            .collect();
        self
    }
}

impl<Message> canvas::Program<Message> for ChordDiagram {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let n = self.voicing.strings.len();
        if n < 2 {
            return vec![frame.into_geometry()];
        }

        // Asymmetric horizontal padding: more space on the left so
        // position markers like "12fr" don't overflow off-canvas.
        let pad_left = 22.0_f32;
        let pad_right = 6.0_f32;
        let header_h = 14.0_f32;
        let pad_bottom = 4.0_f32;
        let avail_w = (bounds.width - pad_left - pad_right).max(40.0);
        let avail_h = (bounds.height - header_h - pad_bottom).max(40.0);

        let frets_shown = 4_usize;
        let string_x_step = avail_w / (n - 1) as f32;
        let fret_y_step = avail_h / frets_shown as f32;

        let board_left = pad_left;
        let board_top = header_h;
        let board_right = board_left + avail_w;
        let board_bottom = board_top + avail_h;

        // Dot radius scales with the smaller cell dimension so dots
        // never overflow string-spacing or fret-region bounds.
        let dot_radius = string_x_step.min(fret_y_step) * 0.36;

        let line_color = Color::from_rgb(0.6, 0.6, 0.6);
        let dot_color = self.colors.note_dot;

        // Strings (vertical) — lowest pitch on the left (string index 0).
        for i in 0..n {
            let x = board_left + i as f32 * string_x_step;
            let path = canvas::Path::line(
                Point::new(x, board_top),
                Point::new(x, board_bottom),
            );
            frame.stroke(
                &path,
                canvas::Stroke::default().with_width(1.0).with_color(line_color),
            );
        }

        // Frets (horizontal). Top line is the nut (thicker) when top_fret == 1.
        for f in 0..=frets_shown {
            let y = board_top + f as f32 * fret_y_step;
            let width = if f == 0 && self.top_fret == 1 { 4.0 } else { 1.0 };
            let path = canvas::Path::line(
                Point::new(board_left, y),
                Point::new(board_right, y),
            );
            frame.stroke(
                &path,
                canvas::Stroke::default().with_width(width).with_color(line_color),
            );
        }

        // Position marker if not at nut.
        if self.top_fret > 1 {
            let marker = canvas::Text {
                content: format!("{}fr", self.top_fret),
                position: Point::new(board_left - 4.0, board_top + fret_y_step / 2.0),
                color: Color::from_rgb(0.85, 0.85, 0.85),
                size: 10.0.into(),
                horizontal_alignment: iced::alignment::Horizontal::Right,
                vertical_alignment: iced::alignment::Vertical::Center,
                ..canvas::Text::default()
            };
            frame.fill_text(marker);
        }

        // Header markers (X / O) and fretted dots.
        for (idx, play) in self.voicing.strings.iter().enumerate() {
            let x = board_left + idx as f32 * string_x_step;
            match play {
                StringPlay::Muted => {
                    let t = canvas::Text {
                        content: "×".to_string(),
                        position: Point::new(x, header_h / 2.0),
                        color: self.colors.muted_marker,
                        size: 14.0.into(),
                        horizontal_alignment: iced::alignment::Horizontal::Center,
                        vertical_alignment: iced::alignment::Vertical::Center,
                        ..canvas::Text::default()
                    };
                    frame.fill_text(t);
                }
                StringPlay::Played { fret, interval_from_root, .. } => {
                    let is_root = matches!(
                        interval_from_root,
                        Some(iv) if *iv == music_theory::interval::Interval::PERFECT_UNISON
                    );
                    let this_dot_color =
                        if is_root { self.colors.root_dot } else { dot_color };
                    if *fret == 0 {
                        let t = canvas::Text {
                            content: "○".to_string(),
                            position: Point::new(x, header_h / 2.0),
                            color: self.colors.open_marker,
                            size: 14.0.into(),
                            horizontal_alignment: iced::alignment::Horizontal::Center,
                            vertical_alignment: iced::alignment::Vertical::Center,
                            ..canvas::Text::default()
                        };
                        frame.fill_text(t);
                    } else {
                        // Local fret region (1-indexed from top).
                        let local = if self.top_fret == 1 {
                            *fret as i32
                        } else {
                            *fret as i32 - self.top_fret as i32 + 1
                        };
                        if local >= 1 && local <= frets_shown as i32 {
                            let y = board_top + (local as f32 - 0.5) * fret_y_step;
                            let dot =
                                canvas::Path::circle(Point::new(x, y), dot_radius);
                            frame.fill(&dot, this_dot_color);

                            // Optional label inside the dot — dark text
                            // on the bright dot color reads much better
                            // than white at small sizes.
                            if let Some(label) = self.labels.get(idx) {
                                if !label.is_empty() {
                                    let label_size = (dot_radius * 1.7).max(10.0);
                                    let t = canvas::Text {
                                        content: label.clone(),
                                        position: Point::new(x, y),
                                        color: self.colors.label_text,
                                        size: label_size.into(),
                                        horizontal_alignment:
                                            iced::alignment::Horizontal::Center,
                                        vertical_alignment:
                                            iced::alignment::Vertical::Center,
                                        ..canvas::Text::default()
                                    };
                                    frame.fill_text(t);
                                }
                            }
                        }
                    }
                }
            }
        }

        vec![frame.into_geometry()]
    }
}

fn accidental_str(a: music_theory::pitch::Accidental) -> &'static str {
    use music_theory::pitch::Accidental as A;
    match a {
        A::DoubleFlat => "bb",
        A::Flat => "b",
        A::Natural => "",
        A::Sharp => "#",
        A::DoubleSharp => "##",
    }
}

// === Fretboard canvas ===

struct FretboardCanvas {
    fretboard: Fretboard,
    positions: Vec<Position>,
    /// Optional per-position label text. Indexed in parallel with
    /// `positions`. Empty string = no label drawn for that position.
    /// Default empty for callers that don't supply labels.
    labels: Vec<String>,
    colors: DiagramColors,
}

impl<Message> canvas::Program<Message> for FretboardCanvas {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());

        let n_strings = self.fretboard.tuning.strings.len();
        let n_frets = self.fretboard.fret_count as usize;
        if n_strings < 2 || n_frets < 1 {
            return vec![frame.into_geometry()];
        }

        let margin_x = 60.0_f32;
        let margin_y = 30.0_f32;
        let avail_w = (bounds.width - 2.0 * margin_x).max(50.0);
        let avail_h = (bounds.height - 2.0 * margin_y).max(50.0);

        let fret_w = avail_w / n_frets as f32;
        let string_h = avail_h / (n_strings - 1) as f32;

        let board_left = margin_x;
        let board_top = margin_y;
        let board_right = board_left + avail_w;
        let board_bottom = board_top + (n_strings - 1) as f32 * string_h;

        for i in 0..n_strings {
            let visual = n_strings - 1 - i;
            let y = board_top + visual as f32 * string_h;
            let path = canvas::Path::line(
                Point::new(board_left, y),
                Point::new(board_right, y),
            );
            frame.stroke(
                &path,
                canvas::Stroke::default()
                    .with_width(1.5)
                    .with_color(Color::from_rgb(0.7, 0.7, 0.7)),
            );
        }

        for f in 0..=n_frets {
            let x = board_left + f as f32 * fret_w;
            let width = if f == 0 { 4.0 } else { 1.0 };
            let path = canvas::Path::line(
                Point::new(x, board_top),
                Point::new(x, board_bottom),
            );
            frame.stroke(
                &path,
                canvas::Stroke::default()
                    .with_width(width)
                    .with_color(Color::from_rgb(0.5, 0.5, 0.5)),
            );
        }

        let mid_y = board_top + (n_strings - 1) as f32 / 2.0 * string_h;
        for &fret in &[3usize, 5, 7, 9, 15, 17, 19, 21] {
            if fret > n_frets {
                break;
            }
            let x = board_left + (fret as f32 - 0.5) * fret_w;
            let dot = canvas::Path::circle(Point::new(x, mid_y), 4.0);
            frame.fill(&dot, Color::from_rgb(0.3, 0.3, 0.3));
        }
        for &fret in &[12usize, 24] {
            if fret > n_frets {
                continue;
            }
            let x = board_left + (fret as f32 - 0.5) * fret_w;
            for offset in [-1.0_f32, 1.0] {
                let y = mid_y + offset * string_h;
                let dot = canvas::Path::circle(Point::new(x, y), 4.0);
                frame.fill(&dot, Color::from_rgb(0.3, 0.3, 0.3));
            }
        }

        for (i, pos) in self.positions.iter().enumerate() {
            let visual_string = n_strings - 1 - pos.string_index;
            let y = board_top + visual_string as f32 * string_h;
            let x = if pos.fret == 0 {
                board_left
            } else {
                board_left + (pos.fret as f32 - 0.5) * fret_w
            };

            let is_root = pos.interval_from_root == Some(Interval::PERFECT_UNISON);
            let color = if is_root {
                self.colors.root_dot
            } else {
                self.colors.note_dot
            };

            let circle = canvas::Path::circle(Point::new(x, y), 13.0);
            frame.fill(&circle, color);

            // Draw label if present.
            if let Some(label) = self.labels.get(i) {
                if !label.is_empty() {
                    let t = canvas::Text {
                        content: label.clone(),
                        position: Point::new(x, y),
                        color: self.colors.label_text,
                        size: 12.0.into(),
                        horizontal_alignment: iced::alignment::Horizontal::Center,
                        vertical_alignment: iced::alignment::Vertical::Center,
                        ..canvas::Text::default()
                    };
                    frame.fill_text(t);
                }
            }
        }

        vec![frame.into_geometry()]
    }
}
