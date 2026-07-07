//! The desktop [`MidiBackend`]: `woodshed-audio`'s `midir` layer behind
//! the core's MIDI seam (audio-depth slice 13).
//!
//! Input is a connected `MidiIn` — its `midir` reader thread queues
//! events and the `MidiClockSync` derives BPM from incoming clock, which
//! the host polls to slave its transport. Output is a dedicated
//! clock-generator thread that emits 24-PPQN MIDI clock + Start/Stop so
//! external gear slaves to Woodshed's tempo; the thread owns the output
//! connection, so the `midir` handle never crosses a thread boundary.
//! The web host implements the same trait over Web MIDI.

use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::thread::JoinHandle;
use std::time::Duration;

use woodshed_audio::{
    list_midi_input_ports, list_midi_output_ports, MidiEvent, MidiIn, MidiOut,
};
use woodshed_core::midi::MidiBackend;

/// Commands to the clock-out thread. The thread owns the output
/// connection (it calls `connect_by_name` itself), so no `midir` handle
/// crosses the channel — only these plain-data messages do.
enum ClockCmd {
    Connect(Option<String>),
    Enabled(bool),
    Playing(bool),
    Bpm(f32),
    Stop,
}

pub struct MidiHost {
    input: Option<MidiIn>,
    connected_in: Option<String>,
    connected_out: Option<String>,
    clock_tx: Sender<ClockCmd>,
    _clock_thread: JoinHandle<()>,
    /// De-dup the per-tick clock-out pushes so a steady transport doesn't
    /// spam the channel every frame.
    last_out: (bool, bool, i32),
    error: Option<String>,
}

impl MidiHost {
    pub fn new() -> Self {
        let (tx, rx) = channel::<ClockCmd>();
        let thread = std::thread::spawn(move || clock_out_loop(rx));
        Self {
            input: None,
            connected_in: None,
            connected_out: None,
            clock_tx: tx,
            _clock_thread: thread,
            last_out: (false, false, -1),
            error: None,
        }
    }
}

impl Default for MidiHost {
    fn default() -> Self {
        Self::new()
    }
}

impl MidiBackend for MidiHost {
    fn input_ports(&self) -> Vec<String> {
        list_midi_input_ports().unwrap_or_default()
    }

    fn output_ports(&self) -> Vec<String> {
        list_midi_output_ports().unwrap_or_default()
    }

    fn connect_input(&mut self, port: Option<&str>) {
        match port {
            None => {
                self.input = None;
                self.connected_in = None;
            }
            Some(name) => {
                if self.connected_in.as_deref() == Some(name) {
                    return;
                }
                match MidiIn::connect_by_name(name) {
                    Ok(m) => {
                        self.input = Some(m);
                        self.connected_in = Some(name.to_string());
                    }
                    Err(e) => {
                        self.input = None;
                        self.connected_in = None;
                        self.error = Some(format!("midi in: {e}"));
                    }
                }
            }
        }
    }

    fn connect_output(&mut self, port: Option<&str>) {
        if self.connected_out.as_deref() == port {
            return;
        }
        self.connected_out = port.map(|s| s.to_string());
        let _ = self
            .clock_tx
            .send(ClockCmd::Connect(port.map(|s| s.to_string())));
    }

    fn connected_input(&self) -> Option<&str> {
        self.connected_in.as_deref()
    }

    fn connected_output(&self) -> Option<&str> {
        self.connected_out.as_deref()
    }

    fn clock_bpm(&self) -> Option<f32> {
        self.input
            .as_ref()
            .and_then(|m| m.snapshot().clock_sync.estimated_bpm())
    }

    fn recent_events(&self) -> Vec<String> {
        let Some(m) = self.input.as_ref() else {
            return Vec::new();
        };
        let snap = m.snapshot();
        // Newest-first, drop the clock spam, keep the last handful, then
        // restore chronological (newest last) order.
        let mut recent: Vec<String> = snap
            .events
            .iter()
            .rev()
            .filter(|(_, e)| !matches!(e, MidiEvent::Clock))
            .take(6)
            .map(|(_, e)| fmt_event(e))
            .collect();
        recent.reverse();
        recent
    }

    fn set_clock_out(&mut self, enabled: bool, playing: bool, bpm: f32) {
        let key = (enabled, playing, bpm.round() as i32);
        if key == self.last_out {
            return;
        }
        self.last_out = key;
        let _ = self.clock_tx.send(ClockCmd::Enabled(enabled));
        let _ = self.clock_tx.send(ClockCmd::Playing(playing));
        let _ = self.clock_tx.send(ClockCmd::Bpm(bpm));
    }

    fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

impl Drop for MidiHost {
    fn drop(&mut self) {
        let _ = self.clock_tx.send(ClockCmd::Stop);
    }
}

fn fmt_event(e: &MidiEvent) -> String {
    match e {
        MidiEvent::NoteOn { channel, note, velocity } => {
            format!("NoteOn ch{} {} v{}", channel + 1, note, velocity)
        }
        MidiEvent::NoteOff { channel, note, .. } => {
            format!("NoteOff ch{} {}", channel + 1, note)
        }
        MidiEvent::ControlChange { channel, controller, value } => {
            format!("CC ch{} #{}={}", channel + 1, controller, value)
        }
        MidiEvent::Clock => "Clock".to_string(),
        MidiEvent::Start => "Start".to_string(),
        MidiEvent::Continue => "Continue".to_string(),
        MidiEvent::Stop => "Stop".to_string(),
    }
}

/// The clock-out generator loop. Owns the output connection; while
/// enabled + playing with a port connected, it emits 24-PPQN MIDI clock
/// at the current BPM and sends Start/Stop on the play-state transitions.
fn clock_out_loop(rx: Receiver<ClockCmd>) {
    let mut conn: Option<MidiOut> = None;
    let mut enabled = false;
    let mut playing = false;
    let mut bpm = 120.0_f32;
    let mut last_transport = false;
    loop {
        // Drain all pending commands before deciding what to send.
        loop {
            match rx.try_recv() {
                Ok(ClockCmd::Connect(Some(name))) => {
                    conn = MidiOut::connect_by_name(&name).ok();
                }
                Ok(ClockCmd::Connect(None)) => conn = None,
                Ok(ClockCmd::Enabled(e)) => enabled = e,
                Ok(ClockCmd::Playing(p)) => playing = p,
                Ok(ClockCmd::Bpm(b)) => bpm = b,
                Ok(ClockCmd::Stop) => return,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
        let want = enabled && playing && conn.is_some();
        if want != last_transport {
            if let Some(c) = conn.as_mut() {
                let _ = if want { c.send_start() } else { c.send_stop() };
            }
            last_transport = want;
        }
        if want {
            if let Some(c) = conn.as_mut() {
                let _ = c.send_clock_tick();
            }
            let interval = 60.0 / (bpm.max(1.0) * 24.0);
            std::thread::sleep(Duration::from_secs_f32(interval));
        } else {
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
