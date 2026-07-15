//! The fretboard as a Sprigging paint leaf.
//!
//! CSS could not render the board well: gradient strings read as a soft glow,
//! crisp thin lines and real spacing are out of reach, and pseudo-element tricks
//! do not render. A paint leaf draws the neck with `fill_rect`, which is crisp
//! and exact — the sanctioned tool for custom visuals.
//!
//! The host registers a [`FretboardLeaf`] under [`FRETBOARD_LEAF_KEY`] in its
//! `LeafRegistry` and feeds it the board model; the view places a `custom_leaf`
//! box of [`fretboard_px_size`] for it. Note labels stay as the CSS marker grid
//! overlaid on top (text in a leaf needs font shaping, a later step); this leaf
//! draws the neck and the coloured note markers.

use sprigging::{ColorF, Leaf, PaintCx, Size, SizeHint};

/// Stable leaf key shared by the host registry and the view's `custom_leaf`.
pub const FRETBOARD_LEAF_KEY: u64 = 0x5753_4642; // "WSFB"

/// The Rehearsal screen's board is a second, independently fed leaf (it shows
/// the card under the cursor, not the live Stage lens). Distinct key so the two
/// boards never fight over one registry slot; only one is ever on screen.
pub const REHEARSAL_FRETBOARD_LEAF_KEY: u64 = 0x5753_4652; // "WSFR"

// Geometry, device px. Shared by the leaf and [`fretboard_px_size`] so the
// custom_leaf box and the painted content agree.
const PAD: f32 = 6.0;
/// Open-string column, left of the nut.
const OPEN_W: f32 = 30.0;
const FRET_W: f32 = 42.0;
const STRING_SP: f32 = 30.0;

/// One placed note on the board (label lives in the CSS overlay for now).
#[derive(Clone, Copy)]
pub struct Dot {
    pub string_index: usize,
    pub fret: u8,
    pub is_root: bool,
    /// Selected by the player on the editable (Rehearsal) board: drawn with an
    /// accent ring. Neutral on its own; `excluded` is what the mode does with it.
    pub marked: bool,
    /// Silenced by the current mark mode (Solo hides the unmarked, Mute hides
    /// the marked): painted dim so it reads as "off but still here". Always
    /// false on the read-only Stage board.
    pub excluded: bool,
}

/// How the note markers are drawn. Exposed as a setting; Sharp is the default.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MarkerStyle {
    Sharp,
    Rounded,
    Circle,
    Diamond,
}

impl MarkerStyle {
    pub fn from_name(name: &str) -> Self {
        match name {
            "Rounded" => Self::Rounded,
            "Circle" => Self::Circle,
            "Diamond" => Self::Diamond,
            _ => Self::Sharp,
        }
    }
}

/// A rounded-rectangle path (lines + quadratic corners) in the leaf's local
/// coordinates, for the Rounded marker style.
fn rounded_rect(x: f32, y: f32, w: f32, h: f32, radius: f32) -> sprigging::PathData {
    let r = radius.min(w / 2.0).min(h / 2.0);
    sprigging::Path::new()
        .move_to(x + r, y)
        .line_to(x + w - r, y)
        .quad_to(x + w, y, x + w, y + r)
        .line_to(x + w, y + h - r)
        .quad_to(x + w, y + h, x + w - r, y + h)
        .line_to(x + r, y + h)
        .quad_to(x, y + h, x, y + h - r)
        .line_to(x, y + r)
        .quad_to(x, y, x + r, y)
        .close()
        .build()
}

/// The box size (device px) for a board of this shape. The view passes this to
/// `custom_leaf`; the host builds the leaf at the same size, so they align.
pub fn fretboard_px_size(string_count: usize, fret_count: u8) -> (u32, u32) {
    let w = PAD * 2.0 + OPEN_W + fret_count as f32 * FRET_W;
    let h = PAD * 2.0 + string_count as f32 * STRING_SP;
    (w.ceil() as u32, h.ceil() as u32)
}

/// Subtle, fixed dark-theme ink (like the neighbourhood leaf; palette-derived
/// theming is a later step). Quieter and less saturated than the old CSS board.
const C_STRING: ColorF = ColorF { r: 0.50, g: 0.52, b: 0.57, a: 0.7 };
const C_WIRE: ColorF = ColorF { r: 0.30, g: 0.32, b: 0.37, a: 0.85 };
const C_NUT: ColorF = ColorF { r: 0.56, g: 0.58, b: 0.63, a: 1.0 };
const C_INLAY: ColorF = ColorF { r: 0.26, g: 0.28, b: 0.33, a: 1.0 };
const C_NOTE: ColorF = ColorF { r: 0.34, g: 0.44, b: 0.58, a: 1.0 };
const C_ROOT: ColorF = ColorF { r: 0.64, g: 0.52, b: 0.36, a: 1.0 };
/// The note sounding right now during a run/arpeggiation: a bright warm accent
/// so the step reads at a glance.
const C_ACTIVE: ColorF = ColorF { r: 0.97, g: 0.80, b: 0.44, a: 1.0 };
/// A note the current mark mode silences (Solo's unmarked, Mute's marked):
/// faint and desaturated so it reads as present-but-off, still clickable.
const C_EXCLUDED: ColorF = ColorF { r: 0.28, g: 0.30, b: 0.35, a: 0.55 };
/// The selection ring around a marked note: a cool bright accent, distinct from
/// the warm root/active tones so "selected" never reads as "sounding".
const C_MARK: ColorF = ColorF { r: 0.42, g: 0.80, b: 0.94, a: 1.0 };
/// The touch's path trail: a translucent warm line threaded through the markers
/// in visit order, so the treatment (which way the run climbs) is shown, not
/// just named. Shares the active accent's hue so the trail and the sounding step
/// read as one gesture.
const C_PATH: ColorF = ColorF { r: 0.93, g: 0.74, b: 0.42, a: 0.55 };

/// Marker box size (device px). The CSS label overlay reuses these so each label
/// sits exactly over its painted marker.
pub const MARKER_W: f32 = FRET_W * 0.7;
pub const MARKER_H: f32 = STRING_SP * 0.62;

/// y of string `i`'s centre. Shared by the leaf's paint and the label overlay.
pub fn string_center_y(i: usize) -> f32 {
    PAD + STRING_SP / 2.0 + i as f32 * STRING_SP
}

/// x of fret wire `fret` (fret 0 is the nut, at the open column's edge).
pub fn wire_center_x(fret: u8) -> f32 {
    PAD + OPEN_W + fret as f32 * FRET_W
}

/// x of a note at `fret`: centred in the open column (fret 0) or in the fret
/// space between wire fret-1 and wire fret.
pub fn note_center_x(fret: u8) -> f32 {
    if fret == 0 {
        PAD + OPEN_W / 2.0
    } else {
        PAD + OPEN_W + (fret as f32 - 0.5) * FRET_W
    }
}

const INLAY_FRETS: [u8; 8] = [3, 5, 7, 9, 15, 17, 19, 21];
const OCTAVE_FRETS: [u8; 2] = [12, 24];

/// Lower strings (smaller index, as tunings read low to high) render thicker,
/// like a wound bass string next to a plain treble string.
fn string_thickness(i: usize, n: usize) -> f32 {
    if n <= 1 {
        return 1.4;
    }
    let t = i as f32 / (n - 1) as f32; // 0 at the lowest string, 1 at the highest
    2.2 - t * 1.4 // 2.2px (low) down to 0.8px (high)
}

pub struct FretboardLeaf {
    string_count: usize,
    fret_count: u8,
    dots: Vec<Dot>,
    marker_style: MarkerStyle,
    /// The (string, fret) sounding right now during a run, painted with the
    /// active accent. Pushed live by the host each step, like the neighbourhood
    /// leaf's values.
    active: Option<(usize, u8)>,
    /// The touch's path, as an ordered visit list of `(string, fret)`. Drawn as
    /// a trail when `show_path` is on, so the treatment is visible.
    path: Vec<(usize, u8)>,
    show_path: bool,
    size: Size,
    dirty: bool,
}

impl FretboardLeaf {
    pub fn new(
        string_count: usize,
        fret_count: u8,
        dots: Vec<Dot>,
        marker_style: MarkerStyle,
    ) -> Self {
        let (w, h) = fretboard_px_size(string_count, fret_count);
        Self {
            string_count,
            fret_count,
            dots,
            marker_style,
            active: None,
            path: Vec::new(),
            show_path: false,
            size: Size {
                width: w as f32,
                height: h as f32,
            },
            dirty: true,
        }
    }

    /// Set the note sounding now (host pushes this each step). Repaints only on
    /// change so idle boards stay cached.
    pub fn set_active(&mut self, active: Option<(usize, u8)>) {
        if self.active != active {
            self.active = active;
            self.dirty = true;
        }
    }

    /// Set the touch's ordered path and whether to draw it. The host pushes both
    /// when the board or the Path toggle changes.
    pub fn set_path(&mut self, path: Vec<(usize, u8)>, show: bool) {
        if self.show_path != show || self.path != path {
            self.path = path;
            self.show_path = show;
            self.dirty = true;
        }
    }

    fn string_y(&self, i: usize) -> f32 {
        string_center_y(i)
    }

    fn wire_x(&self, fret: u8) -> f32 {
        wire_center_x(fret)
    }

    fn note_x(&self, fret: u8) -> f32 {
        note_center_x(fret)
    }
}

impl Leaf for FretboardLeaf {
    fn measure(&mut self, _known: SizeHint, _available: SizeHint) -> Size {
        self.size
    }

    fn paint(&mut self, cx: &mut PaintCx<'_>) {
        if self.string_count == 0 {
            self.dirty = false;
            return;
        }
        let top = self.string_y(0);
        let bottom = self.string_y(self.string_count - 1);
        let mid_y = (top + bottom) / 2.0;

        // Inlays: faint marks centred on the neck (single) or paired (octaves).
        let ir = 2.5;
        for &f in INLAY_FRETS.iter() {
            if f <= self.fret_count {
                let x = self.note_x(f);
                cx.fill_rect(x - ir, mid_y - ir, ir * 2.0, ir * 2.0, C_INLAY);
            }
        }
        for &f in OCTAVE_FRETS.iter() {
            if f <= self.fret_count {
                let x = self.note_x(f);
                let dy = STRING_SP;
                cx.fill_rect(x - ir, mid_y - dy - ir, ir * 2.0, ir * 2.0, C_INLAY);
                cx.fill_rect(x - ir, mid_y + dy - ir, ir * 2.0, ir * 2.0, C_INLAY);
            }
        }

        // Strings: horizontal hairlines, thickness per string.
        let x0 = PAD;
        let x1 = self.size.width - PAD;
        for i in 0..self.string_count {
            let y = self.string_y(i);
            let th = string_thickness(i, self.string_count);
            cx.fill_rect(x0, y - th / 2.0, x1 - x0, th, C_STRING);
        }

        // Fret wires: thin verticals; the nut (fret 0) is thicker and brighter.
        for f in 1..=self.fret_count {
            let x = self.wire_x(f);
            cx.fill_rect(x - 0.5, top, 1.0, bottom - top, C_WIRE);
        }
        let nx = self.wire_x(0);
        cx.fill_rect(nx - 1.5, top, 3.0, bottom - top, C_NUT);

        // Touch path trail: a translucent line threaded through the markers in
        // visit order, under the markers so they stay legible. Shows the
        // treatment (which way the run climbs) at a glance.
        if self.show_path && self.path.len() >= 2 {
            let pts: Vec<(f32, f32)> = self
                .path
                .iter()
                .filter(|(s, f)| *s < self.string_count && *f <= self.fret_count)
                .map(|&(s, f)| (self.note_x(f), self.string_y(s)))
                .collect();
            if pts.len() >= 2 {
                cx.stroke_path(
                    sprigging::Path::polyline(&pts),
                    sprigging::round_stroke(C_PATH, 2.5),
                );
            }
        }

        // Note markers: quiet rects, root warmer. Labels are drawn by the CSS
        // overlay that sits over this leaf at the same marker centres.
        let mw = MARKER_W;
        let mh = MARKER_H;
        for d in &self.dots {
            if d.string_index >= self.string_count || d.fret > self.fret_count {
                continue;
            }
            let x = self.note_x(d.fret);
            let y = self.string_y(d.string_index);
            let color = if Some((d.string_index, d.fret)) == self.active {
                C_ACTIVE
            } else if d.excluded {
                C_EXCLUDED
            } else if d.is_root {
                C_ROOT
            } else {
                C_NOTE
            };
            let (mx, my) = (x - mw / 2.0, y - mh / 2.0);
            match self.marker_style {
                MarkerStyle::Sharp => cx.fill_rect(mx, my, mw, mh, color),
                MarkerStyle::Rounded => cx.fill_path(rounded_rect(mx, my, mw, mh, 5.0), color),
                MarkerStyle::Circle => {
                    cx.fill_path(sprigging::Path::circle(x, y, mh * 0.56), color);
                }
                MarkerStyle::Diamond => {
                    let (dx, dy) = (mw * 0.46, mh * 0.66);
                    let path = sprigging::Path::new()
                        .move_to(x, y - dy)
                        .line_to(x + dx, y)
                        .line_to(x, y + dy)
                        .line_to(x - dx, y)
                        .close()
                        .build();
                    cx.fill_path(path, color);
                }
            }
            // Selection ring for a marked note, on top of the fill and tracking
            // the marker's silhouette so it reads as a halo, not a second marker.
            if d.marked {
                let ring = match self.marker_style {
                    MarkerStyle::Circle => sprigging::Path::circle(x, y, mh * 0.56 + 3.0),
                    MarkerStyle::Diamond => {
                        let (dx, dy) = (mw * 0.46 + 3.0, mh * 0.66 + 3.0);
                        sprigging::Path::new()
                            .move_to(x, y - dy)
                            .line_to(x + dx, y)
                            .line_to(x, y + dy)
                            .line_to(x - dx, y)
                            .close()
                            .build()
                    }
                    _ => rounded_rect(mx - 3.0, my - 3.0, mw + 6.0, mh + 6.0, 5.0),
                };
                cx.stroke_path(ring, sprigging::round_stroke(C_MARK, 2.0));
            }
        }

        self.dirty = false;
    }

    fn paint_dirty(&self) -> bool {
        self.dirty
    }
}
