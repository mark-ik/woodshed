use woodshed_core::step_set;
use woodshedding::rehearsal::{LoopMode, Recipe};
use cambium::{clickable, custom_leaf, el, text};

use super::{UiChild, UiState};
use crate::fretboard_leaf::{
    fretboard_px_size, note_center_x, string_center_y, MARKER_H, MARKER_W,
    REHEARSAL_FRETBOARD_LEAF_KEY,
};

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

    // Current card's material on the big board — the same Sprigging paint leaf
    // the Stage board uses (crisp neck, coloured markers). Over it sits one
    // clickable label per note: click toggles that note's membership in the card
    // (deactivate / reactivate). A deactivated note stays visible, dimmed, so it
    // remains a target to switch back on. This is the board-as-editor surface.
    let card = &ui.set.cards[cursor];
    let dot_list = ui.stage.dots_for_card(card);
    let string_count = ui.stage.string_count();
    let (w, h) = fretboard_px_size(string_count, ui.stage.fret_count);
    let labels: Vec<UiChild> = dot_list
        .iter()
        .map(|d| {
            let (si, fret) = (d.string_index, d.fret);
            let lx = note_center_x(fret) - MARKER_W / 2.0;
            let ly = string_center_y(si) - MARKER_H / 2.0;
            let class = if ui.card_muted(si, fret) {
                "fret-label muted"
            } else {
                "fret-label"
            };
            Box::new(clickable(
                el("div", text(d.label.clone())).attr("class", class).attr(
                    "style",
                    format!(
                        "left:{lx:.1}px; top:{ly:.1}px; width:{MARKER_W:.1}px; height:{MARKER_H:.1}px"
                    ),
                ),
                move |ui: &mut UiState, _| ui.toggle_card_mute(si, fret),
            )) as UiChild
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
                    el(
                        "div",
                        (
                            custom_leaf::<UiState, ()>(REHEARSAL_FRETBOARD_LEAF_KEY, w, h),
                            el("div", labels).attr("class", "label-layer"),
                        ),
                    )
                    .attr("class", "fretboard-stack")
                    .attr("style", format!("width:{w}px; height:{h}px")),
                    el("div", text(card.label.clone())).attr("class", "scale-name"),
                ),
            )
            .attr("class", "board"),
        ),
    ))
}
