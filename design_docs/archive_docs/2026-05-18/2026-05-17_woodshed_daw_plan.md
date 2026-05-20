# Woodshed-DAW Plan

A cooperative, understandable, p2p-native audio workstation as a
sibling project to Woodshed. Shares infrastructure (the audio engine
crate, the masonry custom-widget pattern, the Xilem stack, the
forthcoming `trait AudioBackend`) with the guitarist's toolkit; ships
as a distinct binary with its own scope.

This document captures the strategic decision (Xilem over gpui), the
workspace shape, the design-space framing, and a phased plan with
validation criteria.

---

## Scope flag — surfacing for the maintainer

`PROJECT_DESCRIPTION.md` lists **"Multi-track recording / DAW features"**
under Non-Goals for Woodshed-the-toolkit. That line still stands. This
plan does **not** propose adding DAW features to Woodshed; it proposes
a **separate sibling app** that lives in the same workspace and shares
infrastructure crates.

Three resolutions are possible; the maintainer to pick:

1. **Same workspace, separate crate.** `crates/woodshed-daw` + a new
   sibling binary. Shared deps via the existing `woodshedding` /
   `woodshed-audio` (which may get further pulled apart — see below).
   Workspace becomes "woodshed family of music apps." `PROJECT_DESCRIPTION.md`
   gets a short addendum naming the family relationship and pointing at
   the DAW's own `PROJECT_DESCRIPTION.md`.
2. **Separate workspace, path-dep'd shared crates.** New repo
   `repos/woodshed-daw/`, depends on woodshed's shared crates via path.
   Cleaner scope boundary, more git/CI overhead.
3. **Defer entirely.** Park this plan, revisit after the
   iced → xilem migration of Woodshed completes.

Recommendation: **(1)**. The shared-infrastructure case is strong (see
[Crate Extractions](#crate-extractions) below), the maintainer cost of
managing two repos for what is effectively one developer's work is
non-zero, and the *strategic* benefit of a sibling app sitting beside
the toolkit (cross-pollinating widget designs, debugging,
distribution) is real.

---

## What this is (and isn't)

**Is:**
- A nondestructive multi-track audio workstation in Rust
- Native + web from day one, via Xilem + xilem_web and a `trait AudioBackend`
- CLAP-first plugin hosting (via [clack](https://github.com/prokopyl/clack))
  on native; native-only initially
- A first-class **own** plugin format that compiles to both native and
  wasm-AudioWorklet from a single source, with a Rust trait surface
- P2P session sharing and plugin distribution via Moothold / Murm
  (the same federation primitives planned for Mere)
- Nondestructive timeline as a **data model** decision — clips are
  references into media, edits are nodes in an append-only history
  graph, merges are CRDT-shaped
- A surface that prioritizes **understanding** (provenance of every
  parameter), **cooperation** (async branch/suggest, not just live
  multiplayer), and **leverage from crates** (Dropseed, creek,
  symphonia, rubato, fundsp) over Ableton-style feature parity

**Is not:**
- An Ableton / Bitwig / Logic competitor. Top-tier production
  workflows are not the target audience.
- A fork of Meadowlark's app code. Meadowlark's UI is essentially
  greenfield post-Yarrow/RootVG pivot; the value to draw from is its
  infrastructure crates (Dropseed, creek, symphonium, clack,
  meadow-dsp, fixed-resample, fast-interleave).
- A Yarrow consumer. Yarrow's widget set today is button +
  knob + slider + label + scroll + popup; the audio-specific frames
  (timeline, waveform, automation, mixer strip) don't exist and aren't
  on its roadmap. Yarrow is also all-or-nothing as a host (owns the
  winit event loop); the embeddable layer is just RootVG / yarrow-vg,
  which Vello already supersedes.

---

## Strategic decisions

### Xilem + Masonry + Vello over gpui

Investigated in the 2026-05-17 session. Summary in
[Findings](#findings-from-2026-05-17-research) below; the decision
record:

- **Vello antialiases polylines by default**; gpui's `line_to` does
  not (zed#20762). Waveforms, automation curves, and MIDI note
  outlines are exactly the polyline-heavy drawing a DAW needs. This
  is a substantive rendering-quality difference, not a polish nit.
- **Masonry's `Widget` trait + `paint(ctx, props, painter)`** is the
  embedding model gpui doesn't provide and Yarrow doesn't expose.
  Custom widgets compose freely into the layout tree without
  framework-level escape hatches.
- **xilem_web is real and shipping.** Glass-HQ/gpui's web target is a
  skeleton (one PR, no CI, no audio subsystem, no working example,
  WebGPU-only). The web leg of any dual-target architecture is
  meaningfully cheaper on Xilem today.
- **Woodshed is already proving this stack.** The xilem migration plan
  (2026-05-16) lights up FretboardWidget / ChordDiagram / CentsMeter /
  LevelMeter / BeatWheel as masonry custom widgets via the same
  `paint` interface a WaveformWidget would use. The pattern transfers
  unchanged; only the drawing math differs.
- **Mobile and a11y already on the Woodshed roadmap.** Xilem ships
  Android examples; Masonry plugs into AccessKit. Both transfer.

### CLAP-first via clack

- Native plugin hosting uses [clack](https://github.com/prokopyl/clack)
  (Prokopyl's maintained CLAP host+plugin crate). The
  MeadowlarkDAW/clack fork on GitHub is archived; Prokopyl's is the
  live one.
- VST3 hosting in Rust is essentially unsolved (no good native
  binding; Steinberg SDK is C++). Defensible to **not ship VST3
  hosting**. CLAP-only is increasingly mainstream in 2026 (Bitwig,
  Reaper, FL Studio all support CLAP; Surge and Vital ship CLAP
  builds).
- Web has no third-party plugin hosting at all. The "your-format
  plugins only on web" constraint becomes a feature (curation,
  quality, no piracy concern because everything's open by default).

### Own plugin format as keystone

A Rust trait + parameter schema that produces two artifacts from one
source:

- **Native**: `.so` / `.dylib` / `.dll` (CLAP-wrapped, so the same
  plugin loads in Bitwig / Reaper / etc.)
- **Web**: AudioWorklet processor module + UI wasm

Constraints to design around (these are real):

- AudioWorklet processor lives in a separate JS scope from the main
  thread; the UI rendered in wasm-main and the DSP in
  wasm-worklet can share state only through `MessagePort` or
  `SharedArrayBuffer` (which requires COOP/COEP cross-origin
  isolation).
- "Same source, two artifacts" works cleanly for the DSP trait
  (`process(&mut self, ports, params, ...)`). The UI side is "shared
  parameter schema, separate render paths" — Masonry widgets on
  native, xilem_web SVG/DOM on web — not literally the same code.

### Nondestructive timeline as data model

The timeline widget falls out of the data model. The hard work is the
model:

- Clips reference media by content-addressed hash, not by file path
- Edits are nodes in an append-only history graph
- Merges are CRDT-shaped; concurrent edits in different sessions
  reconcile without lost work
- The widget renders a *view* of this model at a given history cursor
- Scrubbing history is a first-class operation

This is also the substrate for **p2p collaboration**: sync the
history graph, not the audio buffers (those are content-addressed and
sync separately under Moothold).

### P2P niche framing

Live-coding music (Tidal, Sonic Pi, Strudel) is solo or
one-shot-broadcast. Collab DAWs (Endlesss, Soundtrap, BandLab) are
synchronous and centralized. The unexplored quadrant: **async,
Figma-shaped, branchable, comment-on-region collaboration on
nondestructive timelines**. Moothold + Murm + a CRDT timeline model
is the minimum viable kit.

If the project takes "p2p audio coding" literally — networked live
coding alongside clip-based authoring — that's a third design axis.
A text-editing pane that hosts patterns and compiles them to nodes in
the Dropseed audio graph. Xilem's text-editing story is weaker than
gpui's today; this is the one place gpui would actually win, and is
worth a sub-spike before committing.

---

## Crate extractions

Today `crates/woodshed-audio` is a single crate covering output path
(sequencer / sound / samples / engine), input path (input / onset),
and cross-cutting (calibration / midi / looper / song / song_engine /
chord_audio / offline).

For a DAW-sibling to consume only what it needs without dragging in
toolkit-specific surfaces, pull `woodshed-audio` apart:

| New crate                  | Contents                                                          | Consumers                 |
|----------------------------|-------------------------------------------------------------------|---------------------------|
| `woodshed-audio-engine`    | cpal stream, voice mixer, `AudioBackend` trait                    | toolkit, DAW              |
| `woodshed-audio-sequencer` | `SequencerPattern`, `Step`, `TimeSignature`, `Subdivision`        | toolkit, DAW              |
| `woodshed-audio-song`      | `Song`, `Bar`, `ChordRef`, `SongCursor`, `SongEngine`             | toolkit                   |
| `woodshed-audio-input`     | `InputEngine`, `Analyzer`, `PitchAnalyzer`, `OnsetAnalyzer`       | toolkit, DAW (input mon.) |
| `woodshed-audio-onset`     | `OnsetDetector`, `estimate_bpm`                                   | toolkit, DAW              |
| `woodshed-audio-looper`    | `Looper` (bar-aligned overdub)                                    | toolkit, DAW              |
| `woodshed-audio-samples`   | `SampleBank`, sample loading                                      | toolkit, DAW              |
| `woodshed-audio-calibration` | latency measurement + driver session                            | toolkit, DAW              |
| `woodshed-audio-midi`      | midir wrapper, `MidiClockSync`, `MidiIn` / `MidiOut`              | toolkit, DAW              |
| `woodshed-audio-offline`   | render-to-Vec / render-to-WAV                                     | toolkit, DAW              |

Done in **Feature Target 1** below. Drop the umbrella `woodshed-audio`
crate or keep it as a re-export façade — preference is to drop, since
the umbrella encourages cross-coupling.

### New crates owned by the DAW

| Crate                     | Contents                                                       |
|---------------------------|----------------------------------------------------------------|
| `woodshed-daw-model`      | `Project`, `Track`, `Clip`, `Media`, history graph, CRDT ops   |
| `woodshed-daw-engine`     | Dropseed integration (or replacement), audio graph, transport  |
| `woodshed-daw-plugin`     | Own plugin format trait + param schema + native + wasm builds  |
| `woodshed-daw-clap`       | clack-based CLAP host bridge (native only)                     |
| `woodshed-daw-widgets`    | Masonry custom widgets: timeline, waveform, piano roll, etc.   |
| `woodshed-daw-sync`       | Moothold/Murm bindings, session protocol, CRDT merge           |
| `woodshed-daw-xilem`      | Xilem app: views, layout, state                                |
| `woodshed-daw`            | Binary, wiring                                                 |

---

## Plan

Feature targets are ordered by *unblocking* — each lights up
capabilities the next depends on. No calendar estimates; validation
criteria are done conditions.

### Feature Target 1: Crate extraction + workspace expansion

Pull `woodshed-audio` apart into the per-module crates above. Add the
new DAW skeleton crates with stub `lib.rs` files. Both the existing
iced binary, the in-progress xilem binary, and a new `woodshed-daw`
stub binary all build.

**Tasks:**
- Add per-module audio crates with their current contents lifted
- Update `crates/woodshed` (iced) and `crates/woodshed-xilem` to
  depend on the unbundled crates
- Add `crates/woodshed-daw-{model,engine,plugin,clap,widgets,sync,xilem}`
  + `crates/woodshed-daw` (binary), all stubs
- Workspace `Cargo.toml` updated; root `README.md` gets a short
  "family of music apps" addendum
- `DOC_README.md` updated to point at this plan

**Validation:**
- `cargo build` succeeds workspace-wide
- All existing tests still pass
- `cargo run -p woodshed` and `cargo run -p woodshed-xilem` behave
  unchanged
- `cargo run -p woodshed-daw` prints a placeholder line

### Feature Target 2: Waveform spike (the gpui-vs-xilem proof)

Build a `WaveformWidget` masonry custom widget. Render a peak-file
LOD of a WAV onto the timeline; pan + zoom + scroll. This is the
**Phase 0 critical spike** — if vello/masonry can't do this well at
the perf curve a DAW needs, find out now.

**Tasks:**
- Peak-file generator (min/max/RMS at multiple LOD tiers)
- `WaveformWidget` in `woodshed-daw-widgets`, paints via vello
  `Painter` against the masonry `Widget` trait
- Demo binary in `woodshed-daw-xilem`: load a WAV, render the
  waveform, pan/zoom/scroll with mouse + keyboard
- Frame timing instrumentation: log per-frame paint time at varying
  zoom levels and clip counts

**Validation:**
- Stable 60fps panning + zooming on a 10-minute WAV at all LOD tiers
  on Mark's primary Windows laptop (the weakest of the four target
  machines)
- Visual quality: waveform polylines are antialiased, no visible
  staircasing at any zoom
- Same code path runs on iMac (macOS) and Fedora 44 (Wayland)
  without target-specific shims

### Feature Target 3: Engine boundary + Dropseed evaluation

Decide: pull Dropseed in as a dep, fork it, or write our own audio
graph on top of cpal + a custom node trait. The answer depends on
Dropseed's current API shape (mid-extraction back out of Meadowlark
as of 2025-11-10) and how clean its UI-agnosticism is in practice.

**Tasks:**
- Clone Dropseed locally, audit public API and threading model
- Sketch our own audio graph as a fallback (node trait, ringbuf
  message passing, basedrop ownership)
- Decide; document the decision in this plan's Findings section

**Validation:**
- A sine wave plays through the chosen engine, fed by a transport
  position, controlled by play/stop from the UI
- Engine boundary is `Send`able command + event channels;
  no UI-framework types leak into engine code

### Feature Target 4: Project / Track / Clip data model

The nondestructive substrate. `woodshed-daw-model` ships before
anything that consumes it visually beyond the waveform spike.

**Tasks:**
- `Media` (content-addressed reference + sample-rate + channel count)
- `Clip` (Media reference + in-point + out-point + tempo + per-clip
  warping, optional)
- `Track` (ordered Clips + insert chain placeholder + automation
  lanes placeholder)
- `Project` (tracks + tempo map + time signature map + history root)
- History graph: `Edit` enum, `commit(Edit) -> NodeId`, `checkout(NodeId)`
- Save / load via rkyv to a project directory (project.bin + media/
  subdir + history.bin)
- CRDT merge ops: deferred to Target 9 but the model shape supports
  it (every edit is content-addressed and parent-pointered)

**Validation:**
- Round-trip: build a 4-track project programmatically, save, load,
  inspect — bit-identical
- History scrubbing: undo / redo across 100 commits in <16ms
- Doc test exercising the merge-shape (two divergent histories
  reconcile into a deterministic result)

### Feature Target 5: Timeline widget + clip rendering

The arrangement view. Multi-lane host with `WaveformWidget`s
rendering inside `Clip` bounds, ruler with bars/beats/SMPTE,
playhead, scroll/zoom, drag-resize-snap. Bounded canvas per pane —
not a 100k-pixel infinite scroll surface — but pan-to-extend
seamless within a pane.

**Tasks:**
- Timeline ruler widget (bars/beats + SMPTE + samples, LOD-aware)
- Arrangement canvas widget hosting tracks vertically + clips
  horizontally
- Drag interaction: move clip, resize edges, split, snap-to-grid
- Selection model (marquee, shift-click, ctrl-click)
- Playhead overlay element

**Validation:**
- A 16-track project with 50 clips per track scrolls + zooms at
  60fps on the primary Windows laptop
- Drag-resize-snap feels accurate to within 1 pixel of intent at all
  zoom levels
- Splitting a clip mid-drag does not produce phantom or
  zero-length clips

### Feature Target 6: Playback through one track

End-to-end vertical slice: load a WAV into a Clip, play it back
through the audio engine, see the playhead move on the timeline.

**Tasks:**
- Engine consumes Clip references from the Track model
- Sample-accurate scheduling
- Disk streaming via `creek` (or equivalent) for clips longer than a
  RAM budget
- Transport: play / stop / loop / locate
- Audible output via cpal `AudioBackend`

**Validation:**
- A 10-minute WAV plays without glitches at 48kHz / 256-sample buffer
- Locate-during-playback is glitch-free (sample-accurate, no clicks)
- Loop point honored to sample accuracy
- CPU and memory metrics logged

### Feature Target 7: CLAP host integration

Native-only. Load a CLAP plugin, insert into a track's insert chain,
route audio through it, see its native UI in a child window.

**Tasks:**
- clack integration in `woodshed-daw-clap`
- Per-OS plugin window embedding: NSView reparent (macOS),
  HWND reparent (Windows), Wayland subsurface + X11 child (Linux).
  Wrap via baseview-shaped shim adapted to masonry/winit's window
  handle.
- Parameter schema bridge: CLAP params → DAW parameter store
- Preset save/load via CLAP state

**Validation:**
- Surge XT (or another known-good CLAP plugin) loads, plays, and
  saves state on all four target machines
- Plugin GUI resizes when host resizes
- Plugin parameter automation through the host parameter store works

### Feature Target 8: Own plugin format + reference plugin

Define the trait, build a reference plugin (gain + filter), compile
to both `.dll`/`.so`/`.dylib` (via clap-wrapping for native ecosystem
compatibility) and AudioWorklet wasm. Both artifacts ship as a single
DAW plugin.

**Tasks:**
- `woodshed-daw-plugin` trait: `process()`, parameter schema,
  optional UI hook
- Native build: trait → CLAP plugin via clap-rs
- Web build: trait → AudioWorklet processor module + UI wasm
- Reference plugin: gain + lowpass filter, with knobs
- Demo loading reference plugin in both native and web builds of the
  DAW

**Validation:**
- Reference plugin loads in Bitwig (sanity check that the
  CLAP artifact is standard-conformant)
- Reference plugin loads in the DAW's native build
- Reference plugin loads in the DAW's web build via AudioWorklet
- Parameter automation works identically on both targets

### Feature Target 9: MIDI piano roll + sequencer integration

A MIDI Clip type, a piano roll widget for editing notes, and
playback through MIDI-instrument plugins.

**Tasks:**
- `MidiClip` variant of `Clip` (notes + CC events)
- `PianoRollWidget` (masonry custom widget): note grid,
  velocity lane, paint / select / drag / quantize
- Integration with `woodshed-audio-midi` for external MIDI in/out
- MIDI clock sync option (slave or master)

**Validation:**
- Draw a MIDI note, route to a CLAP synth, hear it play
- Quantize, transpose, velocity scale operations are
  history-recorded and reversible
- External MIDI controller plays the synth via the DAW

### Feature Target 10: Mixer + automation lanes

Per-track fader / pan / sends / inserts, master bus, automation
lanes for any parameter (track gain, plugin params).

**Tasks:**
- `MixerStripWidget`, `LevelMeterWidget`, `KnobWidget`,
  `AutomationLaneWidget` — all masonry custom widgets, vello-painted
- Automation point types: linear, bezier, step, hold-release
- Per-parameter "what's automating this?" inspector

**Validation:**
- Automate plugin params from a lane in the arrangement; playback
  honors curves sample-accurately
- Visual feedback (knob ring, fader position) reflects automation in
  realtime during playback
- Inspector explains the live value of any parameter ("this is
  -3.0 dB because: user-set baseline -6.0 dB + automation +3.0 dB
  at t=10.4s, source: track 4 automation lane 'gain'")

### Feature Target 11: Web target

The xilem_web build. AudioWorklet for output, OPFS for project
files, Web MIDI on Chrome/Edge, masonry widgets re-rendered as
xilem_web SVG/DOM. Cross-origin isolation headers documented.

**Tasks:**
- `AudioBackend` impl for Web Audio API + AudioWorklet
- File backend impl for OPFS (read/write project bundles)
- Custom widgets: masonry `paint` → xilem_web SVG path
  generation. Reuse the same `peniko`/`kurbo` geometry code that
  drives Vello on native.
- COOP/COEP deploy doc, self-hosted demo
- Latency profiling on Chrome / Firefox / Safari (where supported)

**Validation:**
- Web build loads a project from OPFS, plays back through
  AudioWorklet, scrubs the timeline
- Web build runs the reference plugin
- Latency floor measured + documented per browser
- Same project file round-trips between native and web

### Feature Target 12: P2P session + project sync (Moothold/Murm)

The collaboration moat. Asynchronous, branchable, comment-on-region.

**Tasks:**
- `woodshed-daw-sync`: serialize history-graph nodes to
  Moothold-addressable blobs, sync media via content-addressed
  Moothold blobs
- Session protocol: invite, join, present-while-editing (optional),
  branch, suggest, accept
- Comment-on-region surface in the timeline
- Plugin distribution via Moothold: subscribe to a plugin,
  auto-update on publish

**Validation:**
- Two instances of the DAW on the same LAN (one Windows, one Fedora)
  reconcile divergent histories deterministically
- A media file added on one side appears on the other without
  manual file transfer
- Suggest-then-accept workflow round-trips
- Plugin published from one node loads on another

### Feature Target 13: PWA packaging + native installer

The "same artifact runs everywhere" story. PWA on web (offline
install + elevated permissions when installed). Itch.io / Gumroad
desktop installers. Mobile (Android via cargo-apk; iOS via PWA
initially, native later).

**Tasks:**
- PWA manifest, service worker, OPFS-based offline
- Desktop installers signed for Windows + macOS
- Android via cargo-apk wrapper (cpal supports AAudio)
- iOS deferred to PWA-on-Safari initially

**Validation:**
- PWA installs and runs offline on Chrome
- Signed Windows + macOS installers verified on Mark's hardware
- Android build runs on Mark's test phone

---

## Findings (from 2026-05-17 research)

### Meadowlark state

- Moved from GitHub to Codeberg in Sept 2025; GitHub org archived
- Lead dev Billy Messenger went iced → tuix → egui → Flutter →
  Yarrow/RootVG over the project's life
- 2025-11-10 Dropseed commit: "move code from Meadowlark repo to
  Dropseed repo" — engine is mid-extraction back out of the app
- Most active sibling crates (recent commits): symphonium (2026-04),
  fixed-resample (2026-04), creek (2025-09), meadow-dsp-essentials
  (2026-02)
- No shipping UI on main; the Vizia UI was abandoned in 2023, the
  Flutter pivot was abandoned in 2024, Yarrow/RootVG is "functional
  but very incomplete"
- **Implication**: no UI to fork; the engine + sibling crates are
  the value. Time the fork well — engine is being made standalone
  right now.

### gpui for DAW-style canvas

- `canvas(prepaint, paint)` element exists; `paint_path` API
- **`line_to` is not antialiased** (zed#20762); only `curve_to` is
- PR #42905 (expand drawing API) was **rejected**; maintainers
  redirected non-Zed feature work to gpui-ce (discussion #41673)
- Event-driven by default; `request_animation_frame()` schedules
  continuous redraws (used in Zed's 120fps Metal pipeline)
- No virtualization for continuous canvases — you write your own
- No shipping gpui app does graphics-heavy work; `awesome-gpui`
  lists zero DAWs, painting apps, or games
- **Implication**: technically possible, but you'd be the proof
  point on a framework whose maintainers aren't expanding the
  exact capability you most need

### Glass-HQ/gpui web target

- `gpui_web` crate exists, wasm32 build path is there
- **No CI**, no working example committed, audio subsystem
  entirely absent, file dialogs and clipboard stubbed
- WebGPU-only (no WebGL2 fallback) — knocks out Safari and Firefox
  on non-Windows today
- Single contributor doing batch syncs from upstream; ~9 weeks of
  web work as of 2026-05-17
- **Implication**: skeleton, not shipping capability. The web leg
  of any architecture that depends on Glass-HQ-on-wasm is a
  speculative bet on one contributor plus substantial work-out.

### Yarrow widget inventory + embedding

- Live repo at codeberg.org/Meadowlark/Yarrow (GitHub mirror is
  archived); 5 sub-crates (yarrow-core, yarrow-application,
  yarrow-frames, yarrow-vg, yarrow-winit) + yarrow-derive
- **Shipped widgets ("frames")**: Button family (Button,
  ToggleButton, LineToggleButton, RadioButton, Switch), Label,
  Paragraph, TextInput, Tooltip, PopUp, PanelResizer, QuadFrame,
  RectFrame, ScissorRect, ScrollBar, ScrollRegion, VirtualSlider
  (with DefaultKnob + FilledSlider renderers)
- **Not present**: dropdown, level meter, **timeline**, **piano
  roll**, **waveform**, **automation lane**, **mixer strip**,
  routing graph, virtualized list, XY pad, image frame
- **Roadmap (planned, not done)**: virtual list, image, waveform
  frame, XY pad, automation spline, modulation arc, range slider,
  tick marks, line-graph rendering
- **Timeline does not exist and is not on the roadmap.** No clip
  model, no media reference, no nondestructive scaffolding.
- **Embedding: all-or-nothing.** `yarrow_winit::run_blocking` owns
  the event loop and creates its own winit windows. No
  `render_to_texture`, no `RawWindowHandle` ingress, no public
  "draw this frame into my wgpu pass" API. Smallest reusable unit
  below the whole app is RootVG / yarrow-vg, which is the wgpu 2D
  primitive batcher — at which point Vello supersedes it.
- README: "currently functional but still very incomplete... The
  aim is *NOT* to be a generic, general-purpose GUI library."

### Why Xilem wins for this project specifically

Six points stacked together:

1. Vello antialiases polylines by default (waveforms, automation,
   MIDI note outlines) — gpui's path API does not
2. Masonry's `Widget::paint` is the embedding model gpui doesn't
   provide and Yarrow doesn't expose
3. `xilem_web` is real and shipping; Glass-HQ web is a skeleton
4. Woodshed is already proving the masonry custom-widget pattern
   for audio-app surfaces (FretboardWidget / CentsMeter / etc.) —
   the DAW widgets are the same pattern with different drawing
   math
5. Mobile and a11y are already on the Woodshed roadmap, both
   transferable
6. Mere is going Xilem too — shared infrastructure across Mark's
   stack reduces context-switch cost and lets shared
   build/test/deploy tooling accumulate

The one place gpui actually wins: text editing (for a live-coding
pane). Worth a sub-spike before fully committing, not enough to
flip the framework decision for the project as a whole.

---

## Progress

(session log — appended as work proceeds)

### 2026-05-17

- Plan drafted. Awaiting maintainer decision on the
  [Scope flag](#scope-flag--surfacing-for-the-maintainer).
