//! The Stage screen over live state (S2).
//!
//! Header dropdowns (tuning, root — xilem_serval `select`), the lens strip
//! (Scale / Chord / Arpeggio / Progression / Exercise), a per-lens catalog
//! sidebar, and the fretboard rendered as DOM dots. The runner state is
//! [`UiState`]: the portable `woodshed_core::StageState` plus the
//! view-layer dropdown state; hosts call [`UiState::sync`] after any
//! dispatch so dropdown picks land in the core state.

use std::collections::HashMap;

use woodshed_core::audio::{TransportState, TunerState};
use woodshed_core::search::{search_corpus, SearchHit};
use woodshed_core::storage::{PersistedSession, Tab};
use woodshed_core::song::{song_from_progression, SongDoc, SECTION_LABELS};
use woodshed_core::{set_from_practice, step_set, tunings, Lens, StageState, ROOT_NAMES};
use woodshedding::rehearsal::{FretWindow, Hold, LoopMode, Recipe, Set, Touch};
use xilem_serval::{
    clickable, el, map_state, select, text, text_field, AnyView, SelectState, ServalCtx,
    ServalElement, TextInput,
};

use crate::theme::ThemeMode;

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
    pub tab: Tab,
    pub theme: ThemeMode,
    pub board_layout: BoardLayout,
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
            rehearsal_running: false,
            song: SongDoc::default(),
            song_playing: false,
            song_bar_live: 0,
            song_edit_cursor: 0,
            song_rewind_requested: false,
            tab: Tab::Stage,
            theme: ThemeMode::default(),
            board_layout: BoardLayout::default(),
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
                self.tab = Tab::Stage;
            }
            SearchHit::Chord(i) => {
                self.stage.set_lens(Lens::Chords);
                self.stage.select_chord(i);
                self.tab = Tab::Stage;
            }
            SearchHit::Arpeggio(i) => {
                self.stage.set_lens(Lens::Arpeggios);
                self.stage.select_arpeggio(i);
                self.tab = Tab::Stage;
            }
            SearchHit::Progression(i) => {
                self.stage.set_lens(Lens::Progressions);
                self.stage.select_progression(i);
                self.tab = Tab::Stage;
            }
            SearchHit::Exercise(i) => {
                self.stage.set_lens(Lens::Exercises);
                self.stage.select_exercise(i);
                self.tab = Tab::Stage;
            }
            SearchHit::Recipe(i) => {
                if let Some(ps) = woodshedding::practice::catalog().get(i) {
                    self.set = set_from_practice(ps);
                    self.tab = Tab::Rehearsal;
                }
            }
            SearchHit::Tuning(i) => {
                self.stage.set_tuning(i);
                self.tuning_dd = SelectState::new(self.stage.tuning_idx);
                self.tab = Tab::Stage;
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

    /// Pitches + shape for the on-demand "♪ Hear" preview, resolved from
    /// context: the current rehearsal card on the Rehearsal tab, else the
    /// active Stage lens. Empty pitches = nothing to voice. The host
    /// consumes [`Self::preview_requested`] and calls this.
    pub fn preview_voicing(&self) -> (Vec<f32>, f32, f32) {
        if self.tab == Tab::Rehearsal && !self.set.cards.is_empty() {
            let cursor = self.set.cursor.min(self.set.cards.len() - 1);
            return self.stage.card_voicing(&self.set.cards[cursor]);
        }
        self.stage.voicing_preview()
    }

    /// Snapshot the persistable subset (the W0.2 seam's payload).
    pub fn to_persisted(&self) -> PersistedSession {
        PersistedSession::capture(
            &self.stage,
            self.tab,
            self.transport.bpm,
            self.theme.label(),
            self.board_layout.label(),
            &self.set,
            &self.song,
        )
    }

    /// Restore a persisted session (indices clamp; unknown theme names
    /// fall back to the default).
    pub fn apply_persisted(&mut self, session: &PersistedSession) {
        session.restore(&mut self.stage);
        self.set = session.set.clone();
        self.song = session.song.clone();
        self.tab = session.tab;
        self.transport.bpm = session.bpm.clamp(30.0, 300.0);
        self.theme = ThemeMode::from_name(&session.theme).unwrap_or_default();
        self.board_layout =
            BoardLayout::from_name(&session.board_layout).unwrap_or_default();
        self.tuning_dd = SelectState::new(self.stage.tuning_idx);
        self.root_dd = SelectState::new(self.stage.root_idx);
    }
}

/// Boxed heterogeneous child view over [`UiState`].
pub type UiChild = Box<dyn AnyView<UiState, (), ServalCtx, ServalElement>>;

fn pill(tab: Tab, active: bool) -> UiChild {
    Box::new(clickable(
        el("span", text(tab.label())).attr(
            "class",
            if active { "pill pill-active" } else { "pill" },
        ),
        move |ui: &mut UiState, _| {
            ui.tab = tab;
        },
    ))
}

/// MIDI device panel (audio-depth slice 13): port pickers, clock-slave /
/// clock-master toggles, and a live status + event readout. The host
/// realizes the selections through the `MidiBackend` seam.
fn midi_panel(ui: &UiState) -> UiChild {
    let in_opts: Vec<&str> = std::iter::once("None")
        .chain(ui.midi.input_ports.iter().map(|s| s.as_str()))
        .collect();
    let out_opts: Vec<&str> = std::iter::once("None")
        .chain(ui.midi.output_ports.iter().map(|s| s.as_str()))
        .collect();
    let input_dd = map_state(select(&ui.midi.input_dd, &in_opts), |ui: &mut UiState| {
        &mut ui.midi.input_dd
    });
    let output_dd = map_state(select(&ui.midi.output_dd, &out_opts), |ui: &mut UiState| {
        &mut ui.midi.output_dd
    });
    let slave_label = if ui.midi.clock_slave {
        "Sync to clock: on"
    } else {
        "Sync to clock: off"
    };
    let out_label = if ui.midi.clock_out {
        "Send clock: on"
    } else {
        "Send clock: off"
    };
    let clock = ui
        .midi
        .clock_bpm
        .map(|b| format!("clock {b:.1} bpm"))
        .unwrap_or_else(|| "no clock".to_string());
    let status = format!(
        "in: {} · out: {} · {clock}",
        ui.midi.connected_in.as_deref().unwrap_or("—"),
        ui.midi.connected_out.as_deref().unwrap_or("—"),
    );
    let events_line = if ui.midi.events.is_empty() {
        "(no events)".to_string()
    } else {
        ui.midi.events.join("    ")
    };
    Box::new(el(
        "div",
        (
            el("div", text("MIDI"))
                .attr("class", "settings-heading settings-gap"),
            el(
                "div",
                (
                    el("span", text("In")).attr("class", "header-label"),
                    input_dd,
                    el("span", ()).attr("class", "header-gap"),
                    el("span", text("Out")).attr("class", "header-label"),
                    output_dd,
                ),
            )
            .attr("class", "header-row"),
            el(
                "div",
                (
                    clickable(
                        el("div", text(slave_label)).attr("class", "t-btn"),
                        |ui: &mut UiState, _| ui.midi.clock_slave = !ui.midi.clock_slave,
                    ),
                    clickable(
                        el("div", text(out_label)).attr("class", "t-btn"),
                        |ui: &mut UiState, _| ui.midi.clock_out = !ui.midi.clock_out,
                    ),
                    clickable(
                        el("div", text("Refresh ports")).attr("class", "t-btn"),
                        |ui: &mut UiState, _| ui.midi.refresh_requested = true,
                    ),
                ),
            )
            .attr("class", "transport"),
            el("div", text(status)).attr("class", "settings-line"),
            el("div", text(events_line)).attr("class", "settings-line midi-events"),
        ),
    ))
}

fn settings_screen(ui: &UiState) -> UiChild {
    let themes: Vec<UiChild> = ThemeMode::ALL
        .iter()
        .map(|&mode| {
            let class = if mode == ui.theme {
                "side-item side-active"
            } else {
                "side-item"
            };
            Box::new(clickable(
                el("div", text(mode.label())).attr("class", class),
                move |ui: &mut UiState, _| {
                    ui.theme = mode;
                },
            )) as UiChild
        })
        .collect();
    let layouts: Vec<UiChild> = BoardLayout::ALL
        .iter()
        .map(|&layout| {
            let class = if layout == ui.board_layout {
                "side-item side-active"
            } else {
                "side-item"
            };
            Box::new(clickable(
                el("div", text(layout.label())).attr("class", class),
                move |ui: &mut UiState, _| {
                    ui.board_layout = layout;
                },
            )) as UiChild
        })
        .collect();
    let audio_line = match &ui.audio_error {
        Some(err) => format!("Audio: {err}"),
        None => "Audio: output and input streams open.".to_string(),
    };
    Box::new(
        el(
            "div",
            (
                el(
                    "div",
                    (
                        el("div", text("Theme")).attr("class", "settings-heading"),
                        el("div", themes),
                        el("div", text("Fretboard layout"))
                            .attr("class", "settings-heading settings-gap"),
                        el("div", layouts),
                    ),
                )
                .attr("class", "side"),
                el(
                    "div",
                    (
                        el("div", text("Session")).attr("class", "settings-heading"),
                        el("div", text(audio_line)).attr("class", "settings-line"),
                        el(
                            "div",
                            text(
                                "Selections, tempo, and theme persist to \
                                 serval-state.json and restore on launch.",
                            ),
                        )
                        .attr("class", "settings-line"),
                        midi_panel(ui),
                    ),
                )
                .attr("class", "board"),
            ),
        )
        .attr("class", "body"),
    )
}

fn header(ui: &UiState) -> UiChild {
    let tuning_names: Vec<&str> = tunings().iter().map(|t| t.name).collect();
    let tuning_dd = map_state(
        select(&ui.tuning_dd, &tuning_names),
        |ui: &mut UiState| &mut ui.tuning_dd,
    );
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
                el("div", text(format!("{:.0} bpm", ui.transport.bpm)))
                    .attr("class", "t-readout"),
                clickable(
                    el("div", text("+")).attr("class", "t-btn t-narrow"),
                    |ui: &mut UiState, _| ui.transport.nudge_bpm(5.0),
                ),
                clickable(
                    el("div", text("♪ Hear")).attr("class", "t-btn t-hear"),
                    |ui: &mut UiState, _| ui.preview_requested = true,
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
                    el("div", text("+ Rehearse")).attr("class", "t-btn"),
                    |ui: &mut UiState, _| {
                        if let Some(card) = ui.stage.card_from_lens() {
                            ui.set.push(card);
                        }
                    },
                ),
                el("div", text(format!("{} cards", ui.set.cards.len())))
                    .attr("class", "t-readout"),
            ),
        )
        .attr("class", "transport"),
    )
}

fn recipe_line(recipe: &Recipe) -> String {
    match recipe {
        Recipe::Progression { name, .. } => format!("from {name}"),
        Recipe::Exercise { name } => format!("from {name}"),
        Recipe::PracticeSet { name } => format!("from {name}"),
        Recipe::Song { name, bar } => format!("from {name} · bar {bar}"),
    }
}

fn rehearsal_screen(ui: &UiState) -> UiChild {
    if ui.set.cards.is_empty() {
        return Box::new(
            el(
                "div",
                el(
                    "div",
                    text("The set is empty. Add cards from Stage with + Rehearse."),
                )
                .attr("class", "placeholder"),
            )
            .attr("class", "board"),
        );
    }
    let cursor = ui.set.cursor.min(ui.set.cards.len() - 1);
    let loop_label = match ui.set.loop_mode {
        LoopMode::Off => "Loop: off",
        LoopMode::All => "Loop: all",
    };
    let deck: UiChild = Box::new(
        el(
            "div",
            (
                clickable(
                    el(
                        "div",
                        text(if ui.rehearsal_running { "Pause" } else { "Run" }),
                    )
                    .attr("class", "t-btn"),
                    |ui: &mut UiState, _| {
                        ui.rehearsal_running = !ui.rehearsal_running;
                    },
                ),
                clickable(
                    el("div", text("Prev")).attr("class", "t-btn"),
                    |ui: &mut UiState, _| {
                        step_set(&mut ui.set, -1);
                    },
                ),
                clickable(
                    el("div", text("Next")).attr("class", "t-btn"),
                    |ui: &mut UiState, _| {
                        step_set(&mut ui.set, 1);
                    },
                ),
                clickable(
                    el("div", text("♪ Hear")).attr("class", "t-btn t-hear"),
                    |ui: &mut UiState, _| ui.preview_requested = true,
                ),
                clickable(
                    el("div", text(loop_label)).attr("class", "t-btn"),
                    |ui: &mut UiState, _| {
                        ui.set.loop_mode = match ui.set.loop_mode {
                            LoopMode::Off => LoopMode::All,
                            LoopMode::All => LoopMode::Off,
                        };
                    },
                ),
                clickable(
                    el("div", text("Remove")).attr("class", "t-btn"),
                    |ui: &mut UiState, _| {
                        let idx = ui.set.cursor;
                        ui.set.remove(idx);
                    },
                ),
                el(
                    "div",
                    text(format!("card {}/{}", cursor + 1, ui.set.cards.len())),
                )
                .attr("class", "t-readout"),
            ),
        )
        .attr("class", "transport"),
    );
    // The measured filmstrip (redesign P5): every card with its tag,
    // provenance, and touch; played cards dim behind the cursor
    // (engine group opacity), the current card ringed; the strip is a
    // horizontal scroll container (engine element scroll).
    let films: Vec<UiChild> = ui
        .set
        .cards
        .iter()
        .enumerate()
        .map(|(i, card)| {
            let class = match i.cmp(&cursor) {
                std::cmp::Ordering::Less => "film-card film-played",
                std::cmp::Ordering::Equal => "film-card film-current",
                std::cmp::Ordering::Greater => "film-card",
            };
            let provenance = card
                .from
                .as_ref()
                .map(recipe_line)
                .unwrap_or_else(|| "hand-added".to_string());
            let touch = match &card.touch {
                woodshedding::rehearsal::Touch::Block => "block".to_string(),
                woodshedding::rehearsal::Touch::Arpeggiate { direction, .. } => {
                    format!("arpeggiate {}", direction.label())
                }
            };
            Box::new(clickable(
                el(
                    "div",
                    (
                        el("div", text(card.material.tag())).attr("class", "film-tag"),
                        el("div", text(card.label.clone())).attr("class", "film-label"),
                        el("div", text(touch)).attr("class", "film-meta"),
                        el("div", text(provenance)).attr("class", "film-meta"),
                    ),
                )
                .attr("class", class),
                move |ui: &mut UiState, _| {
                    ui.set.cursor = i;
                },
            )) as UiChild
        })
        .collect();
    // Card editor (S4 slice 9): touch, dwell, tempo override, and the
    // pinned hand position of the card under the cursor. Edits write
    // straight into the set (persisted with the session).
    let card_now = &ui.set.cards[cursor];
    let touch_label = match &card_now.touch {
        Touch::Block => "Touch: block".to_string(),
        Touch::Arpeggiate { direction, .. } => {
            format!("Touch: arp {}", direction.label())
        }
    };
    let hold_label = match card_now.timing.hold {
        Hold::Manual => "Hold: manual".to_string(),
        Hold::Bars(n) => format!("Hold: {n} bars"),
        Hold::Seconds(s) => format!("Hold: {s:.0}s"),
        Hold::Reps(r) => format!("Hold: {r} reps"),
    };
    let bpm_label = match card_now.timing.bpm {
        Some(b) => format!("{b:.0} bpm"),
        None => "transport bpm".to_string(),
    };
    let window_label = match card_now.setting.fret_window {
        Some(w) => format!("frets {}-{}", w.start, w.start + w.span),
        None => "whole neck".to_string(),
    };
    let editor: UiChild = Box::new(
        el(
            "div",
            (
                clickable(
                    el("div", text(touch_label)).attr("class", "t-btn"),
                    move |ui: &mut UiState, _| {
                        let cursor = ui.set.cursor.min(ui.set.cards.len() - 1);
                        let card = &mut ui.set.cards[cursor];
                        card.touch = match &card.touch {
                            Touch::Block => Touch::Arpeggiate {
                                direction: Default::default(),
                                inversion: 0,
                            },
                            Touch::Arpeggiate { direction, inversion } => {
                                // Cycle direction; back to block after Down.
                                use woodshed_core::arpeggio::ArpeggioDirection as D;
                                match direction {
                                    D::UpDown => Touch::Arpeggiate {
                                        direction: D::Up,
                                        inversion: *inversion,
                                    },
                                    D::Up => Touch::Arpeggiate {
                                        direction: D::Down,
                                        inversion: *inversion,
                                    },
                                    D::Down => Touch::Block,
                                }
                            }
                        };
                    },
                ),
                clickable(
                    el("div", text(hold_label)).attr("class", "t-btn"),
                    |ui: &mut UiState, _| {
                        let cursor = ui.set.cursor.min(ui.set.cards.len() - 1);
                        let card = &mut ui.set.cards[cursor];
                        card.timing.hold = match card.timing.hold {
                            Hold::Manual => Hold::Bars(2),
                            Hold::Bars(2) => Hold::Bars(4),
                            Hold::Bars(4) => Hold::Bars(8),
                            Hold::Bars(_) => Hold::Seconds(30.0),
                            Hold::Seconds(_) => Hold::Manual,
                            Hold::Reps(_) => Hold::Manual,
                        };
                    },
                ),
                clickable(
                    el("div", text("-")).attr("class", "t-btn t-narrow"),
                    |ui: &mut UiState, _| {
                        let cursor = ui.set.cursor.min(ui.set.cards.len() - 1);
                        let card = &mut ui.set.cards[cursor];
                        let base = card.timing.bpm.unwrap_or(ui.transport.bpm);
                        card.timing.bpm = Some((base - 5.0).clamp(30.0, 300.0));
                    },
                ),
                el("div", text(bpm_label)).attr("class", "t-readout"),
                clickable(
                    el("div", text("+")).attr("class", "t-btn t-narrow"),
                    |ui: &mut UiState, _| {
                        let cursor = ui.set.cursor.min(ui.set.cards.len() - 1);
                        let card = &mut ui.set.cards[cursor];
                        let base = card.timing.bpm.unwrap_or(ui.transport.bpm);
                        card.timing.bpm = Some((base + 5.0).clamp(30.0, 300.0));
                    },
                ),
                clickable(
                    el("div", text("<")).attr("class", "t-btn t-narrow"),
                    |ui: &mut UiState, _| {
                        let cursor = ui.set.cursor.min(ui.set.cards.len() - 1);
                        let card = &mut ui.set.cards[cursor];
                        card.setting.fret_window = match card.setting.fret_window {
                            None => Some(FretWindow { start: 0, span: 4 }),
                            Some(w) => Some(FretWindow {
                                start: w.start.saturating_sub(1),
                                span: w.span,
                            }),
                        };
                    },
                ),
                el("div", text(window_label)).attr("class", "t-readout"),
                clickable(
                    el("div", text(">")).attr("class", "t-btn t-narrow"),
                    |ui: &mut UiState, _| {
                        let cursor = ui.set.cursor.min(ui.set.cards.len() - 1);
                        let card = &mut ui.set.cards[cursor];
                        card.setting.fret_window = match card.setting.fret_window {
                            None => Some(FretWindow { start: 0, span: 4 }),
                            Some(w) => Some(FretWindow {
                                start: (w.start + 1).min(ui.stage.fret_count - w.span),
                                span: w.span,
                            }),
                        };
                    },
                ),
                clickable(
                    el("div", text("free")).attr("class", "t-btn"),
                    |ui: &mut UiState, _| {
                        let cursor = ui.set.cursor.min(ui.set.cards.len() - 1);
                        ui.set.cards[cursor].setting.fret_window = None;
                    },
                ),
            ),
        )
        .attr("class", "transport"),
    );

    // Current card's material on the big board.
    let card = &ui.set.cards[cursor];
    let dots: HashMap<(usize, u8), (bool, String)> = ui
        .stage
        .dots_for_card(card)
        .into_iter()
        .map(|d| ((d.string_index, d.fret), (d.is_root, d.label)))
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
                        None => {
                            Box::new(el("div", ()).attr("class", cell_class)) as UiChild
                        }
                    }
                })
                .collect();
            Box::new(el("div", cells).attr("class", "string")) as UiChild
        })
        .collect();
    Box::new(el(
        "div",
        (
            deck,
            el("div", films).attr("class", "filmstrip"),
            editor,
            el(
                "div",
                (
                    el("div", rows),
                    el("div", text(card.label.clone())).attr("class", "scale-name"),
                ),
            )
            .attr("class", "board"),
        ),
    ))
}

fn lens_strip(ui: &UiState) -> UiChild {
    let items: Vec<UiChild> = Lens::ALL
        .iter()
        .map(|&lens| {
            let class = if lens == ui.stage.lens {
                "lens lens-active"
            } else {
                "lens"
            };
            Box::new(clickable(
                el("div", text(lens.label())).attr("class", class),
                move |ui: &mut UiState, _| {
                    ui.stage.set_lens(lens);
                },
            )) as UiChild
        })
        .collect();
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
    let play_label = if state.exercise_playing { "Pause" } else { "Run" };
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
                el("div", text(format!("fret {}", board.starting_fret)))
                    .attr("class", "t-readout"),
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
                        None => {
                            Box::new(el("div", ()).attr("class", cell_class)) as UiChild
                        }
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
                el("div", text("Pick a progression from the list."))
                    .attr("class", "placeholder"),
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
                        None => {
                            Box::new(el("div", ()).attr("class", cell_class)) as UiChild
                        }
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
    let play_label = if state.arpeggio_playing { "Pause" } else { "Run" };
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
                    text(format!("shape {}/{}", board.position_idx + 1, board.shape_count)),
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
                        None => {
                            Box::new(el("div", ()).attr("class", cell_class)) as UiChild
                        }
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

fn board(ui: &UiState) -> UiChild {
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
                        None => {
                            Box::new(el("div", ()).attr("class", cell_class)) as UiChild
                        }
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

fn practice_screen(ui: &UiState) -> UiChild {
    let tiles: Vec<UiChild> = woodshedding::practice::catalog()
        .into_iter()
        .enumerate()
        .map(|(i, ps)| {
            let meta = format!("{} cards · tap to fill the set", ps.items.len());
            let name = ps.name.clone();
            let desc = ps.description.clone();
            let _ = i;
            Box::new(clickable(
                el(
                    "div",
                    (
                        el("div", text(name)).attr("class", "recipe-name"),
                        el("div", text(desc)).attr("class", "recipe-desc"),
                        el("div", text(meta)).attr("class", "recipe-meta"),
                    ),
                )
                .attr("class", "recipe-tile"),
                move |ui: &mut UiState, _| {
                    ui.set = set_from_practice(&ps);
                    ui.tab = Tab::Rehearsal;
                },
            )) as UiChild
        })
        .collect();
    Box::new(
        el(
            "div",
            (
                el("div", text("Recipes")).attr("class", "settings-heading"),
                el("div", tiles).attr("class", "recipe-grid"),
            ),
        )
        .attr("class", "board"),
    )
}

fn song_deck(ui: &UiState) -> UiChild {
    let from_prog_label = match ui.stage.progression_idx {
        Some(_) => "From progression",
        None => "From progression (pick one on Stage first)",
    };
    Box::new(
        el(
            "div",
            (
                clickable(
                    el(
                        "div",
                        text(if ui.song_playing { "Stop" } else { "Play" }),
                    )
                    .attr("class", "t-btn"),
                    |ui: &mut UiState, _| {
                        if !ui.song.is_empty() {
                            ui.song_playing = !ui.song_playing;
                        }
                    },
                ),
                clickable(
                    el("div", text("Rewind")).attr("class", "t-btn"),
                    |ui: &mut UiState, _| {
                        ui.song_rewind_requested = true;
                        ui.song_bar_live = 0;
                    },
                ),
                clickable(
                    el(
                        "div",
                        text(if ui.song.one_shot {
                            "Once"
                        } else {
                            "Loop"
                        }),
                    )
                    .attr("class", "t-btn"),
                    |ui: &mut UiState, _| ui.song.one_shot = !ui.song.one_shot,
                ),
                clickable(
                    el(
                        "div",
                        text(if ui.song.click {
                            "Click: on"
                        } else {
                            "Click: off"
                        }),
                    )
                    .attr("class", "t-btn"),
                    |ui: &mut UiState, _| ui.song.click = !ui.song.click,
                ),
                clickable(
                    el("div", text(from_prog_label)).attr("class", "t-btn"),
                    |ui: &mut UiState, _| {
                        if let Some(doc) =
                            song_from_progression(&ui.stage, ui.transport.bpm)
                        {
                            ui.song = doc;
                            ui.song_playing = false;
                            ui.song_bar_live = 0;
                            ui.song_edit_cursor = 0;
                            ui.song_rewind_requested = true;
                        }
                    },
                ),
                el(
                    "div",
                    text(if ui.song.name.is_empty() {
                        format!("{} bars", ui.song.bars.len())
                    } else {
                        format!("{} · {} bars", ui.song.name, ui.song.bars.len())
                    }),
                )
                .attr("class", "t-readout"),
            ),
        )
        .attr("class", "transport"),
    )
}

/// Add / duplicate / remove / reorder the bar under the edit cursor.
fn song_bar_ops(_ui: &UiState) -> UiChild {
    Box::new(
        el(
            "div",
            (
                clickable(
                    el("div", text("+ Bar")).attr("class", "t-btn"),
                    |ui: &mut UiState, _| {
                        ui.song_edit_cursor = ui.song.add_bar_after(ui.song_edit_cursor);
                    },
                ),
                clickable(
                    el("div", text("Dup")).attr("class", "t-btn"),
                    |ui: &mut UiState, _| {
                        ui.song_edit_cursor = ui.song.duplicate(ui.song_edit_cursor);
                    },
                ),
                clickable(
                    el("div", text("Remove")).attr("class", "t-btn"),
                    |ui: &mut UiState, _| {
                        ui.song_edit_cursor = ui.song.remove(ui.song_edit_cursor);
                    },
                ),
                clickable(
                    el("div", text("◀ Move")).attr("class", "t-btn t-narrow"),
                    |ui: &mut UiState, _| {
                        ui.song_edit_cursor = ui.song.move_bar(ui.song_edit_cursor, -1);
                    },
                ),
                clickable(
                    el("div", text("Move ▶")).attr("class", "t-btn t-narrow"),
                    |ui: &mut UiState, _| {
                        ui.song_edit_cursor = ui.song.move_bar(ui.song_edit_cursor, 1);
                    },
                ),
            ),
        )
        .attr("class", "transport"),
    )
}

/// Per-bar chord / tempo / meter / section editor for the cursor bar.
fn song_bar_editor(ui: &UiState) -> UiChild {
    let cursor = ui.song_edit_cursor.min(ui.song.bars.len().saturating_sub(1));
    let bar = &ui.song.bars[cursor];
    let root_label = if bar.formula_name.is_empty() {
        "silent".to_string()
    } else {
        bar.root_name()
    };
    let chord_label = if bar.formula_name.is_empty() {
        "—".to_string()
    } else {
        bar.formula_name.clone()
    };
    let section_label = if bar.label.is_empty() {
        "(no section)".to_string()
    } else {
        bar.label.clone()
    };
    Box::new(
        el(
            "div",
            (
                clickable(
                    el("div", text(format!("Root: {root_label}")))
                        .attr("class", "t-btn"),
                    move |ui: &mut UiState, _| {
                        ui.song.bars[cursor].cycle_root();
                    },
                ),
                clickable(
                    el("div", text(format!("Chord: {chord_label}")))
                        .attr("class", "t-btn"),
                    move |ui: &mut UiState, _| {
                        ui.song.bars[cursor].cycle_formula();
                    },
                ),
                clickable(
                    el("div", text("silent")).attr("class", "t-btn"),
                    move |ui: &mut UiState, _| {
                        ui.song.bars[cursor].toggle_silent();
                    },
                ),
                clickable(
                    el("div", text("-")).attr("class", "t-btn t-narrow"),
                    move |ui: &mut UiState, _| ui.song.bars[cursor].nudge_bpm(-5.0),
                ),
                el("div", text(format!("{:.0} bpm", bar.bpm)))
                    .attr("class", "t-readout"),
                clickable(
                    el("div", text("+")).attr("class", "t-btn t-narrow"),
                    move |ui: &mut UiState, _| ui.song.bars[cursor].nudge_bpm(5.0),
                ),
                clickable(
                    el("div", text(format!("{}/4", bar.beats)))
                        .attr("class", "t-btn"),
                    move |ui: &mut UiState, _| ui.song.bars[cursor].cycle_beats(),
                ),
                clickable(
                    el("div", text(format!("x{}", bar.length)))
                        .attr("class", "t-btn"),
                    move |ui: &mut UiState, _| ui.song.bars[cursor].cycle_length(),
                ),
                clickable(
                    el("div", text(section_label)).attr("class", "t-btn"),
                    move |ui: &mut UiState, _| {
                        let cur = &ui.song.bars[cursor].label;
                        let i = SECTION_LABELS
                            .iter()
                            .position(|s| *s == cur.as_str())
                            .unwrap_or(0);
                        let next = SECTION_LABELS[(i + 1) % SECTION_LABELS.len()];
                        ui.song.bars[cursor].label = next.to_string();
                    },
                ),
            ),
        )
        .attr("class", "transport"),
    )
}

fn song_screen(ui: &UiState) -> UiChild {
    let deck = song_deck(ui);
    if ui.song.is_empty() {
        return Box::new(el(
            "div",
            (
                deck,
                song_bar_ops(ui),
                el(
                    "div",
                    el(
                        "div",
                        text(
                            "No bars yet. '+ Bar' to start one, or pick a \
                             progression on Stage and 'From progression'.",
                        ),
                    )
                    .attr("class", "placeholder"),
                )
                .attr("class", "board"),
            ),
        ));
    }
    let play_cursor = ui.song_bar_live.min(ui.song.bars.len() - 1);
    let edit_cursor = ui.song_edit_cursor.min(ui.song.bars.len() - 1);
    let chips: Vec<UiChild> = ui
        .song
        .bars
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let mut class = String::from("bar-chip");
            if i == play_cursor && ui.song_playing {
                class.push_str(" bar-current");
            }
            if i == edit_cursor {
                class.push_str(" bar-edit");
            }
            let section = if b.label.is_empty() {
                String::new()
            } else {
                b.label.clone()
            };
            let chord = if b.chord_label.is_empty() {
                "·".to_string()
            } else {
                b.chord_label.clone()
            };
            Box::new(clickable(
                el(
                    "div",
                    (
                        el("div", text(section)).attr("class", "bar-label"),
                        el("div", text(chord)).attr("class", "bar-chord"),
                        el(
                            "div",
                            text(format!("{:.0} · {}/4", b.bpm, b.beats)),
                        )
                        .attr("class", "bar-meta"),
                    ),
                )
                .attr("class", class),
                move |ui: &mut UiState, _| {
                    ui.song_edit_cursor = i;
                },
            )) as UiChild
        })
        .collect();
    Box::new(el(
        "div",
        (
            deck,
            song_bar_ops(ui),
            el(
                "div",
                (
                    el("div", chips).attr("class", "bar-lane"),
                    song_bar_editor(ui),
                    el(
                        "div",
                        text(format!(
                            "editing bar {}/{} — chords voice at each bar top; \
                             the click follows each bar's tempo",
                            edit_cursor + 1,
                            ui.song.bars.len()
                        )),
                    )
                    .attr("class", "scale-name"),
                ),
            )
            .attr("class", "board"),
        ),
    ))
}

/// The window chrome (CSD): title, drag surface, window buttons. The
/// host consumes the request flags after dispatch.
fn chrome(_ui: &UiState) -> UiChild {
    Box::new(
        el(
            "div",
            (
                el("div", text("Woodshed")).attr("class", "chrome-title"),
                clickable(el("div", ()).attr("class", "chrome-drag"), |ui: &mut UiState,
                                                                       _| {
                    ui.chrome_drag = true;
                }),
                clickable(
                    el("div", text("–")).attr("class", "chrome-btn"),
                    |ui: &mut UiState, _| {
                        ui.chrome_minimize = true;
                    },
                ),
                clickable(
                    el("div", text("□")).attr("class", "chrome-btn"),
                    |ui: &mut UiState, _| {
                        ui.chrome_maximize = true;
                    },
                ),
                clickable(
                    el("div", text("×")).attr("class", "chrome-btn chrome-close"),
                    |ui: &mut UiState, _| {
                        ui.chrome_close = true;
                    },
                ),
            ),
        )
        .attr("class", "chrome"),
    )
}

fn stage_screen(ui: &UiState) -> UiChild {
    let body: UiChild = match ui.board_layout {
        BoardLayout::TwoPane => {
            Box::new(el("div", (sidebar(ui), board(ui))).attr("class", "body"))
        }
        BoardLayout::Hero => Box::new(el(
            "div",
            (
                board(ui),
                el("div", (sidebar(ui),)).attr("class", "side-strip"),
            ),
        )),
        BoardLayout::FullCanvas => board(ui),
    };
    Box::new(el(
        "div",
        (header(ui), transport(ui), lens_strip(ui), body),
    ))
}

fn tab_content(ui: &UiState) -> UiChild {
    match ui.tab {
        Tab::Stage => stage_screen(ui),
        Tab::Practice => practice_screen(ui),
        Tab::Song => song_screen(ui),
        Tab::Rehearsal => rehearsal_screen(ui),
        Tab::Settings => settings_screen(ui),
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

/// The app root. Boxed so hosts can name the runner's view type on
/// stable Rust (`fn(&UiState) -> UiChild`).
pub fn stage_root(ui: &UiState) -> UiChild {
    let mut nav: Vec<UiChild> = Tab::ALL
        .iter()
        .map(|&t| pill(t, t == ui.tab))
        .collect();
    nav.push(Box::new(el("div", ()).attr("class", "nav-spacer")));
    nav.push(search_view(ui));
    Box::new(
        el(
            "div",
            (
                chrome(ui),
                el("div", nav).attr("class", "pills"),
                tab_content(ui),
            ),
        )
        .attr("class", format!("root {}", ui.board_layout.class())),
    )
}
