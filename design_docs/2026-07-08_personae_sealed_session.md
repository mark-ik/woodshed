# Personae-Sealed Practice Session

**Date:** 2026-07-08
**Status:** complete (host wiring landed 2026-08-08).

Woodshed is the first cross-app consumer of `personae`, the Merely suite's
identity + carry crate (see
`personae/design_docs/2026-07-08_personae_across_the_suite.md`). It proves the
"one persona, many devices, encrypt-at-rest" pole with the smallest real slice:
seal the private practice session at rest under a persona-derived key.

## What landed (woodshed-core)

`SealedStorage<S: Storage>` in `crates/woodshed-core/src/storage.rs`: a wrapper
over the host `Storage` seam (`load() -> Option<String>` / `save(&str)`) that
seals the serialized `PersistedSession` under a persona-derived key (personae's
`seal_bytes`, XChaCha20-Poly1305), hex-encoded to fit the String seam so the
host's filesystem / OPFS realization is unchanged.

Built with `SealedStorage::for_provider(inner, &provider)`, the key derives from
the user's personae identity (`derive_keypair("woodshed.practice-session.seal.v1")`),
which gives three properties:

- **Encrypt-at-rest.** The practice session (selections, custom songs, rehearsal
  sets) is ciphertext on disk, not plaintext JSON.
- **Carry.** Any device holding the persona seed re-derives the key and reads the
  state; a device without it gets `None` (a fresh session), never the plaintext.
- **No stranding.** A seal failure skips the write (prior session stays), per the
  Storage contract's "a broken disk must not strand practice."

Three tests: sealed-at-rest + round-trip, wrong-key-degrades-to-`None`, and the
carry property (a second device with the same persona seed reads the sealed
session; a different persona cannot).

## What changed since (2026-07-25, storage dedup)

`Storage` / `SettingsStorage` / `SealedStorage` are gone. They were a two-slot,
`String`-shaped restatement of muniment's `SlotStore`, with one filesystem
realization against muniment's shipped redb / zip / IndexedDB / memory. The
names above map to:

- `SealedStorage<S: Storage>` → `SealedBackend<B: muniment::Backend>`
  (`crates/woodshed-core/src/sealed_backend.rs`). It seals the bytes of every
  slot rather than one session string, so a second slot is protected by
  construction rather than by remembering to wrap it. Not a `muniment::Codec`:
  a codec's methods are associated functions with no `&self`, so there is
  nowhere to hold a key.
- `Storage` + `SettingsStorage` → `SessionStore<B>` over named slots.
- `FsStorage` → `FsBackend`, which now only maps slot names to file paths.

## Host wiring (2026-08-08) — landed

`storage::open_store()` in `woodshed-genet`. The key comes from the
**family-shared** personae vault via `personae::roster::open_shared`, not from a
woodshed-local identity, so the practice session is sealed under the same
persona Turnstone and Knot use. See mere's
[family-shared identity plan](../../mere/design_docs/mere_docs/implementation_strategy/2026-08-08_family_shared_identity_plan.md).

Two decisions worth keeping:

- **Sealing is not a gate.** Woodshed practiced without an identity before
  sealing existed, and a machine with no vault backend — no DPAPI, no
  `PERSONAE_PASSPHRASE` — still has to be able to open a tuner. A vault that
  will not open is said out loud and stepped over, never raised.
- **The migration is `adopting_plaintext()`.** A session written before sealing
  was switched on is read once as it stands, and the next save seals it in
  place. Nothing to run, and nothing to run in the right order. Turn it off
  once no unsealed stores remain in the wild.

Which backend is chosen at startup, so the field is `SessionStore<HostBackend>`
where `HostBackend = Box<dyn Backend + Send + Sync>` — muniment gained
`impl Backend for Box<B>` for exactly this, rather than each host writing an
enum that delegates all six methods.

## Follow-ons

- Real cross-device carry needs the wallet's transport + pairing to sync the
  sealed session between your own devices, which is the trust-plane plan's
  territory, not woodshed's.
- **Choosing a persona.** Woodshed opens whichever persona the vault ladder
  resolves and has no way to switch. mere's `mere-persona-picker` is the shared
  list, but woodshed's settings surface is the `genet_host_api::settings`
  provider contract rather than a Cambium view, so a persona row here is a
  `SettingControl::Choice` over the roster — and applying it has to reopen the
  store under a new key, which is restart-shaped and not a settings write.
