// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use config::{SharedCommittee, SharedWorkerCache, WorkerCache, WorkerIndex};
use crypto::PublicKey;
use std::{collections::BTreeMap, sync::Arc};
use tap::TapOptional;
use tokio::{sync::watch, task::JoinHandle};
use tracing::{info, warn};
use types::{metered_channel::Receiver, Certificate, ReconfigureNotification, Round};

/// Receives the highest round reached by consensus and update it for all tasks.
pub struct StateHandler {
    /// The public key of this authority.
    name: PublicKey,
    /// The committee information.
    committee: SharedCommittee,
    /// The worker information cache.
    worker_cache: SharedWorkerCache,
    /// Receives the ordered certificates from consensus.
    rx_consensus: Receiver<Certificate>,
    /// Signals a new consensus round
    tx_consensus_round_updates: watch::Sender<u64>,
    /// Receives notifications to reconfigure the system.
    rx_reconfigure: Receiver<ReconfigureNotification>,
    /// Channel to signal committee changes.
    tx_reconfigure: watch::Sender<ReconfigureNotification>,
    /// The latest round committed by consensus.
    last_committed_round: Round,
}

impl StateHandler {
    #[must_use]
    pub fn spawn(
        name: PublicKey,
        committee: SharedCommittee,
        worker_cache: SharedWorkerCache,
        rx_consensus: Receiver<Certificate>,
        tx_consensus_round_updates: watch::Sender<u64>,
        rx_reconfigure: Receiver<ReconfigureNotification>,
        tx_reconfigure: watch::Sender<ReconfigureNotification>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            Self {
                name,
                committee,
                worker_cache,
                rx_consensus,
                tx_consensus_round_updates,
                rx_reconfigure,
                tx_reconfigure,
                last_committed_round: 0,
            }
            .run()
            .await;
        })
    }

    async fn handle_sequenced(&mut self, certificate: Certificate) {
        // TODO [issue #9]: Re-include batch digests that have not been sequenced into our next block.

        let round = certificate.round();
        if round > self.last_committed_round {
            self.last_committed_round = round;

            // Trigger cleanup on the primary.
            let _ = self.tx_consensus_round_updates.send(round); // ignore error when receivers dropped.
        }
    }

    async fn run(&mut self) {
        info!(
            "StateHandler on node {} has started successfully.",
            self.name
        );
        loop {
            tokio::select! {
                Some(certificate) = self.rx_consensus.recv() => {
                    self.handle_sequenced(certificate).await;
                },

                Some(message) = self.rx_reconfigure.recv() => {
                    let shutdown = match &message {
                        ReconfigureNotification::NewEpoch(committee) => {
                            // TODO: Duplicated code in the same file.
                            // Update the worker cache.
                            self.worker_cache.swap(Arc::new(WorkerCache {
                                epoch: committee.epoch,
                                workers: committee.keys().iter().map(|key|
                                    (
                                        (*key).clone(),
                                        self.worker_cache
                                            .load()
                                            .workers
                                            .get(key)
                                            .tap_none(||
                                                warn!("Worker cache does not have a key for the new committee member"))
                                            .unwrap_or(&WorkerIndex(BTreeMap::new()))
                                            .clone()
                                    )).collect(),
                            }));

                            // Update the committee.
                            self.committee.swap(Arc::new(committee.clone()));

                            // Trigger cleanup on the primary.
                            let _ = self.tx_consensus_round_updates.send(0); // ignore error when receivers dropped.

                            tracing::debug!("Committee updated to {}", self.committee);
                            false
                        },
                        ReconfigureNotification::UpdateCommittee(committee) => {
                            // Update the worker cache.
                            self.worker_cache.swap(Arc::new(WorkerCache {
                                epoch: committee.epoch,
                                workers: committee.keys().iter().map(|key|
                                    (
                                        (*key).clone(),
                                        self.worker_cache
                                            .load()
                                            .workers
                                            .get(key)
                                            .tap_none(||
                                                warn!("Worker cache does not have a key for the new committee member"))
                                            .unwrap_or(&WorkerIndex(BTreeMap::new()))
                                            .clone()
                                    )).collect(),
                            }));

                            // Update the committee.
                            self.committee.swap(Arc::new(committee.clone()));

                            tracing::debug!("Committee updated to {}", self.committee);
                            false
                        }
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
