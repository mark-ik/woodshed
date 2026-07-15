# Fretboard Marker Detail Cards Plan

Make the painted fretboard interactive: hover a marker for an ephemeral detail
card, click to pin it. Pins are multi (compare several notes at once). The card
shows the note's essentials and plays it. This turns the board from a static
diagram into something you interrogate, without leaving the Stage.

Builds on the paint-leaf board and its per-marker label overlay (see
[2026-07-14_instruments_and_fretboard_rendering_plan.md](2026-07-14_instruments_and_fretboard_rendering_plan.md)).

## Decisions (locked 2026-07-15)

- **Multi-pin.** Clicking pins a card; clicking more markers pins more, to
  compare. Click a pinned marker again to unpin; a clear-all affordance resets.
- **Essentials + play in v1.** Note name, scale degree, interval, octave,
  string/fret, plus a button that plays the note. Chord membership and
  other-position lookups are deferred.
- **Design-doc first** (this doc), then build.

## Findings (genet capabilities, verified)

- **No hover events.** `PointerPhase` is `Down`/`Move`/`Up`, and `Move` fires
  only during a press (a captured drag). True mouse-over-without-press hover is
  not an event we can handle. But CSS `:hover` works (the Settings items use it).
- **Interaction lives in the view layer, not the leaf.** `Leaf::event` is a
  placeholder, and Cambium routes semantic interaction through the view layer.
  The board is one paint leaf with no per-marker child elements.
- **The per-marker overlay already exists.** Note labels ride as
  absolutely-positioned view elements over the leaf's markers (2026-07-15). That
  overlay is the interaction surface; no new hit-testing is needed.
- **Events available:** `clickable` and `on_pointer` (Down/Move/Up).
- **Audio preview path exists.** `StageState::preview_requested` (a one-shot
  flag) plus `preview_voicing() -> (pitches, ...)`; the host consumes the flag
  and voices through the audio backend. A single-note play button extends this
  same pattern.

## Design

### Interaction surface

Each marker's overlay element is the hover and click target. Nothing routes
through the leaf. The overlay is also the seam for later board interactivity
(click-to-hear, click-to-stage), so this is foundational, not one-off.

### Hover peek (ephemeral), via CSS `:hover`

No hover events fire, so the peek card is pre-rendered hidden inside each marker
element and revealed by CSS `:hover`. Content is static per marker (recomputed
only when the board changes), so pre-rendering every marker's card is fine. It
disappears on mouse-out. This is the "ephemeral" half.

### Click pin (persistent), multi-pin

Clicking a marker toggles it in a pinned set, `pinned: Vec<MarkerRef>` keyed by
`(string_index, fret)`. Every pinned marker renders its card persistently, so
several stand at once for comparison. Dismiss by clicking the marker again or a
small close control on the card; a clear-all control empties the set. This is the
event-driven half.

### Card content: `NoteDetail` + play

Computed per marker from the theory model: note name with correct enharmonic
spelling, **scale degree** relative to root (e.g. `b3`), interval name, octave,
string + fret, and MIDI/frequency. Plus a **play** button.

### Play the note

A one-shot single-note request mirroring `preview_requested` (for example
`preview_note_requested: Option<Pitch>`). The play button sets it; the host
consumes it and voices that single pitch through the audio backend, reusing the
`preview_voicing` path. No new audio engine work, just a narrower request.

### Positioning

An edge-aware popover near the marker: above by default, flipping below or to the
side at the board's edges. Builds on the existing
[popup proposal](2026-05-18_xilem_popup_view_proposal.md).

### Design language

A quiet ephemeral surface: hairline border, subtle fill, small, in the board's
restrained language. A detail gloss, not a dialog.

### Reusable component

"Hover or click a thing, get an ephemeral detail card" is general. Build it as a
reusable **detail-popover** (hover-peek + click-pin + edge-aware placement),
parameterised by its content, so it also serves catalog items and set cards. This
fits the component-catalog direction rather than a fretboard one-off.

## Plan

### Phase 1 — Click-to-pin essentials

The per-marker overlay becomes clickable; `pinned: Vec<MarkerRef>`; each pinned
marker renders a card with the essentials (name, degree, interval, octave,
string/fret). Multi-pin, toggle to dismiss, clear-all.

- **Done when**: clicking markers pins and unpins their cards; several pin at
  once; each shows the correct essentials.

### Phase 2 — Hover peek

CSS `:hover` reveals a pre-rendered card on each marker, independent of pins.

- **Done when**: hovering a marker shows its card and it hides on mouse-out.

### Phase 3 — Play the note

Single-note one-shot request plus host consume; a play button in the card.

- **Done when**: the play button voices the marker's pitch through the backend.

### Phase 4 — Extract the reusable detail-popover

Generalise the card plus placement into a reusable component; the fretboard card
becomes one consumer.

- **Done when**: a second surface can reuse the same popover.

## Open decisions

- Positioning specifics: flip thresholds; whether a pinned card tracks the board
  on scroll/resize.
- Multi-pin crowding: cap the pin count, or add a pins tray if many.

## Progress

- **2026-07-15**: Drafted. Decisions locked (multi-pin, essentials + play,
  design-doc first). Grounded in verified genet capabilities (no hover events so
  CSS `:hover` for peek; interaction in the view layer over the existing
  per-marker overlay; the audio preview path already exists).
- **2026-07-15**: **Phase 1 (click-to-pin essentials) landed.** `FretDot` now
  surfaces `octave`, `degree`, and `interval_name` (via `degree_label` /
  `interval_name` on the semitone distance from root); `UiState` holds
  `pinned_markers: Vec<(usize, u8)>` with `toggle_pin` / `is_pinned`. The label
  overlay is now `clickable` (toggles the pin, pinned markers get a ring), and a
  quiet `.note-card` renders per pin, flipping above/below to stay on the board.
  Verified in `testing/woodshed/woodshed-pins.png`: pinning gave A2 · 1 · Root ·
  string 6 · fret 5 and A4 · 1 · Root · string 2 · fret 10, both correct.
  Placement is basic (cards overlap the board a little); edge-aware refinement is
  a follow-on.
- **2026-07-15**: **Phase 3 (play) + close-all landed.** `FretDot` carries
  `frequency`; the card has a "♪ Play" button that sets
  `UiState.preview_note_requested`, which the host consumes via the backend's
  existing `preview_note(freq, secs)`. A "Close N cards" control in the board
  caption (shown only when pins exist) calls `clear_pins`. Verified in
  `testing/woodshed/woodshed-cards-play.png` and `woodshed-cleared.png`: play
  fires without crashing, close-all empties the pins. Remaining on this feature:
  Phase 2 (hover peek) on the real hover events the cambium agent shipped.
