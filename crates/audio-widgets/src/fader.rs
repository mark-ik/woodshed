//! Gain fader — a slider over a dB range.
//!
//! A thin semantic wrapper over Xilem's tested `slider` view: the value
//! space *is* dB (linear-in-dB travel, which is how faders read), so
//! `on_change` hands the host a dB value directly. Reuses masonry's
//! drag-tested `Slider` widget rather than a custom one.
//!
//! Orientation note: masonry's `Slider` is horizontal-only, so this
//! fader is horizontal for now. A conventional vertical fader needs a
//! custom widget (same shape as [`crate::knob`]) — a later refinement.

use xilem::view::slider;
use xilem::WidgetView;

/// A horizontal gain fader spanning `[min_db, max_db]`, currently at
/// `value_db`. `on_change(state, db)` fires with the new dB value as
/// the user drags. Typical range: `min_db = -60.0`, `max_db = 6.0`.
pub fn fader<State, F>(
    min_db: f64,
    max_db: f64,
    value_db: f64,
    on_change: F,
) -> impl WidgetView<State>
where
    State: 'static,
    F: Fn(&mut State, f64) + Send + Sync + 'static,
{
    slider(min_db, max_db, value_db, move |state: &mut State, v: f64| {
        on_change(state, v);
    })
}
