// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::primary::PrimaryWorkerMessage;
use config::SharedCommittee;
use crypto::traits::VerifyingKey;
use network::PrimaryToWorkerNetwork;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use tokio::{
    sync::{mpsc::Receiver, watch},
    task::JoinHandle,
};
use types::{Certificate, ReconfigureNotification, Round};

/// Receives the highest round reached by consensus and update it for all tasks.
pub struct StateHandler<PublicKey: VerifyingKey> {
    /// The public key of this authority.
    name: PublicKey,
    /// The committee information.
    committee: SharedCommittee<PublicKey>,
    /// The current consensus round (used for cleanup).
    consensus_round: Arc<AtomicU64>,
    /// Receives the ordered certificates from consensus.
    rx_consensus: Receiver<Certificate<PublicKey>>,
    /// Receives notifications to reconfigure the system.
    rx_reconfigure: Receiver<ReconfigureNotification<PublicKey>>,
    /// Channel to signal committee changes.
    tx_reconfigure: watch::Sender<ReconfigureNotification<PublicKey>>,
    /// The latest round committed by consensus.
    last_committed_round: Round,
    /// A network sender to notify our workers of cleanup events.
    worker_network: PrimaryToWorkerNetwork,
}

impl<PublicKey: VerifyingKey> StateHandler<PublicKey> {
    pub fn spawn(
        name: PublicKey,
        committee: SharedCommittee<PublicKey>,
        consensus_round: Arc<AtomicU64>,
        rx_consensus: Receiver<Certificate<PublicKey>>,
        rx_reconfigure: Receiver<ReconfigureNotification<PublicKey>>,
        tx_reconfigure: watch::Sender<ReconfigureNotification<PublicKey>>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            Self {
                name,
                committee,
                consensus_round,
                rx_consensus,
                rx_reconfigure,
                tx_reconfigure,
                last_committed_round: 0,
                worker_network: Default::default(),
            }
            .run()
            .await;
        })
    }

    async fn handle_sequenced(&mut self, certificate: Certificate<PublicKey>) {
        // TODO [issue #9]: Re-include batch digests that have not been sequenced into our next block.

        let round = certificate.round();
        if round > self.last_committed_round {
            self.last_committed_round = round;

            // Trigger cleanup on the primary.
            self.consensus_round.store(round, Ordering::Relaxed);

            // Trigger cleanup on the workers..
            let addresses = self
                .committee
                .load()
                .our_workers(&self.name)
                .expect("Our public key or worker id is not in the committee")
                .into_iter()
                .map(|x| x.primary_to_worker)
                .collect();
            let message = PrimaryWorkerMessage::<PublicKey>::Cleanup(round);
            self.worker_network.broadcast(addresses, &message).await;
        }
    }

    async fn run(&mut self) {
        loop {
            tokio::select! {
                Some(certificate) = self.rx_consensus.recv() => {
                    self.handle_sequenced(certificate).await;
                },

                Some(message) = self.rx_reconfigure.recv() => {
                    let shutdown = match &message {
                        ReconfigureNotification::NewCommittee(committee) => {
                            // Update the committee.
                            self.committee.swap(Arc::new(committee.clone()));
                            tracing::debug!("Committee updated to {}", self.committee);

                            // Trigger cleanup on the primary.
                            self.consensus_round.store(0, Ordering::Relaxed);

                            false
                        },
                        ReconfigureNotification::Shutdown => true,
                    };

                    // Notify all other tasks.
                    self.tx_reconfigure
                        .send(message)
                        .expect("Reconfigure channel dropped");

                    // Exit only when we are sure that all the other tasks received
                    // the shutdown message.
                    if shutdown {
                        self.tx_reconfigure.closed().await;
                        return;
                    }
                }
            }
        }
    }
}
