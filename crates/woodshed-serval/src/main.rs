//! Woodshed's serval desktop host (S2: interaction spine).
//!
//! A winit window presenting `woodshed-views`' Stage screen over live state:
//! `ServalAppRunner` diffs the views into a `ScriptedDom`, a retained
//! `IncrementalLayout` lays it out at logical size (DPI aware, incremental
//! `apply` for attribute-only batches), paint emission lowers to a
//! `netrender::Scene`, and `serval-winit-host`'s `SurfaceHost` rasterizes at
//! physical resolution and composites onto the backbuffer.
//!
//! Input: clicks hit-test the retained layout and dispatch through the
//! runner (sidebar, lens strip, header dropdowns); Tab traverses focus;
//! other keys route through `key_event_from_winit` → `dispatch_key`. After
//! every dispatch the host calls `UiState::sync` so dropdown picks land in
//! the core state.

mod audio;
mod midi;
mod storage;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use audio::CpalBackend;
use midi::MidiHost;
use storage::FsStorage;
use woodshed_core::audio::{AudioBackend, CalibrationStatus};
use woodshed_core::midi::MidiBackend as _;
use woodshed_core::storage::Storage as _;
use woodshed_views::theme::ThemeMode;

use netrender::{ColorLoad, ExternalTexturePlacement, NetrenderOptions};
use paint_list_api::{DeviceIntSize, PaintList as _};
use serval_layout::{
    Applied, IncrementalLayout, InteractionState, ScrollOffsets, SourceNodeId,
};
use layout_dom_api::{DomMutation, LayoutDomMut as _};
use serval_scripted_dom::{NodeId, ScriptedDom};
use serval_winit_host::{key_event_from_winit, modifiers_from_winit, SurfaceHost};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent as WinitKeyEvent, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key as WinitKey, ModifiersState, NamedKey as WinitNamedKey};
use winit::window::{Window, WindowId};
use woodshed_views::stage::{stage_root, UiChild, UiState};
use woodshed_views::theme::slate_stage_css;
use xilem_serval::{PointerClick, Propagation, ServalAppRunner};

type Runner = ServalAppRunner<UiState, fn(&UiState) -> UiChild, UiChild>;

struct App {
    window: Option<Arc<Window>>,
    host: Option<SurfaceHost>,
    runner: Option<Runner>,
    /// Retained layout session in logical coordinates — hit-test target
    /// and incremental-apply subject.
    layout: Option<IncrementalLayout<NodeId>>,
    /// Logical size the retained layout was built at.
    layout_size: (f32, f32),
    sheet: String,
    /// Cursor position in logical coordinates.
    cursor: (f32, f32),
    modifiers: ModifiersState,
    /// The W0.1 audio seam: cpal on desktop, Web Audio on the web host.
    backend: Option<CpalBackend>,
    /// The MIDI seam: midir on desktop, Web MIDI on the web host.
    midi: MidiHost,
    /// Last arpeggio auto-advance instant (the step clock while the
    /// arpeggio transport runs).
    last_arp_step: Option<std::time::Instant>,
    /// Last rehearsal dwell-advance instant.
    last_rehearsal_step: Option<std::time::Instant>,
    /// The W0.2 storage seam: fs on desktop, OPFS on the web host.
    storage: FsStorage,
    /// Theme the current sheet was generated from; a change re-skins.
    theme: ThemeMode,
    /// The hovered node's opaque id, for `:hover` restyles on target
    /// change (not per pixel).
    last_hover: Option<u64>,
    /// The focused node's opaque id (same discipline for `:focus`).
    last_focus: Option<u64>,
    /// The song last pushed through the backend seam (push on change).
    last_song: woodshed_core::song::SongDoc,
    /// Set by the chrome close button; drives event-loop exit.
    close_requested: bool,
    /// Last resize-edge the cursor was over, to dedup `set_cursor` calls.
    resize_hint: Option<winit::window::ResizeDirection>,
    /// Monotonic base for the CSS-transition animation clock (seconds
    /// since app start).
    anim_base: std::time::Instant,
}

/// Resolve a MIDI port dropdown selection to a connect target: index 0
/// = "None" (disconnect), else `ports[idx - 1]`.
fn midi_port_at(ports: &[String], selected: usize) -> Option<String> {
    if selected == 0 {
        None
    } else {
        ports.get(selected - 1).cloned()
    }
}

/// Which resize edge a point near the window border maps to, in logical
/// coordinates with an 8px grab margin. `None` in the interior.
fn resize_edge(x: f32, y: f32, w: f32, h: f32) -> Option<winit::window::ResizeDirection> {
    use winit::window::ResizeDirection as R;
    const M: f32 = 8.0;
    let left = x <= M;
    let right = x >= w - M;
    let top = y <= M;
    let bottom = y >= h - M;
    match (left, right, top, bottom) {
        (true, _, true, _) => Some(R::NorthWest),
        (_, true, true, _) => Some(R::NorthEast),
        (true, _, _, true) => Some(R::SouthWest),
        (_, true, _, true) => Some(R::SouthEast),
        (true, ..) => Some(R::West),
        (_, true, ..) => Some(R::East),
        (_, _, true, _) => Some(R::North),
        (_, _, _, true) => Some(R::South),
        _ => None,
    }
}

/// The resize cursor icon for a border direction — an undecorated (CSD)
/// window gets no OS resize cursors, so the app supplies the affordance.
fn edge_cursor(dir: winit::window::ResizeDirection) -> winit::window::CursorIcon {
    use winit::window::{CursorIcon as C, ResizeDirection as R};
    match dir {
        R::East | R::West => C::EwResize,
        R::North | R::South => C::NsResize,
        R::NorthEast | R::SouthWest => C::NeswResize,
        R::NorthWest | R::SouthEast => C::NwseResize,
    }
}

impl App {
    fn scale_factor(&self) -> f64 {
        self.window.as_ref().map_or(1.0, |w| w.scale_factor())
    }

    /// Show the matching resize arrow near a floating window's border, the
    /// default cursor elsewhere. An undecorated (CSD) window gets no OS
    /// resize cursors, so we supply the affordance ourselves. Deduped via
    /// `resize_hint` so we only touch the cursor on a transition.
    fn update_resize_cursor(&mut self) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        let dir = if window.is_maximized() {
            None
        } else {
            let size = window.inner_size();
            let s = window.scale_factor() as f32;
            resize_edge(
                self.cursor.0,
                self.cursor.1,
                size.width as f32 / s,
                size.height as f32 / s,
            )
        };
        if dir != self.resize_hint {
            self.resize_hint = dir;
            window.set_cursor(dir.map(edge_cursor).unwrap_or(winit::window::CursorIcon::Default));
        }
    }

    fn redraw(&mut self) {
        // Animation drives (desktop's W0.4 stand-in — the browser host uses
        // rAF the same way): while the tuner listens, fold the latest
        // backend reading into the state; while the arpeggio transport
        // runs, advance a step each beat at the transport bpm. Either keeps
        // frames coming.
        let mut animating = false;
        // Poll the MIDI seam (immutable) before borrowing runner/backend.
        let midi_in_connected = self.midi.connected_input().is_some();
        let midi_clock_bpm = self.midi.clock_bpm();
        let midi_events = self.midi.recent_events();
        // Clock-out master values, captured inside the update, pushed after.
        let mut clock_out_enabled = false;
        let mut clock_out_playing = false;
        let mut clock_out_bpm = 120.0_f32;
        if let (Some(runner), Some(backend)) = (self.runner.as_mut(), self.backend.as_mut()) {
            let now = std::time::Instant::now();
            let last_arp = &mut self.last_arp_step;
            let last_rehearsal = &mut self.last_rehearsal_step;
            runner.update(|ui| {
                if ui.tuner.enabled {
                    ui.tuner.reading = backend.tuner_reading();
                    animating = true;
                }
                if ui.song_playing {
                    if let Some(bar) = backend.song_bar() {
                        ui.song_bar_live = bar;
                    }
                    ui.song_recording = backend.song_recording();
                    ui.song_loop_bars = backend.song_loop_bars();
                    animating = true;
                }
                if ui.rehearsal_running && !ui.set.cards.is_empty() {
                    let cursor = ui.set.cursor.min(ui.set.cards.len() - 1);
                    match woodshed_core::card_dwell(
                        &ui.set.cards[cursor],
                        ui.transport.bpm,
                    ) {
                        Some(dwell) => {
                            match last_rehearsal {
                                Some(t) if now.duration_since(*t) >= dwell => {
                                    if woodshed_core::step_set(&mut ui.set, 1) {
                                        // Landed on a new card — voice its
                                        // material ("hear it as you land").
                                        let c = ui.set.cursor.min(ui.set.cards.len() - 1);
                                        let (pitches, d, strum) =
                                            ui.stage.card_voicing(&ui.set.cards[c]);
                                        if !pitches.is_empty() {
                                            backend.preview_pitches(&pitches, d, strum);
                                        }
                                    } else {
                                        // End of set, loop off: stop.
                                        ui.rehearsal_running = false;
                                    }
                                    *last_rehearsal = Some(now);
                                }
                                None => *last_rehearsal = Some(now),
                                _ => {}
                            }
                            animating = true;
                        }
                        // Manual card: the dwell transport waits here.
                        None => *last_rehearsal = None,
                    }
                } else {
                    *last_rehearsal = None;
                }
                let stepping = ui.stage.arpeggio_playing || ui.stage.exercise_playing;
                if stepping {
                    let beat = std::time::Duration::from_secs_f32(
                        60.0 / ui.transport.bpm.max(30.0),
                    );
                    match last_arp {
                        Some(t) if now.duration_since(*t) >= beat => {
                            // Sonify the step we land on — the arpeggio
                            // climbs audibly, the exercise plays its notes.
                            let note_secs = beat.as_secs_f32() * 0.85;
                            if ui.stage.arpeggio_playing {
                                ui.stage.arpeggio_advance();
                                if let Some(freq) = ui.stage.arpeggio_current_pitch_hz() {
                                    backend.preview_note(freq, note_secs);
                                }
                            }
                            if ui.stage.exercise_playing {
                                ui.stage.exercise_advance();
                                if let Some(freq) = ui.stage.exercise_current_pitch_hz() {
                                    backend.preview_note(freq, note_secs);
                                }
                            }
                            *last_arp = Some(now);
                        }
                        None => *last_arp = Some(now),
                        _ => {}
                    }
                    animating = true;
                } else {
                    *last_arp = None;
                }
                // MIDI: reflect polled state; slave the transport to
                // incoming clock; capture the clock-out master values.
                ui.midi.clock_bpm = midi_clock_bpm;
                ui.midi.events = midi_events.clone();
                if midi_in_connected
                    && (ui.midi.clock_slave
                        || ui.tab == woodshed_core::storage::Tab::Settings)
                {
                    animating = true;
                }
                if midi_in_connected && ui.midi.clock_slave {
                    if let Some(bpm) = midi_clock_bpm {
                        let bpm = bpm.clamp(30.0, 300.0);
                        if (ui.transport.bpm - bpm).abs() > 0.3 {
                            ui.transport.bpm = bpm;
                            backend.set_metronome(ui.transport);
                        }
                    }
                }
                // Latency calibration: poll while a run is active; drop
                // out of active on any terminal status.
                if ui.calib_active {
                    let status = backend.calibration_poll();
                    ui.calib_status = status;
                    animating = true;
                    if !matches!(status, CalibrationStatus::Running { .. }) {
                        ui.calib_active = false;
                    }
                }
                ui.latency_ms = backend.latency_ms();
                clock_out_enabled = ui.midi.clock_out;
                clock_out_playing = ui.transport.playing;
                clock_out_bpm = ui.transport.bpm;
            });
        }
        self.midi
            .set_clock_out(clock_out_enabled, clock_out_playing, clock_out_bpm);
        let tuner_live = animating;
        let (Some(window), Some(host), Some(runner)) =
            (self.window.as_ref(), self.host.as_ref(), self.runner.as_ref())
        else {
            return;
        };
        let size = window.inner_size();
        let (pw, ph) = (size.width.max(1), size.height.max(1));
        let scale = window.scale_factor() as f32;
        let (lw, lh) = (pw as f32 / scale, ph as f32 / scale);

        let now_s = self.anim_base.elapsed().as_secs_f64();
        let (scene, anim_active) = {
            let dom = runner.dom();
            let mut muts: Vec<DomMutation<NodeId>> = Vec::new();
            dom.borrow_mut().drain_mutations(&mut muts);
            let dom_ref = dom.borrow();
            let sheets: Vec<&str> = vec![self.sheet.as_str()];
            let structural = muts
                .iter()
                .any(|m| !matches!(m, DomMutation::AttributeChanged { .. }));
            let size_changed = self.layout_size != (lw, lh);
            match self.layout.as_mut() {
                Some(layout) if !structural && !size_changed => {
                    // Advance the CSS-transition clock to now (interpolating
                    // any in-flight transitions), then apply this frame's
                    // mutations — so a transition a class-swap starts runs
                    // from *now*, not a stale idle-frozen clock.
                    let _ = layout.tick_animations(&*dom_ref, now_s);
                    if !muts.is_empty() {
                        let _ = layout.apply(&*dom_ref, &sheets, &muts);
                    }
                }
                _ => {
                    let mut layout = IncrementalLayout::new(&*dom_ref, &sheets, lw, lh);
                    // Carry element scroll (wheel positions in overflow
                    // containers like the filmstrip) across rebuilds.
                    if let Some(prev) = self.layout.as_ref() {
                        layout.set_element_scroll(prev.element_scroll().clone());
                    }
                    self.layout = Some(layout);
                    self.layout_size = (lw, lh);
                }
            }
            let layout = self.layout.as_ref().expect("layout just ensured");
            let anim_active = layout.has_active_animations();
            let list = layout.emit_paint_list(
                &*dom_ref,
                &ScrollOffsets::default(),
                DeviceIntSize::new(lw as i32, lh as i32),
            );
            let translated = paint_list_render::translate_paint_cmd_stream(
                list.viewport(),
                list.commands(),
                list.fonts(),
                list.images(),
            );
            (translated.scene, anim_active)
        };

        let (_tex, view) = host.core().rasterize_scaled(
            &scene,
            pw,
            ph,
            ColorLoad::Clear(wgpu::Color::BLACK),
            scale,
        );
        let Some(frame) = host.acquire() else { return };
        let target = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        host.renderer().compose_external_texture(
            &view,
            &target,
            host.format(),
            pw,
            ph,
            ExternalTexturePlacement::new([0.0, 0.0, pw as f32, ph as f32]),
        );
        frame.present();
        if tuner_live || anim_active {
            window.request_redraw();
        }
    }

    /// Sync dropdown state into the core, push the audio state through the
    /// backend seam, persist the session, re-skin on a theme change, and
    /// repaint — the tail of every input dispatch.
    fn after_dispatch(&mut self) {
        let mut theme = self.theme;
        let mut persisted: Option<String> = None;
        if let Some(runner) = self.runner.as_mut() {
            let backend = self.backend.as_mut();
            let last_song = &mut self.last_song;
            runner.update(|ui| {
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
                    // Latency-calibration requests.
                    if std::mem::take(&mut ui.calib_start_requested) {
                        backend.calibration_start();
                        ui.calib_active = true;
                        ui.calib_status =
                            CalibrationStatus::Running { clicks_fired: 0, total: 6 };
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
                theme = ui.theme;
                persisted = serde_json::to_string(&ui.to_persisted()).ok();
            });
        }
        // Window-chrome requests (CSD). drag_window must run while the
        // press that requested it is still down — dispatch happens on
        // Pressed, so this is in-window.
        if let (Some(window), Some(runner)) = (self.window.as_ref(), self.runner.as_mut()) {
            let mut minimize = false;
            let mut maximize = false;
            let mut close = false;
            let mut drag = false;
            runner.update(|ui| {
                minimize = std::mem::take(&mut ui.chrome_minimize);
                maximize = std::mem::take(&mut ui.chrome_maximize);
                close = std::mem::take(&mut ui.chrome_close);
                drag = std::mem::take(&mut ui.chrome_drag);
            });
            if minimize {
                window.set_minimized(true);
            }
            if maximize {
                window.set_maximized(!window.is_maximized());
            }
            if close {
                self.close_requested = true;
            }
            if drag {
                let _ = window.drag_window();
            }
        }
        // MIDI device sync: connect / disconnect per the dropdowns, and
        // re-scan the port lists on request.
        if let Some(runner) = self.runner.as_mut() {
            let midi = &mut self.midi;
            runner.update(|ui| {
                if std::mem::take(&mut ui.midi.refresh_requested) {
                    ui.midi.input_ports = midi.input_ports();
                    ui.midi.output_ports = midi.output_ports();
                }
                let in_target = midi_port_at(&ui.midi.input_ports, ui.midi.input_dd.selected);
                if midi.connected_input() != in_target.as_deref() {
                    midi.connect_input(in_target.as_deref());
                }
                let out_target =
                    midi_port_at(&ui.midi.output_ports, ui.midi.output_dd.selected);
                if midi.connected_output() != out_target.as_deref() {
                    midi.connect_output(out_target.as_deref());
                }
                ui.midi.connected_in = midi.connected_input().map(str::to_string);
                ui.midi.connected_out = midi.connected_output().map(str::to_string);
            });
        }
        if theme != self.theme {
            self.theme = theme;
            self.sheet = theme.css();
            // Force a full relayout under the new sheet.
            self.layout = None;
            self.layout_size = (0.0, 0.0);
        }
        if let Some(json) = persisted {
            self.storage.save(&json);
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    /// Drive `:hover` / `:focus` restyles on target change (engine
    /// `set_interaction`; `Unchanged` when nothing interaction-sensitive
    /// matched, so idle movement stays free).
    fn hover(&mut self) {
        let (Some(runner), Some(layout)) = (self.runner.as_ref(), self.layout.as_mut()) else {
            return;
        };
        let (x, y) = self.cursor;
        let dom = runner.dom();
        let dom_ref = dom.borrow();
        let hovered = layout
            .hit_test(&*dom_ref, x, y, &ScrollOffsets::default())
            .map(|n| layout_dom_api::LayoutDom::opaque_id(&*dom_ref, n));
        let focused = runner
            .focus()
            .map(|n| layout_dom_api::LayoutDom::opaque_id(&*dom_ref, n));
        if (hovered, focused) == (self.last_hover, self.last_focus) {
            return;
        }
        self.last_hover = hovered;
        self.last_focus = focused;
        // Bring the transition clock to now before the flip, so a
        // hover/focus transition runs from now rather than a stale
        // idle-frozen clock (same reason as the redraw tick).
        let now_s = self.anim_base.elapsed().as_secs_f64();
        let _ = layout.tick_animations(&*dom_ref, now_s);
        let state = InteractionState {
            hovered: hovered.map(SourceNodeId),
            focused: focused.map(SourceNodeId),
            ..Default::default()
        };
        if layout.set_interaction(&*dom_ref, &state) != Applied::Unchanged {
            drop(dom_ref);
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
        }
    }

    fn click(&mut self) {
        let (Some(runner), Some(layout)) = (self.runner.as_mut(), self.layout.as_ref()) else {
            return;
        };
        let (x, y) = self.cursor;
        let hit = {
            let dom = runner.dom();
            let dom_ref = dom.borrow();
            layout.hit_test(&*dom_ref, x, y, &ScrollOffsets::default())
        };
        let Some(node) = hit else { return };
        runner.dispatch_click(
            node,
            PointerClick {
                local: (0.0, 0.0),
                prop: Propagation::new(),
            },
        );
        self.after_dispatch();
    }

    fn key(&mut self, event: &WinitKeyEvent) {
        if event.state != ElementState::Pressed {
            return;
        }
        let Some(runner) = self.runner.as_mut() else {
            return;
        };
        if let WinitKey::Named(WinitNamedKey::Tab) = event.logical_key {
            runner.focus_traverse(!self.modifiers.shift_key());
            self.after_dispatch();
            self.hover(); // focus changed → refresh :focus styling
            return;
        }
        if let WinitKey::Named(WinitNamedKey::Escape) = event.logical_key {
            // Close any open dropdown.
            runner.update(|ui| {
                ui.tuning_dd.open = false;
                ui.root_dd.open = false;
            });
            self.after_dispatch();
            return;
        }
        let mods = modifiers_from_winit(self.modifiers);
        if let Some(kev) = key_event_from_winit(&event.logical_key, mods) {
            runner.dispatch_key(kev);
            self.after_dispatch();
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("Woodshed")
                        // CSD: the app draws its own chrome (title row,
                        // window buttons, drag surface, edge resize).
                        .with_decorations(false)
                        // Top-anchored and short enough to clear the taskbar
                        // on a 720-logical-tall laptop screen (664 fits a
                        // ~672 work area); the OS/compositor may override the
                        // position (e.g. Wayland), which is fine.
                        .with_position(winit::dpi::LogicalPosition::new(40.0, 8.0))
                        .with_inner_size(winit::dpi::LogicalSize::new(1100.0, 664.0)),
                )
                .expect("create window"),
        );
        let size = window.inner_size();
        let host = SurfaceHost::boot(
            window.clone(),
            size.width.max(1),
            size.height.max(1),
            NetrenderOptions {
                tile_cache_size: Some(1024),
                enable_vello: true,
                ..Default::default()
            },
        )
        .expect("boot serval host");
        let backend = CpalBackend::new();
        let mut ui = UiState::new();
        ui.audio_error = backend.error().map(String::from);
        // Restore the previous session (W0.2): selections, tempo, theme, tab.
        if let Some(json) = self.storage.load() {
            match serde_json::from_str(&json) {
                Ok(session) => ui.apply_persisted(&session),
                Err(e) => eprintln!("[woodshed-serval] ignoring corrupt session: {e}"),
            }
        }
        self.theme = ui.theme;
        self.sheet = ui.theme.css();
        // Populate the MIDI port pickers with what's plugged in now.
        ui.midi.input_ports = self.midi.input_ports();
        ui.midi.output_ports = self.midi.output_ports();
        let dom = Rc::new(RefCell::new(ScriptedDom::new()));
        let runner = Runner::new(dom, stage_root as fn(&UiState) -> UiChild, ui);
        self.window = Some(window);
        self.host = Some(host);
        self.runner = Some(runner);
        self.backend = Some(backend);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(host) = self.host.as_mut() {
                    host.resize(size.width.max(1), size.height.max(1));
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            WindowEvent::ModifiersChanged(mods) => {
                self.modifiers = mods.state();
            }
            WindowEvent::CursorMoved { position, .. } => {
                let scale = self.scale_factor();
                self.cursor = (
                    (position.x / scale) as f32,
                    (position.y / scale) as f32,
                );
                self.update_resize_cursor();
                self.hover();
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                // Edge grab beats content when the window is floating.
                let edge = self.window.as_ref().and_then(|w| {
                    if w.is_maximized() {
                        return None;
                    }
                    let size = w.inner_size();
                    let scale = w.scale_factor() as f32;
                    resize_edge(
                        self.cursor.0,
                        self.cursor.1,
                        size.width as f32 / scale,
                        size.height as f32 / scale,
                    )
                });
                match edge {
                    Some(dir) => {
                        if let Some(w) = self.window.as_ref() {
                            let _ = w.drag_resize_window(dir);
                        }
                    }
                    None => self.click(),
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                // Wheel scrolls the nearest overflow container under the
                // cursor (the engine hit-tests, clamps, and chains).
                let (dx, dy) = serval_winit_host::wheel_delta_from_winit(delta);
                let (x, y) = self.cursor;
                let scrolled = if let (Some(runner), Some(layout)) =
                    (self.runner.as_ref(), self.layout.as_mut())
                {
                    let dom = runner.dom();
                    let dom_ref = dom.borrow();
                    layout.scroll_at(&*dom_ref, x, y, dx, dy)
                } else {
                    false
                };
                if scrolled {
                    if let Some(window) = self.window.as_ref() {
                        window.request_redraw();
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } => self.key(&event),
            WindowEvent::RedrawRequested => self.redraw(),
            _ => {}
        }
        if self.close_requested {
            event_loop.exit();
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().expect("event loop");
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App {
        window: None,
        host: None,
        runner: None,
        layout: None,
        layout_size: (0.0, 0.0),
        sheet: slate_stage_css(),
        cursor: (0.0, 0.0),
        modifiers: ModifiersState::empty(),
        backend: None,
        midi: MidiHost::new(),
        last_arp_step: None,
        last_rehearsal_step: None,
        storage: FsStorage::new(),
        theme: ThemeMode::default(),
        last_hover: None,
        last_focus: None,
        last_song: woodshed_core::song::SongDoc::default(),
        close_requested: false,
        resize_hint: None,
        anim_base: std::time::Instant::now(),
    };
    event_loop.run_app(&mut app).expect("run app");
}
