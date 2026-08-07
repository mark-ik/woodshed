//! Woodshed's Cambium view layer.
//!
//! View fns + CSS sheets consumed by both hosts (`woodshed-genet` desktop,
//! `woodshed-web` browser). Views diff into genet's `ScriptedDom`; styling is
//! plain CSS emitted per sheet, which is where the seed-derived theme engine
//! plugs in from S1 on.
//!
//! S1: [`stage`] renders the Stage lens over live `woodshed_core::StageState`
//! (scale sidebar clicks select), styled by [`theme`]'s Slate sheet. The S0
//! static [`demo`] module is kept for host smoke tests until S2.

pub mod demo;
pub mod fretboard_leaf;
pub mod stage;
pub mod settings_provider;
pub mod theme;

pub use cambium;
