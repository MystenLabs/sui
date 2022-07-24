// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use config::Committee;
use crypto::traits::VerifyingKey;
use futures::{stream::FuturesUnordered, StreamExt};
use network::WorkerToPrimaryNetwork;
use tokio::{
    sync::{mpsc::Receiver, watch},
    task::JoinHandle,
};
use types::{ReconfigureNotification, WorkerPrimaryMessage};

/// The maximum number of digests kept in memory waiting to be sent to the primary.
pub const MAX_PENDING_DIGESTS: usize = 10_000;

// Send batches' digests to the primary.
pub struct PrimaryConnector<PublicKey: VerifyingKey> {
    /// The public key of this authority.
    name: PublicKey,
    /// The committee information.
    committee: Committee<PublicKey>,
    /// Receive reconfiguration updates.
    rx_reconfigure: watch::Receiver<ReconfigureNotification<PublicKey>>,
    /// Input channel to receive the messages to send to the primary.
    rx_digest: Receiver<WorkerPrimaryMessage<PublicKey>>,
    /// A network sender to send the batches' digests to the primary.
    primary_client: WorkerToPrimaryNetwork,
}

impl<PublicKey: VerifyingKey> PrimaryConnector<PublicKey> {
    pub fn spawn(
        name: PublicKey,
        committee: Committee<PublicKey>,
        rx_reconfigure: watch::Receiver<ReconfigureNotification<PublicKey>>,
        rx_digest: Receiver<WorkerPrimaryMessage<PublicKey>>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            Self {
                name,
                committee,
                rx_reconfigure,
                rx_digest,
                primary_client: WorkerToPrimaryNetwork::default(),
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

                    let address = self.committee
                        .primary(&self.name)
                        .expect("Our public key is not in the committee")
                        .worker_to_primary;
                    let handle = self.primary_client.send(address, &digest).await;
                    futures.push(handle);
                },

                // Trigger reconfigure.
                result = self.rx_reconfigure.changed() => {
                    result.expect("Committee channel dropped");
                    let message = self.rx_reconfigure.borrow().clone();
                    match message {
                        ReconfigureNotification::NewCommittee(new_committee) => {
                            self.committee = new_committee;
                            tracing::debug!("Committee updated to {}", self.committee);
                        },
                        ReconfigureNotification::Shutdown => return
                    }
                }

                Some(_result) = futures.next() => ()
            }
        }
    }
}
