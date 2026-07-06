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

/// The built-in themes (seed sets match audio-widgets' engine, so the
/// serval host and the xilem app agree until the parity cut).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ThemeMode {
    #[default]
    Slate,
    Ember,
    Light,
    Dusk,
    Meadow,
    Parchment,
}

impl ThemeMode {
    pub const ALL: [ThemeMode; 6] = [
        ThemeMode::Slate,
        ThemeMode::Ember,
        ThemeMode::Light,
        ThemeMode::Dusk,
        ThemeMode::Meadow,
        ThemeMode::Parchment,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ThemeMode::Slate => "Slate",
            ThemeMode::Ember => "Ember",
            ThemeMode::Light => "Light",
            ThemeMode::Dusk => "Dusk",
            ThemeMode::Meadow => "Meadow",
            ThemeMode::Parchment => "Parchment",
        }
    }

    /// Inverse of [`label`](Self::label), for the persisted theme name.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|m| m.label() == name)
    }

    pub fn seeds(self) -> Seeds {
        match self {
            // Slate — faithful cool-dark: blue / teal / amber.
            ThemeMode::Slate => Seeds {
                primary: hex("#3366c8"),
                secondary: hex("#2e9da6"),
                tertiary: hex("#e0a846"),
                neutral: hex("#101422"),
                text_header: None,
                text_body: None,
                success: hex("#4fb36e"),
                danger: hex("#d54e4e"),
                dark: true,
            },
            // Ember — warm fire: ember orange-red, gold roots, warm
            // charcoal-brown surfaces.
            ThemeMode::Ember => Seeds {
                primary: hex("#da5e3a"),
                secondary: hex("#c27a3c"),
                tertiary: hex("#ebb046"),
                neutral: hex("#24170f"),
                text_header: None,
                text_body: None,
                success: hex("#6fb36e"),
                danger: hex("#d5554e"),
                dark: true,
            },
            ThemeMode::Light => Seeds {
                primary: hex("#2a55b4"),
                secondary: hex("#1f777f"),
                tertiary: hex("#a86c14"),
                neutral: hex("#dfe3ee"),
                text_header: None,
                text_body: None,
                success: hex("#2f8a4f"),
                danger: hex("#b83333"),
                dark: false,
            },
            // Dusk — cool twilight: mauve / periwinkle / violet.
            ThemeMode::Dusk => Seeds {
                primary: hex("#cc6f8c"),
                secondary: hex("#7c71c4"),
                tertiary: hex("#b28adc"),
                neutral: hex("#191628"),
                text_header: None,
                text_body: None,
                success: hex("#6fb36e"),
                danger: hex("#d5554e"),
                dark: true,
            },
            // Meadow — moss / teal / wheat over green-dark panels.
            ThemeMode::Meadow => Seeds {
                primary: hex("#5ba86b"),
                secondary: hex("#3fa89e"),
                tertiary: hex("#e0b84a"),
                neutral: hex("#101e14"),
                text_header: None,
                text_body: None,
                success: hex("#6fc370"),
                danger: hex("#cf5151"),
                dark: true,
            },
            // Parchment — sepia / sage / ochre on warm cream paper.
            ThemeMode::Parchment => Seeds {
                primary: hex("#8a5a2b"),
                secondary: hex("#4e7c6a"),
                tertiary: hex("#a8731a"),
                neutral: hex("#f0e6ce"),
                text_header: None,
                text_body: None,
                success: hex("#2f8a4f"),
                danger: hex("#b83333"),
                dark: false,
            },
        }
    }

    /// The full Stage sheet for this theme.
    pub fn css(self) -> String {
        stage_css(&derive_palette(&self.seeds()))
    }
}

/// The Slate seeds (kept for callers that want the default explicitly).
pub fn slate_seeds() -> Seeds {
    ThemeMode::Slate.seeds()
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
.trail-dot {{ background-color: {surface_hover}; color: {text_dim}; }}
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
.settings-heading {{ color: {text_header}; font-size: 15px; margin-bottom: 8px; }}
.settings-line {{ color: {text_dim}; margin-bottom: 6px; }}
.filmstrip {{ display: flex; overflow: scroll; margin-bottom: 14px; padding: 6px 2px; }}
.film-card {{ background-color: {surface}; border-radius: 10px; padding: 10px 14px;
             margin-right: 10px; width: 190px;
             box-shadow: 0 2px 8px rgba(0, 0, 0, 0.45);
             border: 1px solid {surface_2}; }}
.film-played {{ opacity: 0.45; }}
.film-current {{ border: 1px solid {tertiary}; }}
.film-tag {{ color: {secondary}; font-size: 11px; }}
.film-label {{ color: {text}; font-size: 14px; margin-bottom: 4px; }}
.film-meta {{ color: {text_dim}; font-size: 11px; }}
"#
    )
}

/// The default sheet: Slate seeds through the derivation.
pub fn slate_stage_css() -> String {
    stage_css(&derive_palette(&slate_seeds()))
}
