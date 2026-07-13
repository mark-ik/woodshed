# Project Description

Maintainer-owned. Other project-facing documents derive from this description.

## What it is

Woodshed is an offline-first practice toolkit for guitar and related fretted
instruments. Its chord, scale, arpeggio, progression, exercise, and set-template
catalogs stage material into an ordered practice Set. Rehearsal streams that Set
as guided practice; the Looper repeats it as a backing form for live-input
recording and export. A fretboard, tuner, metronome, MIDI clock, and latency
calibration complete the focused practice environment.

The theory model generalizes across string counts and tunings, so bass, ukulele,
banjo, and custom instruments are valid model inputs rather than a second
product.

## Goals

- **Comprehensive theory coverage** for guitar and related instruments in one
  offline application.
- **Practice, not just reference.** Catalog material can be staged into one Set
  and run through guided Rehearsal or the live-input Looper at tempo.
- **Open source with convenient desktop builds.** Source remains open; signed,
  packaged binaries can later support itch.io or Gumroad distribution.
- **Offline-first.** Core theory, session data, and practice behavior work
  without an account or network connection.

## Product model

### Stage and Set

- Scale, chord, arpeggio, progression, exercise, and set-template catalogs over
  one musical context.
- Named scale and chord catalogs, formulas, root selection, tunings, and
  fretboard position mapping.
- One ordered, heterogeneous Set built by staging configured material, with
  dwell, touch, instrument, tuning, and fret-window controls.
- Corpus search across the catalogs.

### Practice and playback

- Metronome playback and on-demand voicing previews for current material.
- Guided Rehearsal with Set looping and metronome-synchronized fretboard
  articulation.
- Set-derived Looper with repeat transport, click, live-input recording,
  replace/overdub, and WAV export.
- Tuner input, latency calibration, and native MIDI input/output with clock
  sync where devices are available.

### Personalization and local state

- Seed-derived Slate, Ember, Light, Dusk, Meadow, and Parchment themes.
- Persisted session state for selections, tempo, theme, layout, Set, and Looper
  configuration.
- Fretboard layout choices: two pane, hero, and full canvas.
- Canonical Settings pages for each product section plus appearance, instrument,
  tuning, audio, MIDI, and accessibility.

## Delivery status

The current app is a Windows desktop alpha hosted by Genet. It has a shared,
adaptive product view layer, but Mac and Linux receipts have not been completed.
A tagged build can produce a checksummed portable Windows ZIP; code signing,
installer UX, an app icon, and third-party notice aggregation remain before a
broad public release.

The browser/PWA host is planned from the same view layer. Web Audio, OPFS,
accessibility exposure, and browser deployment are not shipped product features.
Mobile follows the web host and is not a current delivery target.

## Distribution path

1. GitHub-tagged portable Windows alpha builds.
2. Signed Windows package, then itch.io / Gumroad distribution.
3. Verified Mac and Linux builds and packages.
4. Browser/PWA deployment, then mobile shells if it earns the work.

## Tech stack

- **Language**: Rust.
- **Product views**: `xilem-serval`, a DOM-shaped Xilem backend.
- **Layout and paint**: Genet layout, PaintList, and netrender over wgpu.
- **Desktop host**: winit 0.30.
- **Audio I/O**: cpal 0.18, with in-house sequencing and DSP helpers.
- **Pitch detection**: `pitch-detector` and `pitch-detection`.
- **MIDI**: `midir` on the native host.
- **Theory**: in-house `woodshedding` crate, not an external theory library.

## Non-goals for the initial public release

- Real-time effects processing.
- Multi-track recording or a full DAW workflow.
- Notation/staff rendering.
- Collaborative or networked practice features.
