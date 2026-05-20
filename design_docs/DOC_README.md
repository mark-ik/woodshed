# design_docs Index

Canonical first-reference document for project documentation. Read this
before any other doc in this directory.

## Project Reference Docs

- [PROJECT_DESCRIPTION.md](PROJECT_DESCRIPTION.md) — Product goals,
  major features, scope. Maintainer-owned.
- [DOC_POLICY.md](DOC_POLICY.md) — Documentation governance.

## Active Plans

- [2026-04-30_initial_plan.md](2026-04-30_initial_plan.md) — Initial
  scaffold and roadmap from theory crate through Iced UI to first
  desktop release.
- [2026-05-15_midi_design.md](2026-05-15_midi_design.md) — MIDI in/out
  design and clock-sync model.
- [2026-05-15_polyphonic_pitch_spike.md](2026-05-15_polyphonic_pitch_spike.md)
  — Spike on polyphonic pitch detection.
- [2026-05-16_song_mode_integration.md](2026-05-16_song_mode_integration.md)
  — Song Mode integration into the practice app. (§3 "UI" superseded by the
  timeline-layers plan below; engine wiring + save format still current.)
- [2026-05-19_song_timeline_layers_plan.md](2026-05-19_song_timeline_layers_plan.md)
  — Bar-quantized layered timeline (section / chord / click lanes) + sampler;
  the "one-person DLR" evolution of the Song tab. Sampler incubates in
  `woodshed-audio` (shared crate Strophe consumes).
- [2026-05-16_xilem_migration_plan.md](2026-05-16_xilem_migration_plan.md)
  — Migration from Iced to Xilem; feature-parity ladder and web/mobile
  follow-on.

## Archive

- `archive_docs/` — retired plans and superseded notes.
- `archive_docs/2026-05-18/2026-05-17_woodshed_daw_plan.md` —
  Original "sibling DAW project under the Woodshed umbrella" plan.
  Superseded same-week: the maintainer chose a separate sibling repo
  (`repos/strophe/`), and the project scope pivoted from "general
  DAW" to a Deeler-inspired collaborative loop recorder. See
  `repos/strophe/design_docs/` for the live plan.

## Working Principles for AI Assistants

These principles apply to AI-assisted work on this project. Update this
section whenever a durable working insight emerges from a session.

- **Theory model is owned**: do not depend on `rust-music-theory` or
  similar upstream theory crates. We need exotic scales, non-tertiary
  chords, and arbitrary tunings; the upstream models do not support
  those, and inheriting their data shape costs more than it saves. Build
  the theory crate from scratch.
- **Pure core, thin shell**: `crates/woodshedding` must remain pure data
  + math — no I/O, no UI, no audio. The app crate consumes it.
- **Desktop first, mobile later**: ship to itch.io / Gumroad for desktop
  before attempting mobile. Iced mobile support is the eventual path;
  contributing to it is part of the project's broader value but does not
  block the music app.
- **Generalize across stringed instruments**: theory model parameterizes
  string count and tuning so bass, ukulele, and banjo fall out for free.
  Do not hard-code 6-string assumptions.
- **Catalog and generators are complementary, not redundant**: the
  catalog answers "what are the well-known tunings?" (preserves cultural
  names, voicing conventions, history); generators answer "what does
  this tuning become under transformation?" (transpose, drop a string,
  apply an interval pattern). The same pattern will apply to scales and
  chords: named-scale catalog + apply-formula-to-root algorithm. Don't
  pick one over the other — they answer different questions.
