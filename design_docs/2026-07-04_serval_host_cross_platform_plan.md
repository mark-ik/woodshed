# Genet host: one codebase for desktop + web (mobile downstream)

Woodshed's UI moves from masonry/xilem (the mark-ik/xilem `woodshed-theme`
fork) to **xilem_serval**: genet renders the app, on desktop and in the
browser, from one DOM-shaped view tree. This supersedes the web-vs-native
split in [2026-06-14_web_profile_plan.md](2026-06-14_web_profile_plan.md);
that plan's Tier 0 seams carry forward here.

Status: **active desktop-alpha hardening.** The Woodshed-side Genet migration
and S5 parity cut are complete; browser, cross-desktop receipts, and publishing
remain separate release work.

## Why

- **xilem_serval is a DOM-shaped xilem backend.** It diffs a xilem view tree
  into genet's `ScriptedDom` (the third backend beside masonry and
  `xilem_web`, same `xilem_core`). A Woodshed written against it is already
  web-vocabulary; there is no second view layer to maintain for the browser.
- **The browser path is proven, not projected.** `genet/examples/genet_web_smoke`
  (genet `2422044ad1a`, receipt PASS in Chrome 2026-07-04) renders a
  woodshed-mock UI through the full chain on wasm32/WebGPU: xilem-serval →
  ScriptedDom → genet-layout → PaintList → paint_list_render → netrender →
  canvas. Pills nav, sidebar, fretboard dots, Roboto text via the new
  `genet_layout::register_host_font` seam.
- **Ecosystem direction.** Woodshed becomes genet's second consumer app
  after meerkat, pressuring the HTML lanes (popups, text input, scroll,
  catalogs) the way Strophe pressures the audio layer. The mark-ik/xilem
  fork and its wgpu-28 skew retire.

**Alternative on file: Dioxus 0.7.** One RSX codebase, `dx` bundles
installers for all five targets, hot patching, Dioxus Native (Blitz/wgpu).
Strongest tooling for pure product speed; not chosen because it feeds
nothing back into the Merely stack and adds a foreign framework. Revisit
trigger: genet churn blocking Woodshed shipping for an extended stretch.

## Shape

- **`woodshedding`** (theory) and **`audio-primitives`**: untouched.
- **`woodshed-audio`**: stays the engine layer, host-agnostic. cpal bumped
  0.15 → 0.18 (2026-07-04, `aaa7cde`); 0.18 ships webaudio + audioworklet
  output backends for the web lane.
- **`woodshed-views`** (new): xilem_serval view fns + the CSS sheets. The
  seed-derived theme engine keeps its OKLCH math and emits CSS (variables +
  sheets) instead of masonry properties. Slate/Ember and the P1-P3 redesign
  language re-express here.
- **`woodshed-genet`** (new host): winit window + netrender present +
  input dispatch, the meerkat main.rs shape (borrow its harness patterns,
  don't import meerkat).
- **`woodshed-web`** (new host): canvas + rAF/resize loop + input
  translation to `GenetAppRunner::dispatch_*`, generalized from
  genet_web_smoke. PWA manifest + service worker when deploy lands.
- **`woodshed-xilem`**: frozen at migration start, deleted at the parity
  cut (S5). Zero users; no parallel maintenance.

Genet/netrender are consumed as git deps (mark-ik remotes) with the usual
gitignored local `paths` override for development. The woodshed workspace
must mirror genet's `[patch.crates-io]` set (stylo, stylo_atoms, taffy,
sonic-rs); this is a standing cost of consuming genet outside its
workspace and the smoke already models it.

## Platform seams (from the web profile plan, still the spine)

- **Audio (W0.1)**: `AudioBackend` trait. Native = cpal 0.18. Web = Web
  Audio output; tuner input via `getUserMedia` + AudioWorklet posting
  Float32Array hops to the wasm pitch detector. The tuner listens in the
  browser; cpal upstream input support is not the gate.
- **Storage (W0.2)**: native = fs (`directories`); web = **OPFS** (matches
  the Mere direction), localStorage acceptable for first settings.
- **Time (W0.3)**: `web-time` pattern, already landed in genet-layout and
  netrender where the smoke hit it.
- **Timers (W0.4)**: the ten tokio interval loops move behind a host
  scheduler seam. The metronome moves to audio-clock lookahead scheduling
  regardless of platform; that is an upgrade on desktop too.
- **MIDI**: native = midir; web = Web MIDI where present (Chromium, Firefox
  108+; Safari never), feature-detected.

## Phases

- **S0 — scaffold.** `woodshed-views` + `woodshed-genet` crates in the
  workspace, genet/netrender git deps resolving, patch mirror in place,
  `woodshed-xilem` still building. *Done when:* a genet host window opens
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
  *Done when:* every screen exists in woodshed-genet and the redesign
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

- **Genet churn.** Active concurrent development (shell-partition, paint
  emission). Pin via lock to known-good genet commits; bump deliberately.
- **Genet-side feature gaps Woodshed will surface**: overlay popups for
  the header dropdowns (meerkat has the patterns), IME/text-input depth,
  a11y tree exposure on web (canvas rendering carries no DOM a11y; genet
  has accesskit plumbing, browser exposure is unproven).
- **audio-widgets / xilem-components** stay masonry-based with Strophe as
  their consumer once Woodshed leaves; Woodshed's genet equivalents grow
  fresh in `woodshed-views`. Decide their long-term stewardship in the
  Strophe context, not here.
- **WebGPU reach**: Chrome/Edge/Safari 26/recent Firefox. No DOM fallback;
  acceptable for a practice tool, revisit only if reach data says otherwise.
- **Unpushed dependencies**: the receipt commits (genet `2422044ad1a`,
  netrender `83e4be37a` + `6520d74ed`) exist locally; git-dep consumption
  needs them pushed.

## Findings

- genet engine core (xilem-serval, genet-scripted-dom, genet-layout)
  checks clean on wasm32 from current main; the June 6 P1 pass landed.
- Browser swapchains reject vello's storage-texture write; render into an
  intermediate `STORAGE_BINDING | TEXTURE_BINDING` RGBA8 texture and blit
  (`wgpu::util::TextureBlitter`). Every web shell must do this.
- `std::time::Instant` panics on wasm; `web-time` fixes landed in
  genet-layout and netrender. Audit woodshed code for the same during S3.
- xilem_serval's control set (button, select, slider, radio_group,
  styled_text_field, overlay) already covers most of Woodshed's widget
  needs; the header dropdown work from redesign P3b maps to `overlay_at`.

## Progress

- 2026-07-10: **Windows-alpha release surface.** Added Windows CI (core tests
  plus host check), a tag-triggered `v*` release workflow, and a manual
  artifact path. `scripts/package-windows.ps1` packages the optimized host as
  a portable `Woodshed-<version>-windows-x86_64.zip`, includes the project
  licenses, README, release instructions, and Cargo.lock inventory, and emits
  a SHA-256 sidecar. It honors `CARGO_TARGET_DIR` for local builds. The first
  exercised package was `0.1.0-alpha.1`: 25.8 MB executable, 9.9 MB ZIP, and
  the expected six package entries. This is intentionally not an installer:
  application icon, signing/SmartScreen reputation, and third-party notice
  aggregation remain before a broadly advertised release. Refreshed the
  maintainer-owned `PROJECT_DESCRIPTION.md` at the same time so the canonical
  product description no longer claims the retired Iced stack or an already
  shipped web/mobile product.
- 2026-07-09: **Adaptive-shell foundation.** CSD titlebar controls and their
  desktop-only drag/resize behavior moved out of the shared product root and
  into `woodshed-genet`; a future browser host therefore composes the same
  app content without inheriting unusable window controls. The desktop host now
  derives Wide / Medium / Narrow width bands from logical window width. Below
  1180px the Stage catalog moves below the neck; below 760px navigation wraps,
  controls enlarge, the fretboard scrolls rather than compressing, and recipes
  become a single column. Native startup dimensions now clamp to the primary
  monitor with a 24px margin instead of assuming a 1100px-wide display. This
  is the adaptable-shell seam, not yet a web host or a mobile layout receipt.
- 2026-07-08: **S5 parity cut (woodshed half) — woodshed-xilem deleted.**
  The old Masonry app is gone (`git rm`); woodshed-genet is now THE
  woodshed app. The mark-ik/xilem fork stays for now: audio-widgets +
  xilem-components (Masonry-coupled shared crates) are still consumed by
  Strophe, and because they're woodshed members their `workspace = true`
  fork deps resolve against woodshed's workspace — so the fork can't leave
  woodshed without relocating them, and Strophe still needs it. Full
  retirement is a family-wide move: refactor Strophe onto `xilem_serval`
  and reincarnate the widget work as chisel leaves (audio-widgets is pure
  custom paint — a canonical chisel-leaf fit; xilem-components is mostly
  reactive structure that `xilem_serval` does natively, so it dissolves).
  Deferred to that Strophe refactor. Manifest housekeeping: synced
  woodshed's `[patch.crates-io] stylo`/`stylo_atoms` mirror to genet's
  fork (`mark-ik/stylo`, `mark-ik/servo-media-features` branch — genet
  moved stylo there); build woodshed **from its own cwd** so the
  local-genet `[patch]` applies (Code-root + `--manifest-path` falls back
  to the pinned git genet, which lacks the newer transition API).
- 2026-07-08: **Polish tail — window fixes, CSS transitions, Hero receipt.**
  - Right-edge clip fixed (`.root` was `width:100%` + 32px padding in
    content-box, so it rendered wider than the window and the search field
    and CSD × spilled off the right and clipped): `box-sizing: border-box`.
    The off-screen × had also caused a mis-aimed click to hit another window.
  - Resize-arrow cursors on border hover (undecorated CSD windows get none
    from the OS): `edge_cursor` + `update_resize_cursor`, 8px grab margin.
    Shipped with the clip fix + a default-window shrink (720→664 logical,
    top-anchored, so it clears the taskbar on a 720-tall laptop screen) as
    `c7044fe`.
  - CSS transitions on genet's new `transition_events` / `tick_animations`:
    subtle hover/active fades on buttons, tabs, lenses, and cards. The host
    ticks genet's animation clock each frame (in redraw before `apply`,
    and in hover before `set_interaction`, so a flip runs from *now* not a
    stale idle clock) and requests frames only while
    `has_active_animations()`, so idle surfaces stay idle.
  - Hero-layout receipt captured (fretboard full-width on top, catalog a
    wrapping strip beneath).
- 2026-07-07: **S4 slice 15 — the looper (audio depth IV).** Record your
  part into a song bar as it loops past — overdub or replace — and clear
  it. The Song screen gains a Rec / Overdub-Replace / Clear-loop row and
  a ⟳ marker on looped bars. The song engine already recorded live input
  into per-bar audio buffers (draining the looper-capture ring while
  `recording`); this slice wires that ring to the input engine (the same
  onset+capture registration slice 14 added), exposes arm / stop / clear
  / mode plus recording-state + per-bar-loop reflection through the audio
  seam, and drives it from the host: arm the edit-cursor bar
  (`PendingChange::StartRecording` + enable capture); on the SR-16 model
  it records when the loop reaches the armed bar. Recorded loops now
  survive song edits (carried across `set_song` by bar index). Also
  removed a pre-existing per-frame chord-cache resync during playback
  (read-only `SongEngineHandle` accessors instead of `with_song`).
  Receipt: the Song screen renders the loop controls over a 6-bar
  progression; build green. Recording itself is Mark's to confirm with a
  mic.
- 2026-07-07: **S4 slice 14 — latency calibration (audio depth III).**
  Measure the real input→output round-trip lag: play a lead of clicks,
  tap along, read the number. A Settings panel (Calibrate → "Tap along
  N/M" → Accept / Retry) drives the built-and-tested `CalibrationSession`
  (correlates click times against detected onsets, takes the median).
  The input engine now registers the onset analyzer (plus the
  looper-capture analyzer, shared with slice 15) beside pitch;
  calibration owns the metronome engine during a run (the host gates
  `set_metronome`) and restores the click after. New `CalibrationStatus`
  plus five `AudioBackend` methods; the accepted latency is stored as the
  active compensation (the onset-timing feedback widget that consumes it
  is the natural follow-up). Receipt: the panel renders in Settings
  ("Latency: uncalibrated" / Calibrate); build green, 31 core + 126 audio
  tests pass. The measurement itself is Mark's to run with a mic.
- 2026-07-07: **S4 slice 13 — MIDI in/out (audio depth II).** Woodshed
  talks to your rig. A MIDI panel in Settings: input/output port pickers,
  "Sync to clock" (slave the transport BPM to an incoming MIDI clock),
  "Send clock" (emit 24-PPQN clock + Start/Stop so external gear follows
  Woodshed's tempo), a status line + live event readout, and Refresh.
  - The engine layer (`woodshed_audio::midi` — byte parse, clock-sync
    BPM derivation, `MidiIn`/`MidiOut` over `midir`) was built + tested
    but never consumed by any app (the old xilem app only ever mentioned
    "future MIDI pickers"); this wires it. New
    `woodshed_core::midi::MidiBackend` seam (parallel to
    AudioBackend/Storage); desktop `MidiHost` over `midir`. Input is a
    connected `MidiIn` (its reader thread queues events + the clock-sync
    derives BPM, host-polled); output is a dedicated clock-generator
    thread that owns the connection, so no `midir` handle crosses a
    thread boundary. The host polls clock BPM in redraw (slaving the
    transport and pushing `set_metronome`), reflects the event readout,
    and connects per the dropdowns after dispatch. Transient (not
    persisted — port availability is session-dependent). Receipt: the
    panel renders on a no-MIDI machine (In/Out "None", "no clock"); build
    green, 126 audio + 31 core tests pass. The clock sync itself is
    Mark's to confirm with gear.
- 2026-07-07: **S4 slice 12 — hear the theory (audio depth I).** The
  fretboard now sounds. A "♪ Hear" button on the Stage transport row
  voices the active lens's material; the same button on the Rehearsal
  deck voices the current card. Running an arpeggio or exercise sonifies
  each step as the transport lands on it (the arpeggio climbs audibly);
  a running rehearsal voices each card as it comes up.
  - **The engine already had most of it.** `woodshed-audio` carried a
    complete one-shot voice path (`SongEngineHandle::play_note_now`,
    `oneshot_voices`/`oneshot_clock`, mixed while the song is stopped)
    plus `chord_audio::render_chord` (additive synth, strum/block, ADSR)
    — but the genet host's thin `AudioBackend` seam only exposed
    metronome + tuner + song, so none of it was reachable. This slice
    widens the seam, it doesn't write DSP.
  - Engine: added `play_chord_now(pitches, dur, strum_ms)` beside
    `play_note_now` (strum offset = block / gentle strum / arpeggiated
    cascade), factoring the shared one-shot push. 2 tests.
  - Core seam: `AudioBackend::preview_pitches` + `preview_note` (default
    no-op, so a backend that can't voice stays silent — the web host
    fills these over Web Audio later). Core `StageState` gained
    `voicing_pitches` / `voicing_preview` / `card_voicing` (chord+scale
    tones via `ScaleFormula`/`ChordFormula::apply_to` → `Pitch::frequency`)
    and `arpeggio_current_pitch_hz` / `exercise_current_pitch_hz` for the
    step-through. 6 core tests (31 total green).
  - Host: `CpalBackend` forwards the two preview calls to the song
    engine; `main.rs` sonifies the arpeggio/exercise step on advance and
    voices the rehearsal card on land (redraw), and consumes a
    `preview_requested` flag after dispatch (the same host-flag idiom as
    rewind / chrome). Views add the "♪ Hear" control (accent-tinted, no
    toolbar reorg) + `UiState::preview_voicing` (Rehearsal card vs. Stage
    lens by tab). Receipt: Stage renders with ♪ Hear inline on the
    transport row, launches clean (no panic). Audible confirmation is
    Mark's by ear — click ♪ Hear or run an arpeggio.
  - Deferred (audio depth II+): MIDI in/out, calibration/timing, the
    bar-aligned looper — all still built-and-tested in `woodshed-audio`,
    waiting on the same seam-widening pattern.
- 2026-07-06: **S4 slice 11 — corpus search field (genet text input
  dogfood).** A small always-on search field in the nav row (right of
  the tabs, no toolbar reorg). `woodshed_core::search::search_corpus`
  ranks one query (exact → prefix → word-start → substring) across every
  named catalog: scales, chords, arpeggios, progressions, exercises,
  practice recipes, tunings (`SearchHit` names the target). The view is
  xilem_serval's real `text_field` over a `TextInput` in `UiState`;
  results drop down as a positioned overlay; clicking one runs
  `apply_search_hit` (jump to the lens/tab, or fill the set for a
  recipe, or swap the tuning) and clears the field. Driven receipts:
  typed "dorian" → ranked list (exact Dorian first) → clicked → landed
  on Stage/Scales/A Dorian; "caged" → three recipe hits. 5 core tests.
  - **Genet dogfood outcomes**: real text input works end to end
    (caret, key routing to the focused field, focus-on-click). Two
    engine-behavior lessons for any genet UI: `<input>` is `inline` in
    genet's UA sheet (Stylo default) so it needs `display: block` to
    take a width (meerkat sets this too); and `width` is content-box, so
    a padded field wants `box-sizing: border-box` or its border-box
    overflows the wrap (was clipping at the window edge). Both are host
    CSS, not engine gaps.
  - Note: the CSD chrome (our own title/buttons, no OS red frame) already
    shipped in slice 8 — visible in every screenshot since. The frame is
    already ours; further chrome refinement stays in the polish tail.
- 2026-07-06: **S4 slice 10 — the song editor.** The song lane is now a
  first-class core `SongDoc { name, bars, one_shot, click }` (replacing
  the split `song_name`/`song_bars` fields, which also un-grew the
  `capture` arg list). `SongBar` gained `root_pc` + the edit verbs
  (`revoice`, `cycle_root`/`cycle_formula`/`toggle_silent`, `nudge_bpm`,
  `cycle_beats`, `cycle_length`); `SongDoc` the structural verbs
  (`add_bar_after`/`duplicate`/`remove`/`move_bar`). The Song tab:
  transport deck (adds Once/Loop + Click toggles), bar-ops row (+ Bar /
  Dup / Remove / Move ◀▶), selectable timeline chips (edit cursor ringed
  tertiary, play cursor secondary), and a per-bar editor row (root,
  chord, silent, tempo ±, meter, length, section-label cycle). Bars are
  built from scratch OR laid down from a progression; the whole doc
  persists. Driven receipt: three bars added from empty, bar 1 edited to
  a D Minor "Verse" (chip + editor updated live), song survived
  relaunch (persist), Play voiced it (deck showed Stop). Backend
  `set_song(&SongDoc)` now carries `one_shot`/`click` into the engine.
  - Bug caught + fixed in-slice: the empty-state hid the `+ Bar` button
    (it lived only in the non-empty ops row) — the placeholder pointed
    at a control that wasn't there; the ops row now shows on empty too.
- 2026-07-06: **S4 slice 9 — the card editor + dwell transport.**
  Rehearsal's editor strip edits the cursor card in place (persisted
  with the set): touch cycle (block → arp up-down → up → down), hold
  cycle (manual → 2/4/8 bars → 30s), per-card tempo override (card bpm
  wins over transport), pinned fret-window nudge + free. Core:
  `dots_for_card` honors the window (first Setting-fidelity piece);
  `card_dwell` maps Hold to wall-clock. The rehearsal deck gains
  Run/Pause: the host advances on each card's own dwell, parks on
  manual cards, and stops at the end when loop is off. Driven receipt:
  2-bar hold at 120 auto-advanced to card 2/12 at the 4s mark, played
  card dimmed, board re-resolved with the window applied, transport
  parked on the next card's manual hold.
  - **Workspace lesson (cost an hour): never use cargo's global
    `paths` override.** It matches by package NAME graph-wide, so the
    xilem fork's `xilem_core` hijacked genet's vendored `xilem_core`
    (surfaced when a concurrent genet edit extended a trait). A
    source-keyed `[patch]` can't express it either (two path-sourced
    same-version packages collide in the lock). Resolution: the
    dormant xilem fork rides its git dep; only the active genet
    family keeps local overrides.
- 2026-07-05: **S4 slice 8 — P4 fretboard layouts + CSD chrome + polish
  batch.**
  - **P4**: `BoardLayout` (Two pane / Hero / Full canvas) as a Settings
    picker, persisted. Structure branches in `stage_screen`; sizing
    rides descendant CSS off a root class (`.layout-canvas .dot {...}`),
    so every lens board gets the treatment without touching the grid
    renderers. All three render the same resolved positions. Driven:
    Full canvas (sidebar gone, 36px dots); Hero's branch shares the
    machinery but hasn't been screenshot-driven yet.
  - **CSD chrome**: undecorated window; our own chrome row (title,
    drag surface, minimize / maximize-toggle / close with danger
    hover), host consumes request flags after dispatch
    (`drag_window` while the press is live), 8-direction edge resize
    with a 6px grab margin. The old red OS title bar is gone.
  - **Polish**: dot labels vertically centered (line-height); Escape
    closes open selects; `:focus` styling wired (focused travels in
    the InteractionState with hover, refreshed after Tab).
  - Known nits: the × button clips slightly at the right edge
    (chrome row padding); chrome buttons + drag + edge resize not yet
    synthetically driven; Hero layout needs its receipt.
- 2026-07-05: **S4 slice 7 — Song tab: timeline + engine playback. The
  tab row is complete (no placeholders left).**
  `woodshed_core::song::SongBar` is the neutral bar DTO (display
  strings + pre-computed chord-tone frequencies, so backends stay
  theory-free — the ChordRef posture); `song_from_progression`
  materializes the selected progression as one labeled bar block per
  chord ("Send to Song"). The audio seam grows song methods
  (set_song / transport / rewind / live bar cursor); CpalBackend
  converts to `woodshed_audio::Song` and runs the real `SongEngine`
  (third cpal stream). The Song tab: transport deck (Play/Stop,
  Rewind, From progression), bar-chip timeline (numeral, chord, bpm)
  with the current chip ringed by the engine's live cursor on the
  animation chain. Song name + bars persist. Driven receipt: I-IV-V
  in A laid onto the timeline, playing, captured at bar 2/3 with the
  IV/D chip highlighted (cursor math checks: 3.2s at 120bpm 4/4).
  Deferred (the song editor deep end): add/remove/reorder bars,
  per-bar chord/tempo/meter editing, sections, one-shot mode toggle,
  click toggle, bar recording/looper — tracked for a dedicated
  editing slice.
- 2026-07-05: **S4 slice 6 — Practice tab (P6 recipe tiles) + `:hover`
  via a new engine feature.**
  - **Engine: `IncrementalLayout::set_interaction`** (genet
    `b4e0edc051f`) — the cascade had `restyle_for_interaction` but the
    retained session had no way to reach it, so no host had ever wired
    `:hover`. The new method lands on the same paths as `apply`
    (RepaintOnly for color-tier rules, full relayout for geometry,
    Unchanged when nothing matched); woodshed-genet is the first
    consumer (CursorMoved → hit test → restyle on target change).
    Hover rules added across the app's sheets.
  - **Practice tab**: the P6 treatment — `woodshedding::practice`
    recipes as a CSS **grid** of tiles (name, blurb, card count),
    one tap fills the rehearsal set (`set_from_practice`: cards with
    PracticeSet provenance + pinned fret windows, loop-all) and jumps
    to Rehearsal.
  - Driven receipts: 3-column grid rendered first try (taffy grid
    through the cascade); hovered tile shows its gold border while
    others stay flat; one tap on "Major — all 12 keys" produced 12
    provenance-stamped cards in Rehearsal with the filmstrip
    overflowing (wheel-scroll receipt over the overflowing strip still
    to drive).
- 2026-07-05: **S4 slice 5 — Rehearsal tab: R1 material portability +
  the P5 filmstrip.** "+ Rehearse" on the Stage builds a Card from any
  lens (`StageState::card_from_lens`: scale/chord material, arpeggio
  touch, progression + exercise recipe provenance) into a
  `woodshedding::rehearsal::Set` that persists with the session. The
  Rehearsal tab is the redesign-P5 treatment: measured filmstrip (tag
  badge, label, touch, provenance; played cards dim behind the cursor
  via engine group opacity; current card ringed), transport deck
  (Prev/Next honoring LoopMode, Remove, count), the cursor card's
  material resolved on the big board. Host gains wheel scrolling
  (`scroll_at` + element-scroll carry across rebuilds). Driven
  receipt: four cards from four lenses, relaunch restored the set,
  Next-Next walked to 3/4 with cards 1-2 dimmed.
  - Pressure-cooker outcome: group opacity, outset box-shadow,
    border+radius cards, and overflow scroll all worked first try; no
    new engine bugs this slice. Host-side gaps noted instead: hover
    styling (engine has `apply_interaction` /
    `restyle_for_interaction` — the host never calls them), cursor
    shape changes, and CSS transitions (engine gap; dim/ring changes
    snap). Card-editor fidelity (Setting capo/window/voicing, Timing
    dwell auto-advance, per-card sound) deferred to the next
    Rehearsal slice.
- 2026-07-05: **S4 slice 4 — persistence (W0.2) + Settings tab + tab
  nav + all six themes.** `woodshed_core::storage` defines the seam
  (`Storage` trait + `PersistedSession`, serde with
  `#[serde(default)]` forward-compat and clamping restore) and the
  `Tab` enum; the desktop host persists to `genet-state.json` beside
  the xilem app's `state.json` (same ProjectDirs, distinct file — no
  clobbering during coexistence), saving after every dispatch and
  restoring at boot. Pills nav is real tab switching (Practice / Song
  / Rehearsal are honest placeholders). Settings ships the theme
  picker: all six seed sets (Slate, Ember, Light, Dusk, Meadow,
  Parchment) ported into `woodshed-views::theme::ThemeMode`, derived
  live through tinct, re-skinning on click (sheet regen + forced
  relayout). Driven receipt: Ember picked in Settings, app killed and
  relaunched — came back in Ember on the Settings tab with the pick
  highlighted. This is redesign P1 parity in the new stack.
- 2026-07-05: **S4 slice 3 — Exercise lens migrated; the lens strip is
  complete.** All generation lives in the theory crate
  (`Exercise::generate` over tuning + `ExerciseParams`); core adds the
  sequence-aware board (current step + fading trail of
  `EXERCISE_TRAIL` steps, newest-wins on collisions, fingering labels)
  and the four-fret hand-position nudge with clamping. The host beat
  clock now advances whichever step transport is playing. All five
  lenses resolve; the placeholder path is deleted. Driven receipt:
  Chromatic 1-2-3-4 running at step 7/48 with the trail exactly
  matching the generator (string-5 fingers 1-2 behind the current 3,
  string-6 finger 4 from the prior pass). Direction/trill params and
  per-step audio deferred with the S3 audio deepening.
  S4 remaining: Practice / Song / Rehearsal / Settings tabs,
  persistence (W0.2), redesign P4-P6.
- 2026-07-05: **S4 slice 2 — Progression lens migrated.** The theory
  crate already owned the hard part (`Progression::apply_in_key`
  materializes roman-numeral roles in the shared root's major key,
  matching woodshed-xilem); core adds `progression_board()` (cards +
  expanded chord's tones) and a ported `format_role` that now includes
  the degree-alteration prefix the old app dropped (♭VII renders as
  ♭VII). Views: catalog sidebar, chord-card strip (numeral in tertiary,
  concrete chord below, expanded card in `surface_hover`), chord-tone
  board, description caption. Cold start prompts until a pick. Driven
  receipt: I-IV-V in A → IV card expands → "showing D (IV)" with D-F#-A
  on the board. Deferred with the voicing work: per-chord voicing
  browser, overlay-all-voicings mode, per-chord hues.
  Cosmetic: dot labels clip slightly at 10px in 24px dots — take with
  the board polish pass.
- 2026-07-05: **S4 slice 1 — Arpeggio lens migrated.**
  `woodshed_core::arpeggio` ports the woodshed-xilem algorithm verbatim
  (bass-anchored CAGED shape generation, pitch-ascending run from the
  inversion's bass tone, up/down/ping-pong walk without turnaround
  repeats), unit-tested (shapes windowed, run ascends from bass, UpDown
  walk = 2n-2). StageState grows the arpeggio fields +
  `arpeggio_board()`; the view adds the deck (Run/Pause, Step, direction
  cycler, inversion cycler, shape ‹ n/m ›) and the step-dot highlight in
  `secondary`; the host advances the transport on the redraw chain at the
  transport bpm (still the W0.4 stand-in; the shared beat grid /
  metronome phase-lock from the old app is deferred with the sound-per-
  step voice). Driven receipt: Run at 120 bpm, step counter advancing
  (5/10 → 7/10 across 0.7 s) with the highlight walking the run;
  ping-pong turnaround verified against the walk math.
- 2026-07-05: **S3 spine done — engines through the W0.1 seam.**
  `woodshed_core::audio` defines the seam (`AudioBackend` trait +
  `TransportState`/`TunerState`/`TunerReading`, pure data);
  `woodshed-genet::audio::CpalBackend` realizes it over woodshed-audio's
  `SequencerEngine` (4/4 `Sound::click()` pattern) and
  `InputEngineBuilder::with_pitch`, degrading to an in-UI error string
  when devices are missing. Transport row (Play/Stop, ±5 bpm, Tuner
  toggle + readout) drives it; the host pushes state through the seam
  after every dispatch and polls tuner snapshots on a self-chaining
  redraw while listening (desktop's W0.4 stand-in).
  Driven receipt: Play → Stop at 130 bpm after two nudges, tuner
  listening with no reading in a quiet room (honest levels, no placebo).
  Audible click confirmed by Mark (2026-07-05) — sound-out through the
  seam is real end to end.
  Remaining for full S3/S4 parity, tracked for the screen migrations:
  chord/scale render playback, MIDI in/out, song engine, calibration,
  metronome patterns beyond straight quarters.
- 2026-07-05: **S2 done — dropdowns, lens strip, keyboard.** Root/tuning
  as xilem_serval `select` overlays in the header; lens strip (Scale /
  Chord / Arpeggio / Progression / Exercise) with Scales + Chords lenses
  resolving on the board (`positions_for_chord`) and the other three as
  S4 placeholders; Tab traversal + Enter activation through
  `focus_traverse` / `dispatch_key` with the `genet-winit-host` key
  mapping; incremental `apply` for attribute-only mutation batches.
  Driven receipts: Chord lens by mouse and by Tab-Tab-Enter, Root → C
  through the overlay ("C Major — 21 positions", C-E-G).
  - **Theming now rides `tinct`** (the pure OKLCH engine, repo dir
    `tincture`) — Slate seeds → `derive_palette` → CSS. The
    audio-widgets extraction question is closed; audio-widgets can
    itself migrate to tinct later if wanted.
  - **Genet engine fix #3, found by the dropdown:** hit-testing walked
    DOM order, so the open select overlay lost clicks to the in-flow
    sidebar behind it (clicks went *through* the popup). Fixed in
    genet-layout by lifting positioned subtrees over the whole in-flow
    hit walk, mirroring paint's plane split (two commits: sibling-level
    reorder `d15455130a1` proved insufficient for the cross-subtree
    shape; the deferred-queue lift `db0c9751d81` fixes it; regression
    tests for both shapes). Same approximation tier as paint: no z-index
    bucket sort, no negative-z.
  - Polish deferred: focus-ring styling (traversal is functional but
    invisible), select caret glyph, `Escape` closing an open list.
- 2026-07-05: **S1 done — Stage renders from live state and a click selects
  a scale** (screenshots: `woodshed-genet-s1-initial.png` /
  `-clicked.png`; synthetic click on "Major" moved the selection, the
  caption updated to "A Major — 47 positions", and the dots re-spelled to
  sharps from the theory crate).
  - New `woodshed-core` crate (the W1.1 split): `StageState` over
    `woodshedding` (tuning + root + scale, `dots()` via
    `Fretboard::positions_for_scale`), unit-tested. The plan's Shape gains
    this crate; the remaining lenses migrate into it during S4.
  - `woodshed-views`: `stage` module over live state (`clickable` sidebar
    mutating `StageState`) + `theme` module. Theme S1 carries the DERIVED
    Slate palette verbatim (probed from `audio-widgets`'
    `derive_palette`); porting the OKLCH engine to a pure crate (so
    Ember/user themes work without masonry) is an open follow-up — it
    touches the Strophe-shared `audio-widgets`, so it wants its own call.
  - Host: DPI-aware (`IncrementalLayout` at logical size,
    `rasterize_scaled` at physical), retained layout hit-testing, click
    dispatch through `GenetAppRunner::dispatch_click`. Layout is rebuilt
    per frame (fine at this scale); incremental `apply` lands with S2.
- 2026-07-05: **S0 done.** `woodshed-views` (demo Stage sheet ported from
  the smoke) + `woodshed-genet` (winit host on `genet-winit-host`'s
  `SurfaceHost`: rasterize → acquire → `compose_external_texture` →
  present). Genet/netrender consumed as git deps with the mere-pattern
  local `[patch]` overrides in the gitignored `.cargo/config.toml` (whose
  long-inert xilem-woodshed `paths` override was also fixed: it sat below
  a `[target]` header and parsed as a key of that table). Committed patch
  mirror: stylo/stylo_atoms (servo/stylo rev), taffy + ipc-channel via
  mark-ik/genet. Verified by screenshot: window renders the sheet on
  Windows, colors matching the browser receipt.
  - **Finding: sRGB surfaces double-encode the genet scene.** vello
    writes display-referred bytes; `genet-winit-host::create_surface`
    preferred the sRGB backbuffer, which re-encodes and washes out every
    color. Fixed in genet (`40e5dd92760`, prefer non-srgb). Meerkat uses
    the same path and will darken to true colors on its next build.
  - Note: S0 lays out at physical pixels (no DPI scaling); wire
    `scale_factor` through `rasterize_scaled` during S1.
- 2026-07-04: Plan created. Decision locked: xilem_serval host, Dioxus
  recorded as the alternative. Prior receipts: cpal 0.18 bump (`aaa7cde`),
  genet browser render receipt (genet `2422044ad1a`), netrender wasm
  backend split (`6520d74ed`) + web-time (`83e4be37a`).
