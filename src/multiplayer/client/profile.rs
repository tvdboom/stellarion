//! Coalesced profile persistence, independent of the network task queue.

use super::*;
use crate::platform::storage::{save_profile, StorageError};

#[derive(Resource, Default)]
pub(super) struct ProfileWrites {
    task: Option<Task<(ClientProfile, Result<(), StorageError>)>>,
    saved: Option<ClientProfile>,
    retry_after: f32,
}

pub(super) fn flush_profile(
    runtime: Res<ClientRuntime>,
    mut writes: ResMut<ProfileWrites>,
    time: Res<Time>,
) {
    if let Some(task) = &mut writes.task {
        if let Some((profile, result)) = block_on(poll_once(task)) {
            writes.task = None;
            if result.is_ok() {
                writes.saved = Some(profile);
            } else {
                writes.retry_after = 5.;
            }
        }
    }
    writes.retry_after = (writes.retry_after - time.delta_secs()).max(0.);
    if writes.task.is_some()
        || writes.retry_after > 0.
        || writes.saved.as_ref() == Some(&runtime.profile)
    {
        return;
    }
    let profile = runtime.profile.clone();
    let storage = runtime.storage.clone();
    let write = async move {
        let result = save_profile(storage.as_ref(), &profile);
        (profile, result)
    };
    // Native fsync runs on the IO pool. Browser localStorage must use the JS thread.
    #[cfg(not(target_arch = "wasm32"))]
    {
        writes.task = Some(IoTaskPool::get().spawn(write));
    }
    #[cfg(target_arch = "wasm32")]
    {
        writes.task = Some(IoTaskPool::get().spawn_local(write));
    }
}
