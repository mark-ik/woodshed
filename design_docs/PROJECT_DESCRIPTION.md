# Project Description

Maintainer-owned. Other docs are derived from this.

## What it is

A guitarist's toolkit application: tuner, comprehensive chord and scale
libraries with formulas, chord progressions, classic practice exercises,
and a metronome that extends into a simple drum machine. The theory
model generalizes to other stringed instruments (bass, ukulele, banjo).

## Goals

- **Comprehensive theory coverage** for guitar and related stringed
  instruments, in a single offline application
- **Open source** code, with a paid packaged binary distributed via
  itch.io / Gumroad (and eventually Google Play, Apple App Store) for
  users who want convenience
- **Offline-first** — no network connection required for any feature

## Major Features

### Tuner
- All standard guitar tunings (standard, drop-D, drop-C, DADGAD, open G,
  open D, etc.) and equivalents for bass, ukulele, banjo
- Arbitrary user-defined tunings, savable as named presets

### Chord Library
- Comprehensive chord catalog with formulas (intervals from root)
- Tertiary (triads, sevenths, extensions, altered)
- Suspended, quartal, quintal, cluster
- Per-tuning fingering generation

### Scale Library
- Diatonic modes (Ionian through Locrian)
- Altered minor forms (harmonic, melodic, phrygian dominant, hungarian
  minor, double harmonic, neapolitan, etc.)
- Diminished (whole-half, half-whole), augmented, whole tone
- Exotic / world (hirajoshi, in sen, pelog, etc.)
- Bebop scales
- Non-tertiary and altered non-tertiary
- Per-tuning fretboard mapping

### Chord Progressions
- Standard progressions (I-IV-V, ii-V-I, 12-bar blues, etc.)
- Symmetric progressions
- User-definable arbitrary progressions

### Exercises
- Chromatic and derivations
- Spider and derivations
- Trill
- Ladder
- X-pattern

### Metronome / Drum Machine
- Definable time signatures
- Rhythm sequencer subdivisible to 1/32nd notes
- Triplet subdivision toggle (for swing, shuffle, 12/8 feels)
- Per-step velocity / accent
- Multiple drum sounds — the metronome is a special-case drum pattern

## Distribution Plan

1. itch.io / Gumroad (desktop: Windows, macOS, Linux)
2. Google Play (Android)
3. Apple App Store (iOS)

The web version is free and self-hosted; paid binaries are for users who
want a packaged, signed, offline-installable app without compiling.

## Tech Stack

- **Language**: Rust
- **UI**: Iced (custom-rendered via wgpu — same look across platforms)
- **Audio I/O**: cpal
- **DSP**: fundsp (drum/click synthesis)
- **Pitch detection**: pitch-detector (or pitch-detection)
- **Theory**: in-house `music-theory` crate (no external theory dep)

## Non-Goals (Initial Release)

- Real-time effects processing
- Multi-track recording / DAW features
- Notation rendering (sheet music staff display)
- Collaborative / network features
