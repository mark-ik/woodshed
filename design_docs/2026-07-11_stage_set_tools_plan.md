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

Catalog relations and practice history may be projected through the shared
projection engine inside Stage. Woodshed owns the musical facts and actions;
the engine owns selection, relationship filters, layout, and placed
representations. This is not an Explore section. Its job is to explain the
current material and answer a practical question: **what might I stage next?**

The Set itself may also be projected as a graph. In that projection each staged
Card occurrence is a numbered node, including repeated material, and Set order
is a typed `Next` edge. Selecting a node opens the same Card editor used by the
tray. Harmonic and historical edges may be layered onto this snapshot, but they
do not become a parallel material document or overwrite Set order.

This graph is the Stage workspace, not merely a diagram beside it. Staging
material adds a Card occurrence to the Set and therefore a node to the graph.
The same occurrence may appear as a numbered glyph, a compact summary, or its
full editable Card. Expansion state belongs to the projection; Card edits land
on the one Set. The list/tray remains an alternate projection for dense and
accessible operation rather than a second workflow.

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

### Project catalog relations and practice history through a shared boundary

`woodshed-graph` already gives catalog objects stable IDs and relates
progressions to their chords and chords to scales they fit. Its stemma proof
records practice lineage against those same IDs. Extend that projection rather
than creating a recommendation-only store or importing Mere's graph kernel as
Woodshed truth. A Woodshed adapter should expose portable material and relation
snapshots to the shared projection engine (`scenograph`, named below).

Keep catalog formulas as stable nodes. Root, instrument, tuning, timing, and
touch belong to the staged Card or practice event; avoid multiplying the
catalog into a node for every possible configured realization.

Record typed engagement events: previewed, staged, rehearsed, completed,
looped, and recorded. Preview and staging are evidence of interest; completed
Rehearsal time is evidence of practice. Suggestions should identify which
evidence and relation produced them.

The compact **Related** swatch is the focused frontier of the Stage graph: a
small ranked set of useful neighbors, each stageable as a Card occurrence. It
can expand without changing identity into a deeper relationship view with the
selected material at the center, theory relations around it, and the player's
recent path or practice strength overlaid. Catalog search remains the direct
way to find a named item; the graph explains, extends, compares, and stages the
relationships around the current focus.

### Keep the Stage graph's authorities layered

The projected Stage graph composes four layers which must remain identifiable:

1. **Set layer:** staged Card occurrences and the `Next` edges derived from Set
   order. This is authored session material.
2. **Theory layer:** deterministic catalog and contextual musical relations
   such as contains interval, fits scale, fifth of, resolves to, shares tones,
   or voice-leads-to. Woodshed owns these facts and computations.
3. **Evidence layer:** observed practice transitions and engagement strength,
   with event kind and observation time retained.
4. **Suggestion layer:** derived or learned frontier nodes and edges, carrying
   producer, model/version where relevant, confidence, and an explanation.

Showing or hiding a relation family changes projection state. Staging a
frontier node, accepting a suggested relationship, drawing a Set edit, or
editing a Card changes an owning document through an explicit action. A filter
must never silently delete a relation, and a suggestion must never silently
become catalog or Set truth.

One material pair may carry several relations at once. Ranking chooses what to
surface first; it must not deduplicate `diatonic`, `shares tones`,
`voice-leading`, and `practiced after` into one winning reason. Selecting an
edge should expose every applicable reason and its authority.

### Project through the scenograph scene contract

The shared projection engine named above is the `scenograph` family (`sceno`
core, `scenomise` layout, `scenotime` runtime), the product-family projection
compiler and runtime founded in mere's
`2026-07-21_projection_engine_prior_art_brief`, where Woodshed is already
forcing-function #3. Woodshed consumes its scene contract and does not import
mere's graph kernel. `StageGraphSnapshot` is Woodshed's source adapter into that
contract, the analog of mere's `cartography` graph adapter: Woodshed owns the
musical facts and typed relations; `sceno` owns selection, placement,
footprints, representation, and gesture routing.

The engine's vocabulary maps onto the instrument. A fretboard is a `sceno`
frame, a coordinate space from (string, fret) to screen; a note is a projected
item with a point footprint; a fingering is a path footprint; a Tonnetz or
circle is a second fixed-layout frame; a voice-leading relation is a routed
edge. The P4e catalog is therefore `scenomise` layouts over Woodshed source
facts, not seven hand-built swatches, and the current `related_swatch` radial
placement is the first thing the contract retires.

Woodshed is the source-model sanity check for that contract. Every other
consumer (merecat, hocket, isometry) authors or generates content before the
engine has anything to project; music theory hands Woodshed a dense,
deterministic, multi-relational fixture on day one, where one chord pair carries
diatonic, shared-tone, voice-leading, and practiced-after relations at once with
no authoring step. That makes it the natural stress test of the projection-graph
half of the contract (selection, multi-family edges, ranking without dedup),
complementary to isometry's proof of the scene half (footprints, placement,
representation). Its relations are static and deterministic, so it does not
exercise the late-arriving signal or streaming-uncertainty paths; it is the
clean first fixture, not the only proof. Sequencing follows: Woodshed's typed
relations (P4a identity, P4b relations) are wall-side truth and proceed now,
doubling as design pressure on `sceno`'s source and channel model;
scene-contract consumption waits until mere and isometry prove and freeze it,
since P4e swatches built before then are throwaway.

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
in `woodshed-core`; keep desktop realization in `woodshed-genet`.

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

### P4. Make the Stage graph the relationship and composition surface

P4 grows the landed Related swatch and the in-progress Set graph into one
reconfigurable projection over the current Set, catalog material, musical
context, and practice evidence. It does not replace `Set`, `Card`, or
`woodshed-graph`; it makes their relationships directly legible and editable.

#### P4a. Finish the derived Set graph baseline

Give every staged Card occurrence a stable `CardId` which survives reorder,
save/load, selection, Rehearsal, and Looper lowering. Staging the same material
twice creates two IDs. Duplicating a Card creates a new ID; editing or moving it
retains its ID. Migrate existing saved Sets once by assigning missing IDs on
load and persisting them on the next save.

Project the ordered Set into occurrence nodes addressed by `CardId`, with the
visible number derived from current Set order and `Next` edges derived between
adjacent occurrences. Keep the ordered `Vec<Card>` authoritative while the
product remains linear. Repeats and Set looping use the existing loop model.
Promote `Next` into authored flow only when an actual branching or alternate-
ending workflow requires it.

Replace the current all-or-nothing sequence-edge toggle with a serializable
relation-visibility set. Preserve node selection, graph/list cursor parity,
and graph focus through reorder or removal by stable identity rather than
vector index.

Done when duplicate material yields distinct stable occurrence nodes, reorder
changes numbering without changing identity, removal drops only incident
projected edges, a round-trip preserves the selected Card, and hiding `Next`
changes only the view.

#### P4b. Preserve typed musical relationships

Split neighbor ranking from relation truth. Replace the flattened
`RelatedMaterial { reason, score }` boundary with typed material relations that
retain source, target, direction, kind, weight or distance, explanation, and
provenance. Several relations may connect one pair. Ranking operates over
these records and may choose a display order without deleting multiplicity.

Grow the deterministic vocabulary from the existing `Contains`,
`FitsInScale`, and `Realizes` relations toward:

- contains pitch class, interval, degree, or material;
- mode of, relative to, parallel to, and fifth of;
- diatonic in, borrowed from, extends, alters, or substitutes for;
- dominant of and resolves to;
- shared tones and symmetric voice-leading distance;
- used together or adjacent within a catalog progression;
- practiced before/after and engagement strength.

Keep root-independent formulas, contextual realizations, and instrument
placements distinct. `scale:Major` and `chord:Dominant 7` remain catalog
formula identities. A view may derive `C major` or `G7` from formula + tonic
without multiplying the durable catalog twelvefold. Concrete voicings and
string/fret positions remain Card setting and projection output.

Add first-class pitch-class and interval identities when the first interval or
circle projection needs them. Do not encode those relationships only in label
text. Progression continuations must carry their context: current key,
preceding material, and whether the reason is theoretical, historical, or
learned.

Done when an edge can explain all applicable relationships between two
materials, formula and keyed-instance identity are unambiguous, deterministic
relations are available without history or ML, and ranking no longer erases
relation kinds.

#### P4c. Compose one Stage projection snapshot

Expose a portable `StageGraphSnapshot` from Woodshed core or a narrow adapter,
Woodshed's source adapter into `sceno`'s scene contract (see the scenograph
boundary decision above). Its inputs are the Set occurrence graph, the focused
catalog/material identity,
the current tonic/instrument/tuning, typed theory relations, practice evidence,
and optional analysis signals. Its output carries stable node-instance IDs,
typed edges, relation authority, selection, and representation hints. It must
not depend on Genet, Chisel, wgpu, Burn, or Mere's kernel.

Projection settings include:

- focus and expansion depth;
- visible relation families;
- layout/preset;
- theory, history, and learned-suggestion layers;
- node label mode and edge explanation mode;
- optional pinned visual positions;
- level-of-detail thresholds for glyph, summary, and Card forms.

Keep these settings separate from musical truth. Durable user choices live in
`AppSettings::stage`; transient hover, animation, and temporary expansion stay
in view state. The compact Related swatch and expanded Stage graph consume the
same snapshot and actions.

Done when one snapshot drives both surfaces, relation filters do not rebuild or
mutate the Set/catalog, disabling history retains deterministic theory, and a
suggested frontier is visibly distinct from staged Card occurrences.

#### P4d. Make nodes expand into Cards without changing identity

Give each staged occurrence three representations of the same Card:

- **Glyph:** number or Roman numeral, suitable for dense maps.
- **Summary:** material name, function, key, and compact state.
- **Card:** the shared editor with material, setting, touch, timing, voicing,
  audition, and Stage actions.

Selecting a node synchronizes the Set cursor and the alternate list/tray.
Expanding it changes projection state and gives the Card an assigned region;
editing it updates the one Card. Shrinking it restores the compact
representation. Expansion may move neighboring nodes but must preserve graph
focus and camera. Respect reduced-motion settings.

Keep semantics and actions in ordinary Cambium/`xilem_serval` elements. The
painted graph layer may draw geometry underneath, but each visible node and
edge explanation needs an accessible semantic target. The current external
selected-Card editor is an acceptable bridge; P4d is complete only when the
expanded Card occupies the node's projected region rather than appearing as an
unrelated panel.

Done when a numbered node expands into its editable Card and collapses back
without losing identity, selection, edits, keyboard focus, or graph position;
the same operations remain available through the list projection.

#### P4e. Ship a small projection catalog

Avoid one universal force layout. Each view should state which relationships
and coordinate rules make it intelligible:

1. **Set sequence:** numbered staged occurrences, `Next` as the primary path,
   harmonic and evidence edges optional.
2. **Focused relationships:** the current material with progressively
   expandable typed neighbors. Arbitrary depth is a query capability; the view
   reveals it on demand rather than drawing the entire catalog.
3. **Circle of fifths:** contextual keys in a fixed cycle, expandable into
   scales, diatonic chords, relative modes, and borrowed material.
4. **Interval map:** pitch classes connected by selected intervals, with paths
   projected onto the current instrument.
5. **Scale family:** scales arranged by mode, contained degrees, or set
   difference.
6. **Voice leading:** chords placed by motion cost with shared and moving tones
   exposed on edges.
7. **Progression possibilities:** a directed frontier conditioned on key and
   preceding staged Cards, separating deterministic function, catalog usage,
   practice history, and learned suggestions.

The Set-sequence and focused-relationship views are the first two consumers.
The circle of fifths is the first full theory-map acceptance surface because it
forces contextual material identity, fixed semantic layout, nested expansion,
and Stage/fretboard synchronization without requiring ML. These layouts are
`scenomise` arrangements over `sceno` frames rather than Woodshed-owned swatches,
and they land when the scene contract does (see the scenograph boundary
decision).

Done when the same focused material can move between Set, relationship, and
circle projections without changing musical truth; projection choice and
relation filters persist as settings; and at least two layouts have headed
interaction receipts rather than static screenshots.

#### P4f. Join graph understanding to sound and practice

Selecting a node updates Stage and the Fretboard. Selecting an edge exposes its
reasons and offers an audition appropriate to the relation: shared tones,
before/after chords, or animated voice movement. Staging a frontier node uses
the ordinary Stage action and states where the new occurrence will enter the
Set. The initial behavior may append; insertion after the focused occurrence
must be an explicit action before it is offered.

P5 supplies the shared clock and event-position identity. P6 and P7 consume
the same `Next` traversal for Rehearsal and Looper. Practice events annotate
the evidence layer after their honest lifecycle boundaries rather than after a
mere hover or projection change.

Burn-backed producers may later supply embeddings, transition likelihood,
clusters, or personalized frontier ranking. Keep inference off the render
path. Every learned edge carries model/version, confidence, and generation;
turning the learned layer off leaves the deterministic engine intact. Accepting
a suggestion is an explicit Stage or relation action.

Done when a user can stage a short progression from the visible frontier,
expand its Cards to choose voicings, hear and see why each transition relates,
run the numbered Set through Rehearsal, and reopen the same Set and projection
settings after restart.

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
- The in-progress `Set::graph()` slice is a useful wiring proof, not the final
  projection model. It addresses occurrences by vector index, derives only
  `Next`, lays them out in a hardcoded serpentine grid, exposes one Boolean edge
  toggle, and opens the shared Card editor beside the graph. P4a-P4d name the
  remaining identity, relationship, layout, and inline-representation work.
- Card is a sound domain unit, but the current UI should not force one visual
  card treatment across Stage, Set tray, Rehearsal, and Looper.
- Chisel is a good fit for custom-painted material projections. Its semantic
  event/action seam is still a placeholder, so ordinary `xilem_serval` elements
  remain the right owner for Card interaction and accessibility.
- `woodshed-graph` projects scales, chords, arpeggios, progression and exercise
  identities, scored theory relations, and practice lineage into both the
  Related list and Chisel neighborhood. The public neighbor boundary still
  flattens multiple relations into one reason/score and then rebuilds a
  center-star snapshot; progression, exercise, key, pitch-class, and interval
  relationships remain thin or absent.
- The current Cambium `GraphCanvasSwatch` carries uniform nodes and untyped
  `from/to` edges. It is enough for the Set-graph wiring proof. Directed and
  typed edge treatments, per-node regions, semantic zoom, and live Card slots
  belong at the shared projection boundary rather than as Woodshed-only swatch
  exceptions.
- Catalog formulas and contextual realizations are distinct. The catalog owns
  `Major` and `Dominant 7`; a projection derives `C major` and `G7` under the
  current tonic. A Card owns the concrete instrument setting and touch.
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

- **2026-07-22, scenograph reconciliation:** Named the shared projection engine
  as the `scenograph` family (`sceno`/`scenomise`/`scenotime`) founded in mere's
  projection-engine prior-art brief, where Woodshed is already forcing-function
  #3. Reframed `StageGraphSnapshot` as Woodshed's source adapter into `sceno`'s
  scene contract, the analog of mere's `cartography` graph adapter, and recorded
  the fretboard-as-frame, note-as-point, fingering-as-path mapping; the P4e
  catalog is `scenomise` layouts, not Woodshed swatches, and `related_swatch` is
  the first placement the contract retires. Recorded Woodshed's role as the
  source-model sanity check: a dense, deterministic, multi-relational fixture
  that needs no authoring, complementary to isometry's scene-side proof and not
  a replacement for the proof ladder. Sequencing unchanged on the truth side
  (P4a identity, P4b relations proceed now as contract pressure); scene-contract
  consumption waits for mere and isometry to prove and freeze it. No code
  changed.

- **2026-07-21, Stage graph projection plan:** Expanded P4 from a ranked Related
  neighborhood into the authored Stage-graph direction. The plan now separates
  Set, deterministic theory, practice evidence, and learned suggestions;
  requires stable Card-occurrence identity and typed multi-relations; defines
  node-to-Card semantic zoom; and names the Set, focused relationship, circle
  of fifths, interval, scale-family, voice-leading, and progression projections.
  No code was changed in this planning pass. The existing dirty-tree Set graph
  remains the P4a wiring baseline described below.

- **2026-07-21, authored Set graph slice:** Began evolving the Set tray from a
  card-only arrangement into a graph projection of the same Set. The portable
  Set now derives distinct numbered Card-occurrence nodes and typed `Next`
  edges; the view exposes sequence-edge visibility as a durable Stage setting.
  The graph remains a projection: staging, editing, Rehearsal, Looper, and
  persistence still address the one Set. Rich harmonic edge layers, alternative
  layouts, direct graph authoring, and stable Card identity across arbitrary
  branching remain follow-ons.

- **2026-07-11:** Reconciled the maintainer's product model with the live Set,
  PracticeSet, SongDoc, AudioBackend, capture engine, persistence, navigation,
  settings, and responsive view seams. Wrote the replacement plan and updated
  the project authority/index language.
- **2026-07-11, related-material slice:** Added a cached, explainable
  `woodshed-graph` neighbor query; mapped stable graph identities to core Stage
  selections; and added a responsive Related panel with Select and Stage
  actions. Renamed the existing `+ Rehearse` action to `Stage`. Verified 8
  graph tests and 35 core tests, `cargo check -p woodshed-views`,
  `cargo build -p woodshed-genet`, and a live Windows receipt at 1100x664.
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
- **2026-07-11, Stage Set tray slice:** Added a Stage-owned Set tray showing
  ordered Card kind, label, touch, dwell, tuning, and recipe provenance. The
  selected Card can move, duplicate, or be removed; the tray also owns clear,
  Set looping, and the transition into Rehearsal. Set Templates now fills the
  tray without navigating away from Stage. Verified views/desktop checks, 39
  core tests, a desktop build, and a live 1100x664 receipt over a real 12-card
  Set; duplicate/remove changed 12 -> 13 -> 12 as expected. P3 remains partial:
  Card timing/touch/placement editing still lives in Rehearsal, and the tray is
  document-bottom rather than a sticky/collapsible bottom rail.
- **2026-07-11, shared Card editor slice:** Moved touch, dwell, tempo override,
  and fret-window mutations behind shared `UiState` actions and exposed the
  same selected-Card editor in both the Stage Set tray and Rehearsal. The Stage
  tray can now collapse without changing the durable Set. P3 remains partial
  on reusable user Set save/load and whether the tray should become a sticky
  bottom rail; it is currently a collapsible document-bottom surface.
