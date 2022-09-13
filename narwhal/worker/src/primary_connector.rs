// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crypto::NetworkPublicKey;
use futures::{stream::FuturesUnordered, StreamExt};
use network::{ReliableNetwork2, WorkerToPrimaryNetwork};
use tokio::{sync::watch, task::JoinHandle};
use types::{metered_channel::Receiver, ReconfigureNotification, WorkerPrimaryMessage};

/// The maximum number of digests kept in memory waiting to be sent to the primary.
pub const MAX_PENDING_DIGESTS: usize = 10_000;

// Send batches' digests to the primary.
pub struct PrimaryConnector {
    /// The public key of this authority.
    primary_name: NetworkPublicKey,
    /// Receive reconfiguration updates.
    rx_reconfigure: watch::Receiver<ReconfigureNotification>,
    /// Input channel to receive the messages to send to the primary.
    rx_digest: Receiver<WorkerPrimaryMessage>,
    /// A network sender to send the batches' digests to the primary.
    primary_client: WorkerToPrimaryNetwork,
}

impl PrimaryConnector {
    #[must_use]
    pub fn spawn(
        primary_name: NetworkPublicKey,
        rx_reconfigure: watch::Receiver<ReconfigureNotification>,
        rx_digest: Receiver<WorkerPrimaryMessage>,
        primary_client: WorkerToPrimaryNetwork,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            Self {
                primary_name,
                rx_reconfigure,
                rx_digest,
                primary_client,
            }
            .run()
            .await;
        })
    }

    async fn run(&mut self) {
        let mut futures = FuturesUnordered::new();
        loop {
            tokio::select! {
                // Send the digest through the network.
                Some(digest) = self.rx_digest.recv() => {
                    if futures.len() >= MAX_PENDING_DIGESTS {
                        tracing::warn!("Primary unreachable: dropping {digest:?}");
                        continue;
                    }

                    let handle = self.primary_client.send(self.primary_name.to_owned(), &digest).await;
                    futures.push(handle);
                },

                // Trigger reconfigure.
                result = self.rx_reconfigure.changed() => {
                    result.expect("Committee channel dropped");
                    // TODO: Move logic to handle epoch & committee changes to wherever anemo
                    // network is managed after worker-to-worker interface is migrated.
                    if self.rx_reconfigure.borrow().clone() == ReconfigureNotification::Shutdown {
                        return
                    }
                }

                Some(_result) = futures.next() => ()
            }
        }
    }
}
