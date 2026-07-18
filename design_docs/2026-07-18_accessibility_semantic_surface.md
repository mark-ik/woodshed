# Accessibility: one semantic surface, three readers

A plan and a first build (2026-07-18), from a conversation about whether
Woodshed will serve disabled musicians.

## The framing

Every cambium app emits one artifact: a semantic, ARIA-attributed DOM, laid out
by genet-layout. Three consumers read it:

- **AccessKit** projects it to the OS screen reader (NVDA / JAWS / VoiceOver /
  Orca). That is accessibility.
- **genet-probe** resolves a selector (role or class, plus text or aria-label) to
  a window point over it. That is scripting, testing, and model-driving.
- An **AI agent** driving the app navigates the same tree.

So accessibility is not a feature bolted onto the product. It is the same
investment that makes the app automatable and agent-drivable. Enriching the
semantic layer serves a blind guitarist, a test that clicks "the button labelled
Vertical" instead of a pixel, and an agent, at once. For a project this size that
is the argument that turns "should we" into "obviously".

Woodshed also starts ahead on the audio axis: Hear, the stepping run (audible and
visible), the tuner, per-note play, the metronome. A blind or low-vision musician
already gets a great deal from the ears. The "touch should be audible" thread is,
in effect, accessibility work.

## What exists in the stack

- **AccessKit bridge** in `genet-winit-host` (`a11y.rs`): pushes a `TreeUpdate`
  to the OS adapter, queues screen-reader `ActionRequest`s back to the host.
- **DOM to AccessKit** in `genet-layout` (`a11y.rs`): `build_subtree` /
  `accesskit_tree` walk a laid-out DOM into AccessKit nodes. `role=` then tag
  sets the role; `aria-label` then direct text sets the name; `aria-checked` /
  `aria-selected` set state; buttons/tabs/switches/text-fields become routable.
- **Leaf semantics** in sprigging: `Leaf::accessibility(&mut accesskit::Node)`.
  A leaf fills its own node (a knob announces as a slider). `GraphGlyph`,
  `Meter`, `Knob`, `Swatch` implement it.
- **genet-probe**: resolves selectors over the same DOM for a driver/test.

## What is NOT wired in Woodshed (the gaps)

- **The host does not turn AccessKit on.** Woodshed runs on `cambium-winit`,
  which has no AccessKit code, so nothing reaches a screen reader yet. This is
  the biggest gap and it is host-level (likely a `cambium-winit` change that
  benefits every cambium app). Call it **Tier 0**.
- **Hand-rolled controls carry a class but no `role`.** Raw `el("div")` +
  `clickable` controls are findable by class+text but would not announce as
  buttons. Adding roles is accessibility work and probe-hardening in one move.
- **The Accessibility settings page is a placeholder** — it says so:
  "Reduced-motion and screen-reader preferences still need host wiring."

## The mechanism finding that decides the fretboard (and scroll/fit)

A leaf's `accessibility()` fills **one** node. It cannot emit a subtree. So a
painted fretboard cannot expose 40 navigable markers *through the leaf* — the
markers must be real DOM elements (the CSS label overlay), which the walk
projects as their own nodes. The leaf announces the *surface*; the overlay
announces the *notes*.

This locks the earlier scroll/fit decision: moving the labels into the leaf (for
true zoom-to-fit) would collapse them to a single node and blind the screen
reader and the driver alike. **"Leaf owns its viewport" is only permitted if the
markers stay in the DOM** (or a subtree mechanism is added to leaf a11y first).
The paint leaf being opaque is one blind spot with three symptoms: silent to a
screen reader, invisible to probe, and the reason the test harness was reduced to
guessing pixel coordinates.

## What landed (2026-07-18): the fretboard's semantic surface

Two coordinated parts, both dormant until Tier 0 lights them up, both testable
now:

- **The markers are named buttons.** Each overlay marker carries `role="button"`
  and an `aria-label` from `woodshed_core::marker_a11y_label` — "A2, root,
  string 6, fret 5", "C#3, Major 3rd, string 2, open". On the Rehearsal board it
  also states the mark state ("…, marked" / "…, muted"). Unit-tested on the
  default board. A screen reader navigates the notes; a driver resolves them by
  aria-label today.
- **The neck is a named region and a described graphic.** The `fretboard-stack`
  carries an `aria-label` naming the board ("A Minor Pentatonic fretboard, 40
  notes, frets 0-12"). `FretboardLeaf::accessibility()` sets the leaf's role to
  `GraphicsObject` with a structural fallback summary ("horizontal fretboard, 6
  strings, frets 0-12, 40 notes"); the author's region name outranks it.
  Unit-tested (leaf announces itself; an authored name is not overwritten).

Also on-theme: cambium's `GraphCanvasNode` grew a `key: Option<String>` ("a
driver or test selects on it; a screen reader still reads `label`") — the same
selector substrate. Woodshed's Related swatch now sets it (catalog id for the
centre, title for suggestions).

## The tiered plan

- **Tier 0 — flip the switch (host). LANDED 2026-07-18.** The switch is on: a
  screen reader now sees Woodshed. In `woodshed-genet`:
  - `SpriggingA11y` bridges genet-layout's `LeafA11ySource` to the Sprigging
    `LeafRegistry`, so each `<custom-leaf>`'s `accessibility()` fills its node
    (the mirror of `SpriggingSource` for paint). The fretboard announces itself.
  - `sync_a11y` (after every `redraw`) `project()`s the retained layout into an
    AccessKit `TreeUpdate` with those leaf semantics, installs it the first frame
    and updates it after, follows the app's keyboard focus, and drains a screen
    reader's actions, routing `Click` / `Focus` through the same `dispatch_click`
    a mouse uses (via an AccessKit-id -> DOM-node map rebuilt each frame).
  - The Windows adapter must attach before the window is shown, so the window is
    created hidden and revealed by `sync_a11y` after the first install. The
    hidden window will not receive a deferred redraw (the adapter's own caveat),
    so the first frame is driven synchronously in `resumed`, not awaited.
  - A wake `AtomicBool` (set by the adapter, honored in `about_to_wait` ->
    `request_redraw`) drains actions that arrive while the app is idle, without
    changing the event-loop type.
  - Verified: launches and renders normally; logs `accessibility Installed, 303
    nodes projected`; a probe of the tree read back a real marker,
    `"E2, Perfect 5th, string 6, open"` — the neck's markers reach the OS with
    their spoken labels. Not verified here: an actual screen reader voicing and
    navigating it (needs a manual NVDA / VoiceOver pass).
  - **Lifted to `cambium-winit` 2026-07-18.** `A11yHost` (install / update /
    drain, install-before-show) and `SpriggingA11y` now live in
    `cambium-winit::a11y`, so every cambium app gets the bridge as shared code.
    Woodshed's `sync_a11y` delegates to `A11yHost::sync`. Verified: cambium-winit
    compiles clean and woodshed still logs `303 nodes projected`.
- **Tier 1 — the neck's meaning.** Landed above for the fretboard. Extend to the
  arpeggio / exercise / progression boards (still CSS-grid) and add `role` to the
  hand-rolled controls.
- **Tier 2 — preferences (make that settings page real). LANDED 2026-07-18.** The
  Accessibility settings page is real: three toggle rows writing to
  `AccessibilitySettings` (`reduce_motion`, `distinguish_root`, `text_scale`).
  - **Reduce motion** appends a `transition: none` rule for the interactive
    classes (`theme::apply_accessibility`), so the hover/active fades stop while
    functional feedback like the stepping run stays.
  - **Text size** (Normal / Large / Larger) scales every `font-size: Npx` in the
    sheet by 1.0 / 1.15 / 1.3 (`theme::scale_font_sizes`); the UI reflows.
  - **Distinguish root** draws a light outline ring on root markers in the
    fretboard leaf (`C_ROOT_RING` via the `ring_at` closure), so root-vs-note is
    not carried by color alone. The host reads the flag, hashes it into the leaf
    sig, and passes it at both board sync sites.
  - Both preferences re-skin live: `accessible_sheet()` wraps `theme.css()` and
    the re-skin condition in `after_dispatch` now fires on a11y changes too, not
    just theme changes. Verified in-app: the board's root markers gained their
    ring, the whole UI enlarged at Larger, all three toggles registered.
- **Tier 3 — keyboard the board.** Arrow between markers, mark / draw / play by
  key, so the practice loop is operable without a mouse. Serves motor-impaired
  players who cannot hit small targets.

## Consequence for testing

Once Woodshed exposes a `ProbeSurface`, the harness resolves "the stepper
labelled +" instead of pixel math, and every passing probe test is also proof the
semantic layer a screen reader needs is intact. The marker aria-labels added here
already give a driver richer handles on the neck.
