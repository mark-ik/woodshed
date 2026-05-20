# Xilem Migration Plan

Captures the path from the foundation scaffold in `crates/woodshed-xilem`
to feature parity with the iced build, plus the eventual deletion of
the iced crate.

The iced crate (`crates/woodshed`) stays in the workspace and stays
shippable throughout the migration — both binaries build, both run.
That's the "side-by-side, then swap" pattern.

---

## What's done (scaffold)

- New crate `crates/woodshed-xilem` in the workspace.
- Path-dep on the local xilem checkout at `../xilem`.
- Binary: tab bar, 8 tab variants, placeholder content per tab.
- Scales tab is the first vertical-slice port: catalog selection,
  scale name + interval display, prev/next navigation.
- Workspace bumped `midir` from 0.10 → 0.11 to resolve a `windows`
  crate version conflict with wgpu 28 (a transitive dep of vello /
  masonry). midir 0.11 still works with the iced build.

## Migration ladder

The order matters: each step lights up capabilities the next step
depends on. Roughly:

### 1. Custom widgets — `FretboardWidget` first

Masonry's `Widget` trait gives finer control than iced's
`canvas::Program`. For our use cases (fretboards, chord diagrams,
beat-wheel, level/cents meters), each becomes a custom Masonry
widget implementing `paint(&mut self, ctx, props, painter)`. The
drawing math (geometry, color mapping, label positioning) transfers
unchanged — only the painting calls swap from `canvas::Frame::stroke()`
to `Painter::stroke().draw()`.

Order:
1. **FretboardWidget** — load-bearing for Scales, Chords, Exercises,
   Practice, Progressions. Once this widget exists, four tabs unblock
   simultaneously.
2. **ChordDiagram** — for the voicing cards and the progression
   expanded chord. Smaller than FretboardWidget; mostly a reskin of
   the existing drawing logic in `crates/woodshed/src/main.rs`.
3. **CentsMeter + LevelMeter** — for the Tuner tab. Trivial; ~60
   lines each.
4. **BeatWheel** — for Practice tab timing feedback. Maybe ~200 lines.

For each: implement Widget trait, expose a builder method on the
Xilem side that constructs `NewWidget<MyCustomWidget>`.

Estimate: 1-2 days for the fretboard, half a day each for the rest.

### 2. Audio integration

Pure plumbing — the audio engines (`SequencerEngine`, `SongEngine`,
`InputEngine`, plus their handles) are already framework-agnostic.
The migration is just owning the engine structs on `AppState` and
threading the handle messages through `text_button` callbacks (or
custom hover/press handlers when we want richer interactions).

Subscriptions / timer-driven state (tuner tick, practice tick,
metronome tick, calibration tick, song tick) move to Xilem's
`task()` view — async tasks that send messages back to state via
the proxy.

The Iced build uses `iced::time::every(Duration::from_millis(50))`;
Xilem equivalents use `tokio::time::interval` inside a `task()` that
loops on the proxy.

Estimate: 1 day for transport tabs (Metronome, Practice), 1 day for
the Song tab's wiring.

### 3. Tab-by-tab feature parity

Once widgets + audio are in place, port the tabs in order of
increasing complexity:

1. **Tuner** — small surface; pitch handle + CentsMeter + LevelMeter
   + target picker.
2. **Scales** — extend the current scaffold with FretboardWidget +
   instrument/tuning row + label-mode picker.
3. **Chords** — chord catalog picker + chord-tone fretboard +
   voicing cards (use ChordDiagram).
4. **Exercises** — exercise catalog + step viewer + position picker.
5. **Progressions** — most complex non-Song tab. Chord-card grid +
   expansion + voice navigation.
6. **Metronome** — straightforward once audio integration lands.
7. **Practice** — practice-set picker + transport + beat indicator +
   beat-wheel timing feedback + calibration widget.
8. **Song** — biggest. Bar list + per-bar editor + transport + ops.

Estimate: ~2 days per tab average. Faster as patterns settle in.

### 4. Polish + cross-cutting

- **Theme system** — port the 5 `DiagramTheme` palettes. Xilem has
  a more capable styling system; the palette structure stays the
  same.
- **Tab visibility-gated analyzers** — pitch and onset analyzers
  enable/disable based on tab. Move the existing logic over.
- **Calibration widget** — reusable component shared across rhythmic
  tabs.
- **Save/load** — file-picker via `rfd` works in both UIs. Probably
  port to Xilem first since that's where new work lands.

### 5. Web target (xilem_web)

Once native parity is achieved, evaluate the web target. This is
**a separate effort** with its own architecture decisions:

- **Audio**: `cpal` doesn't compile to wasm. Need a `trait
  AudioBackend` abstraction with cpal (native) + Web Audio API
  (browser) implementations. Roughly 2 weeks of work, useful
  regardless of UI framework.
- **MIDI**: `midir` ↔ Web MIDI API. Partial browser support
  (Chrome/Edge only). Probably ship native MIDI first, web MIDI
  later.
- **View vocabulary**: xilem_web uses DOM elements (div, button,
  audio, canvas, SVG); native xilem uses Masonry widgets (flex,
  prose, sized_box). The state + business logic stays shared, but
  view fns get a per-platform variant. Plan a shared `view-core`
  crate that exposes platform-agnostic state + transition logic,
  with `view-native` and `view-web` thin wrappers per backend.
- **Custom widgets**: fretboards / chord diagrams render via
  xilem_web SVG (peniko/kurbo geometry → SVG paths). Algorithms
  unchanged.

Estimate: 3-4 weeks once native is stable, including audio backend
work.

### 6. Mobile (Android, then iOS)

- **Android**: Xilem already ships Android examples
  (`xilem/examples/android/*.rs`); each is a ~26-line wrapper
  around the shared desktop binary. Once `woodshed-xilem` runs on
  desktop, adding Android is mostly assembling the
  `android-native-activity` entry point and packaging via
  `cargo-apk`. `cpal` has Android support via AAudio.
- **iOS**: Xilem doesn't ship iOS today. The PWA route via
  xilem_web works on iOS Safari with audio caveats. A native iOS
  port would mean upstream work (probably contributable to
  Linebender — winit has iOS support, so the path is
  "ports masonry_winit's app shell to iOS, then everything else
  follows"). High-value-low-cost in terms of community
  contribution.

Estimate: Android — 1 week including packaging. iOS — open-ended.

## Deletion checkpoint

When **all 8 tabs reach parity** with the iced build, retire the
iced crate. Concrete checklist:

- [ ] Scales (basic) — *done in scaffold*
- [ ] Scales (with FretboardWidget + tuning row)
- [ ] Chords
- [ ] Tuner
- [ ] Progressions
- [ ] Exercises
- [ ] Metronome (with audio)
- [ ] Practice (with audio + beat wheel + calibration)
- [ ] Song (with audio + bar editor + transport)
- [ ] All custom widgets ported
- [ ] Theme system ported
- [ ] Save/load (if shipped)

When all checked: delete `crates/woodshed`, remove iced from
workspace deps, rename `crates/woodshed-xilem` → `crates/woodshed`,
move the binary name accordingly.

## Things to watch

- **Xilem API churn.** Currently alpha. Plan to track its main
  branch via path-dep, accept periodic refactor as the cost of
  being on the bleeding edge.
- **Performance.** Vello + Parley should be very good, but the
  first time we run a full Song-mode arrangement with chord audio
  + click + recording + onset analysis, we'll see how the audio
  thread and the UI thread cooperate. The iced build already
  handles this fine; the xilem migration shouldn't regress.
- **Touch input.** Masonry's pointer-event story is general enough
  to handle touch, but ergonomics for the bar-strip drag-to-reorder
  (a future Song-mode feature) want testing on a touch screen.
- **Accessibility.** Masonry plugs into AccessKit; Iced does not.
  This is actually a clean win for the migration — once we wire
  `accessibility()` impls on the custom widgets, screen readers
  "just work."
