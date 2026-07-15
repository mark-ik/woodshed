# Material and Touch: a unifying model

A design reframe from a 2026-07-15 conversation. Extends, and partly reframes,
[2026-07-11_stage_set_tools_plan.md](2026-07-11_stage_set_tools_plan.md). Not an
implementation plan yet: a shape to build toward, with the gaps named.

## The reframe

Content is arranged notes and relationships. Scale, chord, arpeggio,
progression, and exercise are not five kinds of thing; they are one substrate
seen through five treatments. The distinction a player feels is real, but it
lives in the **touch**, not the content.

## Two axes

### Material (the what)

A set of notes with relationships and positions on the neck: intervals from a
root, fret positions, and an intrinsic order (a scale runs stepwise, a chord
stacks, a progression is a timeline).

Material has an **abstraction level**:

- **Abstract (a generic):** pure relationships. A scale degree with a quality
  (ii7, V7, bVII), no key, no neck position.
- **Concrete:** a generic realized at a root and voiced on the strings.

So "content is relationships" is literally the abstract layer; the notes are its
realization. This is the axis that lets progressions be assembled key-free and
dropped into a key on demand.

### Touch (the how)

How you traverse and voice a material in time. The expressive axis, and it is
content-aware. Its parts:

- **Path:** the order notes are visited. Ascending, descending, up-down,
  down-up, all-at-once (block), or a custom path (which is what an exercise is).
- **Rhythm:** note value (whole down to 32nd) plus grouping (duplets, triplets,
  tuplets) mapping the path onto the tempo.
- **Selection:** which notes take part. A range within a scale, a string group
  within a voicing, an octave span.

A playable card is Material times Touch. The same material can be touched many
ways: a voicing is "block" or "arpeggiate up in triplets"; a scale is "run frets
1 to 5 ascending in eighths."

## Generics and realization

A library of generics furnishes every degree, quality, extension, alteration,
and inversion. You assemble a progression from generics; it stays key-independent
until realized into a key, where each generic becomes a concrete voicing on the
neck. Generics are the relationship layer made first-class.

## Material of materials

A progression is a timeline of (usually abstract) materials, each with its own
touch. That is a thin layer above the Material-times-Touch pair, not something to
force into it.

## The lenses as presets

- **Scale** = a scale note-set + a run touch (range, direction, rhythm).
- **Chord** = a voicing + a block touch.
- **Arpeggio** = a voicing + a sequence touch. An arpeggio is a chord touched
  melodically, not a separate content type.
- **Progression** = a timeline of generics, realized into a key.
- **Exercise** = a material + a custom touch-path. The octave spider is a
  spidering path across strings, a touch pattern, not a new content type.

## Playback is the expression of touch (the keystone)

Today touch is a label, not a behavior: up, down, and block sound and look the
same. In this shape, playback walks the path at the rhythm, sounding notes in
sequence for an arpeggio and together for a block, and lights the markers in
step. Up and down reverse the path; block flashes together. Ear and eye both get
it, with no new UI.

This is the smallest first step and the load-bearing one. The pieces exist: the
metronome clock, single-note play (`preview_note`), and the animated paint leaf.
Wiring "walk the path at the subdivision, sound and light each step" is the
keystone; richer paths, string groups, and tuplets hang off it.

## The board is an editor

If content is arranged notes, sculpting the arrangement is clicking notes on and
off. Clicking a position toggles its membership in the current material (add or
remove), so the board becomes the material editor, not just a display. This is
also the natural home for authoring and for the specialty cases.

**Interaction decision (resolved 2026-07-15):** **click edits, hover shows.** On
the editable board, click toggles a note's membership and hover peeks the detail
card (real `on_hover` events are now in Cambium). Clicking to pin a detail card
stays on the read-only Stage board until the editable surface lands, so the two
gestures never collide on the same board.

## Authoring

Create material splits cleanly: author a note-set (a material) or author a path
(a touch, i.e. an exercise). Most specialty cases are one or the other. The
octave spider is the second.

## Open questions and gaps

- **Subdivision clock:** tie path steps to the metronome's beats and a tuplet
  grid. This is the substantive engineering under the keystone.
- **Voicing membership:** string groups need voicings that know per-string
  membership, which the chord voicings likely already imply.
- ~~**Pin vs toggle** on marker click~~ — resolved: click edits, hover shows.
- **How much collapses:** how much current per-lens code folds into Material x
  Touch versus staying as preset builders.
- ~~**Rehearsal board** still uses the old CSS grid~~ — done: it now paints on
  the same Sprigging leaf as the Stage board and is the click-to-edit surface.

## A path (phased, feature targets)

1. **Keystone.** *(landed 2026-07-15)* Stepping playback for one case (a scale
   run and a chord arpeggio) on the existing clock: audible and visible. Proves
   "content is notes, lenses are treatments" in the hand.
2. **Rehearsal board to the paint leaf.** *(landed 2026-07-15)* Consistency, and
   the editable surface.
3. **Click-to-toggle membership.** *(landed 2026-07-15)* The board as editor
   (after the pin-vs-toggle decision).
4. **Content-aware touch editor.** Draw the path as an ordered trail over the
   markers, plus the pulse, so the treatment is shown, not just named.
5. **Generics and progression assembly.** The generics library; build a
   progression from degrees and realize into a key.
6. **Material authoring.** Author note-sets and paths.

## Progress

- **2026-07-15**: Drafted from the design conversation (touch should be audible
  and visible; scales and chords want different touches; progressions from
  generics; material of materials; the board as editor; content is arranged
  notes and relationships). The marker-detail work
  ([2026-07-15_fretboard_marker_detail_plan.md](2026-07-15_fretboard_marker_detail_plan.md))
  is the first interaction brick this builds on.
- **2026-07-15**: **Keystone (Phase 1) landed and verified.** Stepping playback
  on the existing metronome clock: a "♪ Run" / "■ Stop" button in the board
  caption walks the placed notes in pitch order, sounding each with
  `preview_note` and lighting the sounding marker a bright warm accent
  (`C_ACTIVE`) in the paint leaf. Verified on Aadd11: two frames a beat apart
  showed the active marker step A → C# audibly and visibly. Wiring:
  `StageState::{scale_run_playing, scale_run_step, scale_run_active}` +
  `scale_run_path`/`scale_run_tick`/`toggle_scale_run`; the host beat loop ticks
  it and pushes `scale_run_active` to `FretboardLeaf::set_active` each frame
  (`sync_fretboard_active`). Not yet committed. Path is pitch-sorted for now;
  direction/rhythm/selection (real touch parameters) hang off this.
- **Interaction decision resolved:** **click edits, hover shows.** On the
  editable board, click toggles a note's membership; hover peeks the detail card
  (needs real `on_hover` events, now shipped in Cambium). This settles the open
  pin-vs-toggle question below. The Stage board keeps click-pins-detail until the
  toggle surface lands.
- **2026-07-15**: **Phases 2 + 3 landed and verified** (the pair Mark asked for
  together). The Rehearsal board now paints on the same Sprigging leaf as the
  Stage board — a second leaf (`REHEARSAL_FRETBOARD_LEAF_KEY`) fed from the card
  under the cursor, synced host-side by `sync_rehearsal_fretboard_leaf`. Over it,
  each note label is a click target: clicking toggles that position's membership
  in the card. A deactivated position renders dim (`C_MUTED` marker + faded
  label) and stays clickable to switch back on. Verified on the Aadd11 card: the
  root A dimmed on click and returned to amber on a second click, while the
  card's *other* A positions stayed lit (position-level, not pitch-level).
  Wiring: `Setting::muted: Vec<(usize,u8)>` (neck-space, serde-default, so zero
  churn on Card builders); `UiState::{toggle_card_mute, card_muted}`;
  `fretboard_leaf::Dot::muted` + paint precedence (active > muted > root > note).
  Not yet committed.
  - **Deferred (honest scope):** muting is *visual/structural* today. It does not
    yet silence the note in the card's block-strum "hear it as you land" — that
    audio path (`card_voicing`) is pitch-set based, not position based, so a
    position→pitch reconciliation (a pitch is off only when all its positions
    are) is the follow-on that makes muting audible. Muted positions *are* absent
    from a position walk (dots), so a future rehearsal-board Run would already
    skip them.
