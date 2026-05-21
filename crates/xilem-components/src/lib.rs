//! Domain-neutral reusable Xilem/Masonry UI components.
//!
//! The design-system layer of the Strophos family (Woodshed, Strophe,
//! Mere). Everything here is **audio-agnostic and product-agnostic** —
//! generic over the host's `State`, never referencing any product type
//! — so all three apps (including Mere, which isn't an audio app) can
//! share it. Audio-domain widgets (waveform, fader, knob, meter) live
//! one layer up in `audio-widgets`, which depends on this crate.
//!
//! Widgets take their colors/sizing as plain parameters rather than
//! reading a palette, so this crate stays independent of where the
//! shared `theme` ultimately lives. (Once the theme system settles,
//! `theme` migrates down here and these widgets read it directly.)
//!
//! ## Components
//! - [`combobox`] — a trigger button that toggles an inline list of
//!   mutually-exclusive options. Generic over `State`: the caller
//!   supplies the open-state setter + selection callback.

pub mod combobox;

pub use combobox::combobox;
