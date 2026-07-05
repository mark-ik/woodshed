//! The desktop [`AudioBackend`]: `woodshed-audio`'s cpal engines behind
//! the core's seam (serval-host plan W0.1). The web host implements the
//! same trait over Web Audio / AudioWorklet.

use woodshed_audio::{
    InputEngine, InputEngineBuilder, SequencerEngine, SequencerPattern, Sound, Step,
    Subdivision, TimeSignature, Track, TunerHandle,
};
use woodshed_core::audio::{AudioBackend, TransportState, TunerReading};

/// A 4/4 quarter-note click at `bpm`, downbeat accented.
fn click_pattern(bpm: f32) -> SequencerPattern {
    let mut steps = vec![Step::Active { accent: true }];
    steps.extend(std::iter::repeat_n(Step::Active { accent: false }, 3));
    SequencerPattern {
        bpm,
        time_signature: TimeSignature::default(),
        subdivision: Subdivision::QUARTER,
        tracks: vec![Track {
            name: "click".to_string(),
            steps,
            sound: Sound::click(),
            muted: false,
        }],
    }
}

pub struct CpalBackend {
    /// Output engine kept alive for its cpal stream; controlled through
    /// the handle.
    _sequencer: Option<SequencerEngine>,
    handle: Option<woodshed_audio::EngineHandle>,
    /// Input engine kept alive for its cpal stream; the tuner handle
    /// reads analysis snapshots.
    _input: Option<InputEngine>,
    tuner: Option<TunerHandle>,
    error: Option<String>,
}

impl CpalBackend {
    /// Construct eagerly (streams open once at startup, like
    /// woodshed-xilem). Missing devices degrade: the field stays `None`
    /// and [`error`](AudioBackend::error) reports it.
    pub fn new() -> Self {
        let mut error: Option<String> = None;
        let (sequencer, handle) = match SequencerEngine::new(click_pattern(120.0)) {
            Ok(engine) => {
                let handle = engine.handle();
                (Some(engine), Some(handle))
            }
            Err(e) => {
                error = Some(format!("audio output: {e}"));
                (None, None)
            }
        };
        let (input, tuner) = {
            let (builder, tuner) = InputEngineBuilder::new().with_pitch();
            tuner.set_enabled(false);
            match builder.build() {
                Ok(engine) => (Some(engine), Some(tuner)),
                Err(e) => {
                    let msg = format!("audio input: {e}");
                    error = Some(match error {
                        Some(prev) => format!("{prev}; {msg}"),
                        None => msg,
                    });
                    (None, None)
                }
            }
        };
        Self {
            _sequencer: sequencer,
            handle,
            _input: input,
            tuner,
            error,
        }
    }
}

impl AudioBackend for CpalBackend {
    fn set_metronome(&mut self, transport: TransportState) {
        let Some(handle) = self.handle.as_ref() else {
            return;
        };
        handle.set_bpm(transport.bpm);
        if transport.playing != handle.is_playing() {
            if transport.playing {
                handle.play();
            } else {
                handle.stop();
            }
        }
    }

    fn set_tuner_enabled(&mut self, enabled: bool) {
        if let Some(tuner) = self.tuner.as_ref() {
            if tuner.is_enabled() != enabled {
                tuner.set_enabled(enabled);
            }
        }
    }

    fn tuner_reading(&self) -> Option<TunerReading> {
        let tuner = self.tuner.as_ref()?;
        if !tuner.is_enabled() {
            return None;
        }
        let snap = tuner.snapshot();
        let level = snap.input_level;
        snap.note.map(|n| TunerReading {
            note: n.name.to_string(),
            octave: n.octave,
            cents: n.cents_offset,
            in_tune: n.in_tune,
            level,
        })
    }

    fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}
