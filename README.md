# Woodshed

A guitarist's practice toolkit, named for the musician's term for focused
solitary practice ("woodshedding"). It provides a tuner, comprehensive chord and
scale libraries with interval formulas, chord progressions, classic practice
exercises, a metronome that extends into a simple drum machine, and a Practice
Mode that walks the user through rotations of musical material at tempo so the
app drives the session. The theory model generalizes across stringed instruments
(bass, ukulele, banjo) by parameterizing string count and tuning.

Built in Rust with [Xilem](https://github.com/linebender/xilem) +
[Masonry](https://github.com/linebender/xilem/tree/main/masonry) for the UI
(migrated from Iced on 2026-05-18). Desktop first (Windows, macOS, Linux), with a
planned path to web and mobile.

This is a Cargo workspace and an internal developer repository, not a published
library. The crates are not on crates.io (most are marked `publish = false`).

**Made with AI**

## Status

Pre-alpha, under active development. The current direction is a UI redesign pass
(GPUI-quiet chrome, Slate and Ember built-in palettes, segmented-pill navigation,
fretboard-layout setting, redesigned Rehearsal and Practice screens). See
`design_docs/2026-06-15_redesign_plan.md` for the active plan and
`design_docs/DOC_README.md` for the full plan index.

## Workspace layout

```
crates/
  woodshedding/      Portable gerund core: pure theory, instrument realization,
                     progressions, exercises, fretboard mapping, voicings,
                     practice/rehearsal sets. No I/O, UI, or audio.
  woodshed-audio/    Real-time audio engine: cpal-backed sequencer/click,
                     tuner-grade pitch detection, MIDI, looper, song engine,
                     latency calibration, offline render.
  audio-primitives/  Shared, framework-agnostic DSP primitives (click synth,
                     onset/tempo detection, latency estimation, buffer shaping).
                     Pure std, no engine or UI dependency.
  xilem-components/  Domain-neutral, product-agnostic Xilem/Masonry UI
                     components (combobox, ...). The design-system layer.
  audio-widgets/     Shared Masonry/Vello audio widgets (waveform, meter,
                     fader, knob). Audio-domain layer above xilem-components.
  woodshed-xilem/    The Xilem + Masonry application. Depends on all of the
                     above. This is the binary crate.
design_docs/         Plans, doc policy, project description.
```

### Crate roles and dependency direction

- `woodshedding` is the pure operation core (no `cpal`, no UI, no file I/O). It
  owns the canonical model of pitches, intervals, tunings, scales, chords,
  progressions, exercises, fretboard mappings, voicings, and practice/rehearsal
  sets. Keep it pure; UI- or audio-coupled code belongs in the consuming crates.
  The name follows the gerund-crate convention used across these repos
  (`murmuring`, `mooting`, etc.): the gerund names the portable core that makes
  the activity possible.
- `audio-primitives` is pure `std` with no dependencies. If something takes plain
  sample slices or timestamps and returns plain data it belongs here; anything
  that owns an audio stream or live engine is a "driver" and lives in the
  consumer.
- `woodshed-audio` builds the cpal-backed driver layers on top of
  `audio-primitives` (output sequencer/click, input pitch/onset analyzers, MIDI,
  looper, song engine, offline render, calibration session).
- `xilem-components` is the audio-agnostic, product-agnostic UI component layer.
- `audio-widgets` is the audio-domain widget layer, one layer above
  `xilem-components`.
- `woodshed-xilem` is the application: it depends on `woodshedding`,
  `woodshed-audio`, `audio-widgets`, and `xilem-components`.

`audio-primitives`, `audio-widgets`, and `xilem-components` are shared
infrastructure crates intended for cross-repo reuse by sibling projects in the
Strophos family (Strophe, Mere) via path dependencies. Within this workspace they
are consumed by `woodshed-audio` and `woodshed-xilem`.

## Build and run

```
# Build the whole workspace
cargo build

# Run the application
cargo run -p woodshed-xilem

# Run all tests
cargo test
```

The binary target is `woodshed-xilem` (`crates/woodshed-xilem/src/main.rs`).

### Toolchain and platform notes

- Workspace edition is 2021 with `rust-version = "1.80"` for the
  workspace package; the Xilem-facing crates (`xilem-components`,
  `audio-widgets`, `woodshed-xilem`) use edition 2024 and require Rust 1.92.
- On Windows MSVC, the Xilem generic view types can overflow the per-symbol PDB
  budget (LNK4319). This is handled by the `/DEBUG:LongSymbolTruncate` linker
  flag set in `.cargo/config.toml`.

## UI dependency: the Xilem fork

The UI rides a lean fork branch rather than crates.io:

```
xilem        = { git = "https://github.com/mark-ik/xilem.git", branch = "woodshed-theme", version = "0.4.0" }
masonry      = { git = "https://github.com/mark-ik/xilem.git", branch = "woodshed-theme", version = "0.4.0" }
masonry_winit = { git = "https://github.com/mark-ik/xilem.git", branch = "woodshed-theme", version = "0.4.0" }
```

The `woodshed-theme` branch is `upstream/main` plus exactly one commit: the
PR #1822 commit `WindowView::with_default_properties` (no-restart retheming).
The Masonry-side counterpart (PR #1821 `set_default_properties`) is already
upstream, so no Masonry patch is carried; the combobox/dropdown UI uses
upstream's own `selector` view and `Selector`/`SelectorMenu` widgets, not a
fork addition. It rides upstream's
wgpu-28 / vello-0.8 (not the mere/serval wgpu-29 fork). Git deps (not path) keep
the app buildable on any machine without a local fork worktree; `Cargo.lock` pins
the exact commit. To iterate on the fork locally, override with a machine-local
`paths = [...]` in a parent `.cargo/config.toml` rather than editing the manifest.
Keep Xilem's default features enabled (including `imaging_vello`); disabling them
panics the Masonry imaging layer at first paint.

See `design_docs/xilem_fork_patches.md` for the ledger of fork edits.

## Key dependencies

| Area            | Crate / version                                  |
|-----------------|--------------------------------------------------|
| UI              | xilem / masonry / masonry_winit 0.4.0 (forked)   |
| Windowing       | winit 0.30                                        |
| Audio I/O       | cpal 0.15                                          |
| Pitch detection | pitch-detector 0.3 (hinted), pitch-detection 0.3 |
| WAV I/O         | hound 3.5                                          |
| MIDI            | midir 0.11                                         |
| Async runtime   | tokio 1.50 (app crate)                            |
| Config dirs     | directories 5 (app crate)                         |
| Persistence     | serde 1.0, serde_json 1.0                         |

The McLeod (NSDF) detector from `pitch-detection` is used for
octave-confusion-resistant tuning on guitar low strings; raw-frequency to
note/cents conversion is done locally. `fundsp` (richer synthesis) and
`symphonia` (FLAC/OGG/MP3 sample loading) are noted as future, deferred
dependencies.

## Project policy notes

- The theory model is owned. Do not depend on `rust-music-theory` or similar
  upstream theory crates; their scale and chord coverage is insufficient and the
  project needs exotic scales, non-tertiary chords, and arbitrary tunings.
- The theory model must support arbitrary string counts and tunings; do not
  hard-code 6-string assumptions.
- Plans live in `design_docs/` under the `YYYY-MM-DD_<keyword>_plan.md`
  convention. Read `design_docs/DOC_README.md` first; follow `DOC_POLICY.md` for
  documentation changes.

## Relationship to sibling repos

Woodshed sits under the Strophos parent brand alongside Strophe (a collaborative
loop recorder) and Mere. Woodshed is a pressure vessel for shared audio and UI
infrastructure: stable pieces are factored into `audio-primitives`,
`audio-widgets`, and `xilem-components`, which Strophe and Mere consume. The
dependency direction is one-way (siblings consume Woodshed's shared crates).

## License

Dual-licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
