//! The desktop [`AudioBackend`]: `woodshed-audio`'s cpal engines behind
//! the core's seam (serval-host plan W0.1). The web host implements the
//! same trait over Web Audio / AudioWorklet.

use woodshed_audio::{
    Bar, ChordRef, InputEngine, InputEngineBuilder, SequencerEngine, SequencerPattern, Song,
    SongEngine, SongEngineHandle, Sound, Step, Subdivision, TimeSignature, Track, TunerHandle,
};
use woodshed_core::audio::{AudioBackend, TransportState, TunerReading};
use woodshed_core::song::SongBar;

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
    /// Song engine kept alive for its cpal stream; controlled through
    /// the handle.
    _song: Option<SongEngine>,
    song: Option<SongEngineHandle>,
    error: Option<String>,
}

/// Convert the core's neutral bar description into the audio model.
fn to_song(name: &str, bars: &[SongBar]) -> Song {
    let mut song = Song::new();
    song.name = name.to_string();
    song.bars = bars
        .iter()
        .map(|b| Bar {
            bpm: b.bpm,
            time_signature: TimeSignature::new(b.beats.max(1), 4),
            subdivision: Subdivision::QUARTER,
            chord_ref: (b.root_freq_hz > 0.0).then(|| ChordRef {
                formula_name: b.formula_name.clone(),
                root_freq_hz: b.root_freq_hz,
                pitches_hz: b.pitches_hz.clone(),
                label: b.chord_label.clone(),
            }),
            audio_buffer: None,
            label: b.label.clone(),
            length: b.length.max(1),
        })
        .collect();
    if song.bars.is_empty() {
        song.bars.push(Bar::default());
    }
    song
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
        let (song_engine, song) = match SongEngine::new(Song::new()) {
            Ok(engine) => {
                let handle = engine.handle();
                (Some(engine), Some(handle))
            }
            Err(e) => {
                let msg = format!("song engine: {e}");
                error = Some(match error {
                    Some(prev) => format!("{prev}; {msg}"),
                    None => msg,
                });
                (None, None)
            }
        };
        Self {
            _sequencer: sequencer,
            handle,
            _input: input,
            tuner,
            _song: song_engine,
            song,
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

    fn set_song(&mut self, name: &str, bars: &[SongBar]) {
        if let Some(song) = self.song.as_ref() {
            let was_playing = song.with_song(|s| s.playing);
            song.set_song(to_song(name, bars));
            if was_playing {
                song.play();
            }
        }
    }

    fn set_song_transport(&mut self, playing: bool) {
        let Some(song) = self.song.as_ref() else {
            return;
        };
        if song.with_song(|s| s.playing) != playing {
            if playing {
                song.play();
            } else {
                song.stop();
            }
        }
    }

    fn song_rewind(&mut self) {
        if let Some(song) = self.song.as_ref() {
            song.rewind();
        }
    }

    fn song_bar(&self) -> Option<usize> {
        self.song
            .as_ref()
            .map(|song| song.with_song(|s| s.cursor.bar_idx))
    }

    fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}
