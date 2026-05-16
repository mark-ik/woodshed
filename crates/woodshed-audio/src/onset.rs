//! Onset detection and tempo estimation.
//!
//! [`OnsetDetector`] consumes a stream of audio samples and emits the
//! sample-time of each transient onset (pick attack, drum hit, etc.).
//! It's the kernel that powers two user-facing features:
//!
//! - **Timing feedback** on the Practice tab: compare detected onset
//!   times against the click schedule to tell the user "you're 30ms
//!   late on average."
//! - **Tap-tempo from audio**: collect a handful of onsets and feed
//!   them to [`estimate_bpm`] for hands-free BPM detection.
//!
//! # Algorithm
//!
//! Time-domain energy envelope with an adaptive threshold:
//!
//! 1. Slice the input into non-overlapping frames of `frame_size`
//!    samples (default ≈ 5ms at 48 kHz). Most pick / drum attacks
//!    are sharper than this; the frame is small enough to localize
//!    them within ±5ms.
//! 2. Compute each frame's energy (mean square).
//! 3. Maintain a rolling **median** of the last
//!    `median_window_frames` frame energies (default ≈ 200ms). The
//!    median is the noise floor / local background level. Median
//!    (not mean) is robust to the impulse you're trying to detect.
//! 4. Declare an **onset** when:
//!    - current frame energy > `noise_floor × threshold_multiplier`,
//!    - current frame is greater than the previous frame (rising
//!      edge — rules out the *peak* of a sustained sound),
//!    - at least `min_spacing_samples` have passed since the last
//!      onset (debounce against single attacks splattering into
//!      multiple onsets).
//!
//! ## Why not spectral flux?
//!
//! Spectral flux is the standard in MIR papers and handles pitched
//! attacks well. But for our use cases (single-line picking, drum
//! hits, finger taps), energy envelope is simpler, allocation-free,
//! and works well — and it doesn't drag in an FFT dependency that the
//! audio crate doesn't otherwise need.
//!
//! If a future use case demands pitched-attack robustness (legato runs
//! where energy alone won't trigger), we can swap in a spectral-flux
//! detector behind the same `OnsetDetector` API.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Stream, StreamConfig};

use crate::engine::AudioError;

/// Default detection frame in samples. 256 samples ≈ 5.3 ms at
/// 48 kHz. Small enough to localize onsets; large enough to smooth
/// the per-sample noise.
pub const DEFAULT_FRAME_SIZE: usize = 256;

/// Default rolling-median window in frames. 40 frames ≈ 213 ms at
/// 48 kHz / 256-sample frames. Long enough to span a beat at fast
/// tempos without absorbing the onset itself.
pub const DEFAULT_MEDIAN_WINDOW: usize = 40;

/// Default threshold multiplier above the noise floor. 3.0 = onset
/// must be 3× the median frame energy. Tuned for typical mic + guitar
/// input; user-adjustable.
pub const DEFAULT_THRESHOLD_MULTIPLIER: f32 = 3.0;

/// Default debounce between onsets, in milliseconds. 50 ms = 1200 BPM
/// ceiling on detected hits; well above any musical tempo, low enough
/// not to merge consecutive sixteenth-notes at 240 BPM (62.5 ms apart).
pub const DEFAULT_MIN_SPACING_MS: f32 = 50.0;

/// Streaming onset detector. Hand it audio samples; it tells you when
/// transient onsets occurred. Sample-rate agnostic at construction
/// time so the same instance can be reused if the engine reconfigures.
#[derive(Clone, Debug)]
pub struct OnsetDetector {
    sample_rate_hz: f32,
    frame_size: usize,
    threshold_multiplier: f32,
    min_spacing_samples: u64,
    /// Rolling history of the last N frame energies, used to compute
    /// the noise-floor median. Sized at `median_window_frames`.
    energy_history: VecDeque<f32>,
    median_window_frames: usize,
    /// Carry-over samples between `feed` calls that didn't fill a full
    /// frame. Kept small (< frame_size).
    pending: Vec<f32>,
    /// Energy of the most recently completed frame, used for the
    /// rising-edge check on the next frame.
    last_frame_energy: f32,
    /// Total samples ever fed. Used to assign global timestamps to
    /// onsets so consumers can correlate against an external clock.
    samples_consumed: u64,
    /// Timestamp of the last-emitted onset, in samples. `None` = no
    /// onsets yet.
    last_onset_sample: Option<u64>,
}

impl OnsetDetector {
    /// Build a detector with sensible defaults for the given sample
    /// rate.
    pub fn new(sample_rate_hz: f32) -> Self {
        let min_spacing_samples =
            (sample_rate_hz * DEFAULT_MIN_SPACING_MS / 1000.0) as u64;
        Self {
            sample_rate_hz,
            frame_size: DEFAULT_FRAME_SIZE,
            threshold_multiplier: DEFAULT_THRESHOLD_MULTIPLIER,
            min_spacing_samples,
            energy_history: VecDeque::with_capacity(DEFAULT_MEDIAN_WINDOW),
            median_window_frames: DEFAULT_MEDIAN_WINDOW,
            pending: Vec::with_capacity(DEFAULT_FRAME_SIZE),
            last_frame_energy: 0.0,
            samples_consumed: 0,
            last_onset_sample: None,
        }
    }

    /// Override the detection threshold. Higher = more conservative
    /// (fewer false positives, more missed quiet hits).
    pub fn with_threshold(mut self, multiplier: f32) -> Self {
        self.threshold_multiplier = multiplier.max(1.0);
        self
    }

    /// Override the debounce interval in milliseconds.
    pub fn with_min_spacing_ms(mut self, ms: f32) -> Self {
        let safe_ms = ms.max(1.0);
        self.min_spacing_samples =
            (self.sample_rate_hz * safe_ms / 1000.0) as u64;
        self
    }

    /// Override the frame size in samples. Smaller = finer onset
    /// localization, more CPU per-second of audio.
    pub fn with_frame_size(mut self, frame_size: usize) -> Self {
        self.frame_size = frame_size.max(1);
        self.pending.reserve(self.frame_size);
        self
    }

    /// Reset all per-stream state. Call when starting fresh capture
    /// (e.g. user pressed Stop and then Play). Configuration is
    /// preserved.
    pub fn reset(&mut self) {
        self.energy_history.clear();
        self.pending.clear();
        self.last_frame_energy = 0.0;
        self.samples_consumed = 0;
        self.last_onset_sample = None;
    }

    /// Sample-rate the detector was configured for.
    pub fn sample_rate_hz(&self) -> f32 {
        self.sample_rate_hz
    }

    /// Total samples consumed across the lifetime of this detector.
    /// Useful as a clock reference for downstream consumers.
    pub fn samples_consumed(&self) -> u64 {
        self.samples_consumed
    }

    /// Feed a batch of samples. Returns the sample-indexed timestamps
    /// of any onsets detected within the batch. The timestamps are
    /// in the detector's own clock — see [`Self::samples_consumed`].
    pub fn feed(&mut self, samples: &[f32]) -> Vec<u64> {
        let mut onsets = Vec::new();

        // Pull a full frame whenever pending + new gives us one.
        let mut cursor = 0;
        loop {
            let needed = self.frame_size.saturating_sub(self.pending.len());
            if cursor + needed > samples.len() {
                // Not enough samples this call to complete a frame —
                // stash what we have and return.
                self.pending.extend_from_slice(&samples[cursor..]);
                self.samples_consumed += (samples.len() - cursor) as u64;
                return onsets;
            }
            // Form a frame from pending + a slice of new samples.
            let new_slice = &samples[cursor..cursor + needed];
            let energy = if self.pending.is_empty() {
                frame_energy(new_slice)
            } else {
                let combined_energy = frame_energy_two(&self.pending, new_slice);
                self.pending.clear();
                combined_energy
            };

            cursor += needed;

            // Update global sample clock for the just-completed frame
            // BEFORE deciding on an onset, so onset timestamps land at
            // the *end* of the frame they were detected in.
            self.samples_consumed += self.frame_size as u64;

            if let Some(onset_sample) = self.evaluate_frame(energy) {
                // Record the onset's timestamp so the debounce window
                // suppresses follow-up frames during the attack tail.
                self.last_onset_sample = Some(onset_sample);
                onsets.push(onset_sample);
            }

            self.update_history(energy);
            self.last_frame_energy = energy;
        }
    }

    /// Evaluate whether the just-computed frame energy constitutes an
    /// onset. Returns the sample-time of the onset if so.
    fn evaluate_frame(&self, energy: f32) -> Option<u64> {
        if self.energy_history.len() < self.median_window_frames / 4 {
            // Not enough warmup data to trust the noise floor.
            return None;
        }
        let noise_floor = median(&self.energy_history);
        let threshold = noise_floor * self.threshold_multiplier;
        if energy <= threshold {
            return None;
        }
        if energy <= self.last_frame_energy {
            // Not a rising edge — we're past the attack peak.
            return None;
        }
        // Onset timestamp = end of the current frame (we already
        // incremented samples_consumed for this frame).
        let onset_sample = self.samples_consumed;
        if let Some(last) = self.last_onset_sample {
            if onset_sample - last < self.min_spacing_samples {
                return None;
            }
        }
        Some(onset_sample)
    }

    fn update_history(&mut self, energy: f32) {
        if self.energy_history.len() >= self.median_window_frames {
            self.energy_history.pop_front();
        }
        self.energy_history.push_back(energy);
    }

    /// Manually record an onset (e.g. from `evaluate_frame`'s return
    /// value) so the debounce tracker knows about it. `feed` does this
    /// internally; exposed mostly for testing.
    #[doc(hidden)]
    pub fn record_onset(&mut self, sample: u64) {
        self.last_onset_sample = Some(sample);
    }
}

// =================================================================
// I/O layer — cpal-backed `OnsetEngine`
// =================================================================
//
// Wraps the pure DSP detector in an audio-input stream so the rest of
// the app can ask "what onsets has the user played?" without managing
// cpal directly. Mirrors the shape of `TunerEngine` / `SequencerEngine`
// (engine struct that owns the stream, plus a clone-able handle).

/// A single detected onset, timestamped both in sample-clock and
/// wall-clock. Wall-clock makes it easy to correlate against the
/// sequencer's `Instant`-based play time.
#[derive(Copy, Clone, Debug)]
pub struct DetectedOnset {
    /// Detector-clock sample index (since the engine's input stream
    /// started). Useful for inter-onset interval analysis.
    pub sample_index: u64,
    /// Instant the onset frame completed processing on the audio
    /// thread. Useful for syncing against a wall-clock metronome.
    pub at: Instant,
}

/// Snapshot of recent onsets, snapshotted by the UI thread.
#[derive(Clone, Debug, Default)]
pub struct OnsetSnapshot {
    /// Most recent onsets, oldest first. Bounded — see
    /// [`OnsetEngine::SNAPSHOT_CAPACITY`].
    pub recent: Vec<DetectedOnset>,
    /// Rolling input level (mean-square of the last analysis frame).
    /// Useful for a "listening" indicator in the UI.
    pub input_level: f32,
}

struct OnsetInternals {
    detector: OnsetDetector,
    snapshot: OnsetSnapshot,
    /// Most recent onsets, used both for the snapshot and for BPM
    /// estimation. Trimmed to `SNAPSHOT_CAPACITY` from the front.
    history: VecDeque<DetectedOnset>,
}

/// Thread-safe handle to control / inspect an `OnsetEngine`.
#[derive(Clone)]
pub struct OnsetHandle {
    inner: Arc<Mutex<OnsetInternals>>,
}

impl OnsetHandle {
    /// Read a snapshot of recent activity. Cheap clone.
    pub fn snapshot(&self) -> OnsetSnapshot {
        self.inner.lock().unwrap().snapshot.clone()
    }

    /// Drop all stored history. Use when starting a fresh
    /// listening session (e.g. tap-tempo session begins).
    pub fn reset(&self) {
        let mut s = self.inner.lock().unwrap();
        s.detector.reset();
        s.history.clear();
        s.snapshot = OnsetSnapshot::default();
    }

    /// Estimate BPM from the currently stored history, if there are
    /// enough onsets. See [`estimate_bpm`].
    pub fn current_bpm(&self) -> Option<f32> {
        let s = self.inner.lock().unwrap();
        let samples: Vec<u64> = s.history.iter().map(|o| o.sample_index).collect();
        estimate_bpm(&samples, s.detector.sample_rate_hz())
    }

    /// Overwrite the detector's threshold multiplier on the fly.
    pub fn set_threshold(&self, multiplier: f32) {
        let mut s = self.inner.lock().unwrap();
        s.detector.threshold_multiplier = multiplier.max(1.0);
    }

    pub fn threshold(&self) -> f32 {
        self.inner.lock().unwrap().detector.threshold_multiplier
    }
}

/// Audio-input engine that runs an [`OnsetDetector`] over a cpal
/// input stream and exposes detected onsets via an [`OnsetHandle`].
///
/// # Resource sharing
///
/// On most platforms cpal can open multiple input streams on the same
/// device, so running an `OnsetEngine` alongside a `TunerEngine` works
/// fine. On platforms where it doesn't, the right move is to share one
/// stream and fan samples out — a refactor we can do when we hit it.
pub struct OnsetEngine {
    handle: OnsetHandle,
    _stream: Stream,
}

impl OnsetEngine {
    /// How many recent onsets to retain in the snapshot history.
    /// 32 is plenty for tap-tempo estimation (you want at most ~8
    /// hits) and for a "last few hits" UI display.
    pub const SNAPSHOT_CAPACITY: usize = 32;

    pub fn new() -> Result<Self, AudioError> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or(AudioError::NoInputDevice)?;
        let supported = device
            .default_input_config()
            .map_err(AudioError::StreamConfig)?;
        let sample_format = supported.sample_format();
        let config: StreamConfig = supported.into();
        let channels = config.channels as usize;
        let sample_rate = config.sample_rate.0 as f32;

        let internals = Arc::new(Mutex::new(OnsetInternals {
            detector: OnsetDetector::new(sample_rate),
            snapshot: OnsetSnapshot::default(),
            history: VecDeque::with_capacity(Self::SNAPSHOT_CAPACITY),
        }));
        let internals_for_callback = Arc::clone(&internals);

        // Reusable scratch buffer downmixed to mono — sized roughly
        // for one cpal callback; will grow on demand.
        let mut mono_scratch: Vec<f32> = Vec::with_capacity(2048);

        let stream = match sample_format {
            cpal::SampleFormat::F32 => device
                .build_input_stream(
                    &config,
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        // Downmix to mono.
                        mono_scratch.clear();
                        mono_scratch.reserve(data.len() / channels);
                        for frame in data.chunks(channels) {
                            let mean =
                                frame.iter().copied().sum::<f32>() / channels as f32;
                            mono_scratch.push(mean);
                        }
                        // RMS for level display.
                        let level = if mono_scratch.is_empty() {
                            0.0
                        } else {
                            let sum_sq: f32 =
                                mono_scratch.iter().map(|s| s * s).sum();
                            (sum_sq / mono_scratch.len() as f32).sqrt()
                        };

                        let now = Instant::now();
                        let mut s = internals_for_callback.lock().unwrap();
                        let new_onsets = s.detector.feed(&mono_scratch);
                        for sample_index in new_onsets {
                            let detected = DetectedOnset {
                                sample_index,
                                at: now,
                            };
                            if s.history.len() >= Self::SNAPSHOT_CAPACITY {
                                s.history.pop_front();
                            }
                            s.history.push_back(detected);
                        }
                        s.snapshot.input_level = level;
                        s.snapshot.recent = s.history.iter().copied().collect();
                    },
                    |err| eprintln!("onset input error: {err}"),
                    None,
                )
                .map_err(AudioError::StreamBuild)?,
            other => return Err(AudioError::UnsupportedSampleFormat(other)),
        };
        stream.play().map_err(AudioError::StreamPlay)?;

        Ok(Self {
            handle: OnsetHandle { inner: internals },
            _stream: stream,
        })
    }

    pub fn handle(&self) -> OnsetHandle {
        self.handle.clone()
    }
}

/// Estimate BPM from a list of sample-indexed onset timestamps. Uses
/// the **median** inter-onset interval for robustness against missed
/// or extra hits.
///
/// Returns `None` if fewer than two onsets are provided or the median
/// interval is degenerate (zero / near-zero).
///
/// # Range
///
/// The result is clamped to `[40, 240]` BPM — the musically useful
/// window. Out-of-range estimates almost always mean the detector saw
/// double-hits or missed every other beat; clamping prevents the UI
/// from displaying garbage.
pub fn estimate_bpm(onsets: &[u64], sample_rate_hz: f32) -> Option<f32> {
    if onsets.len() < 2 {
        return None;
    }
    let mut intervals_secs: Vec<f32> = onsets
        .windows(2)
        .map(|w| (w[1].saturating_sub(w[0])) as f32 / sample_rate_hz)
        .collect();
    intervals_secs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median_secs = intervals_secs[intervals_secs.len() / 2];
    if median_secs < 0.001 {
        return None;
    }
    Some((60.0 / median_secs).clamp(40.0, 240.0))
}

/// Mean-square energy of a single frame.
fn frame_energy(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f32 = samples.iter().map(|&s| s * s).sum();
    sum_sq / samples.len() as f32
}

/// Mean-square energy spanning two slices (pending + new). Saves
/// allocating a combined buffer.
fn frame_energy_two(a: &[f32], b: &[f32]) -> f32 {
    let total = a.len() + b.len();
    if total == 0 {
        return 0.0;
    }
    let sum_sq: f32 = a.iter().map(|&s| s * s).sum::<f32>()
        + b.iter().map(|&s| s * s).sum::<f32>();
    sum_sq / total as f32
}

/// Median of a `VecDeque<f32>`. Allocates a small temp vec; called
/// once per frame which is fine for the workload.
fn median(values: &VecDeque<f32>) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted: Vec<f32> = values.iter().copied().collect();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        sorted[mid]
    } else {
        (sorted[mid - 1] + sorted[mid]) * 0.5
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cheap deterministic pseudo-random noise sample, in [-1, 1].
    /// Pure function of an index — lets warmup and the test signal
    /// share an identical noise floor so the detector's median is
    /// well-calibrated when the impulse arrives.
    fn noise(i: usize) -> f32 {
        ((i as f32 * 12.9898).sin() * 43_758.547).fract() - 0.5
    }

    /// Generate a synthetic signal of `len` samples: low-amplitude
    /// noise floor with sharp impulse "hits" + short decay tails at
    /// the given sample indices.
    fn synthetic_signal(len: usize, hit_at: &[usize]) -> Vec<f32> {
        let mut out: Vec<f32> = (0..len).map(|i| noise(i) * 0.01).collect();
        for &h in hit_at {
            if h < len {
                out[h] += 0.8;
            }
            // Short exponential decay tail (one frame's worth) so
            // each impulse spans a measurable energy bump.
            for j in 1..256 {
                if h + j < len {
                    out[h + j] += 0.8 * (-0.02 * j as f32).exp();
                }
            }
        }
        out
    }

    /// Feed enough background noise to populate the energy-history
    /// median. Uses the same noise generator as the test signal so the
    /// floor is calibrated when the test impulse arrives.
    fn warmup_detector(detector: &mut OnsetDetector, sample_rate: f32) {
        let n = (sample_rate * 0.3) as usize;
        let quiet: Vec<f32> = (0..n).map(|i| noise(i) * 0.01).collect();
        let _ = detector.feed(&quiet);
    }

    #[test]
    fn detector_warmup_returns_no_onsets() {
        let mut det = OnsetDetector::new(48_000.0);
        let quiet = vec![0.001_f32; 48_000];
        let onsets = det.feed(&quiet);
        // Quiet floor → no false positives.
        assert!(onsets.is_empty(), "got false-positive onsets: {onsets:?}");
    }

    #[test]
    fn detector_finds_isolated_impulse() {
        let sample_rate = 48_000.0;
        let mut det = OnsetDetector::new(sample_rate);
        warmup_detector(&mut det, sample_rate);
        let baseline = det.samples_consumed();

        // 1 second window with a hit at 0.5s.
        let signal = synthetic_signal(48_000, &[24_000]);
        let onsets = det.feed(&signal);

        assert_eq!(onsets.len(), 1, "expected 1 onset, got {}: {onsets:?}", onsets.len());

        // Onset should land somewhere near sample 24000 (offset from
        // baseline), within one frame of the actual hit.
        let hit_global = baseline + 24_000;
        let detected = onsets[0];
        let delta = detected.abs_diff(hit_global);
        assert!(
            delta <= DEFAULT_FRAME_SIZE as u64 * 2,
            "detected onset {detected} too far from hit {hit_global}; delta = {delta}"
        );
    }

    #[test]
    fn detector_finds_multiple_well_spaced_impulses() {
        let sample_rate = 48_000.0;
        let mut det = OnsetDetector::new(sample_rate);
        warmup_detector(&mut det, sample_rate);

        // 4 hits, 250ms apart = 120 BPM quarter notes.
        let hits = [12_000, 24_000, 36_000, 48_000];
        let signal = synthetic_signal(60_000, &hits);
        let onsets = det.feed(&signal);

        assert_eq!(onsets.len(), 4, "expected 4 onsets, got {}: {onsets:?}", onsets.len());
    }

    #[test]
    fn detector_debounces_too_close_hits() {
        let sample_rate = 48_000.0;
        // Aggressive debounce: 200ms between accepted hits.
        let mut det = OnsetDetector::new(sample_rate).with_min_spacing_ms(200.0);
        warmup_detector(&mut det, sample_rate);

        // Two hits 100ms apart (4800 samples) — second should be
        // suppressed by the debounce.
        let signal = synthetic_signal(48_000, &[10_000, 14_800]);
        let onsets = det.feed(&signal);

        assert_eq!(onsets.len(), 1, "debounce should reject second hit: {onsets:?}");
    }

    #[test]
    fn detector_reset_clears_history() {
        let sample_rate = 48_000.0;
        let mut det = OnsetDetector::new(sample_rate);
        warmup_detector(&mut det, sample_rate);
        let signal = synthetic_signal(24_000, &[12_000]);
        det.feed(&signal);

        det.reset();
        assert_eq!(det.samples_consumed(), 0);
        // After reset, the warmup needs to happen again before we
        // trust onsets. Just verify the next quiet feed yields zero
        // onsets (i.e. no stale state firing).
        let onsets = det.feed(&vec![0.001_f32; 48_000]);
        assert!(onsets.is_empty(), "post-reset quiet feed yielded onsets: {onsets:?}");
    }

    #[test]
    fn detector_handles_split_buffer_seam() {
        // The same impulse should be detected whether fed as one
        // batch or split across a buffer boundary that lands mid-frame.
        let sample_rate = 48_000.0;
        let signal = synthetic_signal(48_000, &[24_000]);

        let mut one_shot = OnsetDetector::new(sample_rate);
        warmup_detector(&mut one_shot, sample_rate);
        let single = one_shot.feed(&signal);

        let mut split = OnsetDetector::new(sample_rate);
        warmup_detector(&mut split, sample_rate);
        // Split mid-frame on purpose (DEFAULT_FRAME_SIZE = 256).
        let split_at = 24_137; // not a multiple of 256
        let first = split.feed(&signal[..split_at]);
        let second = split.feed(&signal[split_at..]);
        let combined: Vec<u64> = first.into_iter().chain(second).collect();

        assert_eq!(single.len(), 1);
        assert_eq!(combined.len(), 1);
        // Within one frame either way.
        let delta = single[0].abs_diff(combined[0]);
        assert!(delta <= DEFAULT_FRAME_SIZE as u64, "single={single:?} split={combined:?}");
    }

    // === Tempo estimation ===

    #[test]
    fn estimate_bpm_returns_none_below_two_onsets() {
        assert!(estimate_bpm(&[], 48_000.0).is_none());
        assert!(estimate_bpm(&[1000], 48_000.0).is_none());
    }

    #[test]
    fn estimate_bpm_120bpm_from_quarter_intervals() {
        // 120 BPM = 0.5s = 24000 samples at 48kHz between hits.
        let sample_rate = 48_000.0;
        let onsets: Vec<u64> = (0..8).map(|i| i as u64 * 24_000).collect();
        let bpm = estimate_bpm(&onsets, sample_rate).unwrap();
        assert!((bpm - 120.0).abs() < 0.5, "got {bpm}");
    }

    #[test]
    fn estimate_bpm_60bpm_from_one_second_intervals() {
        let sample_rate = 48_000.0;
        let onsets: Vec<u64> = (0..5).map(|i| i as u64 * 48_000).collect();
        let bpm = estimate_bpm(&onsets, sample_rate).unwrap();
        assert!((bpm - 60.0).abs() < 0.5, "got {bpm}");
    }

    #[test]
    fn estimate_bpm_clamps_excessive_tempo() {
        // 10ms between hits = 6000 BPM. Should clamp to 240.
        let sample_rate = 48_000.0;
        let onsets: Vec<u64> = (0..5).map(|i| i as u64 * 480).collect();
        let bpm = estimate_bpm(&onsets, sample_rate).unwrap();
        assert_eq!(bpm, 240.0);
    }

    #[test]
    fn estimate_bpm_clamps_too_slow_tempo() {
        // 3 seconds between hits = 20 BPM. Should clamp to 40.
        let sample_rate = 48_000.0;
        let onsets: Vec<u64> = (0..5).map(|i| i as u64 * 144_000).collect();
        let bpm = estimate_bpm(&onsets, sample_rate).unwrap();
        assert_eq!(bpm, 40.0);
    }

    #[test]
    fn estimate_bpm_median_rejects_outlier() {
        // Four onsets at 24000-sample intervals (120 BPM) plus one
        // gappy outlier in the middle. Median should still pick 120.
        let sample_rate = 48_000.0;
        let onsets: Vec<u64> = vec![
            0,
            24_000,
            48_000,
            48_000 + 96_000, // huge gap — simulates a missed hit
            48_000 + 96_000 + 24_000,
            48_000 + 96_000 + 48_000,
        ];
        let bpm = estimate_bpm(&onsets, sample_rate).unwrap();
        assert!((bpm - 120.0).abs() < 1.0, "got {bpm}");
    }
}
