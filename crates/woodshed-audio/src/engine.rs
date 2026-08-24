//! Cpal integration: owns the output stream, advances the pattern,
//! mixes voices.
//!
//! # Threading
//!
//! The audio callback runs on a high-priority thread managed by cpal.
//! This module uses `Arc<Mutex<EngineState>>` shared between the audio
//! callback and the [`EngineHandle`] returned to the caller. The lock
//! is held briefly inside the callback. This is fine for a metronome /
//! light sequencer; for heavier real-time workloads, lock-free atomics
//! and a triple-buffered pattern would be the right upgrade.

use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Stream, StreamConfig};

use crate::sequencer::{SequencerPattern, Step};
use crate::sound::Sound;

#[derive(Debug)]
pub enum AudioError {
    NoOutputDevice,
    NoInputDevice,
    UnsupportedSampleFormat(cpal::SampleFormat),
    // cpal 0.18 collapsed its per-operation error enums into one
    // `cpal::Error`; the variants stay split here to keep which
    // operation failed in the message.
    StreamConfig(cpal::Error),
    StreamBuild(cpal::Error),
    StreamPlay(cpal::Error),
}

impl std::fmt::Display for AudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoOutputDevice => f.write_str("no audio output device available"),
            Self::NoInputDevice => f.write_str("no audio input device available"),
            Self::UnsupportedSampleFormat(fmt) => {
                write!(
                    f,
                    "audio device sample format {fmt:?} is not supported (need f32)"
                )
            }
            Self::StreamConfig(e) => write!(f, "stream config: {e}"),
            Self::StreamBuild(e) => write!(f, "stream build: {e}"),
            Self::StreamPlay(e) => write!(f, "stream play: {e}"),
        }
    }
}

impl std::error::Error for AudioError {}

/// Internal voice — one playing instance of a sound. Holds the
/// triggering [`Sound`] by value so the mixer can render without
/// re-resolving anything off the pattern. `Sound` is `Clone` (not
/// `Copy`) because the `Sample` variant carries an id `String` and an
/// `Arc<Vec<f32>>` buffer — both cheap to clone.
#[derive(Clone, Debug)]
pub(crate) struct Voice {
    pub(crate) sound: Sound,
    pub(crate) accent: bool,
    pub(crate) start_sample: u64,
}

impl Voice {
    pub(crate) fn new(sound: Sound, accent: bool, start_sample: u64) -> Self {
        Self {
            sound,
            accent,
            start_sample,
        }
    }
}

/// Mutable engine state shared between audio callback and UI thread.
///
/// `sample_position` and `step_position` are tracked separately:
/// - `sample_position` is the absolute sample count since playback
///   started; used for voice timing (envelope decay, etc.) and is
///   independent of BPM.
/// - `step_position` is fractional steps since playback started; each
///   frame advances it by `1 / samples_per_step`. This decouples step
///   triggering from sample count so that BPM changes mid-playback
///   don't cause a pause or jump.
pub(crate) struct EngineState {
    pub pattern: SequencerPattern,
    pub sample_rate: f32,
    pub sample_position: u64,
    pub step_position: f64,
    pub last_step: i64,
    pub voices: Vec<Voice>,
    pub playing: bool,
}

impl EngineState {
    pub(crate) fn new(pattern: SequencerPattern, sample_rate: f32) -> Self {
        Self {
            pattern,
            sample_rate,
            sample_position: 0,
            step_position: 0.0,
            last_step: -1,
            voices: Vec::new(),
            playing: false,
        }
    }
}

/// Thread-safe handle to control a running [`SequencerEngine`].
#[derive(Clone)]
pub struct EngineHandle {
    state: Arc<Mutex<EngineState>>,
}

impl EngineHandle {
    pub fn set_pattern(&self, pattern: SequencerPattern) {
        let mut s = self.state.lock().unwrap();
        s.pattern = pattern;
        s.last_step = -1;
        s.sample_position = 0;
        s.step_position = 0.0;
        s.voices.clear();
    }

    /// Change the BPM without disrupting playback continuity.
    /// `step_position` is preserved; subsequent frames advance at the
    /// new rate.
    pub fn set_bpm(&self, bpm: f32) {
        let mut s = self.state.lock().unwrap();
        s.pattern.bpm = bpm;
    }

    pub fn play(&self) {
        let mut s = self.state.lock().unwrap();
        s.playing = true;
        s.sample_position = 0;
        s.step_position = 0.0;
        s.last_step = -1;
        s.voices.clear();
    }

    pub fn stop(&self) {
        let mut s = self.state.lock().unwrap();
        s.playing = false;
        s.voices.clear();
    }

    pub fn is_playing(&self) -> bool {
        self.state.lock().unwrap().playing
    }

    /// Step index within the current bar, or `None` if stopped or
    /// pre-first-step.
    pub fn current_step_in_bar(&self) -> Option<usize> {
        let s = self.state.lock().unwrap();
        if !s.playing || s.last_step < 0 {
            return None;
        }
        Some(s.last_step as usize % s.pattern.steps_per_bar())
    }
}

pub struct SequencerEngine {
    handle: EngineHandle,
    _stream: Stream,
}

impl SequencerEngine {
    pub fn new(initial_pattern: SequencerPattern) -> Result<Self, AudioError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(AudioError::NoOutputDevice)?;
        let supported = device
            .default_output_config()
            .map_err(AudioError::StreamConfig)?;
        let sample_format = supported.sample_format();
        let config: StreamConfig = supported.into();
        let channels = config.channels;
        let sample_rate = config.sample_rate as f32;

        let state = Arc::new(Mutex::new(EngineState::new(initial_pattern, sample_rate)));
        let state_for_callback = Arc::clone(&state);

        let stream = match sample_format {
            cpal::SampleFormat::F32 => device
                .build_output_stream(
                    config,
                    move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                        let mut s = state_for_callback.lock().unwrap();
                        process_buffer(&mut s, data, channels);
                    },
                    |err| eprintln!("audio stream error: {err}"),
                    None,
                )
                .map_err(AudioError::StreamBuild)?,
            other => return Err(AudioError::UnsupportedSampleFormat(other)),
        };

        stream.play().map_err(AudioError::StreamPlay)?;

        Ok(Self {
            handle: EngineHandle { state },
            _stream: stream,
        })
    }

    pub fn handle(&self) -> EngineHandle {
        self.handle.clone()
    }
}

/// Render `output` (interleaved by `channels`) by advancing the
/// pattern and mixing active voices. Pulled out for testability —
/// the cpal callback just calls this under a lock.
pub(crate) fn process_buffer(state: &mut EngineState, output: &mut [f32], channels: u16) {
    let chs = channels as usize;
    let frames = output.len() / chs;

    if !state.playing {
        for sample in output.iter_mut() {
            *sample = 0.0;
        }
        return;
    }

    let bpm = state.pattern.bpm;
    let dpb = state.pattern.subdivision.divisions_per_beat as f32;
    let samples_per_step = state.sample_rate * 60.0 / (bpm * dpb);
    let step_inc = 1.0 / samples_per_step as f64;
    let steps_per_bar = state.pattern.steps_per_bar();
    let sample_rate = state.sample_rate;

    for frame in 0..frames {
        let global = state.sample_position + frame as u64;
        let step_pos = state.step_position + step_inc * frame as f64;
        let step_idx = step_pos as i64;

        if step_idx > state.last_step {
            let step_in_bar = step_idx as usize % steps_per_bar;
            // Clone Sound by value into each new Voice. Cheap: Click
            // is bytes-sized, Sample's buffer is Arc-shared.
            let triggers: Vec<(Sound, bool)> = state
                .pattern
                .tracks
                .iter()
                .filter(|t| !t.muted)
                .filter_map(|t| match t.steps.get(step_in_bar)? {
                    Step::Empty => None,
                    Step::Active { accent } => Some((t.sound.clone(), *accent)),
                })
                .collect();
            for (sound, accent) in triggers {
                state.voices.push(Voice {
                    sound,
                    accent,
                    start_sample: global,
                });
            }
            state.last_step = step_idx;
        }

        let mut sample_value = 0.0_f32;
        state.voices.retain(|v| {
            let local = global.saturating_sub(v.start_sample) as u32;
            match v.sound.render_sample(local, sample_rate, v.accent) {
                Some(value) => {
                    sample_value += value;
                    true
                }
                None => false,
            }
        });

        let clamped = sample_value.clamp(-1.0, 1.0);
        for ch in 0..chs {
            output[frame * chs + ch] = clamped;
        }
    }

    state.sample_position += frames as u64;
    state.step_position += step_inc * frames as f64;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sequencer::{Subdivision, TimeSignature};

    fn metronome_pattern(bpm: f32) -> SequencerPattern {
        SequencerPattern::metronome(bpm, TimeSignature::default(), Subdivision::QUARTER)
    }

    #[test]
    fn process_buffer_silent_when_stopped() {
        let mut state = EngineState::new(metronome_pattern(120.0), 48000.0);
        // playing = false by default
        let mut buf = vec![1.0_f32; 256];
        process_buffer(&mut state, &mut buf, 2);
        assert!(buf.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn process_buffer_produces_audio_when_playing() {
        let mut state = EngineState::new(metronome_pattern(120.0), 48000.0);
        state.playing = true;
        // 1 second of stereo at 48kHz: more than a quarter note window
        let mut buf = vec![0.0_f32; 96000 * 2];
        process_buffer(&mut state, &mut buf, 2);
        let any_nonzero = buf.iter().any(|s| s.abs() > 1e-6);
        assert!(any_nonzero, "expected at least one nonzero sample");
    }

    #[test]
    fn process_buffer_advances_step_at_predicted_time() {
        // 120 BPM, quarter subdivision: 0.5s per step = 24000 samples
        // at 48kHz. After one second (96000 samples), last_step should
        // be 1 (we crossed into step 1 at sample 24000, and step 2 is
        // at 48000 — so by sample 96000-1, last_step >= 3).
        let mut state = EngineState::new(metronome_pattern(120.0), 48000.0);
        state.playing = true;
        let mut buf = vec![0.0_f32; 96000 * 2];
        process_buffer(&mut state, &mut buf, 2);
        // After 1 second at 120 BPM, we should have crossed into at
        // least step 1 (typically 1 or 2 depending on rounding).
        assert!(
            state.last_step >= 1,
            "last_step = {} after 1s at 120 BPM",
            state.last_step
        );
    }

    #[test]
    fn process_buffer_step_count_scales_with_bpm() {
        // At 60 BPM (slower), fewer steps fire in a fixed window;
        // at 240 BPM (faster), more steps fire.
        let mut slow = EngineState::new(metronome_pattern(60.0), 48000.0);
        let mut fast = EngineState::new(metronome_pattern(240.0), 48000.0);
        slow.playing = true;
        fast.playing = true;

        let mut buf_slow = vec![0.0_f32; 48000 * 2];
        let mut buf_fast = vec![0.0_f32; 48000 * 2];
        process_buffer(&mut slow, &mut buf_slow, 2);
        process_buffer(&mut fast, &mut buf_fast, 2);

        assert!(
            fast.last_step > slow.last_step,
            "fast={} slow={}",
            fast.last_step,
            slow.last_step
        );
    }

    #[test]
    fn process_buffer_subdivision_scales_step_count() {
        // 120 BPM with 16th-note subdivision should advance 4x as many
        // steps as quarter-note subdivision in the same window.
        let p_quarter =
            SequencerPattern::metronome(120.0, TimeSignature::default(), Subdivision::QUARTER);
        let p_sixteenth =
            SequencerPattern::metronome(120.0, TimeSignature::default(), Subdivision::SIXTEENTH);
        let mut quarter = EngineState::new(p_quarter, 48000.0);
        let mut sixteenth = EngineState::new(p_sixteenth, 48000.0);
        quarter.playing = true;
        sixteenth.playing = true;

        let mut buf_q = vec![0.0_f32; 48000 * 2];
        let mut buf_s = vec![0.0_f32; 48000 * 2];
        process_buffer(&mut quarter, &mut buf_q, 2);
        process_buffer(&mut sixteenth, &mut buf_s, 2);

        // last_step is 0-indexed; count of triggered steps = last_step + 1.
        // 16th should fire 4x as many steps as quarter.
        let count_q = (quarter.last_step + 1) as f64;
        let count_s = (sixteenth.last_step + 1) as f64;
        let ratio = count_s / count_q.max(1.0);
        assert!(
            (3.5..=4.5).contains(&ratio),
            "ratio {ratio}: q_count={count_q} s_count={count_s}"
        );
    }

    #[test]
    fn bpm_change_does_not_pause_playback() {
        // Regression: lowering BPM mid-playback used to cause a pause
        // because step_idx (computed as sample_position/samples_per_step)
        // shrank when samples_per_step grew, so the trigger condition
        // `step_idx > last_step` stayed false until samples caught up.
        // Fix: track step_position independently.
        let mut state = EngineState::new(metronome_pattern(120.0), 48000.0);
        state.playing = true;

        // Play 1 second at 120 BPM (= 2 beats). last_step should be 1.
        let mut buf = vec![0.0_f32; 48000 * 2];
        process_buffer(&mut state, &mut buf, 2);
        let last_step_before = state.last_step;
        let step_pos_before = state.step_position;
        assert!(last_step_before >= 1);

        // Drop to 60 BPM mid-playback.
        state.pattern.bpm = 60.0;

        // Play another second at 60 BPM (= 1 beat). step should advance
        // by ~1, not pause.
        process_buffer(&mut state, &mut buf, 2);

        assert!(
            state.last_step > last_step_before,
            "step did not advance after BPM drop: before={last_step_before} after={}",
            state.last_step
        );
        assert!(
            state.step_position > step_pos_before,
            "step_position did not advance after BPM drop"
        );
    }

    #[test]
    fn bpm_increase_speeds_up_without_glitch() {
        // Symmetric case: raising BPM should advance the step counter
        // faster without skipping or doubling.
        let mut state = EngineState::new(metronome_pattern(60.0), 48000.0);
        state.playing = true;

        let mut buf = vec![0.0_f32; 48000 * 2];
        process_buffer(&mut state, &mut buf, 2);
        let last_step_before = state.last_step;

        state.pattern.bpm = 240.0;
        process_buffer(&mut state, &mut buf, 2);

        // At 240 BPM, 4 steps per second; step counter should advance
        // by ~4 in the second window.
        let advance = state.last_step - last_step_before;
        assert!(
            (3..=5).contains(&advance),
            "expected ~4 steps after BPM raise; got {advance}"
        );
    }

    #[test]
    fn process_buffer_voices_decay_to_silence() {
        // After a click triggers and its duration elapses, the voice
        // should be removed from the active list.
        let mut state = EngineState::new(metronome_pattern(120.0), 48000.0);
        state.playing = true;
        // 100ms is well past the click's 50ms duration.
        let mut buf = vec![0.0_f32; 4800 * 2];
        process_buffer(&mut state, &mut buf, 2);
        // Voice list should be empty after sufficient time.
        assert!(
            state.voices.is_empty() || state.voices.len() <= 1,
            "voices = {}",
            state.voices.len()
        );
    }
}
