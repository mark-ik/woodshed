//! Shared Masonry/Vello audio widgets.
//!
//! Product-agnostic canvas widgets consumed across the Strophos audio
//! family (Woodshed, Strophe, Mere) — the first `audio-widgets`
//! extraction from the pressure-vessel doctrine. Widgets take plain
//! sample data + colors, never any product (`woodshed::` / `strophe::`)
//! types, so any host can drive them.
//!
//! Each widget is a thin wrapper around Xilem's `canvas` view: a paint
//! closure receives a Vello `Scene` + `Size`, wrapped in a `Painter`
//! for kurbo/peniko drawing.
//!
//! Ships [`waveform_view`] + [`compute_peaks`] (extracted from
//! Strophe's `strophe-widgets` at its FT5) and [`meter::meter_view`]
//! (level bar). Future widgets (fader, knob, transport button, mini
//! routing graph) follow the same canvas-closure pattern.
//!
//! This is the *audio-domain* widget layer (Woodshed + Strophe). The
//! domain-neutral components (combobox, …) usable by Mere too live one
//! layer down in `xilem-components`.

pub mod fader;
pub mod knob;
pub mod meter;
pub mod theme;

pub use fader::fader;
pub use knob::knob;
pub use meter::{db_to_norm, meter_view};

use masonry::imaging::record::Scene;
use masonry::imaging::Painter;
use masonry::kurbo::{BezPath, Size, Stroke};
use masonry::peniko::Color;
use xilem::view::canvas;
use xilem::WidgetView;

/// A min/max peak pair for one horizontal column of a waveform, in
/// the normalized sample range `[-1.0, 1.0]`.
pub type Peak = (f32, f32);

/// Downsample an audio buffer into `columns` (min, max) peak pairs —
/// a single-LOD peak file. Each column summarizes the
/// `samples.len() / columns` samples that map to it.
///
/// Returns an empty vec for empty input or zero columns. Multi-LOD
/// tiers (for zoomable waveforms) are a later refinement; this ships
/// the single-tier version.
pub fn compute_peaks(samples: &[f32], columns: usize) -> Vec<Peak> {
    if samples.is_empty() || columns == 0 {
        return Vec::new();
    }
    let len = samples.len();
    let columns = columns.min(len); // never more columns than samples
    let per_col = len as f64 / columns as f64;

    (0..columns)
        .map(|c| {
            let start = (c as f64 * per_col) as usize;
            let end = (((c + 1) as f64 * per_col) as usize).clamp(start + 1, len);
            let slice = &samples[start..end];
            let mut min = 0.0_f32;
            let mut max = 0.0_f32;
            for &s in slice {
                min = min.min(s);
                max = max.max(s);
            }
            (min, max)
        })
        .collect()
}

/// A Xilem view that paints a waveform from precomputed peaks.
///
/// The peaks are drawn as a filled envelope (top edge = per-column
/// max, bottom edge = per-column min), centered vertically. A faint
/// zero-line is stroked across the middle so an empty/quiet waveform
/// still reads as "a track that exists."
///
/// Pass an empty `peaks` vec to render just the zero-line (an empty
/// slot).
pub fn waveform_view<State: 'static>(
    peaks: Vec<Peak>,
    wave_color: Color,
    zero_line_color: Color,
) -> impl WidgetView<State> {
    canvas(move |_state: &mut State, _ctx, scene: &mut Scene, size: Size| {
        let mut painter = Painter::new(scene);
        draw_waveform(&mut painter, size, &peaks, wave_color, zero_line_color);
    })
    .alt_text("Audio waveform")
}

fn draw_waveform(
    painter: &mut Painter<'_, Scene>,
    size: Size,
    peaks: &[Peak],
    wave_color: Color,
    zero_line_color: Color,
) {
    let w = size.width;
    let h = size.height;
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    let mid = h / 2.0;

    // Zero-line first, so the waveform fill sits on top of it.
    let mut zero = BezPath::new();
    zero.move_to((0.0, mid));
    zero.line_to((w, mid));
    painter
        .stroke(&zero, &Stroke::new(1.0), zero_line_color)
        .draw();

    let n = peaks.len();
    if n == 0 {
        return;
    }

    // Column x-position. Guard the single-column case (avoid /0).
    let x_at = |i: usize| -> f64 {
        if n == 1 {
            w / 2.0
        } else {
            i as f64 / (n - 1) as f64 * w
        }
    };

    // Filled envelope: top edge (max) left→right, then bottom edge
    // (min) right→left, closed. Vello fills this antialiased.
    let mut path = BezPath::new();
    for (i, &(_min, max)) in peaks.iter().enumerate() {
        let x = x_at(i);
        let y = mid - (max as f64).clamp(-1.0, 1.0) * mid;
        if i == 0 {
            path.move_to((x, y));
        } else {
            path.line_to((x, y));
        }
    }
    for (i, &(min, _max)) in peaks.iter().enumerate().rev() {
        let x = x_at(i);
        let y = mid - (min as f64).clamp(-1.0, 1.0) * mid;
        path.line_to((x, y));
    }
    path.close_path();
    painter.fill(&path, wave_color).draw();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_peaks_empty_input() {
        assert!(compute_peaks(&[], 10).is_empty());
        assert!(compute_peaks(&[0.1, 0.2], 0).is_empty());
    }

    #[test]
    fn compute_peaks_column_count_capped_to_samples() {
        let peaks = compute_peaks(&[0.5, -0.5], 10);
        assert_eq!(peaks.len(), 2); // never more columns than samples
    }

    #[test]
    fn compute_peaks_captures_min_and_max() {
        // One column over the whole buffer: min should be the lowest,
        // max the highest.
        let samples = vec![0.0, 0.8, -0.6, 0.2];
        let peaks = compute_peaks(&samples, 1);
        assert_eq!(peaks.len(), 1);
        let (min, max) = peaks[0];
        assert!((max - 0.8).abs() < 1e-6, "max = {max}");
        assert!((min - -0.6).abs() < 1e-6, "min = {min}");
    }

    #[test]
    fn compute_peaks_splits_columns() {
        // 4 samples into 2 columns: [0.8, 0.0] | [0.0, -0.6]
        let samples = vec![0.8, 0.0, 0.0, -0.6];
        let peaks = compute_peaks(&samples, 2);
        assert_eq!(peaks.len(), 2);
        assert!((peaks[0].1 - 0.8).abs() < 1e-6);
        assert!((peaks[1].0 - -0.6).abs() < 1e-6);
    }
}
