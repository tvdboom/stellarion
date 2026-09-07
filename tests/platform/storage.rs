use super::*;
use crate::core::identity::UserId;

#[cfg(not(target_arch = "wasm32"))]
impl NativeStorage {
    /// Creates native storage at an isolated test directory.
    fn at(root: std::path::PathBuf) -> Self {
        Self {
            root,
            file_access: Mutex::new(()),
            _instance_lock: None,
            temporary_instance: false,
        }
    }
}

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

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn concurrent_native_storage_calls_preserve_complete_values() {
    let root = std::env::temp_dir().join(format!(
        "stellarion-storage-threads-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos(),
    ));
    let storage = NativeStorage::at(root.clone());
    let values = (0..4).map(|index| format!("writer-{index}:").repeat(1024)).collect::<Vec<_>>();
    storage.store("profile", &values[0]).unwrap();
    let start = std::sync::Barrier::new(values.len());
    std::thread::scope(|scope| {
        for value in &values {
            let (storage, values, start) = (&storage, &values, &start);
            scope.spawn(move || {
                start.wait();
                for _ in 0..20 {
                    storage.store("profile", value).unwrap();
                    let loaded = storage.load("profile").unwrap().unwrap();
                    assert!(values.contains(&loaded), "a reader observed a partial write");
                }
            });
        }
    });
    storage.remove("profile").unwrap();
    assert_eq!(storage.load("profile").unwrap(), None);
    std::fs::remove_dir_all(root).unwrap();
}
