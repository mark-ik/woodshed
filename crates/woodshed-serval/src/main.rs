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
mod storage;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use audio::CpalBackend;
use storage::FsStorage;
use woodshed_core::audio::AudioBackend;
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
    /// Last arpeggio auto-advance instant (the step clock while the
    /// arpeggio transport runs).
    last_arp_step: Option<std::time::Instant>,
    /// The W0.2 storage seam: fs on desktop, OPFS on the web host.
    storage: FsStorage,
    /// Theme the current sheet was generated from; a change re-skins.
    theme: ThemeMode,
    /// The hovered node's opaque id, for `:hover` restyles on target
    /// change (not per pixel).
    last_hover: Option<u64>,
}

impl App {
    fn scale_factor(&self) -> f64 {
        self.window.as_ref().map_or(1.0, |w| w.scale_factor())
    }

    fn redraw(&mut self) {
        // Animation drives (desktop's W0.4 stand-in — the browser host uses
        // rAF the same way): while the tuner listens, fold the latest
        // backend reading into the state; while the arpeggio transport
        // runs, advance a step each beat at the transport bpm. Either keeps
        // frames coming.
        let mut animating = false;
        if let (Some(runner), Some(backend)) = (self.runner.as_mut(), self.backend.as_ref()) {
            let now = std::time::Instant::now();
            let last_arp = &mut self.last_arp_step;
            runner.update(|ui| {
                if ui.tuner.enabled {
                    ui.tuner.reading = backend.tuner_reading();
                    animating = true;
                }
                let stepping = ui.stage.arpeggio_playing || ui.stage.exercise_playing;
                if stepping {
                    let beat = std::time::Duration::from_secs_f32(
                        60.0 / ui.transport.bpm.max(30.0),
                    );
                    match last_arp {
                        Some(t) if now.duration_since(*t) >= beat => {
                            if ui.stage.arpeggio_playing {
                                ui.stage.arpeggio_advance();
                            }
                            if ui.stage.exercise_playing {
                                ui.stage.exercise_advance();
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
            });
        }
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

        let scene = {
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
            translated.scene
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
        if tuner_live {
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
            runner.update(|ui| {
                ui.sync();
                if let Some(backend) = backend {
                    backend.set_metronome(ui.transport);
                    backend.set_tuner_enabled(ui.tuner.enabled);
                }
                theme = ui.theme;
                persisted = serde_json::to_string(&ui.to_persisted()).ok();
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

    /// Drive `:hover` restyles on pointer-target change (engine
    /// `set_interaction`; `Unchanged` when nothing hover-sensitive
    /// matched, so idle mouse movement stays free).
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
        if hovered == self.last_hover {
            return;
        }
        self.last_hover = hovered;
        let state = InteractionState {
            hovered: hovered.map(SourceNodeId),
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
                        .with_title("Woodshed (serval host)")
                        .with_inner_size(winit::dpi::LogicalSize::new(1100.0, 720.0)),
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
                self.hover();
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => self.click(),
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
        last_arp_step: None,
        storage: FsStorage::new(),
        theme: ThemeMode::default(),
        last_hover: None,
    };
    event_loop.run_app(&mut app).expect("run app");
}
