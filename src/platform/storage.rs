//! Browser localStorage and native application-data persistence for sessions and preferences.

use std::collections::BTreeMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::core::identity::GameId;
use crate::multiplayer::model::AuthSession;

#[cfg(target_arch = "wasm32")]
const STORAGE_PREFIX: &str = "stellarion.v1.";

/// Small local profile; authoritative multiplayer state never belongs here.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct ClientProfile {
    /// Restorable anonymous authentication session.
    pub session: Option<AuthSession>,
    /// Recently opened game identifiers used only as a convenience hint.
    pub recent_games: Vec<GameId>,
    /// Last lobby display name entered by this installation.
    pub display_name: String,
}

impl ClientProfile {
    /// Adds a recent game once and bounds the convenience list to twenty entries.
    pub fn remember_game(&mut self, game_id: GameId) {
        self.recent_games.retain(|existing| existing != &game_id);
        self.recent_games.insert(0, game_id);
        self.recent_games.truncate(20);
    }
}

/// Synchronous key/value abstraction implemented separately for browser and native clients.
pub trait ClientStorage: Send + Sync {
    /// Loads one UTF-8 value, returning `None` when it has never been stored.
    fn load(&self, key: &str) -> Result<Option<String>, StorageError>;
    /// Stores one UTF-8 value.
    fn store(&self, key: &str, value: &str) -> Result<(), StorageError>;
    /// Removes one optional value.
    fn remove(&self, key: &str) -> Result<(), StorageError>;
}

/// Loads the standard local client profile.
pub fn load_profile(storage: &dyn ClientStorage) -> Result<ClientProfile, StorageError> {
    storage.load("client-profile")?.map_or(Ok(ClientProfile::default()), |json| {
        serde_json::from_str(&json).map_err(|error| StorageError::InvalidData(error.to_string()))
    })
}

/// Stores the standard local client profile as JSON.
pub fn save_profile(
    storage: &dyn ClientStorage,
    profile: &ClientProfile,
) -> Result<(), StorageError> {
    let json = serde_json::to_string(profile)
        .map_err(|error| StorageError::InvalidData(error.to_string()))?;
    storage.store("client-profile", &json)
}

/// Client-local persistence failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum StorageError {
    /// Platform API or filesystem operation failed.
    #[error("local storage failed: {0}")]
    Platform(String),
    /// Stored JSON was malformed.
    #[error("local storage contains malformed data: {0}")]
    InvalidData(String),
    /// Key would escape or collide with the storage namespace.
    #[error("local storage key is invalid")]
    InvalidKey,
}

/// In-memory implementation used by platform-independent tests.
#[derive(Default)]
pub struct MemoryStorage {
    values: Mutex<BTreeMap<String, String>>,
}

impl ClientStorage for MemoryStorage {
    /// Loads a cloned in-memory value.
    fn load(&self, key: &str) -> Result<Option<String>, StorageError> {
        validate_key(key)?;
        self.values
            .lock()
            .map(|values| values.get(key).cloned())
            .map_err(|_| StorageError::Platform("memory storage lock was poisoned".to_string()))
    }

    /// Inserts or replaces an in-memory value.
    fn store(&self, key: &str, value: &str) -> Result<(), StorageError> {
        validate_key(key)?;
        self.values
            .lock()
            .map_err(|_| StorageError::Platform("memory storage lock was poisoned".to_string()))?
            .insert(key.to_string(), value.to_string());
        Ok(())
    }

    /// Removes an in-memory value when present.
    fn remove(&self, key: &str) -> Result<(), StorageError> {
        validate_key(key)?;
        self.values
            .lock()
            .map_err(|_| StorageError::Platform("memory storage lock was poisoned".to_string()))?
            .remove(key);
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
/// Native storage rooted in the operating system's per-user config directory.
pub struct NativeStorage {
    root: std::path::PathBuf,
    /// Holding this handle keeps the primary installation profile exclusive to one process.
    _instance_lock: Option<std::fs::File>,
    temporary_instance: bool,
}

#[cfg(not(target_arch = "wasm32"))]
impl NativeStorage {
    /// Resolves Stellarion's platform-appropriate per-user configuration directory.
    pub fn new() -> Result<Self, StorageError> {
        let project =
            directories::ProjectDirs::from("io", "tvdboom", "Stellarion").ok_or_else(|| {
                StorageError::Platform("application-data directory unavailable".to_string())
            })?;
        Self::from_primary_root(project.config_local_dir().to_path_buf())
    }

    /// Uses an exclusive process lock for the persistent profile and isolates extra instances.
    fn from_primary_root(primary_root: std::path::PathBuf) -> Result<Self, StorageError> {
        std::fs::create_dir_all(&primary_root)
            .map_err(|error| StorageError::Platform(error.to_string()))?;
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(primary_root.join("instance.lock"))
            .map_err(|error| StorageError::Platform(error.to_string()))?;

        match fs2::FileExt::try_lock_exclusive(&lock) {
            Ok(()) => Ok(Self {
                root: primary_root,
                _instance_lock: Some(lock),
                temporary_instance: false,
            }),
            Err(error) if error.raw_os_error() == fs2::lock_contended_error().raw_os_error() => {
                let unique = format!(
                    "stellarion-instance-{}-{}",
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |duration| duration.as_nanos())
                );
                let root = std::env::temp_dir().join(unique);
                std::fs::create_dir_all(&root)
                    .map_err(|error| StorageError::Platform(error.to_string()))?;
                Ok(Self {
                    root,
                    _instance_lock: None,
                    temporary_instance: true,
                })
            },
            Err(error) => Err(StorageError::Platform(error.to_string())),
        }
    }

    /// Resolves one already-validated key below the configured root.
    fn path(&self, key: &str) -> Result<std::path::PathBuf, StorageError> {
        validate_key(key)?;
        Ok(self.root.join(format!("{key}.json")))
    }

    #[cfg(test)]
    /// Creates native storage at an isolated test directory.
    fn at(root: std::path::PathBuf) -> Self {
        Self {
            root,
            _instance_lock: None,
            temporary_instance: false,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for NativeStorage {
    fn drop(&mut self) {
        if self.temporary_instance {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
/// Removes a file if present while preserving other filesystem errors.
fn remove_file_if_present(path: &std::path::Path) -> Result<(), StorageError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(StorageError::Platform(error.to_string())),
    }
}

#[cfg(not(target_arch = "wasm32"))]
/// Reads a UTF-8 file while distinguishing absence from other failures.
fn read_optional_file(path: &std::path::Path) -> Result<Option<String>, StorageError> {
    match std::fs::read_to_string(path) {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(StorageError::Platform(error.to_string())),
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl ClientStorage for NativeStorage {
    /// Reads one optional native configuration file.
    fn load(&self, key: &str) -> Result<Option<String>, StorageError> {
        let path = self.path(key)?;
        if let Some(value) = read_optional_file(&path)? {
            return Ok(Some(value));
        }
        read_optional_file(&path.with_extension("json.bak"))
    }

    /// Atomically replaces a value while retaining a recoverable backup across interruptions.
    fn store(&self, key: &str, value: &str) -> Result<(), StorageError> {
        use std::io::Write as _;

        let path = self.path(key)?;
        std::fs::create_dir_all(&self.root)
            .map_err(|error| StorageError::Platform(error.to_string()))?;
        let temporary = path.with_extension("json.tmp");
        let backup = path.with_extension("json.bak");
        let mut file = std::fs::File::create(&temporary)
            .map_err(|error| StorageError::Platform(error.to_string()))?;
        file.write_all(value.as_bytes())
            .and_then(|()| file.sync_all())
            .map_err(|error| StorageError::Platform(error.to_string()))?;
        drop(file);

        let moved_primary = if path.exists() {
            remove_file_if_present(&backup)?;
            std::fs::rename(&path, &backup)
                .map_err(|error| StorageError::Platform(error.to_string()))?;
            true
        } else {
            false
        };
        if let Err(error) = std::fs::rename(&temporary, &path) {
            if moved_primary {
                let _ = std::fs::rename(&backup, &path);
            }
            let _ = remove_file_if_present(&temporary);
            return Err(StorageError::Platform(error.to_string()));
        }
        let _ = remove_file_if_present(&backup);
        Ok(())
    }

    /// Removes one native configuration file when present.
    fn remove(&self, key: &str) -> Result<(), StorageError> {
        let path = self.path(key)?;
        remove_file_if_present(&path)?;
        remove_file_if_present(&path.with_extension("json.tmp"))?;
        remove_file_if_present(&path.with_extension("json.bak"))
    }
}

#[cfg(target_arch = "wasm32")]
/// Browser implementation backed by origin-scoped localStorage.
pub struct BrowserStorage;

#[cfg(target_arch = "wasm32")]
impl BrowserStorage {
    /// Creates browser storage after confirming localStorage is available.
    pub fn new() -> Result<Self, StorageError> {
        browser_storage()?;
        Ok(Self)
    }
}

#[cfg(target_arch = "wasm32")]
impl ClientStorage for BrowserStorage {
    /// Loads one origin-scoped browser value.
    fn load(&self, key: &str) -> Result<Option<String>, StorageError> {
        validate_key(key)?;
        browser_storage()?.get_item(&format!("{STORAGE_PREFIX}{key}")).map_err(js_storage_error)
    }

    /// Stores one origin-scoped browser value.
    fn store(&self, key: &str, value: &str) -> Result<(), StorageError> {
        validate_key(key)?;
        browser_storage()?
            .set_item(&format!("{STORAGE_PREFIX}{key}"), value)
            .map_err(js_storage_error)
    }

    /// Removes one origin-scoped browser value.
    fn remove(&self, key: &str) -> Result<(), StorageError> {
        validate_key(key)?;
        browser_storage()?.remove_item(&format!("{STORAGE_PREFIX}{key}")).map_err(js_storage_error)
    }
}

#[cfg(target_arch = "wasm32")]
/// Returns the current browser's localStorage object.
fn browser_storage() -> Result<web_sys::Storage, StorageError> {
    web_sys::window()
        .ok_or_else(|| StorageError::Platform("browser window unavailable".to_string()))?
        .local_storage()
        .map_err(js_storage_error)?
        .ok_or_else(|| StorageError::Platform("localStorage is disabled".to_string()))
}

#[cfg(target_arch = "wasm32")]
/// Converts an opaque JavaScript exception into a useful storage error.
fn js_storage_error(value: wasm_bindgen::JsValue) -> StorageError {
    StorageError::Platform(format!("{value:?}"))
}

/// Restricts keys to a portable flat namespace.
fn validate_key(key: &str) -> Result<(), StorageError> {
    if !key.is_empty()
        && key.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        Ok(())
    } else {
        Err(StorageError::InvalidKey)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::identity::UserId;

    #[test]
    /// Profile/session persistence round-trips without containing game state.
    fn profile_round_trips_in_memory() {
        let storage = MemoryStorage::default();
        let mut profile = ClientProfile {
            session: Some(AuthSession::new(UserId::new("user"), "access", "refresh")),
            display_name: "Nova".to_string(),
            ..ClientProfile::default()
        };
        profile.remember_game(GameId::new("game-1"));
        profile.remember_game(GameId::new("game-1"));
        save_profile(&storage, &profile).unwrap();
        let loaded = load_profile(&storage).unwrap();
        assert_eq!(loaded.display_name, "Nova");
        assert_eq!(loaded.recent_games, vec![GameId::new("game-1")]);
        assert_eq!(loaded.session.unwrap().user_id, UserId::new("user"));
    }

    #[test]
    /// Keys cannot escape the platform storage root.
    fn rejects_unsafe_keys() {
        let storage = MemoryStorage::default();
        for key in ["", "../session", "a/b", "a b"] {
            assert_eq!(storage.store(key, "value"), Err(StorageError::InvalidKey));
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    /// Native replacement and interrupted-write recovery preserve a complete prior value.
    fn native_storage_replaces_atomically_and_recovers_backup() {
        let unique = format!(
            "stellarion-storage-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |duration| duration.as_nanos())
        );
        let root = std::env::temp_dir().join(unique);
        let storage = NativeStorage::at(root.clone());

        storage.store("profile", "first").unwrap();
        storage.store("profile", "second").unwrap();
        assert_eq!(storage.load("profile").unwrap().as_deref(), Some("second"));

        let primary = storage.path("profile").unwrap();
        let backup = primary.with_extension("json.bak");
        std::fs::rename(&primary, &backup).unwrap();
        std::fs::write(primary.with_extension("json.tmp"), "partial").unwrap();
        assert_eq!(storage.load("profile").unwrap().as_deref(), Some("second"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    /// Concurrent desktop processes never reuse one anonymous multiplayer identity.
    fn concurrent_native_instances_use_isolated_profiles() {
        let unique = format!(
            "stellarion-storage-lock-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |duration| duration.as_nanos())
        );
        let root = std::env::temp_dir().join(unique);
        let primary = NativeStorage::from_primary_root(root.clone()).unwrap();
        primary.store("client-profile", "primary").unwrap();

        let secondary = NativeStorage::from_primary_root(root.clone()).unwrap();
        assert!(secondary.temporary_instance);
        assert_ne!(secondary.root, root);
        assert_eq!(secondary.load("client-profile").unwrap(), None);
        let secondary_root = secondary.root.clone();
        secondary.store("client-profile", "secondary").unwrap();
        assert_eq!(primary.load("client-profile").unwrap().as_deref(), Some("primary"));

        drop(secondary);
        assert!(!secondary_root.exists());
        drop(primary);
        std::fs::remove_dir_all(root).unwrap();
    }
}
