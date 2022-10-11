// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use config::{SharedCommittee, SharedWorkerCache, WorkerCache, WorkerIndex};

use network::P2pNetwork;
use primary::PrimaryWorkerMessage;
use std::{collections::BTreeMap, sync::Arc};

use tap::TapOptional;
use tokio::{sync::watch, task::JoinHandle};
use tracing::warn;
use types::{metered_channel::Receiver, ReconfigureNotification};

// The `Synchronizer` is responsible to keep the worker in sync with the others.
pub struct Synchronizer {
    /// The committee information.
    committee: SharedCommittee,
    /// The worker information cache.
    worker_cache: SharedWorkerCache,
    /// Input channel to receive the commands from the primary.
    rx_message: Receiver<PrimaryWorkerMessage>,
    /// A network sender to send requests to the other workers.
    network: P2pNetwork,
    /// Send reconfiguration update to other tasks.
    tx_reconfigure: watch::Sender<ReconfigureNotification>,
}

impl Synchronizer {
    #[must_use]
    pub fn spawn(
        committee: SharedCommittee,
        worker_cache: SharedWorkerCache,
        rx_message: Receiver<PrimaryWorkerMessage>,
        tx_reconfigure: watch::Sender<ReconfigureNotification>,
        network: P2pNetwork,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            Self {
                committee,
                worker_cache,
                rx_message,
                network,
                tx_reconfigure,
            }
            .run()
            .await;
        })
    }

    /// Main loop listening to the primary's messages.
    async fn run(&mut self) {
        loop {
            tokio::select! {
                // Handle primary's messages.
                Some(message) = self.rx_message.recv() => match message {
                    PrimaryWorkerMessage::Reconfigure(message) => {
                        // Reconfigure this task and update the shared committee.
                        let shutdown = match &message {
                            ReconfigureNotification::NewEpoch(new_committee) => {
                                self.network.cleanup(self.worker_cache.load().network_diff(new_committee.keys()));
                                self.committee.swap(Arc::new(new_committee.clone()));

                                // TODO: duplicated code in this file.
                                // Update the worker cache.
                                self.worker_cache.swap(Arc::new(WorkerCache {
                                    epoch: new_committee.epoch,
                                    workers: new_committee.keys().iter().map(|key|
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

                                false
                            }
                            ReconfigureNotification::UpdateCommittee(new_committee) => {
                                self.network.cleanup(self.worker_cache.load().network_diff(new_committee.keys()));
                                self.committee.swap(Arc::new(new_committee.clone()));

                                // Update the worker cache.
                                self.worker_cache.swap(Arc::new(WorkerCache {
                                    epoch: new_committee.epoch,
                                    workers: new_committee.keys().iter().map(|key|
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

                                tracing::debug!("Committee updated to {}", self.committee);
                                false
                            }
                            ReconfigureNotification::Shutdown => true
                        };

                        // Notify all other tasks.
                        self.tx_reconfigure.send(message).expect("All tasks dropped");

                        // Exit only when we are sure that all the other tasks received
                        // the shutdown message.
                        if shutdown {
                            self.tx_reconfigure.closed().await;
                            return;
                        }
                    }
                },
            }
        }
    }
}
