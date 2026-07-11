# Stage, Set, Tools, Rehearsal, and Looper Plan

## Product model

Woodshed is a practice app. Its organizing action is staging material for a
practice session, not browsing an explorer and not composing a song.

The product spine is:

`Catalogs -> Stage -> Set -> Rehearsal or Looper`

- **Catalogs** contain chords, scales, arpeggios, progressions, exercises, and
  set templates.
- **Stage** is the verb that adds configured material to the current Set.
- **Set** is the ordered, heterogeneous practice material for the session.
- **Rehearsal** plays through the Set as guided practice, including looping,
  metronome-synchronized articulation, and highlighted positions.
- **Looper** turns a clocked Set into a repeating backing form, captures a
  performance over it, supports replace and overdub, and exports the result.
- **Tools** are the fretboard, metronome, and tuner. They have standalone
  homes and appear contextually where useful.
- **Settings** is the canonical home for configuration. Contextual controls
  edit the same state rather than maintaining screen-local copies.

Catalog relations and practice history may be projected through Mere inside
Stage. This is not an Explore section. Its job is to explain the current
material and answer a practical question: **what might I stage next?**

The Looper is deliberately smaller than a DAW. It does not introduce tracks,
arrangement sections, editing lanes, effects chains, or a song-authoring mode.

## Boundary decisions

### Keep the existing Set spine

`woodshedding::rehearsal::{Set, Card, Material, Setting, Touch, Timing}` is the
right portable core. A Card already means material plus instrument placement,
articulation, and dwell. Catalog choices and generated templates should stamp
Cards into one Set. Rehearsal and Looper should consume that Set instead of
maintaining independent musical documents.

`PracticeSet` remains useful as a catalog template or generator. It moves under
Stage and materializes Cards through the existing `set_from_practice` path. It
is not a top-level product section.

The current `Lens` name describes catalog selection, not a general projection
system. Rename it to `CatalogKind` once the views are decomposed. Fretboard
layout is a projection choice and belongs to the Fretboard tool/settings.

### Treat Card as a domain object with several projections

Card is the right unit because it is a finite practice instruction: play this
material, on this instrument and tuning, with this touch, for this long. It
must not imply one large visual card everywhere.

- Stage may show a detailed editable Card.
- The Set tray may show the same Card as a compact row or tile.
- Rehearsal may show it as a filmstrip frame and a full active surface.
- Looper may show it as a clocked segment.

Use ordinary `xilem_serval` elements for the Card's text, controls, focus,
reordering, and accessibility. Use Chisel for custom-painted projections inside
or beside it: fretboard geometry, interval diagrams, progression strips, and a
graph neighborhood. Chisel does not own Card state or product actions.

### Project catalog relations and practice history through Mere

`woodshed-graph` already gives catalog objects stable IDs and relates
progressions to their chords and chords to scales they fit. Its stemma proof
records practice lineage against those same IDs. Extend that projection rather
than creating a recommendation-only store.

Keep catalog formulas as stable nodes. Root, instrument, tuning, timing, and
touch belong to the staged Card or practice event; avoid multiplying the
catalog into a node for every possible configured realization.

Record typed engagement events: previewed, staged, rehearsed, completed,
looped, and recorded. Preview and staging are evidence of interest; completed
Rehearsal time is evidence of practice. Suggestions should identify which
evidence and relation produced them.

The default Stage surface remains cards and lists. A contextual **Related**
panel shows a small ranked set of useful neighbors as stageable cards. An
optional graph view explains the neighborhood, with the selected catalog item
at the center, theory relations around it, and the player's recent path or
practice strength overlaid. The graph is a projection, not a second catalog
editor and not the primary way to find material.

### Replace the Song model

`SongDoc`, `SongBar`, `Tab::Song`, and the public Song engine vocabulary are
retired. They overstate the product and duplicate the ordered timing already
owned by Set.

Replace them with two narrower models:

- `LoopPlan`: a derived, immutable-for-a-pass clocked rendering of a Set. Its
  segments carry stable Card IDs, tempo, meter, duration, click behavior, and
  resolved pitches.
- `LoopSession`: capture state and recorded layers associated with stable
  segment IDs, plus export settings. Musical material remains in the Set.

The audio layer may retain an internal segment sequencer while it is migrated,
but `AudioBackend` should expose loop-plan, transport, capture, clear, and
export operations. Recorded buffers must not be preserved by vector index.

Looper requires finite timing. `LoopPlan::from_set` returns readiness issues for
Cards that cannot be clocked. The UI links each issue back to the affected Card.
Defaults such as bars per Card, meter, count-in, and export format live in
Looper settings. They are never silent hardcoded repairs to unresolved Cards.

### Keep tools reusable and state-light

The Fretboard renders resolved material for an instrument and tuning. It does
not own material. Rehearsal can provide the active Card and active note;
Looper can provide the active segment; the standalone tool can use the current
catalog selection.

The Metronome owns one shared musical clock and exposes compact contextual
controls. Rehearsal and Looper follow that clock rather than creating private
tempo authorities. The Tuner remains a live-input tool and may be opened beside
practice without changing the Set.

## Plan

### P1. Separate the product views without changing behavior

Split the monolithic `woodshed-views/src/stage.rs` into an application shell,
Stage, Rehearsal, Looper, Tools, Settings, and shared controls. Keep coordination
in `woodshed-core`; keep desktop realization in `woodshed-serval`.

Done when each product section can be changed and tested without editing one
multi-thousand-line screen file, existing session loading still works, and the
desktop host contains no product composition.

### P2. Establish navigation and canonical settings

Replace `Tab` with a nested route model:

- Stage
- Rehearsal
- Looper
- Tools: Fretboard, Metronome, Tuner
- Settings: General, Appearance, Instrument, Tuning, Stage, Fretboard,
  Metronome, Tuner, Rehearsal, Looper, Audio and MIDI, Accessibility

Introduce a serializable `AppSettings` in `woodshed-core`. Separate portable
preferences from host-local device selection and transient runtime state.
Instrument and tuning form one current musical context shared by catalogs,
Fretboard, Rehearsal, and Looper. Contextual controls address fields in
`AppSettings` or that shared context directly.

Done when every exposed configuration has one owner, every settings page has a
route, old Practice routes open Stage templates, old Song routes open Looper,
and missing host devices do not corrupt portable settings.

### P3. Make Stage an explicit workflow

Build Stage around a catalog rail, a material workspace, and a persistent Set
tray. The catalog rail contains Scales, Chords, Arpeggios, Progressions,
Exercises, and Set Templates. Search filters or jumps into these catalogs.

The primary action is **Stage**. It adds the currently configured material as a
Card. Progressions, exercises, and templates may stage several Cards, with a
preview of what will be added. The Set tray supports selection, reorder,
duplicate, remove, timing, touch, instrument placement, clear, and save/load.

Done when every catalog kind can stage valid Cards, the exact staged result is
visible before leaving Stage, and neither Rehearsal nor Looper needs its own
material editor.

### P4. Add related material and practice-history projections

Extend `woodshed-graph` to include arpeggios, stable Card provenance, typed
practice events, and the relations required for useful suggestions. Start with
deterministic musical explanations: contains, fits scale, shares tones,
voice-leading distance, used together in a progression, adjacent in practice,
and neglected relative to nearby material.

Expose a narrow projection snapshot from Woodshed core. Render ranked Related
cards in ordinary `xilem_serval`; adapt the same snapshot to a Chisel-hosted
Mere neighborhood view. Selecting a related node updates the Stage preview.
Staging it still goes through the one Stage action.

Done when every suggestion names its reason, can be staged directly, respects
the current instrument/tuning context, and can be dismissed or disabled in
Settings; the graph and list select the same catalog identities; and disabling
history-based suggestions leaves deterministic theory relations available.

### P5. Unify the clock and visual articulation

Create one core clock snapshot with beat, subdivision, Set cursor, Card-local
progress, and active sequence step. Map scale, arpeggio, chord, and exercise
sequences to stable display-position IDs so the Fretboard can highlight the
same event the audio backend articulates.

Done when changing tempo affects click, automatic Card dwell, audio preview,
and highlighted dots together; pause/resume does not drift; and manual Cards
remain manually advanceable.

### P6. Finish Rehearsal as the guided Set runner

Rehearsal streams the current Set with previous/current/next context, a large
instrument view, transport, Card progress, and loop controls. It supports Set
looping and focused Card looping. Per-Card timing and touch remain editable via
the shared Set controls. Tool panels may open without leaving the rehearsal.

Done when a mixed Set can run from first Card to last, loop according to the
selected mode, articulate sequences in sync, recover predictably after edits,
and remain fully operable by keyboard.

### P7. Rebuild Song as Looper over Set

Add `LoopPlan` lowering and readiness validation. Migrate the current song
engine to a loop engine that plays Set-derived segments and preserves captures
by stable segment ID. Provide count-in, play/stop/rewind, replace, overdub,
clear, input level, and capture status.

Export the rendered backing plus captured loop to WAV through an explicit host
file-save seam. Keep raw captured audio available to the session while the app
is open; define companion-file persistence before claiming that recordings
survive restart.

Done when a clock-ready mixed Set loops without a parallel bar editor, capture
survives Set reordering where Card identity is retained, exported WAV duration
and tempo match the LoopPlan, and the UI contains no Song or DAW language.

### P8. Adaptive polish and release acceptance

Give each section explicit wide, medium, and narrow compositions. Wide layouts
may expose the Set tray and tool panels simultaneously. Narrow layouts use one
primary workspace with drawers or sheets for catalogs, Set, and tools. Adapt to
available width and input capabilities rather than naming device classes.

Add focus order, visible focus, accessible names and state, reduced-motion
behavior, touch targets, empty/loading/error states, and a compact command map.
Use design tokens for spacing, type, color, focus, motion, and control size.

Done when Stage, Rehearsal, Looper, Tools, and every Settings page work at the
three width bands; core flows work with mouse, keyboard, and touch-sized
controls; theme contrast is checked; Windows packaging passes; and Mac/Linux
receipts are recorded before those platforms are advertised.

## Migration and stop rules

- Make one bounded persisted-session migration, then delete the obsolete
  models. Do not maintain Stage/Practice or Looper/Song in parallel.
- Preserve a user's Set and settings. Best-effort import Song bars as staged
  chord Cards only if identity and timing can be represented honestly.
- Rehearsal owns guided visual articulation. Looper owns capture and export.
- Set owns ordered practice material. Fretboard owns its representation.
- Instrument and tuning are shared context. Audio/MIDI device IDs remain
  host-local.
- WAV is the first export target. Additional formats require a separate need.
- Multi-track arrangement, destructive waveform editing, plug-ins, and mixing
  are outside this plan.

## Findings

- The existing portable `Set` and `Card` model already carries material,
  setting, touch, timing, provenance, cursor, and loop mode.
- Card is a sound domain unit, but the current UI should not force one visual
  card treatment across Stage, Set tray, Rehearsal, and Looper.
- Chisel is a good fit for custom-painted material projections. Its semantic
  event/action seam is still a placeholder, so ordinary `xilem_serval` elements
  remain the right owner for Card interaction and accessibility.
- `woodshed-graph` projects scales, chords, arpeggios, stable IDs, scored theory
  relations, and practice lineage into both the Related list and Chisel
  neighborhood. Progressions and exercises still need first-class graph nodes.
- Current `PracticeSet` values already lower to the same Cards, so Practice can
  become Set Templates without a new data model.
- Current `SongDoc` duplicates ordering, tempo, and duration. Progressions can
  currently bypass Set and become Song bars directly.
- Recorded loops are preserved by bar index during Song updates. Insert or
  reorder can attach a recording to the wrong musical material.
- Offline WAV export exists for sequencer patterns, but captured loop export is
  not exposed through the application backend.
- Settings currently combines theme/layout, device status, MIDI, and latency
  in one screen. Several durable settings still live as view-owned fields.
- Responsive width classes exist, but product sections do not yet have a
  coherent narrow-screen information architecture.

## Progress

- **2026-07-11:** Reconciled the maintainer's product model with the live Set,
  PracticeSet, SongDoc, AudioBackend, capture engine, persistence, navigation,
  settings, and responsive view seams. Wrote the replacement plan and updated
  the project authority/index language.
- **2026-07-11, related-material slice:** Added a cached, explainable
  `woodshed-graph` neighbor query; mapped stable graph identities to core Stage
  selections; and added a responsive Related panel with Select and Stage
  actions. Renamed the existing `+ Rehearse` action to `Stage`. Verified 8
  graph tests and 35 core tests, `cargo check -p woodshed-views`,
  `cargo build -p woodshed-serval`, and a live Windows receipt at 1100x664.
  The receipt proved that staging a suggestion changes the catalog projection
  and increments the Set; the test Card was removed afterward. P4 is partial:
  practice-history ranking, arpeggio nodes, richer relations, and the Chisel
  graph view remain.
- **2026-07-11, engagement-history slice:** Added persisted `PracticeHistory`
  with typed Previewed, Staged, Rehearsed, Completed, Looped, and Recorded
  events over stable catalog IDs. Stage, Related-Stage, preview, rehearsal
  start, manual running steps, and automatic rehearsal advance now record at
  their honest boundaries. Related choices remember their origin; repeated
  Stage paths rise in the panel with a history explanation, while previews do
  not affect ranking. The panel also shows the four most recent engagements,
  and a running rehearsal records Completed when it advances past a Card.
  Verified 37 core tests, 8 graph tests, and checks for the views and desktop
  host. P4 remains partial: elapsed practice facts, history retention settings,
  arpeggio nodes, richer harmonic scoring, and the Chisel graph view remain.
- **2026-07-11, P1 decomposition slice:** Moved Settings with MIDI/calibration,
  Rehearsal, the current Song-to-Looper surface, Related/history, and Set
  Templates into owned view modules. `stage.rs` fell from 2,087 lines to about
  1,040 and now concentrates shared app state plus the catalog/fretboard Stage
  surface. This is behavior-preserving and keeps coordination in the existing
  `UiState`; P1 remains partial until shared shell controls and the remaining
  large `UiState` coordination are separated.
- **2026-07-11, harmonic-neighborhood slice:** Added stable arpeggio graph
  identities and scored direct relations plus shared-tone and symmetric
  voice-leading chord affinity. Core now projects the ranked neighborhood once
  for both the Related list and a Chisel graph glyph; selecting an arpeggio
  changes both surfaces to the same identity. Also continued the P1 shell split
  with owned Stage, Rehearsal, Looper, Tools, and Settings sections and Catalog
  and Templates Stage pages. Verified 38 core tests, 10 graph tests, checks for
  the views and desktop host, a desktop build, and a live Windows receipt at
  1100x664. P4 remains partial: graph-node selection, context-sensitive
  instrument/tuning filtering, history controls, dismissals, and progression
  or scale-to-scale affinity remain.
- **2026-07-11, Related controls slice:** Added persisted settings for history
  ranking and the Chisel neighborhood, per-identity Hide actions, and a Restore
  hidden control. Turning history ranking off preserves deterministic theory
  suggestions, and hidden identities are removed from both the list and graph.
  Verified 39 core tests, checks and a desktop build, plus a live Windows
  receipt covering both toggles and dismissal restoration. P4 still needs
  Chisel event routing before graph nodes can select material directly; it also
  needs instrument/tuning context and broader progression/scale affinity.
- **2026-07-11, Settings routes slice:** Replaced the mixed Settings surface
  with explicit General, Appearance, Instrument, Tuning, Stage, Fretboard,
  Metronome, Tuner, Rehearsal, Looper, Audio and MIDI, and Accessibility
  pages. Each live control projects the same state used contextually elsewhere;
  pages with missing backend/configuration support say so directly. Verified
  39 core tests and checks for views and the desktop host. P2 remains partial
  until the durable fields move under a canonical core `AppSettings` envelope
  and nested page selection is persisted.
- **2026-07-11, AppSettings slice:** Added a canonical core `AppSettings` with
  typed Appearance, Instrument, Tuning, Stage, Fretboard, Metronome, Tuner,
  Rehearsal, Looper, Audio/MIDI, and Accessibility subsections. Existing durable
  theme, tuning, Related, layout, and tempo fields now live under those Rust
  owners while Serde flattening preserves the legacy JSON keys. Empty
  subsections mark routes whose runtimes do not yet implement durable knobs.
  Verified 39 core tests, including old flat-session migration and flat-wire
  round-trip coverage. The nested Settings page is now a core enum and restores
  with the session. P2 remains partial until contextual controls bind directly
  to `AppSettings` and the currently empty subsections gain real runtime knobs.
- **2026-07-11, contextual settings binding:** `UiState` now owns one
  `AppSettings`; theme, fretboard layout, tuning, Related/history behavior,
  metronome tempo, and the active Settings page read and write it directly.
  Transient transport playback still lives in `TransportState`, with tempo
  mutation routed through `UiState` so the durable metronome setting stays in
  sync. `PersistedSession` snapshots the canonical model instead of rebuilding
  a parallel settings copy. Verified 39 core tests and checks for views and the
  desktop host. P2 now remains open only for real settings in the empty
  Instrument, Tuner, Rehearsal, Looper, Audio/MIDI, and Accessibility sections.
