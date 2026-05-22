# Arpeggio lens

Add **arpeggios** to the Fretboard surface as a fifth lens (Scale · Chord ·
Progression · Exercise · **Arpeggio**). An arpeggio is a chord's tones played
one note at a time, ascending then descending — so it reuses the existing chord
catalog for its note content and the Exercise lens's step-through transport for
the up/down motion, rendered through the shared fretboard surface.

Status: **design — building incrementally (visual-first).**

## Decisions (Mark, 2026-05-21)

- **Position/shape cards** (CAGED-style): the lens generates the arpeggio as a
  set of **neck-position shapes**; each renders as a card (like the Progression
  chord cards). Clicking a card loads that shape into the main neck + transport.
  *(Chosen over a flat quality-catalog-only form.)*
- **Visual stepping now**, no audio: the transport highlights notes ascending
  then descending in time (BPM-driven), exactly like the Exercise lens today.
  Audio is a later pass, wired into the metronome clock (Phase 3d of the
  composable-surface plan) + a pitched voice.

## Model

No new theory primitive: an arpeggio's notes **are** a `ChordFormula`'s tones.
- Quality comes from the **chord catalog** (`chord_catalog()` — 45+ qualities:
  maj7, min7, dom7, dim7, …); root is the **shared** `AppState.root`.
- Tones across the neck: `Fretboard::positions_for_chord(chord, root)`
  (woodshedding/src/fretboard.rs:180) — already returns every chord-tone
  position with `interval_from_root` (root pops in the root-dot color).

### Position-shape generation (the new bit)

A **shape** is the arpeggio's tones inside a neck window (~4–5 frets) that sits
under one hand position — the CAGED idea. First-pass algorithm (app-side, like
`enumerate_voicings`):

```text
ArpeggioShape { start_fret: u8, positions: Vec<Position> }

generate_arpeggio_shapes(fretboard, chord, root, span≈4) -> Vec<ArpeggioShape>:
  - all = positions_for_chord(chord, root)   // every chord tone on the neck
  - anchor windows up the neck (e.g. start frets where the lowest-string root
    falls, plus a couple between) — or slide [0..span], [3..3+span], … to ~fret 15
  - for each window: positions in [start, start+span], one box
  - drop windows that don't cover all chord tones / too few notes
  - dedup overlapping windows; sort by start_fret
```

Refinements deferred: true CAGED anchoring (E/A/D/G/C shapes), one-note-per-
string filtering, two-octave spans. v1 = windowed boxes, good enough to read +
step. (Long-term this generator likely graduates into woodshedding alongside
`positions_for_chord` / voicings.)

### Up/down transport

The step sequence for the active shape = its positions **sorted by pitch**
ascending, then descending (without repeating the top/bottom note), looping.
Reuse the Exercise lens's `task_raw` + tokio-interval timer (main.rs ~4269):
a per-arpeggio `step_idx` advances at `bpm`; the current note highlights, with
the same trailing-fade treatment as exercises. `direction` ∈ {Up, Down, UpDown}.

## State (`AppState` + `Settings`)

- `arpeggio_idx: usize` — index into the chord catalog (the quality). Persisted.
- `arpeggio_position_idx: usize` — which generated shape is active. (Transient
  or persisted; lean persisted.)
- `arpeggio_step_idx: usize` — transport cursor (transient).
- `arpeggio_playing: bool` — transient.
- `arpeggio_bpm: f32` — persisted.
- `arpeggio_direction` — Up / Down / UpDown (persisted).
- `SidebarVisibility.arpeggios` for the quality-catalog sidebar.

## Layout (`arpeggios_view`)

Mirrors `progressions_view` (sidebar + neck + cards):
- **Sidebar:** arpeggio-quality catalog (the chord catalog), click to select;
  ● on the active quality. Collapsible via the header hamburger.
- **Surface (left split child):** the active shape on the fretboard
  (`fretboard_widget`), the transport stepping through it up/down. Goes through
  `surface_left` like the other lenses, so tuner/metronome can stack with it.
- **Right pane:** the **position-shape cards** (the generated shapes) + a
  compact transport bar (▶/■, BPM, direction). Clicking a card sets
  `arpeggio_position_idx`.

## Enabler: lift the `OneOf9` cap

`tab_content` returns `OneOf9` (xilem's max; no `OneOf10`). A fifth fretboard
lens is a 10th `Tab`. Fix by **boxing** the dispatch: `tab_content` returns
`Box<AnyWidgetView<AppState>>` (each arm `.boxed()`), removing the arity ceiling
permanently. Negligible per-frame cost at the top-level tab boundary.

## Phased build (never break the working app)

- **A1 — Enabler + scaffold.** Box `tab_content`; add `Tab::Arpeggios` + lens
  button + `tab_has_fretboard` + sidebar flag + settings fields. Minimal
  `arpeggios_view`: quality catalog sidebar + the arpeggio's full-neck tones
  on the surface (no shapes/transport yet). *Done when:* the Arpeggio lens
  shows + selects a quality, root carries over, nothing else regresses.
- **A2 — Position-shape cards.** `generate_arpeggio_shapes` + render shape cards
  in the right pane; click selects the active shape; the surface neck shows the
  active shape (windowed) instead of the full neck.
- **A3 — Up/down transport.** Step sequence (pitch-sorted, up/down) + the
  exercise-style timer + trailing-fade highlight on the surface neck; transport
  bar (▶/■, BPM, direction). *Done when:* a shape arpeggiates up and down in
  time, visually.
- **A4 (later) — Audio + CAGED refinement.** Pitched voice on the clock;
  true CAGED anchoring / one-per-string shapes; possibly graduate the shape
  generator into woodshedding.

- 2026-05-22: **A4 (audio) shipped.** New `SongEngineHandle::play_note_now(freq,
  dur)` in `woodshed-audio`: renders a single-pitch block-chord voice and pushes
  it to a new one-shot voice list that the Song callback mixes **even when the
  song isn't playing** (own `oneshot_clock`; cleared when Song playback starts).
  Arpeggio + Exercise transports build per-step frequency lists and run a ~20ms
  audio `task_raw` that fires `play_note_now` on each cursor-step change (covers
  own-timer + metronome-driven). `transport_sound` (persisted, default on) +
  🔊/🔇 toggle per transport. Song engine force-created on first sounded step.

## Findings

(Populated during execution.)

## Progress

- 2026-05-21: Plan created. Arpeggio = chord tones via the chord catalog; 5th
  lens; position/shape cards + visual up/down stepping chosen. `OneOf9` cap to
  be lifted by boxing `tab_content`.
- 2026-05-21: **A1 shipped.** `tab_content` boxed (cap lifted); `Tab::Arpeggios`
  + lens button + `tab_has_fretboard`/`tab_has_list`/sidebar flag; arpeggio
  state (`arpeggio_idx`/`_position_idx`/`_step_idx`/`_playing`/`_bpm`/
  `_direction`/`_label`) + Settings round-trip. `arpeggios_view`: quality
  catalog (chord catalog) + root, chord tones on the windowed neck, Notes panel.
- 2026-05-21: **A3 shipped (on full neck; shapes pending).** Pitch-ordered
  walk over the *visible* chord tones; `▶/■` + `◀ Step`/`Step ▶`/`⏮`; direction
  Up/Down/UpDown(ping-pong); current note pops in `tertiary`; `Note k/n`
  indicator. Label-mode button (notes/intervals/steps/blank — `ArpeggioLabel`).
  Tempo = shared metronome `bpm` (no in-pane tempo); timer is exercise-style
  `task_raw` — **not** yet phase-locked to the click (that's 3d). Also reflowed
  the Metronome widget (BPM on its own line, settings wrapped to two rows) to
  stop it clipping in a narrow pane.
- 2026-05-21: **A2 shipped.** `ArpeggioShape` + `generate_arpeggio_shapes`
  (anchor a ~5-fret window a fret below each root on the low two strings,
  collect chord tones, keep usable boxes; whole-neck fallback). The active
  shape (`arpeggio_position_idx`) now drives the surface neck (windowed to the
  shape, in a plain `thin_card` — no start-fret arrows, the cards control
  position) and the transport run (the shape's notes ordered by pitch). A
  **Positions (N)** section in the right pane lists one mini-neck card per
  shape (`Pos k · fret X`), click loads it + resets the cursor; active caption
  pops. Cards are a vertical list for now (no reflow grid).
- 2026-05-21: **Inversions (start-on-degree).** `arpeggio_inversion` (persisted)
  rotates the run to begin on the chosen chord tone (its lowest occurrence in
  the shape) — `Inv: Root/1st/2nd/3rd…` cycle button. Implemented by rotating
  the pitch-ordered `seq` left to the inversion degree; `Steps` labels + the
  cursor follow the rotated order. (Mark chose start-on-degree over bass-anchored
  shapes; the latter is a possible later add.)
- 2026-05-21: **3d shared clock (arpeggio).** `metronome_started_at` anchors a
  beat grid on `play_metronome`; `AppState::metronome_beat()` gives quarter-note
  beats elapsed. A heartbeat `task_raw` in `app_logic` (active while the
  metronome runs) ticks ~30ms to drive rebuilds. The arpeggio cursor =
  `metronome_beat` when running (phase-locked to the click), else its own
  Play/Step; the own timer is gated off while the metronome drives. Caption
  shows "synced to metronome" vs "run Metronome to sync". **Exercise lens not
  yet migrated to the shared clock** (still its own `exercise_bpm` timer) — easy
  follow-on with the same `metronome_beat`.
