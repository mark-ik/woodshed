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

## The board is an editor: mark, then act

If content is arranged notes, sculpting the arrangement is selecting notes and
acting on the selection. The primitive is **marking**: clicking a marker marks
it (a neutral selection, an accent ring), nothing more. A **mode** then decides
what the marked set means:

- **Off** — marks are just a saved selection; everything still sounds.
- **Solo** — only the marked notes play; the rest go dim and quiet.
- **Mute** — the marked notes go dim and quiet; the rest play.

This is a general select-then-act pattern — the "node" is a fretboard marker
here, but the same shape lands on graph nodes — and it *is* the touch model's
**Selection** axis (which notes take part) made interactive. Marking authors the
selection; the mode is how a touch applies it. It replaced an earlier
per-position mute that could only dim, not sound: mark + solo/mute is the model
that makes the selection audible in one click.

**Interaction decision (resolved 2026-07-15):** **click marks, hover shows.** On
the editable board, click marks a note and hover peeks its detail card (real
`on_hover` events are now in Cambium). Clicking to pin a detail card stays on the
read-only Stage board until hover-peek lands, so the two gestures never collide.

## Authoring

Create material splits cleanly: author a note-set (a material) or author a path
(a touch, i.e. an exercise). Most specialty cases are one or the other. The
octave spider is the second.

### Drawing the path is authoring (2026-07-15 idea)

The Path trail (Phase 4) is read-only today — a view of a derived order. Make it
*drawable* and it stops being a view: **a drawn path is the arrangement.** A
path's vertices are the notes, its segments the relationships (each step is an
interval + a hand move), so drawing a path authors both the *what* (which notes)
and the *how* (what order) in one gesture. Material and touch, which this model
splits, collapse at this end — the path is both.

That makes one spectrum out of the lenses:

- **Preset end:** a lens hands you a note-set (scale/chord from theory); you draw
  a path *over* it (run, arpeggiate). Notes given, traversal authored.
- **Free end:** no given set; you draw from scratch and the vertices *are* the
  material — a lick, an exercise, the spider.
- **Middle:** draw constrained — snap to a scale's tones (a melodic pattern in a
  key) or a chord's positions (a specific arpeggio fingering).

The operations (the verbs a player reaches for):

- **Join notes (draw):** click markers in sequence; the trail becomes the
  authored path, numbered. Snap: free / to-scale / to-chord.
- **Determine treatment:** the path is the order; hang the rest of touch on it
  (note values, tuplets, direction: as-drawn / reverse / palindrome / loop).
  Playing walks the drawn order — the stepping run already does this.
- **Shift ordering:** two moves — reorder steps (drag a vertex, insert/delete),
  *and* shift the whole shape (transpose up a string/fret or by an interval).
  The second **is** the octave spider: draw one cell, shift it across the neck →
  a generated drill. A path + a shift rule = an exercise.

It ladders into generics: a path over concrete positions is a specific lick; a
path over the degree ladder (1, b3, 5…) is a key-independent pattern, transposable
anywhere — a portable exercise. And **save closes the loop**: a drawn path (+
treatment + shift) is a new Riff/exercise material that stages, rehearses, and
joins a set. Draw → save → practice.

**First slice (landed 2026-07-16, Stage board).** A **Draw** toggle in the board
caption: while lit, clicking a marker appends it to `StageState::authored_path`
instead of pinning its detail card, so you draw the touch's path by hand. The
existing trail renders it in click order and the stepping Run walks it (the
`effective_run_path` seam: a hand-drawn path overrides the derived pitch-order).
Editing tools — Undo / Reverse (retrograde) / Rotate (shift the start) / Clear —
sit beside it. Reuses the Path trail, the marker click-surface, and the keystone;
no new leaf work. Verified: drew a 6-step path, the Run stepped along the drawn
order (not the pitch order).

**Save as material (landed 2026-07-16) — the loop closes.** A **Save** in the
draw tools turns the drawn path into a card. This needed a new material kind:
every other `Material` *names* a catalog formula (`Scale`/`Chord` by name+root,
`Riff` by exercise name), but a drawn path has no catalog entry — it *is* its
content. Hence `Material::Path { positions, root }`, carrying the ordered
positions inline plus the root they were drawn over so degrees still resolve.
Consequences, each of which fell out cleanly:

- `dots_for_card` resolves a Path via `Fretboard::pitch_at`, naming each note and
  its degree from the stored root (positions off a narrower neck are dropped, not
  panicked on).
- `card_voicing` sounds a Path as a cascade **in drawn order** (its order is the
  material), not a block.
- `catalog_id_for_card` became `Option<String>`: a drawn path is not a catalog
  subject, so it stays out of practice history and the Related graph. Honest —
  it has no graph identity to record.
- Mark + solo/mute compose with Path cards for free (they filter dots/pitch
  classes, which a Path now supplies).

Verified end-to-end: drew A→B→C→C# over A Major Blues, Save → "3 cards", and the
card lands in the set as **Path · "A Major Blues path — 4 notes"**; rehearsing it
shows exactly those four notes (root A amber) instead of the 40-position scale.
Unit-tested (`drawn_path_saves_as_a_playable_card`): the card round-trips to the
drawn positions *in drawn order*, every note carries a real pitch, the voicing
sounds all of them, and it has no catalog id.

Follow-ons, in rough order:

- ~~**Naming + touch.**~~ **Landed 2026-07-16** (below).
- ~~**Step numbers.**~~ **Landed 2026-07-16** (below).
- **Snap is already the behaviour, free drawing is the gap.** The board only
  renders click targets for the current lens's tones (`labels` come from
  `state.dots()`), so drawing is *already* constrained to the scale/chord — there
  is no snap to add. The missing counterpart is **free drawing** anywhere on the
  neck, which needs click targets on empty positions (and pitch resolved from the
  fretboard, which `effective_run_path` now does).
- **Generics.** Draw over the degree ladder for a key-independent (transposable)
  pattern, realized at any root.
- **A shift that fits a 12-fret neck.** The octave shift landed but is
  unreachable (below). A **scale-step shift** — move each note to the next scale
  tone on its string — stays in the material *and* fits the neck, and is the
  classic sequencing drill ("1-2-3, 2-3-4, 3-4-5"). Likely the more valuable
  generator than the octave.

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
3. **Mark + solo/mute.** *(landed 2026-07-15)* The board as editor: click marks
   a note; an `[Off · Solo · Mute]` mode acts on the marked set (visible dim +
   audible filter). The Selection axis of touch, made interactive.
4. **Content-aware touch editor.** *(v1 landed 2026-07-15)* Draw the path as an
   ordered trail over the markers, plus the pulse, so the treatment is shown, not
   just named.
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
- **Interaction decision resolved:** **click marks, hover shows.** On the
  editable board, click marks a note (a neutral selection); hover peeks the
  detail card (needs real `on_hover` events, now shipped in Cambium). This
  settles the open pin-vs-toggle question below. The Stage board keeps
  click-pins-detail until hover-peek lands.
- **2026-07-15**: **Phase 2 (rehearsal board → paint leaf) landed and verified.**
  The Rehearsal board now paints on the same Sprigging leaf as the Stage board —
  a second leaf (`REHEARSAL_FRETBOARD_LEAF_KEY`) fed from the card under the
  cursor, synced host-side by `sync_rehearsal_fretboard_leaf`.
- **2026-07-15**: **Phase 3 reframed to mark + solo/mute, landed and verified.**
  Mark's reframe: clicking a note *marks* it (a neutral selection, a cyan ring),
  and a separate `[Off · Solo · Mute]` mode decides what marks do — Solo plays
  only the marked, Mute silences them; the excluded set dims. A general
  select-then-act primitive, and the touch model's **Selection** axis made
  interactive. This *replaced* the first-cut per-position `muted` (which could
  only dim, not sound). Verified on Aadd11: three top-string notes marked (rings)
  → Solo lit only those and dimmed the rest → Mute inverted it; a unit test
  (`card_sounding_pitches_respects_mark_mode`) proves the audio (Off = full
  voicing, Solo = the marked positions' pitches, Mute = voicing minus the marked
  pitch classes). Wiring: `Setting::{marked: Vec<(usize,u8)>, mark_mode:
  MarkMode}` (serde-default, zero churn on Card builders);
  `StageState::card_sounding_pitches` + `pc_from_hz`, routed through the Hear /
  dwell paths; `UiState::{toggle_card_mark, set_card_mark_mode, clear_card_marks,
  card_marked, card_excluded}`; `fretboard_leaf::Dot::{marked, excluded}` with a
  `C_MARK` ring + `C_EXCLUDED` dim (precedence active > excluded > root > note,
  ring on top). Not yet committed.
  - **Deferred:** Solo plays the literal marked positions; Mute subtracts the
    marked pitch classes from the clean voicing (so a dense whole-neck board
    doesn't wash). A rehearsal-board stepping Run that walks only the effective
    set (reusing the keystone) is the natural next audible surface.
- **2026-07-15**: **Interactive graph-canvas swatch + linked data pane landed
  and verified** (Related panel restructure, Mark's "graph beside the toggles /
  data panes" direction). The static neighbourhood glyph became Cambium's
  interactive `graph_canvas_swatch`: nodes are the current material (centre) +
  suggestions (satellites), id = `Option<RelatedTarget>` so a node links 1:1 to
  its pane row. Beside it, the loose Stage/Hide list became a structured pane
  (kind badge / name / why / Stage / ×). Hovering a node highlights its row and
  rings the node; hovering a row rings the node (shared `UiState::related_hover`);
  clicking a node navigates (verified: node click → lens Arpeggios); Expand grows
  the swatch. Wiring: `related_swatch(ui)` builds the shared `GraphCanvasSwatch`
  (view renders it, host `sync_related_swatch` paints its leaf with a host-side
  `related_kind_color` palette). Not yet committed.
  - **Cambium fix (sibling repo, uncommitted):** the `graph_canvas_swatch`
    component's container used `display:inline-block; overflow:hidden`, which
    suppressed the custom-leaf's composited paint in Genet (the leaf rendered but
    was invisible). Changed to `display:block` with no `overflow:hidden` (nodes
    are already inset, so the clip was redundant). This is the correct fix at the
    component, not a woodshed workaround.
- **2026-07-15**: **Touch editor v1 (Phase 4) landed and verified.** A "Path"
  toggle on the Stage board draws the touch's path as a translucent trail
  (`C_PATH`) threaded through the markers in visit order (pitch-ascending run
  order), under the markers so they stay legible; the stepping active marker (the
  keystone) moves along it as the pulse. Path + pulse together show the treatment,
  not just its name. Wiring: `StageState::{path_shown, toggle_path,
  run_positions}`; `FretboardLeaf::{path, show_path, set_path}` +
  `Path::polyline` draw; host `sync_fretboard_active` pushes the ordered path +
  toggle each frame. On a whole-neck 28-position board the trail is dense (honest
  for that many notes); it reads cleanest on a scale-in-position or a soloed
  shape, so it composes with the mark/solo selection. Step numbers and a rhythm
  grid (note values, tuplets) are the follow-ons. Not yet committed.
- **2026-07-15**: **Hover-peek landed and verified** (the "hover shows" half).
  Hovering a marker on the rehearsal board peeks its detail card (note, degree,
  interval, position, Play); moving off clears it. Click still marks. Wiring:
  Cambium's `on_hover` wraps each marker's `clickable`
  (`UiState::hover_peek: Option<(usize,u8)>` set on Enter, cleared on Leave); the
  host grew `hover_dispatch()` — hit-test each `CursorMoved`, route `on_hover`
  Enter/Leave as the hit node changes (`last_hover_hit`, transition detection
  host-side). Verified on Aadd11: hover the A → "A2 · 1 · Root · string 6 · fret
  5" card peeked; move away → gone; no panics through the dispatch→rebuild path.
  Not yet committed.
- **2026-07-16**: The four entries above (mark + solo/mute, touch path, hover-peek,
  graph-canvas swatch) **landed in commit `5d4d752`** — the "Not yet committed"
  notes on them are historical.
- **2026-07-16**: **Draw mode landed and verified** (Stage board) — drawing the
  path is authoring, first slice. A "Draw" toggle beside Run/Path; while lit,
  clicking a marker appends it to `StageState::authored_path` (else it pins the
  detail card, via `board_marker_click`). The trail renders the drawn order and
  the Run walks it — the `effective_run_path` seam: a non-empty `authored_path`
  overrides the derived pitch-order for both the trail (`run_positions`) and the
  step (`scale_run_tick`). Tools: Undo / Reverse / Rotate / Clear. Wiring:
  `StageState::{draw_mode, authored_path, toggle_draw_mode, append_to_path,
  undo_path, clear_path, reverse_path, rotate_path, effective_run_path}`; host
  `sync_fretboard_active` shows the trail when `draw_mode`. Verified: drew a
  6-step path (A→C#→D→E→A→D), the Run stepped along the drawn order, not the
  pitch order. Transient (save-as-material is the next leg). Not yet committed.
- **2026-07-16**: **Save as material landed and verified — draw → save → practice
  closes.** A drawn path saves as a card via the new `Material::Path { positions,
  root }`: the first material that *carries* its content instead of naming a
  catalog formula. `dots_for_card` resolves it through `Fretboard::pitch_at`
  (degrees from the stored root); `card_voicing` cascades it in drawn order;
  `catalog_id_for_card` became `Option<String>` so a drawn path stays out of
  practice history / the Related graph (it has no catalog identity). Wiring:
  `StageState::card_from_drawn_path`, `UiState::save_drawn_path`, a Save tool.
  Verified: drew A→B→C→C# over A Major Blues → Save → "3 cards" → the card
  rehearses as **Path · "A Major Blues path — 4 notes"** showing exactly those
  four notes (root A amber). Unit test `drawn_path_saves_as_a_playable_card`
  asserts the round-trip (drawn order preserved, real pitches, voices all, no
  catalog id). 41 core tests green. Committed `dd490eb` (with Draw mode).
- **2026-07-16**: **Rename + the Walk touch landed and verified.**
  - **`Touch::Walk`** — the third touch: *visit the material's notes in the order
    the material carries* (a drawn Path walks as drawn, a scale climbs). Where
    Arpeggiate imposes a direction on a chord's tones, Walk defers to the
    material's own order, which is the whole point of material that has one. It
    is a behaviour, not a label: `card_voicing` gives a Walk card the cascade
    shape instead of a block. Drawn paths now save with `Touch::Walk` (they were
    `Block`, which was a lie the UI printed). Cycle: block → arp up/down/updown →
    walk → block. Verified: a freshly drawn path reads "walk" in the filmstrip
    and the editor, while a path saved before the change still reads "block".
  - **Rename** — a text field in the card editor renames the selected card. A
    text field owns a `TextInput` but a card stores a `String`, so
    `UiState::sync_card_rename` (host, per frame) two-ways them: adopt the card's
    label when the selection moves, else commit what was typed. Verified: typing
    renamed the card live in the field, the filmstrip, and the board caption.
  - **Gotcha worth remembering:** the field first rendered as bare text and
    would not focus. A Cambium text field renders its buffer as element
    *content*, so the inner `input` needs its own box (`display:block` + padding,
    as `.search-wrap input` has) or it has no hit area to click into. Styling the
    wrapper alone is not enough.
  - Committed `427ee0a`.
- **2026-07-16**: **Step numbers landed and verified; octave shift built but
  unreachable.**
  - **Step numbers** — while drawing, a marker on the path shows its step
    index instead of its note name, so the order reads without playing it.
    Revisits join ("1,5"). Undrawn markers keep their names, so you still see
    what's available. Verified: drew A→B→C→C#→A and the A read "1,5".
  - **`effective_run_path` now resolves each step's pitch from the neck**
    (`Fretboard::pitch_at`) instead of looking it up among the current dots,
    which fell back to `0.0` — a step that silently played nothing. A drawn note
    is a real position whatever the lens shows.
  - **Octave shift (`shift_path(±12)`)** — moves the shape while keeping every
    note's pitch class, so it stays on the material's tones and its degrees hold:
    the octave spider's generator. It refuses rather than clamping when the shape
    would leave the neck, and the controls are gated on `can_shift_path` so there
    is never a button that silently does nothing.
  - **Honest gap:** `StageState::fret_count` is hardcoded to `12` and *nothing in
    the UI changes it*, so an octave shift only fits a path sitting entirely at
    fret 0 — the 8ve controls essentially never appear. The feature is correct and
    unit-tested (`octave_shift_moves_the_shape_and_keeps_its_notes`) but is
    groundwork, not a shipped capability, until the neck is configurable. A
    neck-length setting was already design intent (per-instrument neck length in
    the fretboard-settings plan); that, or the scale-step shift above, is what
    makes shifting real.
