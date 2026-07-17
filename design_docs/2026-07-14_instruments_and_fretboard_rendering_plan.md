# Stringed Instrument Variety and Fretboard Rendering Plan

Two linked expansions, sharing one insight: the theory model is already tuning
general, so most of this is additive data plus a view redesign, not new music
theory.

1. **Instrument variety.** Broaden the catalog well past the current five
   instruments, including the bowed family, without touching the theory math.
2. **Fretboard rendering.** Turn the board from a fixed dot diagram into a
   configurable surface: note *regions* instead of dots, realistic or schematic
   spacing, full-neck or windowed extent, markers, and fill modes. Every choice
   lives in Settings.

The Fretboard is a Tool with contextual forms (see
[2026-07-11_stage_set_tools_plan.md](2026-07-11_stage_set_tools_plan.md)).
Settings owns the durable configuration; contextual controls edit the same
state.

## Findings (verified against current code)

- The theory model is fully tuning general. A `Tuning` is an ordered
  `Vec<Pitch>` plus name, instrument, and category; string count is implicit in
  the list length. `Fretboard { tuning, fret_count }` iterates
  `tuning.strings` with no fixed count
  ([fretboard.rs](../crates/woodshedding/src/fretboard.rs)). No six-string
  assumptions exist in production code; the only `6`s are in test fixtures.
- The board render already adapts to string count:
  `string_count() = tuning().string_count()`
  ([lib.rs:755](../crates/woodshed-core/src/lib.rs)), rows are
  `0..string_count()`, frets are `0..=fret_count`. A 4-string bass draws 4
  strings, a 7-string guitar draws 7.
- `Instrument` is `Guitar | Bass | Ukulele | Banjo | Mandolin | Other`
  ([tuning.rs](../crates/woodshedding/src/tuning.rs)). There is no Violin,
  Viola, or Cello.
- The bowed-string pitches already exist as the mandolin family, because both
  are tuned in fifths: mandolin equals violin (G3 D4 A4 E5), "Mandola (CGDA)"
  equals viola (C3 G3 D4 A4), "Mandocello (CGDA)" equals cello (C2 G2 D3 A3). A
  test even labels the equivalence.
- **No exhaustive `match` on `Instrument` anywhere.** Consumers use `Display`
  and `Instrument::ALL`, so adding variants does not break downstream compiles.
  This is what makes catalog growth safe to do while the host build is blocked.
- `fret_count` is a single global default of 12
  ([lib.rs:478](../crates/woodshed-core/src/lib.rs)), not per instrument. There
  is no scale-length or neck data on `Instrument` or `TuningSpec`.
- A fret window already exists: `fret_window`, `starting_fret`, and `window()`
  drive staged cards and exercise mode. The adjustable span is partly built.
- There is no instrument picker yet. The Tuning dropdown is one flat catalog
  across all instruments; a real picker is deferred in a code comment
  ([lib.rs:262](../crates/woodshed-core/src/lib.rs)).
- Custom-painted fretboard geometry as a Sprigging leaf is the already-intended
  architecture (per the Stage/Set/Tools plan).

## Design decisions

### Spacing is automatic and needs no physical measurements

Relative fret positions are a pure ratio, `x(n) = 1 - 2^(-n/12)` of the neck,
identical for every instrument regardless of real scale length. Realistic
spacing therefore works for every instrument we ever add, with zero
per-instrument data to enter. Physical scale length in millimetres only matters
for showing two different instruments side by side at true relative size, which
is out of scope here.

### Two orthogonal axes, like text alignment

- **Spacing**: Realistic (logarithmic) or Schematic (equal cells). The analogy
  is a subway map, deliberately distorted for legibility, versus a geographic
  map. Schematic is the release valve for dense upper positions.
- **Extent**: Full standard neck (a per-instrument fret count) or Windowed span
  (adjustable start and width, good for small screens).

The four combinations are all valid. A window high on the neck in Realistic mode
shows the true cramped spacing; flipping that same window to Schematic
re-expands it to equal cells. So spacing mode is the on-demand fix for dense
clusters that a fixed diagram cannot give.

### Note regions, not dots

- **Fretted**: fill the fret cell (from wire N-1 to wire N), inset a hair each
  side so adjacent notes on a string stay distinct and the wires stay visible.
  This is the physically real playable zone; the wire does the stopping, so the
  whole space behind it sounds the note.
- **Fretless**: a semitone-wide band centred on the note's node, bounded on each
  side at the quarter-tone point where the neighbour note becomes closer.
  Optional brighter centre line at the exact node as the intonation target.
- **Open string** (fret 0): a point at the nut, not a span, so it stays a marker
  rather than a rect. There is no zone behind the nut to fill.

### Markers

Configurable on/off/style. Fretless defaults markers on, since with no wires they
are the only orientation. In Realistic mode the 12th-fret marker sits at the
exact string midpoint (the octave), a real anchor.

### Fill modes

Single note color, or color by scale degree / interval function (root, third,
fifth, seventh). Opt-in. The projection currently carries only "is this the
root"; degree coloring extends it to carry the interval. The rect is the region;
the fill mode is only what color it takes.

### Everything is a setting

A compact **Fretboard** settings group: Spacing, Extent (plus window size),
Markers, Fill. Context-aware defaults, all overridable. Suggested defaults:
fretless picks Realistic with markers on; a small viewport picks the window;
a dense upper range is where the user reaches for Schematic.

### Out of scope here (deeper model work, flagged not scheduled)

- **Courses.** A 12-string guitar and a mandolin (8 strings in 4 doubled
  courses) are not single strings. The model is a flat pitch list, so mandolin
  is simplified to 4. A course concept (visual pairing, unison or octave
  doubling) is a separate model change.
- **Non-12-TET.** Oud, saz, and various folk instruments use microtonal or
  movable frets. The pitch model is 12-TET (`NoteName` + `Accidental` + octave),
  so they cannot be represented faithfully yet. Promoted into scope as the gate
  for Phase A3.
- **Physical cross-instrument scale.** True relative neck lengths need
  scale-length data; only relevant when comparing instruments side by side.

## Plan

Phases are ordered so the pure, test-verifiable work lands first, independent of
the host build (currently blocked by the concurrent Cambium 0.2.0 migration).

### Phase A — Instrument catalog breadth (`woodshedding`, pure)

Decided as three passes of increasing depth:

- **A1 — Bowed family (DONE 2026-07-14).** Added `Violin`, `Viola`, `Cello`, and
  `DoubleBass` variants plus tuning specs: violin/viola/cello in fifths (pitches
  identical to the mandolin family, asserted by test), double bass in fourths
  (the outlier), with fiddle cross-tunings, a 5-string cello, and double-bass
  solo and 5-string tunings. Confirmed no exhaustive `Instrument` match exists,
  so variants add cleanly. 7 new tests, `cargo test -p woodshedding` green.
- **A2 — Common world/folk (DONE 2026-07-14).** Added `Bouzouki`, `Charango`,
  `Cavaquinho`, `Balalaika`, and `MountainDulcimer` variants with 9 tuning specs:
  Irish (GDAD, GDAE) and Greek (tetrachordo CFAD, trichordo DAD) bouzouki,
  charango GCEAE, Brazilian cavaquinho DGBD, prima balalaika EEA, and dulcimer
  DAD/DAA. Two honesty notes carried in the code: multi-course instruments are
  flattened to one string per course (a courses-phase refinement), and the
  mountain dulcimer's real fretboard is diatonic (a fret-pattern refinement). 6
  new tests.
- **A3 — Microtonal instruments.** Oud, saz, and kin. This pass is not just data:
  it needs the non-12-TET pitch-model extension listed under out-of-scope below,
  so it is gated on that work rather than being a catalog drop.

- **Done when** (per pass): `cargo test -p woodshedding` is green; `catalog_for`
  is non-empty for each new instrument.

### Phase B — Per-instrument neck data (`woodshedding`)

Add a standard fret count per `Instrument`. Reserve an optional scale length for
future cross-instrument sizing only if wanted now.

- **Done when**: each `Instrument` reports a standard fret count that the
  full-neck extent consumes.

### Phase C — Fretboard as a Sprigging paint leaf (`woodshed-views`)

Decided 2026-07-14: CSS cannot render the board well (see Findings) — strings are
soft, and crisp thin lines, real spacing, and on-board inlays are out of reach.
So the board becomes a **Sprigging paint leaf**, the sanctioned tool for custom
visuals (not a workaround: genet's CSS gaps are worth closing for regular UI, but
a fretboard is custom graphics like a chart). The leaf implements
`sprigging::Leaf` (`measure` + `paint(cx)`), is keyed in the host's
`LeafRegistry`, and is fed the board model (open strings, dots with colour and
label, fret count, window) plus the palette. `PaintCx::fill_rect` draws crisp
solids, exactly what CSS lacked:

- Strings as thin `fill_rect`s at each string's centre, thickness per string (a
  real taper, low thick to high thin) — the concrete fix for the strings issue.
- Fret wires and the nut as `fill_rect`s; inlays as centre-of-neck dots.
- Note markers as rects (or rounded via a `Path`), coloured by note or degree,
  with centred labels via the glyph/text path.
- Spacing and extent computed in `paint`: Realistic (logarithmic) vs Schematic
  (equal), Full-neck vs Windowed, all from one geometry routine.

Reference leaves: `sprigging::{Meter, Knob, GraphGlyph}` in
`repos/cambium/crates/sprigging/src/glyphs.rs`; host wiring via `LeafRegistry`.

- **Done when**: the leaf renders the board with crisp thin variable strings,
  both spacing modes, and markers with centred labels, replacing the CSS board.

### Phase D — Fretless presentation (`woodshed-views`)

Semitone-band regions, node centre line, fretless marker style, no wires.

- **Done when**: a violin or cello tuning renders as zones with markers.

### Phase E — Fill modes and marker config

Degree/interval fill (extend the projection to carry degree); marker style
choosable (dot, rect, and later forms) and markers on/off. The rect marker form
landed early (see Progress); this phase turns the choice into a persisted
setting rather than a hardcoded form, and adds proper on-board inlays (a board
background layer) in place of the fret-number-ruler-only position cue.

- **Done when**: marker style is a persisted setting; fill toggles between note
  and degree; on-board inlays render without the checkerboard look.

### Phase F — Settings and instrument picker

The Fretboard settings group drives all of the above and persists. Build the
deferred instrument picker: filter the tuning list by instrument.

- **Done when**: settings persist and drive the board; the picker filters
  tunings by instrument.

**Dependency**: Phases C through F need the host to build. Phases A and B are
pure `woodshedding` and verify now.

## Open decisions

- **Catalog breadth.** Decided: three passes, bowed family (done), then common
  world/folk, then microtonal (gated on the pitch-model extension). Specific
  world/folk shortlist for A2 still to confirm.
- **Scale-length data.** Add it now as a reserved field, or defer until
  cross-instrument sizing is a real feature?
- **Default policy.** Confirm the context-aware defaults above.
- **12-string and mandolin.** Model as flat doubled strings for now (two rows),
  or wait for a real course concept?

## Findings — CSS fretboard rendering hits genet's limits (2026-07-14)

Trying to make the strings crisp, thin, and variable-thickness in CSS surfaced
hard limits in genet's CSS support, and reframes the board as paint-leaf work:

- **Gradient bands render soft.** A hard-edged `linear-gradient` string band
  anti-aliases into a soft glow, and sub-pixel thickness differences (0.8px vs
  3px) get lost in rasterisation, so a per-string thickness taper is barely
  visible. This is why the strings read as glowy rather than as crisp hairlines.
- **Solid `::before` hairlines do not render.** `.string::before { content:"";
  position:absolute; ... z-index:-1 }` produced no visible line at all (the
  pseudo-element, absolute positioning, or negative z-index is unsupported or
  dropped behind the opaque board).
- **Flex `order` is unsupported.** genet honours DOM/insertion order only, so a
  stable-DOM layout cannot re-arrange pieces by CSS; layout modes must differ by
  DOM structure (which is what makes the layout-switch reconciler crash hard to
  dodge from the Woodshed side).

Conclusion: crisp strings, real (logarithmic) fret spacing, and proper on-board
inlays are **paint-leaf work** (Sprigging), not CSS. This is direct evidence for
doing Phase C/D as a custom-painted board rather than continuing to push CSS.

Two genet-side bugs, diagnosed by the cambium agent (2026-07-15) and being fixed
at the engine level. My first-pass guesses here were wrong; the accurate account:

- **Stale-`NodeId` crash across the DOM/layout publication boundary** (not a
  reconciler double-free, and not the "root-class-change rebuild" I guessed). A
  type-changing subtree replacement (e.g. the Settings-page content swap) makes
  `replace_inner` remove node N and genet drop its subtree at once, while a
  retained layout snapshot / focus / hit-test still holds N; the next input
  routing reads N and `ScriptedDom::node()` panics. A pure board-layout change
  repeated hundreds of times retires nothing, so the layout switch is only
  associated, not causal. Fix: genet `hit_test` returns `None` for a non-live id;
  cambium clears dead focus/pointer-capture after every rebuild and `is_live`-
  guards click/key/pointer/hover/wheel targets.
- **Settings hit-test offset**: a click can land on a neighbouring option. My own
  sweep found fresh hit-testing accurate, so this is the transient drift from the
  stale layout above (a retired-but-reachable id), which the same fix removes.

## Progress — paint leaf (2026-07-15)

Phase C milestone 1 landed: the Scale/Chord board is now a **Sprigging paint
leaf**, not a CSS grid. The host plumbing already existed (the Related-panel
`GraphGlyph` leaf), so it was a matter of extending it: a `FretboardLeaf`
(`crates/woodshed-views/src/fretboard_leaf.rs`) implementing `sprigging::Leaf`,
placed via `custom_leaf(FRETBOARD_LEAF_KEY, w, h)` in `board()`, and rebuilt from
the board dots by `sync_fretboard_leaf` in the host (mirroring
`sync_neighborhood_leaf`). `fill_rect` gives exactly what CSS could not: crisp
thin strings with a real per-string thickness taper, crisp fret wires, a thicker
nut, faint centre-of-neck inlays, and quiet muted-colour markers (a subtler look
than the old bright blue/orange). Verified in `testing/woodshed/woodshed-leaf.png`.

Labels landed (2026-07-15): note names ride as a thin CSS text layer over the
leaf, each `fret-label` absolutely positioned at the leaf's own marker centre
(`note_center_x` / `string_center_y` / `MARKER_W` / `MARKER_H` are shared by the
leaf's paint and the overlay), so they align exactly. Verified in
`testing/woodshed/woodshed-leaf-labels.png`. Crisp board, quiet colours, readable
labels — the board is now complete-feeling.

Marker-style setting landed (2026-07-15): `FretboardSettings.marker_style` drives
the leaf, with **Sharp** (default), **Rounded**, **Circle**, and **Diamond** all
rendering (rounded/circle/diamond via `fill_path`; verified in
`testing/woodshed/woodshed-marker-{circle,diamond}.png`). Picker added to the
Fretboard settings page. Two findings surfaced: persisted `genet-state.json`
overrides code defaults (`serde(default)` only fills missing fields), and the
pre-existing settings hit-test offset means the picker is not reliably clickable
in-app yet, so a marker style can only be chosen by editing the state file until
the hit-test bug is fixed. Interactivity spun out to
[2026-07-15_fretboard_marker_detail_plan.md](2026-07-15_fretboard_marker_detail_plan.md).

Remaining refinements, in priority order:

- **Palette-derived colours** so the board is theme-aware (fixed dark ink now).
- **Other lenses** (Arpeggio/Progression/Exercise/Rehearsal) still use the CSS
  board; move them onto the leaf too.
- **Spacing modes** (Realistic/Schematic) and **extent** (full-neck/window), now
  cheap to compute in `paint`.

## Progress

- **2026-07-14**: Plan drafted. A first fretboard fix (drawing strings, fret
  wires, nut, inlay markers, and a fret-number ruler in place of bare dots) is
  written in `woodshed-views` (theme.rs, stage.rs, stage/rehearsal.rs) and
  routed through shared helpers, but compile and visual verification are blocked
  by a concurrent Cambium 0.2.0 migration that skews the workspace's `stylo`
  resolution. Confirmed via a scoped probe that adding `Instrument` variants
  does not break downstream compiles, so Phase A can proceed and verify against
  `woodshedding`'s own tests independent of the block.
- **2026-07-14**: **Phase A1 (bowed family) landed.** Added `Violin`, `Viola`,
  `Cello`, `DoubleBass` to the `Instrument` enum, `ALL`, and `Display`, plus 9
  tuning specs and 7 tests in `crates/woodshedding/src/tuning.rs`. Violin/viola/
  cello standard tunings verified pitch-identical to the mandolin family; double
  bass verified in fourths; bass vs double-bass disambiguation covered. `cargo
  test -p woodshedding tuning`: 39 passed, 0 failed. Untouched by the host
  block.
- **2026-07-14**: **Phase A2 (common world/folk) landed.** Added `Bouzouki`,
  `Charango`, `Cavaquinho`, `Balalaika`, `MountainDulcimer` and 9 tuning specs to
  `crates/woodshedding/src/tuning.rs`, with 6 new tests (Irish GDAE verified
  equal to octave mandolin, balalaika unison E strings, charango re-entrancy,
  cavaquinho DGBD). Full crate suite green: 163 passed, 0 failed, no warnings.
  Instrument count is now 14 named plus `Other`.
- **2026-07-14**: **Host build unblocked** (the concurrent Cambium 0.2.0
  migration landed and the workspace resolves again). Built `woodshed-genet`
  clean (0 errors, 51s), launched, and captured
  `testing/woodshed/woodshed-fretboard-fixed.png`. The earlier fretboard fix is
  now visually verified: fret wires, strings through the note centres, the thick
  nut at the open column, inlay tint (strongest at the 12th), and the
  fret-number ruler all render, replacing the bare dots. The search field shows
  its "Search catalog" placeholder. The new instrument tunings are selectable in
  the flat Tuning dropdown.
- **2026-07-14**: **Marker redesign (interim Phase C/E).** Per feedback: note
  markers are now rounded rects centred in the fret space and coloured like the
  notes (root vs note), with labels centred via flex. The inlay column tints that
  read as a checkerboard are removed, leaving a clean lattice of wires, strings,
  and markers; position reference is the bolded fret-number ruler for now, with
  proper on-board inlays deferred to Phase E. Renamed the "Set Templates" catalog
  tab to "Sets". Built clean and captured
  `testing/woodshed/woodshed-fretboard-rects.png`. The dot-versus-rect choice
  becomes a persisted setting in Phase E. Separately requested and not yet
  started: revive theme creation from seed colours (its own doc,
  [2026-05-20_theme_system_design.md](2026-05-20_theme_system_design.md), built
  on the `tinct` OKLCH derivation), and a look at whether the
  Stage-to-Set-to-Rehearsal flow reads as intuitively as intended.
- **2026-07-16**: **Phase B (per-instrument neck) + windowed Extent landed and
  verified — the old fret-range capability restored.** `fret_count` had been
  hardcoded to 12; even "full neck" was wrong (a guitar is 22). Now:
  - `Instrument::standard_fret_count()` gives each of the 15 instruments its own
    full neck (guitar 22, bass 24, ukulele 15, the bowed family 24 semitone
    positions, dulcimer 14, …). Tested (`every_instrument_has_a_sane_standard_neck`).
  - `FretboardSettings` gained `neck_start` + `neck_end` (`None` = the
    instrument's full neck). `StageState::apply_neck` resolves them into
    `fret_start..=fret_count`; `UiState::sync_neck` pushes them each frame so the
    extent tracks the settings and the current instrument with no special-casing.
    `dots()` / `dots_for_card` window to the range.
  - The paint leaf's geometry honours the window: the open-string column and the
    thick nut exist only when `fret_start == 0`; a mid-neck window is bounded by a
    plain wire, and inlays stay at their **absolute** frets (5/7/12) so they still
    orient you when no open string is in view. New `cells_left` / `first_cell_fret`
    helpers; `fretboard_px_size` / `note_center_x` / `wire_center_x` /
    `FretboardLeaf::new` all take `fret_start`.
  - A **Neck (frets)** control on the Fretboard settings page: Full, 0-5, 0-12,
    5-9, 8-16, 2-22.
  - Verified in the app: **Full** = the 22-fret guitar neck (70 positions);
    **0-12** = nut + open column (40); **5-9** = a mid-neck window, no nut, plain
    left boundary, inlays at absolute 5/7/9 (17). Screenshots `neck-full`,
    `neck-0-12`, `neck-5-9`.
  - **Follow-ons:** the Full 22-fret board is wider than a narrow pane and clips
    at the right — the window *is* the intended fix (the plan's "Windowed span,
    good for small screens"), but a wide board also wants horizontal scroll, and
    Schematic spacing would compress it. Arbitrary numeric From/To entry (beyond
    the presets) is the other obvious extension.
  - **Test note:** the `woodshed-core` unit test (`neck_window_bounds_the_board_
    and_the_dots`) is written but could not be *run* this session — a concurrent
    `chartulary` API refactor left the unrelated `woodshed-graph` sibling
    mid-reconcile, so the `woodshed-core` test binary won't link. The
    `woodshedding` suite (160, incl. Phase B) is green and the app verifies the
    feature end-to-end; re-run the core test once the sibling settles.
