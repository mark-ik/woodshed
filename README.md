# guitar-toolkit (placeholder name)

A guitarist's toolkit: tuner, comprehensive chord and scale libraries with
formulas, chord progressions, classic practice exercises, and a metronome
extensible into a simple drum machine. Theory model generalizes to other
stringed instruments (bass, ukulele, banjo).

Built in Rust with [Iced](https://iced.rs) for the UI. Targets desktop first
(Windows, macOS, Linux) with a planned path to mobile.

## Status

Pre-alpha. See [`design_docs/`](design_docs/) for project description, doc
policy, and the active plan.

## Layout

```
crates/
  music-theory/   Pure-Rust theory primitives (no I/O, no UI)
  app/            Iced application — depends on music-theory
design_docs/      Plans, policy, project description
```

## Build

```
cargo build
cargo run -p guitar-toolkit
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
