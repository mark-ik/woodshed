//! Woodshed in a browser.
//!
//! The claim this crate exists to make good on is narrow and worth stating
//! exactly: the views here are `woodshed-views`, the same module the desktop
//! binary draws, rendered by the same Cambium and genet-layout and netrender,
//! onto a canvas instead of a window. Nothing about the interface is
//! reimplemented in JavaScript, and nothing about it is browser-specific.
//!
//! What differs from `woodshed-genet` is the event source and what boots:
//!
//! | | desktop | here |
//! |---|---|---|
//! | source | `cambium-genet-winit-host` | `cambium-genet-web-host` |
//! | entry | `run` owns the thread | `mount` returns, listeners keep it alive |
//! | state | `Shared::boot`: settings, MIDI, sealed store | none yet, see below |
//!
//! ## What this does not do yet
//!
//! It mounts the S0 Stage sheet, not the application. `Shared::boot` opens a
//! persona-sealed store, a MIDI host, and an audio backend, none of which have
//! browser realizations wired up: `woodshed-audio` is `cpal`, and the persona
//! vault is a filesystem. Those are separate pieces of work with their own
//! seams already in place (muniment ships an IndexedDB backend, and the audio
//! seam is where WebAudio goes).
//!
//! So this is a receipt for the rendering and input path, and it is honest
//! about being one. A reader will hear a canvas with a label and no structure;
//! see `cambium-genet-web-host`'s accessibility module for why that gap is
//! stated rather than stubbed.
//!
//! ## Building it
//!
//! ```text
//! cargo build -p woodshed-web --target wasm32-unknown-unknown --release
//! wasm-bindgen --target web --no-typescript --out-dir pkg \
//!     target/wasm32-unknown-unknown/release/woodshed_web.wasm
//! ```
//!
//! The page needs a `<canvas id="woodshed">` sized by CSS; the host reads that
//! box and sizes the backing store to physical pixels itself.
#![cfg(target_arch = "wasm32")]

use cambium_genet_web_host::mount;
use cambium_rootstock::{HostHooks, HostOptions, Init};
use wasm_bindgen::prelude::*;
use woodshed_views::demo::{Child, DEMO_SHEET, demo_root};

/// The canvas this looks for when none is named.
const DEFAULT_CANVAS_ID: &str = "woodshed";

/// Mount Woodshed onto a canvas.
///
/// Returns once the application is up; the listeners and frame callback it
/// installed keep it running, which is how a browser application lives with
/// nothing holding it on the stack.
#[wasm_bindgen]
pub async fn start(canvas_id: Option<String>) -> Result<(), JsValue> {
    // Panics in a wasm module are otherwise an unhelpful "unreachable
    // executed" in the console, with no line and no message.
    console_error_panic_hook::set_once();

    let id = canvas_id.unwrap_or_else(|| DEFAULT_CANVAS_ID.to_string());
    let canvas = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(&id))
        .ok_or_else(|| JsValue::from_str(&format!("no element with id {id:?}")))?
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .map_err(|_| JsValue::from_str(&format!("element {id:?} is not a canvas")))?;

    let options = HostOptions {
        title: "Woodshed".into(),
        // A tab has no frame to draw, so the client-side decorations the
        // desktop build turns on have nothing to decorate here.
        decorations: true,
        ..Default::default()
    };

    mount(
        canvas,
        options,
        |_window, _commands, _wake| Init {
            state: (),
            logic: demo_root as fn(&()) -> Child,
            sheet: DEMO_SHEET.to_string(),
        },
        HostHooks {
            frame: Box::new(|_ctx| false),
            after_dispatch: Box::new(|_ctx| {}),
            after_frame: Box::new(|_ctx| {}),
            after_wake: Box::new(|_ctx| {}),
            close_request: Box::new(|_ctx, _request| {
                // Nothing closes a tab from inside the application.
                cambium_rootstock::CloseDisposition::KeepVisible
            }),
            focused_text: Box::new(|_runner| None),
            key_intercept: Box::new(|_runner, _press| false),
        },
    )
    .await
    .map_err(|e| JsValue::from_str(&e))?;

    Ok(())
}
