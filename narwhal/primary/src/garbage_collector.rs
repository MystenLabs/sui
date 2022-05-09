// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::primary::PrimaryWorkerMessage;
use config::Committee;
use crypto::traits::VerifyingKey;
use multiaddr::Multiaddr;
use network::PrimaryToWorkerNetwork;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use tokio::sync::mpsc::Receiver;
use types::Certificate;

/// Receives the highest round reached by consensus and update it for all tasks.
pub struct GarbageCollector<PublicKey: VerifyingKey> {
    /// The current consensus round (used for cleanup).
    consensus_round: Arc<AtomicU64>,
    /// Receives the ordered certificates from consensus.
    rx_consensus: Receiver<Certificate<PublicKey>>,
    /// The network addresses of our workers.
    addresses: Vec<Multiaddr>,
    /// A network sender to notify our workers of cleanup events.
    worker_network: PrimaryToWorkerNetwork,
}

impl<PublicKey: VerifyingKey> GarbageCollector<PublicKey> {
    pub fn spawn(
        name: &PublicKey,
        committee: &Committee<PublicKey>,
        consensus_round: Arc<AtomicU64>,
        rx_consensus: Receiver<Certificate<PublicKey>>,
    ) {
        let addresses = committee
            .our_workers(name)
            .expect("Our public key or worker id is not in the committee")
            .into_iter()
            .map(|x| x.primary_to_worker)
            .collect();

        tokio::spawn(async move {
            Self {
                consensus_round,
                rx_consensus,
                addresses,
                worker_network: Default::default(),
            }
            .run()
            .await;
        });
    }

    async fn run(&mut self) {
        let mut last_committed_round = 0;
        while let Some(certificate) = self.rx_consensus.recv().await {
            // TODO [issue #9]: Re-include batch digests that have not been sequenced into our next block.

            let round = certificate.round();
            if round > last_committed_round {
                last_committed_round = round;

                // Trigger cleanup on the primary.
                self.consensus_round.store(round, Ordering::Relaxed);

                // Trigger cleanup on the workers..
                let message = PrimaryWorkerMessage::<PublicKey>::Cleanup(round);
                self.worker_network
                    .broadcast(self.addresses.clone(), &message)
                    .await;
            }
        }
    }
}
