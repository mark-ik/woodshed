use woodshed_core::song::{song_from_progression, SECTION_LABELS};
use cambium::{clickable, el, text};

use super::{UiChild, UiState};

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
                    el("div", text(if ui.song_playing { "Stop" } else { "Play" }))
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
                    el("div", text(if ui.song.one_shot { "Once" } else { "Loop" }))
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
                        if let Some(doc) = song_from_progression(&ui.stage, ui.transport.bpm) {
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

/// Looper controls (audio-depth slice 15): record your part into the
/// selected bar as the song loops past it, overdub or replace, and clear.
/// The engine records live input into the bar's audio buffer; these drive
/// it through the audio seam.
fn song_loop_ops(ui: &UiState) -> UiChild {
    let rec_label = if ui.song_recording {
        "● Recording"
    } else {
        "● Rec bar"
    };
    let rec_class = if ui.song_recording {
        "t-btn rec-on"
    } else {
        "t-btn"
    };
    let mode_label = if ui.song_record_replace {
        "Replace"
    } else {
        "Overdub"
    };
    Box::new(
        el(
            "div",
            (
                clickable(
                    el("div", text(rec_label)).attr("class", rec_class),
                    |ui: &mut UiState, _| ui.song_record_toggle_requested = true,
                ),
                clickable(
                    el("div", text(mode_label)).attr("class", "t-btn"),
                    |ui: &mut UiState, _| ui.song_record_replace = !ui.song_record_replace,
                ),
                clickable(
                    el("div", text("Clear loop")).attr("class", "t-btn"),
                    |ui: &mut UiState, _| ui.song_clear_loop_requested = true,
                ),
                el(
                    "div",
                    text(
                        "play the song, then Rec to capture your part into \
                         the selected bar as it loops past",
                    ),
                )
                .attr("class", "t-readout"),
            ),
        )
        .attr("class", "transport"),
    )
}

/// Per-bar chord / tempo / meter / section editor for the cursor bar.
fn song_bar_editor(ui: &UiState) -> UiChild {
    let cursor = ui
        .song_edit_cursor
        .min(ui.song.bars.len().saturating_sub(1));
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
                    el("div", text(format!("Root: {root_label}"))).attr("class", "t-btn"),
                    move |ui: &mut UiState, _| {
                        ui.song.bars[cursor].cycle_root();
                    },
                ),
                clickable(
                    el("div", text(format!("Chord: {chord_label}"))).attr("class", "t-btn"),
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
                el("div", text(format!("{:.0} bpm", bar.bpm))).attr("class", "t-readout"),
                clickable(
                    el("div", text("+")).attr("class", "t-btn t-narrow"),
                    move |ui: &mut UiState, _| ui.song.bars[cursor].nudge_bpm(5.0),
                ),
                clickable(
                    el("div", text(format!("{}/4", bar.beats))).attr("class", "t-btn"),
                    move |ui: &mut UiState, _| ui.song.bars[cursor].cycle_beats(),
                ),
                clickable(
                    el("div", text(format!("x{}", bar.length))).attr("class", "t-btn"),
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

pub(super) fn screen(ui: &UiState) -> UiChild {
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
            let looped = ui.song_loop_bars.get(i).copied().unwrap_or(false);
            Box::new(clickable(
                el(
                    "div",
                    (
                        el("div", text(section)).attr("class", "bar-label"),
                        el("div", text(chord)).attr("class", "bar-chord"),
                        el(
                            "div",
                            text(format!(
                                "{:.0} · {}/4{}",
                                b.bpm,
                                b.beats,
                                if looped { " ⟳" } else { "" }
                            )),
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
            song_loop_ops(ui),
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
