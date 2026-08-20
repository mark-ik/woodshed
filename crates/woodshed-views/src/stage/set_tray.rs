use woodshedding::rehearsal::{Hold, LoopMode, Recipe, SetGraphEdgeKind, Touch};
use woodshed_core::storage::AppSection;
use woodshed_core::stage_scene::StageInstanceRef;
use cambium::{
    GraphCanvasEvent, clickable, el, graph_canvas, map_state, text, text_field,
};

use super::{
    set_graph_relation_choices, set_graph_snapshot, set_graph_swatch_from_snapshot, UiChild,
    UiState,
};

/// Label for one relation family's visibility toggle. The family is named, so
/// adding the harmonic or evidence layer reads as another entry rather than a
/// second unexplained switch.
fn relation_toggle_label(ui: &UiState, kind: SetGraphEdgeKind) -> String {
    let state = if ui.app_settings.stage.shows_relation(kind) {
        "on"
    } else {
        "off"
    };
    format!("{} edges: {state}", kind.label())
}

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
    let snapshot = set_graph_snapshot(ui);
    let swatch = set_graph_swatch_from_snapshot(&snapshot, ui, ui.set_tray_expanded);
    let event_snapshot = snapshot.clone();
    let graph = graph_canvas(
        &swatch,
        move |ui: &mut UiState, event: GraphCanvasEvent<StageInstanceRef>| {
            ui.handle_set_graph_event(&event_snapshot, event);
        },
    );
    let relation_choices = set_graph_relation_choices(&snapshot, ui);
    let visible_relations = relation_choices
        .iter()
        .filter(|relation| relation.visible)
        .count();
    let relation_rows: Vec<UiChild> = relation_choices
        .iter()
        .cloned()
        .map(|choice| {
            let key = choice.key.clone();
            let wire_key = key.wire_key();
            let toggle_snapshot = snapshot.clone();
            let toggle_class = if choice.visible {
                "set-relation-toggle set-relation-toggle-on"
            } else {
                "set-relation-toggle"
            };
            Box::new(
                el(
                    "div",
                    (
                        clickable(
                            el(
                                "div",
                                text(if choice.visible { "Shown" } else { "Hidden" }),
                            )
                            .attr("class", toggle_class)
                            .attr("aria-pressed", choice.visible.to_string()),
                            move |ui: &mut UiState, _| {
                                ui.toggle_set_graph_relation(&toggle_snapshot, key.clone());
                            },
                        ),
                        el(
                            "div",
                            (
                                el("div", text(choice.pair))
                                    .attr("class", "set-relation-pair"),
                                el(
                                    "div",
                                    text(format!(
                                        "{} · {} · {}%",
                                        choice.relation, choice.authority, choice.weight
                                    )),
                                )
                                .attr("class", "set-relation-kind"),
                                el("div", text(choice.explanation))
                                    .attr("class", "set-relation-explanation"),
                            ),
                        )
                        .attr("class", "set-relation-copy"),
                    ),
                )
                .attr("class", "set-relation-row")
                .attr("data-relation-key", wire_key),
            ) as UiChild
        })
        .collect();
    let hide_snapshot = snapshot.clone();
    let relation_inventory: UiChild = Box::new(
        el(
            "div",
            (
                el(
                    "div",
                    (
                        el(
                            "div",
                            text(format!(
                                "Relations · {visible_relations} of {} shown",
                                relation_choices.len()
                            )),
                        )
                        .attr("class", "set-relation-heading"),
                        clickable(
                            el("div", text("Show all")).attr("class", "t-btn"),
                            |ui: &mut UiState, _| ui.show_all_set_graph_relations(),
                        ),
                        clickable(
                            el("div", text("Hide all")).attr("class", "t-btn"),
                            move |ui: &mut UiState, _| {
                                ui.hide_all_set_graph_relations(&hide_snapshot);
                            },
                        ),
                    ),
                )
                .attr("class", "set-relation-toolbar"),
                el("div", relation_rows).attr("class", "set-relation-list"),
            ),
        )
        .attr("class", "set-relation-inventory"),
    );
    let relation_detail: UiChild = ui
        .set_graph_relation
        .and_then(|reference| snapshot.relation_detail(reference, &ui.set))
        .map(|relation| {
            Box::new(
                el(
                    "div",
                    text(format!(
                        "Selected · {} · {} · {}% · {}",
                        relation.label,
                        relation.authority,
                        relation.weight,
                        relation.explanation
                    )),
                )
                    .attr("class", "set-graph-relation"),
            ) as UiChild
        })
        .unwrap_or_else(|| Box::new(el("div", ())) as UiChild);
    let content: UiChild = if ui.set_tray_expanded {
        let expanded_card: UiChild = if ui.set_graph_card_expanded {
            card_editor(ui)
        } else {
            Box::new(el("div", ()))
        };
        Box::new(el(
            "div",
            (
                el(
                    "div",
                    (
                        el("div", text("Set graph")).attr("class", "set-graph-heading"),
                        graph,
                        relation_detail,
                        relation_inventory,
                        el(
                            "div",
                            clickable(
                                el(
                                    "div",
                                    text(relation_toggle_label(ui, SetGraphEdgeKind::Next)),
                                )
                                .attr("class", "t-btn"),
                                |ui: &mut UiState, _| {
                                    ui.app_settings
                                        .stage
                                        .toggle_relation(SetGraphEdgeKind::Next);
                                    ui.set_graph_relation = None;
                                },
                            ),
                        )
                        .attr("class", "set-graph-controls"),
                    ),
                )
                .attr("class", "set-graph"),
                expanded_card,
                body,
            ),
        ))
    } else {
        Box::new(el("div", graph).attr("class", "set-graph set-graph-compact"))
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
