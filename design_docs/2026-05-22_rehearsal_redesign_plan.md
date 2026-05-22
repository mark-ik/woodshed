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

## Decisions on the critique (Mark)

1. **Rehearsal queue** — agreed. A queue of cards/compositions you step through
   is the missing backbone; today's tabs are a flat menu, not a practice flow.
2. **Musical-object vocabulary (cards)** — agreed. Unify scale/chord/voicing/
   progression/arpeggio/exercise under one `Card` vocabulary.
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

- **Card is a tagged union, not trait-soup.** One `enum Card { Scale, Chord,
  Voicing, Progression, Arpeggio, Exercise }` with shared metadata (id, name,
  tags, root/key context), not a `dyn Card` trait. Pattern-match at the
  projection sites. Keeps it `serde`-trivial and avoids premature abstraction.
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
- **R4 — Move material authoring out of Settings.** The custom tuning/
  progression/exercise editors (★) leave Settings and attach to card creation
  where they belong. *Done when:* Settings holds only settings.
- **R5 (later) — Exercises as traversal over cards/compositions.** Generalize
  the exercise step-engine so an exercise can traverse *any* card or a
  composition (not just its own stored steps): "play this progression as an
  arpeggio run," "walk this scale in thirds." This is where exercise becomes the
  *verb* the spine describes.
- **R6 (later) — Relationship explorer.** Deferred per refinement above.

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
