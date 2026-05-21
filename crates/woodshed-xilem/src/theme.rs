// Copyright 2026 the Woodshed Authors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Woodshed theme — a thin product layer over the shared
//! [`audio_widgets::theme`] base.
//!
//! Spacing (`SP_*`), type scale (`TS_*`), [`mono_family`], the
//! [`ThemeMode`] enum, and the base [`Palette`] semantic tokens
//! (surfaces, text hierarchy, the primary/secondary/tertiary triad,
//! success/danger) live in the shared `audio-widgets` crate so Strophe
//! and Mere share them verbatim. Woodshed re-exports those and extends
//! the palette with its **fretboard-diagram colors**, which are
//! product-specific and don't belong in the shared base.
//!
//! View code still refers to named tokens (`palette.surface`, `SP_2`,
//! `TS_SM`) exactly as before — the `palette.*` API stays flat.

pub use audio_widgets::theme::{
    SP_0, SP_1, SP_2, SP_3, SP_4, SP_5, SP_6, SP_8, TS_2XL, TS_LG, TS_MD, TS_SM, TS_XL, TS_XS,
    ThemeMode, mono_family,
};

use audio_widgets::theme::Palette as BasePalette;
use masonry::peniko::Color;

// =================================================================
// Palette — the shared base tokens, extended with Woodshed's
// fretboard-diagram colors. The base 15 fields mirror
// `audio_widgets::theme::Palette` (and are copied from it in
// `dark()`/`light()`, so the shared crate is the single source of
// truth for those values); the 7 diagram fields are Woodshed's own.
// =================================================================

#[allow(dead_code)]
#[derive(Copy, Clone, Debug)]
pub struct Palette {
    // --- Shared base (mirrors `audio_widgets::theme::Palette`) ---
    pub bg: Color,
    pub surface: Color,
    pub surface_2: Color,
    pub surface_hover: Color,
    pub text_header: Color,
    pub text: Color,
    pub text_dim: Color,
    pub text_disabled: Color,
    pub primary: Color,
    pub on_primary: Color,
    pub secondary: Color,
    pub on_secondary: Color,
    pub tertiary: Color,
    pub on_tertiary: Color,
    pub success: Color,
    pub danger: Color,

    // --- Woodshed-specific: fretboard diagram colors ---
    // `note_dot` follows `primary`, `root_dot` follows `tertiary` by
    // convention so the root pops against the chord tones; themes
    // override either when a different mapping reads better.
    pub fret_line: Color,
    pub string_line: Color,
    pub inlay: Color,
    pub root_dot: Color,
    pub note_dot: Color,
    pub dot_label: Color,
    pub dot_outline: Color,
}

impl Default for Palette {
    fn default() -> Self {
        Self::dark()
    }
}

impl Palette {
    /// Default dark theme — shared base + Woodshed's dark diagram hues.
    pub const fn dark() -> Self {
        let b = BasePalette::dark();
        Self {
            bg: b.bg,
            surface: b.surface,
            surface_2: b.surface_2,
            surface_hover: b.surface_hover,
            text_header: b.text_header,
            text: b.text,
            text_dim: b.text_dim,
            text_disabled: b.text_disabled,
            primary: b.primary,
            on_primary: b.on_primary,
            secondary: b.secondary,
            on_secondary: b.on_secondary,
            tertiary: b.tertiary,
            on_tertiary: b.on_tertiary,
            success: b.success,
            danger: b.danger,
            // Fretboard (dark): light dots on dark text. `root_dot`
            // tracks tertiary (warm amber) so it pops against the
            // blue note dots.
            fret_line: Color::from_rgb8(0x80, 0x80, 0x80),
            string_line: Color::from_rgb8(0xB0, 0xB0, 0xB0),
            inlay: Color::from_rgb8(0x4D, 0x4D, 0x4D),
            root_dot: Color::from_rgb8(0xE0, 0xA8, 0x46),
            note_dot: Color::from_rgb8(0x77, 0xAA, 0xDD),
            dot_label: Color::from_rgb8(0x14, 0x14, 0x1A),
            dot_outline: Color::from_rgb8(0x10, 0x10, 0x14),
        }
    }

    /// Light theme — shared base + Woodshed's light diagram hues
    /// (dark dots, light text; lines darkened so the lattice reads on
    /// white).
    pub const fn light() -> Self {
        let b = BasePalette::light();
        Self {
            bg: b.bg,
            surface: b.surface,
            surface_2: b.surface_2,
            surface_hover: b.surface_hover,
            text_header: b.text_header,
            text: b.text,
            text_dim: b.text_dim,
            text_disabled: b.text_disabled,
            primary: b.primary,
            on_primary: b.on_primary,
            secondary: b.secondary,
            on_secondary: b.on_secondary,
            tertiary: b.tertiary,
            on_tertiary: b.on_tertiary,
            success: b.success,
            danger: b.danger,
            fret_line: Color::from_rgb8(0x70, 0x70, 0x76),
            string_line: Color::from_rgb8(0x4C, 0x4C, 0x54),
            inlay: Color::from_rgb8(0xC0, 0xC0, 0xC8),
            root_dot: Color::from_rgb8(0xA8, 0x6C, 0x14),
            note_dot: Color::from_rgb8(0x2A, 0x55, 0xB4),
            dot_label: Color::from_rgb8(0xF4, 0xF4, 0xF8),
            dot_outline: Color::from_rgb8(0x14, 0x14, 0x1A),
        }
    }
}

/// The palette a [`ThemeMode`] names: the shared derived base plus
/// Woodshed's fretboard-diagram colors, themselves derived from the
/// base (note dots ← primary, root dots ← tertiary, label ← contrast
/// pick, lines ← neutral steps). Free function rather than an inherent
/// method because `ThemeMode` is the shared (foreign) type.
///
/// Per the "derived + overrides" model, any diagram color that reads
/// wrong for a given theme can be special-cased here later; for now
/// all built-ins derive cleanly.
pub fn palette_for(mode: ThemeMode) -> Palette {
    palette_from_seeds(&mode.seeds())
}

/// Core derivation: shared base from `seeds` + Woodshed's derived
/// fretboard-diagram colors. Used by both built-in themes
/// ([`palette_for`]) and user themes (which carry raw [`Seeds`]).
pub fn palette_from_seeds(seeds: &audio_widgets::theme::Seeds) -> Palette {
    let b = audio_widgets::theme::derive_palette(seeds);
    Palette {
        bg: b.bg,
        surface: b.surface,
        surface_2: b.surface_2,
        surface_hover: b.surface_hover,
        text_header: b.text_header,
        text: b.text,
        text_dim: b.text_dim,
        text_disabled: b.text_disabled,
        primary: b.primary,
        on_primary: b.on_primary,
        secondary: b.secondary,
        on_secondary: b.on_secondary,
        tertiary: b.tertiary,
        on_tertiary: b.on_tertiary,
        success: b.success,
        danger: b.danger,
        // Fretboard diagram — derived from the base.
        fret_line: b.text_disabled,
        string_line: b.text_dim,
        inlay: b.surface_2,
        root_dot: b.tertiary,
        note_dot: b.primary,
        dot_label: audio_widgets::theme::best_on(b.primary),
        dot_outline: b.bg,
    }
}
