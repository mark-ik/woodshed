//! Encryption at rest, as a muniment backend.
//!
//! Sealing wraps [`muniment::Backend`] rather than woodshed's own storage trait,
//! which is what lets it compose with any store muniment ships (filesystem,
//! redb, zip, IndexedDB) and with any codec, instead of only with the one
//! `String`-shaped seam woodshed used to own.
//!
//! It is not a [`muniment::Codec`]. A codec's methods are associated functions
//! with no `&self`, so there is nowhere to hold a key; sealing needs per-instance
//! state. `Backend` takes `&self` and moves bytes, which is what sealing acts on.
//!
//! Sealing here also covers more than the old decorator did. That one sealed one
//! session string; this seals the bytes of every slot written through it, so a
//! second slot is protected by construction rather than by remembering to wrap
//! it.

use async_trait::async_trait;
use muniment::backend::WriteOp;
use muniment::{Backend, StoreError};
use personae::{seal_bytes, unseal_bytes, IdentityProvider};

use crate::storage::WOODSHED_SEAL_CONTEXT;

/// A backend that seals what it writes and unseals what it reads.
///
/// Keys are not sealed. A store has to look keys up to find anything, and the
/// slot names woodshed uses ("session", "settings") disclose nothing the file's
/// existence does not already.
pub struct SealedBackend<B> {
    inner: B,
    key: [u8; 32],
    adopt_plaintext: bool,
}

impl<B> SealedBackend<B> {
    /// Seal under an explicit 32-byte key. The primitive, used by hosts that
    /// already hold the key and by tests.
    pub fn new(inner: B, key: [u8; 32]) -> Self {
        Self {
            inner,
            key,
            adopt_plaintext: false,
        }
    }

    /// Read an unsealed value as though it were sealed, once.
    ///
    /// The migration path for a store written before sealing was switched on:
    /// the old plaintext is read, the app uses it, and the next write seals it.
    /// No separate migration step, and nothing to run in the right order.
    ///
    /// Bounded deliberately. It only adopts bytes that are valid UTF-8, because
    /// woodshed's slots hold JSON and sealed bytes are a nonce plus ciphertext
    /// plus a tag, which is very unlikely to be. That is a heuristic rather than
    /// a proof: it tells old plaintext from sealed bytes, not from an arbitrary
    /// foreign file. Turn it off once no unsealed stores remain in the wild.
    pub fn adopting_plaintext(mut self) -> Self {
        self.adopt_plaintext = true;
        self
    }

    /// Derive the sealing key from the user's personae identity, so a sealed
    /// store is readable on any device carrying the persona seed and opaque
    /// without it.
    pub fn for_provider(
        inner: B,
        provider: &dyn IdentityProvider,
    ) -> Result<Self, personae::IdentityError> {
        let key = provider.derive_keypair(WOODSHED_SEAL_CONTEXT)?.to_seed();
        Ok(Self::new(inner, key))
    }

    fn seal(&self, bytes: &[u8]) -> Result<Vec<u8>, StoreError> {
        seal_bytes(&self.key, WOODSHED_SEAL_CONTEXT, bytes)
            .map_err(|error| StoreError::Backend(format!("seal failed: {error}")))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<B: Backend + Sync> Backend for SealedBackend<B> {
    /// Bytes that will not unseal read as absent rather than as an error.
    ///
    /// This is the old decorator's contract kept deliberately: a session sealed
    /// to a different persona, or a truncated file, starts a fresh session
    /// instead of stranding practice behind an error the user cannot act on.
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        let Some(stored) = self.inner.get(key).await? else {
            return Ok(None);
        };
        if let Ok(plain) = unseal_bytes(&self.key, WOODSHED_SEAL_CONTEXT, &stored) {
            return Ok(Some(plain));
        }
        if self.adopt_plaintext && std::str::from_utf8(&stored).is_ok() {
            return Ok(Some(stored));
        }
        Ok(None)
    }

    async fn put(&self, key: &str, bytes: &[u8]) -> Result<(), StoreError> {
        let sealed = self.seal(bytes)?;
        self.inner.put(key, &sealed).await
    }

    async fn delete(&self, key: &str) -> Result<(), StoreError> {
        self.inner.delete(key).await
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>, StoreError> {
        self.inner.list(prefix).await
    }

    async fn scan(&self, start: &str, end: &str) -> Result<Vec<String>, StoreError> {
        self.inner.scan(start, end).await
    }

    /// Every `Put` in the batch is sealed before the batch is applied, so the
    /// inner backend still sees one atomic unit. Sealing op by op inside the
    /// transaction would be the same bytes with a failure point in the middle.
    async fn apply(&self, ops: &[WriteOp]) -> Result<(), StoreError> {
        let mut sealed = Vec::with_capacity(ops.len());
        for op in ops {
            sealed.push(match op {
                WriteOp::Put { key, value } => WriteOp::Put {
                    key: key.clone(),
                    value: self.seal(value)?,
                },
                WriteOp::Delete { key } => WriteOp::Delete { key: key.clone() },
            });
        }
        self.inner.apply(&sealed).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use muniment::MemoryBackend;

    fn backend() -> SealedBackend<MemoryBackend> {
        SealedBackend::new(MemoryBackend::default(), [1u8; 32])
    }

    #[tokio::test]
    async fn what_is_written_reads_back_and_is_not_stored_in_the_clear() {
        let inner = MemoryBackend::default();
        let sealed = SealedBackend::new(inner.clone(), [1u8; 32]);
        sealed.put("session", b"my practice state").await.unwrap();

        assert_eq!(
            sealed.get("session").await.unwrap().as_deref(),
            Some(&b"my practice state"[..])
        );
        let at_rest = inner.get("session").await.unwrap().expect("stored");
        assert!(
            !at_rest.windows(8).any(|w| w == b"practice"),
            "the plaintext must not survive in the store"
        );
    }

    #[tokio::test]
    async fn another_persona_reads_a_fresh_session_rather_than_an_error() {
        let inner = MemoryBackend::default();
        SealedBackend::new(inner.clone(), [1u8; 32])
            .put("session", b"mine")
            .await
            .unwrap();

        let theirs = SealedBackend::new(inner, [2u8; 32]);
        assert_eq!(
            theirs.get("session").await.unwrap(),
            None,
            "a wrong key starts a fresh session instead of stranding practice"
        );
    }

    #[tokio::test]
    async fn every_slot_is_sealed_not_just_the_session() {
        // The reason sealing moved to the backend: a second slot is covered by
        // construction rather than by remembering to wrap it.
        let inner = MemoryBackend::default();
        let sealed = SealedBackend::new(inner.clone(), [1u8; 32]);
        sealed.put("settings", b"tuning=drop-d").await.unwrap();
        let at_rest = inner.get("settings").await.unwrap().expect("stored");
        assert!(
            !at_rest.windows(6).any(|w| w == b"drop-d"),
            "settings are sealed too"
        );
    }

    #[tokio::test]
    async fn a_batch_seals_every_put_and_stays_one_unit() {
        let inner = MemoryBackend::default();
        let sealed = SealedBackend::new(inner.clone(), [1u8; 32]);
        sealed
            .apply(&[
                WriteOp::Put {
                    key: "session".into(),
                    value: b"state".to_vec(),
                },
                WriteOp::Put {
                    key: "settings".into(),
                    value: b"prefs".to_vec(),
                },
            ])
            .await
            .unwrap();

        assert_eq!(
            sealed.get("session").await.unwrap().as_deref(),
            Some(&b"state"[..])
        );
        assert_eq!(
            sealed.get("settings").await.unwrap().as_deref(),
            Some(&b"prefs"[..])
        );
        for key in ["session", "settings"] {
            let at_rest = inner.get(key).await.unwrap().expect("stored");
            assert!(at_rest.len() > 5, "sealed bytes carry nonce and tag");
        }
    }

    #[tokio::test]
    async fn a_second_device_with_the_same_persona_reads_what_the_first_sealed() {
        // The carry property, and the reason the key derives from the persona
        // rather than from the device: pairing a second device by seed is what
        // makes the practice session follow the practitioner.
        use personae::InMemoryProvider;
        let inner = MemoryBackend::default();
        let seed = [3u8; 32];

        SealedBackend::for_provider(inner.clone(), &InMemoryProvider::from_seed(seed))
            .unwrap()
            .put("session", b"my practice state")
            .await
            .unwrap();

        let device_b =
            SealedBackend::for_provider(inner.clone(), &InMemoryProvider::from_seed(seed)).unwrap();
        assert_eq!(
            device_b.get("session").await.unwrap().as_deref(),
            Some(&b"my practice state"[..])
        );

        let other =
            SealedBackend::for_provider(inner, &InMemoryProvider::from_seed([4u8; 32])).unwrap();
        assert_eq!(other.get("session").await.unwrap(), None);
    }

    #[tokio::test]
    async fn a_plaintext_store_is_adopted_and_sealed_on_the_next_write() {
        // The migration, end to end: a session written before sealing was on is
        // read once as it is, and the next save seals it in place.
        let inner = MemoryBackend::default();
        inner.put("session", br#"{"tab":"Stage"}"#).await.unwrap();

        let sealed = SealedBackend::new(inner.clone(), [1u8; 32]).adopting_plaintext();
        let read = sealed.get("session").await.unwrap().expect("adopted");
        assert_eq!(read, br#"{"tab":"Stage"}"#);

        sealed.put("session", &read).await.unwrap();
        let at_rest = inner.get("session").await.unwrap().expect("stored");
        assert!(
            !at_rest.windows(5).any(|w| w == b"Stage"),
            "the next write sealed what the migration read"
        );
        assert_eq!(
            sealed.get("session").await.unwrap().as_deref(),
            Some(&read[..])
        );
    }

    #[tokio::test]
    async fn plaintext_is_not_adopted_unless_asked_for() {
        let inner = MemoryBackend::default();
        inner.put("session", br#"{"tab":"Stage"}"#).await.unwrap();
        let strict = SealedBackend::new(inner, [1u8; 32]);
        assert_eq!(
            strict.get("session").await.unwrap(),
            None,
            "adoption is opt-in, so a strict store still refuses unsealed bytes"
        );
    }

    #[tokio::test]
    async fn keys_stay_readable_so_the_store_can_still_find_things() {
        let sealed = backend();
        sealed.put("session", b"x").await.unwrap();
        assert_eq!(sealed.list("").await.unwrap(), vec!["session".to_string()]);
    }
}
