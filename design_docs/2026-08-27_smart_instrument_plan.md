# Smart-instrument control: Woodshed drives a HyVibe guitar

**Date:** 2026-08-27
**Status:** in progress. **W1 is written but cannot be verified inside this
workspace**: `crates/woodshed-instrument` exists, compiles, and passes its
tests when built outside Woodshed, but the Woodshed workspace itself does not
currently resolve — a pre-existing breakage unrelated to this work, recorded
under Findings. The protocol side is done and hardware-verified; see
`antinode/design_docs/2026-08-27_antinode_founding.md`.

---

## What this is

[Antinode](https://crates.io/crates/antinode) is an independent client for the
HyVibe smart guitar — an acoustic with an actuator in the body that makes the
instrument its own amplifier, effects processor, looper and speaker. Its
protocol was recovered from the vendor's app and confirmed against real
hardware on 2026-08-27: the GATT surface, both transports, the JSON-RPC layer,
and thirty-odd methods.

This plan is the Woodshed half. Woodshed already owns a metronome, a tuner, a
looper, a MIDI clock, live-input recording and latency calibration. The guitar
exposes **the same concepts over Bluetooth**. So the two are not neighbours
that need an adapter; they are the same vocabulary on either side of a wire,
and the work is to join them.

## Why this belongs in Woodshed

- The instrument's metronome takes `bpm`, `num`, `den`, `bars` — which is
  Woodshed's metronome, exactly.
- Its tuner, looper and recorder each have a Woodshed counterpart already
  built and already accessible.
- Practice happens at the instrument. A practice toolkit that can *drive* the
  instrument closes a loop no other tool in this workspace closes.

## What the protocol actually offers, and what it does not

Confirmed working against hardware (antinode H4, H7, H9, H13, H14, H17):

| Capability | Method | Note |
|---|---|---|
| Device identity, battery, storage | `GetStatus` | includes both firmware versions |
| Metronome state | `ReadMetronome` | returns live `bpm`/`num`/`den` |
| Metronome control | `Start`/`Stop`/`UpdateMetronome` | |
| Banks (presets) | `ReadBank`, `SwitchBank`, … | |
| Effects | `Add`/`Update`/`Remove`/`MoveEffect` | |
| Equalizer | `SetEQGain`, `SetEQBandGain` | **write-only** |
| Aux routing | `AuxIn`, `AuxOut`, dry/wet | **write-only** |
| Body resonances | `GetAnalysis` | the instrument's measured plate modes |
| Recordings on device | `GetFileInfo`, `DumpFile` | readable by name |
| Controller bindings | `SetController` | maps a knob or pedal to a parameter |

Two constraints to design around rather than discover later:

- **`ReadConfig` is unusable.** It returns nothing and wedges the firmware's
  RPC handler until the guitar is power-cycled (antinode H18). Woodshed must
  never call it, and the client should refuse to.
- **The equalizer and aux settings are write-only.** Nothing reads them back
  (antinode H19), so Woodshed's own last-written values are the only record,
  and the UI must present them as *sent* rather than as *confirmed*.

## Where the code goes

A new crate, **`woodshed-instrument`**: connection lifecycle, device state, and
the mapping between Woodshed's concepts and the wire.

Named for what it does rather than for the vendor — the workspace's naming rule
keeps a vendor's mark out of crate names, and a plain name also leaves room for
a second instrument later without a rename. It sits beside `woodshed-audio` as
a native, I/O-owning crate: `btleplug` and `tokio` make it no more portable than
`cpal` does.

It must not go in `woodshed-core`, which is explicitly portable state with no
native I/O, nor in `woodshed-audio`, whose subject is the local audio path
rather than a remote instrument.

```
crates/woodshed-instrument/     connection, device state, concept mapping
crates/woodshed-views/          the practice-facing surface (later phase)
```

**Dependency form.** Antinode has no git remote yet, so this begins as a path
dependency to `../antinode`. That is a local-development arrangement, not the
end state: the moment antinode is pushed it becomes a git dependency in the
same form as `genet-host-api` and the cambium crates. Recorded here so the path
dep is understood as temporary rather than as a decision.

## Phases

### W1 — The seam, and one honest round trip

**Feature target:** Woodshed can connect to the instrument and read its state.

Done-conditions:

- `woodshed-instrument` exists, depends on `antinode` + `antinode-ble`, and
  builds clean with no warnings.
- A `Connection` type owns discover → connect → disconnect, and cannot leak a
  connection on an error path. (Antinode's probe leaked one and it locked out
  the following run; the same mistake is easy to repeat here.)
- `InstrumentState` holds what has actually been read — identity, firmware
  versions, battery, storage, metronome — and distinguishes *read from the
  device* from *not yet read*, so nothing is displayed as fact by default.
- One integration test or example connects to a real guitar and prints its
  state, run manually and recorded.

### W2 — Metronome, joined

**Feature target:** one metronome, two clocks, and no argument about which is
right.

The instrument has its own metronome and so does Woodshed. Both can be running.
This phase decides and implements what that means rather than leaving two
tempos to drift.

Done-conditions:

- Woodshed can read the instrument's metronome and adopt it, or push its own to
  the instrument. Which direction is the default is a **decision for Mark**,
  not something to settle by implementation.
- Changing tempo or time signature in Woodshed reaches the instrument within
  one bar.
- The UI shows plainly whether the instrument is following Woodshed or not.

### W3 — The surface Woodshed uniquely earns

**Feature target:** the things a desktop can do that the phone app does not.

Candidates, in rough order of value:

- **Bank management as files.** Export, import, diff and version presets on
  disk. The instrument holds them; nothing else does.
- **`GetAnalysis` made visible.** The guitar reports its own measured body
  resonances as filter rows — frequency, gain, Q. Woodshed already draws
  instrument-shaped things; this is a plot of the physical instrument that
  happens to be sitting in the room. On the reference guitar: 106 Hz (the
  Helmholtz air resonance), 228 Hz (principal top-plate mode), 545 Hz, 3760 Hz.
- **`SetSpeakerBiquads`.** Raw biquad coefficients on the speaker path, i.e.
  arbitrary filter design on the instrument's output, with no firmware work.
  Untested and state-changing — needs its own assessment before a first call.
- **Loop retrieval.** `GetFileInfo` + `DumpFile` read recordings off the device
  by name; the vendor offers this only over USB mass storage. Slow over BLE
  (hex doubles the payload, ~200 bytes of file per call), so this is a
  background transfer, not an interactive one.

Done-conditions: **deferred**, set once W1 and W2 land and Mark picks a target.

## Decisions (Mark's)

- **WD1 — Metronome authority.** Does Woodshed follow the instrument, drive it,
  or offer both with an explicit toggle? Affects W2's shape throughout.
- **WD2 — Connection lifetime.** Connect on demand for a single action, or hold
  a session open while Woodshed runs? The instrument serves one client at a
  time, so holding it locks out the phone app.
- **WD3 — Whether to push antinode**, converting the path dep to a git dep and
  making Woodshed buildable by anyone else.

## Open questions

- Does the instrument emit anything unprompted — a knob turn, a bank switch, a
  footswitch press? Nothing unsolicited has been observed, but nothing has sat
  connected and idle for long either. If it does, Woodshed can follow the
  instrument rather than poll it.
- What the `JUNK` chunk in a loop file's WAV header means. Labelled
  "HyVibe loop file", carrying `1, 200, 7, 8, 4, 0`; `8` and `4` look like a
  time signature and bar count, which would let Woodshed import a loop with its
  tempo intact.

## Findings

**Woodshed's workspace does not currently resolve, and it is not this work's
doing.** `cargo build` fails for every crate, including ones untouched here:

```
error: patch location `https://github.com/merely-made/genet.git?branch=main`
       does not contain packages matching `stylo_taffy`
```

Woodshed's root `Cargo.toml` patches `stylo_taffy` from `genet.git` (line ~166,
with a comment explaining that the registry copy names an incompatible taffy),
and genet's `main` no longer provides that package. The local genet checkout
has `support/patches/{gpu-allocator, ipc-channel, parley, sonic-rs-0.5.8,
taffy}` — no `stylo_taffy`. Whether it was renamed, folded into `taffy`, or
dropped is a question for genet, and **this plan does not touch genet**: it is
another repo, and noticing a fault next door is not licence to work there.

Consequence for W1: the crate was verified by building and testing it outside
the Woodshed workspace, where it compiles clean and its five tests pass. That
is real verification of the code and **not** verification that Woodshed
integrates it, which cannot be shown until the workspace resolves again.

## Progress

- **2026-08-27** — Plan written.
- **2026-08-27 — W1 written, verification blocked.** `woodshed-instrument`
  created: `Connection` with a scope-based release that cannot leak on an error
  path, `InstrumentState` whose fields are all optional so unread is never
  displayed as zero, `Metronome` translating the wire's `bpm`/`num`/`den` into
  Woodshed's names once at the boundary, `Resonance` for the instrument's
  measured body modes, and a refusal list that blocks `ReadConfig` in code
  rather than trusting a comment. Five tests, clippy clean, built against the
  captured replies from the reference instrument. Added to the workspace
  members list; the workspace cannot build for the unrelated reason above.
