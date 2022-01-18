// Copyright(C) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0
use config::WorkerId;
use crypto::Digest;
use store::Store;
use tokio::sync::mpsc::Receiver;

use crate::primary::PayloadToken;

/// Receives batches' digests of other authorities. These are only needed to verify incoming
/// headers (i.e.. make sure we have their payload).
pub struct PayloadReceiver {
    /// The persistent storage.
    store: Store<(Digest, WorkerId), PayloadToken>,
    /// Receives batches' digests from the network.
    rx_workers: Receiver<(Digest, WorkerId)>,
}

impl PayloadReceiver {
    pub fn spawn(
        store: Store<(Digest, WorkerId), PayloadToken>,
        rx_workers: Receiver<(Digest, WorkerId)>,
    ) {
        tokio::spawn(async move {
            Self { store, rx_workers }.run().await;
        });
    }

    async fn run(&mut self) {
        while let Some((digest, worker_id)) = self.rx_workers.recv().await {
            self.store.write((digest, worker_id), 0u8).await;
        }
    }
}
