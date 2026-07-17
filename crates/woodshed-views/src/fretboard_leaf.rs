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

/// Which way the neck runs. Horizontal (default) lays the frets left-to-right
/// with the strings stacked high-E-on-top; Vertical stands the neck up — nut at
/// the top, frets running down, low E on the left — the way a chord diagram
/// reads. Orientation is the *only* thing that changes which screen axis the
/// neck and the strings map to; everything else is shared.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    Horizontal,
    Vertical,
}

impl Orientation {
    pub fn from_name(name: &str) -> Self {
        match name {
            "Vertical" => Self::Vertical,
            _ => Self::Horizontal,
        }
    }
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

/// Every board position in one place, so the leaf's paint and the view's CSS
/// overlay agree — and so a single orientation flag transposes both. The neck
/// runs along one axis, the strings across the other; `to_xy` is the only place
/// orientation decides which screen axis each maps to.
#[derive(Clone, Copy)]
pub struct BoardGeom {
    pub string_count: usize,
    pub fret_start: u8,
    pub fret_count: u8,
    pub orientation: Orientation,
}

impl BoardGeom {
    /// The lowest fret drawn as a *cell*. Fret 0 is the open-string column, not a
    /// cell, so a nut-anchored window's first cell is fret 1.
    fn first_cell_fret(&self) -> f32 {
        self.fret_start.max(1) as f32
    }
    /// Width of the open-string column — present only for a nut-anchored window.
    fn open_off(&self) -> f32 {
        if self.fret_start == 0 {
            OPEN_W
        } else {
            0.0
        }
    }
    fn cell_count(&self) -> f32 {
        (self.fret_count as f32 - self.first_cell_fret() + 1.0).max(0.0)
    }
    /// Content length along the neck (no pad).
    fn neck_len(&self) -> f32 {
        self.open_off() + self.cell_count() * FRET_W
    }
    /// Content length across the strings (no pad).
    fn cross_len(&self) -> f32 {
        self.string_count as f32 * STRING_SP
    }

    pub fn in_window(&self, fret: u8) -> bool {
        fret >= self.fret_start && fret <= self.fret_count
    }

    /// Distance along the neck to a note at `fret` — the open column for fret 0,
    /// else the centre of its cell.
    fn fret_axis(&self, fret: u8) -> f32 {
        if fret == 0 && self.fret_start == 0 {
            OPEN_W / 2.0
        } else {
            self.open_off() + (fret as f32 - self.first_cell_fret() + 0.5) * FRET_W
        }
    }
    /// Distance along the neck to fret `fret`'s wire (its cell's far edge).
    fn wire_axis(&self, fret: u8) -> f32 {
        self.open_off() + (fret as f32 - self.first_cell_fret() + 1.0) * FRET_W
    }
    /// Distance across to string `i`'s centre. High E on top (horizontal) puts
    /// the low string at the far edge; low E on the left (vertical) puts it at
    /// the near edge.
    fn string_cross(&self, i: usize) -> f32 {
        let row = match self.orientation {
            Orientation::Horizontal => self.string_count.saturating_sub(1).saturating_sub(i),
            Orientation::Vertical => i,
        };
        row as f32 * STRING_SP + STRING_SP / 2.0
    }

    /// Map (along-neck, across-strings) content coords to screen (x, y),
    /// including the pad. This is the whole of what orientation does.
    fn to_xy(&self, along: f32, cross: f32) -> (f32, f32) {
        match self.orientation {
            Orientation::Horizontal => (PAD + along, PAD + cross),
            Orientation::Vertical => (PAD + cross, PAD + along),
        }
    }

    /// Centre of the marker for `(string_index, fret)`, device px.
    pub fn note_pos(&self, i: usize, fret: u8) -> (f32, f32) {
        self.to_xy(self.fret_axis(fret), self.string_cross(i))
    }

    /// The leaf's box size, device px.
    pub fn size(&self) -> (f32, f32) {
        let (along, cross) = (2.0 * PAD + self.neck_len(), 2.0 * PAD + self.cross_len());
        match self.orientation {
            Orientation::Horizontal => (along, cross),
            Orientation::Vertical => (cross, along),
        }
    }

    pub fn size_u32(&self) -> (u32, u32) {
        let (w, h) = self.size();
        (w.ceil() as u32, h.ceil() as u32)
    }

    /// Marker box (w, h): the fret-cell extent along the neck, the string spacing
    /// across. Swaps with orientation so the marker fits its cell either way.
    pub fn marker_size(&self) -> (f32, f32) {
        match self.orientation {
            Orientation::Horizontal => (MARKER_W, MARKER_H),
            Orientation::Vertical => (MARKER_H, MARKER_W),
        }
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
    /// The neck window: the board draws frets `fret_start..=fret_count`.
    fret_start: u8,
    fret_count: u8,
    orientation: Orientation,
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
        fret_start: u8,
        fret_count: u8,
        orientation: Orientation,
        dots: Vec<Dot>,
        marker_style: MarkerStyle,
    ) -> Self {
        let (w, h) = BoardGeom {
            string_count,
            fret_start,
            fret_count,
            orientation,
        }
        .size();
        Self {
            string_count,
            fret_start,
            fret_count,
            orientation,
            dots,
            marker_style,
            active: None,
            path: Vec::new(),
            show_path: false,
            size: Size {
                width: w,
                height: h,
            },
            dirty: true,
        }
    }

    fn geom(&self) -> BoardGeom {
        BoardGeom {
            string_count: self.string_count,
            fret_start: self.fret_start,
            fret_count: self.fret_count,
            orientation: self.orientation,
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
        let g = self.geom();
        let orient = self.orientation;
        // Content spans (with pad): across the strings, and along the neck.
        let cross0 = PAD;
        let cross1 = PAD + g.cross_len();
        let neck0 = PAD;
        let neck1 = PAD + g.neck_len();

        // Inlays: faint marks on the neck's centre line (single) or paired
        // (octaves), at absolute frets so a windowed board still anchors to the
        // real 5/7/12 dots. `to_xy` places them for whichever orientation.
        {
            let ir = 2.5;
            let mid = g.cross_len() / 2.0;
            let mut inlay = |along: f32, cross: f32| {
                let (x, y) = g.to_xy(along, cross);
                cx.fill_rect(x - ir, y - ir, ir * 2.0, ir * 2.0, C_INLAY);
            };
            for &f in INLAY_FRETS.iter() {
                if g.in_window(f) {
                    inlay(g.fret_axis(f), mid);
                }
            }
            for &f in OCTAVE_FRETS.iter() {
                if g.in_window(f) {
                    inlay(g.fret_axis(f), mid - STRING_SP);
                    inlay(g.fret_axis(f), mid + STRING_SP);
                }
            }
        }

        // Strings: run the length of the neck at each string's cross position,
        // thickness per string (low string thick).
        for i in 0..self.string_count {
            let c = PAD + g.string_cross(i);
            let th = string_thickness(i, self.string_count);
            match orient {
                Orientation::Horizontal => {
                    cx.fill_rect(neck0, c - th / 2.0, neck1 - neck0, th, C_STRING)
                }
                Orientation::Vertical => {
                    cx.fill_rect(c - th / 2.0, neck0, th, neck1 - neck0, C_STRING)
                }
            }
        }

        // Fret wires cross the neck at each drawn cell's far edge; the window's
        // start edge is a thick bright nut (open-anchored) or a plain boundary.
        {
            let mut cross_line = |along: f32, thick: f32, color: ColorF| {
                let a = PAD + along;
                match orient {
                    Orientation::Horizontal => {
                        cx.fill_rect(a - thick / 2.0, cross0, thick, cross1 - cross0, color)
                    }
                    Orientation::Vertical => {
                        cx.fill_rect(cross0, a - thick / 2.0, cross1 - cross0, thick, color)
                    }
                }
            };
            for f in self.fret_start.max(1)..=self.fret_count {
                cross_line(g.wire_axis(f), 1.0, C_WIRE);
            }
            let (nt, nc) = if self.fret_start == 0 {
                (3.0, C_NUT)
            } else {
                (1.0, C_WIRE)
            };
            cross_line(g.open_off(), nt, nc);
        }

        // Touch path trail: a translucent line threaded through the markers in
        // visit order, under the markers so they stay legible.
        if self.show_path && self.path.len() >= 2 {
            let pts: Vec<(f32, f32)> = self
                .path
                .iter()
                .filter(|(s, f)| *s < self.string_count && g.in_window(*f))
                .map(|&(s, f)| g.note_pos(s, f))
                .collect();
            if pts.len() >= 2 {
                cx.stroke_path(
                    sprigging::Path::polyline(&pts),
                    sprigging::round_stroke(C_PATH, 2.5),
                );
            }
        }

        // Note markers: quiet rects, root warmer. Labels are drawn by the CSS
        // overlay that sits over this leaf at the same marker centres. The marker
        // box swaps extents with orientation so it fits its cell either way.
        let (mw, mh) = g.marker_size();
        let radius = mw.min(mh) * 0.56;
        for d in &self.dots {
            if d.string_index >= self.string_count || !g.in_window(d.fret) {
                continue;
            }
            let (x, y) = g.note_pos(d.string_index, d.fret);
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
                    cx.fill_path(sprigging::Path::circle(x, y, radius), color);
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
                    MarkerStyle::Circle => sprigging::Path::circle(x, y, radius + 3.0),
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
