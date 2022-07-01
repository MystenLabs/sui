// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::primary::{ConsensusPrimaryMessage, PrimaryWorkerMessage, Reconfigure};
use config::{Committee, SharedCommittee};
use crypto::traits::VerifyingKey;
use network::PrimaryToWorkerNetwork;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use tokio::sync::{mpsc::Receiver, watch};
use types::{Certificate, Round};

/// Receives the highest round reached by consensus and update it for all tasks.
pub struct StateHandler<PublicKey: VerifyingKey> {
    /// The public key of this authority.
    name: PublicKey,
    /// The committee information.
    committee: SharedCommittee<PublicKey>,
    /// The current consensus round (used for cleanup).
    consensus_round: Arc<AtomicU64>,
    /// Receives the ordered certificates from consensus.
    rx_consensus: Receiver<ConsensusPrimaryMessage<PublicKey>>,
    /// Channel to signal committee changes.
    tx_reconfigure: watch::Sender<Reconfigure<PublicKey>>,
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
        rx_consensus: Receiver<ConsensusPrimaryMessage<PublicKey>>,
        tx_reconfigure: watch::Sender<Reconfigure<PublicKey>>,
    ) {
        tokio::spawn(async move {
            Self {
                name,
                committee,
                consensus_round,
                rx_consensus,
                tx_reconfigure,
                last_committed_round: 0,
                worker_network: Default::default(),
            }
            .run()
            .await;
        });
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
                .our_workers(&self.name)
                .expect("Our public key or worker id is not in the committee")
                .into_iter()
                .map(|x| x.primary_to_worker)
                .collect();
            let message = PrimaryWorkerMessage::<PublicKey>::Cleanup(round);
            self.worker_network.broadcast(addresses, &message).await;
        }
    }

    fn update_committee(&mut self, new_committee: Committee<PublicKey>) {
        self.committee.update_committee(new_committee);
    }

    async fn run(&mut self) {
        while let Some(message) = self.rx_consensus.recv().await {
            match message {
                ConsensusPrimaryMessage::Sequenced(certificate) => {
                    self.handle_sequenced(certificate).await;
                }
                ConsensusPrimaryMessage::Committee(committee) => {
                    // Update the committee.
                    self.update_committee(committee.clone());

                    // Trigger cleanup on the primary.
                    self.consensus_round.store(0, Ordering::Relaxed);

                    // Notify all other tasks.
                    let message = Reconfigure::NewCommittee(committee);
                    self.tx_reconfigure
                        .send(message)
                        .expect("Reconfigure channel dropped");
                }
                ConsensusPrimaryMessage::Shutdown(token) => {
                    let message = Reconfigure::Shutdown(token);
                    self.tx_reconfigure
                        .send(message)
                        .expect("Reconfigure channel dropped");
                    break;
                }
            }
        }
    }
}
