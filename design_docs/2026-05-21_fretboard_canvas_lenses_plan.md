# Fretboard-as-canvas, tabs as lenses

Reorient Woodshed from a *toolbox* (nine independent tabs) toward an
*instrument*: one persistent fretboard surface that different **lenses**
reconfigure. The Navigator principle (single surface, configurable
scope/form factor — not many instances) applied to the practice app.

Status: **design proposal — needs sign-off before building.**

## The problem it solves

Today Scales / Chords / Progressions / Exercises each build their *own*
fretboard + their own controls in a `split`, with their own root pitch
(`scale_root_pc`, `chord_root_pc`, `progression_key_pc` — all separate).
So they're four little apps that happen to draw the same instrument. Switching
tabs tears down one fretboard and builds another; nothing carries over.

## The model

**One canvas, many lenses.** A single persistent fretboard surface; a *lens*
decides what it highlights and what controls sit beside it.

```text
FretLens = Scale | Chord | Progression | Exercise

Each lens, given the shared MusicalContext, yields:
  - highlights: Vec<Position> + labels + per-position colors
  - a controls/info view (the right pane)
```

**Shared `MusicalContext`** — the spine that makes lenses cohere:

```text
MusicalContext {
    root: ChromaticPc,     // one "current key", replaces the 3 per-tab roots
    instrument + tuning,   // already global (state.fretboard)
}
```

Switch from the Scale lens (A Dorian) to the Chord lens and you see a chord
*in A* on the *same* fretboard — because root carried over. That's the
"this is one instrument" feeling.

**Layout** stays the proven `split`: `split(fretboard_canvas(ctx, lens),
lens_controls(state))`. The fretboard widget persists across lens switches
(same view position in the tree → not torn down; only its highlight data +
the right-pane controls change). The current tab strip's four theory entries
collapse into one **Fretboard** surface with a **lens switcher** (segmented
control / sub-tabs) where the per-tab pickers used to be.

## Refinement (Mark, 2026-05-21): the left pane is a composable *instrument surface*

The canvas isn't "the fretboard" — it's the **instrument surface**, and the
fretboard is just one widget that can live there. Tuner and Metronome are
*also* instrument-surface widgets, not separate tabs or a top utility bar.

So the left pane becomes configurable: **choose which instrument widget(s) to
show** — fretboard, tuner meter, metronome — and (the richer form) show
several at once as **resizable vertical sub-panes that share the one right
border** (the main split bar). This unifies the old direction-#3
(tuner/metronome utilities) *into* the canvas concept instead of bolting it on.

Two forms to decide between:

- **A. Selector** — the left pane shows exactly one of {Fretboard, Tuner,
  Metronome} at a time; a compact switcher picks which. Simplest; one
  instrument-view at a time.
- **B. Composable stack** — each widget can be toggled on; multiple show
  stacked vertically, each independently resizable (nested vertical splits),
  all sharing the main split's right edge. Most powerful (tune *while* seeing
  the fretboard, keep the click visible) and the fullest Navigator expression;
  more layout machinery (nested splits + per-widget show/hide + persisted
  sizes).

Either way, **right pane = the active lens's controls/content** (scale/chord/
progression/exercise pickers; tuner readout text; metronome controls). The
fretboard lenses (Scale/Chord/Progression/Exercise) sub-select *within* the
fretboard widget when it's the active/visible instrument.

### The fretboard's fret-span is itself a scope dial (Mark, 2026-05-21)

The fretboard widget should be **shortenable down to the four-fret chord-card
form, or expandable up to the full 12 frets** — a continuous fret-window
scope. This *converges our two existing widgets*: `chord_diagram_view` (a
compact 4-fret window with open/muted markers) and `fretboard_view` (the
12-fret vertical neck) are just the two ends of one configurable fretboard.

Implication: fold them into a single widget taking a **fret window**
(`start_fret`, `fret_count` ∈ ~4..=12). A tight window (4 frets) renders the
chord-card form; a wide one (0–12) renders the full neck. So the fretboard has
two independent scope dials — *fret span* (how much neck) and *lens* (what's
highlighted) — plus its pane size. Pure Navigator: one surface, configurable
scope + form factor.

(Progressions already crops 4-fret voicing windows via `chord_diagram_view`;
that becomes "the fretboard widget at fret-span 4" rather than a separate
widget.)

## What changes, what stays

- **Become lenses (collapse into the Fretboard surface):** Scales, Chords,
  Progressions, Exercises.
- **Stay distinct surfaces:** Practice, Song, Settings.
- **Become always-on utilities (companion direction, optional):** Tuner +
  Metronome — used *during* any activity, not destinations. Could move to a
  persistent utility bar/popover, freeing the tab strip. (Sequenced after the
  lens work; flagged here so the nav reorg is coherent.)

Resulting top-level nav: **Fretboard** (Scale/Chord/Progression/Exercise
lenses) · **Practice** · **Song** · **Settings**, with **Tune/Click** as
utilities. Four destinations + utilities, versus nine flat tabs.

## Migration — phased, never breaking the working app

- **Phase 1 — Shared `MusicalContext` (root). ✅ Done 2026-05-21.** The three
  per-tab roots (`scale_root_pc` / `chord_root_pc` / `progression_key_pc`)
  collapsed into one `AppState.root` (+ `Settings.root`); Scales / Chords /
  Progressions all read + write it, so setting the key in one is reflected in
  the others. Old saves migrate via `#[serde(alias = "scale_root_pc")]`
  (scale root carries over; the other two old fields are ignored). Tabs
  otherwise unchanged this phase. (Tuner/Exercises don't use a root picker.)
- **Phase 2 — Fretboard surface + lens switcher.** Introduce a `FretLens` enum
  and a single `fretboard_view(state)` that renders the active lens's
  highlights + swaps the controls pane. Reuse each tab's existing
  position-computation + controls (move them into per-lens functions). Replace
  the four theory tab entries with one **Fretboard** tab + lens switcher.
  *Done when:* all four lenses work on the one persistent surface, root + split
  carry across lens switches, parity with today's per-tab behavior.
- **Phase 3 — Composable instrument surface.** Tuner + Metronome become
  *modules* mounted into a resizable vertical stack on the left surface,
  coordinating with the fretboard (tuner pauses others; metronome clocks
  exercises/progressions). Form **B** chosen; spun out into its own plan:
  [2026-05-21_composable_instrument_surface_plan.md](2026-05-21_composable_instrument_surface_plan.md).
  Carries through to the 1.0 definition (custom-authored exercises /
  progressions / tunings).

## Progress

- 2026-05-21: **Fret-span dial shipped** (the foundational scope dial).
  `fretboard_view` now takes a `fret_window: (start_fret, fret_count)`;
  `draw_fretboard` windows the neck (nut only when the window starts at 0,
  a "Nfr" position label otherwise, inlays/positions mapped to the window).
  Shared persisted `AppState.fret_span` (4..=12, default 12); a "Frets −/+"
  stepper in the header shows on fretboard tabs only. Callers pass
  `(0, fret_span)` (from-nut).
- 2026-05-21: **Chord diagram folded in.** `fretboard_view` gained a
  `marks: Vec<StringMark>` param (open ring / muted ✕ above the nut).
  Progressions' chord cards now call `fretboard_view` with an anchored 4-fret
  window (`start_fret = lowest_fretted − 1`), fretted-only positions in the
  chord hue (root pops), open/muted marks, and the "Nfr" label.
  `chord_diagram_view` + `draw_chord_diagram` (~190 lines) **deleted** — one
  configurable fretboard now spans chord-card → full neck.

- 2026-05-21: **Lens-surface polish.** Inactive lens buttons now use body
  `text` (legible, not disabled-dim); the `Lens:` row gained a left inset;
  the `◀/▶` cycle arrows (`button_sm`) got tight `from_vh(SP_1, SP_2)` padding
  so single glyphs read compact; the Scale lens info pane fills its dead space
  with a **Degrees** list (one row per scale degree, root in `tertiary`).
  Layout fix: the content portal was scrolling horizontally with an unbounded
  child, so the `flex(1.0)` split collapsed to content width and the right
  card's backdrop stopped short. Added `.constrain_horizontal(true)` to the
  portal + switched the inner column to `CrossAxisAlignment::Stretch`, so the
  split fills the viewport and both panes reach the window bounds.

- 2026-05-21: **Phase 2 — lens surface (safe variant).** The four theory tabs
  collapse into one **Fretboard** entry in the tab strip plus a **lens
  switcher** row (Scale / Chord / Progression / Exercise). Kept as the *safe*
  shape: the four `Tab` variants stay (they're the lens identities), so no
  enum removal, no `tab` serde migration, no per-view sidebar rewrite. The
  "Fretboard" tab button returns to `last_lens`; the lens buttons set
  `tab` + `last_lens`. Sidebar/hamburger/fret-span keep keying off the
  underlying lens tab. (Lens switch still rebuilds the view — true
  fretboard-widget persistence across lenses is a later optimization if
  flicker shows.)

## Open questions

1. **Lens switcher form.** Segmented control above the fretboard? A second
   row in the tab strip? A dropdown? (Leaning: a compact segmented control in
   the controls pane header — keeps it near the lens's own controls.)
2. **Root vs. key.** Some lenses think in "root" (scale/chord), Progressions
   in "key." Same `ChromaticPc` underneath; just label per lens.
3. **Exercise lens fit.** Exercises step through positions over time (a
   transport). It's still fretboard highlights, so it's a valid lens, but its
   controls differ most. Confirm it belongs in the canvas vs. staying closer
   to Practice.
4. **Per-lens state retention.** Keep each lens's last selection (which scale,
   which chord) when switching away and back — yes; store per-lens, only
   `root` is shared.

## Why this is the right ambitious move

It's the one reorg that changes Woodshed's *identity* rather than adding
another feature: the app stops being a menu of tools and becomes "your
fretboard, viewed different ways," with a shared musical moment running
through it. Everything else (theming, the DLR, practice) layers onto a
coherent instrument instead of decorating a toolbox.
