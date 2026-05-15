//! Audio input capture + pitch detection for the tuner.
//!
//! [`TunerEngine`] owns the input cpal stream and runs pitch detection
//! over a sliding window. The most recent detection is published to a
//! shared [`TunerSnapshot`] readable from any thread via [`TunerHandle`].
//!
//! Free tuner mode: no target tuning, no expectation of which note is
//! played. The detector returns the nearest 12-TET note plus cent
//! deviation, regardless.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Stream, StreamConfig};
use pitch_detector::core::NoteName as PdNoteName;
use pitch_detector::note::detect_note_in_range;
use pitch_detector::note::hinted::HintedNoteDetector;
use pitch_detector::pitch::{HannedFftDetector, PowerCepstrum};

use crate::engine::AudioError;

/// Analysis window size. 8192 samples at 48 kHz ≈ 170 ms gives a
/// frequency resolution of ~6 Hz per FFT bin, which is fine enough
/// to separate close low-register pitches (E2 = 82 Hz vs B1 = 62 Hz
/// are 20 Hz apart).
const ANALYSIS_WINDOW: usize = 8192;

/// Hop size between detections. Window slides forward by `HOP_SIZE`
/// samples for each detection, so successive analyses overlap by
/// `ANALYSIS_WINDOW - HOP_SIZE`. Smaller hops → faster update rate at
/// the cost of more CPU. 4096 gives ~12 detections/sec at 48 kHz,
/// matching the rate before the window was enlarged.
const HOP_SIZE: usize = 4096;

/// Default RMS amplitude below which detection is suppressed. The
/// active threshold is held in the shared state and can be live-updated
/// by the UI via [`TunerHandle::set_threshold`].
pub const DEFAULT_SILENCE_RMS_THRESHOLD: f64 = 0.001;

/// Frequency search range for pitch detection. Covers low bass B0
/// (≈31 Hz) up to roughly the high register, which encompasses every
/// stringed instrument we ship.
const MIN_DETECT_FREQ: f64 = 30.0;
const MAX_DETECT_FREQ: f64 = 2100.0;

/// Number of recent detections kept for smoothing. A note is published
/// only if a majority of the last `HISTORY_LEN` raw detections agree
/// on `(name, octave)`. Trades ~170 ms of additional latency for a
/// dramatic reduction in flicker between adjacent detections.
const HISTORY_LEN: usize = 3;

/// How many consecutive harmonic-suspect detections are tolerated
/// before we accept the new pitch as a real change. Without this,
/// a real pitch jump (e.g. user moves from E2 to B3) would be
/// permanently suppressed because we'd treat the new note as a
/// harmonic of the prior stable note.
const HARMONIC_SUPPRESS_GRACE: usize = 3;

#[derive(Clone, Debug)]
pub struct DetectedNote {
    /// Nearest 12-TET note name (sharps spelling — pitch-detector's
    /// convention).
    pub name: PdNoteName,
    pub octave: i32,
    pub note_freq_hz: f64,
    pub actual_freq_hz: f64,
    /// Distance in cents from the nearest 12-TET note. Range
    /// roughly [-50, +50].
    pub cents_offset: f64,
    pub in_tune: bool,
}

#[derive(Clone, Debug, Default)]
pub struct TunerSnapshot {
    pub note: Option<DetectedNote>,
    /// RMS amplitude of the most recent analysis window, in [0, 1].
    /// Useful for level metering and for the UI to render a "listening
    /// / silent / clipping" indicator.
    pub input_level: f32,
}

/// Which pitch-detection algorithm to use.
///
/// - [`Fft`](DetectorKind::Fft): picks the largest peak in the FFT
///   spectrum. Fast and accurate when the fundamental is the strongest
///   peak. Tends to confuse harmonics on instruments where a low
///   fundamental has been rolled off (e.g. laptop mics on guitar low E).
/// - [`Cepstrum`](DetectorKind::Cepstrum): infers the fundamental from
///   the *spacing* between harmonics. Robust against missing or weak
///   fundamentals; can be noisier on cents accuracy.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum DetectorKind {
    Fft,
    Cepstrum,
}

impl DetectorKind {
    pub const ALL: [Self; 2] = [Self::Fft, Self::Cepstrum];

    pub fn label(self) -> &'static str {
        match self {
            Self::Fft => "FFT",
            Self::Cepstrum => "Cepstrum",
        }
    }
}

impl core::fmt::Display for DetectorKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.label())
    }
}

struct TunerInternals {
    snapshot: TunerSnapshot,
    threshold_rms: f64,
    detector_kind: DetectorKind,
    /// Optional target note name. When set, the detector finds the
    /// strongest spectral peak that maps to this note name (any
    /// octave), bypassing harmonic confusion entirely. Use for tuning
    /// to a known string.
    target_hint: Option<PdNoteName>,
}

/// Cloneable handle. `snapshot()` reads the latest detection result;
/// `set_threshold` reconfigures the silence gate live.
#[derive(Clone)]
pub struct TunerHandle {
    inner: Arc<Mutex<TunerInternals>>,
}

impl TunerHandle {
    pub fn snapshot(&self) -> TunerSnapshot {
        self.inner.lock().unwrap().snapshot.clone()
    }

    /// Set the RMS amplitude threshold below which detection is
    /// suppressed. Effective on the next analysis window.
    pub fn set_threshold(&self, threshold_rms: f64) {
        self.inner.lock().unwrap().threshold_rms = threshold_rms;
    }

    pub fn threshold(&self) -> f64 {
        self.inner.lock().unwrap().threshold_rms
    }

    /// Switch the active pitch-detection algorithm. Effective on the
    /// next analysis window. Both detectors are kept warm so switching
    /// is instantaneous.
    pub fn set_detector_kind(&self, kind: DetectorKind) {
        self.inner.lock().unwrap().detector_kind = kind;
    }

    pub fn detector_kind(&self) -> DetectorKind {
        self.inner.lock().unwrap().detector_kind
    }

    /// Set the target note name hint, or `None` for free mode.
    /// When set, detection uses pitch-detector's hinted algorithm
    /// which finds the strongest spectral peak whose nearest 12-TET
    /// note matches the hint — avoiding harmonic confusion when the
    /// user knows what string they're tuning.
    pub fn set_target_hint(&self, hint: Option<DetectedNoteName>) {
        self.inner.lock().unwrap().target_hint = hint;
    }

    pub fn target_hint(&self) -> Option<DetectedNoteName> {
        self.inner.lock().unwrap().target_hint.clone()
    }
}

pub struct TunerEngine {
    handle: TunerHandle,
    _stream: Stream,
}

impl TunerEngine {
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
        let channels = config.channels;
        let sample_rate = config.sample_rate.0 as f64;

        let internals = Arc::new(Mutex::new(TunerInternals {
            snapshot: TunerSnapshot::default(),
            threshold_rms: DEFAULT_SILENCE_RMS_THRESHOLD,
            // FFT is the default — empirically more accurate than
            // cepstrum on typical built-in mic setups. The picker in
            // the UI lets users switch.
            detector_kind: DetectorKind::Fft,
            target_hint: None,
        }));
        let internals_for_callback = Arc::clone(&internals);

        let mut buffer: Vec<f64> = Vec::with_capacity(ANALYSIS_WINDOW * 2);
        // Both detectors kept warm so switching kind is instant.
        let mut fft_detector = HannedFftDetector::default();
        let mut cepstrum_detector = PowerCepstrum::default();
        let mut history: VecDeque<DetectedNote> = VecDeque::with_capacity(HISTORY_LEN);
        let mut harmonic_suppress_streak: usize = 0;
        let chs = channels as usize;

        let stream = match sample_format {
            cpal::SampleFormat::F32 => device
                .build_input_stream(
                    &config,
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        // Mono: average channels (cheap and avoids losing
                        // signal on stereo mics where one side may be silent).
                        for frame in data.chunks(chs) {
                            let mean = frame.iter().copied().sum::<f32>() / chs as f32;
                            buffer.push(mean as f64);
                        }
                        if buffer.len() >= ANALYSIS_WINDOW {
                            let start = buffer.len() - ANALYSIS_WINDOW;
                            let signal = &buffer[start..];
                            let rms = compute_rms(signal);

                            // Read live config from shared state.
                            let (threshold, kind, hint) = {
                                let s = internals_for_callback.lock().unwrap();
                                (
                                    s.threshold_rms,
                                    s.detector_kind,
                                    s.target_hint.clone(),
                                )
                            };

                            let raw = if rms >= threshold {
                                let range = MIN_DETECT_FREQ..MAX_DETECT_FREQ;
                                // pitch-detector v0.3 only implements
                                // HintedNoteDetector for HannedFftDetector,
                                // so when a target hint is set we route
                                // through FFT regardless of selected kind.
                                let result = match (kind, hint) {
                                    (_, Some(name)) => fft_detector
                                        .detect_note_with_hint_and_range(
                                            name,
                                            signal,
                                            sample_rate,
                                            Some(range),
                                        ),
                                    (DetectorKind::Fft, None) => detect_note_in_range(
                                        signal,
                                        &mut fft_detector,
                                        sample_rate,
                                        range,
                                    ),
                                    (DetectorKind::Cepstrum, None) => detect_note_in_range(
                                        signal,
                                        &mut cepstrum_detector,
                                        sample_rate,
                                        range,
                                    ),
                                };
                                result.map(|n| DetectedNote {
                                    name: n.note_name,
                                    octave: n.octave,
                                    note_freq_hz: n.note_freq,
                                    actual_freq_hz: n.actual_freq,
                                    cents_offset: n.cents_offset,
                                    in_tune: n.in_tune,
                                })
                            } else {
                                None
                            };

                            // Octave-error correction: if the new raw
                            // detection's frequency is a small-integer
                            // multiple/fraction of the prior stable
                            // frequency, the FFT detector likely
                            // latched onto a harmonic instead of the
                            // fundamental. Suppress the new detection
                            // and keep showing the prior stable note —
                            // unless this happens for several
                            // consecutive detections, in which case
                            // the user genuinely moved to a new pitch
                            // and we accept the change.
                            let prior_stable = compute_stable(&history);
                            let suspect = matches!(
                                (&raw, &prior_stable),
                                (Some(note), Some(stable))
                                    if is_harmonic_of(note.actual_freq_hz, stable.actual_freq_hz)
                            );

                            if suspect {
                                harmonic_suppress_streak += 1;
                                if harmonic_suppress_streak >= HARMONIC_SUPPRESS_GRACE {
                                    history.clear();
                                    if let Some(note) = raw {
                                        history.push_back(note);
                                    }
                                    harmonic_suppress_streak = 0;
                                }
                                // else: keep prior stable visible.
                            } else {
                                harmonic_suppress_streak = 0;
                                match raw {
                                    Some(note) => {
                                        if history.len() >= HISTORY_LEN {
                                            history.pop_front();
                                        }
                                        history.push_back(note);
                                    }
                                    None => history.clear(),
                                }
                            }

                            let stable = compute_stable(&history);

                            let mut s = internals_for_callback.lock().unwrap();
                            s.snapshot.input_level = rms as f32;
                            s.snapshot.note = stable;

                            // Drain by HOP_SIZE so successive analyses
                            // overlap. Buffer length stays ~ANALYSIS_WINDOW
                            // ready for the next detection.
                            let to_drop = HOP_SIZE.min(buffer.len());
                            buffer.drain(..to_drop);
                        }
                    },
                    |err| eprintln!("audio input error: {err}"),
                    None,
                )
                .map_err(AudioError::StreamBuild)?,
            other => return Err(AudioError::UnsupportedSampleFormat(other)),
        };

        stream.play().map_err(AudioError::StreamPlay)?;

        Ok(Self {
            handle: TunerHandle { inner: internals },
            _stream: stream,
        })
    }

    pub fn handle(&self) -> TunerHandle {
        self.handle.clone()
    }
}

// Re-export pitch-detector's NoteName so app code can convert it
// without taking pitch-detector as a direct dep.
pub use pitch_detector::core::NoteName as DetectedNoteName;

fn compute_rms(signal: &[f64]) -> f64 {
    if signal.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = signal.iter().map(|s| s * s).sum();
    (sum_sq / signal.len() as f64).sqrt()
}

/// True if `raw` looks like a harmonic-confusion artifact of `reference`:
/// approximately a 2×, 3×, or 4× multiple, or a 1/2, 1/3, 1/4 fraction.
/// Used to suppress octave/fifth errors common in FFT-based detection
/// of strings with prominent overtones (notably guitar low E).
fn is_harmonic_of(raw_freq: f64, reference_freq: f64) -> bool {
    if reference_freq < 1.0 || raw_freq < 1.0 {
        return false;
    }
    let ratio = raw_freq / reference_freq;
    let near = |target: f64| (ratio - target).abs() / target < 0.03;
    near(2.0) || near(3.0) || near(4.0) || near(0.5) || near(1.0 / 3.0) || near(0.25)
}

/// Return a stable detection from the recent history if a majority
/// of detections agree on `(note_name, octave)`. Returns the most
/// recent matching detection so cents/frequency stay current.
///
/// Includes a **fundamental bias**: if the majority consensus would
/// be a harmonic (2×, 3×, 4×) of any single other entry in history,
/// the lower entry is promoted as the true fundamental. This catches
/// the case where the FFT detector picks the 3rd harmonic of a low
/// guitar string on attack, even when the harmonic appears more often
/// in the buffer than the fundamental.
fn compute_stable(history: &VecDeque<DetectedNote>) -> Option<DetectedNote> {
    if history.len() < 2 {
        return None;
    }
    let mut best_count = 0;
    let mut best: Option<&DetectedNote> = None;
    for candidate in history {
        let count = history
            .iter()
            .filter(|n| n.name == candidate.name && n.octave == candidate.octave)
            .count();
        if count > best_count {
            best_count = count;
            best = Some(candidate);
        }
    }
    let required = history.len().div_ceil(2);
    if best_count < required {
        return None;
    }
    let consensus = best?;
    let consensus_freq = consensus.actual_freq_hz;

    // Fundamental-bias promotion. If any history entry is a clean
    // sub-multiple of the consensus (i.e. consensus is its harmonic),
    // promote that lower entry as the actual fundamental.
    if consensus_freq > 1.0 {
        for entry in history {
            let f = entry.actual_freq_hz;
            if f < 1.0 || f >= consensus_freq * 0.95 {
                continue;
            }
            if is_harmonic_of(consensus_freq, f) {
                return Some(entry.clone());
            }
        }
    }

    history
        .iter()
        .rev()
        .find(|n| n.name == consensus.name && n.octave == consensus.octave)
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rms_of_silence_is_zero() {
        let signal = vec![0.0_f64; 1024];
        assert_eq!(compute_rms(&signal), 0.0);
    }

    #[test]
    fn rms_of_constant_one_is_one() {
        let signal = vec![1.0_f64; 1024];
        assert!((compute_rms(&signal) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn rms_of_pure_sine_is_one_over_sqrt_two() {
        // A unit-amplitude sine has RMS = 1/sqrt(2) ≈ 0.707.
        let n = 4096;
        let signal: Vec<f64> = (0..n)
            .map(|i| (i as f64 * std::f64::consts::TAU / n as f64).sin())
            .collect();
        let rms = compute_rms(&signal);
        let expected = 1.0 / 2.0_f64.sqrt();
        assert!(
            (rms - expected).abs() < 1e-3,
            "rms = {rms}, expected ≈ {expected}"
        );
    }

    #[test]
    fn silence_threshold_gates_very_quiet_signal() {
        // A 0.0005-amplitude sine has RMS ≈ 0.00035, well below the
        // 0.005 threshold.
        let n = 4096;
        let amplitude = 0.0005;
        let signal: Vec<f64> = (0..n)
            .map(|i| amplitude * (i as f64 * std::f64::consts::TAU / n as f64).sin())
            .collect();
        let rms = compute_rms(&signal);
        assert!(
            rms < DEFAULT_SILENCE_RMS_THRESHOLD,
            "rms {rms} should be below threshold {DEFAULT_SILENCE_RMS_THRESHOLD}"
        );
    }

    fn note(name: PdNoteName, octave: i32) -> DetectedNote {
        DetectedNote {
            name,
            octave,
            note_freq_hz: 0.0,
            actual_freq_hz: 0.0,
            cents_offset: 0.0,
            in_tune: true,
        }
    }

    fn note_with_freq(name: PdNoteName, octave: i32, freq: f64) -> DetectedNote {
        DetectedNote {
            name,
            octave,
            note_freq_hz: freq,
            actual_freq_hz: freq,
            cents_offset: 0.0,
            in_tune: true,
        }
    }

    #[test]
    fn smoothing_returns_none_when_history_too_short() {
        let mut h = VecDeque::new();
        h.push_back(note(PdNoteName::A, 4));
        assert!(compute_stable(&h).is_none());
    }

    #[test]
    fn smoothing_returns_consensus_when_majority_agrees() {
        let mut h = VecDeque::new();
        h.push_back(note(PdNoteName::A, 4));
        h.push_back(note(PdNoteName::A, 4));
        h.push_back(note(PdNoteName::B, 4));
        let stable = compute_stable(&h).expect("majority should win");
        assert_eq!(stable.name, PdNoteName::A);
        assert_eq!(stable.octave, 4);
    }

    #[test]
    fn smoothing_returns_none_when_no_majority() {
        let mut h = VecDeque::new();
        h.push_back(note(PdNoteName::A, 4));
        h.push_back(note(PdNoteName::B, 4));
        h.push_back(note(PdNoteName::C, 5));
        assert!(compute_stable(&h).is_none());
    }

    #[test]
    fn third_harmonic_is_recognized() {
        // E2 = 82.41 Hz, B3 (3rd harmonic) = 247.94 Hz. Classic FFT
        // confusion case the suppression logic targets.
        assert!(is_harmonic_of(247.94, 82.41));
    }

    #[test]
    fn second_harmonic_is_recognized() {
        // A3 = 220, A4 = 440. 2× ratio.
        assert!(is_harmonic_of(440.0, 220.0));
    }

    #[test]
    fn sub_octave_is_recognized() {
        assert!(is_harmonic_of(220.0, 440.0)); // 0.5×
    }

    #[test]
    fn unrelated_pitches_are_not_harmonics() {
        // E2 = 82, A2 = 110. No simple ratio.
        assert!(!is_harmonic_of(110.0, 82.0));
    }

    #[test]
    fn same_pitch_is_not_a_harmonic() {
        // Identity (ratio 1) should not match any of {2, 3, 4, 1/2, 1/3, 1/4}.
        assert!(!is_harmonic_of(440.0, 440.0));
    }

    #[test]
    fn smoothing_uses_most_recent_matching_detection() {
        // Different cents on each A4 detection — make sure we get the
        // latest one's data, not the earliest.
        let mut h = VecDeque::new();
        let mut a1 = note(PdNoteName::A, 4);
        a1.cents_offset = -10.0;
        let mut a2 = note(PdNoteName::A, 4);
        a2.cents_offset = 5.0;
        h.push_back(a1);
        h.push_back(note(PdNoteName::B, 4));
        h.push_back(a2);
        let stable = compute_stable(&h).expect("A4 has majority");
        assert_eq!(stable.cents_offset, 5.0);
    }

    #[test]
    fn fundamental_bias_promotes_lower_entry_when_consensus_is_3rd_harmonic() {
        // Classic guitar low-E case: detector picks B3 (247 Hz, 3rd
        // harmonic) two times out of three, with one E2 (82 Hz)
        // detection in between. Expected: promoted to E2.
        let mut h = VecDeque::new();
        h.push_back(note_with_freq(PdNoteName::B, 3, 247.94));
        h.push_back(note_with_freq(PdNoteName::E, 2, 82.41));
        h.push_back(note_with_freq(PdNoteName::B, 3, 247.94));
        let stable = compute_stable(&h).expect("majority B with E sub-multiple");
        assert_eq!(stable.name, PdNoteName::E);
        assert_eq!(stable.octave, 2);
    }

    #[test]
    fn fundamental_bias_does_not_promote_when_no_sub_multiple_present() {
        // [A4, B4, A4]: A4 is consensus; B4 is unrelated to A4
        // (not a harmonic ratio). Expected: A4.
        let mut h = VecDeque::new();
        h.push_back(note_with_freq(PdNoteName::A, 4, 440.0));
        h.push_back(note_with_freq(PdNoteName::B, 4, 493.88));
        h.push_back(note_with_freq(PdNoteName::A, 4, 440.0));
        let stable = compute_stable(&h).expect("A majority");
        assert_eq!(stable.name, PdNoteName::A);
        assert_eq!(stable.octave, 4);
    }

    #[test]
    fn fundamental_bias_does_not_demote_when_consensus_is_already_lowest() {
        // [E2, E2, B3]: E2 is consensus AND the lowest. Don't promote
        // anything else.
        let mut h = VecDeque::new();
        h.push_back(note_with_freq(PdNoteName::E, 2, 82.41));
        h.push_back(note_with_freq(PdNoteName::E, 2, 82.41));
        h.push_back(note_with_freq(PdNoteName::B, 3, 247.94));
        let stable = compute_stable(&h).expect("E2 majority");
        assert_eq!(stable.name, PdNoteName::E);
        assert_eq!(stable.octave, 2);
    }

    #[test]
    fn quiet_playing_passes_threshold() {
        // A 0.05-amplitude sine (quiet but real signal) has RMS
        // ≈ 0.035, comfortably above 0.005.
        let n = 4096;
        let amplitude = 0.05;
        let signal: Vec<f64> = (0..n)
            .map(|i| amplitude * (i as f64 * std::f64::consts::TAU / n as f64).sin())
            .collect();
        let rms = compute_rms(&signal);
        assert!(
            rms >= DEFAULT_SILENCE_RMS_THRESHOLD,
            "rms {rms} should pass threshold {DEFAULT_SILENCE_RMS_THRESHOLD}"
        );
    }
}
