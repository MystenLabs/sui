// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use config::{Committee, SharedWorkerCache, WorkerId};
use crypto::PublicKey;
use network::{P2pNetwork, UnreliableNetwork};
use store::Store;
use tokio::{sync::watch, task::JoinHandle};
use tracing::{error, trace, warn};
use types::{
    metered_channel::Receiver, Batch, BatchDigest, ReconfigureNotification, WorkerMessage,
};

#[cfg(test)]
#[path = "tests/helper_tests.rs"]
pub mod helper_tests;

/// A task dedicated to help other authorities by replying to their batch requests.
pub struct Helper {
    /// The id of this worker.
    #[allow(unused)]
    id: WorkerId,
    /// The committee information.
    committee: Committee,
    /// The worker information cache.
    #[allow(unused)]
    worker_cache: SharedWorkerCache,
    /// The persistent storage.
    store: Store<BatchDigest, Batch>,
    /// Receive reconfiguration updates.
    rx_reconfigure: watch::Receiver<ReconfigureNotification>,
    /// Input channel to receive batch requests from workers.
    rx_worker_request: Receiver<(Vec<BatchDigest>, PublicKey)>,
    /// A network sender to send the batches to the other workers.
    network: P2pNetwork,
}

impl Helper {
    #[must_use]
    pub fn spawn(
        id: WorkerId,
        committee: Committee,
        worker_cache: SharedWorkerCache,
        store: Store<BatchDigest, Batch>,
        rx_reconfigure: watch::Receiver<ReconfigureNotification>,
        rx_worker_request: Receiver<(Vec<BatchDigest>, PublicKey)>,
        network: P2pNetwork,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            Self {
                id,
                committee,
                worker_cache,
                store,
                rx_reconfigure,
                rx_worker_request,
                network,
            }
            .run()
            .await;
        })
    }

    async fn run(&mut self) {
        // TODO [issue #7]: Do some accounting to prevent bad actors from monopolizing our resources.
        loop {
            tokio::select! {
                // Handle requests from other workers.
                Some((digests, origin)) = self.rx_worker_request.recv() => {
                    // get the requestors peer_id.
                    let origin = match self.worker_cache.load().worker(&origin, &self.id) {
                        Ok(x) => x.name,
                        Err(e) => {
                            warn!("Unexpected batch request: {e}");
                            continue;
                        }
                    };

                    // Reply to the request (the best we can).
                    for digest in digests {
                        match self.store.read(digest).await {
                            Ok(Some(batch)) => {
                                let _ = self.network.unreliable_send(origin.clone(), &WorkerMessage::Batch(batch)).await;
                            }
                            Ok(None) => {
                                trace!("No Batches found for requested digests {:?}", digest);
                            },
                            Err(e) => error!("{e}"),
                        }
                    }
                },

                // Trigger reconfigure.
                result = self.rx_reconfigure.changed() => {
                    result.expect("Committee channel dropped");
                    let message = self.rx_reconfigure.borrow().clone();
                    match message {
                        ReconfigureNotification::NewEpoch(new_committee) => {
                            self.network.cleanup(self.committee.network_diff(&new_committee));
                            self.committee = new_committee;
                        },
                        ReconfigureNotification::UpdateCommittee(new_committee) => {
                            self.network.cleanup(self.committee.network_diff(&new_committee));
                            self.committee = new_committee;

                        },
                        ReconfigureNotification::Shutdown => return
                    }
                    tracing::debug!("Committee updated to {}", self.committee);
                }
            }
        }
    }
}
