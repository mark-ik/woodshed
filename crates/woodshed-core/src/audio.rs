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

/// Neutral snapshot of a round-trip latency calibration run, for the UI
/// (audio-depth slice 14). Latencies are in milliseconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CalibrationStatus {
    /// No run in progress.
    Idle,
    /// Playing clicks and listening for the player's taps.
    Running { clicks_fired: usize, total: usize },
    /// Finished — a measured round-trip latency to accept.
    Success { latency_ms: f32, matched: usize, total: usize },
    /// Finished but too few hits matched; offer a retry.
    Insufficient { matched: usize, total: usize },
    /// The audio engines aren't available (no mic, etc.).
    Unavailable,
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
    /// Voice a set of chord / scale tones on demand — the "hear it"
    /// preview — independent of transport. `strum_ms` staggers note
    /// onsets: 0 = block chord, ~18 = a gentle strum, larger = an
    /// arpeggiated cascade (a scale run). Default no-op so a backend
    /// that can't voice previews stays silent rather than being forced
    /// to implement it.
    fn preview_pitches(&mut self, _pitches_hz: &[f32], _duration_secs: f32, _strum_ms: f32) {}
    /// Voice a single pitched note on demand — the arpeggio / exercise
    /// step-through sonification. Default no-op (see
    /// [`preview_pitches`](Self::preview_pitches)).
    fn preview_note(&mut self, _freq_hz: f32, _duration_secs: f32) {}
    /// Start a round-trip latency calibration run: play a lead of clicks
    /// the player taps along to; the onset detector times the hits.
    /// No-op default: a backend without input calibration stays idle.
    fn calibration_start(&mut self) {}
    /// Poll a running calibration; returns a neutral status snapshot.
    fn calibration_poll(&mut self) -> CalibrationStatus {
        CalibrationStatus::Idle
    }
    /// Cancel a running calibration and restore the metronome.
    fn calibration_cancel(&mut self) {}
    /// The active input→output latency compensation (ms), if calibrated.
    fn latency_ms(&self) -> Option<f32> {
        None
    }
    /// Set or clear the active latency compensation (ms) — the user
    /// accepting a calibration result, or clearing it.
    fn set_latency_ms(&mut self, _ms: Option<f32>) {}
    /// Arm song-mode loop recording for the bar at `bar_idx` — capture
    /// begins when playback next reaches that bar. No-op default.
    fn song_arm_record(&mut self, _bar_idx: usize) {}
    /// Stop song-mode loop recording.
    fn song_stop_record(&mut self) {}
    /// Clear (erase) the recorded loop on the bar at `bar_idx`.
    fn song_clear_loop(&mut self, _bar_idx: usize) {}
    /// Recording mode: `true` = replace the bar's audio each pass,
    /// `false` = overdub (sum) onto it.
    fn song_set_record_replace(&mut self, _replace: bool) {}
    /// Whether the song engine is currently capturing input into a bar.
    fn song_recording(&self) -> bool {
        false
    }
    /// Per-bar flags: does each bar hold a recorded loop? Empty when the
    /// backend has no song engine.
    fn song_loop_bars(&self) -> Vec<bool> {
        Vec::new()
    }
    /// A device/stream failure to surface in the UI, if any.
    fn error(&self) -> Option<&str>;
}
