# Smart-instrument control: Woodshed drives a HyVibe guitar

**Date:** 2026-08-27
**Status:** in progress. **W2 written; one hardware condition outstanding.**
**W1 landed** — `crates/woodshed-instrument` builds
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
  the instrument. — **met 2026-08-27.** `MetronomeLink` has three states and
  `Session::sync_metronome` acts on whichever holds. Default is `Detached`,
  so connecting never changes an instrument's settings by itself.
- Changing tempo or time signature in Woodshed reaches the instrument within
  one bar. — **written, not yet proven on hardware.** The write path is a
  single RPC, and a `GetStatus` round trip measured well under a second, so
  latency is not in doubt; what is unproven is the round trip itself. The
  example `metronome_link` is the proof and needs a woken guitar.
- The UI shows plainly whether the instrument is following Woodshed or not. —
  **met 2026-08-27.** `MetronomeLink::describe` returns a phrase naming both
  sides ("Instrument follows Woodshed"), and a test asserts every variant does
  so rather than echoing a variant name at the player.

**Design notes worth keeping.**

- **The link is one setting with three states, not two switches.** Two
  independent "follow" toggles would allow both to be on, and two metronomes
  each adopting the other is a loop that drifts rather than settles. A test
  asserts no state lets both sides lead — the loop-freedom argument checked
  rather than trusted to prose.
- **`Detached` is the default.** Connecting to an instrument should not begin
  changing its settings.
- **Changing the link forgets what was last pushed**, so switching back
  re-sends. The instrument has knobs on it, and assuming it was left untouched
  while Woodshed was not driving it would be wrong.
- **`Drive` writes only on change**, so holding a steady tempo costs one write
  and then nothing, which is what makes `sync_metronome` safe to call on a
  timer.
- **`Session` cannot release itself.** Releasing is a real exchange with the
  device and Rust has no async `Drop`, so `release` is explicit and consuming.
  `Drop` prints a warning if it was skipped, because a silently-held instrument
  fails the *next* attempt and the symptom appears somewhere unrelated — this
  project has already lost a session to exactly that.

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

Done-conditions, now that W1 and W2 have landed:

- **Loop retrieval works and is checksum-verified.** — **met 2026-08-27,
  against hardware.** `/Loops/loop0031.wav` was pulled off the instrument in
  full: 741,468 bytes, **checksum verified**, and the file on disk is a valid
  WAV whose internal RIFF length agrees with what `GetFileInfo` reported —
  8.41 s of mono 44.1 kHz 16-bit audio, with the vendor's `JUNK` chunk intact
  ahead of `fmt ` and `data`.
- **The resonance report is readable.** — **met.** `Connection::read_resonances`
  returns the instrument's measured body modes as typed rows.
- **Bank management as files** — deferred; the reference instrument has no
  banks, so there is nothing to export until one exists.
- **`SetSpeakerBiquads`** — assessed below and **not attempted**.

### Findings from building it

**The checksum is not the CRC-32 you would reach for.** `GetFileInfo` reports
one computed MSB-first over the unreflected polynomial `0x04C11DB7` from an
all-ones start with no final inversion — CRC-32/MPEG-2. A stock `crc32` crate
returns a different number, so a perfectly intact download would appear
corrupt. Implemented in `ringdown::crc32` with the table generated from the
polynomial and pinned by the catalogue's published check value.

**`GetLastRecordingName` does not name the last recording.** With 31 loops it
answers `loop0032.wav`, which does not exist; `GetFileInfo` on it fails.
`latest_recording_name` steps back one and confirms the file opens, so callers
get a name they can actually use.

**Throughput is the real constraint, now measured.** 741,468 bytes took
**620.7 seconds — about 1.2 KB/s across roughly 3,700 round trips.** Replies
are hex, so every byte costs two characters, and one reply carries about two
hundred bytes of file. A ten-minute wait for eight seconds of audio. That makes retrieval a background transfer with progress, not an interactive
one — a design constraint on any UI over it rather than a detail. It also
means bulk archival of 31 loops is a five-hour job, so the sensible product
shape is fetching a chosen take rather than syncing everything.

**The checksum identification is confirmed by this run.** A wrong polynomial
would have failed a download that arrived perfectly; CRC-32/MPEG-2 verified
first time over three-quarters of a megabyte.

### `SetSpeakerBiquads`: assessed, not attempted

The plan called for an assessment before a first call, and this is it. The
conclusion is **do not call it yet**, on evidence rather than nerves.

What is known: the name is in the firmware's keyword dictionary, so the string
exists. Nothing else. Its parameters are unmapped, its units are unknown, and
whether it is even implemented on this firmware is untested — `GetLevels` sat
in that same dictionary and turned out not to exist.

What makes it different from the other unknowns is the failure mode. It writes
filter coefficients to the speaker path, which is the actuator driving the
instrument's own top. A wrong biquad is not a wrong number in a display; it is
an unstable filter on a transducer glued to a soundboard. And this project has
already demonstrated, with `den`, that this device accepts writes it does not
apply and returns `true` regardless — so a call that appears to succeed proves
nothing, and there is no read-back to check it against.

The order that would make it safe, if it is ever wanted:

1. Establish whether the method exists at all, by calling it with parameters
   deliberately malformed enough to be rejected, and reading the error.
2. Find the parameter shape from the error messages rather than by guessing
   coefficient arrays.
3. Establish a way to *undo* it — `LaunchCalibration` may reset the filter
   bank, but that is a guess and needs its own confirmation — **before**
   writing anything real.
4. Only then write a coefficient set, at a volume where an unstable filter is
   an annoyance rather than damage.

Step 3 is the one that matters. Nothing should be written to this path until
there is a demonstrated way back, because the lesson `den` taught was cheap
only by luck.

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

**W2 hardware run, 2026-08-27: the link works, and `den` does not.**

The metronome link is confirmed in both directions. `Detached` touched
nothing; `Follow` read the instrument's tempo; `Drive` sent 96 bpm and the
instrument reported 96 bpm back through a separate read; a second identical
sync correctly wrote nothing. **The done-condition is met: a tempo set in
Woodshed reaches the instrument.**

**But the restore failed, and an instrument was left changed.** The run read
`{bpm: 60, num: 5, den: 8}`, wrote its own values, then wrote the original
back — and the instrument came back `den: 4`. Four further attempts could not
restore it: `den: 8` sent alone is refused outright (`UpdateMetronome` returns
`false`), and sent alongside `bpm`, `num` and `bars` it returns `true` and
changes nothing. `den` moved 8 → 4 and would not move back.

**The naming was wrong, and the instrument's own display proves it.** The
guitar read **5/4 before the run and 5/4 after**, while the wire's `den` went
from 8 to 4. A field that changes by half while the displayed denominator does
not move is not the denominator. Its meaning is unknown.

`num` *is* the numerator: it went 5 → 4 → 5 across the run and the display's
leading number tracked it. So of the three fields, `bpm` and `num` are
understood and round-trip, and `den` is neither.

Every reading in this plan and in ringdown's Findings that described the
instrument as being "in 5/8" was an interpretation of `den`, not an
observation, and it was wrong. The instrument was in 5/4 throughout.

**Nothing user-visible was actually disturbed** — which is luck rather than
care, since the write went out before anyone knew what the field did.

**What changed as a result:**

- `Connection::set_metronome` no longer sends `den` at all. It writes `bpm` and
  `num` and leaves the third field alone.
- `Metronome::beat_unit` is documented as the raw wire value with unknown
  meaning, explicitly not a denominator, and explicitly never written.
- The hardware example still restores what it read, which now succeeds because
  it no longer tries to write the field it cannot restore.

**The lesson is one this project keeps relearning, in its most expensive
form yet.** A field was named `beat_unit` because `den` looked like
"denominator", the name was believed, and it was then *written to someone's
instrument*. Reading an unknown field costs nothing; writing one costs a
setting you may not be able to put back. **Do not write a field whose
round-trip has not been demonstrated** — and demonstrate it by reading back,
not by trusting a `true` return, which this device gives for writes it
silently ignores.

**Still open:** what `den` actually encodes. Mapping it means writing candidate
values and reading the instrument's *screen*, which is the only ground truth,
and that is the owner's call rather than something to sweep.

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
- **2026-08-27 — W2 written.** `session` module: `MetronomeLink`
  (Detached/Follow/Drive), `SyncOutcome`, and `Session` holding the instrument
  across actions with an explicit `release`. Thirteen tests, clippy clean. The
  hardware proof (`examples/metronome_link`) is written and builds; it reads
  the instrument's tempo, drives a different one, reads it back, and restores
  what it found. Not yet run — the guitar was asleep.
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
