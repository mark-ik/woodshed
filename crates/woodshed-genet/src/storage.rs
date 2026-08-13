//! The desktop backend: files in the config dir the xilem app uses
//! (`ProjectDirs dev/Woodshed/Woodshed`), under their own filenames so the two
//! apps never clobber each other during the migration.
//!
//! A [`muniment::Backend`] rather than woodshed's own storage trait, so the
//! store, the sealing, and the slot naming above it are all muniment's and this
//! file is only the platform half: which directory, which filename per slot.
//! The web host realizes the same trait over OPFS.

use std::path::PathBuf;

use async_trait::async_trait;
use directories::ProjectDirs;
use muniment::backend::WriteOp;
use muniment::{Backend, StoreError};
use personae::bootstrap::{self, Unlock};
use personae::roster::{self, OpenedVault};
use personae::vault::ProfileId;
use woodshed_core::sealed_backend::SealedBackend;
use woodshed_core::storage::SessionStore;

/// Open the practice store, sealed to the chosen persona when the family
/// vault opens and plain files when it does not.
///
/// **Sealing is not a gate.** Woodshed practiced without an identity before
/// sealing existed, and a machine with no vault backend — no DPAPI, no
/// `PERSONAE_PASSPHRASE` — still has to be able to open a tuner. So a vault
/// that will not open is said out loud and stepped over, never raised.
///
/// The key derives from the persona chosen in the shared personae vault
/// ([`roster::open_shared`]), which is what makes a session sealed on one
/// machine readable on another carrying the same persona, and what makes
/// switching personas switch practice sessions.
pub fn open_store() -> SessionStore<HostBackend> {
    open_store_as(None)
}

/// The practice store, sealed to `profile` when one was named.
///
/// `None` runs the family convention ([`roster::open_shared`]), which is the
/// startup path on every machine that needs no asking. `Some` is the answer to
/// the persona gate: the id goes straight into the open rather than through the
/// remembered file, so a vault directory that refuses the write still practises
/// as the persona the user picked.
pub fn open_store_as(profile: Option<&ProfileId>) -> SessionStore<HostBackend> {
    SessionStore::new(open_backend(profile))
}

/// The store woodshed practices over, decided at startup.
pub type HostBackend = Box<dyn Backend + Send + Sync>;

fn open_vault(profile: Option<&ProfileId>) -> Result<OpenedVault, personae::IdentityError> {
    let unlock = Unlock::from_env();
    match profile {
        Some(id) => roster::open_profile(&bootstrap::default_vault_dir(), unlock, id),
        None => roster::open_shared(unlock),
    }
}

fn open_backend(profile: Option<&ProfileId>) -> HostBackend {
    let files = FsBackend::new();
    let opened = match open_vault(profile) {
        Ok(opened) => opened,
        Err(error) => {
            eprintln!("[woodshed] no identity vault ({error}); practice will be stored unsealed");
            return Box::new(files);
        }
    };
    match SealedBackend::for_provider(files, &opened.vault) {
        // Adopting plaintext is the migration: a session written before sealing
        // was switched on is read once as it stands, and the next save seals it.
        // Nothing to run, and nothing to run in the right order.
        Ok(sealed) => {
            eprintln!(
                "[woodshed] practice sealed to persona {:?} ({})",
                opened.profile.0, opened.description
            );
            Box::new(sealed.adopting_plaintext())
        }
        Err(error) => {
            eprintln!(
                "[woodshed] could not derive a sealing key from persona {:?}: {error}; \
                 practice will be stored unsealed",
                opened.profile.0
            );
            Box::new(FsBackend::new())
        }
    }
}

pub struct FsBackend {
    /// `None` when the platform exposes no config dir. Persistence is silently
    /// disabled, matching woodshed-xilem's posture: a machine without a config
    /// dir still practices, it just does not remember.
    session: Option<PathBuf>,
    settings: Option<PathBuf>,
}

impl FsBackend {
    pub fn new() -> Self {
        // `WOODSHED_STATE` points the session at another file. A scenario run
        // sets it to a scratch profile: without it, an automated run would read
        // and then overwrite the real practice session.
        let settings_override = std::env::var("WOODSHED_SETTINGS").ok();
        if let Ok(path) = std::env::var("WOODSHED_STATE") {
            let state_path = PathBuf::from(path);
            return Self {
                settings: settings_override
                    .map(PathBuf::from)
                    .or_else(|| Some(state_path.with_extension("settings.json"))),
                session: Some(state_path),
            };
        }
        let (session, default_settings) = ProjectDirs::from("dev", "Woodshed", "Woodshed")
            .map(|dirs| {
                (
                    dirs.config_dir().join("genet-state.json"),
                    dirs.config_dir().join("genet-settings.json"),
                )
            })
            .unzip();
        Self {
            session,
            settings: settings_override.map(PathBuf::from).or(default_settings),
        }
    }

    /// Which file a slot lives in. Slot names are muniment's; the mapping to
    /// filenames is this host's, which is why an unknown slot has no file rather
    /// than a derived one: a typo should lose data loudly, not write somewhere
    /// nobody looks.
    fn path(&self, key: &str) -> Option<&PathBuf> {
        match key {
            "session" => self.session.as_ref(),
            "settings" => self.settings.as_ref(),
            _ => None,
        }
    }
}

impl Default for FsBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Backend for FsBackend {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        let Some(path) = self.path(key) else {
            return Ok(None);
        };
        match std::fs::read(path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(StoreError::Backend(error.to_string())),
        }
    }

    async fn put(&self, key: &str, bytes: &[u8]) -> Result<(), StoreError> {
        let Some(path) = self.path(key) else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(path, bytes).map_err(|error| StoreError::Backend(error.to_string()))
    }

    async fn delete(&self, key: &str) -> Result<(), StoreError> {
        let Some(path) = self.path(key) else {
            return Ok(());
        };
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(StoreError::Backend(error.to_string())),
        }
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>, StoreError> {
        let mut keys = Vec::new();
        for key in ["session", "settings"] {
            if key.starts_with(prefix) && self.path(key).is_some_and(|path| path.exists()) {
                keys.push(key.to_string());
            }
        }
        Ok(keys)
    }

    async fn scan(&self, start: &str, end: &str) -> Result<Vec<String>, StoreError> {
        let mut keys = self.list("").await?;
        keys.retain(|key| key.as_str() >= start && key.as_str() < end);
        keys.sort();
        Ok(keys)
    }

    /// Two files, written in order.
    ///
    /// Not atomic, and it does not pretend to be: this host has a fixed two-slot
    /// key space and nothing here writes a pair that must land together. A
    /// backend whose consumers need real batches wants redb, which muniment
    /// already ships.
    async fn apply(&self, ops: &[WriteOp]) -> Result<(), StoreError> {
        for op in ops {
            match op {
                WriteOp::Put { key, value } => self.put(key, value).await?,
                WriteOp::Delete { key } => self.delete(key).await?,
            }
        }
        Ok(())
    }
}
