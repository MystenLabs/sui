// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use anemo::{Network, Request};
use config::{Committee, Epoch, SharedWorkerCache, WorkerId};
use consensus::dag::Dag;
use crypto::{NetworkPublicKey, PublicKey};
use fastcrypto::hash::Hash as _;
use mysten_metrics::spawn_monitored_task;
use network::{anemo_ext::NetworkExt, RetryConfig};
use parking_lot::Mutex;
use std::{
    cmp::min,
    collections::{hash_map, BTreeMap, HashMap, VecDeque},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use storage::{CertificateStore, PayloadToken};
use store::Store;
use tokio::{
    sync::{broadcast, oneshot, watch},
    task::JoinSet,
    time::timeout,
};
use tracing::{debug, error, trace, warn};
use types::{
    ensure,
    error::{DagError, DagResult},
    metered_channel::Sender,
    BatchDigest, Certificate, CertificateDigest, Header, PrimaryToPrimaryClient,
    PrimaryToWorkerClient, Round, SendCertificateRequest, WorkerSynchronizeMessage,
};

use crate::{aggregators::CertificatesAggregator, metrics::PrimaryMetrics};

#[cfg(test)]
#[path = "tests/synchronizer_tests.rs"]
pub mod synchronizer_tests;

struct Inner {
    /// The public key of this primary.
    name: PublicKey,
    /// Committee of the current epoch.
    committee: Committee,
    /// The worker information cache.
    worker_cache: SharedWorkerCache,
    /// The depth of the garbage collector.
    gc_depth: Round,
    /// Highest round that has been GC'ed.
    gc_round: AtomicU64,
    /// Highest round of certificate accepted into the certificate store.
    highest_processed_round: AtomicU64,
    /// Highest round of verfied certificate that has been received.
    highest_received_round: AtomicU64,
    /// The persistent storage tables.
    certificate_store: CertificateStore,
    payload_store: Store<(BatchDigest, WorkerId), PayloadToken>,
    /// Send missing certificates to the `CertificateFetcher`.
    tx_certificate_fetcher: Sender<Certificate>,
    /// Output all certificates to the consensus layer. Must send certificates in causal order.
    tx_new_certificates: Sender<Certificate>,
    /// Send valid a quorum of certificates' ids to the `Proposer` (along with their round).
    tx_parents: Sender<(Vec<Certificate>, Round, Epoch)>,
    /// Send own certificates to be broadcasted to all other peers.
    tx_own_certificate_broadcast: broadcast::Sender<Certificate>,
    /// Get a signal when the commit round changes.
    rx_consensus_round_updates: watch::Receiver<Round>,
    /// Genesis digests and contents.
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
    /// Map of digest of certificate pending to be accepted, to channel for signalling the
    /// certificate has been accepted.
    /// The invariant is that if a certificate digest exists in the pending table, it must not
    /// exist in the certificate store. Otherwise waiters will never get notified.
    /// This lock also ensures writing a certificate into store and sending it to consensus is
    /// atomic.
    /// TODO: store certificate and accept it into the store immediately after its parents have
    /// been accepted, instead of waiting for the next external attempt to accept the certificate.
    /// TODO: use a lighter weight alternative to broadcast::channel.
    pending: tokio::sync::Mutex<HashMap<CertificateDigest, broadcast::Sender<()>>>,
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
        name: PublicKey,
        committee: Committee,
        worker_cache: SharedWorkerCache,
        gc_depth: Round,
        certificate_store: CertificateStore,
        payload_store: Store<(BatchDigest, WorkerId), PayloadToken>,
        tx_certificate_fetcher: Sender<Certificate>,
        tx_new_certificates: Sender<Certificate>,
        tx_parents: Sender<(Vec<Certificate>, Round, Epoch)>,
        rx_consensus_round_updates: watch::Receiver<Round>,
        rx_synchronizer_network: oneshot::Receiver<Network>,
        dag: Option<Arc<Dag>>,
        metrics: Arc<PrimaryMetrics>,
    ) -> Self {
        let committee: &Committee = &committee;
        let genesis = Self::make_genesis(committee);
        let highest_processed_round = certificate_store.highest_round_number();
        let highest_created_certificate = certificate_store.last_round(&name).unwrap();
        let gc_round = (*rx_consensus_round_updates.borrow()).saturating_sub(gc_depth);
        let (tx_own_certificate_broadcast, _rx_own_certificate_broadcast) =
            broadcast::channel(1000);
        let inner = Arc::new(Inner {
            name,
            committee: committee.clone(),
            worker_cache,
            gc_depth,
            gc_round: AtomicU64::new(gc_round),
            highest_processed_round: AtomicU64::new(highest_processed_round),
            highest_received_round: AtomicU64::new(0),
            certificate_store,
            payload_store,
            tx_certificate_fetcher,
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
            pending: tokio::sync::Mutex::new(HashMap::new()),
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
                    // this happens during reconfig when the other side hangs up.
                    return;
                }
                let gc_round = (*rx_consensus_round_updates.borrow()).saturating_sub(gc_depth);
                let Some(inner) = weak_inner.upgrade() else {
                    // this happens if Narwhal is shutting down.
                    return;
                };
                // this is the only task updating gc_round
                inner.gc_round.store(gc_round, Ordering::Release);
                inner
                    .certificates_aggregators
                    .lock()
                    .retain(|k, _| k > &gc_round);
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
                .others_primaries(&inner_senders.name)
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
    pub async fn try_accept_certificate(
        &self,
        certificate: Certificate,
        network: &Network,
    ) -> DagResult<()> {
        self.process_certificate_internal(certificate, network, true)
            .await
    }

    /// Tries to accept a certificate that is already sanitized.
    // TODO: produce a different type after sanitize, e.g. VerifiedCertificate.
    pub async fn try_accept_sanitized_certificate(
        &self,
        certificate: Certificate,
        network: &Network,
    ) -> DagResult<()> {
        self.process_certificate_internal(certificate, network, false)
            .await
    }

    /// Validates the certificate and accepts it into the DAG, if the certificate can be verified
    /// and has all parents in the certificate store.
    /// If the certificate has missing parents, wait until all parents are available to accept the
    /// certificate.
    /// Otherwise returns an error.
    pub async fn wait_to_accept_certificate(
        &self,
        certificate: Certificate,
        network: &Network,
    ) -> DagResult<()> {
        match self
            .process_certificate_internal(certificate, network, true)
            .await
        {
            Err(DagError::Suspended(notify)) => {
                let notify = notify.lock().unwrap().take();
                notify
                    .unwrap()
                    .recv()
                    .await
                    .map_err(|_| DagError::ShuttingDown)
            }
            result => result,
        }
    }

    /// Accepts a certificate produced by this primary. This is not expected to fail unless
    /// the primary is shutting down.
    pub async fn accept_own_certificate(
        &self,
        certificate: Certificate,
        network: &Network,
    ) -> DagResult<()> {
        // Process the new certificate.
        match self
            .process_certificate_internal(certificate.clone(), network, false)
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
        let header_to_certificate_duration =
            Duration::from_millis(certificate.metadata.created_at - certificate.header.created_at)
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
            certificate.header.digest(),
            certificate.header.round,
            certificate.header.payload.len(),
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
            .verify(&self.inner.committee, self.inner.worker_cache.clone())
            .map_err(DagError::from)
    }

    async fn process_certificate_internal(
        &self,
        certificate: Certificate,
        network: &Network,
        sanitize: bool,
    ) -> DagResult<()> {
        let digest = certificate.digest();
        if self.inner.certificate_store.contains(&digest)? {
            trace!("Certificate {digest:?} has already been processed. Skip processing.");
            self.inner.metrics.duplicate_certificates_processed.inc();
            return Ok(());
        }

        if sanitize {
            self.sanitize_certificate(&certificate)?;
        }

        debug!(
            "Processing certificate {:?} round:{:?}",
            certificate,
            certificate.round()
        );

        let certificate_source = if self.inner.name.eq(&certificate.origin()) {
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
        let header = certificate.header.clone();
        let sync_network = network.clone();
        let max_age = self.inner.gc_depth.saturating_sub(1);
        self.inner.batch_tasks.lock().spawn(async move {
            Synchronizer::sync_batches_internal(inner, &header, sync_network, max_age).await
        });

        // Ensure either we have all the ancestors of this certificate, or the parents have been garbage collected.
        // If we don't, the synchronizer will start fetching missing certificates.
        if certificate.round() > self.inner.gc_round.load(Ordering::Acquire) + 1
            && !self.check_parents(&certificate).await?
        {
            let rx = {
                let mut pending = self.inner.pending.lock().await;
                // This check is necessary to ensure the certificate has not been accepted while trying
                // to acquire the pending lock.
                if self.inner.certificate_store.contains(&digest)? {
                    return Ok(());
                }
                self.inner
                    .metrics
                    .certificates_suspended
                    .with_label_values(&["missing_parents"])
                    .inc();
                match pending.entry(digest) {
                    hash_map::Entry::Occupied(entry) => entry.get().subscribe(),
                    hash_map::Entry::Vacant(entry) => {
                        let (tx, rx) = broadcast::channel(1);
                        entry.insert(tx);
                        rx
                    }
                }
            };
            debug!(
                "Processing certificate {:?} suspended: missing ancestors",
                certificate
            );
            return Err(DagError::Suspended(Arc::new(std::sync::Mutex::new(Some(
                rx,
            )))));
        }

        self.accept_certificate_internal(certificate).await
    }

    async fn accept_certificate_internal(&self, certificate: Certificate) -> DagResult<()> {
        let digest = certificate.digest();

        // This lock must be held for the scope of the function, to ensure the operations to write
        // certificate into storage and sending the certificate to consensus is atomic. Otherwise
        // it would be possible for certificates to be sent to consensus not in causal order,
        // leading to inconsistencies or consensus forks.
        let mut pending = self.inner.pending.lock().await;

        // Store the certificate. Afterwards, the certificate must be sent to consensus
        // or Narwhal needs to shutdown, to avoid insistencies certificate store and
        // consensus dag.
        self.inner.certificate_store.write(certificate.clone())?;

        // Update metrics for accepted certificates.
        let highest_processed_round = self
            .inner
            .highest_processed_round
            .fetch_max(certificate.round(), Ordering::AcqRel)
            .max(certificate.round());
        let certificate_source = if self.inner.name.eq(&certificate.origin()) {
            "own"
        } else {
            "other"
        };
        self.inner
            .metrics
            .highest_processed_round
            .with_label_values(&[certificate_source])
            .set(highest_processed_round as i64);
        self.inner
            .metrics
            .certificates_processed
            .with_label_values(&[certificate_source])
            .inc();

        // Append the certificate to the aggregator of the
        // corresponding round.
        if let Err(e) = self
            .inner
            .append_certificate_in_aggregator(certificate.clone())
            .await
        {
            warn!(
                "Failed to aggregate certificate {} for header: {}",
                digest, e
            );
            return Err(DagError::ShuttingDown);
        }

        // Send it to the consensus layer.
        if let Err(e) = self.inner.tx_new_certificates.send(certificate).await {
            warn!(
                "Failed to deliver certificate {} to the consensus: {}",
                digest, e
            );
            return Err(DagError::ShuttingDown);
        }

        if let Some(sender) = pending.remove(&digest) {
            // Ignore error if there is no more listeners.
            let _ = sender.send(());
        }

        Ok(())
    }

    /// Pushes new certificates received from the rx_own_certificate_broadcast channel
    /// to the target peer continuously. Only exits when the primary is shutting down.
    // TODO: allow sending multiple (<=10) outgoing requests concurrently.
    async fn push_certificates(
        network: Network,
        name: PublicKey,
        network_key: NetworkPublicKey,
        mut rx_own_certificate_broadcast: broadcast::Receiver<Certificate>,
    ) {
        const PUSH_TIMEOUT: Duration = Duration::from_secs(5);
        const MIN_FAILURE_BACKOFF: Duration = Duration::from_millis(100);
        const MAX_FAILURE_BACKOFF: Duration = Duration::from_secs(10);
        let peer_id = anemo::PeerId(network_key.0.to_bytes());
        let peer = network.waiting_peer(peer_id);
        let mut client = PrimaryToPrimaryClient::new(peer);
        let mut certificates = VecDeque::new();
        let mut failure_backoff = MIN_FAILURE_BACKOFF;
        loop {
            let wait_timeout = if certificates.is_empty() {
                MAX_FAILURE_BACKOFF
            } else {
                failure_backoff
            };
            match timeout(wait_timeout, rx_own_certificate_broadcast.recv()).await {
                Ok(Ok(cert)) => certificates.push_back(cert),
                Ok(Err(broadcast::error::RecvError::Closed)) => {
                    trace!("Certificate sender {name} shutting down!");
                    return;
                }
                Ok(Err(broadcast::error::RecvError::Lagged(e))) => {
                    warn!("Certificate broadcaster {name} lagging! {e}");
                    // Re-run the loop to receive again.
                    continue;
                }
                Err(_) => {
                    if certificates.is_empty() {
                        // MAX_FAILURE_BACKOFF has elapsed without a certificate created.
                        // This should rarely happen.
                        continue;
                    }
                }
            };
            // Get more certificates if available in the broadcast channel.
            'recv: loop {
                match rx_own_certificate_broadcast.try_recv() {
                    Ok(cert) => {
                        certificates.push_back(cert);
                    }
                    Err(broadcast::error::TryRecvError::Closed) => {
                        trace!("Certificate sender {name} shutting down!");
                        return;
                    }
                    Err(broadcast::error::TryRecvError::Lagged(e)) => {
                        warn!("Certificate sender {name} lagging! {e}");
                    }
                    Err(broadcast::error::TryRecvError::Empty) => {
                        break 'recv;
                    }
                };
            }
            // TODO: support sending multiple certificates concurrently.
            // Only send the latest certificate.
            while certificates.len() > 1 {
                certificates.pop_front();
            }
            let cert = certificates.front().unwrap().clone();
            let request = Request::new(SendCertificateRequest { certificate: cert })
                .with_timeout(PUSH_TIMEOUT);
            match client.send_certificate(request).await {
                Ok(_) => {
                    certificates.pop_front();
                    failure_backoff = MIN_FAILURE_BACKOFF;
                }
                Err(status) => {
                    debug!("Failed to send certificate to {name}! {status:?}");
                    failure_backoff = min(failure_backoff * 2, MAX_FAILURE_BACKOFF);
                }
            }
        }
    }

    /// Synchronizes batches in the given header with other nodes (through our workers).
    /// Blocks until either synchronization is complete, or the current consensus rounds advances
    /// past the max allowed age. (`max_age == 0` means the header's round must match current
    /// round.)
    pub async fn sync_batches(
        &self,
        header: &Header,
        network: anemo::Network,
        max_age: Round,
    ) -> DagResult<()> {
        Synchronizer::sync_batches_internal(self.inner.clone(), header, network, max_age).await
    }

    async fn sync_batches_internal(
        inner: Arc<Inner>,
        header: &Header,
        network: anemo::Network,
        max_age: Round,
    ) -> DagResult<()> {
        if header.author == inner.name {
            debug!("skipping sync_batches for header {header}: no need to sync payload from own workers");
            return Ok(());
        }

        // Clone the round updates channel so we can get update notifications specific to
        // this RPC handler.
        let mut rx_consensus_round_updates = inner.rx_consensus_round_updates.clone();
        let mut consensus_round = *rx_consensus_round_updates.borrow();
        ensure!(
            header.round >= consensus_round.saturating_sub(max_age),
            DagError::TooOld(
                header.digest().into(),
                header.round,
                consensus_round.saturating_sub(max_age)
            )
        );

        let mut missing = HashMap::new();
        for (digest, (worker_id, _)) in header.payload.iter() {
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
            if inner
                .payload_store
                .read((*digest, *worker_id))
                .await?
                .is_none()
            {
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
                .load()
                .worker(&inner.name, &worker_id)
                .expect("Author of valid header is not in the worker cache")
                .name;
            let network = network.clone();
            let retry_config = RetryConfig {
                retrying_max_elapsed_time: None, // Retry forever.
                ..Default::default()
            };
            let handle = retry_config.retry(move || {
                let network = network.clone();
                let digests = digests.clone();
                let message = WorkerSynchronizeMessage {
                    digests: digests.clone(),
                    target: header.author.clone(),
                };
                let peer = network.waiting_peer(anemo::PeerId(worker_name.0.to_bytes()));
                let mut client = PrimaryToWorkerClient::new(peer);
                let inner = inner.clone();
                async move {
                    let result = client.synchronize(message).await.map_err(|e| {
                        backoff::Error::transient(DagError::NetworkError(format!("{e:?}")))
                    });
                    if result.is_ok() {
                        for digest in digests.clone() {
                            inner
                                .payload_store
                                .async_write((digest, worker_id), 0u8)
                                .await;
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
                    consensus_round = *rx_consensus_round_updates.borrow();
                    ensure!(
                        header.round >= consensus_round.saturating_sub(max_age),
                        DagError::TooOld(
                            header.digest().into(),
                            header.round,
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
        for digest in &header.parents {
            let cert = if header.round == 1 {
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

    /// Checks whether we have seen all the ancestors of the certificate. If we don't, send the
    /// certificate to the `CertificateFetcher` which will trigger range fetching of missing
    /// certificates.
    pub async fn check_parents(&self, certificate: &Certificate) -> DagResult<bool> {
        if certificate.round() == 1 {
            for digest in &certificate.header.parents {
                if self.inner.genesis.contains_key(digest) {
                    continue;
                }
                return Ok(false);
            }
            return Ok(true);
        }

        for digest in &certificate.header.parents {
            if !self.has_processed_certificate(*digest).await? {
                self.inner
                    .tx_certificate_fetcher
                    .send(certificate.clone())
                    .await
                    .map_err(|_| DagError::ShuttingDown)?;
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// This method answers to the question of whether the certificate with the
    /// provided digest has ever been successfully processed (seen) by this
    /// node. Depending on the mode of running the node (internal Vs external
    /// consensus) either the dag will be used to confirm that or the
    /// certificate_store.
    async fn has_processed_certificate(&self, digest: CertificateDigest) -> DagResult<bool> {
        if let Some(dag) = &self.inner.dag {
            return Ok(dag.has_ever_contained(digest).await);
        }
        Ok(self.inner.certificate_store.contains(&digest)?)
    }
}
