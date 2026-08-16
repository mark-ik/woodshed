# CLAUDE.md — Woodshed Repository Role

This file defines how Claude Code should behave in this repository. Read
it first when starting any session.

---

## Project Identity

**Woodshed** is an open-source practice toolkit for guitar and other
fretted instruments: theory catalogs staged into an ordered practice Set,
Rehearsal and Looper consuming that Set, plus a fretboard, tuner, metronome,
and MIDI clock. The theory model generalizes across string counts and
tunings. Built in Rust on the Genet engine; since 2026-08-09 the desktop
assembly (event loop, surface, layout, paint, input, a11y) lives in the
shared `cambium-genet-winit-host` crate in the genet repo. Woodshed was
that host's donor and is its first consumer. The earlier Xilem + Masonry
and Iced stacks are retired.

The name comes from "woodshedding", musicians' slang for focused,
solitary practice.

See `design_docs/PROJECT_DESCRIPTION.md` for the product description and
`design_docs/DOC_README.md` for the doc index.

## Document Structure

All authoritative design material lives in `design_docs/`. Read
`design_docs/DOC_README.md` first.

| Path | What's there |
|------|-------------|
| `design_docs/DOC_README.md` | Index and AI working principles |
| `design_docs/DOC_POLICY.md` | Documentation governance |
| `design_docs/PROJECT_DESCRIPTION.md` | Product goals, features (maintainer-owned) |
| `design_docs/<date>_<keyword>_plan.md` | Active feature plans |
| `design_docs/archive_docs/<date>/` | Retired plans |

## Workspace Layout

```
crates/
  woodshedding/      Pure-Rust theory and playable-practice model. No I/O,
                     no UI, no audio.
  audio-primitives/  Pure-std DSP helpers (click synth, onset/tempo,
                     latency, waveform projection, meter ballistics).
                     Shared cross-repo with hocket.
  woodshed-audio/    Real-time audio: cpal drivers, tuner-grade pitch
                     detection, MIDI, looping, calibration.
  woodshed-core/     Portable application state, persistence payloads, and
                     host-facing seams. No windowing, no browser APIs.
  woodshed-views/    Cambium product views and CSS themes. No window
                     chrome, no audio drivers.
  woodshed-genet/    The desktop application binary, composed on
                     cambium-genet-winit-host.
  woodshed-graph/    Theory catalog projected into the chartulary graph.
```

Keep `woodshedding` and `audio-primitives` pure: no `cpal`, no UI, no file
I/O. Audio-coupled code belongs in `woodshed-audio`, product composition in
`woodshed-views`, host and native concerns in `woodshed-genet`. Do not
re-add direct deps on genet-layout, genet-winit-host, netrender, or the
paint-list crates; the shared host owns that assembly and they arrive
transitively.

## General Guidelines

- Rust: follow standard idioms. No `unsafe` without documented justification.
- Theory model must support arbitrary string counts and arbitrary
  tunings — do not hard-code 6-string assumptions.
- Plans go in `design_docs/` per the date-keyword-plan convention. Do
  not store project plans in `.claude/plans/`.
- Follow `DOC_POLICY.md` for documentation changes.

## Workspace tooling

sem and weave are wired into this repo. Both are described once in
`Code/CLAUDE.md`, which loads for any session at or below `Code`.

## Important Don'ts

- Do not depend on `rust-music-theory` or similar upstream theory
  crates. Their scale and chord coverage is insufficient for this
  project; we own the theory model.
- Do not add features beyond the active plan's current feature target
  without surfacing the scope change first.
- Do not pin Rust toolchain versions without reason — this project has
  no upstream constraint forcing a specific version.
