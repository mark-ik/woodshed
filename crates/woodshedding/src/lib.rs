//! Music theory primitives for stringed instruments — the theory core
//! of the Woodshed project.
//!
//! Pure data + math. No I/O, no UI, no audio. Consumers (the Woodshed
//! app, future web frontend, future CLI) depend on this crate for the
//! canonical model of pitches, intervals, tunings, scales, chords,
//! progressions, exercises, and practice sets.

pub mod pitch;
pub mod interval;
pub mod tuning;
pub mod scale;
pub mod chord;
pub mod fretboard;
pub mod exercise;
pub mod progression;
pub mod practice;
