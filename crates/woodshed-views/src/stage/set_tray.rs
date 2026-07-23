use woodshedding::rehearsal::{Hold, LoopMode, Recipe, Touch};
use woodshed_core::storage::AppSection;
use cambium::{clickable, el, graph_canvas_swatch, map_state, text, text_field};

use super::{set_graph_swatch, UiChild, UiState};

fn hold_label(hold: &Hold) -> String {
    match hold {
        Hold::Manual => "manual".into(),
        Hold::Bars(n) => format!("{n} bars"),
        Hold::Seconds(seconds) => format!("{seconds:.0}s"),
        Hold::Reps(reps) => format!("{reps} reps"),
    }
}

fn touch_label(touch: &Touch) -> String {
    match touch {
        Touch::Block => "block".into(),
        Touch::Arpeggiate { direction, .. } => format!("arp {}", direction.label()),
        Touch::Walk => "walk".into(),
    }
}

fn source_label(source: &Option<Recipe>) -> String {
    match source {
        Some(Recipe::Progression { name, .. }) => format!("from {name}"),
        Some(Recipe::Exercise { name }) => format!("from {name}"),
        Some(Recipe::PracticeSet { name }) => format!("from {name}"),
        Some(Recipe::Song { name, bar }) => format!("from {name} · bar {}", bar + 1),
        None => "staged directly".into(),
    }
}

pub(super) fn card_editor(ui: &UiState) -> UiChild {
    if ui.set.cards.is_empty() {
        return Box::new(el("div", ()));
    }
    let cursor = ui.set.cursor.min(ui.set.cards.len() - 1);
    let card = &ui.set.cards[cursor];
    let bpm = card
        .timing
        .bpm
        .map(|value| format!("{value:.0} bpm"))
        .unwrap_or_else(|| "transport bpm".into());
    let window = card
        .setting
        .fret_window
        .map(|value| format!("frets {}-{}", value.start, value.start + value.span))
        .unwrap_or_else(|| "whole neck".into());
    Box::new(
        el(
            "div",
            (
                el("div", text("Selected Card")).attr("class", "set-editor-label"),
                // Rename: the buffer is kept in step with this card's label by
                // UiState::sync_card_rename, so typing here renames the card.
                el(
                    "div",
                    map_state(text_field(&ui.card_rename), |ui: &mut UiState| {
                        &mut ui.card_rename
                    }),
                )
                .attr("class", "card-rename"),
                clickable(
                    el(
                        "div",
                        text(format!("Touch: {}", touch_label(&card.touch))),
                    )
                    .attr("class", "t-btn"),
                    |ui: &mut UiState, _| ui.cycle_card_touch(),
                ),
                clickable(
                    el(
                        "div",
                        text(format!("Hold: {}", hold_label(&card.timing.hold))),
                    )
                    .attr("class", "t-btn"),
                    |ui: &mut UiState, _| ui.cycle_card_hold(),
                ),
                clickable(
                    el("div", text("-")).attr("class", "t-btn t-narrow"),
                    |ui: &mut UiState, _| ui.nudge_card_bpm(-5.0),
                ),
                el("div", text(bpm)).attr("class", "t-readout"),
                clickable(
                    el("div", text("+")).attr("class", "t-btn t-narrow"),
                    |ui: &mut UiState, _| ui.nudge_card_bpm(5.0),
                ),
                clickable(
                    el("div", text("<")).attr("class", "t-btn t-narrow"),
                    |ui: &mut UiState, _| ui.shift_card_window(-1),
                ),
                el("div", text(window)).attr("class", "t-readout"),
                clickable(
                    el("div", text(">")).attr("class", "t-btn t-narrow"),
                    |ui: &mut UiState, _| ui.shift_card_window(1),
                ),
                clickable(
                    el("div", text("Free position")).attr("class", "t-btn"),
                    |ui: &mut UiState, _| ui.clear_card_window(),
                ),
            ),
        )
        .attr("class", "set-editor"),
    )
}

pub(super) fn view(ui: &UiState) -> UiChild {
    let loop_label = match ui.set.loop_mode {
        LoopMode::Off => "Loop set: off",
        LoopMode::All => "Loop set: on",
    };
    let cards: Vec<UiChild> = ui
        .set
        .cards
        .iter()
        .enumerate()
        .map(|(index, card)| {
            let class = if index == ui.set.cursor {
                "set-card set-card-active"
            } else {
                "set-card"
            };
            let tuning = card.setting.tuning.as_deref().unwrap_or("default tuning");
            Box::new(clickable(
                el(
                    "div",
                    (
                        el("div", text(format!("{} · {}", index + 1, card.material.tag())))
                            .attr("class", "set-card-kind"),
                        el("div", text(card.label.clone())).attr("class", "set-card-title"),
                        el(
                            "div",
                            text(format!(
                                "{} · {} · {}",
                                touch_label(&card.touch),
                                hold_label(&card.timing.hold),
                                tuning,
                            )),
                        )
                        .attr("class", "set-card-meta"),
                        el("div", text(source_label(&card.from))).attr("class", "set-card-source"),
                    ),
                )
                .attr("class", class),
                move |ui: &mut UiState, _| {
                    ui.set.cursor = index;
                    ui.set_graph_card_expanded = true;
                },
            )) as UiChild
        })
        .collect();

    let body: UiChild = if cards.is_empty() {
        Box::new(
            el("div", text("Stage catalog material to build this Set."))
                .attr("class", "set-empty"),
        )
    } else {
        Box::new(el("div", cards).attr("class", "set-cards"))
    };
    let content: UiChild = if ui.set_tray_expanded {
        let swatch = set_graph_swatch(ui);
        let expanded_card: UiChild = if ui.set_graph_card_expanded {
            card_editor(ui)
        } else {
            Box::new(el("div", ()))
        };
        let graph = graph_canvas_swatch(
            &swatch,
            |ui: &mut UiState, index: usize| {
                let was_selected = ui.set.cursor == index;
                ui.set.cursor = index;
                ui.set_graph_card_expanded = if was_selected {
                    !ui.set_graph_card_expanded
                } else {
                    true
                };
            },
            |ui: &mut UiState, index: Option<usize>| ui.set_graph_hover = index,
            |_ui: &mut UiState| {},
        );
        Box::new(el(
            "div",
            (
                el(
                    "div",
                    (
                        el("div", text("Set graph")).attr("class", "set-graph-heading"),
                        graph,
                        clickable(
                            el(
                                "div",
                                text(if ui.app_settings.stage.show_set_sequence_edges {
                                    "Sequence edges: on"
                                } else {
                                    "Sequence edges: off"
                                }),
                            )
                            .attr("class", "t-btn"),
                            |ui: &mut UiState, _| {
                                ui.app_settings.stage.show_set_sequence_edges =
                                    !ui.app_settings.stage.show_set_sequence_edges;
                            },
                        ),
                    ),
                )
                .attr("class", "set-graph"),
                expanded_card,
                body,
            ),
        ))
    } else {
        Box::new(el("div", ()))
    };

    Box::new(
        el(
            "section",
            (
                el(
                    "div",
                    (
                        el(
                            "div",
                            text(format!("Set · {} cards", ui.set.cards.len())),
                        )
                        .attr("class", "set-heading"),
                        clickable(
                            el(
                                "div",
                                text(if ui.set_tray_expanded { "Collapse" } else { "Expand" }),
                            )
                            .attr("class", "t-btn"),
                            |ui: &mut UiState, _| {
                                ui.set_tray_expanded = !ui.set_tray_expanded;
                            },
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
                            el("div", text("←")).attr("class", "t-btn t-narrow"),
                            |ui: &mut UiState, _| {
                                let cursor = ui.set.cursor;
                                ui.set.move_card(cursor, -1);
                            },
                        ),
                        clickable(
                            el("div", text("→")).attr("class", "t-btn t-narrow"),
                            |ui: &mut UiState, _| {
                                let cursor = ui.set.cursor;
                                ui.set.move_card(cursor, 1);
                            },
                        ),
                        clickable(
                            el("div", text("Duplicate")).attr("class", "t-btn"),
                            |ui: &mut UiState, _| {
                                let cursor = ui.set.cursor;
                                ui.set.duplicate(cursor);
                                if !ui.set.cards.is_empty() {
                                    ui.set.cursor = (cursor + 1).min(ui.set.cards.len() - 1);
                                }
                            },
                        ),
                        clickable(
                            el("div", text("Remove")).attr("class", "t-btn"),
                            |ui: &mut UiState, _| {
                                let cursor = ui.set.cursor;
                                ui.set.remove(cursor);
                            },
                        ),
                        clickable(
                            el("div", text("Clear")).attr("class", "t-btn"),
                            |ui: &mut UiState, _| ui.set = Default::default(),
                        ),
                        clickable(
                            el("div", text("Rehearse")).attr("class", "t-btn t-hear"),
                            |ui: &mut UiState, _| {
                                if !ui.set.cards.is_empty() {
                                    ui.section = AppSection::Rehearsal;
                                }
                            },
                        ),
                    ),
                )
                .attr("class", "set-toolbar"),
                content,
            ),
        )
        .attr("class", "set-tray"),
    )
}
