use std::collections::HashMap;

use woodshed_core::step_set;
use woodshedding::rehearsal::{LoopMode, Recipe};
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
    // The same Card editor appears in Stage's Set tray and Rehearsal. Actions
    // mutate the one persisted Set through UiState helpers.
    let editor = super::set_tray::card_editor(ui);

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

