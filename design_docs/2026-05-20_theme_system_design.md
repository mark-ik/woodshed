# Theme system design — seed-derived palettes + theme management

Proposal for the shared theming model used across the Strophos family
(Woodshed, Strophe, Mere) via `audio_widgets::theme`. Answers two
questions: (1) a **formula** for mapping a few seed colors → the full UI
palette, and (2) the **management** model (built-in vs user themes; edit /
rename / remove semantics).

Status: **proposal — needs Mark's sign-off before building.**

## Prior art

- **gpui / Zed**: ~200+ explicit named tokens, kept sane by per-hue
  **`ColorScaleSet`s** (stepped scales) + **refinement** (missing tokens
  filled from a compile-time default; e.g. status backgrounds derived from
  foregrounds). Mostly explicit, lightly derived.
- **Material You**: a single seed → tonal palettes (13 tones per hue) →
  roles pick tones. Heavily derived.
- **Radix Colors**: 12-step scales per hue, with a fixed contract (step 1–2
  app bg, 3–5 component bg, 6–8 borders, 9–10 solid, 11–12 text) and a
  text-contrast rule. Derived + principled.
- **Math**: do it in **OKLCH** (perceptually-even lightness/chroma steps,
  unlike HSL) and gate text with **APCA** (or WCAG as a fallback).

Our current `Palette` is the Radix-ish "explicit semantic roles" shape
(surfaces, text hierarchy, triad, semantic flags). The proposal keeps that
role *interface* but lets the role values be **derived from seeds**.

## Part 1 — seed → palette formula

**Seeds** (what a theme actually stores — small + pickable):

```text
primary, secondary, tertiary   // the brand triad (hue anchors)
neutral                        // surface/text hue (often near-grey, slight temp)
mode: Dark | Light             // which direction the ladders run
// success / danger are fixed semantic hues by default (overridable)
```

**Derivation** (all in OKLCH, then to sRGB for masonry `Color`):

- **Surface ladder** — steps off `neutral` lightness. Dark: `bg` darkest →
  `surface` → `surface_2` → `surface_hover` each +ΔL. Light: inverted, start
  near-white-but-not-blowout and step down. ΔL ≈ a fixed perceptual step
  (e.g. 0.03–0.05 L) so elevation reads evenly.
- **Text hierarchy** — `text` = the neutral-scale step (near-black/near-white)
  that clears an **APCA target** against `surface`; `text_dim` and
  `text_disabled` are reduced-contrast steps toward the surface (lower Lc
  targets). This is the "readability formula": pick the step that passes, not
  a hardcoded grey.
- **`on_*`** — for a fill in hue H, `on_H` = whichever of {near-white,
  near-black, or a tone of H} first passes the contrast target on H. Replaces
  hand-picking each `on_primary`.
- **Accent variants** (the "+/- N per field for distinctive accents" you
  described) — derived by **lightness step** (tone ±N off the seed) for
  hover/active/muted states, and optionally **hue rotation** (analogous ±15°
  or complementary +180°) where a distinct-but-related accent is wanted.
- **Semantic** — `success`/`danger` are their own fixed seed hues (green/red)
  run through the same on_* + step logic, so they stay universally legible
  rather than drifting with the brand.

**Product layers stay derived too**, so a new theme is *just the seeds*:
- Woodshed fretboard: `note_dot ← primary`, `root_dot ← tertiary`,
  `dot_label ← on_(note_dot)`, lines ← neutral steps.
- Strophe waveform: `wave ← primary`, zero-line ← neutral-dim.

Net: **a theme = 4 seed hues + a mode + (optional) semantic overrides.**
Everything else is computed. Hand-tuning is then an *override layer* on top
of the derived values, not the default authoring burden.

## Part 2 — theme management

Replace the `ThemeMode { Dark, Light }` enum with:

```rust
struct ThemeId(String);              // stable key, e.g. "builtin:dark", "user:abc123"

struct ThemeDef {
    id: ThemeId,
    name: String,                    // display; editable for user themes
    seeds: Seeds,
    overrides: Vec<(Role, Color)>,   // optional hand-tweaks on top of derivation
}

enum ThemeSource { BuiltIn, User }
```

- **Built-ins**: code-defined `ThemeDef`s with stable ids. Immutable in code.
- **User themes**: `Vec<ThemeDef>` persisted in `Settings`; created via a
  color-picker UI editing the seeds (live preview, since switching is already
  runtime).
- **Selection**: `Settings.active_theme: ThemeId` (a string).

**CRUD semantics (your rules):**

- **Create** (user): pick seeds → new `user:` theme.
- **Remove**: only `User` themes are removable. Built-ins can't be deleted.
- **Edit / rename a built-in**: **forks** to a `user:` copy seeded from the
  built-in; the built-in stays intact, the edit lands on the copy. (So
  "editing a built-in" is non-destructive and the original is always there.)
- **Edit / rename a user theme**: in place.

**Graceful handling (your requirement):**

- `active_theme` is resolved by id at load. If the id is **missing** (user
  removed it, or a built-in id changed): fall back to the default built-in
  and log — never crash, never strand the user on a blank theme.
- **Rename** doesn't change the id, so selection survives renames.
- **Old-format migration**: existing `state.json` has `theme_mode: "Dark" |
  "Light"`. A `#[serde(default)]` + a migration maps the legacy field to
  `active_theme = "builtin:dark"|"builtin:light"` on first load, then writes
  the new shape. Old saves keep working.

## Where it lives

`audio_widgets::theme`: the OKLCH math, the derivation engine
(`Seeds -> Palette`), the `ThemeDef`/`ThemeId`/`Seeds` types, and the
built-in seed set. Each app reads the derived `Palette` and layers its
product colors (also derived from the same seeds). The Settings/persistence
of *user* themes lives per-app (it's app settings), but the types are shared.

## Open decisions for Mark

1. **Derivation depth**: full seed→formula (Material-You-ish, most magic,
   least authoring) vs. derived-with-explicit-overrides (Radix-ish, the
   proposal's default) vs. stay explicit-per-palette (gpui-ish, most control,
   most work per theme). Proposal picks the middle.
2. **User color-picker now, or built-in set first?** The management model
   supports both; we could ship a curated built-in set first and add the
   picker once the derivation engine is proven.
3. **OKLCH dependency**: pull a small crate (e.g. `palette` or hand-roll the
   OKLCH↔sRGB transform — it's ~40 lines) vs. approximate in HSL (worse steps,
   no new dep). Recommend hand-rolled OKLCH (no churn, correct).
4. **Semantic colors**: fixed green/red always, or let a theme reseed them?
   Proposal: fixed by default, optional override.

## Status / findings

- 2026-05-20: **Engine + curated built-in set shipped.** OKLCH derivation
  (`Seeds → derive_palette`, 8 tests) in `audio_widgets::theme`; `ThemeMode`
  expanded to Dark / Light / Dusk / Meadow / Parchment, all derived; Woodshed
  fretboard colors derive from the base; Settings picker lists all; live
  switching confirmed. User color-picker + CRUD still deferred.
- **Distinctiveness finding (Mark, 2026-05-20):** with near-grey `neutral`
  seeds the themes read too alike ("blue dark / orange dark / green dark + blue
  light / yellow light") — surfaces dominate the field and only the accent
  swaps. **Addressed:** tinted each built-in's `neutral` seed (blue-cool Dark,
  warm-brown Dusk, green Meadow, warm-cream Parchment, cool Light) so the
  surface ladder carries the hue.
- 2026-05-20: **User themes + live color editing shipped** (the deferred Part 2,
  MVP). `Settings.user_themes: Vec<UserThemeDef>` (hex seeds, serde) +
  `active_user_theme: Option<String>`; additive to the existing `theme_mode`
  field so old saves migrate (absent → built-in). AppState resolves
  `current_seeds()` → `palette_from_seeds` → live rebuild. CRUD: select any
  built-in or user theme; **+ New custom** forks the active seeds into an
  editable copy; user themes rename / remove (built-ins can't); a missing
  active name falls back to the built-in. Editor: a Dark/Light toggle + rename
  + per-seed color editing.
- 2026-05-20: **Expanded role model (Mark's "split the tiers" request).** Seeds
  grew text_header/text_body as **optional overrides** (None = derive from
  neutral, legible); `Palette` gained `text_header`; dim/disabled now derive
  from body (so a body override cascades). Routing so the seeds *matter*:
  `text_header` colors all 11 titles/headings (new `header_label` helper +
  the big TS_LG/XL/2XL labels); `secondary` now tints the header strip
  app-wide (faint mix toward surface) on top of its existing lane use
  (alternating section bands / cursor); `tertiary` continues to drive root
  dots + playhead. Editor gained per-tier **Derived↔Custom** toggles with
  R/G/B sliders when custom. Element split done per the agreed mapping; can
  push secondary/tertiary into more spots (chord cards, sub-actions, active
  tab) later.
- 2026-05-20: **Color sliders picker.** Replaced the hex inputs with, per seed,
  a live **swatch + R/G/B sliders** (`AppState::set_seed_channel`) that
  re-derive the whole palette as you drag, plus a hex readout. Sliders are
  controlled (write state every tick) so they avoid the textbox
  reset-on-rebuild that made the hex inputs need a buffer. Still RGB (HSL/OKLCH
  sliders are a nicer-feel follow-up); per-role overrides still future.

## Out of scope (this doc)

Syntax-highlight palettes (we have none), per-widget token explosion (we
keep the compact semantic-role set, not gpui's 200), and the deferred popup
overlay.
