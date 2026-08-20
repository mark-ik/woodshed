# Woodshed Settings Persistence Split

**Date**: 2026-08-06  
**Status**: landed

## Boundary

`PersistedSession` owns practice artifacts and session navigation: stage
selection, the rehearsal set, the song document, and practice history.
`AppSettings` owns application preferences such as theme, accessibility,
tuning, fretboard, metronome defaults, and desktop window placement.

The desktop host writes these lanes separately:

- `genet-state.json` through `Storage`
- `genet-settings.json` through `SettingsStorage`

`WOODSHED_STATE` selects an isolated session file for scenarios. The adjacent
settings path follows that override unless `WOODSHED_SETTINGS` is supplied.

## Migration

`PersistedSession` no longer serializes flattened application settings.
`decode_session` still inspects old session JSON and returns
`SessionLoad::legacy_settings` when it finds the former flattened fields. The
desktop host uses that payload only when the separate settings file does not
exist, then writes the split form on the next frame.

The core `Storage` trait is unchanged for browser hosts. They must implement
`SettingsStorage` beside it when they adopt the split. No universal settings
file or cross-product schema is introduced.

The Genet desktop adapter stores window position, logical size, and maximized
state as `WindowSettings`. Startup reads that slot before creating the native
window. A valid settings file also applies over a default practice session when
the session slot has not been written yet.

## Receipts

- `cargo test -p woodshed-core storage -- --nocapture`
- `cargo check -p woodshed-genet`
- `cargo test --offline -p woodshed-core settings::tests::window_geometry_is_optional_and_round_trips -- --exact`
- `cargo test --offline -p woodshed-genet session::tests::settings_apply_without_a_practice_session -- --exact`
- `cargo test --offline -p woodshed-genet tests::window_geometry_conversion_preserves_every_axis -- --exact`
- `cargo build --offline --locked -p woodshed-genet`

The session wire test proves settings are absent from the new artifact file;
the legacy decode test proves old flat settings remain recoverable.

`ee0c852` completed the desktop window lane. The headed Windows receipt in
`testing/woodshed/window-state-20260820-093601/` moved and resized a fresh
instance to `(202, 158)` at `1078 x 642`, closed it through the application
hook, and observed a second process open at the identical rectangle from the
same sealed scratch settings slot.
