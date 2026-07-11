# Personae-Sealed Practice Session

**Date:** 2026-07-08
**Status:** core slice landed; host wiring deferred.

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

## Deferred: host wiring (woodshed-serval)

The app's `FsStorage` (writes `serval-state.json`) wraps in
`SealedStorage::for_provider(fs, &provider)`, where `provider` is a personae
identity unlocked at startup (Windows DPAPI via personae's `startup_unlock`; a
passphrase elsewhere). That is the one heavier step, it builds the serval app and
adds identity bootstrap to `main`, so it is left for a focused host pass.

## Follow-ons

- Real cross-device carry needs the wallet's transport + pairing to sync the
  sealed session between your own devices, which is the trust-plane plan's
  territory, not woodshed's.
- A single persona is right for woodshed; the persona registry + multi-face is
  mere's story and is not needed here.
