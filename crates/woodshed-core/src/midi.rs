//! The MIDI seam (genet-host plan; audio-depth slice 13).
//!
//! The core describes what the app wants from MIDI in neutral data; a
//! host supplies a [`MidiBackend`] that realizes it — `midir` on
//! desktop, Web MIDI in the browser. The core never touches a MIDI API,
//! so the same intent (which ports, slave-to-clock, send-clock) drives
//! both. The BPM-from-clock math and byte parsing live in
//! `woodshed-audio`'s `midi` module (tested there); this is only the
//! boundary the host implements.

/// The host-supplied MIDI realization. Implementations should be
/// tolerant of a missing subsystem: construct in a degraded state and
/// report through [`error`](Self::error) rather than failing the app.
pub trait MidiBackend {
    /// Available input port names (a fresh scan each call — ports come
    /// and go as gear is plugged in).
    fn input_ports(&self) -> Vec<String>;
    /// Available output port names (fresh scan).
    fn output_ports(&self) -> Vec<String>;
    /// Connect the named input port; `None` disconnects. Once connected,
    /// incoming MIDI clock feeds [`clock_bpm`](Self::clock_bpm) and other
    /// events feed [`recent_events`](Self::recent_events).
    fn connect_input(&mut self, port: Option<&str>);
    /// Connect the named output port; `None` disconnects. The clock-out
    /// master (see [`set_clock_out`](Self::set_clock_out)) drives it.
    fn connect_output(&mut self, port: Option<&str>);
    /// The connected input port name, if any.
    fn connected_input(&self) -> Option<&str>;
    /// The connected output port name, if any.
    fn connected_output(&self) -> Option<&str>;
    /// BPM derived from incoming MIDI clock — `None` when the input is
    /// disconnected or not receiving clock. The host polls this to slave
    /// its transport tempo.
    fn clock_bpm(&self) -> Option<f32>;
    /// Recent incoming events as short display strings (newest last),
    /// for a live readout. Clock ticks are summarized out.
    fn recent_events(&self) -> Vec<String>;
    /// Drive the clock-out master. While `enabled && playing` and an
    /// output is connected, emit 24-PPQN MIDI clock at `bpm` and send
    /// Start / Stop on the play-state transitions so external gear
    /// slaves to Woodshed's tempo. Idempotent; call every dispatch/tick.
    fn set_clock_out(&mut self, enabled: bool, playing: bool, bpm: f32);
    /// A subsystem/connection failure to surface in the UI, if any.
    fn error(&self) -> Option<&str>;
}
