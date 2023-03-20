// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crypto::NetworkPublicKey;
use fastcrypto::hash::Hash;
use mysten_common::notify_once::NotifyOnce;
use mysten_metrics::monitored_future;
use network::WorkerRpc;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::Duration;
use store::Store;
// use tokio::task::JoinSet;
use tokio::{sync::Mutex, task::JoinHandle};
// use tracing::debug;
use types::Batch;
use types::{metered_channel::Receiver, BatchDigest, Round, WorkerFetchBatchMessage};

use crate::TransactionValidator;

#[cfg(test)]
#[path = "tests/payload_fetcher_tests.rs"]
pub mod payload_fetcher_tests;

// 3 entry points for fetching batches:
// 1) While processing request vote synchronize all batches referenced in the header.
//  Check to make sure batch is fully validated in this scenario. This is just to ensure
//  that the batch available in the store but not actually returned.
// 2) While processing certificate synchronize all batches referenced in the certificate.
//  This happens asynchronously and is not required to be fully validated. This is just to
//  ensure that the batch is available in the store but not actually returned.
// 3) After subdag has been committed fetch all batches in the subdag to be used
//  for execution. This is the only case where the actual batch contents are returned.
// NOTE: [external consensus] block_synchronizer/block_waiter do also have a path to fetching batches

// If we maintain a notify channel for each batch digest, requesters can just wait on that.
// Case 1 & 2 will wait on notify, case 2 will not wait on notify.

// We could have three channels for accepting incoming batches that require a fetch
// then we could handle them differently if needed while utilizing a majority of shared code

// Maximum number of payloads to fetch with one request.
// TODO Change this to a server side check instead of checking on the client
// send all requests to server and check as batches are being processed
const MAX_BATCH_TO_FETCH: usize = 100; // BATCH_SIZE (.5mb) * 100 = 50MB

/// Holds fetch information for a missing digest
#[derive(Debug, Clone)]
pub struct DigestFetchInfo {
    digest: BatchDigest,
    // TODO ? option network pub key... header from author and certificate from everyone
    // ensure that there is an upgrade from header to certificarte "canddidates"
    fetch_candidates: HashSet<NetworkPublicKey>,
    notify: Arc<NotifyOnce>,
    fetch_now: bool,
    max_age: Round,
    validate: bool,
}

pub struct PayloadFetcherState {
    digests_map: HashMap<BatchDigest, DigestFetchInfo>,
    // TODO ? parallel fetch of headers and certificates
    fetch_now_digests: VecDeque<BatchDigest>,
    fetch_later_digests: VecDeque<BatchDigest>,
}

struct PayloadFetcher<V> {
    // Internal state of fetcher containing missing digests. Thread-safe.
    pub state: Arc<Mutex<PayloadFetcherState>>,
    // The local batch store
    store: Store<BatchDigest, Batch>,
    // The worker network to use for sending requests to other workers.
    network: anemo::Network,
    // Number of random nodes (from preferred worker peer list)to query
    // when retrying batch requests.
    pub request_batch_retry_nodes: usize,
    // Validate incoming batches
    pub validator: V,
    rx_fetch_batch: Receiver<WorkerFetchBatchMessage>,
    /// Keeps the handle to the inflight fetch batch tasks. We want to immediately
    /// send out a fetch batch request if there are no inflight requests. If there
    /// is atleast one inflight fetch task we will wait for a short period of time
    /// to enqueue more digests so we can issue a batch fetch request.
    inflight_requests: HashMap<BatchDigest, (HashSet<NetworkPublicKey>, JoinHandle<()>)>,
}

impl<V: TransactionValidator> PayloadFetcher<V> {
    #[must_use]
    pub fn spawn(
        store: Store<BatchDigest, Batch>,
        network: anemo::Network,
        request_batch_retry_nodes: usize,
        validator: V,
        rx_fetch_batch: Receiver<WorkerFetchBatchMessage>,
    ) -> JoinHandle<()> {
        let state = Arc::new(Mutex::new(PayloadFetcherState {
            digests_map: HashMap::new(),
            fetch_now_digests: VecDeque::new(),
            fetch_later_digests: VecDeque::new(),
        }));
        let inflight_requests = HashMap::new();

        let mut fetcher = Self {
            state,
            store,
            network,
            request_batch_retry_nodes,
            validator,
            rx_fetch_batch,
            inflight_requests,
        };

        tokio::spawn(async move {
            fetcher.run().await;
        })
    }

    async fn run(&mut self) {
        let mut fetch_timer = tokio::time::interval(Duration::from_millis(50));

        loop {
            tokio::select! {
                Some(msg) = self.rx_fetch_batch.recv() => {
                    let WorkerFetchBatchMessage {
                        digest,
                        fetch_candidates,
                        validate,
                        fetch_now,
                        notify_sender,
                        max_age,
                    } = msg;

                    let mut state_guard = self.state.lock().await;
                    let PayloadFetcherState {digests_map, fetch_now_digests, fetch_later_digests} = &mut *state_guard;

                    if let Some(fetch_info) = digests_map.get_mut(&digest) {
                        // Update priority if the new request has a higher priority.
                        if fetch_now && !fetch_info.fetch_now {
                            fetch_later_digests.retain(|d| d != &digest);
                            fetch_now_digests.push_back(digest.clone());
                            fetch_info.fetch_now = true;
                        }
                        // Update max_age if the new request has a higher max_age.
                        // TODO ? fine to always take the latest max_age received?
                        // No attack vector because only effects the node
                        if max_age > fetch_info.max_age {
                            fetch_info.max_age = max_age;
                        }
                        // Only way this will flip is if header digest becomes part of a certificate
                        // and its okay not to validate
                        fetch_info.validate = validate;
                        // Always optimistically add the new fetch candidates
                        fetch_info.fetch_candidates.extend(fetch_candidates);
                        // Send notify channel to the requester to wait on.
                        notify_sender.send(fetch_info.notify.clone()).unwrap();
                    } else {
                        let fetch_info = DigestFetchInfo {
                            digest: digest.clone(),
                            fetch_candidates,
                            notify: Arc::new(NotifyOnce::new()),
                            fetch_now,
                            max_age,
                            validate
                        };

                        // Send notify channel to the requester to wait on.
                        notify_sender.send(fetch_info.notify.clone()).unwrap();

                        // Add the new DigestFetchInfo to the digests_map.
                        digests_map.insert(digest.clone(), fetch_info);

                        // Add the digest to the appropriate queue.
                        if fetch_now {
                            fetch_now_digests.push_back(digest.clone());
                        } else {
                            fetch_later_digests.push_back(digest.clone());
                        }
                    }
                }
                _ = fetch_timer.tick() => {
                    self.process_fetch_queue().await;
                }

            }

            // Add a step to check if the timer elapsed since the last request
            // and then fetch all pending digests.
        }
    }

    async fn process_fetch_queue(&mut self) {
        let mut state_guard = self.state.lock().await;
        let PayloadFetcherState {
            digests_map,
            fetch_now_digests,
            fetch_later_digests,
        } = &mut *state_guard;

        let mut combined_digests = fetch_now_digests.clone();
        combined_digests.append(&mut fetch_later_digests.clone());

        let chunks: Vec<_> = combined_digests
            .into_iter()
            .collect::<Vec<_>>()
            .chunks(MAX_BATCH_TO_FETCH)
            .map(|chunk| chunk.to_vec())
            .collect();

        for chunk in chunks {
            let mut fetch_candidates_superset = HashSet::new();
            for digest in &chunk {
                if let Some(fetch_info) = digests_map.get(digest) {
                    fetch_candidates_superset.extend(fetch_info.fetch_candidates.clone());
                }
            }
            let handles: Vec<_> = fetch_candidates_superset
                .into_iter()
                .map(|worker| {
                    let chunk_cloned = chunk.clone();
                    monitored_future!(async move {
                        Self::request_batches(self.network.clone(), chunk_cloned, worker).await
                    })
                })
                .collect();

            // Collect and process the results.
            for handle in handles {
                if let Ok(batches) = handle.await {
                    for batch_opt in batches {
                        if let Some(batch) = batch_opt {
                            let batch_digest = batch.digest();
                            if digests_map.contains_key(&batch_digest) {
                                // Atomically write to the store and remove the digest from the HashMap and VecDeque.
                                self.store.sync_write(batch_digest.clone(), batch).await;
                                digests_map.remove(&batch_digest);
                                fetch_now_digests.retain(|digest| digest != &batch_digest);
                                fetch_later_digests.retain(|digest| digest != &batch_digest);
                            }
                        }
                    }
                }
            }
        }
    }

    async fn request_batches(
        network: anemo::Network,
        digests: Vec<BatchDigest>,
        worker: NetworkPublicKey,
    ) -> anyhow::Result<Vec<Option<Batch>>> {
        network.request_batches(worker, digests).await
    }
}
