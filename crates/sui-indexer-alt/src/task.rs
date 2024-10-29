// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::iter;

use futures::future::{self, Either};
use tokio::{signal, sync::oneshot, task::JoinHandle};
use tokio_util::sync::CancellationToken;

/// Manages cleanly exiting the process, either because one of its constituent services has stopped
/// or because an interrupt signal was sent to the process.
pub async fn graceful_shutdown(
    services: impl IntoIterator<Item = JoinHandle<()>>,
    cancel: CancellationToken,
) {
    // If the service is naturalling winding down, we don't need to wait for an interrupt signal.
    // This channel is used to short-circuit the await in that case.
    let (cancel_ctrl_c_tx, cancel_ctrl_c_rx) = oneshot::channel();

    let interrupt = async {
        tokio::select! {
            _ = cancel_ctrl_c_rx => {}
            _ = cancel.cancelled() => {}
            _ = signal::ctrl_c() => cancel.cancel(),
        }

        Ok(())
    };

    tokio::pin!(interrupt);
    let futures: Vec<_> = services
        .into_iter()
        .map(Either::Left)
        .chain(iter::once(Either::Right(interrupt)))
        .collect();

    // Wait for the first service to finish, or for an interrupt signal.
    let (_, _, rest) = future::select_all(futures).await;
    let _ = cancel_ctrl_c_tx.send(());

    // Wait for the remaining services to finish.
    let _ = future::join_all(rest).await;
}
