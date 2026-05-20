# Woodshed

A guitarist's practice toolkit, named for the musician's term for focused
solitary practice. Tuner, comprehensive chord and scale libraries with
formulas, chord progressions, classic practice exercises, a metronome
extensible into a simple drum machine, and a **Practice Mode** that
walks you through rotations of material at tempo so the app drives you
rather than the other way around. The theory model generalizes to other
stringed instruments (bass, ukulele, banjo).

Built in Rust with [Xilem](https://github.com/linebender/xilem) +
[Masonry](https://github.com/linebender/xilem/tree/main/masonry) for the
UI (migrated from Iced 2026-05-18). Targets desktop first (Windows,
macOS, Linux) with a planned path to mobile and web.

## Status

Pre-alpha. See [`design_docs/`](design_docs/) for the product
description, doc policy, and the active plan.

## Layout

```
crates/
  woodshedding/    Pure-Rust theory primitives (no I/O, no UI, no audio)
  woodshed-audio/  Tuner-grade pitch detection + sequencer/click engine
  woodshed-xilem/  Xilem + Masonry application — depends on the two above
design_docs/       Plans, policy, project description
```

## Build

```
cargo build
cargo run -p woodshed-xilem
```

## License

Dual-licensed under either of:

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.
