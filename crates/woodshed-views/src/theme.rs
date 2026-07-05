//! Theme CSS for the serval host.
//!
//! S1 carries the **derived Slate palette verbatim** (probed from
//! `audio-widgets::theme::derive_palette` on the Slate seeds, 2026-07-05)
//! as constants, emitted as one CSS sheet. The OKLCH seed engine itself
//! still lives in `audio-widgets` (masonry-coupled, Strophe-shared);
//! porting the derivation math to a pure crate so Ember/user themes work
//! here is a follow-up recorded in the serval-host plan.

/// Derived Slate palette (probe output, hex).
pub mod slate {
    pub const BG: &str = "#090c1a";
    pub const SURFACE: &str = "#131826";
    pub const SURFACE_2: &str = "#1f2332";
    pub const SURFACE_HOVER: &str = "#2d3242";
    pub const TEXT_HEADER: &str = "#e7eeff";
    pub const TEXT: &str = "#e7eeff";
    pub const TEXT_DIM: &str = "#a3aaba";
    pub const TEXT_DISABLED: &str = "#686e7d";
    pub const PRIMARY: &str = "#3366c8";
    pub const ON_PRIMARY: &str = "#f4f4f8";
    pub const SECONDARY: &str = "#2e9da6";
    pub const ON_SECONDARY: &str = "#14141a";
    pub const TERTIARY: &str = "#e0a846";
    pub const ON_TERTIARY: &str = "#14141a";
    pub const SUCCESS: &str = "#4fb36e";
    pub const DANGER: &str = "#d54e4e";
}

/// The Stage sheet in the Slate palette.
pub fn slate_stage_css() -> String {
    use slate::*;
    format!(
        r#"
.root {{ width: 100%; height: 100%; background-color: {BG}; color: {TEXT};
        font-family: sans-serif; font-size: 14px; padding: 16px; }}
.title {{ font-size: 18px; color: {TEXT_HEADER}; margin-bottom: 12px; }}
.pills {{ display: flex; margin-bottom: 16px; }}
.pill {{ padding: 6px 14px; margin-right: 6px; border-radius: 14px; color: {TEXT_DIM}; }}
.pill-active {{ background-color: {SURFACE_2}; color: {TERTIARY}; }}
.body {{ display: flex; }}
.side {{ width: 220px; margin-right: 16px; }}
.side-item {{ padding: 5px 10px; color: {TEXT_DIM}; border-radius: 6px; }}
.side-active {{ background-color: {SURFACE_2}; color: {TEXT}; }}
.board {{ background-color: {SURFACE}; border-radius: 10px; padding: 14px; }}
.string {{ display: flex; margin-bottom: 6px; }}
.fret {{ width: 46px; height: 28px; }}
.nut-gap {{ margin-right: 8px; }}
.dot {{ width: 24px; height: 24px; border-radius: 12px; background-color: {PRIMARY};
       color: {ON_PRIMARY}; font-size: 10px; text-align: center; }}
.root-dot {{ background-color: {TERTIARY}; color: {ON_TERTIARY}; }}
.scale-name {{ margin-top: 10px; color: {TEXT_DIM}; font-size: 12px; }}
.caption {{ margin-top: 12px; color: {TEXT_DISABLED}; font-size: 12px; }}
"#
    )
}
