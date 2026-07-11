//! The desktop [`AudioBackend`]: `woodshed-audio`'s cpal engines behind
//! the core's seam (serval-host plan W0.1). The web host implements the
//! same trait over Web Audio / AudioWorklet.

use woodshed_audio::{
    Bar, CalibrationOutcome, CalibrationSession, ChordRef, InputEngine, InputEngineBuilder,
    LooperCaptureHandle, OnsetAnalyzer, OnsetHandle, PendingChange, SampleBuffer, SequencerEngine,
    SequencerPattern, Song, SongEngine, SongEngineHandle, Sound, Step, Subdivision, TimeSignature,
    Track, TunerHandle,
};
use woodshed_core::audio::{AudioBackend, CalibrationStatus, TransportState, TunerReading};
use woodshed_core::song::SongDoc;

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
    /// Input engine kept alive for its cpal stream; the tuner, onset,
    /// and looper-capture handles read/drive its analyzers.
    _input: Option<InputEngine>,
    tuner: Option<TunerHandle>,
    /// Onset detector handle — drives latency calibration + (later)
    /// timing feedback.
    onset: Option<OnsetHandle>,
    /// Looper-capture handle — wired to the song engine so song-mode
    /// recording drains input from it; enabled while recording is armed.
    capture: Option<LooperCaptureHandle>,
    /// The latency-calibration session driver.
    calib: CalibrationSession,
    /// Accepted input→output round-trip latency compensation (ms).
    latency_ms: Option<f32>,
    /// Song engine kept alive for its cpal stream; controlled through
    /// the handle.
    _song: Option<SongEngine>,
    song: Option<SongEngineHandle>,
    error: Option<String>,
}

/// Convert the core's neutral song document into the audio model.
fn to_song(doc: &SongDoc) -> Song {
    let mut song = Song::new();
    song.name = doc.name.clone();
    song.one_shot = doc.one_shot;
    song.click_enabled = doc.click;
    song.bars = doc
        .bars
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
        // One input engine, three analyzers: onset (calibration + timing),
        // looper-capture (song-mode record), and pitch (tuner). Each
        // carries its own enable flag, so the DSP only runs when its
        // feature is on.
        let (input, tuner, onset, capture) = {
            let onset_analyzer = OnsetAnalyzer::new();
            let onset_handle = onset_analyzer.handle();
            let (builder, capture_handle) = InputEngineBuilder::new()
                .with_analyzer(onset_analyzer)
                .with_looper_capture();
            let (builder, tuner) = builder.with_pitch();
            tuner.set_enabled(false);
            match builder.build() {
                Ok(engine) => (
                    Some(engine),
                    Some(tuner),
                    Some(onset_handle),
                    Some(capture_handle),
                ),
                Err(e) => {
                    let msg = format!("audio input: {e}");
                    error = Some(match error {
                        Some(prev) => format!("{prev}; {msg}"),
                        None => msg,
                    });
                    (None, None, None, None)
                }
            }
        };
        let (song_engine, song) = match SongEngine::new(Song::new()) {
            Ok(engine) => {
                let handle = engine.handle();
                // Wire the looper-capture ring so song-mode recording
                // (slice 15) can drain live input.
                if let Some(cap) = capture.as_ref() {
                    handle.set_capture(cap);
                }
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
            onset,
            capture,
            calib: CalibrationSession::new(),
            latency_ms: None,
            _song: song_engine,
            song,
            error,
        }
    }

    /// End an active calibration run: disable the onset detector and
    /// restore the app's click pattern (the session leaves the engine on
    /// the calibration metronome), stopped.
    fn end_calibration(&self) {
        if let Some(o) = self.onset.as_ref() {
            o.set_enabled(false);
        }
        if let Some(h) = self.handle.as_ref() {
            h.set_pattern(click_pattern(120.0));
            h.stop();
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

    fn set_song(&mut self, doc: &SongDoc) {
        if let Some(song) = self.song.as_ref() {
            let was_playing = song.with_song(|s| s.playing);
            // Preserve recorded loops across a structural update — a chord
            // or tempo edit shouldn't wipe what you've laid down. Carried
            // over by bar index (buffers are Arc-shared, so cheap); a
            // reorder/insert can misalign, which is acceptable for now.
            let loops: Vec<Option<SampleBuffer>> =
                song.with_song(|s| s.bars.iter().map(|b| b.audio_buffer.clone()).collect());
            song.set_song(to_song(doc));
            song.with_song(|s| {
                for (i, buf) in loops.into_iter().enumerate() {
                    if let (Some(bar), Some(buf)) = (s.bars.get_mut(i), buf) {
                        bar.audio_buffer = Some(buf);
                    }
                }
            });
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
        // Read-only accessor (no chord-cache resync) — polled every frame.
        self.song.as_ref().map(|song| song.cursor_bar())
    }

    fn preview_pitches(&mut self, pitches_hz: &[f32], duration_secs: f32, strum_ms: f32) {
        if let Some(song) = self.song.as_ref() {
            song.play_chord_now(pitches_hz, duration_secs, strum_ms);
        }
    }

    fn preview_note(&mut self, freq_hz: f32, duration_secs: f32) {
        if let Some(song) = self.song.as_ref() {
            song.play_note_now(freq_hz, duration_secs);
        }
    }

    fn calibration_start(&mut self) {
        let (Some(h), Some(o)) = (self.handle.clone(), self.onset.clone()) else {
            return;
        };
        self.calib.start(&h, &o);
    }

    fn calibration_poll(&mut self) -> CalibrationStatus {
        let (Some(h), Some(o)) = (self.handle.clone(), self.onset.clone()) else {
            return CalibrationStatus::Unavailable;
        };
        match self.calib.poll(&h, &o) {
            CalibrationOutcome::InProgress {
                clicks_fired,
                total_clicks,
            } => CalibrationStatus::Running {
                clicks_fired,
                total: total_clicks,
            },
            CalibrationOutcome::Success {
                latency,
                matched_pairs,
                total_clicks,
            } => {
                self.end_calibration();
                CalibrationStatus::Success {
                    latency_ms: latency.as_secs_f32() * 1000.0,
                    matched: matched_pairs,
                    total: total_clicks,
                }
            }
            CalibrationOutcome::InsufficientPairs {
                matched_pairs,
                total_clicks,
            } => {
                self.end_calibration();
                CalibrationStatus::Insufficient {
                    matched: matched_pairs,
                    total: total_clicks,
                }
            }
            CalibrationOutcome::EngineUnavailable => CalibrationStatus::Unavailable,
        }
    }

    fn calibration_cancel(&mut self) {
        if let Some(h) = self.handle.clone() {
            self.calib.cancel(&h);
        }
        self.end_calibration();
    }

    fn latency_ms(&self) -> Option<f32> {
        self.latency_ms
    }

    fn set_latency_ms(&mut self, ms: Option<f32>) {
        self.latency_ms = ms;
    }

    fn song_arm_record(&mut self, bar_idx: usize) {
        // Prime the capture ring so it's filling before the boundary hits.
        if let Some(cap) = self.capture.as_ref() {
            cap.set_enabled(true);
        }
        if let Some(song) = self.song.as_ref() {
            song.queue(PendingChange::StartRecording { bar_idx });
        }
    }

    fn song_stop_record(&mut self) {
        if let Some(song) = self.song.as_ref() {
            song.queue(PendingChange::StopRecording);
        }
        if let Some(cap) = self.capture.as_ref() {
            cap.set_enabled(false);
        }
    }

    fn song_clear_loop(&mut self, bar_idx: usize) {
        if let Some(song) = self.song.as_ref() {
            song.with_song(|s| {
                let _ = s.detach_buffer(bar_idx);
            });
        }
    }

    fn song_set_record_replace(&mut self, replace: bool) {
        if let Some(song) = self.song.as_ref() {
            song.with_song(|s| s.record_replace = replace);
        }
    }

    fn song_recording(&self) -> bool {
        self.song
            .as_ref()
            .map(|s| s.is_recording())
            .unwrap_or(false)
    }

    fn song_loop_bars(&self) -> Vec<bool> {
        self.song
            .as_ref()
            .map(|s| s.loop_flags())
            .unwrap_or_default()
    }

    fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}
