# Woodshed onto the shared Cambium desktop host

**Date:** 2026-08-09
**Status:** done
**Upstream:** genet `components/cambium/cambium-genet-winit-host`, lane G1 of
retinue `design_docs/2026-08-09_signalman_cambium_desktop_scope.md`

## What happened

Woodshed was the donor for genet's single-root Cambium desktop host. It is now
its first consumer: the whole native-host assembly is gone from this repo.

`crates/woodshed-genet/src/main.rs` was 1728 lines, most of it a winit
`ApplicationHandler` owning a window, a `SurfaceHost`, a retained
`IncrementalLayout`, the paint pass, hit testing, pointer/keyboard/IME/wheel
routing, the overlay-scrollbar fade, and the `A11yHost` install-before-show
lifecycle. All of that is `cambium-genet-winit-host` now. What replaced it is
`HostOptions`, an `Init`, and five plain closures.

| before | after |
|---|---|
| `main.rs` 1728 lines | `main.rs` 211 |
| crate 3067 lines | crate 2300 |
| 1 file over the 600-line ceiling | largest file 472 |

The reduction is not the point; **the duplication is**. The same assembly is
hand-written in pelt, cleromancy, isometry, and turnstone. Woodshed is the
first to stop carrying its copy.

## The new shape

```text
main.rs      wiring only: HostOptions, boot_state, the five hooks
shared.rs    what woodshed owns beside the host: backends, storage, theme,
             leaf signatures, the scenario lane
drive.rs     the per-frame drive: tuner, song, rehearsal dwell, transport
             steps, MIDI, calibration — returns "keep frames coming"
leaves.rs    the custom-paint leaves and their rebuild signatures
sync.rs      the dispatch tail: backend, chrome, MIDI, persistence, re-skin
text.rs      the focused-text seam (which of two fields has the caret)
scenario.rs  the self-drive lane, now routing through the host
audio.rs / midi.rs / storage.rs   unchanged
```

Hook by hook:

- **`frame`** — refresh the viewport band, run `drive::frame`, push the MIDI
  clock-out master values, refresh the leaves. Its return keeps frames coming.
- **`after_dispatch`** — `sync::after_dispatch`: dropdown state into the core,
  audio state through the backend seam, MIDI connect/disconnect, window-chrome
  requests, persistence, and a theme or accessibility change handed back to the
  host as a new sheet through `ctx.set_sheet`.
- **`after_frame`** — `scenario::drive`: one scenario step per presented frame.
- **`focused_text`** — `text::focused_text`, recognizing the focused `<input>`
  by its wrapper's class and handing back borrows of the matching `TextInput`.
- **`key_intercept`** — Escape closes an open dropdown. Deliberately an
  intercept rather than a view handler: it is a window-wide policy, not a
  control's.

Scenario pumping moved from a bespoke `RedrawRequested` tail to `after_frame`,
and screenshots from a private `capture_view` + `read_texture_rgba` pair to
`AppCtx::capture` plus the host's `read_frame`. Woodshed now only encodes the
PNG; the readback is the host's.

The one genuinely new thing the migration needed was pointer delivery. The host
owns hit testing, capture, and dispatch order, so the scenario lane must not
re-roll them — it queues `HostPointer::{Moved, Press, Release}` on `AppCtx` and
the host runs each through the same path a real mouse takes. `press` therefore
lands just after the tick that requested it rather than inside it, which
changes nothing: the driver pumps one step per frame and asserts on the next.

## Dependencies dropped

`genet-layout`, `genet-winit-host`, `netrender`, `paint_list_api`,
`paint_list_render`, `cambium-winit`, and `cambium-winit-a11y` are no longer
named by this workspace. They were exactly the pieces needed to assemble a
native host by hand, and reaching around `cambium-genet-winit-host` for them
would put the duplication straight back. They still arrive transitively, so the
resolved graph is unchanged — only what woodshed is allowed to name.

`wgpu` stays (the capture closure signature) and `image` stays (PNG encoding).

**One local-development note.** `.cargo/config.toml` needs
`cambium-genet-winit-host` *and* `meristem` in its
`[patch."https://github.com/merely-made/genet.git"]` table. Without them the
host resolves from the git checkout while everything under it resolves from the
local path, two `meristem` crates land in the graph, and the build fails with
`the trait ViewPathTracker is not implemented for GenetCtx` — the same silent
`[patch]`-bypass shape the tinct note in that file already records.

## Receipts

Both existing semantic scenario receipts pass unchanged on the migrated app —
the scenario files were not touched.

```text
$ .\run-scenario.ps1 ..\..\repos\woodshed\scenarios\p4a_occurrence_identity.scn
RESULT ok
P4a: occurrence identity in the Set graph
captured 01_three_occurrences
captured 02_selected_by_identity
captured 03_reordered_same_identity
captured 04_relations_hidden
P4a receipt complete

$ .\run-scenario.ps1 ..\..\repos\woodshed\scenarios\p4b_typed_relations.scn
RESULT ok
P4b: typed relations, multiplicity retained
captured 01_related_multiplicity
authorities: deterministic only, before any practice is recorded
captured 02_staged_from_frontier
P4b receipt complete
```

`02_selected_by_identity.png` was inspected: CSD chrome, the fretboard leaf,
the Related graph swatch, the Set graph with occurrence 2 ringed, and the card
strip all render as before. Interaction by identity (`@data-key=set-card-2`),
`act move-selected-down`, and relation-visibility toggling all still work, so
click routing, the leaf registry, and the app's own state seams survived intact.

`cargo check --workspace` is clean.

## What this proves for G3

Woodshed is consumer one of the host boundary. It uses:

- root creation and the redraw/resize lifecycle — via `run` + `HostOptions`;
- native input and retained-DOM dispatch — entirely the host's, including the
  new pointer/wheel/Tab routing;
- the layout/paint/presentation seam — `relayout` + the paint pass;
- AccessKit synchronization and action dispatch — install-before-show and the
  typed Click/Focus routing;
- the test harness for semantic interaction — `genet-probe` over `HostPointer`.

It needed **no** Pelt concepts, no application trait, no multi-window, and no
command system. The only host API the migration added is `HostPointer`, which
is a routing seam rather than product policy.

Consumer two is `signalman-desktop`. Pelt is then an optional migration, not a
prerequisite.
