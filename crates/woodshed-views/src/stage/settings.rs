use woodshed_core::audio::CalibrationStatus;
use xilem_serval::{clickable, el, map_state, select, text};

use super::{BoardLayout, UiChild, UiState};
use crate::theme::ThemeMode;

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
            el("div", text("MIDI")).attr("class", "settings-heading settings-gap"),
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

/// Latency-calibration panel (audio-depth slice 14): measure the
/// input→output round-trip lag by playing clicks the player taps along
/// to. The host drives the `CalibrationSession` through the audio seam.
fn calibration_panel(ui: &UiState) -> UiChild {
    let latency_line = match ui.latency_ms {
        Some(ms) => format!("Latency: {ms:.0} ms round-trip"),
        None => "Latency: uncalibrated".to_string(),
    };
    let start_btn = |label: &str| {
        Box::new(clickable(
            el("div", text(label.to_string())).attr("class", "t-btn"),
            |ui: &mut UiState, _| ui.calib_start_requested = true,
        )) as UiChild
    };
    let readout = |s: String| Box::new(el("div", text(s)).attr("class", "t-readout")) as UiChild;
    let controls: Vec<UiChild> = match ui.calib_status {
        CalibrationStatus::Idle => vec![Box::new(clickable(
            el("div", text("Calibrate")).attr("class", "t-btn t-hear"),
            |ui: &mut UiState, _| ui.calib_start_requested = true,
        )) as UiChild],
        CalibrationStatus::Running {
            clicks_fired,
            total,
        } => vec![
            readout(format!("Tap along… {clicks_fired}/{total}")),
            Box::new(clickable(
                el("div", text("Cancel")).attr("class", "t-btn"),
                |ui: &mut UiState, _| ui.calib_cancel_requested = true,
            )) as UiChild,
        ],
        CalibrationStatus::Success {
            latency_ms,
            matched,
            total,
        } => vec![
            readout(format!("{latency_ms:.0} ms · {matched}/{total} hits")),
            Box::new(clickable(
                el("div", text("Accept")).attr("class", "t-btn t-hear"),
                |ui: &mut UiState, _| ui.calib_accept_requested = true,
            )) as UiChild,
            start_btn("Retry"),
        ],
        CalibrationStatus::Insufficient { matched, total } => vec![
            readout(format!("only {matched}/{total} hits caught")),
            start_btn("Retry"),
        ],
        CalibrationStatus::Unavailable => vec![readout("no input device".to_string())],
    };
    Box::new(el(
        "div",
        (
            el("div", text("Latency calibration")).attr("class", "settings-heading settings-gap"),
            el("div", text(latency_line)).attr("class", "settings-line"),
            el("div", controls).attr("class", "transport"),
            el(
                "div",
                text(
                    "Plays a lead of clicks — tap your guitar with each one \
                     so Woodshed can measure the round-trip lag.",
                ),
            )
            .attr("class", "settings-line midi-events"),
        ),
    ))
}

pub(super) fn screen(ui: &UiState) -> UiChild {
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
    let history_label = if ui.related.use_history {
        "History ranking: on"
    } else {
        "History ranking: off"
    };
    let graph_label = if ui.related.show_neighborhood {
        "Neighborhood graph: on"
    } else {
        "Neighborhood graph: off"
    };
    let hidden_count = ui.related.dismissed_ids.len();
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
                        el("div", text("Related"))
                            .attr("class", "settings-heading settings-gap"),
                        clickable(
                            el("div", text(history_label)).attr("class", "side-item"),
                            |ui: &mut UiState, _| {
                                ui.related.use_history = !ui.related.use_history;
                            },
                        ),
                        clickable(
                            el("div", text(graph_label)).attr("class", "side-item"),
                            |ui: &mut UiState, _| {
                                ui.related.show_neighborhood = !ui.related.show_neighborhood;
                            },
                        ),
                        clickable(
                            el("div", text(format!("Restore hidden ({hidden_count})")))
                                .attr("class", "side-item"),
                            |ui: &mut UiState, _| ui.related.dismissed_ids.clear(),
                        ),
                    ),
                )
                .attr("class", "side"),
                el(
                    "div",
                    (
                        el("div", text("Session")).attr("class", "settings-heading"),
                        el(
                            "div",
                            text(format!(
                                "Woodshed {} · desktop alpha",
                                env!("CARGO_PKG_VERSION")
                            )),
                        )
                        .attr("class", "settings-line"),
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
                        calibration_panel(ui),
                    ),
                )
                .attr("class", "board"),
            ),
        )
        .attr("class", "body"),
    )
}

