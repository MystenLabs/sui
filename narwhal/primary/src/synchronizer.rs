// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use anemo::{rpc::Status, Network, Request, Response};
use config::{AuthorityIdentifier, Committee, Epoch, WorkerCache};
use consensus::consensus::ConsensusRound;
use consensus::dag::Dag;
use crypto::NetworkPublicKey;
use fastcrypto::hash::Hash as _;
use network::{anemo_ext::NetworkExt, RetryConfig};
use std::{collections::HashMap, sync::Arc};
use storage::{CertificateStore, PayloadToken};
use store::Store;
use tokio::sync::{mpsc, watch};
use tracing::debug;
use types::{
    ensure,
    error::{DagError, DagResult},
    BatchDigest, Certificate, CertificateDigest, Header, PrimaryToWorkerClient, Round,
    WorkerSynchronizeMessage,
};

use crate::{aggregators::CertificatesAggregator, metrics::PrimaryMetrics, CHANNEL_CAPACITY};

#[cfg(test)]
#[path = "tests/synchronizer_tests.rs"]
pub mod synchronizer_tests;

/// Only try to accept or suspend a certificate, if it is within this limit above the
/// locally highest processed round.
const NEW_CERTIFICATE_ROUND_LIMIT: Round = 100;

struct Inner {
    /// The id of this primary.
    authority_id: AuthorityIdentifier,
    /// Committee of the current epoch.
    committee: Committee,
    /// The worker information cache.
    worker_cache: WorkerCache,
    /// The depth of the garbage collector.
    gc_depth: Round,
    /// Highest round that has been GC'ed.
    gc_round: AtomicU64,
    /// Highest round of certificate accepted into the certificate store.
    highest_processed_round: AtomicU64,
    /// Highest round of verfied certificate that has been received.
    highest_received_round: AtomicU64,
    /// Client for fetching payloads.
    client: NetworkClient,
    /// The persistent storage tables.
    certificate_store: CertificateStore,
    payload_store: Store<(BatchDigest, WorkerId), PayloadToken>,
    /// Send commands to the `CertificateFetcher`.
    tx_certificate_fetcher: mpsc::Sender<Certificate>,
    /// Get a signal when the round changes.
    rx_consensus_round_updates: watch::Receiver<Round>,
    /// The genesis and its digests.
    genesis: HashMap<CertificateDigest, Certificate>,
    /// The dag used for the external consensus
    dag: Option<Arc<Dag>>,
    /// Contains Synchronizer specific metrics among other Primary metrics.
    metrics: Arc<PrimaryMetrics>,
    /// Background tasks synchronizing worker batches for processed certificates.
    batch_tasks: Mutex<JoinSet<DagResult<()>>>,
    /// Background tasks broadcasting newly formed certificates.
    certificate_senders: Mutex<JoinSet<()>>,
    /// Aggregates certificates to use as parents for new headers.
    certificates_aggregators: Mutex<BTreeMap<Round, Box<CertificatesAggregator>>>,
    /// State for tracking suspended certificates and when they can be accepted.
    state: tokio::sync::Mutex<State>,
}

impl Inner {
    async fn append_certificate_in_aggregator(&self, certificate: Certificate) -> DagResult<()> {
        // Check if we have enough certificates to enter a new dag round and propose a header.
        let Some(parents) = self
            .certificates_aggregators
            .lock()
            .entry(certificate.round())
            .or_insert_with(|| Box::new(CertificatesAggregator::new()))
            .append(certificate.clone(), &self.committee) else {
                return Ok(());
            };
        // Send it to the `Proposer`.
        self.tx_parents
            .send((parents, certificate.round(), certificate.epoch()))
            .await
            .map_err(|_| DagError::ShuttingDown)
    }

    async fn accept_suspended_certificate(
        &self,
        lock: &MutexGuard<'_, State>,
        suspended: SuspendedCertificate,
    ) -> DagResult<()> {
        self.accept_certificate_internal(lock, suspended.certificate.clone())
            .await?;
        // Notify waiters that the certificate is no longer suspended.
        // Must be after certificate acceptance.
        // It is ok if there is no longer any waiter.
        suspended
            .notify
            .notify()
            .expect("Suspended certificate should be notified once.");
        Ok(())
    }

    // State lock must be held when calling this function.
    async fn accept_certificate_internal(
        &self,
        _lock: &MutexGuard<'_, State>,
        certificate: Certificate,
    ) -> DagResult<()> {
        let digest = certificate.digest();

        // TODO: remove this validation later to reduce rocksdb access.
        if certificate.round() > self.gc_round.load(Ordering::Acquire) + 1 {
            for digest in certificate.header().parents() {
                if !self.certificate_store.contains(digest).unwrap() {
                    panic!("Parent {digest:?} not found for {certificate:?}!");
                }
            }
        }

        // Store the certificate and make it available as parent to other certificates.
        self.certificate_store
            .write(certificate.clone())
            .expect("Writing certificate to storage cannot fail!");

        // From this point, the certificate must be sent to consensus or Narwhal needs to shutdown,
        // to avoid inconsistencies in certificate store and consensus dag.

        // Update metrics for accepted certificates.
        let highest_processed_round = self
            .highest_processed_round
            .fetch_max(certificate.round(), Ordering::AcqRel)
            .max(certificate.round());
        let certificate_source = if self.authority_id.eq(&certificate.origin()) {
            "own"
        } else {
            "other"
        };
        self.metrics
            .highest_processed_round
            .with_label_values(&[certificate_source])
            .set(highest_processed_round as i64);
        self.metrics
            .certificates_processed
            .with_label_values(&[certificate_source])
            .inc();

        // Append the certificate to the aggregator of the
        // corresponding round.
        if let Err(e) = self
            .append_certificate_in_aggregator(certificate.clone())
            .await
        {
            warn!(
                "Failed to aggregate certificate {} for header: {}",
                digest, e
            );
            return Err(DagError::ShuttingDown);
        }

        // Send the accepted certificate to the consensus layer.
        if let Err(e) = self.tx_new_certificates.send(certificate).await {
            warn!(
                "Failed to deliver certificate {} to the consensus: {}",
                digest, e
            );
            return Err(DagError::ShuttingDown);
        }

        Ok(())
    }

    /// Tries to get all missing parents of the certificate. If there is any, sends the
    /// certificate to `CertificateFetcher` which will trigger range fetching of missing
    /// certificates.
    async fn get_missing_parents(
        &self,
        certificate: &Certificate,
    ) -> DagResult<Vec<CertificateDigest>> {
        let mut result = Vec::new();
        if certificate.round() == 1 {
            for digest in certificate.header().parents() {
                if !self.genesis.contains_key(digest) {
                    return Err(DagError::InvalidGenesisParent(*digest));
                }
            }
            return Ok(result);
        }

        for digest in certificate.header().parents() {
            if !self.has_processed_certificate(*digest).await? {
                result.push(*digest);
            }
        }
        if !result.is_empty() {
            self.tx_certificate_fetcher
                .send(certificate.clone())
                .await
                .map_err(|_| DagError::ShuttingDown)?;
        }
        Ok(result)
    }

    /// This method answers to the question of whether the certificate with the
    /// provided digest has ever been successfully processed (seen) by this
    /// node. Depending on the mode of running the node (internal Vs external
    /// consensus) either the dag will be used to confirm that or the
    /// certificate_store.
    async fn has_processed_certificate(&self, digest: CertificateDigest) -> DagResult<bool> {
        if let Some(dag) = &self.dag {
            return Ok(dag.has_ever_contained(digest).await);
        }
        Ok(self.certificate_store.contains(&digest)?)
    }
}

/// `Synchronizer` helps this primary and other peers stay in sync with each other,
/// w.r.t. certificates and the DAG. Specifically, it is responsible for
/// - Validating and accepting certificates received from peers.
/// - Triggering fetching for certificates and batches.
/// - Broadcasting created certificates.
/// `Synchronizer` contains most of the certificate processing logic in Narwhal.
#[derive(Clone)]
pub struct Synchronizer {
    /// Internal data that are thread safe.
    inner: Arc<Inner>,
}

impl Synchronizer {
    pub fn new(
        authority_id: AuthorityIdentifier,
        committee: Committee,
        worker_cache: WorkerCache,
        gc_depth: Round,
        client: NetworkClient,
        certificate_store: CertificateStore,
        payload_store: Store<(BatchDigest, WorkerId), PayloadToken>,
        tx_certificate_fetcher: mpsc::Sender<Certificate>,
        rx_consensus_round_updates: watch::Receiver<Round>,
        dag: Option<Arc<Dag>>,
        metrics: Arc<PrimaryMetrics>,
    ) -> Self {
        let committee: &Committee = &committee;
        let genesis = Self::make_genesis(committee);
        let highest_processed_round = certificate_store.highest_round_number();
        let highest_created_certificate = certificate_store.last_round(authority_id).unwrap();
        let gc_round = rx_consensus_round_updates.borrow().gc_round;
        let (tx_own_certificate_broadcast, _rx_own_certificate_broadcast) =
            broadcast::channel(CHANNEL_CAPACITY);
        let (tx_certificate_acceptor, mut rx_certificate_acceptor) =
            mpsc::channel(CHANNEL_CAPACITY);
        let inner = Arc::new(Inner {
            authority_id,
            committee: committee.clone(),
            worker_cache,
            gc_depth,
            gc_round: AtomicU64::new(gc_round),
            highest_processed_round: AtomicU64::new(highest_processed_round),
            highest_received_round: AtomicU64::new(0),
            client,
            certificate_store,
            payload_store,
            tx_certificate_fetcher,
            tx_certificate_acceptor,
            tx_new_certificates,
            tx_parents,
            tx_own_certificate_broadcast: tx_own_certificate_broadcast.clone(),
            rx_consensus_round_updates: rx_consensus_round_updates.clone(),
            genesis,
            dag,
            metrics,
            batch_tasks: Mutex::new(JoinSet::new()),
            certificate_senders: Mutex::new(JoinSet::new()),
            certificates_aggregators: Mutex::new(BTreeMap::new()),
            state: tokio::sync::Mutex::new(State::default()),
        });

        // Start a task to recover parent certificates for proposer.
        let inner_proposer = inner.clone();
        spawn_monitored_task!(async move {
            let last_round_certificates = inner_proposer
                .certificate_store
                .last_two_rounds_certs()
                .expect("Failed recovering certificates in primary core");
            for certificate in last_round_certificates {
                if let Err(e) = inner_proposer
                    .append_certificate_in_aggregator(certificate)
                    .await
                {
                    debug!("Failed to recover certificate, assuming Narwhal is shutting down. {e}");
                    return;
                }
            }
        });

        // Start a task to update gc_round and gc in-memory data.
        let weak_inner = Arc::downgrade(&inner);
        spawn_monitored_task!(async move {
            let mut rx_consensus_round_updates = rx_consensus_round_updates.clone();
            loop {
                let result = rx_consensus_round_updates.changed().await;
                if result.is_err() {
                    debug!("Synchronizer is shutting down.");
                    return;
                }
                let gc_round = rx_consensus_round_updates.borrow().gc_round;
                let Some(inner) = weak_inner.upgrade() else {
                    debug!("Synchronizer is shutting down.");
                    return;
                };
                // this is the only task updating gc_round
                inner.gc_round.store(gc_round, Ordering::Release);
                inner
                    .certificates_aggregators
                    .lock()
                    .retain(|k, _| k > &gc_round);
                // Accept certificates at gc round + 1, if there is any.
                let mut state = inner.state.lock().await;
                for suspended_cert in state.run_gc(gc_round) {
                    let suspended_certs = state.accept_children(
                        suspended_cert.certificate.round(),
                        suspended_cert.certificate.digest(),
                    );
                    // Iteration must be in causal order.
                    for suspended in iter::once(suspended_cert).chain(suspended_certs.into_iter()) {
                        match inner.accept_suspended_certificate(&state, suspended).await {
                            Ok(()) => {}
                            Err(DagError::ShuttingDown) => return,
                            Err(e) => {
                                panic!("Unexpected error accepting certificate during GC! {e}")
                            }
                        }
                    }
                }
            }
        });

        // Start a task to accept certificates. See comment above `process_certificate_with_lock()`
        // for why this task is needed.
        let weak_inner = Arc::downgrade(&inner);
        spawn_monitored_task!(async move {
            loop {
                let Some((certificate, result_sender, early_suspend)) = rx_certificate_acceptor.recv().await else {
                    debug!("Synchronizer is shutting down.");
                    return;
                };
                let Some(inner) = weak_inner.upgrade() else {
                    debug!("Synchronizer is shutting down.");
                    return;
                };
                // Ignore error if receiver has been dropped.
                let _ = result_sender.send(
                    Self::process_certificate_with_lock(&inner, certificate, early_suspend).await,
                );
            }
        });

        // Start tasks to broadcast created certificates.
        let inner_senders = inner.clone();
        spawn_monitored_task!(async move {
            let Ok(network) = rx_synchronizer_network.await else {
                error!("Failed to receive Network!");
                return;
            };
            let mut senders = inner_senders.certificate_senders.lock();
            for (name, _, network_key) in inner_senders
                .committee
                .others_primaries_by_id(inner_senders.authority_id)
                .into_iter()
            {
                senders.spawn(Self::push_certificates(
                    network.clone(),
                    name,
                    network_key,
                    tx_own_certificate_broadcast.subscribe(),
                ));
            }
            if let Some(cert) = highest_created_certificate {
                // Error can be ignored.
                if tx_own_certificate_broadcast.send(cert).is_err() {
                    error!("Failed to populate initial certificate to send to peers!");
                }
            }
        });

        Self { inner }
    }

    /// Validates the certificate and accepts it into the DAG, if the certificate can be verified
    /// and has all parents in the certificate store. Otherwise an error is returned.
    /// If the certificate has missing parents and cannot be accepted immediately, the error would
    /// contain a value that can be awaited on, for signaling when the certificate is accepted.
    pub async fn try_accept_certificate(&self, certificate: Certificate) -> DagResult<()> {
        self.process_certificate_internal(certificate, true, true)
            .await
    }

    /// Tries to accept a certificate from certificate fetcher.
    /// Fetched certificates are already sanitized, so it is unnecessary to duplicate the work.
    /// Also, this method always checks parents of fetched certificates and uses the result to
    /// validate the suspended certificates state, instead of relying on suspended certificates to
    /// potentially return early. This helps to verify consistency, and has little extra cost
    /// because fetched certificates usually are not suspended.
    pub async fn try_accept_fetched_certificate(&self, certificate: Certificate) -> DagResult<()> {
        self.process_certificate_internal(certificate, false, false)
            .await
    }

    /// Validates the certificate and accepts it into the DAG, if the certificate can be verified
    /// and has all parents in the certificate store.
    /// If the certificate has missing parents, wait until all parents are available to accept the
    /// certificate.
    /// Otherwise returns an error.
    pub async fn wait_to_accept_certificate(&self, certificate: Certificate) -> DagResult<()> {
        match self
            .process_certificate_internal(certificate, true, true)
            .await
        {
            Err(DagError::Suspended(notify)) => {
                notify.wait().await;
                Ok(())
            }
            result => result,
        }
    }

    /// Accepts a certificate produced by this primary. This is not expected to fail unless
    /// the primary is shutting down.
    pub async fn accept_own_certificate(&self, certificate: Certificate) -> DagResult<()> {
        // Process the new certificate.
        match self
            .process_certificate_internal(certificate.clone(), false, false)
            .await
        {
            Ok(()) => Ok(()),
            result @ Err(DagError::ShuttingDown) => result,
            Err(e) => panic!("Failed to process locally-created certificate: {e}"),
        }?;

        // Broadcast the certificate.
        if self
            .inner
            .tx_own_certificate_broadcast
            .send(certificate.clone())
            .is_err()
        {
            return Err(DagError::ShuttingDown);
        }

        // Update metrics.
        let round = certificate.round();
        let header_to_certificate_duration = Duration::from_millis(
            certificate.metadata().created_at - *certificate.header().created_at(),
        )
        .as_secs_f64();
        self.inner
            .metrics
            .certificate_created_round
            .set(round as i64);
        self.inner.metrics.certificates_created.inc();
        self.inner
            .metrics
            .header_to_certificate_latency
            .observe(header_to_certificate_duration);

        // NOTE: This log entry is used to compute performance.
        debug!(
            "Header {:?} at round {} with {} batches, took {} seconds to be materialized to a certificate {:?}",
            certificate.header().digest(),
            certificate.header().round(),
            certificate.header().payload().len(),
            header_to_certificate_duration,
            certificate.digest()
        );

        Ok(())
    }

    fn make_genesis(committee: &Committee) -> HashMap<CertificateDigest, Certificate> {
        Certificate::genesis(committee)
            .into_iter()
            .map(|x| (x.digest(), x))
            .collect()
    }

    /// Checks if the certificate is valid and can potentially be accepted into the DAG.
    // TODO: produce a different type after sanitize, e.g. VerifiedCertificate.
    pub fn sanitize_certificate(&self, certificate: &Certificate) -> DagResult<()> {
        ensure!(
            self.inner.committee.epoch() == certificate.epoch(),
            DagError::InvalidEpoch {
                expected: self.inner.committee.epoch(),
                received: certificate.epoch()
            }
        );
        // Ok to drop old certificate, because it will never be included into the consensus dag.
        let gc_round = self.inner.gc_round.load(Ordering::Acquire);
        ensure!(
            gc_round < certificate.round(),
            DagError::TooOld(certificate.digest().into(), certificate.round(), gc_round)
        );
        // Verify the certificate (and the embedded header).
        certificate
            .verify(&self.inner.committee, &self.inner.worker_cache)
            .map_err(DagError::from)
    }

    async fn process_certificate_internal(
        &self,
        certificate: Certificate,
        sanitize: bool,
        early_suspend: bool,
    ) -> DagResult<()> {
        let digest = certificate.digest();
        if self.inner.certificate_store.contains(&digest)? {
            trace!("Certificate {digest:?} has already been processed. Skip processing.");
            self.inner.metrics.duplicate_certificates_processed.inc();
            return Ok(());
        }
        // Ensure parents are checked if !early_suspend.
        // See comments above `try_accept_fetched_certificate()` for details.
        if early_suspend {
            if let Some(notify) = self.inner.state.lock().await.check_suspended(&digest) {
                trace!("Certificate {digest:?} is still suspended. Skip processing.");
                self.inner
                    .metrics
                    .certificates_suspended
                    .with_label_values(&["dedup"])
                    .inc();
                return Err(DagError::Suspended(notify));
            }
        }
        if sanitize {
            self.sanitize_certificate(&certificate)?;
        }

        debug!(
            "Processing certificate {:?} round:{:?}",
            certificate,
            certificate.round()
        );

        let certificate_source = if self.inner.authority_id.eq(&certificate.origin()) {
            "own"
        } else {
            "other"
        };
        let highest_received_round = self
            .inner
            .highest_received_round
            .fetch_max(certificate.round(), Ordering::AcqRel)
            .max(certificate.round());
        self.inner
            .metrics
            .highest_received_round
            .with_label_values(&[certificate_source])
            .set(highest_received_round as i64);

        // Let the proposer draw early conclusions from a certificate at this round and epoch, without its
        // parents or payload (which we may not have yet).
        //
        // Since our certificate is well-signed, it shows a majority of honest signers stand at round r,
        // so to make a successful proposal, our proposer must use parents at least at round r-1.
        //
        // This allows the proposer not to fire proposals at rounds strictly below the certificate we witnessed.
        let minimal_round_for_parents = certificate.round().saturating_sub(1);
        self.inner
            .tx_parents
            .send((vec![], minimal_round_for_parents, certificate.epoch()))
            .await
            .map_err(|_| DagError::ShuttingDown)?;

        // Instruct workers to download any missing batches referenced in this certificate.
        // Since this header got certified, we are sure that all the data it refers to (ie. its batches and its parents) are available.
        // We can thus continue the processing of the certificate without blocking on batch synchronization.
        let inner = self.inner.clone();
        let header = certificate.header().clone();
        let max_age = self.inner.gc_depth.saturating_sub(1);
        self.inner.batch_tasks.lock().spawn(async move {
            Synchronizer::sync_batches_internal(inner, &header, max_age, true).await
        });

        let highest_processed_round = self.inner.highest_processed_round.load(Ordering::Acquire);
        if highest_processed_round + NEW_CERTIFICATE_ROUND_LIMIT < certificate.round() {
            self.inner
                .tx_certificate_fetcher
                .send(certificate.clone())
                .await
                .map_err(|_| DagError::ShuttingDown)?;
            return Err(DagError::TooNew(
                certificate.digest().into(),
                certificate.round(),
                highest_processed_round,
            ));
        }

        let (sender, receiver) = oneshot::channel();
        self.inner
            .tx_certificate_acceptor
            .send((certificate, sender, early_suspend))
            .await
            .expect("Synchronizer should shut down before certificate acceptor task.");
        receiver
            .await
            .expect("Synchronizer should shut down before certificate acceptor task.")
    }

    /// This function checks if a certificate has all parents and can be accepted into storage.
    /// If yes, writing the certificate to storage and sending it to consensus need to happen
    /// atomically. Otherwise, there will be divergence between certificate storage and consensus
    /// DAG. A certificate that is sent to consensus must have all of its parents already in
    /// the consensu DAG.
    ///
    /// Because of the atomicity requirement, this function cannot be made cancellation safe.
    /// So it is run in a loop inside a separate task, connected to `Synchronizer` via a channel.
    async fn process_certificate_with_lock(
        inner: &Inner,
        certificate: Certificate,
        early_suspend: bool,
    ) -> DagResult<()> {
        // The state lock must be held for the rest of the function, to ensure updating state,
        // writing certificates into storage and sending certificates to consensus are atomic.
        // The atomicity makes sure the internal state is consistent with DAG in certificate store,
        // and certificates are sent to consensus in causal order.
        // It is possible to reduce the critical section below, but it seems unnecessary for now.
        let mut state = inner.state.lock().await;

        let digest = certificate.digest();

        // Ensure parents are checked if !early_suspend.
        // See comments above `try_accept_fetched_certificate()` for details.
        if early_suspend {
            // Re-check if the certificate has been suspended, which can happen before the lock is
            // acquired.
            if let Some(notify) = state.check_suspended(&digest) {
                trace!("Certificate {digest:?} is still suspended. Skip processing.");
                inner
                    .metrics
                    .certificates_suspended
                    .with_label_values(&["dedup_locked"])
                    .inc();
                return Err(DagError::Suspended(notify));
            }
        }

        // Ensure either we have all the ancestors of this certificate, or the parents have been garbage collected.
        // If we don't, the synchronizer will start fetching missing certificates.
        if certificate.round() > inner.gc_round.load(Ordering::Acquire) + 1 {
            let missing_parents = inner.get_missing_parents(&certificate).await?;
            if !missing_parents.is_empty() {
                debug!(
                    "Processing certificate {:?} suspended: missing ancestors",
                    certificate
                );
                inner
                    .metrics
                    .certificates_suspended
                    .with_label_values(&["missing_parents"])
                    .inc();
                // There is no upper round limit to suspended certificates. Currently there is no
                // memory usage issue and this will speed up catching up. But we can revisit later.
                let notify = state.insert(certificate, missing_parents, !early_suspend);
                inner
                    .metrics
                    .certificates_currently_suspended
                    .set(state.num_suspended() as i64);
                return Err(DagError::Suspended(notify));
            }
        }

        let suspended_certs = state.accept_children(certificate.round(), certificate.digest());
        // Accept in causal order.
        inner
            .accept_certificate_internal(&state, certificate)
            .await?;
        for suspended in suspended_certs {
            inner
                .accept_suspended_certificate(&state, suspended)
                .await?;
        }

        inner
            .metrics
            .certificates_currently_suspended
            .set(state.num_suspended() as i64);

        Ok(())
    }

    /// Pushes new certificates received from the rx_own_certificate_broadcast channel
    /// to the target peer continuously. Only exits when the primary is shutting down.
    // TODO: move this to proposer, since this naturally follows after a certificate is created.
    async fn push_certificates(
        network: Network,
        authority_id: AuthorityIdentifier,
        network_key: NetworkPublicKey,
        mut rx_own_certificate_broadcast: broadcast::Receiver<Certificate>,
    ) {
        const PUSH_TIMEOUT: Duration = Duration::from_secs(10);
        let peer_id = anemo::PeerId(network_key.0.to_bytes());
        let peer = network.waiting_peer(peer_id);
        let client = PrimaryToPrimaryClient::new(peer);
        // Older broadcasts return early, so the last broadcast must be the latest certificate.
        // This will contain at most certificates created within the last PUSH_TIMEOUT.
        let mut requests = FuturesOrdered::new();
        // Back off and retry only happen when there is only one certificate to be broadcasted.
        // Otherwise no retry happens.
        const BACKOFF_INTERVAL: Duration = Duration::from_millis(100);
        const MAX_BACKOFF_MULTIPLIER: u32 = 100;
        let mut backoff_multiplier: u32 = 0;

        async fn send_certificate(
            mut client: PrimaryToPrimaryClient<WaitingPeer>,
            request: Request<SendCertificateRequest>,
            cert: Certificate,
        ) -> (
            Certificate,
            Result<Response<SendCertificateResponse>, Status>,
        ) {
            let resp = client.send_certificate(request).await;
            (cert, resp)
        }

        loop {
            tokio::select! {
                result = rx_own_certificate_broadcast.recv() => {
                    let cert = match result {
                        Ok(cert) => cert,
                        Err(broadcast::error::RecvError::Closed) => {
                            trace!("Certificate sender {authority_id} is shutting down!");
                            return;
                        }
                        Err(broadcast::error::RecvError::Lagged(e)) => {
                            warn!("Certificate broadcaster {authority_id} lagging! {e}");
                            // Re-run the loop to receive again.
                            continue;
                        }
                    };
                    let request = Request::new(SendCertificateRequest { certificate: cert.clone() }).with_timeout(PUSH_TIMEOUT);
                    requests.push_back(send_certificate(client.clone(),request, cert));
                }
                Some((cert, resp)) = requests.next() => {
                    backoff_multiplier = match resp {
                        Ok(_) => {
                            0
                        },
                        Err(_) => {
                            if requests.is_empty() {
                                // Retry broadcasting the latest certificate, to help the network stay alive.
                                let request = Request::new(SendCertificateRequest { certificate: cert.clone() }).with_timeout(PUSH_TIMEOUT);
                                requests.push_back(send_certificate(client.clone(), request, cert));
                                min(backoff_multiplier * 2 + 1, MAX_BACKOFF_MULTIPLIER)
                            } else {
                                // TODO: add backoff and retries for transient & retriable errors.
                                0
                            }
                        },
                    };
                    if backoff_multiplier > 0 {
                        sleep(BACKOFF_INTERVAL * backoff_multiplier).await;
                    }
                }
            };
        }
    }

    /// Synchronizes batches in the given header with other nodes (through our workers).
    /// Blocks until either synchronization is complete, or the current consensus rounds advances
    /// past the max allowed age. (`max_age == 0` means the header's round must match current
    /// round.)
    pub async fn sync_header_batches(&self, header: &Header, max_age: Round) -> DagResult<()> {
        Synchronizer::sync_batches_internal(self.inner.clone(), header, max_age, false).await
    }

    // TODO: Add batching support to synchronizer and use this call from executor.
    // pub async fn sync_certificate_batches(
    //     &self,
    //     header: &Header,
    //     network: anemo::Network,
    //     max_age: Round,
    // ) -> DagResult<()> {
    //     Synchronizer::sync_batches_internal(self.inner.clone(), header, max_age, true)
    //         .await
    // }

    async fn sync_batches_internal(
        inner: Arc<Inner>,
        header: &Header,
        max_age: Round,
        is_certified: bool,
    ) -> DagResult<()> {
        if header.author() == inner.authority_id {
            debug!("skipping sync_batches for header {header}: no need to sync payload from own workers");
            return Ok(());
        }

        // Clone the round updates channel so we can get update notifications specific to
        // this RPC handler.
        let mut rx_consensus_round_updates = inner.rx_consensus_round_updates.clone();
        let mut consensus_round = rx_consensus_round_updates.borrow().committed_round;
        ensure!(
            header.round() >= consensus_round.saturating_sub(max_age),
            DagError::TooOld(
                header.digest().into(),
                header.round(),
                consensus_round.saturating_sub(max_age)
            )
        );

        let mut missing = HashMap::new();
        for (digest, (worker_id, _)) in header.payload().iter() {
            // Check whether we have the batch. If one of our worker has the batch, the primary stores the pair
            // (digest, worker_id) in its own storage. It is important to verify that we received the batch
            // from the correct worker id to prevent the following attack:
            //      1. A Bad node sends a batch X to 2f good nodes through their worker #0.
            //      2. The bad node proposes a malformed block containing the batch X and claiming it comes
            //         from worker #1.
            //      3. The 2f good nodes do not need to sync and thus don't notice that the header is malformed.
            //         The bad node together with the 2f good nodes thus certify a block containing the batch X.
            //      4. The last good node will never be able to sync as it will keep sending its sync requests
            //         to workers #1 (rather than workers #0). Also, clients will never be able to retrieve batch
            //         X as they will be querying worker #1.
            if !inner.payload_store.contains(*digest, *worker_id)? {
                missing
                    .entry(*worker_id)
                    .or_insert_with(Vec::new)
                    .push(*digest);
            }
        }

        // Build Synchronize requests to workers.
        let mut synchronize_handles = Vec::new();
        for (worker_id, digests) in missing {
            let inner = inner.clone();
            let worker_name = inner
                .worker_cache
                .worker(
                    inner
                        .committee
                        .authority(&inner.authority_id)
                        .unwrap()
                        .protocol_key(),
                    &worker_id,
                )
                .expect("Author of valid header is not in the worker cache")
                .name;
            let client = inner.client.clone();
            let retry_config = RetryConfig {
                retrying_max_elapsed_time: None, // Retry forever.
                ..Default::default()
            };
            let handle = retry_config.retry(move || {
                let digests = digests.clone();
                let message = WorkerSynchronizeMessage {
                    digests: digests.clone(),
                    target: header.author(),
                    is_certified,
                };
                let client = client.clone();
                let worker_name = worker_name.clone();
                let inner = inner.clone();
                async move {
                    let result = client.synchronize(worker_name, message).await.map_err(|e| {
                        backoff::Error::transient(DagError::NetworkError(format!("{e:?}")))
                    });
                    if result.is_ok() {
                        for digest in &digests {
                            inner
                                .payload_store
                                .write(digest, &worker_id)
                                .map_err(|e| backoff::Error::permanent(DagError::StoreError(e)))?
                        }
                    }
                    result
                }
            });
            synchronize_handles.push(handle);
        }

        // Wait until results are back, or this request gets too old to continue.
        let mut wait_synchronize = futures::future::try_join_all(synchronize_handles);
        loop {
            tokio::select! {
                results = &mut wait_synchronize => {
                    break results
                        .map(|_| ())
                        .map_err(|e| DagError::NetworkError(format!("error synchronizing batches: {e:?}")))
                },
                // This aborts based on consensus round and not narwhal round. When this function
                // is used as part of handling vote requests, this may cause us to wait a bit
                // longer than needed to give up on synchronizing batches for headers that are
                // too old to receive a vote. This shouldn't be a big deal (the requester can
                // always abort their request at any point too), however if the extra resources
                // used to attempt to synchronize batches for longer than strictly needed become
                // problematic, this function could be augmented to also support cancellation based
                // on narwhal round.
                Ok(()) = rx_consensus_round_updates.changed() => {
                    consensus_round = rx_consensus_round_updates.borrow().committed_round;
                    ensure!(
                        header.round() >= consensus_round.saturating_sub(max_age),
                        DagError::TooOld(
                            header.digest().into(),
                            header.round(),
                            consensus_round.saturating_sub(max_age),
                        )
                    );
                },
            }
        }
    }

    /// Returns the parent certificates of the given header, and a list of digests for any
    /// that are missing.
    pub fn get_parents(
        &self,
        header: &Header,
    ) -> DagResult<(Vec<Certificate>, Vec<CertificateDigest>)> {
        let mut missing = Vec::new();
        let mut parents = Vec::new();
        for digest in header.parents() {
            let cert = if header.round() == 1 {
                self.inner.genesis.get(digest).cloned()
            } else {
                self.inner.certificate_store.read(*digest)?
            };
            match cert {
                Some(certificate) => parents.push(certificate),
                None => missing.push(*digest),
            };
        }

        Ok((parents, missing))
    }

    /// Tries to get all missing parents of the certificate. If there is any, sends the
    /// certificate to `CertificateFetcher` which will trigger range fetching of missing
    /// certificates.
    #[cfg(test)]
    pub async fn get_missing_parents(
        &self,
        certificate: &Certificate,
    ) -> DagResult<Vec<CertificateDigest>> {
        self.inner.get_missing_parents(certificate).await
    }
}

/// Holds information for a suspended certificate. The certificate can be accepted into the DAG
/// once `missing_parents` become empty.
struct SuspendedCertificate {
    certificate: Certificate,
    missing_parents: HashSet<CertificateDigest>,
    notify: AcceptNotification,
}

impl Drop for SuspendedCertificate {
    fn drop(&mut self) {
        // Make sure waiters are notified on shutdown.
        let _ = self.notify.notify();
    }
}

/// Keeps track of suspended certificates and their missing parents.
/// The digest keys in `suspended` and `missing` can overlap, but a digest can exist in one map
/// but not the other.
///
/// They can be combined into a single map, but it seems more complex to differentiate between
/// suspended certificates that is not a missing parent of another, from a missing parent without
/// the actual certificate.
///
/// Traversal of certificates that can be accepted should start from the missing map, i.e.
/// 1. If a certificate exists in `missing`, remove its entry.
/// 2. Find children of the certificate, update their missing parents.
/// 3. If a child certificate no longer has missing parent, traverse from it with step 1.
///
/// Synchronizer should access this struct via its methods, to avoid making inconsistent changes.
#[derive(Default)]
struct State {
    // Maps digests of suspended certificates to details including the certificate itself.
    suspended: HashMap<CertificateDigest, SuspendedCertificate>,
    // Maps digests of certificates that are not yet in the DAG, to digests of certificates that
    // include them as parents. Keys are prefixed by round number to allow GC.
    missing: BTreeMap<(Round, CertificateDigest), HashSet<CertificateDigest>>,
}

impl State {
    /// Checks if a digest is suspended. If it is, gets a notification for when it is accepted.
    fn check_suspended(&self, digest: &CertificateDigest) -> Option<AcceptNotification> {
        self.suspended
            .get(digest)
            .map(|suspended_cert| suspended_cert.notify.clone())
    }

    /// Inserts a certificate with its missing parents into the suspended state.
    /// When `allow_reinsert` is false and the same certificate digest is inserted again,
    /// this function will panic. Otherwise, this function checks the missing parents of
    /// the certificate and verifies the same set is stored, before allowing a reinsertion.
    fn insert(
        &mut self,
        certificate: Certificate,
        missing_parents: Vec<CertificateDigest>,
        allow_reinsert: bool,
    ) -> AcceptNotification {
        let digest = certificate.digest();
        let missing_round = certificate.round() - 1;
        let missing_parents_map: HashSet<_> = missing_parents.iter().cloned().collect();
        if allow_reinsert {
            if let Some(suspended_cert) = self.suspended.get(&digest) {
                assert_eq!(
                    suspended_cert.missing_parents, missing_parents_map,
                    "Inconsistent missing parents! {:?} vs {:?}",
                    suspended_cert.missing_parents, missing_parents_map
                );
                return suspended_cert.notify.clone();
            }
        }
        let notify = Arc::new(NotifyOnce::new());
        assert!(self
            .suspended
            .insert(
                digest,
                SuspendedCertificate {
                    certificate,
                    missing_parents: missing_parents_map,
                    notify: notify.clone(),
                }
            )
            .is_none());
        for d in missing_parents {
            assert!(self
                .missing
                .entry((missing_round, d))
                .or_default()
                .insert(digest));
        }
        notify
    }

    /// Examines children of a certificate that has been accepted, and returns the children that
    /// can be accepted as well.
    fn accept_children(
        &mut self,
        round: Round,
        digest: CertificateDigest,
    ) -> Vec<SuspendedCertificate> {
        // This validation is only triggered for fetched and own certificates.
        // Certificates from other sources will find the suspended certificate and wait on its
        // accept notification, so no parent check or validation is done.
        if let Some(suspended_cert) = self.suspended.remove(&digest) {
            panic!(
                "Suspended certificate {digest:?} has no missing parent ({:?} exist in store)",
                suspended_cert.missing_parents
            )
        }
        let mut to_traverse = VecDeque::new();
        let mut to_accept = Vec::new();
        to_traverse.push_back((round, digest));
        while let Some((round, digest)) = to_traverse.pop_front() {
            let Some(child_digests) = self.missing.remove(&(round, digest)) else {
                continue;
            };
            for child in &child_digests {
                let suspended_child = self.suspended.get_mut(child).expect("Inconsistency found!");
                suspended_child.missing_parents.remove(&digest);
                if suspended_child.missing_parents.is_empty() {
                    let suspended_child = self.suspended.remove(child).unwrap();
                    to_traverse.push_back((
                        suspended_child.certificate.round(),
                        suspended_child.certificate.digest(),
                    ));
                    to_accept.push(suspended_child);
                }
            }
        }
        to_accept
    }

    /// Runs GC on the suspended certificates, returns a list that can be accepted at gc round + 1.
    /// It is caller's responsibility to check if some children of the returned certificates can
    /// also be accepted.
    fn run_gc(&mut self, gc_round: Round) -> Vec<SuspendedCertificate> {
        // Remove suspended certificates below gc round, and collect digests for certificates just
        // above the gc round.
        let mut gc_certificates = Vec::new();
        let mut certificates_above_gc_round = HashSet::new();
        while let Some(((round, digest), children)) = self.missing.iter().next() {
            if *round > gc_round {
                break;
            }
            if *round == gc_round {
                certificates_above_gc_round.extend(children.iter().cloned());
            }
            // It is ok to notify waiters here (via Drop). The certificate will never and does
            // not need to get into certificate store.
            if let Some(suspended) = self.suspended.remove(digest) {
                gc_certificates.push(suspended);
            }
            self.missing.remove(&(*round, *digest));
        }
        // Notify waiters on GC'ed certificates.
        for suspended in gc_certificates {
            suspended
                .notify
                .notify()
                .expect("Suspended certificate should be notified once.");
        }
        // All certificates at gc round + 1 can be accepted.
        let mut to_accept = Vec::new();
        for digest in certificates_above_gc_round {
            let mut suspended_cert = self
                .suspended
                .remove(&digest)
                .expect("Inconsistency found!");
            suspended_cert.missing_parents.clear();
            to_accept.push(suspended_cert);
        }
        to_accept
    }

    fn num_suspended(&self) -> usize {
        self.suspended.len()
    }
}
