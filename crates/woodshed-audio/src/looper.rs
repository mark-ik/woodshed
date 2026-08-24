//! Loop recording and overdub.
//!
//! A [`Looper`] is a bar-length circular audio buffer. While
//! `recording` is true, samples written via [`Looper::feed`] are
//! captured. While `playing` is true, samples returned by
//! [`Looper::tick`] play back. Both flags can be true simultaneously
//! — that's overdub: new audio mixes into whatever's already there.
//!
//! # What this layer does and doesn't do
//!
//! **Pure data.** No cpal, no threads, no audio device. The looper
//! exposes a single-sample-at-a-time API that callers (typically an
//! audio callback on one side and a level-tracking UI thread on the
//! other) drive in lockstep with the master clock.
//!
//! **Bar-aligned.** Capture and playback share the same bar phase.
//! The looper knows its bar length in samples; calls to [`Looper::tick`]
//! advance an internal write-cursor that wraps every bar. The cursor
//! lives at the *output* sample rate; whoever owns the master clock
//! (the sequencer engine) is responsible for keeping that rate
//! consistent with the bar length passed at construction.
//!
//! **Single bar today.** First-iteration scope: a one-bar loop, which
//! is the highest-value case for guitar-practice "play against
//! yourself." Multi-bar loops fall out by sizing the buffer to N bars
//! and is a feature, not a redesign — left for a follow-up.

/// Maximum sustainable bar length, in samples. Caps memory at ~1.6 MB
/// (10 seconds at 48 kHz, mono f32) — comfortably more than 10 BPM
/// would need but small enough that allocating it doesn't blink.
pub const MAX_BAR_SAMPLES: usize = 480_000;

/// A single-bar audio looper.
#[derive(Clone, Debug)]
pub struct Looper {
    /// Recorded audio. Length is exactly `bar_samples`; positions are
    /// indexed modulo the bar length.
    buffer: Vec<f32>,
    /// Write/read cursor. Advances by 1 per [`Self::tick`]. Wraps at
    /// `bar_samples`.
    cursor: usize,
    /// Total length of one bar in samples.
    bar_samples: usize,
    /// Capturing input on this bar? When transitioning from false to
    /// true at a bar boundary, the buffer is cleared (a fresh loop).
    /// Overdub mode (`recording && playing`) preserves prior content.
    recording: bool,
    /// Emitting output on this tick?
    playing: bool,
    /// Stored separately so we can implement "start recording at the
    /// next bar boundary" — the toggle takes effect when the cursor
    /// next wraps to zero.
    pending_record: Option<bool>,
    /// Has the buffer been filled at least once? Used to decide
    /// between "fresh record" (clear on start) and "overdub" (keep
    /// existing content).
    has_content: bool,
}

impl Looper {
    /// Construct a looper for a bar of `bar_samples` audio frames.
    /// Clamps to [`MAX_BAR_SAMPLES`].
    pub fn new(bar_samples: usize) -> Self {
        let n = bar_samples.clamp(1, MAX_BAR_SAMPLES);
        Self {
            buffer: vec![0.0; n],
            cursor: 0,
            bar_samples: n,
            recording: false,
            playing: false,
            pending_record: None,
            has_content: false,
        }
    }

    /// Bar length the looper was configured for, in samples.
    pub fn bar_samples(&self) -> usize {
        self.bar_samples
    }

    /// Whether the looper holds any recorded content.
    pub fn has_content(&self) -> bool {
        self.has_content
    }

    /// Whether the looper is currently emitting playback samples.
    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// Whether the looper is currently capturing input samples.
    pub fn is_recording(&self) -> bool {
        self.recording
    }

    /// Schedule a record-state change to take effect at the next bar
    /// boundary. Use this for UI-friendly "click Record now, capture
    /// starts on beat 1 of the next bar" behavior.
    pub fn arm_record(&mut self, on: bool) {
        self.pending_record = Some(on);
    }

    /// Immediately set the recording state without waiting for a bar
    /// boundary. Useful in tests; the UI typically prefers [`Self::arm_record`].
    pub fn set_recording(&mut self, on: bool) {
        if on && !self.recording {
            // Fresh record (not overdub) — wipe.
            if !self.playing {
                self.buffer.fill(0.0);
                self.has_content = false;
            }
        }
        self.recording = on;
    }

    pub fn set_playing(&mut self, on: bool) {
        self.playing = on;
    }

    /// Clear the recorded buffer and reset cursor + state.
    pub fn clear(&mut self) {
        self.buffer.fill(0.0);
        self.cursor = 0;
        self.recording = false;
        self.playing = false;
        self.pending_record = None;
        self.has_content = false;
    }

    /// Process one frame.
    ///
    /// - `input` is the live audio sample from the input stream
    ///   (typically the user's mic), at the same time as this frame.
    /// - Returns the mix to add to the engine's output for this
    ///   frame: the recorded loop, scaled by `playback_gain`, or 0
    ///   when not playing.
    ///
    /// Recording semantics:
    /// - If `recording && playing`: overdub. The incoming sample is
    ///   mixed *into* the existing buffer (sample is added, not
    ///   overwritten).
    /// - If `recording && !playing`: fresh record. Incoming sample
    ///   replaces the buffer at the cursor.
    /// - If `!recording`: buffer untouched.
    pub fn tick(&mut self, input: f32, playback_gain: f32) -> f32 {
        // Bar-boundary actions: apply any pending record toggle at
        // cursor=0.
        if self.cursor == 0 {
            if let Some(on) = self.pending_record.take() {
                self.set_recording(on);
            }
        }

        // Capture.
        if self.recording {
            let i = self.cursor;
            if self.playing {
                // Overdub: sum existing + new, soft-clip so successive
                // overdubs don't immediately distort.
                self.buffer[i] = soft_clip(self.buffer[i] + input);
            } else {
                self.buffer[i] = input;
            }
            self.has_content = true;
        }

        // Playback.
        let out = if self.playing {
            self.buffer[self.cursor] * playback_gain
        } else {
            0.0
        };

        // Advance cursor with wrap.
        self.cursor += 1;
        if self.cursor >= self.bar_samples {
            self.cursor = 0;
        }

        out
    }

    /// Current cursor position (sample within the bar).
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Peak (max abs) value across the recorded buffer. Cheap to call
    /// once per UI frame for a "loop fullness" meter.
    pub fn peak_amplitude(&self) -> f32 {
        self.buffer.iter().map(|s| s.abs()).fold(0.0_f32, f32::max)
    }
}

/// Soft-knee clipper. Approximates tanh — keeps small signals linear
/// while smoothly compressing larger ones. Cheaper than a real tanh.
#[inline]
fn soft_clip(x: f32) -> f32 {
    if x > 1.0 {
        1.0 - (-(x - 1.0)).exp() * 0.5
    } else if x < -1.0 {
        -1.0 + (x + 1.0).exp() * 0.5
    } else {
        x
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_looper_is_quiet_and_empty() {
        let mut lp = Looper::new(1000);
        assert!(!lp.has_content());
        assert!(!lp.is_recording());
        assert!(!lp.is_playing());
        // Quiet input, not playing → quiet output.
        let out = lp.tick(0.0, 1.0);
        assert_eq!(out, 0.0);
    }

    #[test]
    fn recording_captures_input_at_cursor() {
        let mut lp = Looper::new(10);
        lp.set_recording(true);
        lp.tick(0.3, 1.0);
        lp.tick(0.4, 1.0);
        // After 2 ticks, cursor = 2. Buffer[0..2] should be [0.3, 0.4].
        assert_eq!(lp.cursor(), 2);
        assert!(lp.has_content());
        assert!((lp.peak_amplitude() - 0.4).abs() < 1e-6);
    }

    #[test]
    fn playback_returns_recorded_audio() {
        let mut lp = Looper::new(4);
        lp.set_recording(true);
        for &s in &[0.1_f32, 0.2, 0.3, 0.4] {
            lp.tick(s, 1.0);
        }
        // Cursor wraps to 0 after 4 ticks.
        lp.set_recording(false);
        lp.set_playing(true);

        let out: Vec<f32> = (0..4).map(|_| lp.tick(0.0, 1.0)).collect();
        // Output should match the original recording, possibly with
        // soft-clipping (none of these values exceed 1.0, so they
        // pass through).
        for (got, want) in out.iter().zip([0.1, 0.2, 0.3, 0.4]) {
            assert!((got - want).abs() < 1e-6, "got {got}, want {want}");
        }
    }

    #[test]
    fn playback_loops_at_bar_boundary() {
        let mut lp = Looper::new(2);
        lp.set_recording(true);
        lp.tick(0.5, 1.0);
        lp.tick(0.7, 1.0);
        lp.set_recording(false);
        lp.set_playing(true);

        // Iterate 6 samples: should be [0.5, 0.7, 0.5, 0.7, 0.5, 0.7].
        let out: Vec<f32> = (0..6).map(|_| lp.tick(0.0, 1.0)).collect();
        for (i, &v) in out.iter().enumerate() {
            let expected = if i % 2 == 0 { 0.5 } else { 0.7 };
            assert!(
                (v - expected).abs() < 1e-6,
                "sample {i}: got {v}, want {expected}"
            );
        }
    }

    #[test]
    fn overdub_sums_into_existing_buffer() {
        let mut lp = Looper::new(4);
        // First pass: record a quiet pulse.
        lp.set_recording(true);
        for &s in &[0.1_f32, 0.0, 0.0, 0.0] {
            lp.tick(s, 1.0);
        }
        // Switch to playback (cursor wraps to 0).
        lp.set_recording(false);
        lp.set_playing(true);

        // Second pass: overdub another pulse at index 2.
        lp.set_recording(true); // recording + playing = overdub
        for &s in &[0.0_f32, 0.0, 0.2, 0.0] {
            lp.tick(s, 1.0);
        }

        // Now the buffer should contain ~[0.1, 0, 0.2, 0]. Verify via
        // peak.
        let peak = lp.peak_amplitude();
        assert!(
            (0.18..=0.22).contains(&peak),
            "peak {peak} should be near 0.2"
        );
    }

    #[test]
    fn fresh_record_after_clear_starts_silent() {
        let mut lp = Looper::new(4);
        lp.set_recording(true);
        for &s in &[0.5_f32, 0.5, 0.5, 0.5] {
            lp.tick(s, 1.0);
        }
        assert!(lp.has_content());

        lp.clear();
        assert!(!lp.has_content());
        assert_eq!(lp.cursor(), 0);
        assert!(!lp.is_recording());
        assert!(!lp.is_playing());
        assert_eq!(lp.peak_amplitude(), 0.0);
    }

    #[test]
    fn playback_gain_scales_output() {
        let mut lp = Looper::new(2);
        lp.set_recording(true);
        lp.tick(0.5, 1.0);
        lp.tick(0.5, 1.0);
        lp.set_recording(false);
        lp.set_playing(true);

        let full = lp.tick(0.0, 1.0);
        let half = lp.tick(0.0, 0.5);
        assert!((full - 0.5).abs() < 1e-6);
        assert!((half - 0.25).abs() < 1e-6);
    }

    #[test]
    fn arm_record_takes_effect_at_bar_boundary() {
        let mut lp = Looper::new(3);
        lp.set_playing(true);
        // Arm recording. At cursor 0 we tick once → cursor 1, but
        // pending was consumed at the start of that tick. So the
        // first tick already had recording on. Verify by checking
        // that the input got captured.
        lp.arm_record(true);
        lp.tick(0.5, 1.0); // cursor: 0 → 1, pending consumed at cursor 0
        lp.tick(0.0, 1.0); // cursor: 1 → 2
        lp.tick(0.0, 1.0); // cursor: 2 → 0
        assert!(lp.is_recording());
        assert!(lp.has_content());
    }

    #[test]
    fn arm_record_off_at_bar_boundary() {
        let mut lp = Looper::new(2);
        lp.set_recording(true);
        lp.tick(0.5, 1.0); // cursor 0 → 1
        lp.tick(0.5, 1.0); // cursor 1 → 0 (bar boundary)
                           // Arm "off" now — will take effect at the next cursor=0.
        lp.arm_record(false);
        // First tick after arming: cursor=0, pending consumed → recording off.
        lp.tick(0.9, 1.0); // should NOT capture 0.9
                           // Buffer at cursor 0 was 0.5 from before; check it didn't get overwritten.
                           // We can check via the peak (no 0.9 anywhere).
        assert!(lp.peak_amplitude() < 0.55);
    }

    #[test]
    fn soft_clip_passes_small_signals_unchanged() {
        assert!((soft_clip(0.5) - 0.5).abs() < 1e-6);
        assert!((soft_clip(-0.5) - (-0.5)).abs() < 1e-6);
        assert!((soft_clip(0.0) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn soft_clip_compresses_large_signals_below_unity() {
        assert!(soft_clip(2.0) <= 1.0);
        assert!(soft_clip(-2.0) >= -1.0);
        assert!(soft_clip(10.0) <= 1.0);
    }
}
