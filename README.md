# Woodshed

Woodshed is an offline-first practice toolkit for guitar and other fretted
instruments. Its catalogs stage chords, scales, arpeggios, progressions, and
exercises into an ordered practice Set. Rehearsal streams that Set as guided
practice; the Looper repeats it for live-input recording and export. A
fretboard, tuner, metronome, MIDI clock, and latency calibration round out the
practice environment.

The theory model supports arbitrary string counts and tunings, so bass,
ukulele, banjo, and custom instruments are first-class rather than afterthoughts.

Built in Rust on [Genet](https://github.com/mark-ik/genet): a shared
DOM-shaped Xilem view tree is laid out and painted by Genet/netrender, with a
winit desktop host today and a browser host planned from the same view layer.

**Made with AI**

## Status

Desktop alpha. The Genet migration is complete on the Woodshed side and the
Windows host is functional, but this is not a packaged public release yet.
The next release work is adaptive-screen polish, Mac and Linux receipts,
packaging, and a current public build/install path. Browser and mobile hosts are
separate work, not implied by the shared view layer.

See [the Genet host plan](design_docs/2026-07-04_genet_host_cross_platform_plan.md)
for the delivery architecture and [the documentation index](design_docs/DOC_README.md)
for the wider project record.

## Workspace layout

```
crates/
  woodshedding/      Pure musical theory and playable-practice model.
  audio-primitives/  Framework-independent DSP helpers.
  woodshed-audio/    cpal-backed audio, pitch/onset analysis, MIDI, looping,
                     calibration, and offline render.
  woodshed-core/     Portable application state and host seams.
  woodshed-views/    Shared xilem_serval product views and CSS themes.
  woodshed-genet/   Desktop winit + netrender host. The application binary.
  woodshed-graph/    Theory catalog projection into the chartulary graph.
design_docs/         Product description, plans, and documentation policy.
```

### Dependency direction

- `woodshedding` remains pure: no UI, audio device, or filesystem dependency.
- `audio-primitives` is pure `std`, including click/onset/calibration,
  min/max waveform projection, and configurable meter display ballistics;
  `woodshed-audio` owns real-time drivers.
- `woodshed-core` owns portable application state, persistence payloads, and
  host-facing seams. It does not own desktop windowing or browser APIs.
- `woodshed-views` owns product composition and responsive presentation.
  It contains neither desktop window chrome nor audio drivers.
- `woodshed-genet` owns the desktop frame, winit event loop, CSD resize/drag
  behavior, native storage, audio, and MIDI realization.

`audio-primitives` is shared infrastructure for sibling Merely projects. The
old Masonry-specific UI crates and Woodshed's Xilem app have been retired.

## Build and run

```powershell
# Build the desktop app
cargo build -p woodshed-genet

# Run it
cargo run -p woodshed-genet

# Run the workspace tests
cargo test --workspace
```

The binary target is `woodshed-genet` in
`crates/woodshed-genet/src/main.rs`. Local development may use gitignored
`.cargo/config.toml` patches for sibling Genet, netrender, and tincture
checkouts; the committed manifest resolves those dependencies from their owned
Git repositories.

## Key dependencies

| Area | Crate / role |
|---|---|
| Product views | `xilem-serval` over Genet's `ScriptedDom` |
| Layout and paint | `genet-layout`, `paint-list`, `netrender` |
| Desktop host | `winit` 0.30, `wgpu` 29 |
| Audio I/O | `cpal` 0.18 |
| Pitch detection | `pitch-detector` and `pitch-detection` |
| MIDI | `midir` |
| Persistence | `serde`, `serde_json`, `directories` |

The tuner uses the McLeod/NSDF detector for reliable low-string readings;
frequency-to-note and cents conversion remain local to the project.

## Publishing posture

The code is open source under the licenses below. Tagged `v*` builds produce a
checksummed portable Windows ZIP on GitHub. It is intentionally an alpha
artifact: installer UX, code signing, an application icon, and third-party
license notices are still release work. Until the first tagged build is
published, treat source builds as the supported way to try Woodshed.

## Project policy

- The theory model is owned. External theory crates do not cover the required
  combination of exotic scales, non-tertiary chords, and arbitrary tunings.
- Do not hard-code six-string guitar assumptions.
- Keep the shared product view thin and platform-neutral. Browser and desktop
  hosts own their respective frames and native capabilities.
- Plans live in `design_docs/`. Start with `DOC_README.md` and follow
  `DOC_POLICY.md` for documentation changes.

## License

Dual-licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
