use cambium::{clickable, el, text};

use super::{board, ToolPage, UiChild, UiState};

fn tool_nav(ui: &UiState) -> UiChild {
    let pages = [
        (ToolPage::Fretboard, "Fretboard"),
        (ToolPage::Metronome, "Metronome"),
        (ToolPage::Tuner, "Tuner"),
    ];
    let items: Vec<UiChild> = pages
        .into_iter()
        .map(|(page, label)| {
            let class = if ui.tool_page == page {
                "lens lens-active"
            } else {
                "lens"
            };
            Box::new(clickable(
                el("div", text(label)).attr("class", class),
                move |ui: &mut UiState, _| ui.tool_page = page,
            )) as UiChild
        })
        .collect();
    Box::new(el("div", items).attr("class", "lens-strip"))
}

fn metronome(ui: &UiState) -> UiChild {
    Box::new(
        el(
            "div",
            (
                el("div", text("Metronome")).attr("class", "settings-heading"),
                el(
                    "div",
                    (
                        clickable(
                            el(
                                "div",
                                text(if ui.transport.playing { "Stop" } else { "Play" }),
                            )
                            .attr("class", "t-btn t-hear"),
                            |ui: &mut UiState, _| {
                                ui.transport.playing = !ui.transport.playing;
                            },
                        ),
                        clickable(
                            el("div", text("-5")).attr("class", "t-btn"),
                            |ui: &mut UiState, _| ui.nudge_bpm(-5.0),
                        ),
                        el("div", text(format!("{:.0} bpm", ui.transport.bpm)))
                            .attr("class", "t-readout"),
                        clickable(
                            el("div", text("+5")).attr("class", "t-btn"),
                            |ui: &mut UiState, _| ui.nudge_bpm(5.0),
                        ),
                    ),
                )
                .attr("class", "transport"),
                el(
                    "div",
                    text("The same clock drives Stage previews, Rehearsal, and Looper."),
                )
                .attr("class", "settings-line"),
            ),
        )
        .attr("class", "board tool-board"),
    )
}

fn tuner(ui: &UiState) -> UiChild {
    let reading = match &ui.tuner.reading {
        Some(reading) => format!(
            "{}{} {:+.0} cents{}",
            reading.note,
            reading.octave,
            reading.cents,
            if reading.in_tune { " · in tune" } else { "" }
        ),
        None if ui.tuner.enabled => "Listening…".to_string(),
        None => "Tuner is off".to_string(),
    };
    Box::new(
        el(
            "div",
            (
                el("div", text("Tuner")).attr("class", "settings-heading"),
                clickable(
                    el(
                        "div",
                        text(if ui.tuner.enabled {
                            "Stop listening"
                        } else {
                            "Start listening"
                        }),
                    )
                    .attr("class", "t-btn t-hear"),
                    |ui: &mut UiState, _| {
                        ui.tuner.enabled = !ui.tuner.enabled;
                        if !ui.tuner.enabled {
                            ui.tuner.reading = None;
                        }
                    },
                ),
                el("div", text(reading)).attr("class", "tool-reading"),
            ),
        )
        .attr("class", "board tool-board"),
    )
}

pub(super) fn screen(ui: &UiState) -> UiChild {
    let body = match ui.tool_page {
        ToolPage::Fretboard => board(ui),
        ToolPage::Metronome => metronome(ui),
        ToolPage::Tuner => tuner(ui),
    };
    Box::new(el("div", (tool_nav(ui), body)))
}
