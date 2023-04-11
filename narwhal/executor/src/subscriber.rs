// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{errors::SubscriberResult, metrics::ExecutorMetrics, ExecutionState};

use config::{AuthorityIdentifier, Committee, WorkerCache, WorkerId};
use crypto::NetworkPublicKey;

use futures::stream::FuturesOrdered;
use futures::StreamExt;

use network::PrimaryToWorkerClient;

use network::client::NetworkClient;
use std::collections::HashMap;
use std::collections::HashSet;
use std::{sync::Arc, time::Duration, vec};
use types::FetchBatchesRequest;

use fastcrypto::hash::Hash;
use mysten_metrics::spawn_logged_monitored_task;
use tokio::task::JoinHandle;
use tracing::{debug, error, info};
use types::{
    metered_channel, Batch, BatchAPI, BatchDigest, Certificate, CertificateAPI, CommittedSubDag,
<<<<<<< HEAD
    ConditionalBroadcastReceiver, ConsensusOutput, HeaderAPI, Timestamp,
=======
    ConditionalBroadcastReceiver, ConsensusOutput, HeaderAPI, RequestBatchesResponse, Timestamp,
>>>>>>> fork/testnet
};

/// The `Subscriber` receives certificates sequenced by the consensus and waits until the
/// downloaded all the transactions references by the certificates; it then
/// forward the certificates to the Executor.
pub struct Subscriber {
    /// Receiver for shutdown
    rx_shutdown: ConditionalBroadcastReceiver,
    /// A channel to receive sequenced consensus messages.
    rx_sequence: metered_channel::Receiver<CommittedSubDag>,
    /// Inner state.
    inner: Arc<Inner>,
}

struct Inner {
    authority_id: AuthorityIdentifier,
    worker_cache: WorkerCache,
    committee: Committee,
    client: NetworkClient,
    metrics: Arc<ExecutorMetrics>,
}

pub fn spawn_subscriber<State: ExecutionState + Send + Sync + 'static>(
    authority_id: AuthorityIdentifier,
    worker_cache: WorkerCache,
    committee: Committee,
    client: NetworkClient,
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
                worker_cache,
                committee,
                rx_shutdown_subscriber,
                rx_sequence,
                client,
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
    mut rx_notify: metered_channel::Receiver<ConsensusOutput>,
    mut rx_shutdown: ConditionalBroadcastReceiver,
) {
    loop {
        tokio::select! {
            Some(message) = rx_notify.recv() => {
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
    worker_cache: WorkerCache,
    committee: Committee,
    rx_shutdown: ConditionalBroadcastReceiver,
    rx_sequence: metered_channel::Receiver<CommittedSubDag>,
    client: NetworkClient,
    metrics: Arc<ExecutorMetrics>,
    restored_consensus_output: Vec<CommittedSubDag>,
    tx_notifier: metered_channel::Sender<ConsensusOutput>,
) {
    info!("Starting subscriber");
    let subscriber = Subscriber {
        rx_shutdown,
        rx_sequence,
        inner: Arc::new(Inner {
            authority_id,
            committee,
            worker_cache,
            client,
            metrics,
        }),
    };
    subscriber
        .run(restored_consensus_output, tx_notifier)
        .await
        .expect("Failed to run subscriber")
}

impl Subscriber {
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
            let future = Self::fetch_batches(self.inner.clone(), message);
            waiting.push_back(future);

            self.inner
                .metrics
                .subscriber_recovered_certificates_count
                .inc();
        }

        // Listen to sequenced consensus message and process them.
        loop {
            tokio::select! {
                // Receive the ordered sequence of consensus messages from a consensus node.
                Some(sub_dag) = self.rx_sequence.recv(), if waiting.len() < Self::MAX_PENDING_PAYLOADS => {
                    // We can schedule more then MAX_PENDING_PAYLOADS payloads but
                    // don't process more consensus messages when more
                    // then MAX_PENDING_PAYLOADS is pending
                    waiting.push_back(Self::fetch_batches(self.inner.clone(), sub_dag));
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

            self.inner
                .metrics
                .waiting_elements_subscriber
                .set(waiting.len() as i64);
        }
    }

    /// Returns ordered vector of futures for downloading batches for certificates
    /// Order of futures returned follows order of batches in the certificates.
    /// See BatchFetcher for more details.
    async fn fetch_batches(inner: Arc<Inner>, deliver: CommittedSubDag) -> ConsensusOutput {
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

        let mut batch_digests_and_workers: HashMap<
            NetworkPublicKey,
            (HashSet<BatchDigest>, HashSet<NetworkPublicKey>),
        > = HashMap::new();

        for cert in &sub_dag.certificates {
            for (digest, (worker_id, _)) in cert.header().payload().iter() {
<<<<<<< HEAD
                let own_worker_name = inner
                    .worker_cache
                    .worker(
                        inner
                            .committee
                            .authority(&inner.authority_id)
                            .unwrap()
                            .protocol_key(),
                        worker_id,
                    )
                    .unwrap_or_else(|_| panic!("worker_id {worker_id} is not in the worker cache"))
                    .name;
                let workers = Self::workers_for_certificate(&inner, cert, worker_id);
                let (batch_set, worker_set) = batch_digests_and_workers
                    .entry(own_worker_name)
                    .or_default();
                batch_set.insert(*digest);
                worker_set.extend(workers);
=======
                let workers = self.network.workers_for_certificate(cert, worker_id);
                batch_digests_and_workers
                    .entry(*worker_id)
                    .and_modify(|(batch_set, worker_set)| {
                        worker_set.extend(workers.clone());
                        batch_set.insert(*digest);
                    })
                    .or_insert_with(|| {
                        let mut batch_set = HashSet::new();
                        let mut worker_set = HashSet::new();
                        worker_set.extend(workers);
                        batch_set.insert(*digest);
                        (batch_set, worker_set)
                    });
>>>>>>> fork/testnet
            }
        }

        let fetched_batches_timer = inner
            .metrics
            .batch_fetch_for_committed_subdag_total_latency
            .start_timer();
        inner
            .metrics
            .committed_subdag_batch_count
            .observe(num_batches as f64);
        let fetched_batches =
            Self::fetch_batches_from_workers(&inner, batch_digests_and_workers).await;
        drop(fetched_batches_timer);

        // Map all fetched batches to their respective certificates and submit as
        // consensus output
        for cert in &sub_dag.certificates {
            let mut output_batches = Vec::with_capacity(cert.header().payload().len());
            let output_cert = cert.clone();

            inner
                .metrics
                .subscriber_current_round
                .set(cert.round() as i64);

            inner
                .metrics
                .subscriber_certificate_latency
                .observe(cert.metadata().created_at.elapsed().as_secs_f64());

            for (digest, (_, _)) in cert.header().payload().iter() {
<<<<<<< HEAD
                inner.metrics.subscriber_processed_batches.inc();
=======
                self.metrics.subscriber_processed_batches.inc();
>>>>>>> fork/testnet
                let batch = fetched_batches
                    .get(digest)
                    .expect("[Protocol violation] Batch not found in fetched batches from workers of certificate signers");

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

<<<<<<< HEAD
=======
    /// Bulk fetches payload from workers
    /// This future performs infinite retries and blocks until all batches are available
    /// As an optimization it tries to download from local worker first, but then fans out
    /// requests to remote worker if not found locally
    async fn fetch_batches_from_worker(
        &self,
        batch_digests_and_workers: HashMap<
            WorkerId,
            (HashSet<BatchDigest>, HashSet<NetworkPublicKey>),
        >,
    ) -> HashMap<BatchDigest, Batch> {
        let mut fetched_batches = HashMap::new();

        for (worker_id, (digests, workers)) in batch_digests_and_workers {
            debug!(
                "Attempting to fetch {} digests from {} worker_{worker_id}'s",
                digests.len(),
                workers.len()
            );
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
                        local_batch.metadata().created_at.elapsed().as_secs_f64();
                    self.metrics
                        .batch_execution_latency
                        .observe(batch_fetch_duration);
                    trace!(
                        "Batch {local_batch_digest:?} took {batch_fetch_duration} seconds to be fetched for execution since creation",
                    );
                    fetched_batches.insert(local_batch_digest, local_batch);
                }
            }

            if remaining_digests.is_empty() {
                continue;
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
                            remote_batch.metadata().created_at.elapsed().as_secs_f64();
                        self.metrics
                            .batch_execution_latency
                            .observe(batch_fetch_duration);
                        trace!(
                            "Batch {remote_batch_digest:?} took {batch_fetch_duration} seconds to be fetched for execution since creation"
                        );

                        fetched_batches.insert(remote_batch_digest, remote_batch);
                    }
                }

                if remaining_digests.is_empty() {
                    break;
                }
            }
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
            debug!("Local attempt to fetch {} digests", digests_to_fetch.len());
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
                        is_size_limit_reached,
                    } = request_batches_response;
                    debug!("Locally found {} batches", batches.len());
                    for local_batch in batches {
                        self.metrics
                            .subscriber_batch_fetch
                            .with_label_values(&["local", "success"])
                            .inc();
                        digests_to_fetch.remove(&local_batch.digest());
                        fetched_batches.insert(local_batch.digest(), local_batch);
                    }

                    if !is_size_limit_reached {
                        break;
                    }
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
                "Remote attempt #{attempt} to fetch {} digests from {worker}",
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
    fn my_worker(&self, worker_id: &WorkerId) -> NetworkPublicKey;
>>>>>>> fork/testnet
    fn workers_for_certificate(
        inner: &Inner,
        certificate: &Certificate,
        worker_id: &WorkerId,
    ) -> Vec<NetworkPublicKey> {
        // Can include own authority and worker, but worker will always check local storage when
        // fetching paylods.
        let authorities = certificate.signed_authorities(&inner.committee);
        authorities
            .into_iter()
            .filter_map(|authority| {
                let worker = inner.worker_cache.worker(&authority, worker_id);
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

    async fn fetch_batches_from_workers(
        inner: &Inner,
        batch_digests_and_workers: HashMap<
            NetworkPublicKey,
            (HashSet<BatchDigest>, HashSet<NetworkPublicKey>),
<<<<<<< HEAD
        >,
    ) -> HashMap<BatchDigest, Batch> {
        let mut fetched_batches = HashMap::new();
=======
        > = HashMap::from_iter(vec![(
            0,
            (
                HashSet::from_iter(expected_batches.clone().into_keys()),
                HashSet::from_iter(test_pks(&[1, 2, 3])),
            ),
        )]);
        let fetcher = Fetcher {
            network,
            metrics: Arc::new(ExecutorMetrics::default()),
        };
        let fetched_batches = fetcher
            .fetch_batches_from_worker(batch_digests_and_workers)
            .await;
        assert_eq!(fetched_batches, expected_batches);
    }

    struct TestSubscriberNetwork {
        data: HashMap<WorkerId, HashMap<BatchDigest, HashMap<NetworkPublicKey, Batch>>>,
        worker_cache: HashMap<NetworkPublicKey, WorkerId>,
        my: HashMap<WorkerId, NetworkPublicKey>,
    }

    impl TestSubscriberNetwork {
        pub fn new(workers: u8) -> Self {
            let mut my = HashMap::new();
            let mut worker_cache = HashMap::new();
            for i in 0..workers {
                let pk = test_pk(i);
                // Not ideal but keys 0..workers are reserved for local worker keys.
                my.insert(i as u32, pk.clone());
                worker_cache.insert(pk, i as u32);
            }
            let data = Default::default();
            Self {
                data,
                worker_cache,
                my,
            }
        }

        pub fn put(&mut self, worker_id: WorkerId, keys: &[u8], batch: Batch) {
            let digest = batch.digest();
            let entry = self
                .data
                .entry(worker_id)
                .or_default()
                .entry(digest)
                .or_default();
            for key in keys {
                let key = test_pk(*key);
                entry.insert(key.clone(), batch.clone());
                if let Some(existing_worker_id) = self.worker_cache.insert(key, worker_id) {
                    assert!(
                        existing_worker_id == worker_id,
                        "Worker key should be unique across worker ids"
                    );
                }
            }
        }
    }

    #[async_trait]
    impl SubscriberNetwork for TestSubscriberNetwork {
        fn my_worker(&self, worker_id: &WorkerId) -> NetworkPublicKey {
            self.my.get(worker_id).expect("invalid worker id").clone()
        }

        fn workers_for_certificate(
            &self,
            certificate: &Certificate,
            worker_id: &WorkerId,
        ) -> Vec<NetworkPublicKey> {
            let payload = certificate.header().payload().to_owned();
            let digest = payload.keys().next().unwrap();
            self.data
                .get(worker_id)
                .unwrap()
                .get(digest)
                .unwrap()
                .keys()
                .cloned()
                .collect()
        }
>>>>>>> fork/testnet

        for (worker_name, (digests, known_workers)) in batch_digests_and_workers {
            debug!(
                "Attempting to fetch {} digests from {} known workers, {worker_name}'s",
                digests.len(),
                known_workers.len()
            );
            // TODO: Can further parallelize this by worker if necessary. Maybe move the logic
            // to NetworkClient.
            // Only have one worker for now so will leave this for a future
            // optimization.
            let request = FetchBatchesRequest {
                digests,
                known_workers,
            };
            let batches = loop {
                match inner
                    .client
                    .fetch_batches(worker_name.clone(), request.clone())
                    .await
                {
                    Ok(resp) => break resp.batches,
                    Err(e) => {
                        error!("Failed to fetch batches from worker {worker_name}: {e:?}");
                        // Loop forever on failure. During shutdown, this should get cancelled.
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                }
            };
            for (digest, batch) in batches {
                let batch_fetch_duration = batch.metadata().created_at.elapsed().as_secs_f64();
                inner
                    .metrics
                    .batch_execution_latency
                    .observe(batch_fetch_duration);
                fetched_batches.insert(digest, batch);
            }
        }

        fetched_batches
    }
}

// TODO: add a unit test
