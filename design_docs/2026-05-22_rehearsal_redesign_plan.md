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

## Next spine: the set and its cards (agreed direction 2026-05-23)

> **Doc housekeeping:** this section is the live plan going forward. R1–R4
> (below) are shipped foundation; when U1 begins, lead the doc with the card
> model and demote the R-series to Progress/history so the file reads as one
> roadmap, not two.

The R1/R2 `Card` queue proved material portability, but its kinds still mirror
the old tabs. The stronger model is a **set: cards played in sequence**, where
each card owns enough context to say:

- **what** material is being practiced;
- **where** it sits on the neck (its setting);
- **how** it's played (its touch);
- **how long** to stay on it (its timing).

That is the shared shape the current **Stage**, **Practice**, and **Song**
surfaces are all reaching for:

- Stage wants the current card's setting on the neck.
- Practice wants named or generated card sequences with auto-advance.
- Song wants bar/section card sequences with looping, audio, and per-bar
  context.
- Progressions, exercises, practice sets, and songs are **recipes** that fill a
  set with cards.
- Arpeggios are a **touch** over chord or voicing material, not a peer kind
  beside chord.

So the next implementation should not add yet another page. It should make
Practice and Song fill the same set, then let their UIs be **views** over it.

### Two axes: neck (space) × set (time)

Woodshed has two orthogonal axes, and they are the two halves of the card model:

- **The neck is space.** Strings by frets, where a note lives. Movement here
  (slide the capo, change the voicing, pan the fret window) changes *how the
  current card sits on the neck* without moving through the session. This axis
  is the card's **setting**.
- **The set is time.** The horizontal sequence of cards. Movement here (advance
  the cursor) changes *which card is current*; the neck reframes to follow. This
  axis is the card's **material** + **touch** + **timing**.

The case that confirms the model: a **capo** sits on the space axis. It's part
of the instrument setup, where the card sits on the neck, so it's a **setting**
field, not **material** (see the struct). Two visual "horizontals" must stay
distinct: the capo slides along the neck's own fret axis (space); the set scrubs
along time. They look alike and mean different things.

This also locates the crate boundary: **space stays in Woodshed** (the neck
rendering, the moat), while **the time axis travels** (the same sequence/timeline
shape Strophe makes literal as recorded loops). Promote the time-axis types at
U7; keep the neck rendering here.

### Target model

Keep the first implementation boring and serializable. Names/ids resolve at the
edge, just like R1 cards already do. Important split: `Material` is **atomic
practiceable material**, not everything the UI can name. Progressions,
exercises, practice sets, and songs are **recipes** that fill a set with
`Card`s.

```rust
struct Set {
    title: String,
    cards: Vec<Card>,
    cursor: usize,
    loop_mode: LoopMode,
    // No set-level `from` or `clock`: a set is heterogeneous (U6 mixes
    // chords/scales/progressions/song bars), so provenance is per-card
    // (`Card::from`) and the clock is derived from the card under the
    // cursor, not stored. See "Where a card came from, and what keeps
    // time" below.
}

struct Card {
    label: String,
    material: Material, // what you're working on
    setting: Setting,   // where/how it sits on the neck
    touch: Touch,       // how you play it
    timing: Timing,     // how long to stay on it
    // Where this card came from: which recipe stamped it. A one-time
    // stamp, NOT a live binding: editing the card does not resync to its
    // recipe, and re-running a recipe appends/replaces fresh cards rather
    // than mutating these in place. `None` = hand-added. (Default
    // decision; see below — flip to a span-level live binding only if
    // recipe-sync is wanted.)
    from: Option<Recipe>,
}

enum Material {
    Scale { name: String, root: PitchRef },
    Chord { name: String, root: PitchRef },
    // Do NOT add until a named-voicing-library feature pulls it. A
    // specific voicing is expressible today as `Chord +
    // Setting::voicing_idx`; a `Voicing` variant would create a second
    // path to the same positions and force the U5 resolver to handle
    // both. Add it only when a voicing needs identity divorced from a
    // chord (a saved/named custom voicing).
    // Voicing { chord: String, root: PitchRef, voicing_id: String },
    Riff { name: String }, // later
    NoteGroup { notes: Vec<PitchRef> }, // later
}

// A recipe makes cards: a progression, a practice set, an exercise, or a
// song each fills a set. No `Manual` variant: a hand-added card is
// `from: None`. (One way to say "no recipe", not two.)
enum Recipe {
    PracticeSet { name: String },
    Progression { name: String, key: PitchRef },
    Exercise { name: String },
    Song { name: String },
}

// The setting: how the card sits on the neck (the space axis).
struct Setting {
    instrument: String,
    tuning: Option<String>,
    fret_window: Option<FretWindow>,
    // Capo: part of the instrument setup, so it lives here on the space
    // axis. It raises every open string by N frets — the shape you
    // finger stays the same, the pitch it sounds rises — so the U5
    // resolver applies the shift when it computes sounding notes. `None`
    // = no capo. Distinct from `fret_window`, which only pans the view.
    capo: Option<u8>,
    // Some fields only apply to some material (voicing_idx is chord-only;
    // a scale ignores it). Don't type this coupling: the U5 resolver
    // *ignores* fields that don't apply to the material rather than
    // erroring.
    voicing_idx: Option<usize>,
}

// The touch: how you play the card.
enum Touch {
    Block,
    Arpeggiate { direction: ArpeggioDirection },
    ExercisePattern { name: String }, // a way of walking the material (e.g. in thirds)
    Strum { direction: StrumDirection }, // later, once affordances exist
}

// The timing: tempo, meter, and how long to stay on the card.
struct Timing {
    bpm: Option<f32>,
    meter: Option<TimeSignatureRef>,
    hold: Hold,
}

// How long to stay on a card before moving on.
enum Hold {
    Bars(u8),
    Seconds(f32),
    Reps(u16),
    Manual,
}

// What keeps time. A resolver output, NOT a stored field. Derived from
// the card under the cursor: a Song recipe or recorded loop → Song;
// otherwise Metronome (if running) or Manual. This is what lets one set
// interleave song-locked and free-practice cards.
enum Clock {
    Manual,
    Metronome,
    Song,
}
```

U1 should implement only the variants today's app exercises. The enum shape
keeps room for the known consumers, but unused variants should not get behavior
until U2/U3/U4 pulls them into use.

#### Where a card comes from, and what keeps time

Two properties that look set-global are actually per-card, because U6's set is
heterogeneous (it mixes chords/scales/progressions/song bars in one sequence):

- **Where a card came from: a one-time stamp (default).** A recipe
  (`PracticeSet` / `Progression` / `Song`) *fills* the set with cards that each
  carry a `from` label, then become editable clay. Editing a stamped card does
  not resync to its recipe; re-running a recipe appends or replaces a fresh
  batch. This keeps U6 ("reorder, duplicate, edit") simple and makes a mixed set
  expressible (each card knows its own `from`, the set carries none). **Decision
  flag:** the alternative is a *live* binding (change the progression's key and
  its cards recompile in place), which needs `from` on a *span* of cards and a
  defined regeneration step. Defaulting to one-time stamp; revisit only if
  recipe-sync becomes a real want. The `Recipe -> Card` validation test can't be
  written until this is locked, and the missing test is the tell.
- **What keeps time: derived at the cursor.** Not stored; computed from the
  current card (a `Song` recipe or recorded loop gives `Clock::Song`, else
  `Clock::Metronome` / `Clock::Manual`). The cursor moving onto a song bar hands
  time to the engine; moving back to a free scale returns it to the metronome.
  Same rule U4 states, generalized so one set can interleave both.

This does **not** mean all of this lands in `woodshedding` on day one. The rule:
portable, no-UI/no-audio operation types belong in `woodshedding`; app
persistence, selected-row state, engine handles, recorded buffers, and Xilem
views stay in consuming crates.

### Implementation phases: unifying Practice and Song

- **U1 — Grow the shipped `Card` into the richer card, app-side.** Do this
  before moving anything into `woodshedding`. Keep it name-based, serde-friendly,
  and mechanically derived from today's app state: today's card (name + root +
  kind) becomes a card with `material` / `setting` / `touch` / `timing`, behind
  a `serde(default)` migration so old saved sets still load. Keep U1 thin:
  implement only current behaviors (`Block` / `Arpeggiate`, `Hold::Bars` /
  `Hold::Manual`, manual stepping). *Done when:* the set renders today's cards
  unchanged on the richer shape, with no behavior loss.
- **U2 — Fill a set from a `PracticeSet`.** Replace `PracticeItem`-specific
  rendering paths with `practice_set_to_cards`. Preserve `practice_bpm`,
  `practice_bars_per_item`, auto-advance, and the current elapsed-seconds runner
  by mapping them onto `Hold::Bars`, `Hold::Seconds`, or `Hold::Manual` as
  appropriate. Resolve each card through one shared stage adapter. *Done when:*
  the Practice tab is a recipe that fills a set, not a separate runner.
- **U3 — Fill a set from a progression.** A selected `Progression` produces one
  chord card per role, with key/root context and a default `Block` touch. Later
  toggles can switch those cards to arpeggiate/strum/etc. *Done when:* a ii-V-I
  becomes a set in one action and loops like any practice set.
- **U4 — Fill a set from Song bars.** Keep `woodshed-audio::Song` as the
  audio/bar engine for now; add an app-side step that turns `Song::bars` into
  cards. Each bar becomes a card (or a repeated card when `length > 1`) with
  chord material, tempo, meter, section label, click, and recorded-loop
  metadata. *Done when:* the Song page and the set agree on the cursor/current
  card, while the song engine still owns recorded audio playback.

  **What keeps time:** U4 is a view boundary, not absorption. When a song or
  recorded loop is active, the engine owns time (`Clock::Song`) and the set
  cursor follows the song cursor. When free-practicing, the metronome or manual
  hold owns time. Do not add a third clock.
- **U5 — One stage resolver.** Converge the duplicated paths (`load_card`,
  `positions_for_practice_item`, and song chord display) onto
  `resolve_card_for_stage`. It returns positions, labels, the selected
  voicing/shape, transport hints, and warnings if material no longer resolves.
  *Done when:* Stage, Practice, Progression, Arpeggio, and Song use the same
  resolve path.

  This is the highest-effort refactor in the series. Today scales/chords/
  progressions/arpeggios/practice each compute `Vec<Position>` near their view
  code, and `load_card` mostly restores indices and switches tabs. U5 is where
  that work moves behind one resolver keyed by a `Card`.

  **Open before U7:** instrument/touch affordances. `Arpeggiate` makes sense for
  fretted chord/voicing material; `Strum` does not apply to every future
  instrument. Before touch types move into `woodshedding`, model which
  instruments/materials support which touches.
- **U6 — The set becomes a horizontal timeline track + inspector.** The set is
  the time axis, so render it as a **horizontal lane of cards with a playhead**,
  running under the neck, not the vertical list R2 shipped (that was a first
  cut; supersede it). The lane is the canonical practice surface: select,
  reorder (drag along the lane), loop, duplicate, remove, and edit the current
  card's touch/timing/setting in an inspector. Because it's one lane, **Practice
  and Song stop being separate runners and become the same track**: a practice
  set, a progression, and a song's bars all fill the one horizontal stream,
  which is also the shape Strophe's loop timeline takes. *Done when:* a user can
  build a set from chords / scales / progressions / exercises / song bars and
  scrub/play it from one horizontal surface, with the neck reframing per card.
- **U7 — Promote stable pure types into `woodshedding`.** Once U1-U6 settle the
  vocabulary, move the portable pieces (`Material`, `Setting`, `Touch`,
  `Timing`, `Hold`, recipe-compilation helpers) into `woodshedding`. Do not move
  Xilem state, settings adapters, audio buffers, clock glue, or engine handles.
  *Done when:* future CLI/web/app shells can consume the same card/set core
  without depending on `woodshed-xilem`.
- **U8 — Cleanup / collapse old affordances.** Remove or demote the old separate
  Practice runner state, redundant progression stepping, and tab-mirrored card
  kinds. Keep browse/view affordances only where they help author material.
  Relationship explorer remains deferred.

### Relationship to R5/R6

The U-series **subsumes R5**. "Exercises as traversal over cards/compositions"
becomes two explicit paths: an exercise recipe can **fill** a set with cards, or
an exercise pattern can **decorate** existing material as a `Touch`. R6
(relationship explorer) stays deferred and should remain a view over the settled
card/material graph, not a prerequisite for this work.

### Validation gates

- Existing `cargo test -p woodshedding` stays green throughout.
- Add unit tests for conversion functions:
  - `PracticeSet -> Card`s preserves item count and labels, and tags each card's
    `from` with the recipe.
  - `Progression -> Card`s produces one chord card per role with the expected
    chord roots in key.
  - `Song -> Card`s preserves bar count/length, tempo, meter, labels, chord
    refs, and cursor mapping.
  - One-time-stamp behavior: editing a stamped card, then re-running its recipe,
    appends/replaces rather than mutating the edited card in place (locks in the
    "where a card came from" decision above).
  - Clock is derived correctly: a set mixing a song card and a free-practice
    card reports `Clock::Song` on the former and `Clock::Metronome` / `Manual`
    on the latter at the cursor.
- Add app-level smoke checks before deleting old paths:
  - mixed hand-added cards still load on the stage;
  - a practice set auto-advances through the same set cursor;
  - a song bar cursor highlights the same card the set considers current;
  - old settings files without saved sets still load through `serde(default)`.

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
- 2026-05-23: **U1–U5 shipped (each builds + runs clean; committed in order).**
  The set-and-cards spine is now real code.
  - **U1** (`6549cb4`): replaced `Card`/`CardKind`/`Rehearsal` with the rich
    model — `Set { cards, cursor }`, `Card { label, material, setting, touch,
    timing, from }`, atomic `Material` (Scale/Chord/Riff), `Setting`, `Touch`
    (Block/Arpeggiate), `Timing`+`Hold`, `Recipe`. Capture maps each lens; a
    progression *expands* to one chord card per role (pulled forward since
    atomic material requires it); arpeggio is a Chord + Arpeggiate touch.
    `Settings.rehearsal` → `Settings.set` (old data loads empty, not error).
  - **U2** (`5016536`): Practice tab gains "➕ Rehearse this set" —
    `practice_item_to_card` + `fill_set_from_practice`, mapping bars-per-item
    onto `Hold::Bars`, tempo onto `Timing.bpm`, tagged `from` the set.
  - **U3** (`f032a66`): `LoopMode` + `Set.loop_mode` (rehearse_step wraps when
    looping); `cycle_card_touch` flips a chord card Block ⇄ Arp; Rehearsal view
    gains a Loop toggle + a per-row Block/Arp button on chord cards.
  - **U4a** (`1ce1b77`): `Recipe::Song` + `song_to_cards` (chord bars → chord
    cards, tempo/length/section preserved; silent bars skipped) +
    "➕ Rehearse this song" on the Song transport. **U4b** (live song
    cursor/clock sync, `Clock::Song` owning time) deferred to ride with U5/U6.
  - **U5 foundation** (`236cc05`): `StageRender` + `resolve_card_for_stage` —
    the canonical card→neck path (Scale/Chord/Riff resolve by name; missing
    name → warning, not empty neck). `Setting.fret_window` added; practice
    items pin their hand position there. Practice tab now renders through the
    resolver (first consumer; `positions_for_practice_item` removed). **Still
    owed:** converging the five lens render paths onto the resolver, which
    rides on U6's set-stage to avoid regressing the rich lens views in one pass.
  - **U6** (`e7d7912` stage/timeline/inspector + `7559cbc` auto-advance): the
    Rehearsal tab is now the **set stage**, not a vertical list. A neck renders
    the cursor card via `resolve_card_for_stage` (the resolver's second
    consumer, proving it; reframes per card with no lens switch; honors the
    card's pinned window; shows a warning instead of a blank neck). Below it, a
    **horizontal timeline lane** of card chips (`portal` + `constrain_vertical`,
    scrolls sideways; click a chip to put it on the neck, cursor chip pops in
    tertiary). An **inspector/transport** row: Play/Stop, scrub ◀/▶, Touch
    block/arp, move/duplicate/remove, "Edit on lens" (jumps to authoring), Loop,
    Clear all. **Auto-advance** (`tick_set_playback` + a ~50ms task in
    `app_logic`): a card's dwell comes from `card_duration_secs` (Hold + tempo;
    Manual → two bars so it flows), the cursor steps when it elapses, stops at
    the end unless looping. New helpers: `set_cursor` / `cursor_step` (no lens
    switch), `Set::duplicate`. Playback is **visual** (neck walks the set);
    sounding each card + the U4b song-engine clock handoff are later.
  - **Remaining in the U-series:** U7 (promote `Material`/`Setting`/`Touch`/
    `Timing`/`Hold` + compile helpers into `woodshedding`), U8 (retire the old
    Practice runner + redundant progression stepping). Deferred: U4b (song
    engine owns the clock, cursor follows), full lens-render convergence onto
    the resolver, and set-card audio.
