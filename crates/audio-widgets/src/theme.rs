//! Shared theme primitives — spacing, type scale, mono font, base palette.
//!
//! Product-agnostic visual tokens for the Strophos audio family
//! (Woodshed, Strophe, Mere). The spacing rhythm, type scale, and
//! mono-font helper are *identical* across apps — copying them per-app
//! would be genuine duplication, so they live here. The [`Palette`]
//! base carries the universally-shared semantic colors (surfaces, text
//! hierarchy, a Material-You-style accent triad, success/danger).
//!
//! Product-specific colors (Woodshed's fretboard diagram colors,
//! Strophe's waveform/track colors) are NOT here — each host layers
//! those on top, reading the shared base for everything else.
//!
//! Conventions:
//!
//!   - **Spacing** is a strict 4-pixel base unit. Use the `SP_*`
//!     consts; a non-multiple is a signal to question the layout.
//!   - **Type scale** is a modular scale at ~1.2× per step, rounded
//!     to integer px.
//!   - **Mono font** ([`mono_family`]) for any changing numeric
//!     readout so it doesn't jitter horizontally as digit widths vary.

use masonry::layout::Length;
use masonry::parley::{FontFamily, FontFamilyName, GenericFamily};
use masonry::peniko::Color;

// =================================================================
// Spacing — 4-pixel base unit. `SP_N` == N * 4 px.
// =================================================================

pub const SP_0: Length = Length::const_px(0.0);
pub const SP_1: Length = Length::const_px(4.0);
pub const SP_2: Length = Length::const_px(8.0);
pub const SP_3: Length = Length::const_px(12.0);
pub const SP_4: Length = Length::const_px(16.0);
pub const SP_5: Length = Length::const_px(20.0);
pub const SP_6: Length = Length::const_px(24.0);
pub const SP_8: Length = Length::const_px(32.0);

// =================================================================
// Type scale — modular, ~1.2× ratio rounded to integer px. Passed
// by value because the view crate's `text_size` API is numeric.
// =================================================================

/// 11px — caption, hint text, secondary metadata.
pub const TS_XS: f32 = 11.0;
/// 13px — body text, control labels.
pub const TS_SM: f32 = 13.0;
/// 15px — emphasized body, sub-section headings.
pub const TS_MD: f32 = 15.0;
/// 20px — section headings, panel titles.
pub const TS_LG: f32 = 20.0;
/// 28px — tab-level or window-level titles.
pub const TS_XL: f32 = 28.0;
/// 48px — display readouts.
pub const TS_2XL: f32 = 48.0;

// =================================================================
// Mono font helper. Apply via `.font(mono_family())` to any text
// whose contents change at runtime and are numeric (BPM, level,
// bar phase, timestamps) so the readout doesn't jitter.
// =================================================================

pub fn mono_family() -> FontFamily<'static> {
    FontFamily::Single(FontFamilyName::Generic(GenericFamily::Monospace))
}

// =================================================================
// Palette — shared semantic color tokens.
//
// View code reads through these names, never raw `Color::...`. A
// host that needs a product-specific color (waveform fill, fretboard
// line) keeps that in its own struct and reads this base for the
// rest.
// =================================================================

#[derive(Copy, Clone, Debug)]
pub struct Palette {
    // Surfaces — layered backgrounds. `bg` is the window itself;
    // `surface` is one elevation up (cards); `surface_2` is two
    // (controls on cards); `surface_hover` is the hover state.
    pub bg: Color,
    pub surface: Color,
    pub surface_2: Color,
    pub surface_hover: Color,

    // Text hierarchy. `text_header` colors titles / section headings /
    // tab labels (the big type tiers); `text` is body + control text;
    // `text_dim` is secondary metadata; `text_disabled` the inactive
    // ghost. Header + body are independently seedable (optional
    // overrides); dim/disabled derive from body.
    pub text_header: Color,
    pub text: Color,
    pub text_dim: Color,
    pub text_disabled: Color,

    // Material-You-style tonal triad. `primary` anchors brand
    // identity (active states, current selection, primary actions);
    // `secondary` is a sympathetic support hue; `tertiary` is the
    // contrasting emphasis hue ("you are here" markers). Each `on_*`
    // is the text/icon color to render on top of a fill in that hue.
    pub primary: Color,
    pub on_primary: Color,
    pub secondary: Color,
    pub on_secondary: Color,
    pub tertiary: Color,
    pub on_tertiary: Color,

    // Semantic flags — functional colors kept distinct from brand.
    pub success: Color,
    pub danger: Color,
}

impl Default for Palette {
    fn default() -> Self {
        Self::dark()
    }
}

/// A built-in theme. Persisted (by name) through a host's settings and
/// toggled from its theme picker. Each variant is a curated [`Seeds`]
/// set; the palette is *derived* (see [`derive_palette`]).
///
/// **Serde is additive**: variants may be added but never renamed or
/// removed, or existing saved configs would fail to round-trip. (A
/// future user-themes layer will move selection to a string id; see
/// the theme-system design doc.)
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ThemeMode {
    #[default]
    Dark,
    Light,
    Dusk,
    Meadow,
    Parchment,
}

impl ThemeMode {
    /// All built-ins, in picker order.
    pub const ALL: [Self; 5] = [
        Self::Dark,
        Self::Light,
        Self::Dusk,
        Self::Meadow,
        Self::Parchment,
    ];

    /// The seeds this theme is authored from.
    pub fn seeds(self) -> Seeds {
        let rgb = Color::from_rgb8;
        match self {
            // Cool blue / teal / amber over a faintly blue-cool ladder.
            Self::Dark => Seeds {
                primary: rgb(0x33, 0x66, 0xC8),
                secondary: rgb(0x2E, 0x9D, 0xA6),
                tertiary: rgb(0xE0, 0xA8, 0x46),
                neutral: rgb(0x10, 0x14, 0x22),
                text_header: None,
                text_body: None,
                success: rgb(0x4F, 0xB3, 0x6E),
                danger: rgb(0xD5, 0x4E, 0x4E),
                dark: true,
            },
            Self::Light => Seeds {
                primary: rgb(0x2A, 0x55, 0xB4),
                secondary: rgb(0x1F, 0x77, 0x7F),
                tertiary: rgb(0xA8, 0x6C, 0x14),
                neutral: rgb(0xDF, 0xE3, 0xEE),
                text_header: None,
                text_body: None,
                success: rgb(0x2F, 0x8A, 0x4F),
                danger: rgb(0xB8, 0x33, 0x33),
                dark: false,
            },
            // Warm dark — terracotta / gold / violet over warm-brown panels.
            Self::Dusk => Seeds {
                primary: rgb(0xD0, 0x6A, 0x4E),
                secondary: rgb(0xD9, 0xA4, 0x41),
                tertiary: rgb(0x9B, 0x6F, 0xD0),
                neutral: rgb(0x26, 0x18, 0x12),
                text_header: None,
                text_body: None,
                success: rgb(0x6F, 0xB3, 0x6E),
                danger: rgb(0xD5, 0x55, 0x4E),
                dark: true,
            },
            // Green dark — moss / teal / wheat over green-dark panels.
            Self::Meadow => Seeds {
                primary: rgb(0x5B, 0xA8, 0x6B),
                secondary: rgb(0x3F, 0xA8, 0x9E),
                tertiary: rgb(0xE0, 0xB8, 0x4A),
                neutral: rgb(0x10, 0x1E, 0x14),
                text_header: None,
                text_body: None,
                success: rgb(0x6F, 0xC3, 0x70),
                danger: rgb(0xCF, 0x51, 0x51),
                dark: true,
            },
            // Warm light — sepia / sage / ochre on warm cream paper.
            Self::Parchment => Seeds {
                primary: rgb(0x8A, 0x5A, 0x2B),
                secondary: rgb(0x4E, 0x7C, 0x6A),
                tertiary: rgb(0xA8, 0x73, 0x1A),
                neutral: rgb(0xF0, 0xE6, 0xCE),
                text_header: None,
                text_body: None,
                success: rgb(0x2F, 0x8A, 0x4F),
                danger: rgb(0xB8, 0x33, 0x33),
                dark: false,
            },
        }
    }

    /// The derived palette for this theme.
    pub fn palette(self) -> Palette {
        derive_palette(&self.seeds())
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Dark => "Dark",
            Self::Light => "Light",
            Self::Dusk => "Dusk",
            Self::Meadow => "Meadow",
            Self::Parchment => "Parchment",
        }
    }
}

impl Palette {
    /// Default dark theme. Cool-blue `primary` ↔ teal `secondary` ↔
    /// warm-amber `tertiary`, over a layered deep-neutral surface
    /// ladder. (Shared base; lifted from Woodshed's proven values.)
    pub const fn dark() -> Self {
        Self {
            bg: Color::from_rgb8(0x08, 0x08, 0x0B),
            surface: Color::from_rgb8(0x12, 0x12, 0x16),
            surface_2: Color::from_rgb8(0x1A, 0x1A, 0x20),
            surface_hover: Color::from_rgb8(0x24, 0x24, 0x2C),
            text_header: Color::from_rgb8(0xEC, 0xEC, 0xF0),
            text: Color::from_rgb8(0xEC, 0xEC, 0xF0),
            text_dim: Color::from_rgb8(0x9A, 0x9A, 0xA4),
            text_disabled: Color::from_rgb8(0x55, 0x55, 0x5E),
            primary: Color::from_rgb8(0x33, 0x66, 0xC8),
            on_primary: Color::from_rgb8(0xF4, 0xF4, 0xF8),
            secondary: Color::from_rgb8(0x2E, 0x9D, 0xA6),
            on_secondary: Color::from_rgb8(0x08, 0x18, 0x1A),
            tertiary: Color::from_rgb8(0xE0, 0xA8, 0x46),
            on_tertiary: Color::from_rgb8(0x1A, 0x14, 0x08),
            success: Color::from_rgb8(0x4F, 0xB3, 0x6E),
            danger: Color::from_rgb8(0xD5, 0x4E, 0x4E),
        }
    }

    /// Light theme — same triad shifted for visibility on light
    /// surfaces.
    pub const fn light() -> Self {
        Self {
            bg: Color::from_rgb8(0xDE, 0xDE, 0xE2),
            surface: Color::from_rgb8(0xEC, 0xEC, 0xF0),
            surface_2: Color::from_rgb8(0xC4, 0xC4, 0xCC),
            surface_hover: Color::from_rgb8(0xD4, 0xD4, 0xDC),
            text_header: Color::from_rgb8(0x1E, 0x1E, 0x24),
            text: Color::from_rgb8(0x1E, 0x1E, 0x24),
            text_dim: Color::from_rgb8(0x4E, 0x4E, 0x58),
            text_disabled: Color::from_rgb8(0x80, 0x80, 0x8A),
            primary: Color::from_rgb8(0x2A, 0x55, 0xB4),
            on_primary: Color::from_rgb8(0xFF, 0xFF, 0xFF),
            secondary: Color::from_rgb8(0x1F, 0x77, 0x7F),
            on_secondary: Color::from_rgb8(0xFF, 0xFF, 0xFF),
            tertiary: Color::from_rgb8(0xA8, 0x6C, 0x14),
            on_tertiary: Color::from_rgb8(0xFF, 0xFF, 0xFF),
            success: Color::from_rgb8(0x2F, 0x8A, 0x4F),
            danger: Color::from_rgb8(0xB8, 0x33, 0x33),
        }
    }
}

// =================================================================
// Seed-derived palettes.
//
// A theme is authored as a handful of seed hues + a light/dark mode;
// the full role set is *derived* (surface ladder, text hierarchy,
// `on_*` contrast picks) so adding a palette is cheap and consistent.
// Derivation runs in OKLCH for perceptually-even lightness steps.
// Callers may still hand-override individual roles on top (the
// "derived + overrides" model) — `derive_palette` is the derived base.
//
// See design_docs/2026-05-20_theme_system_design.md.
// =================================================================

/// The minimal authored input for a derived palette.
#[derive(Copy, Clone, Debug)]
pub struct Seeds {
    /// Brand triad — hue anchors. `primary`/`secondary`/`tertiary` are
    /// used as-is for those roles; surfaces + text derive from
    /// `neutral`.
    pub primary: Color,
    pub secondary: Color,
    pub tertiary: Color,
    /// Surface/text hue. Usually near-grey with a slight temperature;
    /// the surface ladder derives from this, and text defaults to a
    /// lightness step off it.
    pub neutral: Color,
    /// Optional explicit heading / body text colors. `None` (the
    /// default) derives them from `neutral` (legible); `Some` overrides
    /// — used as-is, the user's call. `text_body` also drives the
    /// derived dim/disabled tiers.
    pub text_header: Option<Color>,
    pub text_body: Option<Color>,
    /// Functional hues, kept distinct from the brand triad.
    pub success: Color,
    pub danger: Color,
    /// Run the ladders dark (low surface L, high text L) or light.
    pub dark: bool,
}

/// Derive the full base [`Palette`] from [`Seeds`]. Surfaces + text are
/// perceptual lightness steps off `neutral`; `on_*` are contrast-picked
/// against each brand fill.
pub fn derive_palette(s: &Seeds) -> Palette {
    let n = oklch::Oklch::from_color(s.neutral);
    // Perceptual L targets for the surface ladder + text hierarchy,
    // per mode. Dark: dark surfaces, light text. Light: light
    // surfaces, dark text. `surface_2` sits a touch *darker* than
    // `surface` in light mode (inset look) — matches the hand-tuned
    // light theme.
    let (bg_l, surf_l, surf2_l, hover_l, text_l, dim_l, dis_l) = if s.dark {
        (0.16, 0.21, 0.26, 0.32, 0.95, 0.72, 0.46)
    } else {
        (0.93, 0.97, 0.86, 0.90, 0.24, 0.42, 0.60)
    };
    let _ = (dim_l, dis_l); // dim/disabled now derive from body, below
    let step = |l: f64| oklch::to_color(&n.with_l(l));
    let surface = step(surf_l);
    // Body text: explicit override, else the neutral-derived step.
    // Header defaults to body. Dim/disabled blend body → surface so
    // they track a body override.
    let body = s.text_body.unwrap_or_else(|| step(text_l));
    let header = s.text_header.unwrap_or(body);
    let (dim_t, dis_t) = (0.32, 0.60);
    Palette {
        bg: step(bg_l),
        surface,
        surface_2: step(surf2_l),
        surface_hover: step(hover_l),
        text_header: header,
        text: body,
        text_dim: mix(body, surface, dim_t),
        text_disabled: mix(body, surface, dis_t),
        primary: s.primary,
        on_primary: best_on(s.primary),
        secondary: s.secondary,
        on_secondary: best_on(s.secondary),
        tertiary: s.tertiary,
        on_tertiary: best_on(s.tertiary),
        success: s.success,
        danger: s.danger,
    }
}

/// Format a color as `#RRGGBB` (alpha dropped — seeds are opaque).
pub fn color_to_hex(c: Color) -> String {
    let [r, g, b, _] = c.to_rgba8().to_u8_array();
    format!("#{r:02X}{g:02X}{b:02X}")
}

/// Parse `#RRGGBB` / `RRGGBB` (case-insensitive) to an opaque color.
/// Returns `None` for anything that isn't six hex digits.
pub fn color_from_hex(s: &str) -> Option<Color> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some(Color::from_rgba8(r, g, b, 0xFF))
}

/// Near-white or near-black, whichever has the higher WCAG contrast
/// against `bg`. Used for `on_*` text/icon colors over brand fills.
pub fn best_on(bg: Color) -> Color {
    let white = Color::from_rgb8(0xF4, 0xF4, 0xF8);
    let black = Color::from_rgb8(0x14, 0x14, 0x1A);
    if contrast(white, bg) >= contrast(black, bg) {
        white
    } else {
        black
    }
}

fn relative_luminance(col: Color) -> f64 {
    let [r, g, b, _] = col.to_rgba8().to_u8_array();
    let lin = |c: u8| {
        let c = c as f64 / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
}

/// Straight-RGB lerp `a`→`b` by `t` (0..1). Crude but fine for blending
/// text toward a surface for the dim/disabled tiers, or tinting a
/// surface toward an accent hue.
pub fn mix(a: Color, b: Color, t: f64) -> Color {
    let [ar, ag, ab, _] = a.to_rgba8().to_u8_array();
    let [br, bg, bb, _] = b.to_rgba8().to_u8_array();
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| ((x as f64) * (1.0 - t) + (y as f64) * t).round() as u8;
    Color::from_rgba8(m(ar, br), m(ag, bg), m(ab, bb), 0xFF)
}

/// WCAG contrast ratio between two colors (1.0 … 21.0).
fn contrast(a: Color, b: Color) -> f64 {
    let la = relative_luminance(a);
    let lb = relative_luminance(b);
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// Minimal OKLCH ↔ sRGB transform (Björn Ottosson's oklab), hand-rolled
/// to avoid a color-crate dependency. `h` is in radians.
mod oklch {
    use masonry::peniko::Color;

    pub struct Oklch {
        pub l: f64,
        pub c: f64,
        pub h: f64,
    }

    fn srgb_to_linear(c: f64) -> f64 {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }

    fn linear_to_srgb(c: f64) -> f64 {
        if c <= 0.0031308 {
            12.92 * c
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        }
    }

    impl Oklch {
        pub fn from_color(col: Color) -> Self {
            let [r8, g8, b8, _] = col.to_rgba8().to_u8_array();
            let r = srgb_to_linear(r8 as f64 / 255.0);
            let g = srgb_to_linear(g8 as f64 / 255.0);
            let b = srgb_to_linear(b8 as f64 / 255.0);
            let l = 0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b;
            let m = 0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b;
            let s = 0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b;
            let l_ = l.cbrt();
            let m_ = m.cbrt();
            let s_ = s.cbrt();
            let ll = 0.2104542553 * l_ + 0.7936177850 * m_ - 0.0040720468 * s_;
            let aa = 1.9779984951 * l_ - 2.4285922050 * m_ + 0.4505937099 * s_;
            let bb = 0.0259040371 * l_ + 0.7827717662 * m_ - 0.8086757660 * s_;
            Self {
                l: ll,
                c: (aa * aa + bb * bb).sqrt(),
                h: bb.atan2(aa),
            }
        }

        /// Same hue + chroma, new lightness.
        pub fn with_l(&self, l: f64) -> Self {
            Self {
                l,
                c: self.c,
                h: self.h,
            }
        }
    }

    pub fn to_color(o: &Oklch) -> Color {
        let a = o.c * o.h.cos();
        let b = o.c * o.h.sin();
        let l_ = o.l + 0.3963377774 * a + 0.2158037573 * b;
        let m_ = o.l - 0.1055613458 * a - 0.0638541728 * b;
        let s_ = o.l - 0.0894841775 * a - 1.2914855480 * b;
        let l = l_ * l_ * l_;
        let m = m_ * m_ * m_;
        let s = s_ * s_ * s_;
        let r = 4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s;
        let g = -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s;
        let bb = -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s;
        let to8 = |x: f64| (linear_to_srgb(x).clamp(0.0, 1.0) * 255.0).round() as u8;
        Color::from_rgba8(to8(r), to8(g), to8(bb), 0xFF)
    }
}

#[cfg(test)]
mod theme_tests {
    use super::*;

    fn lum(c: Color) -> f64 {
        relative_luminance(c)
    }

    #[test]
    fn oklch_roundtrips() {
        for c in [
            Color::from_rgb8(0x33, 0x66, 0xC8),
            Color::from_rgb8(0x12, 0x12, 0x16),
            Color::from_rgb8(0xEC, 0xEC, 0xF0),
        ] {
            let back = oklch::to_color(&oklch::Oklch::from_color(c));
            let [r0, g0, b0, _] = c.to_rgba8().to_u8_array();
            let [r1, g1, b1, _] = back.to_rgba8().to_u8_array();
            // Within a couple of 8-bit steps (gamma rounding).
            assert!((r0 as i32 - r1 as i32).abs() <= 2, "r {r0} vs {r1}");
            assert!((g0 as i32 - g1 as i32).abs() <= 2, "g {g0} vs {g1}");
            assert!((b0 as i32 - b1 as i32).abs() <= 2, "b {b0} vs {b1}");
        }
    }

    #[test]
    fn best_on_picks_readable() {
        // Dark fill → light text; light fill → dark text.
        let dark = Color::from_rgb8(0x2A, 0x55, 0xB4);
        let light = Color::from_rgb8(0xE0, 0xA8, 0x46);
        assert!(contrast(best_on(dark), dark) >= 3.0);
        assert!(contrast(best_on(light), light) >= 3.0);
    }

    #[test]
    fn derived_dark_has_ordered_ladder_and_readable_text() {
        let seeds = Seeds {
            primary: Color::from_rgb8(0x33, 0x66, 0xC8),
            secondary: Color::from_rgb8(0x2E, 0x9D, 0xA6),
            tertiary: Color::from_rgb8(0xE0, 0xA8, 0x46),
            neutral: Color::from_rgb8(0x18, 0x18, 0x1E),
            text_header: None,
            text_body: None,
            success: Color::from_rgb8(0x4F, 0xB3, 0x6E),
            danger: Color::from_rgb8(0xD5, 0x4E, 0x4E),
            dark: true,
        };
        let p = derive_palette(&seeds);
        // Surface ladder ascends in luminance; text is far brighter.
        assert!(lum(p.bg) < lum(p.surface));
        assert!(lum(p.surface) < lum(p.surface_2));
        assert!(contrast(p.text, p.surface) >= 4.5, "text on surface");
    }

    #[test]
    fn derived_light_inverts() {
        let seeds = Seeds {
            primary: Color::from_rgb8(0x2A, 0x55, 0xB4),
            secondary: Color::from_rgb8(0x1F, 0x77, 0x7F),
            tertiary: Color::from_rgb8(0xA8, 0x6C, 0x14),
            neutral: Color::from_rgb8(0xE6, 0xE6, 0xEA),
            text_header: None,
            text_body: None,
            success: Color::from_rgb8(0x2F, 0x8A, 0x4F),
            danger: Color::from_rgb8(0xB8, 0x33, 0x33),
            dark: false,
        };
        let p = derive_palette(&seeds);
        // Light mode: surfaces bright, text dark, still readable.
        assert!(lum(p.surface) > 0.6);
        assert!(lum(p.text) < 0.2);
        assert!(contrast(p.text, p.surface) >= 4.5, "text on surface");
    }
}
