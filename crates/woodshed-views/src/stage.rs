//! The Stage screen over live state (S2).
//!
//! Header dropdowns (tuning, root — xilem_serval `select`), the lens strip
//! (Scale / Chord / Arpeggio / Progression / Exercise), a per-lens catalog
//! sidebar, and the fretboard rendered as DOM dots. The runner state is
//! [`UiState`]: the portable `woodshed_core::StageState` plus the
//! view-layer dropdown state; hosts call [`UiState::sync`] after any
//! dispatch so dropdown picks land in the core state.

use std::collections::HashMap;

use woodshed_core::audio::{CalibrationStatus, TransportState, TunerState};
use woodshed_core::history::{catalog_id_for_card, EngagementKind, PracticeHistory};
use woodshed_core::search::{search_corpus, SearchHit};
use woodshed_core::song::SongDoc;
use woodshed_core::storage::{AppSection, PersistedSession, RelatedSettings};
use woodshed_core::{set_from_practice, tunings, Lens, StageState, ROOT_NAMES};
use woodshedding::rehearsal::Set;
use xilem_serval::{
    clickable, el, map_state, select, text, text_field, AnyView, SelectState, ServalCtx,
    ServalElement, TextInput,
};

use crate::theme::ThemeMode;

mod looper;
mod related;
mod rehearsal;
mod settings;
mod templates;
mod tools;

pub const NEIGHBORHOOD_LEAF_KEY: u64 = 0x5753_4e42;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StagePage {
    #[default]
    Catalog,
    Templates,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToolPage {
    #[default]
    Fretboard,
    Metronome,
    Tuner,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
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

/// Fretboard layout (redesign P4): how the Stage arranges catalog and
/// neck. All three render the same resolved positions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BoardLayout {
    /// Catalog sidebar beside the neck (the classic).
    #[default]
    TwoPane,
    /// Full-width neck up top, catalog as a wrapping strip beneath.
    Hero,
    /// The neck alone, enlarged; the catalog stays on the other layouts.
    FullCanvas,
}

impl BoardLayout {
    pub const ALL: [BoardLayout; 3] = [
        BoardLayout::TwoPane,
        BoardLayout::Hero,
        BoardLayout::FullCanvas,
    ];

    pub fn label(self) -> &'static str {
        match self {
            BoardLayout::TwoPane => "Two pane",
            BoardLayout::Hero => "Hero",
            BoardLayout::FullCanvas => "Full canvas",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|l| l.label() == name)
    }

    /// Root class suffix; descendant CSS keys sizing off it.
    fn class(self) -> &'static str {
        match self {
            BoardLayout::TwoPane => "layout-two-pane",
            BoardLayout::Hero => "layout-hero",
            BoardLayout::FullCanvas => "layout-canvas",
        }
    }
}

/// Coarse layout class derived by the host from available logical width.
/// It changes only presentation, never selected or persisted musical state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ViewportClass {
    #[default]
    Wide,
    Medium,
    Narrow,
}

impl ViewportClass {
    /// These are width bands rather than device names. They preserve a usable
    /// fretboard before the shell starts to crowd it.
    pub fn for_width(width: f32) -> Self {
        if width < 760.0 {
            Self::Narrow
        } else if width < 1_180.0 {
            Self::Medium
        } else {
            Self::Wide
        }
    }

    fn class(self) -> &'static str {
        match self {
            Self::Wide => "viewport-wide",
            Self::Medium => "viewport-medium",
            Self::Narrow => "viewport-narrow",
        }
    }
}

/// MIDI panel state (audio-depth slice 13). Port lists + connection
/// status are host-populated; selections + toggles are view-owned and
/// the host realizes them through the `MidiBackend` seam. Transient (not
/// persisted — port availability is session-dependent).
pub struct MidiUiState {
    /// Available ports, host-populated on startup + refresh.
    pub input_ports: Vec<String>,
    pub output_ports: Vec<String>,
    /// Dropdown selection: index 0 = "None", else `ports[idx - 1]`.
    pub input_dd: SelectState,
    pub output_dd: SelectState,
    /// Slave the transport BPM to incoming MIDI clock.
    pub clock_slave: bool,
    /// Send 24-PPQN clock + Start/Stop to the connected output.
    pub clock_out: bool,
    /// Host-polled readout: BPM derived from incoming clock.
    pub clock_bpm: Option<f32>,
    /// Host-populated connection status.
    pub connected_in: Option<String>,
    pub connected_out: Option<String>,
    /// Host-polled recent-events readout (newest last).
    pub events: Vec<String>,
    /// One-shot: re-scan the port lists. Host consumes after dispatch.
    pub refresh_requested: bool,
}

impl MidiUiState {
    pub fn new() -> Self {
        Self {
            input_ports: Vec::new(),
            output_ports: Vec::new(),
            input_dd: SelectState::new(0),
            output_dd: SelectState::new(0),
            clock_slave: false,
            clock_out: false,
            clock_bpm: None,
            connected_in: None,
            connected_out: None,
            events: Vec::new(),
            refresh_requested: false,
        }
    }
}

impl Default for MidiUiState {
    fn default() -> Self {
        Self::new()
    }
}

/// Runner state: the portable core slice plus view-layer widget state.
pub struct UiState {
    pub stage: StageState,
    pub set: Set,
    pub practice_history: PracticeHistory,
    pub related: RelatedSettings,
    /// Rehearsal dwell transport running (transient).
    pub rehearsal_running: bool,
    pub song: SongDoc,
    pub song_playing: bool,
    /// Host-polled live bar cursor during playback (timeline follow).
    pub song_bar_live: usize,
    /// Which bar the editor targets (click a chip to select).
    pub song_edit_cursor: usize,
    /// One-shot rewind request the host consumes after dispatch.
    pub song_rewind_requested: bool,
    pub section: AppSection,
    pub stage_page: StagePage,
    pub tool_page: ToolPage,
    pub settings_page: SettingsPage,
    pub theme: ThemeMode,
    pub board_layout: BoardLayout,
    /// Host-observed width band. This is transient rather than a user setting.
    pub viewport: ViewportClass,
    /// Window-chrome requests the host consumes after dispatch (CSD).
    pub chrome_minimize: bool,
    pub chrome_maximize: bool,
    pub chrome_close: bool,
    pub chrome_drag: bool,
    pub transport: TransportState,
    pub tuner: TunerState,
    /// A device/stream failure reported by the host's audio backend.
    pub audio_error: Option<String>,
    pub tuning_dd: SelectState,
    pub root_dd: SelectState,
    /// Corpus search field (a small always-available field in the nav row).
    pub search: TextInput,
    /// One-shot "♪ Hear" request the host consumes after dispatch: voice
    /// the current lens (or rehearsal card) through the audio backend.
    pub preview_requested: bool,
    /// MIDI panel state (Settings tab).
    pub midi: MidiUiState,
    /// Latency-calibration state (Settings tab). `calib_active` drives
    /// host polling; the request flags are consumed after dispatch.
    pub calib_status: CalibrationStatus,
    pub calib_active: bool,
    pub calib_start_requested: bool,
    pub calib_cancel_requested: bool,
    pub calib_accept_requested: bool,
    /// Accepted round-trip latency (ms), host-reflected.
    pub latency_ms: Option<f32>,
    /// Looper (song-mode record) state. Recording status + per-bar loop
    /// flags are host-reflected; the toggle/clear requests are consumed
    /// after dispatch; `song_record_replace` is a view-owned toggle.
    pub song_recording: bool,
    pub song_loop_bars: Vec<bool>,
    pub song_record_replace: bool,
    pub song_record_toggle_requested: bool,
    pub song_clear_loop_requested: bool,
}

impl Default for UiState {
    fn default() -> Self {
        Self::new()
    }
}

impl UiState {
    pub fn new() -> Self {
        let stage = StageState::new();
        Self {
            set: Set::default(),
            practice_history: PracticeHistory::default(),
            related: RelatedSettings::default(),
            rehearsal_running: false,
            song: SongDoc::default(),
            song_playing: false,
            song_bar_live: 0,
            song_edit_cursor: 0,
            song_rewind_requested: false,
            section: AppSection::Stage,
            stage_page: StagePage::default(),
            tool_page: ToolPage::default(),
            settings_page: SettingsPage::default(),
            theme: ThemeMode::default(),
            board_layout: BoardLayout::default(),
            viewport: ViewportClass::default(),
            chrome_minimize: false,
            chrome_maximize: false,
            chrome_close: false,
            chrome_drag: false,
            transport: TransportState::default(),
            tuner: TunerState::default(),
            audio_error: None,
            tuning_dd: SelectState::new(stage.tuning_idx),
            root_dd: SelectState::new(stage.root_idx),
            search: TextInput::new(""),
            preview_requested: false,
            midi: MidiUiState::new(),
            calib_status: CalibrationStatus::Idle,
            calib_active: false,
            calib_start_requested: false,
            calib_cancel_requested: false,
            calib_accept_requested: false,
            latency_ms: None,
            song_recording: false,
            song_loop_bars: Vec::new(),
            song_record_replace: false,
            song_record_toggle_requested: false,
            song_clear_loop_requested: false,
            stage,
        }
    }

    /// Jump to a search result: focus the target lens/tab (or fill the
    /// set, or swap the tuning), then clear the field so the dropdown
    /// closes.
    fn apply_search_hit(&mut self, hit: SearchHit) {
        match hit {
            SearchHit::Scale(i) => {
                self.stage.set_lens(Lens::Scales);
                self.stage.select_scale(i);
                self.stage_page = StagePage::Catalog;
                self.section = AppSection::Stage;
            }
            SearchHit::Chord(i) => {
                self.stage.set_lens(Lens::Chords);
                self.stage.select_chord(i);
                self.stage_page = StagePage::Catalog;
                self.section = AppSection::Stage;
            }
            SearchHit::Arpeggio(i) => {
                self.stage.set_lens(Lens::Arpeggios);
                self.stage.select_arpeggio(i);
                self.stage_page = StagePage::Catalog;
                self.section = AppSection::Stage;
            }
            SearchHit::Progression(i) => {
                self.stage.set_lens(Lens::Progressions);
                self.stage.select_progression(i);
                self.stage_page = StagePage::Catalog;
                self.section = AppSection::Stage;
            }
            SearchHit::Exercise(i) => {
                self.stage.set_lens(Lens::Exercises);
                self.stage.select_exercise(i);
                self.stage_page = StagePage::Catalog;
                self.section = AppSection::Stage;
            }
            SearchHit::Recipe(i) => {
                if let Some(ps) = woodshedding::practice::catalog().get(i) {
                    self.set = set_from_practice(ps);
                    self.section = AppSection::Rehearsal;
                }
            }
            SearchHit::Tuning(i) => {
                self.stage.set_tuning(i);
                self.tuning_dd = SelectState::new(self.stage.tuning_idx);
                self.section = AppSection::Stage;
            }
        }
        self.search = TextInput::new("");
    }

    /// Mirror dropdown picks into the core state. Hosts call this after
    /// every dispatch (the `select` widget mutates only its own
    /// `SelectState`).
    pub fn sync(&mut self) {
        self.stage.set_tuning(self.tuning_dd.selected);
        self.stage.set_root(self.root_dd.selected);
    }

    /// Update the transient layout band after a host resize. Returns whether a
    /// view rebuild is required.
    pub fn set_viewport_width(&mut self, width: f32) -> bool {
        let next = ViewportClass::for_width(width);
        if self.viewport == next {
            false
        } else {
            self.viewport = next;
            true
        }
    }

    /// Pitches + shape for the on-demand "♪ Hear" preview, resolved from
    /// context: the current rehearsal card on the Rehearsal tab, else the
    /// active Stage lens. Empty pitches = nothing to voice. The host
    /// consumes [`Self::preview_requested`] and calls this.
    pub fn preview_voicing(&self) -> (Vec<f32>, f32, f32) {
        if self.section == AppSection::Rehearsal && !self.set.cards.is_empty() {
            let cursor = self.set.cursor.min(self.set.cards.len() - 1);
            return self.stage.card_voicing(&self.set.cards[cursor]);
        }
        self.stage.voicing_preview()
    }

    pub fn request_preview(&mut self) {
        let subject_id = if self.section == AppSection::Rehearsal && !self.set.cards.is_empty() {
            let cursor = self.set.cursor.min(self.set.cards.len() - 1);
            Some(catalog_id_for_card(&self.set.cards[cursor]))
        } else {
            self.stage.catalog_id()
        };
        if let Some(subject_id) = subject_id {
            self.practice_history
                .record(subject_id, EngagementKind::Previewed, None);
        }
        self.preview_requested = true;
    }

    pub fn stage_current(&mut self, from_id: Option<String>) {
        let subject_id = self.stage.catalog_id();
        if let Some(card) = self.stage.card_from_lens() {
            self.set.push(card);
            if let Some(subject_id) = subject_id {
                self.practice_history
                    .record(subject_id, EngagementKind::Staged, from_id);
            }
        }
    }

    pub fn record_rehearsal_cursor(&mut self) {
        if self.set.cards.is_empty() {
            return;
        }
        let cursor = self.set.cursor.min(self.set.cards.len() - 1);
        self.practice_history.record(
            catalog_id_for_card(&self.set.cards[cursor]),
            EngagementKind::Rehearsed,
            None,
        );
    }

    pub fn complete_rehearsal_cursor(&mut self) {
        if self.set.cards.is_empty() {
            return;
        }
        let cursor = self.set.cursor.min(self.set.cards.len() - 1);
        self.practice_history.record(
            catalog_id_for_card(&self.set.cards[cursor]),
            EngagementKind::Completed,
            None,
        );
    }

    /// Snapshot the persistable subset (the W0.2 seam's payload).
    pub fn to_persisted(&self) -> PersistedSession {
        PersistedSession::capture(
            &self.stage,
            self.section,
            self.transport.bpm,
            self.theme.label(),
            self.board_layout.label(),
            &self.set,
            &self.song,
            &self.practice_history,
            &self.related,
        )
    }

    /// Restore a persisted session (indices clamp; unknown theme names
    /// fall back to the default).
    pub fn apply_persisted(&mut self, session: &PersistedSession) {
        session.restore(&mut self.stage);
        self.set = session.set.clone();
        self.song = session.song.clone();
        self.practice_history = session.practice_history.clone();
        self.related = session.settings.stage.related.clone();
        self.section = session.section;
        self.transport.bpm = session.settings.metronome.bpm.clamp(30.0, 300.0);
        self.theme = ThemeMode::from_name(&session.settings.appearance.theme).unwrap_or_default();
        self.board_layout = BoardLayout::from_name(&session.settings.fretboard.board_layout)
            .unwrap_or_default();
        self.tuning_dd = SelectState::new(self.stage.tuning_idx);
        self.root_dd = SelectState::new(self.stage.root_idx);
    }
}

/// Boxed heterogeneous child view over [`UiState`].
pub type UiChild = Box<dyn AnyView<UiState, (), ServalCtx, ServalElement>>;

fn pill(section: AppSection, active: bool) -> UiChild {
    Box::new(clickable(
        el("span", text(section.label()))
            .attr("class", if active { "pill pill-active" } else { "pill" }),
        move |ui: &mut UiState, _| {
            ui.section = section;
        },
    ))
}

fn header(ui: &UiState) -> UiChild {
    let tuning_names: Vec<&str> = tunings().iter().map(|t| t.name).collect();
    let tuning_dd = map_state(select(&ui.tuning_dd, &tuning_names), |ui: &mut UiState| {
        &mut ui.tuning_dd
    });
    let root_dd = map_state(select(&ui.root_dd, &ROOT_NAMES), |ui: &mut UiState| {
        &mut ui.root_dd
    });
    Box::new(
        el(
            "div",
            (
                el("span", text("Tuning")).attr("class", "header-label"),
                tuning_dd,
                el("span", ()).attr("class", "header-gap"),
                el("span", text("Root")).attr("class", "header-label"),
                root_dd,
            ),
        )
        .attr("class", "header-row"),
    )
}

fn transport(ui: &UiState) -> UiChild {
    let play_label = if ui.transport.playing { "Stop" } else { "Play" };
    let tuner_label = if ui.tuner.enabled {
        "Tuner: on"
    } else {
        "Tuner: off"
    };
    let readout = if let Some(err) = &ui.audio_error {
        format!("audio: {err}")
    } else if ui.tuner.enabled {
        match &ui.tuner.reading {
            Some(r) => format!(
                "{}{} {}{:.0}¢{}",
                r.note,
                r.octave,
                if r.cents >= 0.0 { "+" } else { "" },
                r.cents,
                if r.in_tune { "  in tune" } else { "" },
            ),
            None => "listening...".to_string(),
        }
    } else {
        String::new()
    };
    Box::new(
        el(
            "div",
            (
                clickable(
                    el("div", text(play_label)).attr("class", "t-btn"),
                    |ui: &mut UiState, _| {
                        ui.transport.playing = !ui.transport.playing;
                    },
                ),
                clickable(
                    el("div", text("-")).attr("class", "t-btn t-narrow"),
                    |ui: &mut UiState, _| ui.transport.nudge_bpm(-5.0),
                ),
                el("div", text(format!("{:.0} bpm", ui.transport.bpm))).attr("class", "t-readout"),
                clickable(
                    el("div", text("+")).attr("class", "t-btn t-narrow"),
                    |ui: &mut UiState, _| ui.transport.nudge_bpm(5.0),
                ),
                clickable(
                    el("div", text("♪ Hear")).attr("class", "t-btn t-hear"),
                    |ui: &mut UiState, _| ui.request_preview(),
                ),
                el("span", ()).attr("class", "header-gap"),
                clickable(
                    el("div", text(tuner_label)).attr("class", "t-btn"),
                    |ui: &mut UiState, _| {
                        ui.tuner.enabled = !ui.tuner.enabled;
                        if !ui.tuner.enabled {
                            ui.tuner.reading = None;
                        }
                    },
                ),
                el("div", text(readout)).attr("class", "t-readout"),
                el("span", ()).attr("class", "header-gap"),
                clickable(
                    el("div", text("Stage")).attr("class", "t-btn"),
                    |ui: &mut UiState, _| ui.stage_current(None),
                ),
                el("div", text(format!("{} cards", ui.set.cards.len()))).attr("class", "t-readout"),
            ),
        )
        .attr("class", "transport"),
    )
}

fn lens_strip(ui: &UiState) -> UiChild {
    let mut items: Vec<UiChild> = Lens::ALL
        .iter()
        .map(|&lens| {
            let class = if ui.stage_page == StagePage::Catalog && lens == ui.stage.lens {
                "lens lens-active"
            } else {
                "lens"
            };
            Box::new(clickable(
                el("div", text(lens.label())).attr("class", class),
                move |ui: &mut UiState, _| {
                    ui.stage_page = StagePage::Catalog;
                    ui.stage.set_lens(lens);
                },
            )) as UiChild
        })
        .collect();
    let templates_class = if ui.stage_page == StagePage::Templates {
        "lens lens-active"
    } else {
        "lens"
    };
    items.push(Box::new(clickable(
        el("div", text("Set Templates")).attr("class", templates_class),
        |ui: &mut UiState, _| ui.stage_page = StagePage::Templates,
    )));
    Box::new(el("div", items).attr("class", "lens-strip"))
}

fn sidebar(ui: &UiState) -> UiChild {
    let items: Vec<UiChild> = match ui.stage.lens {
        Lens::Scales => ui
            .stage
            .scales()
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let class = if i == ui.stage.scale_idx {
                    "side-item side-active"
                } else {
                    "side-item"
                };
                Box::new(clickable(
                    el("div", text(s.name)).attr("class", class),
                    move |ui: &mut UiState, _| {
                        ui.stage.select_scale(i);
                    },
                )) as UiChild
            })
            .collect(),
        Lens::Chords => ui
            .stage
            .chords()
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let class = if i == ui.stage.chord_idx {
                    "side-item side-active"
                } else {
                    "side-item"
                };
                let label = if c.symbol.is_empty() {
                    c.name.to_string()
                } else {
                    format!("{} ({})", c.name, c.symbol)
                };
                Box::new(clickable(
                    el("div", text(label)).attr("class", class),
                    move |ui: &mut UiState, _| {
                        ui.stage.select_chord(i);
                    },
                )) as UiChild
            })
            .collect(),
        Lens::Arpeggios => ui
            .stage
            .chords()
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let class = if i == ui.stage.arpeggio_idx {
                    "side-item side-active"
                } else {
                    "side-item"
                };
                let label = if c.symbol.is_empty() {
                    c.name.to_string()
                } else {
                    format!("{} ({})", c.name, c.symbol)
                };
                Box::new(clickable(
                    el("div", text(label)).attr("class", class),
                    move |ui: &mut UiState, _| {
                        ui.stage.select_arpeggio(i);
                    },
                )) as UiChild
            })
            .collect(),
        Lens::Progressions => ui
            .stage
            .progressions()
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let class = if ui.stage.progression_idx == Some(i) {
                    "side-item side-active"
                } else {
                    "side-item"
                };
                Box::new(clickable(
                    el("div", text(p.name)).attr("class", class),
                    move |ui: &mut UiState, _| {
                        ui.stage.select_progression(i);
                    },
                )) as UiChild
            })
            .collect(),
        Lens::Exercises => ui
            .stage
            .exercises()
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let class = if i == ui.stage.exercise_idx {
                    "side-item side-active"
                } else {
                    "side-item"
                };
                Box::new(clickable(
                    el("div", text(e.name)).attr("class", class),
                    move |ui: &mut UiState, _| {
                        ui.stage.select_exercise(i);
                    },
                )) as UiChild
            })
            .collect(),
    };
    Box::new(el("div", items).attr("class", "side"))
}

fn exercise_board_view(ui: &UiState) -> UiChild {
    let state = &ui.stage;
    let board = state.exercise_board();
    let play_label = if state.exercise_playing {
        "Pause"
    } else {
        "Run"
    };
    let deck: UiChild = Box::new(
        el(
            "div",
            (
                clickable(
                    el("div", text(play_label)).attr("class", "t-btn"),
                    |ui: &mut UiState, _| {
                        ui.stage.exercise_playing = !ui.stage.exercise_playing;
                    },
                ),
                clickable(
                    el("div", text("Step")).attr("class", "t-btn"),
                    |ui: &mut UiState, _| ui.stage.exercise_advance(),
                ),
                clickable(
                    el("div", text("<")).attr("class", "t-btn t-narrow"),
                    |ui: &mut UiState, _| ui.stage.exercise_nudge_fret(-1),
                ),
                el("div", text(format!("fret {}", board.starting_fret))).attr("class", "t-readout"),
                clickable(
                    el("div", text(">")).attr("class", "t-btn t-narrow"),
                    |ui: &mut UiState, _| ui.stage.exercise_nudge_fret(1),
                ),
            ),
        )
        .attr("class", "transport"),
    );
    let dots: HashMap<(usize, u8), (usize, String)> = board
        .dots
        .iter()
        .map(|d| ((d.string_index, d.fret), (d.recency, d.label.clone())))
        .collect();
    let rows: Vec<UiChild> = (0..state.string_count())
        .map(|string_index| {
            let cells: Vec<UiChild> = (0..=state.fret_count)
                .map(|fret| {
                    let cell_class = if fret == 0 { "fret nut-gap" } else { "fret" };
                    match dots.get(&(string_index, fret)) {
                        Some((recency, label)) => {
                            let dot_class = if *recency == 0 {
                                "dot step-dot"
                            } else {
                                "dot trail-dot"
                            };
                            Box::new(
                                el(
                                    "div",
                                    (el("div", text(label.clone())).attr("class", dot_class),),
                                )
                                .attr("class", cell_class),
                            ) as UiChild
                        }
                        None => Box::new(el("div", ()).attr("class", cell_class)) as UiChild,
                    }
                })
                .collect();
            Box::new(el("div", cells).attr("class", "string")) as UiChild
        })
        .collect();
    Box::new(
        el(
            "div",
            (
                deck,
                el("div", rows),
                el(
                    "div",
                    text(format!(
                        "{} — step {}/{} · {}",
                        board.name,
                        board.step + 1,
                        board.total,
                        board.description,
                    )),
                )
                .attr("class", "scale-name"),
            ),
        )
        .attr("class", "board"),
    )
}

fn progression_board_view(ui: &UiState) -> UiChild {
    let Some(board) = ui.stage.progression_board() else {
        return Box::new(
            el(
                "div",
                el("div", text("Pick a progression from the list.")).attr("class", "placeholder"),
            )
            .attr("class", "board"),
        );
    };
    let cards: Vec<UiChild> = board
        .cards
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let class = if c.is_expanded {
                "prog-card prog-card-active"
            } else {
                "prog-card"
            };
            Box::new(clickable(
                el(
                    "div",
                    (
                        el("div", text(c.numeral.clone())).attr("class", "prog-numeral"),
                        el("div", text(c.chord_label.clone())).attr("class", "prog-chord"),
                    ),
                )
                .attr("class", class),
                move |ui: &mut UiState, _| {
                    ui.stage.progression_expand(i);
                },
            )) as UiChild
        })
        .collect();
    let dots: HashMap<(usize, u8), (bool, String)> = board
        .dots
        .iter()
        .map(|d| ((d.string_index, d.fret), (d.is_root, d.label.clone())))
        .collect();
    let state = &ui.stage;
    let rows: Vec<UiChild> = (0..state.string_count())
        .map(|string_index| {
            let cells: Vec<UiChild> = (0..=state.fret_count)
                .map(|fret| {
                    let cell_class = if fret == 0 { "fret nut-gap" } else { "fret" };
                    match dots.get(&(string_index, fret)) {
                        Some((is_root, label)) => {
                            let dot_class = if *is_root { "dot root-dot" } else { "dot" };
                            Box::new(
                                el(
                                    "div",
                                    (el("div", text(label.clone())).attr("class", dot_class),),
                                )
                                .attr("class", cell_class),
                            ) as UiChild
                        }
                        None => Box::new(el("div", ()).attr("class", cell_class)) as UiChild,
                    }
                })
                .collect();
            Box::new(el("div", cells).attr("class", "string")) as UiChild
        })
        .collect();
    Box::new(
        el(
            "div",
            (
                el("div", cards).attr("class", "prog-cards"),
                el("div", rows),
                el(
                    "div",
                    text(format!(
                        "{} — {} · showing {}",
                        state.material_name(),
                        board.description,
                        board.expanded_label,
                    )),
                )
                .attr("class", "scale-name"),
            ),
        )
        .attr("class", "board"),
    )
}

fn arpeggio_board_view(ui: &UiState) -> UiChild {
    use woodshed_core::arpeggio::ArpeggioDirection;
    let state = &ui.stage;
    let board = state.arpeggio_board();
    let dir_label = match board.direction {
        ArpeggioDirection::UpDown => "Up-Down",
        ArpeggioDirection::Up => "Up",
        ArpeggioDirection::Down => "Down",
    };
    let play_label = if state.arpeggio_playing {
        "Pause"
    } else {
        "Run"
    };
    let deck: UiChild = Box::new(
        el(
            "div",
            (
                clickable(
                    el("div", text(play_label)).attr("class", "t-btn"),
                    |ui: &mut UiState, _| {
                        ui.stage.arpeggio_playing = !ui.stage.arpeggio_playing;
                    },
                ),
                clickable(
                    el("div", text("Step")).attr("class", "t-btn"),
                    |ui: &mut UiState, _| ui.stage.arpeggio_advance(),
                ),
                clickable(
                    el("div", text(dir_label)).attr("class", "t-btn"),
                    |ui: &mut UiState, _| ui.stage.arpeggio_cycle_direction(),
                ),
                clickable(
                    el("div", text(board.inversion_label.clone())).attr("class", "t-btn"),
                    |ui: &mut UiState, _| ui.stage.arpeggio_cycle_inversion(),
                ),
                clickable(
                    el("div", text("<")).attr("class", "t-btn t-narrow"),
                    |ui: &mut UiState, _| {
                        let b = ui.stage.arpeggio_board();
                        let prev = (b.position_idx + b.shape_count - 1) % b.shape_count;
                        ui.stage.arpeggio_select_position(prev);
                    },
                ),
                el(
                    "div",
                    text(format!(
                        "shape {}/{}",
                        board.position_idx + 1,
                        board.shape_count
                    )),
                )
                .attr("class", "t-readout"),
                clickable(
                    el("div", text(">")).attr("class", "t-btn t-narrow"),
                    |ui: &mut UiState, _| {
                        let b = ui.stage.arpeggio_board();
                        let next = (b.position_idx + 1) % b.shape_count;
                        ui.stage.arpeggio_select_position(next);
                    },
                ),
            ),
        )
        .attr("class", "transport"),
    );
    let dots: HashMap<(usize, u8), (bool, bool, String)> = board
        .dots
        .iter()
        .map(|d| {
            (
                (d.string_index, d.fret),
                (d.is_root, d.is_current, d.label.clone()),
            )
        })
        .collect();
    let rows: Vec<UiChild> = (0..state.string_count())
        .map(|string_index| {
            let cells: Vec<UiChild> = (0..=state.fret_count)
                .map(|fret| {
                    let cell_class = if fret == 0 { "fret nut-gap" } else { "fret" };
                    match dots.get(&(string_index, fret)) {
                        Some((is_root, is_current, label)) => {
                            let dot_class = if *is_current {
                                "dot step-dot"
                            } else if *is_root {
                                "dot root-dot"
                            } else {
                                "dot"
                            };
                            Box::new(
                                el(
                                    "div",
                                    (el("div", text(label.clone())).attr("class", dot_class),),
                                )
                                .attr("class", cell_class),
                            ) as UiChild
                        }
                        None => Box::new(el("div", ()).attr("class", cell_class)) as UiChild,
                    }
                })
                .collect();
            Box::new(el("div", cells).attr("class", "string")) as UiChild
        })
        .collect();
    Box::new(
        el(
            "div",
            (
                deck,
                el("div", rows),
                el(
                    "div",
                    text(format!(
                        "{} — frets {}-{}, step {}/{}",
                        state.material_name(),
                        board.start_fret,
                        board.start_fret + woodshed_core::arpeggio::ARP_SHAPE_SPAN,
                        board.step + 1,
                        board.walk_len,
                    )),
                )
                .attr("class", "scale-name"),
            ),
        )
        .attr("class", "board"),
    )
}

pub(super) fn board(ui: &UiState) -> UiChild {
    let state = &ui.stage;
    if state.lens == Lens::Arpeggios {
        return arpeggio_board_view(ui);
    }
    if state.lens == Lens::Progressions {
        return progression_board_view(ui);
    }
    if state.lens == Lens::Exercises {
        return exercise_board_view(ui);
    }
    let dots: HashMap<(usize, u8), (bool, String)> = state
        .dots()
        .into_iter()
        .map(|d| ((d.string_index, d.fret), (d.is_root, d.label)))
        .collect();
    let rows: Vec<UiChild> = (0..state.string_count())
        .map(|string_index| {
            let cells: Vec<UiChild> = (0..=state.fret_count)
                .map(|fret| {
                    let cell_class = if fret == 0 { "fret nut-gap" } else { "fret" };
                    match dots.get(&(string_index, fret)) {
                        Some((is_root, label)) => {
                            let dot_class = if *is_root { "dot root-dot" } else { "dot" };
                            Box::new(
                                el(
                                    "div",
                                    (el("div", text(label.clone())).attr("class", dot_class),),
                                )
                                .attr("class", cell_class),
                            ) as UiChild
                        }
                        None => Box::new(el("div", ()).attr("class", cell_class)) as UiChild,
                    }
                })
                .collect();
            Box::new(el("div", cells).attr("class", "string")) as UiChild
        })
        .collect();
    Box::new(
        el(
            "div",
            (
                el("div", rows),
                el(
                    "div",
                    text(format!(
                        "{} — {} positions",
                        state.material_name(),
                        dots.len()
                    )),
                )
                .attr("class", "scale-name"),
            ),
        )
        .attr("class", "board"),
    )
}

fn stage_screen(ui: &UiState) -> UiChild {
    if ui.stage_page == StagePage::Templates {
        return Box::new(el("div", (header(ui), lens_strip(ui), templates::screen(ui))));
    }
    let body: UiChild = match (ui.board_layout, ui.viewport) {
        (BoardLayout::TwoPane, ViewportClass::Wide) => {
            Box::new(el("div", (sidebar(ui), board(ui), related::panel(ui))).attr("class", "body"))
        }
        (BoardLayout::FullCanvas, _) => board(ui),
        // Preserve the catalog on smaller surfaces without narrowing the neck.
        _ => Box::new(el(
            "div",
            (
                board(ui),
                related::panel(ui),
                el("div", (sidebar(ui),)).attr("class", "side-strip"),
            ),
        )),
    };
    Box::new(el("div", (header(ui), transport(ui), lens_strip(ui), body)))
}

fn tab_content(ui: &UiState) -> UiChild {
    match ui.section {
        AppSection::Stage => stage_screen(ui),
        AppSection::Rehearsal => rehearsal::screen(ui),
        AppSection::Looper => looper::screen(ui),
        AppSection::Tools => tools::screen(ui),
        AppSection::Settings => settings::screen(ui),
    }
}

/// The corpus search field + its results dropdown — a small always-on
/// field in the nav row (no toolbar reorganization). Typing recomputes
/// the results; clicking one jumps to its lens/tab.
fn search_view(ui: &UiState) -> UiChild {
    let field = map_state(text_field(&ui.search), |ui: &mut UiState| &mut ui.search);
    let query = ui.search.text().to_string();
    let results = search_corpus(&query, 8);
    let list = (!results.is_empty()).then(|| {
        let items: Vec<UiChild> = results
            .into_iter()
            .map(|r| {
                let hit = r.hit;
                Box::new(clickable(
                    el(
                        "div",
                        (
                            el("span", text(r.label)).attr("class", "search-label"),
                            el("span", text(r.kind)).attr("class", "search-kind"),
                        ),
                    )
                    .attr("class", "search-item"),
                    move |ui: &mut UiState, _| ui.apply_search_hit(hit),
                )) as UiChild
            })
            .collect();
        el("div", items)
            .attr("class", "search-list")
            .attr("style", "position: absolute; top: 100%; right: 0;")
    });
    Box::new(
        el("div", (field, list))
            .attr("class", "search-wrap")
            .attr("style", "position: relative;"),
    )
}

/// Shared product root. Desktop hosts add CSD chrome and resize affordances;
/// browser hosts supply neither. Boxed so hosts can name the runner view type.
pub fn stage_root(ui: &UiState) -> UiChild {
    let mut nav: Vec<UiChild> = AppSection::ALL
        .iter()
        .map(|&section| pill(section, section == ui.section))
        .collect();
    nav.push(Box::new(el("div", ()).attr("class", "nav-spacer")));
    nav.push(search_view(ui));
    Box::new(
        el(
            "div",
            (el("div", nav).attr("class", "pills"), tab_content(ui)),
        )
        .attr(
            "class",
            format!("root {} {}", ui.board_layout.class(), ui.viewport.class()),
        ),
    )
}
