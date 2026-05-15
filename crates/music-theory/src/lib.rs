//! Music theory primitives for stringed instruments.
//!
//! Pure data + math. No I/O, no UI, no audio. Consumers (the app, future
//! web frontend, future CLI) depend on this crate for the canonical model
//! of pitches, intervals, tunings, scales, chords, and progressions.

pub mod pitch;
pub mod interval;
pub mod tuning;
pub mod scale;
pub mod chord;
pub mod fretboard;
pub mod exercise;
pub mod progression;
pub mod practice;
