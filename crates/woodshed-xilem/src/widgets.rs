// Copyright 2026 the Woodshed Authors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Custom widgets for the Xilem build.
//!
//! Each widget here is a thin wrapper around Xilem's `canvas` view —
//! we paint into a Vello `Scene` with the same geometry math we use
//! in the iced build, just translated from `canvas::Frame` →
//! `Painter`. The pure-Rust theory + audio crates supply data
//! unchanged.

use masonry::core::render_text;
use masonry::imaging::{Painter, record::Scene};
use masonry::kurbo::{Affine, BezPath, Circle, Point, Size, Stroke};
use masonry::parley::{
    Alignment, AlignmentOptions, FontFamily, FontFamilyName, GenericFamily, StyleProperty,
};
use masonry::peniko::Color;
use xilem::WidgetView;
use xilem::view::canvas;

use woodshedding::fretboard::{Fretboard, Position};
use woodshedding::interval::Interval;

/// Color subset used by fretboard / chord-diagram canvases. Built
/// from a [`crate::theme::Palette`] via [`DiagramColors::from_palette`]
/// — view code should never construct this directly; theme changes
/// flow through `Palette` and DiagramColors is a projection of it.
#[derive(Copy, Clone, Debug)]
pub struct DiagramColors {
    pub root_dot: Color,
    pub note_dot: Color,
    pub label_text: Color,
    pub fret: Color,
    pub string: Color,
    pub inlay: Color,
    /// Outline drawn around every position dot. A subtle stroke that
    /// makes dots pop off the strings/frets without competing with
    /// the dot fill colors. Set to a fully-transparent color to skip
    /// outline rendering.
    pub dot_outline: Color,
}

impl DiagramColors {
    /// Project a full [`Palette`] down to the diagram-color subset.
    /// Keeps the diagram colors in sync with the active theme without
    /// the parallel-config burden the previous `classic()` constant
    /// carried.
    pub fn from_palette(palette: &crate::theme::Palette) -> Self {
        Self {
            root_dot: palette.root_dot,
            note_dot: palette.note_dot,
            label_text: palette.dot_label,
            fret: palette.fret_line,
            string: palette.string_line,
            inlay: palette.inlay,
            dot_outline: palette.dot_outline,
        }
    }
}

/// Construct a Xilem view that renders a fretboard with highlighted
/// note positions and optional labels.
///
/// Layout: **vertical** fretboard. Nut at the top, frets descending
/// downward; low string on the left, high string on the right.
/// Matches the orientation of a typical chord-diagram book — natural
/// for guitarists reading along.
///
/// `dot_colors` is an optional per-position color override, indexed
/// parallel to `positions`. When `None`, dot color is decided by the
/// position's `interval_from_root` (root vs. other-tone). When
/// `Some`, each position's color is taken from the override —
/// suitable for trail fades, animated highlights, etc.
/// Per-string marker drawn above the nut: an **open** ring or a
/// **muted** ✕. `None` draws nothing. Used by the chord-card form of
/// the fretboard (a tight window) to show how each string is played.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum StringMark {
    None,
    Open,
    Muted,
}

/// `fret_window` is `(start_fret, fret_count)` — the visible neck span.
/// `(0, 12)` is the full neck; a tight window like `(5, 4)` is the
/// chord-card form (four frets from the 5th, with a "5fr" position
/// label and no nut). This is the fretboard's *scope dial*: shorten it
/// toward a chord card or open it to the whole neck.
pub fn fretboard_view<State>(
    fretboard: Fretboard,
    positions: Vec<Position>,
    labels: Vec<String>,
    colors: DiagramColors,
    dot_colors: Option<Vec<Color>>,
    fret_window: (u8, u8),
    marks: Vec<StringMark>,
) -> impl WidgetView<State>
where
    State: 'static,
{
    canvas(move |_state: &mut State, ctx, scene: &mut Scene, size: Size| {
        let mut painter = Painter::new(scene);
        draw_fretboard(
            &mut painter,
            ctx,
            size,
            &fretboard,
            &positions,
            &labels,
            colors,
            dot_colors.as_deref(),
            fret_window.0 as usize,
            (fret_window.1 as usize).max(1),
            &marks,
        );
    })
    .alt_text("Vertical fretboard diagram showing highlighted note positions.")
}

/// The drawing kernel. Pulled out so future widgets (ChordDiagram,
/// position-window cropped views) can reuse the geometry.
#[allow(clippy::too_many_arguments)]
fn draw_fretboard(
    painter: &mut Painter<'_, Scene>,
    ctx: &mut masonry::core::MutateCtx<'_>,
    size: Size,
    fretboard: &Fretboard,
    positions: &[Position],
    labels: &[String],
    colors: DiagramColors,
    dot_colors: Option<&[Color]>,
    start_fret: usize,
    fret_count: usize,
    marks: &[StringMark],
) {
    let n_strings = fretboard.tuning.strings.len();
    // `n_frets` is the number of *visible* fret rows (the window),
    // capped to what the model actually has. The window covers
    // absolute frets `start_fret + 1 ..= start_fret + n_frets`, with
    // the boundary line above the first row being the nut iff the
    // window starts at 0.
    let n_frets = fret_count.min(fretboard.fret_count as usize).max(1);
    let starts_at_nut = start_fret == 0;
    if n_strings < 2 {
        return;
    }

    // Margins: enough top room for an "open string" marker zone
    // (positions where pos.fret == 0 sit on the nut line itself);
    // enough horizontal room so dot labels at edge strings don't
    // get clipped.
    let margin_x = 24.0_f64;
    let margin_y = 30.0_f64;
    let avail_w = (size.width - 2.0 * margin_x).max(50.0);
    let avail_h = (size.height - 2.0 * margin_y).max(50.0);

    // In the vertical layout: strings span width, frets span height.
    // The board scales as a *single proportional unit* (cells stay
    // ~square-ish, fret rows a touch taller than string spacing, like a
    // real neck) and fits within the pane on both axes — so it never
    // looks squashed (flat wide cells) or stretched. A max cap keeps a
    // big pane from yielding giant cells; the board is then centered in
    // the leftover space. Text labels are drawn at a fixed size
    // (`draw_position_label`), so they stay readable as the board
    // scales down.
    const CELL_ASPECT: f64 = 1.15; // fret_h / string_w
    const MAX_STRING_W: f64 = 56.0;
    let cols = (n_strings - 1) as f64;
    let rows = n_frets as f64;
    // Largest string spacing that fits both axes at the target aspect.
    let string_w = (avail_w / cols)
        .min(avail_h / (rows * CELL_ASPECT))
        .min(MAX_STRING_W)
        .max(8.0);
    let fret_h = string_w * CELL_ASPECT;

    let board_w = cols * string_w;
    let board_h = rows * fret_h;
    // Center on both axes in the leftover pane space (the top offset
    // also leaves room for the open/muted markers above the nut).
    let board_left = margin_x + (avail_w - board_w).max(0.0) / 2.0;
    let board_top = margin_y + (avail_h - board_h).max(0.0) / 2.0;
    let board_right = board_left + board_w;
    let board_bottom = board_top + board_h;

    // Strings — vertical lines. Index 0 (low string) on the left.
    for i in 0..n_strings {
        let x = board_left + i as f64 * string_w;
        let mut path = BezPath::new();
        path.move_to(Point::new(x, board_top));
        path.line_to(Point::new(x, board_bottom));
        painter
            .stroke(&path, &Stroke::new(1.5), colors.string)
            .draw();
    }

    // Frets — horizontal lines. The top line is the nut (drawn
    // thicker) only when the window starts at the open position.
    for f in 0..=n_frets {
        let y = board_top + f as f64 * fret_h;
        let width = if f == 0 && starts_at_nut { 4.0 } else { 1.0 };
        let mut path = BezPath::new();
        path.move_to(Point::new(board_left, y));
        path.line_to(Point::new(board_right, y));
        painter
            .stroke(&path, &Stroke::new(width), colors.fret)
            .draw();
    }

    // Open / muted string markers above the nut (chord-card form).
    // Open = small ring, muted = ✕. Only strings with a mark draw.
    for i in 0..n_strings {
        let mark = marks.get(i).copied().unwrap_or(StringMark::None);
        if mark == StringMark::None {
            continue;
        }
        let x = board_left + i as f64 * string_w;
        let my = board_top - 10.0;
        match mark {
            StringMark::Open => {
                painter
                    .stroke(
                        Circle::new(Point::new(x, my), 4.0),
                        &Stroke::new(1.4),
                        colors.fret,
                    )
                    .draw();
            }
            StringMark::Muted => {
                let mut p = BezPath::new();
                p.move_to(Point::new(x - 4.0, my - 4.0));
                p.line_to(Point::new(x + 4.0, my + 4.0));
                p.move_to(Point::new(x - 4.0, my + 4.0));
                p.line_to(Point::new(x + 4.0, my - 4.0));
                painter.stroke(&p, &Stroke::new(1.6), colors.fret).draw();
            }
            StringMark::None => {}
        }
    }

    // Single-dot inlays at the fretboard center, mapped from absolute
    // fret to the visible window (`local = fret - start_fret`).
    let mid_x = board_left + (n_strings - 1) as f64 / 2.0 * string_w;
    let in_window = |fret: usize| fret > start_fret && fret <= start_fret + n_frets;
    for &fret in &[3usize, 5, 7, 9, 15, 17, 19, 21] {
        if !in_window(fret) {
            continue;
        }
        let local = fret - start_fret;
        let y = board_top + (local as f64 - 0.5) * fret_h;
        painter
            .fill(Circle::new(Point::new(mid_x, y), 4.0), colors.inlay)
            .draw();
    }
    // Double inlays at 12 and 24, offset to either side of center.
    for &fret in &[12usize, 24] {
        if !in_window(fret) {
            continue;
        }
        let local = fret - start_fret;
        let y = board_top + (local as f64 - 0.5) * fret_h;
        for offset in [-1.0_f64, 1.0] {
            let x = mid_x + offset * string_w;
            painter
                .fill(Circle::new(Point::new(x, y), 4.0), colors.inlay)
                .draw();
        }
    }

    // Position markers — drawn last so they sit on top of strings
    // and inlays. Open-string notes (pos.fret == 0) sit on the nut,
    // and only show when the window includes the open position.
    // Fretted notes outside the window are skipped.
    for (i, pos) in positions.iter().enumerate() {
        let fret = pos.fret as usize;
        let y = if fret == 0 {
            if !starts_at_nut {
                continue;
            }
            board_top
        } else {
            let local = fret as i64 - start_fret as i64;
            if local < 1 || local as usize > n_frets {
                continue;
            }
            board_top + (local as f64 - 0.5) * fret_h
        };
        let x = board_left + pos.string_index as f64 * string_w;

        let dot_color = match dot_colors.and_then(|c| c.get(i).copied()) {
            Some(c) => c,
            None => {
                let is_root = pos.interval_from_root == Some(Interval::PERFECT_UNISON);
                if is_root {
                    colors.root_dot
                } else {
                    colors.note_dot
                }
            }
        };

        // Cap radius to half-string-spacing so dots can't overlap
        // on narrow widths. With a 12-fret display, fret_h is
        // generous (≈50px at 660px height), so the upper bound is
        // tuned to a chord-diagram-book sized dot.
        let dot_radius = (string_w * 0.42).min(fret_h * 0.4).clamp(10.0, 18.0);
        let circle = Circle::new(Point::new(x, y), dot_radius);
        painter.fill(circle, dot_color).draw();
        painter
            .stroke(circle, &Stroke::new(1.2), colors.dot_outline)
            .draw();

        if let Some(label) = labels.get(i).filter(|s| !s.is_empty()) {
            draw_position_label(painter, ctx, label, x, y, colors.label_text);
        }
    }

    // Fret-position label (e.g. "5fr") below the board when the window
    // is anchored above the nut, so a cropped span isn't ambiguous.
    if !starts_at_nut {
        let text = format!("{}fr", start_fret + 1);
        let (fcx, lcx) = ctx.text_contexts();
        let mut builder = lcx.ranged_builder(fcx, &text, 1.0, true);
        builder.push_default(StyleProperty::FontFamily(FontFamily::Single(
            FontFamilyName::Generic(GenericFamily::SansSerif),
        )));
        builder.push_default(StyleProperty::FontSize(11.0));
        let mut layout = builder.build(&text);
        layout.break_all_lines(None);
        layout.align(None, Alignment::Start, AlignmentOptions::default());
        let transform = Affine::translate((board_left, board_bottom + 4.0));
        render_text(painter, transform, &layout, &[colors.label_text.into()], true);
    }
}


/// Render a centered single-line label inside a fretboard dot.
/// Builds a Parley text layout, measures it, then positions it so
/// the layout's center sits at `(cx, cy)`.
fn draw_position_label(
    painter: &mut Painter<'_, Scene>,
    ctx: &mut masonry::core::MutateCtx<'_>,
    text: &str,
    cx: f64,
    cy: f64,
    color: Color,
) {
    let (fcx, lcx) = ctx.text_contexts();
    let mut builder = lcx.ranged_builder(fcx, text, 1.0, true);
    builder.push_default(StyleProperty::FontFamily(FontFamily::Single(
        FontFamilyName::Generic(GenericFamily::SansSerif),
    )));
    builder.push_default(StyleProperty::FontSize(13.0));
    let mut layout = builder.build(text);
    layout.break_all_lines(None);
    layout.align(None, Alignment::Center, AlignmentOptions::default());

    let w = layout.width() as f64;
    let h = layout.height() as f64;
    let transform = Affine::translate((cx - w * 0.5, cy - h * 0.5));
    render_text(painter, transform, &layout, &[color.into()], true);
}

// =================================================================
// Tuner meter widgets — cents needle + input level bar.
//
// Both replace the utility `progress_bar` stand-ins on the Tuner
// tab with audio-app aesthetics: tick marks, center-line reference,
// in-tune highlight zone, color-zoned fills, and a clear "no signal"
// state via `Option<f64>` inputs.
// =================================================================

/// Color palette projection for the meter widgets. Same pattern as
/// `DiagramColors::from_palette` — keeps the canvas widget code
/// palette-agnostic while the call site supplies theme-aware values.
#[derive(Copy, Clone, Debug)]
pub struct MeterColors {
    /// Track background — the unfilled portion.
    pub track: Color,
    /// Tick marks + center reference line.
    pub tick: Color,
    /// In-tune / safe zone fill (green-ish).
    pub safe: Color,
    /// Caution zone fill (amber).
    pub caution: Color,
    /// Clip / danger zone fill (red).
    pub danger: Color,
    /// Needle / indicator stroke.
    pub indicator: Color,
}

impl MeterColors {
    /// Project a [`crate::theme::Palette`] down to the meter color
    /// subset. Functional colors (safe / caution / danger) come from
    /// the palette's semantic flags; track + tick lean on the surface
    /// ladder so meters look at home next to cards.
    pub fn from_palette(palette: &crate::theme::Palette) -> Self {
        Self {
            track: palette.surface_2,
            tick: palette.text_dim,
            safe: palette.success,
            caution: palette.tertiary,
            danger: palette.danger,
            indicator: palette.text,
        }
    }
}

/// A cents-offset needle meter. Shows how far the detected pitch is
/// from the target — center is "in tune," left is flat, right is
/// sharp.
///
/// `cents` is the offset, clamped internally to ±50. `None` renders
/// track + ticks but no needle (the "no signal" state). `in_tune`
/// shades the center-zone background green when the detector reports
/// we're within tolerance; otherwise the zone is desaturated so it
/// still reads as "the target" without claiming success.
pub fn cents_meter_view<State>(
    cents: Option<f64>,
    in_tune: bool,
    colors: MeterColors,
) -> impl WidgetView<State>
where
    State: 'static,
{
    canvas(move |_state: &mut State, _ctx, scene: &mut Scene, size: Size| {
        let mut painter = Painter::new(scene);
        draw_cents_meter(&mut painter, size, cents, in_tune, colors);
    })
    .alt_text("Cents-offset tuner meter. Center is in tune; left flat, right sharp.")
}

fn draw_cents_meter(
    painter: &mut Painter<'_, Scene>,
    size: Size,
    cents: Option<f64>,
    in_tune: bool,
    colors: MeterColors,
) {
    let margin_x = 12.0_f64;
    let avail_w = (size.width - 2.0 * margin_x).max(40.0);
    let center_y = size.height * 0.5;
    let track_h = (size.height * 0.45).min(18.0);
    let track_top = center_y - track_h * 0.5;
    let track_bottom = center_y + track_h * 0.5;
    let left_x = margin_x;
    let right_x = margin_x + avail_w;

    // Track — full bar in track color, rounded so it doesn't look
    // CRT-utilitarian.
    let track_rect = masonry::kurbo::Rect::new(left_x, track_top, right_x, track_bottom);
    painter
        .fill(track_rect.to_rounded_rect(track_h * 0.5), colors.track)
        .draw();

    // In-tune zone — center ±5 cents highlighted. Width fraction
    // (5 / 50) = 0.1 of avail; centered on `center_x`.
    let center_x = (left_x + right_x) * 0.5;
    let zone_half_w = avail_w * (5.0 / 100.0);
    let zone_fill = if in_tune {
        colors.safe
    } else {
        // Desaturated safe color when not actively in tune — reads
        // as "the target zone" without claiming success. Mix toward
        // track at 50% so the desaturation is uniform across themes.
        mix_color(colors.safe, colors.track, 0.5)
    };
    let zone_rect = masonry::kurbo::Rect::new(
        center_x - zone_half_w,
        track_top,
        center_x + zone_half_w,
        track_bottom,
    );
    painter
        .fill(zone_rect.to_rounded_rect(track_h * 0.5), zone_fill)
        .draw();

    // Tick marks at -50, -25, 0, +25, +50.
    let tick_h = track_h * 1.3;
    let tick_top = center_y - tick_h * 0.5;
    let tick_bottom = center_y + tick_h * 0.5;
    for &cents_pos in &[-50.0_f64, -25.0, 0.0, 25.0, 50.0] {
        let t = (cents_pos + 50.0) / 100.0;
        let x = left_x + t * avail_w;
        let mut path = BezPath::new();
        path.move_to(Point::new(x, tick_top));
        path.line_to(Point::new(x, tick_bottom));
        // Center tick is thicker as the visual anchor.
        let width = if cents_pos == 0.0 { 2.0 } else { 1.0 };
        painter
            .stroke(&path, &Stroke::new(width), colors.tick)
            .draw();
    }

    // Needle — vertical line at the current cents value.
    if let Some(cents) = cents {
        let clamped = cents.clamp(-50.0, 50.0);
        let t = (clamped + 50.0) / 100.0;
        let x = left_x + t * avail_w;
        let needle_top = center_y - tick_h * 0.7;
        let needle_bottom = center_y + tick_h * 0.7;
        let mut path = BezPath::new();
        path.move_to(Point::new(x, needle_top));
        path.line_to(Point::new(x, needle_bottom));
        painter
            .stroke(&path, &Stroke::new(2.5), colors.indicator)
            .draw();
    }
}

/// A signal-level meter showing input loudness (RMS, linear 0..1).
///
/// Fill color is picked by which zone the tip lives in — safe up to
/// ~0.7, caution up to ~0.9, then danger. Tick marks at the zone
/// boundaries make the zones legible without a legend. Useful for
/// spotting when input is sub-threshold or about to clip.
pub fn level_meter_view<State>(
    level: Option<f64>,
    colors: MeterColors,
) -> impl WidgetView<State>
where
    State: 'static,
{
    canvas(move |_state: &mut State, _ctx, scene: &mut Scene, size: Size| {
        let mut painter = Painter::new(scene);
        draw_level_meter(&mut painter, size, level, colors);
    })
    .alt_text("Input-level meter (linear 0..1, color-zoned).")
}

fn draw_level_meter(
    painter: &mut Painter<'_, Scene>,
    size: Size,
    level: Option<f64>,
    colors: MeterColors,
) {
    let margin_x = 12.0_f64;
    let avail_w = (size.width - 2.0 * margin_x).max(40.0);
    let center_y = size.height * 0.5;
    let track_h = (size.height * 0.55).min(14.0);
    let track_top = center_y - track_h * 0.5;
    let track_bottom = center_y + track_h * 0.5;
    let left_x = margin_x;
    let right_x = margin_x + avail_w;

    // Track.
    let track_rect = masonry::kurbo::Rect::new(left_x, track_top, right_x, track_bottom);
    painter
        .fill(track_rect.to_rounded_rect(track_h * 0.4), colors.track)
        .draw();

    // Fill — proportional to level, color by zone-where-tip-lives.
    // Single-color fill (no gradient across zones) so the current
    // zone reads instantly without the user having to interpret a
    // gradient transition.
    if let Some(level) = level {
        let l = level.clamp(0.0, 1.0);
        if l > 0.0 {
            let fill_w = l * avail_w;
            let fill_color = if l >= 0.9 {
                colors.danger
            } else if l >= 0.7 {
                colors.caution
            } else {
                colors.safe
            };
            let fill_rect = masonry::kurbo::Rect::new(
                left_x,
                track_top,
                left_x + fill_w,
                track_bottom,
            );
            painter
                .fill(fill_rect.to_rounded_rect(track_h * 0.4), fill_color)
                .draw();
        }
    }

    // Tick marks at zone boundaries (0.7, 0.9) so the zones read
    // even before the fill enters them.
    let tick_h = track_h * 1.35;
    let tick_top = center_y - tick_h * 0.5;
    let tick_bottom = center_y + tick_h * 0.5;
    for &boundary in &[0.7_f64, 0.9] {
        let x = left_x + boundary * avail_w;
        let mut path = BezPath::new();
        path.move_to(Point::new(x, tick_top));
        path.line_to(Point::new(x, tick_bottom));
        painter
            .stroke(&path, &Stroke::new(1.0), colors.tick)
            .draw();
    }
}

// =================================================================
// Song timeline — section / marker lane.
//
// The top lane of the bar-quantized Song timeline: colored bands
// spanning bar ranges, each labeled with its section name (Intro,
// Verse, Chorus). Pure structure — no audio. A band runs from a bar
// that starts a section until the next section-start (or song end).
// =================================================================

/// One contiguous section in the timeline. `start_bar` is the
/// zero-based index of the bar that opens the section; `len` is how
/// many bars it spans (always ≥ 1). `label` is the section name.
#[derive(Clone, Debug)]
pub struct SectionBand {
    pub start_bar: usize,
    pub len: usize,
    pub label: String,
}

/// Color projection for the section lane. Bands alternate between two
/// tints so adjacent sections stay visually distinct; `track` fills
/// any leading run of bars before the first labeled section.
#[derive(Copy, Clone, Debug)]
pub struct SectionColors {
    /// Fill for bars not yet assigned to a section (before the first
    /// section-start).
    pub track: Color,
    /// Primary band tint.
    pub band: Color,
    /// Alternating band tint, for adjacency contrast.
    pub band_alt: Color,
    /// Section label text.
    pub label_text: Color,
    /// Hairline between bands + lane outline.
    pub border: Color,
    /// Sweeping playhead line ("you are here").
    pub playhead: Color,
}

impl SectionColors {
    /// Project a [`crate::theme::Palette`]. Bands are the brand
    /// hues mixed well toward the surface so the section name reads
    /// in `text` on top of them across both themes.
    pub fn from_palette(palette: &crate::theme::Palette) -> Self {
        Self {
            track: palette.surface_2,
            band: mix_color(palette.primary, palette.surface, 0.6),
            band_alt: mix_color(palette.secondary, palette.surface, 0.6),
            label_text: palette.text,
            border: palette.surface_2,
            playhead: palette.tertiary,
        }
    }
}

/// Draw the sweeping playhead as a vertical line at `frac` (0..1) of
/// the lane width. Shared by the section + chord lanes so the line
/// reads as one continuous sweep across the stacked lanes.
fn draw_playhead(painter: &mut Painter<'_, Scene>, size: Size, frac: f64, color: Color) {
    let x = frac.clamp(0.0, 1.0) * size.width;
    let mut path = BezPath::new();
    path.move_to(Point::new(x, 0.0));
    path.line_to(Point::new(x, size.height));
    painter.stroke(&path, &Stroke::new(2.0), color).draw();
}

/// A horizontal section lane. `bands` are laid out left-to-right,
/// each sized proportional to its `len` against `total_bars`. Any
/// bars not covered by a band (a leading unlabeled run) render in the
/// track color. `total_bars` must be ≥ the span the bands cover.
pub fn section_lane_view<State>(
    bands: Vec<SectionBand>,
    total_bars: usize,
    playhead: Option<f64>,
    colors: SectionColors,
) -> impl WidgetView<State>
where
    State: 'static,
{
    canvas(move |_state: &mut State, ctx, scene: &mut Scene, size: Size| {
        let mut painter = Painter::new(scene);
        draw_section_lane(&mut painter, ctx, size, &bands, total_bars, playhead, colors);
    })
    .alt_text("Song section lane: labeled bands spanning bar ranges.")
}

#[allow(clippy::too_many_arguments)]
fn draw_section_lane(
    painter: &mut Painter<'_, Scene>,
    ctx: &mut masonry::core::MutateCtx<'_>,
    size: Size,
    bands: &[SectionBand],
    total_bars: usize,
    playhead: Option<f64>,
    colors: SectionColors,
) {
    let total = total_bars.max(1) as f64;
    let pad = 2.0_f64;
    let top = pad;
    let bottom = (size.height - pad).max(top + 1.0);
    let bar_w = size.width / total;

    // Track behind everything — covers any leading unlabeled run and
    // keeps the lane reading as one strip even with gaps.
    let track_rect = masonry::kurbo::Rect::new(0.0, top, size.width, bottom);
    painter
        .fill(track_rect.to_rounded_rect(4.0), colors.track)
        .draw();

    for (i, band) in bands.iter().enumerate() {
        if band.len == 0 {
            continue;
        }
        let left = band.start_bar as f64 * bar_w;
        let right = (band.start_bar + band.len) as f64 * bar_w;
        let fill = if i % 2 == 0 { colors.band } else { colors.band_alt };
        let rect = masonry::kurbo::Rect::new(left + 1.0, top, (right - 1.0).max(left + 1.0), bottom);
        painter.fill(rect.to_rounded_rect(4.0), fill).draw();
        painter
            .stroke(rect.to_rounded_rect(4.0), &Stroke::new(1.0), colors.border)
            .draw();

        if !band.label.is_empty() {
            let cx = (left + right) * 0.5;
            let cy = (top + bottom) * 0.5;
            draw_position_label(painter, ctx, &band.label, cx, cy, colors.label_text);
        }
    }

    if let Some(frac) = playhead {
        draw_playhead(painter, size, frac, colors.playhead);
    }
}

/// A chord lane: one cell per bar, laid out left-to-right and aligned
/// with the section lane above it. `labels[i]` is bar `i`'s chord name
/// (empty = no chord, rendered as a dim track cell). `selected` gets a
/// thicker accent outline; `cursor` (the playhead bar) gets a filled
/// accent so "you are here" reads at a glance.
pub fn chord_lane_view<State>(
    labels: Vec<String>,
    selected: Option<usize>,
    cursor: Option<usize>,
    playhead: Option<f64>,
    colors: SectionColors,
) -> impl WidgetView<State>
where
    State: 'static,
{
    canvas(move |_state: &mut State, ctx, scene: &mut Scene, size: Size| {
        let mut painter = Painter::new(scene);
        draw_chord_lane(&mut painter, ctx, size, &labels, selected, cursor, playhead, colors);
    })
    .alt_text("Song chord lane: one chord cell per bar.")
}

#[allow(clippy::too_many_arguments)]
fn draw_chord_lane(
    painter: &mut Painter<'_, Scene>,
    ctx: &mut masonry::core::MutateCtx<'_>,
    size: Size,
    labels: &[String],
    selected: Option<usize>,
    cursor: Option<usize>,
    playhead: Option<f64>,
    colors: SectionColors,
) {
    let n = labels.len().max(1);
    let pad = 2.0_f64;
    let top = pad;
    let bottom = (size.height - pad).max(top + 1.0);
    let cell_w = size.width / n as f64;

    for (i, lbl) in labels.iter().enumerate() {
        let left = i as f64 * cell_w;
        let right = (i + 1) as f64 * cell_w;
        let has_chord = !lbl.is_empty();
        let is_cursor = cursor == Some(i);
        let fill = if is_cursor {
            colors.band_alt
        } else if has_chord {
            colors.band
        } else {
            colors.track
        };
        let rect = masonry::kurbo::Rect::new(
            left + 1.0,
            top,
            (right - 1.0).max(left + 1.0),
            bottom,
        );
        painter.fill(rect.to_rounded_rect(4.0), fill).draw();
        let outline_w = if selected == Some(i) { 2.5 } else { 1.0 };
        painter
            .stroke(rect.to_rounded_rect(4.0), &Stroke::new(outline_w), colors.border)
            .draw();

        let text = if has_chord { lbl.as_str() } else { "—" };
        let cx = (left + right) * 0.5;
        let cy = (top + bottom) * 0.5;
        draw_position_label(painter, ctx, text, cx, cy, colors.label_text);
    }

    if let Some(frac) = playhead {
        draw_playhead(painter, size, frac, colors.playhead);
    }
}

/// Linear interpolation between two colors in straight RGB. Crude
/// (no gamma correction) but enough for desaturating the in-tune
/// zone when the detector isn't actively reporting "in tune."
fn mix_color(a: Color, b: Color, t: f64) -> Color {
    let [ar, ag, ab, _aa] = a.to_rgba8().to_u8_array();
    let [br, bg, bb, _ba] = b.to_rgba8().to_u8_array();
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| ((x as f64) * (1.0 - t) + (y as f64) * t).round() as u8;
    Color::from_rgba8(mix(ar, br), mix(ag, bg), mix(ab, bb), 0xFF)
}
