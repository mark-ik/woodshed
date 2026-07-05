//! The desktop [`Storage`]: `serval-state.json` in the same config dir the
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
        let path = ProjectDirs::from("dev", "Woodshed", "Woodshed")
            .map(|dirs| dirs.config_dir().join("serval-state.json"));
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
            eprintln!("[woodshed-serval] failed to persist session: {e}");
        }
    }
}
