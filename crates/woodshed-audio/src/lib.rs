//! Audio engine for Woodshed.
//!
//! - [`sequencer`] — pure data: patterns, tracks, steps, time signatures,
//!   subdivisions. No audio dependencies. The metronome is just a
//!   special-case sequencer pattern. Serde-serializable for preset
//!   persistence.
//! - [`sound`] — sample-by-sample synthesis. Two variants today:
//!   synthesized [`Sound::Click`] (sine burst + envelope) and PCM
//!   [`Sound::Sample`] (a loaded WAV played back with gain and pitch
//!   shift).
//! - [`samples`] — [`SampleBank`] for loading WAV files and serving
//!   them to `Sound::Sample` instances by id.
//! - [`engine`] — cpal integration: owns the output stream, runs the
//!   pattern, mixes voices. Use [`engine::SequencerEngine::new`] to
//!   start, [`engine::EngineHandle`] to control from another thread.

pub mod sequencer;
pub mod sound;
pub mod samples;
pub mod engine;
pub mod input;
pub mod onset;
pub mod offline;
pub mod midi;
pub mod looper;

pub use engine::{AudioError, EngineHandle, SequencerEngine};
pub use midi::{
    list_input_ports as list_midi_input_ports,
    list_output_ports as list_midi_output_ports, MidiClockSync, MidiError, MidiEvent,
    MidiIn, MidiInState, MidiOut,
};
pub use looper::Looper;
pub use offline::{export_wav, render_pattern};
pub use onset::{
    estimate_bpm, DetectedOnset, OnsetDetector, OnsetEngine, OnsetHandle, OnsetSnapshot,
};
pub use input::{
    DetectedNote, DetectedNoteName, DetectorKind, TunerEngine, TunerHandle, TunerSnapshot,
};
pub use samples::{SampleBank, SampleError};
pub use sequencer::{SequencerPattern, Step, Subdivision, TimeSignature, Track};
pub use sound::{SampleBuffer, Sound};
