//! MIDI input/output.
//!
//! Two layers:
//!
//! - **Pure helpers** ([`MidiClockSync`], event parsing) — testable
//!   without hardware. These encode the algorithms that matter
//!   (BPM derivation from clock ticks, byte-stream → typed events).
//! - **I/O wrappers** ([`MidiOut`], [`MidiIn`]) — thin adapters over
//!   `midir`. Open a named port, send/receive bytes, hand parsed
//!   events to a callback.
//!
//! See `design_docs/2026-05-15_midi_design.md` for the full design
//! note covering transport ownership and what's out of scope.
//!
//! # Why we don't trigger sounds from MIDI Note In
//!
//! Woodshed is a practice tool, not a synth. Note On / Off events
//! from a controller are visible via [`MidiEvent::NoteOn`] /
//! [`MidiEvent::NoteOff`] for app-level use (e.g. a future "tap-tempo
//! by MIDI pedal" feature), but the audio engine itself doesn't
//! consume them.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use midir::{
    Ignore, MidiInput, MidiInputConnection, MidiInputPort, MidiOutput,
    MidiOutputConnection, MidiOutputPort,
};

/// Errors that can come out of the MIDI layer.
#[derive(Debug)]
pub enum MidiError {
    /// Failed to construct the MIDI client (driver missing, OS
    /// permissions, etc.).
    Init(midir::InitError),
    /// Couldn't find a port matching the requested name. The string
    /// inside is the requested name for debugging.
    PortNotFound(String),
    Connect(String),
    Send(midir::SendError),
}

impl std::fmt::Display for MidiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Init(e) => write!(f, "midi init: {e}"),
            Self::PortNotFound(name) => write!(f, "midi port not found: {name}"),
            Self::Connect(s) => write!(f, "midi connect: {s}"),
            Self::Send(e) => write!(f, "midi send: {e}"),
        }
    }
}

impl std::error::Error for MidiError {}

impl From<midir::InitError> for MidiError {
    fn from(e: midir::InitError) -> Self {
        Self::Init(e)
    }
}

impl From<midir::SendError> for MidiError {
    fn from(e: midir::SendError) -> Self {
        Self::Send(e)
    }
}

// === Constants for MIDI System Realtime / Channel messages we care about ===

/// MIDI realtime: clock tick. 24 per quarter note.
pub const CLOCK_TICK: u8 = 0xF8;
/// MIDI realtime: start playback from beat 1.
pub const TRANSPORT_START: u8 = 0xFA;
/// MIDI realtime: continue playback from current position.
pub const TRANSPORT_CONTINUE: u8 = 0xFB;
/// MIDI realtime: stop playback.
pub const TRANSPORT_STOP: u8 = 0xFC;

// === Typed events ===

/// Parsed MIDI events the rest of the app cares about. Channel
/// messages carry a 0-based channel (0..16) — different from the
/// 1-based notation common in DAW UIs.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MidiEvent {
    NoteOn {
        channel: u8,
        note: u8,
        velocity: u8,
    },
    NoteOff {
        channel: u8,
        note: u8,
        velocity: u8,
    },
    ControlChange {
        channel: u8,
        controller: u8,
        value: u8,
    },
    Clock,
    Start,
    Continue,
    Stop,
}

/// Parse a single MIDI message (a raw byte slice straight from
/// `midir`'s callback). Returns `None` for messages we don't model.
///
/// Realtime System messages can be interleaved into the middle of a
/// channel message stream in MIDI 1.0; `midir` already splits them
/// for us, so each callback delivers exactly one logical message.
pub fn parse_message(bytes: &[u8]) -> Option<MidiEvent> {
    if bytes.is_empty() {
        return None;
    }
    let status = bytes[0];
    // System Realtime messages: single byte, no data.
    match status {
        CLOCK_TICK => return Some(MidiEvent::Clock),
        TRANSPORT_START => return Some(MidiEvent::Start),
        TRANSPORT_CONTINUE => return Some(MidiEvent::Continue),
        TRANSPORT_STOP => return Some(MidiEvent::Stop),
        _ => {}
    }
    // Channel messages: 0x80..=0xEF with channel in low nibble.
    if !(0x80..=0xEF).contains(&status) {
        return None;
    }
    let channel = status & 0x0F;
    let kind = status & 0xF0;
    match kind {
        0x80 => {
            // Note Off
            let note = *bytes.get(1)?;
            let velocity = *bytes.get(2).unwrap_or(&0);
            Some(MidiEvent::NoteOff { channel, note, velocity })
        }
        0x90 => {
            // Note On — by convention, velocity 0 = Note Off.
            let note = *bytes.get(1)?;
            let velocity = *bytes.get(2).unwrap_or(&0);
            if velocity == 0 {
                Some(MidiEvent::NoteOff { channel, note, velocity })
            } else {
                Some(MidiEvent::NoteOn { channel, note, velocity })
            }
        }
        0xB0 => {
            // Control Change
            let controller = *bytes.get(1)?;
            let value = *bytes.get(2).unwrap_or(&0);
            Some(MidiEvent::ControlChange { channel, controller, value })
        }
        _ => None,
    }
}

// === Pure clock-sync helper ===

/// Derive BPM from a stream of incoming MIDI Clock tick timestamps.
///
/// MIDI Clock is 24 ticks per quarter note. We average inter-tick
/// intervals over the most recent N ticks and smooth across calls
/// with a small exponential moving average so a host that jitters
/// by a few ms doesn't make our reported BPM oscillate.
#[derive(Clone, Debug)]
pub struct MidiClockSync {
    /// Ticks retained for averaging. ~24 = one beat of history.
    history: VecDeque<Instant>,
    /// History cap.
    capacity: usize,
    /// Exponential smoothing factor (0 = no smoothing, 1 = freeze).
    /// 0.3 = lean toward new measurements but soak some jitter.
    smoothing: f32,
    /// Most recent smoothed BPM estimate, `None` until enough ticks.
    smoothed_bpm: Option<f32>,
}

impl Default for MidiClockSync {
    fn default() -> Self {
        Self {
            history: VecDeque::with_capacity(24),
            capacity: 24,
            smoothing: 0.3,
            smoothed_bpm: None,
        }
    }
}

impl MidiClockSync {
    pub fn new() -> Self {
        Self::default()
    }

    /// Drop all history. Use when leaving / re-entering clock-slave
    /// mode.
    pub fn reset(&mut self) {
        self.history.clear();
        self.smoothed_bpm = None;
    }

    /// Record one incoming MIDI clock tick at the given wall-clock
    /// instant. Updates the smoothed BPM estimate as a side effect.
    pub fn record_tick(&mut self, at: Instant) {
        if self.history.len() >= self.capacity {
            self.history.pop_front();
        }
        self.history.push_back(at);
        self.recompute();
    }

    fn recompute(&mut self) {
        if self.history.len() < 2 {
            return;
        }
        // Mean of inter-tick intervals.
        let intervals: Vec<f32> = self
            .history
            .iter()
            .zip(self.history.iter().skip(1))
            .map(|(a, b)| b.duration_since(*a).as_secs_f32())
            .collect();
        let mean_secs_per_tick =
            intervals.iter().sum::<f32>() / intervals.len() as f32;
        if mean_secs_per_tick < 1e-6 {
            return;
        }
        // 24 ticks per quarter note.
        let secs_per_beat = mean_secs_per_tick * 24.0;
        let raw_bpm = (60.0 / secs_per_beat).clamp(40.0, 240.0);
        self.smoothed_bpm = Some(match self.smoothed_bpm {
            None => raw_bpm,
            Some(prev) => prev + (raw_bpm - prev) * (1.0 - self.smoothing),
        });
    }

    /// Current smoothed BPM estimate, if there's enough history.
    pub fn estimated_bpm(&self) -> Option<f32> {
        self.smoothed_bpm
    }

    /// Number of ticks currently retained.
    pub fn tick_count(&self) -> usize {
        self.history.len()
    }
}

// === I/O ===

/// List the names of available MIDI input ports. Returns an empty
/// vec if no MIDI subsystem is available.
pub fn list_input_ports() -> Result<Vec<String>, MidiError> {
    let input = MidiInput::new("woodshed-list")?;
    let ports = input.ports();
    let mut names = Vec::with_capacity(ports.len());
    for p in &ports {
        match input.port_name(p) {
            Ok(name) => names.push(name),
            Err(_) => names.push(String::from("<unnamed>")),
        }
    }
    Ok(names)
}

/// List the names of available MIDI output ports.
pub fn list_output_ports() -> Result<Vec<String>, MidiError> {
    let output = MidiOutput::new("woodshed-list")?;
    let ports = output.ports();
    let mut names = Vec::with_capacity(ports.len());
    for p in &ports {
        match output.port_name(p) {
            Ok(name) => names.push(name),
            Err(_) => names.push(String::from("<unnamed>")),
        }
    }
    Ok(names)
}

/// Open a MIDI output port by name. Mostly a convenience that hides
/// `midir`'s `port-by-index` API.
pub struct MidiOut {
    conn: MidiOutputConnection,
}

impl MidiOut {
    pub fn connect_by_name(port_name: &str) -> Result<Self, MidiError> {
        let output = MidiOutput::new("woodshed-out")?;
        let port = find_output_port(&output, port_name)?;
        let conn = output
            .connect(&port, "woodshed-out")
            .map_err(|e| MidiError::Connect(format!("{e}")))?;
        Ok(Self { conn })
    }

    /// Send raw bytes. Prefer the named helpers below where they
    /// exist; this is the escape hatch for messages we don't model.
    pub fn send_raw(&mut self, bytes: &[u8]) -> Result<(), MidiError> {
        self.conn.send(bytes)?;
        Ok(())
    }

    pub fn send_note_on(
        &mut self,
        channel: u8,
        note: u8,
        velocity: u8,
    ) -> Result<(), MidiError> {
        self.send_raw(&[0x90 | (channel & 0x0F), note & 0x7F, velocity & 0x7F])
    }

    pub fn send_note_off(
        &mut self,
        channel: u8,
        note: u8,
        velocity: u8,
    ) -> Result<(), MidiError> {
        self.send_raw(&[0x80 | (channel & 0x0F), note & 0x7F, velocity & 0x7F])
    }

    pub fn send_clock_tick(&mut self) -> Result<(), MidiError> {
        self.send_raw(&[CLOCK_TICK])
    }

    pub fn send_start(&mut self) -> Result<(), MidiError> {
        self.send_raw(&[TRANSPORT_START])
    }

    pub fn send_stop(&mut self) -> Result<(), MidiError> {
        self.send_raw(&[TRANSPORT_STOP])
    }

    pub fn send_continue(&mut self) -> Result<(), MidiError> {
        self.send_raw(&[TRANSPORT_CONTINUE])
    }
}

fn find_output_port(
    output: &MidiOutput,
    name: &str,
) -> Result<MidiOutputPort, MidiError> {
    for p in output.ports() {
        if let Ok(pn) = output.port_name(&p) {
            if pn == name {
                return Ok(p);
            }
        }
    }
    Err(MidiError::PortNotFound(name.to_string()))
}

fn find_input_port(
    input: &MidiInput,
    name: &str,
) -> Result<MidiInputPort, MidiError> {
    for p in input.ports() {
        if let Ok(pn) = input.port_name(&p) {
            if pn == name {
                return Ok(p);
            }
        }
    }
    Err(MidiError::PortNotFound(name.to_string()))
}

/// Bounded queue of recent MIDI events with a clock-sync tracker.
#[derive(Clone, Debug, Default)]
pub struct MidiInState {
    pub events: VecDeque<(Instant, MidiEvent)>,
    pub clock_sync: MidiClockSync,
}

impl MidiInState {
    pub const CAPACITY: usize = 256;

    fn push(&mut self, at: Instant, event: MidiEvent) {
        if matches!(event, MidiEvent::Clock) {
            self.clock_sync.record_tick(at);
        }
        if self.events.len() >= Self::CAPACITY {
            self.events.pop_front();
        }
        self.events.push_back((at, event));
    }
}

/// Open a MIDI input port by name. Events arrive on midir's reader
/// thread and are queued into shared state behind a mutex; UI / app
/// code polls [`Self::state`] to see what's come in.
pub struct MidiIn {
    state: Arc<Mutex<MidiInState>>,
    _conn: MidiInputConnection<()>,
}

impl MidiIn {
    pub fn connect_by_name(port_name: &str) -> Result<Self, MidiError> {
        let mut input = MidiInput::new("woodshed-in")?;
        input.ignore(Ignore::None);
        let port = find_input_port(&input, port_name)?;
        let state = Arc::new(Mutex::new(MidiInState::default()));
        let state_for_callback = Arc::clone(&state);
        let conn = input
            .connect(
                &port,
                "woodshed-in",
                move |_timestamp_us, bytes, _| {
                    if let Some(event) = parse_message(bytes) {
                        let mut s = state_for_callback.lock().unwrap();
                        s.push(Instant::now(), event);
                    }
                },
                (),
            )
            .map_err(|e| MidiError::Connect(format!("{e}")))?;
        Ok(Self {
            state,
            _conn: conn,
        })
    }

    /// Snapshot the current shared state.
    pub fn snapshot(&self) -> MidiInState {
        self.state.lock().unwrap().clone()
    }

    /// Drop all queued events and reset the clock sync. Call when
    /// entering a fresh sync session.
    pub fn reset(&self) {
        let mut s = self.state.lock().unwrap();
        s.events.clear();
        s.clock_sync.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    // === Parse tests ===

    #[test]
    fn parse_note_on_with_velocity() {
        let bytes = [0x91, 60, 96]; // channel 1, middle C, vel 96
        let ev = parse_message(&bytes).unwrap();
        assert_eq!(
            ev,
            MidiEvent::NoteOn {
                channel: 1,
                note: 60,
                velocity: 96
            }
        );
    }

    #[test]
    fn parse_note_on_velocity_zero_is_note_off() {
        let bytes = [0x90, 60, 0];
        let ev = parse_message(&bytes).unwrap();
        assert_eq!(
            ev,
            MidiEvent::NoteOff {
                channel: 0,
                note: 60,
                velocity: 0
            }
        );
    }

    #[test]
    fn parse_explicit_note_off() {
        let bytes = [0x82, 64, 64];
        let ev = parse_message(&bytes).unwrap();
        assert_eq!(
            ev,
            MidiEvent::NoteOff {
                channel: 2,
                note: 64,
                velocity: 64
            }
        );
    }

    #[test]
    fn parse_control_change() {
        let bytes = [0xB0, 7, 100]; // ch 0, volume, value 100
        let ev = parse_message(&bytes).unwrap();
        assert_eq!(
            ev,
            MidiEvent::ControlChange {
                channel: 0,
                controller: 7,
                value: 100
            }
        );
    }

    #[test]
    fn parse_realtime_messages() {
        assert_eq!(parse_message(&[CLOCK_TICK]), Some(MidiEvent::Clock));
        assert_eq!(parse_message(&[TRANSPORT_START]), Some(MidiEvent::Start));
        assert_eq!(parse_message(&[TRANSPORT_STOP]), Some(MidiEvent::Stop));
        assert_eq!(
            parse_message(&[TRANSPORT_CONTINUE]),
            Some(MidiEvent::Continue)
        );
    }

    #[test]
    fn parse_unknown_returns_none() {
        // Program Change is 0xC0..=0xCF; we don't model it.
        assert!(parse_message(&[0xC0, 5]).is_none());
        assert!(parse_message(&[]).is_none());
    }

    // === Clock sync tests ===

    /// Build a sequence of ticks spaced `interval_ms` apart starting
    /// from `now`. Returns the corresponding Instant series.
    fn tick_series(start: Instant, count: usize, interval_ms: u64) -> Vec<Instant> {
        (0..count)
            .map(|i| start + Duration::from_millis(interval_ms * i as u64))
            .collect()
    }

    #[test]
    fn clock_sync_120_bpm_from_simulated_ticks() {
        // 120 BPM, 24 PPQN: secs_per_beat = 0.5s, secs_per_tick =
        // 0.5 / 24 ≈ 20.833 ms. Use that as interval.
        let mut sync = MidiClockSync::new();
        let start = Instant::now();
        // Use fractional milliseconds via micros.
        let interval = Duration::from_micros(20_833);
        for i in 0..48 {
            sync.record_tick(start + interval * i);
        }
        let bpm = sync.estimated_bpm().unwrap();
        assert!((bpm - 120.0).abs() < 1.0, "got {bpm}");
    }

    #[test]
    fn clock_sync_60_bpm_from_simulated_ticks() {
        // 60 BPM: secs_per_beat = 1s, secs_per_tick = 1/24 ≈ 41.667ms.
        let mut sync = MidiClockSync::new();
        let start = Instant::now();
        let interval = Duration::from_micros(41_667);
        for i in 0..48 {
            sync.record_tick(start + interval * i);
        }
        let bpm = sync.estimated_bpm().unwrap();
        assert!((bpm - 60.0).abs() < 1.0, "got {bpm}");
    }

    #[test]
    fn clock_sync_returns_none_without_ticks() {
        let sync = MidiClockSync::new();
        assert!(sync.estimated_bpm().is_none());
    }

    #[test]
    fn clock_sync_reset_clears_state() {
        let mut sync = MidiClockSync::new();
        let ticks = tick_series(Instant::now(), 24, 20);
        for t in ticks {
            sync.record_tick(t);
        }
        assert!(sync.estimated_bpm().is_some());
        sync.reset();
        assert!(sync.estimated_bpm().is_none());
        assert_eq!(sync.tick_count(), 0);
    }

    #[test]
    fn list_ports_does_not_panic() {
        // Don't assert anything about port counts — the test
        // environment may or may not have MIDI. We just verify the
        // call doesn't panic.
        let _ = list_input_ports();
        let _ = list_output_ports();
    }
}
