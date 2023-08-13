// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use anemo::{rpc::Status, Network, Request, Response};
use config::{AuthorityIdentifier, Committee, Epoch, Stake, WorkerCache};
use consensus::consensus::ConsensusRound;
use crypto::NetworkPublicKey;
use fastcrypto::hash::Hash as _;
use futures::{stream::FuturesOrdered, StreamExt};
use mysten_common::sync::notify_once::NotifyOnce;
use mysten_metrics::metered_channel::{channel_with_total, Sender};
use mysten_metrics::{monitored_scope, spawn_logged_monitored_task};
use network::{
    anemo_ext::{NetworkExt, WaitingPeer},
    client::NetworkClient,
    PrimaryToWorkerClient, RetryConfig,
};
use parking_lot::Mutex;
use std::{
    cmp::min,
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use storage::{CertificateStore, PayloadStore};
use tokio::{
    sync::{broadcast, oneshot, watch, MutexGuard},
    task::JoinSet,
    time::{sleep, timeout},
};
use tracing::{debug, error, instrument, trace, warn};
use types::{
    ensure,
    error::{AcceptNotification, DagError, DagResult},
    Certificate, CertificateAPI, CertificateDigest, Header, HeaderAPI, PrimaryToPrimaryClient,
    Round, SendCertificateRequest, SendCertificateResponse, WorkerSynchronizeMessage,
};

use crate::{
    aggregators::CertificatesAggregator, certificate_fetcher::CertificateFetcherCommand,
    metrics::PrimaryMetrics, PrimaryChannelMetrics, CHANNEL_CAPACITY,
};

#[cfg(test)]
#[path = "tests/synchronizer_tests.rs"]
pub mod synchronizer_tests;

/// Only try to accept or suspend a certificate, if it is within this limit above the
/// locally highest processed round.
/// Expected max memory usage with 100 nodes: 100 nodes * 1000 rounds * 3.3KB per certificate = 330MB.
const NEW_CERTIFICATE_ROUND_LIMIT: Round = 1000;

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
    /// The persistent store of the available batch digests produced either via our own workers
    /// or others workers.
    payload_store: PayloadStore,
    /// Send missing certificates to the `CertificateFetcher`.
    tx_certificate_fetcher: Sender<CertificateFetcherCommand>,
    /// Send certificates to be accepted into a separate task that runs
    /// `process_certificate_with_lock()` in a loop.
    /// See comment above `process_certificate_with_lock()` for why this is necessary.
    tx_certificate_acceptor: Sender<(Certificate, oneshot::Sender<DagResult<()>>, bool)>,
    /// Output all certificates to the consensus layer. Must send certificates in causal order.
    tx_new_certificates: Sender<Certificate>,
    /// Send valid a quorum of certificates' ids to the `Proposer` (along with their round).
    tx_parents: Sender<(Vec<Certificate>, Round, Epoch)>,
    /// Send own certificates to be broadcasted to all other peers.
    tx_own_certificate_broadcast: broadcast::Sender<Certificate>,
    /// Get a signal when the commit & gc round changes.
    rx_consensus_round_updates: watch::Receiver<ConsensusRound>,
    /// Genesis digests and contents.
    genesis: HashMap<CertificateDigest, Certificate>,
    /// Contains Synchronizer specific metrics among other Primary metrics.
    metrics: Arc<PrimaryMetrics>,
    /// Background tasks broadcasting newly formed certificates.
    certificate_senders: Mutex<JoinSet<()>>,
    /// A background task that synchronizes batches. A tuple of a header and the maximum accepted
    /// age is sent over.
    tx_batch_tasks: Sender<(Header, u64)>,
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
    #[instrument(level = "debug", skip_all)]
    async fn accept_certificate_internal(
        &self,
        _lock: &MutexGuard<'_, State>,
        certificate: Certificate,
    ) -> DagResult<()> {
        let _scope = monitored_scope("Synchronizer::accept_certificate_internal");

        debug!("Accepting certificate {:?}", certificate);

        let digest = certificate.digest();

        // Validate that certificates are accepted in causal order.
        // Currently it is relatively cheap because of certificate store caching.
        let gc_round = self.gc_round.load(Ordering::Acquire);
        let existence = self
            .certificate_store
            .multi_contains(certificate.header().ancestor_digests().iter())?;
        for ((round, ancestor), exists) in certificate
            .header()
            .ancestors()
            .iter()
            .zip(existence.iter())
        {
            if !*exists && *round > gc_round {
                panic!("Ancestor {ancestor:?} not found for {certificate:?}!")
            }
        }

        // Store the certificate and make it available as an ancestor to other certificates.
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

    /// Returns ancestor digests that do no exist either in storage or among suspended.
    async fn get_unknown_ancestor_digests(
        &self,
        header: &Header,
    ) -> DagResult<Vec<(Round, CertificateDigest)>> {
        let _scope = monitored_scope("Synchronizer::get_unknown_ancestor_digests");

        if header.round() == 1 {
            for digest in header.ancestor_digests() {
                if !self.genesis.contains_key(&digest) {
                    return Err(DagError::InvalidGenesisParent(digest));
                }
            }
            return Ok(Vec::new());
        }

        let existence = self
            .certificate_store
            .multi_contains(header.ancestor_digests().iter())?;
        let mut unknown: Vec<_> = header
            .ancestors()
            .iter()
            .zip(existence.iter())
            .filter_map(|(ancestor, exists)| if *exists { None } else { Some(*ancestor) })
            .collect();
        let state = self.state.lock().await;
        unknown.retain(|ancestor| !state.suspended.contains_key(ancestor));
        Ok(unknown)
    }

    /// Tries to get all missing parents of the certificate. If there is any, sends the
    /// certificate to `CertificateFetcher` which will trigger range fetching of missing
    /// certificates.
    async fn get_missing_ancestors(
        &self,
        certificate: &Certificate,
    ) -> DagResult<Vec<(Round, CertificateDigest)>> {
        let _scope = monitored_scope("Synchronizer::get_missing_ancestors");

        let mut result = Vec::new();
        if certificate.round() == 1 {
            for digest in certificate.header().ancestor_digests() {
                if !self.genesis.contains_key(&digest) {
                    return Err(DagError::InvalidGenesisParent(digest));
                }
            }
            return Ok(result);
        }

        let existence = self
            .certificate_store
            .multi_contains(certificate.header().ancestor_digests().iter())?;
        for (ancestor, exists) in certificate
            .header()
            .ancestors()
            .iter()
            .zip(existence.iter())
        {
            if !*exists {
                result.push(*ancestor);
            }
        }
        if !result.is_empty() {
            self.tx_certificate_fetcher
                .send(CertificateFetcherCommand::Ancestors(certificate.clone()))
                .await
                .map_err(|_| DagError::ShuttingDown)?;
        }
        Ok(result)
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
        payload_store: PayloadStore,
        tx_certificate_fetcher: Sender<CertificateFetcherCommand>,
        tx_new_certificates: Sender<Certificate>,
        tx_parents: Sender<(Vec<Certificate>, Round, Epoch)>,
        rx_consensus_round_updates: watch::Receiver<ConsensusRound>,
        metrics: Arc<PrimaryMetrics>,
        primary_channel_metrics: &PrimaryChannelMetrics,
    ) -> Self {
        let committee: &Committee = &committee;
        let genesis = Self::make_genesis(committee);
        let highest_processed_round = certificate_store.highest_round_number();
        let highest_created_certificate = certificate_store.last_round(authority_id).unwrap();
        let gc_round = rx_consensus_round_updates.borrow().gc_round;
        let (tx_own_certificate_broadcast, _rx_own_certificate_broadcast) =
            broadcast::channel(CHANNEL_CAPACITY);
        let (tx_certificate_acceptor, mut rx_certificate_acceptor) = channel_with_total(
            CHANNEL_CAPACITY,
            &primary_channel_metrics.tx_certificate_acceptor,
            &primary_channel_metrics.tx_certificate_acceptor_total,
        );

        let (tx_batch_tasks, mut rx_batch_tasks) = channel_with_total(
            CHANNEL_CAPACITY,
            &primary_channel_metrics.tx_batch_tasks,
            &primary_channel_metrics.tx_batch_tasks_total,
        );

        let inner = Arc::new(Inner {
            authority_id,
            committee: committee.clone(),
            worker_cache,
            gc_depth,
            gc_round: AtomicU64::new(gc_round),
            highest_processed_round: AtomicU64::new(highest_processed_round),
            highest_received_round: AtomicU64::new(0),
            client: client.clone(),
            certificate_store,
            payload_store,
            tx_certificate_fetcher,
            tx_certificate_acceptor,
            tx_new_certificates,
            tx_parents,
            tx_own_certificate_broadcast: tx_own_certificate_broadcast.clone(),
            rx_consensus_round_updates: rx_consensus_round_updates.clone(),
            genesis,
            metrics,
            tx_batch_tasks,
            certificate_senders: Mutex::new(JoinSet::new()),
            certificates_aggregators: Mutex::new(BTreeMap::new()),
            state: tokio::sync::Mutex::new(State::default()),
        });

        // Start a task to recover parent certificates for proposer.
        let inner_proposer = inner.clone();
        spawn_logged_monitored_task!(
            async move {
                let highest_round_number = inner_proposer.certificate_store.highest_round_number();
                let mut certificates = vec![];
                // The last or last 2 rounds are sufficient for recovery.
                for i in 0..2 {
                    let round = highest_round_number - i;
                    // Do not recover genesis certificates. They are initialized into certificate
                    // aggregator already.
                    if round == 0 {
                        break;
                    }
                    let round_certs = inner_proposer
                        .certificate_store
                        .at_round(round)
                        .expect("Failed recovering certificates in primary core");
                    let stake: Stake = round_certs
                        .iter()
                        .map(|c: &Certificate| inner_proposer.committee.stake_by_id(c.origin()))
                        .sum();
                    certificates.extend(round_certs.into_iter());
                    // If a round has a quorum of certificates, enough have recovered because
                    // a header can be proposed with these parents.
                    if stake >= inner_proposer.committee.quorum_threshold() {
                        break;
                    } else {
                        // Only the last round can have less than a quorum of stake.
                        assert_eq!(i, 0);
                    }
                }
                // Unnecessary to append certificates in ascending round order, but it doesn't
                // hurt either.
                for certificate in certificates.into_iter().rev() {
                    if let Err(e) = inner_proposer
                        .append_certificate_in_aggregator(certificate)
                        .await
                    {
                        debug!(
                            "Failed to recover certificate, assuming Narwhal is shutting down. {e}"
                        );
                        return;
                    }
                }
            },
            "Synchronizer::RecoverCertificates"
        );

        // Start a task to update gc_round, gc in-memory data, and trigger certificate catchup
        // if no gc / consensus commit happened for 30s.
        let weak_inner = Arc::downgrade(&inner);
        spawn_logged_monitored_task!(
            async move {
                const FETCH_TRIGGER_TIMEOUT: Duration = Duration::from_secs(30);
                let mut rx_consensus_round_updates = rx_consensus_round_updates.clone();
                loop {
                    let Ok(result) = timeout(FETCH_TRIGGER_TIMEOUT, rx_consensus_round_updates.changed()).await else {
                        // When consensus commit has not happened for 30s, it is possible that no new
                        // certificate is received by this primary or created in the network, so
                        // fetching should definitely be started.
                        // For other reasons of timing out, there is no harm to start fetching either.
                        let Some(inner) = weak_inner.upgrade() else {
                            debug!("Synchronizer is shutting down.");
                            return;
                        };
                        if inner.tx_certificate_fetcher.send(CertificateFetcherCommand::Kick).await.is_err() {
                            debug!("Synchronizer is shutting down.");
                            return;
                        }
                        inner.metrics.synchronizer_gc_timeout.inc();
                        warn!("No consensus commit happened for {:?}, triggering certificate fetching.", FETCH_TRIGGER_TIMEOUT);
                        continue;
                    };
                    if result.is_err() {
                        debug!("Synchronizer is shutting down.");
                        return;
                    }
                    let _scope = monitored_scope("Synchronizer::gc_iteration");
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
                        match inner
                            .accept_suspended_certificate(&state, suspended_cert)
                            .await
                        {
                            Ok(()) => {}
                            Err(DagError::ShuttingDown) => return,
                            Err(e) => {
                                panic!("Unexpected error accepting certificate during GC! {e}")
                            }
                        }
                    }
                }
            },
            "Synchronizer::GarbageCollection"
        );

        // Start a task to accept certificates. See comment above `process_certificate_with_lock()`
        // for why this task is needed.
        let weak_inner = Arc::downgrade(&inner);
        spawn_logged_monitored_task!(
            async move {
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
                        Self::process_certificate_with_lock(&inner, certificate, early_suspend)
                            .await,
                    );
                }
            },
            "Synchronizer::AcceptCertificates"
        );

        // Start tasks to broadcast created certificates.
        let inner_senders = inner.clone();
        spawn_logged_monitored_task!(
            async move {
                let Ok(network) = client.get_primary_network().await else {
                    error!("Failed to get primary Network!");
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
            },
            "Synchronizer::BroadcastCertificates"
        );

        // Start a task to async download batches if needed
        let weak_inner = Arc::downgrade(&inner);
        spawn_logged_monitored_task!(
            async move {
                let mut batch_tasks: JoinSet<DagResult<()>> = JoinSet::new();

                loop {
                    tokio::select! {
                        result = rx_batch_tasks.recv() => {
                            let (header, _max_age) = match result {
                                Some(r) => r,
                                None => {
                                    // exit loop if the channel has been closed
                                    break;
                                }
                            };

                            let Some(inner) = weak_inner.upgrade() else {
                                debug!("Synchronizer is shutting down.");
                                return;
                            };

                            batch_tasks.spawn(async move {
                                Synchronizer::sync_batches_internal(inner.clone(), &header, true).await
                            });
                        },
                        Some(result) = batch_tasks.join_next() => {
                            if let Err(err) = result  {
                                error!("Error when synchronizing batches: {err:?}")
                            }
                        }
                    }
                }
            },
            "Synchronizer::SyncrhonizeBatches"
        );

        Self { inner }
    }

    /// Validates the certificate and accepts it into the DAG, if the certificate can be verified
    /// and has all ancestors in the certificate store. Otherwise an error is returned.
    /// If the certificate has missing ancestors and cannot be accepted immediately, the error would
    /// contain a value that can be awaited on, for signaling when the certificate is accepted.
    pub async fn try_accept_certificate(&self, certificate: Certificate) -> DagResult<()> {
        let _scope = monitored_scope("Synchronizer::try_accept_certificate");
        self.process_certificate_internal(certificate, true, true)
            .await
    }

    /// Tries to accept a certificate from certificate fetcher.
    /// Fetched certificates are already sanitized, so it is unnecessary to duplicate the work.
    /// Also, this method always checks ancestors of fetched certificates and uses the result to
    /// validate the suspended certificates state, instead of relying on suspended certificates to
    /// potentially return early. This helps to verify consistency, and has little extra cost
    /// because fetched certificates usually are not suspended.
    pub async fn try_accept_fetched_certificate(&self, certificate: Certificate) -> DagResult<()> {
        let _scope = monitored_scope("Synchronizer::try_accept_fetched_certificate");
        self.process_certificate_internal(certificate, false, false)
            .await
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
        let _scope = monitored_scope("Synchronizer::process_certificate_internal");

        let digest = certificate.digest();
        if self.inner.certificate_store.contains(&digest)? {
            trace!("Certificate {digest:?} has already been processed. Skip processing.");
            self.inner.metrics.duplicate_certificates_processed.inc();
            return Ok(());
        }
        // Ensure ancestors are checked if !early_suspend.
        // See comments above `try_accept_fetched_certificate()` for details.
        if early_suspend {
            if let Some(notify) = self
                .inner
                .state
                .lock()
                .await
                .check_suspended((certificate.round(), digest))
            {
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

        // Instruct workers to download any missing batches referenced in this certificate.
        // Since this header got certified, we are sure that all the data it refers to (ie. its batches) are available.
        // We can thus continue the processing of the certificate without blocking on batch synchronization.
        let header = certificate.header().clone();
        let max_age = self.inner.gc_depth.saturating_sub(1);
        self.inner
            .tx_batch_tasks
            .send((header.clone(), max_age))
            .await
            .map_err(|_| DagError::ShuttingDown)?;

        let highest_processed_round = self.inner.highest_processed_round.load(Ordering::Acquire);
        if highest_processed_round + NEW_CERTIFICATE_ROUND_LIMIT < certificate.round() {
            self.inner
                .tx_certificate_fetcher
                .send(CertificateFetcherCommand::Ancestors(certificate.clone()))
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

    /// This function checks if a certificate has all ancestors and can be accepted into storage.
    /// If yes, writing the certificate to storage and sending it to consensus need to happen
    /// atomically. Otherwise, there will be divergence between certificate storage and consensus
    /// DAG. A certificate that is sent to consensus must have all of its ancestors already in
    /// the consensu DAG.
    ///
    /// Because of the atomicity requirement, this function cannot be made cancellation safe.
    /// So it is run in a loop inside a separate task, connected to `Synchronizer` via a channel.
    #[instrument(level = "debug", skip_all)]
    async fn process_certificate_with_lock(
        inner: &Inner,
        certificate: Certificate,
        early_suspend: bool,
    ) -> DagResult<()> {
        let _scope = monitored_scope("Synchronizer::process_certificate_with_lock");
        let digest = certificate.digest();

        // We re-check here in case we already have in pipeline the same certificate for processing
        // more that once.
        if inner.certificate_store.contains(&digest)? {
            debug!("Skip processing certificate {:?}", certificate);
            return Ok(());
        }

        debug!("Processing certificate {:?} with lock", certificate);

        // The state lock must be held for the rest of the function, to ensure updating state,
        // writing certificates into storage and sending certificates to consensus are atomic.
        // The atomicity makes sure the internal state is consistent with DAG in certificate store,
        // and certificates are sent to consensus in causal order.
        // It is possible to reduce the critical section below, but it seems unnecessary for now.
        let mut state = inner.state.lock().await;

        let digest = certificate.digest();

        // Ensure ancestors are checked if !early_suspend.
        // See comments above `try_accept_fetched_certificate()` for details.
        if early_suspend {
            // Re-check if the certificate has been suspended, which can happen before the lock is
            // acquired.
            if let Some(notify) = state.check_suspended((certificate.round(), digest)) {
                trace!("Certificate {digest:?} is still suspended. Skip processing.");
                inner
                    .metrics
                    .certificates_suspended
                    .with_label_values(&["dedup_locked"])
                    .inc();
                return Err(DagError::Suspended(notify));
            }
        }

        // Check if all non-gc'ed ancestors of this certificate are available.
        // If not, the synchronizer will start fetching missing certificates.
        let gc_round = inner.gc_round.load(Ordering::Acquire);
        let missing_ancestors: Vec<_> = inner
            .get_missing_ancestors(&certificate)
            .await?
            .into_iter()
            .filter(|(r, _c)| *r > gc_round)
            .collect();
        if !missing_ancestors.is_empty() {
            debug!(
                "Processing certificate {:?} suspended: missing ancestors",
                certificate
            );
            inner
                .metrics
                .certificates_suspended
                .with_label_values(&["missing_ancestors"])
                .inc();
            // There is no upper round limit to suspended certificates. Currently there is no
            // memory usage issue and this will speed up catching up. But we can revisit later.
            let notify = state.insert(certificate, missing_ancestors, !early_suspend);
            inner
                .metrics
                .certificates_currently_suspended
                .set(state.num_suspended() as i64);
            return Err(DagError::Suspended(notify));
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
    pub async fn sync_header_batches(&self, header: &Header) -> DagResult<()> {
        Synchronizer::sync_batches_internal(self.inner.clone(), header, false).await
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
        is_certified: bool,
    ) -> DagResult<()> {
        if header.author() == inner.authority_id {
            debug!("skipping sync_batches for header {header}: no need to sync payload from own workers");
            return Ok(());
        }

        // Clone the round updates channel so we can get update notifications specific to
        // this RPC handler.
        let mut rx_consensus_round_updates = inner.rx_consensus_round_updates.clone();

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
            let retry_config = RetryConfig::default(); // 30s timeout
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
                // Headers past GC round will never become part of commit, so it becomes
                // unnecessary to sync the payloads.
                Ok(()) = rx_consensus_round_updates.changed() => {
                    let gc_round = rx_consensus_round_updates.borrow().gc_round;
                    if header.round() < gc_round {
                        break Ok(())
                    }
                },
            }
        }
    }

    /// Returns the parent certificates of the given header, waits for availability if needed.
    pub async fn notify_read_ancestor_certificates(
        &self,
        header: &Header,
    ) -> DagResult<Vec<Certificate>> {
        let mut ancestors = Vec::new();
        if header.round() == 1 {
            for (_round, digest) in header.ancestors() {
                match self.inner.genesis.get(&digest) {
                    Some(certificate) => ancestors.push(certificate.clone()),
                    None => return Err(DagError::InvalidGenesisParent(digest)),
                };
            }
        } else {
            let mut cert_notifications: FuturesOrdered<_> = header
                .ancestors()
                .into_iter()
                .map(|(_round, digest)| async move {
                    self.inner.certificate_store.notify_read(digest).await
                })
                .collect();
            while let Some(result) = cert_notifications.next().await {
                ancestors.push(result?);
            }
        }
        Ok(ancestors)
    }

    /// Returns parent digests that do no exist either in storage or among suspended.
    pub async fn get_unknown_ancestor_digests(
        &self,
        header: &Header,
    ) -> DagResult<Vec<(Round, CertificateDigest)>> {
        self.inner.get_unknown_ancestor_digests(header).await
    }

    /// Tries to get all missing ancestors of the certificate. If there is any, sends the
    /// certificate to `CertificateFetcher` which will trigger range fetching of missing
    /// certificates.
    #[cfg(test)]
    pub async fn get_missing_ancestors(
        &self,
        certificate: &Certificate,
    ) -> DagResult<Vec<(Round, CertificateDigest)>> {
        self.inner.get_missing_ancestors(certificate).await
    }
}

/// Holds information for a suspended certificate. The certificate can be accepted into the DAG
/// once `missing_ancestors` become empty.
struct SuspendedCertificate {
    certificate: Certificate,
    missing_ancestors: HashSet<(Round, CertificateDigest)>,
    notify: AcceptNotification,
}

impl Drop for SuspendedCertificate {
    fn drop(&mut self) {
        // Make sure waiters are notified on shutdown.
        let _ = self.notify.notify();
    }
}

/// Keeps track of suspended certificates and their missing ancestors.
/// The digest keys in `suspended` and `missing` can overlap, but a digest can exist in one map
/// but not the other.
///
/// They can be combined into a single map, but it seems more complex to differentiate between
/// suspended certificates that is not a missing ancestor of another, from a missing ancestor
/// without the actual certificate.
///
/// Traversal of certificates that can be accepted should start from the missing map, i.e.
/// 1. If a certificate exists in `missing`, remove its entry.
/// 2. Find children of the certificate, update their missing ancestors.
/// 3. If a child certificate no longer has missing ancestor, traverse from it with step 1.
///
/// Synchronizer should access this struct via its methods, to avoid making inconsistent changes.
#[derive(Default)]
struct State {
    // Maps digests of suspended certificates to details including the certificate itself.
    suspended: HashMap<(Round, CertificateDigest), SuspendedCertificate>,
    // Maps certificates that are not yet in the DAG, to certificates that
    // include them as ancestors.
    missing: BTreeMap<(Round, CertificateDigest), HashSet<(Round, CertificateDigest)>>,
}

impl State {
    /// Checks if a digest is suspended. If it is, gets a notification for when it is accepted.
    fn check_suspended(&self, key: (Round, CertificateDigest)) -> Option<AcceptNotification> {
        self.suspended
            .get(&key)
            .map(|suspended_cert| suspended_cert.notify.clone())
    }

    /// Inserts a certificate with its missing ancestors into the suspended state.
    /// When `allow_reinsert` is false and the same certificate digest is inserted again,
    /// this function will panic. Otherwise, this function checks the missing ancestors of
    /// the certificate and verifies the same set is stored, before allowing a reinsertion.
    fn insert(
        &mut self,
        certificate: Certificate,
        missing_ancestors: Vec<(Round, CertificateDigest)>,
        allow_reinsert: bool,
    ) -> AcceptNotification {
        let digest = certificate.digest();
        let cert_key = (certificate.round(), digest);
        let missing_ancestors_set: HashSet<_> = missing_ancestors.iter().cloned().collect();
        if allow_reinsert {
            if let Some(suspended_cert) = self.suspended.get(&cert_key) {
                assert_eq!(
                    suspended_cert.missing_ancestors, missing_ancestors_set,
                    "Inconsistent missing ancestors! {:?} vs {:?}",
                    suspended_cert.missing_ancestors, missing_ancestors_set
                );
                return suspended_cert.notify.clone();
            }
        }
        let notify = Arc::new(NotifyOnce::new());
        assert!(self
            .suspended
            .insert(
                cert_key,
                SuspendedCertificate {
                    certificate,
                    missing_ancestors: missing_ancestors_set,
                    notify: notify.clone(),
                }
            )
            .is_none());
        for d in missing_ancestors {
            assert!(self.missing.entry(d).or_default().insert(cert_key));
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
        // accept notification, so no ancestor check or validation is done.
        if let Some(suspended_cert) = self.suspended.remove(&(round, digest)) {
            panic!(
                "Suspended certificate {digest:?} is being accepted, but it has missing ancestors {:?}!",
                suspended_cert.missing_ancestors
            )
        }
        let mut to_traverse = VecDeque::new();
        let mut to_accept = Vec::new();
        to_traverse.push_back((round, digest));
        while let Some(ancestor) = to_traverse.pop_front() {
            let Some(children) = self.missing.remove(&ancestor) else {
                // No certificate is missing this ancestor.
                continue;
            };
            for child in &children {
                let suspended_child = self.suspended.get_mut(child).expect("Inconsistency found!");
                suspended_child.missing_ancestors.remove(&ancestor);
                if suspended_child.missing_ancestors.is_empty() {
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
        // Accept suspended certificates below gc round, and child certificates that can be accepted now.
        let mut accept_certificates = Vec::new();
        while let Some(((round, digest), _children)) = self.missing.iter().next() {
            if *round > gc_round {
                break;
            }
            // When this certificate is only referenced by others and not received by this primary yet,
            // it will not be found in self.suspended.
            if let Some(suspended) = self.suspended.remove(&(*round, *digest)) {
                accept_certificates.push(suspended);
            }
            accept_certificates.extend(self.accept_children(*round, *digest).into_iter());
        }
        // Return to caller to accept.
        accept_certificates
    }

    fn num_suspended(&self) -> usize {
        self.suspended.len()
    }
}
