//! Woodshed's desktop application.
//!
//! There is no native host in this crate any more. `cambium-genet-winit-host`
//! owns the winit lifecycle, the genet surface, the retained layout, the paint
//! pass, hit testing, pointer/keyboard/IME/wheel routing, the overlay-scrollbar
//! fade, and the AccessKit install-before-show lifecycle — all of it extracted
//! from this file, which woodshed was the donor for. Woodshed is now its first
//! consumer.
//!
//! What is left here is woodshed: which views to render, which state they run
//! over, the audio and MIDI seams, persistence, the custom-paint leaves, and
//! the self-drive lane. It reaches the host through seven plain closures
//! ([`HostHooks`]) with its own state in their captured environment.
//!
//! | hook | woodshed's half |
//! |------|-----------------|
//! | `frame` | advance the transports and the tuner ([`drive`]), refresh the leaves ([`leaves`]) |
//! | `after_dispatch` | push state through the backend, MIDI, chrome, and persistence seams ([`sync`]) |
//! | `after_frame` | pump the scenario one step ([`scenario`]) |
//! | `after_wake` | no worker channel to drain yet |
//! | `close_request` | exit: Woodshed has no background operation to retain |
//! | `focused_text` | which of the two text fields has the caret ([`text`]) |
//! | `key_intercept` | Escape closes an open dropdown |

mod audio;
mod drive;
mod leaves;
mod midi;
mod persona;
mod scenario;
mod session;
mod shared;
mod storage;
mod sync;
mod text;

use std::cell::RefCell;
use std::rc::Rc;

use cambium::{clickable, el, text as text_node};
use cambium_genet_winit_host::{
    HostHooks, HostOptions, Init, Key, KeyPress, NamedKey, Runner, WindowCommands, run,
};
use woodshed_core::audio::AudioBackend as _;
use woodshed_core::midi::MidiBackend as _;
use woodshed_core::settings::WindowSettings;
use woodshed_views::stage::{UiChild, UiState, ViewportClass, stage_root};

use crate::audio::CpalBackend;
use crate::shared::Shared;
use crate::sync::{Ctx, Logic};

/// One caption button. The glyph is what the eye reads; `aria-label` is what
/// the ear gets — without it a screen reader announces these as "dash",
/// "white square", and "multiplication sign".
///
/// The handler captures the window-verb handle rather than setting a flag on
/// `UiState`: window control is a desktop concern, and the shared state is
/// also the browser host's.
fn caption(
    glyph: &'static str,
    name: &'static str,
    class: &'static str,
    commands: &WindowCommands,
    verb: fn(&WindowCommands),
) -> UiChild {
    let commands = commands.clone();
    Box::new(clickable(
        el("div", text_node(glyph))
            .attr("class", class)
            .attr("role", "button")
            .attr("aria-label", name),
        move |_ui: &mut UiState, _| verb(&commands),
    ))
}

/// Desktop-only frame. The shared Woodshed root deliberately contains product
/// UI only, so a browser host does not inherit window controls it cannot use.
///
/// The bar itself carries `--app-region: drag` in the sheet, so moving the
/// window, double-click-to-maximize, and the right-click system menu are the
/// host's and need no handler here.
fn desktop_chrome(commands: &WindowCommands) -> UiChild {
    Box::new(
        el(
            "div",
            (
                el("div", text_node("Woodshed")).attr("class", "chrome-title"),
                // Spacer. Hidden from the accessibility tree: it is a
                // grabbable gap, not a control, and would otherwise be a
                // focus stop that announces "group" and does nothing.
                el("div", ())
                    .attr("class", "chrome-drag")
                    .attr("aria-hidden", "true"),
                caption(
                    "–",
                    "Minimize",
                    "chrome-btn",
                    commands,
                    WindowCommands::minimize,
                ),
                caption(
                    "□",
                    "Maximize",
                    "chrome-btn",
                    commands,
                    WindowCommands::toggle_maximize,
                ),
                caption(
                    "×",
                    "Close",
                    "chrome-btn chrome-close",
                    commands,
                    WindowCommands::close,
                ),
            ),
        )
        .attr("class", "chrome"),
    )
}

fn desktop_root(ui: &UiState, commands: &WindowCommands) -> UiChild {
    Box::new(el("div", (desktop_chrome(commands), stage_root(ui))).attr("class", "desktop-frame"))
}

/// Build the starting state: the audio backend, the restored session, and the
/// application settings. Runs once, inside the host, after the window exists
/// but before the first frame.
fn boot_state(
    shared: &Rc<RefCell<Shared>>,
    window: &dyn cambium_genet_winit_host::HostWindow,
    commands: &WindowCommands,
) -> Init<UiState, Logic> {
    let mut shared = shared.borrow_mut();
    let backend = CpalBackend::new();
    let mut ui = UiState::new();
    let (size_w, size_h) = window.inner_size();
    let scale = window.scale_factor() as f32;
    ui.set_viewport_width(size_w as f32 / scale);
    ui.set_viewport_height(size_h as f32 / scale);
    ui.audio_error = backend.error().map(String::from);

    // Restore the artifact session and the separate application settings, when
    // there is a store to restore from. There is not, on a machine whose vault
    // holds several personas with none chosen: the gate goes up instead, and
    // `persona::after_dispatch` restores once the question is answered.
    match shared.storage.as_ref() {
        Some(storage) => session::restore(storage, &mut ui),
        None => persona::seed(&mut shared, &mut ui),
    }
    // Outside the match, deliberately. This lived inside `seed`, which only
    // runs on the gate path, so on every ordinary launch Settings reported no
    // persona while the store was sealed to one. One assignment, both paths,
    // and they cannot drift apart again.
    ui.seal = shared.seal.clone();
    shared.theme = ui.theme();
    shared.reduce_motion = ui.app_settings.accessibility.reduce_motion;
    shared.text_scale = ui.app_settings.accessibility.text_scale.clone();
    let sheet = shared.accessible_sheet();
    // Populate the MIDI port pickers with what's plugged in now.
    ui.midi.input_ports = shared.midi.input_ports();
    ui.midi.output_ports = shared.midi.output_ports();
    shared.backend = Some(backend);

    let commands = commands.clone();
    Init {
        state: ui,
        logic: Box::new(move |ui: &UiState| desktop_root(ui, &commands)) as Logic,
        sheet,
    }
}

/// Refresh the shared view's transient width band after a resize or DPI change.
/// Returns whether the retained root actually needed rebuilding.
fn sync_viewport(ctx: &mut Ctx<'_>) -> bool {
    let (width, height) = ctx.logical_size;
    let changed = {
        let ui = ctx.runner.state();
        ui.viewport != ViewportClass::for_width(width) || (ui.viewport_h - height).abs() >= 16.0
    };
    if !changed {
        return false;
    }
    ctx.runner.update(|ui| {
        // `|` not `||`: both must run, and either change needs a rebuild (the
        // height bounds a vertical board's scroll viewport).
        let _ = ui.set_viewport_width(width) | ui.set_viewport_height(height);
    });
    true
}

/// What Escape means, window-wide.
///
/// An intercept rather than a view handler because it is a policy, not a
/// control's behaviour: it runs before dispatch and does not care what has the
/// caret. Named rather than inline so a test drives the shipping decision
/// instead of a copy of it.
fn escape_policy(runner: &mut Runner<UiState, Logic, UiChild>, press: &KeyPress) -> bool {
    if !matches!(press.key, Key::Named(NamedKey::Escape)) {
        return false;
    }
    // While the persona gate is up, Escape is how you practise without a
    // persona, and it has to work on the first press. The gate's picker does
    // ask for the caret now, and would report its own Escape, but the policy
    // answers first and consumes it: declining is not a thing to make
    // conditional on a focus request having landed.
    if runner.state().persona.is_some() {
        runner.update(|ui| {
            if let Some(pick) = ui.persona.as_mut() {
                pick.record(woodshed_views::persona::dismissed());
            }
        });
        return true;
    }
    // Otherwise it closes any open dropdown.
    runner.update(|ui| {
        ui.tuning_dd.open = false;
        ui.root_dd.open = false;
    });
    true
}

fn to_host_geometry(settings: WindowSettings) -> cambium_genet_winit_host::WindowGeometry {
    cambium_genet_winit_host::WindowGeometry {
        position: (settings.x, settings.y),
        size: (settings.width, settings.height),
        maximized: settings.maximized,
    }
}

fn to_window_settings(geometry: cambium_genet_winit_host::WindowGeometry) -> WindowSettings {
    WindowSettings {
        x: geometry.position.0,
        y: geometry.position.1,
        width: geometry.size.0,
        height: geometry.size.1,
        maximized: geometry.maximized,
    }
}

fn initial_window_geometry(
    shared: &Rc<RefCell<Shared>>,
) -> Option<cambium_genet_winit_host::WindowGeometry> {
    shared
        .borrow()
        .storage
        .as_ref()
        .and_then(session::load_settings)
        .and_then(|settings| settings.window)
        .map(to_host_geometry)
}

fn persist_window_geometry(
    shared: &mut Shared,
    ctx: &mut Ctx<'_>,
    geometry: cambium_genet_winit_host::WindowGeometry,
) {
    let mut json = None;
    ctx.runner.update(|ui| {
        ui.app_settings.window = Some(to_window_settings(geometry));
        json = serde_json::to_string(&ui.app_settings).ok();
    });
    if let (Some(storage), Some(json)) = (shared.storage.as_ref(), json) {
        storage.save_settings(&json);
    }
}

fn hooks(shared: &Rc<RefCell<Shared>>) -> HostHooks<UiState, Logic, UiChild> {
    let frame_shared = shared.clone();
    let dispatch_shared = shared.clone();
    let after_frame_shared = shared.clone();
    let close_shared = shared.clone();
    HostHooks {
        frame: Box::new(move |ctx: &mut Ctx<'_>| {
            let mut shared = frame_shared.borrow_mut();
            let drag_active = ctx.runner.state().set_graph_drag_active;
            shared.drag_frame_metrics.begin(drag_active);
            let phase = std::time::Instant::now();
            let viewport_rebuilt = sync_viewport(ctx);
            shared
                .drag_frame_metrics
                .note_viewport(phase.elapsed(), viewport_rebuilt);
            let mut animating = false;
            let phase = std::time::Instant::now();
            let drive_rebuilt =
                !drag_active || drive::requires_live_frame(&shared, ctx.runner.state());
            if drive_rebuilt {
                ctx.runner
                    .update(|ui| animating = drive::frame(&mut shared, ui));
            }
            shared
                .drag_frame_metrics
                .note_drive(phase.elapsed(), drive_rebuilt);
            let (out_enabled, out_playing, out_bpm) = drive::clock_out(ctx.runner.state());
            shared.midi.set_clock_out(out_enabled, out_playing, out_bpm);
            let phase = std::time::Instant::now();
            if drag_active && !drive_rebuilt {
                leaves::sync_set_graph(&mut shared, ctx.runner.state(), ctx.leaves);
            } else {
                leaves::sync_all(&mut shared, ctx.runner.state(), ctx.leaves);
            }
            shared.drag_frame_metrics.note_leaves(phase.elapsed());
            animating
        }),
        after_dispatch: Box::new(move |ctx: &mut Ctx<'_>| {
            let mut shared = dispatch_shared.borrow_mut();
            sync::after_dispatch(&mut shared, ctx);
        }),
        after_frame: Box::new(move |ctx: &mut Ctx<'_>| {
            let mut shared = after_frame_shared.borrow_mut();
            shared.drag_frame_metrics.finish(ctx.frame_profile);
            scenario::drive(&mut shared, ctx);
        }),
        after_wake: Box::new(|_ctx| {}),
        close_request: Box::new(move |ctx, _request| {
            if let Some(geometry) = ctx.geometry {
                persist_window_geometry(&mut close_shared.borrow_mut(), ctx, geometry);
            }
            cambium_genet_winit_host::CloseDisposition::Exit
        }),
        focused_text: Box::new(text::focused_text),
        key_intercept: Box::new(escape_policy),
    }
}

fn main() {
    let shared = Shared::boot();
    let init_shared = shared.clone();
    let initial_geometry = initial_window_geometry(&shared);
    let options = HostOptions {
        title: "Woodshed".into(),
        // CSD: the app draws its own chrome (title row, window buttons, drag
        // surface); the host supplies the edge-resize grab margins and cursors.
        decorations: false,
        initial_logical_size: (1_100.0, 664.0),
        initial_geometry,
        // A scenario run asks for a deterministic window: a receipt captured at
        // a different size is a different layout.
        size_env: Some(("WOODSHED_WIDTH".into(), "WOODSHED_HEIGHT".into())),
        ..Default::default()
    };
    run(
        options,
        move |window, commands, _wake| boot_state(&init_shared, window, commands),
        hooks(&shared),
    )
    .expect("run app");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_geometry_conversion_preserves_every_axis() {
        let host = cambium_genet_winit_host::WindowGeometry {
            position: (120.5, 80.25),
            size: (900.0, 640.0),
            maximized: true,
        };
        assert_eq!(to_host_geometry(to_window_settings(host)), host);
    }
}
