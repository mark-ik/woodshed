# CLAUDE.md — guitar-toolkit Repository Role

This file defines how Claude Code should behave in this repository. Read
it first when starting any session.

---

## Project Identity

**guitar-toolkit** (placeholder name) is an open-source guitarist's
toolkit: tuner, comprehensive theory libraries, exercises, and a
metronome / drum machine. Theory model generalizes across stringed
instruments. Built in Rust with Iced.

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
  music-theory/   Pure Rust theory primitives. No I/O, no UI, no audio.
  app/            Iced application. Depends on music-theory.
```

Keep `music-theory` pure. UI- or audio-coupled code belongs in `app` or
in a future dedicated crate.

## General Guidelines

- Rust: follow standard idioms. No `unsafe` without documented justification.
- Theory model must support arbitrary string counts and arbitrary
  tunings — do not hard-code 6-string assumptions.
- Plans go in `design_docs/` per the date-keyword-plan convention. Do
  not store project plans in `.claude/plans/`.
- Follow `DOC_POLICY.md` for documentation changes.

## Important Don'ts

- Do not depend on `rust-music-theory` or similar upstream theory
  crates. Their scale and chord coverage is insufficient for this
  project; we own the theory model.
- Do not add features beyond the active plan's current feature target
  without surfacing the scope change first.
- Do not pin Rust toolchain versions without reason — this project has
  no upstream constraint forcing a specific version.
