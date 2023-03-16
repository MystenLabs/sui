// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use config::WorkerId;
use crypto::NetworkPublicKey;
use mysten_common::notify_once::NotifyOnce;
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use store::Store;
use tokio::task::JoinSet;
use tokio::{sync::Mutex, task::JoinHandle};
use tracing::debug;
use types::{metered_channel::Receiver, Batch, BatchDigest, WorkerFetchBatchMessage};

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
struct DigestFetchInfo {
    worker_id: WorkerId,
    workers: HashSet<NetworkPublicKey>,
    notify: Arc<NotifyOnce>,
}

struct PayloadFetcher<V> {
    // The batch store
    store: Store<BatchDigest, Batch>,
    // The network to use for sending requests
    network: anemo::Network,
    // Number of random nodes to query when retrying batch requests.
    pub request_batch_retry_nodes: usize,
    // Validate incoming batches
    pub validator: V,
    rx_fetch_batch_immediately: Receiver<WorkerFetchBatchMessage>,
    rx_fetch_batch_eventually: Receiver<WorkerFetchBatchMessage>,
    // Digests missing locally. Thread-safe.
    pub digests_to_fetch: Arc<Mutex<HashMap<WorkerId, HashMap<BatchDigest, DigestFetchInfo>>>>,
    /// Keeps the handle to the inflight fetch batch tasks. We want to immediately
    /// send out a fetch batch request if there are no inflight requests. If there
    /// is atleast one inflight fetch task we will wait for a short period of time
    /// to enqueue more digests so we can issue a batch fetch request.
    fetch_batch_task: JoinSet<()>,
}

impl<V: TransactionValidator> PayloadFetcher<V> {
    #[must_use]
    pub fn spawn(
        store: Store<BatchDigest, Batch>,
        network: anemo::Network,
        request_batch_retry_nodes: usize,
        validator: V,
        rx_fetch_batch_immediately: Receiver<WorkerFetchBatchMessage>,
        rx_fetch_batch_eventually: Receiver<WorkerFetchBatchMessage>,
    ) -> JoinHandle<()> {
        let digests_to_fetch = Arc::new(Mutex::new(HashMap::new()));
        let fetch_batch_task = JoinSet::new();

        let mut fetcher = Self {
            store,
            network,
            request_batch_retry_nodes,
            validator,
            rx_fetch_batch_immediately,
            rx_fetch_batch_eventually,
            digests_to_fetch,
            fetch_batch_task,
        };

        tokio::spawn(async move {
            fetcher.run().await;
        })
    }

    async fn run(&mut self) {
        loop {
            tokio::select! {
                // Process a batch fetch request that needs to be fetched immediately.
                Some(msg) = self.rx_fetch_batch_immediately.recv() => {
                    let WorkerFetchBatchMessage { digest, worker_id, fetch_candidates, validate, notify_sender } = msg;

                    // If the batch is already in the fetch queue, skip it but send notification to requester.
                    if self.digests_to_fetch.lock().await[&worker_id].contains_key(&digest) {
                        notify_sender.send(self.digests_to_fetch.lock().await[&worker_id].get(&digest).unwrap().notify.clone()).unwrap();
                        continue;
                    }

                    // Add the digest to the fetch queue.
                    let notify = Arc::new(NotifyOnce::new());
                    let fetch_info = DigestFetchInfo {
                        worker_id,
                        workers: fetch_candidates.clone().into_iter().collect(),
                        notify: notify.clone(),
                    };

                    // Send notify back to the requester to wait on.
                    notify_sender.send(notify.clone()).unwrap();

                    let mut digests_to_fetch = self.digests_to_fetch.lock().await;
                    //.get_mut(&worker_id).unwrap().insert(digest, fetch_info);
                    match digests_to_fetch.entry(worker_id) {
                        Entry::Occupied(mut entry) => {
                            entry.get_mut().insert(digest, fetch_info);
                        }
                        Entry::Vacant(entry) => {
                            let mut map = HashMap::new();
                            map.insert(digest, fetch_info);
                            entry.insert(map);
                        }
                    }

                    // Notify the fetcher that there is a new digest to fetch.
                    notify.notify();


                    // Fetch the batches immediately.
                    // self.fetch_batches(worker_id).await;
                }
                // Process a batch fetch request that can be fetched eventually.
                Some(msg) = self.rx_fetch_batch_eventually.recv() => {
                    // let WorkerFetchBatchMessage
                }
            }
        }
    }
}
