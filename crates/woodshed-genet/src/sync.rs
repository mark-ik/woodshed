//! The tail of every input dispatch: push the state through the seams it owns.
//!
//! Dropdown picks land in the core, the audio backend is told the new
//! transport, the MIDI ports are connected or dropped, window-chrome requests
//! are honoured, the session is persisted, and a theme or accessibility change
//! asks the host for a new sheet.

use cambium_genet_winit_host::AppCtx;
use woodshed_core::audio::{AudioBackend, CalibrationStatus};
use woodshed_core::midi::MidiBackend as _;
use woodshed_views::stage::{UiChild, UiState};

use crate::shared::Shared;

/// The runner shape this application uses, spelled once.
/// A boxed closure rather than a `fn` pointer: the desktop root captures the
/// host's window-verb handle, so the caption buttons can invoke it without a
/// flag on the shared (browser-portable) `UiState`.
pub type Logic = Box<dyn FnMut(&UiState) -> UiChild>;
/// The host context a woodshed hook receives.
pub type Ctx<'a> = AppCtx<'a, UiState, Logic, UiChild>;

/// Resolve a MIDI port dropdown selection to a connect target: index 0 = "None"
/// (disconnect), else `ports[idx - 1]`.
fn midi_port_at(ports: &[String], selected: usize) -> Option<String> {
    if selected == 0 {
        None
    } else {
        ports.get(selected - 1).cloned()
    }
}

/// Everything woodshed does after an input dispatch.
pub fn after_dispatch(shared: &mut Shared, ctx: &mut Ctx<'_>) {
    push_backend(shared, ctx);
    // Window verbs used to be drained here from four `UiState` flags. The
    // host owns them now: the caption buttons call `WindowCommands` directly
    // and the title bar drags from its `--app-region` declaration.
    midi_devices(shared, ctx);
}

/// Sync dropdown state into the core, push the audio state through the backend
/// seam, capture the skin settings, and persist.
fn push_backend(shared: &mut Shared, ctx: &mut Ctx<'_>) {
    let mut theme = shared.theme;
    let mut reduce_motion = shared.reduce_motion;
    let mut text_scale = shared.text_scale.clone();
    let mut persisted: Option<String> = None;
    let mut persisted_settings: Option<String> = None;

    let backend = shared.backend.as_mut();
    let last_song = &mut shared.last_song;
    ctx.runner.update(|ui| {
        ui.sync();
        if let Some(backend) = backend {
            // Calibration owns the metronome engine during a run.
            if !ui.calib_active {
                backend.set_metronome(ui.transport);
            }
            backend.set_tuner_enabled(ui.tuner.enabled);
            if ui.song != *last_song {
                backend.set_song(&ui.song);
                *last_song = ui.song.clone();
            }
            backend.set_song_transport(ui.song_playing);
            if ui.song_rewind_requested {
                backend.song_rewind();
                ui.song_rewind_requested = false;
            }
            if ui.preview_requested {
                ui.preview_requested = false;
                let (pitches, dur, strum) = ui.preview_voicing();
                if !pitches.is_empty() {
                    backend.preview_pitches(&pitches, dur, strum);
                }
            }
            if let Some(freq) = ui.preview_note_requested.take() {
                backend.preview_note(freq, 0.9);
            }
            // Latency-calibration requests.
            if std::mem::take(&mut ui.calib_start_requested) {
                backend.calibration_start();
                ui.calib_active = true;
                ui.calib_status = CalibrationStatus::Running {
                    clicks_fired: 0,
                    total: 6,
                };
            }
            if std::mem::take(&mut ui.calib_cancel_requested) {
                backend.calibration_cancel();
                ui.calib_active = false;
                ui.calib_status = CalibrationStatus::Idle;
            }
            if std::mem::take(&mut ui.calib_accept_requested) {
                if let CalibrationStatus::Success { latency_ms, .. } = ui.calib_status {
                    backend.set_latency_ms(Some(latency_ms));
                }
                ui.calib_status = CalibrationStatus::Idle;
            }
            ui.latency_ms = backend.latency_ms();
            // Looper (song-mode record) requests.
            if std::mem::take(&mut ui.song_record_toggle_requested) {
                if ui.song_recording {
                    backend.song_stop_record();
                } else {
                    backend.song_arm_record(ui.song_edit_cursor);
                }
            }
            if std::mem::take(&mut ui.song_clear_loop_requested) {
                backend.song_clear_loop(ui.song_edit_cursor);
            }
            backend.song_set_record_replace(ui.song_record_replace);
            ui.song_recording = backend.song_recording();
            ui.song_loop_bars = backend.song_loop_bars();
        }
        theme = ui.theme();
        reduce_motion = ui.app_settings.accessibility.reduce_motion;
        text_scale = ui.app_settings.accessibility.text_scale.clone();
        persisted = serde_json::to_string(&ui.to_persisted()).ok();
        persisted_settings = serde_json::to_string(&ui.app_settings).ok();
    });

    if let Some(sheet) = shared.reskin_if_changed(theme, reduce_motion, text_scale) {
        // The host takes it from here: a new sheet forces a full relayout.
        *ctx.set_sheet = Some(sheet);
    }
    if let Some(json) = persisted {
        shared.storage.save(&json);
    }
    if let Some(json) = persisted_settings {
        shared.storage.save_settings(&json);
    }
}

/// Connect / disconnect per the dropdowns, and re-scan the port lists on
/// request.
fn midi_devices(shared: &mut Shared, ctx: &mut Ctx<'_>) {
    let midi = &mut shared.midi;
    ctx.runner.update(|ui| {
        if std::mem::take(&mut ui.midi.refresh_requested) {
            ui.midi.input_ports = midi.input_ports();
            ui.midi.output_ports = midi.output_ports();
        }
        let in_target = midi_port_at(&ui.midi.input_ports, ui.midi.input_dd.selected);
        if midi.connected_input() != in_target.as_deref() {
            midi.connect_input(in_target.as_deref());
        }
        let out_target = midi_port_at(&ui.midi.output_ports, ui.midi.output_dd.selected);
        if midi.connected_output() != out_target.as_deref() {
            midi.connect_output(out_target.as_deref());
        }
        ui.midi.connected_in = midi.connected_input().map(str::to_string);
        ui.midi.connected_out = midi.connected_output().map(str::to_string);
    });
}
