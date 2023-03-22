// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{errors::SubscriberResult, metrics::ExecutorMetrics, ExecutionState};

use config::{AuthorityIdentifier, Committee, WorkerCache, WorkerId};
use crypto::NetworkPublicKey;

use futures::stream::FuturesOrdered;
use futures::FutureExt;
use futures::StreamExt;

use futures::stream::FuturesUnordered;
use network::WorkerRpc;

use anyhow::bail;
use prometheus::IntGauge;
use std::collections::HashMap;
use std::collections::HashSet;
use std::{sync::Arc, time::Duration, vec};
use types::RequestBatchesRequest;

use async_trait::async_trait;
use fastcrypto::hash::Hash;
use mysten_metrics::spawn_logged_monitored_task;
use tokio::time::Instant;
use tokio::{sync::oneshot, task::JoinHandle};
use tracing::{debug, error, warn};
use tracing::{info, instrument};
use types::{
    metered_channel, Batch, BatchDigest, Certificate, CommittedSubDag,
    ConditionalBroadcastReceiver, ConsensusOutput, RequestBatchesResponse, Timestamp,
};

/// The `Subscriber` receives certificates sequenced by the consensus and waits until the
/// downloaded all the transactions references by the certificates; it then
/// forward the certificates to the Executor.
pub struct Subscriber<Network> {
    /// Receiver for shutdown
    rx_shutdown: ConditionalBroadcastReceiver,
    /// A channel to receive sequenced consensus messages.
    rx_sequence: metered_channel::Receiver<CommittedSubDag>,
    /// The metrics handler
    metrics: Arc<ExecutorMetrics>,

    fetcher: Fetcher<Network>,
}

struct Fetcher<Network> {
    network: Network,
    metrics: Arc<ExecutorMetrics>,
}

pub fn spawn_subscriber<State: ExecutionState + Send + Sync + 'static>(
    authority_id: AuthorityIdentifier,
    network: oneshot::Receiver<anemo::Network>,
    worker_cache: WorkerCache,
    committee: Committee,
    mut shutdown_receivers: Vec<ConditionalBroadcastReceiver>,
    rx_sequence: metered_channel::Receiver<CommittedSubDag>,
    metrics: Arc<ExecutorMetrics>,
    restored_consensus_output: Vec<CommittedSubDag>,
    state: State,
) -> Vec<JoinHandle<()>> {
    // This is ugly but has to be done this way for now
    // Currently network incorporate both server and client side of RPC interface
    // To construct server side we need to set up routes first, which requires starting Primary
    // Some cleanup is needed

    let (tx_notifier, rx_notifier) =
        metered_channel::channel(primary::CHANNEL_CAPACITY, &metrics.tx_notifier);

    let rx_shutdown_notify = shutdown_receivers
        .pop()
        .unwrap_or_else(|| panic!("Not enough shutdown receivers"));
    let rx_shutdown_subscriber = shutdown_receivers
        .pop()
        .unwrap_or_else(|| panic!("Not enough shutdown receivers"));

    vec![
        spawn_logged_monitored_task!(
            run_notify(state, rx_notifier, rx_shutdown_notify),
            "SubscriberNotifyTask"
        ),
        spawn_logged_monitored_task!(
            create_and_run_subscriber(
                authority_id,
                network,
                worker_cache,
                committee,
                rx_shutdown_subscriber,
                rx_sequence,
                metrics,
                restored_consensus_output,
                tx_notifier,
            ),
            "SubscriberTask"
        ),
    ]
}

async fn run_notify<State: ExecutionState + Send + Sync + 'static>(
    state: State,
    mut tr_notify: metered_channel::Receiver<ConsensusOutput>,
    mut rx_shutdown: ConditionalBroadcastReceiver,
) {
    loop {
        tokio::select! {
            Some(message) = tr_notify.recv() => {
                state.handle_consensus_output(message).await;
            }

            _ = rx_shutdown.receiver.recv() => {
                return
            }

        }
    }
}

async fn create_and_run_subscriber(
    authority_id: AuthorityIdentifier,
    network: oneshot::Receiver<anemo::Network>,
    worker_cache: WorkerCache,
    committee: Committee,
    rx_shutdown: ConditionalBroadcastReceiver,
    rx_sequence: metered_channel::Receiver<CommittedSubDag>,
    metrics: Arc<ExecutorMetrics>,
    restored_consensus_output: Vec<CommittedSubDag>,
    tx_notifier: metered_channel::Sender<ConsensusOutput>,
) {
    let network = network.await.expect("Failed to receive network");
    info!("Starting subscriber");
    let network = SubscriberNetworkImpl {
        authority_id,
        worker_cache,
        committee,
        network,
    };
    let fetcher = Fetcher {
        network,
        metrics: metrics.clone(),
    };
    let subscriber = Subscriber {
        rx_shutdown,
        rx_sequence,
        metrics,
        fetcher,
    };
    subscriber
        .run(restored_consensus_output, tx_notifier)
        .await
        .expect("Failed to run subscriber")
}

impl<Network: SubscriberNetwork> Subscriber<Network> {
    /// Returns the max number of sub-dag to fetch payloads concurrently.
    const MAX_PENDING_PAYLOADS: usize = 1000;

    /// Main loop connecting to the consensus to listen to sequence messages.
    async fn run(
        mut self,
        restored_consensus_output: Vec<CommittedSubDag>,
        tx_notifier: metered_channel::Sender<ConsensusOutput>,
    ) -> SubscriberResult<()> {
        // It's important to have the futures in ordered fashion as we want
        // to guarantee that will deliver to the executor the certificates
        // in the same order we received from rx_sequence. So it doesn't
        // matter if we somehow managed to fetch the batches from a later
        // certificate. Unless the earlier certificate's payload has been
        // fetched, no later certificate will be delivered.
        let mut waiting = FuturesOrdered::new();

        // First handle any consensus output messages that were restored due to a restart.
        // This needs to happen before we start listening on rx_sequence and receive messages sequenced after these.
        for message in restored_consensus_output {
            let future = self.fetcher.fetch_batches(message);
            waiting.push_back(future);

            self.metrics.subscriber_recovered_certificates_count.inc();
        }

        // Listen to sequenced consensus message and process them.
        loop {
            tokio::select! {
                // Receive the ordered sequence of consensus messages from a consensus node.
                Some(sub_dag) = self.rx_sequence.recv(), if waiting.len() < Self::MAX_PENDING_PAYLOADS => {
                    // We can schedule more then MAX_PENDING_PAYLOADS payloads but
                    // don't process more consensus messages when more
                    // then MAX_PENDING_PAYLOADS is pending
                    waiting.push_back(self.fetcher.fetch_batches(sub_dag));
                },

                // Receive here consensus messages for which we have downloaded all transactions data.
                Some(message) = waiting.next() => {
                    if let Err(e) = tx_notifier.send(message).await {
                        error!("tx_notifier closed: {}", e);
                        return Ok(());
                    }
                },

                _ = self.rx_shutdown.receiver.recv() => {
                    return Ok(())
                }

            }

            self.metrics
                .waiting_elements_subscriber
                .set(waiting.len() as i64);
        }
    }
}

impl<Network: SubscriberNetwork> Fetcher<Network> {
    /// Returns ordered vector of futures for downloading batches for certificates
    /// Order of futures returned follows order of batches in the certificate
    /// See fetch_batches_from_worker for more details
    #[instrument(level = "debug", skip_all, fields(certificate = % deliver.leader.digest()))]
    async fn fetch_batches(&self, deliver: CommittedSubDag) -> ConsensusOutput {
        let num_batches = deliver.num_batches();
        let num_certs = deliver.len();
        if num_batches == 0 {
            debug!("No batches to fetch, payload is empty");
            return ConsensusOutput {
                sub_dag: Arc::new(deliver),
                batches: vec![],
            };
        }

        let sub_dag = Arc::new(deliver);
        let mut subscriber_output = ConsensusOutput {
            sub_dag: sub_dag.clone(),
            batches: Vec::with_capacity(num_certs),
        };

        let mut workers_for_certificates: HashSet<NetworkPublicKey> = HashSet::new();
        let mut batch_digests: HashMap<WorkerId, HashSet<BatchDigest>> = HashMap::new();

        let mut num_batches = 0;
        for cert in &sub_dag.certificates {
            for (digest, (worker_id, _)) in cert.header.payload.iter() {
                let workers = self.network.workers_for_certificate(cert, worker_id);
                workers_for_certificates.extend(workers);
                batch_digests.entry(*worker_id).or_default().insert(*digest);
                num_batches += 1;
            }
        }

        let fetched_batches_timer = self
            .metrics
            .batch_fetch_for_committed_subdag_total_latency
            .start_timer();
        self.metrics
            .committed_subdag_batch_count
            .observe(num_batches as f64);
        let fetched_batches = self
            .fetch_batches_from_worker(
                batch_digests,
                workers_for_certificates.into_iter().collect(),
            )
            .await;
        drop(fetched_batches_timer);

        // Map all fetched batches to their respective certificates and submit as
        // consensus output
        for cert in &sub_dag.certificates {
            let mut output_batches = Vec::with_capacity(num_batches);
            let output_cert = cert.clone();

            self.metrics
                .subscriber_current_round
                .set(cert.round() as i64);

            self.metrics
                .subscriber_certificate_latency
                .observe(cert.metadata.created_at.elapsed().as_secs_f64());

            for (digest, (_, _)) in cert.header.payload.iter() {
                self.metrics.subscriber_processed_batches.inc();
                let batch = fetched_batches
                    .get(digest)
                    .expect("[Protocol violation] Batch not found in fetched batches from certificate authors");

                debug!(
                    "Adding fetched batch {digest} from certificate {} to consensus output",
                    cert.digest()
                );
                output_batches.push(batch.clone());
            }
            subscriber_output
                .batches
                .push((output_cert, output_batches));
        }
        subscriber_output
    }

    /// Bulk fetches payload from workers
    /// This future performs infinite retries and blocks until all batches are available
    /// As an optimization it tries to download from local worker first, but then fans out
    /// requests to remote worker if not found locally
    async fn fetch_batches_from_worker(
        &self,
        batch_digests: HashMap<WorkerId, HashSet<BatchDigest>>,
        workers: Vec<NetworkPublicKey>,
    ) -> HashMap<BatchDigest, Batch> {
        let mut fetched_batches = HashMap::new();

        for (worker_id, digests) in batch_digests {
            debug!("Attempting to fetch {} digests from workers {worker_id}: {workers:?} digests:{digests:?}", digests.len());
            // TODO: Can further parallelize this by worker_id if necessary.
            // Only have one worker for now so will leave this for a future
            // optimization.
            let mut remaining_digests = digests.clone();

            let local_batches = self
                .try_fetch_locally(remaining_digests.clone().into_iter().collect(), worker_id)
                .await;
            for (local_batch_digest, local_batch) in local_batches {
                if remaining_digests.remove(&local_batch_digest) {
                    let batch_fetch_duration =
                        local_batch.metadata.created_at.elapsed().as_secs_f64();
                    self.metrics
                        .batch_execution_latency
                        .observe(batch_fetch_duration);
                    debug!(
                        "Batch {:?} took {} seconds to be fetched for execution since creation",
                        local_batch_digest, batch_fetch_duration
                    );
                    fetched_batches.insert(local_batch_digest, local_batch);
                }
            }

            if remaining_digests.is_empty() {
                return fetched_batches;
            }

            let _timer = self.metrics.subscriber_remote_fetch_latency.start_timer();
            let mut stagger = Duration::from_secs(0);
            let mut futures = FuturesUnordered::new();
            for worker in &workers {
                let future = self.fetch_remote(stagger, worker.clone(), remaining_digests.clone());
                futures.push(future.boxed());
                // TODO: Make this a parameter, and also record workers / authorities that are down
                // to request from them batches later.
                stagger += Duration::from_millis(500);
            }

            while let Some(remote_batches) = futures.next().await {
                for (remote_batch_digest, remote_batch) in remote_batches {
                    if remaining_digests.remove(&remote_batch_digest) {
                        let batch_fetch_duration =
                            remote_batch.metadata.created_at.elapsed().as_secs_f64();
                        self.metrics
                            .batch_execution_latency
                            .observe(batch_fetch_duration);
                        debug!(
                            "Batch {:?} took {} seconds to be fetched for execution since creation",
                            remote_batch_digest, batch_fetch_duration
                        );

                        fetched_batches.insert(remote_batch_digest, remote_batch);
                    }
                }

                if remaining_digests.is_empty() {
                    return fetched_batches;
                }
            }

            // Should never reach here, as we should keep looping until we have all batches remotely or locally.
            error!("Failed to fetch all required batches from workers. Remaining batches: {remaining_digests:?}");
        }
        fetched_batches
    }

    #[instrument(level = "debug", skip_all, fields(digests = ? digests, worker_id = % worker_id))]
    async fn try_fetch_locally(
        &self,
        digests: HashSet<BatchDigest>,
        worker_id: WorkerId,
    ) -> HashMap<BatchDigest, Batch> {
        let _timer = self.metrics.subscriber_local_fetch_latency.start_timer();
        let mut fetched_batches: HashMap<BatchDigest, Batch> = HashMap::new();
        let worker = self.network.my_worker(&worker_id);
        let mut digests_to_fetch = digests;

        // Continue to bulk request from local worker until no remaining digests
        // are available.
        loop {
            if digests_to_fetch.is_empty() {
                break;
            }
            debug!(
                "Attempting to fetch {} digests locally: {digests_to_fetch:?}",
                digests_to_fetch.len()
            );
            let timeout = Duration::from_secs(10);
            let rpc_response = self
                .network
                .request_batches(
                    digests_to_fetch.clone().into_iter().collect(),
                    worker.clone(),
                    timeout,
                )
                .await;

            match rpc_response {
                Ok(request_batches_response) => {
                    let RequestBatchesResponse {
                        batches,
                        remaining_batch_digests,
                    } = request_batches_response;
                    debug!("Found {} batches locally: {batches:?}", batches.len());
                    for local_batch in batches {
                        self.metrics
                            .subscriber_batch_fetch
                            .with_label_values(&["local", "success"])
                            .inc();
                        digests_to_fetch.remove(&local_batch.digest());
                        fetched_batches.insert(local_batch.digest(), local_batch);
                    }

                    digests_to_fetch = process_request_batches_remaining_batches(
                        &digests_to_fetch,
                        remaining_batch_digests,
                    );
                }
                Err(err) => {
                    if err.to_string().contains("Timeout") {
                        self.metrics
                            .subscriber_batch_fetch
                            .with_label_values(&["local", "timeout"])
                            .inc();
                    } else {
                        self.metrics
                            .subscriber_batch_fetch
                            .with_label_values(&["local", "fail"])
                            .inc();
                    }
                    warn!("Error communicating with our own worker: {err}");
                    break;
                }
            }
        }

        fetched_batches
    }

    /// This future performs a fetch from a given remote worker
    /// This future performs infinite retries with exponential backoff
    /// You can specify stagger_delay before request is issued
    #[instrument(level = "debug", skip_all, fields(stagger_delay = ? stagger_delay, worker = % worker, digests = ? digests))]
    async fn fetch_remote(
        &self,
        stagger_delay: Duration,
        worker: NetworkPublicKey,
        digests: HashSet<BatchDigest>,
    ) -> HashMap<BatchDigest, Batch> {
        tokio::time::sleep(stagger_delay).await;
        // TODO: Make these config parameters
        let max_timeout = Duration::from_secs(60);
        let mut timeout = Duration::from_secs(10);
        let mut attempt = 0usize;
        let mut fetched_batches: HashMap<BatchDigest, Batch> = HashMap::new();
        loop {
            attempt += 1;
            debug!(
                "Attempting to fetch {} digests remotely attempt {attempt}: {digests:?}",
                digests.len(),
            );
            let deadline = Instant::now() + timeout;
            let request_batch_guard =
                PendingGuard::make_inc(&self.metrics.pending_remote_request_batch);
            let response = self
                .safe_request_batches(digests.clone(), worker.clone(), timeout)
                .await;
            drop(request_batch_guard);
            match response {
                Ok(remote_batches) => {
                    self.metrics
                        .subscriber_batch_fetch
                        .with_label_values(&["remote", "success"])
                        .inc();
                    debug!(
                        "Found {} batches remotely: {remote_batches:?}",
                        remote_batches.len(),
                    );
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
                            .subscriber_batch_fetch
                            .with_label_values(&["remote", "timeout"])
                            .inc();
                    } else {
                        self.metrics
                            .subscriber_batch_fetch
                            .with_label_values(&["remote", "fail"])
                            .inc();
                    }
                    debug!("Error retrieving payloads {digests:?} from {worker} attempt {attempt}: {err}")
                }
            }

            timeout += timeout / 2;
            timeout = std::cmp::min(max_timeout, timeout);
            // Since the call might have returned before timeout, we wait until originally planned deadline
            tokio::time::sleep_until(deadline).await;
        }
    }

    /// Issue request_batches RPC and verifies response integrity
    #[instrument(level = "debug", skip_all, fields(worker = % worker, digests = ? digests, timeout = ? timeout))]
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
                remaining_batch_digests,
            } = self
                .network
                .request_batches(
                    digests_to_fetch.clone().into_iter().collect(),
                    worker.clone(),
                    timeout,
                )
                .await?;
            for batch in batches {
                let batch_digest = batch.digest();
                if !digests_to_fetch.contains(&batch_digest) {
                    bail!("[Protocol violation] Worker {worker} returned batch with digest {batch_digest} which is not part of the requested digests: {digests:?}");
                } else {
                    verified_batches.insert(batch_digest, batch);
                    digests_to_fetch.remove(&batch_digest);
                }
            }
            digests_to_fetch = process_request_batches_remaining_batches(
                &digests_to_fetch,
                remaining_batch_digests,
            );
        }

        Ok(verified_batches)
    }
}

fn process_request_batches_remaining_batches(
    digests_to_fetch: &HashSet<BatchDigest>,
    remaining_batch_digests: Vec<BatchDigest>,
) -> HashSet<BatchDigest> {
    let mut digests_to_refetch = HashSet::new();
    if !remaining_batch_digests.is_empty() {
        // Double check the remaining batches are what we were expecting.
        digests_to_refetch = remaining_batch_digests
            .into_iter()
            .filter(|digest| {
                if digests_to_fetch.contains(digest) {
                    true
                } else {
                    warn!("Unexpected remaining batch digest returned from local worker: {digest}");
                    false
                }
            })
            .collect();
    }
    digests_to_refetch
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
    fn my_worker(&self, worker_id: &WorkerId) -> NetworkPublicKey;
    fn workers_for_certificate(
        &self,
        certificate: &Certificate,
        worker_id: &WorkerId,
    ) -> Vec<NetworkPublicKey>;

    async fn request_batches(
        &self,
        batch_digests: Vec<BatchDigest>,
        worker: NetworkPublicKey,
        timeout: Duration,
    ) -> anyhow::Result<RequestBatchesResponse>;
}

struct SubscriberNetworkImpl {
    authority_id: AuthorityIdentifier,
    network: anemo::Network,
    worker_cache: WorkerCache,
    committee: Committee,
}

#[async_trait]
impl SubscriberNetwork for SubscriberNetworkImpl {
    fn my_worker(&self, worker_id: &WorkerId) -> NetworkPublicKey {
        self.worker_cache
            .worker(
                self.committee
                    .authority(&self.authority_id)
                    .unwrap()
                    .protocol_key(),
                worker_id,
            )
            .expect("Own worker not found in cache")
            .name
    }

    fn workers_for_certificate(
        &self,
        certificate: &Certificate,
        worker_id: &WorkerId,
    ) -> Vec<NetworkPublicKey> {
        let authorities = certificate.signed_authorities(&self.committee);
        authorities
            .into_iter()
            .filter_map(|authority| {
                let worker = self.worker_cache.worker(&authority, worker_id);
                match worker {
                    Ok(worker) => Some(worker.name),
                    Err(err) => {
                        error!(
                            "Worker {} not found for authority {}: {:?}",
                            worker_id, authority, err
                        );
                        None
                    }
                }
            })
            .collect()
    }

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
    use rand::rngs::StdRng;
    use std::collections::HashMap;

    #[tokio::test]
    pub async fn test_fetcher() {
        let mut network = TestSubscriberNetwork::new();
        let batch1 = Batch::new(vec![vec![1]]);
        let batch2 = Batch::new(vec![vec![2]]);
        let bulk_batches: HashMap<WorkerId, HashSet<BatchDigest>> = HashMap::from_iter(vec![(
            0,
            HashSet::from_iter(vec![batch1.digest(), batch2.digest()]),
        )]);
        network.put(&[1, 2], batch1.clone());
        network.put(&[2, 3], batch2.clone());
        let fetcher = Fetcher {
            network,
            metrics: Arc::new(ExecutorMetrics::default()),
        };
        let expected_batches = HashMap::from_iter(vec![
            (batch1.digest(), batch1.clone()),
            (batch2.digest(), batch2.clone()),
        ]);
        let fetched_batches = fetcher
            .fetch_batches_from_worker(bulk_batches, test_pks(&[1, 2]))
            .await;
        assert_eq!(fetched_batches, expected_batches);
    }

    #[tokio::test]
    pub async fn test_fetcher_locally_with_remaining() {
        // Limit is set to two batches in test request_batches(). Request 3 batches
        // and ensure another request is sent to get the remaining batches.
        let mut network = TestSubscriberNetwork::new();
        let batch1 = Batch::new(vec![vec![1]]);
        let batch2 = Batch::new(vec![vec![2]]);
        let batch3 = Batch::new(vec![vec![3]]);
        let batches_to_fetch: HashMap<WorkerId, HashSet<BatchDigest>> = HashMap::from_iter(vec![(
            0,
            HashSet::from_iter(vec![batch1.digest(), batch2.digest(), batch3.digest()]),
        )]);
        network.put(&[0, 1, 2], batch1.clone());
        network.put(&[0, 2, 3], batch2.clone());
        network.put(&[0, 3, 4], batch3.clone());
        let fetcher = Fetcher {
            network,
            metrics: Arc::new(ExecutorMetrics::default()),
        };
        let expected_batches = HashMap::from_iter(vec![
            (batch1.digest(), batch1.clone()),
            (batch2.digest(), batch2.clone()),
            (batch3.digest(), batch3.clone()),
        ]);
        let fetched_batches = fetcher
            .fetch_batches_from_worker(batches_to_fetch, test_pks(&[1, 2, 3]))
            .await;
        assert_eq!(fetched_batches, expected_batches);
    }

    #[tokio::test]
    pub async fn test_fetcher_remote_with_remaining() {
        // Limit is set to two batches in test request_batches(). Request 3 batches
        // and ensure another request is sent to get the remaining batches.
        let mut network = TestSubscriberNetwork::new();
        let batch1 = Batch::new(vec![vec![1]]);
        let batch2 = Batch::new(vec![vec![2]]);
        let batch3 = Batch::new(vec![vec![3]]);
        let batches_to_fetch: HashMap<WorkerId, HashSet<BatchDigest>> = HashMap::from_iter(vec![(
            0,
            HashSet::from_iter(vec![batch1.digest(), batch2.digest(), batch3.digest()]),
        )]);
        network.put(&[3, 4], batch1.clone());
        network.put(&[2, 3], batch2.clone());
        network.put(&[2, 3, 4], batch3.clone());
        let fetcher = Fetcher {
            network,
            metrics: Arc::new(ExecutorMetrics::default()),
        };
        let expected_batches = HashMap::from_iter(vec![
            (batch1.digest(), batch1.clone()),
            (batch2.digest(), batch2.clone()),
            (batch3.digest(), batch3.clone()),
        ]);
        let fetched_batches = fetcher
            .fetch_batches_from_worker(batches_to_fetch, test_pks(&[2, 3, 4]))
            .await;
        assert_eq!(fetched_batches, expected_batches);
    }

    #[tokio::test]
    pub async fn test_fetcher_local_and_remote() {
        let mut network = TestSubscriberNetwork::new();
        let batch1 = Batch::new(vec![vec![1]]);
        let batch2 = Batch::new(vec![vec![2]]);
        let batch3 = Batch::new(vec![vec![3]]);
        let batches_to_fetch: HashMap<WorkerId, HashSet<BatchDigest>> = HashMap::from_iter(vec![(
            0,
            HashSet::from_iter(vec![batch1.digest(), batch2.digest(), batch3.digest()]),
        )]);
        network.put(&[0, 1, 2, 3], batch1.clone());
        network.put(&[2, 3, 4], batch2.clone());
        network.put(&[1, 4], batch3.clone());
        let fetcher = Fetcher {
            network,
            metrics: Arc::new(ExecutorMetrics::default()),
        };
        let expected_batches = HashMap::from_iter(vec![
            (batch1.digest(), batch1.clone()),
            (batch2.digest(), batch2.clone()),
            (batch3.digest(), batch3.clone()),
        ]);
        let fetched_batches = fetcher
            .fetch_batches_from_worker(batches_to_fetch, test_pks(&[1, 2, 3, 4]))
            .await;
        assert_eq!(fetched_batches, expected_batches);
    }

    struct TestSubscriberNetwork {
        data: HashMap<BatchDigest, HashMap<NetworkPublicKey, Batch>>,
        my: NetworkPublicKey,
    }

    impl TestSubscriberNetwork {
        pub fn new() -> Self {
            let my = test_pk(0);
            let data = Default::default();
            Self { data, my }
        }

        pub fn put(&mut self, keys: &[u8], batch: Batch) {
            let digest = batch.digest();
            let entry = self.data.entry(digest).or_default();
            for key in keys {
                let key = test_pk(*key);
                entry.insert(key, batch.clone());
            }
        }
    }

    #[async_trait]
    impl SubscriberNetwork for TestSubscriberNetwork {
        fn my_worker(&self, _worker_id: &WorkerId) -> NetworkPublicKey {
            self.my.clone()
        }

        fn workers_for_certificate(
            &self,
            certificate: &Certificate,
            _worker_id: &WorkerId,
        ) -> Vec<NetworkPublicKey> {
            let digest = certificate.header.payload.keys().next().unwrap();
            self.data.get(digest).unwrap().keys().cloned().collect()
        }

        async fn request_batches(
            &self,
            digests: Vec<BatchDigest>,
            worker: NetworkPublicKey,
            _timeout: Duration,
        ) -> anyhow::Result<RequestBatchesResponse> {
            let mut batches: Vec<Batch> = Vec::new();
            let mut remaining_batch_digests = Vec::new();
            // Use this to simulate server side response size limit in RequestBatches
            let limit = 2;
            for digest in digests {
                if let Some(batch) = self.data.get(&digest).unwrap().get(&worker) {
                    if batches.len() < limit {
                        batches.push(batch.clone());
                    } else {
                        remaining_batch_digests.push(digest);
                    }
                }
            }
            Ok(RequestBatchesResponse {
                batches,
                remaining_batch_digests,
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
