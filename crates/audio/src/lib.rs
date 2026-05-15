//! Audio engine for guitar-toolkit.
//!
//! - [`sequencer`] — pure data: patterns, tracks, steps, time signatures,
//!   subdivisions. No audio dependencies. The metronome is just a
//!   special-case sequencer pattern.
//! - [`sound`] — sample-by-sample synthesis (currently click only).
//! - [`engine`] — cpal integration: owns the output stream, runs the
//!   pattern, mixes voices. Use [`engine::SequencerEngine::new`] to
//!   start, [`engine::EngineHandle`] to control from another thread.

pub mod sequencer;
pub mod sound;
pub mod engine;
pub mod input;

pub use engine::{AudioError, EngineHandle, SequencerEngine};
pub use input::{
    DetectedNote, DetectedNoteName, DetectorKind, TunerEngine, TunerHandle, TunerSnapshot,
};
pub use sequencer::{SequencerPattern, Step, Subdivision, TimeSignature, Track};
pub use sound::Sound;
