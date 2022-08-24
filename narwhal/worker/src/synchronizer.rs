// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::metrics::WorkerMetrics;
use config::{SharedCommittee, SharedWorkerCache, WorkerCache, WorkerId, WorkerIndex};
use crypto::PublicKey;
use futures::stream::{futures_unordered::FuturesUnordered, StreamExt as _};
use network::{LuckyNetwork, UnreliableNetwork, WorkerNetwork};
use primary::PrimaryWorkerMessage;
use std::collections::HashSet;
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use store::{Store, StoreError};
use tap::TapFallible;
use tap::TapOptional;
use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
    time::{sleep, Duration, Instant},
};
use tracing::{debug, error, trace, warn};
use types::error::DagError;
use types::{
    metered_channel::{Receiver, Sender},
    BatchDigest, ReconfigureNotification, Round, SerializedBatchMessage, WorkerMessage,
    WorkerPrimaryError, WorkerPrimaryMessage,
};

#[cfg(test)]
#[path = "tests/synchronizer_tests.rs"]
pub mod synchronizer_tests;

/// Resolution of the timer managing retrials of sync requests (in ms).
const TIMER_RESOLUTION: u64 = 1_000;

// The `Synchronizer` is responsible to keep the worker in sync with the others.
pub struct Synchronizer {
    /// The public key of this authority.
    name: PublicKey,
    /// The id of this worker.
    id: WorkerId,
    /// The committee information.
    committee: SharedCommittee,
    /// The worker information cache.
    worker_cache: SharedWorkerCache,
    // The persistent storage.
    store: Store<BatchDigest, SerializedBatchMessage>,
    /// The depth of the garbage collection.
    gc_depth: Round,
    /// The delay to wait before re-trying to send sync requests.
    sync_retry_delay: Duration,
    /// Determine with how many nodes to sync when re-trying to send sync-requests. These nodes
    /// are picked at random from the committee.
    sync_retry_nodes: usize,
    /// Input channel to receive the commands from the primary.
    rx_message: Receiver<PrimaryWorkerMessage>,
    /// A network sender to send requests to the other workers.
    network: WorkerNetwork,
    /// Loosely keep track of the primary's round number (only used for cleanup).
    round: Round,
    /// Keeps the digests (of batches) that are waiting to be processed by the primary. Their
    /// processing will resume when we get the missing batches in the store or we no longer need them.
    /// It also keeps the round number and a time stamp (`u128`) of each request we sent.
    pending: HashMap<BatchDigest, (Round, mpsc::Sender<()>, u128)>,
    /// Send reconfiguration update to other tasks.
    tx_reconfigure: watch::Sender<ReconfigureNotification>,
    /// Output channel to send out the batch requests.
    tx_primary: Sender<WorkerPrimaryMessage>,
    /// Metrics handler
    metrics: Arc<WorkerMetrics>,
}

impl Synchronizer {
    #[must_use]
    pub fn spawn(
        name: PublicKey,
        id: WorkerId,
        committee: SharedCommittee,
        worker_cache: SharedWorkerCache,
        store: Store<BatchDigest, SerializedBatchMessage>,
        gc_depth: Round,
        sync_retry_delay: Duration,
        sync_retry_nodes: usize,
        rx_message: Receiver<PrimaryWorkerMessage>,
        tx_reconfigure: watch::Sender<ReconfigureNotification>,
        tx_primary: Sender<WorkerPrimaryMessage>,
        metrics: Arc<WorkerMetrics>,
        network: WorkerNetwork,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            Self {
                name,
                id,
                committee,
                worker_cache,
                store,
                gc_depth,
                sync_retry_delay,
                sync_retry_nodes,
                rx_message,
                network,
                round: Round::default(),
                pending: HashMap::new(),
                tx_reconfigure,
                tx_primary,
                metrics,
            }
            .run()
            .await;
        })
    }

    /// Helper function. It waits for a batch to become available in the storage
    /// and then delivers its digest.
    async fn waiter(
        missing: BatchDigest,
        store: Store<BatchDigest, SerializedBatchMessage>,
        deliver: BatchDigest,
        mut handler: mpsc::Receiver<()>,
    ) -> Result<Option<BatchDigest>, StoreError> {
        tokio::select! {
            result = store.notify_read(missing) => {
                result.map(|_| Some(deliver))
            }
            _ = handler.recv() => Ok(None),
        }
    }

    /// Main loop listening to the primary's messages.
    async fn run(&mut self) {
        let mut waiting = FuturesUnordered::new();

        let timer = sleep(Duration::from_millis(TIMER_RESOLUTION));
        tokio::pin!(timer);

        loop {
            tokio::select! {
                // Handle primary's messages.
                Some(message) = self.rx_message.recv() => match message {
                    PrimaryWorkerMessage::Synchronize(digests, target) => {
                        let mut missing = HashSet::new();
                        let mut available = HashSet::new();

                        for digest in digests.iter() {
                            // Ensure we do not send twice the same sync request.
                            if self.pending.contains_key(digest) {
                                continue;
                            }

                            // Check if we received the batch in the meantime.
                            match self.store.read(*digest).await {
                                Ok(None) => {
                                    missing.insert(*digest);
                                    debug!("Requesting sync for batch {digest}");
                                },
                                Ok(Some(_)) => {
                                    // The batch arrived in the meantime: no need to request it.
                                    available.insert(*digest);
                                    trace!("Digest {digest} already in store, nothing to sync");
                                    continue;
                                },
                                Err(e) => {
                                    error!("{e}");
                                    continue;
                                }
                            };
                        }

                        // reply back immediately for the available ones
                        if !available.is_empty() {
                            // Doing this will ensure the batch id will be populated to primary even
                            // when other processes fail to do so (ex we received a batch from a peer
                            // worker and message has been missed by primary).
                            for digest in available {
                                let message = WorkerPrimaryMessage::OthersBatch(digest, self.id);
                                let _ = self.tx_primary.send(message).await.tap_err(|err|{
                                    debug!("{err:?} {}", DagError::ShuttingDown);
                                });
                            }
                        }

                        if !missing.is_empty() {
                            let now = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .expect("Failed to measure time")
                            .as_millis();

                            // now add all requests as pending
                            for digest in missing.iter() {
                                // Add the digest to the waiter.
                                let deliver = *digest;
                                let (tx_cancel, rx_cancel) = mpsc::channel(1);
                                let fut = Self::waiter(*digest, self.store.clone(), deliver, rx_cancel);
                                waiting.push(fut);
                                self.pending.insert(*digest, (self.round, tx_cancel, now));
                            }

                            // Send sync request to a single node. If this fails, we will send it
                            // to other nodes when a timer times out.
                            let address = match self.worker_cache.load().worker(&target, &self.id) {
                                Ok(address) => address.worker_to_worker,
                                Err(e) => {
                                    error!("The primary asked us to sync with an unknown node: {e}");
                                    continue;
                                }
                            };

                            debug!("Sending BatchRequest message to {} for missing batches {:?}", address, missing.clone());

                            let message = WorkerMessage::BatchRequest(missing.into_iter().collect::<Vec<_>>(), self.name.clone());
                            self.network.unreliable_send(address, &message).await;
                        } else {
                            debug!("All batches are already available {:?} nothing to request from peers", digests);
                        }
                    },
                    PrimaryWorkerMessage::Cleanup(round) => {
                        // Keep track of the primary's round number.
                        self.round = round;

                        // Cleanup internal state.
                        if self.round < self.gc_depth {
                            continue;
                        }

                        let mut gc_round = self.round - self.gc_depth;
                        for (r, handler, _) in self.pending.values() {
                            if r <= &gc_round {
                                let _ = handler.send(()).await;
                            }
                        }
                        self.pending.retain(|_, (r, _, _)| r > &mut gc_round);
                    },
                    PrimaryWorkerMessage::Reconfigure(message) => {
                        // Reconfigure this task and update the shared committee.
                        let shutdown = match &message {
                            ReconfigureNotification::NewEpoch(new_committee) => {
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

                                self.pending.clear();
                                self.round = 0;
                                waiting.clear();


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
                    PrimaryWorkerMessage::RequestBatch(digest) => {
                        self.handle_request_batch(digest).await;
                    },
                    PrimaryWorkerMessage::DeleteBatches(digests) => {
                        self.handle_delete_batches(digests).await;
                    }
                },

                // Stream out the futures of the `FuturesUnordered` that completed.
                Some(result) = waiting.next() => match result {
                    Ok(Some(digest)) => {
                        // We got the batch, remove it from the pending list.
                        self.pending.remove(&digest);
                    },
                    Ok(None) => {
                        // The sync request for this batch has been canceled.
                    },
                    Err(e) => error!("{e}")
                },

                // Triggers on timer's expiration.
                () = &mut timer => {
                    // We optimistically sent sync requests to a single node. If this timer triggers,
                    // it means we were wrong to trust it. We are done waiting for a reply and we now
                    // broadcast the request to a bunch of other nodes (selected at random).
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .expect("Failed to measure time")
                        .as_millis();

                    let mut retry = Vec::new();
                    for (digest, (_, _, timestamp)) in self.pending.iter_mut() {
                        if *timestamp + self.sync_retry_delay.as_millis() < now {
                            debug!("Requesting sync for batch {digest} (retry)");
                            retry.push(*digest);
                            // reset the time at which this request was last issued
                            *timestamp = now;
                        }
                    }
                    if !retry.is_empty() {
                        let addresses = self.worker_cache.load()
                            .others_workers(&self.name, &self.id)
                            .into_iter()
                            .map(|(_, address)| address.worker_to_worker)
                            .collect();
                        let message = WorkerMessage::BatchRequest(retry, self.name.clone());
                        self.network
                            .lucky_broadcast(addresses, &message, self.sync_retry_nodes)
                            .await;
                    }

                    // Reschedule the timer.
                    timer.as_mut().reset(Instant::now() + Duration::from_millis(TIMER_RESOLUTION));

                    // Report list of pending elements
                    self.metrics.pending_elements_worker_synchronizer
                    .with_label_values(&[&self.committee.load().epoch.to_string()])
                    .set(self.pending.len() as i64);
                },
            }
        }
    }

    async fn handle_request_batch(&mut self, digest: BatchDigest) {
        let message = match self.store.read(digest).await {
            Ok(Some(batch_serialised)) => {
                let batch = match bincode::deserialize(&batch_serialised).unwrap() {
                    WorkerMessage::Batch(batch) => batch,
                    _ => {
                        panic!("Wrong type has been stored!");
                    }
                };
                WorkerPrimaryMessage::RequestedBatch(digest, batch)
            }
            _ => WorkerPrimaryMessage::Error(WorkerPrimaryError::RequestedBatchNotFound(digest)),
        };

        self.tx_primary
            .send(message)
            .await
            .expect("Failed to send message to primary channel");
    }

    async fn handle_delete_batches(&mut self, digests: Vec<BatchDigest>) {
        let message = match self.store.remove_all(digests.clone()).await {
            Ok(_) => WorkerPrimaryMessage::DeletedBatches(digests),
            Err(err) => {
                error!("{err}");
                WorkerPrimaryMessage::Error(WorkerPrimaryError::ErrorWhileDeletingBatches(
                    digests.clone(),
                ))
            }
        };

        self.tx_primary
            .send(message)
            .await
            .expect("Failed to send message to primary channel");
    }
}
