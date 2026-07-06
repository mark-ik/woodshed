//! The audio seam (serval-host plan W0.1).
//!
//! The core describes what the app wants (transport state, tuner state) in
//! pure data; a host supplies an [`AudioBackend`] that realizes it — cpal
//! through `woodshed-audio` on desktop, Web Audio / AudioWorklet in the
//! browser. The core never touches an audio API, so the same state drives
//! both.

/// One tuner analysis snapshot, host-agnostic.
#[derive(Clone, Debug, PartialEq)]
pub struct TunerReading {
    /// Nearest 12-TET note name ("A", "C#").
    pub note: String,
    pub octave: i32,
    /// Cents from the nearest note, roughly [-50, +50].
    pub cents: f64,
    pub in_tune: bool,
    /// Input RMS level in [0, 1] — drives a level meter and the
    /// "listening but silent" presentation.
    pub level: f32,
}

/// What the metronome transport should be doing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransportState {
    pub bpm: f32,
    pub playing: bool,
}

impl Default for TransportState {
    fn default() -> Self {
        Self {
            bpm: 120.0,
            playing: false,
        }
    }
}

impl TransportState {
    pub fn nudge_bpm(&mut self, delta: f32) {
        self.bpm = (self.bpm + delta).clamp(30.0, 300.0);
    }
}

/// Whether the tuner is listening, and the latest reading a host polled.
#[derive(Clone, Debug, Default)]
pub struct TunerState {
    pub enabled: bool,
    /// Latest reading; `None` while disabled or below the level
    /// threshold. Hosts poll their backend and write this.
    pub reading: Option<TunerReading>,
}

/// The host-supplied audio realization. Implementations should be
/// tolerant of missing devices: construct in a degraded state and report
/// through [`error`](Self::error) rather than failing the app.
pub trait AudioBackend {
    /// Realize the metronome transport (idempotent; called after every
    /// input dispatch).
    fn set_metronome(&mut self, transport: TransportState);
    /// Start/stop the tuner analysis pipeline (idempotent).
    fn set_tuner_enabled(&mut self, enabled: bool);
    /// Latest tuner reading, `None` when disabled or no signal.
    fn tuner_reading(&self) -> Option<TunerReading>;
    /// Load the song lane (replaces the current song; the transport
    /// keeps its playing state).
    fn set_song(&mut self, doc: &crate::song::SongDoc);
    /// Start/stop song playback (idempotent).
    fn set_song_transport(&mut self, playing: bool);
    /// Snap the song cursor back to the top.
    fn song_rewind(&mut self);
    /// The bar block under the playback cursor (for timeline follow).
    fn song_bar(&self) -> Option<usize>;
    /// A device/stream failure to surface in the UI, if any.
    fn error(&self) -> Option<&str>;
}
