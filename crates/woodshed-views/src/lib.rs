//! Woodshed's xilem_serval view layer.
//!
//! View fns + CSS sheets consumed by both hosts (`woodshed-serval` desktop,
//! `woodshed-web` browser). Views diff into serval's `ScriptedDom`; styling is
//! plain CSS emitted per sheet, which is where the seed-derived theme engine
//! plugs in from S1 on.
//!
//! S0 ships only the [`demo`] module: the static Stage sheet (ported from
//! serval's `serval_web_smoke`, the 2026-07-04 browser receipt). S1 replaces
//! it with views over the real `AppState`.

pub mod demo;

pub use xilem_serval;
