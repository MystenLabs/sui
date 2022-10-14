// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::future::Future;
use std::sync::Arc;
use tokio::sync::{broadcast, OwnedRwLockWriteGuard, RwLock};
use tokio::task::JoinHandle;
use tracing::{debug, trace};

use tap::TapFallible;

use sui_types::error::{SuiError, SuiResult};

/// CancellableSpawner allows spawned tasks to be cancelled easily without keeping track of every
/// JoinHandle that gets created.
pub struct CancellableSpawner {
    cancel_tx: broadcast::Sender<()>,
    spawn_lock: Arc<RwLock<()>>,
    task_lock: Arc<RwLock<()>>,
}

/// While a CancelGuard is held, no new tasks can be spawned on the CancellableSpawner.
pub struct CancelGuard(OwnedRwLockWriteGuard<()>);

impl CancellableSpawner {
    pub fn new() -> Self {
        let (cancel_tx, _) = broadcast::channel(1);
        Self {
            cancel_tx,
            spawn_lock: Default::default(),
            task_lock: Default::default(),
        }
    }

    /// Spawn a future via tokio::spawn - may fail if cancel_all_tasks has been called.
    pub fn spawn<F>(&self, fut: F) -> SuiResult<JoinHandle<()>>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        // if spawn_lock is held for write, the spawner is in the process of cancelling, so new
        // spawn attempts must fail.
        let _spawn_guard = self
            .spawn_lock
            .try_read()
            .tap_err(|_| {
                debug!("task could not be spawned, CancellableSpawner::cancel() is in progress")
            })
            .map_err(|_| SuiError::TaskSpawnError)?;

        // While spawn_lock is held, task_lock.write() cannot be called, so try_read must succeed.
        let task_guard = self.task_lock.clone().try_read_owned().unwrap();
        let mut cancel_rx = self.cancel_tx.subscribe();

        Ok(tokio::spawn(async move {
            let _task_guard = task_guard;
            let recv = cancel_rx.recv();
            tokio::select! {
                _ = recv => {
                    debug!("task cancelled before completion");
                }
                _ = fut => {
                    trace!("task finished normally");
                }
            }
        }))
    }

    /// Cancel all tasks that were spawned via this instance.
    /// Returns a guard which, while held, prevents new tasks from spawning.
    pub async fn cancel_all_tasks(&self) -> CancelGuard {
        debug!("cancelling all tasks");
        // no new readers of task_lock can lock while spawn_lock is held for write, because
        // task_lock.read() is only called while spawn_lock is held.
        let spawn_guard = self.spawn_lock.clone().write_owned().await;

        debug!("sending cancel broadcast");
        let _ = self.cancel_tx.send(());

        // await all tasks exiting.
        let _ = self.task_lock.write().await;
        debug!("all tasks exited");

        CancelGuard(spawn_guard)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{distributions::Uniform, rngs::OsRng, Rng};
    use sui_macros::sui_test;
    use tokio::time::{sleep, Duration};

    #[sui_test]
    async fn test_cancellable_spawner() {
        telemetry_subscribers::init_for_testing();
        let spawner = CancellableSpawner::new();
        let dist = Uniform::new(10, 1000);

        let handles: Vec<_> = (0..1000)
            .map(|_| {
                spawner
                    .spawn(async move {
                        sleep(Duration::from_millis(OsRng.sample(dist))).await;
                    })
                    .unwrap()
            })
            .collect();

        sleep(Duration::from_millis(OsRng.sample(dist))).await;
        let guard = spawner.cancel_all_tasks().await;

        assert!(handles.into_iter().all(|h| h.is_finished()));

        // can't spawn while guard is held.
        spawner.spawn(async move {}).unwrap_err();
        std::mem::drop(guard);
        spawner.spawn(async move {}).unwrap();
    }
}
