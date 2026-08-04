// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::FutureExt as _;
use tokio::task::{JoinHandle, JoinSet};

use crate::error::{ConsensusError, ConsensusResult};

/// Awaits the task and propagates its panic if it panicked. Task cancellation is ignored.
pub(crate) async fn join_and_propagate_panic<T>(task: JoinHandle<T>) {
    if let Err(error) = task.await
        && error.is_panic()
    {
        std::panic::resume_unwind(error.into_panic());
    }
}

/// Aborts all tasks in the set and awaits them, propagating the first panic among them.
/// Task cancellations are ignored.
pub(crate) async fn shutdown_join_set<T: 'static>(tasks: &mut JoinSet<T>) {
    tasks.abort_all();
    // Drain the whole set before propagating a panic, so no task is dropped without
    // being awaited.
    let mut first_panic = None;
    while let Some(result) = tasks.join_next().await {
        if let Err(error) = result
            && error.is_panic()
            && first_panic.is_none()
        {
            first_panic = Some(error.into_panic());
        }
    }
    if let Some(panic) = first_panic {
        std::panic::resume_unwind(panic);
    }
}

/// Reaps the task if it has finished, propagating its panic if it panicked.
/// Returns true when the task has been reaped and can be dropped.
pub(crate) fn reap_finished_task(task: &mut JoinHandle<()>) -> bool {
    if !task.is_finished() {
        return false;
    }
    // A finished task completes immediately when polled.
    if let Some(Err(error)) = task.now_or_never()
        && error.is_panic()
    {
        std::panic::resume_unwind(error.into_panic());
    }
    true
}

/// Runs the closure on the blocking pool, like tokio::task::spawn_blocking, but resolves
/// to ConsensusError::Shutdown instead of a JoinError when the task is cancelled by
/// runtime shutdown. Panics from the closure are resumed on the calling thread.
pub(crate) async fn spawn_blocking<F, T>(f: F) -> ConsensusResult<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(spawn_blocking_join_error)
}

/// Converts a JoinError from a spawn_blocking task into a ConsensusError.
/// Panics from the blocking task are resumed on the calling thread. Otherwise the task
/// was cancelled, which only happens when the runtime is shutting down.
fn spawn_blocking_join_error(e: tokio::task::JoinError) -> ConsensusError {
    if e.is_panic() {
        std::panic::resume_unwind(e.into_panic());
    }
    ConsensusError::Shutdown
}

#[cfg(test)]
mod test {
    use super::*;

    /// Reproduces the production scenario: spawn_blocking racing with runtime shutdown
    /// gets its task cancelled instead of run, so its JoinHandle resolves to
    /// JoinError::Cancelled.
    #[tokio::test]
    async fn test_spawn_blocking_join_error_on_runtime_shutdown() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .build()
            .unwrap();
        let handle = runtime.handle().clone();
        runtime.shutdown_background();

        let cancelled = handle.spawn_blocking(|| ());
        let join_error = cancelled.await.unwrap_err();
        assert!(join_error.is_cancelled());
        assert!(matches!(
            spawn_blocking_join_error(join_error),
            ConsensusError::Shutdown
        ));
    }

    #[tokio::test]
    #[should_panic(expected = "boom")]
    async fn test_spawn_blocking_resumes_panic() {
        let _ = spawn_blocking(|| panic!("boom")).await;
    }
}
