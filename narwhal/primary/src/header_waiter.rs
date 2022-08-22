// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    metrics::PrimaryMetrics,
    primary::{PayloadToken, PrimaryMessage, PrimaryWorkerMessage},
};
use config::{Committee, WorkerId};
use crypto::PublicKey;
use futures::future::{try_join_all, BoxFuture};
use network::{LuckyNetwork, PrimaryNetwork, PrimaryToWorkerNetwork, UnreliableNetwork};
use serde::{de::DeserializeOwned, Serialize};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use store::Store;
use tokio::{
    sync::{oneshot, watch},
    task::JoinHandle,
    time::{sleep, Duration, Instant},
};
use tracing::{debug, info};
use types::{
    bounded_future_queue::BoundedFuturesUnordered,
    error::{DagError, DagResult},
    metered_channel::{Receiver, Sender},
    try_fut_and_permit, BatchDigest, Certificate, CertificateDigest, Header, HeaderDigest,
    ReconfigureNotification, Round,
};

#[cfg(test)]
#[path = "tests/header_waiter_tests.rs"]
pub mod header_waiter_tests;

/// The resolution of the timer that checks whether we received replies to our sync requests, and triggers
/// new sync requests if we didn't.
const TIMER_RESOLUTION: u64 = 1_000;

/// The commands that can be sent to the `Waiter`.
#[derive(Debug)]
pub enum WaiterMessage {
    SyncBatches(HashMap<BatchDigest, WorkerId>, Header),
    SyncParents(Vec<CertificateDigest>, Header),
}

/// Waits for missing parent certificates and batches' digests.
pub struct HeaderWaiter {
    /// The name of this authority.
    name: PublicKey,
    /// The committee information.
    committee: Committee,
    /// The persistent storage for parent Certificates.
    certificate_store: Store<CertificateDigest, Certificate>,
    /// The persistent storage for payload markers from workers.
    payload_store: Store<(BatchDigest, WorkerId), PayloadToken>,
    /// A watch channel receiver to get consensus round updates.
    rx_consensus_round_updates: watch::Receiver<u64>,
    /// The depth of the garbage collector.
    gc_depth: Round,
    /// The delay to wait before re-trying sync requests.
    sync_retry_delay: Duration,
    /// Determine with how many nodes to sync when re-trying to send sync-request.
    sync_retry_nodes: usize,

    /// Watch channel to reconfigure the committee.
    rx_reconfigure: watch::Receiver<ReconfigureNotification>,
    /// Receives sync commands from the `Synchronizer`.
    rx_synchronizer: Receiver<WaiterMessage>,
    /// Loops back to the core headers for which we got all parents and batches.
    tx_core: Sender<Header>,

    /// Network driver allowing to send messages.
    primary_network: PrimaryNetwork,
    worker_network: PrimaryToWorkerNetwork,
    /// Keeps the digests of the all certificates for which we sent a sync request,
    /// along with a time stamp (`u128`) indicating when we sent the request.
    parent_requests: HashMap<CertificateDigest, (Round, u128)>,
    /// Keeps the digests of the all TX batches for which we sent a sync request,
    /// similarly to `header_requests`.
    batch_requests: HashMap<BatchDigest, Round>,
    /// List of digests (headers or tx batch) that are waiting to be processed.
    /// Their processing will resume when we get all their dependencies.
    pending: HashMap<HeaderDigest, (Round, oneshot::Sender<()>)>,
    /// Metrics handler
    metrics: Arc<PrimaryMetrics>,
}

impl HeaderWaiter {
    /// Returns the max amount of pending certificates x pending parents messages we should expect. In the worst case of causal completion,
    /// this can be `self.gc_depth` x `self.committee.len()` for each
    pub fn max_pending_header_waiter_requests(&self) -> usize {
        self.gc_depth as usize * self.committee.size() * 4
    }

    #[must_use]
    pub fn spawn(
        name: PublicKey,
        committee: Committee,
        certificate_store: Store<CertificateDigest, Certificate>,
        payload_store: Store<(BatchDigest, WorkerId), PayloadToken>,
        rx_consensus_round_updates: watch::Receiver<u64>,
        gc_depth: Round,
        sync_retry_delay: Duration,
        sync_retry_nodes: usize,
        rx_reconfigure: watch::Receiver<ReconfigureNotification>,
        rx_synchronizer: Receiver<WaiterMessage>,
        tx_core: Sender<Header>,
        metrics: Arc<PrimaryMetrics>,
        primary_network: PrimaryNetwork,
        worker_network: PrimaryToWorkerNetwork,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            Self {
                name,
                committee,
                certificate_store,
                payload_store,
                rx_consensus_round_updates,
                gc_depth,
                sync_retry_delay,
                sync_retry_nodes,
                rx_reconfigure,
                rx_synchronizer,
                tx_core,
                primary_network,
                worker_network,
                parent_requests: HashMap::new(),
                batch_requests: HashMap::new(),
                pending: HashMap::new(),
                metrics,
            }
            .run()
            .await;
        })
    }

    /// Helper function. It waits for particular data to become available in the storage
    /// and then delivers the specified header.
    async fn waiter<T, V>(
        missing: Vec<T>,
        store: Store<T, V>,
        deliver: Header,
        handler: oneshot::Receiver<()>,
    ) -> DagResult<Option<Header>>
    where
        T: Serialize + DeserializeOwned + Send + Clone,
        V: Serialize + DeserializeOwned + Send,
    {
        let waiting: Vec<_> = missing.into_iter().map(|x| store.notify_read(x)).collect();
        tokio::select! {
            result = try_join_all(waiting) => {
                result.map(|_| Some(deliver)).map_err(DagError::from)
            }
            _ = handler => Ok(None),
        }
    }

    /// Main loop listening to the `Synchronizer` messages.
    async fn run(&mut self) {
        let mut waiting: BoundedFuturesUnordered<BoxFuture<'_, _>> =
            BoundedFuturesUnordered::with_capacity(self.max_pending_header_waiter_requests());

        let timer = sleep(Duration::from_millis(TIMER_RESOLUTION));
        tokio::pin!(timer);

        info!(
            "HeaderWaiter on node {} has started successfully.",
            self.name
        );
        loop {
            let mut attempt_garbage_collection = false;

            tokio::select! {
                Some(message) = self.rx_synchronizer.recv(), if waiting.available_permits() > 0 => {
                    match message {
                        WaiterMessage::SyncBatches(missing, header) => {
                            debug!("Synching the payload of {header}");
                            let header_id = header.id;
                            let round = header.round;
                            let author = header.author.clone();

                            // Ensure we sync only once per header.
                            if self.pending.contains_key(&header_id) {
                                continue;
                            }

                            // Add the header to the waiter pool. The waiter will return it to when all
                            // its parents are in the store.
                            let wait_for = missing
                                .iter().map(|(x, y)| (*x, *y))
                                .collect();
                            let (tx_cancel, rx_cancel) = oneshot::channel();
                            self.pending.insert(header_id, (round, tx_cancel));
                            let fut = Self::waiter(wait_for, self.payload_store.clone(), header, rx_cancel);
                            // pointer-size allocation, bounded by the # of blocks (may eventually go away, see rust RFC #1909)
                            waiting.push(Box::pin(fut)).await;

                            // Ensure we didn't already send a sync request for these parents.
                            let mut requires_sync = HashMap::new();
                            for (digest, worker_id) in missing.into_iter() {
                                self.batch_requests.entry(digest).or_insert_with(|| {
                                    requires_sync.entry(worker_id).or_insert_with(Vec::new).push(digest);
                                    round
                                });
                            }
                            for (worker_id, digests) in requires_sync {
                                let address = self.committee
                                    .worker(&self.name, &worker_id)
                                    .expect("Author of valid header is not in the committee")
                                    .primary_to_worker;

                                // TODO [issue #423]: This network transmission needs to be reliable: the worker may crash-recover.
                                let message = PrimaryWorkerMessage::Synchronize(digests, author.clone());
                                self.worker_network.unreliable_send(address, &message).await;
                            }
                        }

                        WaiterMessage::SyncParents(missing, header) => {
                            debug!("Synching the parents of {header}");
                            let header_id = header.id;
                            let round = header.round;
                            let author = header.author.clone();

                            // Ensure we sync only once per header.
                            if self.pending.contains_key(&header_id) {
                                continue;
                            }

                            // Add the header to the waiter pool. The waiter will return it to us
                            // when all its parents are in the store.
                            let wait_for = missing.clone();
                            let (tx_cancel, rx_cancel) = oneshot::channel();
                            self.pending.insert(header_id, (round, tx_cancel));
                            let fut = Self::waiter(wait_for, self.certificate_store.clone(), header, rx_cancel);
                            // pointer-size allocation, bounded by the # of blocks (may eventually go away, see rust RFC #1909)
                            waiting.push(Box::pin(fut)).await;

                            // Ensure we didn't already sent a sync request for these parents.
                            // Optimistically send the sync request to the node that created the certificate.
                            // If this fails (after a timeout), we broadcast the sync request.
                            let now = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .expect("Failed to measure time")
                                .as_millis();
                            let mut requires_sync = Vec::new();
                            for missing in missing {
                                self.parent_requests.entry(missing).or_insert_with(|| {
                                    requires_sync.push(missing);
                                    (round, now)
                                });
                            }
                            if !requires_sync.is_empty() {
                                let address = self.committee
                                    .primary(&author)
                                    .expect("Author of valid header not in the committee")
                                    .primary_to_primary;
                                let message = PrimaryMessage::CertificatesRequest(requires_sync, self.name.clone());
                                self.primary_network.unreliable_send(address, &message).await;
                            }
                        }
                    }
                },

                // we poll the availability of a slot to send the result to the core simultaneously
                (Some(result), permit) = try_fut_and_permit!(waiting.try_next(), self.tx_core) => if let Some(header) = result {
                    let _ = self.pending.remove(&header.id);
                    for x in header.payload.keys() {
                        let _ = self.batch_requests.remove(x);
                    }
                    for x in &header.parents {
                        let _ = self.parent_requests.remove(x);
                    }
                    permit.send(header);
                },  // This request has been canceled when result is None.

                () = &mut timer => {
                    // We optimistically sent sync requests to a single node. If this timer triggers,
                    // it means we were wrong to trust it. We are done waiting for a reply and we now
                    // broadcast the request to all nodes.
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .expect("Failed to measure time")
                        .as_millis();

                    let mut retry = Vec::new();
                    for (digest, (_, timestamp)) in self.parent_requests.iter_mut() {
                        if *timestamp + self.sync_retry_delay.as_millis() < now {
                            debug!("Requesting sync for certificate {digest} (retry)");
                            retry.push(*digest);
                            // reset the time at which this request was last issued
                            *timestamp = now;
                        }
                    }

                    if !retry.is_empty() {
                        let addresses = self.committee
                            .others_primaries(&self.name)
                            .into_iter()
                            .map(|(_, x)| x.primary_to_primary)
                            .collect();
                        let message = PrimaryMessage::CertificatesRequest(retry, self.name.clone());
                        self.primary_network.lucky_broadcast(addresses, &message, self.sync_retry_nodes).await;
                    }
                    // Reschedule the timer.
                    timer.as_mut().reset(Instant::now() + Duration::from_millis(TIMER_RESOLUTION));
                },

                // Check whether the committee changed.
                result = self.rx_reconfigure.changed() => {
                    result.expect("Committee channel dropped");
                    let message = self.rx_reconfigure.borrow().clone();
                    match message {
                        ReconfigureNotification::NewEpoch(new_committee) => {
                            // Update the committee and cleanup internal state.
                            self.primary_network.cleanup(self.committee.network_diff(&new_committee));
                            self.worker_network.cleanup(self.committee.network_diff(&new_committee));

                            self.committee = new_committee;

                            self.pending.clear();
                            self.batch_requests.clear();
                            self.parent_requests.clear();
                        },
                        ReconfigureNotification::UpdateCommittee(new_committee) => {
                            self.primary_network.cleanup(self.committee.network_diff(&new_committee));
                            self.worker_network.cleanup(self.committee.network_diff(&new_committee));
                            self.committee = new_committee;
                        },
                        ReconfigureNotification::Shutdown => return
                    }
                    tracing::debug!("Committee updated to {}", self.committee);
                },

                // Check for a new consensus round number
                Ok(()) = self.rx_consensus_round_updates.changed() => {
                    attempt_garbage_collection = true;
                },

            }

            if attempt_garbage_collection {
                let round = *self.rx_consensus_round_updates.borrow();
                if round > self.gc_depth {
                    let now = Instant::now();

                    let mut gc_round = round - self.gc_depth;

                    // Cancel expired `notify_read`s, keep the rest in the map
                    // TODO: replace with `drain_filter` once that API stabilizes
                    self.pending = self
                        .pending
                        .drain()
                        .flat_map(|(digest, (r, handler))| {
                            if r <= gc_round {
                                // note: this send can fail, harmlessly, if the certificate has been delivered (`notify_read`)
                                // and the present code path fires before the corresponding `waiting` item is unpacked above.
                                let _ = handler.send(());
                                None
                            } else {
                                Some((digest, (r, handler)))
                            }
                        })
                        .collect();
                    self.batch_requests.retain(|_, r| r > &mut gc_round);
                    self.parent_requests.retain(|_, (r, _)| r > &mut gc_round);

                    self.metrics
                        .gc_header_waiter_latency
                        .with_label_values(&[&self.committee.epoch.to_string()])
                        .observe(now.elapsed().as_secs_f64());
                }
            }

            // measure the pending & parent elements
            self.metrics
                .pending_elements_header_waiter
                .with_label_values(&[&self.committee.epoch.to_string()])
                .set(self.pending.len() as i64);

            self.metrics
                .parent_requests_header_waiter
                .with_label_values(&[&self.committee.epoch.to_string()])
                .set(self.parent_requests.len() as i64);

            self.metrics
                .waiting_elements_header_waiter
                .with_label_values(&[&self.committee.epoch.to_string()])
                .set(waiting.len() as i64);
        }
    }
}
