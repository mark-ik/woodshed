# UI redesign — GPUI-quiet refresh + Rehearsal/Practice reshape

From the **Redesign Explorations** board (`screenshots/Woodshed — Redesign
Explorations.pdf`, 11 pages, 2026-06-15). The board treats the existing design
docs as its brief (the two-axis neck/set model, the U6 set-timeline, and
fretboard-as-canvas) and stays inside the seed-derived theme engine: nothing it
shows uses a colour the engine can't produce. The goal is a quieter, warmer,
less-generic shell. Most of it is reskin + recompose, not new infrastructure.

Status: **active — decisions locked 2026-06-15 (below); building cheap layer first.**

## Decisions (Mark, 2026-06-15)

1. **Palette: ship both Slate and Ember as built-ins.** Slate = the faithful
   cool-dark direction (the current dark, cleaned up). Ember = the bold
   warm-dark "Dusk sibling": terracotta notes, gold roots, woody surfaces. Both
   derive from the existing seed engine, beside Dark / Light / Dusk / Meadow /
   Parchment.
2. **Nav: segmented pills.** A real segmented control (quiet active fill +
   `tertiary` "you are here" indicator), with the ◀▶ steppers replaced by
   labelled dropdowns. The **left material rail** is kept on file as the
   candidate to revisit **if it proves better for mobile**. The command
   breadcrumb + ⌘K option is not pursued now.
3. **Fretboard layout: a user setting**, not one hardcoded winner — Vertical
   two-pane / Horizontal hero / Full-canvas.

## The cross-cutting move ("GPUI-quiet")

Applies under every palette, so it is palette-independent:

- Hard split bars become **hairline borders**; always-on scrollbars become
  breathing room (auto-hide where a scrollbar is still needed).
- A real **type scale** and calmer **density**.
- **◀▶ steppers become labelled dropdowns / palette-coloured glyphs.** This
  also retires the theme-immune blue arrows: today's ◀▶ render as colour emoji,
  which ignore the palette; dropdowns and palette-glyph controls fix that.
- Everything re-skins from theme seeds (no element painted outside the engine).

## Phases

Sequenced cheap-and-high-impact first, then the one structural nav call, then
the per-screen reworks. Mobile is a separate downstream track.

### P1 — Palette: Slate + Ember built-ins
- Add **Ember** as a new warm-dark seed set in `audio-widgets::theme` beside the
  existing built-ins; its note/root colours fall out of the seeds we already
  route to the dots (`primary` → note, `tertiary` → root), over a warm neutral.
- Add (or refine into) **Slate**, the cleaned-up cool-dark. Open sub-point:
  whether Slate supersedes the current `Dark` or sits beside it — Slate is
  close to today's Dark palette, so decide during P1 to avoid a near-duplicate.
- *Done when:* both appear in the Settings theme picker, switch live, and
  re-skin every surface (fretboard dots, header tint, selection, chrome) with
  no theme-immune element left.

### P2 — GPUI-quiet chrome pass
- Hard split bars → hairline borders; remove/auto-hide always-on scrollbars;
  apply the type scale + density rhythm.
- Replace the ◀▶ steppers (instrument, tuning, fret-span, root, voicing) with
  labelled dropdowns or palette-coloured glyph buttons.
- *Done when:* no hard split bar or always-on scrollbar remains in the main
  surfaces, the cyclers read as themed controls, and instrument/tuning/root use
  dropdowns. The blue-emoji arrows are gone.

### P3 — Navigation: segmented pills
- Collapse the stacked tab strip + lens row into a real segmented control with
  a quiet active fill and a `tertiary` indicator (board: "Refined — Segmented
  pills"), folding cleanly with the CSD header (which is now the title bar).
- Keep the left-material-rail design recorded as the mobile/alt candidate.
- *Done when:* nav reads as a segmented control rather than two button rows,
  and the chrome above the neck is one tidy strip set.

### P4 — Stage: fretboard layout as a setting
- Add a setting: fretboard layout = **Vertical two-pane** (today) | **Horizontal
  hero** | **Full-canvas**. Persist it.
- Horizontal hero and full-canvas need the **horizontal-neck orientation** that
  the composable-surface plan deferred; implement it here.
- *Done when:* the user can pick the layout, all three render the same resolved
  positions (shared `resolve_card_for_stage` / `fretboard_view`), and the choice
  persists across restart.

### P5 — Rehearsal: measured filmstrip + transport deck
- Reskin the shipped U6 horizontal set lane as a **measured filmstrip**: each
  card shows its diagram + touch/timing + provenance (`from …`), and played
  cards dim behind the cursor.
- Pull transport into a **dedicated deck** (tempo, loop, count-in, per-card
  sound), per the board's Rehearsal screen.
- *Done when:* the Rehearsal stage matches the filmstrip + deck treatment,
  passed cards dim during playback, and transport is one grouped surface.

### P6 — Practice: recipes as tiles
- Present recipes (practice sets / progressions / exercises) as a **tile grid**:
  thumbnail + blurb + tags + one-tap "Fill set."
- *Done when:* Practice is a recipe tile grid that fills the set in one tap and
  reads well as the recipe library grows.

### Downstream (separate track) — mobile companion
- The same card model in a portrait frame: a bottom dock on Stage, a horizontally
  scrolling lane on Rehearse, a tap-to-fill recipe list on Practice.
- This is the `xilem_web` / shared-view-core effort already scoped in
  [2026-06-14_web_profile_plan.md](2026-06-14_web_profile_plan.md). Not part of
  this pass; the left material rail (P3 alt) may serve the mobile nav.

## Relationship to existing plans

- **Theme system** ([2026-05-20_theme_system_design.md](2026-05-20_theme_system_design.md)):
  P1 is new seed sets through that engine; no new theming infrastructure.
- **Composable instrument surface**
  ([2026-05-21_composable_instrument_surface_plan.md](2026-05-21_composable_instrument_surface_plan.md)):
  P4's horizontal neck is the orientation work that doc deferred.
- **Rehearsal redesign** ([2026-05-22_rehearsal_redesign_plan.md](2026-05-22_rehearsal_redesign_plan.md)):
  P5 reskins the U6 lane it shipped; the model is unchanged, only the surface.
- **CSD chrome** (shipped 2026-06-15, on `main`): the header is now the title
  bar, which is the foundation P3 reduces chrome on top of.

## Findings

- The board re-skins entirely from existing theme seeds. The engine already
  derives note/root/surface from seeds, so Slate/Ember are seed sets, not repaints.
- The single fretboard widget is already parameterised (scale / chord / voicing /
  position), so the board's "one widget, many scopes" page is render params, not
  new widgets — low cost, and reassuring that the architecture already fits.
- Source board pages rendered to `screenshots/_redesign_pages/` (scratch).

## Progress

- 2026-06-16: **P3 started. Feasibility resolved + P3a (segmented pills) shipped.**
  - **Combobox prerequisite (resolved):** masonry already ships the overlay
    machinery — a `layers/` system (`selector_menu`, `tooltip`) and a built-in
    `Selector` widget ("a combo box in some frameworks") whose menu pops as a
    floating layer, not inline. `ZStack` is exposed as a xilem view; `Selector`
    is not. **Decision (Mark): adopt `Selector`** (Path A) — add a xilem
    `selector` view wrapper in the `woodshed-theme` fork and theme its menu,
    rather than hand-rolling an app-side `zstack` overlay. The current inline
    combobox lives in `xilem-components` (editable), but Path A reuses the
    mature widget (native anchoring / keyboard / a11y). P3b.
  - **P3a — segmented pills (done):** added a reusable `pill` helper (active =
    quiet `surface_2` fill + `tertiary` label, the "you are here" indicator;
    inactive = flat, default button border neutralized) and routed both the tab
    strip (Stage / Practice / Song / Rehearsal / Settings) and the lens strip
    (Scale / Chord / Arpeggio / Progression / Exercise + the Tuner/Metronome
    mount toggles) through it. Retires the `[Stage]` bracket cue and the
    `●`/`○`/`  ` glyph prefixes — the fill is the indicator now. Verified
    (`screenshots/p3a-pills.png`).
  - **P3b — next:** header instrument/tuning/scale dropdowns via `Selector`
    (fork view wrapper + DefaultProperties theming for the menu), replacing the
    `‹ ›` cyclers with labelled dropdowns per the board.
- 2026-06-16: **P2c done (auto-hide scrollbars) + P2d found already-applied; P2
  is complete.**
  - **P2c:** masonry's `Portal` carries an `AutoHideScrollBar(bool)` property
    (rest opacity 0, fade-in on pointer move, fade-out after a 400ms timeout);
    the default is always-on (`false`). Two portals already opted in (rehearsal
    queue, lens); added `.prop(AutoHideScrollBar(true))` to the remaining six
    catalog sidebars (Scales / Chords / Arpeggios / Progressions / Exercises /
    Tunings). A global `DefaultProperties` insert can't do this: the xilem
    `portal` view builds `Portal<Child::Widget>` (concrete child type), so every
    site is a distinct `TypeId` — the property has to be set per-portal. Now no
    always-on scrollbar remains; they overlay and stay hidden at rest, so
    content keeps full width (the "breathing room" effect).
  - **P2d:** the type scale and density rhythm are already applied uniformly —
    a sweep found zero magic-number `text_size`s (all go through `TS_*`) and
    only one off-grid spacing value (the Button default `Padding::from_vh(6, 16)`
    in `build_default_properties`, a deliberate button choice). So there is no
    scale/spacing normalization left to do. The remaining density win is the
    cramped triple header strip (header / nav / material), which **P3's nav
    reskin collapses** — folding breathing-room there avoids churn P3 would
    redo, rather than a speculative pass now.
- 2026-06-16: **UI font → `SansSerif` (Segoe UI on Windows); root-cause fix
  for the glyph tofu.** Added a shared `ui_family()` beside `mono_family()` in
  `audio-widgets::theme` (`SansSerif`), re-exported through Woodshed's `theme`,
  and routed every label + text button through it by shadowing `xilem::view`'s
  `label`/`text_button` in `main.rs` (the framework default `SystemUi` lacks the
  Dingbats / Misc-Symbols / geometric-arrow blocks, which is why those glyphs
  tofu'd). The wrappers are drop-in: every helper (`button_sm`, `dim_label`,
  the `*_prose` color wrappers) and bare call site inherits the font; the
  monospace readouts keep their `.font(mono_family())` override. Done in the
  **app crate**, not the lean xilem fork (font is an app decision, and the build
  rides the `mark-ik/xilem` git dep with no path override). Verified
  cross-screen via the screenshot harness (`scry-shots/shoot-woodshed.ps1`):
  `screenshots/font-fix-sansserif.png` shows clean, consistent type with no
  tofu — the combobox `▼`, `‹ › ‹‹`, `− +` all render. This closes the P2a
  "glyph-complete UI font" follow-up. Themed-glyph *restoration* (filled
  triangles, ♯/♭, ★, ☰) is now unblocked but optional — the current ASCII /
  guillemet glyphs read cleanly and intentionally; accidentals in note names
  live in the `woodshedding` theory crate, so any ♯/♭ pass is a separate,
  larger change there, not a `main.rs` reskin.
- 2026-06-15: Plan created from the Redesign Explorations board. Decisions
  locked (both palettes; pills nav, rail held for mobile; fretboard-layout
  setting). Sequencing: cheap layer (P1–P2) → nav (P3) → screens (P4–P6);
  mobile downstream.
- 2026-06-16: **P2a completed + P2b shipped; screenshot self-verify established.**
  - **Screenshot pipeline:** launch the built binary → maximize via Win32 →
    capture the screen → read the PNG. The app reopens on the last tab, so lens
    tabs self-verify too (not just Settings). This caught regressions live.
  - **P2a tofu cleanup** (`1691a3b`, `353238e`): the de-emoji exposed that the
    UI font lacks the Dingbats / Misc-Symbols / filled-triangle blocks, so the
    text triangles, `✕`, `♯`/`♭`, `★`, `☰`, `⏮` all rendered as tofu boxes. Fixed
    with font-independent glyphs: cyclers/transports use `‹`/`›` + `‹‹`, `×` for
    remove/close, `#`/`b` accidentals, `*` markers; dropped decorative
    `☰`/`✓`/`♪`. **Deeper finding:** a glyph-complete UI font (Segoe UI) would
    let nicer icons return as *themed* glyphs — flagged as P2 polish.
  - **P2b split dividers** (`fe0acae`, `13d1bae`): the divider was painted with
    hardcoded `theme::ZYNC` (heavy two-line bar, theme-immune). Threaded a
    palette `bar_color` through the vendored split widget + view; the default is
    now a 1px solid `surface_hover` hairline that re-skins. Cards were already
    1px hairlines, so P2b's borders are done.
- 2026-06-16: **P2a shipped** (`9c18ca1` arrows; `64f9fe1` plus/speaker). Every
  theme-immune glyph now re-skins: the emoji arrows U+25C0/25B6 became the
  text triangles `◂`/`▸` (U+25C2/25B8, palette-coloured) app-wide; the heavy-plus
  emoji became `+`; the speaker emoji became text `Sound`/`Muted`. **Header
  instrument/tuning dropdowns deferred to P3.** The `xilem-components` combobox
  is inline-only (expands beneath the trigger), and the header already carries a
  note that a dropdown in the fixed-height strip "blows the layout open." They
  need an overlay-popup combobox, which belongs with the P3 header/nav redesign
  (the board's header dropdowns assume a popup). Palette-arrow cyclers stay
  meanwhile. So P3 gains a prerequisite: build the overlay-popup combobox.
- 2026-06-15: **P1 shipped** (`7f15d5d`; builds + `audio-widgets` tests green).
  Folded `Dark` into `Slate` (serde `alias = "Dark"` keeps old configs; cool-dark
  seeds carried forward unchanged) and added `Ember` (warm-dark Dusk sibling:
  terracotta notes = `primary`, gold roots = `tertiary`, woody warm-brown
  surfaces). Picker now lists Slate · Ember · Light · Dusk · Meadow · Parchment;
  default Slate. Pending Mark's visual check (Ember warmth, live re-skin). The
  GPUI-quiet *cleanup* of Slate is P2, not P1 (P1 is palette only).
