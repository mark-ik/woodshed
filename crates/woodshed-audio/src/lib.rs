//! Audio engine for Woodshed.
//!
//! ## Output path
//! - [`sequencer`] — pure data: patterns, tracks, steps, time signatures,
//!   subdivisions. Serde-serializable for preset persistence.
//! - [`sound`] — sample-by-sample synthesis: [`Sound::Click`] (sine
//!   burst + envelope) and [`Sound::Sample`] (loaded WAV with gain +
//!   pitch shift).
//! - [`samples`] — [`SampleBank`] for loading WAV files into runtime
//!   buffers.
//! - [`engine`] — cpal output stream + voice mixer.
//!
//! ## Input path
//! - [`input`] — [`InputEngine`] owns one cpal input stream; analyzers
//!   plug in via [`InputEngineBuilder`] and the [`input::Analyzer`] trait.
//! - [`input::PitchAnalyzer`] — drives the tuner.
//! - [`onset`] — [`onset::OnsetAnalyzer`] for transient detection, plus
//!   the pure [`OnsetDetector`] + [`estimate_bpm`] for audio-tap-tempo.
//!
//! ## Cross-cutting
//! - [`calibration`] — measures input-to-output round-trip latency by
//!   correlating known click times against detected onsets. Reusable
//!   widget shape; pure DSP helper + a driver session struct.
//! - [`offline`] — render a pattern to a `Vec<f32>` or `.wav` without
//!   going through cpal.
//! - [`midi`] — MIDI in/out, clock-slave BPM derivation.
//! - [`looper`] — bar-aligned audio loop record + overdub.

pub mod calibration;
pub mod chord_audio;
pub mod engine;
pub mod input;
pub mod looper;
pub mod midi;
pub mod offline;
pub mod onset;
pub mod samples;
pub mod sequencer;
pub mod song;
pub mod song_engine;
pub mod sound;

pub use calibration::{estimate_latency_from_pairs, CalibrationOutcome, CalibrationSession};
pub use chord_audio::{render_chord, ChordRender};
pub use engine::{AudioError, EngineHandle, SequencerEngine};
pub use input::{
    Analyzer, DetectedNote, DetectedNoteName, DetectorKind, InputEngine, InputEngineBuilder,
    LooperCaptureAnalyzer, LooperCaptureHandle, PitchAnalyzer, TunerEngine, TunerHandle,
    TunerSnapshot,
};
pub use looper::Looper;
pub use midi::{
    list_input_ports as list_midi_input_ports, list_output_ports as list_midi_output_ports,
    MidiClockSync, MidiError, MidiEvent, MidiIn, MidiInState, MidiOut,
};
pub use offline::{export_wav, render_pattern};
pub use onset::{
    estimate_bpm, DetectedOnset, OnsetAnalyzer, OnsetDetector, OnsetHandle, OnsetSnapshot,
};
pub use samples::{SampleBank, SampleError};
pub use sequencer::{SequencerPattern, Step, Subdivision, TimeSignature, Track};
pub use song::{Bar, ChordRef, PendingChange, Song, SongCursor, SongError};
pub use song_engine::{SongEngine, SongEngineHandle};
pub use sound::{SampleBuffer, Sound};
