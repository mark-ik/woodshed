# Stage, Set, Tools, Rehearsal, and Looper Plan

> **2026-08-10 — the gate is open.** The scenograph scene contract this plan
> gates on ("what remains is the freeze, not the proof") froze 2026-07-24 at
> 0.0.3: emphasis channels and a default pick added, intents stay
> protocol-side, `sceno::measure` deleted. Isometry and turnstone are
> re-resolved against it. Adoption can begin; the expansion map, with
> woodshed adoption as its L1 release gate, is mere's
> `design_docs/mere_docs/research/2026-08-10_scenograph_expansion_brief.md`.

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
consumer (turnstone, hocket, isometry) authors or generates content before the
engine has anything to project; music theory hands Woodshed a dense,
deterministic, multi-relational fixture on day one, where one chord pair carries
diatonic, shared-tone, voice-leading, and practiced-after relations at once with
no authoring step. That makes it the natural stress test of the projection-graph
half of the contract (selection, multi-family edges, ranking without dedup),
complementary to isometry's proof of the scene half (footprints, placement,
representation). Its relations are static and deterministic, so it does not
exercise the late-arriving signal or streaming-uncertainty paths; it is the
clean first fixture, not the only proof.

Sequencing: Woodshed's typed relations (P4a identity, P4b relations) are
wall-side truth and proceed now, doubling as design pressure on `sceno`'s
source and channel model. Scene-contract consumption was gated on mere and
isometry proving and freezing the contract. **The proving half is done**
(verified 2026-07-24): mere consumes `sceno` through `cartography::scene_out`
and a persisted spiral score; isometry deleted `Overmap::layout` and its force
solver, emitting the same score/scene types for its overmap and tactical
board; a serialized coastal fixture exercises the geographic path; and
graphshell consumes `scenotime`'s snapshot/diff pair for remote replay. The
family also moved: `sceno`/`scenomise`/`scenotime` 0.0.2 now live in mere at
`crates/scenograph`, not a standalone repo.

**The freeze landed 2026-07-24**, published as `sceno` / `scenomise` /
`scenotime` / `scenograph` 0.0.3 on crates.io. The gate on scene-contract
consumption is lifted; Woodshed can adopt against a stated contract. The four
questions and their rulings, with what each means here:

- **Action intents stay out of `sceno`, permanently.** They live in the
  consuming protocol, bound to an instance id plus the epoch and revision it
  was observed at. Woodshed's gesture story is therefore its own to define,
  and it inherits no vocabulary it would have to agree with.
- **`measure` is deleted.** Hosts stamp the measured extent on
  `ScoreItem.footprint`, which is where `StageGraphSnapshot` should put a
  note's or fingering's size. There is no separate measurement map to fill.
- **Per-item emphasis channels landed** as an open `Vec<(String, f32)>` on
  `ProjectedItem`. Practice recency is exactly this shape: a per-note or
  per-card scalar the view shades by, carried inside the scene rather than
  read back out of Woodshed's store.
- **Picking landed in `scenotime`**, resolving a point to the topmost
  instance through the space chain. This is directly the fretboard case: a
  `Space` mapping (string, fret) to screen means a click resolves to a note
  instance without Woodshed writing hit-testing at all.

**The multi-reason requirement is satisfied with no contract change**, which
was the open worry. A chord pair carrying diatonic, shared-tone,
voice-leading, and practiced-after is **four `RoutedRelation`s**, not one
relation with four reasons. Relations deliberately did not get a channel map,
because mere had already ruled that multi-edge is truth and collapsing to one
line is an experience setting. Selecting an edge exposing every applicable
reason and its authority, the requirement stated above, is the fanned form
rendered without dedup.

Two
consequences for this plan. First, `scenomise::relax` (2026-07-23) is
dependency-free relaxation aimed squarely at swatch-scale surfaces, which is
exactly what `related_swatch` is, so the retirement named above has a landed
mechanism waiting for it.

Second, the arrangement question is **not** answered by the freeze, and that
is deliberate rather than an oversight. The shipped arrangements remain
`Spiral`, `Board`, and `Geographic`; a circle of fifths and an interval map
are fixed semantic layouts none of them covers. The freeze settled the
contract's *shape* questions, not the arrangement catalog, which is a growth
axis: `Arrangement` is a closed enum, so adding a variant is a routine break
at `0.0.x` and remains available whenever it is earned.

The recommendation is to start with `Placement::Coordinate` inside a
Woodshed-owned `Space`, which needs no upstream change at all and is the
usage the contract note already blesses (a fretboard *is* a `Space`; a
Tonnetz or circle is a second fixed-layout frame). Promote a shared
arrangement upstream only when a second consumer wants the same layout,
which is the same "decide when a consumer forces it" standard the family
applied to every question it just closed. Woodshed proving the fixed-layout
case locally is the evidence that would justify promoting it.

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

**Landed 2026-07-24** (see Progress), with unit tests per done condition and a
headed receipt (`scenarios/p4a_occurrence_identity.scn`). `Card` identity is not
yet consumed by the Looper: preserving captures by occurrence rather than by bar
index is P7's work, and it is the reason this slice came first.

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

**Landed 2026-07-24** (see Progress), with unit tests per done condition and a
headed receipt (`scenarios/p4b_typed_relations.scn`). The keyed half is
deliberately not in it: `diatonic in`, `borrowed from`, `dominant of`,
`resolves to`, `relative to`, and `parallel to` are contextual relations that
exist only once a tonic is chosen, so they land with the keyed-instance layer
and first-class pitch-class identities rather than being faked over formulas.

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
`scenomise` arrangements over `sceno` frames rather than Woodshed-owned
swatches. The shipped arrangements (`Spiral`, `Board`, `Geographic`) do not
cover a circle of fifths or an interval map, so this catalog is where Woodshed
either contributes a fixed-semantic-layout arrangement upstream or places
authored coordinates in its own frame (see the scenograph boundary decision).

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
- `Set::graph()` now addresses occurrences by stable `CardId` and filters by a
  serializable relation set (P4a, 2026-07-24). What remains of the original
  finding still stands: it derives only `Next`, lays occurrences out in a
  hardcoded serpentine grid, and opens the shared Card editor beside the graph
  rather than in the node's own region. P4b-P4d name the remaining
  relationship, layout, and inline-representation work.
- Card is a sound domain unit, but the current UI should not force one visual
  card treatment across Stage, Set tray, Rehearsal, and Looper.
- Chisel is a good fit for custom-painted material projections. Its semantic
  event/action seam is still a placeholder, so ordinary `xilem_serval` elements
  remain the right owner for Card interaction and accessibility.
- `woodshed-graph` projects scales, chords, arpeggios, progression and exercise
  identities, typed theory relations, and practice lineage into both the
  Related list and Chisel neighborhood. The flattened `{reason, score}` boundary
  is gone (P4b, 2026-07-24): a pair now carries every applicable relation, each
  with its own weight, measurement, and authority. What remains thin: keyed
  relations (diatonic in, borrowed from, dominant of, resolves to, relative and
  parallel) are deliberately absent, because they exist only under a chosen
  tonic and belong to the keyed-instance layer with first-class pitch-class and
  interval identities; the center-star snapshot rebuild is still there.
- Ranking crowding is a real effect, found by receipt: `Major` appears in nine
  catalog progressions that all score 96, so a six-row panel showed one relation
  family and no harmonic neighbour at all. Display now interleaves families,
  which deletes nothing a longer list would have kept, but it is a symptom worth
  remembering — a flat weight per relation kind ranks by family, not by
  usefulness to the player.
- The Related panel scrolls inside a row whose height the fretboard sets, so at
  1500x1200 roughly two rows are visible and the multiplicity line needs a
  scroll to reach. The relations are correct and observable; their presentation
  is not yet. Panel height belongs with P8's adaptive compositions.
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

- **2026-07-24, P4b landed — typed relations, and the end of the flattened
  reason:** `woodshed-graph`'s public boundary was `RelatedMaterial { reason,
  score }`, one winning string per neighbour, and the index deduplicated by
  target so the second and third ways two materials relate were discarded before
  anyone could see them. It is now `MaterialRelation { source, target, kind,
  weight, distance, shared_tones, explanation, authority }` with
  `RelatedNeighbor` carrying **every** relation for a pair, `relations_between`
  for the full list, and `RelationKind` naming a 14-member deterministic
  vocabulary with `inverse`/`is_symmetric` so one computation records both
  directions honestly. New derived relations, all root-independent: chord
  `Extends`/`ExtendedBy` (strict subset), `Alters` (same size, one tone moved),
  `SharesTones` and `VoiceLeadsTo` as *separate* records rather than one blended
  score, scale `ModeOf` (rotation of the same interval set) and `ScaleNeighbor`
  (one degree apart), and `UsedTogether` for chords the catalog itself puts
  adjacent inside a progression. Every record carries its `RelationAuthority`
  (Catalog / Computed / Evidence), and `MaterialRelation::evidence` is the only
  public constructor, so an observation cannot enter wearing catalog authority.
  Core's history ranking now *inserts* an evidence relation instead of
  overwriting the theory reason, which was the erasure the plan named. The
  Related row shows its primary reason plus an `also ...` line naming the other
  kinds. Verified: 14 graph tests (multiplicity survives ranking, authorities
  are attributable, modes relate as modes, deterministic relations need no
  history), 46 core tests, and `p4b_typed_relations.scn` asserting on the
  relation records themselves rather than on rendered text: `RESULT ok`, two
  captures. **Found by receipt**: the first run failed honestly. Every visible
  neighbour of `Major` carried exactly one relation, because nine catalog
  progressions all score 96 and filled the six-row panel; the multi-relation
  pairs sat at rank 10. Added a display-side family interleave (documented as a
  display policy that deletes nothing) and a test pinning it. Recorded but not
  fixed: the panel's own height leaves about two rows visible.

- **2026-07-26, the evidence layer adopted stemma.** Asked whether the clock work
  should generalize to mere first; the investigation inverted the question. **Mere
  already had the general thing and woodshed had built a second one**:
  `chartulary::stemma`, already a dependency through `woodshed-graph`, maintains
  per-subject `first_seen_at_ms` / `last_seen_at_ms` / `visit_count` and
  aggregates per-pair traversals with their own recency — the exact inputs a
  decayed strength model needs and exactly what the local log did not keep. Its
  `context: X` is a generic per-visit payload, and `StemmaSnapshot` is serde, so
  it rides woodshed's existing String-moving `Storage` seam with no new
  persistence lane. **Adopted rather than deferred** (Mark: unification is worth
  work up front, rather than growing two implementations of one idea) — and the
  deferral had been incoherent anyway, since the trigger I named was the strength
  model, which is the next task.
  `PracticeHistory` keeps its name, vocabulary, and every consumer; only the store
  beneath it moved. Woodshed still owns the musical judgment (`EngagementKind`,
  `is_practice`), the substrate owns the structure — the same line P4b drew.
  **Three mappings decided by the code, not by preference.** The engagement kind
  rides the visit `context`, not stemma's `TransitionKind`, whose variants answer
  "how did you get here" and map one-to-one onto a browsing trace (recorded on
  that type in chartulary, which had "generalize TransitionKind" as a declared
  open decision; the finding is that if it is ever generalized it wants a type
  parameter, preserving `Copy`/`Hash`/rkyv, not an open string tail). Datedness
  rides the context too, since `visit_entry` takes a `u64` and not an `Option`, so
  an undated engagement is `at_ms = 0` plus `dated: false` and no reader may read
  it as 1970. And **the stated reason is not the walked path**: a test failure
  caught that discarding `from_id` in favour of stemma's parent edge silently lost
  provenance — a first engagement has no parent yet can still name its source — so
  `from_id` lives in the context as the player's claim while the lineage keeps the
  path actually walked. Both are now queryable and distinct:
  `related_transition_count` (the stated reason, ranking a suggestion the player
  has taken before) and `traversals` (the lineage's own count plus its recency,
  which is what the strength model will decay).
  Gained for free: `engagement_count` maintained upstream, per-pair recency, and a
  class of bug deleted rather than fixed — the sequence counter that could collide
  on a legacy load no longer exists, because the lineage has no counter.
  One bounded migration: a session written as the flat log replays into the
  lineage on load and persists as a lineage from then on, with each engagement's
  kind, time, and span preserved exactly. Verified: 8 history tests including the
  flat-log replay and the stated-reason/walked-path separation, 53 core + 14 graph
  + 6 views + 167 woodshedding green, host builds, and both headed receipts re-run
  `RESULT ok`.
  **Next, on the substrate that now supports it**: the strength function
  (recency-decayed, kind-weighted, emitting both `PracticedBefore`/`PracticedAfter`
  plus subject-level strength) replacing core's `weight = 90 + count.min(10)`.
  Retention stays after that, and stemma does not solve it either — `delete_owner`
  collects ownerless branches, not old visits — so the Alembic/Athanor forgetting
  pass remains the answer, with `codicil` as the append-only home a growing
  lineage wants.

- **2026-07-26, the evidence layer gets a clock and a stopwatch:** The layer
  the plan defines as "observed practice transitions and engagement strength,
  with event kind and observation time retained" was failing its own spec:
  `PracticeEvent` carried only a `sequence` counter, with a comment admitting a
  host could enrich it "when history gains calendar views". Everything downstream
  wanted that field — strength is inherently time-weighted (twenty reps last year
  is not strength today), retention cannot evict what it cannot date, and the
  ranking fudge in core was `weight = 90 + count.min(10)` standing in for a
  model. So: `PracticeEvent.at_ms` (Unix epoch millis) and `practiced_ms` (the
  measured span, where an event has one), `record` now takes the time positionally
  so no call site can forget it silently and returns `&mut PracticeEvent` so a
  caller with a span attaches it (one door, optional enrichment, rather than a
  second timed variant to pick wrong). Reads that make the data mean something:
  `total_practiced_ms`, `last_seen_ms`, `has_times`.
  **The clock belongs to the host.** Core and views read none of their own — a
  browser host has a different clock and the portable core has no clock at all —
  so `UiState.now_ms` is refreshed once per frame by `woodshed-genet` and every
  engagement is dated from there. `None` means *unknown*, never epoch zero, or
  every legacy event would silently become maximally ancient.
  **Elapsed practice is now measured, not counted.** `record_rehearsal_cursor`
  opens a span when a card becomes active; `complete_rehearsal_cursor` closes it,
  and completing one card opens the next one's, so spans neither overlap nor
  restart from the run's beginning. A `checked_sub` means a system-time change
  mid-session yields no measurement rather than a wrapped one. This is the
  difference the plan already asserted and could not previously honour: preview
  and staging are evidence of *interest*, completed practice time is evidence of
  practice.
  **Found while writing the legacy test**: `next_sequence` is serialized but
  defaults to 0, so a session missing it would have minted duplicate sequences
  over its own events. Same shape as the `CardId` mint, fixed the same way (the
  mint continues past the highest stored event, not merely past the counter).
  Verified: 5 new `history` tests (supplied time retained, measured practice
  outweighing a pile of previews, spans accumulating, the legacy-session
  migration and its collision), 4 new view tests (the real span, no clock, a
  backwards clock, consecutive cards), 50 core + 6 views + 167 woodshedding + 14
  graph green, desktop host builds. No wire migration code needed: both fields are
  `#[serde(default)]`, so old sessions load as undated and persist dated on the
  next save.
  **Still open, in dependency order** (the chain this slice unblocks): a strength
  model to replace the count fudge (recency-decayed, kind-weighted, emitting both
  `PracticedBefore`/`PracticedAfter` and a subject-level strength the
  practice-strength overlay needs), then retention — the real wall, since
  `events` grows forever inside the sealed session JSON rewritten on every save.
  Retention is where mere's **Alembic/Athanor** pattern is the answer rather than
  an analogy: raw events as the short layer, distilled per-subject and per-pair
  strength as the long layer, a forgetting pass evicting raw events without
  losing the strength they contributed, and `codicil` as the append-only home a
  growing event log actually wants.

- **2026-07-24, P4a receipt — woodshed drives itself:** Woodshed had no
  self-drive lane, so its receipts were SendKeys plus a desktop grab, which the
  harness notes warn loses the foreground race and can photograph the wrong
  window. It now consumes `genet-probe`: the generic half (scenario parsing, the
  verb loop, selector resolution, assertions) is the shared crate, and what
  landed here is only woodshed's half —
  [`crates/woodshed-genet/src/scenario.rs`](../crates/woodshed-genet/src/scenario.rs)
  implementing `Automatable`/`Driveable` (surfaces, a typed snapshot, semantic
  events diffed from real state transitions, named commands, pointer routing
  through the app's own hit-test path) plus an in-process capture: the frame's
  own rasterized view composed into a `COPY_SRC` target and read back, so a
  capture needs no compositor, no foreground, and no ffmpeg. `WOODSHED_STATE`
  points the session at a scratch profile, because an automated run would
  otherwise read and then overwrite the real practice session.
  `p4a_occurrence_identity.scn` stages one catalog material three times, so the
  three occurrences are label-identical and only identity can tell them apart:
  it selects the second through its DOM key, reorders it, and asserts the id
  held while the number moved 2 -> 3, then hides the relation family and asserts
  the edges went while the occurrences stayed. `RESULT ok`, four captures in
  `testing/woodshed/scenarios/p4a_occurrence_identity/`, run through
  `testing/woodshed/run-scenario.ps1`. **Found by looking at the frames**: the
  relation toggle sat beside the swatch and was overdrawn by node labels that
  paint past the swatch's box. Fixed by giving the controls their own row; the
  underlying overflow (a 520px swatch with labels wider than its node spacing)
  is layout work for P4d/P4e, recorded rather than papered over.

- **2026-07-24, P4a landed — occurrence identity:** Every staged Card carries a
  `CardId`, minted by the owning `Set` and never reused, so staging the same
  material twice yields two occurrences and duplicating mints a third while the
  original keeps its own. `Set` gained `from_cards`, `ensure_card_ids`,
  `index_of`, `id_at`, `cursor_id`, `select_id`, `card`/`card_mut`;
  `Set::graph()` addresses nodes and `Next` edges by id, with the visible number
  and serpentine slot derived from current order. The all-or-nothing edge toggle
  became `StageSettings::visible_set_relations`, a serializable set over
  `SetGraphEdgeKind` with `ALL`/`label`, so the harmonic, evidence, and
  suggestion families join it as members; `SetGraph::with_relations` filters the
  projection without touching Set truth, and the Settings page lists one entry
  per family. The Set-graph swatch is now keyed by `CardId` (selection, hover,
  DOM key `set-card-<id>`), and native focus is tracked honestly through
  `graph_canvas_swatch_with_focus` rather than painting a ring where the
  keyboard is not. One bounded load migration in `apply_persisted`: legacy Sets
  gain ids, the legacy boolean folds into the relation set, and both persist on
  the next save; the legacy key stops being written. Verified: 7 new
  `woodshedding` tests (167 total) covering distinct occurrences, reorder,
  removal without id reuse, relation hiding, round-trip identity, and legacy
  migration idempotence; a new `woodshed-core` storage test for the settings
  fold (45 total); `cargo check` green for `woodshed-views` and
  `woodshed-genet`. **Build blocker found and repointed**: genet's 2026-07-24
  sweep moved the family to cambium 0.3.1 / cambium-winit 0.3.0 / sprigging
  0.2.1, which silently stopped matching this workspace's 0.2.0 pins, so the
  local `[patch]` entries went unused and the host built against the published
  0.2.0 API without `graph_canvas_swatch` or `on_hover`, while looking green.
  Resolved by taking all three from **genet.git by branch**, not from crates.io
  (Mark, 2026-07-24: cambium-winit will never be published, and the
  consolidation rewired it onto crates inheriting genet's `publish = false`;
  hocket reached the same conclusion the same day off a clean Linux checkout).
  One source for the family is not a preference: `cambium-winit` path-deps
  cambium and sprigging inside the genet repo, so a git `cambium-winit` beside a
  registry `cambium` puts two copies of the same types in one graph, and the
  published cambium/sprigging carry the crates.io `paint_list_api` while the
  rest of the stack git-deps netrender's. Two dead `[patch.crates-io]` entries
  removed, and `tinct`'s redirect moved into the genet.git table where it can
  actually match: it had been keyed to its retired standalone repo, so the local
  checkout was silently unused. **The lesson generalizes**: every one of these
  announced itself only as a "patch was not used" warning, which is the one
  cargo message this workspace must never scroll past. No headed receipt yet: the changed
  surfaces (node selection, the relation toggle's label) want one before P4a is
  called finished.

- **2026-07-24, the gate opened:** Re-checked the scenograph sequencing clause
  against the family's actual state. Mere and isometry both proved the scene
  contract on 2026-07-22 (mere's `cartography::scene_out` + persisted spiral
  score with a headed receipt; isometry deleting `Overmap::layout` and emitting
  the same score/scene types), P5's geographic fixture landed the same day, and
  2026-07-23 added `scenomise::relax` plus graphshell's consumption of
  `scenotime` diffs. So the "wait for mere and isometry" half of the gate is
  satisfied and only the freeze remains; the boundary decision now names the
  specific open items rather than a general wait. Also recorded that the family
  moved into mere at `crates/scenograph` (0.0.2) in the 2026-07-23
  consolidation, and that Woodshed's fixed semantic layouts are not covered by
  the shipped `Spiral`/`Board`/`Geographic` arrangements, which is the first
  design question Woodshed owes the contract. Counterpart notes recorded on the
  mere side in the scene contract note and the prior-art brief. No code
  changed.

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
