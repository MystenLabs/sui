// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    metrics::PrimaryMetrics,
    primary::{PayloadToken, PrimaryMessage},
};
use anyhow::Result;
use config::{Committee, SharedWorkerCache, WorkerId};
use crypto::PublicKey;
use futures::future::{try_join_all, BoxFuture};
use network::{CancelOnDropHandler, P2pNetwork, ReliableNetwork, UnreliableNetwork};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use storage::CertificateStore;
use store::Store;
use tokio::{
    sync::{oneshot, watch},
    task::JoinHandle,
    time::Instant,
};
use tracing::{debug, info, warn};
use types::{
    bounded_future_queue::BoundedFuturesUnordered,
    error::{DagError, DagResult},
    metered_channel::{Receiver, Sender},
    BatchDigest, CertificateDigest, Header, HeaderDigest, ReconfigureNotification, Round,
    WorkerSynchronizeMessage,
};

#[cfg(test)]
#[path = "tests/header_waiter_tests.rs"]
pub mod header_waiter_tests;

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
    /// The worker information cache.
    worker_cache: SharedWorkerCache,
    /// The persistent storage for parent Certificates.
    certificate_store: CertificateStore,
    /// The persistent storage for payload markers from workers.
    payload_store: Store<(BatchDigest, WorkerId), PayloadToken>,
    /// A watch channel receiver to get consensus round updates.
    rx_consensus_round_updates: watch::Receiver<u64>,
    /// The depth of the garbage collector.
    gc_depth: Round,

    /// Watch channel to reconfigure the committee.
    rx_reconfigure: watch::Receiver<ReconfigureNotification>,
    /// Receives sync commands from the `Synchronizer`.
    rx_header_waiter: Receiver<WaiterMessage>,
    /// Loops back to core the headers for which we got all parents and batches.
    tx_headers_loopback: Sender<Header>,

    /// Network driver allowing to send messages.
    network: P2pNetwork,

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
        worker_cache: SharedWorkerCache,
        certificate_store: CertificateStore,
        payload_store: Store<(BatchDigest, WorkerId), PayloadToken>,
        rx_consensus_round_updates: watch::Receiver<u64>,
        gc_depth: Round,
        rx_reconfigure: watch::Receiver<ReconfigureNotification>,
        rx_header_waiter: Receiver<WaiterMessage>,
        tx_headers_loopback: Sender<Header>,
        metrics: Arc<PrimaryMetrics>,
        primary_network: P2pNetwork,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            Self {
                name,
                committee,
                worker_cache,
                certificate_store,
                payload_store,
                rx_consensus_round_updates,
                gc_depth,
                rx_reconfigure,
                rx_header_waiter,
                tx_headers_loopback,
                network: primary_network,
                parent_requests: HashMap::new(),
                batch_requests: HashMap::new(),
                pending: HashMap::new(),
                metrics,
            }
            .run()
            .await;
        })
    }

    async fn wait_for_batches(
        digests: HashMap<u32, Vec<BatchDigest>>,
        synchronize_handles: Vec<CancelOnDropHandler<Result<anemo::Response<()>>>>,
        store: Store<(BatchDigest, WorkerId), PayloadToken>,
        deliver: Header,
        handler: oneshot::Receiver<()>,
    ) -> DagResult<Option<Header>> {
        tokio::select! {
            result = try_join_all(synchronize_handles) => {
                result.map_err(|e| DagError::NetworkError(format!("{e:?}")))?;
                for (worker_id, worker_digests) in digests {
                    for digest in worker_digests {
                        store.write((digest, worker_id), 0u8).await;
                    }
                }
                Ok(Some(deliver))
            },
            _ = handler => Ok(None),
        }
    }

    async fn wait_for_parents(
        missing: Vec<CertificateDigest>,
        store: CertificateStore,
        deliver: Header,
        handler: oneshot::Receiver<()>,
    ) -> DagResult<Option<Header>> {
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

        info!(
            "HeaderWaiter on node {} has started successfully.",
            self.name
        );
        loop {
            let mut attempt_garbage_collection = false;

            tokio::select! {
                Some(message) = self.rx_header_waiter.recv(), if waiting.available_permits() > 0 => {
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

                            // Ensure we didn't already send a sync request for these parents.
                            let mut requires_sync = HashMap::new();
                            for (digest, worker_id) in missing.into_iter() {
                                self.batch_requests.entry(digest).or_insert_with(|| {
                                    requires_sync.entry(worker_id).or_insert_with(Vec::new).push(digest);
                                    round
                                });
                            }
                            let mut synchronize_handles = Vec::new();
                            for (worker_id, digests) in requires_sync.clone() {
                                let worker_name = self.worker_cache
                                    .load()
                                    .worker(&self.name, &worker_id)
                                    .expect("Author of valid header is not in the worker cache")
                                    .name;

                                let message = WorkerSynchronizeMessage{digests, target: author.clone()};
                                synchronize_handles.push(self.network.send(worker_name, &message).await);
                            }

                            // Add the header to the waiter pool. The waiter will return it to when all
                            // its parents are in the store.
                            let (tx_cancel, rx_cancel) = oneshot::channel();
                            self.pending.insert(header_id, (round, tx_cancel));
                            let fut = Self::wait_for_batches(
                                requires_sync,
                                synchronize_handles,
                                self.payload_store.clone(),
                                header,
                                rx_cancel);
                            // pointer-size allocation, bounded by the # of blocks
                            // (may eventually go away, see rust RFC #1909)
                            waiting.push(Box::pin(fut)).await;
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
                            self.metrics.last_parent_missing_round
                            .with_label_values(&[&self.committee.epoch.to_string()]).set(round as i64);

                            // Add the header to the waiter pool. The waiter will return it to us
                            // when all its parents are in the store.
                            let wait_for = missing.clone();
                            let (tx_cancel, rx_cancel) = oneshot::channel();
                            self.pending.insert(header_id, (round, tx_cancel));
                            let fut = Self::wait_for_parents(wait_for, self.certificate_store.clone(), header, rx_cancel);
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
                                let message = PrimaryMessage::CertificatesRequest(requires_sync, self.name.clone());
                                let _ = self.network.unreliable_send(self.committee.network_key(&author).unwrap(), &message);
                            }
                        }
                    }
                },

                // we poll the availability of a slot to send the result to the core simultaneously
                Some(header) = waiting.next() => {
                    let header = match header{
                        Err(err) => {
                            warn!("Error fetching header {}", err);
                            continue;
                        },
                        Ok(header) => header,
                    };
                    if let Some(header) = header {
                        if let Some((_, tx_cancel)) = self.pending.remove(&header.id) {
                            let _ = tx_cancel.send(());
                        }
                        for x in header.payload.keys() {
                            let _ = self.batch_requests.remove(x);
                        }
                        for x in &header.parents {
                            let _ = self.parent_requests.remove(x);
                        }
                        // Ok to drop the header if core is overloaded.
                        let _ = self.tx_headers_loopback.try_send(header);
                    }
                },  // This request has been canceled when result is None.

                // Check whether the committee changed.
                result = self.rx_reconfigure.changed() => {
                    result.expect("Committee channel dropped");
                    let message = self.rx_reconfigure.borrow().clone();
                    match message {
                        ReconfigureNotification::NewEpoch(new_committee) => {
                            self.committee = new_committee;

                            self.pending.clear();
                            self.batch_requests.clear();
                            self.parent_requests.clear();
                        },
                        ReconfigureNotification::UpdateCommittee(new_committee) => {
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
