# Song Timeline — Layered, Bar-Quantized View + Sampler

A scoped "one-person DLR" for the Song tab: a bar-quantized timeline with
stacked lanes, sitting next to Woodshed's theory/praxis reference material.
This **supersedes §3 ("UI") of
[`2026-05-16_song_mode_integration.md`](2026-05-16_song_mode_integration.md)** —
that doc's "bar list strip + per-bar editor + transport row" becomes the
multi-lane grid described here. Engine wiring (§1) and save format from that
doc still stand.

## Premise

The Song tab grows into three complementary tools, in dependency order:

1. **Composer** — arrange bars, set chords, sections, tempo/meter (the three
   lanes below).
2. **Looper** — playhead sweeps the grid at song tempo, driving chord cache +
   metronome click off one shared clock.
3. **Sampler** — capture/loop real audio into bar ranges. **This is the goal**;
   the three lanes get us in position for it.

Scope guardrails (Mark, restated): no p2p, no daw-lite widgets (lanes are
fixed kinds, not free-floating clips), basic loop+sequence for one, leverage
`woodshed-audio` (chords) + metronome (click). Deeler profile, but solo.

## Where the sampler lives (doctrine refinement)

Originally the sampler was to incubate in Strophe (the "audio pressure
vessel"). **Adjusted 2026-05-19:** Strophe is far from UI polish, and it
**path-deps `woodshed-audio`**. So building the sampler *directly in
`woodshed-audio`* develops the shared codebase and helps Strophe — it is **not**
an inversion of the pressure-vessel flow; `woodshed-audio` is itself a shared
crate Strophe consumes. Same discipline applies (general API, no
product-specific types, design for eventual `audio-primitives` extraction),
just done in `woodshed-audio` now instead of waiting on Strophe.

## The model

- **Horizontal axis = time, bar-quantized.** Bars are columns; everything
  snaps to bar boundaries. `Song` is already `Vec<Bar>` with a cursor and
  `advance(frames, sr)`.
- **Vertical axis = lanes**, fixed kinds (lanes, not clips). Top to bottom:
  section/marker, chord, click/transport.

### Lane 1 — Section / marker (cheapest, pure data) — **shipped**

Colored bands spanning bar ranges with labels (Intro / Verse / Chorus). No
audio. Reads like a score next to the theory/praxis material.

- **Data:** **reuses the existing `Bar.label: String`** as the section marker
  rather than adding a `section_start` field. `label` already meant
  "Verse 1"/"Bridge" and already serializes — a parallel field would be
  redundant (anti-redundancy per DOC_POLICY). A non-empty `label` opens a band
  that runs until the next labeled bar (or song end); a leading run of
  unlabeled bars renders as track. Bar-quantized, no overlap bookkeeping.
- **Implementation:** `widgets::section_lane_view` (canvas; `SectionBand` +
  `SectionColors::from_palette`); `main::compute_section_bands(&Song)` groups
  bars; a "Section:" `text_input` in the per-bar editor commits straight to
  `bar.label`.

### Lane 2 — Chord

One cell per bar showing the chord name; editing a cell sets that bar's
`chord_ref`. Drives playback via the now-fixed chord cache
(`resync_chord_cache` clears + resizes, so edited qualities re-render).

- **Entry (decision #3): two paths.**
  - **Primary / fallback — comboboxes:** root pitch + chord quality. Reliable,
    always resolves. (Reuses the full-catalog `combobox` just built.)
  - **Power path — typed formula:** double-click the cell to type a chord
    formula; validate the text against `chord_catalog()` so anything accepted
    is guaranteed to render. Reuses the `editable_big_number` /
    `handle_numeric_click` double-click-to-edit pattern, with a catalog-lookup
    validation step replacing numeric parsing. Invalid text → reject / no-op,
    keep previous chord.

### Lane 3 — Click / transport

Thin lane showing per-bar tempo / time-sig **only when non-default** (keeps it
quiet). The metronome engine becomes the timeline's clock.

- **Per-bar config (decision #2 cont.):** time signature, tempo, **and bar
  length** (number of measures the bar spans, `1..=x`). A "bar" in the timeline
  is thus a *block* that can hold multiple measures of the same content — fits
  looper thinking and keeps repeated content from exploding into many cells.
  - **Data:** `Bar` gains `length: u8` (≥ 1). Chord/tempo/meter apply across the
    whole block; the playhead loops through `length` measures before advancing
    to the next `Bar`.
  - **Engine impact:** `Song::advance()` must account for multi-measure bars
    (cursor walks `length` measures per `Bar`). This is the main data-model
    change in this phase. Add tests mirroring the existing 13 `Song` tests.

## The new engineering: playhead + shared clock

Everything above is pure data or existing engines. The genuinely new UI piece
is a **playhead that sweeps the grid at song tempo**. `SongEngine` already
walks `song.cursor` via `advance()`; the UI reads cursor position each refresh
tick (Woodshed already polls for the tuner snapshot) and paints a vertical line
at the right bar + fraction. Play/Stop stay as a transport row; the clock
unifies the lanes.

### Layout: interactive cells vs. painted overlay

- Chord cells must be clickable (→ combobox / typed entry) ⇒ **real widgets**
  (flex_row of per-bar cells per lane).
- Playhead + section bands want **canvas painting** (smooth sweep, colored
  spans) ⇒ a `widgets.rs` canvas view like `fretboard_view` / `cents_meter_view`.

**Resolution:** a `zstack` — real widget lanes underneath, a transparent
playhead/section canvas on top reading the cursor. The playhead lives inside the
**same scroll region** as the lanes, so no paint-beyond-layout is needed — a
same-bounds overlay suffices. **Does not block on the deferred popup view.**

### Selection vs. cursor (decision #1, confirmed)

Playhead = moving filled vertical line; selected bar = outlined column. Both
clickable; visibly distinct.

## Phases & validation

Build order: the three lanes, then the sampler. **T1–T3 + QoL + count-in +
per-measure chord re-strike all confirmed working by Mark 2026-05-19.**

- **Phase T1 — Section lane.** `Bar.section_start: Option<String>`; section-band
  canvas + label editing. *Done when:* labeling a bar shows a band spanning to
  the next labeled bar; round-trips through serde + settings.
- **Phase T2 — Chord lane.** Inline chord cells with combobox (root + quality)
  primary and typed-formula-validated-against-catalog power path. *Done when:*
  every catalog quality is selectable per bar, typed valid formulas resolve,
  invalid typed text is rejected, and audio reflects the choice.
- **Phase T3 — Click/transport lane + playhead.** `Bar.length: u8`; `advance()`
  multi-measure handling + tests; per-bar tempo/meter/length editor; playhead
  canvas reading cursor; Play/Stop transport row. *Done when:* playhead sweeps
  bar-quantized at song tempo, multi-measure bars loop correctly, click track
  follows per-bar meter. **Built; awaiting visual confirm.**
  - `Bar.length: u8` (`#[serde(default = default_bar_length)]` → 1 for legacy
    saves); `duration_secs()` ×= length, so `advance()`/`duration_samples`
    inherit multi-measure spans with no further change. 3 new tests (16 total
    in `song.rs` pass).
  - `SongEngineHandle::sample_rate()` added so the UI can turn
    `cursor.sample_in_bar` into a within-bar fraction.
  - Playhead = `(bar_idx + within-bar fraction) / bar_count`, drawn as a
    `tertiary`-colored vertical line in both lanes via shared
    `widgets::draw_playhead`; refreshes off the existing 50 ms `tick_task`.
    Bar-to-bar stepping is exact; intra-cell motion uses the engine rate.
  - Editor gained a "Length: N bars" −/+ control (clamp 1..=16); bar buttons
    show "×N" when length > 1. (Per-bar click subdivision left as-is; the
    metronome already honors per-bar meter.)
- 2026-05-19: **QoL batch** (Mark's transport/meter/pitch wishlist), built clean:
  - **Chord octave** — `make_chord_ref(pc, octave, name)`; editor gained an
    octave combobox (C1..C6, `CHORD_OCTAVE_RANGE`); `chord_root_from_freq`
    recovers (pc, octave) by scanning the range. `bar_chord_root` /
    `set_bar_chord` helpers de-dup the 3 chord-entry closures.
  - **Time-sig denominator** — Time row shows `num/denom`; second ◀/▶ steps
    the denominator through `TIME_DENOMINATORS` [1,2,4,8,16] (clamped, no
    wrap). Timing model = numerator beats/bar at bpm, so denominator is the
    beat-unit label — musically coherent without a timing change.
  - **Tempo slider** — per-bar tempo is now a 40..240 slider + mono readout
    (replaces the ±1 buttons).
  - **Click on/off** — `Song.click_enabled` (`#[serde(default=true)]`); gated
    in `process_song_buffer`; "Click: on/off" toggle in the transport row.
  - **Count-in: deferred** — needs a transport state machine (N count-in bars
    before playback; audible-only vs. armed-record interplay). Flagged for a
    short design discussion before building.

- 2026-05-19: **Count-in + multi-measure fixes** (Mark requests), built clean,
  136 audio tests pass:
  - **Count-in** — fixed one bar of click before playback (Mark's pick).
    `SongEngineInternals.count_in_remaining/count_in_pos`; `play()` arms it from
    the cursor bar's measure length *only when `click_enabled`* (muted click =
    no count-in, plays immediately). During count-in the cursor holds still and
    only the click sounds; `stop`/`rewind`/`set_song` clear it.
  - **Per-measure chord re-strike** — the chord trigger is now measure-aware:
    a length-N bar re-strikes its chord at each measure boundary ("C ×4"),
    rendered one measure long so each strike sounds then breaks before the
    next. Previously the chord fired once at block start and the rest of the
    block was silent. The click downbeat accent is likewise per-measure now.
    New `last_chord_measure` tracker; `new_block || new_measure` fire
    condition. (Mixer logic — verified audibly, not unit-tested.)
- **Phase T4 — Sampler (in `woodshed-audio`).** Capture/loop real audio into a
  bar's `audio_buffer`. See the dedicated section below.

## T4 — Sampler design pass

### What already works (the looper half)

End-to-end loop recording is wired today:

- `LooperCaptureAnalyzer` (input fan-out) writes mic samples to a shared ring;
  `SongEngineHandle::set_capture` hands the ring to the engine (called on engine
  init in the app).
- The Record button queues `StartRecording { bar_idx }` (SR-16 pending change),
  enables capture, and arms the bar. At the bar boundary the engine drains the
  ring into that bar's `audio_buffer` — **overdub** style (sum existing + new,
  soft-clamp), buffer sized to the bar's full (multi-measure) length.
- During playback the bar's `audio_buffer` loops (`sample_in_bar % len`).
  "Clear audio" detaches it. Bar list shows a loop ●/○ marker.
- New synergy: the one-bar **count-in** gives a click lead-in before the armed
  bar records.

So "record a loop into a bar and hear it back" already exists.

### The gap to a "sampler"

What's missing is everything that turns a recorded clip into a *sample you can
see and shape*: no waveform feedback; no sample ops (trim, gain, reverse,
normalize); no replace-vs-overdub choice; no file import; no one-shot-vs-loop
playback mode. The **full** Strophe-owned sampler is deferred by Mark's standing
note — so T4 is a scoped step on the working looper, not the whole instrument.

### Doctrine hooks

- A **waveform widget** is already named as the first `audio-widgets` extraction
  candidate (pressure-vessel memo). Build it Strophe-promotable (no `woodshed::`
  types) — Strophe will want the same.
- Sample ops (normalize, reverse, trim, gain) are `audio-primitives` candidates.

### v1 shape — needs Mark's pick (see question)

Candidate scopes, lightest → heaviest:

- **A. See + shape the loop.** Waveform display of the bar's `audio_buffer` +
  basic ops (trim-to-bar, gain, reverse, normalize, replace-vs-overdub toggle).
  Pure on the existing buffer; no new capture plumbing. Strongest
  looper→sampler upgrade for the least new surface.
- **B. A + file import.** Also load a WAV into a bar (drag/drop or file picker)
  as an alternative to recording. Adds a file path + decode dependency.
- **C. A + sample slot / one-shot triggers.** A small set of named sample slots
  (recorded or loaded) triggerable per bar as one-shots, beginning to separate
  "sample" from "bar." Closest to a real sampler; most new model + UI.

### T4 — built (Mark picked **A**, reuse Strophe's waveform widget)

- 2026-05-19: **Waveform widget extracted to a shared crate** (Mark's "extract"
  choice). New `crates/audio-widgets` in the woodshed repo holds
  `compute_peaks` + `waveform_view` (+ `Peak`), wired into the woodshed
  workspace (4 tests pass). Strophe's `strophe-widgets` now **re-exports** from
  it (`pub use audio_widgets::…`), dropped its local copy + `strophe-model`
  dep; full Strophe workspace builds against the cross-repo path-dep
  (`../woodshed/crates/audio-widgets`). This is the first realized
  `audio-widgets` extraction from the pressure-vessel doctrine.
  - **Correction surfaced:** `repos/strophe/Cargo.toml` shows Strophe **dropped
    `woodshed-audio`** at FT3b-prime (Firewheel pivot) — sharing is meant to go
    through these shared crates, not direct coupling. So the earlier "build in
    `woodshed-audio` to help Strophe" premise is stale; DSP that should reach
    Strophe must live in a shared crate. (Memory corrected; flagged for Mark.)
- 2026-05-19: **Loop-shaping ops** in `woodshed-audio` (`SampleBuffer`):
  `apply_gain`, `normalize(peak)`, `reverse` — all length-preserving so the
  bar-locked loop stays the right length; written as pure per-sample passes
  (extraction-ready). 4 new tests (15 in `sound.rs` pass). Trim deferred (a
  length-changing op fights the `sample_in_bar % len` bar-lock).
- 2026-05-19: **Replace-vs-overdub** — `Song.record_replace` (`#[serde(default)]`
  = overdub, preserving current behavior); engine recording write overwrites
  per-sample when set. "Rec: overdub/replace" toggle in the transport row.
- 2026-05-19: **Sampler UI** in the per-bar editor — `waveform_view` of the
  bar's `audio_buffer` (peaks via `compute_peaks`, primary/text-dim colors) +
  Normalize / Reverse / Gain−/Gain+ / Clear audio (via a `sample_op` helper).
  Built clean; awaiting Mark's audible/visual confirm.

## Theme: shared base + live runtime switching

- 2026-05-19: **Shared theme module.** Mark extracted spacing (`SP_*`), type
  scale (`TS_*`), `mono_family`, base `Palette` tokens, and `ThemeMode` into
  `audio_widgets::theme`. Woodshed's `theme.rs` is now a thin layer that
  re-exports those and rebuilds its `Palette` from the shared base, adding only
  its fretboard-diagram colors (`palette.x` API stays flat). `palette_for(mode)`
  free fn replaces the old `ThemeMode::palette()` (the enum is now the shared
  foreign type). Strophe + Mere consume the same base.
- 2026-05-19: **Gutter fix** — the window base/clear color was never set, so it
  stayed Xilem's dark default (dark gutters in light mode). `run()` now sets it
  from the palette.
- 2026-05-19: **Live runtime theme switching** (Mark chose the fork path).
  - **Carried xilem patch** (local `../xilem` checkout, shared with Strophe;
    additive — Strophe builds unaffected): `RenderRoot::set_default_properties`
    (masonry_core — swaps `property_arena.default_properties` + `request_render_all`);
    `WindowView` gains a reactive `default_properties: Option<Arc<DefaultProperties>>`
    field + `with_default_properties`, applied on rebuild when the `Arc` identity
    changes. (Base color was already reactive in `WindowView`.)
  - **Woodshed** moved from `Xilem::new_simple` to the windowed `Xilem::new` API
    so the window view feeds `base_color` + `default_properties` from
    `state.palette` each frame; `impl XilemAppState` (keep_running) +
    `on_close` flips a `running` flag; `AppState` caches the
    `Arc<DefaultProperties>` (rebuilt only in `set_theme`, so steady frames
    skip re-apply via `Arc::ptr_eq`). Toggling theme now re-colors everything —
    including bare labels / buttons / prose — with no restart.

## Findings

- The `section_start` field the plan first sketched was redundant: `Bar.label`
  already carries section-name semantics ("Verse 1"/"Bridge") and already
  serializes. Reused it; no model change for T1.
- Canvas full-width sizing: section lane wraps the `canvas` view in a
  `sized_box(..).fixed_height(34px)` inside a `CrossAxisAlignment::Stretch`
  card, leaving width unconstrained so the canvas fills the lane. (Confirm at
  runtime — `sized_box.fixed_width` is a *preference*, not a clamp; same gotcha
  that bit the chord-card reflow.)

## Progress

- 2026-05-19: Plan drafted. Full-catalog Song chord picker (combobox) shipped
  and verified earlier same session; chord-cache invalidation bug fixed
  (`resync_chord_cache` clears + resizes). Decisions #1–#3 resolved with Mark.
  Sampler relocated from Strophe to `woodshed-audio` (pressure-vessel
  refinement above). Build order set: three lanes → sampler.
- 2026-05-19: **T1 (section lane) built + confirmed.** Added
  `widgets::section_lane_view` + `SectionBand`/`SectionColors`,
  `main::compute_section_bands`, a "Section:" text input in the per-bar editor,
  and the lane card above the bar strip in `song_view_render`. Reuses
  `Bar.label`. Full-width fill confirmed at runtime (canvas fills the Stretch
  card). Per Mark, **Bar ops moved into the Bars card** (right of the bar
  strip) — bottom row is now just the editor.
- 2026-05-19: **T2 (chord lane + entry) built, clean compile.** Added
  `widgets::chord_lane_view` (one cell/bar, selected outlined, playhead
  filled), stacked under the section lane. Editor chord entry reworked to
  decision #3: **Root** combobox (per-bar pitch class, recovered via
  `pc_from_root_freq` — no longer silently the Progressions key) + **Quality**
  combobox + a typed **"Type quality:"** power input validated by
  `formula_index_from_input` (matches catalog name or symbol; invalid = no-op,
  valid clears the buffer + applies at the bar's current root). New
  `AppState.song_formula_buf` scratch field. Pending Mark's visual confirm.

## Out of scope (reaffirmed)

From the song-mode doc, still binding: no per-track recording (one buffer per
bar), no real-time effects (dry capture), no MIDI export of songs. The sampler
(T4) operates within these — it fills the existing per-bar `audio_buffer`, it
does not introduce tracks.
