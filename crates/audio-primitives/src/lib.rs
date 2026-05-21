//! Shared, framework-agnostic audio DSP primitives.
//!
//! The pure-DSP layer of the Strophos audio family's pressure-vessel
//! doctrine: the engine-and-UI-agnostic kernels that both Woodshed
//! (cpal-direct) and Strophe (Firewheel) need, factored out so neither
//! product carries a private copy.
//!
//! Everything here is pure `std` — no `cpal`, no Firewheel, no UI. The
//! rule of thumb: if it takes plain sample slices / timestamps and
//! returns plain data, it belongs here; if it owns an audio stream,
//! a `Mutex`-published handle, or a live engine, it's a *driver* and
//! stays in the consuming crate (woodshed-audio's `OnsetAnalyzer` /
//! `CalibrationSession`, strophe-engine's Firewheel graph).
//!
//! ## Modules
//! - [`click`] — sine-burst click synthesis (one sample, or a full
//!   metronome bar). Shared by Woodshed's `Sound::Click` voice and
//!   Strophe's pre-rendered click loop.
//! - [`onset`] — [`onset::OnsetDetector`] streaming transient detection
//!   + [`onset::estimate_bpm`] median-interval tempo estimation.
//! - [`calibration`] — [`calibration::estimate_latency_from_pairs`]:
//!   pair expected click instants with detected onsets, return the
//!   median input-to-output round-trip latency.
//! - [`buffer`] — in-place sample-buffer shaping (gain / normalize /
//!   reverse). The looper/sampler "shape the take" kernels, behind
//!   Woodshed's `SampleBuffer` and available to any future sampler.

pub mod buffer;
pub mod calibration;
pub mod click;
pub mod onset;

pub use buffer::{apply_gain, normalize, reverse};
pub use calibration::{count_matches, estimate_latency_from_pairs, MATCH_WINDOW};
pub use click::{click_sample, render_click_bar};
pub use onset::{estimate_bpm, OnsetDetector};
