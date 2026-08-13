//! Reading the practice session back out of whichever store opened.
//!
//! Lifted out of `boot_state` when the persona gate arrived: a machine that
//! asks which persona to practise as cannot restore at boot, because the key
//! that decrypts the session is the answer to the question still on screen. So
//! the restore has two callers and one run, and it lives here rather than in
//! either of them.

use woodshed_core::storage::SessionStore;
use woodshed_views::stage::UiState;

use crate::storage::HostBackend;

/// Apply the stored session and application settings to a fresh [`UiState`].
///
/// A corrupt file is reported and skipped rather than raised: a session that
/// will not parse must not stop the application opening, and the next save
/// replaces it. A legacy flat session migrates its settings when no split
/// settings file exists yet.
///
/// Settings ride in through [`UiState::apply_persisted`] rather than being
/// assigned, because several pieces of state are derived from them (the
/// transport's bpm, the tuning and root dropdowns, the legacy relation-set
/// migration). That is also why a store holding settings but no session applies
/// neither: the derivations live on the session path. Faithful to what
/// `boot_state` did before this extraction, and worth a look on its own.
pub fn restore(storage: &SessionStore<HostBackend>, ui: &mut UiState) {
    let mut app_settings = storage
        .load_settings()
        .and_then(|json| match serde_json::from_str(&json) {
            Ok(settings) => Some(settings),
            Err(error) => {
                eprintln!("[woodshed-genet] ignoring corrupt application settings: {error}");
                None
            }
        })
        .unwrap_or_default();
    let Some(json) = storage.load() else {
        return;
    };
    match woodshed_core::storage::decode_session(&json) {
        Ok(loaded) => {
            if storage.load_settings().is_none() {
                if let Some(legacy) = loaded.legacy_settings {
                    app_settings = legacy;
                }
            }
            ui.apply_persisted(&loaded.session, app_settings);
        }
        Err(error) => eprintln!("[woodshed-genet] ignoring corrupt session: {error}"),
    }
}
