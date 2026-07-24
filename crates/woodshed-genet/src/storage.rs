//! The desktop [`Storage`]: `genet-state.json` in the same config dir the
//! xilem app uses (`ProjectDirs dev/Woodshed/Woodshed`), under its own
//! filename so the two apps never clobber each other during the migration.
//! The web host implements the same trait over OPFS.

use std::path::PathBuf;

use directories::ProjectDirs;
use woodshed_core::storage::Storage;

pub struct FsStorage {
    /// `None` when the platform exposes no config dir — persistence
    /// silently disabled, matching woodshed-xilem's posture.
    path: Option<PathBuf>,
}

impl FsStorage {
    pub fn new() -> Self {
        // `WOODSHED_STATE` points the session at another file. A scenario run
        // sets it to a scratch profile: without it, an automated run would read
        // and then overwrite the real practice session.
        if let Ok(path) = std::env::var("WOODSHED_STATE") {
            return Self {
                path: Some(PathBuf::from(path)),
            };
        }
        let path = ProjectDirs::from("dev", "Woodshed", "Woodshed")
            .map(|dirs| dirs.config_dir().join("genet-state.json"));
        Self { path }
    }
}

impl Storage for FsStorage {
    fn load(&self) -> Option<String> {
        std::fs::read_to_string(self.path.as_ref()?).ok()
    }

    fn save(&self, contents: &str) {
        let Some(path) = self.path.as_ref() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(path, contents) {
            eprintln!("[woodshed-genet] failed to persist session: {e}");
        }
    }
}
