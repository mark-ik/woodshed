//! Input-to-output round-trip latency calibration.
//!
//! What it measures:
//!
//! ```text
//!     ┌──────────┐  output  ┌────────┐         ┌─────┐  input  ┌────────┐
//!     │ App emits│  latency │Speaker │ travel  │ Mic │ latency │ Onset  │
//!     │  click   │ ───────► │        │ ──────► │     │ ──────► │detector│
//!     │ at t=T0  │          │        │  (~ms)  │     │         │  T1    │
//!     └──────────┘          └────────┘         └─────┘         └────────┘
//! ```
//!
//! `T1 - T0` is the full round-trip the user perceives as "lag" between
//! a metronome click and the moment their hit gets registered. Both
//! the onset-timing-feedback widget and the loop recorder need this
//! number to display anything honest:
//!
//! - Without it, "you're 30ms late" might be entirely the system
//!   latency, not the player.
//! - For loop record, samples captured "in time" actually land in
//!   the buffer one round-trip late; without compensation the loop
//!   sounds noticeably behind on playback.
//!
//! # API shape
//!
//! Two layers:
//!
//! - [`estimate_latency_from_pairs`] — pure DSP. Given a list of
//!   expected click instants and detected onset instants, returns the
//!   median latency. Trivially testable, no audio dependency.
//! - [`CalibrationSession`] — drives a live calibration run. The UI
//!   constructs one with the engine handles, calls [`Self::start`]
//!   when the user clicks "Calibrate," then polls [`Self::poll`]
//!   each tick. When [`Self::poll`] returns a terminal
//!   [`CalibrationOutcome`], the UI shows the result and lets the
//!   user accept / discard.
//!
//! The session does *not* own the audio engines — the UI passes in
//! the relevant handles so calibration composes with whatever
//! transport the user is otherwise running.

use std::time::{Duration, Instant};

use crate::onset::OnsetHandle;
use crate::sequencer::{SequencerPattern, Subdivision, TimeSignature};
use crate::EngineHandle;

// Pure pairing / median-latency DSP (and the `MATCH_WINDOW` it keys
// off) now lives in `audio_primitives::calibration`, shared with
// Strophe. Re-exported so existing
// `woodshed_audio::{estimate_latency_from_pairs, MATCH_WINDOW}` paths
// resolve unchanged. The live session driver below builds on it.
pub use audio_primitives::calibration::{count_matches, estimate_latency_from_pairs, MATCH_WINDOW};

/// BPM the calibration metronome runs at. 60 BPM = one click per
/// second — slow enough to leave plenty of "guard space" between
/// clicks so a detected onset is unambiguously the click that
/// preceded it, not one further back.
pub const CALIBRATION_BPM: f32 = 60.0;

/// How many clicks to play during a calibration run. 6 is enough to
/// get a stable median while keeping the experience under 10 seconds.
pub const CALIBRATION_CLICKS: usize = 6;

// ============================================================
// Session driver
// ============================================================

/// Phase the calibration session is in.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CalibrationPhase {
    /// Hasn't started yet. UI shows the "Calibrate" button.
    Idle,
    /// Playing clicks and listening. UI shows progress.
    Running,
    /// Run finished. UI shows the result (success or failure) and
    /// offers Accept / Retry.
    Finished,
}

/// Terminal outcome from [`CalibrationSession::poll`].
#[derive(Clone, Debug)]
pub enum CalibrationOutcome {
    /// Still running; nothing to display yet beyond a progress meter.
    InProgress {
        /// How many of the expected clicks have fired by now.
        clicks_fired: usize,
        /// Total expected clicks for this run.
        total_clicks: usize,
    },
    /// Calibration succeeded. Apply this latency to the app's settings.
    Success {
        latency: Duration,
        matched_pairs: usize,
        total_clicks: usize,
    },
    /// Not enough clicks matched — the user probably didn't play
    /// along, or the threshold was too high. The UI should offer to
    /// retry.
    InsufficientPairs {
        matched_pairs: usize,
        total_clicks: usize,
    },
    /// The audio engines aren't available (e.g. mic permission
    /// denied). UI should surface this and not try again.
    EngineUnavailable,
}

/// Stateful driver for one calibration run.
///
/// Lifecycle:
/// ```text
///   Idle → start() → Running → poll() repeatedly → Finished
/// ```
///
/// The session doesn't take ownership of the engines — UIs pass them
/// in by reference at call sites. This keeps the session compatible
/// with any UI framework and avoids fighting with whatever transport
/// the user has running.
pub struct CalibrationSession {
    phase: CalibrationPhase,
    /// Wall-clock time the run started (i.e., when the first click
    /// was scheduled to play).
    start_at: Option<Instant>,
    /// Pre-computed expected instants for each click in the run.
    expected_clicks: Vec<Instant>,
    /// Configurable: how many clicks per run.
    total_clicks: usize,
    /// Configurable: BPM the calibration plays at.
    bpm: f32,
}

impl Default for CalibrationSession {
    fn default() -> Self {
        Self::new()
    }
}

impl CalibrationSession {
    pub fn new() -> Self {
        Self {
            phase: CalibrationPhase::Idle,
            start_at: None,
            expected_clicks: Vec::new(),
            total_clicks: CALIBRATION_CLICKS,
            bpm: CALIBRATION_BPM,
        }
    }

    /// Override the click count for the run. Useful for tests; the
    /// shipping UI sticks to the constants.
    pub fn with_total_clicks(mut self, n: usize) -> Self {
        self.total_clicks = n.max(2);
        self
    }

    /// Override the calibration BPM.
    pub fn with_bpm(mut self, bpm: f32) -> Self {
        self.bpm = bpm.clamp(40.0, 240.0);
        self
    }

    pub fn phase(&self) -> CalibrationPhase {
        self.phase
    }

    /// Begin a calibration run. Resets the onset history (so prior
    /// playing doesn't pollute pairing), installs a metronome pattern
    /// on the output engine, and starts playback.
    ///
    /// The caller is responsible for ensuring the onset analyzer is
    /// enabled. The session enables it as a courtesy but the UI
    /// should leave it enabled until the run finishes.
    pub fn start(&mut self, engine: &EngineHandle, onset: &OnsetHandle) {
        onset.reset();
        onset.set_enabled(true);

        let pattern = SequencerPattern::metronome(
            self.bpm,
            TimeSignature::new(4, 4),
            Subdivision::QUARTER,
        );
        engine.set_pattern(pattern);
        engine.play();

        let now = Instant::now();
        let secs_per_click = 60.0 / self.bpm;
        // First click is "now-ish" — there's some output latency
        // before it actually emerges, but that's exactly what we're
        // trying to measure, so the expected instant is the schedule,
        // not the actual emergence.
        self.expected_clicks = (0..self.total_clicks)
            .map(|i| now + Duration::from_secs_f32(secs_per_click * i as f32))
            .collect();
        self.start_at = Some(now);
        self.phase = CalibrationPhase::Running;
    }

    /// Cancel a running session without producing a result.
    pub fn cancel(&mut self, engine: &EngineHandle) {
        engine.stop();
        self.phase = CalibrationPhase::Idle;
        self.start_at = None;
        self.expected_clicks.clear();
    }

    /// Check progress. Call from a UI tick (~10-50 Hz is plenty —
    /// the click cadence is 1 Hz).
    ///
    /// Returns:
    /// - [`CalibrationOutcome::InProgress`] while clicks are still
    ///   firing. Use the included counts to draw a progress bar.
    /// - [`CalibrationOutcome::Success`] or
    ///   [`CalibrationOutcome::InsufficientPairs`] when the run is
    ///   over. The session transitions to [`CalibrationPhase::Finished`].
    pub fn poll(&mut self, engine: &EngineHandle, onset: &OnsetHandle) -> CalibrationOutcome {
        if self.start_at.is_none() {
            return CalibrationOutcome::EngineUnavailable;
        }

        let now = Instant::now();
        let clicks_fired = self
            .expected_clicks
            .iter()
            .filter(|&&t| t <= now)
            .count();

        // Give a settle window equal to the match window after the
        // last expected click so a late onset on the final click
        // still gets paired.
        let final_click = *self
            .expected_clicks
            .last()
            .expect("non-empty after start()");
        let done = now >= final_click + MATCH_WINDOW;

        if !done {
            return CalibrationOutcome::InProgress {
                clicks_fired,
                total_clicks: self.total_clicks,
            };
        }

        // Run is complete — stop the click and tally.
        engine.stop();
        self.phase = CalibrationPhase::Finished;

        let snapshot = onset.snapshot();
        let onset_instants: Vec<Instant> =
            snapshot.recent.iter().map(|o| o.at).collect();
        // Require at least half the clicks to have matched.
        let minimum = self.total_clicks / 2;
        match estimate_latency_from_pairs(&self.expected_clicks, &onset_instants, minimum) {
            Some(latency) => CalibrationOutcome::Success {
                latency,
                matched_pairs: count_matches(&self.expected_clicks, &onset_instants),
                total_clicks: self.total_clicks,
            },
            None => CalibrationOutcome::InsufficientPairs {
                matched_pairs: count_matches(&self.expected_clicks, &onset_instants),
                total_clicks: self.total_clicks,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Pure pairing/median tests (estimate_latency_from_pairs,
    // count_matches) moved with the code to audio-primitives. What
    // remains here covers the session driver's own state machine.

    #[test]
    fn session_starts_idle() {
        let s = CalibrationSession::new();
        assert_eq!(s.phase(), CalibrationPhase::Idle);
    }

    #[test]
    fn session_with_total_clicks_clamps_minimum() {
        let s = CalibrationSession::new().with_total_clicks(0);
        assert!(s.total_clicks >= 2);
    }
}
