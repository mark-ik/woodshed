# Composable instrument surface (Phase 3 → 1.0)

The full expression of the Navigator principle for Woodshed: the left pane
stops being "the fretboard" and becomes a **composable instrument surface** —
a stack of resizable instrument *modules* (fretboard, tuner, metronome, …) that
are **aware of one another** and combine to reproduce — in one configurable UI —
the modes that used to be separate tabs.

This is the doc for **Phase 3** of the lenses plan
([2026-05-21_fretboard_canvas_lenses_plan.md](2026-05-21_fretboard_canvas_lenses_plan.md))
and the road to **1.0**. Form **B (composable stack)** chosen by Mark
(2026-05-21), explicitly over the simpler selector form A.

Status: **design proposal — needs sign-off before building 3b+.**

## The vision (Mark, 2026-05-21)

> B, definitely. It would be excellent if the other widgets were aware and
> paused when the tuner is on, the fretboard widget can coordinate with the
> metronome widget for exercises, or expand to the side and down to host
> multiple chord 4×4 card progressions when in progression mode. Showing people
> multiple at once and then combining them to yield the modes that were once
> tabs in one configurable UI… then adding more exercises, progressions, and
> custom exercises, progressions, and tuners… and I think that will be worth a
> 1.0.

Unpacked into concrete behaviors:

1. **Stack, resizable.** Multiple modules show at once, stacked vertically,
   each independently resizable, all sharing the main split's right edge.
2. **Tuner claims focus.** When the tuner is armed, the other modules pause
   (no click while you tune; the surface knows tuning is the current intent).
3. **Fretboard ↔ metronome for exercises.** A mounted, running metronome is
   the clock; the Exercise lens advances positions on its beats instead of
   running a private timer.
4. **Fretboard grid for progressions.** In Progression mode the fretboard
   region expands (right and down) into a tiled grid of 4×4 chord cards.
5. **Tabs become compositions.** Exercises = fretboard(Exercise lens) +
   metronome. Progressions = fretboard(Progression lens, grid). The old tabs
   are recovered as *configurations* of one surface, not separate screens.
6. **Then: depth.** More built-in exercises / progressions / tunings, plus
   **user-authored** custom exercises, progressions, and tunings. That breadth
   + the composable surface = 1.0.

## Architecture

### The surface is a list of modules

```text
SurfaceModule {
    kind: ModuleKind,   // Fretboard | Tuner | Metronome  (extensible)
    visible: bool,      // toggled on/off by the user
    weight: f64,        // relative vertical size in the stack (persisted)
}

AppState.surface: Vec<SurfaceModule>   // ordered top→bottom; persisted
```

The **Fretboard** module is special: it always exists, can't be removed, and
carries the lens sub-mode (Scale/Chord/Progression/Exercise) already in
`AppState.tab`/`last_lens`. Tuner and Metronome are optional modules the user
mounts. (Future kinds — chord-trainer, ear-trainer — slot into the same list.)

### Layout: nested vertical splits sharing one right edge

The main horizontal split stays `split(surface, lens_controls)`. The **surface**
(left child) is now a vertical stack of the visible modules, rendered as nested
binary `split`s:

```text
split(                              // main, horizontal
  v_split(Fretboard,                // surface = nested vertical splits
    v_split(Tuner, Metronome)),     //   each divider persists its point
  lens_controls)                    // right pane (see "Controls ownership")
```

Each vertical divider gets its own `on_split_changed` persisting into the
module `weight`s. N visible modules → N−1 vertical dividers. (Xilem `split` is
binary; a right-leaning nest gives the stack. If divider ergonomics get awkward
past ~3 modules, revisit with a custom multi-pane Flex-with-handles widget —
deferred until the binary nest actually hurts.)

### Coordination: shared state + a reconcile arbiter + a clock

The "awareness" needs no event bus. Three mechanisms, all on `AppState`:

1. **Shared session state** (exists today): `root`, `fretboard`/tuning, tempo,
   `tuner_active`, `metronome_playing`. Every module's view fn reads it; Xilem
   rebuild propagates. Setting the key in one lens is seen by all.
2. **Resource arbiter** (the two-phase pattern already used elsewhere): a pure
   state update, then one reconcile pass that arbitrates the scarce audio
   resources —
   - **input claim**: tuner armed → enable the pitch analyzer *and* pause the
     metronome/song output (`metronome_playing = false`, engine paused);
     disarming restores prior transport.
   - **output claim**: metronome/song own the output engine; only one transport
     drives it at a time.
   The arbiter is one function (`reconcile_session_claims`) reading the desired
   flags and issuing engine calls — analogous to graphshell's
   `reconcile_webview_lifecycle`.
3. **Clock subscription**: when a Metronome module is mounted *and* running, it
   is the session clock. The Exercise lens reads the beat/bar count to advance;
   the Progression lens advances chord-per-bar (reusing the Song-timeline
   count-in + per-measure re-strike). When no metronome is mounted, the
   Exercise lens falls back to its own `exercise_bpm` timer (today's behavior),
   so the fretboard module is still useful standalone.

This keeps modules decoupled: they read shared state, never call each other.

### Controls ownership (the right pane)

**Recommended default:** the main split's right pane stays the **Fretboard
lens's** controls/info (the rich pickers + Degrees panel — the primary surface
earns the real estate). **Tuner and Metronome panes are self-contained**: their
readout + compact controls live *inline within their own pane* in the left
stack. A tuner meter and metronome beat-dots read fine in a narrow column, and
this avoids a "which module owns the right pane?" focus-tracking problem.

*Alternative (flagged):* a focused-module model where clicking a module routes
its controls to the right pane. More flexible, more state (focus tracking) and
more surprising (right pane changes as you click around). Hold unless the
inline form proves cramped.

### Module mounting UI

A compact module toggler in the surface header (or the existing hamburger area):
chips/toggles for Tuner and Metronome that add/remove their module from
`surface`. Fretboard has no toggle (always on). Persist visibility + weights so
the user's chosen composition restores on launch.

## What folds in / what stays

- **Into the surface as modules:** Tuner, Metronome. Their **top-strip tabs are
  removed** — top nav becomes **Fretboard · Practice · Song · Settings**.
- **Stay distinct surfaces:** Practice, Song, Settings. (Practice/Song have
  their own transport-heavy layouts; they can *consume* the same engines but
  aren't part of the instrument-surface stack — revisit post-1.0 if it's
  natural to mount them too.)

## Phased build (never break the working app)

- **3a — Module model, no behavior change.** Add `SurfaceModule`/`ModuleKind` +
  `AppState.surface` (+ persisted `Settings`). Render the surface *from the
  list*, but seed it with only Fretboard mounted → pixel-identical to today.
  *Done when:* the fretboard tabs still work, surface renders from the list,
  weights round-trip through settings.
- **3b — Mount Tuner + Metronome.** Extract today's `tuner_view`/`metronome_view`
  bodies into self-contained module renderers; add the mount toggles; render the
  visible modules as the nested vertical-split stack with persisted per-divider
  weights. Remove Tuner/Metronome from the top strip.
  *Done when:* user can show fretboard + tuner + metronome together, resize each,
  and the composition persists across restart; old per-tab behavior intact.
- **3c — Resource arbiter.** Add `reconcile_session_claims`: tuner armed pauses
  metronome/song output and claims input; disarm restores. *Done when:* arming
  the tuner visibly pauses a running metronome and re-arming/disarming is
  lossless.
- **3d — Shared clock.** Metronome-as-clock; Exercise lens advances on beats
  when metronome mounted+running (fallback timer otherwise); Progression
  advances chord-per-bar on the clock. *Done when:* an exercise steps in time
  with a visible running metronome, and a progression walks bar-by-bar.
- **3e — Fretboard grid.** Progression lens renders a wrapped grid of 4×4 cards
  that grows down/right; surface width coordinates with the main split. *Done
  when:* a multi-chord progression shows all cards tiled, resizable, on the one
  surface.
- **Phase 4 — Depth + custom authoring (the 1.0 push).** Expand built-in
  exercise / progression / scale / chord / tuning catalogs; add **user-authored**
  custom exercises, progressions, and tunings (CRUD + persistence, mirroring the
  `UserThemeDef` pattern). *Done when:* a user can build, save, name, edit, and
  delete their own exercise / progression / tuning and use it like a built-in.

### 1.0 done conditions

1. Composable surface: fretboard + tuner + metronome mountable, stackable,
   resizable, persisted (3a–3b).
2. Coordination live: tuner pauses others; metronome clocks exercises and
   progressions (3c–3d).
3. Progression grid on the surface (3e).
4. Custom authoring: user exercises / progressions / tunings (Phase 4).
5. No regressions to Practice / Song / Settings.

- 2026-05-21: **3b widget-not-page pass.** Per Mark, surface modules are
  widgets that must *fit* their pane, never scroll. Dropped `scroll_tab` from
  the stack modules; built compact `tuner_module` (title·note·transport line +
  cents needle + level bar + string row + detector cycle, keeps its polling
  fork; full catalog/threshold/help stay on the Tuner tab) and
  `metronome_module` (BPM line + slider + ± row + folded settings row). The
  fretboard now scales as a proportional unit (cells stay square-ish, fits the
  pane both axes, centered; labels fixed-size). Chord-card diagram enlarged
  120×150 → 150×180 with button chrome stripped (less pillowy); surface stack
  `min_lengths` raised to 190/170 as a soft fretboard floor (≈ chord-card size)
  — a hard per-cell floor would break the shared chord-card minis, so the floor
  is enforced via pane min-height. Tuner meters narrowed 240→200 to curb
  horizontal clipping.
- 2026-05-21: **Fret-window slide + widget polish.** Fretboard model bumped
  12 → 24 frets (`FRETBOARD_MODEL_FRETS`); new persisted `fret_start` (clamped
  so `start + span ≤ 24`, re-clamped when span changes). New `fretboard_widget`
  wrapper = a `thin_card` (tighter padding) + a start-fret strip (`▼`/`▲` via
  `nudge_fret_start`, `from nut` / `fret N+` caption); the neck flexes to fill.
  All four lens necks use `(fret_start, fret_span)`; chord-card minis keep their
  chord-anchored windows. Chord-card background restored (had been wrongly
  stripped) with tighter padding + larger 150×180 diagram.
- 2026-05-21: **Orientation decision (Mark): keep vertical, revisit later.**
  Fretboard + chord cards stay vertical for now; a horizontal-neck option /
  app-wide orientation convention is deferred (meaningful layout work) until
  after the composable surface + coordination (3c/3d) land.

- 2026-05-21: **3c + 3d (exercise) shipped.** 3c resource arbiter: arming the
  tuner (`start_tuner`) pauses a running metronome and remembers it
  (`tuner_paused_metronome`); `stop_tuner` restores it — the first cross-widget
  arbitration. (Song mode, a separate destination, not yet wired — follow-on.)
  3d exercise: the Exercise lens cursor now follows `metronome_beat` when the
  metronome runs (phase-locked), own timer gated off then — matching the
  arpeggio. Shared clock now drives both transports.

## Open questions

1. ~~**Controls ownership**~~ — **Resolved (Mark, 2026-05-21): inline per
   module.** Tuner/Metronome panes are self-contained; the main right pane stays
   the Fretboard lens's controls.
2. **Multi-pane widget** — keep the binary-split nest, or build a custom
   N-pane vertical stack with handles once >3 modules are common? *Defer until
   the nest hurts.*
3. **Clock authority when both metronome and Song are running** — Song owns a
   bar timeline too; do they share one transport, or is the surface metronome
   distinct from Song's? *Likely: one session transport; Song mode borrows it.*
4. **Do Practice/Song eventually mount as surface modules** too, or stay as
   distinct destinations? *Post-1.0.*

## Findings

(Populated during execution.)

## Progress

- 2026-05-21: Plan created. Form B chosen. Coordination model settled as
  shared-state + reconcile-arbiter + clock-subscription (no event bus);
  grounded in existing `AppState` engine/handle fields.
- 2026-05-21: **3a shipped (behavior-neutral).** `ModuleKind` (Fretboard /
  Tuner / Metronome) + `SurfaceModule { kind, visible, weight }` added;
  `AppState.surface: Vec<SurfaceModule>` seeded `[Fretboard]`; round-trips
  through `Settings.surface` (`#[serde(default)]`, additive — old saves load
  fretboard-only). `sanitize_surface` enforces the "exactly one Fretboard"
  invariant + resets bad weights on load. No rendering change yet; verified
  builds + loads against a pre-existing config. Controls ownership resolved:
  inline per module.
- 2026-05-21: **3b-1 shipped (stack + mounting; tabs kept as safety net).**
  `surface_left(state, fretboard_card)` folds the visible modules into
  right-leaning nested **vertical** `split`s (each divider persists into module
  `weight` via `AppState::set_module_split`; weight↔fraction with the lower
  modules held fixed). The four fretboard lens views now route their left side
  through it (Practice keeps its own fretboard — distinct destination). Mount
  toggles (`● Tuner` / `○ Metronome`) added to the lens bar via
  `toggle_module` / `module_shown`. Companion modules reuse the existing
  `tuner_view` / `metronome_view` (already self-contained cards; tuner carries
  its polling `fork`) — zero new render code, lowest risk. **Tuner/Metronome
  top-strip tabs deliberately kept this step** so the module forms can be
  validated against the tab forms before removal. Notes for 3b-2: compact
  module forms (tuner's 220px tunings sidebar is awkward stacked; needs an
  inline tuning picker to avoid losing tuning selection when the tab goes),
  then remove the tabs + redirect `tab == Tuner/Metronome` on load.
- 2026-05-21: **3b-1 layout-bounding fix.** First mount revealed the surface
  was unbounded (it sat in the page's vertical-scroll portal, so module panes
  took natural height and overflowed; the right pane went empty when scrolled).
  Fix: `app_logic` no longer wraps everything in one scrolling portal — header
  + tab bar + lens bar are fixed rows and `tab_content` takes the remaining
  window height (`flex(1.0)`). That bounded height is the surface's viewport.
  Each surface module + the lens controls pane scroll internally via a
  `scroll_tab` helper (`portal(view).constrain_horizontal(true)`); the
  non-fretboard destinations (Tuner/Metronome/Practice/Song/Settings) each wrap
  in `scroll_tab` too (they lost the page scrollbar). The four lens views
  switched to cross-axis `Stretch` so the split fills the bounded height.
  Tradeoff: `scroll_tab` is generic, so it can't carry `AutoHideScrollBar`
  (needs a `Sized` widget that opaque `impl WidgetView` returns don't expose) —
  scrollbars show on overflow rather than auto-hiding; restore per-pane later.
