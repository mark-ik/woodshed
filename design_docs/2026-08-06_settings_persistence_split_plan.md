# Woodshed Settings Persistence Split

**Date**: 2026-08-06  
**Status**: landed

## Boundary

`PersistedSession` owns practice artifacts and session navigation: stage
selection, the rehearsal set, the song document, and practice history.
`AppSettings` owns application preferences such as theme, accessibility,
tuning, fretboard, and metronome defaults.

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

## Receipts

- `cargo test -p woodshed-core storage -- --nocapture`
- `cargo check -p woodshed-genet`

The session wire test proves settings are absent from the new artifact file;
the legacy decode test proves old flat settings remain recoverable.
