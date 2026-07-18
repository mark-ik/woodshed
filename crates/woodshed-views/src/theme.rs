//! Theme CSS for the genet host, derived through `tinct`.
//!
//! A theme is a few seed colours; `tinct::derive_palette` produces the full
//! semantic palette (surface ladder, text tiers, tonal triad, flags), and
//! this module renders it as the CSS sheet the views class against. Same
//! seeds as `audio-widgets::theme`'s Slate, so the genet host and the
//! xilem app agree until the parity cut.

use tinct::{color_from_hex, color_to_hex, derive_palette, Palette, Seeds};

fn hex(s: &str) -> tinct::Srgb {
    color_from_hex(s).expect("valid seed hex")
}

/// The built-in themes (seed sets match audio-widgets' engine, so the
/// genet host and the xilem app agree until the parity cut).
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
    let danger = color_to_hex(p.danger);
    // Fretboard ink, derived from the same palette so every theme gets a board:
    // the fret wires, the strings running through the note centres, and the nut.
    let fret_wire = &surface_2;
    let string_line = &text_disabled;
    let nut = &text_dim;
    format!(
        r#"
.root {{ width: 100%; height: 100%; box-sizing: border-box;
        background-color: {bg}; color: {text};
        font-family: sans-serif; font-size: 14px; padding: 8px 16px 16px 16px; }}
.desktop-frame {{ width: 100%; height: 100%; box-sizing: border-box;
                  display: flex; flex-direction: column; background-color: {bg};
                  color: {text}; font-family: sans-serif; font-size: 14px;
                  padding: 8px 16px 0 16px; }}
.desktop-frame .root {{ width: 100%; height: auto; min-height: 0; flex-grow: 1;
                        background-color: transparent; padding: 0 0 16px 0; }}
.chrome {{ display: flex; margin-bottom: 10px; }}
.chrome-title {{ color: {text_header}; font-size: 15px; padding: 4px 8px 4px 0; }}
.chrome-drag {{ flex-grow: 1; height: 26px; }}
.chrome-btn {{ color: {text_dim}; padding: 2px 12px; border-radius: 6px;
              font-size: 15px; }}
.chrome-btn:hover {{ background-color: {surface_2}; color: {text}; }}
.chrome-close:hover {{ background-color: {danger}; color: {on_primary}; }}
.header-row {{ display: flex; margin-bottom: 12px; }}
.header-label {{ color: {text_dim}; padding: 4px 6px 4px 0; }}
.header-gap {{ width: 18px; }}
.pills {{ display: flex; margin-bottom: 12px; }}
.pill {{ padding: 6px 14px; margin-right: 6px; border-radius: 14px; color: {text_dim}; }}
.pill-active {{ background-color: {surface_2}; color: {tertiary}; }}
.nav-spacer {{ flex-grow: 1; }}
.search-wrap {{ width: 240px; margin-right: 8px; box-sizing: border-box; }}
.search-wrap input {{ display: block; width: 240px; box-sizing: border-box;
                     background-color: {surface_2}; color: {text}; padding: 6px 12px;
                     border-radius: 14px; border: 1px solid {surface_2}; font-size: 13px; }}
.search-wrap input:focus {{ border: 1px solid {tertiary}; }}
/* The field renders its text as element content and carries no browser input
   value semantics, so there is no ::placeholder to style. The hint is a sibling
   overlaid on the empty field, and pointer-events: none keeps a click on it
   landing in the field. Padding matches the field's 6px/12px plus its 1px border
   so the hint sits exactly where the typed text will appear. */
.search-hint {{ position: absolute; top: 0; left: 0; padding: 7px 13px;
               color: {text_disabled}; font-size: 13px; pointer-events: none; }}
.search-list {{ background-color: {surface_2}; border-radius: 8px; padding: 4px;
               width: 240px; box-sizing: border-box;
               box-shadow: 0 4px 14px rgba(0, 0, 0, 0.5); }}
.search-item {{ display: flex; padding: 5px 10px; border-radius: 6px; }}
.search-item:hover {{ background-color: {surface_hover}; }}
.search-label {{ color: {text}; flex-grow: 1; font-size: 13px; }}
.search-kind {{ color: {text_dim}; font-size: 11px; padding: 2px 0 0 8px; }}
.lens-strip {{ display: flex; margin-bottom: 16px; }}
.lens {{ padding: 4px 12px; margin-right: 6px; border-radius: 12px; color: {text_dim};
        font-size: 13px; }}
.lens-active {{ background-color: {surface_2}; color: {tertiary}; }}
.body {{ display: flex; }}
.side {{ width: 220px; margin-right: 16px; }}
.side-item {{ padding: 5px 10px; color: {text_dim}; border-radius: 6px; }}
.side-active {{ background-color: {surface_2}; color: {text}; }}
.board {{ background-color: {surface}; border-radius: 10px; padding: 14px; }}
/* The board is a Sprigging paint leaf (crisp neck + markers); note labels ride
   as a thin text layer over it, each positioned at the leaf's marker centre. */
.fretboard-stack {{ position: relative; }}
/* The board's scroll viewport: relative so it is the overlay's containing block
   (its absolute label/card layers then clip and scroll with the leaf), and the
   overflow axis + size come from an inline style so orientation picks which way
   the neck scrolls. A neck that already fits shows no scrollbar. */
.board-viewport {{ position: relative; }}
/* Adjustable neck range: From/To steppers + a Full (auto-track) toggle. */
.neck-control {{ display: flex; align-items: center; margin-top: 10px; }}
.neck-label {{ color: {text_dim}; font-size: 13px; margin-right: 8px; }}
.neck-label-gap {{ margin-left: 18px; }}
.neck-step {{ padding: 3px 10px; border-radius: 6px; background-color: {surface_2};
             color: {tertiary}; font-size: 14px; cursor: pointer; }}
.neck-step:hover {{ background-color: {surface_hover}; color: {text_header}; }}
.neck-value {{ min-width: 26px; text-align: center; color: {text}; font-size: 14px;
              padding: 0 4px; }}
.neck-full {{ margin-left: 16px; padding: 4px 12px; border-radius: 6px;
             color: {text_dim}; font-size: 13px; cursor: pointer; }}
.neck-full.side-active {{ background-color: {surface_2}; color: {text}; }}
.neck-full:hover {{ background-color: {surface}; color: {text}; }}
.label-layer {{ position: absolute; top: 0; left: 0; }}
.fret-label {{ position: absolute; display: flex; align-items: center;
              justify-content: center; color: {on_primary}; font-size: 11px;
              cursor: pointer; }}
.fret-label.pinned {{ border: 1.5px solid {tertiary}; border-radius: 4px;
                     box-sizing: border-box; }}
/* Marked note: label brightened to match its leaf selection ring. Excluded note
   (what the current mode silences): faded to match its dim marker, still a click
   target. */
.fret-label.marked {{ color: {text_header}; }}
.fret-label.excluded {{ opacity: 0.4; }}
/* Pinned marker detail cards (quiet floating popovers over the board). */
.card-layer {{ position: absolute; top: 0; left: 0; }}
.note-card {{ position: absolute; background-color: {surface_2};
             border: 1px solid {surface_hover}; border-radius: 8px;
             padding: 7px 10px; box-sizing: border-box; }}
.note-card-title {{ color: {text_header}; font-size: 14px; margin-bottom: 3px; }}
.note-card-row {{ color: {text_dim}; font-size: 11px; line-height: 15px; }}
.note-card-play {{ margin-top: 7px; padding: 4px 0; border-radius: 5px;
                  background-color: {surface}; color: {tertiary}; font-size: 11px;
                  text-align: center; cursor: pointer; }}
.note-card-play:hover {{ background-color: {surface_hover}; }}
.board-caption {{ display: flex; align-items: center; }}
.clear-pins {{ margin-left: auto; padding: 3px 10px; border-radius: 6px;
              color: {text_dim}; font-size: 12px; cursor: pointer; }}
.clear-pins:hover {{ background-color: {surface_2}; color: {text}; }}
.run-btn {{ padding: 4px 12px; border-radius: 6px; background-color: {surface_2};
           color: {tertiary}; font-size: 12px; cursor: pointer; margin-right: 12px; }}
.run-btn:hover {{ background-color: {surface_hover}; color: {text_header}; }}
/* The Path toggle, lit when the touch's trail is shown. */
.run-btn.path-on {{ background-color: {surface_hover}; color: {tertiary}; }}
/* Draw mode, lit with a ring outline to read as "you're authoring the path". */
.run-btn.draw-on {{ background-color: {surface_hover}; color: {tertiary};
                   box-shadow: inset 0 0 0 1px {tertiary}; }}
/* A drawn marker's step number, shown while drawing in place of its note name:
   brighter than a name so the order pops out of the board. */
.fret-label.step {{ color: {text_header}; font-weight: 600; }}
/* Path-editing tools (undo / reverse / rotate / clear): quiet inline actions. */
.draw-tools {{ display: flex; align-items: center; margin-right: 12px; }}
.draw-tool {{ padding: 3px 8px; border-radius: 5px; color: {text_dim};
             font-size: 12px; cursor: pointer; }}
.draw-tool:hover {{ background-color: {surface_2}; color: {text}; }}
/* Save is the loop-closer, so it reads as the affirmative action. */
.draw-tool.save {{ color: {tertiary}; }}
.draw-tool.save:hover {{ background-color: {surface_hover}; color: {text_header}; }}
/* Rename field for the selected card. The inner input needs its own box (like
   .search-wrap input): a text field renders its buffer as element content, so
   without padding/display it has no hit area to click into and never focuses. */
.card-rename {{ width: 260px; margin-right: 12px; box-sizing: border-box; }}
.card-rename input {{ display: block; width: 260px; box-sizing: border-box;
                     background-color: {surface_2}; color: {text}; padding: 6px 12px;
                     border-radius: 8px; border: 1px solid {surface_2}; font-size: 13px; }}
.card-rename input:focus {{ border: 1px solid {tertiary}; }}
/* Segmented mode control [Off · Solo · Mute]: a structured toggle, not a loose
   button row. The active segment is lit. */
.mode-seg {{ display: flex; margin-right: 12px; border-radius: 6px; }}
.seg {{ padding: 4px 12px; font-size: 12px; color: {text_dim}; cursor: pointer;
       background-color: {surface_2}; }}
.seg:hover {{ color: {text}; }}
.seg.active {{ background-color: {surface_hover}; color: {text_header}; }}
/* The board is a grid of fret cells. Each cell paints its own fret wire (the
   right border) and the segment of string running through it (a 2px band at the
   cell's vertical centre, via a background-image), so the rows must sit flush:
   any gap between strings would break the wires into dashes. Cells are
   border-box, which keeps the thicker nut from widening its column and pushing
   the fret-number ruler out of alignment. */
/* Each string is a thin band at its cell's vertical centre, drawn on the cell.
   The row carries a thickness tier (.string-1 thick .. .string-6 thin): lower
   strings (smaller index, as tunings read low to high) are thicker, like a wound
   bass string next to a plain treble string. genet renders these gradient bands
   softly, so they read as soft hairlines; crisp lines want a Sprigging paint
   leaf (plan Phase C). 28px cell, centred on 14px. */
.string {{ display: flex; }}
.string-1 .fret {{ background-image: linear-gradient(to bottom, transparent 12.7px, {string_line} 12.7px, {string_line} 15.3px, transparent 15.3px); }}
.string-2 .fret {{ background-image: linear-gradient(to bottom, transparent 13.0px, {string_line} 13.0px, {string_line} 15.0px, transparent 15.0px); }}
.string-3 .fret {{ background-image: linear-gradient(to bottom, transparent 13.2px, {string_line} 13.2px, {string_line} 14.8px, transparent 14.8px); }}
.string-4 .fret {{ background-image: linear-gradient(to bottom, transparent 13.4px, {string_line} 13.4px, {string_line} 14.6px, transparent 14.6px); }}
.string-5 .fret {{ background-image: linear-gradient(to bottom, transparent 13.5px, {string_line} 13.5px, {string_line} 14.5px, transparent 14.5px); }}
.string-6 .fret {{ background-image: linear-gradient(to bottom, transparent 13.6px, {string_line} 13.6px, {string_line} 14.4px, transparent 14.4px); }}
.fret {{ width: 46px; height: 28px; box-sizing: border-box;
        display: flex; align-items: center; justify-content: center;
        border-right: 1px solid {fret_wire}; }}
.nut-gap {{ border-right: 3px solid {nut}; }}
/* Note markers are rounded rects centred in the fret space (between two wires),
   coloured like the notes they mark, with the label centred via flex. The
   dot-versus-rect choice will become a setting; this is the rect form. */
.dot {{ width: 38px; height: 22px; border-radius: 5px; background-color: {primary};
       color: {on_primary}; font-size: 11px;
       display: flex; align-items: center; justify-content: center; }}
.fret-nums {{ display: flex; margin-top: 5px; }}
.fret-num {{ width: 46px; box-sizing: border-box; text-align: center;
            color: {text_disabled}; font-size: 10px; }}
.fret-num-marker {{ color: {text_dim}; }}
.layout-canvas .fret {{ width: 64px; height: 42px; }}
/* Canvas cells are taller (42px), so the string band re-centres on 21px. */
.layout-canvas .string-1 .fret {{ background-image: linear-gradient(to bottom, transparent 19.7px, {string_line} 19.7px, {string_line} 22.3px, transparent 22.3px); }}
.layout-canvas .string-2 .fret {{ background-image: linear-gradient(to bottom, transparent 20.0px, {string_line} 20.0px, {string_line} 22.0px, transparent 22.0px); }}
.layout-canvas .string-3 .fret {{ background-image: linear-gradient(to bottom, transparent 20.2px, {string_line} 20.2px, {string_line} 21.8px, transparent 21.8px); }}
.layout-canvas .string-4 .fret {{ background-image: linear-gradient(to bottom, transparent 20.4px, {string_line} 20.4px, {string_line} 21.6px, transparent 21.6px); }}
.layout-canvas .string-5 .fret {{ background-image: linear-gradient(to bottom, transparent 20.5px, {string_line} 20.5px, {string_line} 21.5px, transparent 21.5px); }}
.layout-canvas .string-6 .fret {{ background-image: linear-gradient(to bottom, transparent 20.6px, {string_line} 20.6px, {string_line} 21.4px, transparent 21.4px); }}
.layout-canvas .fret-num {{ width: 64px; }}
.layout-canvas .dot {{ width: 56px; height: 32px; border-radius: 6px;
                      font-size: 14px; }}
.side-strip {{ margin-top: 12px; }}
.side-strip .side {{ width: 100%; display: flex; flex-wrap: wrap; margin-right: 0; }}
.side-strip .side-item {{ margin-right: 6px; margin-bottom: 6px; }}
.related-panel {{ width: 380px; flex: 0 0 380px; margin-left: 16px; background-color: {surface}; border-radius: 10px; padding: 14px; }}
.related-heading {{ color: {text}; font-size: 15px; font-weight: 700; }}
.related-subtitle, .related-empty, .related-why {{ color: {text_dim}; font-size: 12px; }}
.related-subtitle {{ margin-top: 2px; margin-bottom: 10px; }}
/* Graph swatch and suggestions pane sit side by side (wrap when the panel is
   narrow). */
.related-body {{ display: flex; align-items: flex-start; flex-wrap: wrap; }}
.related-graph-col {{ flex: 0 0 auto; background-color: {surface_2}; border-radius: 8px; padding: 4px; margin-right: 10px; margin-bottom: 10px; }}
.related-pane-col {{ flex: 1; min-width: 150px; }}
/* Cambium graph-canvas-swatch: the painted leaf shows the nodes and emphasis;
   the buttons are transparent hit targets. A small quiet Expand button. */
.graph-canvas-swatch-node {{ background-color: transparent; border-width: 0; cursor: pointer; }}
.graph-canvas-swatch-expand {{ background-color: {surface}; color: {text_dim}; border-width: 0; border-radius: 4px; font-size: 10px; padding: 2px 5px; cursor: pointer; }}
.graph-canvas-swatch-expand:hover {{ color: {text}; }}
.related-history {{ margin-bottom: 10px; padding-bottom: 8px; border-bottom-width: 1px; border-bottom-color: {surface_2}; }}
.history-heading {{ color: {text_dim}; font-size: 11px; margin-bottom: 4px; }}
.history-list {{ display: flex; flex-wrap: wrap; }}
.history-item {{ background-color: {surface_2}; border-radius: 10px; margin-right: 5px; margin-bottom: 4px; padding: 3px 7px; font-size: 11px; }}
.history-kind {{ color: {text_dim}; margin-right: 4px; }}
.history-title {{ color: {text}; }}
.tool-board {{ min-height: 220px; }}
.tool-reading {{ color: {tertiary}; font-size: 28px; margin-top: 18px; }}
.settings-shell {{ align-items: flex-start; }}
.settings-nav {{ flex: 0 0 220px; }}
.settings-page {{ flex: 1; min-height: 300px; }}
.settings-options {{ margin-top: 10px; }}
.set-tray {{ margin-top: 14px; background-color: {surface}; border-radius: 10px; padding: 12px; }}
.set-toolbar {{ display: flex; align-items: center; flex-wrap: wrap; margin-bottom: 8px; }}
.set-heading {{ color: {text}; font-size: 15px; font-weight: 700; margin-right: auto; padding-right: 10px; }}
.set-cards {{ display: flex; flex-wrap: wrap; }}
.set-card {{ width: 190px; background-color: {surface_2}; border-radius: 8px; padding: 9px; margin-right: 7px; margin-bottom: 7px; cursor: pointer; }}
.set-card-active {{ background-color: {surface_hover}; outline-width: 1px; outline-color: {tertiary}; }}
.set-card-kind, .set-card-source, .set-empty {{ color: {text_dim}; font-size: 10px; }}
.set-card-title {{ color: {text}; font-size: 13px; margin-top: 3px; }}
.set-card-meta {{ color: {tertiary}; font-size: 10px; margin-top: 4px; }}
.set-card-source {{ margin-top: 3px; }}
.set-editor {{ display: flex; align-items: center; flex-wrap: wrap; border-top-width: 1px; border-top-color: {surface_2}; padding-top: 8px; }}
.set-editor-label {{ color: {text_dim}; font-size: 11px; margin-right: 8px; }}
.viewport-narrow .set-card {{ width: 44%; }}
.viewport-narrow .settings-shell {{ display: block; }}
.viewport-narrow .settings-nav {{ width: 100%; display: flex; flex-wrap: wrap; margin-bottom: 12px; }}
/* Suggestions pane: one structured row each. A row (or its graph node) lights
   the other via `.hovered`. */
.related-row {{ display: flex; align-items: center; border-top-width: 1px; border-top-color: {surface_2}; padding: 6px 4px; border-radius: 6px; }}
.related-row.hovered {{ background-color: {surface_2}; }}
.related-copy {{ display: flex; align-items: center; flex: 1; min-width: 0; cursor: pointer; }}
.related-kind {{ flex: 0 0 auto; font-size: 9px; color: {text_dim}; background-color: {surface_hover}; border-radius: 4px; padding: 2px 5px; margin-right: 7px; }}
.related-copy-text {{ min-width: 0; }}
.related-name {{ color: {text}; font-size: 13px; }}
.related-why {{ margin-top: 1px; }}
.related-actions {{ display: flex; align-items: center; margin-left: 6px; }}
.related-stage {{ color: {tertiary}; font-size: 12px; padding: 4px 7px; border-radius: 5px; cursor: pointer; }}
.related-hide {{ color: {text_dim}; font-size: 14px; padding: 2px 7px; cursor: pointer; }}
.related-stage:hover, .related-hide:hover {{ background-color: {surface_hover}; color: {text}; }}
.viewport-medium .related-panel, .viewport-narrow .related-panel {{ width: auto; flex: 1 1 auto; margin-left: 0; margin-top: 12px; }}
.settings-gap {{ margin-top: 16px; }}
.t-btn:focus {{ background-color: {surface_hover}; color: {tertiary}; }}
.side-item:focus {{ background-color: {surface}; }}
.lens:focus {{ color: {text}; }}
.pill:focus {{ color: {text}; }}
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
.t-hear {{ color: {tertiary}; }}
.rec-on {{ background-color: {danger}; color: {on_primary}; }}
.t-readout {{ color: {text_dim}; padding: 4px 10px 4px 0; }}
.select-box {{ background-color: {surface_2}; color: {text}; padding: 4px 12px;
              border-radius: 6px; }}
.select-list {{ background-color: {surface_2}; border-radius: 6px; padding: 4px;
               width: 220px; }}
.select-option {{ color: {text}; padding: 4px 10px; border-radius: 4px; }}
.settings-heading {{ color: {text_header}; font-size: 15px; margin-bottom: 8px; }}
.settings-line {{ color: {text_dim}; margin-bottom: 6px; }}
.midi-events {{ font-size: 11px; color: {text_disabled}; }}
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
.recipe-grid {{ display: grid; grid-template-columns: repeat(3, 260px); gap: 12px; }}
.recipe-tile {{ background-color: {surface_2}; border-radius: 10px; padding: 12px 14px;
               border: 1px solid {surface_2}; }}
.recipe-name {{ color: {text_header}; font-size: 15px; margin-bottom: 4px; }}
.recipe-desc {{ color: {text_dim}; font-size: 12px; margin-bottom: 6px; }}
.recipe-meta {{ color: {secondary}; font-size: 11px; }}
.recipe-tile:hover {{ border: 1px solid {tertiary}; }}
.bar-lane {{ display: flex; flex-wrap: wrap; margin-bottom: 10px; }}
.bar-chip {{ background-color: {surface_2}; border-radius: 8px; padding: 8px 12px;
            margin: 0 8px 8px 0; width: 110px; border: 1px solid {surface_2}; }}
.bar-current {{ border: 1px solid {secondary}; background-color: {surface_hover}; }}
.bar-edit {{ border: 1px solid {tertiary}; }}
.bar-chip:hover {{ border: 1px solid {text_dim}; }}
.bar-label {{ color: {tertiary}; font-size: 11px; height: 13px; }}
.bar-chord {{ color: {text}; font-size: 15px; }}
.bar-meta {{ color: {text_dim}; font-size: 10px; }}
.t-btn:hover {{ background-color: {surface_hover}; }}
.pill:hover {{ color: {text}; }}
.lens:hover {{ color: {text}; }}
.side-item:hover {{ background-color: {surface}; color: {text}; }}
.select-box:hover {{ background-color: {surface_hover}; }}
.select-option:hover {{ background-color: {surface_hover}; }}
.film-card:hover {{ border: 1px solid {secondary}; }}
.prog-card:hover {{ background-color: {surface_hover}; }}
/* CSS transitions (genet transition_events): subtle hover/active fades
   on the interactive chrome. The host ticks the animation clock each
   frame while any transition runs. */
.t-btn, .chrome-btn, .side-item, .select-box, .select-option,
.search-item, .prog-card {{ transition: background-color 0.12s ease, color 0.12s ease; }}
.pill, .lens {{ transition: background-color 0.14s ease, color 0.14s ease; }}
.film-card, .recipe-tile, .bar-chip {{ transition: border-color 0.14s ease, background-color 0.14s ease; }}
.search-wrap input {{ transition: border-color 0.12s ease; }}
/* Responsive shell. The host selects a width band so the same product view
   stays legible on a small desktop window and future browser canvas. */
.viewport-medium .recipe-grid {{ grid-template-columns: repeat(2, 260px); }}
.viewport-medium .transport {{ flex-wrap: wrap; }}
.viewport-medium .t-btn {{ margin-bottom: 6px; }}
.viewport-narrow .pills {{ flex-wrap: wrap; margin-bottom: 8px; }}
.viewport-narrow .pill {{ padding: 8px 12px; margin-bottom: 6px; }}
.viewport-narrow .nav-spacer {{ display: none; }}
.viewport-narrow .search-wrap {{ width: 100%; margin: 0 0 8px 0; }}
.viewport-narrow .search-wrap input, .viewport-narrow .search-list {{ width: 100%; }}
.viewport-narrow .header-row, .viewport-narrow .transport,
.viewport-narrow .prog-cards {{ flex-wrap: wrap; }}
.viewport-narrow .header-gap {{ width: 10px; }}
.viewport-narrow .t-btn {{ padding: 8px 12px; margin-bottom: 6px; }}
.viewport-narrow .board {{ overflow: scroll; padding: 12px; }}
.viewport-narrow .side-strip .side {{ flex-wrap: wrap; }}
.viewport-narrow .recipe-grid {{ display: flex; flex-direction: column; }}
.viewport-narrow .recipe-tile {{ width: 100%; box-sizing: border-box; }}
"#
    )
}

/// The default sheet: Slate seeds through the derivation.
pub fn slate_stage_css() -> String {
    stage_css(&derive_palette(&slate_seeds()))
}

/// The UI text-scale factor for a named setting ("Large" / "Larger", else 1.0).
pub fn text_scale_factor(name: &str) -> f32 {
    match name {
        "Large" => 1.15,
        "Larger" => 1.3,
        _ => 1.0,
    }
}

/// Apply the accessibility preferences to a generated sheet, as a post-process
/// so the base theme stays untouched. Reduced motion drops the non-essential
/// hover/active fades; a text scale multiplies every `font-size`.
pub fn apply_accessibility(mut css: String, reduce_motion: bool, text_scale: f32) -> String {
    if (text_scale - 1.0).abs() > 0.001 {
        css = scale_font_sizes(&css, text_scale);
    }
    if reduce_motion {
        // Later rules win at equal specificity, so override the four transition
        // rules (theme.rs) to none.
        css.push_str(
            "\n.search-item, .prog-card, .pill, .lens, .film-card, .recipe-tile, \
             .bar-chip, .search-wrap input { transition: none; }\n",
        );
    }
    css
}

/// Scale every `font-size: Npx` in a sheet by `scale`, leaving all else intact.
fn scale_font_sizes(css: &str, scale: f32) -> String {
    const NEEDLE: &str = "font-size: ";
    let mut out = String::with_capacity(css.len() + 32);
    let mut rest = css;
    while let Some(pos) = rest.find(NEEDLE) {
        out.push_str(&rest[..pos + NEEDLE.len()]);
        rest = &rest[pos + NEEDLE.len()..];
        match rest.find("px") {
            Some(px) => match rest[..px].trim().parse::<f32>() {
                Ok(n) => {
                    out.push_str(&((n * scale).round() as i32).to_string());
                    out.push_str("px");
                    rest = &rest[px + 2..];
                }
                // Not a plain "Npx" — leave the declaration as written.
                Err(_) => {}
            },
            None => {}
        }
    }
    out.push_str(rest);
    out
}
