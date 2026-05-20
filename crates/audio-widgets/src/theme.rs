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

    // Text hierarchy. `text` is primary content; `text_dim` is
    // secondary metadata; `text_disabled` is the inactive ghost.
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

/// Which palette is active. Persistable + toggleable by a host's
/// settings UI.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ThemeMode {
    #[default]
    Dark,
    Light,
}

impl ThemeMode {
    pub const fn palette(self) -> Palette {
        match self {
            Self::Dark => Palette::dark(),
            Self::Light => Palette::light(),
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Dark => "Dark",
            Self::Light => "Light",
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
