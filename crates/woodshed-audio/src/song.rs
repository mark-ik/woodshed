//! Song-mode arrangement: a multi-bar sequence with per-bar settings.
//!
//! A [`Song`] is an ordered list of [`Bar`]s. Each bar carries its own
//! BPM, time signature, optional recorded audio, and optional chord
//! reference. Songs are the upgrade path from the single-bar
//! [`crate::Looper`]: a one-bar song with a recorded audio buffer *is*
//! a simple looper.
//!
//! # What this module is, what it isn't
//!
//! **Is**: a pure-data model with operations (add, remove, copy/paste,
//! seek, advance) and tests. Bar-by-bar audio rendering, integration
//! with the [`crate::SequencerEngine`] mixer, and the UI gestures
//! live elsewhere — this is the foundation those layers build on.
//!
//! **Isn't**: a multi-track DAW. Each bar holds at most one audio
//! buffer (the practice-recording layer). Want chords too? Use the
//! `chord_ref` field — the engine resolves it via
//! [`crate::chord_audio`] and mixes alongside the audio buffer.
//!
//! # Pending-change model (SR-16 pattern)
//!
//! Mid-song settings changes are queued via [`Song::queue`] and
//! applied at the next bar boundary by [`Song::on_bar_boundary`].
//! This matches how the Alesis SR-16 drum machine handles tempo and
//! pattern changes — you press the button now, the change lands when
//! musically appropriate.
//!
//! # Serde
//!
//! [`Song`], [`Bar`], and [`ChordRef`] all serialize. Audio buffers
//! follow the same convention as [`crate::Sound::Sample`]: serialized
//! songs carry buffer metadata (sample rate, length) but the PCM
//! itself is `#[serde(skip)]` and rehydrated by a runtime asset bank
//! or — for songs saved as standalone files — embedded in a separate
//! companion `.wav` and re-attached on load.

use serde::{Deserialize, Serialize};

use crate::sequencer::{Subdivision, TimeSignature};
use crate::sound::SampleBuffer;

/// Reference to a chord by formula name + root frequency. Audio
/// crates don't depend on the woodshedding theory crate, so callers
/// translate (chord formula, root pitch) → (formula_name string,
/// root frequency in Hz) before stashing in a [`Bar`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChordRef {
    /// Catalog key for the chord formula (e.g. `"Major"`, `"Dominant 7"`).
    pub formula_name: String,
    /// Root pitch as a frequency in Hz. The renderer adds intervals
    /// on top to produce the chord's pitches.
    pub root_freq_hz: f32,
    /// Pre-computed chord-tone frequencies, oldest-to-highest. Stored
    /// so the audio layer doesn't need theory dependencies. Empty =
    /// only the root sounds.
    pub pitches_hz: Vec<f32>,
    /// User-facing label for display ("Em7", "Cmaj9"). Cosmetic; the
    /// engine doesn't read this.
    pub label: String,
}

/// One bar in a [`Song`]. Holds the bar's own tempo + time sig (so
/// per-bar tempo changes are first-class), an optional recorded audio
/// buffer, and an optional chord reference for chord-card display +
/// audio.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Bar {
    /// Tempo for this bar, in BPM.
    pub bpm: f32,
    pub time_signature: TimeSignature,
    pub subdivision: Subdivision,
    /// Optional one-shot chord that plays at the start of this bar.
    /// `None` = silent (just the click + any recorded audio).
    pub chord_ref: Option<ChordRef>,
    /// Recorded audio for this bar (the practice-loop layer). Skipped
    /// from serde — see module docs.
    #[serde(skip)]
    pub audio_buffer: Option<SampleBuffer>,
    /// Cosmetic label ("Verse 1", "Bridge"). Optional. Doubles as the
    /// section marker in the timeline UI: a non-empty label opens a
    /// section band that runs until the next labeled bar.
    pub label: String,
    /// How many measures this bar spans (≥ 1). A length-4 bar is one
    /// timeline cell holding four measures of the same tempo / meter /
    /// chord — the looper-friendly "this block repeats for N bars"
    /// unit. Playback walks all `length` measures before advancing to
    /// the next [`Bar`].
    #[serde(default = "default_bar_length")]
    pub length: u8,
}

/// Serde default for [`Bar::length`] — old saves predate the field and
/// must read back as single-measure bars, not zero-length ones.
fn default_bar_length() -> u8 {
    1
}

impl Default for Bar {
    fn default() -> Self {
        Self {
            bpm: 120.0,
            time_signature: TimeSignature::default(),
            subdivision: Subdivision::QUARTER,
            chord_ref: None,
            audio_buffer: None,
            label: String::new(),
            length: 1,
        }
    }
}

impl Bar {
    /// Length of this bar in wall-clock seconds — the full block,
    /// i.e. one measure's duration times [`Bar::length`].
    pub fn duration_secs(&self) -> f32 {
        let num = self.time_signature.numerator.max(1) as f32;
        let secs_per_beat = 60.0 / self.bpm.max(1.0);
        let measures = self.length.max(1) as f32;
        num * secs_per_beat * measures
    }

    /// Length of this bar in samples at the given output rate.
    pub fn duration_samples(&self, sample_rate_hz: f32) -> u64 {
        (self.duration_secs() * sample_rate_hz).round() as u64
    }

    /// Construct with just BPM + time-signature (the common case for
    /// "add a new bar matching the current song settings").
    pub fn with_tempo(bpm: f32, time_signature: TimeSignature) -> Self {
        Self {
            bpm,
            time_signature,
            ..Self::default()
        }
    }

    /// Convenience: produce a deep copy. `clone()` already does this
    /// (SampleBuffer's `data` is Arc-shared so the audio is shared, not
    /// duplicated); kept as a named operation so app code can spell
    /// out copy/paste semantics.
    pub fn duplicate(&self) -> Self {
        self.clone()
    }
}

/// A queued mid-song change waiting to land at the next bar boundary.
/// Matches the SR-16 transport model: press the button now, the
/// change applies when musically appropriate.
#[derive(Clone, Debug, PartialEq)]
pub enum PendingChange {
    /// Start capturing input audio into the buffer at this bar index
    /// when playback reaches it.
    StartRecording { bar_idx: usize },
    /// Stop capturing input audio.
    StopRecording,
    /// Jump playback to a specific bar.
    SeekTo { bar_idx: usize },
    /// Toggle play state at the boundary.
    SetPlaying(bool),
}

/// Where playback currently is. `bar_idx` indexes into [`Song::bars`];
/// `sample_in_bar` advances every output frame and wraps when it
/// reaches the bar's duration (at which point `bar_idx` advances).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SongCursor {
    pub bar_idx: usize,
    pub sample_in_bar: u64,
}

/// Errors from bar-manipulation operations.
#[derive(Debug, PartialEq)]
pub enum SongError {
    BarIndexOutOfRange {
        idx: usize,
        len: usize,
    },
    /// Tried to remove the last remaining bar — songs must have at
    /// least one bar.
    CannotRemoveLastBar,
}

impl std::fmt::Display for SongError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BarIndexOutOfRange { idx, len } => {
                write!(f, "bar index {idx} out of range (len {len})")
            }
            Self::CannotRemoveLastBar => {
                f.write_str("cannot remove the last bar; songs must have ≥1 bar")
            }
        }
    }
}

impl std::error::Error for SongError {}

/// A multi-bar arrangement with playback cursor and queued changes.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Song {
    pub bars: Vec<Bar>,
    /// When true, playback stops at the end of the last bar instead
    /// of wrapping. Default false (= loop the whole song).
    pub one_shot: bool,
    pub cursor: SongCursor,
    /// Currently playing? Stored on the song so the engine and UI
    /// agree on transport state.
    pub playing: bool,
    /// Currently recording into the bar at `cursor.bar_idx`?
    pub recording: bool,
    /// Changes waiting on a bar boundary.
    #[serde(skip)]
    pub pending: Vec<PendingChange>,
    /// User-supplied song name for the song list / file picker.
    pub name: String,
    /// Whether the metronome click sounds during playback. Default
    /// true; toggled from the transport row. Legacy saves predate the
    /// field and read back as `true`.
    #[serde(default = "default_true")]
    pub click_enabled: bool,
    /// Recording mode: `false` (default) overdubs onto the bar's
    /// existing audio (sum + soft-clamp); `true` replaces it
    /// per-sample as the cursor passes, so a new take overwrites the
    /// old one across a full loop pass.
    #[serde(default)]
    pub record_replace: bool,
}

/// Serde default for boolean fields that should be `true` when absent
/// from an older save.
fn default_true() -> bool {
    true
}

impl Default for Song {
    fn default() -> Self {
        Self::new()
    }
}

impl Song {
    /// A fresh song with one empty bar at 120 BPM 4/4.
    pub fn new() -> Self {
        Self {
            bars: vec![Bar::default()],
            one_shot: false,
            cursor: SongCursor::default(),
            playing: false,
            recording: false,
            pending: Vec::new(),
            name: String::from("Untitled Song"),
            click_enabled: true,
            record_replace: false,
        }
    }

    /// Number of bars in the song. Always ≥ 1.
    pub fn len(&self) -> usize {
        self.bars.len()
    }

    pub fn is_empty(&self) -> bool {
        // The song itself always has at least one bar (enforced by
        // remove guards). `is_empty` mirrors the standard collection
        // method for API consistency but will always return false on
        // a well-formed song.
        self.bars.is_empty()
    }

    /// Borrow the bar at the cursor.
    pub fn current_bar(&self) -> &Bar {
        &self.bars[self.cursor.bar_idx]
    }

    /// Mutable borrow of the bar at the cursor.
    pub fn current_bar_mut(&mut self) -> &mut Bar {
        &mut self.bars[self.cursor.bar_idx]
    }

    /// Borrow a specific bar.
    pub fn bar(&self, idx: usize) -> Result<&Bar, SongError> {
        self.bars.get(idx).ok_or(SongError::BarIndexOutOfRange {
            idx,
            len: self.bars.len(),
        })
    }

    pub fn bar_mut(&mut self, idx: usize) -> Result<&mut Bar, SongError> {
        let len = self.bars.len();
        self.bars
            .get_mut(idx)
            .ok_or(SongError::BarIndexOutOfRange { idx, len })
    }

    /// Append a new bar, returning its index. Defaults match the
    /// current cursor-bar's tempo + time signature so adding a bar
    /// in the middle of working doesn't snap settings back to 120/4.
    pub fn add_bar(&mut self) -> usize {
        let template = self.current_bar().clone();
        let mut new = template;
        // Don't carry forward the recorded audio when adding a new
        // bar — that would be surprising. Same for any pending state.
        new.audio_buffer = None;
        new.label.clear();
        self.bars.push(new);
        self.bars.len() - 1
    }

    /// Insert `bar` at position `idx`, shifting existing bars right.
    /// Cursor stays on its current bar (if its index was ≥ idx, it
    /// shifts right too).
    pub fn insert_bar(&mut self, idx: usize, bar: Bar) -> Result<(), SongError> {
        if idx > self.bars.len() {
            return Err(SongError::BarIndexOutOfRange {
                idx,
                len: self.bars.len(),
            });
        }
        self.bars.insert(idx, bar);
        if self.cursor.bar_idx >= idx {
            self.cursor.bar_idx += 1;
        }
        Ok(())
    }

    /// Remove the bar at `idx`. The cursor adjusts: if it was on the
    /// removed bar (or after), it shifts left by one (clamped to the
    /// new length).
    pub fn remove_bar(&mut self, idx: usize) -> Result<Bar, SongError> {
        if self.bars.len() == 1 {
            return Err(SongError::CannotRemoveLastBar);
        }
        if idx >= self.bars.len() {
            return Err(SongError::BarIndexOutOfRange {
                idx,
                len: self.bars.len(),
            });
        }
        let removed = self.bars.remove(idx);
        if self.cursor.bar_idx >= idx && self.cursor.bar_idx > 0 {
            self.cursor.bar_idx -= 1;
        }
        if self.cursor.bar_idx >= self.bars.len() {
            self.cursor.bar_idx = self.bars.len() - 1;
        }
        Ok(removed)
    }

    /// Return a clone of the bar at `idx` (your clipboard).
    pub fn copy_bar(&self, idx: usize) -> Result<Bar, SongError> {
        self.bar(idx).map(|b| b.duplicate())
    }

    /// Paste `bar` into position `idx`, replacing whatever was there.
    /// The bar's audio buffer Arc is shared with the source — paste
    /// is cheap regardless of buffer size.
    pub fn paste_bar(&mut self, idx: usize, bar: Bar) -> Result<(), SongError> {
        let len = self.bars.len();
        let slot = self
            .bars
            .get_mut(idx)
            .ok_or(SongError::BarIndexOutOfRange { idx, len })?;
        *slot = bar;
        Ok(())
    }

    /// Move a bar from one position to another. Useful for drag-to-
    /// reorder gestures.
    pub fn move_bar(&mut self, from: usize, to: usize) -> Result<(), SongError> {
        let len = self.bars.len();
        if from >= len {
            return Err(SongError::BarIndexOutOfRange { idx: from, len });
        }
        if to >= len {
            return Err(SongError::BarIndexOutOfRange { idx: to, len });
        }
        if from == to {
            return Ok(());
        }
        let bar = self.bars.remove(from);
        self.bars.insert(to, bar);
        Ok(())
    }

    /// Queue a pending change to apply at the next bar boundary.
    pub fn queue(&mut self, change: PendingChange) {
        self.pending.push(change);
    }

    /// Cancel all queued changes without applying them.
    pub fn clear_pending(&mut self) {
        self.pending.clear();
    }

    /// Apply queued changes; called when the playback cursor reaches
    /// a bar boundary. Returns the number of changes applied.
    ///
    /// Engine integration: call this from the audio thread (or from
    /// a control thread that holds the song lock) every time the
    /// cursor wraps from bar N to bar N+1.
    pub fn on_bar_boundary(&mut self) -> usize {
        let n = self.pending.len();
        let changes = std::mem::take(&mut self.pending);
        for change in changes {
            match change {
                PendingChange::StartRecording { bar_idx } => {
                    self.recording = self.cursor.bar_idx == bar_idx;
                }
                PendingChange::StopRecording => {
                    self.recording = false;
                }
                PendingChange::SeekTo { bar_idx } => {
                    if bar_idx < self.bars.len() {
                        self.cursor.bar_idx = bar_idx;
                        self.cursor.sample_in_bar = 0;
                    }
                }
                PendingChange::SetPlaying(p) => {
                    self.playing = p;
                }
            }
        }
        n
    }

    /// Advance the cursor by `frames` samples at `sample_rate_hz`.
    /// If the cursor crosses a bar boundary, applies queued changes
    /// and (for the song's `one_shot` mode) stops at the final bar.
    /// Returns true iff at least one bar boundary was crossed.
    pub fn advance(&mut self, frames: u64, sample_rate_hz: f32) -> bool {
        if !self.playing || frames == 0 {
            return false;
        }
        let mut remaining = frames;
        let mut crossed = false;
        while remaining > 0 {
            let bar_len = self.current_bar().duration_samples(sample_rate_hz);
            if bar_len == 0 {
                break;
            }
            let until_boundary = bar_len.saturating_sub(self.cursor.sample_in_bar);
            if remaining < until_boundary {
                self.cursor.sample_in_bar += remaining;
                break;
            }
            // Cross the boundary.
            remaining -= until_boundary;
            crossed = true;
            self.cursor.sample_in_bar = 0;
            let next_idx = self.cursor.bar_idx + 1;
            if next_idx >= self.bars.len() {
                if self.one_shot {
                    self.playing = false;
                    break;
                }
                self.cursor.bar_idx = 0;
            } else {
                self.cursor.bar_idx = next_idx;
            }
            self.on_bar_boundary();
        }
        crossed
    }

    /// Total song duration in seconds. Sum of all bar durations.
    pub fn duration_secs(&self) -> f32 {
        self.bars.iter().map(|b| b.duration_secs()).sum()
    }

    /// Attach a freshly-rendered audio buffer to the bar at `idx`.
    /// Replaces any existing buffer there.
    pub fn attach_buffer(&mut self, idx: usize, buffer: SampleBuffer) -> Result<(), SongError> {
        self.bar_mut(idx)?.audio_buffer = Some(buffer);
        Ok(())
    }

    /// Detach (and return) the audio buffer on bar `idx`, leaving the
    /// bar audio-empty. Useful for "clear loop" gestures.
    pub fn detach_buffer(&mut self, idx: usize) -> Result<Option<SampleBuffer>, SongError> {
        Ok(self.bar_mut(idx)?.audio_buffer.take())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar_at(bpm: f32, beats: u8) -> Bar {
        Bar::with_tempo(bpm, TimeSignature::new(beats, 4))
    }

    #[test]
    fn new_song_has_one_default_bar() {
        let s = Song::new();
        assert_eq!(s.len(), 1);
        assert_eq!(s.cursor.bar_idx, 0);
        assert!(!s.playing);
    }

    #[test]
    fn add_bar_appends_and_inherits_settings() {
        let mut s = Song::new();
        s.bar_mut(0).unwrap().bpm = 140.0;
        let idx = s.add_bar();
        assert_eq!(idx, 1);
        assert_eq!(s.len(), 2);
        // New bar should inherit the tempo, not snap back to default.
        assert_eq!(s.bar(1).unwrap().bpm, 140.0);
        // But audio buffer should NOT carry forward.
        assert!(s.bar(1).unwrap().audio_buffer.is_none());
    }

    #[test]
    fn insert_bar_shifts_cursor_right_when_needed() {
        let mut s = Song::new();
        s.add_bar();
        s.add_bar(); // 3 bars
        s.cursor.bar_idx = 2;
        s.insert_bar(1, bar_at(100.0, 4)).unwrap();
        // Cursor was at 2; we inserted at 1; cursor should be at 3.
        assert_eq!(s.cursor.bar_idx, 3);
        assert_eq!(s.len(), 4);
    }

    #[test]
    fn remove_bar_adjusts_cursor() {
        let mut s = Song::new();
        s.add_bar();
        s.add_bar(); // 3 bars
        s.cursor.bar_idx = 2;
        s.remove_bar(1).unwrap();
        // Removed bar 1, cursor was at 2 → now at 1.
        assert_eq!(s.cursor.bar_idx, 1);
        assert_eq!(s.len(), 2);
    }

    #[test]
    fn remove_last_remaining_bar_errors() {
        let mut s = Song::new();
        assert!(matches!(
            s.remove_bar(0),
            Err(SongError::CannotRemoveLastBar)
        ));
    }

    #[test]
    fn copy_paste_round_trips() {
        let mut s = Song::new();
        s.add_bar(); // 2 bars
        s.bar_mut(0).unwrap().bpm = 90.0;
        s.bar_mut(0).unwrap().label = "Verse".into();
        let clip = s.copy_bar(0).unwrap();
        s.paste_bar(1, clip).unwrap();
        assert_eq!(s.bar(1).unwrap().bpm, 90.0);
        assert_eq!(s.bar(1).unwrap().label, "Verse");
    }

    #[test]
    fn move_bar_repositions_without_changing_count() {
        let mut s = Song::new();
        s.add_bar();
        s.add_bar(); // 3 bars
        s.bar_mut(0).unwrap().label = "A".into();
        s.bar_mut(1).unwrap().label = "B".into();
        s.bar_mut(2).unwrap().label = "C".into();
        s.move_bar(0, 2).unwrap();
        // Order should now be B, C, A.
        assert_eq!(s.bar(0).unwrap().label, "B");
        assert_eq!(s.bar(1).unwrap().label, "C");
        assert_eq!(s.bar(2).unwrap().label, "A");
        assert_eq!(s.len(), 3);
    }

    #[test]
    fn pending_changes_apply_at_bar_boundary() {
        let mut s = Song::new();
        s.add_bar();
        s.add_bar(); // 3 bars
        s.playing = true;
        s.queue(PendingChange::SeekTo { bar_idx: 2 });
        assert_eq!(s.cursor.bar_idx, 0);

        // Cross a boundary.
        let bar_samples = s.current_bar().duration_samples(48_000.0);
        s.advance(bar_samples + 100, 48_000.0);

        // Cursor should have crossed to next bar AND applied SeekTo
        // moving us to bar 2. The SeekTo applies during the
        // on_bar_boundary call as we cross.
        assert_eq!(s.cursor.bar_idx, 2);
    }

    #[test]
    fn loop_wraps_at_end() {
        let mut s = Song::new();
        s.add_bar(); // 2 bars
        s.playing = true;
        let bar0_samples = s.bar(0).unwrap().duration_samples(48_000.0);
        let bar1_samples = s.bar(1).unwrap().duration_samples(48_000.0);
        s.advance(bar0_samples + bar1_samples + 100, 48_000.0);
        // After crossing both bars, looped back to bar 0 with ~100
        // samples in.
        assert_eq!(s.cursor.bar_idx, 0);
        assert!(s.cursor.sample_in_bar <= 200);
    }

    #[test]
    fn one_shot_stops_at_end() {
        let mut s = Song::new();
        s.add_bar(); // 2 bars
        s.one_shot = true;
        s.playing = true;
        let total = s.bar(0).unwrap().duration_samples(48_000.0)
            + s.bar(1).unwrap().duration_samples(48_000.0);
        s.advance(total + 1000, 48_000.0);
        assert!(!s.playing, "one-shot song should stop at end");
    }

    #[test]
    fn pending_clear_drops_unapplied() {
        let mut s = Song::new();
        s.add_bar();
        s.playing = true;
        s.queue(PendingChange::SeekTo { bar_idx: 1 });
        s.queue(PendingChange::StopRecording);
        assert_eq!(s.pending.len(), 2);
        s.clear_pending();
        assert!(s.pending.is_empty());
        // And the change should NOT apply on the next boundary.
        let bar_samples = s.current_bar().duration_samples(48_000.0);
        s.advance(bar_samples + 100, 48_000.0);
        // Just rolled into bar 1 normally (next bar), not by SeekTo.
        assert_eq!(s.cursor.bar_idx, 1);
    }

    #[test]
    fn song_serializes_to_json() {
        let mut s = Song::new();
        s.name = "Test Song".into();
        s.add_bar();
        s.bar_mut(1).unwrap().bpm = 90.0;
        s.bar_mut(1).unwrap().chord_ref = Some(ChordRef {
            formula_name: "Minor".into(),
            root_freq_hz: 220.0,
            pitches_hz: vec![220.0, 261.63, 329.63],
            label: "Am".into(),
        });
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("Test Song"));
        assert!(json.contains("90"));
        let back: Song = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, s.name);
        assert_eq!(back.len(), 2);
        assert_eq!(back.bar(1).unwrap().bpm, 90.0);
        assert_eq!(back.bar(1).unwrap().chord_ref.as_ref().unwrap().label, "Am");
    }

    #[test]
    fn bar_length_multiplies_duration() {
        let mut b = Bar::with_tempo(120.0, TimeSignature::new(4, 4));
        let one = b.duration_secs();
        b.length = 3;
        assert!((b.duration_secs() - one * 3.0).abs() < 1e-4);
    }

    #[test]
    fn multi_measure_bar_advances_after_full_block() {
        let mut s = Song::new();
        s.add_bar(); // 2 bars
        s.bar_mut(0).unwrap().length = 2;
        s.playing = true;
        let block0 = s.bar(0).unwrap().duration_samples(48_000.0);
        // One measure in (half the block) shouldn't cross the boundary.
        s.advance((block0 / 2).saturating_sub(10), 48_000.0);
        assert_eq!(s.cursor.bar_idx, 0, "still inside the length-2 block");
        // Past the full block, we land in bar 1.
        s.advance(block0, 48_000.0);
        assert_eq!(s.cursor.bar_idx, 1);
    }

    #[test]
    fn bar_length_serde_default_for_legacy() {
        // A Bar JSON predating the `length` field must read back as a
        // single-measure bar, not a zero-length one.
        let mut v = serde_json::to_value(Bar::default()).unwrap();
        v.as_object_mut().unwrap().remove("length");
        let b: Bar = serde_json::from_value(v).unwrap();
        assert_eq!(b.length, 1);
    }

    #[test]
    fn attach_and_detach_audio_buffer() {
        let mut s = Song::new();
        let buf = SampleBuffer::new(vec![0.5; 100], 48_000);
        s.attach_buffer(0, buf).unwrap();
        assert!(s.bar(0).unwrap().audio_buffer.is_some());
        let removed = s.detach_buffer(0).unwrap();
        assert!(removed.is_some());
        assert!(s.bar(0).unwrap().audio_buffer.is_none());
    }
}
