// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use anemo::{rpc, Network, Request};
use config::{Committee, Epoch, SharedCommittee, SharedWorkerCache, WorkerId};
use consensus::dag::Dag;
use crypto::{NetworkPublicKey, PublicKey};
use fastcrypto::hash::Hash as _;
use mysten_metrics::spawn_monitored_task;
use network::{anemo_ext::NetworkExt, ReliableNetwork, RetryConfig};
use parking_lot::Mutex;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use storage::{CertificateStore, PayloadToken};
use store::Store;
use tokio::{
    sync::{oneshot, watch},
    task::JoinSet,
    time::sleep,
};
use tracing::{debug, trace, warn};
use types::{
    ensure,
    error::{DagError, DagResult},
    metered_channel::Sender,
    BatchDigest, Certificate, CertificateDigest, Header, PrimaryMessage, PrimaryToPrimaryClient,
    PrimaryToWorkerClient, Round, WorkerSynchronizeMessage,
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
    /// Highest round of certificate created at this primary.
    highest_created_round: AtomicU64,
    /// The persistent storage.
    certificate_store: CertificateStore,
    payload_store: Store<(BatchDigest, WorkerId), PayloadToken>,
    /// Send commands to the `CertificateFetcher`.
    tx_certificate_fetcher: Sender<Certificate>,
    /// Output all certificates to the consensus layer.
    tx_new_certificates: Sender<Certificate>,
    /// Send valid a quorum of certificates' ids to the `Proposer` (along with their round).
    tx_parents: Sender<(Vec<Certificate>, Round, Epoch)>,
    /// Get a signal when the round changes.
    rx_consensus_round_updates: watch::Receiver<Round>,
    /// The genesis and its digests.
    genesis: HashMap<CertificateDigest, Certificate>,
    /// The dag used for the external consensus
    dag: Option<Arc<Dag>>,
    /// Contains Synchronizer specific metrics among other Primary metrics.
    metrics: Arc<PrimaryMetrics>,
    /// Contains background tasks for:
    /// - synchronizing worker batches for processed certificates
    /// - broadcasting newly formed certificates
    background_tasks: Mutex<JoinSet<DagResult<()>>>,
    /// Aggregates certificates to use as parents for new headers.
    certificates_aggregators: Mutex<HashMap<Round, Box<CertificatesAggregator>>>,
}

/// The `Synchronizer` provides functions for retrieving missing certificates and batches.
pub struct Synchronizer {
    /// Internal data that are thread safe.
    inner: Arc<Inner>,
}

impl Synchronizer {
    pub fn new(
        name: PublicKey,
        committee: SharedCommittee,
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
        let committee: &Committee = &committee.load();
        let genesis = Self::make_genesis(committee);
        let highest_processed_round = certificate_store.highest_round_number();
        let highest_created_certificate = certificate_store.last_round(&name).unwrap();
        let gc_round = (*rx_consensus_round_updates.borrow()).saturating_sub(gc_depth);
        let inner = Arc::new(Inner {
            name,
            committee: committee.clone(),
            worker_cache,
            gc_depth,
            gc_round: AtomicU64::new(gc_round),
            highest_processed_round: AtomicU64::new(highest_processed_round),
            highest_received_round: AtomicU64::new(0),
            highest_created_round: AtomicU64::new(
                highest_created_certificate.map_or(0, |c| c.round()),
            ),
            certificate_store,
            payload_store,
            tx_certificate_fetcher,
            tx_new_certificates,
            tx_parents,
            rx_consensus_round_updates: rx_consensus_round_updates.clone(),
            genesis,
            dag,
            metrics,
            background_tasks: Mutex::new(JoinSet::new()),
            certificates_aggregators: Mutex::new(HashMap::with_capacity(2 * gc_depth as usize)),
        });

        // Start a task to update gc_round.
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
                    // this happens during reconfig when Narwhal is shutting down.
                    return;
                };
                // this is the only task updating gc_round
                inner.gc_round.store(gc_round, Ordering::Release);
            }
        });

        // Start a task to broadcast last created certificate.
        // inner
        // .background_tasks
        // .lock()
        // .spawn(Self::broadcast_certificate(
        //     inner.clone(),
        //     network.clone(),
        //     certificate.clone(),
        // ));

        Self { inner }
    }

    fn make_genesis(committee: &Committee) -> HashMap<CertificateDigest, Certificate> {
        Certificate::genesis(committee)
            .into_iter()
            .map(|x| (x.digest(), x))
            .collect()
    }

    async fn process_certificate(
        &self,
        certificate: Certificate,
        network: &Network,
    ) -> DagResult<()> {
        let digest = certificate.digest();
        if self.inner.certificate_store.contains(&digest)? {
            trace!("Certificate {digest:?} has already been processed. Skip processing.");
            self.inner.metrics.duplicate_certificates_processed.inc();
            return Ok(());
        }

        self.sanitize_certificate(&certificate).await?;

        let result = self
            .process_certificate_internal(certificate, network)
            .await;
        match result {
            Err(DagError::Suspended) => {
                // if let Some(notify) = notify {
                //     self.pending_certificates
                //         .entry(digest)
                //         .or_insert_with(Vec::new)
                //         .push(notify);
                // }
                Ok(())
            }
            result => {
                // if let Some(notifies) = self.pending_certificates.remove(&digest) {
                //     for notify in notifies {
                //         let _ = notify.send(result.clone()); // no problem if remote side isn't listening
                //     }
                // }
                result
            }
        }
    }

    async fn sanitize_certificate(&self, certificate: &Certificate) -> DagResult<()> {
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

    // Logs Core errors as appropriate.
    // fn process_result(result: &DagResult<()>) {
    //     match result {
    //         Ok(()) => (),
    //         Err(DagError::StoreError(e)) => {
    //             error!("{e}");
    //             panic!("Storage failure: killing node.");
    //         }
    //         Err(
    //             e @ DagError::TooOld(..)
    //             | e @ DagError::VoteTooOld(..)
    //             | e @ DagError::InvalidEpoch { .. },
    //         ) => debug!("{e}"),
    //         Err(e) => warn!("{e}"),
    //     }
    // }

    async fn process_own_certificate(
        &self,
        certificate: Certificate,
        network: &Network,
    ) -> DagResult<()> {
        // Process the new certificate.
        match self
            .process_certificate_internal(certificate.clone(), network)
            .await
        {
            Ok(()) => Ok(()),
            result @ Err(DagError::ShuttingDown) => result,
            Err(e) => panic!("Failed to process locally-created certificate: {e}"),
        }?;

        // Broadcast the certificate.
        let round = certificate.round();
        self.inner
            .background_tasks
            .lock()
            .spawn(Self::broadcast_certificate(
                self.inner.clone(),
                network.clone(),
                certificate.clone(),
            ));

        // Update metrics.
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
            "Header {:?} took {} seconds to be materialized to a certificate {:?}",
            certificate.header.digest(),
            header_to_certificate_duration,
            certificate.digest()
        );

        Ok(())
    }

    async fn process_certificate_internal(
        &self,
        certificate: Certificate,
        network: &Network,
    ) -> DagResult<()> {
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
        let network = network.clone();
        let max_age = self.inner.gc_depth.saturating_sub(1);
        self.inner.background_tasks.lock().spawn(async move {
            Synchronizer::sync_batches_internal(inner, &header, network, max_age).await
        });

        // Ensure either we have all the ancestors of this certificate, or the parents have been garbage collected.
        // If we don't, the synchronizer will start fetching missing certificates.
        if !self.check_parents(&certificate).await? {
            // certificate.round() > self.gc_round + 1 &&
            debug!(
                "Processing certificate {:?} suspended: missing ancestors",
                certificate
            );
            self.inner
                .metrics
                .certificates_suspended
                .with_label_values(&["missing_parents"])
                .inc();
            return Err(DagError::Suspended);
        }

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
        let digest = certificate.digest();
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

        // Send it to the consensus layer.
        if let Err(e) = self.inner.tx_new_certificates.send(certificate).await {
            warn!(
                "Failed to deliver certificate {} to the consensus: {}",
                digest, e
            );
            return Err(DagError::ShuttingDown);
        }

        Ok(())
    }

    // Awaits completion of the given certificate broadcasts, aborting if narwhal round
    // advances past certificate round.
    async fn broadcast_certificate(
        inner: Arc<Inner>,
        network: Network,
        certificate: Certificate,
    ) -> DagResult<()> {
        let mut tasks = JoinSet::new();
        let request = PrimaryMessage::Certificate(certificate);
        for (_, _, network_key) in inner.committee.others_primaries(&inner.name).into_iter() {
            tasks.spawn(Self::send_certificate(
                inner.clone(),
                network.clone(),
                network_key,
                request.clone(),
            ));
        }
        while let Some(result) = tasks.join_next().await {
            match result {
                Ok(None) => {}
                Ok(Some((network_key, status))) => {
                    debug!("Error sending certificate {request:?} to {network_key:?}: {status:?}");
                    sleep(Duration::from_secs(1)).await;
                    // Retry sending certificate when there are errors.
                    tasks.spawn(Self::send_certificate(
                        inner.clone(),
                        network.clone(),
                        network_key,
                        request.clone(),
                    ));
                }
                Err(err) => {
                    if err.is_panic() {
                        panic!("Panic while sending certificate: {err}");
                    }
                }
            }
        }
        Ok(())

        // let mut join_all = futures::future::try_join_all(tasks);
        // loop {
        //     tokio::select! {
        //         _ = &mut join_all => {
        //             // Reliable broadcast will not return errors.
        //             return Ok(())
        //         },
        //         result = rx_narwhal_round_updates.changed() => {
        //             if result.is_err() {
        //                 // this happens during reconfig when the other side hangs up.
        //                 return Ok(());
        //             }
        //             narwhal_round = *rx_narwhal_round_updates.borrow();
        //             if narwhal_round > certificate_round {
        //                 // Round has advanced. No longer need to broadcast this cert to
        //                 // ensure liveness.
        //                 return Ok(())
        //             }
        //         },
        //     }
        // }
    }

    async fn send_certificate(
        inner: Arc<Inner>,
        network: Network,
        network_key: NetworkPublicKey,
        reqeust: PrimaryMessage,
    ) -> Option<(NetworkPublicKey, rpc::Status)> {
        let peer_id = anemo::PeerId(network_key.0.to_bytes());
        let peer = network.waiting_peer(peer_id);
        let mut client = PrimaryToPrimaryClient::new(peer);
        match client
            .send_message(Request::new(reqeust.clone()).with_timeout(Duration::from_secs(10)))
            .await
        {
            Ok(response) => None,
            Err(status) => Some((network_key, status)),
        }
    }

    async fn append_certificate_in_aggregator(&self, certificate: Certificate) -> DagResult<()> {
        // Check if we have enough certificates to enter a new dag round and propose a header.
        let Some(parents) = self
            .inner
            .certificates_aggregators
            .lock()
            .entry(certificate.round())
            .or_insert_with(|| Box::new(CertificatesAggregator::new()))
            .append(certificate.clone(), &self.inner.committee) else {
                return Ok(());
            };
        // Send it to the `Proposer`.
        self.inner
            .tx_parents
            .send((parents, certificate.round(), certificate.epoch()))
            .await
            .map_err(|_| DagError::ShuttingDown)
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
            debug!("skipping sync_batches for header {header}: no need to store payload of our own workers");
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
