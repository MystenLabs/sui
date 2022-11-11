// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use config::{Committee, SharedCommittee, SharedWorkerCache, WorkerCache, WorkerIndex};
use crypto::PublicKey;
use network::{P2pNetwork, UnreliableNetwork};
use std::{collections::BTreeMap, sync::Arc};
use sui_metrics::spawn_monitored_task;
use tap::TapOptional;
use tokio::{sync::watch, task::JoinHandle};
use tracing::{debug, info, warn};
use types::{
    metered_channel::{Receiver, Sender},
    Certificate, ReconfigureNotification, Round, WorkerReconfigureMessage,
};

/// Receives the highest round reached by consensus and update it for all tasks.
pub struct StateHandler {
    /// The public key of this authority.
    name: PublicKey,
    /// The committee information.
    committee: SharedCommittee,
    /// The worker information cache.
    worker_cache: SharedWorkerCache,
    /// Receives the ordered certificates from consensus.
    rx_committed_certificates: Receiver<(Round, Vec<Certificate>)>,
    /// Signals a new consensus round
    tx_consensus_round_updates: watch::Sender<Round>,
    /// Receives notifications to reconfigure the system.
    rx_state_handler: Receiver<ReconfigureNotification>,
    /// Channel to signal committee changes.
    tx_reconfigure: watch::Sender<ReconfigureNotification>,
    /// The latest round committed by consensus.
    last_committed_round: Round,
    /// A channel to update the committed rounds
    tx_commited_own_headers: Option<Sender<(Round, Vec<Round>)>>,

    network: P2pNetwork,
}

impl StateHandler {
    #[must_use]
    pub fn spawn(
        name: PublicKey,
        committee: SharedCommittee,
        worker_cache: SharedWorkerCache,
        rx_committed_certificates: Receiver<(Round, Vec<Certificate>)>,
        tx_consensus_round_updates: watch::Sender<u64>,
        rx_state_handler: Receiver<ReconfigureNotification>,
        tx_reconfigure: watch::Sender<ReconfigureNotification>,
        tx_commited_own_headers: Option<Sender<(Round, Vec<Round>)>>,
        network: P2pNetwork,
    ) -> JoinHandle<()> {
        spawn_monitored_task!(async move {
            Self {
                name,
                committee,
                worker_cache,
                rx_committed_certificates,
                tx_consensus_round_updates,
                rx_state_handler,
                tx_reconfigure,
                last_committed_round: 0,
                tx_commited_own_headers,
                network,
            }
            .run()
            .await;
        })
    }

    async fn handle_sequenced(&mut self, round: Round, certificates: Vec<Certificate>) {
        if round > self.last_committed_round {
            self.last_committed_round = round;

            // Trigger cleanup on the primary.
            let _ = self.tx_consensus_round_updates.send(round); // ignore error when receivers dropped.
        }

        // Now we are going to signal which of our own batches have been committed.
        let own_rounds_committed: Vec<_> = certificates
            .iter()
            .filter_map(|cert| {
                if cert.header.author == self.name {
                    Some(cert.header.round)
                } else {
                    None
                }
            })
            .collect();
        debug!(
            "Own committed rounds {:?} at round {:?}",
            own_rounds_committed, round
        );

        // If a reporting channel is available send the committed own
        // headers to it.
        if let Some(sender) = &self.tx_commited_own_headers {
            let _ = sender.send((round, own_rounds_committed)).await;
        }
    }

    fn update_committee(&mut self, committee: Committee) {
        // Update the worker cache.
        self.worker_cache.swap(Arc::new(WorkerCache {
            epoch: committee.epoch,
            workers: committee
                .keys()
                .iter()
                .map(|key| {
                    (
                        (*key).clone(),
                        self.worker_cache
                            .load()
                            .workers
                            .get(key)
                            .tap_none(|| {
                                warn!(
                                    "Worker cache does not have a key for the new committee member"
                                )
                            })
                            .unwrap_or(&WorkerIndex(BTreeMap::new()))
                            .clone(),
                    )
                })
                .collect(),
        }));

        // Update the committee.
        self.committee.swap(Arc::new(committee));

        tracing::debug!("Committee updated to {}", self.committee);
    }

    fn notify_our_workers(&mut self, message: ReconfigureNotification) {
        let message = WorkerReconfigureMessage { message };
        let our_workers = self
            .worker_cache
            .load()
            .our_workers(&self.name)
            .unwrap()
            .into_iter()
            .map(|info| info.name)
            .collect();

        self.network.unreliable_broadcast(our_workers, &message);
    }

    async fn run(mut self) {
        info!(
            "StateHandler on node {} has started successfully.",
            self.name
        );
        loop {
            tokio::select! {
                Some((round, certificates)) = self.rx_committed_certificates.recv() => {
                    self.handle_sequenced(round, certificates).await;
                },

                Some(message) = self.rx_state_handler.recv() => {
                    // Notify our workers
                    self.notify_our_workers(message.to_owned());

                    let shutdown = match &message {
                        ReconfigureNotification::NewEpoch(committee) => {
                            self.update_committee(committee.to_owned());

                            // Trigger cleanup on the primary.
                            let _ = self.tx_consensus_round_updates.send(0); // ignore error when receivers dropped.

                            false
                        },
                        ReconfigureNotification::UpdateCommittee(committee) => {
                            self.update_committee(committee.to_owned());

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
