//! Woodshed's genet desktop host (S2: interaction spine).
//!
//! A winit window presenting `woodshed-views`' Stage screen over live state:
//! `GenetAppRunner` diffs the views into a `ScriptedDom`, a retained
//! `IncrementalLayout` lays it out at logical size (DPI aware, incremental
//! `apply` for attribute-only batches), paint emission lowers to a
//! `netrender::Scene`, and `genet-winit-host`'s `SurfaceHost` rasterizes at
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
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use audio::CpalBackend;
use midi::MidiHost;
use storage::FsStorage;
use woodshed_core::audio::{AudioBackend, CalibrationStatus};
use woodshed_core::midi::MidiBackend as _;
use woodshed_core::storage::Storage as _;
use woodshed_views::theme::ThemeMode;

use layout_dom_api::{DomMutation, LayoutDomMut as _};
use netrender::{ColorLoad, ExternalTexturePlacement, NetrenderOptions};
use paint_list_api::{DeviceIntSize, PaintList as _};
use genet_layout::{
    Applied, IncrementalLayout, InteractionState, LeafPaintSource, ScrollOffsets, SourceNodeId,
};
use genet_scripted_dom::{NodeId, ScriptedDom};
use cambium_winit::{key_event_from_winit, modifiers_from_winit, A11yHost};
use genet_winit_host::SurfaceHost;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent as WinitKeyEvent, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key as WinitKey, ModifiersState, NamedKey as WinitNamedKey};
use winit::window::{Window, WindowId};
use woodshed_views::stage::{stage_root, UiChild, UiState, NEIGHBORHOOD_LEAF_KEY};
use woodshed_views::theme::slate_stage_css;
use cambium::{
    GenetAppRunner, HoverEvent, HoverPhase, PointerClick, Propagation, clickable, el, text,
};

type Runner = GenetAppRunner<UiState, fn(&UiState) -> UiChild, UiChild>;

struct SpriggingSource<'a>(&'a sprigging::RenderedLeaves);

impl LeafPaintSource for SpriggingSource<'_> {
    fn leaf_commands(&self, key: u64) -> Option<&[paint_list_api::PaintCmd]> {
        self.0.get(key)
    }
}

/// Desktop-only frame. The shared Woodshed root deliberately contains product
/// UI only, so a browser host does not inherit window controls it cannot use.
fn desktop_chrome(_ui: &UiState) -> UiChild {
    Box::new(
        el(
            "div",
            (
                el("div", text("Woodshed")).attr("class", "chrome-title"),
                clickable(
                    el("div", ()).attr("class", "chrome-drag"),
                    |ui: &mut UiState, _| {
                        ui.chrome_drag = true;
                    },
                ),
                clickable(
                    el("div", text("–")).attr("class", "chrome-btn"),
                    |ui: &mut UiState, _| ui.chrome_minimize = true,
                ),
                clickable(
                    el("div", text("□")).attr("class", "chrome-btn"),
                    |ui: &mut UiState, _| ui.chrome_maximize = true,
                ),
                clickable(
                    el("div", text("×")).attr("class", "chrome-btn chrome-close"),
                    |ui: &mut UiState, _| ui.chrome_close = true,
                ),
            ),
        )
        .attr("class", "chrome"),
    )
}

fn desktop_root(ui: &UiState) -> UiChild {
    Box::new(el("div", (desktop_chrome(ui), stage_root(ui))).attr("class", "desktop-frame"))
}

struct App {
    window: Option<Arc<Window>>,
    host: Option<SurfaceHost>,
    runner: Option<Runner>,
    /// Retained layout session in logical coordinates — hit-test target
    /// and incremental-apply subject.
    layout: Option<IncrementalLayout<NodeId>>,
    /// Logical size the retained layout was built at.
    layout_size: (f32, f32),
    neighborhood_leaves: sprigging::LeafRegistry<u64>,
    neighborhood_rendered: sprigging::RenderedLeaves,
    neighborhood_sig: u64,
    fretboard_sig: u64,
    rehearsal_fretboard_sig: u64,
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
    /// The hovered hit node itself, for routing Cambium `on_hover` Enter/Leave
    /// transitions (dispatch_hover). Distinct from `last_hover` (an opaque id
    /// for CSS `:hover`).
    last_hover_hit: Option<NodeId>,
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
    /// The shared accessibility host (Tier 0, `cambium-winit`). `None` until
    /// `resumed` creates it; it owns the OS adapter, the per-frame tree, and the
    /// install-before-show lifecycle.
    a11y: Option<A11yHost>,
    /// Set by the adapter's wake callback when a screen reader acts while the app
    /// is otherwise idle; `about_to_wait` turns it into a redraw so the queued
    /// action gets drained.
    a11y_wake: Arc<AtomicBool>,
    /// The accessibility skin the current sheet was generated with; a change
    /// re-skins, alongside the theme.
    reduce_motion: bool,
    text_scale: String,
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

/// The product palette for a Related node kind. Lives host-side so Cambium's
/// graph component stays palette-neutral; the same mapping drove the old glyph.
fn related_kind_color(kind: &str) -> sprigging::ColorF {
    let [r, g, b] = match kind {
        "Scale" => [0.30, 0.67, 0.76],
        "Chord" => [0.91, 0.38, 0.25],
        "Arpeggio" => [0.70, 0.46, 0.86],
        "Progression" => [0.91, 0.68, 0.28],
        "Exercise" => [0.40, 0.72, 0.42],
        _ => [0.72, 0.74, 0.78],
    };
    sprigging::ColorF { r, g, b, a: 1.0 }
}

impl App {
    /// Paint the Related graph swatch's leaf from the shared swatch model
    /// (built by the view layer), rebuilding only when the paint changes (node
    /// positions/kinds, hovered emphasis, or size). The palette lives here so
    /// product colours stay host-side; the geometry and interactivity are the
    /// component's.
    fn sync_related_swatch(&mut self) {
        let swatch = {
            let Some(runner) = self.runner.as_ref() else {
                return;
            };
            woodshed_views::stage::related_swatch(runner.state())
        };
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        swatch.width.hash(&mut hasher);
        swatch.height.hash(&mut hasher);
        for node in &swatch.graph.nodes {
            node.position.0.to_bits().hash(&mut hasher);
            node.position.1.to_bits().hash(&mut hasher);
            node.kind.hash(&mut hasher);
        }
        let hovered_idx = swatch
            .hovered
            .as_ref()
            .and_then(|h| swatch.graph.nodes.iter().position(|n| &n.id == h));
        hovered_idx.hash(&mut hasher);
        let sig = hasher.finish();
        if sig == self.neighborhood_sig {
            return;
        }
        self.neighborhood_sig = sig;
        let leaf = swatch.paint_leaf(|kind: &&str| related_kind_color(kind));
        self.neighborhood_leaves
            .insert(NEIGHBORHOOD_LEAF_KEY, Box::new(leaf));
    }

    /// Rebuild the fretboard leaf from the current board when it changes. Mirrors
    /// [`Self::sync_neighborhood_leaf`]: the board's dots plus its shape are the
    /// model; the leaf paints the neck and markers.
    fn sync_fretboard_leaf(&mut self) {
        let Some(runner) = self.runner.as_ref() else {
            return;
        };
        let state = runner.state();
        let st = &state.stage;
        let string_count = st.string_count();
        let fret_start = st.fret_start;
        let fret_count = st.fret_count;
        let marker_style = state.app_settings.fretboard.marker_style.clone();
        let orientation = woodshed_views::fretboard_leaf::Orientation::from_name(
            &state.app_settings.fretboard.orientation,
        );
        let distinguish_root = state.app_settings.accessibility.distinguish_root;
        // Scale / Chord carry their richer FretDots (pins, detail cards); the
        // transport lenses (Arpeggio / Exercise / Progression) feed uniform
        // LensMarkers. Either way the leaf needs only the position and whether it
        // is the root — the sounding step rides `set_active` in sync_fretboard_active.
        let dots: Vec<woodshed_views::fretboard_leaf::Dot> = match st.lens {
            woodshed_core::Lens::Scales | woodshed_core::Lens::Chords => st
                .dots()
                .into_iter()
                .map(|d| woodshed_views::fretboard_leaf::Dot {
                    string_index: d.string_index,
                    fret: d.fret,
                    is_root: d.is_root,
                    marked: false,
                    excluded: false,
                })
                .collect(),
            _ => st
                .lens_markers()
                .0
                .into_iter()
                .map(|m| woodshed_views::fretboard_leaf::Dot {
                    string_index: m.string_index,
                    fret: m.fret,
                    is_root: m.is_root,
                    marked: false,
                    excluded: false,
                })
                .collect(),
        };
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        (st.lens as usize).hash(&mut hasher);
        string_count.hash(&mut hasher);
        fret_start.hash(&mut hasher);
        fret_count.hash(&mut hasher);
        marker_style.hash(&mut hasher);
        matches!(orientation, woodshed_views::fretboard_leaf::Orientation::Vertical)
            .hash(&mut hasher);
        distinguish_root.hash(&mut hasher);
        for d in &dots {
            d.string_index.hash(&mut hasher);
            d.fret.hash(&mut hasher);
            d.is_root.hash(&mut hasher);
        }
        let sig = hasher.finish();
        if sig == self.fretboard_sig {
            return;
        }
        let leaf = woodshed_views::fretboard_leaf::FretboardLeaf::new(
            string_count,
            fret_start,
            fret_count,
            orientation,
            distinguish_root,
            dots,
            woodshed_views::fretboard_leaf::MarkerStyle::from_name(&marker_style),
        );
        self.neighborhood_leaves
            .insert(woodshed_views::fretboard_leaf::FRETBOARD_LEAF_KEY, Box::new(leaf));
        self.fretboard_sig = sig;
    }

    /// Push the run's live values into the fretboard leaf each frame: the active
    /// (sounding) note, and the touch's path trail + whether to draw it. The leaf
    /// is otherwise only rebuilt on a board change; the run and the Path toggle
    /// move between those.
    fn sync_fretboard_active(&mut self) {
        let (active, path, show_path) = self
            .runner
            .as_ref()
            .map(|r| {
                let st = &r.state().stage;
                // Scale / Chord: the running step. Transport lenses (Arpeggio /
                // Exercise): the board's current position, which advances live as
                // the transport steps, so it is read here each frame rather than
                // baked into the rebuilt-on-change leaf.
                let active = match st.lens {
                    woodshed_core::Lens::Scales | woodshed_core::Lens::Chords => {
                        st.scale_run_playing.then_some(st.scale_run_active).flatten()
                    }
                    _ => st.lens_markers().1,
                };
                // Draw mode always shows the trail (you're editing it).
                let show = st.path_shown || st.draw_mode;
                let path = if show { st.run_positions() } else { Vec::new() };
                (active, path, show)
            })
            .unwrap_or((None, Vec::new(), false));
        if let Some(leaf) = self
            .neighborhood_leaves
            .get_mut_as::<woodshed_views::fretboard_leaf::FretboardLeaf>(
                &woodshed_views::fretboard_leaf::FRETBOARD_LEAF_KEY,
            )
        {
            leaf.set_active(active);
            leaf.set_path(path, show_path);
        }
    }

    /// The Rehearsal board is a second fretboard leaf, fed from the card under
    /// the cursor (not the live Stage lens) and carrying that card's marks and
    /// mark mode. Only rebuilt while Rehearsal is on screen and the model
    /// changes; the label overlay drives click-to-mark.
    fn sync_rehearsal_fretboard_leaf(&mut self) {
        let Some(runner) = self.runner.as_ref() else {
            return;
        };
        let state = runner.state();
        if state.section != woodshed_core::storage::AppSection::Rehearsal
            || state.set.cards.is_empty()
        {
            return;
        }
        let cursor = state.set.cursor.min(state.set.cards.len() - 1);
        let card = &state.set.cards[cursor];
        let st = &state.stage;
        let string_count = st.string_count();
        let fret_start = st.fret_start;
        let fret_count = st.fret_count;
        let marker_style = state.app_settings.fretboard.marker_style.clone();
        let orientation = woodshed_views::fretboard_leaf::Orientation::from_name(
            &state.app_settings.fretboard.orientation,
        );
        let distinguish_root = state.app_settings.accessibility.distinguish_root;
        // marked / excluded come from the same UiState helpers the view uses, so
        // the mode logic (Off / Solo / Mute) lives in exactly one place.
        let dots: Vec<woodshed_views::fretboard_leaf::Dot> = st
            .dots_for_card(card)
            .into_iter()
            .map(|d| woodshed_views::fretboard_leaf::Dot {
                string_index: d.string_index,
                fret: d.fret,
                is_root: d.is_root,
                marked: state.card_marked(d.string_index, d.fret),
                excluded: state.card_excluded(d.string_index, d.fret),
            })
            .collect();
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        cursor.hash(&mut hasher);
        string_count.hash(&mut hasher);
        fret_start.hash(&mut hasher);
        fret_count.hash(&mut hasher);
        marker_style.hash(&mut hasher);
        matches!(orientation, woodshed_views::fretboard_leaf::Orientation::Vertical)
            .hash(&mut hasher);
        distinguish_root.hash(&mut hasher);
        for d in &dots {
            d.string_index.hash(&mut hasher);
            d.fret.hash(&mut hasher);
            d.is_root.hash(&mut hasher);
            d.marked.hash(&mut hasher);
            d.excluded.hash(&mut hasher);
        }
        let sig = hasher.finish();
        if sig == self.rehearsal_fretboard_sig {
            return;
        }
        let leaf = woodshed_views::fretboard_leaf::FretboardLeaf::new(
            string_count,
            fret_start,
            fret_count,
            orientation,
            distinguish_root,
            dots,
            woodshed_views::fretboard_leaf::MarkerStyle::from_name(&marker_style),
        );
        self.neighborhood_leaves.insert(
            woodshed_views::fretboard_leaf::REHEARSAL_FRETBOARD_LEAF_KEY,
            Box::new(leaf),
        );
        self.rehearsal_fretboard_sig = sig;
    }

    /// Refresh the shared view's transient width band after a resize or DPI
    /// change. The view only rebuilds when crossing a band boundary.
    fn sync_viewport(&mut self) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        let width = window.inner_size().width as f32 / window.scale_factor() as f32;
        let mut changed = false;
        if let Some(runner) = self.runner.as_mut() {
            runner.update(|ui| changed = ui.set_viewport_width(width));
        }
        if changed {
            window.request_redraw();
        }
    }

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
            window.set_cursor(
                dir.map(edge_cursor)
                    .unwrap_or(winit::window::CursorIcon::Default),
            );
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
                    match woodshed_core::card_dwell(&ui.set.cards[cursor], ui.transport.bpm) {
                        Some(dwell) => {
                            match last_rehearsal {
                                Some(t) if now.duration_since(*t) >= dwell => {
                                    ui.complete_rehearsal_cursor();
                                    if woodshed_core::step_set(&mut ui.set, 1) {
                                        ui.record_rehearsal_cursor();
                                        // Landed on a new card — voice its
                                        // material ("hear it as you land").
                                        let c = ui.set.cursor.min(ui.set.cards.len() - 1);
                                        let (pitches, d, strum) =
                                            ui.stage.card_sounding_pitches(&ui.set.cards[c]);
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
                let stepping = ui.stage.arpeggio_playing
                    || ui.stage.exercise_playing
                    || ui.stage.scale_run_playing;
                if stepping {
                    let beat =
                        std::time::Duration::from_secs_f32(60.0 / ui.transport.bpm.max(30.0));
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
                            if ui.stage.scale_run_playing {
                                if let Some(freq) = ui.stage.scale_run_tick() {
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
                        || ui.section == woodshed_core::storage::AppSection::Settings)
                {
                    animating = true;
                }
                if midi_in_connected && ui.midi.clock_slave {
                    if let Some(bpm) = midi_clock_bpm {
                        let bpm = bpm.clamp(30.0, 300.0);
                        if (ui.transport.bpm - bpm).abs() > 0.3 {
                            ui.set_bpm(bpm);
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
                // Track the neck-window settings + instrument into the stage
                // before the leaf/dots read fret_start/fret_count this frame.
                ui.sync_neck();
                // Two-way the card-rename buffer: adopt the selected card's
                // label when the selection moves, else commit what was typed.
                ui.sync_card_rename();
                clock_out_enabled = ui.midi.clock_out;
                clock_out_playing = ui.transport.playing;
                clock_out_bpm = ui.transport.bpm;
            });
        }
        self.midi
            .set_clock_out(clock_out_enabled, clock_out_playing, clock_out_bpm);
        self.sync_related_swatch();
        self.sync_fretboard_leaf();
        self.sync_fretboard_active();
        self.sync_rehearsal_fretboard_leaf();
        let tuner_live = animating;
        let (Some(window), Some(host), Some(runner)) = (
            self.window.as_ref(),
            self.host.as_ref(),
            self.runner.as_ref(),
        ) else {
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
            let sizes: HashMap<u64, (f32, f32)> =
                layout.custom_leaf_boxes().into_iter().collect();
            self.neighborhood_leaves.render_into(
                |key| {
                    sizes
                        .get(&key)
                        .map(|&(width, height)| sprigging::Size { width, height })
                },
                &mut self.neighborhood_rendered,
            );
            let source = SpriggingSource(&self.neighborhood_rendered);
            let list = layout.emit_paint_list_with_leaves(
                &*dom_ref,
                &ScrollOffsets::default(),
                DeviceIntSize::new(lw as i32, lh as i32),
                &source,
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
    /// Hand this frame to the shared accessibility host: it builds the tree (with
    /// each leaf's semantics), installs it the first frame (revealing the window)
    /// and updates it after, and returns the DOM nodes a screen reader asked to
    /// activate, which we route through the same click path a mouse uses. Called
    /// after `redraw`, once the layout for this frame exists.
    fn sync_a11y(&mut self) {
        let nodes = {
            // Take the DOM handle (an owned Rc) so `self.runner` is free again for
            // the dispatch below.
            let dom = match self.runner.as_ref() {
                Some(runner) => runner.dom(),
                None => return,
            };
            let dom_ref = dom.borrow();
            let (Some(a11y), Some(window), Some(layout)) =
                (self.a11y.as_mut(), self.window.as_ref(), self.layout.as_ref())
            else {
                return;
            };
            a11y.sync(
                window,
                &dom_ref,
                layout,
                &mut self.neighborhood_leaves,
                self.last_focus,
            )
        };
        if nodes.is_empty() {
            return;
        }
        for node in nodes {
            if let Some(runner) = self.runner.as_mut() {
                runner.dispatch_click(
                    node,
                    PointerClick {
                        local: (0.0, 0.0),
                        prop: Propagation::new(),
                    },
                );
            }
        }
        self.after_dispatch();
    }

    /// The current theme's sheet with the accessibility preferences applied
    /// (reduced motion, text scale). Regenerated on a theme or preference change.
    fn accessible_sheet(&self) -> String {
        woodshed_views::theme::apply_accessibility(
            self.theme.css(),
            self.reduce_motion,
            woodshed_views::theme::text_scale_factor(&self.text_scale),
        )
    }

    fn after_dispatch(&mut self) {
        let mut theme = self.theme;
        let mut a11y_reduce = self.reduce_motion;
        let mut a11y_scale = self.text_scale.clone();
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
                a11y_reduce = ui.app_settings.accessibility.reduce_motion;
                a11y_scale = ui.app_settings.accessibility.text_scale.clone();
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
                let out_target = midi_port_at(&ui.midi.output_ports, ui.midi.output_dd.selected);
                if midi.connected_output() != out_target.as_deref() {
                    midi.connect_output(out_target.as_deref());
                }
                ui.midi.connected_in = midi.connected_input().map(str::to_string);
                ui.midi.connected_out = midi.connected_output().map(str::to_string);
            });
        }
        if theme != self.theme
            || a11y_reduce != self.reduce_motion
            || a11y_scale != self.text_scale
        {
            self.theme = theme;
            self.reduce_motion = a11y_reduce;
            self.text_scale = a11y_scale;
            self.sheet = self.accessible_sheet();
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

    /// Route Cambium `on_hover` Enter/Leave as the hit node changes. The host
    /// owns transition detection: Leave the old hit, Enter the new one. Move is
    /// not routed (peeks only need enter/leave), so idle motion within a marker
    /// stays free. Coordinates are zeroed — the peek only needs which marker.
    fn hover_dispatch(&mut self) {
        let (x, y) = self.cursor;
        let hit = {
            let (Some(runner), Some(layout)) = (self.runner.as_ref(), self.layout.as_ref()) else {
                return;
            };
            let dom = runner.dom();
            let dom_ref = dom.borrow();
            layout.hit_test(&*dom_ref, x, y, &ScrollOffsets::default())
        };
        if hit == self.last_hover_hit {
            return;
        }
        let old = self.last_hover_hit.take();
        self.last_hover_hit = hit;
        let Some(runner) = self.runner.as_mut() else {
            return;
        };
        if let Some(old) = old {
            runner.dispatch_hover(old, HoverEvent::new(HoverPhase::Leave, (0.0, 0.0), (0.0, 0.0)));
        }
        if let Some(new) = hit {
            runner.dispatch_hover(new, HoverEvent::new(HoverPhase::Enter, (0.0, 0.0), (0.0, 0.0)));
        }
        self.after_dispatch();
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
        // Start comfortably on a large desktop, but never assume one. The
        // monitor API is available on every native host; browser hosts own
        // their canvas size separately.
        let (initial_pos, initial_size) = event_loop
            .primary_monitor()
            .map(|monitor| {
                let scale = monitor.scale_factor();
                let size = monitor.size();
                let pos = monitor.position();
                let logical_w = size.width as f64 / scale;
                let logical_h = size.height as f64 / scale;
                let width = 1_100.0_f64.min((logical_w - 48.0).max(480.0));
                let height = 664.0_f64.min((logical_h - 48.0).max(360.0));
                let x = pos.x as f64 / scale + ((logical_w - width) / 2.0).max(8.0);
                let y = pos.y as f64 / scale + ((logical_h - height) / 2.0).max(8.0);
                (
                    winit::dpi::LogicalPosition::new(x, y),
                    winit::dpi::LogicalSize::new(width, height),
                )
            })
            .unwrap_or((
                winit::dpi::LogicalPosition::new(40.0, 8.0),
                winit::dpi::LogicalSize::new(1_100.0, 664.0),
            ));
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("Woodshed")
                        // CSD: the app draws its own chrome (title row,
                        // window buttons, drag surface, edge resize).
                        .with_decorations(false)
                        // Hidden until the a11y adapter is installed on the first
                        // frame — the Windows AccessKit adapter must attach before
                        // the window is shown. Revealed in `sync_a11y`.
                        .with_visible(false)
                        .with_position(initial_pos)
                        .with_inner_size(initial_size),
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
        .expect("boot genet host");
        let backend = CpalBackend::new();
        let mut ui = UiState::new();
        ui.set_viewport_width(size.width as f32 / window.scale_factor() as f32);
        ui.audio_error = backend.error().map(String::from);
        // Restore the previous session (W0.2): selections, tempo, theme, tab.
        if let Some(json) = self.storage.load() {
            match serde_json::from_str(&json) {
                Ok(session) => ui.apply_persisted(&session),
                Err(e) => eprintln!("[woodshed-genet] ignoring corrupt session: {e}"),
            }
        }
        self.theme = ui.theme();
        self.reduce_motion = ui.app_settings.accessibility.reduce_motion;
        self.text_scale = ui.app_settings.accessibility.text_scale.clone();
        self.sheet = self.accessible_sheet();
        // Populate the MIDI port pickers with what's plugged in now.
        ui.midi.input_ports = self.midi.input_ports();
        ui.midi.output_ports = self.midi.output_ports();
        let dom = Rc::new(RefCell::new(ScriptedDom::new()));
        let runner = Runner::new(dom, desktop_root as fn(&UiState) -> UiChild, ui);
        // Create the shared accessibility host now (it installs the tree on the
        // first frame, once laid out, before the window is revealed). The wake
        // callback nudges the loop to drain a screen reader's action.
        let wake = self.a11y_wake.clone();
        self.a11y = Some(A11yHost::new(move || {
            wake.store(true, Ordering::Relaxed);
        }));
        self.window = Some(window);
        self.host = Some(host);
        self.runner = Some(runner);
        self.backend = Some(backend);
        // Drive the first frame here, synchronously, while the window is still
        // hidden: lay it out, install the a11y tree, then reveal it. A hidden
        // winit window may not receive a deferred redraw (the adapter's own
        // caveat), so we do not rely on one for the install-before-show step.
        self.redraw();
        self.sync_a11y();
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // A screen reader acted while the app was idle: the adapter set the wake
        // flag, so turn it into a redraw and `sync_a11y` will drain the action.
        if self.a11y_wake.swap(false, Ordering::Relaxed) {
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(host) = self.host.as_mut() {
                    host.resize(size.width.max(1), size.height.max(1));
                }
                self.sync_viewport();
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                self.sync_viewport();
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            WindowEvent::ModifiersChanged(mods) => {
                self.modifiers = mods.state();
            }
            WindowEvent::CursorMoved { position, .. } => {
                let scale = self.scale_factor();
                self.cursor = ((position.x / scale) as f32, (position.y / scale) as f32);
                self.update_resize_cursor();
                self.hover();
                self.hover_dispatch();
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
                let (dx, dy) = genet_winit_host::wheel_delta_from_winit(delta);
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
            WindowEvent::RedrawRequested => {
                self.redraw();
                // After the frame is laid out and presented, refresh the
                // accessibility tree and drain any screen-reader actions.
                self.sync_a11y();
            }
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
        neighborhood_leaves: sprigging::LeafRegistry::new(),
        neighborhood_rendered: sprigging::RenderedLeaves::new(),
        neighborhood_sig: 0,
        fretboard_sig: 0,
        rehearsal_fretboard_sig: 0,
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
        last_hover_hit: None,
        last_focus: None,
        last_song: woodshed_core::song::SongDoc::default(),
        close_requested: false,
        resize_hint: None,
        anim_base: std::time::Instant::now(),
        a11y: None,
        a11y_wake: Arc::new(AtomicBool::new(false)),
        reduce_motion: false,
        text_scale: "Normal".into(),
    };
    event_loop.run_app(&mut app).expect("run app");
}
