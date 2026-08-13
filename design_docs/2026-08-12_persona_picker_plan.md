# Persona Picker Plan

**Date:** 2026-08-12
**Status:** open. **P1 landed 2026-08-12**; P2 and P3 open. Spun out of
mere's leverage census (step 3 named `mere-persona-picker` the wire-now item
and woodshed the first consumer: it already consumes personae and cambium,
and it ships next).

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

- **P1 — startup pick. Landed.** When the vault opens with more than one persona
  and none remembered or forced (`PERSONAE_PROFILE` set, or a sole
  persona, keeps today's silent path), present the picker before the
  store opens. On `Chose`: `remember_profile`, then open storage sealed
  to it. On `Dismissed`: practise with no persona at all, saving nothing
  (revised 2026-08-12; it opened on the convention until then, see "No
  persona is a real answer" below). Picking nobody must not block practice,
  the same doctrine as "sealing is not a gate."
- **P2 — live switch. Landed.** A settings row reopens the picker at any time; a
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
- Declining the gate leaves the app fully usable and the vault untouched,
  and the window says for the rest of the session that nothing is saved.
- The picker surface is reachable by genet-probe (the a11y/automation
  surface plan's standing requirement for any new surface).

## Non-goals

- Vault management beyond choose + create (rename, delete, key
  inspection stay with castellan's future surfaces).
- Any emblem/credential presentation UI.
- Changing the family profile convention itself.

## P1 as built

The gate is a screen, not a scrim. When the pick is open, `stage_root`
returns `persona_gate` in place of the product root, because nothing behind
it has been read: the practice session is sealed to the persona the screen
is asking about, so drawing an empty stage behind a modal would state
something false about what is loaded.

Where the code sits:

- `woodshed-views/src/persona.rs`: `PersonaPick` (roster, the picker's
  `CommandState`, a one-shot outcome, a notice) and the gate view, composed
  from `persona_picker` through `lens` + `map_action`.
- `woodshed-genet/src/persona.rs`: `pending_roster` (whether to ask, run
  before the window exists), `after_dispatch` (act on the answer), `settle`
  (remember, reopen, restore, take the gate down), and `seed`.
- `woodshed-genet/src/session.rs`: the session restore, lifted out of
  `boot_state` so both the ordinary path and the post-pick path share it.
- `Shared.storage` is now `Option`. It stays `None` while the gate is up.

Three decisions worth keeping:

1. **The store cannot open early.** `roster::open_shared` resolves an
   unchosen multi-persona vault to `default` and *mints it*, which would add
   a third identity beside the user's two and seal the session to it. So the
   decision to ask runs on a vault read that opens no profile at all, and
   the store opens only once a persona is settled.
2. **The choice does not travel through the remembered file.** `settle`
   writes the choice with `remember_profile` and then opens on the id
   directly, via a new `roster::open_profile` in personae. A vault directory
   that refuses the write would otherwise silently reroute the session to
   whoever the convention picks, while the screen said the user had chosen.
3. **Escape is answered by the window-wide key policy**, not by the picker.
   The picker reports its own Escape, but only to whatever holds the caret,
   and at startup that is nothing, so the first press would have done
   nothing. `escape_policy` in `main.rs` records the dismissal before
   dispatch. Named rather than inline so the test drives the shipping
   decision.

`CreateRequested` is P3's. The shared picker always appends a create row, so
P1 answers it rather than dropping it: the gate stays open and says a
persona comes from `personae-vault` today. A row that silently does nothing
reads as a broken application.

### No persona is a real answer (revised 2026-08-12)

Declining first shipped as "open on the convention", which was the wrong
answer twice over.

It was wrong as product: practising should not require saying who you are.
Now Escape opens no store at all. The app is completely usable, and the
window closing is the end of the session. `Shared.storage` stays `None`,
every save in the dispatch tail is skipped, and `UiState.practice_saved`
turns a nav-row notice on for as long as the window is open. A row rather
than a one-off dialog, because the choice is in force the whole time and
saying it once at startup would leave the honest fact where nobody can
check it.

It was also wrong as behaviour, and in exactly the way this plan's first
decision names. The only vault that reaches the gate is several personas
with none chosen; the convention resolves that to `default` and *mints it*.
So the old decline path added a third identity beside the user's two and
sealed their practice to one they never picked, which is the harm the gate
exists to prevent, left standing on the one path that skips the gate.
`declining_at_startup_would_have_minted_a_third_identity` pins the
convention's behaviour so the reasoning cannot quietly outlive its cause.

Unchanged: a machine with no vault backend still saves, unsealed and out
loud. That fallback predates sealing and is somebody's real practice; "no
persona" here means the user declined one, not that the machine has none.

## P2 as built

The Settings General page gains a Persona heading and a "Switch persona…"
row. It sets `UiState::persona_switch_requested`; the host answers it,
because reading the roster means opening the vault and a view does not do
vault work. The same gate screen comes up, now carrying a `PickPurpose`.

Two decisions, both about what a *second* gate means that the first did not:

1. **Escape means the opposite thing.** At startup, declining opens the
   store on the convention's persona — that is what starts the application.
   During a switch, a store is already open and sealed to somebody, so
   settling on the convention would quietly move the user off the persona
   they are practising as. `PickPurpose::Switch` dismissal therefore takes
   the gate down and touches nothing. The screen says which gate it is.
2. **The switch resets `UiState` before restoring.** This is the hazard P1
   could not have had. `session::restore` returns early on a store with no
   session, and this host saves every dispatch — so switching into a persona
   who has never practised would leave the *outgoing* persona's Set and
   history standing, and the next frame would write them into the incoming
   persona's store. `settle` replaces the state wholesale for a switch.
   Host-fed fields (MIDI port lists, latency) refill on the next dispatch.

A vault that will not open raises the gate anyway, carrying the error as its
notice: the row is a deliberate act and cannot answer with silence, the same
reasoning P1 used for the create row.

**Receipts**: `cargo test --workspace` in woodshed, **437 passed, 0 failed**.
Four are P2's. `switching_into_an_unused_persona_does_not_carry_the_last_one_in`
asserts the hazard in both directions — restoring an empty store over live
state keeps the outgoing song, and reset-then-restore does not — so the test
fails if the reset is ever removed.

## Findings

- **`persona-picker` pulls a second genet source on a clean checkout.** mere
  tracks `genet.git` by `branch = "main"`; woodshed pins
  `rev = "398e4af60"` (its manifest states the reason: the host receipt is
  immutable). Cargo keys a git source by URL *and* reference, so on a
  machine without the local `[patch]` table the picker's `cambium` and
  woodshed's `cambium` are two packages, which is the two-meristem failure
  the `.cargo/config.toml` notes already describe. On a dev machine the
  patch table is keyed by URL alone and unifies them: `cargo metadata`
  resolves exactly one `cambium`, one `meristem`, one `personae`. The
  clean-checkout case is **reasoned, not verified**: the check without the
  patch table timed out fetching genet, which is Servo-sized. Woodshed's
  committed `Cargo.lock` records zero git sources, so it is already a
  machine-local artifact and encodes nothing either way. Moving woodshed's
  genet deps to `branch = "main"` would settle it and match every sibling
  repo, but the pin is deliberate and the call is Mark's.
- **The picker cannot ask for focus.** `cambium::request_focus` takes an
  `ElementView`, and `command_picker` returns
  `impl View<..., Element = GenetElement>` without advertising
  `ElementView`, so neither the picker crate nor woodshed can wrap it. The
  fix is one signature widening in cambium (the return type really is
  `OnKey<El<..>>`), which would let `mere-persona-picker` offer a focused
  variant every startup gate wants. Until then: clicking works cold, Escape
  works cold through the key policy, and arrow-key navigation waits for one
  Tab.
- **Command rows carry position, not identity.** `command_surface`'s DOM id
  is `persona-picker-item-0`, so a driver targets a persona by its visible
  label (`.command-label` containing the name). `graph_canvas` already sets
  the precedent of a `data-key` on each node; the same on a command row
  would make every picker in the family id-addressable.
- **Settings without a session apply to nothing.** `restore` loads
  `genet-settings.json` and then drops it unless a session also decodes,
  because the derivations (transport bpm, the tuning and root dropdowns,
  the legacy relation-set migration) all hang off `apply_persisted`. Carried
  over unchanged from `boot_state` rather than fixed inside this slice.

## Receipts

`cargo test --workspace` in woodshed: **441 passed, 0 failed** (with P2's).

Seventeen of those are the gate's, in `woodshed-genet/src/persona.rs`. Nine
run against a real vault in a scratch directory: two personas and no choice
asks; a sole persona, an empty vault, a remembered choice,
`PERSONAE_PROFILE`, and a vault that will not unlock all stay silent; the
roster carries the personas the vault actually holds, sorted; an open on a
named persona loads it rather than re-minting it; and the convention open
mints a third identity, which is why declining does not use it. Six drive
the real product root through `Harness`: the picker and its dialog resolve
through genet-probe selectors, the product navigation does not render behind
the gate, clicking a row records `Chose` by id, the create row keeps the gate
up and puts its notice on screen, Escape dismisses with nothing focused, and
a declined session reaches the product with the "not being saved" notice on
screen. Six more in `woodshed-views/src/persona.rs` cover the outcome
recording and the declined state directly.

In personae: `roster::open_profile` (the named-persona open, factored so
both open paths unlock once) and `Unlock::passphrase` (so an application
does not need a `zeroize` dependency to name a passphrase vault).
**79 passed, 0 failed.**

