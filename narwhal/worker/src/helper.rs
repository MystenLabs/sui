// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use bytes::Bytes;
use config::{Committee, WorkerId};
use crypto::traits::VerifyingKey;
use network::WorkerNetwork;
use store::Store;
use tokio::{
    sync::{
        mpsc::{Receiver, Sender},
        watch,
    },
    task::JoinHandle,
};
use tracing::{error, trace, warn};
use types::{BatchDigest, ReconfigureNotification, SerializedBatchMessage};

#[cfg(test)]
#[path = "tests/helper_tests.rs"]
pub mod helper_tests;

/// A task dedicated to help other authorities by replying to their batch requests.
pub struct Helper<PublicKey: VerifyingKey> {
    /// The id of this worker.
    id: WorkerId,
    /// The committee information.
    committee: Committee<PublicKey>,
    /// The persistent storage.
    store: Store<BatchDigest, SerializedBatchMessage>,
    /// Receive reconfiguration updates.
    rx_reconfigure: watch::Receiver<ReconfigureNotification<PublicKey>>,
    /// Input channel to receive batch requests from workers.
    rx_worker_request: Receiver<(Vec<BatchDigest>, PublicKey)>,
    /// Input channel to receive batch requests from workers.
    rx_client_request: Receiver<(Vec<BatchDigest>, Sender<SerializedBatchMessage>)>,
    /// A network sender to send the batches to the other workers.
    network: WorkerNetwork,
}

impl<PublicKey: VerifyingKey> Helper<PublicKey> {
    pub fn spawn(
        id: WorkerId,
        committee: Committee<PublicKey>,
        store: Store<BatchDigest, SerializedBatchMessage>,
        rx_reconfigure: watch::Receiver<ReconfigureNotification<PublicKey>>,
        rx_worker_request: Receiver<(Vec<BatchDigest>, PublicKey)>,
        rx_client_request: Receiver<(Vec<BatchDigest>, Sender<SerializedBatchMessage>)>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            Self {
                id,
                committee,
                store,
                rx_reconfigure,
                rx_worker_request,
                rx_client_request,
                network: WorkerNetwork::default(),
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
                    // get the requestors address.
                    let address = match self.committee.worker(&origin, &self.id) {
                        Ok(x) => x.worker_to_worker,
                        Err(e) => {
                            warn!("Unexpected batch request: {e}");
                            continue;
                        }
                    };

                    // Reply to the request (the best we can).
                    for digest in digests {
                        match self.store.read(digest).await {
                            Ok(Some(data)) => {
                                let _ = self.network.unreliable_send_message(address.clone(), Bytes::from(data)).await;
                            }
                            Ok(None) => {
                                trace!("No Batches found for requested digests {:?}", digest);
                            },
                            Err(e) => error!("{e}"),
                        }
                    }
                },

                // Handle requests from clients.
                Some((digests, replier)) = self.rx_client_request.recv() => {
                    // Reply to the request (the best we can).
                    for digest in digests {
                        match self.store.read(digest).await {
                            Ok(Some(data)) => replier
                                .send(data)
                                .await
                                .expect("Failed to reply to network"),
                            Ok(None) => (),
                            Err(e) => error!("{e}"),
                        }
                    }
                }

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
            }
        }
    }
}
