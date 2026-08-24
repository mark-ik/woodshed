//! Sample bank — loads PCM buffers from `.wav` files and serves them
//! to [`Sound::Sample`] instances by id.
//!
//! # Why a bank
//!
//! The sequencer's [`SequencerPattern`] is a small, serializable data
//! structure: it carries sample *ids* (e.g. `"kick"`, `"snare"`), not
//! the audio data itself. The bank lives outside the pattern, gets
//! populated once at startup (or whenever new kits load), and is the
//! source of truth for resolving id → PCM buffer.
//!
//! This separation lets us:
//! - serialize patterns to JSON without pulling megabytes of PCM into
//!   the file,
//! - swap kits without touching the pattern (load a new bank under the
//!   same ids and rehydrate),
//! - share buffers cheaply across voices via `Arc`.
//!
//! # WAV format support
//!
//! Today the loader handles standard PCM `.wav` (16/24/32-bit integer
//! and 32-bit float). Multi-channel input is downmixed to mono by
//! averaging channels — drum samples are almost always mono in
//! practice, and a single-channel buffer matches the engine's
//! sample-by-sample mixer cleanly. Other container formats (FLAC,
//! OGG, MP3) are intentionally out of scope here; layer `symphonia`
//! in later when needed.

use std::collections::HashMap;
use std::path::Path;

use crate::sequencer::SequencerPattern;
use crate::sound::{SampleBuffer, Sound};

/// Errors from sample loading.
#[derive(Debug)]
pub enum SampleError {
    Io(std::io::Error),
    Wav(hound::Error),
    /// The WAV file uses a sample format we don't yet support.
    UnsupportedFormat(String),
}

impl std::fmt::Display for SampleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "i/o: {e}"),
            Self::Wav(e) => write!(f, "wav: {e}"),
            Self::UnsupportedFormat(s) => write!(f, "unsupported wav format: {s}"),
        }
    }
}

impl std::error::Error for SampleError {}

impl From<std::io::Error> for SampleError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<hound::Error> for SampleError {
    fn from(e: hound::Error) -> Self {
        Self::Wav(e)
    }
}

/// A keyed collection of loaded PCM buffers. Cheap to clone (buffers
/// are `Arc`-shared internally).
#[derive(Clone, Debug, Default)]
pub struct SampleBank {
    entries: HashMap<String, SampleBuffer>,
}

impl SampleBank {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a pre-built buffer under the given id.
    pub fn insert(&mut self, id: impl Into<String>, buffer: SampleBuffer) {
        self.entries.insert(id.into(), buffer);
    }

    /// Look up a buffer by id.
    pub fn get(&self, id: &str) -> Option<&SampleBuffer> {
        self.entries.get(id)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(|s| s.as_str())
    }

    /// Load a WAV file and store it under `id`. The file is decoded
    /// into a mono `f32` buffer; multi-channel input is averaged.
    pub fn load_wav(
        &mut self,
        id: impl Into<String>,
        path: impl AsRef<Path>,
    ) -> Result<(), SampleError> {
        let buffer = load_wav_to_buffer(path.as_ref())?;
        self.entries.insert(id.into(), buffer);
        Ok(())
    }

    /// Walk every [`Sound::Sample`] inside the pattern and attach a
    /// buffer from this bank where the id matches. Returns the number
    /// of voices that were successfully attached.
    pub fn rehydrate_pattern(&self, pattern: &mut SequencerPattern) -> usize {
        let mut attached = 0;
        for track in pattern.tracks.iter_mut() {
            if track.sound.rehydrate_from(|id| self.get(id).cloned()) {
                attached += 1;
            }
        }
        attached
    }

    /// Build a `Sound::Sample` referencing this bank's `id`, with the
    /// buffer pre-attached if the id exists.
    pub fn build_sound(&self, id: &str) -> Sound {
        match self.get(id) {
            Some(buf) => Sound::sample_with_buffer(id, buf.clone()),
            None => Sound::sample(id),
        }
    }
}

/// Decode a `.wav` file at `path` to a mono `f32` buffer.
fn load_wav_to_buffer(path: &Path) -> Result<SampleBuffer, SampleError> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;
    let sample_rate = spec.sample_rate;

    // Decode all samples into a flat interleaved Vec<f32>, then
    // downmix to mono by averaging channels per frame.
    let interleaved: Vec<f32> = match (spec.sample_format, spec.bits_per_sample) {
        (hound::SampleFormat::Float, 32) => {
            reader.samples::<f32>().collect::<Result<Vec<_>, _>>()?
        }
        (hound::SampleFormat::Int, 16) => reader
            .samples::<i16>()
            .map(|s| s.map(|v| v as f32 / i16::MAX as f32))
            .collect::<Result<Vec<_>, _>>()?,
        (hound::SampleFormat::Int, 24) => reader
            .samples::<i32>()
            .map(|s| s.map(|v| v as f32 / 8_388_607.0))
            .collect::<Result<Vec<_>, _>>()?,
        (hound::SampleFormat::Int, 32) => reader
            .samples::<i32>()
            .map(|s| s.map(|v| v as f32 / i32::MAX as f32))
            .collect::<Result<Vec<_>, _>>()?,
        (fmt, bits) => {
            return Err(SampleError::UnsupportedFormat(format!(
                "{fmt:?} {bits}-bit"
            )));
        }
    };

    let mono = if channels == 1 {
        interleaved
    } else {
        let frame_count = interleaved.len() / channels;
        let mut out = Vec::with_capacity(frame_count);
        for f in 0..frame_count {
            let mut sum = 0.0_f32;
            for c in 0..channels {
                sum += interleaved[f * channels + c];
            }
            out.push(sum / channels as f32);
        }
        out
    };

    Ok(SampleBuffer::new(mono, sample_rate))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sequencer::{Step, Subdivision, TimeSignature, Track};

    fn fake_buffer(value: f32, len: usize) -> SampleBuffer {
        SampleBuffer::new(vec![value; len], 48_000)
    }

    #[test]
    fn bank_insert_and_get_round_trip() {
        let mut bank = SampleBank::new();
        bank.insert("kick", fake_buffer(0.5, 100));
        let b = bank.get("kick").expect("kick should be present");
        assert_eq!(b.len(), 100);
        assert!(bank.get("snare").is_none());
    }

    #[test]
    fn bank_build_sound_attaches_when_present() {
        let mut bank = SampleBank::new();
        bank.insert("kick", fake_buffer(1.0, 4));
        let sound = bank.build_sound("kick");
        match sound {
            Sound::Sample { id, buffer, .. } => {
                assert_eq!(id, "kick");
                assert_eq!(buffer.len(), 4);
            }
            _ => panic!("expected Sample"),
        }
    }

    #[test]
    fn bank_build_sound_leaves_empty_when_absent() {
        let bank = SampleBank::new();
        let sound = bank.build_sound("ghost");
        match sound {
            Sound::Sample { buffer, .. } => assert!(buffer.is_empty()),
            _ => panic!("expected Sample"),
        }
    }

    #[test]
    fn rehydrate_pattern_attaches_buffers_for_known_ids() {
        let mut bank = SampleBank::new();
        bank.insert("kick", fake_buffer(0.7, 50));
        bank.insert("snare", fake_buffer(0.4, 40));

        // Build a pattern with three tracks: kick (loadable), snare
        // (loadable), ghost (unknown).
        let track_with = |sound: Sound, name: &str| Track {
            name: name.to_string(),
            steps: vec![Step::Active { accent: false }],
            sound,
            muted: false,
        };

        let mut pattern = SequencerPattern {
            bpm: 120.0,
            time_signature: TimeSignature::default(),
            subdivision: Subdivision::QUARTER,
            tracks: vec![
                track_with(Sound::sample("kick"), "Kick"),
                track_with(Sound::sample("snare"), "Snare"),
                track_with(Sound::sample("ghost"), "Ghost"),
            ],
        };

        let attached = bank.rehydrate_pattern(&mut pattern);
        assert_eq!(attached, 2, "should attach kick + snare, skip ghost");

        // Verify buffers actually got attached.
        match &pattern.tracks[0].sound {
            Sound::Sample { buffer, .. } => assert_eq!(buffer.len(), 50),
            _ => panic!(),
        }
        match &pattern.tracks[1].sound {
            Sound::Sample { buffer, .. } => assert_eq!(buffer.len(), 40),
            _ => panic!(),
        }
        match &pattern.tracks[2].sound {
            Sound::Sample { buffer, .. } => assert!(buffer.is_empty()),
            _ => panic!(),
        }
    }

    #[test]
    fn rehydrate_pattern_skips_click_tracks() {
        let bank = SampleBank::new();
        let mut pattern =
            SequencerPattern::metronome(120.0, TimeSignature::default(), Subdivision::QUARTER);
        // No samples in this pattern, so nothing should attach.
        let attached = bank.rehydrate_pattern(&mut pattern);
        assert_eq!(attached, 0);
    }

    #[test]
    fn load_wav_round_trip() {
        // Write a tiny WAV to a temp dir, then read it back through
        // the loader and verify the buffer matches.
        let tmp = std::env::temp_dir().join("woodshed_sample_test.wav");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 48_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        {
            let mut writer = hound::WavWriter::create(&tmp, spec).unwrap();
            // Write a short ramp.
            for i in 0..8_i16 {
                writer.write_sample(i * 1000).unwrap();
            }
            writer.finalize().unwrap();
        }

        let mut bank = SampleBank::new();
        bank.load_wav("ramp", &tmp).unwrap();
        let buf = bank.get("ramp").unwrap();
        assert_eq!(buf.len(), 8);
        assert_eq!(buf.sample_rate_hz, 48_000);
        // First sample should be ~0 (we wrote 0 * 1000 = 0), last
        // should be ~7000/i16::MAX ≈ 0.2136.
        assert!(buf.data[0].abs() < 1e-3);
        assert!((buf.data[7] - 7000.0 / i16::MAX as f32).abs() < 1e-3);

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn load_wav_downmixes_stereo_to_mono() {
        let tmp = std::env::temp_dir().join("woodshed_stereo_test.wav");
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 48_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        {
            let mut writer = hound::WavWriter::create(&tmp, spec).unwrap();
            // Two frames of (L=1000, R=3000) → mono average 2000.
            for _ in 0..2 {
                writer.write_sample(1000_i16).unwrap();
                writer.write_sample(3000_i16).unwrap();
            }
            writer.finalize().unwrap();
        }

        let mut bank = SampleBank::new();
        bank.load_wav("stereo", &tmp).unwrap();
        let buf = bank.get("stereo").unwrap();
        // Two mono frames now, each averaging the L/R pair.
        assert_eq!(buf.len(), 2);
        let expected = 2000.0 / i16::MAX as f32;
        assert!((buf.data[0] - expected).abs() < 1e-3);

        let _ = std::fs::remove_file(&tmp);
    }
}
