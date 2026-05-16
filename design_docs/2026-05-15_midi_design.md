# MIDI Design Note

Brief design note covering MIDI in/out for Woodshed. Captures the
decisions that matter — transport ownership, clock sync, what we do
NOT do — before any code lands. Implementation lives in
`crates/woodshed-audio/src/midi.rs`.

---

## What MIDI is good for here

**MIDI Out (Woodshed is the source):**
- Trigger an external drum machine or soft-synth in time with the
  Woodshed metronome / sequencer.
- Send MIDI Clock so external gear locks to our tempo.
- Send Transport (Start/Stop) so external gear plays/pauses with us.

**MIDI In (Woodshed is the listener):**
- Receive MIDI Clock from a DAW or hardware sequencer and slave
  Woodshed's transport to it. Letting the user jam *with* their DAW
  is high-value: you start your DAW project, Woodshed locks in.

We do **not**:
- Use MIDI to trigger Woodshed sounds via Note On/Off. Woodshed is a
  practice tool, not a synth. If a user wants a MIDI-controlled
  drum machine, they should use a real drum machine.
- Accept MIDI Note input to "play along" notes. Onset detection from
  audio is more useful for a guitarist — and a fretted-instrument
  player isn't going to be plugging in a MIDI controller anyway.

## Transport ownership — who's the master?

A user-facing setting picks one of three modes:

| Mode | Master | Clock source | Transport source |
|------|--------|--------------|-----------------|
| **Internal** (default) | Woodshed | Internal | Internal |
| **MIDI Clock master** | Woodshed | Internal, sent on Out port | Internal, sent on Out port |
| **MIDI Clock slave** | External | Received on In port | Received on In port |

The default has to be Internal — a fresh install with no MIDI gear must
just work. The two MIDI modes are opt-in and surface in a "MIDI" tab
or settings panel.

In Clock-slave mode, the BPM slider and Tap button are display-only;
Play/Stop in the UI still works but its semantics shift to "arm" — the
sequencer waits for an incoming MIDI Start before producing audio.

## MIDI Clock specifics

MIDI Clock is 24 PPQN (pulses per quarter note). Wall-clock interval
between consecutive ticks gives:

```
secs_per_tick = (t_n - t_{n-1})
secs_per_beat = secs_per_tick * 24
bpm           = 60 / secs_per_beat
```

In practice we average over ~24 ticks (one beat) and smooth with an
exponential moving average so transient jitter from the host doesn't
make our displayed BPM dance.

Transport messages we care about:
- `0xFA` Start — set play position to beat 1, begin playing.
- `0xFB` Continue — begin playing from wherever we are.
- `0xFC` Stop — pause.

## Out-of-scope (this pass)

- Sysex / device-specific config.
- MTC (MIDI Time Code) — frame-accurate sync. Different use case
  (post-production), not jamming. If someone files an issue we can
  revisit.
- MIDI 2.0. Tooling support is still spotty in 2026.

## Implementation plan

1. **Enumeration** — `list_input_ports()`, `list_output_ports()`
   returning `Vec<String>` of human-readable names. No device opening.
2. **`MidiOut`** — connect once, send raw bytes. Convenience methods:
   `send_note_on`, `send_note_off`, `send_clock_tick`, `send_start`,
   `send_stop`, `send_continue`.
3. **`MidiIn`** — connect once with a callback that forwards parsed
   `MidiEvent` values into a shared queue. UI thread polls the queue.
4. **`MidiClockSync`** — pure helper. `fn record_tick(at: Instant)`
   and `fn estimated_bpm() -> Option<f32>`. Tested without hardware.
5. **No app-side wiring this pass** — the audio crate exposes the
   types; integration with `SequencerEngine` transport and the UI
   surface comes after the user sees the shape.

## Why this order

The pure helpers (`MidiClockSync`, parsing) are testable without
hardware and capture the algorithm. The `MidiOut` / `MidiIn` wrappers
are thin enough that platform-CI absence isn't a blocker — once they
compile and connect, the runtime behavior is in midir's hands.
