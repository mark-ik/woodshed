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
//! the self-drive lane. It reaches the host through five plain closures
//! ([`HostHooks`]) with its own state in their captured environment.
//!
//! | hook | woodshed's half |
//! |------|-----------------|
//! | `frame` | advance the transports and the tuner ([`drive`]), refresh the leaves ([`leaves`]) |
//! | `after_dispatch` | push state through the backend, MIDI, chrome, and persistence seams ([`sync`]) |
//! | `after_frame` | pump the scenario one step ([`scenario`]) |
//! | `focused_text` | which of the two text fields has the caret ([`text`]) |
//! | `key_intercept` | Escape closes an open dropdown |

mod audio;
mod drive;
mod leaves;
mod midi;
mod scenario;
mod shared;
mod storage;
mod sync;
mod text;

use std::cell::RefCell;
use std::rc::Rc;

use cambium::{clickable, el, text as text_node};
use cambium_genet_winit_host::{HostHooks, HostOptions, Init, WindowCommands, run};
use winit::keyboard::{Key as WinitKey, NamedKey as WinitNamedKey};
use woodshed_core::audio::AudioBackend as _;
use woodshed_core::midi::MidiBackend as _;
use woodshed_views::stage::{UiChild, UiState, stage_root};

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
                caption("–", "Minimize", "chrome-btn", commands, WindowCommands::minimize),
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
    window: &winit::window::Window,
    commands: &WindowCommands,
) -> Init<UiState, Logic> {
    let mut shared = shared.borrow_mut();
    let backend = CpalBackend::new();
    let mut ui = UiState::new();
    let size = window.inner_size();
    let scale = window.scale_factor() as f32;
    ui.set_viewport_width(size.width as f32 / scale);
    ui.set_viewport_height(size.height as f32 / scale);
    ui.audio_error = backend.error().map(String::from);

    // Restore the artifact session and the separate application settings. A
    // legacy flat session migrates its settings when no split settings file
    // exists yet.
    let mut app_settings = shared
        .storage
        .load_settings()
        .and_then(|json| match serde_json::from_str(&json) {
            Ok(settings) => Some(settings),
            Err(error) => {
                eprintln!("[woodshed-genet] ignoring corrupt application settings: {error}");
                None
            }
        })
        .unwrap_or_default();
    if let Some(json) = shared.storage.load() {
        match woodshed_core::storage::decode_session(&json) {
            Ok(loaded) => {
                if shared.storage.load_settings().is_none() {
                    if let Some(legacy) = loaded.legacy_settings {
                        app_settings = legacy;
                    }
                }
                ui.apply_persisted(&loaded.session, app_settings);
            }
            Err(e) => eprintln!("[woodshed-genet] ignoring corrupt session: {e}"),
        }
    }
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
/// The view only rebuilds when crossing a band boundary, so this is cheap to run
/// every frame — and running it every frame is what lets the host stay ignorant
/// of the application's viewport shape.
fn sync_viewport(ctx: &mut Ctx<'_>) {
    let (width, height) = ctx.logical_size;
    ctx.runner.update(|ui| {
        // `|` not `||`: both must run, and either change needs a rebuild (the
        // height bounds a vertical board's scroll viewport).
        let _ = ui.set_viewport_width(width) | ui.set_viewport_height(height);
    });
}

fn hooks(shared: &Rc<RefCell<Shared>>) -> HostHooks<UiState, Logic, UiChild> {
    let frame_shared = shared.clone();
    let dispatch_shared = shared.clone();
    let after_frame_shared = shared.clone();
    HostHooks {
        frame: Box::new(move |ctx: &mut Ctx<'_>| {
            let mut shared = frame_shared.borrow_mut();
            sync_viewport(ctx);
            let mut animating = false;
            ctx.runner.update(|ui| animating = drive::frame(&mut shared, ui));
            let (out_enabled, out_playing, out_bpm) = drive::clock_out(ctx.runner.state());
            shared.midi.set_clock_out(out_enabled, out_playing, out_bpm);
            leaves::sync_all(&mut shared, ctx.runner.state(), ctx.leaves);
            animating
        }),
        after_dispatch: Box::new(move |ctx: &mut Ctx<'_>| {
            let mut shared = dispatch_shared.borrow_mut();
            sync::after_dispatch(&mut shared, ctx);
        }),
        after_frame: Box::new(move |ctx: &mut Ctx<'_>| {
            let mut shared = after_frame_shared.borrow_mut();
            scenario::drive(&mut shared, ctx);
        }),
        focused_text: Box::new(text::focused_text),
        key_intercept: Box::new(|runner, press| {
            // Escape closes any open dropdown. Deliberately an intercept rather
            // than a view handler: it is a window-wide policy, not a control's.
            if !matches!(press.key, WinitKey::Named(WinitNamedKey::Escape)) {
                return false;
            }
            runner.update(|ui| {
                ui.tuning_dd.open = false;
                ui.root_dd.open = false;
            });
            true
        }),
    }
}

fn main() {
    let shared = Shared::boot();
    let init_shared = shared.clone();
    let options = HostOptions {
        title: "Woodshed".into(),
        // CSD: the app draws its own chrome (title row, window buttons, drag
        // surface); the host supplies the edge-resize grab margins and cursors.
        decorations: false,
        initial_logical_size: (1_100.0, 664.0),
        // A scenario run asks for a deterministic window: a receipt captured at
        // a different size is a different layout.
        size_env: Some(("WOODSHED_WIDTH".into(), "WOODSHED_HEIGHT".into())),
        ..Default::default()
    };
    run(
        options,
        move |window, commands| boot_state(&init_shared, window, commands),
        hooks(&shared),
    )
    .expect("run app");
}
