//! Theme CSS for the serval host, derived through `tinct`.
//!
//! A theme is a few seed colours; `tinct::derive_palette` produces the full
//! semantic palette (surface ladder, text tiers, tonal triad, flags), and
//! this module renders it as the CSS sheet the views class against. Same
//! seeds as `audio-widgets::theme`'s Slate, so the serval host and the
//! xilem app agree until the parity cut.

use tinct::{color_from_hex, color_to_hex, derive_palette, Palette, Seeds};

fn hex(s: &str) -> tinct::Srgb {
    color_from_hex(s).expect("valid seed hex")
}

/// The Slate seeds (faithful cool-dark; matches audio-widgets' Slate).
pub fn slate_seeds() -> Seeds {
    Seeds {
        primary: hex("#3366c8"),
        secondary: hex("#2e9da6"),
        tertiary: hex("#e0a846"),
        neutral: hex("#101422"),
        text_header: None,
        text_body: None,
        success: hex("#4fb36e"),
        danger: hex("#d54e4e"),
        dark: true,
    }
}

/// The Stage sheet rendered from a derived palette.
pub fn stage_css(p: &Palette) -> String {
    let bg = color_to_hex(p.bg);
    let surface = color_to_hex(p.surface);
    let surface_2 = color_to_hex(p.surface_2);
    let surface_hover = color_to_hex(p.surface_hover);
    let text_header = color_to_hex(p.text_header);
    let text = color_to_hex(p.text);
    let text_dim = color_to_hex(p.text_dim);
    let text_disabled = color_to_hex(p.text_disabled);
    let primary = color_to_hex(p.primary);
    let on_primary = color_to_hex(p.on_primary);
    let secondary = color_to_hex(p.secondary);
    let on_secondary = color_to_hex(p.on_secondary);
    let tertiary = color_to_hex(p.tertiary);
    let on_tertiary = color_to_hex(p.on_tertiary);
    format!(
        r#"
.root {{ width: 100%; height: 100%; background-color: {bg}; color: {text};
        font-family: sans-serif; font-size: 14px; padding: 16px; }}
.title {{ font-size: 18px; color: {text_header}; margin-bottom: 12px; }}
.header-row {{ display: flex; margin-bottom: 12px; }}
.header-label {{ color: {text_dim}; padding: 4px 6px 4px 0; }}
.header-gap {{ width: 18px; }}
.pills {{ display: flex; margin-bottom: 12px; }}
.pill {{ padding: 6px 14px; margin-right: 6px; border-radius: 14px; color: {text_dim}; }}
.pill-active {{ background-color: {surface_2}; color: {tertiary}; }}
.lens-strip {{ display: flex; margin-bottom: 16px; }}
.lens {{ padding: 4px 12px; margin-right: 6px; border-radius: 12px; color: {text_dim};
        font-size: 13px; }}
.lens-active {{ background-color: {surface_2}; color: {tertiary}; }}
.body {{ display: flex; }}
.side {{ width: 220px; margin-right: 16px; }}
.side-item {{ padding: 5px 10px; color: {text_dim}; border-radius: 6px; }}
.side-active {{ background-color: {surface_2}; color: {text}; }}
.board {{ background-color: {surface}; border-radius: 10px; padding: 14px; }}
.string {{ display: flex; margin-bottom: 6px; }}
.fret {{ width: 46px; height: 28px; }}
.nut-gap {{ margin-right: 8px; }}
.dot {{ width: 24px; height: 24px; border-radius: 12px; background-color: {primary};
       color: {on_primary}; font-size: 10px; text-align: center; }}
.root-dot {{ background-color: {tertiary}; color: {on_tertiary}; }}
.step-dot {{ background-color: {secondary}; color: {on_secondary}; }}
.prog-cards {{ display: flex; margin-bottom: 12px; }}
.prog-card {{ background-color: {surface_2}; border-radius: 8px; padding: 6px 14px;
             margin-right: 8px; }}
.prog-card-active {{ background-color: {surface_hover}; }}
.prog-numeral {{ color: {tertiary}; font-size: 15px; }}
.prog-chord {{ color: {text_dim}; font-size: 12px; }}
.scale-name {{ margin-top: 10px; color: {text_dim}; font-size: 12px; }}
.placeholder {{ color: {text_dim}; padding: 24px; }}
.caption {{ margin-top: 12px; color: {text_disabled}; font-size: 12px; }}
.transport {{ display: flex; margin-bottom: 12px; }}
.t-btn {{ background-color: {surface_2}; color: {text}; padding: 4px 12px;
         margin-right: 6px; border-radius: 6px; }}
.t-narrow {{ padding: 4px 9px; }}
.t-readout {{ color: {text_dim}; padding: 4px 10px 4px 0; }}
.select-box {{ background-color: {surface_2}; color: {text}; padding: 4px 12px;
              border-radius: 6px; }}
.select-list {{ background-color: {surface_2}; border-radius: 6px; padding: 4px;
               width: 220px; }}
.select-option {{ color: {text}; padding: 4px 10px; border-radius: 4px; }}
"#
    )
}

/// The default sheet: Slate seeds through the derivation.
pub fn slate_stage_css() -> String {
    stage_css(&derive_palette(&slate_seeds()))
}
