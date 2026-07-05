//! Woodshed's serval desktop host (S0 walking skeleton).
//!
//! A winit window presenting the `woodshed-views` view tree through serval:
//! `ServalAppRunner` diffs the views into a `ScriptedDom`, serval-layout lays
//! it out at window size, paint emission lowers to a `netrender::Scene`, and
//! `serval-winit-host`'s `SurfaceHost` rasterizes + composites onto the
//! backbuffer. The per-frame shape is the one `serval_winit_host` documents;
//! input dispatch arrives in S2, real `AppState` views in S1.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use netrender::{ColorLoad, ExternalTexturePlacement, NetrenderOptions, Scene};
use paint_list_api::PaintList as _;
use serval_layout::{NoImageLoader, ScrollOffsets};
use serval_scripted_dom::ScriptedDom;
use serval_winit_host::SurfaceHost;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};
use woodshed_views::demo::{demo_root, Child, DEMO_SHEET};
use xilem_serval::ServalAppRunner;

struct App {
    window: Option<Arc<Window>>,
    host: Option<SurfaceHost>,
    runner: Option<ServalAppRunner<(), fn(&()) -> Child, Child>>,
}

impl App {
    fn scene(&self, w: u32, h: u32) -> Option<Scene> {
        let runner = self.runner.as_ref()?;
        let dom = runner.dom();
        let dom_ref = dom.borrow();
        let sheets: Vec<&str> = vec![DEMO_SHEET];
        let layout = serval_layout::lay_out_content(&*dom_ref, &sheets, &NoImageLoader, w, h);
        let (list, _scroll, _links) =
            layout.emit_band(&*dom_ref, 0, h, &ScrollOffsets::default());
        let translated = paint_list_render::translate_paint_cmd_stream(
            list.viewport(),
            list.commands(),
            list.fonts(),
            list.images(),
        );
        Some(translated.scene)
    }

    fn redraw(&mut self) {
        let (Some(window), Some(host)) = (self.window.as_ref(), self.host.as_ref()) else {
            return;
        };
        let size = window.inner_size();
        let (w, h) = (size.width.max(1), size.height.max(1));
        let Some(scene) = self.scene(w, h) else { return };
        let (_tex, view) = host.rasterize(&scene, w, h, ColorLoad::Clear(wgpu::Color::BLACK));
        let Some(frame) = host.acquire() else { return };
        let target = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        host.renderer().compose_external_texture(
            &view,
            &target,
            host.format(),
            w,
            h,
            ExternalTexturePlacement::new([0.0, 0.0, w as f32, h as f32]),
        );
        frame.present();
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
                        .with_title("Woodshed (serval host, S0)")
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
        let dom = Rc::new(RefCell::new(ScriptedDom::new()));
        let runner = ServalAppRunner::new(dom, demo_root as fn(&()) -> Child, ());
        self.window = Some(window);
        self.host = Some(host);
        self.runner = Some(runner);
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
    };
    event_loop.run_app(&mut app).expect("run app");
}
