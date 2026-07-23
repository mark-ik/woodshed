# design_docs Index

Canonical first-reference document for project documentation. Read this
before any other doc in this directory.

## Project Reference Docs

- [PROJECT_DESCRIPTION.md](PROJECT_DESCRIPTION.md) — Product goals,
  major features, scope. Maintainer-owned.
- [DOC_POLICY.md](DOC_POLICY.md) — Documentation governance.
- [xilem_fork_patches.md](xilem_fork_patches.md) — Ledger of meaningful local
  edits in the shared `../xilem` checkout (runtime theming, etc.).

## Active Plans

- [2026-07-18_accessibility_semantic_surface.md](2026-07-18_accessibility_semantic_surface.md)
  — **One semantic surface, three readers.** The ARIA-attributed cambium DOM
  serves a screen reader (AccessKit), a driver/test (genet-probe), and an agent
  alike, so accessibility is the same investment as automatability. Records the
  stack's a11y groundwork, the gaps (host bridge unwired in cambium-winit,
  hand-rolled controls lack roles), the single-node leaf finding that keeps the
  fretboard markers in the DOM (and locks scroll/fit), the first build (marker
  aria-labels + named neck + leaf `accessibility()`), and the Tier 0-3 plan.
- [2026-07-15_material_and_touch_model.md](2026-07-15_material_and_touch_model.md)
  — **The unifying model.** Content is arranged notes and relationships; the five
  lenses are treatments, not content types. Two axes: Material (with an
  abstraction level, so progressions assemble from key-free generics) and Touch
  (content-aware path, rhythm, and selection). Playback as the expression of
  touch is the keystone; the board becomes a material editor (click to toggle
  membership). Reframes the Stage/Set model; phased path from stepping-playback
  through generics and authoring.
- [2026-07-15_fretboard_marker_detail_plan.md](2026-07-15_fretboard_marker_detail_plan.md)
  — **Interactive fretboard markers.** Hover a marker for an ephemeral detail
  card, click to pin (multi-pin, to compare). The card shows note name, scale
  degree, interval, octave, string/fret, and plays the note. Interaction lives in
  the view-layer per-marker overlay, not the leaf; hover peek uses CSS `:hover`
  today (genet has no hover events yet) and moves to real hover events when they
  land. Design-doc-first; not built yet.
- [2026-07-14_instruments_and_fretboard_rendering_plan.md](2026-07-14_instruments_and_fretboard_rendering_plan.md)
  — **Instrument variety and configurable fretboard rendering.** The theory model
  is already tuning-general, so this is additive catalog work (bowed family and
  world instruments) plus a view redesign: note *regions* instead of dots,
  realistic vs schematic spacing (automatic, ratio-only), full-neck vs windowed
  extent, markers, and scale-degree fill, all exposed in Settings. Phases A/B
  (pure `woodshedding`) verify independently of the blocked host build; courses,
  microtonal frets, and physical scale length are flagged out of scope.
- [2026-07-11_audio_material_analysis_plan.md](2026-07-11_audio_material_analysis_plan.md)
  — **The audio-to-material research lane.** Treats recordings as immutable
  evidence and model output as versioned observations; defines a model-neutral
  benchmark for transcription, catalog resolution, optional separation, and
  local agents before any ML runtime enters the product.
- [2026-07-11_stage_set_tools_plan.md](2026-07-11_stage_set_tools_plan.md)
  — **The product architecture.** Catalogs stage material into one Set;
  Rehearsal and Looper consume it; Fretboard, Metronome, and Tuner are shared
  tools; Settings is the canonical configuration home. Stage projects that Set
  as numbered Card-occurrence nodes with `Next` order, typed theory/history
  layers, configurable relationship views, and node-to-Card semantic zoom.
  Retires Practice as a top-level section and replaces Song/DAW framing with
  Set-derived looping.
- [2026-07-04_genet_host_cross_platform_plan.md](2026-07-04_genet_host_cross_platform_plan.md)
  — **The delivery architecture.** The move to the Genet desktop host is
  complete: one DOM-shaped `xilem_serval` view tree rendered by Genet. The
  remaining plan covers browser shell, Web Audio, deploy/PWA, and mobile
  downstream.
- [2026-06-15_redesign_plan.md](2026-06-15_redesign_plan.md) — UI redesign from
  the Redesign Explorations board: GPUI-quiet chrome (hairline borders, calmer
  density, steppers→dropdowns), Slate + Ember palettes as built-ins, segmented-
  pill nav (left rail held for mobile), fretboard-layout setting, Rehearsal
  filmstrip + transport deck, Practice recipe tiles. Decisions locked
  2026-06-15; cheap layer first. **Product/navigation framing superseded
  2026-07-11** by the Stage/Set/Tools plan; retain as visual reference.
- [2026-06-14_web_profile_plan.md](2026-06-14_web_profile_plan.md) —
  **Superseded 2026-07-04** by the genet-host plan above; kept for the web
  profile constraints and the Tier-0 seams (AudioBackend, Storage, timers,
  `Instant`), which carry forward. The Path A/B/C analysis is historical.
- [2026-05-22_rehearsal_redesign_plan.md](2026-05-22_rehearsal_redesign_plan.md)
  — Prototype → designed UI. Card vocabulary (tagged-union `Card`), a rehearsal
  queue/projections spine, bulldoze-then-build the lens nav. Branch
  `rehearsal-redesign`; leads with R1 material portability. **Superseded
  2026-07-11** as product authority; its shipped Set/Card work carries forward.
- [2026-04-30_initial_plan.md](2026-04-30_initial_plan.md) — Initial
  scaffold and roadmap from theory crate through Iced UI to first
  desktop release.
- [2026-05-15_midi_design.md](2026-05-15_midi_design.md) — MIDI in/out
  design and clock-sync model.
- [2026-05-15_polyphonic_pitch_spike.md](2026-05-15_polyphonic_pitch_spike.md)
  — **Superseded 2026-07-11.** Background on guitar polyphony; its embedded
  Basic Pitch recommendation is replaced by the model-neutral analysis plan.
- [2026-05-16_song_mode_integration.md](2026-05-16_song_mode_integration.md)
  — **Superseded 2026-07-11.** Historical Song engine and save-format work;
  Song becomes a Set-derived Looper rather than a parallel product mode.
- [2026-05-19_song_timeline_layers_plan.md](2026-05-19_song_timeline_layers_plan.md)
  — **Superseded 2026-07-11.** Historical layered-timeline proposal. Woodshed's
  Looper stops short of song arrangement and DAW-shaped editing.
- [2026-05-20_theme_system_design.md](2026-05-20_theme_system_design.md)
  — Seed-derived palette formula (OKLCH + contrast) + theme management model
  (built-in vs user themes, edit/rename/remove). Proposal, pending sign-off.
- [2026-05-21_fretboard_canvas_lenses_plan.md](2026-05-21_fretboard_canvas_lenses_plan.md)
  — Reorient from toolbox to instrument: one persistent fretboard surface +
  Scale/Chord/Progression/Exercise *lenses* over a shared musical context
  (Navigator principle). Phases 1–2 shipped; catalog/product framing is
  superseded by the Stage/Set/Tools plan.
- [2026-05-21_arpeggio_lens_plan.md](2026-05-21_arpeggio_lens_plan.md)
  — Arpeggios as a 5th fretboard lens: chord-catalog tones rendered as
  CAGED-style position/shape cards + an up/down (ascending→descending)
  visual step-through transport (Exercise-style). Lifts the `OneOf9`
  tab cap by boxing the dispatch. Phased A1–A4.
- [2026-05-21_composable_instrument_surface_plan.md](2026-05-21_composable_instrument_surface_plan.md)
  — Phase 3 → 1.0: the left pane becomes a composable stack of *aware*
  instrument modules (fretboard / tuner / metronome) coordinating via
  shared state + a reconcile arbiter + a shared clock; folds the old
  tabs into one configurable surface; plus custom-authoring for 1.0.
  Form B chosen. **Superseded 2026-07-11:** the reusable tool principle carries
  forward without folding the product into one configurable surface.
- [2026-05-16_xilem_migration_plan.md](2026-05-16_xilem_migration_plan.md)
  — **Superseded 2026-07-04.** Historical Iced-to-Xilem migration record; the
  live host and delivery path are governed by the Genet-host plan.

## Archive

- `archive_docs/` — retired plans and superseded notes.
- `archive_docs/2026-05-18/2026-05-17_woodshed_daw_plan.md` —
  Original "sibling DAW project under the Woodshed umbrella" plan.
  Superseded same-week: the maintainer chose a separate sibling repo
  (`repos/strophe/`), and the project scope pivoted from "general
  DAW" to a Deeler-inspired collaborative loop recorder. See
  `repos/strophe/design_docs/` for the live plan.

## Working Principles for AI Assistants

These principles apply to AI-assisted work on this project. Update this
section whenever a durable working insight emerges from a session.

- **Theory model is owned**: do not depend on `rust-music-theory` or
  similar upstream theory crates. We need exotic scales, non-tertiary
  chords, and arbitrary tunings; the upstream models do not support
  those, and inheriting their data shape costs more than it saves. Build
  the theory crate from scratch.
- **Gerund crates name activity cores**: `woodshedding` follows the same
  convention as `murmuring` and `mooting`: the gerund names the portable
  operation core that makes the activity possible, where such a core
  exists. If a crate is tempting to describe generically as "data
  structures," explain the activity those structures enable. Here,
  woodshedding means turning musical material into playable practice:
  identify material, realize it on an instrument, arrange it into
  progressions/exercises/practice sets, and feed app shells that rehearse
  it.
- **Pure core, thin shell**: `crates/woodshedding` must remain pure data
  + math — no I/O, no UI, no audio. It may model the portable operations
  required for woodshedding, but app-specific rehearsal UI, persistence,
  and audio engines live in consuming crates.
- **Stage is a verb and Set is the spine**: catalogs supply material; Stage
  adds configured Cards to one ordered Set; Rehearsal and Looper consume it.
  Do not create parallel practice, song, or tool-owned material documents.
- **The Stage graph projects the Set**: each staged Card occurrence is a stable
  node, Set order derives `Next`, and theory, history, and learned suggestions
  are separately identifiable edge layers. Filtering changes the projection;
  staging or editing changes the one Set through explicit actions. A node may
  collapse from Card to summary to glyph without changing Card identity.
- **Tools project shared state**: Fretboard, Metronome, and Tuner have
  standalone homes and contextual forms. Settings owns their durable
  configuration; contextual controls edit that same canonical state.
- **Desktop first, mobile later**: ship to itch.io / Gumroad for desktop
  before attempting mobile. Mobile is a shell around the web build (see the
  genet-host plan's M track); building toward it is part of the project's
  broader value but does not
  block the music app.
- **Generalize across stringed instruments**: theory model parameterizes
  string count and tuning so bass, ukulele, and banjo fall out for free.
  Do not hard-code 6-string assumptions.
- **Catalog and generators are complementary, not redundant**: the
  catalog answers "what are the well-known tunings?" (preserves cultural
  names, voicing conventions, history); generators answer "what does
  this tuning become under transformation?" (transpose, drop a string,
  apply an interval pattern). The same pattern will apply to scales and
  chords: named-scale catalog + apply-formula-to-root algorithm. Don't
  pick one over the other — they answer different questions.
