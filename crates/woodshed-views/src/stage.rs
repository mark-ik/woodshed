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
use woodshed_core::{tunings, Lens, StageState, ROOT_NAMES};
use xilem_serval::{
    clickable, el, map_state, select, text, AnyView, SelectState, ServalCtx, ServalElement,
};

/// Runner state: the portable core slice plus view-layer widget state.
pub struct UiState {
    pub stage: StageState,
    pub transport: TransportState,
    pub tuner: TunerState,
    /// A device/stream failure reported by the host's audio backend.
    pub audio_error: Option<String>,
    pub tuning_dd: SelectState,
    pub root_dd: SelectState,
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
            transport: TransportState::default(),
            tuner: TunerState::default(),
            audio_error: None,
            tuning_dd: SelectState::new(stage.tuning_idx),
            root_dd: SelectState::new(stage.root_idx),
            stage,
        }
    }

    /// Mirror dropdown picks into the core state. Hosts call this after
    /// every dispatch (the `select` widget mutates only its own
    /// `SelectState`).
    pub fn sync(&mut self) {
        self.stage.set_tuning(self.tuning_dd.selected);
        self.stage.set_root(self.root_dd.selected);
    }
}

/// Boxed heterogeneous child view over [`UiState`].
pub type UiChild = Box<dyn AnyView<UiState, (), ServalCtx, ServalElement>>;

fn pill(label: &str, active: bool) -> UiChild {
    Box::new(el("span", text(label.to_string())).attr(
        "class",
        if active { "pill pill-active" } else { "pill" },
    ))
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
            ),
        )
        .attr("class", "transport"),
    )
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

/// The Stage screen root. Boxed so hosts can name the runner's view type
/// on stable Rust (`fn(&UiState) -> UiChild`).
pub fn stage_root(ui: &UiState) -> UiChild {
    Box::new(
        el(
            "div",
            (
                el("div", text("Woodshed")).attr("class", "title"),
                el(
                    "div",
                    (
                        pill("Stage", true),
                        pill("Practice", false),
                        pill("Song", false),
                        pill("Rehearsal", false),
                        pill("Settings", false),
                    ),
                )
                .attr("class", "pills"),
                header(ui),
                transport(ui),
                lens_strip(ui),
                el("div", (sidebar(ui), board(ui))).attr("class", "body"),
                el(
                    "div",
                    text("serval host S2 — lenses, dropdowns, keyboard"),
                )
                .attr("class", "caption"),
            ),
        )
        .attr("class", "root"),
    )
}
