// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    error::{DagError, DagResult},
    messages::{BatchDigest, CertificateDigest, Header, HeaderDigest},
    primary::{PayloadToken, PrimaryMessage, PrimaryWorkerMessage, Round},
    Certificate,
};
use bytes::Bytes;
use config::{Committee, WorkerId};
use crypto::traits::VerifyingKey;
use futures::{
    future::{try_join_all, BoxFuture},
    stream::{futures_unordered::FuturesUnordered, StreamExt as _},
};
use network::SimpleSender;
use serde::{de::DeserializeOwned, Serialize};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{SystemTime, UNIX_EPOCH},
};
use store::Store;
use tokio::{
    sync::mpsc::{channel, Receiver, Sender},
    time::{sleep, Duration, Instant},
};
use tracing::{debug, error};

/// The resolution of the timer that checks whether we received replies to our sync requests, and triggers
/// new sync requests if we didn't.
const TIMER_RESOLUTION: u64 = 1_000;

/// The commands that can be sent to the `Waiter`.
#[derive(Debug)]
pub enum WaiterMessage<PublicKey: VerifyingKey> {
    SyncBatches(HashMap<BatchDigest, WorkerId>, Header<PublicKey>),
    SyncParents(Vec<CertificateDigest>, Header<PublicKey>),
}

/// Waits for missing parent certificates and batches' digests.
pub struct HeaderWaiter<PublicKey: VerifyingKey> {
    /// The name of this authority.
    name: PublicKey,
    /// The committee information.
    committee: Committee<PublicKey>,
    /// The persistent storage for parent Certificates.
    header_store: Store<CertificateDigest, Certificate<PublicKey>>,
    /// The persistent storage for payload markers from workers.
    payload_store: Store<(BatchDigest, WorkerId), PayloadToken>,
    /// The current consensus round (used for cleanup).
    consensus_round: Arc<AtomicU64>,
    /// The depth of the garbage collector.
    gc_depth: Round,
    /// The delay to wait before re-trying sync requests.
    sync_retry_delay: u64,
    /// Determine with how many nodes to sync when re-trying to send sync-request.
    sync_retry_nodes: usize,

    /// Receives sync commands from the `Synchronizer`.
    rx_synchronizer: Receiver<WaiterMessage<PublicKey>>,
    /// Loops back to the core headers for which we got all parents and batches.
    tx_core: Sender<Header<PublicKey>>,

    /// Network driver allowing to send messages.
    network: SimpleSender,
    /// Keeps the digests of the all certificates for which we sent a sync request,
    /// along with a time stamp (`u128`) indicating when we sent the request.
    parent_requests: HashMap<CertificateDigest, (Round, u128)>,
    /// Keeps the digests of the all TX batches for which we sent a sync request,
    /// similarly to `header_requests`.
    batch_requests: HashMap<BatchDigest, Round>,
    /// List of digests (headers or tx batch) that are waiting to be processed.
    /// Their processing will resume when we get all their dependencies.
    pending: HashMap<HeaderDigest, (Round, Sender<()>)>,
}

impl<PublicKey: VerifyingKey> HeaderWaiter<PublicKey> {
    pub fn spawn(
        name: PublicKey,
        committee: Committee<PublicKey>,
        header_store: Store<CertificateDigest, Certificate<PublicKey>>,
        payload_store: Store<(BatchDigest, WorkerId), PayloadToken>,
        consensus_round: Arc<AtomicU64>,
        gc_depth: Round,
        sync_retry_delay: u64,
        sync_retry_nodes: usize,
        rx_synchronizer: Receiver<WaiterMessage<PublicKey>>,
        tx_core: Sender<Header<PublicKey>>,
    ) {
        tokio::spawn(async move {
            Self {
                name,
                committee,
                header_store,
                payload_store,
                consensus_round,
                gc_depth,
                sync_retry_delay,
                sync_retry_nodes,
                rx_synchronizer,
                tx_core,
                network: SimpleSender::new(),
                parent_requests: HashMap::new(),
                batch_requests: HashMap::new(),
                pending: HashMap::new(),
            }
            .run()
            .await;
        });
    }

    /// Helper function. It waits for particular data to become available in the storage
    /// and then delivers the specified header.
    async fn waiter<T, V>(
        missing: Vec<T>,
        store: Store<T, V>,
        deliver: Header<PublicKey>,
        mut handler: Receiver<()>,
    ) -> DagResult<Option<Header<PublicKey>>>
    where
        T: Serialize + DeserializeOwned + Send + Clone,
        V: Serialize + DeserializeOwned + Send,
    {
        let waiting: Vec<_> = missing.into_iter().map(|x| store.notify_read(x)).collect();
        tokio::select! {
            result = try_join_all(waiting) => {
                result.map(|_| Some(deliver)).map_err(DagError::from)
            }
            _ = handler.recv() => Ok(None),
        }
    }

    /// Main loop listening to the `Synchronizer` messages.
    async fn run(&mut self) {
        let mut waiting: FuturesUnordered<BoxFuture<'_, _>> = FuturesUnordered::new();

        let timer = sleep(Duration::from_millis(TIMER_RESOLUTION));
        tokio::pin!(timer);

        loop {
            tokio::select! {
                Some(message) = self.rx_synchronizer.recv() => {
                    match message {
                        WaiterMessage::SyncBatches(missing, header) => {
                            debug!("Synching the payload of {}", header);
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
                            let (tx_cancel, rx_cancel) = channel(1);
                            self.pending.insert(header_id, (round, tx_cancel));
                            let fut = Self::waiter(wait_for, self.payload_store.clone(), header, rx_cancel);
                            // pointer-size allocation, bounded by the # of blocks (may eventually go away, see rust RFC #1909)
                            waiting.push(Box::pin(fut));

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
                                    .worker(&author, &worker_id)
                                    .expect("Author of valid header is not in the committee")
                                    .primary_to_worker;
                                let message = PrimaryWorkerMessage::Synchronize(digests, author.clone());
                                let bytes = bincode::serialize(&message)
                                    .expect("Failed to serialize batch sync request");
                                self.network.send(address, Bytes::from(bytes)).await;
                            }
                        }

                        WaiterMessage::SyncParents(missing, header) => {
                            debug!("Synching the parents of {}", header);
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
                            let (tx_cancel, rx_cancel) = channel(1);
                            self.pending.insert(header_id, (round, tx_cancel));
                            let fut = Self::waiter(wait_for, self.header_store.clone(), header, rx_cancel);
                            // pointer-size allocation, bounded by the # of blocks (may eventually go away, see rust RFC #1909)
                            waiting.push(Box::pin(fut));

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
                                let bytes = bincode::serialize(&message).expect("Failed to serialize cert request");
                                self.network.send(address, Bytes::from(bytes)).await;
                            }
                        }
                    }
                },

                Some(result) = waiting.next() => match result {
                    Ok(Some(header)) => {
                        let _ = self.pending.remove(&header.id);
                        for x in header.payload.keys() {
                            let _ = self.batch_requests.remove(x);
                        }
                        for x in &header.parents {
                            let _ = self.parent_requests.remove(x);
                        }
                        self.tx_core.send(header).await.expect("Failed to send header");
                    },
                    Ok(None) => {
                        // This request has been canceled.
                    },
                    Err(e) => {
                        error!("{}", e);
                        panic!("Storage failure: killing node.");
                    }
                },

                () = &mut timer => {
                    // We optimistically sent sync requests to a single node. If this timer triggers,
                    // it means we were wrong to trust it. We are done waiting for a reply and we now
                    // broadcast the request to all nodes.
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .expect("Failed to measure time")
                        .as_millis();

                    let mut retry = Vec::new();
                    for (digest, (_, timestamp)) in &self.parent_requests {
                        if timestamp + (self.sync_retry_delay as u128) < now {
                            debug!("Requesting sync for certificate {} (retry)", digest);
                            retry.push(*digest);
                        }
                    }

                    let addresses = self.committee
                        .others_primaries(&self.name)
                        .iter()
                        .map(|(_, x)| x.primary_to_primary)
                        .collect();
                    let message = PrimaryMessage::CertificatesRequest(retry, self.name.clone());
                    let bytes = bincode::serialize(&message).expect("Failed to serialize cert request");
                    self.network.lucky_broadcast(addresses, Bytes::from(bytes), self.sync_retry_nodes).await;

                    // Reschedule the timer.
                    timer.as_mut().reset(Instant::now() + Duration::from_millis(TIMER_RESOLUTION));
                }
            }

            // Cleanup internal state.
            let round = self.consensus_round.load(Ordering::Relaxed);
            if round > self.gc_depth {
                let mut gc_round = round - self.gc_depth;

                for (r, handler) in self.pending.values() {
                    if r <= &gc_round {
                        let _ = handler.send(()).await;
                    }
                }
                self.pending.retain(|_, (r, _)| r > &mut gc_round);
                self.batch_requests.retain(|_, r| r > &mut gc_round);
                self.parent_requests.retain(|_, (r, _)| r > &mut gc_round);
            }
        }
    }
}
