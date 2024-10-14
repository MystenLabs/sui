// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::iter;

use futures::future::{self, Either};
use tokio::{signal, task::JoinHandle};
use tokio_util::sync::CancellationToken;

/// Manages cleanly exiting the process, either because one of its constituent services has stopped
/// or because an interrupt signal was sent to the process.
pub async fn graceful_shutdown(
    services: impl IntoIterator<Item = JoinHandle<()>>,
    cancel: CancellationToken,
) {
    let interrupt = async {
        signal::ctrl_c()
            .await
            .expect("Failed to listen for Ctrl-C signal");

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
    cancel.cancel();

    // Wait for the remaining services to finish.
    let _ = future::join_all(rest).await;
}
