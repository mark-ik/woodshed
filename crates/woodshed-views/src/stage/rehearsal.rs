use cambium::{clickable, custom_leaf, el, on_hover, text, HoverEvent, HoverPhase};
use woodshed_core::step_set;
use woodshedding::rehearsal::{LoopMode, MarkMode, Recipe};

use super::{UiChild, UiState};
use crate::fretboard_leaf::{BoardGeom, Orientation, REHEARSAL_FRETBOARD_LEAF_KEY};

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
                el("div", text("The set is empty. Stage material to begin."))
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
                woodshedding::rehearsal::Touch::Walk => "walk".to_string(),
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
    // the Stage board uses. Over it, one clickable label per note: click *marks*
    // the note (a neutral selection, an accent ring). The [Off · Solo · Mute]
    // control below sets what marks do (Solo plays only the marked, Mute
    // silences them), and the board dims whatever the mode excludes. This is the
    // Selection axis of touch, made interactive.
    let card = &ui.set.cards[cursor];
    let dot_list = ui.stage.dots_for_card(card);
    let string_count = ui.stage.string_count();
    let geom = BoardGeom {
        string_count,
        fret_start: ui.stage.fret_start,
        fret_count: ui.stage.fret_count,
        orientation: Orientation::from_name(&ui.app_settings.fretboard.orientation),
    };
    let (w, h) = geom.size_u32();
    let (mw, mh) = geom.marker_size();
    let labels: Vec<UiChild> = dot_list
        .iter()
        .map(|d| {
            let (si, fret) = (d.string_index, d.fret);
            let (px, py) = geom.note_pos(si, fret);
            let lx = px - mw / 2.0;
            let ly = py - mh / 2.0;
            let mut class = String::from("fret-label");
            if ui.card_marked(si, fret) {
                class.push_str(" marked");
            }
            if ui.card_excluded(si, fret) {
                class.push_str(" excluded");
            }
            // The spoken note/role/place plus the mark state, so a screen-reader
            // user knows what a marker is and what marking did to it.
            let mut a11y = woodshed_core::marker_a11y_label(d, string_count);
            if ui.card_marked(si, fret) {
                a11y.push_str(", marked");
            }
            if ui.card_excluded(si, fret) {
                a11y.push_str(", muted");
            }
            // click marks the note; hover peeks its detail card (the resolved
            // "click marks, hover shows").
            Box::new(on_hover(
                clickable(
                    el("div", text(d.label.clone()))
                        .attr("class", class)
                        .attr("role", "button")
                        .attr("aria-label", a11y)
                        .attr(
                            "style",
                            format!(
                                "left:{lx:.1}px; top:{ly:.1}px; width:{mw:.1}px; height:{mh:.1}px"
                            ),
                        ),
                    move |ui: &mut UiState, _| ui.toggle_card_mark(si, fret),
                ),
                move |ui: &mut UiState, ev: HoverEvent| match ev.phase {
                    HoverPhase::Enter | HoverPhase::Move => ui.hover_peek = Some((si, fret)),
                    HoverPhase::Leave => {
                        if ui.hover_peek == Some((si, fret)) {
                            ui.hover_peek = None;
                        }
                    }
                },
            )) as UiChild
        })
        .collect();
    // The hovered marker's ephemeral detail card (reuses the Stage board's
    // card, which is position-free now that Stage flows its pinned cards in a
    // strip). The peek stays an overlay: this wrapper anchors it beside its
    // marker, flipped above for markers in the lower half so it stays on-board.
    let peek: Vec<UiChild> = ui
        .hover_peek
        .and_then(|(si, fret)| {
            dot_list
                .iter()
                .find(|d| d.string_index == si && d.fret == fret)
                .map(|d| {
                    const CARD_W: f32 = 140.0;
                    const CARD_H: f32 = 112.0;
                    let (cx, cy) = geom.note_pos(d.string_index, d.fret);
                    let (_mw, mh) = geom.marker_size();
                    let (_bw, bh) = geom.size();
                    let below = cy < bh / 2.0;
                    let top = if below {
                        cy + mh / 2.0 + 6.0
                    } else {
                        cy - mh / 2.0 - 6.0 - CARD_H
                    };
                    let left = (cx - CARD_W / 2.0).max(2.0);
                    Box::new(el("div", super::note_card(d, string_count)).attr(
                        "style",
                        format!("position:absolute; left:{left:.1}px; top:{top:.1}px;"),
                    )) as UiChild
                })
        })
        .into_iter()
        .collect();
    // The mode control: a small [Off · Solo · Mute] segmented toggle, not a loose
    // button row. Sets what the marked set does to this card.
    let mode = ui.card_mark_mode();
    let mark_count = card.setting.marked.len();
    let seg = |m: MarkMode| -> UiChild {
        let cls = if mode == m { "seg active" } else { "seg" };
        Box::new(clickable(
            el("div", text(m.label())).attr("class", cls),
            move |ui: &mut UiState, _| ui.set_card_mark_mode(m),
        )) as UiChild
    };
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
                            el("div", peek).attr("class", "card-layer"),
                        ),
                    )
                    .attr("class", "fretboard-stack")
                    .attr(
                        "aria-label",
                        format!("{} fretboard, {} notes", card.label, dot_list.len()),
                    )
                    .attr("style", format!("width:{w}px; height:{h}px")),
                    el(
                        "div",
                        (
                            el(
                                "div",
                                (seg(MarkMode::Off), seg(MarkMode::Solo), seg(MarkMode::Mute)),
                            )
                            .attr("class", "mode-seg"),
                            el("div", text(card.label.clone())).attr("class", "scale-name"),
                            (mark_count > 0).then(|| {
                                clickable(
                                    el("div", text(format!("Clear {mark_count} marks")))
                                        .attr("class", "clear-pins"),
                                    |ui: &mut UiState, _| ui.clear_card_marks(),
                                )
                            }),
                        ),
                    )
                    .attr("class", "board-caption"),
                ),
            )
            .attr("class", "board"),
        ),
    ))
}
