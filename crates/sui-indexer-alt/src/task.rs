// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::iter;

use futures::future::{self, Either};
use tokio::{signal, sync::oneshot, task::JoinHandle};
use tokio_util::sync::CancellationToken;

/// Manages cleanly exiting the process, either because one of its constituent services has stopped
/// or because an interrupt signal was sent to the process.
///
/// Returns the exit values from all services that exited successfully.
pub async fn graceful_shutdown<T>(
    services: impl IntoIterator<Item = JoinHandle<T>>,
    cancel: CancellationToken,
) -> Vec<T> {
    // If the service is naturalling winding down, we don't need to wait for an interrupt signal.
    // This channel is used to short-circuit the await in that case.
    let (cancel_ctrl_c_tx, cancel_ctrl_c_rx) = oneshot::channel();

    let interrupt = async {
        tokio::select! {
            _ = cancel_ctrl_c_rx => {}
            _ = cancel.cancelled() => {}
            _ = signal::ctrl_c() => cancel.cancel(),
        }

        None
    };

    tokio::pin!(interrupt);
    let futures: Vec<_> = services
        .into_iter()
        .map(|s| Either::Left(Box::pin(async move { s.await.ok() })))
        .chain(iter::once(Either::Right(interrupt)))
        .collect();

    // Wait for the first service to finish, or for an interrupt signal.
    let (first, _, rest) = future::select_all(futures).await;
    let _ = cancel_ctrl_c_tx.send(());

    // Wait for the remaining services to finish.
    let mut results = vec![];
    results.extend(first);
    results.extend(future::join_all(rest).await.into_iter().flatten());
    results
}
