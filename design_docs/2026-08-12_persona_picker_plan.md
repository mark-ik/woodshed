# Persona Picker Plan

**Date:** 2026-08-12
**Status:** open. Spun out of mere's leverage census (step 3 named
`mere-persona-picker` the wire-now item and woodshed the first consumer:
it already consumes personae and cambium, and it ships next).

## What exists on each side

- **The picker is finished view-model code.** `mere-persona-picker`
  composes cambium's `command_picker` over `identity::roster`:
  `picker_state()` labels the surface, `roster_items(&Roster)` renders one
  row per persona (the one in use says "in use"; the others report their
  key-slot counts) plus a create row, and `persona_picker(&state, &roster)`
  returns `PickerEvent::Chose(ProfileId)` / `CreateRequested` /
  `Dismissed`. The chosen persona comes back as an id, not an index, so a
  roster that changes mid-render cannot pick the wrong person. The picker
  never writes the vault; remembering is the caller's act
  (`roster::remember_profile`).
- **Woodshed already seals to the convention-chosen persona.**
  `woodshed-genet/src/storage.rs` opens the shared vault
  (`roster::open_shared(Unlock::from_env())`) and seals practice storage to
  the persona the family convention picks (env override, remembered
  choice, sole persona, `default`). Its own doc states the payoff this
  plan surfaces: "switching personas switches practice sessions." What is
  missing is only the surface where a human makes that choice.

## Slices

- **P1 — startup pick.** When the vault opens with more than one persona
  and none remembered or forced (`PERSONAE_PROFILE` set, or a sole
  persona, keeps today's silent path), present the picker before the
  store opens. On `Chose`: `remember_profile`, then open storage sealed
  to it. On `Dismissed`: proceed on the convention choice, exactly as
  today — picking nobody must not block practice, the same doctrine as
  "sealing is not a gate."
- **P2 — live switch.** A settings row reopens the picker at any time; a
  `Chose` swaps the sealed store live (close, reopen sealed to the new
  persona, reload the practice session). No restart: persona change is a
  live swap, per the ecosystem's live-switching rule.
- **P3 — create flow.** `CreateRequested` opens woodshed's own name input;
  `roster::create_profile` with the name, then proceed as P1's `Chose`.

The view composes in `woodshed-views` (cambium views live there); the
trigger and storage reopen live in `woodshed-genet`. The dependency is
`mere-persona-picker` from the mere workspace, by the same git branch as
the personae dep it already carries.

## Done conditions

- A vault with two personas presents the list at startup; choosing one
  seals practice to it and the choice persists to the next launch.
- Switching personas from settings swaps the practice session without a
  restart.
- A machine with no vault backend never sees the picker and keeps the
  loud unsealed fallback.
- The picker surface is reachable by genet-probe (the a11y/automation
  surface plan's standing requirement for any new surface).

## Non-goals

- Vault management beyond choose + create (rename, delete, key
  inspection stay with castellan's future surfaces).
- Any emblem/credential presentation UI.
- Changing the family profile convention itself.
