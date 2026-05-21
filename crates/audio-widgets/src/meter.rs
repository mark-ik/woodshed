//! Level meter — a read-only horizontal bar showing a signal level.
//!
//! Same canvas-closure pattern as [`crate::waveform_view`]: takes a
//! normalized level + colors (no product types, no live audio handle),
//! so any host drives it from whatever metering it already has
//! (Strophe's `PeakMeterStereoNode`, Woodshed's input RMS, …).
//!
//! Mapping dB → the normalized `0..=1` the bar wants is the caller's
//! job, but [`db_to_norm`] is provided for the common case.

use masonry::imaging::record::Scene;
use masonry::imaging::Painter;
use masonry::kurbo::{Rect, Size};
use masonry::peniko::Color;
use xilem::view::canvas;
use xilem::WidgetView;

/// Map a dB value to a normalized `0..=1` meter level, linear in dB
/// between `floor_db` (→ 0.0) and 0 dB (→ 1.0). Values at or below the
/// floor (incl. `-inf`) clamp to 0; values above 0 dB clamp to 1.
///
/// Linear-in-dB matches how level meters are read (every N dB is the
/// same screen distance), unlike a linear-amplitude bar that crushes
/// everything quiet into the bottom sliver.
pub fn db_to_norm(db: f32, floor_db: f32) -> f32 {
    if !db.is_finite() || db <= floor_db {
        return 0.0;
    }
    ((db - floor_db) / (0.0 - floor_db)).clamp(0.0, 1.0)
}

/// A Xilem view that paints a horizontal level meter.
///
/// `level` is normalized `0..=1` (already mapped from dB by the
/// caller — see [`db_to_norm`]); it's clamped defensively. The track
/// (unfilled background) fills the whole widget; the level fills from
/// the left in `fill_color`.
pub fn meter_view<State: 'static>(
    level: f32,
    fill_color: Color,
    track_color: Color,
) -> impl WidgetView<State> {
    let level = level.clamp(0.0, 1.0);
    canvas(move |_state: &mut State, _ctx, scene: &mut Scene, size: Size| {
        let mut painter = Painter::new(scene);
        let w = size.width;
        let h = size.height;
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        // Track first, then the filled portion on top.
        painter
            .fill(&Rect::new(0.0, 0.0, w, h), track_color)
            .draw();
        let fill_w = w * level as f64;
        if fill_w > 0.0 {
            painter
                .fill(&Rect::new(0.0, 0.0, fill_w, h), fill_color)
                .draw();
        }
    })
    .alt_text("Level meter")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db_to_norm_floor_and_ceiling() {
        assert_eq!(db_to_norm(f32::NEG_INFINITY, -60.0), 0.0);
        assert_eq!(db_to_norm(-60.0, -60.0), 0.0);
        assert_eq!(db_to_norm(-90.0, -60.0), 0.0);
        assert_eq!(db_to_norm(0.0, -60.0), 1.0);
        assert_eq!(db_to_norm(6.0, -60.0), 1.0); // clamps above unity
    }

    #[test]
    fn db_to_norm_midpoint_is_linear_in_db() {
        // Halfway in dB between -60 and 0 is -30 dB → 0.5.
        assert!((db_to_norm(-30.0, -60.0) - 0.5).abs() < 1e-6);
    }
}
