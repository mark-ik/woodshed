# Rehearsal redesign — from prototype to designed UI

Status: **design — branch `rehearsal-redesign` (destructive edits sanctioned).**
`main` holds the known-good Phase 3/4 checkpoint (commit 6fc2a63). This branch
refines the prototype into a designed instrument, bulldozing where the prototype
shape blocks the designed shape.

## The spine (agreed 2026-05-22)

> **Cards** are Woodshed's musical-object vocabulary. **Compositions** arrange
> cards. **Exercises** define how cards or compositions are *traversed,
> constrained, or generated*. **Rehearsal** binds them into live musical work
> through *projections* — the instrument stage, song view, queue view, and
> (later) relationship explorer.

This replaces the current "lens" framing as the *organizing* idea. Lenses don't
disappear — they become **projections of cards onto the instrument stage**. The
tab strip (Scales · Chords · Progressions · Exercises · Arpeggios) is the
prototype's seam and the first thing to rework: those five are not five
coordinate features, they are *card kinds* + *traversal modes* over one stage.

## Next spine: rehearsal elements (agreed direction 2026-05-23)

The R1/R2 `Card` queue proved material portability, but `CardKind` still mirrors
the old tabs. The stronger model is a **rehearsal queue of elements played in
sequence**, where each element owns enough context to say:

- **what** material is being practiced;
- **where/how** it is realized on the instrument;
- **how** it is articulated or traversed;
- **how long / how often** it should be rehearsed.

That is the shared shape the current **Fretboard**, **Practice**, and **Song**
surfaces are all reaching for:

- Fretboard/Stage wants the current element's instrument realization.
- Practice wants named or generated element sequences with auto-advance.
- Song wants bar/section element sequences with looping, audio, and per-bar
  context.
- Progressions are named templates that compile into chord elements.
- Exercises are traversal/generation recipes that compile into elements or
  decorate existing elements.
- Arpeggios are articulations/traversals over chord or voicing material, not a
  peer ontology beside chord.

So the next implementation should not add yet another page. It should make
Practice and Song compile into the same rehearsal runtime, then let their UIs
be projections over that runtime.

### Target model

Keep the first implementation boring and serializable. Names/ids resolve at the
edge, just like R1 cards already do. Important split: `MaterialRef` is **atomic
practiceable material**, not every thing the UI can name. Progressions,
exercises, practice sets, and songs are sequence sources that compile/project
into `Vec<RehearsalElement>`.

```rust
struct Rehearsal {
    title: String,
    source: Option<RehearsalSource>,
    elements: Vec<RehearsalElement>,
    cursor: usize,
    loop_mode: LoopMode,
    clock: ClockAuthority,
}

struct RehearsalElement {
    label: String,
    material: MaterialRef,
    realization: RealizationSpec,
    articulation: ArticulationSpec,
    timing: TimingSpec,
}

enum MaterialRef {
    Scale { name: String, root: PitchRef },
    Chord { name: String, root: PitchRef },
    // Optional later promotion if a saved voicing needs identity
    // independent of "Chord + RealizationSpec::voicing_idx".
    Voicing { chord: String, root: PitchRef, voicing_id: String },
    Riff { name: String }, // later
    NoteGroup { notes: Vec<PitchRef> }, // later
}

enum RehearsalSource {
    Manual,
    PracticeSet { name: String },
    Progression { name: String, key: PitchRef },
    ExerciseRecipe { name: String },
    SongProjection { song_name: String },
}

struct RealizationSpec {
    instrument: String,
    tuning: Option<String>,
    fret_window: Option<FretWindow>,
    voicing_idx: Option<usize>,
}

enum ArticulationSpec {
    Block,
    Arpeggiate { direction: ArpeggioDirection },
    ExercisePattern { name: String }, // decorator over existing material
    Strum { direction: StrumDirection }, // later, once affordances exist
}

struct TimingSpec {
    bpm: Option<f32>,
    meter: Option<TimeSignatureRef>,
    advance: AdvancePolicy,
}

enum AdvancePolicy {
    Bars(u8),
    Seconds(f32),
    Repetitions(u16),
    Manual,
}

enum ClockAuthority {
    Manual,
    MetronomeGrid,
    SongEngine,
}
```

U1 should implement only the variants today's app exercises. The enum shape
keeps room for the known consumers, but unused variants should not get behavior
until U2/U3/U4 pulls them into use.

This does **not** mean all of this lands in `woodshedding` on day one. The rule:
portable, no-UI/no-audio operation types belong in `woodshedding`; app
persistence, selected-row state, engine handles, recorded buffers, and Xilem
views stay in consuming crates.

### Implementation phases: unifying Practice and Song

- **U1 — Add an app-side `RehearsalElement` runtime next to `Card`.** Do this
  before moving anything into `woodshedding`. Keep it name-based, serde-friendly,
  and mechanically derived from today's app state. Add conversion from current
  `Card` to a one-element rehearsal sequence. Keep U1 thin: implement only
  current app behaviors (`Block` / `Arpeggiate`, `Bars` / `Manual`, manual
  queue stepping). *Done when:* the current Rehearsal tab can render both old
  Cards and new Elements, with no behavior loss.
- **U2 — Compile `PracticeSet` into `RehearsalElement`s.** Replace
  `PracticeItem`-specific rendering paths with `practice_set_to_rehearsal`.
  Preserve `practice_bpm`, `practice_bars_per_item`, auto-advance, and the
  current elapsed-seconds runner by mapping them into `AdvancePolicy::Bars`,
  `AdvancePolicy::Seconds`, or `AdvancePolicy::Manual` as appropriate. Resolve
  each element through one shared stage adapter. *Done when:* the Practice tab
  is a generator/selector for a rehearsal queue, not a separate runner.
- **U3 — Compile progressions into element sequences.** A selected
  `Progression` produces one chord element per role, with key/root context and
  a default block articulation. Later toggles can switch those elements to
  arpeggiate/strum/etc. *Done when:* a ii-V-I can become a rehearsal queue in
  one action and can be looped like any practice set.
- **U4 — Compile Song bars into element sequences.** Keep
  `woodshed-audio::Song` as the audio/bar engine for now; add an app-side
  projection from `Song::bars` into rehearsal elements. Each bar becomes an
  element (or repeated element when `length > 1`) with chord material, tempo,
  meter, section label, click, and recorded-loop metadata. *Done when:* the Song
  page and Rehearsal queue agree on cursor/current element, while the song
  engine still owns recorded audio playback.

  **Clock authority:** U4 is a projection boundary, not absorption. When a song
  or recorded loop is active, `SongEngine` owns time and the rehearsal cursor
  follows the song cursor. When free-practicing, the metronome grid or manual
  advance owns time. Do not introduce a third clock in the rehearsal runtime.
- **U5 — One stage resolver.** Replace duplicated code paths:
  `load_card`, `positions_for_practice_item`, and song chord display should
  converge on `resolve_rehearsal_element_for_stage`. It returns positions,
  labels, selected voicing/shape, transport hints, and warnings if material no
  longer resolves. *Done when:* Fretboard/Stage, Practice, Progression, Arpeggio,
  and Song use the same realization path.

  This is the highest-effort refactor in the series. Today scales/chords/
  progressions/arpeggios/practice each compute `Vec<Position>` near their view
  code, and `load_card` mostly restores indices + switches tabs. U5 is where
  that work moves behind one resolver keyed by `RehearsalElement`.

  **Open before U7:** instrument/articulation affordances. `Arpeggiate` makes
  sense for fretted chord/voicing material; `Strum` does not apply to every
  future instrument. Before articulation specs move into `woodshedding`, model
  which instruments/materials support which articulations.
- **U6 — Rehearsal page becomes queue/timeline + inspector.** The queue is the
  canonical practice surface: select, reorder, loop, duplicate, remove, edit
  articulation, edit timing/repeats. Practice and Song become ways to generate
  or project this queue, not separate conceptual engines. *Done when:* a user can
  build a custom sequence from chords/scales/progressions/exercises/song bars
  and practice it from one surface.
- **U7 — Promote stable pure types into `woodshedding`.** Once U1-U6 settle the
  vocabulary, move the portable pieces (`MaterialRef`, `RealizationSpec`,
  `ArticulationSpec`, `TimingSpec`, `AdvancePolicy`, sequence compilation
  helpers) into `woodshedding`. Do not move Xilem state, settings adapters, audio
  buffers, clock authority glue, or engine handles. *Done when:* future CLI/web/
  app shells can consume the same rehearsal-operation core without depending on
  `woodshed-xilem`.
- **U8 — Cleanup / collapse old affordances.** Remove or demote the old separate
  Practice runner state, redundant progression stepping, and tab-mirrored
  `CardKind` assumptions. Keep browser/projection affordances only where they
  help author material. Relationship explorer remains deferred.

### Relationship to R5/R6

The U-series **subsumes R5**. "Exercises as traversal over cards/compositions"
becomes two explicit paths: exercise recipes can **generate** element sequences,
or exercise patterns can **decorate** existing material as articulation. R6
(relationship explorer) stays deferred and should remain a projection over the
settled element/material graph, not a prerequisite for this work.

### Validation gates

- Existing `cargo test -p woodshedding` stays green throughout.
- Add unit tests for conversion functions:
  - `PracticeSet -> RehearsalElement` preserves item count and labels.
  - `Progression source -> RehearsalElement` produces one chord element per role
    with the expected chord roots in key.
  - `Song projection -> RehearsalElement` preserves bar count/length, tempo,
    meter, labels, chord refs, and cursor mapping.
- Add app-level smoke checks before deleting old paths:
  - mixed manual cards still load on the stage;
  - a generated practice set auto-advances through the same queue cursor;
  - a song bar cursor highlights the same element the queue considers current;
  - old settings files without element queues still load through `serde(default)`.

## Decisions on the critique (Mark)

1. **Rehearsal queue** — agreed. A queue of cards/compositions you step through
   is the missing backbone; today's tabs are a flat menu, not a practice flow.
2. **Musical-object vocabulary (cards)** — agreed for the R1 capture layer:
   heterogeneous lens selections needed one portable queue item. The U-series
   corrects the ontology: progression/exercise/arpeggio are not all atomic
   material; they compile to or decorate rehearsal elements.
3. **UI critique (cruft, dead space, stepper-arrow noise)** — agreed. Remove
   dev scaffolding, fill right-pane dead space with material, reduce the
   ◀▶-stepper grammar.
4. **(critique pt 4)** — *not sold unless the cruft is removed first.* Hence
   **bulldoze → build**: strip the prototype lens-nav and dev cruft before
   layering the new card/queue model on top, rather than grafting onto scaffold.
5a. **Strophe parity / shared-crate boundary** — Woodshed need not match
   Strophe's DAW-lite sophistication or its p2p presentation; they share crates
   for *infra*, and the boundary clarifies as Strophe is built. **Do not chase
   the boundary now** — let it emerge.
5b. Agreed.

### My refinements (carried into the model)

- **R1 Card is a tagged union, not trait-soup.** The shipped adapter uses
  `CardKind` variants for Scale/Chord/Progression/Exercise/Arpeggio with shared
  metadata, not a `dyn Card` trait. Keep that serde-trivial shape while it is a
  migration adapter, but do not promote the tab-shaped variants as final theory:
  U-series splits atomic material from sequence generators and articulations.
- **Rehearsal is the integration hot-spot.** Every projection (stage, song,
  queue) reads from the rehearsal layer; that's where the wiring concentrates.
  Build it as a thin owned model (`Rehearsal { queue: Vec<CardRef>, cursor }`),
  not threaded through every view.
- **Defer the relationship explorer.** Highest-effort, lowest-immediate-payoff
  projection. Note it in the model, build it last.
- **Instrument realization is the moat.** The thing Woodshed does that a
  flashcard app can't: render any card as live, playable, steppable positions on
  a real tuned fretboard. Protect and deepen this; it's the differentiator vs.
  both Strophe (DAW) and generic theory trainers.

## Phased migration — lead with material portability

The first slice is chosen to be **non-destructive and immediately useful**, to
prove the card vocabulary before bulldozing nav: make any lens able to **send
its current material into a rehearsal queue / practice slot**. This forces the
`Card` type into existence and gives the queue its first real content, without
yet touching the tab strip.

- **R1 — Card vocabulary + "Add to rehearsal."** Define `enum Card` + shared
  metadata + `CardRef`. Add a `Rehearsal { queue, cursor }` to `AppState`
  (persisted). Add an "➕ Rehearse" affordance on each lens that captures the
  current material (selected scale/chord/voicing/progression/arpeggio/exercise +
  root/tuning context) as a `Card` and pushes it to the queue. *Done when:* you
  can collect heterogeneous cards from every lens into one queue and see them
  listed. No nav change yet — additive.
- **R2 — Queue projection (the rehearsal view).** A queue panel: ordered cards,
  cursor, next/prev, remove, reorder. Selecting/advancing the cursor loads that
  card onto the instrument stage (reusing the existing fretboard surface +
  transport). *Done when:* you can step a practice session through a queue of
  mixed card kinds, each realized on the neck.
- **R3 — Bulldoze the lens nav → stage + card-kind selector.** Replace the flat
  tab strip with: one **instrument stage** (the shared surface) + a card-kind/
  source selector that feeds it. The five "lenses" become *how you author/pick a
  card*, not five separate pages. Remove dev cruft ("(Xilem)" title, "Xilem
  migration scaffold" label) in the same pass. *Done when:* the stage is the
  center of gravity and lens-switching is card-selection, not page-switching.
- **R4 — Move material authoring out of Settings.** The custom progression /
  exercise editors (★) leave Settings and attach to their lenses (where you pick
  the card). **Tunings stay in Settings** (Mark's call, 2026-05-22): a tuning is
  shared context, not a card kind, and doesn't belong on the top bar. *Done
  when:* Settings holds preferences + tunings only; progression/exercise
  authoring lives on its lens. ✅ shipped — see Progress.
- **R5 (folded into U-series) — Exercises as traversal over elements.**
  Generalize the exercise step-engine so an exercise can generate rehearsal
  elements or decorate existing material as an articulation: "play this
  progression as an arpeggio run," "walk this scale in thirds." See U2/U5/U6.
- **R6 (later) — Relationship explorer.** Deferred per refinement above and
  still not part of the U-series implementation path.

### UI-polish backlog (fold into the phases, not a separate pass)

- Remove dev cruft (title "(Xilem)", "Xilem migration scaffold" label). → R3.
- Reduce ◀▶-stepper grammar noise (root/instrument/tuning/fret-span all use the
  same arrow idiom; differentiate or consolidate). → R3.
- Right-pane dead space → fill with queue / card detail. → R2.
- **P1 bug:** Chord lens fretboard renders empty when the selected voicing falls
  outside the fret window — auto-anchor the window to the voicing, or fall back
  to all-tones. Fix opportunistically (touches the stage rendering R2/R3 will
  rework anyway).

## Findings

- **Card stores selections by name, not catalog index.** `CardKind` holds the
  scale/chord/progression/exercise *name* (catalog or user — names are unique
  within their domain). `load_card` resolves by name at realization time, so a
  catalog edit between sessions can't silently load the wrong material; a removed
  entry simply fails to resolve (the lens keeps its current selection).
- **Instrument is restored by family, not exact tuning.** A card records the
  instrument string ("Guitar" / "Bass" / …); `load_card` retunes to that
  family's *default* catalog tuning, not the specific (possibly custom) tuning
  active at capture. Acceptable for R1/R2; revisit if cards need to pin a tuning.
- **`Tab::Rehearsal` added as the first non-lens projection** (11 tabs now).
  Sits next to Song in the tab bar. This is additive — the bulldoze of the lens
  nav (R3) will fold the lenses into the stage but Rehearsal stays a projection.

## Progress

- 2026-05-22: Plan created. Branch `rehearsal-redesign` cut from the Phase 3/4
  checkpoint (6fc2a63). Spine + critique decisions + my refinements recorded.
  First slice = **R1 material portability** (additive, non-destructive) to force
  the `Card` type into existence before bulldozing nav.
- 2026-05-22: **R1 + R2 shipped (builds + runs clean).**
  - **R1 model:** `Card { name, root, instrument, kind }` + tagged-union
    `CardKind` (Scale/Chord/Progression/Exercise/Arpeggio) + `Rehearsal { queue,
    cursor }` with `push`/`remove`/`move_card`/`card_at_cursor`. All `serde`;
    persisted via `Settings.rehearsal` (`#[serde(default)]`, round-trips through
    `apply_settings`/`snapshot_settings`).
  - **R1 capture:** `AppState::capture_card()` builds a card from the active
    lens (root + instrument + name-based selection); `rehearse_current()` pushes
    it. Header gains a **➕ Rehearse (n)** button (on lens tabs only) — *replaced*
    the "Xilem migration scaffold" dev-cruft label, so one cruft item is already
    gone.
  - **R2 projection:** `Tab::Rehearsal` + `rehearsal_view` — ordered card list,
    cursor marker (▶), kind badge, per-row ▲▼ reorder / **Load** / ✕ remove, a
    Clear-all, and an empty state pointing back at the capture affordance.
    `load_card(idx)` restores root + instrument, resolves the selection by name,
    and jumps to the matching lens.
  - Pending interactive check by Mark: capture from each of the 5 lenses, Load
    round-trips correctly, reorder/remove behave, queue survives a restart.
- 2026-05-22: **R3 shipped (builds + runs clean).** Reframed the nav so the
  stage is the center of gravity and lens-switching reads as card selection.
  - **R3a — cruft removed:** window title "Woodshed (Xilem)" → "Woodshed"; the
    "Xilem migration scaffold" header label was already replaced by ➕ Rehearse
    in R1. Cleared 6 dead imports (`OneOf9`, `progress_bar`, `Exercise`,
    `DiagramColors`, `SP_5/6/8`, duplicate `Style as _`). Warnings 13 → 7 (the
    remaining 7 are pre-existing dead code: `danger_label`, `palette_for`,
    `light`, two enum `ALL`/`label`, two `unused variable` — left alone, not
    redesign cruft).
  - **R3b — nav reframe:** tab-bar "Fretboard" meta-destination → **Stage**; the
    lens bar's "Lens:" prefix → **"Material:"** (the five entries now read as
    *which kind of card is on the stage*, not five separate pages). Surface +
    root + tuning still carry across unchanged, as before.
  - **R3c — rehearsal cursor on the stage:** when the queue is non-empty, the
    Material bar shows a right-aligned **"Rehearsing k/N · card-name"** strip with
    ◀/▶ that step the cursor and load each card onto the stage
    (`AppState::rehearse_step`). The queue now drives the stage as a live
    practice flow, not just a list you Load from.
  - Deferred (logged, not done): header ◀▶ stepper-grammar consolidation
    (instrument/tuning/fret-span share the arrow idiom) — the cyclers are
    functional and Mark previously chose them over popup overlays, so this is a
    visual-density polish for later, not a structural blocker.
  - Follow-up fix: the "Rehearsing k/N · name" strip clipped mid-word at narrow
    window widths → compacted to **◀ ♪ k/N ▶** (the card's name/material is
    already shown on the stage, so dropping it from the strip removes the
    overflow without losing information).
- 2026-05-22: **R4 shipped (builds + runs clean).** Material authoring moved out
  of Settings to where you pick the card.
  - **Decision:** progressions → Progression lens, exercises → Exercise lens,
    **tunings stay in Settings** (shared context, not a card; kept off the top
    bar per Mark).
  - Extracted `user_progression_editor(palette, &def)` and
    `user_exercise_editor(palette, &def)` free fns (own their data, no `state`
    borrow). Each lens sidebar gains **+ New …**; when the selected item is a
    user one (★), its editor opens in the **right pane below** the chord grid /
    info panel — filling the right-pane dead space the critique flagged.
  - Removed the Custom-progressions + Custom-exercises sections from
    `settings_view` (≈190 lines spliced out); Settings now = Theme + Custom
    tunings + Persistence. Deleted the now-dead `apply_user_progression` /
    `apply_user_exercise` (the lens list rows select directly). No new warnings.
