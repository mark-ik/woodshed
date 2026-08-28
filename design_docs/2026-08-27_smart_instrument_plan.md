# Smart-instrument control: Woodshed drives a HyVibe guitar

**Date:** 2026-08-27
**Status:** in progress. **W1 landed** — `crates/woodshed-instrument` builds
and tests clean inside the workspace. Getting there required repairing three
stale genet overrides that had been failing the whole workspace; see Findings.
The protocol side is done and hardware-verified; see
`ringdown/design_docs/2026-08-27_ringdown_founding.md`.

---

## What this is

[Ringdown](https://crates.io/crates/ringdown) is an independent client for the
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

Confirmed working against hardware (ringdown H4, H7, H9, H13, H14, H17):

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
  RPC handler until the guitar is power-cycled (ringdown H18). Woodshed must
  never call it, and the client should refuse to.
- **The equalizer and aux settings are write-only.** Nothing reads them back
  (ringdown H19), so Woodshed's own last-written values are the only record,
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

**Dependency form.** Git dependencies on `merely-made/ringdown`, in the same
form as `genet-host-api` and the cambium crates. It began as a path dependency
while ringdown had no remote and was converted on 2026-08-27 once it was
pushed. Both halves come from the repo rather than one from crates.io, because
`ringdown-ble` is `publish = false` and the two must stay in step.

## Phases

### W1 — The seam, and one honest round trip

**Feature target:** Woodshed can connect to the instrument and read its state.

Done-conditions:

- `woodshed-instrument` exists, depends on `ringdown` + `ringdown-ble`, and
  builds clean with no warnings.
- A `Connection` type owns discover → connect → disconnect, and cannot leak a
  connection on an error path. (Ringdown's probe leaked one and it locked out
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

- ~~**WD1 — Metronome authority.**~~ **Settled 2026-08-27: both, with an
  explicit toggle.** Woodshed can follow the instrument or drive it, and which
  is in force is a visible state rather than an implicit one. W2 implements the
  toggle rather than choosing a direction.
- ~~**WD2 — Connection lifetime.**~~ **Settled 2026-08-27: hold a session, and
  release on demand.** The connection lives as long as Woodshed wants it, with
  an explicit release so the phone app can be handed the instrument back
  without quitting. That makes releasing a first-class action rather than a
  side effect of shutdown — and `Connection::with` as written is
  scope-shaped, so W2 needs a longer-lived form beside it.
- ~~**WD3 — Whether to push ringdown.**~~ **Settled 2026-08-27:** pushed to
  `merely-made/ringdown` and published as `ringdown` 0.1.0 (MPL-2.0). Woodshed
  now takes it as a git dependency, so this workspace is buildable by anyone
  with the genet checkout.

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

**The workspace had stopped resolving, and one upstream commit explains all of
it.** Every crate failed to build, including untouched ones. Three of Woodshed's
overrides pointed at things genet had removed in a single commit,
`55c05d11759 "Retire Stylo and the incumbent layout cone"`:

| Stale entry | Where | Why it broke |
|---|---|---|
| `stylo_taffy` | `Cargo.toml` patch + `.cargo/config.toml` | genet dropped Stylo; the package is gone |
| `taffy` | `Cargo.toml` patch + `.cargo/config.toml` | genet's fork is now the renamed `genet-taffy` |
| `genet-layout` | `.cargo/config.toml` + a `[profile.dev.package]` | the layout cone was retired |

All four fixes are deletions. Nothing in Woodshed depends on any of them:
`stylo_taffy` and `taffy` were patches with no dependents once genet renamed
its vendored forks with a `genet-` prefix, and Woodshed's own `CLAUDE.md`
already forbids depending on `genet-layout` directly. The entries outlived
their subjects.

**The repair stayed inside Woodshed.** The fault was diagnosed in genet but
every change is to Woodshed's own manifests, which is the difference between
fixing your own stale references and wandering into a neighbouring repo. The
lingering `components/genet-layout/` directory in the genet checkout, which has
no `Cargo.toml`, is genet's business and was left alone.

**Worth carrying forward:** a sibling repo retiring a component leaves this
kind of debris in every consumer that pins it locally, and the failure surfaces
as total — the workspace does not resolve *at all*, so it reads far more
alarming than "one retired component is still referenced". Any future genet
retirement should expect a sweep of consumers' `.cargo/config.toml` overrides,
which are the copies most easily forgotten because they are not committed
alongside the dependency that motivated them.

## Progress

- **2026-08-27** — Plan written.
- **2026-08-27 — W1 LANDED.** Verified inside the workspace: builds clean,
  five tests pass, no clippy findings in this crate. Required removing three
  stale genet overrides first (Findings).
- **2026-08-27 — W1 written, initially unverifiable.** `woodshed-instrument`
  created: `Connection` with a scope-based release that cannot leak on an error
  path, `InstrumentState` whose fields are all optional so unread is never
  displayed as zero, `Metronome` translating the wire's `bpm`/`num`/`den` into
  Woodshed's names once at the boundary, `Resonance` for the instrument's
  measured body modes, and a refusal list that blocks `ReadConfig` in code
  rather than trusting a comment. Five tests, clippy clean, built against the
  captured replies from the reference instrument. Added to the workspace
  members list; the workspace cannot build for the unrelated reason above.
