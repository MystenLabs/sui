// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use config::{Committee, Stake, WorkerId};
use crypto::traits::VerifyingKey;
use futures::stream::{futures_unordered::FuturesUnordered, StreamExt as _};
use network::{CancelHandler, MessageResult, WorkerNetwork};
use tokio::{
    sync::{
        mpsc::{Receiver, Sender},
        watch,
    },
    task::JoinHandle,
};
use types::{
    error::DagError, Batch, ReconfigureNotification, SerializedBatchMessage, WorkerMessage,
};

#[cfg(test)]
#[path = "tests/quorum_waiter_tests.rs"]
pub mod quorum_waiter_tests;

/// The QuorumWaiter waits for 2f authorities to acknowledge reception of a batch.
pub struct QuorumWaiter<PublicKey: VerifyingKey> {
    /// The public key of this authority.
    name: PublicKey,
    /// The id of this worker.
    id: WorkerId,
    /// The committee information.
    committee: Committee<PublicKey>,
    /// Receive reconfiguration updates.
    rx_reconfigure: watch::Receiver<ReconfigureNotification<PublicKey>>,
    /// Input Channel to receive commands.
    rx_message: Receiver<Batch>,
    /// Channel to deliver batches for which we have enough acknowledgments.
    tx_batch: Sender<SerializedBatchMessage>,
    /// A network sender to broadcast the batches to the other workers.
    network: WorkerNetwork,
}

impl<PublicKey: VerifyingKey> QuorumWaiter<PublicKey> {
    /// Spawn a new QuorumWaiter.
    pub fn spawn(
        name: PublicKey,
        id: WorkerId,
        committee: Committee<PublicKey>,
        rx_reconfigure: watch::Receiver<ReconfigureNotification<PublicKey>>,
        rx_message: Receiver<Batch>,
        tx_batch: Sender<Vec<u8>>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            Self {
                name,
                id,
                committee,
                rx_reconfigure,
                rx_message,
                tx_batch,
                network: WorkerNetwork::default(),
            }
            .run()
            .await;
        })
    }

    /// Helper function. It waits for a future to complete and then delivers a value.
    async fn waiter(wait_for: CancelHandler<MessageResult>, deliver: Stake) -> Stake {
        let _ = wait_for.await;
        deliver
    }

    /// Main loop.
    async fn run(&mut self) {
        loop {
            tokio::select! {
                Some(batch) = self.rx_message.recv() => {
                    // Broadcast the batch to the other workers.
                    let workers_addresses: Vec<_> = self
                        .committee
                        .others_workers(&self.name, &self.id)
                        .into_iter()
                        .map(|(name, addresses)| (name, addresses.worker_to_worker))
                        .collect();
                    let (names, addresses): (Vec<_>, _) = workers_addresses.iter().cloned().unzip();
                    let message = WorkerMessage::<PublicKey>::Batch(batch);
                    let serialized =
                        bincode::serialize(&message).expect("Failed to serialize our own batch");
                    let handlers = self.network.broadcast(addresses, &message).await;

                    // Collect all the handlers to receive acknowledgements.
                    let mut wait_for_quorum: FuturesUnordered<_> = names
                        .into_iter()
                        .zip(handlers.into_iter())
                        .map(|(name, handler)| {
                            let stake = self.committee.stake(&name);
                            Self::waiter(handler, stake)
                        })
                        .collect();

                    // Wait for the first 2f nodes to send back an Ack. Then we consider the batch
                    // delivered and we send its digest to the primary (that will include it into
                    // the dag). This should reduce the amount of synching.
                    let threshold = self.committee.quorum_threshold();
                    let mut total_stake = self.committee.stake(&self.name);
                    loop {
                        tokio::select! {
                            Some(stake) = wait_for_quorum.next() => {
                                total_stake += stake;
                                if total_stake >= threshold {
                                    if self.tx_batch.send(serialized).await.is_err() {
                                        tracing::debug!("{}", DagError::ShuttingDown);
                                    }
                                    break;
                                }
                            }

                            result = self.rx_reconfigure.changed() => {
                                result.expect("Committee channel dropped");
                                let message = self.rx_reconfigure.borrow().clone();
                                match message {
                                    ReconfigureNotification::NewCommittee(new_committee) => {
                                        self.committee=new_committee;
                                        tracing::debug!("Committee updated to {}", self.committee);
                                        break; // Don't wait for acknowledgements.
                                    },
                                    ReconfigureNotification::Shutdown => return
                                }
                            }
                        }
                    }
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
            }
        }
    }
}
