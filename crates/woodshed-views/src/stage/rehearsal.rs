use std::collections::HashMap;

use woodshed_core::step_set;
use woodshedding::rehearsal::{FretWindow, Hold, LoopMode, Recipe, Touch};
use xilem_serval::{clickable, el, text};

use super::{UiChild, UiState};

fn recipe_line(recipe: &Recipe) -> String {
    match recipe {
        Recipe::Progression { name, .. } => format!("from {name}"),
        Recipe::Exercise { name } => format!("from {name}"),
        Recipe::PracticeSet { name } => format!("from {name}"),
        Recipe::Song { name, bar } => format!("from {name} · bar {bar}"),
    }
}

pub(super) fn screen(ui: &UiState) -> UiChild {
    if ui.set.cards.is_empty() {
        return Box::new(
            el(
                "div",
                el(
                    "div",
                    text("The set is empty. Stage material to begin."),
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
                        if !ui.rehearsal_running {
                            ui.record_rehearsal_cursor();
                        }
                        ui.rehearsal_running = !ui.rehearsal_running;
                    },
                ),
                clickable(
                    el("div", text("Prev")).attr("class", "t-btn"),
                    |ui: &mut UiState, _| {
                        step_set(&mut ui.set, -1);
                        if ui.rehearsal_running {
                            ui.record_rehearsal_cursor();
                        }
                    },
                ),
                clickable(
                    el("div", text("Next")).attr("class", "t-btn"),
                    |ui: &mut UiState, _| {
                        if ui.rehearsal_running {
                            ui.complete_rehearsal_cursor();
                        }
                        step_set(&mut ui.set, 1);
                        if ui.rehearsal_running {
                            ui.record_rehearsal_cursor();
                        }
                    },
                ),
                clickable(
                    el("div", text("♪ Hear")).attr("class", "t-btn t-hear"),
                    |ui: &mut UiState, _| ui.request_preview(),
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
                            Touch::Arpeggiate {
                                direction,
                                inversion,
                            } => {
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
                        None => Box::new(el("div", ()).attr("class", cell_class)) as UiChild,
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

