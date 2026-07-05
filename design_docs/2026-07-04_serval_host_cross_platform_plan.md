# Serval host: one codebase for desktop + web (mobile downstream)

Woodshed's UI moves from masonry/xilem (the mark-ik/xilem `woodshed-theme`
fork) to **xilem_serval**: serval renders the app, on desktop and in the
browser, from one DOM-shaped view tree. This supersedes the web-vs-native
split in [2026-06-14_web_profile_plan.md](2026-06-14_web_profile_plan.md);
that plan's Tier 0 seams carry forward here.

Status: **decided 2026-07-04 (Mark); planning the migration.**

## Why

- **xilem_serval is a DOM-shaped xilem backend.** It diffs a xilem view tree
  into serval's `ScriptedDom` (the third backend beside masonry and
  `xilem_web`, same `xilem_core`). A Woodshed written against it is already
  web-vocabulary; there is no second view layer to maintain for the browser.
- **The browser path is proven, not projected.** `serval/examples/serval_web_smoke`
  (serval `2422044ad1a`, receipt PASS in Chrome 2026-07-04) renders a
  woodshed-mock UI through the full chain on wasm32/WebGPU: xilem-serval →
  ScriptedDom → serval-layout → PaintList → paint_list_render → netrender →
  canvas. Pills nav, sidebar, fretboard dots, Roboto text via the new
  `serval_layout::register_host_font` seam.
- **Ecosystem direction.** Woodshed becomes serval's second consumer app
  after meerkat, pressuring the HTML lanes (popups, text input, scroll,
  catalogs) the way Strophe pressures the audio layer. The mark-ik/xilem
  fork and its wgpu-28 skew retire.

**Alternative on file: Dioxus 0.7.** One RSX codebase, `dx` bundles
installers for all five targets, hot patching, Dioxus Native (Blitz/wgpu).
Strongest tooling for pure product speed; not chosen because it feeds
nothing back into the Strophos stack and adds a foreign framework. Revisit
trigger: serval churn blocking Woodshed shipping for an extended stretch.

## Shape

- **`woodshedding`** (theory) and **`audio-primitives`**: untouched.
- **`woodshed-audio`**: stays the engine layer, host-agnostic. cpal bumped
  0.15 → 0.18 (2026-07-04, `aaa7cde`); 0.18 ships webaudio + audioworklet
  output backends for the web lane.
- **`woodshed-views`** (new): xilem_serval view fns + the CSS sheets. The
  seed-derived theme engine keeps its OKLCH math and emits CSS (variables +
  sheets) instead of masonry properties. Slate/Ember and the P1-P3 redesign
  language re-express here.
- **`woodshed-serval`** (new host): winit window + netrender present +
  input dispatch, the meerkat main.rs shape (borrow its harness patterns,
  don't import meerkat).
- **`woodshed-web`** (new host): canvas + rAF/resize loop + input
  translation to `ServalAppRunner::dispatch_*`, generalized from
  serval_web_smoke. PWA manifest + service worker when deploy lands.
- **`woodshed-xilem`**: frozen at migration start, deleted at the parity
  cut (S5). Zero users; no parallel maintenance.

Serval/netrender are consumed as git deps (mark-ik remotes) with the usual
gitignored local `paths` override for development. The woodshed workspace
must mirror serval's `[patch.crates-io]` set (stylo, stylo_atoms, taffy,
sonic-rs); this is a standing cost of consuming serval outside its
workspace and the smoke already models it.

## Platform seams (from the web profile plan, still the spine)

- **Audio (W0.1)**: `AudioBackend` trait. Native = cpal 0.18. Web = Web
  Audio output; tuner input via `getUserMedia` + AudioWorklet posting
  Float32Array hops to the wasm pitch detector. The tuner listens in the
  browser; cpal upstream input support is not the gate.
- **Storage (W0.2)**: native = fs (`directories`); web = **OPFS** (matches
  the Mere direction), localStorage acceptable for first settings.
- **Time (W0.3)**: `web-time` pattern, already landed in serval-layout and
  netrender where the smoke hit it.
- **Timers (W0.4)**: the ten tokio interval loops move behind a host
  scheduler seam. The metronome moves to audio-clock lookahead scheduling
  regardless of platform; that is an upgrade on desktop too.
- **MIDI**: native = midir; web = Web MIDI where present (Chromium, Firefox
  108+; Safari never), feature-detected.

## Phases

- **S0 — scaffold.** `woodshed-views` + `woodshed-serval` crates in the
  workspace, serval/netrender git deps resolving, patch mirror in place,
  `woodshed-xilem` still building. *Done when:* a serval host window opens
  and renders a static sheet on Windows.
- **S1 — Stage walking skeleton.** Real `AppState` behind it: pills nav,
  catalog sidebar, fretboard as transform-positioned DOM dots, theme CSS
  from the seed engine. *Done when:* Stage renders from live state and a
  click selects a scale.
- **S2 — interaction spine.** Pointer/key dispatch through the runner;
  dropdowns via xilem_serval `select`; text fields via `styled_text_field`.
  *Done when:* instrument/tuning/root selection, lens switching, and
  keyboard focus traversal work.
- **S3 — engines wired.** cpal playback, tuner, metronome, MIDI on desktop
  through the W0 seams (seams land here even though desktop could bypass
  them). *Done when:* sound + tuner parity with woodshed-xilem on Windows.
- **S4 — screen parity + redesign P4-P6.** Practice, Song timeline,
  Rehearsal, Settings. The outstanding redesign phases (fretboard-layout
  setting, Rehearsal filmstrip + transport deck, Practice recipe tiles)
  land directly in the new stack; they are not built twice.
  *Done when:* every screen exists in woodshed-serval and the redesign
  plan's P4-P6 done-conditions hold there.
- **S5 — parity cut.** Delete `woodshed-xilem`, the xilem fork dep, and
  `xilem_fork_patches.md` (archive). Cross-platform desktop validation on
  the iMac / Fedora / Mint machines. *Done when:* main builds with no
  mark-ik/xilem reference and the app runs on all four desktop targets.
- **W1 — web shell.** rAF/resize/input loop over the smoke's chain; host
  fonts bundled; OPFS settings. *Done when:* the S1 Stage screen runs in
  Chrome with working mouse input.
- **W2 — web audio.** Web Audio output + AudioWorklet tuner behind W0.1;
  metronome on the audio clock. *Done when:* the browser build plays the
  click and the tuner tracks a live mic.
- **W3 — deploy.** cargo + wasm-bindgen + a static host (Pages or
  Cloudflare), release wasm profile, PWA manifest, `cargo-about` license
  aggregation. *Done when:* a public URL serves the app and installs as a
  PWA.
- **M (downstream, not scheduled)** — mobile: a wry/Tauri shell around the
  web build with the native Rust audio core compiled in. Recorded only;
  no work until W3 ships.

## Known risks and gaps

- **Serval churn.** Active concurrent development (shell-partition, paint
  emission). Pin via lock to known-good serval commits; bump deliberately.
- **Serval-side feature gaps Woodshed will surface**: overlay popups for
  the header dropdowns (meerkat has the patterns), IME/text-input depth,
  a11y tree exposure on web (canvas rendering carries no DOM a11y; serval
  has accesskit plumbing, browser exposure is unproven).
- **audio-widgets / xilem-components** stay masonry-based with Strophe as
  their consumer once Woodshed leaves; Woodshed's serval equivalents grow
  fresh in `woodshed-views`. Decide their long-term stewardship in the
  Strophe context, not here.
- **WebGPU reach**: Chrome/Edge/Safari 26/recent Firefox. No DOM fallback;
  acceptable for a practice tool, revisit only if reach data says otherwise.
- **Unpushed dependencies**: the receipt commits (serval `2422044ad1a`,
  netrender `83e4be37a` + `6520d74ed`) exist locally; git-dep consumption
  needs them pushed.

## Findings

- serval engine core (xilem-serval, serval-scripted-dom, serval-layout)
  checks clean on wasm32 from current main; the June 6 P1 pass landed.
- Browser swapchains reject vello's storage-texture write; render into an
  intermediate `STORAGE_BINDING | TEXTURE_BINDING` RGBA8 texture and blit
  (`wgpu::util::TextureBlitter`). Every web shell must do this.
- `std::time::Instant` panics on wasm; `web-time` fixes landed in
  serval-layout and netrender. Audit woodshed code for the same during S3.
- xilem_serval's control set (button, select, slider, radio_group,
  styled_text_field, overlay) already covers most of Woodshed's widget
  needs; the header dropdown work from redesign P3b maps to `overlay_at`.

## Progress

- 2026-07-05: **S0 done.** `woodshed-views` (demo Stage sheet ported from
  the smoke) + `woodshed-serval` (winit host on `serval-winit-host`'s
  `SurfaceHost`: rasterize → acquire → `compose_external_texture` →
  present). Serval/netrender consumed as git deps with the mere-pattern
  local `[patch]` overrides in the gitignored `.cargo/config.toml` (whose
  long-inert xilem-woodshed `paths` override was also fixed: it sat below
  a `[target]` header and parsed as a key of that table). Committed patch
  mirror: stylo/stylo_atoms (servo/stylo rev), taffy + ipc-channel via
  mark-ik/serval. Verified by screenshot: window renders the sheet on
  Windows, colors matching the browser receipt.
  - **Finding: sRGB surfaces double-encode the serval scene.** vello
    writes display-referred bytes; `serval-winit-host::create_surface`
    preferred the sRGB backbuffer, which re-encodes and washes out every
    color. Fixed in serval (`40e5dd92760`, prefer non-srgb). Meerkat uses
    the same path and will darken to true colors on its next build.
  - Note: S0 lays out at physical pixels (no DPI scaling); wire
    `scale_factor` through `rasterize_scaled` during S1.
- 2026-07-04: Plan created. Decision locked: xilem_serval host, Dioxus
  recorded as the alternative. Prior receipts: cpal 0.18 bump (`aaa7cde`),
  serval browser render receipt (serval `2422044ad1a`), netrender wasm
  backend split (`6520d74ed`) + web-time (`83e4be37a`).
