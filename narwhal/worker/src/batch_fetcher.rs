// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use anemo::Network;
use anyhow::bail;
use async_trait::async_trait;
use crypto::NetworkPublicKey;
use fastcrypto::hash::Hash;
use futures::{stream::FuturesUnordered, FutureExt, StreamExt};
use itertools::Itertools;
use network::WorkerRpc;
use prometheus::IntGauge;
use rand::{rngs::ThreadRng, seq::SliceRandom};
use store::{rocks::DBMap, Map};
use tracing::debug;
use types::{Batch, BatchDigest, RequestBatchesRequest, RequestBatchesResponse};

use crate::metrics::WorkerMetrics;

const REMOTE_PARALLEL_FETCH_INTERVAL: Duration = Duration::from_secs(2);

pub struct BatchFetcher {
    network: Arc<dyn SubscriberNetwork>,
    batch_store: DBMap<BatchDigest, Batch>,
    metrics: Arc<WorkerMetrics>,
}

impl BatchFetcher {
    pub fn new(
        network: Network,
        batch_store: DBMap<BatchDigest, Batch>,
        metrics: Arc<WorkerMetrics>,
    ) -> Self {
        Self {
            network: Arc::new(SubscriberNetworkImpl { network }),
            batch_store,
            metrics,
        }
    }

    /// Bulk fetches payload from local storage and remote workers.
    /// This function performs infinite retries and blocks until all batches are available.
    pub async fn fetch(
        &self,
        digests: HashSet<BatchDigest>,
        known_workers: HashSet<NetworkPublicKey>,
    ) -> HashMap<BatchDigest, Batch> {
        debug!(
            "Attempting to fetch {} digests from {} workers",
            digests.len(),
            known_workers.len()
        );

        let mut remaining_digests = digests;
        let mut fetched_batches = HashMap::new();
        loop {
            if remaining_digests.is_empty() {
                return fetched_batches;
            }

            let _timer = self.metrics.worker_local_fetch_latency.start_timer();
            fetched_batches.extend(self.fetch_local(remaining_digests.clone()).await);
            drop(_timer);

            remaining_digests.retain(|d| !fetched_batches.contains_key(d));
            if remaining_digests.is_empty() {
                return fetched_batches;
            }

            // TODO: Can further parallelize this by target worker_id if necessary.
            let _timer = self.metrics.worker_remote_fetch_latency.start_timer();
            let mut known_workers = known_workers.clone().into_iter().collect_vec();
            known_workers.shuffle(&mut ThreadRng::default());
            let mut stagger = Duration::from_secs(0);
            let mut futures = FuturesUnordered::new();
            for worker in &known_workers {
                let future = self.fetch_remote(stagger, worker.clone(), remaining_digests.clone());
                futures.push(future.boxed());
                // TODO: Make this a parameter, and also record workers / authorities that are down
                // to request from them batches later.
                stagger += REMOTE_PARALLEL_FETCH_INTERVAL;
            }

            while let Some(remote_batches) = futures.next().await {
                for (remote_batch_digest, remote_batch) in remote_batches {
                    if remaining_digests.remove(&remote_batch_digest) {
                        fetched_batches.insert(remote_batch_digest, remote_batch);
                    }
                }

                if remaining_digests.is_empty() {
                    return fetched_batches;
                }
            }
        }
    }

    async fn fetch_local(&self, digests: HashSet<BatchDigest>) -> HashMap<BatchDigest, Batch> {
        let _timer = self.metrics.worker_local_fetch_latency.start_timer();
        let mut fetched_batches = HashMap::new();
        if digests.is_empty() {
            return fetched_batches;
        }

        // Continue to bulk request from local worker until no remaining digests
        // are available.
        debug!("Local attempt to fetch {} digests", digests.len());
        let local_batches = self
            .batch_store
            .multi_get(digests.clone().into_iter())
            .expect("Failed to get batches");
        for (digest, batch) in digests.into_iter().zip(local_batches.into_iter()) {
            if let Some(batch) = batch {
                self.metrics
                    .worker_batch_fetch
                    .with_label_values(&["local", "success"])
                    .inc();
                fetched_batches.insert(digest, batch);
            } else {
                self.metrics
                    .worker_batch_fetch
                    .with_label_values(&["local", "failure"])
                    .inc();
            }
        }

        fetched_batches
    }

    /// This future performs a fetch from a given remote worker
    /// This future performs infinite retries with exponential backoff
    /// You can specify stagger_delay before request is issued
    async fn fetch_remote(
        &self,
        stagger_delay: Duration,
        worker: NetworkPublicKey,
        digests: HashSet<BatchDigest>,
    ) -> HashMap<BatchDigest, Batch> {
        tokio::time::sleep(stagger_delay).await;
        // TODO: Make these config parameters
        let timeout = Duration::from_secs(10);
        let mut attempt = 0usize;
        let mut fetched_batches: HashMap<BatchDigest, Batch> = HashMap::new();
        loop {
            attempt += 1;
            debug!(
                "Remote attempt #{attempt} to fetch {} digests from {worker}",
                digests.len(),
            );
            let request_batch_guard =
                PendingGuard::make_inc(&self.metrics.pending_remote_request_batch);
            let response = self
                .safe_request_batches(digests.clone(), worker.clone(), timeout)
                .await;
            drop(request_batch_guard);
            match response {
                Ok(remote_batches) => {
                    self.metrics
                        .worker_batch_fetch
                        .with_label_values(&["remote", "success"])
                        .inc();
                    debug!("Found {} batches remotely", remote_batches.len());
                    for (remote_batch_digest, remote_batch) in remote_batches {
                        if digests.contains(&remote_batch_digest) {
                            fetched_batches.insert(remote_batch_digest, remote_batch);
                        }
                    }
                    return fetched_batches;
                }
                Err(err) => {
                    if err.to_string().contains("Timeout") {
                        self.metrics
                            .worker_batch_fetch
                            .with_label_values(&["remote", "timeout"])
                            .inc();
                    } else {
                        self.metrics
                            .worker_batch_fetch
                            .with_label_values(&["remote", "fail"])
                            .inc();
                    }
                    debug!("Error retrieving payloads {digests:?} from {worker} attempt {attempt}: {err}")
                }
            }
            tokio::time::sleep(timeout).await;
        }
    }

    /// Issue request_batches RPC and verifies response integrity
    async fn safe_request_batches(
        &self,
        digests: HashSet<BatchDigest>,
        worker: NetworkPublicKey,
        timeout: Duration,
    ) -> anyhow::Result<HashMap<BatchDigest, Batch>> {
        let mut verified_batches = HashMap::new();
        let mut digests_to_fetch = digests.clone();

        // Continue to bulk request from remote worker until no remaining
        // digests are available.
        loop {
            if digests_to_fetch.is_empty() {
                break;
            }
            let RequestBatchesResponse {
                batches,
                is_size_limit_reached,
            } = self
                .network
                .request_batches(
                    digests_to_fetch.clone().into_iter().collect(),
                    worker.clone(),
                    timeout,
                )
                .await?;
            // Use this flag to determine if the worker returned a valid digest
            // to protect against an adversarial worker returning size limit reached
            // and no batches leading us to retry this worker forever or until
            // we get batches from other workers.
            let mut is_digest_received = false;
            for batch in batches {
                let batch_digest = batch.digest();
                if !digests_to_fetch.contains(&batch_digest) {
                    bail!("[Protocol violation] Worker {worker} returned batch with digest {batch_digest} which is not part of the requested digests: {digests:?}");
                } else {
                    is_digest_received = true;
                    verified_batches.insert(batch_digest, batch);
                    digests_to_fetch.remove(&batch_digest);
                }
            }
            if !is_size_limit_reached || !is_digest_received {
                break;
            }
        }

        Ok(verified_batches)
    }
}

// todo - make it generic so that other can reuse
struct PendingGuard<'a> {
    metric: &'a IntGauge,
}

impl<'a> PendingGuard<'a> {
    pub fn make_inc(metric: &'a IntGauge) -> Self {
        metric.inc();
        Self { metric }
    }
}

impl<'a> Drop for PendingGuard<'a> {
    fn drop(&mut self) {
        self.metric.dec()
    }
}

// Trait for unit tests
#[async_trait]
pub trait SubscriberNetwork: Send + Sync {
    async fn request_batches(
        &self,
        batch_digests: Vec<BatchDigest>,
        worker: NetworkPublicKey,
        timeout: Duration,
    ) -> anyhow::Result<RequestBatchesResponse>;
}

struct SubscriberNetworkImpl {
    network: anemo::Network,
}

#[async_trait]
impl SubscriberNetwork for SubscriberNetworkImpl {
    async fn request_batches(
        &self,
        batch_digests: Vec<BatchDigest>,
        worker: NetworkPublicKey,
        timeout: Duration,
    ) -> anyhow::Result<RequestBatchesResponse> {
        let request =
            anemo::Request::new(RequestBatchesRequest { batch_digests }).with_timeout(timeout);
        self.network.request_batches(worker, request).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crypto::NetworkKeyPair;
    use fastcrypto::hash::Hash;
    use fastcrypto::traits::KeyPair;
    use itertools::Itertools;
    use rand::rngs::StdRng;
    use std::collections::HashMap;

    #[tokio::test]
    pub async fn test_fetcher() {
        let mut network = TestSubscriberNetwork::new();
        let batch_store = test_utils::create_batch_store();
        let batch1 = Batch::new(vec![vec![1]]);
        let batch2 = Batch::new(vec![vec![2]]);
        let (digests, known_workers) = (
            HashSet::from_iter(vec![batch1.digest(), batch2.digest()]),
            HashSet::from_iter(test_pks(&[1, 2])),
        );
        network.put(&[1, 2], batch1.clone());
        network.put(&[2, 3], batch2.clone());
        let fetcher = BatchFetcher {
            network: Arc::new(network.clone()),
            batch_store,
            metrics: Arc::new(WorkerMetrics::default()),
        };
        let expected_batches = HashMap::from_iter(vec![
            (batch1.digest(), batch1.clone()),
            (batch2.digest(), batch2.clone()),
        ]);
        let fetched_batches = fetcher.fetch(digests, known_workers).await;
        assert_eq!(fetched_batches, expected_batches);
    }

    #[tokio::test]
    pub async fn test_fetcher_locally_with_remaining() {
        // Limit is set to two batches in test request_batches(). Request 3 batches
        // and ensure another request is sent to get the remaining batches.
        let mut network = TestSubscriberNetwork::new();
        let batch_store = test_utils::create_batch_store();
        let batch1 = Batch::new(vec![vec![1]]);
        let batch2 = Batch::new(vec![vec![2]]);
        let batch3 = Batch::new(vec![vec![3]]);
        let (digests, known_workers) = (
            HashSet::from_iter(vec![batch1.digest(), batch2.digest(), batch3.digest()]),
            HashSet::from_iter(test_pks(&[1, 2, 3])),
        );
        for batch in &[&batch1, &batch2, &batch3] {
            batch_store.insert(&batch.digest(), batch).unwrap();
        }
        network.put(&[1, 2], batch1.clone());
        network.put(&[2, 3], batch2.clone());
        network.put(&[3, 4], batch3.clone());
        let fetcher = BatchFetcher {
            network: Arc::new(network.clone()),
            batch_store,
            metrics: Arc::new(WorkerMetrics::default()),
        };
        let expected_batches = HashMap::from_iter(vec![
            (batch1.digest(), batch1.clone()),
            (batch2.digest(), batch2.clone()),
            (batch3.digest(), batch3.clone()),
        ]);
        let fetched_batches = fetcher.fetch(digests, known_workers).await;
        assert_eq!(fetched_batches, expected_batches);
    }

    #[tokio::test]
    pub async fn test_fetcher_remote_with_remaining() {
        // Limit is set to two batches in test request_batches(). Request 3 batches
        // and ensure another request is sent to get the remaining batches.
        let mut network = TestSubscriberNetwork::new();
        let batch_store = test_utils::create_batch_store();
        let batch1 = Batch::new(vec![vec![1]]);
        let batch2 = Batch::new(vec![vec![2]]);
        let batch3 = Batch::new(vec![vec![3]]);
        let (digests, known_workers) = (
            HashSet::from_iter(vec![batch1.digest(), batch2.digest(), batch3.digest()]),
            HashSet::from_iter(test_pks(&[2, 3, 4])),
        );
        network.put(&[3, 4], batch1.clone());
        network.put(&[2, 3], batch2.clone());
        network.put(&[2, 3, 4], batch3.clone());
        let fetcher = BatchFetcher {
            network: Arc::new(network.clone()),
            batch_store,
            metrics: Arc::new(WorkerMetrics::default()),
        };
        let expected_batches = HashMap::from_iter(vec![
            (batch1.digest(), batch1.clone()),
            (batch2.digest(), batch2.clone()),
            (batch3.digest(), batch3.clone()),
        ]);
        let fetched_batches = fetcher.fetch(digests, known_workers).await;
        assert_eq!(fetched_batches, expected_batches);
    }

    #[tokio::test]
    pub async fn test_fetcher_local_and_remote() {
        let mut network = TestSubscriberNetwork::new();
        let batch_store = test_utils::create_batch_store();
        let batch1 = Batch::new(vec![vec![1]]);
        let batch2 = Batch::new(vec![vec![2]]);
        let batch3 = Batch::new(vec![vec![3]]);
        let (digests, known_workers) = (
            HashSet::from_iter(vec![batch1.digest(), batch2.digest(), batch3.digest()]),
            HashSet::from_iter(test_pks(&[1, 2, 3, 4])),
        );
        batch_store.insert(&batch1.digest(), &batch1).unwrap();
        network.put(&[1, 2, 3], batch1.clone());
        network.put(&[2, 3, 4], batch2.clone());
        network.put(&[1, 4], batch3.clone());
        let fetcher = BatchFetcher {
            network: Arc::new(network.clone()),
            batch_store,
            metrics: Arc::new(WorkerMetrics::default()),
        };
        let expected_batches = HashMap::from_iter(vec![
            (batch1.digest(), batch1.clone()),
            (batch2.digest(), batch2.clone()),
            (batch3.digest(), batch3.clone()),
        ]);
        let fetched_batches = fetcher.fetch(digests, known_workers).await;
        assert_eq!(fetched_batches, expected_batches);
    }

    #[tokio::test]
    pub async fn test_fetcher_response_size_limit() {
        let mut network = TestSubscriberNetwork::new();
        let batch_store = test_utils::create_batch_store();
        let num_digests = 12;
        let mut expected_batches = Vec::new();
        // 6 batches available locally with response size limit of 2
        for i in 0..num_digests / 2 {
            let batch = Batch::new(vec![vec![i]]);
            batch_store.insert(&batch.digest(), &batch).unwrap();
            network.put(&[1, 2, 3], batch.clone());
            expected_batches.push(batch);
        }
        // 6 batches available remotely with response size limit of 2
        for i in (num_digests / 2)..num_digests {
            let batch = Batch::new(vec![vec![i]]);
            network.put(&[1, 2, 3], batch.clone());
            expected_batches.push(batch);
        }

        let expected_batches = HashMap::from_iter(
            expected_batches
                .iter()
                .map(|batch| (batch.digest(), batch.clone())),
        );
        let (digests, known_workers) = (
            HashSet::from_iter(expected_batches.clone().into_keys()),
            HashSet::from_iter(test_pks(&[1, 2, 3])),
        );
        let fetcher = BatchFetcher {
            network: Arc::new(network.clone()),
            batch_store,
            metrics: Arc::new(WorkerMetrics::default()),
        };
        let fetched_batches = fetcher.fetch(digests, known_workers).await;
        assert_eq!(fetched_batches, expected_batches);
    }

    #[derive(Clone)]
    struct TestSubscriberNetwork {
        // Worker name -> batch digests it has -> batches.
        data: HashMap<NetworkPublicKey, HashMap<BatchDigest, Batch>>,
    }

    impl TestSubscriberNetwork {
        pub fn new() -> Self {
            Self {
                data: HashMap::new(),
            }
        }

        pub fn put(&mut self, keys: &[u8], batch: Batch) {
            for key in keys {
                let key = test_pk(*key);
                let entry = self.data.entry(key).or_default();
                entry.insert(batch.digest(), batch.clone());
            }
        }
    }

    #[async_trait]
    impl SubscriberNetwork for TestSubscriberNetwork {
        async fn request_batches(
            &self,
            digests: Vec<BatchDigest>,
            worker: NetworkPublicKey,
            _timeout: Duration,
        ) -> anyhow::Result<RequestBatchesResponse> {
            // Use this to simulate server side response size limit in RequestBatches
            const MAX_REQUEST_BATCHES_RESPONSE_SIZE: usize = 2;
            const MAX_READ_BATCH_DIGESTS: usize = 5;

            let mut is_size_limit_reached = false;
            let mut batches = Vec::new();
            let mut total_size = 0;

            let digests_chunks = digests
                .chunks(MAX_READ_BATCH_DIGESTS)
                .map(|chunk| chunk.to_vec())
                .collect_vec();
            for digests_chunk in digests_chunks {
                for digest in digests_chunk {
                    if let Some(batch) = self.data.get(&worker).unwrap().get(&digest) {
                        if total_size < MAX_REQUEST_BATCHES_RESPONSE_SIZE {
                            batches.push(batch.clone());
                            total_size += batch.size();
                        } else {
                            is_size_limit_reached = true;
                            break;
                        }
                    }
                }
            }

            Ok(RequestBatchesResponse {
                batches,
                is_size_limit_reached,
            })
        }
    }

    fn test_pk(i: u8) -> NetworkPublicKey {
        use rand::SeedableRng;
        let mut rng = StdRng::from_seed([i; 32]);
        NetworkKeyPair::generate(&mut rng).public().clone()
    }

    fn test_pks(i: &[u8]) -> Vec<NetworkPublicKey> {
        i.iter().map(|i| test_pk(*i)).collect()
    }
}
