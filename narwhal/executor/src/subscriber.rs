// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{errors::SubscriberResult, metrics::ExecutorMetrics, ExecutionState};

use config::{Committee, WorkerCache, WorkerId};
use crypto::{NetworkPublicKey, PublicKey};

use futures::stream::FuturesOrdered;
use futures::FutureExt;
use futures::StreamExt;

use network::WorkerRpc;

use anyhow::bail;
use prometheus::IntGauge;
use std::collections::HashMap;
use std::collections::HashSet;
use std::{sync::Arc, time::Duration, vec};

use async_trait::async_trait;
use fastcrypto::hash::Hash;
use mysten_metrics::spawn_logged_monitored_task;
use rand::prelude::SliceRandom;
use rand::rngs::ThreadRng;
use tokio::time::Instant;
use tokio::{sync::oneshot, task::JoinHandle};
use tracing::{debug, error, warn};
use tracing::{info, instrument};
use types::{
    metered_channel, Batch, BatchDigest, Certificate, CommittedSubDag,
    ConditionalBroadcastReceiver, ConsensusOutput, Timestamp,
};

/// The `Subscriber` receives certificates sequenced by the consensus and waits until the
/// downloaded all the transactions references by the certificates; it then
/// forward the certificates to the Executor Core.
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
    name: PublicKey,
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
                name,
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
    name: PublicKey,
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
        name,
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
            let future = self.fetcher.fetch_payloads(message);
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
                    waiting.push_back(self.fetcher.fetch_payloads(sub_dag));
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
    /// Returns ordered vector of futures for downloading individual payloads for certificate
    /// Order of futures returned follows order of payloads in the certificate
    /// See fetch_payload for more details
    #[instrument(level = "debug", skip_all, fields(certificate = % deliver.leader.digest()))]
    async fn fetch_payloads(&self, deliver: CommittedSubDag) -> ConsensusOutput {
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

        // Get all batch digests for a bulk batch request
        for cert in &sub_dag.certificates {
            for (digest, (worker_id, _)) in cert.header.payload.iter() {
                let mut workers = self.network.workers_for_certificate(cert, worker_id);
                workers.shuffle(&mut ThreadRng::default());
                workers_for_certificates.extend(workers.clone());
                batch_digests.entry(*worker_id).or_default().insert(*digest);
            }
        }

        // send out bulk batch request and return map of digest to batch
        let fetched_batches = self
            .bulk_payload_fetch(
                batch_digests,
                workers_for_certificates.into_iter().collect(),
            )
            .await;

        // map all batches to their respective certificates and submit to
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
                    "Adding fetched batch {digest} (from certificate {}) to consensus output",
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

    /// Bulk fetches payload from network
    /// This future performs infinite retries and blocks until all batches are available
    /// As an optimization it tries to download from local worker first, but then fans out
    /// requests to remote worker if not found locally
    async fn bulk_payload_fetch(
        &self,
        batch_digests: HashMap<WorkerId, HashSet<BatchDigest>>,
        workers: Vec<NetworkPublicKey>,
    ) -> HashMap<BatchDigest, Batch> {
        // For each worker send bulk batch request
        let mut batches = HashMap::new();
        for (worker_id, digests) in batch_digests {
            // TODO - issue each worker id fetch as a separate task that we wait on.
            // not an issue now because we only have one worker per node
            let mut remaining_digests = digests.clone();

            // bulk fetch local first
            let local_batches = self
                .try_fetch_locally(digests.clone().into_iter().collect(), worker_id)
                .await;
            for (local_batch_digest, local_batch) in local_batches {
                if remaining_digests.remove(&local_batch_digest) {
                    batches.insert(local_batch_digest, local_batch);
                }
            }

            if remaining_digests.is_empty() {
                return batches;
            }

            // send remaining missing digests to remote workers for guaranteed
            // fetch
            let _timer = self.metrics.subscriber_remote_fetch_latency.start_timer();
            let mut stagger = Duration::from_secs(0);
            let mut futures = vec![];
            for worker in &workers {
                let future =
                    self.fetch_from_worker(stagger, worker.clone(), remaining_digests.clone());
                futures.push(future.boxed());
                // TODO: Make this a parameter, and also record workers / authorities that are down
                //       to request from them batches later.
                stagger += Duration::from_millis(200);
            }
            let (remote_batches, _, _) = futures::future::select_all(futures).await;

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

                    batches.insert(remote_batch_digest, remote_batch);
                }
            }

            if remaining_digests.is_empty() {
                return batches;
            } else {
                error!("Failed to fetch all required batches {remaining_digests:?} from workers");
            }
        }
        batches
    }

    #[instrument(level = "debug", skip_all, fields(digests = ? digests, worker_id = % worker_id))]
    async fn try_fetch_locally(
        &self,
        digests: Vec<BatchDigest>,
        worker_id: WorkerId,
    ) -> HashMap<BatchDigest, Batch> {
        let _timer = self.metrics.subscriber_local_fetch_latency.start_timer();
        let worker = self.network.my_worker(&worker_id);
        let payloads = self.network.request_batches(digests, worker).await;
        let mut found_batches: HashMap<BatchDigest, Batch> = HashMap::new();

        match payloads {
            Ok(batches) => {
                debug!("Payloads {:?} found locally", batches);
                for batch in batches {
                    match batch {
                        Some(batch) => {
                            self.metrics.subscriber_local_hit.inc();
                            found_batches.insert(batch.digest(), batch);
                        }
                        None => {
                            self.metrics.failed_local_batch_fetch.inc();
                            info!("A payload was not found locally");
                        }
                    }
                }
            }
            Err(err) => {
                self.metrics.failed_local_batch_fetch.inc();
                if err.to_string().contains("Timeout") {
                    self.metrics.local_batch_fetch_timeout.inc();
                }
                warn!("Error communicating with own worker: {}", err)
            }
        }

        found_batches
    }

    /// This future performs fetch from given worker
    /// This future performs infinite retries with exponential backoff
    /// You can specify stagger_delay before request is issued
    #[instrument(level = "debug", skip_all, fields(stagger_delay = ? stagger_delay, worker = % worker, digests = ? digests))]
    async fn fetch_from_worker(
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
        let mut remaining_digests = digests.clone();
        let mut batches: HashMap<BatchDigest, Batch> = HashMap::new();
        loop {
            attempt += 1;
            let deadline = Instant::now() + timeout;
            let request_batch_guard =
                PendingGuard::make_inc(&self.metrics.pending_remote_request_batch);
            let response = tokio::time::timeout_at(
                deadline,
                self.safe_request_batches(remaining_digests.clone(), worker.clone()),
            )
            .await;
            drop(request_batch_guard);
            match response {
                Ok(Ok(remote_batches)) => {
                    self.metrics.successful_remote_batch_fetch.inc();
                    for (remote_batch_digest, remote_batch) in remote_batches {
                        if remaining_digests.remove(&remote_batch_digest) {
                            batches.insert(remote_batch_digest, remote_batch);
                        }
                    }
                }
                Ok(Err(err)) => {
                    self.metrics.failed_remote_batch_fetch.inc();
                    debug!(
                        "Error retrieving payloads {:?} from {}: {}",
                        digests, worker, err
                    )
                }
                Err(_elapsed) => {
                    self.metrics.remote_batch_fetch_timeout.inc();
                    warn!(
                        "Timeout retrieving payloads {:?} from {} attempt {}",
                        digests, worker, attempt
                    )
                }
            }

            if remaining_digests.is_empty() {
                return batches;
            }

            timeout += timeout / 2;
            timeout = std::cmp::min(max_timeout, timeout);
            // Since the call might have returned before timeout, we wait until originally planned deadline
            tokio::time::sleep_until(deadline).await;
        }
    }

    /// Issue request_batches RPC and verifies response integrity
    async fn safe_request_batches(
        &self,
        digests: HashSet<BatchDigest>,
        worker: NetworkPublicKey,
    ) -> anyhow::Result<HashMap<BatchDigest, Batch>> {
        let payloads = self
            .network
            .request_batches(digests.clone().into_iter().collect(), worker.clone())
            .await?;
        let mut batches = HashMap::new();
        for payload in payloads {
            if let Some(payload) = payload {
                let payload_digest = payload.digest();
                if !digests.contains(&payload_digest) {
                    bail!("[Protocol violation] Worker {} returned batch with mismatch digest {} requested {:?}", worker, payload_digest, digests );
                } else {
                    batches.insert(payload_digest, payload);
                }
            }
        }
        Ok(batches)
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
    fn my_worker(&self, worker_id: &WorkerId) -> NetworkPublicKey;
    fn workers_for_certificate(
        &self,
        certificate: &Certificate,
        worker_id: &WorkerId,
    ) -> Vec<NetworkPublicKey>;

    async fn request_batches(
        &self,
        digests: Vec<BatchDigest>,
        worker: NetworkPublicKey,
    ) -> anyhow::Result<Vec<Option<Batch>>>;
}

struct SubscriberNetworkImpl {
    name: PublicKey,
    network: anemo::Network,
    worker_cache: WorkerCache,
    committee: Committee,
}

#[async_trait]
impl SubscriberNetwork for SubscriberNetworkImpl {
    fn my_worker(&self, worker_id: &WorkerId) -> NetworkPublicKey {
        self.worker_cache
            .worker(&self.name, worker_id)
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
        digests: Vec<BatchDigest>,
        worker: NetworkPublicKey,
    ) -> anyhow::Result<Vec<Option<Batch>>> {
        self.network.request_batches(worker, digests).await
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
            .bulk_payload_fetch(bulk_batches, test_pks(&[1, 2]))
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
        ) -> anyhow::Result<Vec<Option<Batch>>> {
            let mut result: Vec<Option<Batch>> = Vec::new();
            for digest in digests {
                result.push(self.data.get(&digest).unwrap().get(&worker).cloned());
            }
            Ok(result)
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
