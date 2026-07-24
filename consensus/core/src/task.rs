// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::FutureExt as _;
use tokio::task::{JoinHandle, JoinSet};

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
