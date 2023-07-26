// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{HashMap, HashSet, VecDeque},
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
use sui_protocol_config::ProtocolConfig;
use tokio::{
    select,
    time::{sleep, sleep_until, Instant},
};
use tracing::debug;
use types::{
    now, validate_batch_version, Batch, BatchAPI, BatchDigest, MetadataAPI, RequestBatchesRequest,
    RequestBatchesResponse,
};

use crate::metrics::WorkerMetrics;

const REMOTE_PARALLEL_FETCH_INTERVAL: Duration = Duration::from_secs(2);
const WORKER_RETRY_INTERVAL: Duration = Duration::from_secs(1);

pub struct BatchFetcher {
    name: NetworkPublicKey,
    network: Arc<dyn RequestBatchesNetwork>,
    batch_store: DBMap<BatchDigest, Batch>,
    metrics: Arc<WorkerMetrics>,
    protocol_config: ProtocolConfig,
}

impl BatchFetcher {
    pub fn new(
        name: NetworkPublicKey,
        network: Network,
        batch_store: DBMap<BatchDigest, Batch>,
        metrics: Arc<WorkerMetrics>,
        protocol_config: ProtocolConfig,
    ) -> Self {
        Self {
            name,
            network: Arc::new(RequestBatchesNetworkImpl { network }),
            batch_store,
            metrics,
            protocol_config,
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
        // TODO: verify known_workers meets quorum threshold, or just use all other workers.
        let known_workers = known_workers
            .into_iter()
            .filter(|worker| worker != &self.name)
            .collect_vec();

        loop {
            if remaining_digests.is_empty() {
                return fetched_batches;
            }

            // Fetch from local storage.
            let _timer = self.metrics.worker_local_fetch_latency.start_timer();
            fetched_batches.extend(self.fetch_local(remaining_digests.clone()).await);
            remaining_digests.retain(|d| !fetched_batches.contains_key(d));
            if remaining_digests.is_empty() {
                return fetched_batches;
            }
            drop(_timer);

            // Fetch from remote workers.
            // TODO: Can further parallelize this by target worker_id if necessary.
            let _timer = self.metrics.worker_remote_fetch_latency.start_timer();
            let mut known_workers: Vec<_> = known_workers.iter().collect();
            known_workers.shuffle(&mut ThreadRng::default());
            let mut known_workers = VecDeque::from(known_workers);
            let mut stagger = Duration::from_secs(0);
            let mut futures = FuturesUnordered::new();

            loop {
                assert!(!remaining_digests.is_empty());
                if let Some(worker) = known_workers.pop_front() {
                    let future = self.fetch_remote(worker.clone(), remaining_digests.clone());
                    futures.push(future.boxed());
                } else {
                    // No more worker to fetch from. This happens after sending requests to all
                    // workers and then another staggered interval has passed.
                    break;
                }
                stagger += REMOTE_PARALLEL_FETCH_INTERVAL;
                let mut interval = Box::pin(sleep(stagger));
                select! {
                    result = futures.next() => {
                        if let Some(remote_batches) = result {
                            let new_batches: HashMap<_, _> = remote_batches.iter().filter(|(d, _)| remaining_digests.remove(d)).collect();
                            // Also persist the batches, so they are available after restarts.
                            let mut write_batch = self.batch_store.batch();

                            // TODO: Remove once we have upgraded to protocol version 12.
                            if self.protocol_config.narwhal_versioned_metadata() {
                                // Set received_at timestamp for remote batches.
                                let mut updated_new_batches = HashMap::new();
                                for (digest, batch) in new_batches {
                                    let mut batch = (*batch).clone();
                                    batch.versioned_metadata_mut().set_received_at(now());
                                    updated_new_batches.insert(*digest, batch.clone());
                                }
                                fetched_batches.extend(updated_new_batches.iter().map(|(d, b)| (*d, (*b).clone())));
                                write_batch.insert_batch(&self.batch_store, updated_new_batches).unwrap();
                            } else {
                                fetched_batches.extend(new_batches.iter().map(|(d, b)| (**d, (*b).clone())));
                                write_batch.insert_batch(&self.batch_store, new_batches).unwrap();
                            }

                            write_batch.write().unwrap();
                            if remaining_digests.is_empty() {
                                return fetched_batches;
                            }
                        }
                    }
                    _ = interval.as_mut() => {
                    }
                }
            }

            // After all known remote workers have been tried, restart the outer loop to fetch
            // from local storage then remote workers again.
            sleep(WORKER_RETRY_INTERVAL).await;
        }
    }

    async fn fetch_local(&self, digests: HashSet<BatchDigest>) -> HashMap<BatchDigest, Batch> {
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
                    .with_label_values(&["local", "missing"])
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
        worker: NetworkPublicKey,
        digests: HashSet<BatchDigest>,
    ) -> HashMap<BatchDigest, Batch> {
        // TODO: Make these config parameters
        let max_timeout = Duration::from_secs(60);
        let mut timeout = Duration::from_secs(10);
        let mut attempt = 0usize;
        loop {
            attempt += 1;
            debug!(
                "Remote attempt #{attempt} to fetch {} digests from {worker}",
                digests.len(),
            );
            let deadline = Instant::now() + timeout;
            let request_guard =
                PendingGuard::make_inc(&self.metrics.pending_remote_request_batches);
            let response = self
                .safe_request_batches(digests.clone(), worker.clone(), timeout)
                .await;
            drop(request_guard);
            match response {
                Ok(remote_batches) => {
                    self.metrics
                        .worker_batch_fetch
                        .with_label_values(&["remote", "success"])
                        .inc();
                    debug!("Found {} batches remotely", remote_batches.len());
                    return remote_batches;
                }
                Err(err) => {
                    if err.to_string().contains("Timeout") {
                        self.metrics
                            .worker_batch_fetch
                            .with_label_values(&["remote", "timeout"])
                            .inc();
                        debug!("Timed out retrieving payloads {digests:?} from {worker} attempt {attempt}: {err}");
                    } else if err.to_string().contains("[Protocol violation]") {
                        self.metrics
                            .worker_batch_fetch
                            .with_label_values(&["remote", "fail"])
                            .inc();
                        debug!("Failed retrieving payloads {digests:?} from possibly byzantine {worker} attempt {attempt}: {err}");
                        // Do not bother retrying if the remote worker is byzantine.
                        return HashMap::new();
                    } else {
                        self.metrics
                            .worker_batch_fetch
                            .with_label_values(&["remote", "fail"])
                            .inc();
                        debug!("Error retrieving payloads {digests:?} from {worker} attempt {attempt}: {err}");
                    }
                }
            }
            timeout += timeout / 2;
            timeout = std::cmp::min(max_timeout, timeout);
            // Since the call might have returned before timeout, we wait until originally planned deadline
            sleep_until(deadline).await;
        }
    }

    /// Issue request_batches RPC and verifies response integrity
    async fn safe_request_batches(
        &self,
        digests_to_fetch: HashSet<BatchDigest>,
        worker: NetworkPublicKey,
        timeout: Duration,
    ) -> anyhow::Result<HashMap<BatchDigest, Batch>> {
        let mut fetched_batches = HashMap::new();
        if digests_to_fetch.is_empty() {
            return Ok(fetched_batches);
        }

        let RequestBatchesResponse {
            batches,
            is_size_limit_reached: _,
        } = self
            .network
            .request_batches(
                digests_to_fetch.clone().into_iter().collect(),
                worker.clone(),
                timeout,
            )
            .await?;
        for batch in batches {
            // TODO: Remove once we have upgraded to protocol version 12.
            validate_batch_version(&batch, &self.protocol_config)
                .map_err(|err| anyhow::anyhow!("[Protocol violation] Invalid batch: {err}"))?;

            let batch_digest = batch.digest();
            if !digests_to_fetch.contains(&batch_digest) {
                bail!(
                    "[Protocol violation] Worker {worker} returned batch with digest \
                    {batch_digest} which is not part of the requested digests: {digests_to_fetch:?}"
                );
            }
            // This batch is part of a certificate, so no need to validate it.
            fetched_batches.insert(batch_digest, batch);
        }

        Ok(fetched_batches)
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
// TODO: migrate this WorkerRpc.
#[async_trait]
pub trait RequestBatchesNetwork: Send + Sync {
    async fn request_batches(
        &self,
        batch_digests: Vec<BatchDigest>,
        worker: NetworkPublicKey,
        timeout: Duration,
    ) -> anyhow::Result<RequestBatchesResponse>;
}

struct RequestBatchesNetworkImpl {
    network: anemo::Network,
}

#[async_trait]
impl RequestBatchesNetwork for RequestBatchesNetworkImpl {
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
    use test_utils::{get_protocol_config, latest_protocol_version};
    use tokio::time::timeout;

    // TODO: Remove once we have upgraded to protocol version 12.
    // Case #1: Receive BatchV1 and network has not upgraded to 12 so we are okay
    #[tokio::test]
    pub async fn test_fetcher_with_batch_v1_and_network_v11() {
        let mut network = TestRequestBatchesNetwork::new();
        let batch_store = test_utils::create_batch_store();
        let v11_protocol_config = get_protocol_config(11);
        let batchv1_1 = Batch::new(vec![vec![1]], &v11_protocol_config);
        let batchv1_2 = Batch::new(vec![vec![2]], &v11_protocol_config);
        let (digests, known_workers) = (
            HashSet::from_iter(vec![batchv1_1.digest(), batchv1_2.digest()]),
            HashSet::from_iter(test_pks(&[1, 2])),
        );
        network.put(&[1, 2], batchv1_1.clone());
        network.put(&[2, 3], batchv1_2.clone());
        let fetcher = BatchFetcher {
            name: test_pk(0),
            network: Arc::new(network.clone()),
            batch_store: batch_store.clone(),
            metrics: Arc::new(WorkerMetrics::default()),
            protocol_config: v11_protocol_config,
        };
        let expected_batches = HashMap::from_iter(vec![
            (batchv1_1.digest(), batchv1_1.clone()),
            (batchv1_2.digest(), batchv1_2.clone()),
        ]);
        let fetched_batches = fetcher.fetch(digests, known_workers).await;
        assert_eq!(fetched_batches, expected_batches);
        assert_eq!(
            batch_store
                .get(&batchv1_1.digest())
                .unwrap()
                .unwrap()
                .digest(),
            batchv1_1.digest()
        );
        assert_eq!(
            batch_store
                .get(&batchv1_2.digest())
                .unwrap()
                .unwrap()
                .digest(),
            batchv1_2.digest()
        );
    }

    // TODO: Remove once we have upgraded to protocol version 12.
    // Case #2: Receive BatchV1 but network has upgraded to 12 so we fail because we expect BatchV2
    #[tokio::test]
    pub async fn test_fetcher_with_batch_v1_and_network_v12() {
        telemetry_subscribers::init_for_testing();
        let mut network = TestRequestBatchesNetwork::new();
        let batch_store = test_utils::create_batch_store();
        let latest_protocol_config = latest_protocol_version();
        let v11_protocol_config = &get_protocol_config(11);
        let batchv1_1 = Batch::new(vec![vec![1]], v11_protocol_config);
        let batchv1_2 = Batch::new(vec![vec![2]], v11_protocol_config);
        let (digests, known_workers) = (
            HashSet::from_iter(vec![batchv1_1.digest(), batchv1_2.digest()]),
            HashSet::from_iter(test_pks(&[1, 2])),
        );
        network.put(&[1, 2], batchv1_1.clone());
        network.put(&[2, 3], batchv1_2.clone());
        let fetcher = BatchFetcher {
            name: test_pk(0),
            network: Arc::new(network.clone()),
            batch_store: batch_store.clone(),
            metrics: Arc::new(WorkerMetrics::default()),
            protocol_config: latest_protocol_config,
        };
        let fetch_result = timeout(
            Duration::from_secs(1),
            fetcher.fetch(digests, known_workers),
        )
        .await;
        assert!(fetch_result.is_err());
    }

    // TODO: Remove once we have upgraded to protocol version 12.
    // Case #3: Receive BatchV2 but network is still in v11 so we fail because we expect BatchV1
    #[tokio::test]
    pub async fn test_fetcher_with_batch_v2_and_network_v11() {
        telemetry_subscribers::init_for_testing();
        let mut network = TestRequestBatchesNetwork::new();
        let batch_store = test_utils::create_batch_store();
        let latest_protocol_config = &latest_protocol_version();
        let v11_protocol_config = get_protocol_config(11);
        let batchv2_1 = Batch::new(vec![vec![1]], latest_protocol_config);
        let batchv2_2 = Batch::new(vec![vec![2]], latest_protocol_config);
        let (digests, known_workers) = (
            HashSet::from_iter(vec![batchv2_1.digest(), batchv2_2.digest()]),
            HashSet::from_iter(test_pks(&[1, 2])),
        );
        network.put(&[1, 2], batchv2_1.clone());
        network.put(&[2, 3], batchv2_2.clone());
        let fetcher = BatchFetcher {
            name: test_pk(0),
            network: Arc::new(network.clone()),
            batch_store: batch_store.clone(),
            metrics: Arc::new(WorkerMetrics::default()),
            protocol_config: v11_protocol_config,
        };
        let fetch_result = timeout(
            Duration::from_secs(1),
            fetcher.fetch(digests, known_workers),
        )
        .await;
        assert!(fetch_result.is_err());
    }

    // TODO: Remove once we have upgraded to protocol version 12.
    // Case #4: Receive BatchV2 and network is upgraded to 12 so we are okay
    #[tokio::test]
    pub async fn test_fetcher_with_batch_v2_and_network_v12() {
        let mut network = TestRequestBatchesNetwork::new();
        let batch_store = test_utils::create_batch_store();
        let latest_protocol_config = &latest_protocol_version();
        let batchv2_1 = Batch::new(vec![vec![1]], latest_protocol_config);
        let batchv2_2 = Batch::new(vec![vec![2]], latest_protocol_config);
        let (digests, known_workers) = (
            HashSet::from_iter(vec![batchv2_1.digest(), batchv2_2.digest()]),
            HashSet::from_iter(test_pks(&[1, 2])),
        );
        network.put(&[1, 2], batchv2_1.clone());
        network.put(&[2, 3], batchv2_2.clone());
        let fetcher = BatchFetcher {
            name: test_pk(0),
            network: Arc::new(network.clone()),
            batch_store: batch_store.clone(),
            metrics: Arc::new(WorkerMetrics::default()),
            protocol_config: latest_protocol_version(),
        };
        let mut expected_batches = HashMap::from_iter(vec![
            (batchv2_1.digest(), batchv2_1.clone()),
            (batchv2_2.digest(), batchv2_2.clone()),
        ]);
        let mut fetched_batches = fetcher.fetch(digests, known_workers).await;
        // Reset metadata from the fetched and expected batches
        for batch in fetched_batches.values_mut() {
            // assert received_at was set to some value before resetting.
            assert!(batch.versioned_metadata().received_at().is_some());
            batch.versioned_metadata_mut().set_received_at(0);
        }
        for batch in expected_batches.values_mut() {
            batch.versioned_metadata_mut().set_received_at(0);
        }
        assert_eq!(fetched_batches, expected_batches);
        assert_eq!(
            batch_store
                .get(&batchv2_1.digest())
                .unwrap()
                .unwrap()
                .digest(),
            batchv2_1.digest()
        );
        assert_eq!(
            batch_store
                .get(&batchv2_2.digest())
                .unwrap()
                .unwrap()
                .digest(),
            batchv2_2.digest()
        );
    }

    #[tokio::test]
    pub async fn test_fetcher() {
        let mut network = TestRequestBatchesNetwork::new();
        let batch_store = test_utils::create_batch_store();
        let batch1 = Batch::new(vec![vec![1]], &latest_protocol_version());
        let batch2 = Batch::new(vec![vec![2]], &latest_protocol_version());
        let (digests, known_workers) = (
            HashSet::from_iter(vec![batch1.digest(), batch2.digest()]),
            HashSet::from_iter(test_pks(&[1, 2])),
        );
        network.put(&[1, 2], batch1.clone());
        network.put(&[2, 3], batch2.clone());
        let fetcher = BatchFetcher {
            name: test_pk(0),
            network: Arc::new(network.clone()),
            batch_store: batch_store.clone(),
            metrics: Arc::new(WorkerMetrics::default()),
            protocol_config: latest_protocol_version(),
        };
        let mut expected_batches = HashMap::from_iter(vec![
            (batch1.digest(), batch1.clone()),
            (batch2.digest(), batch2.clone()),
        ]);
        let mut fetched_batches = fetcher.fetch(digests, known_workers).await;
        // Reset metadata from the fetched and expected batches
        for batch in fetched_batches.values_mut() {
            // assert received_at was set to some value before resetting.
            assert!(batch.versioned_metadata().received_at().is_some());
            batch.versioned_metadata_mut().set_received_at(0);
        }
        for batch in expected_batches.values_mut() {
            batch.versioned_metadata_mut().set_received_at(0);
        }
        assert_eq!(fetched_batches, expected_batches);
        assert_eq!(
            batch_store.get(&batch1.digest()).unwrap().unwrap().digest(),
            batch1.digest()
        );
        assert_eq!(
            batch_store.get(&batch2.digest()).unwrap().unwrap().digest(),
            batch2.digest()
        );
    }

    #[tokio::test]
    pub async fn test_fetcher_locally_with_remaining() {
        // Limit is set to two batches in test request_batches(). Request 3 batches
        // and ensure another request is sent to get the remaining batches.
        let mut network = TestRequestBatchesNetwork::new();
        let batch_store = test_utils::create_batch_store();
        let batch1 = Batch::new(vec![vec![1]], &latest_protocol_version());
        let batch2 = Batch::new(vec![vec![2]], &latest_protocol_version());
        let batch3 = Batch::new(vec![vec![3]], &latest_protocol_version());
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
            name: test_pk(0),
            network: Arc::new(network.clone()),
            batch_store,
            metrics: Arc::new(WorkerMetrics::default()),
            protocol_config: latest_protocol_version(),
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
        let mut network = TestRequestBatchesNetwork::new();
        let batch_store = test_utils::create_batch_store();
        let batch1 = Batch::new(vec![vec![1]], &latest_protocol_version());
        let batch2 = Batch::new(vec![vec![2]], &latest_protocol_version());
        let batch3 = Batch::new(vec![vec![3]], &latest_protocol_version());
        let (digests, known_workers) = (
            HashSet::from_iter(vec![batch1.digest(), batch2.digest(), batch3.digest()]),
            HashSet::from_iter(test_pks(&[2, 3, 4])),
        );
        network.put(&[3, 4], batch1.clone());
        network.put(&[2, 3], batch2.clone());
        network.put(&[2, 3, 4], batch3.clone());
        let fetcher = BatchFetcher {
            name: test_pk(0),
            network: Arc::new(network.clone()),
            batch_store,
            metrics: Arc::new(WorkerMetrics::default()),
            protocol_config: latest_protocol_version(),
        };
        let mut expected_batches = HashMap::from_iter(vec![
            (batch1.digest(), batch1.clone()),
            (batch2.digest(), batch2.clone()),
            (batch3.digest(), batch3.clone()),
        ]);
        let mut fetched_batches = fetcher.fetch(digests, known_workers).await;

        // Reset metadata from the fetched and expected batches
        for batch in fetched_batches.values_mut() {
            // assert received_at was set to some value before resetting.
            assert!(batch.versioned_metadata().received_at().is_some());
            batch.versioned_metadata_mut().set_received_at(0);
        }
        for batch in expected_batches.values_mut() {
            batch.versioned_metadata_mut().set_received_at(0);
        }

        assert_eq!(fetched_batches, expected_batches);
    }

    #[tokio::test]
    pub async fn test_fetcher_local_and_remote() {
        let mut network = TestRequestBatchesNetwork::new();
        let batch_store = test_utils::create_batch_store();
        let batch1 = Batch::new(vec![vec![1]], &latest_protocol_version());
        let batch2 = Batch::new(vec![vec![2]], &latest_protocol_version());
        let batch3 = Batch::new(vec![vec![3]], &latest_protocol_version());
        let (digests, known_workers) = (
            HashSet::from_iter(vec![batch1.digest(), batch2.digest(), batch3.digest()]),
            HashSet::from_iter(test_pks(&[1, 2, 3, 4])),
        );
        batch_store.insert(&batch1.digest(), &batch1).unwrap();
        network.put(&[1, 2, 3], batch1.clone());
        network.put(&[2, 3, 4], batch2.clone());
        network.put(&[1, 4], batch3.clone());
        let fetcher = BatchFetcher {
            name: test_pk(0),
            network: Arc::new(network.clone()),
            batch_store,
            metrics: Arc::new(WorkerMetrics::default()),
            protocol_config: latest_protocol_version(),
        };
        let mut expected_batches = HashMap::from_iter(vec![
            (batch1.digest(), batch1.clone()),
            (batch2.digest(), batch2.clone()),
            (batch3.digest(), batch3.clone()),
        ]);
        let mut fetched_batches = fetcher.fetch(digests, known_workers).await;

        // Reset metadata from the fetched and expected remote batches
        for batch in fetched_batches.values_mut() {
            if batch.digest() != batch1.digest() {
                // assert received_at was set to some value for remote batches before resetting.
                assert!(batch.versioned_metadata().received_at().is_some());
                batch.versioned_metadata_mut().set_received_at(0);
            }
        }
        for batch in expected_batches.values_mut() {
            if batch.digest() != batch1.digest() {
                batch.versioned_metadata_mut().set_received_at(0);
            }
        }

        assert_eq!(fetched_batches, expected_batches);
    }

    #[tokio::test]
    pub async fn test_fetcher_response_size_limit() {
        let mut network = TestRequestBatchesNetwork::new();
        let batch_store = test_utils::create_batch_store();
        let num_digests = 12;
        let mut expected_batches = Vec::new();
        let mut local_digests = Vec::new();
        // 6 batches available locally with response size limit of 2
        for i in 0..num_digests / 2 {
            let batch = Batch::new(vec![vec![i]], &latest_protocol_version());
            local_digests.push(batch.digest());
            batch_store.insert(&batch.digest(), &batch).unwrap();
            network.put(&[1, 2, 3], batch.clone());
            expected_batches.push(batch);
        }
        // 6 batches available remotely with response size limit of 2
        for i in (num_digests / 2)..num_digests {
            let batch = Batch::new(vec![vec![i]], &latest_protocol_version());
            network.put(&[1, 2, 3], batch.clone());
            expected_batches.push(batch);
        }

        let mut expected_batches = HashMap::from_iter(
            expected_batches
                .iter()
                .map(|batch| (batch.digest(), batch.clone())),
        );
        let (digests, known_workers) = (
            HashSet::from_iter(expected_batches.clone().into_keys()),
            HashSet::from_iter(test_pks(&[1, 2, 3])),
        );
        let fetcher = BatchFetcher {
            name: test_pk(0),
            network: Arc::new(network.clone()),
            batch_store,
            metrics: Arc::new(WorkerMetrics::default()),
            protocol_config: latest_protocol_version(),
        };
        let mut fetched_batches = fetcher.fetch(digests, known_workers).await;

        // Reset metadata from the fetched and expected remote batches
        for batch in fetched_batches.values_mut() {
            if !local_digests.contains(&batch.digest()) {
                // assert received_at was set to some value for remote batches before resetting.
                assert!(batch.versioned_metadata().received_at().is_some());
                batch.versioned_metadata_mut().set_received_at(0);
            }
        }
        for batch in expected_batches.values_mut() {
            if !local_digests.contains(&batch.digest()) {
                batch.versioned_metadata_mut().set_received_at(0);
            }
        }

        assert_eq!(fetched_batches, expected_batches);
    }

    // TODO: add test for timeouts, failures and retries.

    #[derive(Clone)]
    struct TestRequestBatchesNetwork {
        // Worker name -> batch digests it has -> batches.
        data: HashMap<NetworkPublicKey, HashMap<BatchDigest, Batch>>,
    }

    impl TestRequestBatchesNetwork {
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
    impl RequestBatchesNetwork for TestRequestBatchesNetwork {
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
