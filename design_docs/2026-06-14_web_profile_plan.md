# Web profile — scope for a browser demo

What it would take to ship Woodshed as a wasm demo on GitHub Pages, and the
decision that gates it. Grounded in a code+web investigation (2026-06-14)
verified against the live fork checkout and `Cargo.lock`.

Status: **superseded 2026-07-04 by
[2026-07-04_serval_host_cross_platform_plan.md](2026-07-04_serval_host_cross_platform_plan.md).**
The path decision came out differently than the A/B/C menu here: Woodshed
moves to a serval host (xilem_serval), which renders the same view tree on
desktop and in the browser (receipt: serval `examples/serval_web_smoke`,
PASS 2026-07-04). The Tier 0 seams below (AudioBackend, storage, Instant,
timers) carry forward unchanged into that plan; the Path A/B analysis is
historical. Constraint updates since writing: the tuner CAN listen in the
browser (getUserMedia + AudioWorklet, no cpal needed), cpal is bumped to
0.18 (webaudio/audioworklet output backends), storage is OPFS, and COOP/COEP
is escapable (coi-serviceworker or a host with real headers).

## The web profile (target + constraints)

The demo must fit a deliberately narrow profile so it can live on stock
GitHub Pages and reach a wide audience:

- **Single-threaded wasm.** No threads, no `SharedArrayBuffer`. GitHub Pages
  cannot set `COOP`/`COEP` headers, so anything needing cross-origin
  isolation is out. (wgpu 29 builds single-threaded on wasm, so this is
  satisfiable.)
- **No filesystem.** Settings move to `localStorage` or are stubbed.
- **No native audio.** `cpal`/`midir` do not target wasm. In-browser audio is
  output-only at best (Web Audio), and the **tuner cannot listen**: there is
  no released web mic-input path in cpal. Web MIDI is Chromium-only.
- **Widest browser reach.** A DOM/SVG renderer degrades gracefully everywhere;
  a WebGPU-only canvas would blank out on Firefox-Linux, Intel-mac Firefox,
  and older Safari.

A demo that meets this profile is primarily a **visual / interaction
showcase** (lenses, themes, the composable surface, the progression grid,
the rehearsal stage), with a metronome possibly audible and the tuner inert.

## The gating decision: which rendering path

| Path | Compiles to wasm today? | Cost | Reach |
|------|------------------------|------|-------|
| **A — native Xilem → wasm** (masonry_winit + winit-web + wgpu + vello on a canvas) | **No (hard compile failure)** | Upstream/framework work | WebGPU-only |
| **B — `xilem_web`** (DOM/SVG reactive layer) | **Yes** | A second view vocabulary + audio rewrite | Widest (DOM) |
| **C — shelve the web demo** | n/a | A screencast/GIF + desktop download link | n/a |

### Why Path A is not feasible now (verified)

The native app cannot even build for `wasm32` on the current fork. This is a
hard compile failure, not a runtime degrade:

- `masonry_winit` imports `masonry_imaging::texture_render` unconditionally,
  but that module (and `imaging_wgpu`/`imaging_vello`/`wgpu`) is `cfg`-gated
  **out** of wasm. A repo-wide grep for `wasm32` in `masonry_winit/src`
  returns zero matches: there is no on-canvas wasm renderer in this fork.
- The run path is native-only: blocking `event_loop.run_app` (web needs async
  `spawn_app`), `pollster::block_on(create_surface)` (wasm wgpu init is
  async-only), and an unconditional native `tokio` multi-thread runtime in
  `xilem/src/app.rs`.
- The renderer Woodshed builds is **vello 0.9 compute = WebGPU-only**. The
  WebGL2-capable `vello_hybrid` is beta in 2026 and is not wired to a window
  surface here (headless only), so even a built demo would be WebGPU-only.

Getting Path A to *build* means landing a real masonry-on-wasm windowed
renderer upstream (vello_hybrid color-attachment to a winit-web canvas +
`spawn_app` + async surface init + a wasm runtime), gated on `vello_hybrid`
maturing. That is Linebender framework work, not a Woodshed-sized task.

**Recommendation: Path B or Path C.** Path A is a research spike to revisit
only when Linebender ships an official masonry-on-wasm windowed example.

## Work to meet the profile

### Tier 0 — cross-cutting seams (needed for ANY web path; also help native)

These are the prerequisites regardless of B vs A, and they are useful on
desktop too. None exist today.

- **W0.1 — AudioBackend trait + a `cfg` seam.** Engines are built eagerly in
  `AppState::new()` and `main.rs` has zero `cfg` splits, so there is nowhere
  to hang a web backend. Introduce a backend trait: native = cpal; wasm = a
  Web-Audio output backend or a no-op stub (silent demo). *Done when:*
  `woodshed-xilem` compiles for `wasm32` with a stub backend, and the native
  build is byte-for-byte behaviourally unchanged.
- **W0.2 — Storage abstraction.** `settings.rs` persists `state.json` via
  `directories::ProjectDirs` + `std::fs`. Abstract it: native = fs; wasm =
  `localStorage`. Save on `visibilitychange`/`beforeunload` on web (Drop will
  not fire reliably in a tab). *Done when:* settings round-trip on both
  targets through one interface.
- **W0.3 — `Instant` shim.** 4 `std::time::Instant::now()` sites (metronome /
  click timing) panic on `wasm32-unknown-unknown`. Swap to `web-time` behind
  the seam. *Done when:* no `std::time::Instant` on the wasm path.
- **W0.4 — Timer rewrite.** 10 `tokio::time::interval` tick loops (~30-60ms:
  metronome, auto-advance, song-follow, tuner) drive the app; `tokio`
  `rt-multi-thread` cannot exist on wasm. Move them to a wasm-compatible
  timer (the `xilem_web` task layer / `gloo-timers` / `requestAnimationFrame`).
  *Done when:* the tick-driven features run on web without `tokio`
  `rt-multi-thread`.

### Tier 1 — Path B (`xilem_web`) specifics

- **W1.1 — Shared view-core.** Factor `AppState` + transition logic out of the
  view fns so one state/logic core feeds two view layers (native masonry, web
  DOM). This is the durable-architecture cost: a `view-core` + `view-native` +
  `view-web` split. *Done when:* the state/logic core has no masonry or
  `xilem_web` dependency and both view layers consume it.
- **W1.2 — Custom widgets as SVG.** Re-express the fretboard, chord diagram,
  beat-wheel, and cents/level meters as `xilem_web` SVG from the same
  `kurbo`/`peniko` geometry. The drawing math transfers; only the paint calls
  swap (canvas stroke/fill to SVG paths). *Done when:* each custom widget
  renders in the browser from shared geometry.
- **W1.3 — DOM view fns** for the tabs, lenses, and composable surface. The
  bulk of the work. Fonts are moot here (browser CSS fonts apply).
- **W1.4 — Web audio (output-only).** Click + chord/scale/song playback via
  Web Audio behind W0.1; tuner stays visual-only; Web MIDI deferred.

### Tier 2 — deploy (last, small once Tier 0/1 compile)

- **W2.1 — `trunk` + `index.html`** (DOM/canvas mount, HiDPI sizing).
- **W2.2 — GitHub Actions `deploy-pages`** (add the `wasm32` target, `trunk`
  build, `actions/deploy-pages`; Pages source = GitHub Actions). Single-thread
  build means **stock Pages works, no COOP/COEP**. The same Actions infra
  hosts the release CI, so build it once.
- **W2.3 — Demo content + framing.** Decide which presets ship; add a landing
  note ("audio is limited in the browser, download the desktop app for the
  tuner and full audio"); aggregate third-party licenses for the bundled fork
  deps (`cargo-about`); state zero telemetry.

## Open decisions (for Mark)

1. **Path B or shelve (Path C)?** Path B means maintaining a second view
   vocabulary forever to demo an app whose audio is mostly off in-browser. If
   that durable cost outweighs the reach, a screencast + desktop download is a
   legitimate, far cheaper "demo." This is the call that gates all of Tier 1/2.
2. **Functional or silent?** Silent (stub audio) only needs Tier 0's seam to
   compile. Output-only adds the Web Audio backend. The tuner is off either
   way. The audio tier is the dominant cost driver.
3. **Is the rest of the xilem/masonry stack a published-crates build or a
   git/path build?** The fork is path-dep'd and the lockfile is already out of
   sync with it (see Findings). A reproducible demo build needs this settled.

## Findings

- Verified versions in `Cargo.lock` at investigation time: wgpu 29.0.3, vello
  0.9.0, winit 0.30.13, parley 0.10.0, tokio 1.52.3, cpal 0.15.3, midir
  0.11.0, directories 5.0.1.
- `xilem_web` in the fork depends only on `futures`/`peniko`/`kurbo`/
  `wasm-bindgen`/`web-sys`/`xilem_core` — zero masonry/wgpu/vello, so it
  inherits none of Path A's blockers.
- The committed `Cargo.lock` (parley/fontique 0.9) is **stale** versus the
  fork checkout, which now resolves parley/fontique/parley_data 0.10 and
  harfrust 0.8; a plain `cargo build` force-syncs the lock. The fork has moved
  past the parley-0.9 pin recorded in `xilem_fork_patches.md`. Decide whether
  to commit the synced lock as a release-hygiene step.

## Progress

- 2026-06-14: Plan created from the release+demo readiness investigation.
  Path A ruled out (hard wasm compile failure on the current fork); Path B
  (`xilem_web`) or Path C (shelve) recommended. Tier 0 seams identified as the
  shared prerequisite. Decision on Path B-vs-shelve and functional-vs-silent
  pending.
